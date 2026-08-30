//! Centralized error types for WebScope.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebScopeError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Crawl limit reached")]
    LimitReached,

    #[error("Cancelled")]
    Cancelled,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Model error: {0}")]
    Model(String),

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, WebScopeError>;

impl From<anyhow::Error> for WebScopeError {
    fn from(err: anyhow::Error) -> Self {
        WebScopeError::Other(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for WebScopeError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        WebScopeError::Other(format!("utf8: {err}"))
    }
}

impl From<csv::Error> for WebScopeError {
    fn from(err: csv::Error) -> Self {
        WebScopeError::Other(format!("csv: {err}"))
    }
}

impl From<reqwest::Error> for WebScopeError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            WebScopeError::Network(format!("timeout: {err}"))
        } else if err.is_connect() {
            WebScopeError::Network(format!("connect: {err}"))
        } else if err.is_request() {
            WebScopeError::Http(format!("request: {err}"))
        } else {
            WebScopeError::Http(err.to_string())
        }
    }
}
