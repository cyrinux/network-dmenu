//! NextDNS profile management module
//!
//! This module provides functionality to manage NextDNS profiles via their API,
//! with proper error handling and state management.

use crate::command::CommandRunner;
use crate::nextdns_api::{NextDnsApi, Profile};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a NextDNS profile for the UI
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextDnsProfile {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub is_current: bool,
}

impl From<Profile> for NextDnsProfile {
    fn from(profile: Profile) -> Self {
        NextDnsProfile {
            id: profile.id,
            name: profile.name,
            is_current: profile.is_current,
        }
    }
}

/// NextDNS action types
#[derive(Debug, Clone, PartialEq)]
pub enum NextDnsAction {
    /// Switch to a specific profile
    SetProfile { profile: NextDnsProfile },
    /// Toggle between two profiles
    ToggleProfiles {
        profile_a: NextDnsProfile,
        profile_b: NextDnsProfile
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
                let name = profile.name.as_deref().unwrap_or(&profile.id);
                if profile.is_current {
                    write!(f, "✅ NextDNS: {} (current)", name)
                } else {
                    write!(f, "🔄 NextDNS: Switch to {}", name)
                }
            }
            NextDnsAction::ToggleProfiles { profile_a, profile_b } => {
                let name_a = profile_a.name.as_deref().unwrap_or(&profile_a.id);
                let name_b = profile_b.name.as_deref().unwrap_or(&profile_b.id);
                write!(f, "🔄 NextDNS: Toggle {} ↔ {}", name_a, name_b)
            }
            NextDnsAction::Disable => write!(f, "❌ NextDNS: Disable (use system DNS)"),
            NextDnsAction::RefreshProfiles => write!(f, "🔄 NextDNS: Refresh Profiles"),
        }
    }
}

/// State file for tracking current NextDNS profile and cached data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NextDnsState {
    current_profile_id: Option<String>,
    profiles: Vec<NextDnsProfile>,
    last_updated: u64,
}

impl Default for NextDnsState {
    fn default() -> Self {
        Self {
            current_profile_id: None,
            profiles: Vec::new(),
            last_updated: 0,
        }
    }
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

/// Fetch profiles from API and update cache
pub async fn fetch_profiles_blocking(api_key: &str) -> Result<Vec<NextDnsProfile>, Box<dyn Error>> {
    // Use tokio::task::spawn_blocking to run the blocking API calls
    let api_key_owned = api_key.to_string();
    let profiles = tokio::task::spawn_blocking(move || {
        let api = NextDnsApi::new(api_key_owned);
        let api_profiles = api.list_profiles()?;

        // Convert API profiles to our profile type
        let profiles: Vec<NextDnsProfile> = api_profiles
            .into_iter()
            .map(NextDnsProfile::from)
            .collect();

        Ok::<_, Box<dyn Error>>(profiles)
    }).await??;

    // Update cache with fresh data
    let mut state = load_state().unwrap_or_default();
    state.profiles = profiles.clone();
    state.last_updated = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    save_state(&state)?;

    Ok(profiles)
}

/// Get the current active profile
pub fn get_current_profile() -> Result<Option<NextDnsProfile>, Box<dyn Error>> {
    let state = load_state()?;
    if let Some(current_id) = state.current_profile_id {
        Ok(state.profiles.into_iter().find(|p| p.id == current_id))
    } else {
        Ok(None)
    }
}

/// Set the current profile using the API
pub fn set_current_profile(
    profile_id: &str,
    _command_runner: &dyn CommandRunner,
    api_key: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    if let Some(key) = api_key {
        let api = NextDnsApi::new(key.to_string());
        api.set_profile(profile_id)?;
    }

    // Update local state regardless of API key availability
    let mut state = load_state().unwrap_or_default();
    state.current_profile_id = Some(profile_id.to_string());
    save_state(&state)?;

    Ok(())
}

/// Disable NextDNS and revert to system DNS
pub fn disable_nextdns(
    _command_runner: &dyn CommandRunner,
    api_key: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    if let Some(key) = api_key {
        let api = NextDnsApi::new(key.to_string());
        api.disable()?;
    }

    // Update local state
    let mut state = load_state().unwrap_or_default();
    state.current_profile_id = None;
    save_state(&state)?;

    Ok(())
}

/// Get available NextDNS actions
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

