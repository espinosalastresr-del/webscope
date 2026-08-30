#![allow(dead_code)]
//! Persistent storage using SQLite with versioned migrations.

mod schema;

pub use schema::*;

use crate::error::{Result, WebScopeError};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Thread-safe database handle.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -8000;
            ",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA temp_store = MEMORY;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let guard = self.conn.lock().map_err(|e| {
            WebScopeError::Other(format!("db lock: {e}"))
        })?;
        f(&guard)
    }

    fn with_conn_mut<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T>,
    {
        let mut guard = self.conn.lock().map_err(|e| {
            WebScopeError::Other(format!("db lock: {e}"))
        })?;
        f(&mut guard)
    }

    fn migrate(&self) -> Result<()> {
        self.with_conn_mut(|conn| {
            conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                ",
            )?;
            let current: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if current < 1 {
                conn.execute_batch(schema::MIGRATION_001)?;
                conn.execute("INSERT INTO schema_migrations (version) VALUES (1)", [])?;
            }
            Ok(())
        })
    }

    // -------------------------------------------------------------------------
    // URLs
    // -------------------------------------------------------------------------

    pub fn upsert_url(&self, url: &UrlRecord) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "
                INSERT INTO urls (
                    url, normalized_url, hostname, path, discovery_source,
                    pattern_id, discovered_at, status, last_checked
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(normalized_url) DO UPDATE SET
                    last_checked = excluded.last_checked,
                    status = CASE
                        WHEN urls.status IN ('completed', 'failed', 'skipped') THEN urls.status
                        ELSE excluded.status
                    END
                ",
                params![
                    url.url,
                    url.normalized_url,
                    url.hostname,
                    url.path,
                    url.discovery_source,
                    url.pattern_id,
                    url.discovered_at.to_rfc3339(),
                    url.status.as_str(),
                    url.last_checked.map(|t| t.to_rfc3339()),
                ],
            )?;
            let id = conn.last_insert_rowid();
            if id == 0 {
                let existing: i64 = conn.query_row(
                    "SELECT id FROM urls WHERE normalized_url = ?1",
                    params![url.normalized_url],
                    |r| r.get(0),
                )?;
                Ok(existing)
            } else {
                Ok(id)
            }
        })
    }

    pub fn get_url_by_normalized(&self, normalized: &str) -> Result<Option<UrlRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, url, normalized_url, hostname, path, discovery_source,
                        pattern_id, discovered_at, status, last_checked
                 FROM urls WHERE normalized_url = ?1",
                params![normalized],
                |row| Ok(row_to_url(row)),
            )
            .optional()
            .map_err(WebScopeError::from)
            .and_then(|opt| Ok(opt.transpose()?))
        })
    }

    pub fn get_url_by_id(&self, id: i64) -> Result<Option<UrlRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, url, normalized_url, hostname, path, discovery_source,
                        pattern_id, discovered_at, status, last_checked
                 FROM urls WHERE id = ?1",
                params![id],
                |row| Ok(row_to_url(row)),
            )
            .optional()
            .map_err(WebScopeError::from)
            .and_then(|opt| Ok(opt.transpose()?))
        })
    }

    pub fn update_url_status(&self, id: i64, status: UrlStatus) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE urls SET status = ?1, last_checked = datetime('now') WHERE id = ?2",
                params![status.as_str(), id],
            )?;
            Ok(())
        })
    }

    pub fn count_urls_by_status(&self, status: UrlStatus) -> Result<i64> {
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM urls WHERE status = ?1",
                params![status.as_str()],
                |r| r.get(0),
            )?;
            Ok(count)
        })
    }

    pub fn count_all_urls(&self) -> Result<i64> {
        self.with_conn(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM urls", [], |r| r.get(0))?;
            Ok(count)
        })
    }

    pub fn fetch_queued_urls(&self, limit: usize) -> Result<Vec<UrlRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, url, normalized_url, hostname, path, discovery_source,
                        pattern_id, discovered_at, status, last_checked
                 FROM urls
                 WHERE status IN ('discovered', 'queued', 'retry')
                 ORDER BY discovered_at ASC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| Ok(row_to_url(row)))?;
            let mut result = Vec::new();
            for r in rows {
                result.push(r??);
            }
            Ok(result)
        })
    }

    pub fn list_hostnames(&self, limit: usize) -> Result<Vec<String>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT hostname FROM urls ORDER BY hostname LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    // -------------------------------------------------------------------------
    // Pages / detections
    // -------------------------------------------------------------------------

    pub fn insert_page(&self, page: &PageRecord) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "
                INSERT INTO pages (
                    url_id, http_status, content_type, content_hash,
                    response_size, fetched_at, title, server_header
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    page.url_id,
                    page.http_status,
                    page.content_type,
                    page.content_hash,
                    page.response_size,
                    page.fetched_at.to_rfc3339(),
                    page.title,
                    page.server_header,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn insert_detection(&self, det: &DetectionRecord) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "
                INSERT INTO detections (
                    url_id, page_id, payment, payment_confidence, payment_category,
                    providers, account_available, account_required,
                    personal_data, captcha, captcha_type, cloudflare,
                    cloudflare_confidence, signals, overall_confidence, classified_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
                ",
                params![
                    det.url_id,
                    det.page_id,
                    det.payment as i32,
                    det.payment_confidence,
                    det.payment_category,
                    serde_json::to_string(&det.providers).unwrap_or_default(),
                    det.account_available as i32,
                    det.account_required as i32,
                    serde_json::to_string(&det.personal_data).unwrap_or_default(),
                    det.captcha as i32,
                    det.captcha_type,
                    det.cloudflare as i32,
                    det.cloudflare_confidence,
                    serde_json::to_string(&det.signals).unwrap_or_default(),
                    det.overall_confidence,
                    det.classified_at.to_rfc3339(),
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn detection_counts(&self) -> Result<DetectionCounts> {
        self.with_conn(|conn| {
            let payment: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE payment = 1",
                [],
                |r| r.get(0),
            )?;
            let account_req: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE account_required = 1",
                [],
                |r| r.get(0),
            )?;
            let phone: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE personal_data LIKE '%phone%'",
                [],
                |r| r.get(0),
            )?;
            let address: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE personal_data LIKE '%address%'",
                [],
                |r| r.get(0),
            )?;
            let captcha: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE captcha = 1",
                [],
                |r| r.get(0),
            )?;
            let cloudflare: i64 = conn.query_row(
                "SELECT COUNT(*) FROM detections WHERE cloudflare = 1",
                [],
                |r| r.get(0),
            )?;
            Ok(DetectionCounts {
                payment,
                account_required: account_req,
                phone,
                address,
                captcha,
                cloudflare,
            })
        })
    }

    /// Export detection rows with URL for JSON/CSV.
    pub fn export_detections(
        &self,
        filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ExportRow>> {
        self.with_conn(|conn| {
            let sql = match filter {
                Some("payment") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.payment = 1
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                Some("account") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.account_available = 1 OR d.account_required = 1
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                Some("phone") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.personal_data LIKE '%phone%'
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                Some("address") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.personal_data LIKE '%address%'
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                Some("captcha") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.captcha = 1
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                Some("cloudflare") => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     WHERE d.cloudflare = 1
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
                _ => {
                    "SELECT u.url, u.hostname, u.discovery_source, d.payment, d.payment_confidence,
                            d.payment_category, d.providers, d.account_available, d.account_required,
                            d.personal_data, d.captcha, d.captcha_type, d.cloudflare,
                            d.cloudflare_confidence, d.signals, d.overall_confidence, d.classified_at,
                            p.http_status
                     FROM detections d
                     JOIN urls u ON u.id = d.url_id
                     LEFT JOIN pages p ON p.id = d.page_id
                     ORDER BY d.classified_at DESC LIMIT ?1"
                }
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok(ExportRow {
                    url: row.get(0)?,
                    hostname: row.get(1)?,
                    discovery_source: row.get(2)?,
                    payment: row.get::<_, i32>(3)? != 0,
                    payment_confidence: row.get(4)?,
                    payment_category: row.get(5)?,
                    providers: parse_json_vec(row.get(6)?),
                    account_available: row.get::<_, i32>(7)? != 0,
                    account_required: row.get::<_, i32>(8)? != 0,
                    personal_data: parse_json_vec(row.get(9)?),
                    captcha: row.get::<_, i32>(10)? != 0,
                    captcha_type: row.get(11)?,
                    cloudflare: row.get::<_, i32>(12)? != 0,
                    cloudflare_confidence: row.get(13)?,
                    signals: parse_json_vec(row.get(14)?),
                    overall_confidence: row.get(15)?,
                    classified_at: row.get(16)?,
                    http_status: row.get(17)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    // -------------------------------------------------------------------------
    // Patterns
    // -------------------------------------------------------------------------

    pub fn upsert_pattern(&self, p: &PatternRecord) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "
                INSERT INTO patterns (
                    identifier, category, language, created_at, uses,
                    discovered_urls, relevant_urls, false_positives,
                    precision, estimated_recall, score, status, last_used
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                ON CONFLICT(identifier) DO UPDATE SET
                    uses = excluded.uses,
                    discovered_urls = excluded.discovered_urls,
                    relevant_urls = excluded.relevant_urls,
                    false_positives = excluded.false_positives,
                    precision = excluded.precision,
                    estimated_recall = excluded.estimated_recall,
                    score = excluded.score,
                    status = excluded.status,
                    last_used = excluded.last_used
                ",
                params![
                    p.identifier,
                    p.category,
                    p.language,
                    p.created_at.to_rfc3339(),
                    p.uses as i64,
                    p.discovered_urls as i64,
                    p.relevant_urls as i64,
                    p.false_positives as i64,
                    p.precision,
                    p.estimated_recall,
                    p.score,
                    p.status.as_str(),
                    p.last_used.map(|t| t.to_rfc3339()),
                ],
            )?;
            let id = conn.last_insert_rowid();
            if id == 0 {
                let existing: i64 = conn.query_row(
                    "SELECT id FROM patterns WHERE identifier = ?1",
                    params![p.identifier],
                    |r| r.get(0),
                )?;
                Ok(existing)
            } else {
                Ok(id)
            }
        })
    }

    pub fn list_patterns(&self) -> Result<Vec<PatternRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, identifier, category, language, created_at, uses,
                        discovered_urls, relevant_urls, false_positives,
                        precision, estimated_recall, score, status, last_used
                 FROM patterns ORDER BY score DESC",
            )?;
            let rows = stmt.query_map([], |row| Ok(row_to_pattern(row)))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r??);
            }
            Ok(out)
        })
    }

    pub fn record_pattern_result(
        &self,
        pattern_id: i64,
        url_id: i64,
        relevant: Option<bool>,
    ) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO pattern_results (pattern_id, url_id, relevant, created_at)
                 VALUES (?1, ?2, ?3, datetime('now'))",
                params![
                    pattern_id,
                    url_id,
                    relevant.map(|b| b as i32),
                ],
            )?;
            Ok(())
        })
    }

    // -------------------------------------------------------------------------
    // Training / active learning
    // -------------------------------------------------------------------------

    pub fn insert_training_sample(&self, s: &TrainingSampleRecord) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "
                INSERT INTO training_samples (
                    url_id, features_json, label_payment, label_account,
                    label_phone, label_address, label_captcha, label_cloudflare,
                    source, created_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                ",
                params![
                    s.url_id,
                    s.features_json,
                    s.label_payment.map(|b| b as i32),
                    s.label_account.map(|b| b as i32),
                    s.label_phone.map(|b| b as i32),
                    s.label_address.map(|b| b as i32),
                    s.label_captcha.map(|b| b as i32),
                    s.label_cloudflare.map(|b| b as i32),
                    s.source,
                    s.created_at.to_rfc3339(),
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn list_uncertain_samples(&self, limit: usize) -> Result<Vec<(i64, String, f64)>> {
        // Prefer detections with mid-range confidence (uncertain).
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT d.id, u.url, d.overall_confidence
                 FROM detections d
                 JOIN urls u ON u.id = d.url_id
                 WHERE d.overall_confidence >= 0.25 AND d.overall_confidence <= 0.65
                 ORDER BY ABS(d.overall_confidence - 0.5) ASC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    pub fn count_training_samples(&self) -> Result<i64> {
        self.with_conn(|conn| {
            let c: i64 = conn.query_row("SELECT COUNT(*) FROM training_samples", [], |r| r.get(0))?;
            Ok(c)
        })
    }

    // -------------------------------------------------------------------------
    // Crawl runs
    // -------------------------------------------------------------------------

    pub fn start_crawl_run(&self, limit: usize, threads: usize) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO crawl_runs (started_at, limit_urls, threads, status)
                 VALUES (datetime('now'), ?1, ?2, 'running')",
                params![limit as i64, threads as i64],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn finish_crawl_run(
        &self,
        id: i64,
        discovered: i64,
        processed: i64,
        skipped: i64,
        errors: i64,
    ) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE crawl_runs SET finished_at = datetime('now'), discovered = ?1,
                 processed = ?2, skipped = ?3, errors = ?4, status = 'finished'
                 WHERE id = ?5",
                params![discovered, processed, skipped, errors, id],
            )?;
            Ok(())
        })
    }

    pub fn clear_all(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "
                DELETE FROM detections;
                DELETE FROM pages;
                DELETE FROM pattern_results;
                DELETE FROM training_samples;
                DELETE FROM errors;
                DELETE FROM urls;
                DELETE FROM crawl_runs;
                ",
            )?;
            Ok(())
        })
    }
}

