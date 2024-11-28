use crate::command::CommandRunner;
use clap::Parser;
use dirs::config_dir;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
use networkmanager::{connect_to_nm_vpn, disconnect_nm_vpn, get_nm_vpn_networks};
use networkmanager::{
    connect_to_nm_wifi, disconnect_nm_wifi, get_nm_wifi_networks, is_nm_connected,
};
use tailscale::{
    check_mullvad, get_mullvad_actions, handle_tailscale_action, is_exit_node_active,
    is_tailscale_enabled, TailscaleAction,
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
}

/// Configuration structure for the application.
#[derive(Debug, Deserialize, Serialize)]
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

    let command_runner = RealCommandRunner;
    let actions = get_actions(&args, &config, &command_runner)?; // Use the loaded config
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
        },
        ActionType::Vpn(vpn_action) => match vpn_action {
            VpnAction::Connect(network) => format_entry("vpn", "", network),
            VpnAction::Disconnect(network) => format_entry("vpn", "❌", network),
        },
        ActionType::Wifi(wifi_action) => match wifi_action {
            WifiAction::Network(network) => format_entry("wifi", "", network),
            WifiAction::Disconnect => format_entry("wifi", "❌", "Disconnect"),
            WifiAction::Connect => format_entry("wifi", "📶", "Connect"),
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
            },
            ActionType::Vpn(vpn_action) => match vpn_action {
                VpnAction::Connect(network) => action == format_entry("vpn", "", network),
                VpnAction::Disconnect(network) => action == format_entry("vpn,", "❌", network),
            },
            ActionType::Wifi(wifi_action) => match wifi_action {
                WifiAction::Network(network) => action == format_entry("wifi", "", network),
                WifiAction::Disconnect => action == format_entry("wifi", "❌", "Disconnect"),
                WifiAction::Connect => action == format_entry("wifi", "📶", "Connect"),
            },
            ActionType::Bluetooth(bluetooth_action) => match bluetooth_action {
                BluetoothAction::ToggleConnect(device) => action == device,
            },
        })
        .ok_or("Selected action not found".into())
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
fn get_actions(
    args: &Args,
    config: &Config, // Change to reference
    command_runner: &dyn CommandRunner,
) -> Result<Vec<ActionType>, Box<dyn Error>> {
    let mut actions = config
        .actions
        .clone() // Clone the actions vector
        .into_iter()
        .map(ActionType::Custom)
        .collect::<Vec<_>>();

    if !args.no_tailscale
        && is_command_installed("tailscale")
        && is_exit_node_active(command_runner)?
    {
        actions.push(ActionType::Tailscale(TailscaleAction::DisableExitNode));
    }

    if !args.no_vpn && is_command_installed("nmcli") {
        actions.extend(
            get_nm_vpn_networks(command_runner)?
                .into_iter()
                .map(ActionType::Vpn),
        );
    }

    if !args.no_wifi {
        if is_command_installed("nmcli") {
            actions.extend(
                get_nm_wifi_networks(command_runner)?
                    .into_iter()
                    .map(ActionType::Wifi),
            );
        } else if is_command_installed("iwctl") {
            actions.extend(
                get_iwd_networks(&args.wifi_interface, command_runner)?
                    .into_iter()
                    .map(ActionType::Wifi),
            );
        }

        if is_command_installed("nmcli") {
            if is_nm_connected(command_runner, &args.wifi_interface)? {
                actions.push(ActionType::Wifi(WifiAction::Disconnect));
            } else {
                actions.push(ActionType::Wifi(WifiAction::Connect));
            }
        } else if is_command_installed("iwctl") {
            if is_iwd_connected(command_runner, &args.wifi_interface)? {
                actions.push(ActionType::Wifi(WifiAction::Disconnect));
            } else {
                actions.push(ActionType::Wifi(WifiAction::Connect));
            }
        }
    }

    if !args.no_wifi && is_command_installed("rfkill") {
        actions.push(ActionType::System(SystemAction::RfkillBlock));
        actions.push(ActionType::System(SystemAction::RfkillUnblock));
    }

    if !args.no_wifi && is_command_installed("nm-connection-editor") {
        actions.push(ActionType::System(SystemAction::EditConnections));
    }

    if !args.no_tailscale && is_command_installed("tailscale") {
        actions.push(ActionType::Tailscale(TailscaleAction::SetEnable(
            !is_tailscale_enabled(command_runner)?,
        )));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(false)));
        actions.push(ActionType::Tailscale(TailscaleAction::SetShields(true)));
        actions.extend(
            get_mullvad_actions(command_runner, &config.exclude_exit_node)
                .into_iter()
                .map(|m| ActionType::Tailscale(TailscaleAction::SetExitNode(m))),
        );
    }

    if !args.no_bluetooth && is_command_installed("bluetoothctl") {
        actions.extend(
            get_paired_bluetooth_devices(command_runner)?
                .into_iter()
                .map(ActionType::Bluetooth),
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
fn parse_vpn_action(action: &str) -> Result<&str, Box<dyn Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
        .map(|(i, _)| i)
        .ok_or("Emoji not found in action")?;

    let tab_pos = action[emoji_pos..]
        .char_indices()
        .find(|(_, c)| *c == '\t')
        .map(|(i, _)| i + emoji_pos)
        .ok_or("Tab character not found in action")?;

    let name = action[emoji_pos + tab_pos..].trim();

    #[cfg(debug_assertions)]
    eprintln!("Failed to connect to VPN network: {name}");
    let parts: Vec<&str> = action[tab_pos + 1..].split('\t').collect();
    if parts.is_empty() {
        return Err("Action format is incorrect".into());
    }
    Ok(name)
}

/// Parses a Wi-Fi action string to extract the SSID and security type.
fn parse_wifi_action(action: &str) -> Result<(&str, &str), Box<dyn Error>> {
    let emoji_pos = action
        .char_indices()
        .find(|(_, c)| *c == '✅' || *c == '📶')
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
            check_mullvad().await?;
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
            check_mullvad().await?;
            Ok(status.success())
        }
        WifiAction::Network(network) => {
            if is_command_installed("nmcli") {
                connect_to_nm_wifi(network, command_runner)?;
            } else if is_command_installed("iwctl") {
                connect_to_iwd_wifi(wifi_interface, network, command_runner)?;
            }
            check_mullvad().await?;
            Ok(true)
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
            handle_tailscale_action(mullvad_action, command_runner).await
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
fn notify_connection(summary: &str, name: &str) -> Result<(), Box<dyn Error>> {
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
