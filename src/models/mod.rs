#![allow(dead_code)]
//! Model versioning and evaluation metrics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub version: String,
    pub dataset_hash: Option<String>,
    pub metrics: Option<ModelMetrics>,
    pub sample_count: usize,
    pub features_version: String,
    pub promoted: bool,
}

impl ModelInfo {
    pub fn rules_baseline() -> Self {
        Self {
            version: "0.1.0-rules".into(),
            dataset_hash: None,
            metrics: None,
            sample_count: 0,
            features_version: "1".into(),
            promoted: true,
        }
    }
}
