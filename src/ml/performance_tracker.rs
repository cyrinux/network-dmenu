//! Performance tracker module for monitoring network metrics over time
//!
//! This module tracks and analyzes network performance metrics to:
//! - Monitor connection quality
//! - Detect performance degradation
//! - Provide historical analysis
//! - Generate performance reports

use super::{
    MlError, NetworkMetrics, ModelPersistence,
    exponential_moving_average,
};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};
use log::debug;

/// Performance tracking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub max_history_size: usize,
    pub sampling_interval_seconds: u32,
    pub alert_threshold_latency_ms: f32,
    pub alert_threshold_packet_loss: f32,
    pub smoothing_factor: f32,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_history_size: 10000,
            sampling_interval_seconds: 60,
            alert_threshold_latency_ms: 200.0,
            alert_threshold_packet_loss: 0.05,
            smoothing_factor: 0.3,
        }
    }
}

/// Performance alert types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PerformanceAlert {
    HighLatency(f32),
    PacketLoss(f32),
    JitterSpike(f32),
    BandwidthDrop(f32),
    ConnectionUnstable,
}

/// Performance summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub average_latency: f32,
    pub p95_latency: f32,
    pub p99_latency: f32,
    pub average_packet_loss: f32,
    pub average_jitter: f32,
    pub average_bandwidth: f32,
    pub uptime_percentage: f32,
    pub total_samples: usize,
    pub time_range: (i64, i64),  // Unix timestamps
}

/// Performance tracker for network metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTracker {
    metrics_history: HashMap<String, VecDeque<NetworkMetrics>>,
    smoothed_metrics: HashMap<String, NetworkMetrics>,
    alerts: VecDeque<(i64, PerformanceAlert)>,  // Unix timestamp
    config: PerformanceConfig,
}

impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            metrics_history: HashMap::new(),
            smoothed_metrics: HashMap::new(),
            alerts: VecDeque::new(),
            config: PerformanceConfig::default(),
        }
    }

    pub fn with_config(config: PerformanceConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Record network metrics
    pub fn record_metrics(&mut self, connection_id: &str, metrics: NetworkMetrics) {
        let history = self.metrics_history
            .entry(connection_id.to_string())
            .or_insert_with(VecDeque::new);

        history.push_back(metrics.clone());

        // Limit history size
        while history.len() > self.config.max_history_size {
            history.pop_front();
        }

        // Update smoothed metrics
        self.update_smoothed_metrics(connection_id, &metrics);

        // Check for alerts
        self.check_alerts(connection_id, &metrics);
    }

    /// Update smoothed metrics using exponential moving average
    fn update_smoothed_metrics(&mut self, connection_id: &str, metrics: &NetworkMetrics) {
        if let Some(smoothed) = self.smoothed_metrics.get_mut(connection_id) {
            smoothed.latency_ms = self.config.smoothing_factor * metrics.latency_ms
                + (1.0 - self.config.smoothing_factor) * smoothed.latency_ms;
            smoothed.packet_loss = self.config.smoothing_factor * metrics.packet_loss
                + (1.0 - self.config.smoothing_factor) * smoothed.packet_loss;
            smoothed.jitter_ms = self.config.smoothing_factor * metrics.jitter_ms
                + (1.0 - self.config.smoothing_factor) * smoothed.jitter_ms;
            smoothed.bandwidth_mbps = self.config.smoothing_factor * metrics.bandwidth_mbps
                + (1.0 - self.config.smoothing_factor) * smoothed.bandwidth_mbps;
            smoothed.timestamp = metrics.timestamp;
        } else {
            self.smoothed_metrics.insert(connection_id.to_string(), metrics.clone());
        }
    }

    /// Check for performance alerts
    fn check_alerts(&mut self, _connection_id: &str, metrics: &NetworkMetrics) {
        let now = chrono::Utc::now().timestamp();

        if metrics.latency_ms > self.config.alert_threshold_latency_ms {
            self.alerts.push_back((now, PerformanceAlert::HighLatency(metrics.latency_ms)));
        }

        if metrics.packet_loss > self.config.alert_threshold_packet_loss {
            self.alerts.push_back((now, PerformanceAlert::PacketLoss(metrics.packet_loss)));
        }

        if metrics.jitter_ms > 50.0 {
            self.alerts.push_back((now, PerformanceAlert::JitterSpike(metrics.jitter_ms)));
        }

        if metrics.bandwidth_mbps < 1.0 {
            self.alerts.push_back((now, PerformanceAlert::BandwidthDrop(metrics.bandwidth_mbps)));
        }

        // Limit alerts history
        while self.alerts.len() > 100 {
            self.alerts.pop_front();
        }
    }

    /// Get performance summary for a connection
    pub fn get_summary(&self, connection_id: &str) -> Option<PerformanceSummary> {
        let history = self.metrics_history.get(connection_id)?;

        if history.is_empty() {
            return None;
        }

        let mut latencies: Vec<f32> = history.iter().map(|m| m.latency_ms).collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let avg_latency = latencies.iter().sum::<f32>() / latencies.len() as f32;
        let p95_index = (latencies.len() as f32 * 0.95) as usize;
        let p99_index = (latencies.len() as f32 * 0.99) as usize;

        let avg_packet_loss = history.iter()
            .map(|m| m.packet_loss)
            .sum::<f32>() / history.len() as f32;

        let avg_jitter = history.iter()
            .map(|m| m.jitter_ms)
            .sum::<f32>() / history.len() as f32;

        let avg_bandwidth = history.iter()
            .map(|m| m.bandwidth_mbps)
            .sum::<f32>() / history.len() as f32;

        let uptime = history.iter()
            .filter(|m| m.packet_loss < 1.0)
            .count() as f32 / history.len() as f32;

        let first_timestamp = history.front()?.timestamp;
        let last_timestamp = history.back()?.timestamp;

        Some(PerformanceSummary {
            average_latency: avg_latency,
            p95_latency: latencies.get(p95_index).copied().unwrap_or(avg_latency),
            p99_latency: latencies.get(p99_index).copied().unwrap_or(avg_latency),
            average_packet_loss: avg_packet_loss,
            average_jitter: avg_jitter,
            average_bandwidth: avg_bandwidth,
            uptime_percentage: uptime * 100.0,
            total_samples: history.len(),
            time_range: (first_timestamp, last_timestamp),
        })
    }

    /// Get recent alerts
    pub fn get_recent_alerts(&self, limit: usize) -> Vec<(i64, PerformanceAlert)> {
        self.alerts.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get smoothed metrics for a connection
    pub fn get_smoothed_metrics(&self, connection_id: &str) -> Option<&NetworkMetrics> {
        self.smoothed_metrics.get(connection_id)
    }

    /// Analyze performance trend
    pub fn analyze_trend(&self, connection_id: &str, window_size: usize) -> Option<String> {
        let history = self.metrics_history.get(connection_id)?;

        if history.len() < window_size {
            return None;
        }

        let recent: Vec<f32> = history.iter()
            .rev()
            .take(window_size)
            .map(|m| m.latency_ms)
            .collect();

        let smoothed = exponential_moving_average(&recent, self.config.smoothing_factor);

        let first = smoothed.first()?;
        let last = smoothed.last()?;
        let change_percent = ((last - first) / first) * 100.0;

        if change_percent > 20.0 {
            Some(format!("Performance degrading: {:.1}% increase in latency", change_percent))
        } else if change_percent < -20.0 {
            Some(format!("Performance improving: {:.1}% decrease in latency", change_percent.abs()))
        } else {
            Some("Performance stable".to_string())
        }
    }
}

impl ModelPersistence for PerformanceTracker {
    fn save(&self, path: &str) -> Result<(), MlError> {
        let model_path = Path::new(path);
        if let Some(parent) = model_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        fs::write(model_path, serialized)?;
        debug!("Performance tracker saved to {}", path);

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
        let tracker: Self = serde_json::from_str(&contents)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        debug!("Performance tracker loaded from {}", path);

        Ok(tracker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metrics(latency: f32, loss: f32) -> NetworkMetrics {
        NetworkMetrics {
            latency_ms: latency,
            packet_loss: loss,
            jitter_ms: 5.0,
            bandwidth_mbps: 100.0,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    #[test]
    fn test_record_metrics() {
        let mut tracker = PerformanceTracker::new();
        let metrics = create_test_metrics(50.0, 0.01);

        tracker.record_metrics("connection1", metrics);

        assert!(tracker.metrics_history.contains_key("connection1"));
        assert!(tracker.smoothed_metrics.contains_key("connection1"));
    }

    #[test]
    fn test_performance_alerts() {
        let mut tracker = PerformanceTracker::new();
        let high_latency_metrics = create_test_metrics(250.0, 0.01);

        tracker.record_metrics("connection1", high_latency_metrics);

        let alerts = tracker.get_recent_alerts(10);
        assert!(!alerts.is_empty());
        assert!(matches!(alerts[0].1, PerformanceAlert::HighLatency(_)));
    }

    #[test]
    fn test_performance_summary() {
        let mut tracker = PerformanceTracker::new();

        for i in 0..10 {
            let metrics = create_test_metrics(50.0 + i as f32, 0.01);
            tracker.record_metrics("connection1", metrics);
        }

        let summary = tracker.get_summary("connection1").unwrap();
        assert_eq!(summary.total_samples, 10);
        assert!(summary.average_latency > 50.0);
    }

    #[test]
    fn test_trend_analysis() {
        let mut tracker = PerformanceTracker::new();

        // Degrading performance
        for i in 0..10 {
            let metrics = create_test_metrics(50.0 + (i as f32 * 5.0), 0.01);
            tracker.record_metrics("connection1", metrics);
        }

        let trend = tracker.analyze_trend("connection1", 5).unwrap();
        assert!(trend.contains("degrading"));
    }
}
