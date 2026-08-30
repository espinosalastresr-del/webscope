//! Cloudflare presence detector (detection only – never bypasses).

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareDetection {
    pub detected: bool,
    pub confidence: f64,
    pub signals: Vec<String>,
}

static CF_HTML_PATTERNS: Lazy<Vec<(&'static str, f64)>> = Lazy::new(|| {
    vec![
        (r#"(?i)cf-browser-verification|cf_challenge|challenge-platform"#, 0.85),
        (r#"(?i)cdn-cgi/challenge|cdn-cgi/l/chk_jschl"#, 0.90),
        (r#"(?i)cloudflare\.com/cdn-cgi"#, 0.70),
        (r#"(?i)Attention Required! \| Cloudflare"#, 0.95),
        (r#"(?i)Checking your browser before accessing"#, 0.90),
        (r#"(?i)cf-ray"#, 0.40),
        (r#"(?i)__cf_bm|cf_clearance"#, 0.60),
    ]
});

pub fn detect_cloudflare(
    _url: &str,
    html: &str,
    headers: &[(String, String)],
    status: u16,
) -> CloudflareDetection {
    let mut signals = Vec::new();
    let mut confidence = 0.0_f64;

    // Header signals
    for (k, v) in headers {
        let key = k.to_lowercase();
        let val = v.to_lowercase();
        if key == "server" && val.contains("cloudflare") {
            signals.push("header:server=cloudflare".into());
            confidence = confidence.max(0.70);
        }
        if key == "cf-ray" {
            signals.push("header:cf-ray".into());
            confidence = confidence.max(0.75);
        }
        if key.starts_with("cf-") {
            signals.push(format!("header:{key}"));
            confidence = confidence.max(0.50);
        }
        if key == "set-cookie" && (val.contains("__cf") || val.contains("cf_clearance")) {
            signals.push("cookie:cf".into());
            confidence = confidence.max(0.60);
        }
    }

    // Status codes commonly returned by CF challenges
    if status == 403 || status == 503 || status == 429 {
        // Only count if we also have other CF signals
        if confidence > 0.0 {
            signals.push(format!("status:{status}"));
            confidence = (confidence + 0.15).min(1.0);
        }
    }

    let sample = if html.len() > 80_000 {
        &html[..80_000]
    } else {
        html
    };

    for (pat, weight) in CF_HTML_PATTERNS.iter() {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(sample) {
                signals.push(format!("html_cf:{weight}"));
                confidence = confidence.max(*weight);
            }
        }
    }

    CloudflareDetection {
        detected: confidence >= 0.55,
        confidence,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cf_headers() {
        let headers = vec![
            ("server".into(), "cloudflare".into()),
            ("cf-ray".into(), "7a1b2c3d4e5f-MAD".into()),
        ];
        let d = detect_cloudflare("https://example.com", "", &headers, 200);
        assert!(d.detected);
        assert!(d.confidence >= 0.70);
    }
}
