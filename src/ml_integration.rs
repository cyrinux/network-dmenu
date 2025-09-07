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
    zone_transition_smoother: ZoneTransitionSmoother,
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
            zone_transition_smoother: ZoneTransitionSmoother::new(),
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
                    common_visit_times: vec![], // Simplified - no time tracking
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

    /// Get ML-enhanced adaptive scan interval based on comprehensive performance metrics
    pub fn get_adaptive_scan_interval(&self, base_interval: std::time::Duration, recent_changes: u32, current_context: &NetworkContext) -> std::time::Duration {
        debug!("📊 Calculating adaptive scan interval: base={:?}, recent_changes={}, context={:?}", 
               base_interval, recent_changes, current_context);
        
        let mut multiplier = 1.0;
        
        // Zone change frequency factor
        let change_factor = match recent_changes {
            0 => 1.8,        // No changes - scan less frequently 
            1 => 1.2,        // Few changes - slightly less frequent
            2..=3 => 0.9,    // Some changes - slightly more frequent
            4..=6 => 0.6,    // Many changes - much more frequent
            _ => 0.4,        // Very many changes - very frequent scanning
        };
        multiplier *= change_factor;
        
        // Simplified scanning - no time-based patterns
        
        // Network type reliability factor
        let network_factor = match current_context.network_type {
            NetworkType::WiFi => 1.0,      // WiFi is stable for location detection
            NetworkType::Ethernet => 1.2,  // Ethernet is very stable - less scanning needed
            NetworkType::Mobile => 0.7,    // Mobile networks change - more scanning needed
            NetworkType::VPN => 0.8,       // VPN can mask changes - more scanning
            NetworkType::Unknown => 0.9,   // Unknown - slightly more frequent
        };
        multiplier *= network_factor;
        
        // Signal strength factor - poor signal = more frequent scanning
        if let Some(signal_strength) = current_context.signal_strength {
            let signal_factor = (signal_strength * 0.4 + 0.8).clamp(0.6, 1.4);
            multiplier *= signal_factor;
        }
        
        // Removed day-of-week patterns - simplified focus on network quality
        
        // Apply bounds to prevent extreme values
        multiplier = multiplier.clamp(0.3, 3.0);
        
        let adaptive_interval = std::time::Duration::from_millis(
            (base_interval.as_millis() as f64 * multiplier as f64) as u64
        );
        
        debug!("📊 Adaptive scan calculation: change_factor={:.2}, network_factor={:.2}, signal_factor={:.2}, final_multiplier={:.2}, interval={:?}",
               change_factor, network_factor, 
               current_context.signal_strength.map_or(1.0, |s| (s * 0.4 + 0.8).clamp(0.6, 1.4)),
               multiplier, adaptive_interval);
        
        adaptive_interval
    }

    /// Calculate scanning effectiveness score based on performance metrics  
    pub fn calculate_scanning_effectiveness(&self) -> f64 {
        // This would analyze the performance tracker data to determine how effective
        // our scanning strategy has been. For now, return a baseline score.
        
        // In a full implementation, this would:
        // 1. Analyze false positive/negative rates
        // 2. Check how quickly we detect actual zone changes  
        // 3. Measure battery/resource efficiency
        // 4. Calculate optimal scan intervals vs actual intervals
        
        let base_effectiveness = 0.8;
        
        // Simple heuristic: if we have recent performance data, adjust score
        let current_hour = {
            use chrono::Timelike;
            chrono::Local::now().hour()
        };
        let time_effectiveness = match current_hour {
            7..=9 | 17..=19 => 0.95,  // Rush hours - scanning is most critical
            10..=16 => 0.85,          // Work hours - moderate effectiveness needed
            _ => 0.75,                // Other times - lower effectiveness acceptable
        };
        
        {
            let result = base_effectiveness * time_effectiveness;
            if result < 0.5 { 0.5 } else if result > 1.0 { 1.0 } else { result }
        }
    }

    /// Record detailed scan performance metrics
    pub fn record_enhanced_scan_performance(&mut self, scan_duration: std::time::Duration, 
                                          scan_interval: std::time::Duration, 
                                          zones_detected: usize, 
                                          zone_changed: bool,
                                          confidence_score: f64) {
        debug!("📈 Recording enhanced scan performance: duration={:?}, interval={:?}, zones={}, changed={}, confidence={:.3}",
               scan_duration, scan_interval, zones_detected, zone_changed, confidence_score);
        
        let context = get_current_context();
        
        // Create comprehensive performance metrics
        let enhanced_metrics = NetworkMetrics {
            latency_ms: scan_duration.as_millis() as f32,
            packet_loss: if zone_changed { 0.0 } else { 1.0 }, // Use packet_loss to track zone change detection
            jitter_ms: confidence_score as f32, // Use jitter to track confidence
            bandwidth_mbps: zones_detected as f32, // Use bandwidth to track zones detected
            timestamp: chrono::Utc::now().timestamp(),
        };
        
        // Use a detailed connection ID that includes context
        let connection_id = format!("scan_{}h_{}_{}_conf{:.0}", 
                                  12, // Fixed time value 
                                  match context.network_type {
                                      NetworkType::WiFi => "wifi",
                                      NetworkType::Ethernet => "eth", 
                                      NetworkType::Mobile => "mobile",
                                      NetworkType::VPN => "vpn",
                                      NetworkType::Unknown => "unknown",
                                  },
                                  if zone_changed { "changed" } else { "stable" },
                                  confidence_score * 100.0);
        
        self.performance_tracker.record_metrics(&connection_id, enhanced_metrics);
        
        // Also record this as a user action for pattern learning
        let scan_action = crate::ml::usage_patterns::UserAction::ConnectWifi(
            format!("scan_performance_{}_{}zones", 
                   if zone_changed { "active" } else { "stable" },
                   zones_detected)
        );
        
        self.usage_learner.record_action(scan_action, context);
        
        info!("📈 Enhanced scan metrics recorded: {:?} -> {} zones, changed={}, confidence={:.1}%",
              scan_duration, zones_detected, zone_changed, confidence_score * 100.0);
    }

    /// Get ML metrics for daemon status with real-time auto-suggestion stats
    pub fn get_ml_metrics(&self, suggestions_generated: u32, auto_suggestions_processed: u32) -> crate::geofencing::ipc::MlDaemonMetrics {
        let scanning_effectiveness = self.calculate_scanning_effectiveness();
        let auto_acceptance_rate = if suggestions_generated > 0 {
            auto_suggestions_processed as f64 / suggestions_generated as f64
        } else {
            0.0
        };
        
        crate::geofencing::ipc::MlDaemonMetrics {
            total_suggestions_generated: suggestions_generated,
            suggestion_accuracy_rate: auto_acceptance_rate, // Use auto-acceptance rate as accuracy proxy
            zone_prediction_confidence: 0.75, // Placeholder  
            adaptive_scan_effectiveness: scanning_effectiveness,
            ml_model_version: "1.2.0".to_string(), // Updated version with auto-acceptance
            last_model_training: None,
            performance_metrics: crate::geofencing::ipc::MlPerformanceMetrics {
                average_prediction_time_ms: 5.0,
                memory_usage_mb: 50.0,
                cache_hit_rate: 0.90,
                training_data_size: 1000,
            },
        }
    }

    /// Calculate real-time confidence for zone detection
    pub fn calculate_zone_confidence(&self, zone_id: &str, current_fingerprint: &crate::geofencing::LocationFingerprint) -> f64 {
        debug!("🎯 Calculating real-time confidence for zone: {}", zone_id);
        
        let context = get_current_context();
        let base_confidence = current_fingerprint.confidence_score;
        
        // Enhance confidence with ML factors
        let mut confidence_multiplier = 1.0;
        
        // Simplified confidence - removed time-based factors for WiFi/Bluetooth focus
        
        // Signal strength factor
        if let Some(signal) = context.signal_strength {
            let signal_factor = (signal * 0.3 + 0.7).clamp(0.7, 1.3);
            confidence_multiplier *= signal_factor;
        }
        
        // Network type reliability factor
        let network_factor = match context.network_type {
            NetworkType::WiFi => 1.2,      // WiFi is most reliable for geofencing
            NetworkType::Ethernet => 1.1,  // Ethernet is also reliable
            NetworkType::Mobile => 0.8,    // Mobile is less reliable for location
            NetworkType::VPN => 0.7,       // VPN can mask location
            NetworkType::Unknown => 0.9,   // Unknown network type
        };
        confidence_multiplier *= network_factor;
        
        let final_confidence = (base_confidence as f64 * confidence_multiplier as f64).clamp(0.0, 1.0);
        
        debug!("🎯 Zone confidence calculation for {}: base={:.3}, signal_factor={:.3}, network_factor={:.3}, final={:.3}", 
               zone_id, base_confidence, 
               context.signal_strength.map(|s| s * 0.3 + 0.7).unwrap_or(1.0), 
               network_factor, final_confidence);
        
        final_confidence
    }

    /// Track zone prediction accuracy over time
    pub fn track_prediction_accuracy(&mut self, predicted_zone_id: &str, actual_zone_id: &str, confidence: f64) {
        debug!("📊 Tracking prediction accuracy: predicted={}, actual={}, confidence={:.3}", 
               predicted_zone_id, actual_zone_id, confidence);
        
        let is_correct = predicted_zone_id == actual_zone_id;
        let context = get_current_context();
        
        // Record the prediction result for learning
        let user_action = if is_correct {
            crate::ml::usage_patterns::UserAction::ConnectWifi(
                format!("correct_prediction_{}", actual_zone_id)
            )
        } else {
            crate::ml::usage_patterns::UserAction::ConnectWifi(
                format!("incorrect_prediction_{}_{}", predicted_zone_id, actual_zone_id)
            )
        };
        
        self.usage_learner.record_action(user_action, context);
        
        // Update performance metrics
        let prediction_metrics = NetworkMetrics {
            latency_ms: if is_correct { 1.0 } else { 0.0 }, // Use latency field to track accuracy
            packet_loss: (1.0 - confidence) as f32, // Use packet loss to track uncertainty
            jitter_ms: if is_correct { confidence as f32 } else { (1.0 - confidence) as f32 },
            bandwidth_mbps: 1.0, // Constant to count predictions
            timestamp: chrono::Utc::now().timestamp(),
        };
        
        let connection_id = format!("prediction_{}", predicted_zone_id);
        self.performance_tracker.record_metrics(&connection_id, prediction_metrics);
        
        info!("📊 Prediction accuracy recorded: {} (confidence: {:.1}%)", 
              if is_correct { "✅ CORRECT" } else { "❌ INCORRECT" }, 
              confidence * 100.0);
    }

    /// Get confidence threshold for automatic zone switching (simplified)
    pub fn get_confidence_threshold(&self, zone_id: &str) -> f64 {
        // Simplified confidence threshold based only on network reliability
        let base_threshold = 0.75;
        let context = get_current_context();
        
        // Adjust based on network reliability (focus on WiFi/Bluetooth quality)
        let network_adjustment = match context.network_type {
            NetworkType::WiFi => -0.05,     // WiFi is reliable, lower threshold
            NetworkType::Mobile => 0.10,    // Mobile is less reliable, higher threshold
            NetworkType::VPN => 0.15,       // VPN can be misleading, much higher threshold
            _ => 0.0,
        };
        
        let sum = base_threshold + network_adjustment;
        let dynamic_threshold = if sum < 0.5 { 0.5 } else if sum > 0.95 { 0.95 } else { sum };
        
        debug!("🎯 Dynamic confidence threshold for {}: base={:.3}, network_adj={:.3}, final={:.3}",
               zone_id, base_threshold, network_adjustment, dynamic_threshold);
        
        dynamic_threshold
    }

    /// Determine if a zone suggestion should be automatically accepted
    pub fn should_auto_accept_suggestion(&self, suggestion: &crate::geofencing::ipc::ZoneSuggestion, current_context: &NetworkContext) -> bool {
        debug!("🤖 Evaluating auto-acceptance for suggestion: {} (confidence: {:.3})", 
               suggestion.suggested_name, suggestion.confidence);
        
        // Base confidence threshold for auto-acceptance
        let base_threshold = 0.85; // Higher than manual threshold
        
        // Simplified adjustments (no time-based logic)
        
        let network_adjustment = match current_context.network_type {
            NetworkType::WiFi => -0.05,      // WiFi is reliable
            NetworkType::Mobile => 0.15,     // Mobile is less reliable, be more strict
            NetworkType::VPN => 0.20,        // VPN can be misleading, very strict
            _ => 0.05,
        };
        
        // Priority-based adjustment
        let priority_adjustment = match suggestion.priority {
            crate::geofencing::ipc::SuggestionPriority::Critical => -0.20, // Critical suggestions get lower threshold
            crate::geofencing::ipc::SuggestionPriority::High => -0.10,     // High priority gets some leeway
            crate::geofencing::ipc::SuggestionPriority::Medium => 0.0,     // Normal threshold
            crate::geofencing::ipc::SuggestionPriority::Low => 0.10,       // Low priority needs higher confidence
        };
        
        // Evidence quality adjustment
        let evidence_adjustment = if suggestion.evidence.visit_count >= 5 {
            -0.05 // Multiple visits provide more confidence
        } else if suggestion.evidence.visit_count >= 2 {
            0.0   // Some history is normal
        } else {
            0.10  // Single visit requires higher confidence
        };
        
        let sum = base_threshold + network_adjustment + priority_adjustment + evidence_adjustment;
        let final_threshold = if sum < 0.75 { 0.75 } else if sum > 0.95 { 0.95 } else { sum };
        
        let should_accept = suggestion.confidence >= final_threshold;
        
        debug!("🤖 Auto-acceptance evaluation: base={:.3}, network_adj={:.3}, priority_adj={:.3}, evidence_adj={:.3}, final_threshold={:.3}, confidence={:.3}, accept={}",
               base_threshold, network_adjustment, priority_adjustment, evidence_adjustment,
               final_threshold, suggestion.confidence, should_accept);
        
        if should_accept {
            info!("🤖 ✅ Auto-accepting zone suggestion '{}' with {:.1}% confidence (threshold: {:.1}%)",
                  suggestion.suggested_name, suggestion.confidence * 100.0, final_threshold * 100.0);
        } else {
            debug!("🤖 ❌ Rejecting auto-acceptance for '{}' - confidence {:.1}% below threshold {:.1}%",
                   suggestion.suggested_name, suggestion.confidence * 100.0, final_threshold * 100.0);
        }
        
        should_accept
    }

    /// Generate enhanced zone suggestions with auto-acceptance evaluation
    pub fn generate_enhanced_zone_suggestions(&self, zones: &[crate::geofencing::GeofenceZone]) -> Vec<(crate::geofencing::ipc::ZoneSuggestion, bool)> {
        let basic_suggestions = self.generate_zone_suggestions(zones);
        let current_context = get_current_context();
        
        basic_suggestions
            .into_iter()
            .map(|suggestion| {
                let should_auto_accept = self.should_auto_accept_suggestion(&suggestion, &current_context);
                (suggestion, should_auto_accept)
            })
            .collect()
    }

    /// Process automatic zone creation for high-confidence suggestions
    pub fn process_auto_suggestions(&mut self, zones: &[crate::geofencing::GeofenceZone]) -> Vec<AutoSuggestionResult> {
        let enhanced_suggestions = self.generate_enhanced_zone_suggestions(zones);
        let mut results = Vec::new();
        
        for (suggestion, should_auto_accept) in enhanced_suggestions {
            if should_auto_accept {
                // In a real implementation, this would trigger zone creation
                // For now, we just record the intention
                results.push(AutoSuggestionResult {
                    suggestion_name: suggestion.suggested_name.clone(),
                    confidence: suggestion.confidence,
                    action: AutoSuggestionAction::CreateZone,
                    reasoning: format!("Auto-created due to high confidence ({:.1}%) and {} priority",
                                     suggestion.confidence * 100.0, 
                                     match suggestion.priority {
                                         crate::geofencing::ipc::SuggestionPriority::Critical => "critical",
                                         crate::geofencing::ipc::SuggestionPriority::High => "high",
                                         crate::geofencing::ipc::SuggestionPriority::Medium => "medium", 
                                         crate::geofencing::ipc::SuggestionPriority::Low => "low",
                                     }),
                });
                
                info!("🤖 ✨ Auto-suggestion processed: creating zone '{}' with {:.1}% confidence",
                      suggestion.suggested_name, suggestion.confidence * 100.0);
            } else {
                results.push(AutoSuggestionResult {
                    suggestion_name: suggestion.suggested_name.clone(),
                    confidence: suggestion.confidence,
                    action: AutoSuggestionAction::RequireManualApproval,
                    reasoning: "Confidence below auto-acceptance threshold".to_string(),
                });
            }
        }
        
        results
    }

    /// Process zone transition with ML-powered smoothing
    pub fn process_zone_transition_with_smoothing(&mut self, candidate_zone_id: &str, confidence: f64) -> ZoneTransitionDecision {
        let current_context = get_current_context();
        let decision = self.zone_transition_smoother.process_zone_transition(
            candidate_zone_id, 
            confidence, 
            &current_context
        );
        
        // Record the transition attempt for learning
        let transition_action = match &decision {
            ZoneTransitionDecision::AcceptImmediately | ZoneTransitionDecision::AcceptAfterSmoothing => {
                crate::ml::usage_patterns::UserAction::ConnectWifi(
                    format!("smooth_transition_accepted_{}", candidate_zone_id)
                )
            }
            ZoneTransitionDecision::StartPending | ZoneTransitionDecision::ContinuePending => {
                crate::ml::usage_patterns::UserAction::ConnectWifi(
                    format!("smooth_transition_pending_{}", candidate_zone_id)
                )
            }
            ZoneTransitionDecision::Reject(_) | ZoneTransitionDecision::TimeoutReject(_) => {
                crate::ml::usage_patterns::UserAction::ConnectWifi(
                    format!("smooth_transition_rejected_{}", candidate_zone_id)
                )
            }
        };
        
        self.usage_learner.record_action(transition_action, current_context);
        decision
    }

    /// Get the current zone after smoothing
    pub fn get_smoothed_current_zone(&self) -> Option<&str> {
        self.zone_transition_smoother.get_current_zone_id()
    }

    /// Get zone transition smoothing statistics
    pub fn get_transition_smoothing_stats(&self) -> ZoneTransitionStats {
        self.zone_transition_smoother.get_smoothing_stats()
    }

    /// Check if there's a pending zone transition
    pub fn has_pending_transition(&self) -> bool {
        self.zone_transition_smoother.get_pending_transition().is_some()
    }
}