fn row_to_url(row: &rusqlite::Row<'_>) -> std::result::Result<UrlRecord, rusqlite::Error> {
    Ok(UrlRecord {
        id: Some(row.get(0)?),
        url: row.get(1)?,
        normalized_url: row.get(2)?,
        hostname: row.get(3)?,
        path: row.get(4)?,
        discovery_source: row.get(5)?,
        pattern_id: row.get(6)?,
        discovered_at: parse_datetime(row.get::<_, String>(7)?)
            .unwrap_or_else(|_| chrono::Utc::now()),
        status: UrlStatus::from_str(&row.get::<_, String>(8)?)
            .unwrap_or(UrlStatus::Discovered),
        last_checked: row
            .get::<_, Option<String>>(9)?
            .and_then(|s| parse_datetime(s).ok()),
    })
}

fn row_to_pattern(row: &rusqlite::Row<'_>) -> std::result::Result<PatternRecord, rusqlite::Error> {
    Ok(PatternRecord {
        id: Some(row.get(0)?),
        identifier: row.get(1)?,
        category: row.get(2)?,
        language: row.get(3)?,
        created_at: parse_datetime(row.get::<_, String>(4)?)
            .unwrap_or_else(|_| chrono::Utc::now()),
        uses: row.get::<_, i64>(5)? as u64,
        discovered_urls: row.get::<_, i64>(6)? as u64,
        relevant_urls: row.get::<_, i64>(7)? as u64,
        false_positives: row.get::<_, i64>(8)? as u64,
        precision: row.get(9)?,
        estimated_recall: row.get(10)?,
        score: row.get(11)?,
        status: PatternStatusDb::from_str(&row.get::<_, String>(12)?)
            .unwrap_or(PatternStatusDb::Active),
        last_used: row
            .get::<_, Option<String>>(13)?
            .and_then(|s| parse_datetime(s).ok()),
    })
}

