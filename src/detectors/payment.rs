//! Payment / checkout / donation / billing detector.

use regex::Regex;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetection {
    pub is_payment: bool,
    pub confidence: f64,
    pub category: Option<String>,
    pub providers: Vec<String>,
    pub signals: Vec<String>,
}

static PAYMENT_KEYWORDS: Lazy<Vec<(&'static str, &'static str, f64)>> = Lazy::new(|| {
    vec![
        // (pattern, category, weight)
        (r"(?i)\bcheckout\b", "checkout", 0.35),
        (r"(?i)\bpayment\b", "payment", 0.15),
        (r"(?i)\bpurchase\b", "purchase", 0.12),
        (r"(?i)\bbuy\s+now\b", "purchase", 0.30),
        (r"(?i)\badd\s+to\s+cart\b", "ecommerce", 0.25),
        (r"(?i)\bshopping\s+cart\b", "ecommerce", 0.30),
        (r"(?i)\bdonate\b", "donation", 0.35),
        (r"(?i)\bdonation\b", "donation", 0.35),
        (r"(?i)\bbilling\b", "billing", 0.12),
        (r"(?i)\bsubscription\b", "subscription", 0.12),
        (r"(?i)\bsubscribe\b", "subscription", 0.10),
        (r"(?i)\border\s+summary\b", "checkout", 0.30),
        (r"(?i)\bplace\s+order\b", "checkout", 0.35),
        (r"(?i)\bcomplete\s+purchase\b", "purchase", 0.35),
        (r"(?i)\bpay\s+now\b", "payment", 0.35),
        (r"(?i)\bcredit\s+card\b", "payment", 0.25),
        (r"(?i)\bdebit\s+card\b", "payment", 0.25),
        (r"(?i)\bcart\b", "ecommerce", 0.08),
        // Spanish
        (r"(?i)\bpago\b", "payment", 0.15),
        (r"(?i)\bcomprar\b", "purchase", 0.25),
        (r"(?i)\bcarrito\b", "ecommerce", 0.25),
        (r"(?i)\bdonar\b", "donation", 0.35),
        (r"(?i)\bdonación\b", "donation", 0.35),
        (r"(?i)\bfacturación\b", "billing", 0.12),
        (r"(?i)\bsuscripción\b", "subscription", 0.12),
        (r"(?i)\bfinalizar\s+compra\b", "checkout", 0.35),
    ]
});

static PROVIDER_PATTERNS: Lazy<Vec<(&'static str, &'static str)>> = Lazy::new(|| {
    vec![
        (r"(?i)js\.stripe\.com|stripe\.com/v3|Stripe\(", "Stripe"),
        (r"(?i)paypal\.com|paypalobjects\.com|paypal\.Buttons", "PayPal"),
        (r"(?i)checkoutshopper.*adyen|adyen\.com", "Adyen"),
        (r"(?i)braintree-api|braintreegateway|braintree\.js", "Braintree"),
        (r"(?i)squareup\.com|square\.cdn|sq-payment", "Square"),
        (r"(?i)js\.authorize\.net|acceptjs", "Authorize.Net"),
        (r"(?i)secure\.authorize\.net", "Authorize.Net"),
        (r"(?i)checkout\.razorpay|razorpay\.com", "Razorpay"),
        (r"(?i)js\.klarna\.com|klarna\.com", "Klarna"),
        (r"(?i)pay\.google\.com|google\.com/pay", "Google Pay"),
        (r"(?i)apple\.com/apple-pay|apple-pay", "Apple Pay"),
        (r"(?i)merchant\.worldpay|worldpay\.com", "Worldpay"),
        (r"(?i)secure\.2checkout|2checkout\.com", "2Checkout"),
    ]
});

static FORM_HINTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)(name|id|autocomplete)\s*=\s*["']?(cc-number|cardnumber|card-number|card_number|cvv|cvc|cc-csc|cc-exp|expiry|exp-date)"#,
    )
    .unwrap()
});

static BUTTON_HINTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)<(button|input)[^>]*(value|aria-label|title)\s*=\s*["'][^"']*\b(pay|purchase|checkout|buy|donate|subscribe|order)\b"#,
    )
    .unwrap()
});

pub fn detect_payment(url: &str, html: &str) -> PaymentDetection {
    let mut score = 0.0_f64;
    let mut signals = Vec::new();
    let mut category_scores: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut providers = Vec::new();

    // URL path signals
    let url_lower = url.to_lowercase();
    for (pat, cat, w) in [
        ("/checkout", "checkout", 0.40),
        ("/cart", "ecommerce", 0.30),
        ("/payment", "payment", 0.35),
        ("/billing", "billing", 0.30),
        ("/donate", "donation", 0.40),
        ("/donation", "donation", 0.40),
        ("/subscribe", "subscription", 0.30),
        ("/order", "checkout", 0.20),
    ] {
        if url_lower.contains(pat) {
            score += w;
            signals.push(format!("url_path:{pat}"));
            *category_scores.entry(cat.to_string()).or_default() += w;
        }
    }

    // Keyword signals in HTML (limit scan size for safety)
    let sample = if html.len() > 200_000 {
        &html[..200_000]
    } else {
        html
    };

    for (pat, cat, weight) in PAYMENT_KEYWORDS.iter() {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(sample) {
                score += weight;
                signals.push(format!("keyword:{cat}"));
                *category_scores.entry(cat.to_string()).or_default() += weight;
            }
        }
    }

    // Provider detection
    for (pat, name) in PROVIDER_PATTERNS.iter() {
        if let Ok(re) = Regex::new(pat) {
            if re.is_match(sample) {
                score += 0.45;
                signals.push(format!("provider:{name}"));
                if !providers.contains(&name.to_string()) {
                    providers.push(name.to_string());
                }
            }
        }
    }

    // Form field hints (card-related autocomplete / names) – we only detect presence
    if FORM_HINTS.is_match(sample) {
        score += 0.40;
        signals.push("form_card_fields".into());
        *category_scores
            .entry("payment".to_string())
            .or_default() += 0.40;
    }

    // Button text hints
    if BUTTON_HINTS.is_match(sample) {
        score += 0.20;
        signals.push("payment_button".into());
    }

    // Clamp and decide
    let confidence = score.min(1.0);
    // Precision-first decision:
    // - strong evidence (provider, card fields, or score >= 0.55) → payment
    // - medium score only if multiple distinct signal classes
    let has_provider = !providers.is_empty();
    let has_form = signals.iter().any(|s| s == "form_card_fields");
    let has_url = signals.iter().any(|s| s.starts_with("url_path:"));
    let keyword_hits = signals.iter().filter(|s| s.starts_with("keyword:")).count();
    let is_payment = has_provider
        || has_form
        || confidence >= 0.55
        || (confidence >= 0.45 && (has_url || keyword_hits >= 2));

    let category = category_scores
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, _)| k);

    PaymentDetection {
        is_payment,
        confidence,
        category,
        providers,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stripe_checkout() {
        let html = r#"
            <html><body>
            <h1>Checkout</h1>
            <script src="https://js.stripe.com/v3/"></script>
            <button>Pay now</button>
            </body></html>
        "#;
        let d = detect_payment("https://shop.example/checkout", html);
        assert!(d.is_payment);
        assert!(d.providers.contains(&"Stripe".to_string()));
        assert!(d.confidence > 0.5);
    }

    #[test]
    fn ignores_normal_page() {
        let html = r#"<html><body><h1>About us</h1><p>We sell widgets.</p></body></html>"#;
        let d = detect_payment("https://example.com/about", html);
        assert!(!d.is_payment);
    }
}
