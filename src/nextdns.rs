//! NextDNS profile management module using HTTP API
//!
//! This module provides functionality to manage NextDNS profiles via their API,
//! without requiring the NextDNS CLI to be installed.
//! NextDNS module for interacting with the NextDNS API and profiles
//! Allows fetching, switching, and toggling between profiles using the NextDNS API.

use crate::command::CommandRunner;
use crate::constants::ICON_CHECK;
use crate::privilege::wrap_privileged_command;
use log::{debug, error, warn};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

const NEXTDNS_API: &str = "https://api.nextdns.io/";

/// Represents a NextDNS profile from the API
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextDnsProfile {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub is_current: bool,
}

/// NextDNS action types
#[derive(Debug, Clone, PartialEq)]
pub enum NextDnsAction {
    /// Switch to a specific profile
    SetProfile { profile: NextDnsProfile },
    /// Toggle between two profiles
    ToggleProfiles {
        profile_a: NextDnsProfile,
        profile_b: NextDnsProfile,
    },
    /// Disable NextDNS (revert to regular DNS)
    Disable,
    /// Refresh profiles from API
    RefreshProfiles,
}

impl fmt::Display for NextDnsAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NextDnsAction::SetProfile { profile } => {
                if profile.is_current {
                    write!(
                        f,
                        "{} NextDNS: {} (current)",
                        ICON_CHECK,
                        profile.name.as_deref().unwrap_or(&profile.id)
                    )
                } else {
                    write!(
                        f,
                        "🔄 NextDNS: Switch to {}",
                        profile.name.as_deref().unwrap_or(&profile.id)
                    )
                }
            }
            NextDnsAction::ToggleProfiles {
                profile_a,
                profile_b,
            } => {
                let name_a = profile_a.name.as_deref().unwrap_or(&profile_a.id);
                let name_b = profile_b.name.as_deref().unwrap_or(&profile_b.id);
                let current_mark = if profile_a.is_current {
                    format!("{} {} ↔ {}", ICON_CHECK, name_a, name_b)
                } else if profile_b.is_current {
                    format!("{} ↔ {}{}", name_a, name_b, ICON_CHECK)
                } else {
                    format!("{} ↔ {}", name_a, name_b)
                };
                write!(f, "🔄 NextDNS: Toggle {}", current_mark)
            }
            NextDnsAction::Disable => write!(f, "❌ NextDNS: Disable (use system DNS)"),
            NextDnsAction::RefreshProfiles => write!(f, "🔄 NextDNS: Refresh Profiles"),
        }
    }
}

/// State file for tracking current NextDNS profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NextDnsState {
    current_profile_id: Option<String>,
    profiles: Vec<NextDnsProfile>,
    last_updated: u64,
}

/// Get the path to the NextDNS state file
fn get_state_file_path() -> Result<PathBuf, Box<dyn Error>> {
    let cache_dir = dirs::cache_dir().ok_or("Could not determine cache directory")?;
    let state_dir = cache_dir.join("network-dmenu");
    fs::create_dir_all(&state_dir)?;
    Ok(state_dir.join("nextdns_state.json"))
}

/// Load the NextDNS state from disk
fn load_state() -> Result<NextDnsState, Box<dyn Error>> {
    let path = get_state_file_path()?;
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(NextDnsState::default())
    }
}

/// Save the NextDNS state to disk
fn save_state(state: &NextDnsState) -> Result<(), Box<dyn Error>> {
    let path = get_state_file_path()?;
    let content = serde_json::to_string_pretty(state)?;
    fs::write(path, content)?;
    Ok(())
}

