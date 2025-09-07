//! Exit node prediction using machine learning
//!
//! This module provides intelligent exit node selection based on:
//! - Historical performance data
//! - Geographic location
//! - Time-based patterns
//! - Network conditions

use super::{
    exponential_moving_average, normalize_features, FeatureExtractor, MlError, ModelPersistence,
    NetworkContext, NetworkMetrics, NodeFeatures, PredictionResult, TrainingData,
};
use crate::tailscale::{TailscaleLocation, TailscalePeer};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[cfg(feature = "ml")]
use smartcore::{
    ensemble::random_forest_regressor::RandomForestRegressor, linalg::basic::matrix::DenseMatrix,
    model_selection::train_test_split,
};

/// Exit node predictor using Random Forest
#[derive(Debug, Serialize, Deserialize)]
pub struct ExitNodePredictor {
    #[serde(skip)]
    model: Option<RandomForestRegressor<f32, f32, DenseMatrix<f32>, Vec<f32>>>,
    performance_history: HashMap<String, Vec<NetworkMetrics>>,
    node_features_cache: HashMap<String, NodeFeatures>,
    training_data: TrainingData<f32>,
    last_trained: Option<i64>,
    config: ExitNodeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitNodeConfig {
    pub max_history_size: usize,
    pub feature_window_size: usize,
    pub alpha: f32, // EMA smoothing factor
    pub distance_weight: f32,
    pub latency_weight: f32,
    pub stability_weight: f32,
    pub priority_weight: f32,
}

impl Default for ExitNodeConfig {
    fn default() -> Self {
        Self {
            max_history_size: 1000,
            feature_window_size: 10,
            alpha: 0.3,
            distance_weight: 0.2,
            latency_weight: 0.35,
            stability_weight: 0.25,
            priority_weight: 0.2,
        }
    }
}

impl Default for ExitNodePredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitNodePredictor {
    pub fn new() -> Self {
        Self {
            model: None,
            performance_history: HashMap::new(),
            node_features_cache: HashMap::new(),
            training_data: TrainingData::default(),
            last_trained: None,
            config: ExitNodeConfig::default(),
        }
    }

