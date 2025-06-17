use super::*;
use crate::command::{CommandRunner, RealCommandRunner};
use crate::constants::{commands, nmcli_commands, iwd_commands};
use crate::errors::{NetworkMenuError, Result};
use crate::parsers::wifi::{parse_networkmanager_wifi, parse_iwd_wifi};
use crate::types::{ActionContext, WifiNetwork};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// WiFi service for managing wireless network connections
#[derive(Debug)]
pub struct WifiService {
    command_runner: Box<dyn CommandRunner>,
    backend: WifiBackend,
}

#[derive(Debug, Clone)]
pub enum WifiBackend {
    NetworkManager,
    Iwd,
    Auto,
}

impl WifiService {
    pub fn new() -> Self {
        Self {
            command_runner: Box::new(RealCommandRunner::new()),
            backend: WifiBackend::Auto,
        }
    }
    
    pub fn with_backend(mut self, backend: WifiBackend) -> Self {
        self.backend = backend;
        self
    }
    
    pub fn with_command_runner(mut self, runner: Box<dyn CommandRunner>) -> Self {
        self.command_runner = runner;
        self
    }
    
    async fn detect_backend(&self) -> WifiBackend {
        if self.command_runner.is_command_available(commands::NMCLI).await {
            WifiBackend::NetworkManager
        } else if self.command_runner.is_command_available(commands::IWCTL).await {
            WifiBackend::Iwd
        } else {
            WifiBackend::NetworkManager // Default fallback
        }
    }
    
    async fn get_wifi_networks(&self, context: &ActionContext) -> Result<Vec<WifiNetwork>> {
        let backend = match self.backend {
            WifiBackend::Auto => self.detect_backend().await,
            ref b => b.clone(),
        };
        
        match backend {
            WifiBackend::NetworkManager => self.get_networkmanager_networks(context).await,
            WifiBackend::Iwd => self.get_iwd_networks(context).await,
            WifiBackend::Auto => unreachable!("Auto backend should be resolved"),
        }
    }
    
