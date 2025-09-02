mod bluetooth;
mod command;
mod constants;
mod diagnostics;
mod dns_cache;
mod iwd;
mod logger;
mod networkmanager;
mod nextdns;
mod privilege;
mod rfkill;
mod streaming;
mod tailscale;
mod tailscale_prefs;
mod utils;

#[macro_use]
extern crate log;
use crate::utils::get_flag;
use bluetooth::{get_connected_devices, handle_bluetooth_action, BluetoothAction};
use clap::Parser;
use command::{is_command_installed, CommandRunner, RealCommandRunner};
use constants::*;
use diagnostics::{diagnostic_action_to_string, handle_diagnostic_action, DiagnosticAction};
use dirs::config_dir;
use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi};
use log::error;
use networkmanager::{
    connect_to_nm_vpn, connect_to_nm_wifi, disconnect_nm_vpn, disconnect_nm_wifi,
};
use nextdns::handle_nextdns_action;
use nextdns::NextDnsAction;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tailscale::{
    check_mullvad, extract_short_hostname, get_locked_nodes, handle_tailscale_action,
    DefaultNotificationSender, TailscaleAction, TailscaleState,
};
use utils::check_captive_portal;

/// Command-line arguments structure for the application.
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "wlan0")]
    wifi_interface: String,
    #[arg(long)]
    no_wifi: bool,
    #[arg(long)]
    no_vpn: bool,
    #[arg(long)]
    no_bluetooth: bool,
    #[arg(long)]
    no_tailscale: bool,
    #[arg(long)]
    no_nextdns: bool,
    #[arg(
        long,
        default_value = "",
        help = "Your NextDNS API key from https://my.nextdns.io/account"
    )]
    nextdns_api_key: String,
    #[arg(long)]
    validate_nextdns_key: bool,
    #[arg(long)]
    refresh_nextdns_profiles: bool,
    #[arg(long)]
    no_diagnostics: bool,
    #[arg(long, help = "Enable profile timings and debug output")]
    profile: bool,
    #[arg(
        long,
        help = "Set log level (error, warn, info, debug, trace)",
        default_value = "warn"
    )]
    log_level: String,
    #[arg(
        long,
        help = "Limit the number of exit nodes shown per country (sorted by priority)"
    )]
    max_nodes_per_country: Option<i32>,
    #[arg(
        long,
        help = "Limit the number of exit nodes shown per city (sorted by priority)"
    )]
    max_nodes_per_city: Option<i32>,
    #[arg(
        long,
        help = "Filter Mullvad exit nodes by country name (e.g. 'USA', 'Japan')"
    )]
    country: Option<String>,
    #[arg(long, help = "Read dmenu selection from stdin (for testing)")]
    stdin: bool,
    #[arg(
        long,
        help = "Output actions to stdout instead of using dmenu (for debugging)"
    )]
    stdout: bool,
}

/// Configuration structure for the application.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Config {
    #[serde(default)]
    actions: Vec<CustomAction>,
    #[serde(default)]
    exclude_exit_node: Vec<String>,
    #[serde(default)]
    max_nodes_per_country: Option<i32>,
    #[serde(default)]
    max_nodes_per_city: Option<i32>,
    #[serde(default)]
    country_filter: Option<String>,
    #[serde(default = "default_true")]
    use_dns_cache: bool,
    #[serde(default)]
    nextdns_api_key: Option<String>,
    #[serde(default)]
    nextdns_toggle_profiles: Option<(String, String)>,
    dmenu_cmd: String,
    dmenu_args: String,
}

/// Custom action structure for user-defined actions.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct CustomAction {
    display: String,
    cmd: String,
}

/// Enum representing different types of actions that can be performed.
#[derive(Debug)]
enum ActionType {
    Bluetooth(BluetoothAction),
    Custom(CustomAction),
    Diagnostic(DiagnosticAction),
    NextDns(nextdns::NextDnsAction),
    System(SystemAction),
    Tailscale(TailscaleAction),
    Vpn(VpnAction),
    Wifi(WifiAction),
}

/// Enum representing system-related actions.
#[derive(Debug)]
enum SystemAction {
    EditConnections,
    RfkillBlock(String, String),   // (device_id, display_text)
    RfkillUnblock(String, String), // (device_id, display_text)
    AirplaneMode(bool),
}

/// Enum representing Wi-Fi-related actions.
#[derive(Debug)]
#[allow(dead_code)]
enum WifiAction {
    Connect,
    ConnectHidden,
    Disconnect,
    Network(String),
}

/// Enum representing VPN-related actions.
#[derive(Debug)]
enum VpnAction {
    Connect(String),
    Disconnect(String),
}

/// Formats an entry for display in the menu.
pub fn format_entry(action: &str, icon: &str, text: &str) -> String {
    if icon.is_empty() {
        format!("{action:<10}- {text}")
    } else {
        format!("{action:<10}- {icon} {text}")
    }
}

/// Helper function for serde default value
fn default_true() -> bool {
    true
}

/// Returns the default configuration as a string.
fn get_default_config() -> String {
    format!(
        r#"# General settings
dmenu_cmd = "{}"
dmenu_args = "{}"

# DNS cache feature (automatically use fastest DNS from benchmark)
# Set to false to disable cached DNS actions
use_dns_cache = true

# NextDNS configuration (no CLI required - uses API)
# Get your API key from: https://my.nextdns.io/account
# With API key, you can list and switch between all your profiles
# nextdns_api_key = "your-api-key-here"

# Quick toggle between two specific NextDNS profiles (optional)
# Works even without API key if you know the profile IDs
# Find your profile IDs at: https://my.nextdns.io
# Example: Home/Work switching
# nextdns_toggle_profiles = ["abc123", "xyz789"]

# Exit node filtering options
# List of exit nodes to exclude
# exclude_exit_node = ["exit1", "exit2"]

# Limit the number of exit nodes shown per country (sorted by priority)
# max_nodes_per_country = 2

# Limit the number of exit nodes shown per city (sorted by priority)
# max_nodes_per_city = 1

# Filter by country name (e.g., "USA", "Japan")
# country_filter = "USA"

[[actions]]
display = "🛡️ Example"
cmd = "notify-send 'hello' 'world'"
"#,
        DEFAULT_DMENU_CMD, DEFAULT_DMENU_ARGS
    )
}