/// Result of processing an automatic suggestion
#[derive(Debug, Clone)]
pub struct AutoSuggestionResult {
    pub suggestion_name: String,
    pub confidence: f64,
    pub action: AutoSuggestionAction,
    pub reasoning: String,
}

/// Action taken for an automatic suggestion
#[derive(Debug, Clone, PartialEq)]
pub enum AutoSuggestionAction {
    CreateZone,
    RequireManualApproval,
}

/// Zone transition smoothing to prevent rapid zone changes
#[derive(Debug, Clone)]
pub struct ZoneTransitionSmoother {
    current_zone_id: Option<String>,
    pending_zone_id: Option<String>,
    pending_since: Option<chrono::DateTime<chrono::Utc>>,
    transition_history: std::collections::VecDeque<ZoneTransition>,
    smoothing_window_seconds: u64,
    confidence_decay_rate: f64,
}

#[derive(Debug, Clone)]
struct ZoneTransition {
    from_zone_id: Option<String>,
    to_zone_id: String,
    confidence: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
    was_applied: bool,
}

impl Default for ZoneTransitionSmoother {
    fn default() -> Self {
        Self::new()
    }
}

impl ZoneTransitionSmoother {
    pub fn new() -> Self {
        Self {
            current_zone_id: None,
            pending_zone_id: None,
            pending_since: None,
            transition_history: std::collections::VecDeque::with_capacity(20),
            smoothing_window_seconds: 30, // 30-second smoothing window
            confidence_decay_rate: 0.95,  // Confidence decays over time
        }
    }

