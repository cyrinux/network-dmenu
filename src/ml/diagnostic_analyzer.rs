//! Diagnostic analyzer using machine learning for intelligent troubleshooting
//!
//! This module provides pattern recognition for network issues:
//! - Symptom correlation
//! - Root cause analysis
//! - Predictive failure detection
//! - Smart test recommendations

use super::{
    cosine_similarity, MlError, ModelPersistence, NetworkContext, NetworkMetrics, NetworkType,
    PredictionResult, TrainingData,
};
use chrono::{Datelike, Timelike};
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Network symptoms that can be observed
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetworkSymptom {
    HighLatency,
    PacketLoss,
    JitterSpike,
    DnsFailure,
    ConnectionTimeout,
    SlowThroughput,
    IntermittentConnection,
    NoConnectivity,
    AuthenticationFailure,
    CertificateError,
}

/// Probable causes of network issues
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProbableCause {
    NetworkCongestion,
    DnsServerIssue,
    GatewayProblem,
    WifiInterference,
    VpnConnectionIssue,
    FirewallBlocking,
    MtuSizeMismatch,
    BandwidthThrottling,
    ServerOverload,
    RoutingProblem,
    HardwareFailure,
    ConfigurationError,
}

/// Diagnostic test recommendations
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagnosticTest {
    PingGateway,
    PingDns,
    TracerouteTarget,
    CheckMtu,
    TestDnsResolution,
    CheckRouting,
    MeasureLatency,
    TestBandwidth,
    CheckFirewall,
    VerifyConfiguration,
}

/// Pattern for symptom-cause mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticPattern {
    pub symptoms: Vec<NetworkSymptom>,
    pub cause: ProbableCause,
    pub confidence: f32,
    pub recommended_tests: Vec<DiagnosticTest>,
}

/// Diagnostic analyzer using pattern matching and ML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticAnalyzer {
    patterns: Vec<DiagnosticPattern>,
    symptom_history: Vec<(Vec<NetworkSymptom>, ProbableCause)>,
    feature_weights: HashMap<NetworkSymptom, f32>,
    training_data: TrainingData<ProbableCause>,
    config: DiagnosticConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticConfig {
    pub min_confidence_threshold: f32,
    pub max_history_size: usize,
    pub pattern_match_threshold: f32,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            min_confidence_threshold: 0.6,
            max_history_size: 500,
            pattern_match_threshold: 0.7,
        }
    }
}

impl Default for DiagnosticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticAnalyzer {
    pub fn new() -> Self {
        Self {
            patterns: Self::initialize_patterns(),
            symptom_history: Vec::new(),
            feature_weights: Self::initialize_weights(),
            training_data: TrainingData::default(),
            config: DiagnosticConfig::default(),
        }
    }

