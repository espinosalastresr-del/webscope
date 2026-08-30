//! Human-readable and machine-readable output.

use crate::crawler::CrawlSnapshot;
use crate::storage::Database;
use serde::Serialize;

#[derive(Serialize)]
pub struct ScanSummary {
    pub limit: usize,
    pub workers: usize,
    pub discovered: usize,
    pub new_urls: usize,
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub payment_pages: usize,
    pub account_required: usize,
    pub phone_requested: usize,
    pub address_requested: usize,
    pub captcha: usize,
    pub cloudflare: usize,
    pub progress_pct: f64,
}

pub fn print_scan_header(limit: usize, workers: usize) {
    println!("WebScope");
    println!("--------------------------------");
    println!("Discovery started");
    println!();
    println!("Limit:              {limit}");
    println!("Workers:            {workers:>5}");
    println!("Cache:             enabled");
    println!("Learning:           enabled");
    println!();
}

pub fn print_scan_summary(s: &ScanSummary) {
    println!("Discovered:      {:>10}", s.discovered);
    println!("New URLs:        {:>10}", s.new_urls);
    println!("Processed:       {:>10}", s.processed);
    println!("Skipped:         {:>10}", s.skipped);
    println!("Errors:          {:>10}", s.errors);
    println!();
    println!("Payment pages:   {:>10}", s.payment_pages);
    println!("Account required:{:>10}", s.account_required);
    println!("Phone requested: {:>10}", s.phone_requested);
    println!("Address requested:{:>9}", s.address_requested);
    println!("CAPTCHA:         {:>10}", s.captcha);
    println!("Cloudflare:      {:>10}", s.cloudflare);
    println!();
    println!("Progress: {:>6.2}%", s.progress_pct);
    println!("--------------------------------");
}

pub fn print_stats(db: &Database) -> crate::error::Result<()> {
    let total = db.count_all_urls()?;
    let completed = db.count_urls_by_status(crate::storage::UrlStatus::Completed)?;
    let failed = db.count_urls_by_status(crate::storage::UrlStatus::Failed)?;
    let queued = db.count_urls_by_status(crate::storage::UrlStatus::Queued)?;
    let counts = db.detection_counts()?;

    println!("WebScope Statistics");
    println!("--------------------------------");
    println!("Total URLs:        {total}");
    println!("Completed:         {completed}");
    println!("Queued:            {queued}");
    println!("Failed:            {failed}");
    println!();
    println!("Payment:           {}", counts.payment);
    println!("Account required:  {}", counts.account_required);
    println!("Phone:             {}", counts.phone);
    println!("Address:           {}", counts.address);
    println!("CAPTCHA:           {}", counts.captcha);
    println!("Cloudflare:        {}", counts.cloudflare);
    println!("--------------------------------");
    Ok(())
}

#[allow(dead_code)]
pub fn print_status(snap: &CrawlSnapshot, limit: usize) {
    let progress = if limit > 0 {
        (snap.processed as f64 / limit as f64) * 100.0
    } else {
        0.0
    };
    println!("WebScope Status");
    println!("--------------------------------");
    println!("Processed:   {}", snap.processed);
    println!("Skipped:     {}", snap.skipped);
    println!("Errors:      {}", snap.errors);
    println!("Progress:    {progress:.2}%");
    println!("--------------------------------");
}

pub fn build_summary(
    limit: usize,
    workers: usize,
    snap: &CrawlSnapshot,
    discovered: usize,
) -> ScanSummary {
    let progress = if limit > 0 {
        (snap.processed as f64 / limit as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    ScanSummary {
        limit,
        workers,
        discovered,
        new_urls: snap.processed + snap.errors, // approximate
        processed: snap.processed,
        skipped: snap.skipped,
        errors: snap.errors,
        payment_pages: snap.payment,
        account_required: snap.account_required,
        phone_requested: snap.phone,
        address_requested: snap.address,
        captcha: snap.captcha,
        cloudflare: snap.cloudflare,
        progress_pct: progress,
    }
}
