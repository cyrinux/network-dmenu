//! Advanced action prioritization module
//!
//! This module provides sophisticated action sorting based on multiple criteria:
//! - Network conditions and signal strength
//! - Time patterns and user habits
//! - System state and resource availability  
//! - Action type priorities and dependencies
//! - Historical success rates and performance

use super::{NetworkContext, NetworkType};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use log::debug;

/// Action priority categories
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionPriority {
    Critical,    // Essential network functions (connectivity tests, basic connections)
    High,        // Frequently used actions (VPN, main WiFi networks)
    Medium,      // Regular actions (Bluetooth, diagnostics)
    Low,         // Rarely used or situational actions
    Contextual,  // Priority depends on current context
}

/// Advanced action scoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPrioritizerConfig {
    pub network_condition_weight: f32,
    pub temporal_pattern_weight: f32,
    pub success_rate_weight: f32,
    pub resource_efficiency_weight: f32,
    pub user_preference_weight: f32,
    pub emergency_boost: f32,
}

impl Default for ActionPrioritizerConfig {
    fn default() -> Self {
        Self {
            network_condition_weight: 0.25,
            temporal_pattern_weight: 0.20,
            success_rate_weight: 0.20,
            resource_efficiency_weight: 0.15,
            user_preference_weight: 0.15,
            emergency_boost: 0.5,
        }
    }
}

/// Action performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetrics {
    pub success_count: u32,
    pub failure_count: u32,
    pub average_execution_time: f32,  // in seconds
    pub last_success_timestamp: Option<i64>,
    pub context_success_rate: HashMap<String, f32>,  // context -> success rate
}

impl Default for ActionMetrics {
    fn default() -> Self {
        Self {
            success_count: 0,
            failure_count: 0,
            average_execution_time: 1.0,
            last_success_timestamp: None,
            context_success_rate: HashMap::new(),
        }
    }
}

impl ActionMetrics {
    pub fn success_rate(&self) -> f32 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.5  // Neutral assumption for new actions
        } else {
            self.success_count as f32 / total as f32
        }
    }
    
    pub fn record_success(&mut self, execution_time: f32, context: &str) {
        self.success_count += 1;
        self.last_success_timestamp = Some(chrono::Utc::now().timestamp());
        
        // Update average execution time
        let total_count = self.success_count + self.failure_count;
        self.average_execution_time = (self.average_execution_time * (total_count - 1) as f32 + execution_time) / total_count as f32;
        
        // Update context-specific success rate
        let context_total = self.context_success_rate.get(context).unwrap_or(&0.5) * 2.0;
        let new_rate = (context_total + 1.0) / (context_total + 2.0);
        self.context_success_rate.insert(context.to_string(), new_rate);
    }
    
    pub fn record_failure(&mut self, context: &str) {
        self.failure_count += 1;
        
        // Update context-specific success rate
        let context_total = self.context_success_rate.get(context).unwrap_or(&0.5) * 2.0;
        let new_rate = context_total / (context_total + 2.0);
        self.context_success_rate.insert(context.to_string(), new_rate);
    }
}

/// Advanced action prioritizer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPrioritizer {
    config: ActionPrioritizerConfig,
    action_priorities: HashMap<String, ActionPriority>,
    action_metrics: HashMap<String, ActionMetrics>,
    network_state_cache: Option<NetworkStateCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkStateCache {
    is_online: bool,
    connection_quality: f32,  // 0.0 to 1.0
    available_interfaces: Vec<String>,
    last_updated: i64,
}

impl Default for ActionPrioritizer {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionPrioritizer {
    pub fn new() -> Self {
        Self {
            config: ActionPrioritizerConfig::default(),
            action_priorities: Self::initialize_default_priorities(),
            action_metrics: HashMap::new(),
            network_state_cache: None,
        }
    }
    
    pub fn with_config(config: ActionPrioritizerConfig) -> Self {
        Self {
            config,
            action_priorities: Self::initialize_default_priorities(),
            action_metrics: HashMap::new(),
            network_state_cache: None,
        }
    }
    
