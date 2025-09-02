//! Network DMenu Library
//!
//! A Rust library for working with network interfaces through dmenu-style interfaces. Supports
//! Wi-Fi networks (via NetworkManager and iwd), VPN connections, and Bluetooth devices.

pub mod bluetooth;
pub mod command;
pub mod constants;
pub mod diagnostics;
pub mod dns_cache;
pub mod iwd;
pub mod logger;
pub mod networkmanager;
pub mod nextdns;
pub mod privilege;
pub mod rfkill;
pub mod tailscale;
pub mod tailscale_prefs;
pub mod utils;

use constants::{ICON_CHECK, ICON_CROSS, ICON_SIGNAL};

// Re-export commonly used types and functions
pub use bluetooth::{get_paired_bluetooth_devices, handle_bluetooth_action, BluetoothAction};
pub use command::{is_command_installed, read_output_lines, CommandRunner, RealCommandRunner};
pub use diagnostics::{
    diagnostic_action_to_string, get_diagnostic_actions, handle_diagnostic_action, DiagnosticAction,
};
pub use dns_cache::{
    generate_dns_actions_from_cache, get_current_network_id, CachedDnsServer, DnsBenchmarkCache,
    DnsCacheStorage,
};
pub use iwd::{
    connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks, is_iwd_connected,
    is_known_network as is_known_iwd_network,
};
pub use networkmanager::{
    connect_to_nm_vpn, connect_to_nm_wifi, disconnect_nm_vpn, disconnect_nm_wifi,
    get_nm_vpn_networks, get_nm_wifi_networks, is_known_network as is_known_nm_network,
    is_nm_connected,
};
pub use nextdns::{get_nextdns_actions, handle_nextdns_action, NextDnsAction};
pub use privilege::{
    get_privilege_command, has_privilege_escalation, wrap_privileged_command,
    wrap_privileged_commands,
};
pub use tailscale::{
    extract_short_hostname, get_locked_nodes, get_mullvad_actions, get_signing_key,
    handle_tailscale_action, is_exit_node_active, is_tailscale_lock_enabled, TailscaleAction,
};

// Re-export async functions
pub use nextdns::fetch_profiles_blocking;

// Re-export logger
pub use logger::Profiler;

pub use utils::{
    check_captive_portal, convert_network_strength, prompt_for_password, prompt_for_ssid,
};

use notify_rust::Notification;
use std::error::Error;

/// Enum representing various action types supported by the application
#[derive(Debug)]
pub enum ActionType {
    Bluetooth(BluetoothAction),
    Custom(CustomAction),
    Diagnostic(DiagnosticAction),
    NextDns(NextDnsAction),
    System(SystemAction),
    Tailscale(TailscaleAction),
    Vpn(VpnAction),
    Wifi(WifiAction),
}

/// Custom action configuration
#[derive(Debug)]
pub struct CustomAction {
    pub display: String,
    pub cmd: String,
}

/// System-level actions
#[derive(Debug)]
pub enum SystemAction {
    EditConnections,
    RfkillBlock,
    RfkillUnblock,
}

/// Wi-Fi related actions
#[derive(Debug)]
pub enum WifiAction {
    Connect,
    ConnectHidden,
    Disconnect,
    Network(String),
}

/// VPN related actions
#[derive(Debug)]
pub enum VpnAction {
    Connect(String),
    Disconnect(String),
}

/// Formats an entry for display in the menu
pub fn format_entry(action: &str, icon: &str, text: &str) -> String {
    if icon.is_empty() {
        format!("{:<10}- {}", action, text)
    } else {
        format!("{:<10}- {} {}", action, icon, text)
    }
}

/// Sends a notification about the connection.
pub fn notify_connection(summary: &str, name: &str) -> Result<(), Box<dyn Error>> {
    Notification::new()
        .summary(summary)
        .body(&format!("Connected to {name}"))
        .show()?;
    Ok(())
}

/// Parses a VPN action string to extract the connection name.
pub fn parse_vpn_action(action: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| {
            *c == ICON_CHECK.chars().next().unwrap() || *c == ICON_SIGNAL.chars().next().unwrap()
        })
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    let name_start = emoji_pos + action[emoji_pos..].chars().next().unwrap().len_utf8();
    let name = action[name_start..].trim();

    if name.is_empty() {
        return Err("No name found after emoji".into());
    }

    Ok(name)
}

/// Parses a Wi-Fi action string to extract the SSID and security type.
pub fn parse_wifi_action(action: &str) -> Result<(&str, &str), Box<dyn Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| {
            *c == ICON_CHECK.chars().next().unwrap()
                || *c == ICON_SIGNAL.chars().next().unwrap()
                || *c == ICON_CROSS.chars().next().unwrap()
        })
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    let tab_pos = action[emoji_pos..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + emoji_pos)
        .ok_or("Tab not found in action")?;

    let ssid_start = emoji_pos + action[emoji_pos..].chars().next().unwrap().len_utf8();
    let ssid = action[ssid_start..tab_pos].trim();

    let security_start = tab_pos + 1;
    let security_end = action[security_start..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + security_start)
        .unwrap_or(action.len());

    let security = action[security_start..security_end].trim();

    if ssid.is_empty() {
        return Err("No SSID found in action".into());
    }

    Ok((ssid, security))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::ICON_STAR;

    #[test]
    fn test_format_entry_integration() {
        let result = format_entry("test", ICON_STAR, "sample text");
        assert_eq!(result, "test      - 🌟 sample text");
    }

    #[test]
    fn test_action_type_creation() {
        let wifi_action = ActionType::Wifi(WifiAction::Connect);
        let bluetooth_action =
            ActionType::Bluetooth(BluetoothAction::ToggleConnect("device".to_string()));
        let tailscale_action = ActionType::Tailscale(TailscaleAction::SetEnable(true));

        // Just ensure they can be created without panicking
        match wifi_action {
            ActionType::Wifi(WifiAction::Connect) => (),
            _ => panic!("Unexpected action type"),
        }

        match bluetooth_action {
            ActionType::Bluetooth(_) => (),
            _ => panic!("Unexpected action type"),
        }

        match tailscale_action {
            ActionType::Tailscale(_) => (),
            _ => panic!("Unexpected action type"),
        }
    }
}