/// Main function for the application.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Initialize logger with appropriate level
    if args.profile {
        std::env::set_var("RUST_LOG", "debug");
    } else {
        std::env::set_var("RUST_LOG", &args.log_level);
    }
    logger::init();

    // Start profiling total execution time
    let list_generation_profiler = logger::Profiler::new("Generated list");

    // Validate NextDNS API key if requested
    if args.validate_nextdns_key && !args.nextdns_api_key.is_empty() {
        println!("Validating NextDNS API key...");
        debug!(
            "Validating NextDNS API key (first 4 chars: {})",
            if args.nextdns_api_key.len() > 4 {
                &args.nextdns_api_key[0..4]
            } else {
                &args.nextdns_api_key
            }
        );

        let client = reqwest::Client::new();
        debug!("Sending request to NextDNS API...");
        let response = client
            .get("https://api.nextdns.io/profiles")
            .header("X-Api-Key", &args.nextdns_api_key)
            .send()
            .await?;

        let status = response.status();
        debug!("API response status: {}", status);
        if status.is_success() {
            let body = response.text().await?;
            debug!("API response body: {}", body);

            // Parse the profiles from the response
            let profiles_json: serde_json::Value = match serde_json::from_str(&body) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("WARNING: Could not parse API response as JSON: {}", e);
                    eprintln!("Raw response: {}", body);
                    println!("NextDNS API key is valid, but response could not be parsed!");
                    return Ok(());
                }
            };

            println!("NextDNS API key is valid!");

            // Extract and display profiles
            let profiles_arr = if let Some(data) = profiles_json.get("data") {
                // New format - object with a "data" array
                data.as_array()
            } else if let serde_json::Value::Array(arr) = &profiles_json {
                // Old format - direct array
                Some(arr)
            } else {
                None
            };

            if let Some(arr) = profiles_arr {
                println!("Found {} NextDNS profiles:", arr.len());
                for profile in arr {
                    let id = profile
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let name = profile
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unnamed");
                    println!(" - {} ({})", name, id);
                }
            } else {
                println!("Warning: API response doesn't contain profiles in a recognized format.");
                debug!("Response format: {:?}", profiles_json);
            }
            return Ok(());
        } else {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Could not read error response".to_string());
            debug!("API error response: {}", error_body);
            eprintln!("Invalid NextDNS API key: {}", status);
            return Err("Invalid NextDNS API key".into());
        }
    }

    // Handle NextDNS profile refresh request
    if args.refresh_nextdns_profiles {
        println!("Refreshing NextDNS profiles...");
        let api_key = if !args.nextdns_api_key.is_empty() {
            args.nextdns_api_key.clone()
        } else {
            get_config()
                .ok()
                .and_then(|c| c.nextdns_api_key.clone())
                .map(|k| k.trim().to_string())
                .unwrap_or_default()
        };

        if api_key.is_empty() {
            error!("NextDNS API key required for profile refresh");
            error!("Provide with --nextdns-api-key or in config.toml");
            return Err("Missing NextDNS API key".into());
        }

        debug!(
            "Using API key (first 4 chars): {}",
            if api_key.len() > 4 {
                &api_key[0..4]
            } else {
                &api_key
            }
        );
        let result = nextdns::fetch_profiles_blocking(&api_key).await?;
        println!("Successfully refreshed {} NextDNS profiles:", result.len());
        for profile in result {
            println!(
                " - {} ({})",
                profile.name.unwrap_or_else(|| "Unnamed".to_string()),
                profile.id
            );
        }
        return Ok(());
    }

    create_default_config_if_missing()?;

    let config = get_config()?; // Load the configuration once

    check_required_commands(&config)?;

    // Performance optimization: We're using a more efficient approach for network scanning
    // that prioritizes faster operations first to improve perceived responsiveness
    let command_runner = RealCommandRunner;

    // Use streaming approach for better responsiveness
    let (action, actions) = streaming::select_action_from_menu_streaming(
        &config,
        &args,
        &command_runner,
        args.stdin,
        args.stdout,
    )
    .await?;

    // Display profiling information if enabled
    // Log the total execution time
    list_generation_profiler.log();

    if args.profile {
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!(
                "Generated list in {:.2?}",
                list_generation_profiler.elapsed()
            ))
            .show();
    }

    if !action.is_empty() {
        let selected_action = find_selected_action(&action, &actions)?;
        let connected_devices = get_connected_devices(&command_runner)?;

        set_action(
            &args.wifi_interface,
            selected_action,
            &connected_devices,
            &command_runner,
            args.profile,
        )
        .await?;
    }
    // When action is empty (user pressed Escape or closed window), just exit silently

    Ok(())
}

/// Checks if required commands are installed.
fn check_required_commands(_config: &Config) -> Result<(), Box<dyn Error>> {
    if !is_command_installed("pinentry-gnome3") {
        eprintln!("Warning: pinentry-gnome3 command missing");
    }

    Ok(())
}

