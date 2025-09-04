//! Geofence zone management and matching
//! 
//! Handles creation, storage, and matching of geographic zones
//! with their associated network configurations.

use super::{
    fingerprinting::{calculate_weighted_similarity, create_wifi_fingerprint},
    GeofenceError, GeofenceZone, GeofencingConfig, LocationChange, LocationFingerprint,
    PrivacyMode, Result, ZoneActions,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(feature = "geofencing")]
use uuid::Uuid;

/// Persistent daemon state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DaemonState {
    current_zone: Option<String>,
    #[serde(default)]
    total_zone_changes: u32,
    last_scan: Option<DateTime<Utc>>,
}

/// Zone manager for geofencing operations
pub struct ZoneManager {
    config: GeofencingConfig,
    zones: HashMap<String, GeofenceZone>,
    current_zone: Option<String>,
    daemon_state: DaemonState,
}

impl ZoneManager {
    /// Create new zone manager with configuration
    pub fn new(config: GeofencingConfig) -> Self {
        let mut manager = Self {
            config,
            zones: HashMap::new(),
            current_zone: None,
            daemon_state: DaemonState::default(),
        };
        
        // Load zones from persistent storage first, then config
        if let Ok(persistent_zones) = manager.load_zones_from_disk() {
            manager.zones = persistent_zones;
        }
        
        // Load daemon state
        if let Ok(state) = manager.load_daemon_state() {
            manager.daemon_state = state;
            manager.current_zone = manager.daemon_state.current_zone.clone();
        }
        
        // Add any zones from config that aren't already loaded
        for zone in &manager.config.zones {
            manager.zones.entry(zone.id.clone()).or_insert_with(|| zone.clone());
        }
        
        manager
    }
    
    /// Get zones storage file path
    fn get_zones_file_path(&self) -> PathBuf {
        let mut path = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("network-dmenu");
        path.push("zones.json");
        path
    }
    
    /// Load zones from disk
    fn load_zones_from_disk(&self) -> Result<HashMap<String, GeofenceZone>> {
        let path = self.get_zones_file_path();
        
        if !path.exists() {
            return Ok(HashMap::new());
        }
        
        let content = std::fs::read_to_string(&path)
            .map_err(|e| GeofenceError::Config(format!("Failed to read zones file: {}", e)))?;
            
        let zones: Vec<GeofenceZone> = serde_json::from_str(&content)
            .map_err(|e| GeofenceError::Config(format!("Failed to parse zones file: {}", e)))?;
            
        let mut zone_map = HashMap::new();
        for zone in zones {
            zone_map.insert(zone.id.clone(), zone);
        }
        
        Ok(zone_map)
    }
    
