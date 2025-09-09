//! Firewalld integration module
//!
//! This module provides functionality to interact with firewalld zones and panic mode
//! using the firewall-cmd command.

use crate::command::{CommandRunner, is_command_installed};
use crate::constants::{ICON_FIREWALL_ALLOW, ICON_FIREWALL_BLOCK, ICON_LOCK};
use crate::privilege::wrap_privileged_command;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as TokioCommand;

/// Firewalld-related actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FirewalldAction {
    /// Change to a specific zone
    SetZone(String),
    /// Toggle panic mode
    TogglePanicMode,
    /// Get current zone
    GetCurrentZone,
    /// Open firewalld configuration editor
    OpenConfigEditor,
}

/// Information about a firewalld zone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewalldZone {
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub is_default: bool,
}

/// Cached firewalld data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewalldCache {
    pub zones: Vec<String>,
    pub active_zones: HashMap<String, Vec<String>>, // zone_name -> interfaces
    pub panic_mode: bool,
    pub cached_at: u64,
}


impl FirewalldAction {
    /// Convert action to display string with current zone and panic mode information
    pub fn to_display_string(&self, current_zone: Option<&str>) -> String {
        self.to_display_string_with_panic(current_zone, None)
    }

    /// Convert action to display string with current zone and panic mode information
    pub fn to_display_string_with_panic(&self, current_zone: Option<&str>, panic_mode: Option<bool>) -> String {
        match self {
            FirewalldAction::SetZone(zone) => {
                let is_current = current_zone.is_some_and(|current| current == zone);
                if is_current {
                    format!(
                        "firewalld  - ✅ Switch to zone: {} (current)",
                        zone
                    )
                } else {
                    format!(
                        "firewalld  - {} Switch to zone: {}",
                        ICON_FIREWALL_ALLOW, zone
                    )
                }
            }
            FirewalldAction::TogglePanicMode => {
                // Use provided panic mode state or check it
                let is_panic_on = panic_mode.unwrap_or_else(|| {
                    std::process::Command::new("firewall-cmd")
                        .arg("--query-panic")
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                });
                
                if is_panic_on {
                    format!("firewalld  - {} Disable panic mode", ICON_FIREWALL_ALLOW)
                } else {
                    format!("firewalld  - {} Enable panic mode", ICON_FIREWALL_BLOCK)
                }
            }
            FirewalldAction::GetCurrentZone => {
                format!("firewalld  - {} Show current zone", ICON_LOCK)
            }
            FirewalldAction::OpenConfigEditor => {
                "firewalld  - ⚙️ Open firewall configuration".to_string()
            }
        }
    }

    /// Convert action to display string (backwards compatibility)
    /// This version dynamically fetches the current zone and panic mode to show proper state
    pub fn to_display_string_simple(&self) -> String {
        // Try to get current zone synchronously for better display
        let current_zone = std::process::Command::new("firewall-cmd")
            .arg("--get-default-zone")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            });
        
        // Try to get panic mode state for toggle action
        let panic_mode = if matches!(self, FirewalldAction::TogglePanicMode) {
            std::process::Command::new("firewall-cmd")
                .arg("--query-panic")
                .output()
                .ok()
                .map(|output| output.status.success())
        } else {
            None
        };
        
        self.to_display_string_with_panic(current_zone.as_deref(), panic_mode)
    }
}

/// Get available firewalld actions (async version) - INSTANT VERSION  
pub async fn get_firewalld_actions_async() -> Vec<FirewalldAction> {
    // Quick check if firewall-cmd exists
    if !is_command_installed("firewall-cmd") {
        debug!("firewall-cmd not installed, skipping firewalld actions");
        return vec![];
    }

    // Get cached firewalld data (no extra calls)
    let cache_data = get_or_refresh_firewalld_cache().await;

    // Generate actions instantly from cache (no more async calls)
    generate_firewalld_actions_from_cache(&cache_data)
}