/// Selects an action from the menu using dmenu
fn action_to_string(action: &ActionType) -> String {
    match action {
        ActionType::Custom(custom_action) => {
            format_entry(ACTION_TYPE_ACTION, "", &custom_action.display)
        }
        ActionType::System(system_action) => match system_action {
            SystemAction::RfkillBlock(_, display_text) => display_text.clone(),
            SystemAction::RfkillUnblock(_, display_text) => display_text.clone(),
            SystemAction::EditConnections => {
                format_entry(ACTION_TYPE_SYSTEM, ICON_SIGNAL, SYSTEM_EDIT_CONNECTIONS)
            }
            SystemAction::AirplaneMode(enable) => {
                if *enable {
                    format_entry(ACTION_TYPE_SYSTEM, ICON_CROSS, SYSTEM_AIRPLANE_MODE_ON)
                } else {
                    format_entry(ACTION_TYPE_SYSTEM, ICON_SIGNAL, SYSTEM_AIRPLANE_MODE_OFF)
                }
            }
        },
        ActionType::Tailscale(mullvad_action) => match mullvad_action {
            TailscaleAction::SetExitNode(node) => node.to_string(),
            TailscaleAction::DisableExitNode => format_entry(
                ACTION_TYPE_TAILSCALE,
                ICON_CROSS,
                TAILSCALE_DISABLE_EXIT_NODE,
            ),
            TailscaleAction::SetEnable(enable) => format_entry(
                ACTION_TYPE_TAILSCALE,
                if *enable { ICON_CHECK } else { ICON_CROSS },
                if *enable {
                    TAILSCALE_ENABLE
                } else {
                    TAILSCALE_DISABLE
                },
            ),
            TailscaleAction::SetShields(enable) => {
                let (icon, text) = if *enable {
                    (ICON_FIREWALL_BLOCK, TAILSCALE_SHIELDS_UP)
                } else {
                    (ICON_FIREWALL_ALLOW, TAILSCALE_SHIELDS_DOWN)
                };
                format_entry(ACTION_TYPE_TAILSCALE, icon, text)
            }
            TailscaleAction::SetAcceptRoutes(enable) => {
                let text = if *enable {
                    TAILSCALE_ALLOW_ADVERTISE_ROUTES
                } else {
                    TAILSCALE_DISALLOW_ADVERTISE_ROUTES
                };
                format_entry(
                    ACTION_TYPE_TAILSCALE,
                    if *enable { ICON_CHECK } else { ICON_CROSS },
                    text,
                )
            }
            TailscaleAction::SetAllowLanAccess(enable) => {
                let text = if *enable {
                    TAILSCALE_ALLOW_LAN_ACCESS_EXIT_NODE
                } else {
                    TAILSCALE_DISALLOW_LAN_ACCESS_EXIT_NODE
                };
                format_entry(
                    ACTION_TYPE_TAILSCALE,
                    if *enable { ICON_CHECK } else { ICON_CROSS },
                    text,
                )
            }
            TailscaleAction::ShowLockStatus => {
                format_entry(ACTION_TYPE_TAILSCALE, ICON_LOCK, TAILSCALE_SHOW_LOCK_STATUS)
            }
            TailscaleAction::ListLockedNodes => format_entry(
                ACTION_TYPE_TAILSCALE,
                ICON_LIST,
                TAILSCALE_LIST_LOCKED_NODES,
            ),
            TailscaleAction::SignAllNodes => {
                let state = TailscaleState::new(&RealCommandRunner);
                if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner, Some(&state)) {
                    let count = locked_nodes.len();
                    if count > 0 {
                        format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_KEY,
                            &format!("Sign All Locked Nodes ({count})"),
                        )
                    } else {
                        format_entry(ACTION_TYPE_TAILSCALE, ICON_KEY, "Sign All Locked Nodes")
                    }
                } else {
                    format_entry(ACTION_TYPE_TAILSCALE, ICON_KEY, "Sign All Locked Nodes")
                }
            }
            TailscaleAction::SignLockedNode(node_key) => {
                // Try to find the hostname for this node key from locked nodes
                let state = TailscaleState::new(&RealCommandRunner);
                if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner, Some(&state)) {
                    if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                        let flag = get_flag(&node.country_code);
                        format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_KEY,
                            &TAILSCALE_SIGN_NODE_DETAILED
                                .replace("{flag}", &flag)
                                .replace("{hostname}", extract_short_hostname(&node.hostname))
                                .replace("{key}", &node_key[..8]),
                        )
                    } else {
                        format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_KEY,
                            &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                        )
                    }
                } else {
                    format_entry(
                        ACTION_TYPE_TAILSCALE,
                        ICON_KEY,
                        &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                    )
                }
            }
        },
        ActionType::Vpn(vpn_action) => match vpn_action {
            VpnAction::Connect(network) => format_entry(ACTION_TYPE_VPN, "", network),
            VpnAction::Disconnect(network) => format_entry(ACTION_TYPE_VPN, ICON_CROSS, network),
        },
        ActionType::Wifi(wifi_action) => match wifi_action {
            WifiAction::Network(network) => format_entry(ACTION_TYPE_WIFI, "", network),
            WifiAction::Disconnect => format_entry(ACTION_TYPE_WIFI, ICON_CROSS, WIFI_DISCONNECT),
            WifiAction::Connect => format_entry(ACTION_TYPE_WIFI, ICON_SIGNAL, WIFI_CONNECT),
            WifiAction::ConnectHidden => {
                format_entry(ACTION_TYPE_WIFI, ICON_SIGNAL, WIFI_CONNECT_HIDDEN)
            }
        },
        ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
            BluetoothAction::ToggleConnect(device) => device.to_string(),
        },
        ActionType::Diagnostic(diagnostic_action) => diagnostic_action_to_string(diagnostic_action),
        ActionType::NextDns(nextdns_action) => {
            format_entry(ACTION_TYPE_NEXTDNS, "", &nextdns_action.to_string())
        }
    }
}

