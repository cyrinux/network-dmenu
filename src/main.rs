use clap::Parser;
use command::CommandRunner;
use dirs::config_dir;
#[cfg(feature = "gtk-ui")]
use network_dmenu::select_action_with_gtk;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

use utils::check_captive_portal;

mod bluetooth;
mod command;
mod constants;
mod diagnostics;
mod iwd;
mod networkmanager;
mod rfkill;
mod tailscale;
mod tailscale_prefs;
mod utils;

use bluetooth::{
    get_connected_devices, get_paired_bluetooth_devices, handle_bluetooth_action, BluetoothAction,
};
use command::{is_command_installed, RealCommandRunner};
use diagnostics::{
    diagnostic_action_to_string, get_diagnostic_actions, handle_diagnostic_action, DiagnosticAction,
};
use iwd::{connect_to_iwd_wifi, disconnect_iwd_wifi, get_iwd_networks, is_iwd_connected};
use networkmanager::{
    connect_to_nm_vpn, connect_to_nm_wifi, disconnect_nm_vpn, disconnect_nm_wifi,
    get_nm_vpn_networks, get_nm_wifi_networks, is_nm_connected,
};
use tailscale::{
    check_mullvad, extract_short_hostname, get_locked_nodes, get_mullvad_actions,
    handle_tailscale_action, is_exit_node_active, is_tailscale_enabled, is_tailscale_lock_enabled,
    DefaultNotificationSender, TailscaleAction,
};
use tailscale_prefs::parse_tailscale_prefs;

use constants::*;
// Make sure ICON_KEY is available
use constants::ICON_KEY;

/// Command-line arguments structure for the application.
#[derive(Parser, Debug)]
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
    no_diagnostics: bool,
    #[arg(long)]
    profile: bool,
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
    #[arg(
        long,
        help = "Use built-in GTK UI instead of dmenu (requires --features gtk-ui)"
    )]
    use_gtk: bool,
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
    dmenu_cmd: String,
    dmenu_args: String,
    #[serde(default)]
    use_gtk: bool,
    #[serde(default)]
    use_gtk_fallback: bool,
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
    System(SystemAction),
    Tailscale(TailscaleAction),
    Vpn(VpnAction),
    Wifi(WifiAction),
}

/// Enum representing system-related actions.
#[derive(Debug)]
enum SystemAction {
    EditConnections,
    RfkillBlock(String),
    RfkillUnblock(String),
    AirplaneMode(bool),
}

/// Enum representing Wi-Fi-related actions.
#[derive(Debug)]
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