/// Generate firewalld actions from cached data (pure function, no async)
fn generate_firewalld_actions_from_cache(cache_data: &FirewalldCache) -> Vec<FirewalldAction> {
    debug!("Generating instant firewalld actions from cache");

    let mut actions = vec![
        FirewalldAction::GetCurrentZone,
        FirewalldAction::OpenConfigEditor,
    ];

    // Add panic mode toggle action  
    actions.push(FirewalldAction::TogglePanicMode);

    // Add simple zone switching actions for all cached zones
    for zone_name in &cache_data.zones {
        actions.push(FirewalldAction::SetZone(zone_name.clone()));
    }

    debug!("Generated {} instant firewalld actions", actions.len());
    actions
}

/// Generate firewalld actions with display strings from cached data
pub fn generate_firewalld_actions_with_display_from_cache(cache_data: &FirewalldCache, current_zone: Option<&str>) -> Vec<(FirewalldAction, String)> {
    debug!("Generating instant firewalld actions with display from cache");

    let mut actions = vec![];

    // Add zone information action
    let zone_action = FirewalldAction::GetCurrentZone;
    actions.push((zone_action.clone(), zone_action.to_display_string_with_panic(current_zone, Some(cache_data.panic_mode))));

    // Add config editor action
    let config_action = FirewalldAction::OpenConfigEditor;
    actions.push((config_action.clone(), config_action.to_display_string_with_panic(current_zone, Some(cache_data.panic_mode))));

    // Add zone switching actions
    for zone_name in &cache_data.zones {
        let action = FirewalldAction::SetZone(zone_name.clone());
        let display = if Some(zone_name.as_str()) == current_zone {
            format!("firewalld  - ✅ Switch to zone: {} (current)", zone_name)
        } else {
            format!("firewalld  - {} Switch to zone: {}", ICON_FIREWALL_ALLOW, zone_name)
        };
        actions.push((action, display));
    }

    // Add panic mode toggle action with cached state
    let panic_action = FirewalldAction::TogglePanicMode;
    let panic_display = panic_action.to_display_string_with_panic(current_zone, Some(cache_data.panic_mode));
    actions.push((panic_action, panic_display));

    debug!("Generated {} instant firewalld actions with display", actions.len());
    actions
}

/// Get available firewalld actions (sync version for backwards compatibility)
pub fn get_firewalld_actions(command_runner: &dyn CommandRunner) -> Vec<FirewalldAction> {
    if !is_firewalld_available() {
        debug!("firewall-cmd not available, skipping firewalld actions");
        return vec![];
    }

    let mut actions = vec![
        FirewalldAction::GetCurrentZone,
        FirewalldAction::OpenConfigEditor,
    ];

    // Add zone switching actions
    if let Ok(zones) = get_available_zones(command_runner) {
        for zone in zones {
            actions.push(FirewalldAction::SetZone(zone.name));
        }
    }

    // Add panic mode toggle
    actions.push(FirewalldAction::TogglePanicMode);

    actions
}

