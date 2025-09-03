//! Network predictor module for WiFi and connection quality prediction
//!
//! This module provides predictive capabilities for:
//! - WiFi network selection
//! - Connection quality prediction
//! - Optimal connection timing
//! - Network availability forecasting

use super::{
    MlError, NetworkContext, NetworkMetrics, PredictionResult,
    TrainingData, ModelPersistence, normalize_features,
};
use std::collections::HashMap;
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};
use log::debug;

/// WiFi network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal_strength: i32,
    pub frequency: u32,
    pub channel: u8,
    pub security: String,
    pub is_saved: bool,
}

/// Network quality prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityPrediction {
    pub expected_latency: f32,
    pub expected_bandwidth: f32,
    pub expected_stability: f32,
    pub connection_success_probability: f32,
}

/// Network predictor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPredictorConfig {
    pub min_signal_strength: i32,
    pub preferred_frequency: u32,
    pub max_channel_overlap: u8,
    pub history_weight: f32,
    pub signal_weight: f32,
    pub security_weight: f32,
}

impl Default for NetworkPredictorConfig {
    fn default() -> Self {
        Self {
            min_signal_strength: -70,
            preferred_frequency: 5000,
            max_channel_overlap: 3,
            history_weight: 0.4,
            signal_weight: 0.35,
            security_weight: 0.25,
        }
    }
}

/// Network predictor for WiFi selection and quality prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPredictor {
    network_history: HashMap<String, Vec<NetworkMetrics>>,
    connection_success: HashMap<String, (u32, u32)>,  // (success, total)
    network_features: HashMap<String, Vec<f32>>,
    training_data: TrainingData<f32>,
    config: NetworkPredictorConfig,
}

impl NetworkPredictor {
    pub fn new() -> Self {
        Self {
            network_history: HashMap::new(),
            connection_success: HashMap::new(),
            network_features: HashMap::new(),
            training_data: TrainingData::default(),
            config: NetworkPredictorConfig::default(),
        }
    }

