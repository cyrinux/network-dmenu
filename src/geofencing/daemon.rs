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
                Self::location_monitoring_loop(zone_manager, status, should_shutdown, scan_interval).await;
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
                    change.from.as_ref().map(|z| z.name.as_str()).unwrap_or("None"),
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
                match super::fingerprinting::create_wifi_fingerprint(super::PrivacyMode::High).await {
                    Ok(fingerprint) => DaemonResponse::LocationUpdate { fingerprint },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to get location: {}", e),
                    },
                }
            },
            
            DaemonCommand::GetActiveZone => {
                let manager = zone_manager.lock().await;
                let zone = manager.get_current_zone().cloned();
                DaemonResponse::ActiveZone { zone }
            },
            
            DaemonCommand::ListZones => {
                let manager = zone_manager.lock().await;
                let zones = manager.list_zones();
                DaemonResponse::ZoneList { zones }
            },
            
            DaemonCommand::CreateZone { name, actions } => {
                let mut manager = zone_manager.lock().await;
                match manager.create_zone_from_current_location(name, actions).await {
                    Ok(zone) => DaemonResponse::ZoneCreated { zone },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to create zone: {}", e),
                    },
                }
            },
            
            DaemonCommand::RemoveZone { zone_id } => {
                let mut manager = zone_manager.lock().await;
                match manager.remove_zone(&zone_id) {
                    Ok(_) => DaemonResponse::Success,
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to remove zone: {}", e),
                    },
                }
            },
            
            DaemonCommand::ActivateZone { zone_id } => {
                let mut manager = zone_manager.lock().await;
                match manager.activate_zone(&zone_id) {
                    Ok(change) => {
                        // Execute zone actions
                        if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await {
                            return DaemonResponse::Error {
                                message: format!("Zone activated but actions failed: {}", e),
                            };
                        }
                        
                        DaemonResponse::ZoneChanged {
                            from_zone_id: change.from.map(|z| z.id),
                            to_zone: change.to,
                            confidence: change.confidence,
                        }
                    },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to activate zone: {}", e),
                    },
                }
            },
            
            DaemonCommand::ExecuteActions { actions } => {
                match Self::execute_zone_actions(&actions).await {
                    Ok(_) => DaemonResponse::Success,
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to execute actions: {}", e),
                    },
                }
            },
            
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
            },
            
            DaemonCommand::Shutdown => {
                info!("Received shutdown command from client");
                *should_shutdown.write().await = true;
                DaemonResponse::Success
            },
        }
    }
    
    /// Execute zone actions (connect to WiFi, VPN, etc.)
    async fn execute_zone_actions(actions: &ZoneActions) -> Result<()> {
        info!("Executing zone actions: {:?}", actions);
        
        // Connect to WiFi
        if let Some(ref wifi_ssid) = actions.wifi {
            info!("Connecting to WiFi: {}", wifi_ssid);
            // TODO: Implement WiFi connection using existing networkmanager code
            // For now, just log the action
            debug!("Would connect to WiFi: {}", wifi_ssid);
        }
        
        // Connect to VPN
        if let Some(ref vpn_name) = actions.vpn {
            info!("Connecting to VPN: {}", vpn_name);
            // TODO: Implement VPN connection
            debug!("Would connect to VPN: {}", vpn_name);
        }
        
        // Configure Tailscale exit node
        if let Some(ref exit_node) = actions.tailscale_exit_node {
            info!("Setting Tailscale exit node: {}", exit_node);
            // TODO: Implement Tailscale exit node switching
            debug!("Would set Tailscale exit node: {}", exit_node);
        }
        
        // Configure Tailscale shields
        if let Some(shields_up) = actions.tailscale_shields {
            info!("Setting Tailscale shields: {}", if shields_up { "up" } else { "down" });
            // TODO: Implement Tailscale shields control
            debug!("Would set Tailscale shields: {}", shields_up);
        }
        
        // Connect Bluetooth devices
        for device in &actions.bluetooth {
            info!("Connecting Bluetooth device: {}", device);
            // TODO: Implement Bluetooth connection
            debug!("Would connect Bluetooth device: {}", device);
        }
        
        // Execute custom commands
        for command in &actions.custom_commands {
            info!("Executing custom command: {}", command);
            // TODO: Implement safe custom command execution
            debug!("Would execute custom command: {}", command);
        }
        
        Ok(())
    }
    
    /// Send zone change notification
    fn send_zone_change_notification(change: &LocationChange) {
        let title = "Network Zone Changed";
        let body = format!(
            "Switched to {} zone",
            change.to.name
        );
        
        // TODO: Implement notification using notify-rust
        info!("Notification: {} - {}", title, body);
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