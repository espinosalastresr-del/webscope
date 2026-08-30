//! CAPTCHA presence detector (detection only – never solves).

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptchaDetection {
    pub detected: bool,
    pub captcha_type: Option<String>,
    pub confidence: f64,
    pub signals: Vec<String>,
}

static CAPTCHA_PATTERNS: Lazy<Vec<(&'static str, &'static str, f64)>> = Lazy::new(|| {
    vec![
        (
            r#"(?i)www\.google\.com/recaptcha|grecaptcha|recaptcha/api"#,
            "reCAPTCHA",
            0.90,
        ),
        (
            r#"(?i)hcaptcha\.com|h-captcha|hcaptcha\.com/1/api"#,
            "hCaptcha",
            0.90,
        ),
        (
            r#"(?i)challenges\.cloudflare\.com/turnstile|cf-turnstile|turnstile/v0"#,
            "Turnstile",
            0.90,
        ),
        (
            r#"(?i)arkoselabs\.com|funcaptcha|arkose"#,
            "Arkose",
            0.85,
        ),
        (
            r#"(?i)captcha-delivery\.com|datadome"#,
            "DataDome",
            0.80,
        ),
        (
            r#"(?i)geetest\.com|gt_captcha"#,
            "GeeTest",
            0.80,
        ),
        (
            r#"(?i)class\s*=\s*["'][^"']*g-recaptcha"#,
            "reCAPTCHA",
            0.85,
        ),
        (
            r#"(?i)data-sitekey\s*="#,
            "generic_captcha",
            0.60,
        ),
    ]
});

pub fn detect_captcha(html: &str, headers: &[(String, String)]) -> CaptchaDetection {
    let mut signals = Vec::new();
    let mut best_type: Option<String> = None;
    let mut confidence = 0.0_f64;

    let sample = if html.len() > 100_000 {
        &html[..100_000]
    } else {
        html
    };

    for (pat, name, weight) in CAPTCHA_PATTERNS.iter() {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(sample) {
                signals.push(format!("html:{name}"));
                if *weight > confidence {
                    confidence = *weight;
                    best_type = Some(name.to_string());
                }
            }
        }
    }

    // Header signals (rare but possible)
    for (k, v) in headers {
        let combined = format!("{}:{}", k.to_lowercase(), v.to_lowercase());
        if combined.contains("captcha") || combined.contains("challenge") {
            signals.push("header_captcha".into());
            confidence = confidence.max(0.50);
        }
    }

    CaptchaDetection {
        detected: confidence >= 0.55,
        captcha_type: best_type,
        confidence,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_recaptcha() {
        let html = r#"<script src="https://www.google.com/recaptcha/api.js"></script><div class="g-recaptcha" data-sitekey="xxx"></div>"#;
        let d = detect_captcha(html, &[]);
        assert!(d.detected);
        assert_eq!(d.captcha_type.as_deref(), Some("reCAPTCHA"));
    }
}
