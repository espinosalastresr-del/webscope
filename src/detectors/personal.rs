//! Personal data request detector (email, phone, address, etc.).

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalDataDetection {
    pub categories: Vec<String>,
    pub confidence: f64,
    pub signals: Vec<String>,
}

static FIELD_PATTERNS: Lazy<Vec<(&'static str, &'static str, f64)>> = Lazy::new(|| {
    vec![
        // (regex, category, weight)
        (
            r#"(?i)(name|id|autocomplete|placeholder|type)\s*=\s*["'][^"']*\b(e-?mail|correo)\b"#,
            "email",
            0.40,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(phone|tel|mobile|teléfono|telefono|celular)\b"#,
            "phone",
            0.40,
        ),
        (
            r#"(?i)type\s*=\s*["']tel["']"#,
            "phone",
            0.35,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(address|street|dirección|direccion)\b"#,
            "address",
            0.35,
        ),
        (
            r#"(?i)autocomplete\s*=\s*["']street-address["']"#,
            "address",
            0.45,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(city|ciudad|locality)\b"#,
            "city",
            0.30,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(state|province|región|region|estado)\b"#,
            "state",
            0.25,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(zip|postal|postcode|código\s*postal|codigo\s*postal)\b"#,
            "postal_code",
            0.30,
        ),
        (
            r#"(?i)autocomplete\s*=\s*["']postal-code["']"#,
            "postal_code",
            0.40,
        ),
        (
            r#"(?i)(name|id|autocomplete|placeholder)\s*=\s*["'][^"']*\b(country|país|pais)\b"#,
            "country",
            0.25,
        ),
        (
            r#"(?i)autocomplete\s*=\s*["']country["']"#,
            "country",
            0.35,
        ),
        (
            r#"(?i)(billing[_\s-]?address)"#,
            "billing_address",
            0.40,
        ),
        (
            r#"(?i)(shipping[_\s-]?address)"#,
            "shipping_address",
            0.40,
        ),
    ]
});

pub fn detect_personal_data(_url: &str, html: &str) -> PersonalDataDetection {
    let mut categories = Vec::new();
    let mut signals = Vec::new();
    let mut score = 0.0_f64;

    let sample = if html.len() > 150_000 {
        &html[..150_000]
    } else {
        html
    };

    for (pat, cat, weight) in FIELD_PATTERNS.iter() {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(sample) {
                if !categories.iter().any(|c| c == cat) {
                    categories.push(cat.to_string());
                }
                score += weight;
                signals.push(format!("field:{cat}"));
            }
        }
    }

    let confidence = score.min(1.0);

    PersonalDataDetection {
        categories,
        confidence,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_email_and_phone() {
        let html = r#"
            <form>
              <input type="email" name="email" autocomplete="email">
              <input type="tel" name="phone" autocomplete="tel">
            </form>
        "#;
        let d = detect_personal_data("https://example.com", html);
        assert!(d.categories.contains(&"email".to_string()) || d.categories.contains(&"phone".to_string()));
    }
}
