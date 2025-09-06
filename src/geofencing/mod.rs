//! Advanced geofencing module for intelligent location-aware network management
//!
//! Provides privacy-first location detection using WiFi fingerprinting,
//! ML-powered zone suggestions, adaptive scanning, comprehensive security,
//! and automatic network configuration based on detected zones.

#[cfg(feature = "geofencing")]
pub mod daemon;
#[cfg(feature = "geofencing")]
pub mod fingerprinting;
#[cfg(feature = "geofencing")]
pub mod ipc;
#[cfg(feature = "geofencing")]
pub mod zones;

// Advanced components
#[cfg(feature = "geofencing")]
pub mod retry;
#[cfg(feature = "geofencing")]
pub mod adaptive;
#[cfg(feature = "geofencing")]
pub mod lifecycle;
#[cfg(feature = "geofencing")]
pub mod security;
#[cfg(feature = "geofencing")]
pub mod performance;
#[cfg(feature = "geofencing")]
pub mod observability;
#[cfg(feature = "geofencing")]
pub mod config;
#[cfg(feature = "geofencing")]
pub mod advanced_zones;

#[cfg(feature = "geofencing")]
pub use fingerprinting::*;
#[cfg(feature = "geofencing")]
pub use zones::*;

// Re-export key types from advanced components
#[cfg(feature = "geofencing")]
pub use retry::{RetryManager, RetryConfig, RetryableAction};
#[cfg(feature = "geofencing")]
pub use adaptive::{AdaptiveScanner, ScanFrequency, MovementState, PowerState};
#[cfg(feature = "geofencing")]
pub use lifecycle::{LifecycleManager, SystemEvent, DaemonState};
#[cfg(feature = "geofencing")]
pub use security::{SecureCommandExecutor, SecurityPolicy};
#[cfg(feature = "geofencing")]
pub use performance::{PerformanceOptimizer, CacheManager, ConnectionPool, BatchProcessor};
#[cfg(feature = "geofencing")]
pub use observability::{ObservabilityManager, HealthStatus, DaemonMetrics};
#[cfg(feature = "geofencing")]
pub use config::{ConfigManager, EnhancedConfig, ValidationResult};
#[cfg(feature = "geofencing")]
pub use advanced_zones::{AdvancedZoneManager, ZoneSuggestion, ZoneHierarchy};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Privacy mode for location detection
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum PrivacyMode {
    /// Only WiFi networks, hashed identifiers, local processing only
    #[default]
    High,
    /// WiFi + Bluetooth beacons, some caching allowed
    Medium,
    /// All methods including IP geolocation
    Low,
}

/// Coarse location information from IP geolocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoarseLocation {
    pub country: String,
    pub region: String,
    pub city: String,
}

/// WiFi network signature for location fingerprinting
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetworkSignature {
    /// SHA-256 hash of SSID for privacy
    pub ssid_hash: String,
    /// First 6 characters of BSSID (manufacturer identifier)
    pub bssid_prefix: String,
    /// Signal strength in dBm
    pub signal_strength: i8,
    /// Network frequency in MHz
    pub frequency: u32,
}

/// Location fingerprint combining multiple detection methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationFingerprint {
    /// WiFi networks visible at this location
    pub wifi_networks: BTreeSet<NetworkSignature>,
    /// Bluetooth device MAC addresses (hashed)
    pub bluetooth_devices: BTreeSet<String>,
    /// IP-based coarse location (optional)
    pub ip_location: Option<CoarseLocation>,
    /// Confidence score (0.0 to 1.0)
    pub confidence_score: f64,
    /// When this fingerprint was created
    pub timestamp: DateTime<Utc>,
}

impl Default for LocationFingerprint {
    fn default() -> Self {
        Self {
            wifi_networks: BTreeSet::new(),
            bluetooth_devices: BTreeSet::new(),
            ip_location: None,
            confidence_score: 0.0,
            timestamp: Utc::now(),
        }
    }
}

