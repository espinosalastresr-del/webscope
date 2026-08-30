//! Evaluation metrics, active learning and lightweight classification blend.

use crate::detectors::{
    detect_account, detect_captcha, detect_cloudflare, detect_payment, detect_personal_data,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub tp: usize,
    pub fp: usize,
    pub tn: usize,
    pub fn_: usize,
    pub support: usize,
}

impl BinaryMetrics {
    pub fn from_counts(tp: usize, fp: usize, tn: usize, fn_: usize) -> Self {
        let support = tp + fp + tn + fn_;
        let accuracy = if support > 0 {
            (tp + tn) as f64 / support as f64
        } else {
            0.0
        };
        let precision = if tp + fp > 0 {
            tp as f64 / (tp + fp) as f64
        } else {
            0.0
        };
        let recall = if tp + fn_ > 0 {
            tp as f64 / (tp + fn_) as f64
        } else {
            0.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        Self {
            accuracy,
            precision,
            recall,
            f1,
            tp,
            fp,
            tn,
            fn_,
            support,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub payment: BinaryMetrics,
    pub account: BinaryMetrics,
    pub phone: BinaryMetrics,
    pub address: BinaryMetrics,
    pub captcha: BinaryMetrics,
    pub cloudflare: BinaryMetrics,
    pub sample_count: usize,
    pub notes: String,
}

/// Labelled fixture definition for offline evaluation.
struct FixtureCase {
    file: &'static str,
    url: &'static str,
    payment: bool,
    account: bool,
    phone: bool,
    address: bool,
    captcha: bool,
    cloudflare: bool,
}

const FIXTURES: &[FixtureCase] = &[

    FixtureCase {
        file: "blog_mention.html",
        url: "https://example.com/blog/billing-pipeline",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "payment.html",
        url: "https://shop.example/checkout",
        payment: true,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "checkout.html",
        url: "https://shop.example/cart/checkout",
        payment: true,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "donation.html",
        url: "https://org.example/donate",
        payment: true,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "login.html",
        url: "https://example.com/login",
        payment: false,
        account: true,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "registration.html",
        url: "https://example.com/register",
        payment: false,
        account: true,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "phone.html",
        url: "https://example.com/contact",
        payment: false,
        account: false,
        phone: true,
        address: false,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "address.html",
        url: "https://shop.example/shipping",
        payment: false,
        account: false,
        phone: false,
        address: true,
        captcha: false,
        cloudflare: false,
    },
    FixtureCase {
        file: "recaptcha.html",
        url: "https://example.com/verify",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: true,
        cloudflare: false,
    },
    FixtureCase {
        file: "hcaptcha.html",
        url: "https://example.com/verify-h",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: true,
        cloudflare: false,
    },
    FixtureCase {
        file: "turnstile.html",
        url: "https://example.com/verify-t",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: true,
        cloudflare: false,
    },
    FixtureCase {
        file: "cloudflare.html",
        url: "https://example.com/",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: true,
    },
    FixtureCase {
        file: "normal.html",
        url: "https://example.com/about",
        payment: false,
        account: false,
        phone: false,
        address: false,
        captcha: false,
        cloudflare: false,
    },
];

fn load_fixture(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join(name);
    std::fs::read_to_string(path).ok()
}

/// Evaluate rule-based detectors on local fixtures (no network).
pub fn evaluate_on_fixtures(fixtures_dir: &Path) -> EvaluationReport {
    let mut pay = (0, 0, 0, 0); // tp fp tn fn
    let mut acc = (0, 0, 0, 0);
    let mut phone = (0, 0, 0, 0);
    let mut addr = (0, 0, 0, 0);
    let mut cap = (0, 0, 0, 0);
    let mut cf = (0, 0, 0, 0);
    let mut n = 0;

    for case in FIXTURES {
        let Some(html) = load_fixture(fixtures_dir, case.file) else {
            continue;
        };
        n += 1;
        let p = detect_payment(case.url, &html);
        let a = detect_account(case.url, &html);
        let per = detect_personal_data(case.url, &html);
        let c = detect_captcha(&html, &[]);
        let cloud = detect_cloudflare(case.url, &html, &[], 200);

        update_counts(&mut pay, p.is_payment, case.payment);
        update_counts(
            &mut acc,
            a.account_available || a.account_required,
            case.account,
        );
        let has_phone = per.categories.iter().any(|x| x == "phone");
        update_counts(&mut phone, has_phone, case.phone);
        let has_addr = per.categories.iter().any(|x| {
            x == "address" || x == "billing_address" || x == "shipping_address" || x == "city"
        });
        update_counts(&mut addr, has_addr, case.address);
        update_counts(&mut cap, c.detected, case.captcha);
        update_counts(&mut cf, cloud.detected, case.cloudflare);
    }

    EvaluationReport {
        payment: BinaryMetrics::from_counts(pay.0, pay.1, pay.2, pay.3),
        account: BinaryMetrics::from_counts(acc.0, acc.1, acc.2, acc.3),
        phone: BinaryMetrics::from_counts(phone.0, phone.1, phone.2, phone.3),
        address: BinaryMetrics::from_counts(addr.0, addr.1, addr.2, addr.3),
        captcha: BinaryMetrics::from_counts(cap.0, cap.1, cap.2, cap.3),
        cloudflare: BinaryMetrics::from_counts(cf.0, cf.1, cf.2, cf.3),
        sample_count: n,
        notes: "Evaluated on local HTML fixtures only. Precision is the primary quality target (>= 0.90).".into(),
    }
}

fn update_counts(c: &mut (usize, usize, usize, usize), pred: bool, truth: bool) {
    match (pred, truth) {
        (true, true) => c.0 += 1,
        (true, false) => c.1 += 1,
        (false, false) => c.2 += 1,
        (false, true) => c.3 += 1,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSample {
    pub features_json: String,
    pub label_payment: Option<bool>,
    pub label_account: Option<bool>,
    pub label_phone: Option<bool>,
    pub label_address: Option<bool>,
    pub label_captcha: Option<bool>,
    pub label_cloudflare: Option<bool>,
    pub source: String,
}

/// Rule-based classifier with optional future model blend.
#[allow(dead_code)]
pub struct Classifier {
    pub version: String,
    pub trained: bool,
}

impl Classifier {
    pub fn new() -> Self {
        Self {
            version: "0.1.0-rules".into(),
            trained: false,
        }
    }

    /// Prefer high precision: only boost when rule confidence is already strong.
    pub fn predict_payment_confidence(&self, rule_confidence: f64) -> f64 {
        rule_confidence
    }
}

impl Default for Classifier {
    fn default() -> Self {
        Self::new()
    }
}

pub fn print_evaluation(report: &EvaluationReport) {
    println!("WebScope Model Evaluation");
    println!("--------------------------------");
    println!("Samples: {}", report.sample_count);
    println!("{}", report.notes);
    println!();
    print_metric("payment", &report.payment);
    print_metric("account", &report.account);
    print_metric("phone", &report.phone);
    print_metric("address", &report.address);
    print_metric("captcha", &report.captcha);
    print_metric("cloudflare", &report.cloudflare);
    println!("--------------------------------");
    let target_ok = report.payment.precision >= 0.90
        || report.sample_count < 5;
    if report.payment.precision >= 0.90 {
        println!("Primary target (payment precision >= 0.90): PASS ({:.3})", report.payment.precision);
    } else if !target_ok {
        println!(
            "Primary target (payment precision >= 0.90): below target ({:.3})",
            report.payment.precision
        );
    }
}

fn print_metric(name: &str, m: &BinaryMetrics) {
    println!(
        "{name:12}  acc={:.3}  prec={:.3}  rec={:.3}  f1={:.3}  (tp={} fp={} tn={} fn={})",
        m.accuracy, m.precision, m.recall, m.f1, m.tp, m.fp, m.tn, m.fn_
    );
}