/// Finds the selected action from the action list.
fn find_selected_action<'a>(
    action: &str,
    actions: &'a [ActionType],
) -> Result<&'a ActionType, Box<dyn Error>> {
    actions
        .iter()
        .find(|a| match a {
            ActionType::Custom(custom_action) => {
                format_entry(ACTION_TYPE_ACTION, "", &custom_action.display) == action
            }
            ActionType::System(system_action) => match system_action {
                SystemAction::RfkillBlock(_, display_text) => action == display_text,
                SystemAction::RfkillUnblock(_, display_text) => action == display_text,
                SystemAction::EditConnections => {
                    action == format_entry(ACTION_TYPE_SYSTEM, ICON_SIGNAL, SYSTEM_EDIT_CONNECTIONS)
                }
                SystemAction::AirplaneMode(enable) => {
                    if *enable {
                        action
                            == format_entry(ACTION_TYPE_SYSTEM, ICON_CROSS, SYSTEM_AIRPLANE_MODE_ON)
                    } else {
                        action
                            == format_entry(
                                ACTION_TYPE_SYSTEM,
                                ICON_SIGNAL,
                                SYSTEM_AIRPLANE_MODE_OFF,
                            )
                    }
                }
            },
            ActionType::Tailscale(mullvad_action) => match mullvad_action {
                TailscaleAction::SetExitNode(node) => action == node,
                TailscaleAction::DisableExitNode => {
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_CROSS,
                            TAILSCALE_DISABLE_EXIT_NODE,
                        )
                }
                TailscaleAction::SetEnable(enable) => {
                    let text = if *enable {
                        TAILSCALE_ENABLE
                    } else {
                        TAILSCALE_DISABLE
                    };
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            if *enable { ICON_CHECK } else { ICON_CROSS },
                            text,
                        )
                }
                TailscaleAction::SetShields(enable) => {
                    let (icon, text) = if *enable {
                        (ICON_FIREWALL_BLOCK, TAILSCALE_SHIELDS_UP)
                    } else {
                        (ICON_FIREWALL_ALLOW, TAILSCALE_SHIELDS_DOWN)
                    };
                    action == format_entry(ACTION_TYPE_TAILSCALE, icon, text)
                }
                TailscaleAction::SetAcceptRoutes(enable) => {
                    let text = if *enable {
                        TAILSCALE_ALLOW_ADVERTISE_ROUTES
                    } else {
                        TAILSCALE_DISALLOW_ADVERTISE_ROUTES
                    };
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            if *enable { ICON_CHECK } else { ICON_CROSS },
                            text,
                        )
                }
                TailscaleAction::SetAllowLanAccess(enable) => {
                    let text = if *enable {
                        TAILSCALE_ALLOW_LAN_ACCESS_EXIT_NODE
                    } else {
                        TAILSCALE_DISALLOW_LAN_ACCESS_EXIT_NODE
                    };
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            if *enable { ICON_CHECK } else { ICON_CROSS },
                            text,
                        )
                }
                TailscaleAction::ShowLockStatus => {
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_LOCK,
                            TAILSCALE_SHOW_LOCK_STATUS,
                        )
                }
                TailscaleAction::ListLockedNodes => {
                    action
                        == format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_LIST,
                            TAILSCALE_LIST_LOCKED_NODES,
                        )
                }
                TailscaleAction::SignAllNodes => {
                    // Match any "Sign All Locked Nodes" action, regardless of count
                    action.starts_with(&format!(
                        "{ACTION_TYPE_TAILSCALE:<10}- {ICON_KEY} Sign All Locked Nodes"
                    ))
                }
                TailscaleAction::SignLockedNode(node_key) => {
                    // Try to find the hostname for this node key from locked nodes
                    let state = TailscaleState::new(&RealCommandRunner);
                    if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner, Some(&state)) {
                        if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                            action
                                == format_entry(
                                    ACTION_TYPE_TAILSCALE,
                                    ICON_KEY,
                                    &TAILSCALE_SIGN_NODE_DETAILED
                                        .replace("{flag}", &get_flag(&node.country_code))
                                        .replace(
                                            "{hostname}",
                                            extract_short_hostname(&node.hostname),
                                        )
                                        .replace("{key}", &node_key[..8]),
                                )
                        } else {
                            action
                                == format_entry(
                                    ACTION_TYPE_TAILSCALE,
                                    ICON_KEY,
                                    &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                                )
                        }
                    } else {
                        action
                            == format_entry(
                                ACTION_TYPE_TAILSCALE,
                                ICON_KEY,
                                &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                            )
                    }
                }
            },
            ActionType::Vpn(vpn_action) => match vpn_action {
                VpnAction::Connect(network) => action == format_entry(ACTION_TYPE_VPN, "", network),
                VpnAction::Disconnect(network) => {
                    action == format_entry(ACTION_TYPE_VPN, ICON_CROSS, network)
                }
            },
            ActionType::Wifi(wifi_action) => match wifi_action {
                WifiAction::Network(network) => {
                    action == format_entry(ACTION_TYPE_WIFI, "", network)
                }
                WifiAction::Disconnect => {
                    action == format_entry(ACTION_TYPE_WIFI, ICON_CROSS, WIFI_DISCONNECT)
                }
                WifiAction::Connect => {
                    action == format_entry(ACTION_TYPE_WIFI, ICON_SIGNAL, WIFI_CONNECT)
                }
                WifiAction::ConnectHidden => {
                    action == format_entry(ACTION_TYPE_WIFI, ICON_SIGNAL, WIFI_CONNECT_HIDDEN)
                }
            },
            ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                BluetoothAction::ToggleConnect(device) => action == device,
            },
            ActionType::Diagnostic(diagnostic_action) => {
                action == diagnostic_action_to_string(diagnostic_action)
            }
            ActionType::NextDns(nextdns_action) => {
                action == format_entry(ACTION_TYPE_NEXTDNS, "", &nextdns_action.to_string())
            }
        })
        .ok_or(format!("Action not found: {action}").into())
}

/// Gets the configuration file path.
fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let config_dir = config_dir().ok_or(ERROR_CONFIG_READ)?;
    Ok(config_dir.join(CONFIG_DIR_NAME).join(CONFIG_FILENAME))
}

/// Creates a default configuration file if it doesn't exist.
fn create_default_config_if_missing() -> Result<(), Box<dyn Error>> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&config_path, get_default_config())?;
    }
    Ok(())
}

/// Reads and returns the configuration.
fn get_config() -> Result<Config, Box<dyn Error>> {
    let config_path = get_config_path()?;
    let config_content = fs::read_to_string(config_path)?;
    let config = toml::from_str(&config_content)?;
    Ok(config)
}

/// Handles a custom action by executing its command.
fn handle_custom_action(action: &CustomAction) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("sh").arg("-c").arg(&action.cmd).status()?;
    Ok(status.success())
}

