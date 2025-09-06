//! Advanced action prioritization module
//!
//! This module provides sophisticated action sorting based on multiple criteria:
//! - Network conditions and signal strength
//! - Time patterns and user habits
//! - System state and resource availability
//! - Action type priorities and dependencies
//! - Historical success rates and performance
//!
//! ## Scoring Algorithm
//! 
//! The prioritizer uses a weighted multi-criteria scoring system:
//! - **Network Condition (25%)**: Adapts to current network type and signal strength
//! - **Temporal Patterns (20%)**: Learns time-of-day and day-of-week preferences
//! - **Success Rate (20%)**: Prioritizes historically successful actions
//! - **Resource Efficiency (15%)**: Favors faster-executing actions
//! - **User Preferences (15%)**: Based on usage frequency patterns
//! - **Emergency Boost (up to 50%)**: Critical actions during network issues
//!
//! ## Signal Strength Adaptation
//! 
//! - **Poor Signal (<30%)**: Prioritizes diagnostics and network switching
//! - **Good Signal (>80%)**: Boosts bandwidth-intensive operations
//!
//! ## Time-Based Intelligence
//! 
//! - **Morning (6-8h)**: VPN and work connections prioritized
//! - **Work Hours (9-16h)**: Productivity tools and stable connections
//! - **Evening (17-21h)**: Entertainment and personal device connections
//! - **Night (22-5h)**: Power-saving and minimal activity actions

use super::{NetworkContext, NetworkType};
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// === ML SCORING CONSTANTS ===

/// Default weights for multi-criteria scoring algorithm
pub mod scoring_weights {
    /// Network condition influence on action priority (25%)
    pub const NETWORK_CONDITION: f32 = 0.25;
    /// Temporal pattern matching weight (20%)
    pub const TEMPORAL_PATTERN: f32 = 0.20;
    /// Historical success rate importance (20%)
    pub const SUCCESS_RATE: f32 = 0.20;
    /// Resource efficiency consideration (15%)
    pub const RESOURCE_EFFICIENCY: f32 = 0.15;
    /// User preference from usage patterns (15%)
    pub const USER_PREFERENCE: f32 = 0.15;
    /// Emergency situation boost (up to 50% bonus)
    pub const EMERGENCY_BOOST: f32 = 0.5;
    /// Base priority weight in final calculation
    pub const BASE_PRIORITY: f32 = 0.2;
}

/// Priority level numeric scores
pub mod priority_scores {
    /// Critical actions (connectivity, basic network functions)
    pub const CRITICAL: f32 = 0.9;
    /// High priority actions (VPN, primary networks)
    pub const HIGH: f32 = 0.7;
    /// Medium priority actions (Bluetooth, diagnostics)
    pub const MEDIUM: f32 = 0.5;
    /// Low priority actions (advanced features)
    pub const LOW: f32 = 0.3;
    /// Default neutral score for new/unknown actions
    pub const NEUTRAL: f32 = 0.5;
    /// Minimal score for unrecognized actions
    pub const MINIMAL: f32 = 0.4;
}

/// Signal strength thresholds for adaptive behavior
pub mod signal_thresholds {
    /// Poor signal threshold - prioritize diagnostics and switching
    pub const POOR_SIGNAL: f32 = 0.3;
    /// Excellent signal threshold - boost bandwidth-intensive actions
    pub const EXCELLENT_SIGNAL: f32 = 0.8;
    /// Very poor signal threshold - emergency actions
    pub const CRITICAL_SIGNAL: f32 = 0.1;
}

