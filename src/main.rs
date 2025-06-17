use clap::Parser;
use std::process;
use tracing::{debug, error, info};

// Import modules
mod command;
mod config;
mod constants;
mod errors;
mod parsers;
mod services;
mod types;
mod utils;

// Legacy modules for backward compatibility
mod bluetooth;
mod iwd;
mod networkmanager;
mod tailscale;

use crate::config::ConfigManager;
use crate::errors::{NetworkMenuError, Result};
use crate::services::{ActionType, NetworkServiceManager};
use crate::types::{ActionContext, Config};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// WiFi interface to use
    #[arg(long, default_value = "wlan0")]
    wifi_interface: String,

    /// Disable WiFi functionality
    #[arg(long)]
    no_wifi: bool,

    /// Disable VPN functionality
    #[arg(long)]
    no_vpn: bool,

    /// Disable Bluetooth functionality
    #[arg(long)]
    no_bluetooth: bool,

    /// Disable Tailscale functionality
    #[arg(long)]
    no_tailscale: bool,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Configuration file path
    #[arg(long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() {
    let result = run().await;
    
    match result {
        Ok(_) => process::exit(0),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    init_logging(args.verbose);
    
    info!("Starting network-dmenu v{}", env!("CARGO_PKG_VERSION"));
    debug!("Args: {:?}", args);
    
    // Load configuration
    let config_manager = ConfigManager::new()?;
    let mut config = config_manager.load().await?;
    
    // Apply command line overrides to config
    apply_args_to_config(&mut config, &args);
    
    // Create service manager with basic services
    let mut service_manager = NetworkServiceManager::new();
    
    // Add services based on enabled flags
    if config.wifi.enabled {
        service_manager = service_manager.add_service(
            Box::new(services::wifi_service::WifiService::new())
        );
    }
    
    if config.vpn.enabled {
        service_manager = service_manager.add_service(
            Box::new(services::vpn_service::VpnService::new())
        );
    }
    
    if config.tailscale.enabled {
        service_manager = service_manager.add_service(
            Box::new(services::tailscale_service::TailscaleService::new())
        );
    }
    
    if config.bluetooth.enabled {
        service_manager = service_manager.add_service(
            Box::new(services::bluetooth_service::BluetoothService::new())
        );
    }
    
    // Always add system service
    service_manager = service_manager.add_service(
        Box::new(services::system_service::SystemService::new())
    );
    
    service_manager.initialize().await?;
    
    // Create action context
    let context = ActionContext::new(config.clone())
        .with_wifi_interface(args.wifi_interface);
    
    // Get all available actions
    let actions = service_manager.get_all_actions(&context).await?;
    
    if actions.is_empty() {
        info!("No actions available");
        return Ok(());
    }
    
    info!("Found {} available actions", actions.len());
    
    // Display menu and get user selection
    let selected_action = display_menu(&config, &actions).await?;
    
    if let Some(action) = selected_action {
        info!("Executing action: {:?}", action);
        
        // Execute the selected action
        match execute_action(action, &context).await {
            Ok(_) => {
                info!("Action executed successfully");
                
                // Check for captive portal if enabled
                if config.tailscale.check_captive_portal {
                    if let Err(e) = crate::utils::check_captive_portal().await {
                        debug!("Captive portal check failed: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Action execution failed: {}", e);
                return Err(e);
            }
        }
    } else {
        info!("No action selected");
    }
    
    Ok(())
}

fn init_logging(verbose: bool) {
    let level = if verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();
}

fn apply_args_to_config(config: &mut Config, args: &Args) {
    if args.no_wifi {
        config.wifi.enabled = false;
    }
    if args.no_vpn {
        config.vpn.enabled = false;
    }
    if args.no_bluetooth {
        config.bluetooth.enabled = false;
    }
    if args.no_tailscale {
        config.tailscale.enabled = false;
    }
}

async fn display_menu(config: &Config, actions: &[ActionType]) -> Result<Option<ActionType>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    
    // Create menu entries
    let menu_entries: Vec<String> = actions
        .iter()
        .map(|action| format_action_for_menu(action))
        .collect();
    
    if menu_entries.is_empty() {
        return Ok(None);
    }
    
    // Create dmenu command
    let mut cmd = Command::new(&config.dmenu_cmd);
    cmd.args(&config.dmenu_args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    
    debug!("Launching dmenu: {} {:?}", config.dmenu_cmd, config.dmenu_args);
    
    // Start dmenu process
    let mut child = cmd.spawn()
        .map_err(|e| NetworkMenuError::command_failed(&config.dmenu_cmd, e.to_string()))?;
    
    // Write menu entries to dmenu stdin
    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        for entry in &menu_entries {
            writeln!(stdin, "{}", entry)
                .map_err(|e| NetworkMenuError::command_failed("dmenu stdin", e.to_string()))?;
        }
    }
    
    // Wait for dmenu to finish and get output
    let output = child.wait_with_output()
        .map_err(|e| NetworkMenuError::command_failed("dmenu wait", e.to_string()))?;
    
    if !output.status.success() {
        debug!("dmenu cancelled or failed");
        return Ok(None);
    }
    
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if selected.is_empty() {
        return Ok(None);
    }
    
    debug!("Selected menu item: {}", selected);
    
    // Find the corresponding action
    for (i, entry) in menu_entries.iter().enumerate() {
        if entry == &selected {
            return Ok(Some(actions[i].clone()));
        }
    }
    
    debug!("Selected menu item not found in actions list");
    Ok(None)
}

fn format_action_for_menu(action: &ActionType) -> String {
    use crate::services::*;
    
    match action {
        ActionType::Wifi(wifi_action) => match wifi_action {
            WifiAction::Connect(ssid) => format!("📶 Connect to {}", ssid),
            WifiAction::ConnectHidden => "📶 Connect to Hidden Network".to_string(),
            WifiAction::Disconnect => "📶 Disconnect WiFi".to_string(),
            WifiAction::Scan => "📶 Scan for Networks".to_string(),
            WifiAction::ForgetNetwork(ssid) => format!("🗑️ Forget {}", ssid),
        },
        ActionType::Vpn(vpn_action) => match vpn_action {
            VpnAction::Connect(name) => format!("🔒 Connect {}", name),
            VpnAction::Disconnect(name) => format!("🔒 Disconnect {}", name),
            VpnAction::Toggle(name) => format!("🔒 Toggle {}", name),
        },
        ActionType::Tailscale(ts_action) => match ts_action {
            TailscaleAction::Enable => "🟢 Enable Tailscale".to_string(),
            TailscaleAction::Disable => "🔴 Disable Tailscale".to_string(),
            TailscaleAction::SetExitNode(node) => format!("🚪 Exit via {}", node),
            TailscaleAction::DisableExitNode => "🚪 Disable Exit Node".to_string(),
            TailscaleAction::SetShields(true) => "🛡️ Shields Up".to_string(),
            TailscaleAction::SetShields(false) => "🛡️ Shields Down".to_string(),
            TailscaleAction::Netcheck => "🔍 Tailscale Netcheck".to_string(),
        },
        ActionType::Bluetooth(bt_action) => match bt_action {
            BluetoothAction::Connect(addr) => format!("🔵 Connect {}", addr),
            BluetoothAction::Disconnect(addr) => format!("🔵 Disconnect {}", addr),
            BluetoothAction::Pair(addr) => format!("🔵 Pair {}", addr),
            BluetoothAction::Unpair(addr) => format!("🔵 Unpair {}", addr),
            BluetoothAction::Trust(addr) => format!("🔵 Trust {}", addr),
            BluetoothAction::Scan => "🔵 Scan for Devices".to_string(),
        },
        ActionType::System(sys_action) => match sys_action {
            SystemAction::EditConnections => "⚙️ Edit Connections".to_string(),
            SystemAction::RfkillBlock => "📵 Block WiFi".to_string(),
            SystemAction::RfkillUnblock => "📶 Unblock WiFi".to_string(),
            SystemAction::RestartNetworkManager => "🔄 Restart NetworkManager".to_string(),
            SystemAction::ShowNetworks => "📋 Show Networks".to_string(),
        },
        ActionType::Custom(custom_action) => custom_action.display.clone(),
    }
}

async fn execute_action(action: ActionType, context: &ActionContext) -> Result<()> {
    use crate::command::RealCommandRunner;
    use crate::services::*;
    
    let command_runner = RealCommandRunner::new();
    
    match action {
        ActionType::Wifi(wifi_action) => {
            execute_wifi_action(wifi_action, context, &command_runner).await
        }
        ActionType::Vpn(vpn_action) => {
            execute_vpn_action(vpn_action, context, &command_runner).await
        }
        ActionType::Tailscale(ts_action) => {
            execute_tailscale_action(ts_action, context, &command_runner).await
        }
        ActionType::Bluetooth(bt_action) => {
            execute_bluetooth_action(bt_action, context, &command_runner).await
        }
        ActionType::System(sys_action) => {
            execute_system_action(sys_action, context, &command_runner).await
        }
        ActionType::Custom(custom_action) => {
            execute_custom_action(custom_action, context, &command_runner).await
        }
    }
}

async fn execute_wifi_action(
    action: services::WifiAction,
    context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    use crate::constants::commands;
    use crate::services::WifiAction;
    
    match action {
        WifiAction::Connect(ssid) => {
            info!("Connecting to WiFi network: {}", ssid);
            let output = command_runner
                .run_command(commands::NMCLI, &[
                    "device", "wifi", "connect", ssid.as_ref()
                ])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Connected to {}", ssid), true)?;
            }
        }
        WifiAction::ConnectHidden => {
            info!("Connecting to hidden WiFi network");
            let ssid = crate::utils::prompt_for_ssid()
                .map_err(|e| NetworkMenuError::network_error(format!("Failed to get SSID: {}", e)))?;
            let password = crate::utils::prompt_for_password(&ssid)
                .map_err(|e| NetworkMenuError::network_error(format!("Failed to get password: {}", e)))?;
            
            let output = command_runner
                .run_command(commands::NMCLI, &[
                    "device", "wifi", "connect", &ssid,
                    "password", &password, "hidden", "yes"
                ])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Connected to {}", ssid), true)?;
            }
        }
        WifiAction::Disconnect => {
            info!("Disconnecting WiFi");
            let interface = context.wifi_interface.as_ref().map(|s| s.as_ref()).unwrap_or("wlan0");
            let output = command_runner
                .run_command(commands::NMCLI, &[
                    "device", "wifi", "disconnect", interface
                ])
                .await?;
            
            if output.status.success() {
                notify_connection("WiFi disconnected", false)?;
            }
        }
        WifiAction::Scan => {
            info!("Scanning for WiFi networks");
            let _ = command_runner
                .run_command(commands::NMCLI, &["device", "wifi", "rescan"])
                .await;
        }
        WifiAction::ForgetNetwork(ssid) => {
            info!("Forgetting WiFi network: {}", ssid);
            let output = command_runner
                .run_command(commands::NMCLI, &["connection", "delete", ssid.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Forgot network {}", ssid), false)?;
            }
        }
    }
    
    Ok(())
}