/// Get firewalld actions with proper display strings
pub async fn get_firewalld_actions_with_display() -> Vec<(FirewalldAction, String)> {
    if !is_firewalld_available_async().await {
        debug!("firewall-cmd not available, skipping firewalld actions");
        return vec![];
    }

    let mut actions = vec![];
    
    // Fetch current zone for better performance  
    let current_zone = get_current_zone_async().await.ok();

    // Add zone information action
    let zone_action = FirewalldAction::GetCurrentZone;
    actions.push((zone_action.clone(), zone_action.to_display_string(current_zone.as_deref())));

    // Add config editor action
    let config_action = FirewalldAction::OpenConfigEditor;
    actions.push((config_action.clone(), config_action.to_display_string(current_zone.as_deref())));

    // Add zone switching actions with proper indicators
    match get_available_zones_async().await {
        Ok(zones) => {
            for zone in zones {
                let action = FirewalldAction::SetZone(zone.name.clone());
                let display = if Some(&zone.name) == current_zone.as_ref() {
                    format!("firewalld  - ✅ Switch to zone: {} (current)", zone.name)
                } else {
                    format!("firewalld  - {} Switch to zone: {}", ICON_FIREWALL_ALLOW, zone.name)
                };
                actions.push((action, display));
            }
        }
        Err(e) => {
            debug!("Failed to get zones: {}, adding fallback zones", e);
            let fallback_zones = vec!["public", "home", "work", "trusted", "block", "drop"];
            for zone in fallback_zones {
                let action = FirewalldAction::SetZone(zone.to_string());
                let display = if Some(zone) == current_zone.as_deref() {
                    format!("firewalld  - ✅ Switch to zone: {} (current)", zone)
                } else {
                    format!("firewalld  - {} Switch to zone: {}", ICON_FIREWALL_ALLOW, zone)
                };
                actions.push((action, display));
            }
        }
    }

    // Add panic mode toggle action with state-aware display
    let panic_action = FirewalldAction::TogglePanicMode;
    let is_panic_on = tokio::process::Command::new("firewall-cmd")
        .arg("--query-panic")
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    let panic_display = if is_panic_on {
        format!("firewalld  - {} Disable panic mode", ICON_FIREWALL_ALLOW)
    } else {
        format!("firewalld  - {} Enable panic mode", ICON_FIREWALL_BLOCK)
    };
    actions.push((panic_action, panic_display));

    actions
}

/// Result of firewalld action execution
#[derive(Debug)]
pub struct FirewalldActionResult {
    pub success: bool,
    pub message: Option<String>,
}

/// Handle firewalld action execution
pub async fn handle_firewalld_action(
    action: &FirewalldAction,
    command_runner: &dyn CommandRunner,
) -> Result<FirewalldActionResult, Box<dyn Error>> {
    if !is_firewalld_available() {
        return Err("firewall-cmd command not found. Please install firewalld.".into());
    }

    debug!("Handling firewalld action: {:?}", action);

    match action {
        FirewalldAction::SetZone(zone) => {
            set_default_zone(zone, command_runner)?;
            Ok(FirewalldActionResult {
                success: true,
                message: Some(format!("Switched to firewalld zone: {}", zone)),
            })
        }
        FirewalldAction::TogglePanicMode => {
            // Check current panic mode state and toggle it
            let current_panic = is_panic_mode_enabled(command_runner).unwrap_or(false);
            let new_panic_state = !current_panic;
            
            set_panic_mode(new_panic_state, command_runner)?;
            let message = if new_panic_state {
                "Firewalld panic mode enabled - all connections blocked"
            } else {
                "Firewalld panic mode disabled"
            };
            Ok(FirewalldActionResult {
                success: true,
                message: Some(message.to_string()),
            })
        }
        FirewalldAction::GetCurrentZone => {
            let zone = get_current_zone(command_runner)?;
            debug!("Current firewalld zone: {}", zone);
            Ok(FirewalldActionResult {
                success: true,
                message: Some(format!("Current firewalld zone: {}", zone)),
            })
        }
        FirewalldAction::OpenConfigEditor => {
            open_firewall_config_editor()?;
            Ok(FirewalldActionResult {
                success: true,
                message: Some("Firewalld configuration editor opened".to_string()),
            })
        }
    }
}

/// Check if firewall-cmd is available and firewalld service is running (async version)
async fn is_firewalld_available_async() -> bool {
    // First check if command exists
    let cmd_exists = tokio::process::Command::new("which")
        .arg("firewall-cmd")
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    if !cmd_exists {
        debug!("firewall-cmd command not found");
        return false;
    }
    
    // Check if firewalld process is running (works with any init system, not just systemd)
    let process_check = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        tokio::process::Command::new("pgrep")
            .args(["-f", "firewalld"])
            .output()
    ).await;
    
    let process_running = process_check
        .map(|result| result.map(|output| output.status.success()).unwrap_or(false))
        .unwrap_or(false);
    
    if !process_running {
        debug!("firewalld process is not running");
        return false;
    }
    
    // Skip the firewall-cmd validation test - if process is running, assume it will work
    // The individual firewall-cmd calls will handle any issues gracefully
    debug!("firewalld process detected, assuming it's functional");
    
    debug!("firewalld appears to be available and running");
    true
}