/// Network condition scoring adjustments
pub mod network_bonuses {
    /// WiFi action bonus when on WiFi network
    pub const WIFI_MATCH: f32 = 0.2;
    /// Bluetooth bonus on WiFi (stable for audio)
    pub const BLUETOOTH_ON_WIFI: f32 = 0.1;
    /// VPN bonus on ethernet (stable connection)
    pub const VPN_ON_ETHERNET: f32 = 0.3;
    /// WiFi penalty when ethernet available
    pub const WIFI_ON_ETHERNET_PENALTY: f32 = -0.1;
    /// Data-saving action bonus on mobile
    pub const DATA_SAVING_ON_MOBILE: f32 = 0.2;
    /// Speed test penalty on mobile
    pub const SPEED_TEST_ON_MOBILE_PENALTY: f32 = -0.1;
    /// Connectivity diagnostic bonus when network unknown
    pub const DIAGNOSTIC_ON_UNKNOWN: f32 = 0.4;
    /// Major connectivity test boost with poor signal
    pub const CONNECTIVITY_POOR_SIGNAL: f32 = 0.5;
    /// General diagnostic/switching boost with poor signal
    pub const SWITCHING_POOR_SIGNAL: f32 = 0.3;
    /// Speed test penalty with poor signal
    pub const SPEED_TEST_POOR_SIGNAL_PENALTY: f32 = -0.3;
    /// Bandwidth-intensive bonus with excellent signal
    pub const BANDWIDTH_EXCELLENT_SIGNAL: f32 = 0.2;
    /// Emergency boost factor for critical situations
    pub const EMERGENCY_DIAGNOSTIC: f32 = 0.7; // 70% of emergency_boost
}

/// Time-of-day scoring patterns
pub mod time_patterns {
    /// Morning hours range (work setup time)
    pub const MORNING_START: u8 = 6;
    pub const MORNING_END: u8 = 8;
    /// Work hours range (productivity focus)
    pub const WORK_START: u8 = 9;
    pub const WORK_END: u8 = 16;
    /// Evening hours range (personal/entertainment)
    pub const EVENING_START: u8 = 17;
    pub const EVENING_END: u8 = 21;
    /// Night hours range (minimal activity)
    pub const NIGHT_START: u8 = 22;
    pub const NIGHT_END: u8 = 23;
    pub const NIGHT_EARLY_START: u8 = 0;
    pub const NIGHT_EARLY_END: u8 = 5;
    
    /// Morning VPN/work connection bonus
    pub const MORNING_VPN_BONUS: f32 = 0.2;
    /// Morning entertainment penalty
    pub const MORNING_ENTERTAINMENT_PENALTY: f32 = -0.1;
    /// Work hours diagnostic/VPN bonus
    pub const WORK_PRODUCTIVITY_BONUS: f32 = 0.15;
    /// Evening bluetooth major boost
    pub const EVENING_BLUETOOTH_MAJOR: f32 = 0.35;
    /// Evening entertainment general boost
    pub const EVENING_ENTERTAINMENT: f32 = 0.25;
    /// Evening exit node bonus (content access)
    pub const EVENING_EXIT_NODE: f32 = 0.1;
    /// Night disconnect/airplane bonus
    pub const NIGHT_POWER_SAVING: f32 = 0.1;
    /// Night general activity penalty
    pub const NIGHT_ACTIVITY_PENALTY: f32 = -0.1;
}

/// Day-of-week scoring patterns
pub mod weekly_patterns {
    /// Weekday range (Monday=0 to Friday=4)
    pub const WEEKDAY_START: u8 = 0;
    pub const WEEKDAY_END: u8 = 4;
    /// Weekend range (Saturday=5, Sunday=6)
    pub const WEEKEND_START: u8 = 5;
    pub const WEEKEND_END: u8 = 6;
    
    /// Weekday work connection bonus
    pub const WEEKDAY_WORK_BONUS: f32 = 0.1;
    /// Weekend personal connection bonus
    pub const WEEKEND_PERSONAL_BONUS: f32 = 0.1;
}

/// Success rate and context scoring
pub mod context_scoring {
    /// Weight for general success rate in context scoring
    pub const GENERAL_SUCCESS_WEIGHT: f32 = 0.3;
    /// Weight for context-specific success rate
    pub const CONTEXT_SPECIFIC_WEIGHT: f32 = 0.7;
    /// Time division for context keys (6-hour blocks)
    pub const TIME_BLOCK_HOURS: u8 = 6;
}

