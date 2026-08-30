//! Autonomous Discovery Engine with adaptive pattern prioritization.

use crate::cache::{hostname_of, normalize_url, path_of};
use crate::error::Result;
use crate::patterns;
use crate::storage::{Database, UrlRecord, UrlStatus};
use async_trait::async_trait;
use chrono::Utc;
use tracing::{debug, info};

#[async_trait]
pub trait DiscoveryProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn discover(&self, limit: usize) -> Result<Vec<DiscoveredUrl>>;
}

#[derive(Debug, Clone)]
pub struct DiscoveredUrl {
    pub url: String,
    pub source: String,
    pub pattern_id: Option<i64>,
}

pub struct SeedProvider {
    seeds: Vec<String>,
}

impl SeedProvider {
    pub fn new() -> Self {
        // Public bootstrap seeds only. No aggressive single-host crawling.
        let seeds = vec![
            "https://www.wikipedia.org/".into(),
            "https://github.com/".into(),
            "https://www.mozilla.org/".into(),
            "https://www.apache.org/".into(),
            "https://www.python.org/".into(),
            "https://www.rust-lang.org/".into(),
            "https://crates.io/".into(),
            "https://www.ietf.org/".into(),
            "https://www.w3.org/".into(),
            "https://example.com/".into(),
            "https://example.com/checkout".into(),
            "https://example.com/cart".into(),
            "https://example.com/donate".into(),
            "https://example.com/login".into(),
            "https://example.com/register".into(),
            "https://example.com/payment".into(),
            "https://example.com/billing".into(),
            "https://example.com/subscribe".into(),
        ];
        Self { seeds }
    }
}

#[async_trait]
impl DiscoveryProvider for SeedProvider {
    fn name(&self) -> &str {
        "seed"
    }

    async fn discover(&self, limit: usize) -> Result<Vec<DiscoveredUrl>> {
        Ok(self
            .seeds
            .iter()
            .take(limit)
            .map(|s| DiscoveredUrl {
                url: s.clone(),
                source: "seed".into(),
                pattern_id: None,
            })
            .collect())
    }
}

pub struct DiscoveryEngine {
    providers: Vec<Box<dyn DiscoveryProvider>>,
}

impl DiscoveryEngine {
    pub fn new() -> Self {
        Self {
            providers: vec![Box::new(SeedProvider::new())],
        }
    }

    pub async fn run(&self, db: &Database, limit: usize) -> Result<usize> {
        let _ = patterns::ensure_patterns(db);
        let mut inserted = 0usize;

        for provider in &self.providers {
            if inserted >= limit {
                break;
            }
            let remaining = limit - inserted;
            match provider.discover(remaining).await {
                Ok(urls) => {
                    for d in urls {
                        if self.insert_if_new(db, &d)? {
                            inserted += 1;
                            if inserted >= limit {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(provider = provider.name(), error = %e, "discovery provider failed");
                }
            }
        }

        // Adaptive path expansion on known hosts, ordered by pattern score
        if inserted < limit {
            let hosts = db.list_hostnames(80).unwrap_or_default();
            let ranked = patterns::ranked_paths(db).unwrap_or_default();
            let paths: Vec<String> = if ranked.is_empty() {
                patterns::default_patterns()
                    .into_iter()
                    .filter_map(|p| p.identifier.strip_prefix("path:").map(|s| s.to_string()))
                    .collect()
            } else {
                ranked.into_iter().map(|(p, _)| p).collect()
            };

            'outer: for host in &hosts {
                for path in &paths {
                    if inserted >= limit {
                        break 'outer;
                    }
                    let url = format!("https://{host}{path}");
                    let d = DiscoveredUrl {
                        url,
                        source: "path_pattern".into(),
                        pattern_id: None,
                    };
                    if self.insert_if_new(db, &d)? {
                        inserted += 1;
                    }
                }
            }
        }

        info!(inserted, "discovery finished");
        Ok(inserted)
    }

    fn insert_if_new(&self, db: &Database, d: &DiscoveredUrl) -> Result<bool> {
        let normalized = match normalize_url(&d.url) {
            Ok(n) => n,
            Err(_) => return Ok(false),
        };
        if db.get_url_by_normalized(&normalized)?.is_some() {
            return Ok(false);
        }
        let hostname = hostname_of(&d.url).unwrap_or_default();
        let path = path_of(&d.url);
        let rec = UrlRecord {
            id: None,
            url: d.url.clone(),
            normalized_url: normalized,
            hostname,
            path,
            discovery_source: Some(d.source.clone()),
            pattern_id: d.pattern_id,
            discovered_at: Utc::now(),
            status: UrlStatus::Queued,
            last_checked: None,
        };
        db.upsert_url(&rec)?;
        Ok(true)
    }
}
