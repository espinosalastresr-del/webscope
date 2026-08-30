//! Command-line interface definition and parsing.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// WebScope — autonomous web discovery, crawling and classification.
#[derive(Parser, Debug)]
#[command(
    name = "webscope",
    version,
    about = "Autonomous web discovery, crawling, classification and incremental learning",
    long_about = "WebScope discovers public web pages related to checkout, payment, account \
                  creation and protection mechanisms without requiring an initial URL or query. \
                  It performs passive analysis only."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Increase verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output in JSON where applicable
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start autonomous discovery and crawling
    Scan {
        /// Maximum number of *new* URLs to process in this run
        #[arg(long, default_value = "1000")]
        limit: usize,

        /// Number of concurrent workers
        #[arg(long, default_value = "4")]
        threads: usize,

        /// Resume from previous state (default behaviour)
        #[arg(long, default_value = "true")]
        resume: bool,

        /// Force re-fetch of already processed URLs
        #[arg(long)]
        refresh: bool,

        /// Maximum response size in bytes
        #[arg(long)]
        max_size: Option<usize>,

        /// Request timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Maximum retries per URL
        #[arg(long)]
        retries: Option<u32>,
    },

    /// Show current status of an active or last scan
    Status,

    /// Show aggregated statistics
    Stats,

    /// Manual review of uncertain classifications (active learning)
    Review {
        /// Maximum items to present
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Pattern discovery engine management
    Patterns {
        #[command(subcommand)]
        action: PatternsAction,
    },

    /// Machine learning model management
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },

    /// Export results
    Export {
        /// Output format
        #[arg(long, value_enum, default_value = "json")]
        format: ExportFormat,

        /// Filter by category
        #[arg(long, value_enum)]
        filter: Option<ExportFilter>,

        /// Output file path (stdout if omitted)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Show version information
    Version,
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Show cache statistics
    Stats,
    /// Clear the entire cache (destructive)
    Clear {
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum PatternsAction {
    /// List all patterns
    List,
    /// Show pattern performance statistics
    Stats,
}

#[derive(Subcommand, Debug)]
pub enum ModelAction {
    /// Train or retrain the classifier
    Train,
    /// Evaluate model on held-out test set
    Evaluate,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ExportFilter {
    Payment,
    Account,
    Phone,
    Address,
    Captcha,
    Cloudflare,
}

/// Parse CLI arguments.
pub fn parse() -> Cli {
    Cli::parse()
}