/// Resource efficiency scoring
pub mod efficiency_scoring {
    /// Base time divisor for efficiency calculation (10 seconds)
    pub const BASE_TIME_DIVISOR: f32 = 10.0;
    /// Minimum reliable execution count for reliability bonus
    pub const RELIABILITY_THRESHOLD: u32 = 5;
    /// Reliability bonus for consistent actions
    pub const RELIABILITY_BONUS: f32 = 0.1;
    /// Default efficiency for unknown actions
    pub const DEFAULT_EFFICIENCY: f32 = 0.6;
}

/// Time calculation constants
pub mod time_constants {
    /// Hours in a week (for exponential decay calculations)
    pub const HOURS_PER_WEEK: f32 = 168.0;
    /// Seconds per hour
    pub const SECONDS_PER_HOUR: f32 = 3600.0;
    /// Maximum score cap
    pub const MAX_SCORE: f32 = 1.0;
}

/// Action priority categories
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionPriority {
    Critical,   // Essential network functions (connectivity tests, basic connections)
    High,       // Frequently used actions (VPN, main WiFi networks)
    Medium,     // Regular actions (Bluetooth, diagnostics)
    Low,        // Rarely used or situational actions
    Contextual, // Priority depends on current context
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
            network_condition_weight: scoring_weights::NETWORK_CONDITION,
            temporal_pattern_weight: scoring_weights::TEMPORAL_PATTERN,
            success_rate_weight: scoring_weights::SUCCESS_RATE,
            resource_efficiency_weight: scoring_weights::RESOURCE_EFFICIENCY,
            user_preference_weight: scoring_weights::USER_PREFERENCE,
            emergency_boost: scoring_weights::EMERGENCY_BOOST,
        }
    }
}