/// Handles a system action.
async fn handle_system_action(
    action: &SystemAction,
    profile: bool,
) -> Result<bool, Box<dyn Error>> {
    // Helper function to handle rfkill block/unblock operations
    async fn handle_rfkill_operation(
        device: &str,
        block: bool,
        profile: bool,
    ) -> Result<bool, Box<dyn Error>> {
        // Check if this is a device ID or device type
        let rfkill_start = if profile {
            Some(std::time::Instant::now())
        } else {
            None
        };

        if let Ok(id) = device.parse::<u32>() {
            if block {
                rfkill::block_device(id).await?;
            } else {
                rfkill::unblock_device(id).await?;
            }
        } else if block {
            rfkill::block_device_type(device).await?;
        } else {
            rfkill::unblock_device_type(device).await?;
        };

        if let Some(start) = rfkill_start {
            let operation = if block { "block" } else { "unblock" };
            let elapsed = start.elapsed();
            eprintln!(
                ">>> PROFILE: Rfkill {} {} took: {:.2?}",
                operation, device, elapsed
            );
            if profile {
                let _ = Notification::new()
                    .summary("Network-dmenu Profiling")
                    .body(&format!(
                        "Rfkill {} {} took: {:.2?}",
                        operation, device, elapsed
                    ))
                    .show();
            }
        }
        Ok(true)
    }

    let start_time = if profile {
        Some(std::time::Instant::now())
    } else {
        None
    };

    let result = match action {
        SystemAction::RfkillBlock(device_id, _) => {
            handle_rfkill_operation(device_id, true, profile).await
        }
        SystemAction::RfkillUnblock(device_id, _) => {
            handle_rfkill_operation(device_id, false, profile).await
        }
        SystemAction::EditConnections => {
            let status = Command::new("nm-connection-editor").status()?;
            Ok(status.success())
        }
        SystemAction::AirplaneMode(enable) => {
            if *enable {
                // Block all radio devices (wifi, bluetooth, etc.)
                rfkill::block_device_type("all").await?;
                // Notify user
                let _ = Notification::new()
                    .summary("Airplane Mode Enabled")
                    .body("All wireless devices have been turned off")
                    .show();
            } else {
                // Unblock all radio devices
                rfkill::unblock_device_type("all").await?;
                // Notify user
                let _ = Notification::new()
                    .summary("Airplane Mode Disabled")
                    .body("Wireless devices have been turned back on")
                    .show();
            }
            Ok(true)
        }
    };

    // Display profiling information if enabled
    if let Some(start) = start_time {
        let action_name = match action {
            SystemAction::RfkillBlock(device_id, _) => format!("Block {}", device_id),
            SystemAction::RfkillUnblock(device_id, _) => format!("Unblock {}", device_id),
            SystemAction::EditConnections => "Edit connections".to_string(),
            SystemAction::AirplaneMode(enable) => {
                format!("Airplane mode {}", if *enable { "ON" } else { "OFF" })
            }
        };
        let elapsed = start.elapsed();
        eprintln!(
            ">>> PROFILE: System action '{}' took: {:.2?}",
            action_name, elapsed
        );
        if profile {
            let _ = Notification::new()
                .summary("Network-dmenu Profiling")
                .body(&format!(
                    "System action '{}' took: {:.2?}",
                    action_name, elapsed
                ))
                .show();
        }
    }

    result
}

/// Parses a VPN action string to extract the connection name.
pub fn parse_vpn_action(action: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    // Use unwrap_or to handle cases where there might not be a next character
    let first_char = action[emoji_pos..].chars().next().unwrap_or(' ');
    let name_start = emoji_pos + first_char.len_utf8();
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
        .find(|(_, c)| *c == '✅' || *c == '📶' || *c == '❌')
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    let tab_pos = action[emoji_pos..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + emoji_pos)
        .ok_or("Tab character not found in action")?;

    let ssid = action[emoji_pos + 4..tab_pos].trim();
    let parts: Vec<&str> = action[tab_pos + 1..].split('\t').collect();
    if parts.len() < 2 {
        return Err("Action format is incorrect".into());
    }
    let security = parts[0].trim();
    Ok((ssid, security))
}

/// Handles a VPN action, such as connecting or disconnecting.
async fn handle_vpn_action(
    action: &VpnAction,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        VpnAction::Connect(network) => {
            if is_command_installed("nmcli") {
                connect_to_nm_vpn(network, command_runner)?;
            }

            // Check mullvad status, assert errors in debug mode
            // Ignore errors from mullvad check, but log in debug mode
            let _e = check_mullvad().await;
            #[cfg(debug_assertions)]
            if let Err(ref e) = _e {
                eprintln!("Failed to check mullvad status: {}", e);
            }

            Ok(true)
        }
        VpnAction::Disconnect(network) => {
            let status = if is_command_installed("nmcli") {
                disconnect_nm_vpn(network, command_runner)?
            } else {
                true
            };
            Ok(status)
        }
    }
}

/// Handles a Wi-Fi action, such as connecting or disconnecting.
async fn handle_wifi_action(
    action: &WifiAction,
    wifi_interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        WifiAction::Disconnect => {
            let status = if is_command_installed("nmcli") {
                disconnect_nm_wifi(wifi_interface, command_runner)?
            } else {
                disconnect_iwd_wifi(wifi_interface, command_runner)?
            };
            Ok(status)
        }
        WifiAction::Connect => {
            let status = Command::new("nmcli")
                .arg("device")
                .arg("connect")
                .arg(wifi_interface)
                .status()?;

            // Only check for captive portal if connection was successful
            if status.success() {
                // Check for captive portal, log errors in debug mode
                let _e = check_captive_portal().await;
                #[cfg(debug_assertions)]
                if let Err(ref e) = _e {
                    eprintln!("Failed to check captive portal: {}", e);
                }
            }

            Ok(status.success())
        }
        WifiAction::ConnectHidden => {
            let ssid = utils::prompt_for_ssid()?;
            let network = format_entry("wifi", ICON_SIGNAL, &format!("{ssid}\tUNKNOWN\t"));
            // FIXME: nmcli connect hidden network looks buggy
            // so we will use iwd directly for the moment
            let connection_result = if is_command_installed("iwctl") {
                let result = connect_to_iwd_wifi(wifi_interface, &network, true, command_runner)?;
                if result {
                    // Check for captive portal, log errors in debug mode
                    let _e = check_captive_portal().await;
                    #[cfg(debug_assertions)]
                    if let Err(ref e) = _e {
                        eprintln!("Failed to check captive portal: {}", e);
                    }
                }
                result
            } else {
                false
            };

            Ok(connection_result)
        }
        WifiAction::Network(network) => {
            let connection_result = if is_command_installed("nmcli") {
                // For NetworkManager, we ensure connection is complete before checking captive portal
                let result = connect_to_nm_wifi(network, false, command_runner)?;
                // Only check for captive portal if connection was successful
                if result {
                    // Check for captive portal, log errors in debug mode
                    let _e = check_captive_portal().await;
                    #[cfg(debug_assertions)]
                    if let Err(ref e) = _e {
                        eprintln!("Failed to check captive portal: {}", e);
                    }
                }
                result
            } else if is_command_installed("iwctl") {
                let result = connect_to_iwd_wifi(wifi_interface, network, false, command_runner)?;
                // For IWD, we check after connection attempt
                let _e = check_captive_portal().await;
                #[cfg(debug_assertions)]
                if let Err(ref e) = _e {
                    eprintln!("Failed to check captive portal: {}", e);
                }
                result
            } else {
                false
            };

            // Check mullvad status, log errors in debug mode
            let _e = check_mullvad().await;
            #[cfg(debug_assertions)]
            if let Err(ref e) = _e {
                eprintln!("Failed to check mullvad status: {}", e);
            }

            Ok(connection_result)
        }
    }
}