/// Actions to execute when entering a geofence zone
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZoneActions {
    /// WiFi network to connect to
    pub wifi: Option<String>,
    /// VPN connection to establish
    pub vpn: Option<String>,
    /// Tailscale exit node ("auto", "none", or specific node)
    pub tailscale_exit_node: Option<String>,
    /// Tailscale shields up/down
    pub tailscale_shields: Option<bool>,
    /// Bluetooth devices to connect
    #[serde(default)]
    pub bluetooth: Vec<String>,
    /// Custom shell commands to execute
    #[serde(default)]
    pub custom_commands: Vec<String>,
    /// Whether to send notifications
    #[serde(default = "default_notifications")]
    pub notifications: bool,
}

/// Default value for notifications field
fn default_notifications() -> bool {
    true
}

impl Default for ZoneActions {
    fn default() -> Self {
        Self {
            wifi: None,
            vpn: None,
            tailscale_exit_node: None,
            tailscale_shields: None,
            bluetooth: Vec::new(),
            custom_commands: Vec::new(),
            notifications: true,
        }
    }
}

/// Geographic zone with location fingerprint and associated actions
#[derive(Debug, Clone, Serialize)]
#[derive(Deserialize)]
#[serde(from = "GeofenceZoneHelper")]
pub struct GeofenceZone {
    /// Unique zone identifier
    pub id: String,
    /// Human-readable zone name
    pub name: String,
    /// Location fingerprints for matching (supports multiple fingerprints per zone)
    pub fingerprints: Vec<LocationFingerprint>,
    /// Confidence threshold for zone matching (0.0 to 1.0)
    pub confidence_threshold: f64,
    /// Actions to execute in this zone
    pub actions: ZoneActions,
    /// When this zone was created
    pub created_at: DateTime<Utc>,
    /// Last time this zone was matched
    pub last_matched: Option<DateTime<Utc>>,
    /// Number of times this zone has been entered
    pub match_count: u32,
}

/// Helper struct for deserializing GeofenceZone with optional id field
#[derive(Debug, Deserialize)]
struct GeofenceZoneHelper {
    /// Optional zone identifier (will be generated from name if not provided)
    pub id: Option<String>,
    /// Human-readable zone name
    pub name: String,
    /// Location fingerprints for matching (supports multiple fingerprints per zone)
    #[serde(default)]
    pub fingerprints: Vec<LocationFingerprint>,
    /// Confidence threshold for zone matching (0.0 to 1.0)
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Actions to execute in this zone
    pub actions: ZoneActions,
    /// When this zone was created
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    /// Last time this zone was matched
    pub last_matched: Option<DateTime<Utc>>,
    /// Number of times this zone has been entered
    #[serde(default)]
    pub match_count: u32,
}

impl From<GeofenceZoneHelper> for GeofenceZone {
    fn from(helper: GeofenceZoneHelper) -> Self {
        let id = helper.id.unwrap_or_else(|| {
            // Generate ID from name: lowercase, replace spaces with underscores
            helper.name.to_lowercase().replace(' ', "_")
        });
        
        Self {
            id,
            name: helper.name,
            fingerprints: helper.fingerprints,
            confidence_threshold: helper.confidence_threshold,
            actions: helper.actions,
            created_at: helper.created_at,
            last_matched: helper.last_matched,
            match_count: helper.match_count,
        }
    }
}

/// Default confidence threshold
fn default_confidence_threshold() -> f64 {
    0.8
}

/// Geofencing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofencingConfig {
    /// Whether geofencing is enabled
    pub enabled: bool,
    /// Privacy mode for location detection
    pub privacy_mode: PrivacyMode,
    /// How often to scan for location changes (seconds)
    pub scan_interval_seconds: u64,
    /// Minimum confidence score to trigger zone actions
    pub confidence_threshold: f64,
    /// Configured geofence zones
    pub zones: Vec<GeofenceZone>,
    /// Whether to send notifications on zone changes
    pub notifications: bool,
}

impl Default for GeofencingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            privacy_mode: PrivacyMode::High,
            scan_interval_seconds: 30,
            confidence_threshold: 0.8,
            zones: Vec::new(),
            notifications: true,
        }
    }
}

