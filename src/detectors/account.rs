//! Account availability / requirement detector.

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDetection {
    pub account_available: bool,
    pub account_required: bool,
    pub confidence: f64,
    pub signals: Vec<String>,
}

static LOGIN_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r#"(?i)\b(log\s*in|sign\s*in|iniciar\s+sesión|acceder)\b"#).unwrap(),
        Regex::new(r#"(?i)href\s*=\s*["'][^"']*login"#).unwrap(),
        Regex::new(r#"(?i)href\s*=\s*["'][^"']*sign[-_]?in"#).unwrap(),
        Regex::new(r#"(?i)<form[^>]*(login|signin)"#).unwrap(),
    ]
});

static REGISTER_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r#"(?i)\b(sign\s*up|register|create\s+account|registr|crear\s+cuenta)\b"#)
            .unwrap(),
        Regex::new(r#"(?i)href\s*=\s*["'][^"']*(register|signup|sign-up)"#).unwrap(),
    ]
});

static REQUIRED_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(
            r#"(?i)(you\s+must|please)\s+(log\s*in|sign\s*in|create\s+an?\s+account|register)"#,
        )
        .unwrap(),
        Regex::new(r#"(?i)(login|sign\s*in)\s+required"#).unwrap(),
        Regex::new(r#"(?i)members?\s+only"#).unwrap(),
        Regex::new(r#"(?i)debes\s+(iniciar\s+sesión|registrarte)"#).unwrap(),
        Regex::new(r#"(?i)cuenta\s+necesaria|se\s+requiere\s+cuenta"#).unwrap(),
    ]
});

pub fn detect_account(url: &str, html: &str) -> AccountDetection {
    let mut signals = Vec::new();
    let mut avail_score = 0.0_f64;
    let mut req_score = 0.0_f64;

    let sample = if html.len() > 150_000 {
        &html[..150_000]
    } else {
        html
    };
    let url_lower = url.to_lowercase();

    if url_lower.contains("/login") || url_lower.contains("/signin") || url_lower.contains("/account")
    {
        avail_score += 0.30;
        signals.push("url_account_path".into());
    }
    if url_lower.contains("/register") || url_lower.contains("/signup") {
        avail_score += 0.30;
        signals.push("url_register_path".into());
    }

    for re in LOGIN_PATTERNS.iter() {
        if re.is_match(sample) {
            avail_score += 0.25;
            signals.push("login_ui".into());
            break;
        }
    }
    for re in REGISTER_PATTERNS.iter() {
        if re.is_match(sample) {
            avail_score += 0.25;
            signals.push("register_ui".into());
            break;
        }
    }
    for re in REQUIRED_PATTERNS.iter() {
        if re.is_match(sample) {
            req_score += 0.45;
            signals.push("account_required_text".into());
            break;
        }
    }

    // Password field presence is a strong signal of account form
    if Regex::new(r#"(?i)type\s*=\s*["']password["']"#)
        .unwrap()
        .is_match(sample)
    {
        avail_score += 0.20;
        signals.push("password_field".into());
    }

    let account_available = avail_score >= 0.30;
    let account_required = req_score >= 0.40;
    let confidence = (avail_score.max(req_score)).min(1.0);

    AccountDetection {
        account_available,
        account_required,
        confidence,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_login_page() {
        let html = r#"<html><body><form action="/login"><input type="password"><button>Log in</button></form></body></html>"#;
        let d = detect_account("https://example.com/login", html);
        assert!(d.account_available);
    }
}