/// Sets and handles the selected action.
async fn set_action(
    wifi_interface: &str,
    action: &ActionType,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
    profile: bool,
) -> Result<bool, Box<dyn Error>> {
    match action {
        ActionType::Custom(custom_action) => handle_custom_action(custom_action),
        ActionType::NextDns(nextdns_action) => {
            // First check for command-line API key
            let args = Args::parse();
            let api_key = if !args.nextdns_api_key.is_empty() {
                let trimmed_key = args.nextdns_api_key.trim().to_string();
                debug!(
                    "Using NextDNS API key from command line in set_action (first 4 chars: {})",
                    if trimmed_key.len() > 4 {
                        &trimmed_key[0..4]
                    } else {
                        &trimmed_key
                    }
                );
                Some(trimmed_key)
            } else {
                // Fall back to config file API key
                debug!("Command line API key is empty in set_action, checking config file");
                let key_opt = get_config().ok().and_then(|c| c.nextdns_api_key.clone());
                if let Some(key) = key_opt {
                    let trimmed_key = key.trim().to_string();
                    if !trimmed_key.is_empty() {
                        debug!("Using NextDNS API key from config file in set_action (first 4 chars: {})",
                                 if trimmed_key.len() > 4 { &trimmed_key[0..4] } else { &trimmed_key });
                        Some(trimmed_key)
                    } else {
                        debug!("Empty NextDNS API key found in config file in set_action");
                        None
                    }
                } else {
                    debug!("No NextDNS API key found in config file in set_action");
                    None
                }
            };

            // Convert Option<String> to Option<&str> for the handler
            let api_key_ref = api_key.as_deref();
            debug!(
                "Passing API key to handle_nextdns_action: {}",
                if let Some(key) = api_key_ref {
                    if key.len() > 4 {
                        &key[0..4]
                    } else {
                        key
                    }
                } else {
                    "None"
                }
            );

            if api_key.is_none() && matches!(nextdns_action, &NextDnsAction::RefreshProfiles) {
                error!("NextDNS API key required for this operation");
                error!("Provide with --nextdns-api-key or in config.toml");
                return Ok(false);
            }

            debug!("Action being handled: {:?}", nextdns_action);
            let result = handle_nextdns_action(nextdns_action, command_runner, api_key_ref).await;
            debug!("handle_nextdns_action result: {:?}", result);
            result
        }
        ActionType::System(system_action) => handle_system_action(system_action, profile).await,
        ActionType::Tailscale(mullvad_action) => {
            let notification_sender = DefaultNotificationSender;
            let tailscale_state = TailscaleState::new(command_runner);
            handle_tailscale_action(
                mullvad_action,
                command_runner,
                Some(&notification_sender),
                Some(&tailscale_state),
            )
            .await
        }
        ActionType::Vpn(vpn_action) => handle_vpn_action(vpn_action, command_runner).await,
        ActionType::Wifi(wifi_action) => {
            handle_wifi_action(wifi_action, wifi_interface, command_runner).await
        }
        ActionType::Bluetooth(bluetooth_action) => {
            handle_bluetooth_action(bluetooth_action, connected_devices, command_runner)
        }
        ActionType::Diagnostic(diagnostic_action) => {
            let result = handle_diagnostic_action(diagnostic_action, command_runner).await?;
            // Show the result in a notification
            let summary = if result.success {
                "Diagnostic Complete"
            } else {
                "Diagnostic Failed"
            };
            let _ = Notification::new()
                .summary(summary)
                .body(&result.output)
                .show();
            Ok(result.success)
        }
    }
}

