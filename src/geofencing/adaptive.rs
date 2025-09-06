//! Adaptive scanning system for intelligent geofencing
//!
//! Dynamically adjusts scanning intervals based on movement patterns,
//! battery status, zone stability, and learning phases.

use crate::geofencing::{LocationFingerprint, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::fs;

/// Movement detection state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MovementState {
    /// Stationary - location hasn't changed significantly
    Stationary,
    /// Moving slowly - gradual location changes
    SlowMovement,
    /// Moving quickly - rapid location changes
    FastMovement,
    /// Learning - mapping a new area
    Learning,
}

/// Power management state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PowerState {
    /// AC power connected
    Plugged,
    /// Battery power, level > 50%
    BatteryHigh,
    /// Battery power, level 20-50%
    BatteryMedium,
    /// Battery power, level < 20%
    BatteryLow,
    /// Critical battery level
    BatteryCritical,
    /// Battery charging (more optimistic than just battery level)
    BatteryCharging { level: u8 },
}

/// Zone stability assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneStability {
    /// How long we've been in current zone
    pub time_in_zone: ChronoDuration,
    /// Confidence score of current zone match
    pub confidence_score: f64,
    /// Number of zone changes in last hour
    pub recent_changes: u32,
    /// Whether the current location is well-established
    pub is_stable: bool,
}

/// Scanning frequency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFrequency {
    /// Base scanning interval
    pub base_interval: Duration,
    /// Minimum allowed interval
    pub min_interval: Duration,
    /// Maximum allowed interval
    pub max_interval: Duration,
    /// Movement-based multipliers
    pub movement_multipliers: MovementMultipliers,
    /// Power-based multipliers  
    pub power_multipliers: PowerMultipliers,
}

/// Movement-based scanning multipliers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovementMultipliers {
    pub stationary: f64,
    pub slow_movement: f64,
    pub fast_movement: f64,
    pub learning: f64,
}

/// Power-based scanning multipliers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMultipliers {
    pub plugged: f64,
    pub battery_high: f64,
    pub battery_medium: f64,
    pub battery_low: f64,
    pub battery_critical: f64,
}

impl Default for ScanFrequency {
    fn default() -> Self {
        Self {
            base_interval: Duration::from_secs(30),
            min_interval: Duration::from_secs(5),
            max_interval: Duration::from_secs(300), // 5 minutes
            movement_multipliers: MovementMultipliers {
                stationary: 2.0,        // Scan less frequently when stationary
                slow_movement: 1.0,     // Normal frequency
                fast_movement: 0.5,     // Scan more frequently when moving fast
                learning: 0.2,          // Very frequent when learning
            },
            power_multipliers: PowerMultipliers {
                plugged: 0.8,          // Can afford frequent scans
                battery_high: 1.0,     // Normal frequency
                battery_medium: 1.5,   // Reduce frequency to save battery
                battery_low: 2.0,      // Further reduce frequency
                battery_critical: 4.0, // Minimal scanning to preserve battery
            },
        }
    }
}

/// Adaptive scanner that adjusts intervals based on context
pub struct AdaptiveScanner {
    config: ScanFrequency,
    movement_detector: MovementDetector,
    power_monitor: PowerMonitor,
    zone_stability: ZoneStability,
    scan_history: VecDeque<ScanEvent>,
    current_interval: Duration,
    last_adjustment: DateTime<Utc>,
}

/// Movement detection based on WiFi fingerprint changes
struct MovementDetector {
    fingerprint_history: VecDeque<LocationFingerprint>,
    movement_state: MovementState,
    stationary_threshold: f64,
    movement_threshold: f64,
    fast_movement_threshold: f64,
    max_history: usize,
}

/// Power monitoring for battery-aware scanning
struct PowerMonitor {
    current_state: PowerState,
    last_check: DateTime<Utc>,
    battery_path: String,
}

/// Individual scan event for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScanEvent {
    timestamp: DateTime<Utc>,
    duration: Duration,
    fingerprint_confidence: f64,
    zone_detected: Option<String>,
    movement_state: MovementState,
    power_state: PowerState,
}

impl AdaptiveScanner {
    /// Create new adaptive scanner
    pub fn new(config: ScanFrequency) -> Self {
        debug!("Creating adaptive scanner with base interval: {:?}", config.base_interval);
        
        let current_interval = config.base_interval;
        
        Self {
            config,
            movement_detector: MovementDetector::new(),
            power_monitor: PowerMonitor::new(),
            zone_stability: ZoneStability {
                time_in_zone: ChronoDuration::zero(),
                confidence_score: 0.0,
                recent_changes: 0,
                is_stable: false,
            },
            scan_history: VecDeque::new(),
            current_interval,
            last_adjustment: Utc::now(),
        }
    }