    /// Save zones to disk
    fn save_zones_to_disk(&self) -> Result<()> {
        let path = self.get_zones_file_path();
        
        #[cfg(debug_assertions)]
        eprintln!("Saving zones to: {}", path.display());
        
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GeofenceError::Config(format!("Failed to create zones directory: {}", e)))?;
        }
        
        // Convert zones to vector for serialization
        let zones: Vec<&GeofenceZone> = self.zones.values().collect();
        
        let content = serde_json::to_string_pretty(&zones)
            .map_err(|e| GeofenceError::Config(format!("Failed to serialize zones: {}", e)))?;
            
        std::fs::write(&path, content)
            .map_err(|e| GeofenceError::Config(format!("Failed to write zones file: {}", e)))?;
        
        #[cfg(debug_assertions)]
        eprintln!("Successfully saved {} zones", zones.len());
            
        Ok(())
    }
    
    /// Get daemon state storage file path
    fn get_daemon_state_path(&self) -> PathBuf {
        let mut path = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("network-dmenu");
        path.push("daemon-state.json");
        path
    }
    
    /// Load daemon state from disk
    fn load_daemon_state(&self) -> Result<DaemonState> {
        let path = self.get_daemon_state_path();
        
        if !path.exists() {
            return Ok(DaemonState::default());
        }
        
        let content = std::fs::read_to_string(&path)
            .map_err(|e| GeofenceError::Config(format!("Failed to read daemon state file: {}", e)))?;
            
        let state: DaemonState = serde_json::from_str(&content)
            .map_err(|e| GeofenceError::Config(format!("Failed to parse daemon state file: {}", e)))?;
            
        Ok(state)
    }
    
    /// Save daemon state to disk
    fn save_daemon_state(&self) -> Result<()> {
        let path = self.get_daemon_state_path();
        
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GeofenceError::Config(format!("Failed to create daemon state directory: {}", e)))?;
        }
        
        let content = serde_json::to_string_pretty(&self.daemon_state)
            .map_err(|e| GeofenceError::Config(format!("Failed to serialize daemon state: {}", e)))?;
            
        std::fs::write(&path, content)
            .map_err(|e| GeofenceError::Config(format!("Failed to write daemon state file: {}", e)))?;
            
        Ok(())
    }
    
    /// Create a new geofence zone from current location
    pub async fn create_zone_from_current_location(
        &mut self,
        name: String,
        actions: ZoneActions,
    ) -> Result<GeofenceZone> {
        // Create location fingerprint
        let fingerprint = create_wifi_fingerprint(self.config.privacy_mode).await?;
        
        if fingerprint.confidence_score < 0.3 {
            return Err(GeofenceError::LocationDetection(
                "Insufficient WiFi networks for reliable zone creation".to_string()
            ));
        }
        
        let zone = GeofenceZone {
            id: generate_zone_id(),
            name,
            fingerprint,
            confidence_threshold: self.config.confidence_threshold,
            actions,
            created_at: Utc::now(),
            last_matched: None,
            match_count: 0,
        };
        
        // Add to zones
        self.zones.insert(zone.id.clone(), zone.clone());
        
        // Save to disk
        self.save_zones_to_disk()?;
        
        Ok(zone)
    }
    
    /// Detect current location and find matching zone
    pub async fn detect_location_change(&mut self) -> Result<Option<LocationChange>> {
        let current_fingerprint = create_wifi_fingerprint(self.config.privacy_mode).await?;
        
        if current_fingerprint.confidence_score < 0.3 {
            // Not enough data for reliable matching
            return Ok(None);
        }
        
        let matched_zone = self.find_best_matching_zone(&current_fingerprint)?;
        
        match matched_zone {
            Some(mut zone) if Some(&zone.id) != self.current_zone.as_ref() => {
                // Zone change detected
                let from_zone = self.current_zone
                    .as_ref()
                    .and_then(|id| self.zones.get(id))
                    .cloned();
                
                // Update zone statistics
                zone.last_matched = Some(Utc::now());
                zone.match_count += 1;
                self.zones.insert(zone.id.clone(), zone.clone());
                
                // Save updated zone statistics to disk
                let _ = self.save_zones_to_disk(); // Don't fail location detection on save error
                
                // Update current zone and daemon state
                self.current_zone = Some(zone.id.clone());
                self.daemon_state.current_zone = Some(zone.id.clone());
                self.daemon_state.total_zone_changes += 1;
                self.daemon_state.last_scan = Some(Utc::now());
                
                // Save daemon state to disk
                let _ = self.save_daemon_state(); // Don't fail location detection on save error
                
                Ok(Some(LocationChange {
                    from: from_zone,
                    to: zone.clone(),
                    confidence: current_fingerprint.confidence_score,
                    suggested_actions: zone.actions.clone(),
                }))
            },
            _ => {
                // No zone change, but update last scan time
                self.daemon_state.last_scan = Some(Utc::now());
                let _ = self.save_daemon_state(); // Don't fail on save error
                Ok(None)
            }
        }
    }
    
    /// Find the best matching zone for a location fingerprint
    fn find_best_matching_zone(&self, fingerprint: &LocationFingerprint) -> Result<Option<GeofenceZone>> {
        let mut best_match = None;
        let mut best_similarity = 0.0;
        
        for zone in self.zones.values() {
            let similarity = calculate_weighted_similarity(&zone.fingerprint, fingerprint);
            
            if similarity > best_similarity && similarity >= zone.confidence_threshold {
                best_similarity = similarity;
                best_match = Some(zone.clone());
            }
        }
        
        Ok(best_match)
    }
    
    /// Get all configured zones
    pub fn list_zones(&self) -> Vec<GeofenceZone> {
        self.zones.values().cloned().collect()
    }
    
    /// Get zone by ID
    pub fn get_zone(&self, zone_id: &str) -> Option<&GeofenceZone> {
        self.zones.get(zone_id)
    }
    
    /// Remove a zone
    pub fn remove_zone(&mut self, zone_id: &str) -> Result<()> {
        if self.zones.remove(zone_id).is_some() {
            // If this was the current zone, clear current zone
            if self.current_zone.as_deref() == Some(zone_id) {
                self.current_zone = None;
                self.daemon_state.current_zone = None;
                // Save daemon state
                let _ = self.save_daemon_state();
            }
            
            // Save zones to disk
            self.save_zones_to_disk()?;
            
            Ok(())
        } else {
            Err(GeofenceError::Config(format!("Zone '{}' not found", zone_id)))
        }
    }
    
    /// Update zone configuration
    pub fn update_zone(&mut self, zone: GeofenceZone) -> Result<()> {
        self.zones.insert(zone.id.clone(), zone);
        
        // Save to disk
        self.save_zones_to_disk()?;
        
        Ok(())
    }
    
    /// Get current active zone
    pub fn get_current_zone(&self) -> Option<&GeofenceZone> {
        self.current_zone
            .as_ref()
            .and_then(|id| self.zones.get(id))
    }
    
    /// Get total zone changes count
    pub fn get_total_zone_changes(&self) -> u32 {
        self.daemon_state.total_zone_changes
    }
    
    /// Get last scan timestamp  
    pub fn get_last_scan(&self) -> Option<DateTime<Utc>> {
        self.daemon_state.last_scan
    }
    
    /// Manually activate a zone (for testing or forced switching)
    pub fn activate_zone(&mut self, zone_id: &str) -> Result<LocationChange> {
        let zone = self.zones
            .get(zone_id)
            .ok_or_else(|| GeofenceError::Config(format!("Zone '{}' not found", zone_id)))?
            .clone();
            
        let from_zone = self.current_zone
            .as_ref()
            .and_then(|id| self.zones.get(id))
            .cloned();
            
        self.current_zone = Some(zone_id.to_string());
        
        Ok(LocationChange {
            from: from_zone,
            to: zone.clone(),
            confidence: 1.0, // Manual activation = full confidence
            suggested_actions: zone.actions.clone(),
        })
    }
    
    /// Improve zone fingerprint with new location data (ML enhancement)
    pub async fn improve_zone_fingerprint(&mut self, zone_id: &str) -> Result<()> {
        let current_fingerprint = create_wifi_fingerprint(self.config.privacy_mode).await?;
        
        if let Some(zone) = self.zones.get_mut(zone_id) {
            // Merge fingerprints - simple approach: add new networks
            zone.fingerprint.wifi_networks.extend(current_fingerprint.wifi_networks);
            
            // Update timestamp
            zone.fingerprint.timestamp = Utc::now();
            
            // Recalculate confidence
            zone.fingerprint.confidence_score = zone.fingerprint.wifi_networks.len() as f64 / 10.0;
            zone.fingerprint.confidence_score = zone.fingerprint.confidence_score.min(0.95);
            
            Ok(())
        } else {
            Err(GeofenceError::Config(format!("Zone '{}' not found", zone_id)))
        }
    }
    
    /// Export zones configuration
    pub fn export_zones(&self) -> GeofencingConfig {
        GeofencingConfig {
            zones: self.zones.values().cloned().collect(),
            ..self.config.clone()
        }
    }
}