/// Action performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetrics {
    pub success_count: u32,
    pub failure_count: u32,
    pub average_execution_time: f32, // in seconds
    pub last_success_timestamp: Option<i64>,
    pub context_success_rate: HashMap<String, f32>, // context -> success rate
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
            priority_scores::NEUTRAL // Neutral assumption for new actions
        } else {
            self.success_count as f32 / total as f32
        }
    }

    pub fn record_success(&mut self, execution_time: f32, context: &str) {
        self.success_count += 1;
        self.last_success_timestamp = Some(chrono::Utc::now().timestamp());

        // Update average execution time
        let total_count = self.success_count + self.failure_count;
        self.average_execution_time = (self.average_execution_time * (total_count - 1) as f32
            + execution_time)
            / total_count as f32;

        // Update context-specific success rate using Bayesian updating
        let context_total = self.context_success_rate.get(context).unwrap_or(&priority_scores::NEUTRAL) * 2.0;
        let new_rate = (context_total + 1.0) / (context_total + 2.0);
        self.context_success_rate
            .insert(context.to_string(), new_rate);
    }

    pub fn record_failure(&mut self, context: &str) {
        self.failure_count += 1;

        // Update context-specific success rate using Bayesian updating
        let context_total = self.context_success_rate.get(context).unwrap_or(&priority_scores::NEUTRAL) * 2.0;
        let new_rate = context_total / (context_total + 2.0);
        self.context_success_rate
            .insert(context.to_string(), new_rate);
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
    connection_quality: f32, // 0.0 to 1.0
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
        priorities.insert(
            "diagnostic_connectivity".to_string(),
            ActionPriority::Critical,
        );
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

        // Combine all scores with weights using the multi-criteria scoring algorithm
        let weighted_score = base_priority * scoring_weights::BASE_PRIORITY
            + network_score * self.config.network_condition_weight
            + temporal_score * self.config.temporal_pattern_weight
            + success_score * self.config.success_rate_weight
            + efficiency_score * self.config.resource_efficiency_weight
            + usage_score * self.config.user_preference_weight
            + emergency_boost;

        debug!("Priority score for '{}': {:.3} (base: {:.2}, network: {:.2}, temporal: {:.2}, success: {:.2}, efficiency: {:.2}, usage: {:.2}, emergency: {:.2})",
            action_str, weighted_score, base_priority, network_score, temporal_score, success_score, efficiency_score, usage_score, emergency_boost);

        weighted_score.min(time_constants::MAX_SCORE)
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

    /// Get base priority score for an action using defined priority levels
    fn get_base_priority_score(&self, action_key: &str, action_str: &str) -> f32 {
        if let Some(priority) = self.action_priorities.get(action_key) {
            match priority {
                ActionPriority::Critical => priority_scores::CRITICAL,
                ActionPriority::High => priority_scores::HIGH,
                ActionPriority::Medium => priority_scores::MEDIUM,
                ActionPriority::Low => priority_scores::LOW,
                ActionPriority::Contextual => self.calculate_contextual_priority(action_str),
            }
        } else {
            // Infer priority from action string patterns
            self.infer_action_priority(action_str)
        }
    }

    /// Calculate network condition-based score with intelligent adaptation
    fn calculate_network_condition_score(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();
        let mut score: f32 = priority_scores::NEUTRAL; // Base score

        // Network type specific adjustments based on optimal action-network pairings
        match context.network_type {
            NetworkType::WiFi => {
                if action.contains("wifi") {
                    score += network_bonuses::WIFI_MATCH;
                } else if action.contains("bluetooth") {
                    score += network_bonuses::BLUETOOTH_ON_WIFI; // Bluetooth works well with WiFi
                }
            }
            NetworkType::Ethernet => {
                if action.contains("tailscale") || action.contains("vpn") {
                    score += network_bonuses::VPN_ON_ETHERNET; // VPNs work best on stable connections
                } else if action.contains("wifi") && !action.contains("disconnect") {
                    score += network_bonuses::WIFI_ON_ETHERNET_PENALTY; // Lower priority for WiFi when ethernet available
                }
            }
            NetworkType::Mobile => {
                if action.contains("disconnect") || action.contains("airplane") {
                    score += network_bonuses::DATA_SAVING_ON_MOBILE; // Data saving actions more important on mobile
                } else if action.contains("speed") {
                    score += network_bonuses::SPEED_TEST_ON_MOBILE_PENALTY; // Speed tests less relevant on mobile
                }
            }
            NetworkType::Unknown => {
                if action.contains("diagnostic") && action.contains("connectivity") {
                    score += network_bonuses::DIAGNOSTIC_ON_UNKNOWN; // Connectivity tests crucial when network unknown
                }
            }
            _ => {}
        }

        // Signal strength adaptive adjustments for optimal user experience
        if let Some(signal) = context.signal_strength {
            if signal < signal_thresholds::POOR_SIGNAL {
                // Poor signal - prioritize network switching and diagnostics
                if action.contains("diagnostic") && action.contains("connectivity") {
                    score += network_bonuses::CONNECTIVITY_POOR_SIGNAL; // Greatly prioritize connectivity tests with poor signal
                } else if action.contains("diagnostic")
                    || action.contains("disconnect")
                    || action.contains("wifi")
                {
                    score += network_bonuses::SWITCHING_POOR_SIGNAL;
                } else if action.contains("speed") || action.contains("streaming") {
                    score += network_bonuses::SPEED_TEST_POOR_SIGNAL_PENALTY; // Lower priority for speed tests with poor signal
                }
            } else if signal > signal_thresholds::EXCELLENT_SIGNAL {
                // Excellent signal - boost bandwidth-intensive actions
                if action.contains("speed") || action.contains("update") || action.contains("sync")
                {
                    score += network_bonuses::BANDWIDTH_EXCELLENT_SIGNAL;
                }
            }
        }

        score.clamp(0.0, time_constants::MAX_SCORE)
    }

    /// Calculate temporal/time-based score using learned patterns
    fn calculate_temporal_score(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();
        let mut score: f32 = priority_scores::NEUTRAL;

        // Time of day patterns based on typical user workflows
        match context.time_of_day {
            time_patterns::MORNING_START..=time_patterns::MORNING_END => {
                // Morning - work setup phase
                if action.contains("vpn") || action.contains("tailscale") {
                    score += time_patterns::MORNING_VPN_BONUS;
                } else if action.contains("entertainment") || action.contains("gaming") {
                    score += time_patterns::MORNING_ENTERTAINMENT_PENALTY;
                }
            }
            time_patterns::WORK_START..=time_patterns::WORK_END => {
                // Work hours - productivity focused environment
                if action.contains("diagnostic") || action.contains("vpn") {
                    score += time_patterns::WORK_PRODUCTIVITY_BONUS;
                }
            }
            time_patterns::EVENING_START..=time_patterns::EVENING_END => {
                // Evening - personal and entertainment use
                if action.contains("bluetooth") || action.contains("entertainment") {
                    score += time_patterns::EVENING_ENTERTAINMENT; // General entertainment boost
                } else if action.contains("exit") && action.contains("node") {
                    score += time_patterns::EVENING_EXIT_NODE; // Geographic shifting for content access
                }
            }
            time_patterns::NIGHT_START..=time_patterns::NIGHT_END | time_patterns::NIGHT_EARLY_START..=time_patterns::NIGHT_EARLY_END => {
                // Night - minimal activity and power saving
                if action.contains("disconnect") || action.contains("airplane") {
                    score += time_patterns::NIGHT_POWER_SAVING;
                } else {
                    score += time_patterns::NIGHT_ACTIVITY_PENALTY;
                }
            }
            _ => {}
        }

        // Day of week patterns for work-life balance adaptation
        match context.day_of_week {
            weekly_patterns::WEEKDAY_START..=weekly_patterns::WEEKDAY_END => {
                // Weekdays - work focused environment
                if action.contains("vpn") || action.contains("work") {
                    score += weekly_patterns::WEEKDAY_WORK_BONUS;
                }
            }
            weekly_patterns::WEEKEND_START..=weekly_patterns::WEEKEND_END => {
                // Weekends - personal focused activities
                if action.contains("bluetooth") || action.contains("personal") {
                    score += weekly_patterns::WEEKEND_PERSONAL_BONUS;
                }
            }
            _ => {}
        }

        score.clamp(0.0, time_constants::MAX_SCORE)
    }

    /// Calculate success rate score with context-aware weighting
    fn calculate_success_rate_score(&self, action_key: &str, context: &NetworkContext) -> f32 {
        if let Some(metrics) = self.action_metrics.get(action_key) {
            let general_success_rate = metrics.success_rate();

            // Generate context-specific key using time blocks for temporal grouping
            let context_key = format!("{}_{}", context.network_type as u8, context.time_of_day / context_scoring::TIME_BLOCK_HOURS);
            let context_success_rate = metrics
                .context_success_rate
                .get(&context_key)
                .copied()
                .unwrap_or(general_success_rate);

            // Weighted combination favoring context-specific historical data
            general_success_rate * context_scoring::GENERAL_SUCCESS_WEIGHT + context_success_rate * context_scoring::CONTEXT_SPECIFIC_WEIGHT
        } else {
            priority_scores::NEUTRAL // Neutral score for unknown actions
        }
    }

    /// Calculate resource efficiency score based on execution speed and reliability
    fn calculate_efficiency_score(&self, action_key: &str) -> f32 {
        if let Some(metrics) = self.action_metrics.get(action_key) {
            // Faster actions get higher scores using inverse relationship
            let time_score = (efficiency_scoring::BASE_TIME_DIVISOR / (metrics.average_execution_time + 1.0)).min(time_constants::MAX_SCORE);

            // Factor in reliability bonus for actions with consistent performance
            let reliability_bonus = if metrics.success_count > efficiency_scoring::RELIABILITY_THRESHOLD { 
                efficiency_scoring::RELIABILITY_BONUS 
            } else { 
                0.0 
            };

            time_score + reliability_bonus
        } else {
            // Default efficiency for unknown actions
            efficiency_scoring::DEFAULT_EFFICIENCY
        }
    }

    /// Calculate emergency situation boost for critical network issues
    fn calculate_emergency_boost(&self, action_str: &str, context: &NetworkContext) -> f32 {
        let action = action_str.to_lowercase();

        // Detect emergency situations requiring immediate action
        let is_emergency = context.signal_strength.is_some_and(|s| s < signal_thresholds::CRITICAL_SIGNAL)
            || context.network_type == NetworkType::Unknown;

        if is_emergency {
            if action.contains("diagnostic") || action.contains("connectivity") {
                return self.config.emergency_boost; // Full emergency boost for diagnostics
            } else if action.contains("disconnect") || action.contains("airplane") {
                return self.config.emergency_boost * network_bonuses::EMERGENCY_DIAGNOSTIC; // Partial boost for emergency disconnections
            }
        }

        0.0 // No boost for non-emergency situations
    }

    /// Calculate contextual priority for flexible priority actions
    fn calculate_contextual_priority(&self, action_str: &str) -> f32 {
        let action = action_str.to_lowercase();

        // Dynamic priority based on action type and current system state
        if action.contains("exit") && action.contains("node") {
            priority_scores::MEDIUM + 0.1 // Medium-high for exit nodes (context-dependent)
        } else if action.contains("profile") {
            priority_scores::MEDIUM - 0.1 // Medium-low for profile switches
        } else {
            priority_scores::NEUTRAL // Default neutral priority
        }
    }

    /// Infer priority from action string patterns using intelligent pattern matching
    fn infer_action_priority(&self, action_str: &str) -> f32 {
        let action = action_str.to_lowercase();

        // Critical patterns - essential network functions
        if action.contains("disconnect")
            || action.contains("connectivity")
            || action.contains("airplane")
        {
            return priority_scores::CRITICAL;
        }

        // High priority patterns - primary network operations
        if action.contains("wifi") || action.contains("vpn") || action.contains("tailscale") {
            return priority_scores::HIGH;
        }

        // Medium priority patterns - secondary network functions
        if action.contains("bluetooth") || action.contains("diagnostic") {
            return priority_scores::MEDIUM;
        }

        // Low priority patterns - advanced/configuration features
        if action.contains("profile") || action.contains("advanced") || action.contains("config") {
            return priority_scores::LOW;
        }

        priority_scores::MINIMAL // Default low-medium for unknown actions
    }

    /// Record action execution result for learning
    pub fn record_action_result(
        &mut self,
        action_str: &str,
        success: bool,
        execution_time: f32,
        context: &NetworkContext,
    ) {
        let action_key = self.normalize_action_key(action_str);
        let context_key = format!("{}_{}", context.network_type as u8, context.time_of_day / context_scoring::TIME_BLOCK_HOURS);

        let metrics = self.action_metrics.entry(action_key).or_default();

        if success {
            metrics.record_success(execution_time, &context_key);
        } else {
            metrics.record_failure(&context_key);
        }

        debug!(
            "Recorded action result: {} = {} in {:.2}s",
            action_str, success, execution_time
        );
    }

    /// Update network state cache for more accurate scoring
    pub fn update_network_state(
        &mut self,
        is_online: bool,
        connection_quality: f32,
        available_interfaces: Vec<String>,
    ) {
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

        let score =
            prioritizer.calculate_priority_score("diagnostic- ✅ Test Connectivity", &context, 0.5);

        assert!(score > 0.5); // Connectivity tests should have high priority
    }

    #[test]
    fn test_network_condition_scoring() {
        let prioritizer = ActionPrioritizer::new();
        let mut context = create_test_context();
        context.signal_strength = Some(0.2); // Very poor signal

        let diagnostic_score = prioritizer
            .calculate_network_condition_score("diagnostic- ✅ Test Connectivity", &context);

        let speedtest_score =
            prioritizer.calculate_network_condition_score("diagnostic- 🚀 Speed Test", &context);

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
        let morning_vpn_score =
            prioritizer.calculate_temporal_score("tailscale- ✅ Enable VPN", &context);

        // Evening context
        context.time_of_day = 20;
        let evening_vpn_score =
            prioritizer.calculate_temporal_score("tailscale- ✅ Enable VPN", &context);
        let evening_bluetooth_score =
            prioritizer.calculate_temporal_score("bluetooth- 📱 Connect Device", &context);

        assert!(morning_vpn_score > evening_vpn_score);
        assert!(evening_bluetooth_score > morning_vpn_score);
    }
}