/// Check if firewall-cmd is available and firewalld service is running (sync version)
fn is_firewalld_available() -> bool {
    // First check if command exists
    let cmd_exists = Command::new("which")
        .arg("firewall-cmd")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    if !cmd_exists {
        debug!("firewall-cmd command not found");
        return false;
    }
    
    // Check if firewalld process is running (works with any init system, not just systemd)
    let process_running = Command::new("pgrep")
        .args(["-f", "firewalld"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    if !process_running {
        debug!("firewalld process is not running");
        return false;
    }
    
    debug!("firewalld appears to be available and running");
    true
}

/// Get the current active zone (async version)
async fn get_current_zone_async() -> Result<String, Box<dyn Error + Send + Sync>> {
    let output = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        tokio::process::Command::new("firewall-cmd")
            .arg("--get-default-zone")
            .output()
    ).await
    .map_err(|_| "firewall-cmd timeout")??;

    if !output.status.success() {
        return Err("Failed to get current zone".into());
    }

    let zone = String::from_utf8(output.stdout)?.trim().to_string();

    debug!("Current firewalld zone: {}", zone);
    Ok(zone)
}

/// Get the current active zone (sync version)
fn get_current_zone(command_runner: &dyn CommandRunner) -> Result<String, Box<dyn Error>> {
    let output = command_runner.run_command("firewall-cmd", &["--get-default-zone"])?;

    if !output.status.success() {
        return Err("Failed to get current zone".into());
    }

    let zone = String::from_utf8(output.stdout)?.trim().to_string();

    debug!("Current firewalld zone: {}", zone);
    Ok(zone)
}

/// Get available firewalld zones with information (async version)
async fn get_available_zones_async() -> Result<Vec<FirewalldZone>, Box<dyn Error + Send + Sync>> {
    // Get list of zones with timeout
    let zones_output = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        tokio::process::Command::new("firewall-cmd")
            .arg("--get-zones")
            .output()
    ).await
    .map_err(|_| "firewall-cmd --get-zones timeout")??;
    if !zones_output.status.success() {
        return Err("Failed to get zones list".into());
    }

    let zones_str = String::from_utf8(zones_output.stdout)?;
    let zone_names: Vec<&str> = zones_str.split_whitespace().collect();

    // Get current default zone
    let current_zone = get_current_zone_async().await.unwrap_or_default();

    // Get active zones with timeout
    let active_zones_output = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        tokio::process::Command::new("firewall-cmd")
            .arg("--get-active-zones")
            .output()
    ).await
    .map_err(|_| "firewall-cmd --get-active-zones timeout")??;
    let active_zones_str = if active_zones_output.status.success() {
        String::from_utf8(active_zones_output.stdout).unwrap_or_default()
    } else {
        String::new()
    };

    let mut zones = Vec::new();

    for zone_name in zone_names {
        // Skip individual zone description calls for performance
        // Use simple zone name as description to avoid multiple firewall-cmd calls
        let description = get_zone_description_simple(zone_name);

        let is_active = active_zones_str.contains(zone_name);
        let is_default = zone_name == current_zone;

        zones.push(FirewalldZone {
            name: zone_name.to_string(),
            description,
            is_active,
            is_default,
        });
    }

    debug!("Found {} firewalld zones (fast mode)", zones.len());
    Ok(zones)
}

/// Get simple zone description without external calls for performance
fn get_zone_description_simple(zone_name: &str) -> String {
    match zone_name {
        "public" => "For use in public areas with limited trust",
        "home" => "For use in home environments with trusted devices",
        "work" => "For use in work environments with some trust",
        "trusted" => "All network connections are accepted",
        "block" => "All incoming network connections are rejected",
        "drop" => "All incoming packets are dropped without reply",
        "dmz" => "For computers in demilitarized zone with limited access",
        "external" => "For use on external networks with masquerading enabled",
        "internal" => "For use on internal networks when you trust other computers",
        "libvirt" => "For libvirt virtual machine networks",
        "libvirt-routed" => "For libvirt routed virtual machine networks",
        _ => "Firewall zone",
    }.to_string()
}

