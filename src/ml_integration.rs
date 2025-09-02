//! Machine Learning integration module for network-dmenu
//!
//! This module provides integration points for ML features throughout the application,
//! making it easy to use ML predictions and learning capabilities without disrupting
//! the existing functional programming patterns.

#[cfg(feature = "ml")]
use crate::ml::{
    diagnostic_analyzer::{DiagnosticAnalyzer, NetworkSymptom, DiagnosticTest},
    exit_node_predictor::ExitNodePredictor,
    network_predictor::{NetworkPredictor, WifiNetwork},
    performance_tracker::PerformanceTracker,
    usage_patterns::{UsagePatternLearner, UserAction},
    MlConfig, NetworkContext, NetworkMetrics, NetworkType, ModelPersistence,
};

use crate::tailscale::TailscalePeer;
use log::{debug, error, info};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

#[cfg(feature = "ml")]
static ML_MANAGER: Lazy<Arc<Mutex<MlManager>>> = Lazy::new(|| {
    Arc::new(Mutex::new(MlManager::new()))
});

/// Central ML manager for coordinating all ML components
#[cfg(feature = "ml")]
pub struct MlManager {
    config: MlConfig,
    exit_node_predictor: ExitNodePredictor,
    diagnostic_analyzer: DiagnosticAnalyzer,
    network_predictor: NetworkPredictor,
    performance_tracker: PerformanceTracker,
    usage_learner: UsagePatternLearner,
    initialized: bool,
}

#[cfg(feature = "ml")]
impl MlManager {
    pub fn new() -> Self {
        let config = MlConfig::default();

        Self {
            config: config.clone(),
            exit_node_predictor: ExitNodePredictor::new(),
            diagnostic_analyzer: DiagnosticAnalyzer::new(),
            network_predictor: NetworkPredictor::new(),
            performance_tracker: PerformanceTracker::new(),
            usage_learner: UsagePatternLearner::new(),
            initialized: false,
        }
    }

    /// Initialize ML models by loading from disk if available
    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }

        // Try to load existing models
        let model_base = &self.config.model_path;

        // Create directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&model_base) {
            error!("Failed to create ML model directory: {}", e);
        }

        let mut loaded_any = false;

        if let Ok(predictor) = ExitNodePredictor::load(&format!("{}/exit_node.json", model_base)) {
            self.exit_node_predictor = predictor;
            debug!("Loaded exit node predictor from disk");
            loaded_any = true;
        }

        if let Ok(analyzer) = DiagnosticAnalyzer::load(&format!("{}/diagnostic.json", model_base)) {
            self.diagnostic_analyzer = analyzer;
            debug!("Loaded diagnostic analyzer from disk");
            loaded_any = true;
        }

        if let Ok(predictor) = NetworkPredictor::load(&format!("{}/network.json", model_base)) {
            self.network_predictor = predictor;
            debug!("Loaded network predictor from disk");
            loaded_any = true;
        }

        if let Ok(tracker) = PerformanceTracker::load(&format!("{}/performance.json", model_base)) {
            self.performance_tracker = tracker;
            debug!("Loaded performance tracker from disk");
            loaded_any = true;
        }

        if let Ok(learner) = UsagePatternLearner::load(&format!("{}/usage.json", model_base)) {
            self.usage_learner = learner;
            debug!("Loaded usage pattern learner from disk");
            loaded_any = true;
        }

        self.initialized = true;
        info!("ML Manager initialized");

        // Save initial models if none were loaded (first run)
        if !loaded_any {
            info!("First run detected, saving initial ML models");
            if let Err(e) = self.save_models() {
                error!("Failed to save initial ML models: {}", e);
            }
        }
    }

    /// Save all models to disk
    pub fn save_models(&self) -> Result<(), Box<dyn std::error::Error>> {
        let model_base = &self.config.model_path;

        self.exit_node_predictor.save(&format!("{}/exit_node.json", model_base))?;
        self.diagnostic_analyzer.save(&format!("{}/diagnostic.json", model_base))?;
        self.network_predictor.save(&format!("{}/network.json", model_base))?;
        self.performance_tracker.save(&format!("{}/performance.json", model_base))?;
        self.usage_learner.save(&format!("{}/usage.json", model_base))?;

        info!("All ML models saved successfully");
        Ok(())
    }
}