/// Location change event
#[derive(Debug, Clone)]
pub struct LocationChange {
    /// Previous zone (if any)
    pub from: Option<GeofenceZone>,
    /// New zone
    pub to: GeofenceZone,
    /// Confidence score for the match
    pub confidence: f64,
    /// Suggested actions based on zone configuration
    pub suggested_actions: ZoneActions,
}

/// Zone creation suggestion from ML analysis (deprecated - use advanced_zones::ZoneSuggestion)
#[derive(Debug, Clone)]
pub struct LegacyZoneSuggestion {
    /// Suggested name for the zone
    pub suggested_name: String,
    /// Confidence that this should be a zone
    pub confidence: f64,
    /// Suggested actions based on usage patterns
    pub suggested_actions: ZoneActions,
}

/// Errors that can occur during geofencing operations
#[derive(Debug, thiserror::Error)]
pub enum GeofenceError {
    #[error("Location detection failed: {0}")]
    LocationDetection(String),

    #[error("Zone matching failed: {0}")]
    ZoneMatching(String),

    #[error("Zone not found: {0}")]
    ZoneNotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Action execution failed: {0}")]
    ActionExecution(String),

    #[error("IPC communication error: {0}")]
    Ipc(String),

    #[error("Daemon error: {0}")]
    Daemon(String),
}

pub type Result<T> = std::result::Result<T, GeofenceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_signature_ordering() {
        let sig1 = NetworkSignature {
            ssid_hash: "hash1".to_string(),
            bssid_prefix: "aa:bb:cc".to_string(),
            signal_strength: -50,
            frequency: 2412,
        };

        let sig2 = NetworkSignature {
            ssid_hash: "hash2".to_string(),
            bssid_prefix: "aa:bb:cc".to_string(),
            signal_strength: -60,
            frequency: 2412,
        };

        assert!(sig1 < sig2); // Deterministic ordering for BTreeSet
    }

    #[test]
    fn test_geofence_zone_creation() {
        let zone = GeofenceZone {
            id: "home".to_string(),
            name: "🏠 Home".to_string(),
            fingerprints: vec![LocationFingerprint::default()],
            confidence_threshold: 0.8,
            actions: ZoneActions {
                wifi: Some("HomeWiFi-5G".to_string()),
                tailscale_exit_node: Some("none".to_string()),
                notifications: true,
                ..Default::default()
            },
            created_at: Utc::now(),
            last_matched: None,
            match_count: 0,
        };

        assert_eq!(zone.id, "home");
        assert!(zone.actions.wifi.is_some());
    }

    #[test]
    fn test_zone_id_auto_generation() {
        let toml_config = r#"
enabled = true
privacy_mode = "High"
scan_interval_seconds = 30
confidence_threshold = 0.8
notifications = true

[[zones]]
name = "home"
[zones.actions]
wifi = "HomeWiFi"
tailscale_shields = true

[[zones]]
name = "My Office"
[zones.actions]
wifi = "OfficeWiFi"
tailscale_exit_node = "office-node"
"#;

        let config: GeofencingConfig = toml::from_str(toml_config).expect("Failed to parse TOML");
        
        assert_eq!(config.zones.len(), 2);
        
        // First zone: "home" should generate id "home"
        let home_zone = config.zones.iter().find(|z| z.name == "home").unwrap();
        assert_eq!(home_zone.id, "home");
        assert_eq!(home_zone.actions.wifi, Some("HomeWiFi".to_string()));
        
        // Second zone: "My Office" should generate id "my_office"  
        let office_zone = config.zones.iter().find(|z| z.name == "My Office").unwrap();
        assert_eq!(office_zone.id, "my_office");
        assert_eq!(office_zone.actions.wifi, Some("OfficeWiFi".to_string()));
        
        // Both zones should have default values
        assert_eq!(home_zone.confidence_threshold, 0.8);
        assert_eq!(office_zone.confidence_threshold, 0.8);
        assert_eq!(home_zone.match_count, 0);
        assert_eq!(office_zone.match_count, 0);
    }
}