/// Get available firewalld zones with information (sync version)
fn get_available_zones(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<FirewalldZone>, Box<dyn Error>> {
    // Get list of zones
    let zones_output = command_runner.run_command("firewall-cmd", &["--get-zones"])?;
    if !zones_output.status.success() {
        return Err("Failed to get zones list".into());
    }

    let zones_str = String::from_utf8(zones_output.stdout)?;
    let zone_names: Vec<&str> = zones_str.split_whitespace().collect();

    // Get current default zone
    let current_zone = get_current_zone(command_runner).unwrap_or_default();

    // Get active zones
    let active_zones_output =
        command_runner.run_command("firewall-cmd", &["--get-active-zones"])?;
    let active_zones_str = if active_zones_output.status.success() {
        String::from_utf8(active_zones_output.stdout).unwrap_or_default()
    } else {
        String::new()
    };

    let mut zones = Vec::new();

    for zone_name in zone_names {
        // Get zone description
        let info_output = command_runner
            .run_command("firewall-cmd", &["--zone", zone_name, "--get-description"])?;

        let description = if info_output.status.success() {
            String::from_utf8(info_output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            format!("Zone: {}", zone_name)
        };

        let is_active = active_zones_str.contains(zone_name);
        let is_default = zone_name == current_zone;

        zones.push(FirewalldZone {
            name: zone_name.to_string(),
            description,
            is_active,
            is_default,
        });
    }

    debug!("Found {} firewalld zones", zones.len());
    Ok(zones)
}

/// Set the default firewalld zone
fn set_default_zone(zone: &str, command_runner: &dyn CommandRunner) -> Result<(), Box<dyn Error>> {
    debug!("Setting firewalld zone to: {}", zone);

    // Use privilege escalation for firewall-cmd commands
    let command = format!("firewall-cmd --set-default-zone {}", zone);
    let privileged_cmd = wrap_privileged_command(&command, false);
    
    debug!("Running privileged command: {}", privileged_cmd);
    let output = command_runner.run_command("sh", &["-c", &privileged_cmd])?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to set zone to {}: {}", zone, error_msg).into());
    }

    debug!("Successfully set firewalld zone to: {}", zone);
    Ok(())
}


/// Check if panic mode is enabled (sync version)
fn is_panic_mode_enabled(command_runner: &dyn CommandRunner) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command("firewall-cmd", &["--query-panic"])?;

    // firewall-cmd returns 0 if panic mode is on, 1 if off
    Ok(output.status.success())
}