    pub fn with_config(config: NetworkPredictorConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Predict best WiFi network to connect to
    pub fn predict_best_network(
        &self,
        available_networks: Vec<WifiNetwork>,
        context: &NetworkContext,
    ) -> PredictionResult<String> {
        let mut scored_networks: Vec<(String, f32)> = available_networks
            .iter()
            .filter(|n| n.signal_strength >= self.config.min_signal_strength)
            .map(|network| {
                let score = self.score_network(network, context);
                (network.ssid.clone(), score)
            })
            .collect();

        scored_networks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if let Some((best_ssid, best_score)) = scored_networks.first() {
            let alternatives: Vec<(String, f32)> = scored_networks
                .iter()
                .skip(1)
                .take(2)
                .map(|(ssid, score)| (ssid.clone(), *score))
                .collect();

            let confidence = if scored_networks.len() > 1 {
                let second_score = scored_networks[1].1;
                (best_score - second_score) / best_score
            } else {
                0.7
            };

            PredictionResult::new(best_ssid.clone(), confidence)
                .with_alternatives(alternatives)
        } else {
            PredictionResult::new(String::new(), 0.0)
        }
    }

    /// Score a WiFi network
    fn score_network(&self, network: &WifiNetwork, _context: &NetworkContext) -> f32 {
        let mut score = 0.0;

        // Signal strength score (normalized to 0-1)
        let signal_score = ((network.signal_strength + 100) as f32 / 70.0).clamp(0.0, 1.0);
        score += signal_score * self.config.signal_weight;

        // Frequency preference (5GHz preferred)
        let freq_score = if network.frequency >= 5000 { 1.0 } else { 0.6 };
        score += freq_score * 0.2;

        // Historical performance
        if let Some(history) = self.network_history.get(&network.ssid) {
            if !history.is_empty() {
                let avg_latency = history.iter().map(|m| m.latency_ms).sum::<f32>() / history.len() as f32;
                let latency_score = 1.0 / (1.0 + avg_latency / 50.0);
                score += latency_score * self.config.history_weight;
            }
        }

        // Connection success rate
        if let Some((success, total)) = self.connection_success.get(&network.ssid) {
            if *total > 0 {
                let success_rate = *success as f32 / *total as f32;
                score += success_rate * 0.15;
            }
        }

        // Security preference (WPA3 > WPA2 > WEP > Open)
        let security_score = match network.security.as_str() {
            "WPA3" => 1.0,
            "WPA2" => 0.9,
            "WPA" => 0.7,
            "WEP" => 0.3,
            _ => 0.1,
        };
        score += security_score * self.config.security_weight;

        // Saved network bonus
        if network.is_saved {
            score += 0.1;
        }

        score * 100.0  // Scale to 0-100
    }

    /// Predict connection quality for a network
    pub fn predict_quality(&self, network: &WifiNetwork) -> QualityPrediction {
        let mut connection_success_probability = 0.8;  // Default

        // Adjust based on signal strength
        let signal_factor = ((network.signal_strength + 100) as f32 / 70.0).clamp(0.0, 1.0);
        let mut expected_latency = 20.0 + (1.0 - signal_factor) * 100.0;
        let mut expected_bandwidth = signal_factor * 200.0;
        let mut expected_stability = signal_factor;

        // Adjust based on history
        if let Some(history) = self.network_history.get(&network.ssid) {
            if !history.is_empty() {
                expected_latency = history.iter().map(|m| m.latency_ms).sum::<f32>() / history.len() as f32;
                expected_bandwidth = history.iter().map(|m| m.bandwidth_mbps).sum::<f32>() / history.len() as f32;

                // Calculate stability from packet loss
                let avg_loss = history.iter().map(|m| m.packet_loss).sum::<f32>() / history.len() as f32;
                expected_stability = 1.0 - avg_loss;
            }
        }

        // Adjust success probability based on history
        if let Some((success, total)) = self.connection_success.get(&network.ssid) {
            if *total > 0 {
                connection_success_probability = *success as f32 / *total as f32;
            }
        }

        QualityPrediction {
            expected_latency,
            expected_bandwidth,
            expected_stability,
            connection_success_probability,
        }
    }

    /// Record network performance
    pub fn record_performance(&mut self, ssid: &str, metrics: NetworkMetrics) {
        let history = self.network_history
            .entry(ssid.to_string())
            .or_insert_with(Vec::new);

        history.push(metrics);

        // Limit history size
        if history.len() > 100 {
            history.remove(0);
        }
    }

    /// Record connection attempt result
    pub fn record_connection_attempt(&mut self, ssid: &str, success: bool) {
        let entry = self.connection_success
            .entry(ssid.to_string())
            .or_insert((0, 0));

        entry.1 += 1;  // Total attempts
        if success {
            entry.0 += 1;  // Successful attempts
        }
    }

    /// Extract features for a network
    pub fn extract_network_features(&self, network: &WifiNetwork, context: &NetworkContext) -> Vec<f32> {
        let mut features = vec![
            // Network features
            ((network.signal_strength + 100) as f32 / 70.0).clamp(0.0, 1.0),
            if network.frequency >= 5000 { 1.0 } else { 0.0 },
            network.channel as f32 / 14.0,
            if network.is_saved { 1.0 } else { 0.0 },

            // Context features
            context.time_of_day as f32 / 24.0,
            context.day_of_week as f32 / 7.0,
        ];

        // Add historical features
        if let Some(history) = self.network_history.get(&network.ssid) {
            if !history.is_empty() {
                let avg_latency = history.iter().map(|m| m.latency_ms).sum::<f32>() / history.len() as f32;
                let avg_bandwidth = history.iter().map(|m| m.bandwidth_mbps).sum::<f32>() / history.len() as f32;
                features.push(avg_latency / 1000.0);  // Normalized
                features.push(avg_bandwidth / 1000.0);  // Normalized
            } else {
                features.extend(vec![0.5, 0.5]);  // Default values
            }
        } else {
            features.extend(vec![0.5, 0.5]);  // Default values
        }

        // Add success rate feature
        if let Some((success, total)) = self.connection_success.get(&network.ssid) {
            if *total > 0 {
                features.push(*success as f32 / *total as f32);
            } else {
                features.push(0.5);
            }
        } else {
            features.push(0.5);
        }

        normalize_features(&features)
    }

    /// Predict optimal connection time
    pub fn predict_optimal_connection_time(&self, ssid: &str) -> Option<u8> {
        let history = self.network_history.get(ssid)?;

        if history.len() < 10 {
            return None;
        }

        // Find hour with best average performance
        let mut hourly_performance: HashMap<u8, (f32, u32)> = HashMap::new();

        for metrics in history {
            let hour = (metrics.timestamp % 86400 / 3600) as u8;
            let performance = 100.0 / (1.0 + metrics.latency_ms);

            let entry = hourly_performance.entry(hour).or_insert((0.0, 0));
            entry.0 += performance;
            entry.1 += 1;
        }

        // Find best hour
        hourly_performance
            .into_iter()
            .map(|(hour, (sum, count))| (hour, sum / count as f32))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(hour, _)| hour)
    }
}