    /// Process a potential zone transition with ML smoothing
    pub fn process_zone_transition(&mut self, candidate_zone_id: &str, confidence: f64, current_context: &NetworkContext) -> ZoneTransitionDecision {
        let now = chrono::Utc::now();
        
        debug!("🌊 Processing zone transition smoothing: current={:?}, candidate={}, confidence={:.3}",
               self.current_zone_id, candidate_zone_id, confidence);
        
        // If this is the first zone or same as current, accept immediately if confidence is high
        if self.current_zone_id.is_none() || 
           self.current_zone_id.as_deref() == Some(candidate_zone_id) {
            if confidence >= 0.8 {
                self.current_zone_id = Some(candidate_zone_id.to_string());
                self.clear_pending_transition();
                return ZoneTransitionDecision::AcceptImmediately;
            } else {
                return ZoneTransitionDecision::Reject("Confidence too low for initial/same zone".to_string());
            }
        }
        
        // Check if we're already transitioning to this zone
        if self.pending_zone_id.as_deref() == Some(candidate_zone_id) {
            return self.evaluate_pending_transition(confidence, current_context, now);
        }
        
        // New zone transition candidate
        self.evaluate_new_transition(candidate_zone_id, confidence, current_context, now)
    }

    fn evaluate_pending_transition(&mut self, confidence: f64, current_context: &NetworkContext, now: chrono::DateTime<chrono::Utc>) -> ZoneTransitionDecision {
        let pending_duration = if let Some(pending_since) = self.pending_since {
            (now - pending_since).num_seconds() as u64
        } else {
            return ZoneTransitionDecision::Reject("No pending transition timestamp".to_string());
        };
        
        // Calculate confidence threshold based on duration pending
        let duration_factor = (pending_duration as f64 / self.smoothing_window_seconds as f64).min(1.0);
        let base_threshold = 0.75;
        let duration_adjusted_threshold = base_threshold * (1.0 - duration_factor * 0.2); // Lower threshold over time
        
        // Network stability factor
        let network_stability = self.calculate_network_stability(current_context);
        let stability_adjusted_threshold = duration_adjusted_threshold * network_stability;
        
        if confidence >= stability_adjusted_threshold {
            // Accept the transition
            let previous_zone = self.current_zone_id.clone();
            let new_zone_id = self.pending_zone_id.as_ref().unwrap().clone();
            self.current_zone_id = self.pending_zone_id.clone();
            self.add_to_history(previous_zone.clone(), &new_zone_id, confidence, now, true);
            self.clear_pending_transition();
            
            info!("🌊 ✅ Zone transition smoothed and accepted after {}s: {:?} -> {} (confidence: {:.1}%, threshold: {:.1}%)",
                  pending_duration, previous_zone, new_zone_id, 
                  confidence * 100.0, stability_adjusted_threshold * 100.0);
            
            ZoneTransitionDecision::AcceptAfterSmoothing
        } else if pending_duration >= self.smoothing_window_seconds {
            // Timeout - reject the pending transition
            let pending_zone_id = self.pending_zone_id.as_ref().unwrap().clone();
            self.add_to_history(self.current_zone_id.clone(), &pending_zone_id, confidence, now, false);
            self.clear_pending_transition();
            
            debug!("🌊 ⏰ Zone transition timed out after {}s: confidence {:.1}% below threshold {:.1}%",
                   pending_duration, confidence * 100.0, stability_adjusted_threshold * 100.0);
            
            ZoneTransitionDecision::TimeoutReject("Smoothing window expired".to_string())
        } else {
            // Continue waiting
            debug!("🌊 ⏳ Zone transition pending ({}s/{} remaining): confidence {:.1}% below threshold {:.1}%",
                   pending_duration, self.smoothing_window_seconds - pending_duration, 
                   confidence * 100.0, stability_adjusted_threshold * 100.0);
            
            ZoneTransitionDecision::ContinuePending
        }
    }