    /// Initialize default priorities for common action types
    fn initialize_default_priorities() -> HashMap<String, ActionPriority> {
        let mut priorities = HashMap::new();
        
        // Critical actions - basic connectivity
        priorities.insert("diagnostic_connectivity".to_string(), ActionPriority::Critical);
        priorities.insert("wifi_disconnect".to_string(), ActionPriority::Critical);
        priorities.insert("system_airplane_mode".to_string(), ActionPriority::Critical);
        
        // High priority - primary network functions
        priorities.insert("tailscale_enable".to_string(), ActionPriority::High);
        priorities.insert("tailscale_disable".to_string(), ActionPriority::High);
        priorities.insert("wifi_connect_primary".to_string(), ActionPriority::High);
        priorities.insert("vpn_connect".to_string(), ActionPriority::High);
        
        // Medium priority - secondary functions
        priorities.insert("bluetooth_connect".to_string(), ActionPriority::Medium);
        priorities.insert("diagnostic_ping".to_string(), ActionPriority::Medium);
        priorities.insert("diagnostic_speedtest".to_string(), ActionPriority::Medium);
        
        // Low priority - advanced/rare functions
        priorities.insert("diagnostic_traceroute".to_string(), ActionPriority::Low);
        priorities.insert("system_rfkill".to_string(), ActionPriority::Low);
        priorities.insert("nextdns_profile".to_string(), ActionPriority::Low);
        
        priorities
    }
    
    /// Calculate comprehensive priority score for an action
    pub fn calculate_priority_score(
        &self,
        action_str: &str,
        context: &NetworkContext,
        usage_score: f32,
    ) -> f32 {
        let action_key = self.normalize_action_key(action_str);
        
        // Base priority from action type
        let base_priority = self.get_base_priority_score(&action_key, action_str);
        
        // Network condition adjustment
        let network_score = self.calculate_network_condition_score(action_str, context);
        
        // Temporal pattern score
        let temporal_score = self.calculate_temporal_score(action_str, context);
        
        // Success rate score
        let success_score = self.calculate_success_rate_score(&action_key, context);
        
        // Resource efficiency score
        let efficiency_score = self.calculate_efficiency_score(&action_key);
        
        // Emergency situation boost
        let emergency_boost = self.calculate_emergency_boost(action_str, context);
        
        // Combine all scores with weights
        let weighted_score = 
            base_priority * 0.2 +
            network_score * self.config.network_condition_weight +
            temporal_score * self.config.temporal_pattern_weight +
            success_score * self.config.success_rate_weight +
            efficiency_score * self.config.resource_efficiency_weight +
            usage_score * self.config.user_preference_weight +
            emergency_boost;
        
        debug!("Priority score for '{}': {:.3} (base: {:.2}, network: {:.2}, temporal: {:.2}, success: {:.2}, efficiency: {:.2}, usage: {:.2}, emergency: {:.2})",
            action_str, weighted_score, base_priority, network_score, temporal_score, success_score, efficiency_score, usage_score, emergency_boost);
        
        weighted_score.min(1.0)
    }
    
    /// Normalize action string to a key for metrics lookup
    fn normalize_action_key(&self, action_str: &str) -> String {
        let action = action_str.to_lowercase();
        
        if action.contains("wifi") && action.contains("disconnect") {
            "wifi_disconnect".to_string()
        } else if action.contains("wifi") {
            "wifi_connect".to_string()
        } else if action.contains("tailscale") && action.contains("enable") {
            "tailscale_enable".to_string()
        } else if action.contains("tailscale") && action.contains("disable") {
            "tailscale_disable".to_string()
        } else if action.contains("exit") && action.contains("node") {
            "tailscale_exit_node".to_string()
        } else if action.contains("bluetooth") {
            "bluetooth_connect".to_string()
        } else if action.contains("diagnostic") {
            if action.contains("connectivity") {
                "diagnostic_connectivity".to_string()
            } else if action.contains("ping") {
                "diagnostic_ping".to_string()
            } else if action.contains("speed") {
                "diagnostic_speedtest".to_string()
            } else {
                "diagnostic_other".to_string()
            }
        } else if action.contains("vpn") {
            "vpn_connect".to_string()
        } else if action.contains("airplane") {
            "system_airplane_mode".to_string()
        } else {
            "custom_action".to_string()
        }
    }
    
    /// Get base priority score for an action
    fn get_base_priority_score(&self, action_key: &str, action_str: &str) -> f32 {
        if let Some(priority) = self.action_priorities.get(action_key) {
            match priority {
                ActionPriority::Critical => 0.9,
                ActionPriority::High => 0.7,
                ActionPriority::Medium => 0.5,
                ActionPriority::Low => 0.3,
                ActionPriority::Contextual => self.calculate_contextual_priority(action_str),
            }
        } else {
            // Infer priority from action string
            self.infer_action_priority(action_str)
        }
    }
    
