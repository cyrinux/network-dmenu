//! Machine Learning integration module for network-dmenu
//!
//! This module provides integration points for ML features throughout the application,
//! making it easy to use ML predictions and learning capabilities without disrupting
//! the existing functional programming patterns.

#[cfg(feature = "ml")]
use crate::ml::{
    action_prioritizer::ActionPrioritizer,
    diagnostic_analyzer::{DiagnosticAnalyzer, DiagnosticTest, NetworkSymptom},
    network_predictor::{NetworkPredictor, WifiNetwork},
    performance_tracker::PerformanceTracker,
    usage_patterns::{UsagePatternLearner, UserAction},
    MlConfig, ModelPersistence, NetworkContext, NetworkMetrics, NetworkType,
};

#[cfg(all(feature = "ml", feature = "tailscale"))]
use crate::ml::exit_node_predictor::ExitNodePredictor;

#[cfg(feature = "tailscale")]
use crate::tailscale::TailscalePeer;

#[cfg(feature = "ml")]
use log::{debug, error, info};
#[cfg(feature = "ml")]
use once_cell::sync::Lazy;
#[cfg(feature = "ml")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "ml")]
static ML_MANAGER: Lazy<Arc<Mutex<MlManager>>> =
    Lazy::new(|| Arc::new(Mutex::new(MlManager::new())));

/// Central ML manager for coordinating all ML components
#[cfg(feature = "ml")]
pub struct MlManager {
    config: MlConfig,
    #[cfg(feature = "tailscale")]
    exit_node_predictor: ExitNodePredictor,
    diagnostic_analyzer: DiagnosticAnalyzer,
    network_predictor: NetworkPredictor,
    performance_tracker: PerformanceTracker,
    usage_learner: UsagePatternLearner,
    action_prioritizer: ActionPrioritizer,
    initialized: bool,
}

