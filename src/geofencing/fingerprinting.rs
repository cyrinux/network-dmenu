//! Location fingerprinting using WiFi networks and Bluetooth beacons
//!
//! Privacy-first implementation that hashes sensitive identifiers
//! and focuses on local pattern matching.

use super::{LocationFingerprint, NetworkSignature, PrivacyMode, Result};
use crate::command::{CommandRunner, RealCommandRunner};
use chrono::Utc;
use log::debug;
use std::collections::BTreeSet;

#[cfg(feature = "geofencing")]
use sha2::{Digest, Sha256};

/// Create location fingerprint - simplified to WiFi only for reliability
pub async fn create_wifi_fingerprint(privacy_mode: PrivacyMode) -> Result<LocationFingerprint> {
    let mut fingerprint = LocationFingerprint::default();

    // Only scan WiFi networks - simplified for maximum reliability
    let wifi_signatures = scan_wifi_signatures(privacy_mode).await?;
    fingerprint.wifi_networks = wifi_signatures;

    // Simplified confidence calculation based only on WiFi signal count
    fingerprint.confidence_score = calculate_wifi_confidence(&fingerprint.wifi_networks);
    fingerprint.timestamp = Utc::now();

    debug!(
        "🎯 Location fingerprint created: {} WiFi networks, confidence: {:.2}",
        fingerprint.wifi_networks.len(),
        fingerprint.confidence_score
    );

    Ok(fingerprint)
}

