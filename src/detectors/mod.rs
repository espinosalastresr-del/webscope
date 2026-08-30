//! Public-content detectors for payment, account, personal data, CAPTCHA and Cloudflare.

mod payment;
mod account;
mod personal;
mod captcha;
mod cloudflare;

pub use payment::{PaymentDetection, detect_payment};
pub use account::{AccountDetection, detect_account};
pub use personal::{PersonalDataDetection, detect_personal_data};
pub use captcha::{CaptchaDetection, detect_captcha};
pub use cloudflare::{CloudflareDetection, detect_cloudflare};

use serde::{Deserialize, Serialize};

/// Aggregated detection result for a single page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDetections {
    pub payment: PaymentDetection,
    pub account: AccountDetection,
    pub personal: PersonalDataDetection,
    pub captcha: CaptchaDetection,
    pub cloudflare: CloudflareDetection,
    pub all_signals: Vec<String>,
    pub overall_confidence: f64,
}

impl PageDetections {
    pub fn from_parts(
        payment: PaymentDetection,
        account: AccountDetection,
        personal: PersonalDataDetection,
        captcha: CaptchaDetection,
        cloudflare: CloudflareDetection,
    ) -> Self {
        let mut signals = Vec::new();
        signals.extend(payment.signals.iter().cloned());
        signals.extend(account.signals.iter().cloned());
        signals.extend(personal.signals.iter().cloned());
        signals.extend(captcha.signals.iter().cloned());
        signals.extend(cloudflare.signals.iter().cloned());

        // Simple aggregate confidence: max of individual confidences weighted lightly
        let overall = [
            payment.confidence,
            account.confidence,
            personal.confidence,
            captcha.confidence,
            cloudflare.confidence,
        ]
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max);

        Self {
            payment,
            account,
            personal,
            captcha,
            cloudflare,
            all_signals: signals,
            overall_confidence: overall,
        }
    }
}

/// Run all detectors on the given HTML + HTTP metadata.
pub fn detect_all(
    url: &str,
    html: &str,
    headers: &[(String, String)],
    status: u16,
) -> PageDetections {
    let payment = detect_payment(url, html);
    let account = detect_account(url, html);
    let personal = detect_personal_data(url, html);
    let captcha = detect_captcha(html, headers);
    let cloudflare = detect_cloudflare(url, html, headers, status);

    PageDetections::from_parts(payment, account, personal, captcha, cloudflare)
}
