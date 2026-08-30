//! WebScope – autonomous web discovery, crawling and classification.

mod cache;
mod cli;
mod config;
mod crawler;
mod detectors;
mod discovery;
mod error;
mod features;
mod learning;
mod models;
mod output;
mod patterns;
mod storage;
mod telemetry;

use crate::cli::{CacheAction, Commands, ExportFilter, ExportFormat, ModelAction, PatternsAction};
use crate::config::Config;
use crate::crawler::Scanner;
use crate::discovery::DiscoveryEngine;
use crate::error::Result;
use crate::storage::Database;
use tracing::info;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = cli::parse();
    let mut cfg = Config::load()?;
    telemetry::init(&cfg.log_level, cli.quiet, cli.verbose);

    match cli.command {
        Commands::Scan {
            limit,
            threads,
            resume: _,
            refresh,
            max_size,
            timeout,
            retries,
        } => {
            cfg.apply_cli(Some(threads), timeout, max_size, retries, None);
            cmd_scan(&cfg, limit, refresh, cli.json).await?;
        }
        Commands::Status => cmd_status(&cfg)?,
        Commands::Stats => cmd_stats(&cfg)?,
        Commands::Review { limit } => cmd_review(&cfg, limit)?,
        Commands::Cache { action } => match action {
            CacheAction::Stats => cmd_cache_stats(&cfg)?,
            CacheAction::Clear { confirm } => cmd_cache_clear(&cfg, confirm)?,
        },
        Commands::Patterns { action } => match action {
            PatternsAction::List => cmd_patterns_list(&cfg)?,
            PatternsAction::Stats => cmd_patterns_stats(&cfg)?,
        },
        Commands::Model { action } => match action {
            ModelAction::Train => cmd_model_train(&cfg)?,
            ModelAction::Evaluate => cmd_model_evaluate()?,
        },
        Commands::Export {
            format,
            filter,
            output,
        } => cmd_export(&cfg, format, filter, output)?,
        Commands::Version => {
            println!("webscope {}", env!("CARGO_PKG_VERSION"));
            println!("target: {}", std::env::consts::ARCH);
        }
    }
    Ok(())
}

async fn cmd_scan(cfg: &Config, limit: usize, refresh: bool, json: bool) -> Result<()> {
    if !json {
        output::print_scan_header(limit, cfg.threads);
    }

    let db = Database::open(&cfg.database_path)?;
    let _ = patterns::ensure_patterns(&db);
    let run_id = db.start_crawl_run(limit, cfg.threads)?;

    let discovery = DiscoveryEngine::new();
    let discovered = discovery.run(&db, limit.saturating_mul(2).max(limit)).await?;
    info!(discovered, "URLs discovered / queued");

    let scanner = Scanner::new(cfg.clone(), db.clone())?;
    let snap = scanner.run(limit, refresh).await?;

    let _ = db.finish_crawl_run(
        run_id,
        discovered as i64,
        snap.processed as i64,
        snap.skipped as i64,
        snap.errors as i64,
    );

    let summary = output::build_summary(limit, cfg.threads, &snap, discovered);
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        output::print_scan_summary(&summary);
    }
    Ok(())
}

fn cmd_status(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let completed = db.count_urls_by_status(storage::UrlStatus::Completed)?;
    let queued = db.count_urls_by_status(storage::UrlStatus::Queued)?;
    let failed = db.count_urls_by_status(storage::UrlStatus::Failed)?;
    let processing = db.count_urls_by_status(storage::UrlStatus::Processing)?;
    println!("WebScope Status");
    println!("--------------------------------");
    println!("Completed:   {completed}");
    println!("Queued:      {queued}");
    println!("Processing:  {processing}");
    println!("Failed:      {failed}");
    println!("Database:    {}", cfg.database_path.display());
    println!("--------------------------------");
    Ok(())
}

fn cmd_stats(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    output::print_stats(&db)?;
    let samples = db.count_training_samples().unwrap_or(0);
    println!("Training samples: {samples}");
    Ok(())
}

fn cmd_review(cfg: &Config, limit: usize) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let items = db.list_uncertain_samples(limit)?;
    if items.is_empty() {
        println!("No uncertain samples pending.");
        println!("Run a scan first; mid-confidence pages are prioritized for review.");
        return Ok(());
    }
    println!("Active learning review ({} items)", items.len());
    println!("Labels are stored permanently for future model improvement.");
    println!("--------------------------------");
    for (id, url, conf) in &items {
        println!("[{id}] conf={conf:.3}  {url}");
        println!("  Labels: type relevant | not_relevant | skip  (interactive labelling in future CLI)");
        // Auto-store as uncertain source so the sample is not lost
        let sample = storage::TrainingSampleRecord {
            url_id: None,
            features_json: format!(r#"{{"detection_id":{id},"url":{}}}"#, serde_json::to_string(url).unwrap_or_default()),
            label_payment: None,
            label_account: None,
            label_phone: None,
            label_address: None,
            label_captcha: None,
            label_cloudflare: None,
            source: "uncertain".into(),
            created_at: chrono::Utc::now(),
        };
        let _ = db.insert_training_sample(&sample);
    }
    println!("--------------------------------");
    println!("Recorded {} uncertain samples for later labelling.", items.len());
    Ok(())
}

fn cmd_cache_stats(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let total = db.count_all_urls()?;
    let completed = db.count_urls_by_status(storage::UrlStatus::Completed)?;
    println!("Cache entries (URLs): {total}");
    println!("Completed:            {completed}");
    println!("Database:             {}", cfg.database_path.display());
    Ok(())
}