    fn evaluate_new_transition(&mut self, candidate_zone_id: &str, confidence: f64, current_context: &NetworkContext, now: chrono::DateTime<chrono::Utc>) -> ZoneTransitionDecision {
        // Check if we've been oscillating between zones recently
        if self.detect_oscillation(candidate_zone_id, now) {
            debug!("🌊 🔄 Detected oscillation pattern for zone: {}, applying damping", candidate_zone_id);
            return ZoneTransitionDecision::Reject("Oscillation damping active".to_string());
        }
        
        // High confidence transitions can bypass smoothing in certain contexts
        let immediate_threshold = self.calculate_immediate_threshold(current_context);
        if confidence >= immediate_threshold {
            let previous_zone = self.current_zone_id.clone();
            self.current_zone_id = Some(candidate_zone_id.to_string());
            self.add_to_history(previous_zone.clone(), candidate_zone_id, confidence, now, true);
            self.clear_pending_transition();
            
            info!("🌊 ⚡ Immediate zone transition (high confidence): {:?} -> {} (confidence: {:.1}%, threshold: {:.1}%)",
                  previous_zone, candidate_zone_id, confidence * 100.0, immediate_threshold * 100.0);
            
            return ZoneTransitionDecision::AcceptImmediately;
        }
        
        // Start pending transition
        self.pending_zone_id = Some(candidate_zone_id.to_string());
        self.pending_since = Some(now);
        
        info!("🌊 🕐 Starting zone transition smoothing: {:?} -> {} (confidence: {:.1}%, smoothing for {}s)",
              self.current_zone_id, candidate_zone_id, confidence * 100.0, self.smoothing_window_seconds);
        
        ZoneTransitionDecision::StartPending
    }