fn parse_datetime(
    s: String,
) -> std::result::Result<chrono::DateTime<chrono::Utc>, rusqlite::Error> {
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
}

fn parse_json_vec(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}

#[derive(Debug, Default)]
pub struct DetectionCounts {
    pub payment: i64,
    pub account_required: i64,
    pub phone: i64,
    pub address: i64,
    pub captcha: i64,
    pub cloudflare: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportRow {
    pub url: String,
    pub hostname: String,
    pub discovery_source: Option<String>,
    pub payment: bool,
    pub payment_confidence: f64,
    pub payment_category: Option<String>,
    pub providers: Vec<String>,
    pub account_available: bool,
    pub account_required: bool,
    pub personal_data: Vec<String>,
    pub captcha: bool,
    pub captcha_type: Option<String>,
    pub cloudflare: bool,
    pub cloudflare_confidence: f64,
    pub signals: Vec<String>,
    pub overall_confidence: f64,
    pub classified_at: String,
    pub http_status: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PatternRecord {
    pub id: Option<i64>,
    pub identifier: String,
    pub category: Option<String>,
    pub language: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub uses: u64,
    pub discovered_urls: u64,
    pub relevant_urls: u64,
    pub false_positives: u64,
    pub precision: Option<f64>,
    pub estimated_recall: Option<f64>,
    pub score: f64,
    pub status: PatternStatusDb,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternStatusDb {
    Active,
    Exploring,
    Low,
    Discarded,
}

impl PatternStatusDb {
    pub fn as_str(&self) -> &'static str {
        match self {
            PatternStatusDb::Active => "active",
            PatternStatusDb::Exploring => "exploring",
            PatternStatusDb::Low => "low",
            PatternStatusDb::Discarded => "discarded",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "exploring" => Some(Self::Exploring),
            "low" => Some(Self::Low),
            "discarded" => Some(Self::Discarded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrainingSampleRecord {
    pub url_id: Option<i64>,
    pub features_json: String,
    pub label_payment: Option<bool>,
    pub label_account: Option<bool>,
    pub label_phone: Option<bool>,
    pub label_address: Option<bool>,
    pub label_captcha: Option<bool>,
    pub label_cloudflare: Option<bool>,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
