//! Location fingerprinting using WiFi networks and Bluetooth beacons
//!
//! Privacy-first implementation that hashes sensitive identifiers
//! and focuses on local pattern matching.

use super::{CoarseLocation, LocationFingerprint, NetworkSignature, PrivacyMode, Result};
use crate::command::{CommandRunner, RealCommandRunner};
use chrono::Utc;
use std::collections::BTreeSet;

#[cfg(feature = "geofencing")]
use sha2::{Digest, Sha256};

/// Create location fingerprint based on privacy mode
pub async fn create_wifi_fingerprint(privacy_mode: PrivacyMode) -> Result<LocationFingerprint> {
    let mut fingerprint = LocationFingerprint::default();

    // Always scan WiFi networks (all privacy modes)
    let wifi_signatures = scan_wifi_signatures(privacy_mode).await?;
    fingerprint.wifi_networks = wifi_signatures;

    // Add Bluetooth beacons for Medium and Low privacy modes
    match privacy_mode {
        PrivacyMode::High => {
            // High privacy: WiFi only, everything hashed, local processing only
        }
        PrivacyMode::Medium => {
            // Medium privacy: WiFi + Bluetooth beacons, some caching allowed
            fingerprint.bluetooth_devices = scan_bluetooth_beacons(privacy_mode).await?;
        }
        PrivacyMode::Low => {
            // Low privacy: All methods including IP geolocation
            fingerprint.bluetooth_devices = scan_bluetooth_beacons(privacy_mode).await?;
            fingerprint.ip_location = get_ip_geolocation().await.ok();
        }
    }

    // Calculate confidence based on available signals
    fingerprint.confidence_score = calculate_confidence(&fingerprint);
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
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "SSID,BSSID,SIGNAL,FREQ",
            "device",
            "wifi",
        ],
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
                            ssid,
                            bssid,
                            signal_strength,
                            frequency,
                            privacy_mode,
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
            for line in stdout.lines().skip(4) {
                // Skip header lines
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("---") {
                    // Extract network name (first column)
                    if let Some(ssid) = trimmed.split_whitespace().next() {
                        if let Some(signature) = create_network_signature(
                            ssid,
                            "",
                            -50,
                            2412,
                            privacy_mode, // Default values for IWD
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
        _ => hash_string(ssid),               // Hash for privacy
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

/// Scan for Bluetooth beacons (Medium and Low privacy modes)
async fn scan_bluetooth_beacons(privacy_mode: PrivacyMode) -> Result<BTreeSet<String>> {
    let command_runner = RealCommandRunner;
    let mut beacons = BTreeSet::new();

    // Use bluetoothctl to scan for nearby devices
    if let Ok(output) = command_runner.run_command("bluetoothctl", &["scan", "on"]) {
        if !output.status.success() {
            // If Bluetooth is not available or disabled, return empty set
            return Ok(beacons);
        }
    }

    // Give scan a moment to populate
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

    // Get list of discovered devices
    if let Ok(output) = command_runner.run_command("bluetoothctl", &["devices"]) {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(mac_start) = line.find(' ') {
                    let rest = &line[mac_start + 1..];
                    if let Some(mac_end) = rest.find(' ') {
                        let mac_address = &rest[..mac_end];

                        // Apply privacy settings
                        let beacon_id = match privacy_mode {
                            PrivacyMode::Low => {
                                // Low privacy: store MAC address directly (first 6 chars for vendor)
                                if mac_address.len() >= 8 {
                                    mac_address[..8].to_string()
                                } else {
                                    mac_address.to_string()
                                }
                            }
                            _ => {
                                // Medium privacy: hash MAC address
                                hash_string(mac_address)
                            }
                        };

                        beacons.insert(beacon_id);
                    }
                }
            }
        }
    }

    // Stop scanning to be polite
    let _ = command_runner.run_command("bluetoothctl", &["scan", "off"]);

    Ok(beacons)
}

/// Get coarse location from IP geolocation (Low privacy mode only)
async fn get_ip_geolocation() -> Result<CoarseLocation> {
    // Use a free IP geolocation service
    let client = reqwest::Client::new();
    let response = client
        .get("http://ip-api.com/json/?fields=country,regionName,city")
        .timeout(tokio::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            super::GeofenceError::LocationDetection(format!("IP geolocation failed: {}", e))
        })?;

    if response.status().is_success() {
        let json: serde_json::Value = response.json().await.map_err(|e| {
            super::GeofenceError::LocationDetection(format!(
                "Failed to parse IP geolocation response: {}",
                e
            ))
        })?;

        Ok(CoarseLocation {
            country: json["country"].as_str().unwrap_or("Unknown").to_string(),
            region: json["regionName"].as_str().unwrap_or("Unknown").to_string(),
            city: json["city"].as_str().unwrap_or("Unknown").to_string(),
        })
    } else {
        Err(super::GeofenceError::LocationDetection(format!(
            "IP geolocation service returned: {}",
            response.status()
        )))
    }
}

/// Calculate confidence score based on all available signals
fn calculate_confidence(fingerprint: &LocationFingerprint) -> f64 {
    let wifi_confidence = calculate_wifi_confidence(&fingerprint.wifi_networks);
    let bluetooth_bonus = if fingerprint.bluetooth_devices.is_empty() {
        0.0
    } else {
        0.1
    };
    let ip_bonus = if fingerprint.ip_location.is_some() {
        0.05
    } else {
        0.0
    };

    // Combine confidence sources (max 1.0)
    (wifi_confidence + bluetooth_bonus + ip_bonus).min(1.0)
}

/// Calculate confidence score based on WiFi network visibility
fn calculate_wifi_confidence(networks: &BTreeSet<NetworkSignature>) -> f64 {
    match networks.len() {
        0 => 0.0,
        1..=2 => 0.3,  // Very low confidence with few networks
        3..=5 => 0.6,  // Medium confidence
        6..=10 => 0.8, // Good confidence
        _ => 0.9,      // High confidence with many unique networks
    }
}

/// Calculate similarity between two location fingerprints
pub fn calculate_fingerprint_similarity(
    fingerprint1: &LocationFingerprint,
    fingerprint2: &LocationFingerprint,
) -> f64 {
    let mut total_similarity = 0.0;
    let mut total_weight = 0.0;

    // WiFi similarity (primary signal, weight: 0.7)
    if !fingerprint1.wifi_networks.is_empty() || !fingerprint2.wifi_networks.is_empty() {
        let wifi_similarity =
            calculate_wifi_similarity(&fingerprint1.wifi_networks, &fingerprint2.wifi_networks);
        total_similarity += wifi_similarity * 0.7;
        total_weight += 0.7;
    }

    // Bluetooth similarity (secondary signal, weight: 0.2)
    if !fingerprint1.bluetooth_devices.is_empty() || !fingerprint2.bluetooth_devices.is_empty() {
        let bt_similarity = calculate_bluetooth_similarity(
            &fingerprint1.bluetooth_devices,
            &fingerprint2.bluetooth_devices,
        );
        total_similarity += bt_similarity * 0.2;
        total_weight += 0.2;
    }

    // IP location similarity (coarse signal, weight: 0.1)
    if fingerprint1.ip_location.is_some() || fingerprint2.ip_location.is_some() {
        let ip_similarity =
            calculate_ip_similarity(&fingerprint1.ip_location, &fingerprint2.ip_location);
        total_similarity += ip_similarity * 0.1;
        total_weight += 0.1;
    }

    if total_weight == 0.0 {
        0.0
    } else {
        total_similarity / total_weight
    }
}

/// Calculate Jaccard similarity for WiFi networks
fn calculate_wifi_similarity(
    networks1: &BTreeSet<NetworkSignature>,
    networks2: &BTreeSet<NetworkSignature>,
) -> f64 {
    let intersection_size = networks1.intersection(networks2).count() as f64;
    let union_size = networks1.union(networks2).count() as f64;

    if union_size == 0.0 {
        0.0
    } else {
        intersection_size / union_size
    }
}

/// Calculate Jaccard similarity for Bluetooth beacons
fn calculate_bluetooth_similarity(beacons1: &BTreeSet<String>, beacons2: &BTreeSet<String>) -> f64 {
    let intersection_size = beacons1.intersection(beacons2).count() as f64;
    let union_size = beacons1.union(beacons2).count() as f64;

    if union_size == 0.0 {
        0.0
    } else {
        intersection_size / union_size
    }
}

/// Calculate similarity for IP geolocation (coarse matching)
fn calculate_ip_similarity(
    location1: &Option<CoarseLocation>,
    location2: &Option<CoarseLocation>,
) -> f64 {
    match (location1, location2) {
        (Some(loc1), Some(loc2)) => {
            let mut matches = 0.0;
            let mut total = 0.0;

            // City match (most specific)
            total += 1.0;
            if loc1.city == loc2.city {
                matches += 1.0;
            }

            // Region match
            total += 1.0;
            if loc1.region == loc2.region {
                matches += 0.7;
            }

            // Country match (least specific)
            total += 1.0;
            if loc1.country == loc2.country {
                matches += 0.3;
            }

            matches / total
        }
        (None, None) => 0.0, // Both missing
        _ => 0.0,            // One missing
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

    let strong_intersection = strong_networks1.intersection(&strong_networks2).count() as f64;

    // Up to 0.5 bonus for matching strong networks, with increased weight
    // Ensures that matching strong networks provides a meaningful bonus
    (strong_intersection / 2.0).min(0.5)
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

        fingerprint1
            .wifi_networks
            .insert(create_test_signature("hash1", -50));
        fingerprint1
            .wifi_networks
            .insert(create_test_signature("hash2", -60));

        fingerprint2
            .wifi_networks
            .insert(create_test_signature("hash1", -50));
        fingerprint2
            .wifi_networks
            .insert(create_test_signature("hash3", -70));

        let similarity = calculate_fingerprint_similarity(&fingerprint1, &fingerprint2);
        assert_eq!(similarity, 1.0 / 3.0); // 1 intersection, 3 in union
    }

    #[test]
    fn test_weighted_similarity_with_strong_signals() {
        let mut fingerprint1 = LocationFingerprint::default();
        let mut fingerprint2 = LocationFingerprint::default();

        // Both have strong signal network
        fingerprint1
            .wifi_networks
            .insert(create_test_signature("hash1", -50)); // Strong
        fingerprint2
            .wifi_networks
            .insert(create_test_signature("hash1", -50)); // Strong

        // Add weak networks to dilute base similarity
        fingerprint1
            .wifi_networks
            .insert(create_test_signature("hash2", -80)); // Weak
        fingerprint2
            .wifi_networks
            .insert(create_test_signature("hash3", -85)); // Weak

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