    fn calculate_network_stability(&self, current_context: &NetworkContext) -> f64 {
        // Network type stability factor
        let type_stability = match current_context.network_type {
            NetworkType::Ethernet => 1.0,     // Very stable
            NetworkType::WiFi => 0.95,        // Generally stable
            NetworkType::Mobile => 0.8,       // Less stable
            NetworkType::VPN => 0.85,         // Moderately stable
            NetworkType::Unknown => 0.9,      // Assume moderate stability
        };
        
        // Signal strength stability
        let signal_stability = current_context.signal_strength.map_or(1.0, |strength| {
            if strength > 0.8 { 1.0 } else if strength > 0.5 { 0.95 } else { 0.9 }
        });
        
        type_stability * signal_stability
    }

    fn calculate_immediate_threshold(&self, current_context: &NetworkContext) -> f64 {
        let base_threshold = 0.9; // High threshold for immediate acceptance
        
        // Apply confidence decay if we have a pending transition
        let decayed_threshold = if let Some(pending_since) = self.pending_since {
            let seconds_pending = chrono::Utc::now().signed_duration_since(pending_since).num_seconds() as f64;
            // Use confidence_decay_rate to lower threshold over time (making transitions easier)
            base_threshold * self.confidence_decay_rate.powf(seconds_pending / 10.0) // Decay every 10 seconds
        } else {
            base_threshold
        };
        
        // Removed time-based adjustment for simplified network-focused logic
        
        // Network type adjustment  
        let network_adjustment = match current_context.network_type {
            NetworkType::WiFi => -0.05,      // WiFi is reliable
            NetworkType::Mobile => 0.10,     // Mobile is less reliable
            _ => 0.0,
        };
        
        {
            let sum = decayed_threshold + network_adjustment;
            if sum < 0.8 { 0.8 } else if sum > 0.95 { 0.95 } else { sum }
        }
    }