/// Sends a notification about the connection.
pub fn notify_connection(summary: &str, name: &str) -> Result<(), Box<dyn Error>> {
    let _e = Notification::new()
        .summary(summary)
        .body(&format!("Connected to {name}"))
        .show();

    #[cfg(debug_assertions)]
    if let Err(ref e) = _e {
        eprintln!("Failed to show notification: {}", e);
    }

    // We don't want to propagate notification errors to the caller
    // as notifications are not critical for functionality
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_entry_with_icon() {
        let result = format_entry("wifi", "📶", "Connect to network");
        assert_eq!(result, "wifi      - 📶 Connect to network");
    }

    #[test]
    fn test_format_entry_without_icon() {
        let result = format_entry("system", "", "Edit connections");
        assert_eq!(result, "system    - Edit connections");
    }

    #[test]
    fn test_format_entry_empty_text() {
        let result = format_entry("test", "🔥", "");
        assert_eq!(result, "test      - 🔥 ");
    }

    #[test]
    fn test_format_entry_long_action() {
        let result = format_entry("verylongaction", "🌟", "Some text");
        assert_eq!(result, "verylongaction- 🌟 Some text");
    }

    #[test]
    fn test_get_default_config() {
        let config = get_default_config();
        assert!(config.contains("dmenu_cmd = \"dmenu\""));
        assert!(config.contains("dmenu_args = \"--no-multi\""));
        assert!(config.contains("exclude_exit_node = [\"exit1\", \"exit2\"]"));
        assert!(config.contains("[[actions]]"));
        assert!(config.contains("display = \"🛡️ Example\""));
        assert!(config.contains("cmd = \"notify-send 'hello' 'world'\""));
    }

    #[test]
    fn test_action_to_string_bluetooth() {
        let action =
            ActionType::Bluetooth(BluetoothAction::ToggleConnect("Device Name".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "Device Name");
    }

    #[test]
    fn test_action_to_string_custom() {
        let custom_action = CustomAction {
            display: "Custom Action".to_string(),
            cmd: "echo test".to_string(),
        };
        let action = ActionType::Custom(custom_action);
        let result = action_to_string(&action);
        assert_eq!(result, "action    - Custom Action");
    }

    #[test]
    fn test_action_to_string_system_rfkill_block() {
        let display_text =
            format_entry(ACTION_TYPE_SYSTEM, ICON_CROSS, "Turn OFF all WiFi devices");
        let action = ActionType::System(SystemAction::RfkillBlock(
            "wlan".to_string(),
            display_text.clone(),
        ));
        let result = action_to_string(&action);
        assert_eq!(result, display_text);
    }

    #[test]
    fn test_action_to_string_system_rfkill_unblock() {
        let display_text =
            format_entry(ACTION_TYPE_SYSTEM, ICON_SIGNAL, "Turn ON all WiFi devices");
        let action = ActionType::System(SystemAction::RfkillUnblock(
            "wlan".to_string(),
            display_text.clone(),
        ));
        let result = action_to_string(&action);
        assert_eq!(result, display_text);
    }

    #[test]
    fn test_action_to_string_system_edit_connections() {
        let action = ActionType::System(SystemAction::EditConnections);
        let result = action_to_string(&action);
        assert_eq!(result, "system    - 📶 Edit connections");
    }

    #[test]
    fn test_action_to_string_system_airplane_mode_on() {
        let action = ActionType::System(SystemAction::AirplaneMode(true));
        let result = action_to_string(&action);
        assert_eq!(result, "system    - ❌ Turn ON airplane mode");
    }

    #[test]
    fn test_action_to_string_system_airplane_mode_off() {
        let action = ActionType::System(SystemAction::AirplaneMode(false));
        let result = action_to_string(&action);
        assert_eq!(result, "system    - 📶 Turn OFF airplane mode");
    }

    #[test]
    fn test_action_to_string_tailscale_set_exit_node() {
        let action =
            ActionType::Tailscale(TailscaleAction::SetExitNode("exit-node-name".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "exit-node-name");
    }

    #[test]
    fn test_action_to_string_tailscale_disable_exit_node() {
        let action = ActionType::Tailscale(TailscaleAction::DisableExitNode);
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - ❌ Disable exit-node");
    }

    #[test]
    fn test_action_to_string_tailscale_enable() {
        let action = ActionType::Tailscale(TailscaleAction::SetEnable(true));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - ✅ Enable tailscale");
    }

    #[test]
    fn test_action_to_string_tailscale_disable() {
        let action = ActionType::Tailscale(TailscaleAction::SetEnable(false));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - ❌ Disable tailscale");
    }

    #[test]
    fn test_action_to_string_tailscale_shields_up() {
        let action = ActionType::Tailscale(TailscaleAction::SetShields(true));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 🚫 Block incoming connections");
    }

    #[test]
    fn test_action_to_string_tailscale_shields_down() {
        let action = ActionType::Tailscale(TailscaleAction::SetShields(false));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 🔓 Allow incoming connections");
    }

    #[test]
    fn test_action_to_string_vpn_connect() {
        let action = ActionType::Vpn(VpnAction::Connect("VPN Network".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "vpn       - VPN Network");
    }

    #[test]
    fn test_action_to_string_vpn_disconnect() {
        let action = ActionType::Vpn(VpnAction::Disconnect("VPN Network".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "vpn       - ❌ VPN Network");
    }

    #[test]
    fn test_action_to_string_wifi_network() {
        let action = ActionType::Wifi(WifiAction::Network("WiFi Network".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "wifi      - WiFi Network");
    }

    #[test]
    fn test_action_to_string_wifi_disconnect() {
        let action = ActionType::Wifi(WifiAction::Disconnect);
        let result = action_to_string(&action);
        assert_eq!(result, "wifi      - ❌ Disconnect");
    }

    #[test]
    fn test_action_to_string_wifi_connect() {
        let action = ActionType::Wifi(WifiAction::Connect);
        let result = action_to_string(&action);
        assert_eq!(result, "wifi      - 📶 Connect");
    }

    #[test]
    fn test_action_to_string_wifi_connect_hidden() {
        let action = ActionType::Wifi(WifiAction::ConnectHidden);
        let result = action_to_string(&action);
        assert_eq!(result, "wifi      - 📶 Connect to hidden network");
    }

    #[test]
    fn test_find_selected_action_success() {
        let actions = vec![
            ActionType::Wifi(WifiAction::Connect),
            ActionType::System(SystemAction::RfkillBlock(
                "wlan".to_string(),
                "system    - ❌ Turn OFF all WiFi devices".to_string(),
            )),
        ];

        let result = find_selected_action("wifi      - 📶 Connect", &actions);
        assert!(result.is_ok());

        match result.unwrap() {
            ActionType::Wifi(WifiAction::Connect) => (),
            _ => panic!("Expected WiFi Connect action"),
        }
    }

    #[test]
    fn test_find_selected_action_not_found() {
        let actions = vec![
            ActionType::Wifi(WifiAction::Connect),
            ActionType::System(SystemAction::RfkillBlock(
                "wlan".to_string(),
                "system    - ❌ Turn OFF all WiFi devices".to_string(),
            )),
        ];

        let result = find_selected_action("nonexistent action", &actions);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Action not found"));
    }

    #[test]
    fn test_find_selected_action_airplane_mode() {
        let actions = vec![
            ActionType::Wifi(WifiAction::Connect),
            ActionType::System(SystemAction::AirplaneMode(true)),
            ActionType::System(SystemAction::AirplaneMode(false)),
        ];

        // Test finding enable airplane mode
        let result_on = find_selected_action("system    - ❌ Turn ON airplane mode", &actions);
        assert!(result_on.is_ok());
        match result_on.unwrap() {
            ActionType::System(SystemAction::AirplaneMode(enable)) => {
                assert!(*enable, "Expected airplane mode to be enabled");
            }
            _ => panic!("Expected SystemAction::AirplaneMode(true)"),
        }

        // Test finding disable airplane mode
        let result_off = find_selected_action("system    - 📶 Turn OFF airplane mode", &actions);
        assert!(result_off.is_ok());
        match result_off.unwrap() {
            ActionType::System(SystemAction::AirplaneMode(enable)) => {
                assert!(!*enable, "Expected airplane mode to be disabled");
            }
            _ => panic!("Expected SystemAction::AirplaneMode(false)"),
        }
    }

    #[test]
    fn test_get_config_path() {
        let path = get_config_path();
        assert!(path.is_ok());
        let path_buf = path.unwrap();
        assert!(path_buf.to_string_lossy().contains("network-dmenu"));
        assert!(path_buf.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn test_parse_vpn_action_connect() {
        let line = "vpn       - 📶 TestVPN";
        let result = parse_vpn_action(line);
        assert!(result.is_ok());

        let name = result.unwrap();
        assert_eq!(name, "TestVPN");
    }

    #[test]
    fn test_parse_vpn_action_disconnect() {
        let line = "vpn       - 📶 TestVPN";
        let result = parse_vpn_action(line);
        assert!(result.is_ok());

        let name = result.unwrap();
        assert_eq!(name, "TestVPN");
    }

    #[test]
    fn test_parse_vpn_action_invalid() {
        let line = "invalid line";
        let result = parse_vpn_action(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_wifi_action_network() {
        let line = "wifi      - 📶 TestNetwork	WPA2	";
        let result = parse_wifi_action(line);
        assert!(result.is_ok());

        let (ssid, security) = result.unwrap();
        assert_eq!(ssid, "TestNetwork");
        assert_eq!(security, "WPA2");
    }

    #[test]
    fn test_parse_wifi_action_disconnect() {
        let line = "wifi      - ❌ Disconnect	WPA2	";
        let result = parse_wifi_action(line);
        assert!(result.is_ok());

        let (ssid, _security) = result.unwrap();
        assert_eq!(ssid, "Disconnect");
    }

    #[test]
    fn test_parse_wifi_action_connect() {
        let line = "wifi      - 📶 Connect	WPA2	";
        let result = parse_wifi_action(line);
        assert!(result.is_ok());

        let (ssid, _security) = result.unwrap();
        assert_eq!(ssid, "Connect");
    }

    #[test]
    fn test_parse_wifi_action_connect_hidden() {
        let line = "wifi      - 📶 Connect to hidden network	WPA2	";
        let result = parse_wifi_action(line);
        assert!(result.is_ok());

        let (ssid, _security) = result.unwrap();
        assert_eq!(ssid, "Connect to hidden network");
    }

    #[test]
    fn test_parse_wifi_action_invalid() {
        let line = "invalid line";
        let result = parse_wifi_action(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_action_to_string_tailscale_show_lock_status() {
        let action = ActionType::Tailscale(TailscaleAction::ShowLockStatus);
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 🔒 Show Tailscale Lock Status");
    }

    #[test]
    fn test_action_to_string_tailscale_list_locked_nodes() {
        let action = ActionType::Tailscale(TailscaleAction::ListLockedNodes);
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 📋 List Locked Nodes");
    }

    #[test]
    fn test_action_to_string_tailscale_sign_locked_node() {
        let action = ActionType::Tailscale(TailscaleAction::SignLockedNode("abcd1234".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 🔑 Sign Node: abcd1234");
    }

    #[test]
    fn test_action_to_string_diagnostic_test_connectivity() {
        let action = ActionType::Diagnostic(DiagnosticAction::TestConnectivity);
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- ✅ Test Connectivity");
    }

    #[test]
    fn test_action_to_string_diagnostic_ping_gateway() {
        let action = ActionType::Diagnostic(DiagnosticAction::PingGateway);
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- 📶 Ping Gateway");
    }

    #[test]
    fn test_action_to_string_diagnostic_traceroute() {
        let action = ActionType::Diagnostic(DiagnosticAction::TraceRoute("8.8.8.8".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- 🗺️ Trace Route to 8.8.8.8");
    }

    #[test]
    fn test_action_to_string_diagnostic_speedtest() {
        let action = ActionType::Diagnostic(DiagnosticAction::SpeedTest);
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- 🚀 Speed Test");
    }

    #[test]
    fn test_action_to_string_diagnostic_speedtest_fast() {
        let action = ActionType::Diagnostic(DiagnosticAction::SpeedTestFast);
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- ⚡ Speed Test (Fast.com)");
    }

    #[test]
    fn test_action_to_string_diagnostic_dns_benchmark() {
        let action = ActionType::Diagnostic(DiagnosticAction::DnsBenchmark);
        let result = action_to_string(&action);
        assert_eq!(result, "diagnostic- 🔍 DNS Benchmark & Optimize");
    }

    #[test]
    fn test_exit_node_filter_config_override() {
        // Test that command-line args override config file settings
        let config = Config {
            actions: Vec::new(),
            exclude_exit_node: Vec::new(),
            max_nodes_per_country: Some(2),
            max_nodes_per_city: None,
            country_filter: Some("Sweden".to_string()),
            use_dns_cache: true,
            nextdns_api_key: None,
            nextdns_toggle_profiles: None,
            dmenu_cmd: "dmenu".to_string(),
            dmenu_args: String::new(),
        };

        // When args are None, config values should be used
        let mut args = Args {
            wifi_interface: "wlan0".to_string(),
            no_wifi: false,
            no_vpn: false,
            no_bluetooth: false,
            no_tailscale: false,
            no_nextdns: false,
            nextdns_api_key: String::new(),
            validate_nextdns_key: false,
            refresh_nextdns_profiles: false,
            no_diagnostics: false,
            profile: false,
            log_level: "warn".to_string(),
            max_nodes_per_country: None,
            max_nodes_per_city: None,
            country: None,
            stdin: false,
            stdout: false,
        };

        let max_per_country = args.max_nodes_per_country.or(config.max_nodes_per_country);
        let country = args
            .country
            .as_deref()
            .or_else(|| config.country_filter.as_deref());

        assert_eq!(max_per_country, Some(2));
        assert_eq!(country, Some("Sweden"));

        // When args are provided, they should override config
        args.max_nodes_per_country = Some(3);
        args.country = Some("USA".to_string());

        let max_per_country = args.max_nodes_per_country.or(config.max_nodes_per_country);
        let country = args
            .country
            .as_deref()
            .or_else(|| config.country_filter.as_deref());

        assert_eq!(max_per_country, Some(3));
        assert_eq!(country, Some("USA"));
    }

    #[test]
    fn test_find_selected_action_diagnostic_success() {
        let actions = vec![
            ActionType::Diagnostic(DiagnosticAction::TestConnectivity),
            ActionType::Diagnostic(DiagnosticAction::PingGateway),
        ];

        let selected = "diagnostic- ✅ Test Connectivity";
        let result = find_selected_action(selected, &actions);

        assert!(result.is_ok());
        match result.unwrap() {
            ActionType::Diagnostic(DiagnosticAction::TestConnectivity) => {}
            _ => panic!("Expected TestConnectivity action"),
        }
    }
}
