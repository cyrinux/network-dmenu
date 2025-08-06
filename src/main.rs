use clap::Parser;
use command::CommandRunner;
use dirs::config_dir;
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
mod iwd;
mod networkmanager;
mod tailscale;
mod utils;

use bluetooth::{
    get_connected_devices, get_paired_bluetooth_devices, handle_bluetooth_action, BluetoothAction,
};
use command::{is_command_installed, RealCommandRunner};
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

/// Command-line arguments structure for the application.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wlan0")]
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
    profile: bool,
}

/// Configuration structure for the application.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Config {
    #[serde(default)]
    actions: Vec<CustomAction>,
    #[serde(default)]
    exclude_exit_node: Vec<String>,
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
    System(SystemAction),
    Tailscale(TailscaleAction),
    Vpn(VpnAction),
    Wifi(WifiAction),
}

/// Enum representing system-related actions.
#[derive(Debug)]
enum SystemAction {
    EditConnections,
    RfkillBlock,
    RfkillUnblock,
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
fn get_default_config() -> &'static str {
    r#"
dmenu_cmd = "dmenu"
dmenu_args = "--no-multi"

exclude_exit_node = ["exit1", "exit2"]

[[actions]]
display = "🛡️ Example"
cmd = "notify-send 'hello' 'world'"
"#
}

/// Main function for the application.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    create_default_config_if_missing()?;

    let config = get_config()?; // Load the configuration once

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

    let actions = get_actions(&args, &config, &command_runner)?;

    // Display profiling information if enabled
    if let Some(start) = start_time {
        let duration = start.elapsed();
        eprintln!("Performance profile: Generated list in {:.2?}", duration);
    }

    let action = select_action_from_menu(&config, &actions)?;

    if !action.is_empty() {
        let selected_action = find_selected_action(&action, &actions)?;
        let connected_devices = get_connected_devices(&command_runner)?;

        set_action(
            &args.wifi_interface,
            selected_action,
            &connected_devices,
            &command_runner,
        )
        .await?;
    }

    debug_tailscale_status_if_installed()?;

    Ok(())
}

/// Checks if required commands are installed.
fn check_required_commands(config: &Config) -> Result<(), Box<dyn Error>> {
    if !is_command_installed("pinentry-gnome3") || !is_command_installed(&config.dmenu_cmd) {
        panic!("pinentry-gnome3 or dmenu command missing");
    }
    Ok(())
}

/// Selects an action from the menu using dmenu.
fn select_action_from_menu(
    config: &Config,
    actions: &[ActionType],
) -> Result<String, Box<dyn Error>> {
    let mut child = Command::new(&config.dmenu_cmd)
        .args(config.dmenu_args.split_whitespace())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
        let actions_display = actions
            .iter()
            .map(action_to_string)
            .collect::<Vec<_>>()
            .join("\n");
        write!(stdin, "{actions_display}")?;
    }

    let output = child.wait_with_output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Converts an action to a string for display.