impl ModelPersistence for NetworkPredictor {
    fn save(&self, path: &str) -> Result<(), MlError> {
        let model_path = Path::new(path);
        if let Some(parent) = model_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        fs::write(model_path, serialized)?;
        debug!("Network predictor saved to {}", path);

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

        debug!("Network predictor loaded from {}", path);

        Ok(predictor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_network(ssid: &str, signal: i32) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.to_string(),
            bssid: "00:11:22:33:44:55".to_string(),
            signal_strength: signal,
            frequency: 5000,
            channel: 36,
            security: "WPA2".to_string(),
            is_saved: true,
        }
    }

    fn create_test_context() -> NetworkContext {
        NetworkContext {
            time_of_day: 14,
            day_of_week: 2,
            location_hash: 12345,
            network_type: NetworkType::WiFi,
            signal_strength: Some(0.8),
        }
    }

    #[test]
    fn test_predict_best_network() {
        let predictor = NetworkPredictor::new();
        let networks = vec![
            create_test_network("Network1", -50),
            create_test_network("Network2", -60),
            create_test_network("Network3", -70),
        ];
        let context = create_test_context();

        let result = predictor.predict_best_network(networks, &context);

        assert_eq!(result.value, "Network1");  // Best signal
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_predict_quality() {
        let mut predictor = NetworkPredictor::new();
        let network = create_test_network("TestNet", -50);

        // Add some history
        predictor.record_performance("TestNet", NetworkMetrics {
            latency_ms: 25.0,
            packet_loss: 0.01,
            jitter_ms: 2.0,
            bandwidth_mbps: 100.0,
            timestamp: chrono::Utc::now().timestamp(),
        });

        let quality = predictor.predict_quality(&network);

        assert!(quality.expected_latency > 0.0);
        assert!(quality.expected_bandwidth > 0.0);
        assert!(quality.expected_stability > 0.0 && quality.expected_stability <= 1.0);
    }

    #[test]
    fn test_record_connection_attempt() {
        let mut predictor = NetworkPredictor::new();

        predictor.record_connection_attempt("TestNet", true);
        predictor.record_connection_attempt("TestNet", true);
        predictor.record_connection_attempt("TestNet", false);

        let (success, total) = predictor.connection_success.get("TestNet").unwrap();
        assert_eq!(*success, 2);
        assert_eq!(*total, 3);
    }

    #[test]
    fn test_model_persistence() {
        let mut predictor = NetworkPredictor::new();
        predictor.record_connection_attempt("TestNet", true);

        let temp_dir = tempfile::tempdir().unwrap();
        let model_path = temp_dir.path().join("network_model.json");

        // Save model
        predictor.save(model_path.to_str().unwrap()).unwrap();
        assert!(model_path.exists());

        // Load model
        let loaded = NetworkPredictor::load(model_path.to_str().unwrap()).unwrap();
        assert!(loaded.connection_success.contains_key("TestNet"));
    }
}
