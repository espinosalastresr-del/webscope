//! Database schema definitions and migrations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Migration 001 – initial schema.
pub const MIGRATION_001: &str = r#"
CREATE TABLE IF NOT EXISTS urls (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url             TEXT    NOT NULL,
    normalized_url  TEXT    NOT NULL UNIQUE,
    hostname        TEXT    NOT NULL,
    path            TEXT,
    discovery_source TEXT,
    pattern_id      INTEGER,
    discovered_at   TEXT    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'discovered',
    last_checked    TEXT
);

CREATE INDEX IF NOT EXISTS idx_urls_normalized ON urls(normalized_url);
CREATE INDEX IF NOT EXISTS idx_urls_hostname ON urls(hostname);
CREATE INDEX IF NOT EXISTS idx_urls_status ON urls(status);
CREATE INDEX IF NOT EXISTS idx_urls_source ON urls(discovery_source);

CREATE TABLE IF NOT EXISTS pages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id          INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    http_status     INTEGER,
    content_type    TEXT,
    content_hash    TEXT,
    response_size   INTEGER,
    fetched_at      TEXT    NOT NULL,
    title           TEXT,
    server_header   TEXT
);

CREATE INDEX IF NOT EXISTS idx_pages_url ON pages(url_id);

CREATE TABLE IF NOT EXISTS detections (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id                INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    page_id               INTEGER REFERENCES pages(id) ON DELETE SET NULL,
    payment               INTEGER NOT NULL DEFAULT 0,
    payment_confidence    REAL    DEFAULT 0.0,
    payment_category      TEXT,
    providers             TEXT,   -- JSON array
    account_available     INTEGER NOT NULL DEFAULT 0,
    account_required      INTEGER NOT NULL DEFAULT 0,
    personal_data         TEXT,   -- JSON array of categories
    captcha               INTEGER NOT NULL DEFAULT 0,
    captcha_type          TEXT,
    cloudflare            INTEGER NOT NULL DEFAULT 0,
    cloudflare_confidence REAL    DEFAULT 0.0,
    signals               TEXT,   -- JSON array of detected signals
    overall_confidence    REAL    DEFAULT 0.0,
    classified_at         TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_det_url ON detections(url_id);
CREATE INDEX IF NOT EXISTS idx_det_payment ON detections(payment);
CREATE INDEX IF NOT EXISTS idx_det_captcha ON detections(captcha);
CREATE INDEX IF NOT EXISTS idx_det_cf ON detections(cloudflare);

CREATE TABLE IF NOT EXISTS patterns (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    identifier      TEXT    NOT NULL UNIQUE,
    category        TEXT,
    language        TEXT,
    created_at      TEXT    NOT NULL,
    uses            INTEGER NOT NULL DEFAULT 0,
    discovered_urls INTEGER NOT NULL DEFAULT 0,
    relevant_urls   INTEGER NOT NULL DEFAULT 0,
    false_positives INTEGER NOT NULL DEFAULT 0,
    precision       REAL,
    estimated_recall REAL,
    score           REAL    NOT NULL DEFAULT 0.0,
    status          TEXT    NOT NULL DEFAULT 'active',
    last_used       TEXT
);

CREATE INDEX IF NOT EXISTS idx_patterns_status ON patterns(status);
CREATE INDEX IF NOT EXISTS idx_patterns_score ON patterns(score DESC);

CREATE TABLE IF NOT EXISTS pattern_results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern_id      INTEGER NOT NULL REFERENCES patterns(id) ON DELETE CASCADE,
    url_id          INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    relevant        INTEGER,
    created_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS training_samples (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id          INTEGER REFERENCES urls(id) ON DELETE SET NULL,
    features_json   TEXT    NOT NULL,
    label_payment   INTEGER,
    label_account   INTEGER,
    label_phone     INTEGER,
    label_address   INTEGER,
    label_captcha   INTEGER,
    label_cloudflare INTEGER,
    source          TEXT,   -- 'manual' | 'auto' | 'uncertain'
    created_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS models (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    version         TEXT    NOT NULL UNIQUE,
    dataset_hash    TEXT,
    trained_at      TEXT    NOT NULL,
    metrics_json    TEXT,
    sample_count    INTEGER,
    features_version TEXT,
    promoted        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS crawl_runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at      TEXT    NOT NULL,
    finished_at     TEXT,
    limit_urls      INTEGER,
    threads         INTEGER,
    discovered      INTEGER DEFAULT 0,
    processed       INTEGER DEFAULT 0,
    skipped         INTEGER DEFAULT 0,
    errors          INTEGER DEFAULT 0,
    status          TEXT    NOT NULL DEFAULT 'running'
);

CREATE TABLE IF NOT EXISTS errors (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id          INTEGER REFERENCES urls(id) ON DELETE SET NULL,
    error_type      TEXT,
    message         TEXT,
    occurred_at     TEXT    NOT NULL
);
"#;

/// Processing status of a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrlStatus {
    Discovered,
    Queued,
    Processing,
    Completed,
    Failed,
    Skipped,
    Retry,
}

impl UrlStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UrlStatus::Discovered => "discovered",
            UrlStatus::Queued => "queued",
            UrlStatus::Processing => "processing",
            UrlStatus::Completed => "completed",
            UrlStatus::Failed => "failed",
            UrlStatus::Skipped => "skipped",
            UrlStatus::Retry => "retry",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "discovered" => Some(UrlStatus::Discovered),
            "queued" => Some(UrlStatus::Queued),
            "processing" => Some(UrlStatus::Processing),
            "completed" => Some(UrlStatus::Completed),
            "failed" => Some(UrlStatus::Failed),
            "skipped" => Some(UrlStatus::Skipped),
            "retry" => Some(UrlStatus::Retry),
            _ => None,
        }
    }
}

/// Record stored in the `urls` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlRecord {
    pub id: Option<i64>,
    pub url: String,
    pub normalized_url: String,
    pub hostname: String,
    pub path: Option<String>,
    pub discovery_source: Option<String>,
    pub pattern_id: Option<i64>,
    pub discovered_at: DateTime<Utc>,
    pub status: UrlStatus,
    pub last_checked: Option<DateTime<Utc>>,
}

/// Record stored in the `pages` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRecord {
    pub id: Option<i64>,
    pub url_id: i64,
    pub http_status: Option<u16>,
    pub content_type: Option<String>,
    pub content_hash: Option<String>,
    pub response_size: Option<i64>,
    pub fetched_at: DateTime<Utc>,
    pub title: Option<String>,
    pub server_header: Option<String>,
}

/// Record stored in the `detections` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionRecord {
    pub id: Option<i64>,
    pub url_id: i64,
    pub page_id: Option<i64>,
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
    pub classified_at: DateTime<Utc>,
}