/// Fetch profiles using blocking HTTP client (for non-async contexts)
pub async fn fetch_profiles_blocking(api_key: &str) -> Result<Vec<NextDnsProfile>, Box<dyn Error>> {
    debug!(
        "Fetching NextDNS profiles with API key: {}",
        if api_key.len() > 4 {
            &api_key[0..4]
        } else {
            api_key
        }
    );

    // Use the async client within the async context
    debug!("Sending request to NextDNS API");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()?;
    let response = client
        .get(format!("{NEXTDNS_API}/profiles"))
        .header("X-Api-Key", api_key)
        .send()
        .await?;

    debug!("NextDNS API response status: {}", response.status());
    if !response.status().is_success() {
        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Could not read error response".to_string());
        let error_msg = format!("API request failed: {} - {}", status, error_body);
        debug!("NextDNS API error: {}", error_msg);
        return Err(error_msg.into());
    }

    debug!("Parsing NextDNS API response...");
    // Get the response text first for better error handling
    let response_text = response.text().await?;
    debug!("Raw response text: {}", response_text);

    // Parse the JSON response
    let profiles_json: serde_json::Value = match serde_json::from_str(&response_text) {
        Ok(json) => json,
        Err(e) => {
            let error_msg = format!(
                "Failed to parse JSON response: {} - Response was: {}",
                e, response_text
            );
            debug!("{}", error_msg);
            return Err(error_msg.into());
        }
    };

    let mut profiles = Vec::new();

    // Check if this is the new format (an object with a "data" field)
    if let Some(data) = profiles_json.get("data") {
        if let Some(arr) = data.as_array() {
            debug!("Found 'data' array with {} items", arr.len());
            // Process each profile in the data array
            for profile_json in arr {
                if let Some(id) = profile_json.get("id").and_then(|v| v.as_str()) {
                    let name = profile_json
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    profiles.push(NextDnsProfile {
                        id: id.to_string(),
                        name,
                        is_current: false,
                    });
                } else {
                    debug!("Profile missing ID: {:?}", profile_json);
                }
            }
        } else {
            debug!("'data' field is not an array: {:?}", data);
            return Err("API response has 'data' field that is not an array".into());
        }
    }
    // Check if this is the old format (a direct array)
    else if let Some(arr) = profiles_json.as_array() {
        debug!("Found direct array with {} items", arr.len());
        for profile_json in arr {
            if let Some(id) = profile_json.get("id").and_then(|v| v.as_str()) {
                let name = profile_json
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                profiles.push(NextDnsProfile {
                    id: id.to_string(),
                    name,
                    is_current: false,
                });
            } else {
                debug!("Profile missing ID: {:?}", profile_json);
            }
        }
    } else {
        debug!(
            "Response is neither an object with 'data' nor an array: {:?}",
            profiles_json
        );
        return Err("API response is not in a recognized format".into());
    }

    debug!(
        "Extracted {} profiles from NextDNS API response",
        profiles.len()
    );
    if profiles.is_empty() {
        warn!("No profiles found in the API response");
    } else {
        for profile in &profiles {
            debug!(
                "Profile: id={}, name={}",
                profile.id,
                profile.name.as_deref().unwrap_or("None")
            );
        }
    }

    // Update the state with fetched profiles
    let mut state = load_state().unwrap_or_default();
    debug!("Loaded state with {} cached profiles", state.profiles.len());

    // Mark the current profile
    if let Some(current_id) = &state.current_profile_id {
        eprintln!("DEBUG: Current profile ID from state: {}", current_id);
        for profile in &mut profiles {
            if profile.id == *current_id {
                profile.is_current = true;
                eprintln!("DEBUG: Marked profile {} as current", profile.id);
                break;
            }
        }
    } else {
        eprintln!("DEBUG: No current profile ID in state");
    }

    state.profiles = profiles.clone();
    state.last_updated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    debug!("Updating state with {} profiles", profiles.len());
    save_state(&state)?;
    debug!("State saved successfully");

    Ok(profiles)
}

/// Get the current NextDNS profile from state
pub fn get_current_profile() -> Result<Option<NextDnsProfile>, Box<dyn Error>> {
    let state = load_state()?;
    if let Some(current_id) = state.current_profile_id {
        Ok(state.profiles.into_iter().find(|p| p.id == current_id))
    } else {
        Ok(None)
    }
}

