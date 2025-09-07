//! Machine Learning module for intelligent network management
//!
//! This module provides ML-based predictions and optimizations for:
//! - Exit node selection
//! - Network diagnostics
//! - Usage pattern learning
//! - Performance prediction

#[cfg(feature = "ml")]
pub mod action_prioritizer;
#[cfg(feature = "ml")]
pub mod diagnostic_analyzer;
#[cfg(all(feature = "ml", feature = "tailscale"))]
pub mod exit_node_predictor;
#[cfg(feature = "ml")]
pub mod network_predictor;
#[cfg(feature = "ml")]
pub mod performance_tracker;
#[cfg(feature = "ml")]
pub mod usage_patterns;

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

/// Custom error type for ML operations
#[derive(Debug)]
pub enum MlError {
    ModelNotTrained,
    InsufficientData,
    PredictionFailed(String),
    SerializationError(String),
    IoError(std::io::Error),
}

impl fmt::Display for MlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MlError::ModelNotTrained => write!(f, "Model has not been trained yet"),
            MlError::InsufficientData => write!(f, "Insufficient data for training"),
            MlError::PredictionFailed(msg) => write!(f, "Prediction failed: {}", msg),
            MlError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            MlError::IoError(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl Error for MlError {}

impl From<std::io::Error> for MlError {
    fn from(err: std::io::Error) -> Self {
        MlError::IoError(err)
    }
}

/// Network metrics used for ML predictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub latency_ms: f32,
    pub packet_loss: f32,
    pub jitter_ms: f32,
    pub bandwidth_mbps: f32,
    pub timestamp: i64,
}

/// Context information for predictions (time/day fields kept for compatibility but fixed to constants)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkContext {
    pub time_of_day: u8,    // Fixed constant - no time-based logic
    pub day_of_week: u8,    // Fixed constant - no day-based logic  
    pub location_hash: u64, // Hashed location identifier
    pub network_type: NetworkType,
    pub signal_strength: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum NetworkType {
    WiFi,
    Ethernet,
    Mobile,
    VPN,
    Unknown,
}

/// Features extracted from network nodes for ML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeFeatures {
    pub geographic_distance: f32,
    pub historical_latency: f32,
    pub historical_stability: f32,
    pub load_factor: f32,
    pub priority_score: f32,
    pub time_since_last_use: f32,
    pub success_rate: f32,
    pub peak_hour_performance: f32,
}

/// ML model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlConfig {
    pub enabled: bool,
    pub model_path: String,
    pub training_data_path: String,
    pub min_training_samples: usize,
    pub update_frequency_hours: u32,
    pub confidence_threshold: f32,
}

impl Default for MlConfig {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("network-dmenu")
            .join("ml");

        Self {
            enabled: true,
            model_path: data_dir.to_string_lossy().to_string(),
            training_data_path: data_dir.join("data").to_string_lossy().to_string(),
            min_training_samples: 100,
            update_frequency_hours: 24,
            confidence_threshold: 0.7,
        }
    }
}

/// Trait for ML model persistence
pub trait ModelPersistence {
    fn save(&self, path: &str) -> Result<(), MlError>;
    fn load(path: &str) -> Result<Self, MlError>
    where
        Self: Sized;
}

/// Trait for feature extraction
pub trait FeatureExtractor<T> {
    fn extract_features(&self, input: &T, context: &NetworkContext) -> Vec<f32>;
}

/// Generic prediction result with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult<T> {
    pub value: T,
    pub confidence: f32,
    pub alternatives: Vec<(T, f32)>,
}

impl<T> PredictionResult<T> {
    pub fn new(value: T, confidence: f32) -> Self {
        Self {
            value,
            confidence,
            alternatives: Vec::new(),
        }
    }

    pub fn with_alternatives(mut self, alternatives: Vec<(T, f32)>) -> Self {
        self.alternatives = alternatives;
        self
    }

    pub fn is_confident(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}

/// Training data storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingData<T> {
    pub features: Vec<Vec<f32>>,
    pub labels: Vec<T>,
    pub contexts: Vec<NetworkContext>,
    pub timestamps: Vec<i64>,
}

impl<T> Default for TrainingData<T> {
    fn default() -> Self {
        Self {
            features: Vec::new(),
            labels: Vec::new(),
            contexts: Vec::new(),
            timestamps: Vec::new(),
        }
    }
}

impl<T> TrainingData<T> {
    pub fn add_sample(&mut self, features: Vec<f32>, label: T, context: NetworkContext) {
        self.features.push(features);
        self.labels.push(label);
        self.contexts.push(context);
        self.timestamps.push(chrono::Utc::now().timestamp());
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    pub fn has_sufficient_data(&self, min_samples: usize) -> bool {
        self.len() >= min_samples
    }
}

/// Helper function to normalize features
pub fn normalize_features(features: &[f32]) -> Vec<f32> {
    if features.is_empty() {
        return Vec::new();
    }

    let mean = features.iter().sum::<f32>() / features.len() as f32;
    let variance = features.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / features.len() as f32;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        features.to_vec()
    } else {
        features.iter().map(|x| (x - mean) / std_dev).collect()
    }
}

/// Calculate exponential moving average for time series data
pub fn exponential_moving_average(values: &[f32], alpha: f32) -> Vec<f32> {
    if values.is_empty() {
        return Vec::new();
    }

    let mut ema = vec![values[0]];
    for i in 1..values.len() {
        let new_ema = alpha * values[i] + (1.0 - alpha) * ema[i - 1];
        ema.push(new_ema);
    }
    ema
}

/// Calculate similarity between two feature vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_features() {
        let features = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let normalized = normalize_features(&features);

        // Check that mean is approximately 0
        let mean: f32 = normalized.iter().sum::<f32>() / normalized.len() as f32;
        assert!((mean - 0.0).abs() < 0.001);

        // Check that std dev is approximately 1
        let variance: f32 = normalized.iter().map(|x| x * x).sum::<f32>() / normalized.len() as f32;
        assert!((variance - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_exponential_moving_average() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ema = exponential_moving_average(&values, 0.5);

        assert_eq!(ema.len(), values.len());
        assert_eq!(ema[0], 1.0);
        assert!((ema[1] - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < 0.001);

        let c = vec![-1.0, -2.0, -3.0];
        let similarity2 = cosine_similarity(&a, &c);
        assert!((similarity2 + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_prediction_result() {
        let result = PredictionResult::new("test", 0.8)
            .with_alternatives(vec![("alt1", 0.6), ("alt2", 0.4)]);

        assert!(result.is_confident(0.7));
        assert!(!result.is_confident(0.9));
        assert_eq!(result.alternatives.len(), 2);
    }

    #[test]
    fn test_training_data() {
        let mut data: TrainingData<String> = TrainingData::default();
        assert!(data.is_empty());

        let context = NetworkContext {
            time_of_day: 12, // Fixed constant
            day_of_week: 3,  // Fixed constant
            location_hash: 12345,
            network_type: NetworkType::WiFi,
            signal_strength: Some(0.8),
        };

        data.add_sample(vec![1.0, 2.0, 3.0], "label".to_string(), context);
        assert_eq!(data.len(), 1);
        assert!(data.has_sufficient_data(1));
        assert!(!data.has_sufficient_data(2));
    }
}