    /// Initialize known diagnostic patterns
    fn initialize_patterns() -> Vec<DiagnosticPattern> {
        vec![
            DiagnosticPattern {
                symptoms: vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss],
                cause: ProbableCause::NetworkCongestion,
                confidence: 0.85,
                recommended_tests: vec![
                    DiagnosticTest::MeasureLatency,
                    DiagnosticTest::TracerouteTarget,
                    DiagnosticTest::TestBandwidth,
                ],
            },
            DiagnosticPattern {
                symptoms: vec![NetworkSymptom::DnsFailure, NetworkSymptom::NoConnectivity],
                cause: ProbableCause::DnsServerIssue,
                confidence: 0.9,
                recommended_tests: vec![DiagnosticTest::TestDnsResolution, DiagnosticTest::PingDns],
            },
            DiagnosticPattern {
                symptoms: vec![
                    NetworkSymptom::ConnectionTimeout,
                    NetworkSymptom::NoConnectivity,
                ],
                cause: ProbableCause::GatewayProblem,
                confidence: 0.8,
                recommended_tests: vec![DiagnosticTest::PingGateway, DiagnosticTest::CheckRouting],
            },
            DiagnosticPattern {
                symptoms: vec![
                    NetworkSymptom::IntermittentConnection,
                    NetworkSymptom::JitterSpike,
                ],
                cause: ProbableCause::WifiInterference,
                confidence: 0.75,
                recommended_tests: vec![
                    DiagnosticTest::MeasureLatency,
                    DiagnosticTest::TestBandwidth,
                ],
            },
            DiagnosticPattern {
                symptoms: vec![
                    NetworkSymptom::AuthenticationFailure,
                    NetworkSymptom::ConnectionTimeout,
                ],
                cause: ProbableCause::VpnConnectionIssue,
                confidence: 0.85,
                recommended_tests: vec![
                    DiagnosticTest::VerifyConfiguration,
                    DiagnosticTest::PingGateway,
                ],
            },
            DiagnosticPattern {
                symptoms: vec![NetworkSymptom::SlowThroughput, NetworkSymptom::PacketLoss],
                cause: ProbableCause::MtuSizeMismatch,
                confidence: 0.7,
                recommended_tests: vec![DiagnosticTest::CheckMtu, DiagnosticTest::TestBandwidth],
            },
            DiagnosticPattern {
                symptoms: vec![NetworkSymptom::SlowThroughput],
                cause: ProbableCause::BandwidthThrottling,
                confidence: 0.65,
                recommended_tests: vec![
                    DiagnosticTest::TestBandwidth,
                    DiagnosticTest::MeasureLatency,
                ],
            },
            DiagnosticPattern {
                symptoms: vec![NetworkSymptom::CertificateError],
                cause: ProbableCause::ConfigurationError,
                confidence: 0.9,
                recommended_tests: vec![DiagnosticTest::VerifyConfiguration],
            },
        ]
    }

    /// Initialize symptom feature weights
    fn initialize_weights() -> HashMap<NetworkSymptom, f32> {
        let mut weights = HashMap::new();
        weights.insert(NetworkSymptom::NoConnectivity, 1.0);
        weights.insert(NetworkSymptom::ConnectionTimeout, 0.9);
        weights.insert(NetworkSymptom::PacketLoss, 0.8);
        weights.insert(NetworkSymptom::HighLatency, 0.7);
        weights.insert(NetworkSymptom::DnsFailure, 0.85);
        weights.insert(NetworkSymptom::SlowThroughput, 0.6);
        weights.insert(NetworkSymptom::IntermittentConnection, 0.75);
        weights.insert(NetworkSymptom::JitterSpike, 0.5);
        weights.insert(NetworkSymptom::AuthenticationFailure, 0.8);
        weights.insert(NetworkSymptom::CertificateError, 0.7);
        weights
    }

    /// Analyze symptoms and predict the probable cause
    pub fn analyze_symptoms(&self, symptoms: &[NetworkSymptom]) -> PredictionResult<ProbableCause> {
        let mut cause_scores: HashMap<ProbableCause, f32> = HashMap::new();

        // Pattern matching
        for pattern in &self.patterns {
            let match_score = self.calculate_pattern_match(symptoms, &pattern.symptoms);

            if match_score >= self.config.pattern_match_threshold {
                let score = match_score * pattern.confidence;
                cause_scores
                    .entry(pattern.cause.clone())
                    .and_modify(|s| *s = s.max(score))
                    .or_insert(score);
            }
        }

        // Learn from history
        for (historical_symptoms, historical_cause) in &self.symptom_history {
            let similarity = self.calculate_symptom_similarity(symptoms, historical_symptoms);
            if similarity > 0.5 {
                cause_scores
                    .entry(historical_cause.clone())
                    .and_modify(|s| *s += similarity * 0.3)
                    .or_insert(similarity * 0.3);
            }
        }

        // Find the most likely cause
        let mut sorted_causes: Vec<(ProbableCause, f32)> = cause_scores.into_iter().collect();
        sorted_causes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if let Some((cause, confidence)) = sorted_causes.first() {
            let alternatives: Vec<(ProbableCause, f32)> = sorted_causes
                .iter()
                .skip(1)
                .take(2)
                .map(|(c, s)| (c.clone(), *s))
                .collect();

            PredictionResult::new(cause.clone(), *confidence).with_alternatives(alternatives)
        } else {
            // Default to network congestion if no pattern matches
            PredictionResult::new(ProbableCause::NetworkCongestion, 0.3)
        }
    }

    /// Calculate similarity between symptom sets
    fn calculate_symptom_similarity(
        &self,
        symptoms1: &[NetworkSymptom],
        symptoms2: &[NetworkSymptom],
    ) -> f32 {
        let vec1 = self.symptoms_to_vector(symptoms1);
        let vec2 = self.symptoms_to_vector(symptoms2);
        cosine_similarity(&vec1, &vec2)
    }

    /// Convert symptoms to feature vector
    fn symptoms_to_vector(&self, symptoms: &[NetworkSymptom]) -> Vec<f32> {
        let all_symptoms = [
            NetworkSymptom::HighLatency,
            NetworkSymptom::PacketLoss,
            NetworkSymptom::JitterSpike,
            NetworkSymptom::DnsFailure,
            NetworkSymptom::ConnectionTimeout,
            NetworkSymptom::SlowThroughput,
            NetworkSymptom::IntermittentConnection,
            NetworkSymptom::NoConnectivity,
            NetworkSymptom::AuthenticationFailure,
            NetworkSymptom::CertificateError,
        ];

        all_symptoms
            .iter()
            .map(|s| {
                if symptoms.contains(s) {
                    *self.feature_weights.get(s).unwrap_or(&0.5)
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// Calculate pattern match score
    fn calculate_pattern_match(
        &self,
        observed: &[NetworkSymptom],
        pattern: &[NetworkSymptom],
    ) -> f32 {
        if pattern.is_empty() {
            return 0.0;
        }

        let matched = pattern.iter().filter(|s| observed.contains(s)).count() as f32;

        let precision = matched / observed.len().max(1) as f32;
        let recall = matched / pattern.len() as f32;

        // F1 score
        if precision + recall > 0.0 {
            2.0 * (precision * recall) / (precision + recall)
        } else {
            0.0
        }
    }

    /// Get recommended diagnostic tests for symptoms
    pub fn recommend_tests(&self, symptoms: &[NetworkSymptom]) -> Vec<DiagnosticTest> {
        let mut test_scores: HashMap<DiagnosticTest, f32> = HashMap::new();

        for pattern in &self.patterns {
            let match_score = self.calculate_pattern_match(symptoms, &pattern.symptoms);

            if match_score >= self.config.pattern_match_threshold {
                for test in &pattern.recommended_tests {
                    test_scores
                        .entry(test.clone())
                        .and_modify(|s| *s += match_score)
                        .or_insert(match_score);
                }
            }
        }

        // Sort tests by score
        let mut sorted_tests: Vec<(DiagnosticTest, f32)> = test_scores.into_iter().collect();
        sorted_tests.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        sorted_tests.into_iter().map(|(test, _)| test).collect()
    }

    /// Analyze network metrics to detect symptoms
    pub fn detect_symptoms(&self, metrics: &NetworkMetrics) -> Vec<NetworkSymptom> {
        let mut symptoms = Vec::new();

        if metrics.latency_ms > 100.0 {
            symptoms.push(NetworkSymptom::HighLatency);
        }

        if metrics.packet_loss > 0.05 {
            symptoms.push(NetworkSymptom::PacketLoss);
        }

        if metrics.jitter_ms > 20.0 {
            symptoms.push(NetworkSymptom::JitterSpike);
        }

        if metrics.bandwidth_mbps < 1.0 {
            symptoms.push(NetworkSymptom::SlowThroughput);
        }

        symptoms
    }

    /// Predict potential failures based on metrics trend
    pub fn predict_failure(
        &self,
        metrics_history: &[NetworkMetrics],
    ) -> Option<PredictionResult<NetworkSymptom>> {
        if metrics_history.len() < 5 {
            return None;
        }

        // Analyze trends
        let recent_metrics = &metrics_history[metrics_history.len() - 5..];

        let latency_trend: Vec<f32> = recent_metrics.iter().map(|m| m.latency_ms).collect();

        let packet_loss_trend: Vec<f32> = recent_metrics.iter().map(|m| m.packet_loss).collect();

        // Check for degradation patterns
        let latency_increasing = Self::is_increasing_trend(&latency_trend);
        let packet_loss_increasing = Self::is_increasing_trend(&packet_loss_trend);

        if latency_increasing && latency_trend.last().copied().unwrap_or(0.0) > 80.0 {
            return Some(PredictionResult::new(NetworkSymptom::HighLatency, 0.75));
        }

        if packet_loss_increasing && packet_loss_trend.last().copied().unwrap_or(0.0) > 0.03 {
            return Some(PredictionResult::new(NetworkSymptom::PacketLoss, 0.7));
        }

        None
    }

    /// Check if values show an increasing trend
    fn is_increasing_trend(values: &[f32]) -> bool {
        if values.len() < 2 {
            return false;
        }

        let mut increasing_count = 0;
        for i in 1..values.len() {
            if values[i] > values[i - 1] {
                increasing_count += 1;
            }
        }

        increasing_count as f32 / (values.len() - 1) as f32 > 0.6
    }

    /// Record a diagnosed issue for learning
    pub fn record_diagnosis(&mut self, symptoms: Vec<NetworkSymptom>, actual_cause: ProbableCause) {
        self.symptom_history
            .push((symptoms.clone(), actual_cause.clone()));

        // Limit history size
        if self.symptom_history.len() > self.config.max_history_size {
            self.symptom_history.remove(0);
        }

        // Update training data
        let feature_vec = self.symptoms_to_vector(&symptoms);
        let context = NetworkContext {
            time_of_day: chrono::Local::now().hour() as u8,
            day_of_week: chrono::Local::now().weekday().num_days_from_monday() as u8,
            location_hash: 0,
            network_type: NetworkType::Unknown,
            signal_strength: None,
        };

        self.training_data
            .add_sample(feature_vec, actual_cause, context);
    }

    /// Update pattern confidence based on feedback
    pub fn update_pattern_confidence(&mut self, pattern_index: usize, success: bool) {
        if let Some(pattern) = self.patterns.get_mut(pattern_index) {
            // Simple exponential moving average update
            let adjustment = if success { 0.05 } else { -0.05 };
            pattern.confidence = (pattern.confidence + adjustment).clamp(0.1, 1.0);
        }
    }
}

impl ModelPersistence for DiagnosticAnalyzer {
    fn save(&self, path: &str) -> Result<(), MlError> {
        let model_path = Path::new(path);
        if let Some(parent) = model_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        fs::write(model_path, serialized)?;
        debug!("Diagnostic analyzer saved to {}", path);

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
        let analyzer: Self = serde_json::from_str(&contents)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        debug!("Diagnostic analyzer loaded from {}", path);

        Ok(analyzer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_symptoms() {
        let analyzer = DiagnosticAnalyzer::new();
        let symptoms = vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss];

        let result = analyzer.analyze_symptoms(&symptoms);

        assert_eq!(result.value, ProbableCause::NetworkCongestion);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn test_recommend_tests() {
        let analyzer = DiagnosticAnalyzer::new();
        let symptoms = vec![NetworkSymptom::DnsFailure, NetworkSymptom::NoConnectivity];

        let tests = analyzer.recommend_tests(&symptoms);

        assert!(!tests.is_empty());
        assert!(tests.contains(&DiagnosticTest::TestDnsResolution));
    }

    #[test]
    fn test_detect_symptoms() {
        let analyzer = DiagnosticAnalyzer::new();
        let metrics = NetworkMetrics {
            latency_ms: 150.0,
            packet_loss: 0.1,
            jitter_ms: 25.0,
            bandwidth_mbps: 0.5,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let symptoms = analyzer.detect_symptoms(&metrics);

        assert!(symptoms.contains(&NetworkSymptom::HighLatency));
        assert!(symptoms.contains(&NetworkSymptom::PacketLoss));
        assert!(symptoms.contains(&NetworkSymptom::JitterSpike));
        assert!(symptoms.contains(&NetworkSymptom::SlowThroughput));
    }

    #[test]
    fn test_predict_failure() {
        let analyzer = DiagnosticAnalyzer::new();

        let mut metrics_history = Vec::new();
        for i in 0..5 {
            metrics_history.push(NetworkMetrics {
                latency_ms: 50.0 + (i as f32 * 20.0),
                packet_loss: 0.01 + (i as f32 * 0.01),
                jitter_ms: 5.0,
                bandwidth_mbps: 100.0,
                timestamp: chrono::Utc::now().timestamp() + i,
            });
        }

        let prediction = analyzer.predict_failure(&metrics_history);

        assert!(prediction.is_some());
        if let Some(pred) = prediction {
            assert_eq!(pred.value, NetworkSymptom::HighLatency);
        }
    }

    #[test]
    fn test_pattern_matching() {
        let analyzer = DiagnosticAnalyzer::new();

        let observed = vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss];
        let pattern = vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss];

        let score = analyzer.calculate_pattern_match(&observed, &pattern);
        assert_eq!(score, 1.0); // Perfect match

        let partial_pattern = vec![NetworkSymptom::HighLatency];
        let partial_score = analyzer.calculate_pattern_match(&observed, &partial_pattern);
        assert!(partial_score > 0.0 && partial_score < 1.0);
    }

    #[test]
    fn test_symptom_similarity() {
        let analyzer = DiagnosticAnalyzer::new();

        let symptoms1 = vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss];
        let symptoms2 = vec![NetworkSymptom::HighLatency, NetworkSymptom::PacketLoss];

        let similarity = analyzer.calculate_symptom_similarity(&symptoms1, &symptoms2);
        assert!(similarity > 0.99); // Nearly identical

        let symptoms3 = vec![NetworkSymptom::DnsFailure];
        let low_similarity = analyzer.calculate_symptom_similarity(&symptoms1, &symptoms3);
        assert!(low_similarity < 0.5);
    }

    #[test]
    fn test_record_diagnosis() {
        let mut analyzer = DiagnosticAnalyzer::new();

        let symptoms = vec![NetworkSymptom::HighLatency];
        let cause = ProbableCause::NetworkCongestion;

        analyzer.record_diagnosis(symptoms.clone(), cause.clone());

        assert_eq!(analyzer.symptom_history.len(), 1);
        assert_eq!(analyzer.training_data.len(), 1);
    }

    #[test]
    fn test_model_persistence() {
        let analyzer = DiagnosticAnalyzer::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let model_path = temp_dir.path().join("diagnostic_model.json");

        // Save model
        analyzer.save(model_path.to_str().unwrap()).unwrap();
        assert!(model_path.exists());

        // Load model
        let loaded = DiagnosticAnalyzer::load(model_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.patterns.len(), analyzer.patterns.len());
    }
}