    pub fn with_config(config: ExitNodeConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Record performance metrics for a node
    pub fn record_performance(&mut self, node_id: &str, metrics: NetworkMetrics) {
        let history = self
            .performance_history
            .entry(node_id.to_string())
            .or_default();

        history.push(metrics);

        // Limit history size
        if history.len() > self.config.max_history_size {
            history.remove(0);
        }
    }

    /// Extract features for a given node
    pub fn extract_node_features(
        &self,
        peer: &TailscalePeer,
        context: &NetworkContext,
    ) -> NodeFeatures {
        let node_id = peer.dns_name.trim_end_matches('.').to_string();

        // Check cache first
        if let Some(cached) = self.node_features_cache.get(&node_id) {
            return cached.clone();
        }

        // Calculate geographic distance (simplified)
        let geographic_distance = self.calculate_geographic_distance(&peer.location);

        // Get historical metrics
        let (historical_latency, historical_stability, success_rate) =
            self.calculate_historical_metrics(&node_id);

        // Calculate enhanced load factor using new fields
        let load_factor = self.calculate_enhanced_load_factor(peer);

        // Priority score from location
        let priority_score = peer
            .location
            .as_ref()
            .and_then(|l| l.priority)
            .unwrap_or(50) as f32
            / 100.0;

        // Time since last use (in hours)
        let time_since_last_use = self.calculate_time_since_last_use(&node_id);

        // Peak hour performance (simplified)
        let peak_hour_performance = self.calculate_peak_hour_performance(&node_id, context);

        NodeFeatures {
            geographic_distance,
            historical_latency,
            historical_stability,
            load_factor,
            priority_score,
            time_since_last_use,
            success_rate,
            peak_hour_performance,
        }
    }

    fn calculate_enhanced_load_factor(&self, peer: &TailscalePeer) -> f32 {
        let mut load_factor: f32 = 0.0;

        // Base load from online status
        if !peer.online {
            load_factor += 0.5;
        }

        // Active connection adds to load
        if peer.active {
            load_factor += 0.1;
        }

        // Consider connection quality indicators
        if !peer.in_network_map || !peer.in_magic_sock || !peer.in_engine {
            load_factor += 0.2;
        }

        // Check if relay is being used (indicates poor direct connectivity)
        if peer.relay.is_some() && !peer.relay.as_ref().unwrap().is_empty() {
            load_factor += 0.3;
        }

        // Consider data transfer volume (high volume = higher load)
        let total_bytes = peer.rx_bytes + peer.tx_bytes;
        if total_bytes > 1_000_000_000 {
            // More than 1GB
            load_factor += 0.2;
        } else if total_bytes > 100_000_000 {
            // More than 100MB
            load_factor += 0.1;
        }

        // Check last seen time (stale = higher load factor)
        if let Some(last_seen) = &peer.last_seen {
            if let Ok(last_seen_time) = chrono::DateTime::parse_from_rfc3339(last_seen) {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(last_seen_time);
                if duration.num_hours() > 24 {
                    load_factor += 0.3;
                } else if duration.num_hours() > 1 {
                    load_factor += 0.1;
                }
            }
        }

        load_factor.min(1.0_f32) // Cap at 1.0
    }

    fn calculate_connection_quality(&self, peer: &TailscalePeer) -> f32 {
        let mut quality: f32 = 1.0;

        // Direct connection is better than relay
        if peer.relay.is_some() && !peer.relay.as_ref().unwrap().is_empty() {
            quality *= 0.7;
        }

        // Check last handshake time for connection freshness
        if let Some(last_handshake) = &peer.last_handshake {
            if let Ok(handshake_time) = chrono::DateTime::parse_from_rfc3339(last_handshake) {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(handshake_time);
                if duration.num_minutes() < 5 {
                    quality *= 1.0; // Fresh connection
                } else if duration.num_minutes() < 30 {
                    quality *= 0.9;
                } else {
                    quality *= 0.8;
                }
            }
        }

        // Network map presence indicates good connectivity
        if peer.in_network_map && peer.in_magic_sock && peer.in_engine {
            quality *= 1.1;
        }

        quality.min(1.0_f32)
    }

    fn calculate_geographic_distance(&self, location: &Option<TailscaleLocation>) -> f32 {
        // Simplified distance calculation
        // In a real implementation, this would use actual coordinates
        location
            .as_ref()
            .map(|l| {
                // Simulate distance based on priority (lower priority = farther)
                let priority = l.priority.unwrap_or(50) as f32;
                (100.0 - priority) / 100.0
            })
            .unwrap_or(0.5)
    }

    fn calculate_historical_metrics(&self, node_id: &str) -> (f32, f32, f32) {
        if let Some(history) = self.performance_history.get(node_id) {
            if history.is_empty() {
                return (50.0, 0.5, 0.5);
            }

            let latencies: Vec<f32> = history.iter().map(|m| m.latency_ms).collect();

            let smoothed = exponential_moving_average(&latencies, self.config.alpha);
            let avg_latency = smoothed.last().copied().unwrap_or(50.0);

            // Calculate stability (inverse of standard deviation)
            let variance = latencies
                .iter()
                .map(|l| (l - avg_latency).powi(2))
                .sum::<f32>()
                / latencies.len() as f32;
            let stability = 1.0 / (1.0 + variance.sqrt());

            // Calculate success rate (based on packet loss)
            let success_rate =
                history.iter().map(|m| 1.0 - m.packet_loss).sum::<f32>() / history.len() as f32;

            (avg_latency, stability, success_rate)
        } else {
            (50.0, 0.5, 0.5) // Default values
        }
    }

    fn calculate_time_since_last_use(&self, node_id: &str) -> f32 {
        // Legacy method kept for compatibility
        if self.performance_history.contains_key(node_id) {
            1.0 // Recently used
        } else {
            24.0 // Not recently used
        }
    }

    fn calculate_peak_hour_performance(&self, _node_id: &str, _context: &NetworkContext) -> f32 {
        // Simplified - removed time-based logic, return consistent performance
        0.8
    }

    /// Score a node based on features
    pub fn score_node(&self, features: &NodeFeatures) -> f32 {
        // If we have a trained model, use it
        #[cfg(feature = "ml")]
        if let Some(ref model) = self.model {
            let feature_vec = self.features_to_vec(features);
            let normalized = normalize_features(&feature_vec);

            if let Ok(matrix) = DenseMatrix::from_2d_vec(&vec![normalized]) {
                if let Ok(prediction) = model.predict(&matrix) {
                    return prediction[0];
                }
            }
        }

        // Fallback to weighted scoring
        self.weighted_score(features)
    }

    fn weighted_score(&self, features: &NodeFeatures) -> f32 {
        let distance_score = 1.0 - features.geographic_distance;
        let latency_score = 1.0 / (1.0 + features.historical_latency / 100.0);
        let stability_score = features.historical_stability;
        let priority_score = features.priority_score;

        let score = distance_score * self.config.distance_weight
            + latency_score * self.config.latency_weight
            + stability_score * self.config.stability_weight
            + priority_score * self.config.priority_weight;

        // Normalize to 0-100
        score * 100.0
    }

    fn features_to_vec(&self, features: &NodeFeatures) -> Vec<f32> {
        vec![
            features.geographic_distance,
            features.historical_latency,
            features.historical_stability,
            features.load_factor,
            features.priority_score,
            features.time_since_last_use,
            features.success_rate,
            features.peak_hour_performance,
        ]
    }

    /// Predict the best exit nodes
    pub fn predict_best_nodes(
        &self,
        peers: &[TailscalePeer],
        context: &NetworkContext,
        top_n: usize,
    ) -> PredictionResult<Vec<(String, f32)>> {
        let mut scored_nodes: Vec<(String, f32, NodeFeatures)> = peers.iter()
            .filter(|p| p.exit_node_option)
            .map(|peer| {
                let features = self.extract_node_features(peer, context);
                let connection_quality = self.calculate_connection_quality(peer);
                let base_score = self.score_node(&features);
                // Adjust score based on connection quality
                let adjusted_score = base_score * connection_quality;
                let node_id = peer.dns_name.trim_end_matches('.').to_string();

                debug!(
                    "Node {}: base_score={:.2}, connection_quality={:.2}, final_score={:.2}, load={:.2}",
                    node_id, base_score, connection_quality, adjusted_score, features.load_factor
                );

                (node_id, adjusted_score, features)
            })
            .collect();

        // Sort by score (descending)
        scored_nodes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let top_nodes: Vec<(String, f32)> = scored_nodes
            .iter()
            .take(top_n)
            .map(|(id, score, _)| (id.clone(), *score))
            .collect();

        let alternatives: Vec<(Vec<(String, f32)>, f32)> = scored_nodes
            .iter()
            .skip(top_n)
            .take(3)
            .map(|(id, score, _)| (vec![(id.clone(), *score)], *score / 100.0))
            .collect();

        // Calculate confidence based on score distribution
        let confidence = if scored_nodes.len() > 1 {
            let best_score = scored_nodes[0].1;
            let second_score = scored_nodes.get(1).map(|n| n.1).unwrap_or(0.0);
            (best_score - second_score) / best_score
        } else {
            0.5
        };

        PredictionResult::new(top_nodes, confidence).with_alternatives(alternatives)
    }

    /// Train the model with collected data
    #[cfg(feature = "ml")]
    pub fn train(&mut self) -> Result<(), MlError> {
        if !self.training_data.has_sufficient_data(50) {
            return Err(MlError::InsufficientData);
        }

        info!(
            "Training exit node predictor with {} samples",
            self.training_data.len()
        );

        let features = DenseMatrix::from_2d_vec(&self.training_data.features).map_err(|e| {
            MlError::PredictionFailed(format!("Failed to create feature matrix: {}", e))
        })?;
        let labels = self.training_data.labels.clone();

        // Split data for training and testing
        let (x_train, x_test, y_train, y_test) =
            train_test_split(&features, &labels, 0.2, true, Some(42));

        // Train Random Forest model
        let model = RandomForestRegressor::fit(&x_train, &y_train, Default::default())
            .map_err(|e| MlError::PredictionFailed(e.to_string()))?;

        // Evaluate model
        let predictions = model
            .predict(&x_test)
            .map_err(|e| MlError::PredictionFailed(e.to_string()))?;

        // Calculate simple metrics manually
        let mse = y_test
            .iter()
            .zip(predictions.iter())
            .map(|(y, pred)| (y - pred).powi(2))
            .sum::<f32>()
            / y_test.len() as f32;

        let mean_y = y_test.iter().sum::<f32>() / y_test.len() as f32;
        let ss_tot = y_test.iter().map(|y| (y - mean_y).powi(2)).sum::<f32>();
        let ss_res = y_test
            .iter()
            .zip(predictions.iter())
            .map(|(y, pred)| (y - pred).powi(2))
            .sum::<f32>();
        let r2 = 1.0 - (ss_res / ss_tot);

        info!("Model trained - MSE: {:.4}, R²: {:.4}", mse, r2);

        self.model = Some(model);
        self.last_trained = Some(chrono::Utc::now().timestamp());

        Ok(())
    }

    /// Add training sample
    pub fn add_training_sample(
        &mut self,
        features: NodeFeatures,
        actual_performance: f32,
        context: NetworkContext,
    ) {
        let feature_vec = self.features_to_vec(&features);
        self.training_data
            .add_sample(feature_vec, actual_performance, context);
    }
}

impl ModelPersistence for ExitNodePredictor {
    fn save(&self, path: &str) -> Result<(), MlError> {
        let model_path = Path::new(path);
        if let Some(parent) = model_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        fs::write(model_path, serialized)?;
        debug!("Exit node predictor saved to {}", path);

        Ok(())
    }