    /// Calculate network condition-based score
    fn calculate_network_condition_score(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();
        let mut score: f32 = 0.5; // Base score
        
        // Network type specific adjustments
        match context.network_type {
            NetworkType::WiFi => {
                if action.contains("wifi") {
                    score += 0.2;
                } else if action.contains("bluetooth") {
                    score += 0.1; // Bluetooth works well with WiFi
                }
            }
            NetworkType::Ethernet => {
                if action.contains("tailscale") || action.contains("vpn") {
                    score += 0.3; // VPNs work best on stable connections
                } else if action.contains("wifi") && !action.contains("disconnect") {
                    score -= 0.1; // Lower priority for WiFi when ethernet available
                }
            }
            NetworkType::Mobile => {
                if action.contains("disconnect") || action.contains("airplane") {
                    score += 0.2; // Data saving actions more important on mobile
                } else if action.contains("speed") {
                    score -= 0.1; // Speed tests less relevant on mobile
                }
            }
            NetworkType::Unknown => {
                if action.contains("diagnostic") && action.contains("connectivity") {
                    score += 0.3; // Connectivity tests crucial when network unknown
                }
            }
            _ => {}
        }
        
        // Signal strength adjustments
        if let Some(signal) = context.signal_strength {
            if signal < 0.3 {
                // Poor signal - prioritize network switching and diagnostics
                if action.contains("diagnostic") || action.contains("disconnect") || action.contains("wifi") {
                    score += 0.3;
                } else if action.contains("speed") || action.contains("streaming") {
                    score -= 0.2;
                }
            } else if signal > 0.8 {
                // Excellent signal - boost bandwidth-intensive actions
                if action.contains("speed") || action.contains("update") || action.contains("sync") {
                    score += 0.2;
                }
            }
        }
        
        score.clamp(0.0, 1.0)
    }
    
    /// Calculate temporal/time-based score
    fn calculate_temporal_score(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();
        let mut score: f32 = 0.5;
        
        // Time of day patterns
        match context.time_of_day {
            6..=8 => {
                // Morning - work setup
                if action.contains("vpn") || action.contains("tailscale") {
                    score += 0.2;
                } else if action.contains("entertainment") || action.contains("gaming") {
                    score -= 0.1;
                }
            }
            9..=16 => {
                // Work hours - productivity focused
                if action.contains("diagnostic") || action.contains("vpn") {
                    score += 0.15;
                } else if action.contains("bluetooth") && action.contains("headphones") {
                    score += 0.1; // Audio for meetings
                }
            }
            17..=21 => {
                // Evening - personal use
                if action.contains("bluetooth") || action.contains("entertainment") {
                    score += 0.15;
                } else if action.contains("exit") && action.contains("node") {
                    score += 0.1; // Geographic shifting for content
                }
            }
            22..=23 | 0..=5 => {
                // Night - minimal activity
                if action.contains("disconnect") || action.contains("airplane") {
                    score += 0.1;
                } else {
                    score -= 0.1;
                }
            }
            _ => {}
        }
        
        // Day of week patterns
        match context.day_of_week {
            0..=4 => {
                // Weekdays - work focused
                if action.contains("vpn") || action.contains("work") {
                    score += 0.1;
                }
            }
            5..=6 => {
                // Weekends - personal focused
                if action.contains("bluetooth") || action.contains("personal") {
                    score += 0.1;
                }
            }
            _ => {}
        }
        
        score.clamp(0.0, 1.0)
    }
    
    /// Calculate success rate score
    fn calculate_success_rate_score(&self, action_key: &str, context: &NetworkContext) -> f32 {
        if let Some(metrics) = self.action_metrics.get(action_key) {
            let general_success_rate = metrics.success_rate();
            
            // Try to get context-specific success rate
            let context_key = format!("{}_{}", context.network_type as u8, context.time_of_day / 6);
            let context_success_rate = metrics.context_success_rate
                .get(&context_key)
                .copied()
                .unwrap_or(general_success_rate);
            
            // Weighted combination favoring context-specific data
            general_success_rate * 0.3 + context_success_rate * 0.7
        } else {
            0.5 // Neutral score for unknown actions
        }
    }
    
    /// Calculate resource efficiency score
    fn calculate_efficiency_score(&self, action_key: &str) -> f32 {
        if let Some(metrics) = self.action_metrics.get(action_key) {
            // Faster actions get higher scores
            let time_score = (10.0 / (metrics.average_execution_time + 1.0)).min(1.0);
            
            // Factor in reliability (consistent execution time)
            let reliability_bonus = if metrics.success_count > 5 { 0.1 } else { 0.0 };
            
            time_score + reliability_bonus
        } else {
            // Default efficiency for unknown actions
            0.6
        }
    }
    
    /// Calculate emergency situation boost
    fn calculate_emergency_boost(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();
        
        // Check for potential emergency situations
        let is_emergency = context.signal_strength.is_some_and(|s| s < 0.1) || 
                          context.network_type == NetworkType::Unknown;
        
        if is_emergency {
            if action.contains("diagnostic") || action.contains("connectivity") {
                return self.config.emergency_boost;
            } else if action.contains("disconnect") || action.contains("airplane") {
                return self.config.emergency_boost * 0.7;
            }
        }
        
        0.0
    }
    