/// Set panic mode on or off
fn set_panic_mode(enable: bool, command_runner: &dyn CommandRunner) -> Result<(), Box<dyn Error>> {
    let arg = if enable { "--panic-on" } else { "--panic-off" };

    debug!("Setting firewalld panic mode: {}", enable);

    // Use privilege escalation for firewall-cmd commands
    let command = format!("firewall-cmd {}", arg);
    let privileged_cmd = wrap_privileged_command(&command, false);
    
    debug!("Running privileged command: {}", privileged_cmd);
    let output = command_runner.run_command("sh", &["-c", &privileged_cmd])?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to {} panic mode: {}",
            if enable { "enable" } else { "disable" },
            error_msg
        )
        .into());
    }

    debug!(
        "Successfully {} panic mode",
        if enable { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// Open firewalld configuration editor
fn open_firewall_config_editor() -> Result<(), Box<dyn Error>> {
    debug!("Opening firewalld configuration editor");

    // Try different firewalld GUI tools in order of preference
    let gui_tools = [
        "firewall-config", // Official GNOME firewalld GUI
        "firewall-applet", // System tray applet with config option
        "gufw",            // UFW frontend that can work with firewalld
    ];

    for tool in &gui_tools {
        if Command::new("which")
            .arg(tool)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            debug!("Found firewall GUI tool: {}", tool);
            let status = Command::new(tool).status()?;

            if status.success() {
                debug!("Successfully launched {}", tool);
                return Ok(());
            } else {
                debug!("Failed to launch {}", tool);
            }
        }
    }

    // Fallback: try to open firewall configuration with system settings
    let fallback_commands = [
        ("gnome-control-center", vec!["network"]),
        ("systemsettings5", vec!["kcm_firewall"]),
        ("systemsettings", vec!["firewall"]),
        ("unity-control-center", vec!["network"]),
    ];

    for (command, args) in &fallback_commands {
        if Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            debug!("Trying fallback: {} with args {:?}", command, args);
            let status = Command::new(command).args(args).status()?;

            if status.success() {
                debug!("Successfully launched {} with network settings", command);
                return Ok(());
            }
        }
    }

    // Final fallback: open a terminal with firewall-cmd help
    let terminal_commands = [
        ("gnome-terminal", vec!["--", "firewall-cmd", "--help"]),
        ("konsole", vec!["-e", "firewall-cmd", "--help"]),
        ("xterm", vec!["-e", "firewall-cmd", "--help"]),
        ("terminator", vec!["-e", "firewall-cmd --help"]),
    ];

    for (terminal, args) in &terminal_commands {
        if Command::new("which")
            .arg(terminal)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            debug!("Opening terminal with firewall-cmd help: {}", terminal);
            let status = Command::new(terminal).args(args).status()?;

            if status.success() {
                debug!("Successfully launched terminal with firewall-cmd help");
                return Ok(());
            }
        }
    }

    Err("No firewall configuration tool found. Please install firewall-config, gufw, or a system settings app.".into())
}

/// Get firewalld cache directory
fn get_firewalld_cache_dir() -> Result<std::path::PathBuf, String> {
    let cache_dir = if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        std::path::PathBuf::from(dir)
    } else if let Some(dir) = std::env::var_os("HOME") {
        std::path::PathBuf::from(dir).join(".cache")
    } else {
        return Err("Cannot determine cache directory".to_string());
    };
    
    let firewalld_cache = cache_dir.join("network-dmenu").join("firewalld");
    if !firewalld_cache.exists() {
        std::fs::create_dir_all(&firewalld_cache).map_err(|e| e.to_string())?;
    }
    
    Ok(firewalld_cache)
}

/// Get cached firewalld data or refresh if expired (1 day)
pub async fn get_or_refresh_firewalld_cache() -> FirewalldCache {
    let cache_path = match get_firewalld_cache_dir() {
        Ok(dir) => dir.join("cache.json"),
        Err(_) => return refresh_firewalld_cache().await,
    };

    // Try to load existing cache
    if let Ok(cache_content) = fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<FirewalldCache>(&cache_content) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            // Cache valid for 1 day (86400 seconds)
            if now - cache.cached_at < 86400 {
                debug!("Using cached firewalld data");
                return cache;
            }
        }
    }

    debug!("Cache expired or missing, refreshing firewalld data");
    let fresh_cache = refresh_firewalld_cache().await;
    
    // Save fresh cache
    if let Ok(json) = serde_json::to_string_pretty(&fresh_cache) {
        let _ = fs::write(&cache_path, json);
    }
    
    fresh_cache
}

