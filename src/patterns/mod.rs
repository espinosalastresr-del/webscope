//! Pattern Discovery Engine – tracks productivity and adapts discovery.

use crate::error::Result;
use crate::storage::{Database, PatternRecord, PatternStatusDb};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: Option<i64>,
    pub identifier: String,
    pub category: Option<String>,
    pub language: Option<String>,
    pub uses: u64,
    pub discovered_urls: u64,
    pub relevant_urls: u64,
    pub false_positives: u64,
    pub precision: Option<f64>,
    pub score: f64,
    pub status: PatternStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatternStatus {
    Active,
    Exploring,
    LowProductivity,
    Discarded,
}

impl PatternStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PatternStatus::Active => "active",
            PatternStatus::Exploring => "exploring",
            PatternStatus::LowProductivity => "low",
            PatternStatus::Discarded => "discarded",
        }
    }
}

#[allow(dead_code)]
impl Pattern {
    pub fn update_score(&mut self) {
        if self.uses == 0 {
            self.score = 0.55; // exploration bonus for new patterns
            return;
        }
        let prec = if self.discovered_urls > 0 {
            self.relevant_urls as f64 / self.discovered_urls as f64
        } else {
            0.0
        };
        self.precision = Some(prec);
        // Reliability-first: precision weighs more than volume
        self.score = prec * 0.75 + (self.relevant_urls as f64).ln_1p() * 0.15 + 0.10;
        if self.uses > 25 && prec < 0.12 {
            self.status = PatternStatus::LowProductivity;
        }
        if self.uses > 60 && prec < 0.06 {
            self.status = PatternStatus::Discarded;
        }
        if prec >= 0.40 && self.relevant_urls >= 3 {
            self.status = PatternStatus::Active;
        }
    }

    pub fn record_outcome(&mut self, relevant: bool) {
        self.uses += 1;
        self.discovered_urls += 1;
        if relevant {
            self.relevant_urls += 1;
        } else {
            self.false_positives += 1;
        }
        self.update_score();
    }
}

pub fn default_patterns() -> Vec<Pattern> {
    let mk = |id: &str, cat: &str, lang: &str| Pattern {
        id: None,
        identifier: id.into(),
        category: Some(cat.into()),
        language: Some(lang.into()),
        uses: 0,
        discovered_urls: 0,
        relevant_urls: 0,
        false_positives: 0,
        precision: None,
        score: 0.55,
        status: PatternStatus::Exploring,
    };
    vec![
        mk("path:/checkout", "checkout", "en"),
        mk("path:/cart", "ecommerce", "en"),
        mk("path:/payment", "payment", "en"),
        mk("path:/billing", "billing", "en"),
        mk("path:/donate", "donation", "en"),
        mk("path:/donation", "donation", "en"),
        mk("path:/subscribe", "subscription", "en"),
        mk("path:/login", "account", "en"),
        mk("path:/signin", "account", "en"),
        mk("path:/register", "account", "en"),
        mk("path:/signup", "account", "en"),
        mk("path:/account", "account", "en"),
        mk("path:/order", "checkout", "en"),
        mk("path:/pago", "payment", "es"),
        mk("path:/carrito", "ecommerce", "es"),
        mk("path:/donar", "donation", "es"),
        mk("path:/donacion", "donation", "es"),
        mk("path:/iniciar-sesion", "account", "es"),
        mk("path:/registro", "account", "es"),
        mk("path:/comprar", "purchase", "es"),
    ]
}

/// Ensure default patterns exist in the database.
pub fn ensure_patterns(db: &Database) -> Result<()> {
    for p in default_patterns() {
        let rec = PatternRecord {
            id: None,
            identifier: p.identifier.clone(),
            category: p.category.clone(),
            language: p.language.clone(),
            created_at: Utc::now(),
            uses: p.uses,
            discovered_urls: p.discovered_urls,
            relevant_urls: p.relevant_urls,
            false_positives: p.false_positives,
            precision: p.precision,
            estimated_recall: None,
            score: p.score,
            status: match p.status {
                PatternStatus::Active => PatternStatusDb::Active,
                PatternStatus::Exploring => PatternStatusDb::Exploring,
                PatternStatus::LowProductivity => PatternStatusDb::Low,
                PatternStatus::Discarded => PatternStatusDb::Discarded,
            },
            last_used: None,
        };
        let _ = db.upsert_pattern(&rec)?;
    }
    Ok(())
}

/// Paths ranked by pattern score (active/exploring only).
pub fn ranked_paths(db: &Database) -> Result<Vec<(String, f64)>> {
    let patterns = db.list_patterns()?;
    let mut paths: Vec<(String, f64)> = patterns
        .into_iter()
        .filter(|p| {
            matches!(
                p.status,
                PatternStatusDb::Active | PatternStatusDb::Exploring
            )
        })
        .filter_map(|p| {
            p.identifier
                .strip_prefix("path:")
                .map(|path| (path.to_string(), p.score))
        })
        .collect();
    paths.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(paths)
}

pub fn list_default_patterns() -> Vec<Pattern> {
    default_patterns()
}

/// Update pattern productivity from a finished detection.
/// Reliability-first: only marks relevant when payment/account/captcha/cloudflare signals are strong.
pub fn record_detection_outcome(
    db: &Database,
    url_path: &str,
    payment: bool,
    account: bool,
    captcha: bool,
    cloudflare: bool,
    payment_confidence: f64,
) -> Result<()> {
    let patterns = db.list_patterns()?;
    let relevant = (payment && payment_confidence >= 0.40)
        || account
        || captcha
        || cloudflare;
    for mut p in patterns {
        let Some(path_key) = p.identifier.strip_prefix("path:") else {
            continue;
        };
        if !url_path.to_lowercase().contains(&path_key.to_lowercase()) {
            continue;
        }
        p.uses = p.uses.saturating_add(1);
        p.discovered_urls = p.discovered_urls.saturating_add(1);
        if relevant {
            p.relevant_urls = p.relevant_urls.saturating_add(1);
        } else {
            p.false_positives = p.false_positives.saturating_add(1);
        }
        let prec = if p.discovered_urls > 0 {
            Some(p.relevant_urls as f64 / p.discovered_urls as f64)
        } else {
            None
        };
        p.precision = prec;
        // Precision-weighted score
        let score = match prec {
            Some(pr) => pr * 0.75 + (p.relevant_urls as f64).ln_1p() * 0.15 + 0.10,
            None => 0.55,
        };
        p.score = score;
        if p.uses > 25 && prec.unwrap_or(0.0) < 0.12 {
            p.status = PatternStatusDb::Low;
        }
        if p.uses > 60 && prec.unwrap_or(0.0) < 0.06 {
            p.status = PatternStatusDb::Discarded;
        }
        if prec.unwrap_or(0.0) >= 0.40 && p.relevant_urls >= 3 {
            p.status = PatternStatusDb::Active;
        }
        p.last_used = Some(Utc::now());
        db.upsert_pattern(&p)?;
        break; // one matching pattern is enough per URL
    }
    Ok(())
}