    fn load(path: &str) -> Result<Self, MlError> {
        let model_path = Path::new(path);
        if !model_path.exists() {
            return Err(MlError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Model file not found",
            )));
        }

        let contents = fs::read_to_string(model_path)?;
        let predictor: Self = serde_json::from_str(&contents)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        debug!("Exit node predictor loaded from {}", path);

        Ok(predictor)
    }
}

impl FeatureExtractor<TailscalePeer> for ExitNodePredictor {
    fn extract_features(&self, peer: &TailscalePeer, context: &NetworkContext) -> Vec<f32> {
        let features = self.extract_node_features(peer, context);
        self.features_to_vec(&features)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::NetworkType;

    fn create_test_peer(dns_name: &str, online: bool, priority: Option<i32>) -> TailscalePeer {
        TailscalePeer {
            dns_name: dns_name.to_string(),
            online,
            exit_node_option: true,
            location: Some(TailscaleLocation {
                country: "US".to_string(),
                country_code: "US".to_string(),
                city: "New York".to_string(),
                city_code: "NYC".to_string(),
                latitude: 40.7128,
                longitude: -74.0060,
                priority,
            }),
            ..Default::default()
        }
    }

    fn create_test_context() -> NetworkContext {
        NetworkContext {
            time_of_day: 12,
            day_of_week: 3,
            location_hash: 12345,
            network_type: NetworkType::WiFi,
            signal_strength: Some(0.8),
        }
    }

    #[test]
    fn test_extract_node_features() {
        let predictor = ExitNodePredictor::new();
        let peer = create_test_peer("test-node.ts.net", true, Some(80));
        let context = create_test_context();

        let features = predictor.extract_node_features(&peer, &context);

        assert!(features.geographic_distance >= 0.0 && features.geographic_distance <= 1.0);
        assert_eq!(features.priority_score, 0.8);
        assert_eq!(features.load_factor, 0.2); // Online node
    }

    #[test]
    fn test_weighted_score() {
        let predictor = ExitNodePredictor::new();

        let features = NodeFeatures {
            geographic_distance: 0.2,
            historical_latency: 20.0,
            historical_stability: 0.9,
            load_factor: 0.3,
            priority_score: 0.8,
            time_since_last_use: 2.0,
            success_rate: 0.95,
            peak_hour_performance: 0.85,
        };

        let score = predictor.weighted_score(&features);
        assert!(score > 0.0 && score <= 100.0);
    }

    #[test]
    fn test_predict_best_nodes() {
        let predictor = ExitNodePredictor::new();
        let peers = vec![
            create_test_peer("node1.ts.net", true, Some(90)),
            create_test_peer("node2.ts.net", true, Some(70)),
            create_test_peer("node3.ts.net", false, Some(80)),
        ];
        let context = create_test_context();

        let result = predictor.predict_best_nodes(&peers, &context, 2);

        assert_eq!(result.value.len(), 2);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);

        // Best node should be node1 (highest priority and online)
        assert!(result.value[0].0.contains("node1"));
    }