/// Refresh firewalld cache by calling firewall-cmd
async fn refresh_firewalld_cache() -> FirewalldCache {
    let mut cache = FirewalldCache {
        zones: Vec::new(),
        active_zones: HashMap::new(),
        panic_mode: false,
        cached_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    // Get zones: firewall-cmd --get-zones
    if let Ok(output) = TokioCommand::new("firewall-cmd")
        .arg("--get-zones")
        .output()
        .await
    {
        if output.status.success() {
            let zones_str = String::from_utf8_lossy(&output.stdout);
            cache.zones = zones_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            debug!("Cached {} zones", cache.zones.len());
        }
    }

    // Get active zones: firewall-cmd --get-active-zones
    if let Ok(output) = TokioCommand::new("firewall-cmd")
        .arg("--get-active-zones")
        .output()
        .await
    {
        if output.status.success() {
            let active_str = String::from_utf8_lossy(&output.stdout);
            cache.active_zones = parse_active_zones_output(&active_str);
            debug!("Cached {} active zones", cache.active_zones.len());
        }
    }

    // Get panic mode: firewall-cmd --query-panic
    if let Ok(output) = TokioCommand::new("firewall-cmd")
        .arg("--query-panic")
        .output()
        .await
    {
        // firewall-cmd returns 0 if panic mode is on, 1 if off
        cache.panic_mode = output.status.success();
        debug!("Panic mode: {}", cache.panic_mode);
    }

    cache
}

/// Parse active zones output like:
/// home (default)
///   interfaces: wlan0
/// public
///   interfaces: docker0
/// trusted
///   interfaces: tailscale0
fn parse_active_zones_output(output: &str) -> HashMap<String, Vec<String>> {
    let mut zones = HashMap::new();
    let mut current_zone: Option<String> = None;
    
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        if line.starts_with("interfaces:") {
            // Parse interfaces line
            if let Some(zone) = &current_zone {
                let interfaces: Vec<String> = line
                    .strip_prefix("interfaces:")
                    .unwrap_or("")
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                zones.insert(zone.clone(), interfaces);
            }
        } else {
            // This is a zone line, extract zone name
            let zone_name = if line.contains(" (default)") {
                line.replace(" (default)", "")
            } else {
                line.to_string()
            };
            current_zone = Some(zone_name);
        }
    }
    
    zones
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::process::{ExitStatus, Output};

    struct MockCommandRunner {
        should_succeed: bool,
        mock_output: String,
    }

    impl MockCommandRunner {
        fn new(should_succeed: bool, output: &str) -> Self {
            Self {
                should_succeed,
                mock_output: output.to_string(),
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, _program: &str, _args: &[&str]) -> Result<Output, std::io::Error> {
            use std::os::unix::process::ExitStatusExt;

            Ok(Output {
                status: if self.should_succeed {
                    ExitStatus::from_raw(0)
                } else {
                    ExitStatus::from_raw(1)
                },
                stdout: self.mock_output.as_bytes().to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn test_firewalld_action_display_strings() {
        let set_zone = FirewalldAction::SetZone("public".to_string());
        assert!(set_zone
            .to_display_string(None)
            .contains("Switch to zone: public"));

        // Test current zone display with checkmark
        assert!(set_zone
            .to_display_string(Some("public"))
            .contains("✅ Switch to zone: public (current)"));

        let panic_toggle = FirewalldAction::TogglePanicMode;
        assert!(panic_toggle.to_display_string(None).contains("Toggle panic mode"));

        let current_zone = FirewalldAction::GetCurrentZone;
        assert!(current_zone
            .to_display_string(None)
            .contains("Show current zone"));

        let config_editor = FirewalldAction::OpenConfigEditor;
        assert!(config_editor
            .to_display_string(None)
            .contains("Open firewall configuration"));
    }

    #[test]
    fn test_get_current_zone() {
        let mock_runner = MockCommandRunner::new(true, "public\n");
        let result = get_current_zone(&mock_runner);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "public");
    }

    #[test]
    fn test_get_current_zone_failure() {
        let mock_runner = MockCommandRunner::new(false, "");
        let result = get_current_zone(&mock_runner);

        assert!(result.is_err());
    }

    #[test]
    fn test_is_panic_mode_enabled() {
        let mock_runner_on = MockCommandRunner::new(true, "");
        assert_eq!(is_panic_mode_enabled(&mock_runner_on).unwrap(), true);

        let mock_runner_off = MockCommandRunner::new(false, "");
        assert_eq!(is_panic_mode_enabled(&mock_runner_off).unwrap(), false);
    }
}