/// Set the current NextDNS profile
pub fn set_current_profile(
    profile_id: &str,
    command_runner: &dyn CommandRunner,
) -> Result<(), Box<dyn Error>> {
    // Build the command to set DNS servers and enable DoT
    let mut commands = Vec::new();

    // Detect the active network interface
    commands.push(
        "iface=$(ip route show default | grep -oP 'dev \\K\\S+' | head -1); iface=${iface:-wlan0}"
            .to_string(),
    );

    // For DNS-over-TLS, we'll use the NextDNS DoT endpoint directly with the profile ID
    let dot_hostname = format!("{}.dns.nextdns.io", profile_id);

    debug!(
        "Setting up NextDNS with DoT for profile {} using hostname {}",
        profile_id, dot_hostname
    );

    // Set DNS-over-TLS with NextDNS
    // Use NextDNS anycast IPs with the DoT hostname
    // Format: IP#DoTHostname will automatically use DoT with the specified hostname
    commands.push(format!(
        "resolvectl dns \"${{iface}}\" '45.90.28.0#{}' '45.90.30.0#{}'",
        dot_hostname, dot_hostname
    ));
    commands.push("resolvectl domain \"${{iface}}\" '~.'".to_string());
    commands.push("resolvectl dnssec \"${{iface}}\" allow-downgrade".to_string());
    commands.push("resolvectl dnsovertls \"${{iface}}\" yes".to_string());

    // Join all commands with semicolons and execute with privilege elevation
    let full_command = commands.join("; ");
    let privileged_cmd = wrap_privileged_command(&full_command, true);

    debug!("Running command: {}", privileged_cmd);
    let output = command_runner.run_command("sh", &["-c", &privileged_cmd])?;
    debug!("Command output: {:?}", output);

    // Find the profile name if available
    let state = load_state().unwrap_or_default();
    let profile_name = state
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .and_then(|p| p.name.as_deref())
        .unwrap_or(profile_id);

    // Send a notification
    let _ = notify_rust::Notification::new()
        .summary("NextDNS Profile Activated")
        .body(&format!("Now using profile: {}", profile_name))
        .show();

    // Update state
    let mut state = load_state().unwrap_or_default();
    state.current_profile_id = Some(profile_id.to_string());
    save_state(&state)?;

    Ok(())
}

/// Disable NextDNS (revert to system DNS)
pub fn disable_nextdns(command_runner: &dyn CommandRunner) -> Result<(), Box<dyn Error>> {
    // Revert DNS settings to DHCP
    let commands = [
        "iface=$(ip route show default | grep -oP 'dev \\K\\S+' | head -1); iface=${iface:-wlan0}",
        "resolvectl revert \"${iface}\"",
        "resolvectl dnsovertls \"${iface}\" no",
        "resolvectl dnssec \"${iface}\" no",
    ];

    debug!("Disabling NextDNS and reverting to system DNS");
    let full_command = commands.join("; ");
    let privileged_cmd = wrap_privileged_command(&full_command, true);

    let result = command_runner.run_command("sh", &["-c", &privileged_cmd])?;

    if result.status.success() {
        // Update the state
        let mut state = load_state().unwrap_or_default();
        state.current_profile_id = None;
        save_state(&state)?;

        // Send a notification
        let _ = notify_rust::Notification::new()
            .summary("NextDNS Disabled")
            .body("Reverted to system DNS configuration")
            .show();

        Ok(())
    } else {
        Err("Failed to revert DNS configuration".into())
    }
}