    #[test]
    fn test_record_performance() {
        let mut predictor = ExitNodePredictor::new();

        let metrics = NetworkMetrics {
            latency_ms: 25.0,
            packet_loss: 0.01,
            jitter_ms: 2.0,
            bandwidth_mbps: 100.0,
            timestamp: chrono::Utc::now().timestamp(),
        };

        predictor.record_performance("test-node", metrics.clone());

        assert!(predictor.performance_history.contains_key("test-node"));
        assert_eq!(predictor.performance_history["test-node"].len(), 1);
    }

    #[test]
    fn test_add_training_sample() {
        let mut predictor = ExitNodePredictor::new();

        let features = NodeFeatures {
            geographic_distance: 0.3,
            historical_latency: 30.0,
            historical_stability: 0.8,
            load_factor: 0.4,
            priority_score: 0.7,
            time_since_last_use: 5.0,
            success_rate: 0.9,
            peak_hour_performance: 0.75,
        };

        let context = create_test_context();

        predictor.add_training_sample(features, 75.0, context);

        assert_eq!(predictor.training_data.len(), 1);
        assert_eq!(predictor.training_data.labels[0], 75.0);
    }

    #[test]
    fn test_model_persistence() {
        let predictor = ExitNodePredictor::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let model_path = temp_dir.path().join("model.json");

        // Save model
        predictor.save(model_path.to_str().unwrap()).unwrap();
        assert!(model_path.exists());

        // Load model
        let loaded = ExitNodePredictor::load(model_path.to_str().unwrap()).unwrap();
        assert_eq!(
            loaded.config.max_history_size,
            predictor.config.max_history_size
        );
    }
}
