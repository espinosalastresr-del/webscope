//! Integration tests – local fixtures only, no live network.

use webscope::cache::normalize_url;
use webscope::detectors::detect_all;
use webscope::learning::evaluate_on_fixtures;
use webscope::patterns::{default_patterns, ensure_patterns};
use webscope::storage::{Database, UrlRecord, UrlStatus};
use chrono::Utc;
use std::path::Path;

#[test]
fn normalize_dedup() {
    let a = normalize_url("HTTPS://Example.COM/Path/?b=2&a=1#x").unwrap();
    let b = normalize_url("https://example.com/Path?a=1&b=2").unwrap();
    assert_eq!(a, b);
}

#[test]
fn normalize_strips_tracking() {
    let n = normalize_url("https://example.com/p?utm_source=x&id=1").unwrap();
    assert_eq!(n, "https://example.com/p?id=1");
}

#[test]
fn sqlite_roundtrip() {
    let db = Database::open_in_memory().expect("db");
    let rec = UrlRecord {
        id: None,
        url: "https://example.com/checkout".into(),
        normalized_url: "https://example.com/checkout".into(),
        hostname: "example.com".into(),
        path: Some("/checkout".into()),
        discovery_source: Some("test".into()),
        pattern_id: None,
        discovered_at: Utc::now(),
        status: UrlStatus::Queued,
        last_checked: None,
    };
    let id = db.upsert_url(&rec).expect("upsert");
    assert!(id > 0);
    let got = db
        .get_url_by_normalized("https://example.com/checkout")
        .unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().status, UrlStatus::Queued);
}

#[test]
fn patterns_seed() {
    let db = Database::open_in_memory().expect("db");
    ensure_patterns(&db).expect("seed");
    let list = db.list_patterns().expect("list");
    assert!(!list.is_empty());
    assert!(!default_patterns().is_empty());
}

#[test]
fn detectors_on_payment_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/payment.html").expect("fixture");
    let d = detect_all("https://shop.example/checkout", &html, &[], 200);
    assert!(d.payment.is_payment, "payment should be detected");
    assert!(d.payment.confidence >= 0.45);
    assert!(d.payment.providers.iter().any(|p| p == "Stripe"));
}

#[test]
fn detectors_on_checkout_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/checkout.html").expect("fixture");
    let d = detect_all("https://shop.example/cart/checkout", &html, &[], 200);
    assert!(d.payment.is_payment);
}

#[test]
fn detectors_on_normal_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/normal.html").expect("fixture");
    let d = detect_all("https://example.com/about", &html, &[], 200);
    assert!(!d.payment.is_payment);
    assert!(!d.captcha.detected);
    assert!(!d.cloudflare.detected);
}

#[test]
fn detectors_on_blog_negative() {
    let html = std::fs::read_to_string("tests/fixtures/blog_mention.html").expect("fixture");
    let d = detect_all("https://example.com/blog/billing-pipeline", &html, &[], 200);
    // "billing" in prose alone should not force payment=true under precision-first threshold
    assert!(
        !d.payment.is_payment || d.payment.confidence < 0.45,
        "blog mention should not be strong payment"
    );
}

#[test]
fn detectors_captcha_fixtures() {
    for (file, kind) in [
        ("recaptcha.html", "reCAPTCHA"),
        ("hcaptcha.html", "hCaptcha"),
        ("turnstile.html", "Turnstile"),
    ] {
        let html = std::fs::read_to_string(format!("tests/fixtures/{file}")).expect("fixture");
        let d = detect_all("https://example.com/form", &html, &[], 200);
        assert!(d.captcha.detected, "{file} should detect captcha");
        assert_eq!(
            d.captcha.captcha_type.as_deref(),
            Some(kind),
            "{file} type"
        );
    }
}

#[test]
fn detectors_cloudflare_fixture() {
    let html = std::fs::read_to_string("tests/fixtures/cloudflare.html").expect("fixture");
    let headers = vec![
        ("server".into(), "cloudflare".into()),
        ("cf-ray".into(), "abc123-MAD".into()),
    ];
    let d = detect_all("https://example.com/", &html, &headers, 403);
    assert!(d.cloudflare.detected);
}

#[test]
fn detectors_login_and_phone() {
    let login = std::fs::read_to_string("tests/fixtures/login.html").expect("fixture");
    let d = detect_all("https://example.com/login", &login, &[], 200);
    assert!(d.account.account_available);

    let phone = std::fs::read_to_string("tests/fixtures/phone.html").expect("fixture");
    let d = detect_all("https://example.com/register", &phone, &[], 200);
    assert!(d.personal.categories.iter().any(|c| c == "phone"));
}

#[test]
fn evaluation_runs_and_precision() {
    let report = evaluate_on_fixtures(Path::new("tests/fixtures"));
    assert!(report.sample_count >= 8);
    // Reliability: payment precision is the primary quality target
    assert!(
        report.payment.precision >= 0.90,
        "payment precision below 0.90: {:.3} (tp={} fp={})",
        report.payment.precision,
        report.payment.tp,
        report.payment.fp
    );
}
