//! Location fingerprinting using WiFi networks and Bluetooth beacons
//! 
//! Privacy-first implementation that hashes sensitive identifiers
//! and focuses on local pattern matching.

use super::{LocationFingerprint, NetworkSignature, PrivacyMode, Result};
use crate::command::{CommandRunner, RealCommandRunner};
use std::collections::BTreeSet;
use chrono::Utc;

#[cfg(feature = "geofencing")]
use sha2::{Digest, Sha256};

/// Create location fingerprint based on current WiFi environment
pub async fn create_wifi_fingerprint(privacy_mode: PrivacyMode) -> Result<LocationFingerprint> {
    let mut fingerprint = LocationFingerprint::default();
    
    // Scan WiFi networks
    let wifi_signatures = scan_wifi_signatures(privacy_mode).await?;
    fingerprint.wifi_networks = wifi_signatures;
    
    // Calculate confidence based on number of unique networks
    fingerprint.confidence_score = calculate_wifi_confidence(&fingerprint.wifi_networks);
    fingerprint.timestamp = Utc::now();
    
    Ok(fingerprint)
}

/// Scan for WiFi networks and create privacy-preserving signatures
async fn scan_wifi_signatures(privacy_mode: PrivacyMode) -> Result<BTreeSet<NetworkSignature>> {
    let command_runner = RealCommandRunner;
    let mut signatures = BTreeSet::new();
    
    // Try NetworkManager first - direct nmcli command for more detailed info
    if let Ok(output) = command_runner.run_command(
        "nmcli",
        &["--colors", "no", "-t", "-f", "SSID,BSSID,SIGNAL,FREQ", "device", "wifi"]
    ) {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 4 {
                    let ssid = parts[0].trim();
                    let bssid = parts[1].trim();
                    let signal_str = parts[2].trim();
                    let freq_str = parts[3].trim();
                    
                    if !ssid.is_empty() {
                        // Parse signal strength (default to -50 if parsing fails)
                        let signal_strength = signal_str.parse::<i8>().unwrap_or(-50);
                        
                        // Parse frequency (default to 2412 MHz if parsing fails)
                        let frequency = freq_str.parse::<u32>().unwrap_or(2412);
                        
                        if let Some(signature) = create_network_signature(
                            ssid, bssid, signal_strength, frequency, privacy_mode
                        ) {
                            signatures.insert(signature);
                        }
                    }
                }
            }
            return Ok(signatures);
        }
    }
    
    // Fallback to IWD with iwctl
    if let Ok(output) = command_runner.run_command("iwctl", &["station", "wlan0", "get-networks"]) {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse iwctl output - simplified version without signal strength
            for line in stdout.lines().skip(4) { // Skip header lines
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("---") {
                    // Extract network name (first column)
                    if let Some(ssid) = trimmed.split_whitespace().next() {
                        if let Some(signature) = create_network_signature(
                            ssid, "", -50, 2412, privacy_mode // Default values for IWD
                        ) {
                            signatures.insert(signature);
                        }
                    }
                }
            }
        }
    }
    
    Ok(signatures)
}

/// Create a privacy-preserving network signature
fn create_network_signature(
    ssid: &str,
    bssid: &str, 
    signal_strength: i8,
    frequency: u32,
    privacy_mode: PrivacyMode,
) -> Option<NetworkSignature> {
    if ssid.is_empty() {
        return None;
    }
    
    let ssid_hash = match privacy_mode {
        PrivacyMode::Low => ssid.to_string(), // Store SSID directly
        _ => hash_string(ssid), // Hash for privacy
    };
    
    let bssid_prefix = if bssid.len() >= 8 {
        bssid[..8].to_string() // Just manufacturer part
    } else {
        "unknown".to_string()
    };
    
    Some(NetworkSignature {
        ssid_hash,
        bssid_prefix,
        signal_strength,
        frequency,
    })
}

/// Hash a string using SHA-256 for privacy
#[cfg(feature = "geofencing")]
fn hash_string(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string() // First 16 chars for space efficiency
}

#[cfg(not(feature = "geofencing"))]
fn hash_string(input: &str) -> String {
    // Fallback when geofencing feature is disabled
    format!("hash_{}", input.len())
}

/// Calculate confidence score based on WiFi network visibility
fn calculate_wifi_confidence(networks: &BTreeSet<NetworkSignature>) -> f64 {
    match networks.len() {
        0 => 0.0,
        1..=2 => 0.3, // Very low confidence with few networks
        3..=5 => 0.6, // Medium confidence
        6..=10 => 0.8, // Good confidence  
        _ => 0.9, // High confidence with many unique networks
    }
}