    /// Calculate optimal scanning interval based on current context
    pub async fn calculate_optimal_interval(&mut self) -> Duration {
        debug!("Calculating optimal scanning interval");

        // Update power state
        self.power_monitor.update_power_state().await;

        // Get current context
        let movement_state = self.movement_detector.get_movement_state();
        let power_state = self.power_monitor.get_power_state();

        debug!("Current context - Movement: {:?}, Power: {:?}, Zone stability: {}", 
               movement_state, power_state, self.zone_stability.is_stable);

        // Calculate base multiplier from movement
        let movement_multiplier = match movement_state {
            MovementState::Stationary => self.config.movement_multipliers.stationary,
            MovementState::SlowMovement => self.config.movement_multipliers.slow_movement,
            MovementState::FastMovement => self.config.movement_multipliers.fast_movement,
            MovementState::Learning => self.config.movement_multipliers.learning,
        };

        // Calculate power multiplier
        let power_multiplier = match power_state {
            PowerState::Plugged => self.config.power_multipliers.plugged,
            PowerState::BatteryHigh => self.config.power_multipliers.battery_high,
            PowerState::BatteryMedium => self.config.power_multipliers.battery_medium,
            PowerState::BatteryLow => self.config.power_multipliers.battery_low,
            PowerState::BatteryCritical => self.config.power_multipliers.battery_critical,
            PowerState::BatteryCharging { level } => {
                // When charging, use more optimistic multiplier based on level
                if *level >= 50 {
                    self.config.power_multipliers.battery_high * 0.8  // 20% more frequent when charging
                } else if *level >= 20 {
                    self.config.power_multipliers.battery_medium * 0.9  // 10% more frequent when charging
                } else {
                    self.config.power_multipliers.battery_low * 0.95  // Slightly more frequent when charging
                }
            }
        };

        // Calculate stability multiplier
        let stability_multiplier = if self.zone_stability.is_stable {
            1.5 // Scan less frequently when in stable zone
        } else {
            0.8 // Scan more frequently when zone is unstable
        };

        // Combine all multipliers
        let total_multiplier = movement_multiplier * power_multiplier * stability_multiplier;

        // Calculate new interval
        let new_interval_secs = (self.config.base_interval.as_secs_f64() * total_multiplier) as u64;
        let new_interval = Duration::from_secs(new_interval_secs)
            .max(self.config.min_interval)
            .min(self.config.max_interval);

        // Only update if significantly different to avoid thrashing
        if self.should_update_interval(new_interval) {
            debug!("Updating scan interval from {:?} to {:?} (multiplier: {:.2})",
                   self.current_interval, new_interval, total_multiplier);
            
            self.current_interval = new_interval;
            self.last_adjustment = Utc::now();
        }

        self.current_interval
    }

    /// Update movement detection with new fingerprint
    pub fn update_movement_detection(&mut self, fingerprint: &LocationFingerprint) {
        debug!("Updating movement detection with fingerprint confidence: {:.2}", 
               fingerprint.confidence_score);
        self.movement_detector.update(fingerprint);
    }

    /// Update zone stability information
    pub fn update_zone_stability(&mut self, zone_id: Option<&str>, confidence: f64) {
        debug!("Updating zone stability - Zone: {:?}, Confidence: {:.2}", zone_id, confidence);

        let now = Utc::now();

        if let Some(_current_zone) = zone_id {
            // Check if zone changed
            if self.zone_stability.time_in_zone == ChronoDuration::zero() {
                // First zone detection
                self.zone_stability.time_in_zone = ChronoDuration::zero();
                self.zone_stability.recent_changes = 1;
            } else {
                // Update time in zone
                self.zone_stability.time_in_zone = 
                    now - (now - self.zone_stability.time_in_zone);
            }

            self.zone_stability.confidence_score = confidence;
            self.zone_stability.is_stable = 
                confidence > 0.8 && 
                self.zone_stability.time_in_zone > ChronoDuration::minutes(5) &&
                self.zone_stability.recent_changes < 3;

        } else {
            // No zone detected
            self.zone_stability.time_in_zone = ChronoDuration::zero();
            self.zone_stability.confidence_score = 0.0;
            self.zone_stability.is_stable = false;
        }

        debug!("Zone stability updated - Stable: {}, Time in zone: {}min, Confidence: {:.2}",
               self.zone_stability.is_stable,
               self.zone_stability.time_in_zone.num_minutes(),
               self.zone_stability.confidence_score);
    }