#[cfg(feature = "ml")]
impl Default for MlManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "ml")]
impl MlManager {
    pub fn new() -> Self {
        let config = MlConfig::default();

        Self {
            config: config.clone(),
            #[cfg(feature = "tailscale")]
            exit_node_predictor: ExitNodePredictor::new(),
            diagnostic_analyzer: DiagnosticAnalyzer::new(),
            network_predictor: NetworkPredictor::new(),
            performance_tracker: PerformanceTracker::new(),
            usage_learner: UsagePatternLearner::new(),
            action_prioritizer: ActionPrioritizer::new(),
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
        if let Err(e) = std::fs::create_dir_all(model_base) {
            error!("Failed to create ML model directory: {}", e);
        }

        let mut loaded_any = false;

        #[cfg(feature = "tailscale")]
        {
            if let Ok(predictor) =
                ExitNodePredictor::load(&format!("{}/exit_node.json", model_base))
            {
                self.exit_node_predictor = predictor;
                debug!("Loaded exit node predictor from disk");
                loaded_any = true;
            }
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

        #[cfg(feature = "tailscale")]
        self.exit_node_predictor
            .save(&format!("{}/exit_node.json", model_base))?;
        self.diagnostic_analyzer
            .save(&format!("{}/diagnostic.json", model_base))?;
        self.network_predictor
            .save(&format!("{}/network.json", model_base))?;
        self.performance_tracker
            .save(&format!("{}/performance.json", model_base))?;
        self.usage_learner
            .save(&format!("{}/usage.json", model_base))?;

        info!("All ML models saved successfully");
        Ok(())
    }

    /// Generate zone suggestions based on ML analysis
    pub fn generate_zone_suggestions(&self, zones: &[crate::geofencing::GeofenceZone]) -> Vec<crate::geofencing::ipc::ZoneSuggestion> {
        use crate::geofencing::ipc::{ZoneSuggestion, SuggestionEvidence, SuggestionPriority};
        
        let context = get_current_context();
        let mut suggestions = Vec::new();
        
        // Simple suggestion logic - can be enhanced with more sophisticated ML
        if zones.len() < 3 {
            suggestions.push(ZoneSuggestion {
                suggested_name: format!("auto-location-{}", context.location_hash % 1000),
                confidence: 0.75,
                reasoning: "Detected new location pattern, suggest creating a zone".to_string(),
                evidence: SuggestionEvidence {
                    visit_count: 1,
                    total_time: std::time::Duration::from_secs(0),
                    average_visit_duration: std::time::Duration::from_secs(0),
                    common_visit_times: vec![format!("{}:00", context.time_of_day)],
                    common_actions: vec!["zone_creation".to_string()],
                    similar_zones: vec![],
                },
                created_at: chrono::Utc::now(),
                priority: SuggestionPriority::Medium,
            });
        }
        
        debug!("Generated {} zone suggestions", suggestions.len());
        suggestions
    }

    /// Record a zone change for ML learning
    pub fn record_zone_change(&mut self, from_zone_id: Option<&str>, to_zone_id: &str) {
        debug!("Recording zone change from {:?} to {:?}", from_zone_id, to_zone_id);
        
        // Update usage patterns with basic zone switch action
        let user_action = crate::ml::usage_patterns::UserAction::ConnectWifi(
            format!("zone-{}", to_zone_id)
        );
        
        let context = get_current_context();
        self.usage_learner.record_action(user_action, context);
    }

    /// Record scan performance for adaptive intervals
    pub fn record_scan_performance(&mut self, scan_duration: std::time::Duration, scan_interval: std::time::Duration) {
        debug!("Recording scan performance: duration={:?}, interval={:?}", 
               scan_duration, scan_interval);
        
        let metrics = NetworkMetrics {
            latency_ms: scan_duration.as_millis() as f32,
            packet_loss: 0.0, // Not measured in scans
            jitter_ms: 0.0, // Not measured in scans
            bandwidth_mbps: 0.0, // Not measured in scans
            timestamp: chrono::Utc::now().timestamp(),
        };
        
        // Use a connection ID based on the current context location hash
        let context = get_current_context();
        let connection_id = format!("scan_{}", context.location_hash);
        self.performance_tracker.record_metrics(&connection_id, metrics);
    }

    /// Get current adaptive scan interval
    pub fn get_adaptive_scan_interval(&self, base_interval: std::time::Duration, recent_changes: u32) -> std::time::Duration {
        // Simple adaptive logic - can be enhanced with ML
        let multiplier = if recent_changes > 3 {
            0.5 // Scan more frequently if there are many changes
        } else if recent_changes == 0 {
            2.0 // Scan less frequently if no changes
        } else {
            1.0 // Normal interval
        };
        
        std::time::Duration::from_millis((base_interval.as_millis() as f64 * multiplier) as u64)
    }

    /// Get ML metrics for daemon status
    pub fn get_ml_metrics(&self) -> crate::geofencing::ipc::MlDaemonMetrics {
        crate::geofencing::ipc::MlDaemonMetrics {
            total_suggestions_generated: 0, // Will be tracked properly later
            suggestion_accuracy_rate: 0.85, // Placeholder
            zone_prediction_confidence: 0.75, // Placeholder  
            adaptive_scan_effectiveness: 0.80, // Placeholder
            ml_model_version: "1.0.0".to_string(),
            last_model_training: None,
            performance_metrics: crate::geofencing::ipc::MlPerformanceMetrics {
                average_prediction_time_ms: 5.0,
                memory_usage_mb: 50.0,
                cache_hit_rate: 0.90,
                training_data_size: 1000,
            },
        }
    }
}

/// Get current network context for ML predictions
#[cfg(feature = "ml")]
pub fn get_current_context() -> NetworkContext {
    use chrono::{Datelike, Local, Timelike};

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
        .args(["route", "show", "default"])
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
        .args(["link", "show"])
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
    if let Ok(output) = std::process::Command::new("iwconfig").output() {
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
#[cfg(all(feature = "ml", feature = "tailscale"))]
pub fn predict_best_exit_nodes(peers: &[TailscalePeer], top_n: usize) -> Vec<(String, f32)> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();
    let result = manager
        .exit_node_predictor
        .predict_best_nodes(peers, &context, top_n);

    if result.confidence > manager.config.confidence_threshold {
        result.value
    } else {
        // Fall back to non-ML selection if confidence is low
        Vec::new()
    }
}

/// Record exit node performance for learning
#[cfg(all(feature = "ml", feature = "tailscale"))]
pub fn record_exit_node_performance(node_id: &str, latency: f32, packet_loss: f32) {
    let mut manager = ML_MANAGER.lock().unwrap();

    let metrics = NetworkMetrics {
        latency_ms: latency,
        packet_loss,
        jitter_ms: 0.0,      // Would need to calculate this properly
        bandwidth_mbps: 0.0, // Would need to measure this
        timestamp: chrono::Utc::now().timestamp(),
    };

    manager
        .exit_node_predictor
        .record_performance(node_id, metrics.clone());
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
    let symptom_enums: Vec<NetworkSymptom> = symptoms
        .iter()
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
    let test_strs: Vec<String> = tests
        .iter()
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

/// Get personalized menu ordering based on usage patterns and smart prioritization
#[cfg(feature = "ml")]
pub fn get_personalized_menu_order(menu_items: Vec<String>) -> Vec<String> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();

    // Get usage-based ordering first
    let usage_ordered = manager
        .usage_learner
        .get_personalized_menu_order(menu_items.clone(), &context);

    // Apply smart prioritization to create final sophisticated ordering
    let mut scored_items: Vec<(String, f32)> = usage_ordered
        .iter()
        .enumerate()
        .map(|(index, action_str)| {
            // Calculate usage score based on position (higher for items that appear earlier)
            let usage_score = 1.0 - (index as f32 / menu_items.len() as f32);

            // Get smart priority score
            let priority_score = manager.action_prioritizer.calculate_priority_score(
                action_str,
                &context,
                usage_score,
            );

            (action_str.clone(), priority_score)
        })
        .collect();

    // Sort by combined score (descending)
    scored_items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    debug!(
        "ML-enhanced menu ordering applied to {} items",
        scored_items.len()
    );
    if log::log_enabled!(log::Level::Debug) {
        for (i, (action, score)) in scored_items.iter().take(5).enumerate() {
            debug!("  {}. {} (score: {:.3})", i + 1, action, score);
        }
    }

    scored_items.into_iter().map(|(action, _)| action).collect()
}

/// Record user action for learning
#[cfg(feature = "ml")]
pub fn record_user_action(action: &str) {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    if let Some(user_action) = parse_user_action(action) {
        let context = get_current_context();
        manager.usage_learner.record_action(user_action, context);
    }

    // Save on every 5th user action for better persistence
    static ACTION_COUNTER: Lazy<Arc<Mutex<u32>>> = Lazy::new(|| Arc::new(Mutex::new(0)));
    let mut counter = ACTION_COUNTER.lock().unwrap();
    *counter += 1;

    if *counter % 5 == 0 {
        debug!(
            "Auto-saving ML models after user action (count: {})",
            *counter
        );
        if let Err(e) = manager.save_models() {
            error!("Failed to save ML models: {}", e);
        }
    }
}

/// Get personalized WiFi network ordering based on usage patterns
#[cfg(feature = "ml")]
pub fn get_personalized_wifi_order(available_networks: Vec<String>) -> Vec<String> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();
    manager
        .usage_learner
        .get_personalized_wifi_order(available_networks, &context)
}

/// Record action execution result for prioritization learning
#[cfg(feature = "ml")]
pub fn record_action_result(action: &str, success: bool, execution_time_ms: u64) {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let context = get_current_context();
    let execution_time = execution_time_ms as f32 / 1000.0; // Convert to seconds

    manager
        .action_prioritizer
        .record_action_result(action, success, execution_time, &context);

    debug!(
        "Recorded action result: '{}' = {} ({}ms)",
        action, success, execution_time_ms
    );
}

/// Update network state for better ML predictions
#[cfg(feature = "ml")]
pub fn update_network_state(
    is_online: bool,
    connection_quality: f32,
    available_interfaces: Vec<String>,
) {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    manager.action_prioritizer.update_network_state(
        is_online,
        connection_quality,
        available_interfaces,
    );
    debug!(
        "Updated network state: online={}, quality={:.2}",
        is_online, connection_quality
    );
}

#[cfg(feature = "ml")]
fn parse_user_action(action: &str) -> Option<UserAction> {
    let action_lower = action.to_lowercase();

    // Enhanced WiFi parsing to extract network names
    if action_lower.contains("wifi") {
        if action_lower.contains("disconnect") {
            return Some(UserAction::DisconnectWifi);
        } else {
            // Extract WiFi network name from various formats:
            // "wifi      - 📶 NetworkName"
            // "wifi      - ✅ Connect to NetworkName"
            let network_name = extract_wifi_network_name(action);
            return Some(UserAction::ConnectWifi(network_name));
        }
    } else if action_lower.contains("bluetooth") {
        if action_lower.contains("disconnect") {
            return Some(UserAction::DisconnectBluetooth);
        } else {
            let device_name = extract_bluetooth_device_name(action);
            return Some(UserAction::ConnectBluetooth(device_name));
        }
    }
    #[cfg(feature = "tailscale")]
    if action_lower.contains("enable") && action_lower.contains("tailscale") {
        return Some(UserAction::EnableTailscale);
    }
    #[cfg(feature = "tailscale")]
    if action_lower.contains("disable") && action_lower.contains("tailscale") {
        return Some(UserAction::DisableTailscale);
    }
    #[cfg(feature = "tailscale")]
    if action_lower.contains("exit") && action_lower.contains("node") {
        let node_name = extract_exit_node_name(action);
        return Some(UserAction::SelectExitNode(node_name));
    }

    if action_lower.contains("diagnostic") {
        let diagnostic_name = extract_diagnostic_name(action);
        return Some(UserAction::RunDiagnostic(diagnostic_name));
    } else if action_lower.contains("airplane") {
        return Some(UserAction::ToggleAirplaneMode);
    }

    Some(UserAction::CustomAction(action.to_string()))
}

/// Extract WiFi network name from action string
#[cfg(feature = "ml")]
fn extract_wifi_network_name(action: &str) -> String {
    // Try different patterns for WiFi network extraction
    if let Some(captures) = action.split(" - ").nth(1) {
        // Format: "wifi - 📶 NetworkName"
        let network_part = captures.trim_start_matches(['📶', '✅', '❌', ' ']);
        if !network_part.is_empty() && !network_part.to_lowercase().contains("connect") {
            return network_part.to_string();
        }
    }

    // Fallback: try to find network name after common keywords
    let keywords = ["connect to", "network", "wifi"];
    for keyword in keywords {
        if let Some(pos) = action.to_lowercase().find(keyword) {
            let after_keyword = &action[pos + keyword.len()..].trim();
            if !after_keyword.is_empty() {
                // Take first word/phrase after the keyword
                let network = after_keyword
                    .split_whitespace()
                    .next()
                    .unwrap_or(after_keyword);
                if network.len() > 1 {
                    return network.to_string();
                }
            }
        }
    }

    // Ultimate fallback: use "unknown" but preserve some context
    if action.len() > 20 {
        format!("unknown_{}", &action[action.len() - 8..]) // Last 8 chars as identifier
    } else {
        "unknown".to_string()
    }
}

/// Extract Bluetooth device name from action string
#[cfg(feature = "ml")]
fn extract_bluetooth_device_name(action: &str) -> String {
    if let Some(captures) = action.split(" - ").nth(1) {
        let device_part = captures.trim_start_matches(['🎧', '📱', '⌚', '🔊', ' ']);
        if !device_part.is_empty() {
            return device_part.to_string();
        }
    }
    "unknown_device".to_string()
}

/// Extract exit node name from action string
#[cfg(all(feature = "ml", feature = "tailscale"))]
fn extract_exit_node_name(action: &str) -> String {
    // Look for patterns like "us-nyc-wg-301.mullvad.ts.net"
    if let Some(node) = action
        .split_whitespace()
        .find(|s| s.contains(".mullvad.ts.net") || s.contains(".ts.net"))
    {
        return node.to_string();
    }

    // Look for country codes or city names
    if let Some(captures) = action.split(" - ").nth(1) {
        return captures.trim().to_string();
    }

    "unknown_node".to_string()
}

/// Extract diagnostic test type from action string
#[cfg(feature = "ml")]
fn extract_diagnostic_name(action: &str) -> String {
    let action_lower = action.to_lowercase();

    if action_lower.contains("connectivity") {
        "connectivity".to_string()
    } else if action_lower.contains("ping") {
        "ping".to_string()
    } else if action_lower.contains("traceroute") || action_lower.contains("trace") {
        "traceroute".to_string()
    } else if action_lower.contains("speed") {
        "speedtest".to_string()
    } else if action_lower.contains("dns") {
        "dns".to_string()
    } else if action_lower.contains("latency") {
        "latency".to_string()
    } else {
        "general".to_string()
    }
}

/// Predict best WiFi network to connect
#[cfg(feature = "ml")]
pub fn predict_best_wifi_network(networks: Vec<(String, i32, String)>) -> Option<String> {
    let mut manager = ML_MANAGER.lock().unwrap();
    manager.initialize();

    let wifi_networks: Vec<WifiNetwork> = networks
        .into_iter()
        .map(|(ssid, signal, security)| WifiNetwork {
            ssid: ssid.clone(),
            bssid: String::new(),
            signal_strength: signal,
            frequency: 2400, // Default, would need to detect
            channel: 1,      // Default
            security,
            is_saved: false, // Would need to check
        })
        .collect();

    let context = get_current_context();
    let result = manager
        .network_predictor
        .predict_best_network(wifi_networks, &context);

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

    manager
        .network_predictor
        .record_performance(ssid, metrics.clone());
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

    manager
        .performance_tracker
        .get_summary(connection_id)
        .map(|summary| {
            format!(
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
            )
        })
}

// Non-ML fallback functions for when ML feature is disabled

#[cfg(not(all(feature = "ml", feature = "tailscale")))]
pub fn predict_best_exit_nodes<T>(_peers: &[T], _top_n: usize) -> Vec<(String, f32)> {
    Vec::new()
}

#[cfg(not(all(feature = "ml", feature = "tailscale")))]
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

#[cfg(not(feature = "ml"))]
pub fn record_action_result(_action: &str, _success: bool, _execution_time_ms: u64) {}

#[cfg(not(feature = "ml"))]
pub fn update_network_state(
    _is_online: bool,
    _connection_quality: f32,
    _available_interfaces: Vec<String>,
) {
}

#[cfg(not(feature = "ml"))]
pub fn get_personalized_wifi_order(available_networks: Vec<String>) -> Vec<String> {
    // Return networks in original order when ML is disabled
    available_networks
}