    /// Calculate contextual priority for flexible priority actions
    fn calculate_contextual_priority(&self, action_str: &str) -> f32 {
        let action = action_str.to_lowercase();
        
        // This would integrate with real-time system state
        if action.contains("exit") && action.contains("node") {
            0.6 // Medium-high for exit nodes (depends on current connection)
        } else if action.contains("profile") {
            0.4 // Medium-low for profile switches
        } else {
            0.5 // Default medium
        }
    }
    
    /// Infer priority from action string patterns
    fn infer_action_priority(&self, action_str: &str) -> f32 {
        let action = action_str.to_lowercase();
        
        // Critical patterns
        if action.contains("disconnect") || action.contains("connectivity") || action.contains("airplane") {
            return 0.9;
        }
        
        // High priority patterns
        if action.contains("wifi") || action.contains("vpn") || action.contains("tailscale") {
            return 0.7;
        }
        
        // Medium priority patterns
        if action.contains("bluetooth") || action.contains("diagnostic") {
            return 0.5;
        }
        
        // Low priority (advanced features)
        if action.contains("profile") || action.contains("advanced") || action.contains("config") {
            return 0.3;
        }
        
        0.4 // Default low-medium for unknown actions
    }
    
    /// Record action execution result for learning
    pub fn record_action_result(&mut self, action_str: &str, success: bool, execution_time: f32, context: &NetworkContext) {
        let action_key = self.normalize_action_key(action_str);
        let context_key = format!("{}_{}", context.network_type as u8, context.time_of_day / 6);
        
        let metrics = self.action_metrics.entry(action_key).or_default();
        
        if success {
            metrics.record_success(execution_time, &context_key);
        } else {
            metrics.record_failure(&context_key);
        }
        
        debug!("Recorded action result: {} = {} in {:.2}s", action_str, success, execution_time);
    }
    
    /// Update network state cache for more accurate scoring
    pub fn update_network_state(&mut self, is_online: bool, connection_quality: f32, available_interfaces: Vec<String>) {
        self.network_state_cache = Some(NetworkStateCache {
            is_online,
            connection_quality,
            available_interfaces,
            last_updated: chrono::Utc::now().timestamp(),
        });
    }
    
    /// Get action metrics for reporting
    pub fn get_action_metrics(&self) -> &HashMap<String, ActionMetrics> {
        &self.action_metrics
    }
    
    /// Set custom priority for specific action
    pub fn set_action_priority(&mut self, action_pattern: String, priority: ActionPriority) {
        self.action_priorities.insert(action_pattern, priority);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
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
    fn test_priority_calculation() {
        let prioritizer = ActionPrioritizer::new();
        let context = create_test_context();
        
        let score = prioritizer.calculate_priority_score(
            "diagnostic- ✅ Test Connectivity",
            &context,
            0.5
        );
        
        assert!(score > 0.5); // Connectivity tests should have high priority
    }
    
    #[test]
    fn test_network_condition_scoring() {
        let prioritizer = ActionPrioritizer::new();
        let mut context = create_test_context();
        context.signal_strength = Some(0.2); // Poor signal
        
        let diagnostic_score = prioritizer.calculate_network_condition_score(
            "diagnostic- ✅ Test Connectivity",
            &context
        );
        
        let speedtest_score = prioritizer.calculate_network_condition_score(
            "diagnostic- 🚀 Speed Test",
            &context
        );
        
        assert!(diagnostic_score > speedtest_score); // Connectivity should be prioritized over speed test with poor signal
    }
    
    #[test]
    fn test_action_metrics_recording() {
        let mut prioritizer = ActionPrioritizer::new();
        let context = create_test_context();
        
        prioritizer.record_action_result("wifi- 📶 Connect", true, 2.5, &context);
        prioritizer.record_action_result("wifi- 📶 Connect", false, 5.0, &context);
        
        let metrics = prioritizer.action_metrics.get("wifi_connect").unwrap();
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.success_rate(), 0.5);
    }
    
    #[test]
    fn test_temporal_scoring() {
        let prioritizer = ActionPrioritizer::new();
        let mut context = create_test_context();
        
        // Morning context
        context.time_of_day = 8;
        let morning_vpn_score = prioritizer.calculate_temporal_score("tailscale- ✅ Enable VPN", &context);
        
        // Evening context  
        context.time_of_day = 20;
        let evening_vpn_score = prioritizer.calculate_temporal_score("tailscale- ✅ Enable VPN", &context);
        let evening_bluetooth_score = prioritizer.calculate_temporal_score("bluetooth- 🎧 Connect Headphones", &context);
        
        assert!(morning_vpn_score > evening_vpn_score);
        assert!(evening_bluetooth_score > morning_vpn_score);
    }
}