/// Returns the default configuration as a string.
fn get_default_config() -> String {
    format!(
        r#"# General settings
dmenu_cmd = "{}"
dmenu_args = "{}"
use_gtk = false
use_gtk_fallback = true

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

    create_default_config_if_missing()?;

    let mut config = get_config()?; // Load the configuration once

    // Override config with command line args if specified
    if args.use_gtk {
        config.use_gtk = true;
    }

    check_required_commands(&config)?;

    // Performance optimization: We're using a more efficient approach for network scanning
    // that prioritizes faster operations first to improve perceived responsiveness
    let command_runner = RealCommandRunner;

    // Measure performance if profiling is enabled
    let start_time = if args.profile {
        Some(std::time::Instant::now())
    } else {
        None
    };

    let actions = get_actions(&args, &config, &command_runner).await?;

    // Display profiling information if enabled
    if let Some(start) = start_time {
        let duration = start.elapsed();
        eprintln!(">>> PROFILE: Generated list in {:.2?}", duration);
        if args.profile {
            let _ = Notification::new()
                .summary("Network-dmenu Profiling")
                .body(&format!("Generated list in {:.2?}", duration))
                .show();
        }
    }

    let action = select_action_from_menu(&config, &actions).await?;

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
fn check_required_commands(config: &Config) -> Result<(), Box<dyn Error>> {
    if !is_command_installed("pinentry-gnome3") {
        eprintln!("Warning: pinentry-gnome3 command missing");
    }

    #[cfg(feature = "gtk-ui")]
    let using_gtk = config.use_gtk;

    #[cfg(not(feature = "gtk-ui"))]
    let using_gtk = false;

    if !using_gtk && !is_command_installed(&config.dmenu_cmd) {
        panic!("dmenu command missing and GTK UI not enabled");
    }

    Ok(())
}

/// Selects an action from the menu using dmenu or GTK UI.
async fn select_action_from_menu(
    config: &Config,
    actions: &[ActionType],
) -> Result<String, Box<dyn Error>> {
    // Convert actions to string representation
    let action_strings: Vec<String> = actions
        .iter()
        .map(|action| action_to_string(action))
        .collect();

    // Try GTK UI if feature is enabled and requested
    #[cfg(feature = "gtk-ui")]
    if config.use_gtk {
        // Use blocking context to prevent thread initialization errors with GTK
        match select_action_with_gtk(action_strings.clone()).await {
            Ok(Some(selected)) => return Ok(selected),
            Ok(None) => {
                if !config.use_gtk_fallback {
                    // Just return empty string when user cancels or presses Escape
                    return Ok(String::new());
                }
                // Falls through to dmenu if use_gtk_fallback is true
            }
            Err(_) => {
                // GTK UI failed to initialize, falling back to dmenu
                eprintln!("GTK UI failed to initialize, falling back to dmenu");
            }
        }
    }

    // Fall back to dmenu if GTK UI is not enabled, not requested, or failed
    let mut child = Command::new(&config.dmenu_cmd)
        .args(config.dmenu_args.split_whitespace())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        for action_string in action_strings {
            writeln!(stdin, "{}", action_string)?;
        }
    }

    let output = child.wait_with_output()?;
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(selected)
}

/// Converts an action to a string for display.
/// Format device information for rfkill block/unblock operations
fn format_rfkill_device(device: &str, block: bool) -> String {
    let (icon, action) = if block {
        (ICON_CROSS, "Turn OFF")
    } else {
        (ICON_SIGNAL, "Turn ON")
    };

    // Check if this is a device ID (numeric) or a device type
    if let Ok(id) = device.parse::<u32>() {
        // Load devices from cache
        let all_devices = rfkill::load_rfkill_devices_from_cache();

        // Find device by ID
        let device_info = all_devices.iter().find(|d| d.id == id);

        match device_info {
            Some(info) => format_entry(
                ACTION_TYPE_SYSTEM,
                icon,
                &format!(
                    "{} {} ({})",
                    action,
                    info.device_type_display(),
                    info.device
                ),
            ),
            None => format_entry(
                ACTION_TYPE_SYSTEM,
                icon,
                &format!("{} device ID: {}", action, id),
            ),
        }
    } else {
        format_entry(
            ACTION_TYPE_SYSTEM,
            icon,
            &format!("{} all {} devices", action, device),
        )
    }
}

fn action_to_string(action: &ActionType) -> String {
    match action {
        ActionType::Custom(custom_action) => {
            format_entry(ACTION_TYPE_ACTION, "", &custom_action.display)
        }
        ActionType::System(system_action) => match system_action {
            SystemAction::RfkillBlock(device) => format_rfkill_device(device, true),
            SystemAction::RfkillUnblock(device) => format_rfkill_device(device, false),
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
                let text = if *enable {
                    TAILSCALE_SHIELDS_UP
                } else {
                    TAILSCALE_SHIELDS_DOWN
                };
                format_entry(ACTION_TYPE_TAILSCALE, ICON_SHIELD, text)
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
                if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner) {
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
                if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner) {
                    if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                        format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_CHECK,
                            &TAILSCALE_SIGN_NODE_DETAILED
                                .replace("{hostname}", extract_short_hostname(&node.hostname))
                                .replace("{machine}", &node.machine_name)
                                .replace("{key}", &node_key[..8]),
                        )
                    } else {
                        format_entry(
                            ACTION_TYPE_TAILSCALE,
                            ICON_CHECK,
                            &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                        )
                    }
                } else {
                    format_entry(
                        ACTION_TYPE_TAILSCALE,
                        ICON_CHECK,
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
                SystemAction::RfkillBlock(device) => action == format_rfkill_device(device, true),
                SystemAction::RfkillUnblock(device) => {
                    action == format_rfkill_device(device, false)
                }
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
                    let text = if *enable {
                        TAILSCALE_SHIELDS_UP
                    } else {
                        TAILSCALE_SHIELDS_DOWN
                    };
                    action == format_entry(ACTION_TYPE_TAILSCALE, ICON_SHIELD, text)
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
                    if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner) {
                        if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                            action
                                == format_entry(
                                    ACTION_TYPE_TAILSCALE,
                                    ICON_CHECK,
                                    &TAILSCALE_SIGN_NODE_DETAILED
                                        .replace(
                                            "{hostname}",
                                            extract_short_hostname(&node.hostname),
                                        )
                                        .replace("{machine}", &node.machine_name)
                                        .replace("{key}", &node_key[..8]),
                                )
                        } else {
                            action
                                == format_entry(
                                    ACTION_TYPE_TAILSCALE,
                                    ICON_CHECK,
                                    &TAILSCALE_SIGN_NODE.replace("{}", &node_key[..8]),
                                )
                        }
                    } else {
                        action
                            == format_entry(
                                ACTION_TYPE_TAILSCALE,
                                ICON_CHECK,
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

/// Retrieves the list of actions based on the command-line arguments and configuration.
///
/// # Performance Optimization Notes
/// This function has been optimized for performance using the following strategies:
/// 1. Prioritizes faster operations first to minimize perceived latency
/// 2. Collects network information early in the process
/// 3. Adds simple stateless items to the list while waiting for network scan results
/// 4. Improves error handling to continue execution even if some network scans fail
/// 5. Organizes code to follow a more logical flow for network scanning operations
///
/// Use the `--profile` flag when running the application to see performance metrics.
///
/// # Arguments
/// * `args` - Command line arguments
/// * `config` - Application configuration
/// * `command_runner` - Interface for running shell commands
///
/// # Returns
/// A vector of actions to display in the menu
async fn get_actions(
    args: &Args,
    config: &Config,
    command_runner: &dyn CommandRunner,
) -> Result<Vec<ActionType>, Box<dyn Error>> {
    let mut actions = config
        .actions
        .clone()
        .into_iter()
        .map(ActionType::Custom)
        .collect::<Vec<_>>();

    // Performance optimization: Start with the fastest operations first
    // Collect Bluetooth devices early - usually very fast
    let bluetooth_start = if args.profile {
        Some(Instant::now())
    } else {
        None
    };
    let bluetooth_devices = if !args.no_bluetooth && is_command_installed("bluetoothctl") {
        get_paired_bluetooth_devices(command_runner).unwrap_or_default()
    } else {
        vec![]
    };
    if args.profile && bluetooth_start.is_some() {
        let elapsed = bluetooth_start.unwrap().elapsed();
        eprintln!(">>> PROFILE: Bluetooth scan took: {:.2?}", elapsed);
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!("Bluetooth scan took: {:.2?}", elapsed))
            .show();
    }

    // Performance optimization: Collect VPN networks early - usually fast
    let vpn_start = if args.profile {
        Some(Instant::now())
    } else {
        None
    };
    let vpn_networks = if !args.no_vpn && is_command_installed("nmcli") {
        get_nm_vpn_networks(command_runner).unwrap_or_default()
    } else {
        vec![]
    };
    if args.profile && vpn_start.is_some() {
        let elapsed = vpn_start.unwrap().elapsed();
        eprintln!(">>> PROFILE: VPN scan took: {:.2?}", elapsed);
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!("VPN scan took: {:.2?}", elapsed))
            .show();
    }

    // Performance optimization: Start WiFi scanning early as it can be slow
    // This is typically the most time-consuming network operation
    let wifi_start = if args.profile {
        Some(Instant::now())
    } else {
        None
    };
    let wifi_networks = if !args.no_wifi {
        if is_command_installed("nmcli") {
            get_nm_wifi_networks(command_runner).unwrap_or_default()
        } else if is_command_installed("iwctl") {
            get_iwd_networks(&args.wifi_interface, command_runner).unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    if args.profile && wifi_start.is_some() {
        let elapsed = wifi_start.unwrap().elapsed();
        eprintln!(">>> PROFILE: WiFi scan took: {:.2?}", elapsed);
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!("WiFi scan took: {:.2?}", elapsed))
            .show();
    }

    // Performance optimization: Add simple stateless items while network scans are processing
    // These operations are extremely fast and require no network interaction
    if !args.no_wifi
        && is_command_installed("nmcli")
        && is_command_installed("nm-connection-editor")
    {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    // Start timing for all system actions
    let system_actions_start = if args.profile {
        Some(std::time::Instant::now())
    } else {
        None
    };

    // Add rfkill device actions for a specific device type
    async fn add_rfkill_device_actions(
        actions: &mut Vec<ActionType>,
        device_type: &str,
        skip_cache: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Check for specific devices first
        let devices = rfkill::get_rfkill_devices_by_type(device_type)
            .await
            .unwrap_or_default();

        // Cache devices for later use in action_to_string
        if !devices.is_empty() && !skip_cache {
            // Cache rfkill devices for non-async functions to use
            let _ = rfkill::cache_rfkill_devices().await;
        }

        if !devices.is_empty() {
            // Add specific device actions
            for device in &devices {
                // Use is_unblocked to determine status - uses both methods from RfkillDevice
                if device.is_blocked() {
                    actions.push(ActionType::System(SystemAction::RfkillUnblock(
                        device.id.to_string(),
                    )));
                } else if device.is_unblocked() {
                    actions.push(ActionType::System(SystemAction::RfkillBlock(
                        device.id.to_string(),
                    )));
                }
            }
        } else {
            // No specific devices found, add generic actions
            actions.push(ActionType::System(SystemAction::RfkillBlock(
                device_type.to_string(),
            )));
            actions.push(ActionType::System(SystemAction::RfkillUnblock(
                device_type.to_string(),
            )));
        }

        Ok(())
    }

    if !args.no_wifi && rfkill::is_rfkill_available() {
        let wifi_rfkill_start = if args.profile {
            Some(std::time::Instant::now())
        } else {
            None
        };

        add_rfkill_device_actions(&mut actions, "wlan", false)
            .await
            .unwrap_or_default();

        if args.profile && wifi_rfkill_start.is_some() {
            let elapsed = wifi_rfkill_start.unwrap().elapsed();
            eprintln!(">>> PROFILE: WiFi rfkill actions took: {:.2?}", elapsed);
            let _ = Notification::new()
                .summary("Network-dmenu Profiling")
                .body(&format!("WiFi rfkill actions took: {:.2?}", elapsed))
                .show();
        }
    }

    if !args.no_bluetooth && rfkill::is_rfkill_available() {
        let bt_rfkill_start = if args.profile {
            Some(std::time::Instant::now())
        } else {
            None
        };

        // Skip caching if we already did it for WiFi devices
        let skip_cache = std::path::Path::new(&rfkill::get_rfkill_cache_path()).exists();
        add_rfkill_device_actions(&mut actions, "bluetooth", skip_cache)
            .await
            .unwrap_or_default();

        if args.profile && bt_rfkill_start.is_some() {
            let elapsed = bt_rfkill_start.unwrap().elapsed();
            eprintln!(
                ">>> PROFILE: Bluetooth rfkill actions took: {:.2?}",
                elapsed
            );
            let _ = Notification::new()
                .summary("Network-dmenu Profiling")
                .body(&format!("Bluetooth rfkill actions took: {:.2?}", elapsed))
                .show();
        }
    }

    // Determine if airplane mode is enabled and add appropriate toggle action
    async fn add_airplane_mode_action(actions: &mut Vec<ActionType>) -> Result<(), Box<dyn Error>> {
        // Check if all devices are blocked (airplane mode is on)
        // Only consider airplane mode if we have radio devices
        // Use get_device_type_summary to efficiently check device status
        let device_summary = rfkill::get_device_type_summary().await.unwrap_or_default();

        if !device_summary.is_empty() {
            // Check if all radio devices are blocked
            let radio_types = ["wlan", "bluetooth", "wwan", "fm", "nfc", "gps"];
            let radio_devices: Vec<_> = device_summary
                .iter()
                .filter(|(device_type, _)| radio_types.contains(&device_type.as_str()))
                .collect();

            if !radio_devices.is_empty() {
                // Check if all radio devices are blocked
                // (blocked_count, unblocked_count)
                let all_blocked = radio_devices
                    .iter()
                    .all(|(_, (blocked, unblocked))| *blocked > 0 && *unblocked == 0);

                if all_blocked {
                    // All devices are blocked, offer to disable airplane mode
                    actions.push(ActionType::System(SystemAction::AirplaneMode(false)));
                } else {
                    // Not all devices are blocked, offer to enable airplane mode
                    actions.push(ActionType::System(SystemAction::AirplaneMode(true)));
                }
            }
        }

        Ok(())
    }

    // Add airplane mode toggle if rfkill is available
    if rfkill::is_rfkill_available() {
        let airplane_mode_start = if args.profile {
            Some(std::time::Instant::now())
        } else {
            None
        };

        add_airplane_mode_action(&mut actions)
            .await
            .unwrap_or_default();

        if args.profile && airplane_mode_start.is_some() {
            let elapsed = airplane_mode_start.unwrap().elapsed();
            eprintln!(">>> PROFILE: Airplane mode action took: {:.2?}", elapsed);
            let _ = Notification::new()
                .summary("Network-dmenu Profiling")
                .body(&format!("Airplane mode action took: {:.2?}", elapsed))
                .show();
        }
    }

    // Display summary of all system actions timing
    if args.profile && system_actions_start.is_some() {
        let elapsed = system_actions_start.unwrap().elapsed();
        eprintln!(">>> PROFILE: All system actions took: {:.2?}", elapsed);
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!("All system actions took: {:.2?}", elapsed))
            .show();
    }

    // Performance optimization: Now add all the collected network information
    // By collecting the data first and then adding it all at once, we optimize the process
    actions.extend(bluetooth_devices.into_iter().map(ActionType::Bluetooth));
    actions.extend(vpn_networks.into_iter().map(ActionType::Vpn));
    actions.extend(wifi_networks.into_iter().map(ActionType::Wifi));

    // Add WiFi connect/disconnect actions
    if !args.no_wifi {
        if is_command_installed("nmcli") {
            if is_nm_connected(command_runner, &args.wifi_interface).unwrap_or(false) {
                actions.push(ActionType::Wifi(WifiAction::Disconnect));
            } else {
                actions.push(ActionType::Wifi(WifiAction::Connect));
                actions.push(ActionType::Wifi(WifiAction::ConnectHidden));
            }
        } else if is_command_installed("iwctl") {
            if is_iwd_connected(command_runner, &args.wifi_interface).unwrap_or(false) {
                actions.push(ActionType::Wifi(WifiAction::Disconnect));
            } else {
                actions.push(ActionType::Wifi(WifiAction::Connect));
                actions.push(ActionType::Wifi(WifiAction::ConnectHidden));
            }
        }
    }

    // Performance optimization: Add Tailscale actions last as they can be expensive
    let tailscale_start = if args.profile {
        Some(Instant::now())
    } else {
        None
    };

    // Get current Tailscale preferences to determine what toggle options to show
    let prefs = parse_tailscale_prefs(command_runner).unwrap();

    if !args.no_tailscale && is_command_installed("tailscale") {
        // Add basic Tailscale actions first (these are simple and fast)
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(
            !prefs.ShieldsUp,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::SetAllowLanAccess(
            !prefs.ExitNodeAllowLANAccess,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::SetAcceptRoutes(
            !prefs.RouteAll,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::ShowLockStatus));

        // Performance optimization: Get Tailscale exit nodes (potentially slower operation)
        // Command-line args override config file settings
        let max_per_country = args.max_nodes_per_country.or(config.max_nodes_per_country);
        let max_per_city = args.max_nodes_per_city.or(config.max_nodes_per_city);
        let country = args.country.as_deref().or(config.country_filter.as_deref());

        let mullvad_actions = get_mullvad_actions(
            command_runner,
            &config.exclude_exit_node,
            max_per_country,
            max_per_city,
            country,
        );
        actions.extend(
            mullvad_actions
                .into_iter()
                .map(|m| ActionType::Tailscale(TailscaleAction::SetExitNode(m))),
        );

        if is_exit_node_active(command_runner).unwrap_or(false) {
            actions.push(ActionType::Tailscale(TailscaleAction::DisableExitNode));
        }

        actions.push(ActionType::Tailscale(TailscaleAction::SetEnable(
            !is_tailscale_enabled(command_runner).unwrap_or(false),
        )));

        // Performance optimization: Add Tailscale Lock actions last (these are the most expensive)
        if is_tailscale_lock_enabled(command_runner).unwrap_or(false) {
            actions.push(ActionType::Tailscale(TailscaleAction::ListLockedNodes));

            // Add individual sign node actions for each locked node
            // This is potentially the slowest operation, so we do it last
            if let Ok(locked_nodes) = get_locked_nodes(command_runner) {
                if !locked_nodes.is_empty() {
                    // Add a single action to sign all nodes at once, placing it FIRST
                    // Count is displayed in the action text automatically
                    actions.insert(0, ActionType::Tailscale(TailscaleAction::SignAllNodes));

                    // Also add individual node signing actions
                    for node in locked_nodes {
                        actions.push(ActionType::Tailscale(TailscaleAction::SignLockedNode(
                            node.node_key,
                        )));
                    }
                }
            }
        }
    }
    if args.profile && tailscale_start.is_some() {
        let elapsed = tailscale_start.unwrap().elapsed();
        eprintln!(">>> PROFILE: Tailscale operations took: {:.2?}", elapsed);
        let _ = Notification::new()
            .summary("Network-dmenu Profiling")
            .body(&format!("Tailscale operations took: {:.2?}", elapsed))
            .show();
    }

    // Add diagnostic actions (get_diagnostic_actions checks for tool availability internally)
    if !args.no_diagnostics {
        let diagnostic_actions = get_diagnostic_actions();
        if !diagnostic_actions.is_empty() {
            actions.extend(diagnostic_actions.into_iter().map(ActionType::Diagnostic));
        }
    }

    Ok(actions)
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
        SystemAction::RfkillBlock(device) => handle_rfkill_operation(device, true, profile).await,
        SystemAction::RfkillUnblock(device) => {
            handle_rfkill_operation(device, false, profile).await
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
            SystemAction::RfkillBlock(device) => format!("Block {}", device),
            SystemAction::RfkillUnblock(device) => format!("Unblock {}", device),
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
        ActionType::System(system_action) => handle_system_action(system_action, profile).await,
        ActionType::Tailscale(mullvad_action) => {
            let notification_sender = DefaultNotificationSender;
            handle_tailscale_action(mullvad_action, command_runner, Some(&notification_sender))
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
        assert!(config.contains("use_gtk = false"));
        assert!(config.contains("use_gtk_fallback = true"));
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
        let action = ActionType::System(SystemAction::RfkillBlock("wifi".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "system    - ❌ Turn OFF all wifi devices");
    }

    #[test]
    fn test_action_to_string_system_rfkill_unblock() {
        let action = ActionType::System(SystemAction::RfkillUnblock("wifi".to_string()));
        let result = action_to_string(&action);
        assert_eq!(result, "system    - 📶 Turn ON all wifi devices");
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
        assert_eq!(result, "tailscale - 🛡️ Shields up");
    }

    #[test]
    fn test_action_to_string_tailscale_shields_down() {
        let action = ActionType::Tailscale(TailscaleAction::SetShields(false));
        let result = action_to_string(&action);
        assert_eq!(result, "tailscale - 🛡️ Shields down");
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
            ActionType::System(SystemAction::RfkillBlock("wifi".to_string())),
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
            ActionType::System(SystemAction::RfkillBlock("wifi".to_string())),
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
        assert_eq!(result, "tailscale - ✅ Sign Node: abcd1234");
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
    fn test_exit_node_filter_config_override() {
        // Test that command-line args override config file settings
        let config = Config {
            actions: Vec::new(),
            exclude_exit_node: Vec::new(),
            max_nodes_per_country: Some(2),
            max_nodes_per_city: None,
            country_filter: Some("Sweden".to_string()),
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
            no_diagnostics: false,
            profile: false,
            max_nodes_per_country: None,
            max_nodes_per_city: None,
            country: None,
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