/// Find the end position of a BSSID in nmcli output format
/// BSSID format: XX\:XX\:XX\:XX\:XX\:XX or XX:XX:XX:XX:XX:XX (depending on nmcli version)
fn find_bssid_end(text: &str) -> Option<usize> {
    // Try both escaped and unescaped formats for compatibility
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut segments = 0;

    while pos < bytes.len() {
        // Check for hex segment (2 hex digits)
        if pos + 1 < bytes.len()
            && bytes[pos].is_ascii_hexdigit()
            && bytes[pos + 1].is_ascii_hexdigit()
        {
            segments += 1;
            pos += 2;

            // Check for separator
            if pos < bytes.len() {
                // Handle escaped colon (\:) or regular colon
                if pos + 1 < bytes.len() && bytes[pos] == b'\\' && bytes[pos + 1] == b':' {
                    pos += 2;
                } else if bytes[pos] == b':' {
                    pos += 1;
                } else if segments == 6 {
                    // No separator after 6th segment = end of BSSID
                    return Some(pos);
                } else {
                    break; // Invalid format
                }
            }

            // Valid MAC address has 6 segments
            if segments == 6 {
                return Some(pos);
            }
        } else {
            break;
        }
    }

    None
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
            debug!("📡 nmcli WiFi scan found {} lines", stdout.lines().count());
            for line in stdout.lines() {
                // Parse nmcli output format: SSID:BSSID:SIGNAL:FREQ
                // Note: BSSID contains escaped colons (\:) so we need careful parsing
                if let Some(ssid_end) = line.find(':') {
                    let ssid = line[..ssid_end].trim();
                    let rest = &line[ssid_end + 1..];

                    // Find BSSID by looking for the pattern with escaped colons
                    // BSSID format: XX\:XX\:XX\:XX\:XX\:XX (17 chars with escapes)
                    if let Some(bssid_end) = find_bssid_end(rest) {
                        let bssid_raw = rest[..bssid_end].trim();
                        let remaining = &rest[bssid_end + 1..];

                        // Parse signal and frequency from remaining parts
                        let parts: Vec<&str> = remaining.split(':').collect();
                        if parts.len() >= 2 {
                            let signal_str = parts[0].trim();
                            let freq_str = parts[1].trim();

                            // Clean up BSSID by removing escape characters
                            let bssid = bssid_raw.replace("\\:", ":");

                            if !ssid.is_empty() {
                                // Parse signal strength (default to -50 if parsing fails)
                                let signal_strength = match signal_str.parse::<i8>() {
                                    Ok(s) => s,
                                    Err(_) => {
                                        debug!("⚠️  Failed to parse signal strength '{}', using default", signal_str);
                                        -50
                                    }
                                };

                                // Parse frequency - remove " MHz" suffix if present
                                let frequency = match freq_str.replace(" MHz", "").trim().parse::<u32>() {
                                    Ok(f) => f,
                                    Err(_) => {
                                        debug!("⚠️  Failed to parse frequency '{}', using default 2412", freq_str);
                                        2412
                                    }
                                };

                                if let Some(signature) = create_network_signature(
                                    ssid,
                                    &bssid,
                                    signal_strength,
                                    frequency,
                                    privacy_mode,
                                ) {
                                    debug!(
                                        "📡 Parsed WiFi network: '{}' signal={} freq={}",
                                        ssid, signal_strength, frequency
                                    );
                                    signatures.insert(signature);
                                }
                            }
                        } else {
                            debug!("⚠️  Failed to parse line (insufficient fields): {}", line);
                        }
                    } else {
                        debug!("⚠️  Failed to find BSSID end in: {}", rest);
                    }
                }
            }
            debug!(
                "📡 nmcli parsing complete: {} WiFi networks found",
                signatures.len()
            );
            return Ok(signatures);
        }
    }

    // Fallback to IWD with iwctl
    let wifi_interface = crate::utils::get_wifi_interface(None);
    if let Ok(output) =
        command_runner.run_command("iwctl", &["station", &wifi_interface, "get-networks"])
    {
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

    let bssid_prefix = match privacy_mode {
        PrivacyMode::High => {
            // High privacy: Hash the full BSSID for fingerprinting
            if !bssid.is_empty() && bssid.len() >= 8 {
                hash_string(bssid)
            } else {
                debug!(
                    "BSSID too short or empty: '{}', length: {}",
                    bssid,
                    bssid.len()
                );
                "unknown".to_string()
            }
        }
        PrivacyMode::Medium => {
            // Medium privacy: Use manufacturer prefix (first 3 octets)
            if bssid.len() >= 8 {
                bssid[..8].to_string() // XX:XX:XX format
            } else {
                debug!(
                    "BSSID too short for manufacturer prefix: '{}', length: {}",
                    bssid,
                    bssid.len()
                );
                "unknown".to_string()
            }
        }
        PrivacyMode::Low => {
            // Low privacy: Store full BSSID
            if !bssid.is_empty() {
                bssid.to_string()
            } else {
                debug!("Empty BSSID in low privacy mode");
                "unknown".to_string()
            }
        }
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

// Bluetooth and IP geolocation functions removed for simplification

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

/// Calculate similarity between two location fingerprints - simplified to WiFi only
pub fn calculate_fingerprint_similarity(
    fingerprint1: &LocationFingerprint,
    fingerprint2: &LocationFingerprint,
) -> f64 {
    // Simplified: only WiFi similarity for maximum reliability
    calculate_wifi_similarity(&fingerprint1.wifi_networks, &fingerprint2.wifi_networks)
}

/// Calculate enhanced WiFi similarity with stability improvements
/// Uses weighted matching that prioritizes core networks and handles fluctuations better
fn calculate_wifi_similarity(
    networks1: &BTreeSet<NetworkSignature>,
    networks2: &BTreeSet<NetworkSignature>,
) -> f64 {
    if networks1.is_empty() && networks2.is_empty() {
        return 1.0;
    }
    if networks1.is_empty() || networks2.is_empty() {
        return 0.0;
    }

    // Create sets of network identifiers (SSID hash + BSSID prefix) ignoring signal strength and frequency
    let identifiers1: std::collections::HashSet<_> = networks1
        .iter()
        .map(|net| (&net.ssid_hash, &net.bssid_prefix))
        .collect();

    let identifiers2: std::collections::HashSet<_> = networks2
        .iter()
        .map(|net| (&net.ssid_hash, &net.bssid_prefix))
        .collect();

    let intersection_size = identifiers1.intersection(&identifiers2).count() as f64;
    let union_size = identifiers1.union(&identifiers2).count() as f64;

    // Standard Jaccard similarity
    let jaccard_similarity = if union_size == 0.0 {
        0.0
    } else {
        intersection_size / union_size
    };

    // Boost similarity if we have strong core network matches
    // This makes detection more stable by prioritizing reliable networks
    let strong_networks1: std::collections::HashSet<_> = networks1
        .iter()
        .filter(|net| net.signal_strength > -65) // Strong signal threshold
        .map(|net| (&net.ssid_hash, &net.bssid_prefix))
        .collect();

    let strong_networks2: std::collections::HashSet<_> = networks2
        .iter()
        .filter(|net| net.signal_strength > -65) // Strong signal threshold
        .map(|net| (&net.ssid_hash, &net.bssid_prefix))
        .collect();

    let strong_intersection = strong_networks1.intersection(&strong_networks2).count() as f64;

    // Bonus for strong network matches (up to 0.3 bonus)
    let strong_bonus = if !strong_networks1.is_empty() && !strong_networks2.is_empty() {
        (strong_intersection / strong_networks1.len().max(strong_networks2.len()) as f64) * 0.3
    } else {
        0.0
    };

    // Stability boost: if we have at least 2 matching networks, give significant bonus
    let stability_bonus = if intersection_size >= 2.0 {
        0.2 // 20% bonus for having multiple matching networks
    } else {
        0.0
    };

    // Combine all factors, capped at 1.0
    (jaccard_similarity + strong_bonus + stability_bonus).min(1.0)
}

// Bluetooth and IP similarity functions removed for simplification

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