    /// Record a scan event for analysis
    pub fn record_scan_event(&mut self, duration: Duration, fingerprint: &LocationFingerprint, zone_id: Option<String>) {
        let event = ScanEvent {
            timestamp: Utc::now(),
            duration,
            fingerprint_confidence: fingerprint.confidence_score,
            zone_detected: zone_id,
            movement_state: self.movement_detector.get_movement_state().clone(),
            power_state: self.power_monitor.get_power_state().clone(),
        };

        debug!("Recording scan event: duration={:?}, confidence={:.2}, zone={:?}",
               event.duration, event.fingerprint_confidence, event.zone_detected);

        self.scan_history.push_back(event);

        // Maintain history size
        while self.scan_history.len() > 100 {
            self.scan_history.pop_front();
        }
    }

    /// Get current scanning statistics
    pub fn get_scanning_stats(&self) -> ScanningStats {
        let recent_events: Vec<_> = self.scan_history
            .iter()
            .filter(|e| Utc::now() - e.timestamp < ChronoDuration::hours(1))
            .collect();

        let avg_scan_duration = if !recent_events.is_empty() {
            let total: u64 = recent_events.iter()
                .map(|e| e.duration.as_millis() as u64)
                .sum();
            Duration::from_millis(total / recent_events.len() as u64)
        } else {
            Duration::from_millis(0)
        };

        let zone_changes = recent_events
            .windows(2)
            .filter(|pair| pair[0].zone_detected != pair[1].zone_detected)
            .count();

        ScanningStats {
            current_interval: self.current_interval,
            movement_state: self.movement_detector.get_movement_state().clone(),
            power_state: self.power_monitor.get_power_state().clone(),
            zone_stability: self.zone_stability.clone(),
            recent_scan_count: recent_events.len(),
            average_scan_duration: avg_scan_duration,
            zone_changes_last_hour: zone_changes,
            last_adjustment: self.last_adjustment,
        }
    }

    /// Check if interval should be updated (avoid thrashing)
    fn should_update_interval(&self, new_interval: Duration) -> bool {
        let diff = if new_interval > self.current_interval {
            new_interval - self.current_interval
        } else {
            self.current_interval - new_interval
        };

        // Only update if difference is significant (> 20% or > 10 seconds)
        let threshold = (self.current_interval.as_secs_f64() * 0.2).max(10.0);
        diff.as_secs_f64() > threshold
    }

    /// Enter learning mode for zone mapping
    pub fn enter_learning_mode(&mut self, duration: Duration) {
        info!("Entering learning mode for {:?}", duration);
        self.movement_detector.set_learning_mode(duration);
    }

    /// Check if currently in learning mode
    pub fn is_learning(&self) -> bool {
        self.movement_detector.is_learning()
    }
}

impl MovementDetector {
    fn new() -> Self {
        Self {
            fingerprint_history: VecDeque::new(),
            movement_state: MovementState::Stationary,
            stationary_threshold: 0.95,    // Very similar fingerprints = stationary
            movement_threshold: 0.7,       // Moderate similarity = slow movement
            fast_movement_threshold: 0.4,  // Low similarity = fast movement
            max_history: 10,
        }
    }

    fn update(&mut self, fingerprint: &LocationFingerprint) {
        debug!("Updating movement detector with {} WiFi networks", 
               fingerprint.wifi_networks.len());

        self.fingerprint_history.push_back(fingerprint.clone());

        // Maintain history size
        while self.fingerprint_history.len() > self.max_history {
            self.fingerprint_history.pop_front();
        }

        // Analyze movement based on fingerprint similarity
        if self.fingerprint_history.len() >= 2 {
            let recent_similarity = self.calculate_recent_similarity();
            self.movement_state = self.classify_movement(recent_similarity);
            
            debug!("Movement analysis - Recent similarity: {:.2}, State: {:?}",
                   recent_similarity, self.movement_state);
        }
    }