    // Determine if we should fetch fresh profiles
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    let should_refresh = (now - state.last_updated) > 3600; // 1 hour

    let profiles = if let Some(api_key) = api_key {
        if state.profiles.is_empty() || should_refresh {
            // Try to fetch fresh profiles, fall back to cache on error
            match fetch_profiles_blocking(api_key).await {
                Ok(profiles) => profiles,
                Err(_) => state.profiles
            }
        } else {
            state.profiles
        }
    } else {
        // No API key, use cached profiles
        state.profiles
    };

    // Add refresh action if we have an API key
    if api_key.is_some() {
        actions.push(NextDnsAction::RefreshProfiles);
    }

    // Add profile switching actions
    for mut profile in profiles.clone() {
        profile.is_current = Some(&profile.id) == current_profile_id.as_ref();
        actions.push(NextDnsAction::SetProfile { profile });
    }

    // Add toggle action if specified
    if let Some((id_a, id_b)) = toggle_profiles {
        let profile_a = profiles
            .iter()
            .find(|p| p.id == id_a)
            .cloned()
            .unwrap_or_else(|| NextDnsProfile {
                id: id_a.to_string(),
                name: None,
                is_current: current_profile_id.as_deref() == Some(id_a),
            });

        let profile_b = profiles
            .iter()
            .find(|p| p.id == id_b)
            .cloned()
            .unwrap_or_else(|| NextDnsProfile {
                id: id_b.to_string(),
                name: None,
                is_current: current_profile_id.as_deref() == Some(id_b),
            });

        actions.push(NextDnsAction::ToggleProfiles { profile_a, profile_b });
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
            set_current_profile(&profile.id, command_runner, api_key)?;
            println!(
                "Switched to NextDNS profile: {}",
                profile.name.as_deref().unwrap_or(&profile.id)
            );
            Ok(true)
        }

        NextDnsAction::ToggleProfiles { profile_a, profile_b } => {
            let current = get_current_profile()?;
            let next_profile = if current.as_ref().map(|p| &p.id) == Some(&profile_a.id) {
                profile_b
            } else {
                profile_a
            };

            set_current_profile(&next_profile.id, command_runner, api_key)?;
            println!(
                "Toggled to NextDNS profile: {}",
                next_profile.name.as_deref().unwrap_or(&next_profile.id)
            );
            Ok(true)
        }

        NextDnsAction::Disable => {
            disable_nextdns(command_runner, api_key)?;
            println!("NextDNS disabled, reverted to system DNS");
            Ok(true)
        }

        NextDnsAction::RefreshProfiles => {
            if let Some(api_key) = api_key {
                let profiles = fetch_profiles_blocking(api_key).await?;
                println!("Refreshed {} NextDNS profiles", profiles.len());
                Ok(true)
            } else {
                println!("API key required to refresh profiles");
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let action = NextDnsAction::SetProfile { profile: profile_current };
        assert_eq!(action.to_string(), "✅ NextDNS: Home (current)");
    }

    #[test]
    fn test_toggle_action_display() {
        let profile_a = NextDnsProfile {
            id: "aaa".to_string(),
            name: Some("Work".to_string()),
            is_current: false,
        };

        let profile_b = NextDnsProfile {
            id: "bbb".to_string(),
            name: Some("Home".to_string()),
            is_current: false,
        };

        let action = NextDnsAction::ToggleProfiles { profile_a, profile_b };
        assert_eq!(action.to_string(), "🔄 NextDNS: Toggle Work ↔ Home");
    }

    #[test]
    fn test_state_serialization() {
        let state = NextDnsState {
            current_profile_id: Some("test123".to_string()),
            profiles: vec![
                NextDnsProfile {
                    id: "test123".to_string(),
                    name: Some("Test".to_string()),
                    is_current: true,
                }
            ],
            last_updated: 1234567890,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: NextDnsState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.current_profile_id, state.current_profile_id);
        assert_eq!(deserialized.profiles.len(), 1);
        assert_eq!(deserialized.last_updated, state.last_updated);
    }
}
