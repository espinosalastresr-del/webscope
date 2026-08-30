//! Asynchronous crawler based on Tokio with resource limits.
mod robots;

use crate::config::Config;
use crate::error::{Result, WebScopeError};
use crate::storage::{Database, PageRecord, UrlRecord, UrlStatus};
use bytes::Bytes;
use chrono::Utc;
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Semaphore, watch};
use tracing::{debug, info, warn};

/// Result of fetching a single URL.
#[derive(Debug)]
pub struct FetchResult {
    pub url_id: i64,
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub server: Option<String>,
    pub body: Bytes,
    pub final_url: String,
    pub content_hash: String,
}

/// Shared crawl statistics.
#[derive(Debug, Default)]
pub struct CrawlStats {
    pub discovered: AtomicUsize,
    pub processed: AtomicUsize,
    pub skipped: AtomicUsize,
    pub errors: AtomicUsize,
    pub payment: AtomicUsize,
    pub account_required: AtomicUsize,
    pub phone: AtomicUsize,
    pub address: AtomicUsize,
    pub captcha: AtomicUsize,
    pub cloudflare: AtomicUsize,
}

impl CrawlStats {
    pub fn snapshot(&self) -> CrawlSnapshot {
        CrawlSnapshot {
            discovered: self.discovered.load(Ordering::Relaxed),
            processed: self.processed.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            payment: self.payment.load(Ordering::Relaxed),
            account_required: self.account_required.load(Ordering::Relaxed),
            phone: self.phone.load(Ordering::Relaxed),
            address: self.address.load(Ordering::Relaxed),
            captcha: self.captcha.load(Ordering::Relaxed),
            cloudflare: self.cloudflare.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrawlSnapshot {
    pub discovered: usize,
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub payment: usize,
    pub account_required: usize,
    pub phone: usize,
    pub address: usize,
    pub captcha: usize,
    pub cloudflare: usize,
}

/// Build an HTTP client respecting config limits.
pub fn build_client(cfg: &Config) -> Result<Client> {
    let client = Client::builder()
        .user_agent(&cfg.user_agent)
        .timeout(Duration::from_secs(cfg.timeout_secs))
        .connect_timeout(Duration::from_secs(cfg.connect_timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(cfg.max_redirects))
        .gzip(true)
        .brotli(true)
        .pool_max_idle_per_host(2)
        .build()?;
    Ok(client)
}

/// Fetch a single URL with size limit and retries.
pub async fn fetch_url(
    client: &Client,
    url: &str,
    cfg: &Config,
) -> Result<(u16, Option<String>, Option<String>, Bytes, String)> {
    let mut last_err = None;
    for attempt in 0..=cfg.max_retries {
        if attempt > 0 {
            let backoff = Duration::from_millis(200 * 2u64.pow(attempt.min(5)));
            let jitter = Duration::from_millis(rand::random::<u64>() % 150);
            tokio::time::sleep(backoff + jitter).await;
        }

        match do_fetch(client, url, cfg).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                debug!(attempt, error = %e, url, "fetch failed");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| WebScopeError::Network("unknown fetch error".into())))
}

async fn do_fetch(
    client: &Client,
    url: &str,
    cfg: &Config,
) -> Result<(u16, Option<String>, Option<String>, Bytes, String)> {
    let resp = client.get(url).send().await?;

    // Honour Retry-After on 429
    if resp.status() == StatusCode::TOO_MANY_REQUESTS {
        if let Some(ra) = resp.headers().get("retry-after") {
            if let Ok(s) = ra.to_str() {
                if let Ok(secs) = s.parse::<u64>() {
                    tokio::time::sleep(Duration::from_secs(secs.min(60))).await;
                }
            }
        }
        return Err(WebScopeError::Http("429 Too Many Requests".into()));
    }

    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let server = resp
        .headers()
        .get(reqwest::header::SERVER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let final_url = resp.url().to_string();

    // Stream body with size limit
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| WebScopeError::Network(e.to_string()))?;
        if body.len() + chunk.len() > cfg.max_response_size {
            warn!(url, "response exceeded max size, truncating");
            break;
        }
        body.extend_from_slice(&chunk);
    }

    Ok((status, content_type, server, Bytes::from(body), final_url))
}

/// Compute SHA-256 hex of content.
pub fn content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// High-level scan runner.
pub struct Scanner {
    pub cfg: Config,
    pub db: Database,
    pub client: Client,
    pub stats: Arc<CrawlStats>,
    pub cancel: Arc<AtomicBool>,
}

impl Scanner {
    pub fn new(cfg: Config, db: Database) -> Result<Self> {
        let client = build_client(&cfg)?;
        Ok(Self {
            cfg,
            db,
            client,
            stats: Arc::new(CrawlStats::default()),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Process up to `limit` new URLs.
    pub async fn run(&self, limit: usize, refresh: bool) -> Result<CrawlSnapshot> {
        let semaphore = Arc::new(Semaphore::new(self.cfg.threads));
        let processed_new = Arc::new(AtomicUsize::new(0));
        let (tx_cancel, rx_cancel) = watch::channel(false);

        // Spawn signal handler
        let cancel_flag = self.cancel.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            info!("SIGINT received – graceful shutdown");
            cancel_flag.store(true, Ordering::SeqCst);
            let _ = tx_cancel.send(true);
        });

        // Main processing loop
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                break;
            }
            if processed_new.load(Ordering::Relaxed) >= limit {
                info!(limit, "limit reached");
                break;
            }

            let batch = self.db.fetch_queued_urls(self.cfg.threads * 2)?;
            if batch.is_empty() {
                // No more work – discovery should have filled the queue
                tokio::time::sleep(Duration::from_millis(200)).await;
                // If still empty after a short wait, exit
                let again = self.db.fetch_queued_urls(1)?;
                if again.is_empty() {
                    break;
                }
                continue;
            }

            let mut handles = Vec::new();
            for url_rec in batch {
                if processed_new.load(Ordering::Relaxed) >= limit {
                    break;
                }
                if self.cancel.load(Ordering::SeqCst) {
                    break;
                }

                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let client = self.client.clone();
                let cfg = self.cfg.clone();
                let db = self.db.clone();
                let stats = self.stats.clone();
                let cancel = self.cancel.clone();
                let processed_new = processed_new.clone();

                let handle = tokio::spawn(async move {
                    let _permit = permit;
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Err(e) = process_one(
                        &client,
                        &cfg,
                        &db,
                        &stats,
                        url_rec,
                        refresh,
                        &processed_new,
                        limit,
                    )
                    .await
                    {
                        debug!(error = %e, "process_one failed");
                        stats.errors.fetch_add(1, Ordering::Relaxed);
                    }
                });
                handles.push(handle);
            }

            for h in handles {
                let _ = h.await;
            }

            // Check cancel watch
            if *rx_cancel.borrow() {
                break;
            }
        }

        Ok(self.stats.snapshot())
    }
}

async fn process_one(
    client: &Client,
    cfg: &Config,
    db: &Database,
    stats: &CrawlStats,
    url_rec: UrlRecord,
    refresh: bool,
    processed_new: &AtomicUsize,
    limit: usize,
) -> Result<()> {
    let id = url_rec.id.ok_or_else(|| WebScopeError::InvalidState("url missing id".into()))?;

    // Skip already completed unless refresh
    if !refresh {
        if let Ok(Some(existing)) = db.get_url_by_normalized(&url_rec.normalized_url) {
            if existing.status == UrlStatus::Completed {
                stats.skipped.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        }
    }

    if processed_new.load(Ordering::Relaxed) >= limit {
        return Ok(());
    }

    db.update_url_status(id, UrlStatus::Processing)?;

    // Responsible crawling: honour robots.txt when available (fail-open on errors)
    if !robots::check_allowed(client, &url_rec.url).await {
        tracing::info!(url = %url_rec.url, "robots.txt disallows path – skipped");
        stats.skipped.fetch_add(1, Ordering::Relaxed);
        db.update_url_status(id, UrlStatus::Skipped)?;
        return Ok(());
    }


    match fetch_url(client, &url_rec.url, cfg).await {
        Ok((status, content_type, server, body, _final_url)) => {
            let hash = content_hash(&body);
            let page = PageRecord {
                id: None,
                url_id: id,
                http_status: Some(status),
                content_type: content_type.clone(),
                content_hash: Some(hash),
                response_size: Some(body.len() as i64),
                fetched_at: Utc::now(),
                title: None, // filled later by features if needed
                server_header: server.clone(),
            };
            let page_id = db.insert_page(&page)?;

            // Convert body to string for analysis (lossy is fine for public HTML)
            let html = String::from_utf8_lossy(&body);

            // Collect simple headers for detectors
            let headers = vec![
                (
                    "content-type".into(),
                    content_type.clone().unwrap_or_default(),
                ),
                ("server".into(), server.clone().unwrap_or_default()),
            ];

            let detections = crate::detectors::detect_all(&url_rec.url, &html, &headers, status);

            let det = crate::storage::DetectionRecord {
                id: None,
                url_id: id,
                page_id: Some(page_id),
                payment: detections.payment.is_payment,
                payment_confidence: detections.payment.confidence,
                payment_category: detections.payment.category,
                providers: detections.payment.providers,
                account_available: detections.account.account_available,
                account_required: detections.account.account_required,
                personal_data: detections.personal.categories.clone(),
                captcha: detections.captcha.detected,
                captcha_type: detections.captcha.captcha_type,
                cloudflare: detections.cloudflare.detected,
                cloudflare_confidence: detections.cloudflare.confidence,
                signals: detections.all_signals,
                overall_confidence: detections.overall_confidence,
                classified_at: Utc::now(),
            };
            db.insert_detection(&det)?;

            // Feed pattern productivity (precision-weighted, reliability-first)
            if let Some(path) = url_rec.path.as_deref() {
                let _ = crate::patterns::record_detection_outcome(
                    db,
                    path,
                    detections.payment.is_payment,
                    detections.account.account_available || detections.account.account_required,
                    detections.captcha.detected,
                    detections.cloudflare.detected,
                    detections.payment.confidence,
                );
            }

            // Extract structural features for offline learning (no user data stored)
            let _feats = crate::features::PageFeatures::extract(
                &url_rec.url,
                &html,
                status,
                content_type.as_deref(),
                server.as_deref(),
                cfg.max_html_size,
            );
            // Persist uncertain cases for active learning
            if detections.overall_confidence > 0.0 && detections.overall_confidence < 0.45 {
                let sample = crate::storage::TrainingSampleRecord {
                    url_id: Some(id),
                    features_json: serde_json::to_string(&_feats).unwrap_or_default(),
                    label_payment: None,
                    label_account: None,
                    label_phone: None,
                    label_address: None,
                    label_captcha: None,
                    label_cloudflare: None,
                    source: "uncertain".into(),
                    created_at: Utc::now(),
                };
                let _ = db.insert_training_sample(&sample);
            }

            // Update stats
            stats.processed.fetch_add(1, Ordering::Relaxed);
            processed_new.fetch_add(1, Ordering::Relaxed);
            if detections.payment.is_payment {
                stats.payment.fetch_add(1, Ordering::Relaxed);
            }
            if detections.account.account_required {
                stats.account_required.fetch_add(1, Ordering::Relaxed);
            }
            if detections.personal.categories.iter().any(|c| c == "phone") {
                stats.phone.fetch_add(1, Ordering::Relaxed);
            }
            if detections
                .personal
                .categories
                .iter()
                .any(|c| c == "address" || c == "billing_address" || c == "shipping_address")
            {
                stats.address.fetch_add(1, Ordering::Relaxed);
            }
            if detections.captcha.detected {
                stats.captcha.fetch_add(1, Ordering::Relaxed);
            }
            if detections.cloudflare.detected {
                stats.cloudflare.fetch_add(1, Ordering::Relaxed);
            }

            db.update_url_status(id, UrlStatus::Completed)?;
        }
        Err(e) => {
            warn!(url = %url_rec.url, error = %e, "fetch failed");
            stats.errors.fetch_add(1, Ordering::Relaxed);
            db.update_url_status(id, UrlStatus::Failed)?;
        }
    }
    Ok(())
}
