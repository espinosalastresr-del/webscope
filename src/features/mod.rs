#![allow(dead_code)]
//! Structural feature extraction from public HTML / HTTP metadata (regex-based).

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageFeatures {
    pub hostname: String,
    pub path: String,
    pub path_depth: usize,
    pub has_query: bool,
    pub url_keywords: Vec<String>,
    pub form_count: usize,
    pub input_count: usize,
    pub password_inputs: usize,
    pub email_inputs: usize,
    pub tel_inputs: usize,
    pub button_count: usize,
    pub iframe_count: usize,
    pub script_count: usize,
    pub external_scripts: Vec<String>,
    pub link_count: usize,
    pub title: Option<String>,
    pub headings: Vec<String>,
    pub autocomplete_values: Vec<String>,
    pub input_names: Vec<String>,
    pub input_types: Vec<String>,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub server: Option<String>,
}

static RE_TITLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
static RE_H: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<h[123][^>]*>(.*?)</h[123]>").unwrap());
static RE_FORM: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<form\b").unwrap());
static RE_INPUT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<input\b([^>]*)>").unwrap());
static RE_BUTTON: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)<button\b|<input[^>]*type\s*=\s*['"]submit['"]"#).unwrap());
static RE_IFRAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<iframe\b").unwrap());
static RE_SCRIPT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<script\b([^>]*)>").unwrap());
static RE_SRC: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)src\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static RE_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<a\b[^>]*href\s*=").unwrap());
static RE_TYPE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)type\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static RE_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)name\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static RE_AUTO: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)autocomplete\s*=\s*['"]([^'"]+)['"]"#).unwrap());
static RE_TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());

impl PageFeatures {
    pub fn extract(
        url: &str,
        html: &str,
        status: u16,
        content_type: Option<&str>,
        server: Option<&str>,
        max_html: usize,
    ) -> Self {
        let truncated = if html.len() > max_html {
            &html[..max_html]
        } else {
            html
        };

        let mut feat = PageFeatures {
            status_code: status,
            content_type: content_type.map(|s| s.to_string()),
            server: server.map(|s| s.to_string()),
            ..Default::default()
        };

        if let Ok(u) = url::Url::parse(url) {
            feat.hostname = u.host_str().unwrap_or("").to_lowercase();
            feat.path = u.path().to_string();
            feat.path_depth = u.path().split('/').filter(|s| !s.is_empty()).count();
            feat.has_query = u.query().is_some();
            feat.url_keywords = extract_url_keywords(url);
        }

        if let Some(c) = RE_TITLE.captures(truncated) {
            let t = RE_TAG.replace_all(c.get(1).map(|m| m.as_str()).unwrap_or(""), "").trim().to_string();
            if !t.is_empty() {
                feat.title = Some(t);
            }
        }

        for c in RE_H.captures_iter(truncated).take(10) {
            let t = RE_TAG.replace_all(c.get(1).map(|m| m.as_str()).unwrap_or(""), "").trim().to_string();
            if !t.is_empty() && t.len() < 200 {
                feat.headings.push(t);
            }
        }

        feat.form_count = RE_FORM.find_iter(truncated).count();
        feat.button_count = RE_BUTTON.find_iter(truncated).count();
        feat.iframe_count = RE_IFRAME.find_iter(truncated).count();
        feat.link_count = RE_LINK.find_iter(truncated).count();

        for c in RE_INPUT.captures_iter(truncated) {
            feat.input_count += 1;
            let attrs = c.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Some(t) = RE_TYPE.captures(attrs) {
                let tl = t.get(1).unwrap().as_str().to_lowercase();
                feat.input_types.push(tl.clone());
                match tl.as_str() {
                    "password" => feat.password_inputs += 1,
                    "email" => feat.email_inputs += 1,
                    "tel" => feat.tel_inputs += 1,
                    _ => {}
                }
            }
            if let Some(n) = RE_NAME.captures(attrs) {
                let name = n.get(1).unwrap().as_str();
                if name.len() < 80 {
                    feat.input_names.push(name.to_string());
                }
            }
            if let Some(a) = RE_AUTO.captures(attrs) {
                feat.autocomplete_values.push(a.get(1).unwrap().as_str().to_string());
            }
        }

        let mut domains = HashSet::new();
        for c in RE_SCRIPT.captures_iter(truncated) {
            feat.script_count += 1;
            let attrs = c.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Some(s) = RE_SRC.captures(attrs) {
                let src = s.get(1).unwrap().as_str();
                if let Ok(u) = url::Url::parse(src) {
                    if let Some(h) = u.host_str() {
                        domains.insert(h.to_lowercase());
                    }
                } else if src.starts_with("//") {
                    if let Ok(u) = url::Url::parse(&format!("https:{src}")) {
                        if let Some(h) = u.host_str() {
                            domains.insert(h.to_lowercase());
                        }
                    }
                }
            }
        }
        feat.external_scripts = domains.into_iter().collect();
        feat.external_scripts.sort();
        feat.input_names.sort();
        feat.input_names.dedup();
        feat.autocomplete_values.sort();
        feat.autocomplete_values.dedup();

        feat
    }
}

fn extract_url_keywords(url: &str) -> Vec<String> {
    let keywords = [
        "checkout", "cart", "payment", "pay", "billing", "donate", "donation",
        "subscribe", "login", "signin", "register", "signup", "account", "order",
        "shop", "store", "buy", "purchase",
    ];
    let lower = url.to_lowercase();
    keywords
        .iter()
        .filter(|k| lower.contains(*k))
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_basic() {
        let html = r#"
            <html><head><title>Checkout</title></head>
            <body>
              <h1>Pay now</h1>
              <form><input type="email" name="email"><input type="password"></form>
              <script src="https://js.stripe.com/v3/"></script>
            </body></html>
        "#;
        let f = PageFeatures::extract("https://shop.example/checkout", html, 200, Some("text/html"), None, 1_000_000);
        assert_eq!(f.title.as_deref(), Some("Checkout"));
        assert!(f.form_count >= 1);
        assert!(f.email_inputs >= 1);
        assert!(f.password_inputs >= 1);
        assert!(f.external_scripts.iter().any(|s| s.contains("stripe")));
    }
}