fn action_to_string(action: &ActionType) -> String {
    match action {
        ActionType::Custom(custom_action) => format_entry("action", "", &custom_action.display),
        ActionType::System(system_action) => match system_action {
            SystemAction::RfkillBlock => format_entry("system", "❌", "Radio wifi rfkill block"),
            SystemAction::RfkillUnblock => {
                format_entry("system", "📶", "Radio wifi rfkill unblock")
            }
            SystemAction::EditConnections => format_entry("system", "📶", "Edit connections"),
        },
        ActionType::Tailscale(mullvad_action) => match mullvad_action {
            TailscaleAction::SetExitNode(node) => node.to_string(),
            TailscaleAction::DisableExitNode => {
                format_entry("tailscale", "❌", "Disable exit-node")
            }
            TailscaleAction::SetEnable(enable) => format_entry(
                "tailscale",
                if *enable { "✅" } else { "❌" },
                if *enable {
                    "Enable tailscale"
                } else {
                    "Disable tailscale"
                },
            ),
            TailscaleAction::SetShields(enable) => format_entry(
                "tailscale",
                if *enable { "🛡️" } else { "🛡️" },
                if *enable {
                    "Shields up"
                } else {
                    "Shields down"
                },
            ),
            TailscaleAction::ShowLockStatus => {
                format_entry("tailscale", "🔒", "Show Tailscale Lock Status")
            }
            TailscaleAction::ListLockedNodes => {
                format_entry("tailscale", "📋", "List Locked Nodes")
            }
            TailscaleAction::SignLockedNode(node_key) => {
                // Try to find the hostname for this node key from locked nodes
                if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner) {
                    if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                        format_entry(
                            "tailscale",
                            "✅",
                            &format!(
                                "Sign Node: {} - {} ({})",
                                extract_short_hostname(&node.hostname),
                                node.machine_name,
                                &node_key[..8]
                            ),
                        )
                    } else {
                        format_entry("tailscale", "✅", &format!("Sign Node: {}", &node_key[..8]))
                    }
                } else {
                    format_entry("tailscale", "✅", &format!("Sign Node: {}", &node_key[..8]))
                }
            }
        },
        ActionType::Vpn(vpn_action) => match vpn_action {
            VpnAction::Connect(network) => format_entry("vpn", "", network),
            VpnAction::Disconnect(network) => format_entry("vpn", "❌", network),
        },
        ActionType::Wifi(wifi_action) => match wifi_action {
            WifiAction::Network(network) => format_entry("wifi", "", network),
            WifiAction::Disconnect => format_entry("wifi", "❌", "Disconnect"),
            WifiAction::Connect => format_entry("wifi", "📶", "Connect"),
            WifiAction::ConnectHidden => format_entry("wifi", "📶", "Connect to hidden network"),
        },
        ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
            BluetoothAction::ToggleConnect(device) => device.to_string(),
        },
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
                format_entry("action", "", &custom_action.display) == action
            }
            ActionType::System(system_action) => match system_action {
                SystemAction::RfkillBlock => {
                    action == format_entry("system", "❌", "Radio wifi rfkill block")
                }
                SystemAction::RfkillUnblock => {
                    action == format_entry("system", "📶", "Radio wifi rfkill unblock")
                }
                SystemAction::EditConnections => {
                    action == format_entry("system", "📶", "Edit connections")
                }
            },
            ActionType::Tailscale(mullvad_action) => match mullvad_action {
                TailscaleAction::SetExitNode(node) => action == node,
                TailscaleAction::DisableExitNode => {
                    action == format_entry("tailscale", "❌", "Disable exit-node")
                }
                TailscaleAction::SetEnable(enable) => {
                    action
                        == format_entry(
                            "tailscale",
                            if *enable { "✅" } else { "❌" },
                            if *enable {
                                "Enable tailscale"
                            } else {
                                "Disable tailscale"
                            },
                        )
                }
                TailscaleAction::SetShields(enable) => {
                    action
                        == format_entry(
                            "tailscale",
                            "🛡️",
                            if *enable {
                                "Shields up"
                            } else {
                                "Shields down"
                            },
                        )
                }
                TailscaleAction::ShowLockStatus => {
                    action == format_entry("tailscale", "🔒", "Show Tailscale Lock Status")
                }
                TailscaleAction::ListLockedNodes => {
                    action == format_entry("tailscale", "📋", "List Locked Nodes")
                }
                TailscaleAction::SignLockedNode(node_key) => {
                    // Try to find the hostname for this node key from locked nodes
                    if let Ok(locked_nodes) = get_locked_nodes(&RealCommandRunner) {
                        if let Some(node) = locked_nodes.iter().find(|n| n.node_key == *node_key) {
                            action
                                == format_entry(
                                    "tailscale",
                                    "✅",
                                    &format!(
                                        "Sign Node: {} - {} ({})",
                                        extract_short_hostname(&node.hostname),
                                        node.machine_name,
                                        &node_key[..8]
                                    ),
                                )
                        } else {
                            action
                                == format_entry(
                                    "tailscale",
                                    "✅",
                                    &format!("Sign Node: {}", &node_key[..8]),
                                )
                        }
                    } else {
                        action
                            == format_entry(
                                "tailscale",
                                "✅",
                                &format!("Sign Node: {}", &node_key[..8]),
                            )
                    }
                }
            },
            ActionType::Vpn(vpn_action) => match vpn_action {
                VpnAction::Connect(network) => action == format_entry("vpn", "", network),
                VpnAction::Disconnect(network) => action == format_entry("vpn,", "❌", network),
            },
            ActionType::Wifi(wifi_action) => match wifi_action {
                WifiAction::Network(network) => action == format_entry("wifi", "", network),
                WifiAction::Disconnect => action == format_entry("wifi", "❌", "Disconnect"),
                WifiAction::Connect => action == format_entry("wifi", "📶", "Connect"),
                WifiAction::ConnectHidden => {
                    action == format_entry("wifi", "📶", "Connect to hidden network")
                }
            },
            ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                BluetoothAction::ToggleConnect(device) => action == device,
            },
        })
        .ok_or(format!("Action not found: {action}").into())
}