async fn execute_vpn_action(
    action: services::VpnAction,
    _context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    use crate::constants::commands;
    use crate::services::VpnAction;
    
    match action {
        VpnAction::Connect(name) => {
            info!("Connecting to VPN: {}", name);
            let output = command_runner
                .run_command(commands::NMCLI, &["connection", "up", name.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("VPN {} connected", name), true)?;
            }
        }
        VpnAction::Disconnect(name) => {
            info!("Disconnecting VPN: {}", name);
            let output = command_runner
                .run_command(commands::NMCLI, &["connection", "down", name.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("VPN {} disconnected", name), false)?;
            }
        }
        VpnAction::Toggle(name) => {
            info!("Toggling VPN: {}", name);
            let output = command_runner
                .run_command(commands::NMCLI, &["connection", "up", name.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("VPN {} toggled", name), true)?;
            }
        }
    }
    
    Ok(())
}

async fn execute_tailscale_action(
    action: services::TailscaleAction,
    _context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    use crate::constants::commands;
    use crate::services::TailscaleAction;
    
    match action {
        TailscaleAction::Enable => {
            info!("Enabling Tailscale");
            let output = command_runner
                .run_command(commands::TAILSCALE, &["up"])
                .await?;
            
            if output.status.success() {
                notify_connection("Tailscale enabled", true)?;
            }
        }
        TailscaleAction::Disable => {
            info!("Disabling Tailscale");
            let output = command_runner
                .run_command(commands::TAILSCALE, &["down"])
                .await?;
            
            if output.status.success() {
                notify_connection("Tailscale disabled", false)?;
            }
        }
        TailscaleAction::SetExitNode(node) => {
            info!("Setting Tailscale exit node: {}", node);
            let output = command_runner
                .run_command(commands::TAILSCALE, &["set", "--exit-node", node.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Exit node set to {}", node), true)?;
            }
        }
        TailscaleAction::DisableExitNode => {
            info!("Disabling Tailscale exit node");
            let output = command_runner
                .run_command(commands::TAILSCALE, &["set", "--exit-node="])
                .await?;
            
            if output.status.success() {
                notify_connection("Exit node disabled", false)?;
            }
        }
        TailscaleAction::SetShields(enabled) => {
            info!("Setting Tailscale shields: {}", enabled);
            let arg = if enabled { "--shields-up" } else { "--shields-up=false" };
            
            let output = command_runner
                .run_command(commands::TAILSCALE, &["set", arg])
                .await?;
            
            if output.status.success() {
                let status = if enabled { "up" } else { "down" };
                notify_connection(&format!("Shields {}", status), enabled)?;
            }
        }
        TailscaleAction::Netcheck => {
            info!("Running Tailscale netcheck");
            let _ = command_runner
                .run_command(commands::TAILSCALE, &["netcheck"])
                .await;
        }
    }
    
    Ok(())
}

async fn execute_bluetooth_action(
    action: services::BluetoothAction,
    _context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    use crate::constants::commands;
    use crate::services::BluetoothAction;
    
    match action {
        BluetoothAction::Connect(addr) => {
            info!("Connecting to Bluetooth device: {}", addr);
            let output = command_runner
                .run_command(commands::BLUETOOTHCTL, &["connect", addr.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Bluetooth device {} connected", addr), true)?;
            }
        }
        BluetoothAction::Disconnect(addr) => {
            info!("Disconnecting Bluetooth device: {}", addr);
            let output = command_runner
                .run_command(commands::BLUETOOTHCTL, &["disconnect", addr.as_ref()])
                .await?;
            
            if output.status.success() {
                notify_connection(&format!("Bluetooth device {} disconnected", addr), false)?;
            }
        }
        BluetoothAction::Pair(addr) => {
            info!("Pairing with Bluetooth device: {}", addr);
            let _ = command_runner
                .run_command(commands::BLUETOOTHCTL, &["pair", addr.as_ref()])
                .await;
        }
        BluetoothAction::Unpair(addr) => {
            info!("Unpairing Bluetooth device: {}", addr);
            let _ = command_runner
                .run_command(commands::BLUETOOTHCTL, &["remove", addr.as_ref()])
                .await;
        }
        BluetoothAction::Trust(addr) => {
            info!("Trusting Bluetooth device: {}", addr);
            let _ = command_runner
                .run_command(commands::BLUETOOTHCTL, &["trust", addr.as_ref()])
                .await;
        }
        BluetoothAction::Scan => {
            info!("Scanning for Bluetooth devices");
            let _ = command_runner
                .run_command(commands::BLUETOOTHCTL, &["scan", "on"])
                .await;
        }
    }
    
    Ok(())
}

async fn execute_system_action(
    action: services::SystemAction,
    _context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    use crate::constants::commands;
    use crate::services::SystemAction;
    
    match action {
        SystemAction::EditConnections => {
            info!("Opening network connection editor");
            let _ = command_runner
                .run_command(commands::NM_CONNECTION_EDITOR, &[])
                .await;
        }
        SystemAction::RfkillBlock => {
            info!("Blocking WiFi with rfkill");
            let output = command_runner
                .run_command(commands::RFKILL, &["block", "wifi"])
                .await?;
            
            if output.status.success() {
                notify_connection("WiFi blocked", false)?;
            }
        }
        SystemAction::RfkillUnblock => {
            info!("Unblocking WiFi with rfkill");
            let output = command_runner
                .run_command(commands::RFKILL, &["unblock", "wifi"])
                .await?;
            
            if output.status.success() {
                notify_connection("WiFi unblocked", true)?;
            }
        }
        SystemAction::RestartNetworkManager => {
            info!("Restarting NetworkManager");
            let _ = command_runner
                .run_command("systemctl", &["restart", "NetworkManager"])
                .await;
        }
        SystemAction::ShowNetworks => {
            info!("Showing network information");
            let _ = command_runner
                .run_command("nmcli", &["device", "status"])
                .await;
        }
    }
    
    Ok(())
}

async fn execute_custom_action(
    action: services::CustomAction,
    _context: &ActionContext,
    command_runner: &dyn crate::command::CommandRunner,
) -> Result<()> {
    info!("Executing custom action: {}", action.display);
    
    if action.confirm {
        debug!("Confirmation dialogs not yet implemented, executing anyway");
    }
    
    let args: Vec<&str> = action.args.iter().map(|s| s.as_str()).collect();
    let output = command_runner
        .run_command(&action.command, &args)
        .await?;
    
    if output.status.success() {
        info!("Custom action completed successfully");
    } else {
        debug!("Custom action failed with exit code: {:?}", output.status.code());
    }
    
    Ok(())
}

fn notify_connection(message: &str, connected: bool) -> Result<()> {
    use notify_rust::Notification;
    
    let summary = if connected {
        "Network Connected"
    } else {
        "Network Disconnected"
    };
    
    if let Err(e) = Notification::new()
        .summary(summary)
        .body(message)
        .show()
    {
        debug!("Failed to show notification: {}", e);
    }
    
    Ok(())
}