    fn calculate_recent_similarity(&self) -> f64 {
        if self.fingerprint_history.len() < 2 {
            return 1.0;
        }

        // Compare last 3 fingerprints for movement trend
        let compare_count = std::cmp::min(3, self.fingerprint_history.len());
        let recent: Vec<_> = self.fingerprint_history
            .iter()
            .rev()
            .take(compare_count)
            .collect();

        if recent.len() < 2 {
            return 1.0;
        }

        // Calculate average similarity between consecutive fingerprints
        let mut total_similarity = 0.0;
        let mut comparisons = 0;

        for i in 1..recent.len() {
            let similarity = self.calculate_fingerprint_similarity(recent[i-1], recent[i]);
            total_similarity += similarity;
            comparisons += 1;
        }

        if comparisons > 0 {
            total_similarity / comparisons as f64
        } else {
            1.0
        }
    }

    fn calculate_fingerprint_similarity(&self, fp1: &LocationFingerprint, fp2: &LocationFingerprint) -> f64 {
        // Simple Jaccard similarity for WiFi networks
        let set1: std::collections::HashSet<_> = fp1.wifi_networks.iter().collect();
        let set2: std::collections::HashSet<_> = fp2.wifi_networks.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 {
            1.0 // Both empty
        } else {
            intersection as f64 / union as f64
        }
    }

    fn classify_movement(&self, similarity: f64) -> MovementState {
        if similarity >= self.stationary_threshold {
            MovementState::Stationary
        } else if similarity >= self.movement_threshold {
            MovementState::SlowMovement
        } else if similarity >= self.fast_movement_threshold {
            // Between movement_threshold and fast_movement_threshold = regular movement speed
            MovementState::SlowMovement
        } else {
            // Below fast_movement_threshold = very rapid location changes
            MovementState::FastMovement
        }
    }

    fn get_movement_state(&self) -> &MovementState {
        &self.movement_state
    }

    fn set_learning_mode(&mut self, _duration: Duration) {
        self.movement_state = MovementState::Learning;
        // In a real implementation, we'd set a timer to exit learning mode
    }

    fn is_learning(&self) -> bool {
        matches!(self.movement_state, MovementState::Learning)
    }
}

impl PowerMonitor {
    fn new() -> Self {
        Self {
            current_state: PowerState::BatteryHigh, // Default assumption
            last_check: Utc::now(),
            // TODO: my battery is here, need to handle it "/sys/class/power_supply/macsmc-battery"
            battery_path: "/sys/class/power_supply/BAT0".to_string(),
        }
    }

    async fn update_power_state(&mut self) {
        // Only check every 30 seconds to avoid excessive I/O
        let now = Utc::now();
        if now - self.last_check < ChronoDuration::seconds(30) {
            return;
        }

        self.last_check = now;

        match self.read_battery_info().await {
            Ok(info) => {
                let old_state = self.current_state.clone();
                self.current_state = self.classify_power_state(info);
                
                if old_state != self.current_state {
                    debug!("Power state changed from {:?} to {:?}", old_state, self.current_state);
                }
            }
            Err(e) => {
                debug!("Failed to read battery info: {}", e);
                // Keep current state on error
            }
        }
    }

    async fn read_battery_info(&self) -> Result<BatteryInfo> {
        // Try to read AC adapter status first
        let ac_connected = if let Ok(content) = fs::read_to_string("/sys/class/power_supply/ADP1/online").await {
            content.trim() == "1"
        } else if let Ok(content) = fs::read_to_string("/sys/class/power_supply/macsmc-ac/online").await {
            content.trim() == "1"
        } else if let Ok(content) = fs::read_to_string("/sys/class/power_supply/AC/online").await {
            content.trim() == "1"
        } else if let Ok(content) = fs::read_to_string("/sys/class/power_supply/ACAD/online").await {
            content.trim() == "1"
        } else {
            false
        };

        if ac_connected {
            return Ok(BatteryInfo {
                ac_connected: true,
                battery_level: 100,
                charging: true,
            });
        }

        // Read battery information
        let capacity_path = format!("{}/capacity", self.battery_path);
        let status_path = format!("{}/status", self.battery_path);

        let battery_level = match fs::read_to_string(&capacity_path).await {
            Ok(content) => content.trim().parse::<u8>().unwrap_or(50),
            Err(_) => {
                debug!("Could not read battery capacity from {}", capacity_path);
                return Ok(BatteryInfo {
                    ac_connected: false,
                    battery_level: 50, // Unknown, assume medium
                    charging: false,
                });
            }
        };

        let charging = match fs::read_to_string(&status_path).await {
            Ok(content) => content.trim().eq_ignore_ascii_case("charging"),
            Err(_) => false,
        };

        Ok(BatteryInfo {
            ac_connected,
            battery_level,
            charging,
        })
    }