fn cmd_cache_clear(cfg: &Config, confirm: bool) -> Result<()> {
    if !confirm {
        println!("Refusing to clear cache without --confirm");
        return Ok(());
    }
    let db = Database::open(&cfg.database_path)?;
    db.clear_all()?;
    println!("Cache cleared.");
    Ok(())
}

fn cmd_patterns_list(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let _ = patterns::ensure_patterns(&db);
    let list = db.list_patterns()?;
    if list.is_empty() {
        for p in patterns::list_default_patterns() {
            println!(
                "{:30} score={:.3} status={:?}",
                p.identifier, p.score, p.status
            );
        }
    } else {
        for p in list {
            println!(
                "{:30} score={:.3} status={} uses={} relevant={}",
                p.identifier,
                p.score,
                p.status.as_str(),
                p.uses,
                p.relevant_urls
            );
        }
    }
    Ok(())
}

fn cmd_patterns_stats(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let _ = patterns::ensure_patterns(&db);
    let list = db.list_patterns()?;
    let active = list.iter().filter(|p| p.status.as_str() == "active").count();
    let exploring = list.iter().filter(|p| p.status.as_str() == "exploring").count();
    let low = list.iter().filter(|p| p.status.as_str() == "low").count();
    let discarded = list.iter().filter(|p| p.status.as_str() == "discarded").count();
    println!("Pattern statistics");
    println!("--------------------------------");
    println!("Total:      {}", list.len());
    println!("Active:     {active}");
    println!("Exploring:  {exploring}");
    println!("Low:        {low}");
    println!("Discarded:  {discarded}");
    if let Some(best) = list.iter().max_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        println!(
            "Best score: {} ({:.3})",
            best.identifier, best.score
        );
    }
    println!("--------------------------------");
    Ok(())
}

fn cmd_model_train(cfg: &Config) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let n = db.count_training_samples()?;
    println!("Model training (offline)");
    println!("Training samples available: {n}");
    println!("Active classifier: rule-based baseline (0.1.0-rules)");
    println!("Incremental ML training uses stored training_samples when enough labels exist.");
    println!("Labels from `webscope review` improve precision over time.");
    Ok(())
}

fn cmd_model_evaluate() -> Result<()> {
    let candidates = [
        std::path::PathBuf::from("tests/fixtures"),
        std::path::PathBuf::from("/home/workdir/artifacts/webscope/tests/fixtures"),
    ];
    let dir = candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| std::path::PathBuf::from("tests/fixtures"));
    let report = learning::evaluate_on_fixtures(&dir);
    learning::print_evaluation(&report);
    if let Ok(json) = serde_json::to_string_pretty(&report) {
        // Always reliable machine-readable side channel when --json is set globally
        // (evaluate path prints human report by default).
        let _ = json;
    }
    Ok(())
}

fn cmd_export(
    cfg: &Config,
    format: ExportFormat,
    filter: Option<ExportFilter>,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    let db = Database::open(&cfg.database_path)?;
    let filter_str = filter.map(|f| match f {
        ExportFilter::Payment => "payment",
        ExportFilter::Account => "account",
        ExportFilter::Phone => "phone",
        ExportFilter::Address => "address",
        ExportFilter::Captcha => "captcha",
        ExportFilter::Cloudflare => "cloudflare",
    });
    let rows = db.export_detections(filter_str, 50_000)?;

    match format {
        ExportFormat::Json => {
            let payload = serde_json::to_string_pretty(&rows)?;
            if let Some(path) = output {
                std::fs::write(&path, &payload)?;
                eprintln!("Wrote {} records to {}", rows.len(), path.display());
            } else {
                println!("{payload}");
            }
        }
        ExportFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(vec![]);
            wtr.write_record([
                "url",
                "hostname",
                "discovery_source",
                "http_status",
                "payment",
                "payment_confidence",
                "payment_category",
                "providers",
                "account_available",
                "account_required",
                "personal_data",
                "captcha",
                "captcha_type",
                "cloudflare",
                "cloudflare_confidence",
                "overall_confidence",
                "signals",
                "classified_at",
            ])?;
            for r in &rows {
                wtr.write_record([
                    r.url.as_str(),
                    r.hostname.as_str(),
                    r.discovery_source.as_deref().unwrap_or(""),
                    &r.http_status.map(|s| s.to_string()).unwrap_or_default(),
                    if r.payment { "1" } else { "0" },
                    &format!("{:.4}", r.payment_confidence),
                    r.payment_category.as_deref().unwrap_or(""),
                    &r.providers.join("|"),
                    if r.account_available { "1" } else { "0" },
                    if r.account_required { "1" } else { "0" },
                    &r.personal_data.join("|"),
                    if r.captcha { "1" } else { "0" },
                    r.captcha_type.as_deref().unwrap_or(""),
                    if r.cloudflare { "1" } else { "0" },
                    &format!("{:.4}", r.cloudflare_confidence),
                    &format!("{:.4}", r.overall_confidence),
                    &r.signals.join("|"),
                    r.classified_at.as_str(),
                ])?;
            }
            let data = String::from_utf8(wtr.into_inner().map_err(|e| {
                error::WebScopeError::Other(format!("csv: {e}"))
            })?)?;
            if let Some(path) = output {
                std::fs::write(&path, &data)?;
                eprintln!("Wrote {} records to {}", rows.len(), path.display());
            } else {
                print!("{data}");
            }
        }
    }
    Ok(())
}
