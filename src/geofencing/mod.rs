//! Geofencing module for location-aware network management
//! 
//! Provides privacy-first location detection using WiFi fingerprinting
//! and automatic network configuration based on detected zones.

#[cfg(feature = "geofencing")]
pub mod fingerprinting;
#[cfg(feature = "geofencing")]
pub mod zones;
#[cfg(feature = "geofencing")]
pub mod daemon;
#[cfg(feature = "geofencing")]
pub mod ipc;

#[cfg(feature = "geofencing")]
pub use fingerprinting::*;
#[cfg(feature = "geofencing")]
pub use zones::*;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use chrono::{DateTime, Utc};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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
    pub bluetooth: Vec<String>,
    /// Custom shell commands to execute
    pub custom_commands: Vec<String>,
    /// Whether to send notifications
    pub notifications: bool,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeofenceZone {
    /// Unique zone identifier
    pub id: String,
    /// Human-readable zone name
    pub name: String,
    /// Location fingerprint for matching
    pub fingerprint: LocationFingerprint,
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

/// Zone creation suggestion from ML analysis
#[derive(Debug, Clone)]
pub struct ZoneSuggestion {
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
            fingerprint: LocationFingerprint::default(),
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
}