/// Get current network context for ML predictions
#[cfg(feature = "ml")]
pub fn get_current_context() -> NetworkContext {
    use chrono::{Local, Timelike, Datelike};

    let now = Local::now();

    NetworkContext {
        time_of_day: now.hour() as u8,
        day_of_week: now.weekday().num_days_from_monday() as u8,
        location_hash: get_location_hash(),
        network_type: detect_network_type(),
        signal_strength: get_signal_strength(),
    }
}

#[cfg(feature = "ml")]
fn get_location_hash() -> u64 {
    // Hash based on current network SSID or gateway
    // This is a simplified implementation
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    if let Ok(output) = std::process::Command::new("ip")
        .args(&["route", "show", "default"])
        .output()
    {
        output.stdout.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(feature = "ml")]
fn detect_network_type() -> NetworkType {
    // Detect current network type
    if let Ok(output) = std::process::Command::new("ip")
        .args(&["link", "show"])
        .output()
    {
        let output_str = String::from_utf8_lossy(&output.stdout);
        if output_str.contains("wlan") || output_str.contains("wlp") {
            return NetworkType::WiFi;
        } else if output_str.contains("eth") || output_str.contains("enp") {
            return NetworkType::Ethernet;
        }
    }
    NetworkType::Unknown
}

#[cfg(feature = "ml")]
fn get_signal_strength() -> Option<f32> {
    // Get WiFi signal strength if available
    if let Ok(output) = std::process::Command::new("iwconfig")
        .output()
    {
        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = output_str.lines().find(|l| l.contains("Signal level")) {
            if let Some(signal) = line.split("Signal level=").nth(1) {
                if let Some(dbm) = signal.split_whitespace().next() {
                    if let Ok(value) = dbm.replace("dBm", "").parse::<f32>() {
                        // Convert dBm to percentage (rough approximation)
                        return Some(((value + 100.0) / 70.0).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }
    None
}

/// Predict best exit nodes using ML
#[cfg(feature = "ml")]
pub fn predict_best_exit_nodes(
    peers: &[TailscalePeer],
    top_n: usize,
) -> Vec<(String, f32)> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();
    let result = manager.exit_node_predictor.predict_best_nodes(peers, &context, top_n);

    if result.confidence > manager.config.confidence_threshold {
        result.value
    } else {
        // Fall back to non-ML selection if confidence is low
        Vec::new()
    }
}

/// Record exit node performance for learning
#[cfg(feature = "ml")]
pub fn record_exit_node_performance(node_id: &str, latency: f32, packet_loss: f32) {
    let mut manager = ML_MANAGER.lock().unwrap();

    let metrics = NetworkMetrics {
        latency_ms: latency,
        packet_loss,
        jitter_ms: 0.0,  // Would need to calculate this properly
        bandwidth_mbps: 0.0,  // Would need to measure this
        timestamp: chrono::Utc::now().timestamp(),
    };

    manager.exit_node_predictor.record_performance(node_id, metrics.clone());
    manager.performance_tracker.record_metrics(node_id, metrics);

    // Periodically save models (every 10 actions for better persistence)
    static SAVE_COUNTER: Lazy<Arc<Mutex<u32>>> = Lazy::new(|| Arc::new(Mutex::new(0)));
    let mut counter = SAVE_COUNTER.lock().unwrap();
    *counter += 1;

    if *counter % 10 == 0 {
        debug!("Auto-saving ML models (action count: {})", *counter);
        if let Err(e) = manager.save_models() {
            error!("Failed to save ML models: {}", e);
        }
    }
}

/// Analyze network symptoms and get diagnostic recommendations
#[cfg(feature = "ml")]
pub fn analyze_network_issues(symptoms: Vec<&str>) -> (String, Vec<String>) {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    // Convert string symptoms to enum
    let symptom_enums: Vec<NetworkSymptom> = symptoms.iter()
        .filter_map(|s| match *s {
            "high_latency" => Some(NetworkSymptom::HighLatency),
            "packet_loss" => Some(NetworkSymptom::PacketLoss),
            "dns_failure" => Some(NetworkSymptom::DnsFailure),
            "no_connectivity" => Some(NetworkSymptom::NoConnectivity),
            "slow_throughput" => Some(NetworkSymptom::SlowThroughput),
            _ => None,
        })
        .collect();

    let cause_result = manager.diagnostic_analyzer.analyze_symptoms(&symptom_enums);
    let tests = manager.diagnostic_analyzer.recommend_tests(&symptom_enums);

    let cause_str = format!("{:?}", cause_result.value);
    let test_strs: Vec<String> = tests.iter()
        .map(|t| match t {
            DiagnosticTest::PingGateway => "Ping Gateway".to_string(),
            DiagnosticTest::PingDns => "Ping DNS".to_string(),
            DiagnosticTest::TracerouteTarget => "Traceroute".to_string(),
            DiagnosticTest::CheckMtu => "Check MTU".to_string(),
            DiagnosticTest::TestDnsResolution => "Test DNS".to_string(),
            DiagnosticTest::CheckRouting => "Check Routing".to_string(),
            DiagnosticTest::MeasureLatency => "Measure Latency".to_string(),
            DiagnosticTest::TestBandwidth => "Speed Test".to_string(),
            _ => "Other Test".to_string(),
        })
        .collect();

    (cause_str, test_strs)
}

/// Get personalized menu ordering based on usage patterns
#[cfg(feature = "ml")]
pub fn get_personalized_menu_order(menu_items: Vec<String>) -> Vec<String> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();
    manager.usage_learner.get_personalized_menu_order(menu_items, &context)
}

/// Record user action for learning
#[cfg(feature = "ml")]
pub fn record_user_action(action: &str) {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let user_action = parse_user_action(action);
    let context = get_current_context();

    manager.usage_learner.record_action(user_action, context);

    // Save on every 5th user action for better persistence
    static ACTION_COUNTER: Lazy<Arc<Mutex<u32>>> = Lazy::new(|| Arc::new(Mutex::new(0)));
    let mut counter = ACTION_COUNTER.lock().unwrap();
    *counter += 1;

    if *counter % 5 == 0 {
        debug!("Auto-saving ML models after user action (count: {})", *counter);
        if let Err(e) = manager.save_models() {
            error!("Failed to save ML models: {}", e);
        }
    }
}

#[cfg(feature = "ml")]
fn parse_user_action(action: &str) -> UserAction {
    if action.contains("WiFi") && action.contains("Connect") {
        UserAction::ConnectWifi(action.to_string())
    } else if action.contains("WiFi") && action.contains("Disconnect") {
        UserAction::DisconnectWifi
    } else if action.contains("Bluetooth") && action.contains("Connect") {
        UserAction::ConnectBluetooth(action.to_string())
    } else if action.contains("Bluetooth") && action.contains("Disconnect") {
        UserAction::DisconnectBluetooth
    } else if action.contains("Enable Tailscale") {
        UserAction::EnableTailscale
    } else if action.contains("Disable Tailscale") {
        UserAction::DisableTailscale
    } else if action.contains("Exit Node") {
        UserAction::SelectExitNode(action.to_string())
    } else if action.contains("Diagnostic") {
        UserAction::RunDiagnostic(action.to_string())
    } else {
        UserAction::CustomAction(action.to_string())
    }
}

/// Predict best WiFi network to connect
#[cfg(feature = "ml")]
pub fn predict_best_wifi_network(networks: Vec<(String, i32, String)>) -> Option<String> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let wifi_networks: Vec<WifiNetwork> = networks.into_iter()
        .map(|(ssid, signal, security)| WifiNetwork {
            ssid: ssid.clone(),
            bssid: String::new(),
            signal_strength: signal,
            frequency: 2400,  // Default, would need to detect
            channel: 1,  // Default
            security,
            is_saved: false,  // Would need to check
        })
        .collect();

    let context = get_current_context();
    let result = manager.network_predictor.predict_best_network(wifi_networks, &context);

    if result.confidence > manager.config.confidence_threshold {
        Some(result.value)
    } else {
        None
    }
}

/// Record WiFi network performance
#[cfg(feature = "ml")]
pub fn record_wifi_performance(ssid: &str, latency: f32, bandwidth: f32) {
    let mut manager = ML_MANAGER.lock().unwrap();

    let metrics = NetworkMetrics {
        latency_ms: latency,
        packet_loss: 0.0,
        jitter_ms: 0.0,
        bandwidth_mbps: bandwidth,
        timestamp: chrono::Utc::now().timestamp(),
    };

    manager.network_predictor.record_performance(ssid, metrics.clone());
    manager.performance_tracker.record_metrics(ssid, metrics);
}

/// Initialize ML system and save initial models if needed
#[cfg(feature = "ml")]
pub fn initialize_ml_system() {
    let mut manager = ML_MANAGER.lock().unwrap();
    if !manager.initialized {
        info!("Initializing ML system on first use");
        manager.initialize();
    }
}

/// Force save all ML models to disk
#[cfg(feature = "ml")]
pub fn force_save_ml_models() -> Result<(), Box<dyn std::error::Error>> {
    let manager = ML_MANAGER.lock().unwrap();
    if manager.initialized {
        info!("Force saving ML models to disk");
        manager.save_models()
    } else {
        Ok(())
    }
}

/// Get performance summary for reporting
#[cfg(feature = "ml")]
pub fn get_performance_summary(connection_id: &str) -> Option<String> {
    let manager = ML_MANAGER.lock().unwrap();

    if let Some(summary) = manager.performance_tracker.get_summary(connection_id) {
        Some(format!(
            "📊 Performance Summary for {}\n\
             Average Latency: {:.1}ms\n\
             P95 Latency: {:.1}ms\n\
             Packet Loss: {:.2}%\n\
             Uptime: {:.1}%\n\
             Samples: {}",
            connection_id,
            summary.average_latency,
            summary.p95_latency,
            summary.average_packet_loss * 100.0,
            summary.uptime_percentage,
            summary.total_samples
        ))
    } else {
        None
    }
}

// Non-ML fallback functions for when ML feature is disabled

#[cfg(not(feature = "ml"))]
pub fn predict_best_exit_nodes(_peers: &[TailscalePeer], _top_n: usize) -> Vec<(String, f32)> {
    Vec::new()
}

#[cfg(not(feature = "ml"))]
pub fn record_exit_node_performance(_node_id: &str, _latency: f32, _packet_loss: f32) {}

#[cfg(not(feature = "ml"))]
pub fn analyze_network_issues(_symptoms: Vec<&str>) -> (String, Vec<String>) {
    ("Unknown".to_string(), Vec::new())
}

#[cfg(not(feature = "ml"))]
pub fn get_personalized_menu_order(menu_items: Vec<String>) -> Vec<String> {
    menu_items
}

#[cfg(not(feature = "ml"))]
pub fn record_user_action(_action: &str) {}

#[cfg(not(feature = "ml"))]
pub fn predict_best_wifi_network(_networks: Vec<(String, i32, String)>) -> Option<String> {
    None
}

#[cfg(not(feature = "ml"))]
pub fn record_wifi_performance(_ssid: &str, _latency: f32, _bandwidth: f32) {}

#[cfg(not(feature = "ml"))]
pub fn get_performance_summary(_connection_id: &str) -> Option<String> {
    None
}

#[cfg(not(feature = "ml"))]
pub fn initialize_ml_system() {}

#[cfg(not(feature = "ml"))]
pub fn force_save_ml_models() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