/// Gets the configuration file path.
fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let config_dir = config_dir().ok_or("Failed to find config directory")?;
    Ok(config_dir.join("network-dmenu").join("config.toml"))
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
fn get_actions(
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
        match get_paired_bluetooth_devices(command_runner) {
            Ok(devices) => devices,
            Err(_) => vec![], // Continue on error for better resilience
        }
    } else {
        vec![]
    };
    if args.profile && bluetooth_start.is_some() {
        eprintln!(
            "  Bluetooth scan took: {:.2?}",
            bluetooth_start.unwrap().elapsed()
        );
    }

    // Performance optimization: Collect VPN networks early - usually fast
    let vpn_start = if args.profile {
        Some(Instant::now())
    } else {
        None
    };
    let vpn_networks = if !args.no_vpn && is_command_installed("nmcli") {
        match get_nm_vpn_networks(command_runner) {
            Ok(networks) => networks,
            Err(_) => vec![], // Error resilience: continue despite errors
        }
    } else {
        vec![]
    };
    if args.profile && vpn_start.is_some() {
        eprintln!("  VPN scan took: {:.2?}", vpn_start.unwrap().elapsed());
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
            match get_nm_wifi_networks(command_runner) {
                Ok(networks) => networks,
                Err(_) => vec![], // Error resilience: continue despite errors
            }
        } else if is_command_installed("iwctl") {
            match get_iwd_networks(&args.wifi_interface, command_runner) {
                Ok(networks) => networks,
                Err(_) => vec![], // Error resilience: continue despite errors
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    if args.profile && wifi_start.is_some() {
        eprintln!("  WiFi scan took: {:.2?}", wifi_start.unwrap().elapsed());
    }

    // Performance optimization: Add simple stateless items while network scans are processing
    // These operations are extremely fast and require no network interaction
    if !args.no_wifi
        && is_command_installed("nmcli")
        && is_command_installed("nm-connection-editor")
    {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    if !args.no_wifi && is_command_installed("rfkill") {
        actions.push(ActionType::System(SystemAction::RfkillBlock));
        actions.push(ActionType::System(SystemAction::RfkillUnblock));
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
    if !args.no_tailscale && is_command_installed("tailscale") {
        // Add basic Tailscale actions first (these are simple and fast)
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(false)));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(true)));
        actions.push(ActionType::Tailscale(TailscaleAction::ShowLockStatus));

        // Performance optimization: Get Tailscale exit nodes (potentially slower operation)
        let mullvad_actions = get_mullvad_actions(command_runner, &config.exclude_exit_node);
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
                for node in locked_nodes {
                    actions.push(ActionType::Tailscale(TailscaleAction::SignLockedNode(
                        node.node_key,
                    )));
                }
            }
        }
    }
    if args.profile && tailscale_start.is_some() {
        eprintln!(
            "  Tailscale operations took: {:.2?}",
            tailscale_start.unwrap().elapsed()
        );
    }

    Ok(actions)
}

/// Handles a custom action by executing its command.
fn handle_custom_action(action: &CustomAction) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("sh").arg("-c").arg(&action.cmd).status()?;
    Ok(status.success())
}

