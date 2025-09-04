//! Geofencing daemon for background location monitoring
//!
//! Runs in the background to continuously monitor location changes
//! and automatically execute zone-based actions.

use super::{
    ipc::{DaemonCommand, DaemonIpcServer, DaemonResponse, DaemonStatus},
    zones::ZoneManager,
    GeofencingConfig, LocationChange, Result, ZoneActions,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;

// Import network functions from the main codebase
use crate::command::{CommandRunner, RealCommandRunner};

/// Main geofencing daemon
pub struct GeofencingDaemon {
    zone_manager: Arc<Mutex<ZoneManager>>,
    config: GeofencingConfig,
    status: Arc<RwLock<DaemonStatusData>>,
    should_shutdown: Arc<RwLock<bool>>,
}

/// Internal daemon status data
#[derive(Debug)]
struct DaemonStatusData {
    monitoring: bool,
    last_scan: Option<chrono::DateTime<chrono::Utc>>,
    total_zone_changes: u32,
    startup_time: Instant,
    current_zone_id: Option<String>,
}

impl GeofencingDaemon {
    /// Create new geofencing daemon
    pub fn new(config: GeofencingConfig) -> Self {
        let zone_manager = Arc::new(Mutex::new(ZoneManager::new(config.clone())));

        let status = Arc::new(RwLock::new(DaemonStatusData {
            monitoring: false,
            last_scan: None,
            total_zone_changes: 0,
            startup_time: Instant::now(),
            current_zone_id: None,
        }));

        Self {
            zone_manager,
            config,
            status,
            should_shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the daemon
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting geofencing daemon");

        // Update status
        {
            let mut status = self.status.write().await;
            status.monitoring = true;
        }

        // Start IPC server
        let mut ipc_server = DaemonIpcServer::new().await?;

        // Clone references for tasks
        let zone_manager = Arc::clone(&self.zone_manager);
        let status = Arc::clone(&self.status);
        let should_shutdown = Arc::clone(&self.should_shutdown);
        let scan_interval = Duration::from_secs(self.config.scan_interval_seconds);

        // Start location monitoring task
        let monitor_task = {
            let zone_manager = Arc::clone(&zone_manager);
            let status = Arc::clone(&status);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::location_monitoring_loop(
                    zone_manager,
                    status,
                    should_shutdown,
                    scan_interval,
                )
                .await;
            })
        };

        // Handle IPC commands
        let ipc_task = {
            let zone_manager = Arc::clone(&self.zone_manager);
            let status = Arc::clone(&self.status);
            let should_shutdown = Arc::clone(&self.should_shutdown);

            tokio::spawn(async move {
                let command_handler = move |cmd| {
                    Self::handle_ipc_command(
                        Arc::clone(&zone_manager),
                        Arc::clone(&status),
                        Arc::clone(&should_shutdown),
                        cmd,
                    )
                };

                if let Err(e) = ipc_server.handle_connections(command_handler).await {
                    error!("IPC server error: {}", e);
                }
            })
        };

        // Wait for shutdown signal or tasks to complete
        tokio::select! {
            _ = monitor_task => {
                info!("Location monitoring task completed");
            },
            _ = ipc_task => {
                info!("IPC task completed");
            },
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                *self.should_shutdown.write().await = true;
            }
        }

        info!("Geofencing daemon shutting down");
        Ok(())
    }

    /// Main location monitoring loop
    async fn location_monitoring_loop(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        scan_interval: Duration,
    ) {
        let mut interval = interval(scan_interval);

        loop {
            interval.tick().await;

            // Check for shutdown
            if *should_shutdown.read().await {
                break;
            }

            // Skip if no zones configured
            {
                let manager = zone_manager.lock().await;
                if manager.list_zones().is_empty() {
                    debug!("No zones configured, skipping scan");
                    continue;
                }
            }

            // Check for location change
            let location_change = {
                let mut manager = zone_manager.lock().await;
                match manager.detect_location_change().await {
                    Ok(change) => change,
                    Err(e) => {
                        warn!("Location detection failed: {}", e);
                        continue;
                    }
                }
            };

            // Update status
            {
                let mut status_data = status.write().await;
                status_data.last_scan = Some(Utc::now());

                if let Some(ref change) = location_change {
                    status_data.total_zone_changes += 1;
                    status_data.current_zone_id = Some(change.to.id.clone());
                }
            }

            // Handle zone change
            if let Some(change) = location_change {
                info!(
                    "Zone change detected: {} -> {} (confidence: {:.2})",
                    change
                        .from
                        .as_ref()
                        .map(|z| z.name.as_str())
                        .unwrap_or("None"),
                    change.to.name,
                    change.confidence
                );

                // Execute zone actions
                if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await {
                    error!("Failed to execute zone actions: {}", e);
                }

                // Send notification if enabled
                if change.suggested_actions.notifications {
                    Self::send_zone_change_notification(&change);
                }
            }
        }

        info!("Location monitoring loop stopped");
    }

    /// Handle IPC commands from clients
    async fn handle_ipc_command(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        command: DaemonCommand,
    ) -> DaemonResponse {
        debug!("Handling IPC command: {:?}", command);

        match command {
            DaemonCommand::GetCurrentLocation => {
                match super::fingerprinting::create_wifi_fingerprint(super::PrivacyMode::High).await
                {
                    Ok(fingerprint) => DaemonResponse::LocationUpdate { fingerprint },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to get location: {}", e),
                    },
                }
            }

            DaemonCommand::GetActiveZone => {
                let manager = zone_manager.lock().await;
                let zone = manager.get_current_zone().cloned();
                DaemonResponse::ActiveZone { zone }
            }

            DaemonCommand::ListZones => {
                let manager = zone_manager.lock().await;
                let zones = manager.list_zones();
                DaemonResponse::ZoneList { zones }
            }

            DaemonCommand::CreateZone { name, actions } => {
                let mut manager = zone_manager.lock().await;
                match manager
                    .create_zone_from_current_location(name, actions)
                    .await
                {
                    Ok(zone) => DaemonResponse::ZoneCreated { zone },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to create zone: {}", e),
                    },
                }
            }

            DaemonCommand::RemoveZone { zone_id } => {
                let mut manager = zone_manager.lock().await;
                match manager.remove_zone(&zone_id) {
                    Ok(_) => DaemonResponse::Success,
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to remove zone: {}", e),
                    },
                }
            }

            DaemonCommand::ActivateZone { zone_id } => {
                let mut manager = zone_manager.lock().await;
                match manager.activate_zone(&zone_id) {
                    Ok(change) => {
                        // Execute zone actions
                        if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await
                        {
                            return DaemonResponse::Error {
                                message: format!("Zone activated but actions failed: {}", e),
                            };
                        }

                        DaemonResponse::ZoneChanged {
                            from_zone_id: change.from.map(|z| z.id),
                            to_zone: change.to,
                            confidence: change.confidence,
                        }
                    }
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to activate zone: {}", e),
                    },
                }
            }

            DaemonCommand::AddFingerprint { zone_name } => {
                let mut manager = zone_manager.lock().await;
                match manager.add_fingerprint_to_zone(&zone_name).await {
                    Ok(true) => DaemonResponse::FingerprintAdded {
                        success: true,
                        message: format!("Added new fingerprint to zone '{}'", zone_name),
                    },
                    Ok(false) => DaemonResponse::FingerprintAdded {
                        success: false,
                        message: format!("Fingerprint too similar to existing ones in zone '{}'", zone_name),
                    },
                    Err(e) => DaemonResponse::FingerprintAdded {
                        success: false,
                        message: format!("Failed to add fingerprint: {}", e),
                    },
                }
            }

            DaemonCommand::ExecuteActions { actions } => {
                match Self::execute_zone_actions(&actions).await {
                    Ok(_) => DaemonResponse::Success,
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to execute actions: {}", e),
                    },
                }
            }

            DaemonCommand::GetStatus => {
                let status_data = status.read().await;
                let daemon_status = DaemonStatus {
                    monitoring: status_data.monitoring,
                    zone_count: {
                        let manager = zone_manager.lock().await;
                        manager.list_zones().len()
                    },
                    active_zone_id: {
                        let manager = zone_manager.lock().await;
                        manager.get_current_zone().map(|z| z.id.clone())
                    },
                    last_scan: {
                        let manager = zone_manager.lock().await;
                        manager.get_last_scan()
                    },
                    total_zone_changes: {
                        let manager = zone_manager.lock().await;
                        manager.get_total_zone_changes()
                    },
                    uptime_seconds: status_data.startup_time.elapsed().as_secs(),
                };
                DaemonResponse::Status {
                    status: daemon_status,
                }
            }

            DaemonCommand::Shutdown => {
                info!("Received shutdown command from client");
                *should_shutdown.write().await = true;
                DaemonResponse::Success
            }
        }
    }

    /// Execute zone actions (connect to WiFi, VPN, etc.)
    async fn execute_zone_actions(actions: &ZoneActions) -> Result<()> {
        info!("Executing zone actions: {:?}", actions);

        // Connect to WiFi
        if let Some(ref wifi_ssid) = actions.wifi {
            info!("Connecting to WiFi: {}", wifi_ssid);

            let command_runner = RealCommandRunner;

            // Try NetworkManager first, then fall back to IWD
            let success = if crate::command::is_command_installed("nmcli") {
                // Use NetworkManager - attempt connection without password first
                let result =
                    command_runner.run_command("nmcli", &["device", "wifi", "connect", wifi_ssid]);

                match result {
                    Ok(output) if output.status.success() => {
                        info!("Successfully connected to WiFi: {}", wifi_ssid);
                        true
                    }
                    _ => {
                        warn!(
                            "Failed to connect to WiFi {} (may need password)",
                            wifi_ssid
                        );
                        false
                    }
                }
            } else if crate::command::is_command_installed("iwctl") {
                // Use IWD - attempt connection
                let result = command_runner
                    .run_command("iwctl", &["station", "wlan0", "connect", wifi_ssid]);

                match result {
                    Ok(output) if output.status.success() => {
                        info!("Successfully connected to WiFi: {}", wifi_ssid);
                        true
                    }
                    _ => {
                        warn!(
                            "Failed to connect to WiFi {} (may need password)",
                            wifi_ssid
                        );
                        false
                    }
                }
            } else {
                warn!("No WiFi manager (nmcli/iwctl) available");
                false
            };

            if !success {
                error!(
                    "WiFi connection to {} failed - geofencing may not work as expected",
                    wifi_ssid
                );
            }
        }

        // Connect to VPN
        if let Some(ref vpn_name) = actions.vpn {
            info!("Connecting to VPN: {}", vpn_name);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("nmcli") {
                let result = command_runner.run_command("nmcli", &["connection", "up", vpn_name]);

                match result {
                    Ok(output) if output.status.success() => {
                        info!("Successfully connected to VPN: {}", vpn_name);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        error!("Failed to connect to VPN {}: {}", vpn_name, stderr);
                    }
                    Err(e) => {
                        error!("Error connecting to VPN {}: {}", vpn_name, e);
                    }
                }
            } else {
                warn!("NetworkManager (nmcli) not available for VPN connection");
            }
        }

        // Configure Tailscale exit node
        if let Some(ref exit_node) = actions.tailscale_exit_node {
            info!("Setting Tailscale exit node: {}", exit_node);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("tailscale") {
                let result = command_runner
                    .run_command("tailscale", &["set", &format!("--exit-node={}", exit_node)]);

                match result {
                    Ok(output) if output.status.success() => {
                        info!("Successfully set Tailscale exit node: {}", exit_node);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        error!(
                            "Failed to set Tailscale exit node {}: {}",
                            exit_node, stderr
                        );
                    }
                    Err(e) => {
                        error!("Error setting Tailscale exit node {}: {}", exit_node, e);
                    }
                }
            } else {
                warn!("Tailscale not available for exit node configuration");
            }
        }

        // Configure Tailscale shields
        if let Some(shields_up) = actions.tailscale_shields {
            info!(
                "Setting Tailscale shields: {}",
                if shields_up { "up" } else { "down" }
            );

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("tailscale") {
                let shield_arg = if shields_up {
                    "--shields-up=true"
                } else {
                    "--shields-up=false"
                };
                let result = command_runner.run_command("tailscale", &["set", shield_arg]);

                match result {
                    Ok(output) if output.status.success() => {
                        info!(
                            "Successfully set Tailscale shields: {}",
                            if shields_up { "up" } else { "down" }
                        );
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        error!("Failed to set Tailscale shields: {}", stderr);
                    }
                    Err(e) => {
                        error!("Error setting Tailscale shields: {}", e);
                    }
                }
            } else {
                warn!("Tailscale not available for shields configuration");
            }
        }

        // Connect Bluetooth devices
        for device_name in &actions.bluetooth {
            info!("Connecting Bluetooth device: {}", device_name);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("bluetoothctl") {
                // First, try to find the device by name to get its address
                match command_runner.run_command("bluetoothctl", &["devices"]) {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let mut device_address: Option<String> = None;

                        // Look for the device by name in the output
                        for line in stdout.lines() {
                            if line.contains(device_name) {
                                // Extract MAC address from line format: "Device AA:BB:CC:DD:EE:FF Device Name"
                                if let Some(address_start) = line.find("Device ") {
                                    if let Some(address_end) = line[address_start + 7..].find(' ') {
                                        device_address = Some(
                                            line[address_start + 7
                                                ..address_start + 7 + address_end]
                                                .to_string(),
                                        );
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(address) = device_address {
                            // Try to connect using the address
                            match command_runner.run_command("bluetoothctl", &["connect", &address])
                            {
                                Ok(output) if output.status.success() => {
                                    info!(
                                        "Successfully connected to Bluetooth device: {} ({})",
                                        device_name, address
                                    );
                                }
                                _ => {
                                    warn!(
                                        "Failed to connect to Bluetooth device: {} ({})",
                                        device_name, address
                                    );
                                }
                            }
                        } else {
                            warn!(
                                "Bluetooth device '{}' not found in paired devices",
                                device_name
                            );
                        }
                    }
                    _ => {
                        error!("Failed to list Bluetooth devices for {}", device_name);
                    }
                }
            } else {
                warn!("bluetoothctl not available for Bluetooth connection");
            }
        }

        // Execute custom commands
        for command in &actions.custom_commands {
            info!("Executing custom command: {}", command);

            // Security: Only allow predefined safe commands or whitelist patterns
            if Self::is_safe_command(command) {
                match tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                {
                    Ok(output) => {
                        if output.status.success() {
                            info!("Successfully executed custom command: {}", command);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if !stdout.trim().is_empty() {
                                debug!("Command output: {}", stdout.trim());
                            }
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            error!(
                                "Custom command failed: {} - Error: {}",
                                command,
                                stderr.trim()
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to execute custom command '{}': {}", command, e);
                    }
                }
            } else {
                warn!("Skipped potentially unsafe custom command: {}", command);
            }
        }

        Ok(())
    }

    /// Check if a custom command is safe to execute
    /// This is a security measure to prevent dangerous commands
    fn is_safe_command(command: &str) -> bool {
        let command_lower = command.to_lowercase();

        // Block dangerous commands and patterns
        let dangerous_patterns = [
            "rm -rf",
            "rm -r",
            "sudo rm",
            "format",
            "mkfs",
            "dd if=",
            "fdisk",
            "parted",
            "> /dev/",
            "shutdown",
            "reboot",
            "halt",
            "iptables -F",
            "ufw --force",
            "chmod 777",
            "chmod -R 777",
            "curl",
            "wget",
            "nc ",
            "netcat",
            "telnet",
            "ssh ",
            "scp ",
            "python -c",
            "perl -e",
            "eval",
            "exec",
            "`",
            "$(",
            "passwd",
            "su ",
            "sudo su",
            "/etc/shadow",
            "/etc/passwd",
            "crontab",
            "/var/",
            "/etc/",
            "/root/",
            "/boot/",
        ];

        // Check for dangerous patterns
        for pattern in &dangerous_patterns {
            if command_lower.contains(pattern) {
                return false;
            }
        }

        // Allow safe commands (whitelist approach would be more secure)
        let safe_prefixes = [
            "systemctl --user start",
            "systemctl --user stop",
            "systemctl --user restart",
            "notify-send",
            "echo ",
            "printf ",
            "logger ",
            "touch /tmp/",
            "mkdir -p /tmp/",
        ];

        // Check for safe command prefixes
        for prefix in &safe_prefixes {
            if command_lower.starts_with(prefix) {
                return true;
            }
        }

        // For now, be conservative and reject unknown commands
        // In production, you might want a more sophisticated whitelist
        false
    }

    /// Send zone change notification
    fn send_zone_change_notification(change: &LocationChange) {
        let title = "Network Zone Changed";
        let body = if let Some(ref from_zone) = change.from {
            format!(
                "Switched from {} to {} zone\nConfidence: {:.0}%",
                from_zone.name,
                change.to.name,
                change.confidence * 100.0
            )
        } else {
            format!(
                "Entered {} zone\nConfidence: {:.0}%",
                change.to.name,
                change.confidence * 100.0
            )
        };

        // Send desktop notification
        if let Err(e) = notify_rust::Notification::new()
            .summary(title)
            .body(&body)
            .icon("network-wireless")
            .timeout(notify_rust::Timeout::Milliseconds(5000))
            .show()
        {
            warn!("Failed to send notification: {}", e);
        }

        info!("Zone change notification sent: {} - {}", title, body);
    }
}

/// Check if daemon is running
pub fn is_daemon_running() -> bool {
    std::path::Path::new("/tmp/network-dmenu-daemon.sock").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_creation() {
        let config = GeofencingConfig::default();
        let daemon = GeofencingDaemon::new(config);

        // Basic creation test
        assert!(!daemon.should_shutdown.try_read().unwrap());
    }

    #[test]
    fn test_daemon_running_check() {
        // Should be false in test environment
        assert!(!is_daemon_running());
    }

    #[tokio::test]
    async fn test_daemon_status_creation() {
        let config = GeofencingConfig::default();
        let daemon = GeofencingDaemon::new(config);

        let status_data = daemon.status.read().await;
        assert!(!status_data.monitoring);
        assert!(status_data.last_scan.is_none());
        assert_eq!(status_data.total_zone_changes, 0);
    }
}