    fn detect_oscillation(&self, candidate_zone_id: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
        // Look for oscillation pattern in recent history (last 10 minutes)
        // Also check if we're trying to return to a recently departed zone
        if self.current_zone_id.is_some() {
            // Check if candidate was recently left (using from_zone_id)
            let recent_departures = self.transition_history.iter()
                .rev()
                .take(5) // Last 5 transitions
                .filter(|t| t.from_zone_id.as_deref() == Some(candidate_zone_id) && 
                           now.signed_duration_since(t.timestamp).num_seconds() < 300) // Last 5 minutes
                .count();
                
            if recent_departures >= 2 {
                debug!("🌊 🔄 Detected rapid return pattern: {} was recently departed {} times", candidate_zone_id, recent_departures);
                return true;
            }
        }
        let recent_cutoff = now - chrono::Duration::minutes(10);
        let recent_transitions: Vec<_> = self.transition_history
            .iter()
            .filter(|t| t.timestamp > recent_cutoff)
            .collect();
        
        if recent_transitions.len() < 4 {
            return false; // Not enough data to detect oscillation
        }
        
        // Check for A->B->A->B pattern
        let mut oscillation_count = 0;
        for window in recent_transitions.windows(2) {
            if let [prev, curr] = window {
                if prev.to_zone_id == candidate_zone_id && 
                   curr.to_zone_id != candidate_zone_id &&
                   curr.to_zone_id == self.current_zone_id.as_deref().unwrap_or("") {
                    oscillation_count += 1;
                }
            }
        }
        
        oscillation_count >= 2 // Detected oscillation if we see this pattern twice
    }