/// Generate NextDNS actions based on configuration and state
pub async fn get_nextdns_actions(
    api_key: Option<&str>,
    toggle_profiles: Option<(&str, &str)>,
) -> Result<Vec<NextDnsAction>, Box<dyn Error>> {
    let mut actions = Vec::new();

    // Load current state
    let state = load_state().unwrap_or_default();
    let current_profile_id = state.current_profile_id.clone();

    // Add disable action if NextDNS is currently active
    if current_profile_id.is_some() {
        actions.push(NextDnsAction::Disable);
    }

    // If we have an API key, we can fetch and list profiles
    if let Some(api_key) = api_key {
        // Check if we have cached profiles or they're stale (older than 1 hour)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        debug!(
            "Checking if profiles need refresh (empty={}, age={}s)",
            state.profiles.is_empty(),
            now - state.last_updated
        );

        let profiles = if state.profiles.is_empty() || (now - state.last_updated) > 3600 {
            // Fetch fresh profiles
            debug!(
                "Fetching fresh profiles with API key: {}",
                if api_key.len() > 4 {
                    &api_key[0..4]
                } else {
                    api_key
                }
            );
            match fetch_profiles_blocking(api_key).await {
                Ok(profiles) => {
                    debug!("Successfully fetched {} profiles", profiles.len());
                    profiles
                }
                Err(e) => {
                    warn!("Error fetching profiles: {}", e);
                    debug!("Falling back to {} cached profiles", state.profiles.len());
                    state.profiles // Fall back to cached profiles
                }
            }
        } else {
            debug!(
                "Using {} cached profiles (cache is fresh)",
                state.profiles.len()
            );
            state.profiles
        };

        // Add action to refresh profiles
        actions.push(NextDnsAction::RefreshProfiles);

        // Add profile switching actions
        for profile in profiles.iter() {
            let mut profile = profile.clone();
            profile.is_current = Some(&profile.id) == current_profile_id.as_ref();
            actions.push(NextDnsAction::SetProfile { profile });
        }

        // Add toggle action if specified
        if let Some((id_a, id_b)) = toggle_profiles {
            if let (Some(profile_a), Some(profile_b)) = (
                profiles.iter().find(|p| p.id == id_a).cloned(),
                profiles.iter().find(|p| p.id == id_b).cloned(),
            ) {
                actions.push(NextDnsAction::ToggleProfiles {
                    profile_a,
                    profile_b,
                });
            }
        }
    } else if let Some((id_a, id_b)) = toggle_profiles {
        // Even without API key, we can create toggle action with just IDs
        let profile_a = NextDnsProfile {
            id: id_a.to_string(),
            name: None,
            is_current: current_profile_id.as_deref() == Some(id_a),
        };
        let profile_b = NextDnsProfile {
            id: id_b.to_string(),
            name: None,
            is_current: current_profile_id.as_deref() == Some(id_b),
        };
        actions.push(NextDnsAction::ToggleProfiles {
            profile_a,
            profile_b,
        });
    }

    Ok(actions)
}

