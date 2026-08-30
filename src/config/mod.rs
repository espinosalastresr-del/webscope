//! Configuration loading and management.
//!
//! Priority: CLI flags > environment variables > webscope.toml > defaults.

use crate::error::{Result, WebScopeError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default User-Agent identifying WebScope.
pub const DEFAULT_USER_AGENT: &str =
    "WebScope/0.1 (+https://github.com/webscope/webscope; research crawler; respectful)";

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub threads: usize,
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub read_timeout_secs: u64,
    pub max_retries: u32,
    pub max_response_size: usize,
    pub max_redirects: usize,
    pub rate_limit_per_host: f64,
    pub database_path: PathBuf,
    pub user_agent: String,
    pub learning_enabled: bool,
    pub discovery_enabled: bool,
    pub cache_enabled: bool,
    pub log_level: String,
    pub max_html_size: usize,
    pub queue_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threads: 4,
            timeout_secs: 30,
            connect_timeout_secs: 10,
            read_timeout_secs: 20,
            max_retries: 3,
            max_response_size: 2 * 1024 * 1024, // 2 MiB
            max_redirects: 5,
            rate_limit_per_host: 1.0, // requests per second
            database_path: PathBuf::from("webscope.db"),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            learning_enabled: true,
            discovery_enabled: true,
            cache_enabled: true,
            log_level: "info".to_string(),
            max_html_size: 1 * 1024 * 1024, // 1 MiB
            queue_capacity: 10_000,
        }
    }
}

impl Config {
    /// Load configuration from file (if present) and environment.
    pub fn load() -> Result<Self> {
        let mut cfg = Self::default();

        // Try local webscope.toml
        if let Ok(content) = std::fs::read_to_string("webscope.toml") {
            let file_cfg: ConfigFile = toml::from_str(&content)
                .map_err(|e| WebScopeError::Config(format!("invalid webscope.toml: {e}")))?;
            cfg.merge_file(file_cfg);
        }

        // Environment overrides
        if let Ok(v) = std::env::var("WEBSCOPE_THREADS") {
            cfg.threads = v
                .parse()
                .map_err(|_| WebScopeError::Config("WEBSCOPE_THREADS must be a number".into()))?;
        }
        if let Ok(v) = std::env::var("WEBSCOPE_TIMEOUT") {
            cfg.timeout_secs = v
                .parse()
                .map_err(|_| WebScopeError::Config("WEBSCOPE_TIMEOUT must be a number".into()))?;
        }
        if let Ok(v) = std::env::var("WEBSCOPE_DB") {
            cfg.database_path = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("WEBSCOPE_USER_AGENT") {
            cfg.user_agent = v;
        }
        if let Ok(v) = std::env::var("WEBSCOPE_LOG") {
            cfg.log_level = v;
        }

        // Clamp threads for safety on constrained devices
        if cfg.threads == 0 {
            cfg.threads = 1;
        }
        if cfg.threads > 64 {
            cfg.threads = 64;
        }

        Ok(cfg)
    }

    fn merge_file(&mut self, f: ConfigFile) {
        if let Some(v) = f.threads {
            self.threads = v;
        }
        if let Some(v) = f.timeout_secs {
            self.timeout_secs = v;
        }
        if let Some(v) = f.connect_timeout_secs {
            self.connect_timeout_secs = v;
        }
        if let Some(v) = f.read_timeout_secs {
            self.read_timeout_secs = v;
        }
        if let Some(v) = f.max_retries {
            self.max_retries = v;
        }
        if let Some(v) = f.max_response_size {
            self.max_response_size = v;
        }
        if let Some(v) = f.max_redirects {
            self.max_redirects = v;
        }
        if let Some(v) = f.rate_limit_per_host {
            self.rate_limit_per_host = v;
        }
        if let Some(v) = f.database {
            self.database_path = PathBuf::from(v);
        }
        if let Some(v) = f.user_agent {
            self.user_agent = v;
        }
        if let Some(v) = f.learning {
            self.learning_enabled = v;
        }
        if let Some(v) = f.discovery {
            self.discovery_enabled = v;
        }
        if let Some(v) = f.cache {
            self.cache_enabled = v;
        }
        if let Some(v) = f.log_level {
            self.log_level = v;
        }
        if let Some(v) = f.max_html_size {
            self.max_html_size = v;
        }
        if let Some(v) = f.queue_capacity {
            self.queue_capacity = v;
        }
    }

    /// Apply CLI overrides (higher priority).
    pub fn apply_cli(
        &mut self,
        threads: Option<usize>,
        timeout: Option<u64>,
        max_size: Option<usize>,
        retries: Option<u32>,
        db: Option<PathBuf>,
    ) {
        if let Some(t) = threads {
            self.threads = t.clamp(1, 64);
        }
        if let Some(t) = timeout {
            self.timeout_secs = t;
        }
        if let Some(s) = max_size {
            self.max_response_size = s;
        }
        if let Some(r) = retries {
            self.max_retries = r;
        }
        if let Some(p) = db {
            self.database_path = p;
        }
    }
}

/// Intermediate structure for TOML deserialization (all optional).
#[derive(Debug, Deserialize)]
struct ConfigFile {
    threads: Option<usize>,
    timeout_secs: Option<u64>,
    connect_timeout_secs: Option<u64>,
    read_timeout_secs: Option<u64>,
    max_retries: Option<u32>,
    max_response_size: Option<usize>,
    max_redirects: Option<usize>,
    rate_limit_per_host: Option<f64>,
    database: Option<String>,
    user_agent: Option<String>,
    learning: Option<bool>,
    discovery: Option<bool>,
    cache: Option<bool>,
    log_level: Option<String>,
    max_html_size: Option<usize>,
    queue_capacity: Option<usize>,
}

/// Write a sample configuration file.
pub fn write_example_config(path: &Path) -> Result<()> {
    let example = r#"# WebScope configuration file
# CLI flags override these values; environment variables also override.

threads = 4
timeout_secs = 30
connect_timeout_secs = 10
read_timeout_secs = 20
max_retries = 3
max_response_size = 2097152
max_redirects = 5
rate_limit_per_host = 1.0
database = "webscope.db"
user_agent = "WebScope/0.1 (+https://github.com/webscope/webscope; research crawler; respectful)"
learning = true
discovery = true
cache = true
log_level = "info"
max_html_size = 1048576
queue_capacity = 10000
"#;
    std::fs::write(path, example)?;
    Ok(())
}