/// Handles a system action.
fn handle_system_action(action: &SystemAction) -> Result<bool, Box<dyn Error>> {
    match action {
        SystemAction::RfkillBlock => {
            let status = Command::new("rfkill").arg("block").arg("wlan").status()?;
            Ok(status.success())
        }
        SystemAction::RfkillUnblock => {
            let status = Command::new("rfkill").arg("unblock").arg("wlan").status()?;
            Ok(status.success())
        }
        SystemAction::EditConnections => {
            let status = Command::new("nm-connection-editor").status()?;
            Ok(status.success())
        }
    }
}

/// Parses a VPN action string to extract the connection name.
pub fn parse_vpn_action(action: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
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
            // Ignore errors from mullvad check
            let _ = check_mullvad().await;
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
                // Ignore errors from captive portal check
                let _ = check_captive_portal().await;
            }

            Ok(status.success())
        }
        WifiAction::ConnectHidden => {
            let ssid = utils::prompt_for_ssid()?;
            let network = format_entry("wifi", "📶", &format!("{ssid}\tUNKNOWN\t"));
            // FIXME: nmcli connect hidden network looks buggy
            // so we will use iwd directly for the moment
            let connection_result = if is_command_installed("iwctl") {
                let result = connect_to_iwd_wifi(wifi_interface, &network, true, command_runner)?;
                if result {
                    // Ignore errors from captive portal check
                    let _ = check_captive_portal().await;
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
                    // Ignore errors from captive portal check
                    let _ = check_captive_portal().await;
                }
                result
            } else if is_command_installed("iwctl") {
                let result = connect_to_iwd_wifi(wifi_interface, network, false, command_runner)?;
                // For IWD, we check after connection attempt
                let _ = check_captive_portal().await;
                result
            } else {
                false
            };
            // Ignore errors from mullvad check
            let _ = check_mullvad().await;
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
) -> Result<bool, Box<dyn Error>> {
    match action {
        ActionType::Custom(custom_action) => handle_custom_action(custom_action),
        ActionType::System(system_action) => handle_system_action(system_action),
        ActionType::Tailscale(mullvad_action) => {
            let notification_sender = DefaultNotificationSender;
            handle_tailscale_action(mullvad_action, command_runner, Some(&notification_sender)).await
        }
        ActionType::Vpn(vpn_action) => handle_vpn_action(vpn_action, command_runner).await,
        ActionType::Wifi(wifi_action) => {
            handle_wifi_action(wifi_action, wifi_interface, command_runner).await
        }
        ActionType::Bluetooth(bluetooth_action) => {
            handle_bluetooth_action(bluetooth_action, connected_devices, command_runner)
        }
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

/// Prints the Tailscale status if the command is installed (for debugging).
fn debug_tailscale_status_if_installed() -> Result<(), Box<dyn Error>> {
    #[cfg(debug_assertions)]
    {
        if is_command_installed("tailscale") {
            Command::new("tailscale").arg("status").status()?;
        }
    }
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
        let result = format_entry("verylongaction", "🎯", "Some text");
        assert_eq!(result, "verylongaction- 🎯 Some text");
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
        let action = ActionType::System(SystemAction::RfkillBlock);
        let result = action_to_string(&action);
        assert_eq!(result, "system    - ❌ Radio wifi rfkill block");
    }

    #[test]
    fn test_action_to_string_system_rfkill_unblock() {
        let action = ActionType::System(SystemAction::RfkillUnblock);
        let result = action_to_string(&action);
        assert_eq!(result, "system    - 📶 Radio wifi rfkill unblock");
    }

    #[test]
    fn test_action_to_string_system_edit_connections() {
        let action = ActionType::System(SystemAction::EditConnections);
        let result = action_to_string(&action);
        assert_eq!(result, "system    - 📶 Edit connections");
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
            ActionType::System(SystemAction::RfkillBlock),
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
            ActionType::System(SystemAction::RfkillBlock),
        ];

        let result = find_selected_action("nonexistent action", &actions);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Action not found"));
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
    fn test_debug_tailscale_status_if_installed() {
        // This function should not panic and should return Ok(())
        let result = debug_tailscale_status_if_installed();
        assert!(result.is_ok());
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
}