/// Handle a NextDNS action
pub async fn handle_nextdns_action(
    action: &NextDnsAction,
    command_runner: &dyn CommandRunner,
    api_key: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    match action {
        NextDnsAction::SetProfile { profile } => {
            set_current_profile(&profile.id, command_runner)?;
            println!(
                "Switched to NextDNS profile: {}",
                profile.name.as_deref().unwrap_or(&profile.id)
            );
            Ok(true)
        }

        NextDnsAction::ToggleProfiles {
            profile_a,
            profile_b,
        } => {
            let current = get_current_profile()?;
            let next_profile = if current.as_ref().map(|p| &p.id) == Some(&profile_a.id) {
                profile_b
            } else {
                profile_a
            };

            set_current_profile(&next_profile.id, command_runner)?;
            println!(
                "Toggled to NextDNS profile: {}",
                next_profile.name.as_deref().unwrap_or(&next_profile.id)
            );
            Ok(true)
        }

        NextDnsAction::Disable => {
            disable_nextdns(command_runner)?;
            println!("NextDNS disabled, reverted to system DNS");
            Ok(true)
        }

        NextDnsAction::RefreshProfiles => {
            if let Some(api_key) = api_key {
                debug!(
                    "Manually refreshing profiles with API key: {}",
                    if api_key.len() > 4 {
                        &api_key[0..4]
                    } else {
                        api_key
                    }
                );

                // Use tokio spawn_blocking to run this operation in a separate thread
                // that won't interfere with the Tokio runtime
                let profiles_result = tokio::task::block_in_place(|| {
                    debug!("Creating runtime for profile refresh");
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all() // Enable all features including time needed by reqwest
                        .build()
                        .unwrap_or_else(|e| {
                            error!("Failed to build runtime: {}", e);
                            panic!("Failed to build Tokio runtime: {}", e);
                        });
                    debug!("Running fetch_profiles_blocking in separate runtime");
                    rt.block_on(fetch_profiles_blocking(api_key))
                });

                match profiles_result {
                    Ok(profiles) => {
                        debug!("Manual refresh complete, got {} profiles", profiles.len());
                        if profiles.is_empty() {
                            println!("Warning: Refreshed 0 NextDNS profiles. Check your API key permissions.");
                        } else {
                            println!("Refreshed {} NextDNS profiles", profiles.len());
                            for profile in &profiles {
                                println!(
                                    " - {} ({})",
                                    profile.name.as_deref().unwrap_or("Unnamed"),
                                    profile.id
                                );
                            }
                        }
                        Ok(true)
                    }
                    Err(e) => {
                        error!("Failed to refresh profiles: {}", e);
                        println!("Error refreshing NextDNS profiles: {}", e);
                        Ok(false)
                    }
                }
            } else {
                warn!("Cannot refresh profiles: No API key provided");
                println!("API key required to refresh profiles");
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;

    #[cfg(test)]
    #[allow(dead_code)]
    struct MockCommandRunner {
        expected_command: String,
        expected_args: Vec<String>,
        return_output: Output,
    }

    impl MockCommandRunner {
        #[cfg(test)]
        #[allow(dead_code)]
        fn new(command: &str, args: &[&str], output: Output) -> Self {
            Self {
                expected_command: command.to_string(),
                expected_args: args.iter().map(|s| s.to_string()).collect(),
                return_output: output,
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            assert_eq!(command, self.expected_command);
            assert_eq!(
                args,
                self.expected_args
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
            );
            Ok(Output {
                status: self.return_output.status,
                stdout: self.return_output.stdout.clone(),
                stderr: self.return_output.stderr.clone(),
            })
        }
    }

    #[test]
    fn test_nextdns_profile_display() {
        let profile = NextDnsProfile {
            id: "abc123".to_string(),
            name: Some("Home".to_string()),
            is_current: false,
        };

        let action = NextDnsAction::SetProfile { profile };
        assert_eq!(action.to_string(), "🔄 NextDNS: Switch to Home");

        let profile_current = NextDnsProfile {
            id: "abc123".to_string(),
            name: Some("Home".to_string()),
            is_current: true,
        };

        let action = NextDnsAction::SetProfile {
            profile: profile_current,
        };
        assert_eq!(action.to_string(), "✅ NextDNS: Home (current)");
    }

    #[test]
    fn test_toggle_action_display() {
        let profile_a = NextDnsProfile {
            id: "abc123".to_string(),
            name: Some("Home".to_string()),
            is_current: false,
        };

        let profile_b = NextDnsProfile {
            id: "xyz789".to_string(),
            name: Some("Work".to_string()),
            is_current: false,
        };

        let action = NextDnsAction::ToggleProfiles {
            profile_a,
            profile_b,
        };
        assert_eq!(action.to_string(), "🔄 NextDNS: Toggle Home ↔ Work");
    }

    #[test]
    fn test_state_serialization() {
        let state = NextDnsState {
            current_profile_id: Some("test123".to_string()),
            profiles: vec![NextDnsProfile {
                id: "test123".to_string(),
                name: Some("Test Profile".to_string()),
                is_current: true,
            }],
            last_updated: 1234567890,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: NextDnsState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.current_profile_id, state.current_profile_id);
        assert_eq!(deserialized.profiles.len(), 1);
        assert_eq!(deserialized.last_updated, state.last_updated);
    }
}