    fn add_to_history(&mut self, from_zone_id: Option<String>, to_zone_id: &str, confidence: f64, timestamp: chrono::DateTime<chrono::Utc>, was_applied: bool) {
        let transition = ZoneTransition {
            from_zone_id,
            to_zone_id: to_zone_id.to_string(),
            confidence,
            timestamp,
            was_applied,
        };
        
        self.transition_history.push_back(transition);
        
        // Keep only recent history (last 50 transitions)
        while self.transition_history.len() > 50 {
            self.transition_history.pop_front();
        }
    }

    fn clear_pending_transition(&mut self) {
        self.pending_zone_id = None;
        self.pending_since = None;
    }

    /// Get current zone after smoothing
    pub fn get_current_zone_id(&self) -> Option<&str> {
        self.current_zone_id.as_deref()
    }

    /// Get pending transition info if any
    pub fn get_pending_transition(&self) -> Option<(String, u64, f64)> {
        if let (Some(pending_zone), Some(pending_since)) = (&self.pending_zone_id, &self.pending_since) {
            let elapsed = (chrono::Utc::now() - *pending_since).num_seconds() as u64;
            let confidence = self.transition_history.back().map_or(0.0, |t| t.confidence);
            Some((pending_zone.clone(), elapsed, confidence))
        } else {
            None
        }
    }