    async fn get_networkmanager_networks(&self, context: &ActionContext) -> Result<Vec<WifiNetwork>> {
        debug!("Getting WiFi networks from NetworkManager");
        
        // First try to get networks, rescan if none are connected
        let output = self.command_runner
            .run_command(commands::NMCLI, nmcli_commands::WIFI_LIST)
            .await?;
        
        if !output.status.success() {
            return Err(NetworkMenuError::command_failed(
                "nmcli wifi list",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut networks = parse_networkmanager_wifi(&stdout)?;
        
        // If no networks are connected and auto_scan is enabled, trigger a rescan
        if context.config.wifi.auto_scan && !networks.iter().any(|n| n.is_connected) {
            debug!("No connected networks found, triggering rescan");
            let rescan_output = self.command_runner
                .run_command(commands::NMCLI, nmcli_commands::WIFI_LIST_RESCAN)
                .await;
            
            if let Ok(rescan_output) = rescan_output {
                if rescan_output.status.success() {
                    let rescan_stdout = String::from_utf8_lossy(&rescan_output.stdout);
                    if let Ok(rescanned_networks) = parse_networkmanager_wifi(&rescan_stdout) {
                        networks = rescanned_networks;
                    }
                }
            }
        }
        
        // Filter by preferred networks if configured
        if !context.config.wifi.preferred_networks.is_empty() {
            let preferred_set: std::collections::HashSet<_> = 
                context.config.wifi.preferred_networks.iter().collect();
            
            networks.sort_by(|a, b| {
                let a_preferred = preferred_set.contains(&a.ssid.as_ref());
                let b_preferred = preferred_set.contains(&b.ssid.as_ref());
                
                match (a_preferred, b_preferred) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.ssid.cmp(&b.ssid),
                }
            });
        }
        
        Ok(networks)
    }
    
    async fn get_iwd_networks(&self, context: &ActionContext) -> Result<Vec<WifiNetwork>> {
        debug!("Getting WiFi networks from iwd");
        
        let interface = context.wifi_interface
            .as_ref()
            .ok_or_else(|| NetworkMenuError::config_error("WiFi interface not specified for iwd"))?;
        
        // First trigger a scan if auto_scan is enabled
        if context.config.wifi.auto_scan {
            let _ = self.command_runner
                .run_command(commands::IWCTL, &[
                    iwd_commands::STATION_SCAN[0],
                    interface.as_ref(),
                    iwd_commands::STATION_SCAN[1],
                ])
                .await;
        }
        
        // Get available networks
        let output = self.command_runner
            .run_command(commands::IWCTL, &[
                iwd_commands::STATION_GET_NETWORKS[0],
                interface.as_ref(),
                iwd_commands::STATION_GET_NETWORKS[1],
            ])
            .await?;
        
        if !output.status.success() {
            return Err(NetworkMenuError::command_failed(
                "iwctl station get-networks",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_iwd_wifi(&stdout)
    }
    
    async fn is_connected(&self, context: &ActionContext) -> Result<bool> {
        let backend = match self.backend {
            WifiBackend::Auto => self.detect_backend().await,
            ref b => b.clone(),
        };
        
        match backend {
            WifiBackend::NetworkManager => self.is_networkmanager_connected(context).await,
            WifiBackend::Iwd => self.is_iwd_connected(context).await,
            WifiBackend::Auto => unreachable!("Auto backend should be resolved"),
        }
    }
    
    async fn is_networkmanager_connected(&self, context: &ActionContext) -> Result<bool> {
        let interface = context.wifi_interface.as_ref().map(|s| s.as_ref()).unwrap_or("wlan0");
        
        let output = self.command_runner
            .run_command(commands::NMCLI, &[
                "--colors", "no", "-t", "-f", "DEVICE,STATE",
                "device", "status"
            ])
            .await?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && parts[0] == interface {
                return Ok(parts[1] == "connected");
            }
        }
        
        Ok(false)
    }
    
    async fn is_iwd_connected(&self, context: &ActionContext) -> Result<bool> {
        let interface = context.wifi_interface
            .as_ref()
            .ok_or_else(|| NetworkMenuError::config_error("WiFi interface not specified for iwd"))?;
        
        let output = self.command_runner
            .run_command(commands::IWCTL, &[
                iwd_commands::STATION_GET_NETWORKS[0],
                interface.as_ref(),
                iwd_commands::STATION_GET_NETWORKS[1],
            ])
            .await?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim().starts_with('>')))
    }
}

impl Default for WifiService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkService for WifiService {
    async fn get_actions(&self, context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut actions = Vec::new();
        
        // Get available networks and create connect actions
        match self.get_wifi_networks(context).await {
            Ok(networks) => {
                for network in networks {
                    if !network.is_connected {
                        actions.push(ActionType::Wifi(WifiAction::Connect(network.ssid.clone())));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to get WiFi networks: {}", e);
            }
        }
        
        // Add connection state actions
        match self.is_connected(context).await {
            Ok(true) => {
                actions.push(ActionType::Wifi(WifiAction::Disconnect));
            }
            Ok(false) => {
                actions.push(ActionType::Wifi(WifiAction::Scan));
                actions.push(ActionType::Wifi(WifiAction::ConnectHidden));
            }
            Err(e) => {
                warn!("Failed to check WiFi connection status: {}", e);
            }
        }
        
        debug!("WiFi service generated {} actions", actions.len());
        Ok(actions)
    }
    
    async fn is_available(&self) -> bool {
        let backend = match self.backend {
            WifiBackend::Auto => self.detect_backend().await,
            ref b => b.clone(),
        };
        
        match backend {
            WifiBackend::NetworkManager => {
                self.command_runner.is_command_available(commands::NMCLI).await
            }
            WifiBackend::Iwd => {
                self.command_runner.is_command_available(commands::IWCTL).await
            }
            WifiBackend::Auto => unreachable!("Auto backend should be resolved"),
        }
    }
    
    fn service_name(&self) -> &'static str {
        "WiFi"
    }
    
    async fn initialize(&mut self) -> Result<()> {
        let backend = match self.backend {
            WifiBackend::Auto => {
                let detected = self.detect_backend().await;
                self.backend = detected.clone();
                detected
            }
            ref b => b.clone(),
        };
        
        info!("WiFi service initialized with backend: {:?}", backend);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;
    use crate::types::Config;
    use std::process::{ExitStatus, Output};
    
    fn create_mock_output(success: bool, stdout: &str, stderr: &str) -> Output {
        Output {
            status: if success {
                ExitStatus::from_raw(0)
            } else {
                ExitStatus::from_raw(1)
            },
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
    
    #[tokio::test]
    async fn test_wifi_service_networkmanager() {
        let nm_output = "*:HomeWiFi:▂▄▆█:WPA2\n:PublicWiFi:▂▄__:";
        let mock_output = create_mock_output(true, nm_output, "");
        
        let runner = MockCommandRunner::new()
            .with_response("nmcli", nmcli_commands::WIFI_LIST, mock_output)
            .with_available_command("nmcli");
        
        let service = WifiService::new()
            .with_backend(WifiBackend::NetworkManager)
            .with_command_runner(Box::new(runner));
        
        assert!(service.is_available().await);
        
        let context = ActionContext::new(Config::default());
        let actions = service.get_actions(&context).await.unwrap();
        
        // Should have at least one connect action for PublicWiFi
        assert!(!actions.is_empty());
        
        let has_connect_action = actions.iter().any(|action| {
            matches!(action, ActionType::Wifi(WifiAction::Connect(ssid)) if ssid.as_ref() == "PublicWiFi")
        });
        assert!(has_connect_action);
    }
    
    #[tokio::test]
    async fn test_wifi_service_unavailable() {
        let runner = MockCommandRunner::new()
            .with_default_success(false);
        
        let service = WifiService::new()
            .with_backend(WifiBackend::NetworkManager)
            .with_command_runner(Box::new(runner));
        
        assert!(!service.is_available().await);
    }
    
    #[tokio::test]
    async fn test_backend_detection() {
        let runner = MockCommandRunner::new()
            .with_available_command("nmcli");
        
        let service = WifiService::new()
            .with_backend(WifiBackend::Auto)
            .with_command_runner(Box::new(runner));
        
        let detected = service.detect_backend().await;
        assert!(matches!(detected, WifiBackend::NetworkManager));
    }
}