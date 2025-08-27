//! NextDNS API client module
//!
//! This module provides a clean API client for interacting with NextDNS
//! using their HTTP API instead of the CLI tool.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// NextDNS API client
pub struct NextDnsApi {
    api_key: String,
    client: Client,
}

/// Represents a NextDNS profile/configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: Option<String>,
    #[serde(skip)]
    pub is_current: bool,
}

/// DNS-over-HTTPS endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DohEndpoint {
    pub url: String,
    pub profile_id: String,
}

/// Local state for tracking the current profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
struct ApiState {
    current_profile_id: Option<String>,
    profiles: Vec<Profile>,
    last_updated: u64,
}

impl NextDnsApi {
    /// Create a new NextDNS API client
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }

    /// List all available profiles from the API
    pub fn list_profiles(&self) -> Result<Vec<Profile>, Box<dyn Error>> {
        let response = self
            .client
            .get("https://api.nextdns.io/profiles")
            .header("X-Api-Key", &self.api_key)
            .send()?;

        if !response.status().is_success() {
            return Err(format!("API request failed: {}", response.status()).into());
        }

        let json: serde_json::Value = response.json()?;
        let mut profiles = Vec::new();

        if let serde_json::Value::Array(arr) = json {
            for item in arr {
                if let serde_json::Value::Object(obj) = item {
                    let id = obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let name = obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if !id.is_empty() {
                        profiles.push(Profile {
                            id,
                            name,
                            is_current: false,
                        });
                    }
                }
            }
        }

        // Update cache
        self.update_profile_cache(&profiles)?;

        Ok(profiles)
    }

    /// Get the current active profile
    pub fn get_current_profile(&self) -> Result<Option<Profile>, Box<dyn Error>> {
        let state = self.load_state()?;

        if let Some(current_id) = state.current_profile_id {
            // Try to find in cached profiles first
            if let Some(profile) = state.profiles.iter().find(|p| p.id == current_id) {
                return Ok(Some(profile.clone()));
            }

            // If not in cache, fetch fresh profiles
            let profiles = self.list_profiles()?;
            Ok(profiles.into_iter().find(|p| p.id == current_id))
        } else {
            Ok(None)
        }
    }

    /// Set the active profile using DNS-over-HTTPS
    pub fn set_profile(&self, profile_id: &str) -> Result<(), Box<dyn Error>> {
        // First verify the profile exists
        let profiles = self.list_profiles()?;
        let _profile = profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(|| format!("Profile {} not found", profile_id))?;

        // Update system DNS to use NextDNS DoH endpoint
        let doh_url = format!("https://dns.nextdns.io/{}", profile_id);

        // Update systemd-resolved or NetworkManager DNS settings
        self.apply_dns_settings(&doh_url)?;

        // Update state
        let mut state = self.load_state().unwrap_or_default();
        state.current_profile_id = Some(profile_id.to_string());
        self.save_state(&state)?;

        Ok(())
    }

    /// Disable NextDNS and revert to system DNS
    pub fn disable(&self) -> Result<(), Box<dyn Error>> {
        // Clear DNS-over-HTTPS settings
        self.clear_dns_settings()?;

        // Update state
        let mut state = self.load_state().unwrap_or_default();
        state.current_profile_id = None;
        self.save_state(&state)?;

        Ok(())
    }

    /// Apply DNS settings to the system
    fn apply_dns_settings(&self, doh_url: &str) -> Result<(), Box<dyn Error>> {
        // Check if systemd-resolved is available
        if self.is_systemd_resolved_available() {
            self.apply_systemd_resolved_settings(doh_url)?;
        } else {
            // Fall back to NetworkManager or direct resolv.conf manipulation
            self.apply_network_manager_settings(doh_url)?;
        }
        Ok(())
    }

    /// Clear custom DNS settings
    fn clear_dns_settings(&self) -> Result<(), Box<dyn Error>> {
        if self.is_systemd_resolved_available() {
            self.clear_systemd_resolved_settings()?;
        } else {
            self.clear_network_manager_settings()?;
        }
        Ok(())
    }

    /// Check if systemd-resolved is available
    fn is_systemd_resolved_available(&self) -> bool {
        std::process::Command::new("systemctl")
            .args(["is-active", "systemd-resolved"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Apply DNS settings using systemd-resolved
    fn apply_systemd_resolved_settings(&self, doh_url: &str) -> Result<(), Box<dyn Error>> {
        // Use resolvectl to set DNS-over-HTTPS
        std::process::Command::new("sudo")
            .args([
                "resolvectl",
                "dns",
                "--interface=*",
                &format!("dns-over-tls:{}", doh_url),
            ])
            .output()?;

        Ok(())
    }

    /// Clear systemd-resolved settings
    fn clear_systemd_resolved_settings(&self) -> Result<(), Box<dyn Error>> {
        std::process::Command::new("sudo")
            .args(["resolvectl", "revert"])
            .output()?;

        Ok(())
    }

    /// Apply DNS settings using NetworkManager
    fn apply_network_manager_settings(&self, _doh_url: &str) -> Result<(), Box<dyn Error>> {
        // Get active connection
        let output = std::process::Command::new("nmcli")
            .args(["-t", "-f", "NAME,UUID", "connection", "show", "--active"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = output_str.lines().next() {
            if let Some(uuid) = line.split(':').nth(1) {
                // Set DNS for the connection
                std::process::Command::new("sudo")
                    .args([
                        "nmcli",
                        "connection",
                        "modify",
                        uuid,
                        "ipv4.dns",
                        "45.90.28.0,45.90.30.0", // NextDNS anycast IPs
                    ])
                    .output()?;

                // Restart connection
                std::process::Command::new("sudo")
                    .args(["nmcli", "connection", "up", uuid])
                    .output()?;
            }
        }

        Ok(())
    }

    /// Clear NetworkManager DNS settings
    fn clear_network_manager_settings(&self) -> Result<(), Box<dyn Error>> {
        let output = std::process::Command::new("nmcli")
            .args(["-t", "-f", "NAME,UUID", "connection", "show", "--active"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = output_str.lines().next() {
            if let Some(uuid) = line.split(':').nth(1) {
                // Clear custom DNS
                std::process::Command::new("sudo")
                    .args(["nmcli", "connection", "modify", uuid, "ipv4.dns", ""])
                    .output()?;

                // Restart connection
                std::process::Command::new("sudo")
                    .args(["nmcli", "connection", "up", uuid])
                    .output()?;
            }
        }

        Ok(())
    }

    /// Update the profile cache
    fn update_profile_cache(&self, profiles: &[Profile]) -> Result<(), Box<dyn Error>> {
        let mut state = self.load_state().unwrap_or_default();
        state.profiles = profiles.to_vec();
        state.last_updated = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        self.save_state(&state)?;
        Ok(())
    }

    /// Get the state file path
    fn get_state_file_path(&self) -> Result<PathBuf, Box<dyn Error>> {
        let cache_dir = dirs::cache_dir().ok_or("Could not determine cache directory")?;
        let state_dir = cache_dir.join("network-dmenu");
        fs::create_dir_all(&state_dir)?;
        Ok(state_dir.join("nextdns_api_state.json"))
    }

    /// Load state from disk
    fn load_state(&self) -> Result<ApiState, Box<dyn Error>> {
        let path = self.get_state_file_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(ApiState::default())
        }
    }

    /// Save state to disk
    fn save_state(&self, state: &ApiState) -> Result<(), Box<dyn Error>> {
        let path = self.get_state_file_path()?;
        let content = serde_json::to_string_pretty(state)?;
        fs::write(path, content)?;
        Ok(())
    }
}

/// Check if profiles are stale (older than 1 hour)
pub fn should_refresh_profiles(last_updated: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    (now - last_updated) > 3600
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_serialization() {
        let profile = Profile {
            id: "test123".to_string(),
            name: Some("Test Profile".to_string()),
            is_current: false,
        };

        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("test123"));
        assert!(json.contains("Test Profile"));
        assert!(!json.contains("is_current")); // Should be skipped
    }

    #[test]
    fn test_should_refresh_profiles() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Fresh profiles
        assert!(!should_refresh_profiles(now));

        // Hour old profiles
        assert!(!should_refresh_profiles(now - 3599));

        // Stale profiles
        assert!(should_refresh_profiles(now - 3601));
    }
}