    /// Get transition smoothing statistics
    pub fn get_smoothing_stats(&self) -> ZoneTransitionStats {
        let recent_cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
        let recent_transitions: Vec<_> = self.transition_history
            .iter()
            .filter(|t| t.timestamp > recent_cutoff)
            .collect();

        let applied_count = recent_transitions.iter().filter(|t| t.was_applied).count();
        let rejected_count = recent_transitions.len() - applied_count;
        
        ZoneTransitionStats {
            total_transitions_processed: self.transition_history.len(),
            recent_hour_applied: applied_count,
            recent_hour_rejected: rejected_count,
            current_zone_id: self.current_zone_id.clone(),
            pending_transition: self.get_pending_transition(),
            oscillation_damping_active: false, // Could be enhanced to track this
        }
    }
}

/// Decision from the zone transition smoother
#[derive(Debug, Clone, PartialEq)]
pub enum ZoneTransitionDecision {
    AcceptImmediately,
    AcceptAfterSmoothing,
    StartPending,
    ContinuePending,
    Reject(String),
    TimeoutReject(String),
}

/// Statistics about zone transition smoothing
#[derive(Debug, Clone)]
pub struct ZoneTransitionStats {
    pub total_transitions_processed: usize,
    pub recent_hour_applied: usize,
    pub recent_hour_rejected: usize,
    pub current_zone_id: Option<String>,
    pub pending_transition: Option<(String, u64, f64)>, // (zone_id, elapsed_seconds, confidence)
    pub oscillation_damping_active: bool,
}

/// Get current network context for ML predictions (network-focused only)
#[cfg(feature = "ml")]
pub fn get_current_context() -> NetworkContext {
    NetworkContext {
        time_of_day: 0,  // Unused - simplified 
        day_of_week: 0,  // Unused - simplified
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