    fn classify_power_state(&self, info: BatteryInfo) -> PowerState {
        if info.ac_connected {
            PowerState::Plugged
        } else if info.charging {
            // When charging, be more optimistic about scanning frequency
            PowerState::BatteryCharging { level: info.battery_level }
        } else if info.battery_level >= 50 {
            PowerState::BatteryHigh
        } else if info.battery_level >= 20 {
            PowerState::BatteryMedium
        } else if info.battery_level >= 10 {
            PowerState::BatteryLow
        } else {
            PowerState::BatteryCritical
        }
    }

    fn get_power_state(&self) -> &PowerState {
        &self.current_state
    }
}

/// Battery information
#[derive(Debug)]
struct BatteryInfo {
    ac_connected: bool,
    battery_level: u8,
    charging: bool,
}

/// Comprehensive scanning statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanningStats {
    pub current_interval: Duration,
    pub movement_state: MovementState,
    pub power_state: PowerState,
    pub zone_stability: ZoneStability,
    pub recent_scan_count: usize,
    pub average_scan_duration: Duration,
    pub zone_changes_last_hour: usize,
    pub last_adjustment: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn create_test_fingerprint(networks: Vec<&str>) -> LocationFingerprint {
        let wifi_networks = networks.into_iter()
            .enumerate()
            .map(|(i, ssid)| crate::geofencing::NetworkSignature {
                ssid_hash: ssid.to_string(),
                bssid_prefix: format!("aa:bb:cc:{:02x}", i),
                signal_strength: -50,
                frequency: 2412,
            })
            .collect();

        LocationFingerprint {
            wifi_networks,
            bluetooth_devices: BTreeSet::new(),
            ip_location: None,
            confidence_score: 0.8,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_scan_frequency_default() {
        let freq = ScanFrequency::default();
        assert_eq!(freq.base_interval, Duration::from_secs(30));
        assert_eq!(freq.min_interval, Duration::from_secs(5));
        assert_eq!(freq.max_interval, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_adaptive_scanner_creation() {
        let config = ScanFrequency::default();
        let scanner = AdaptiveScanner::new(config);
        
        assert_eq!(scanner.current_interval, Duration::from_secs(30));
        assert!(!scanner.is_learning());
    }

    #[test]
    fn test_movement_detection() {
        let mut detector = MovementDetector::new();
        
        // Same location
        let fp1 = create_test_fingerprint(vec!["network1", "network2"]);
        let fp2 = create_test_fingerprint(vec!["network1", "network2"]);
        
        detector.update(&fp1);
        detector.update(&fp2);
        
        assert_eq!(detector.get_movement_state(), &MovementState::Stationary);
        
        // Different location
        let fp3 = create_test_fingerprint(vec!["network3", "network4"]);
        detector.update(&fp3);
        
        // Should detect movement
        assert_ne!(detector.get_movement_state(), &MovementState::Stationary);
    }

    #[test]
    fn test_fingerprint_similarity() {
        let detector = MovementDetector::new();
        
        let fp1 = create_test_fingerprint(vec!["network1", "network2"]);
        let fp2 = create_test_fingerprint(vec!["network1", "network2"]);
        let fp3 = create_test_fingerprint(vec!["network1", "network3"]);
        let fp4 = create_test_fingerprint(vec!["network4", "network5"]);
        
        // Identical fingerprints
        let sim1 = detector.calculate_fingerprint_similarity(&fp1, &fp2);
        assert_eq!(sim1, 1.0);
        
        // Partial overlap
        let sim2 = detector.calculate_fingerprint_similarity(&fp1, &fp3);
        assert!(sim2 > 0.0 && sim2 < 1.0);
        
        // No overlap
        let sim3 = detector.calculate_fingerprint_similarity(&fp1, &fp4);
        assert_eq!(sim3, 0.0);
    }

    #[test]
    fn test_zone_stability_update() {
        let config = ScanFrequency::default();
        let mut scanner = AdaptiveScanner::new(config);
        
        // Initial update
        scanner.update_zone_stability(Some("home"), 0.9);
        assert!(scanner.zone_stability.confidence_score > 0.8);
        
        // No zone
        scanner.update_zone_stability(None, 0.0);
        assert_eq!(scanner.zone_stability.confidence_score, 0.0);
        assert!(!scanner.zone_stability.is_stable);
    }
}
