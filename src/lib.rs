//! WebScope library surface for tests and tooling.

pub mod cache;
pub mod config;
pub mod detectors;
pub mod error;
pub mod features;
pub mod learning;
pub mod patterns;
pub mod storage;

pub use detectors::detect_all;
pub use learning::{evaluate_on_fixtures, EvaluationReport, print_evaluation};
pub use storage::Database;