/// Calculate similarity between two location fingerprints
pub fn calculate_fingerprint_similarity(
    fingerprint1: &LocationFingerprint,
    fingerprint2: &LocationFingerprint,
) -> f64 {
    if fingerprint1.wifi_networks.is_empty() && fingerprint2.wifi_networks.is_empty() {
        return 0.0;
    }
    
    // Calculate Jaccard similarity for WiFi networks
    let intersection_size = fingerprint1
        .wifi_networks
        .intersection(&fingerprint2.wifi_networks)
        .count() as f64;
        
    let union_size = fingerprint1
        .wifi_networks
        .union(&fingerprint2.wifi_networks)
        .count() as f64;
    
    if union_size == 0.0 {
        0.0
    } else {
        intersection_size / union_size
    }
}

/// Enhanced fingerprint matching with signal strength consideration
pub fn calculate_weighted_similarity(
    fingerprint1: &LocationFingerprint, 
    fingerprint2: &LocationFingerprint,
) -> f64 {
    let base_similarity = calculate_fingerprint_similarity(fingerprint1, fingerprint2);
    
    // Bonus for strong signal networks (more stable for location detection)
    let strong_signal_bonus = calculate_strong_signal_bonus(fingerprint1, fingerprint2);
    
    (base_similarity + strong_signal_bonus).min(1.0)
}

/// Calculate bonus for networks with strong, consistent signal strength
fn calculate_strong_signal_bonus(
    fingerprint1: &LocationFingerprint,
    fingerprint2: &LocationFingerprint, 
) -> f64 {
    let strong_threshold = -60i8; // Networks stronger than -60 dBm
    
    let strong_networks1: BTreeSet<_> = fingerprint1
        .wifi_networks
        .iter()
        .filter(|net| net.signal_strength > strong_threshold)
        .collect();
        
    let strong_networks2: BTreeSet<_> = fingerprint2
        .wifi_networks
        .iter()
        .filter(|net| net.signal_strength > strong_threshold)
        .collect();
    
    let strong_intersection = strong_networks1
        .intersection(&strong_networks2)
        .count() as f64;
    
    // Up to 0.2 bonus for matching strong networks
    (strong_intersection / 10.0).min(0.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_signature(ssid_hash: &str, signal: i8) -> NetworkSignature {
        NetworkSignature {
            ssid_hash: ssid_hash.to_string(),
            bssid_prefix: "aa:bb:cc".to_string(),
            signal_strength: signal,
            frequency: 2412,
        }
    }

    #[test]
    fn test_wifi_confidence_calculation() {
        let mut networks = BTreeSet::new();
        assert_eq!(calculate_wifi_confidence(&networks), 0.0);
        
        networks.insert(create_test_signature("hash1", -50));
        assert_eq!(calculate_wifi_confidence(&networks), 0.3);
        
        for i in 2..=6 {
            networks.insert(create_test_signature(&format!("hash{}", i), -50));
        }
        assert_eq!(calculate_wifi_confidence(&networks), 0.8);
    }

    #[test]
    fn test_fingerprint_similarity() {
        let mut fingerprint1 = LocationFingerprint::default();
        let mut fingerprint2 = LocationFingerprint::default();
        
        fingerprint1.wifi_networks.insert(create_test_signature("hash1", -50));
        fingerprint1.wifi_networks.insert(create_test_signature("hash2", -60));
        
        fingerprint2.wifi_networks.insert(create_test_signature("hash1", -50));
        fingerprint2.wifi_networks.insert(create_test_signature("hash3", -70));
        
        let similarity = calculate_fingerprint_similarity(&fingerprint1, &fingerprint2);
        assert_eq!(similarity, 1.0 / 3.0); // 1 intersection, 3 in union
    }

    #[test]
    fn test_weighted_similarity_with_strong_signals() {
        let mut fingerprint1 = LocationFingerprint::default();
        let mut fingerprint2 = LocationFingerprint::default();
        
        // Both have strong signal network
        fingerprint1.wifi_networks.insert(create_test_signature("hash1", -50)); // Strong
        fingerprint2.wifi_networks.insert(create_test_signature("hash1", -50)); // Strong
        
        let weighted = calculate_weighted_similarity(&fingerprint1, &fingerprint2);
        let base = calculate_fingerprint_similarity(&fingerprint1, &fingerprint2);
        
        assert!(weighted > base); // Should have bonus for strong signal match
    }

    #[cfg(feature = "geofencing")]
    #[test]
    fn test_string_hashing() {
        let hash1 = hash_string("MyWiFiNetwork");
        let hash2 = hash_string("MyWiFiNetwork");
        let hash3 = hash_string("DifferentNetwork");
        
        assert_eq!(hash1, hash2); // Same input = same hash
        assert_ne!(hash1, hash3); // Different input = different hash
        assert_eq!(hash1.len(), 16); // Truncated to 16 chars
    }
}