/// Generate a unique zone ID
#[cfg(feature = "geofencing")]
fn generate_zone_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

#[cfg(not(feature = "geofencing"))]
fn generate_zone_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("zone_{}", timestamp)
}

/// Zone suggestion engine for ML-driven zone creation
pub struct ZoneSuggestionEngine {
    visit_history: HashMap<String, VisitPattern>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VisitPattern {
    fingerprint_hash: String,
    visit_count: u32,
    total_duration_minutes: u32,
    typical_actions: Vec<String>,
}

impl Default for ZoneSuggestionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ZoneSuggestionEngine {
    pub fn new() -> Self {
        Self {
            visit_history: HashMap::new(),
        }
    }
    
    /// Analyze if current location should become a new zone
    pub async fn analyze_location_for_zone_suggestion(
        &mut self,
        privacy_mode: PrivacyMode,
    ) -> Result<Option<super::ZoneSuggestion>> {
        let fingerprint = create_wifi_fingerprint(privacy_mode).await?;
        
        if fingerprint.confidence_score < 0.5 {
            return Ok(None); // Not enough data
        }
        
        // Create a simple hash of the fingerprint for tracking
        let fingerprint_hash = format!("{:?}", fingerprint.wifi_networks)
            .chars()
            .take(16)
            .collect::<String>();
            
        // Track visit pattern
        let pattern = self.visit_history
            .entry(fingerprint_hash.clone())
            .or_insert(VisitPattern {
                fingerprint_hash: fingerprint_hash.clone(),
                visit_count: 0,
                total_duration_minutes: 0,
                typical_actions: Vec::new(),
            });
            
        pattern.visit_count += 1;
        
        // Suggest zone creation if visited frequently
        if pattern.visit_count >= 3 && pattern.total_duration_minutes > 30 {
            Ok(Some(super::ZoneSuggestion {
                suggested_name: suggest_zone_name(pattern.visit_count),
                confidence: (pattern.visit_count as f64 / 10.0).min(0.9),
                suggested_actions: ZoneActions {
                    notifications: true,
                    ..Default::default()
                },
            }))
        } else {
            Ok(None)
        }
    }
}

/// Suggest a name for a new zone based on patterns
fn suggest_zone_name(visit_count: u32) -> String {
    match visit_count {
        3..=5 => "🏢 Frequent Location".to_string(),
        6..=10 => "⭐ Important Place".to_string(),
        _ => "🎯 Regular Spot".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn create_test_zone(id: &str, name: &str) -> GeofenceZone {
        GeofenceZone {
            id: id.to_string(),
            name: name.to_string(),
            fingerprint: LocationFingerprint::default(),
            confidence_threshold: 0.8,
            actions: ZoneActions::default(),
            created_at: Utc::now(),
            last_matched: None,
            match_count: 0,
        }
    }

    #[test]
    fn test_zone_manager_creation() {
        let config = GeofencingConfig {
            zones: vec![create_test_zone("home", "🏠 Home")],
            ..Default::default()
        };
        
        let manager = ZoneManager::new(config);
        assert_eq!(manager.zones.len(), 1);
        assert!(manager.get_zone("home").is_some());
    }

    #[test]
    fn test_zone_removal() {
        let config = GeofencingConfig {
            zones: vec![create_test_zone("home", "🏠 Home")],
            ..Default::default()
        };
        
        let mut manager = ZoneManager::new(config);
        assert!(manager.remove_zone("home").is_ok());
        assert!(manager.get_zone("home").is_none());
        assert!(manager.remove_zone("nonexistent").is_err());
    }

    #[test]
    fn test_manual_zone_activation() {
        let config = GeofencingConfig {
            zones: vec![create_test_zone("home", "🏠 Home")],
            ..Default::default()
        };
        
        let mut manager = ZoneManager::new(config);
        let change = manager.activate_zone("home").unwrap();
        
        assert_eq!(change.to.id, "home");
        assert_eq!(change.confidence, 1.0);
        assert_eq!(manager.current_zone, Some("home".to_string()));
    }

    #[test]
    fn test_zone_suggestion_engine() {
        let mut engine = ZoneSuggestionEngine::new();
        
        // Simulate multiple visits to same location
        let pattern = VisitPattern {
            fingerprint_hash: "test_hash".to_string(),
            visit_count: 5,
            total_duration_minutes: 60,
            typical_actions: Vec::new(),
        };
        
        engine.visit_history.insert("test_hash".to_string(), pattern);
        
        // Would suggest zone creation
        assert_eq!(engine.visit_history.len(), 1);
    }
}