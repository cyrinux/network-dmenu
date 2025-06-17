use super::*;
use crate::command::{CommandRunner, RealCommandRunner};
use crate::constants::{commands, bluetooth_commands};
use crate::errors::{NetworkMenuError, Result};
use crate::types::{ActionContext, BluetoothDevice};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Bluetooth service for managing Bluetooth device connections
#[derive(Debug)]
pub struct BluetoothService {
    command_runner: Box<dyn CommandRunner>,
}

impl BluetoothService {
    pub fn new() -> Self {
        Self {
            command_runner: Box::new(RealCommandRunner::new()),
        }
    }
    
    pub fn with_command_runner(mut self, runner: Box<dyn CommandRunner>) -> Self {
        self.command_runner = runner;
        self
    }
    
    async fn get_paired_devices(&self) -> Result<Vec<BluetoothDevice>> {
        let output = self.command_runner
            .run_command(commands::BLUETOOTHCTL, bluetooth_commands::PAIRED_DEVICES)
            .await?;
        
        if !output.status.success() {
            return Ok(Vec::new());
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();
        
        for line in stdout.lines() {
            if let Some(device) = self.parse_device_line(line) {
                devices.push(device);
            }
        }
        
        Ok(devices)
    }
    
    fn parse_device_line(&self, line: &str) -> Option<BluetoothDevice> {
        // Expected format: "Device AA:BB:CC:DD:EE:FF Device Name"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 || parts[0] != "Device" {
            return None;
        }
        
        let address = parts[1];
        let name = parts[2..].join(" ");
        
        if name.is_empty() {
            return None;
        }
        
        Some(BluetoothDevice::new(name, address))
    }
    
    async fn is_device_connected(&self, address: &str) -> bool {
        let output = self.command_runner
            .run_command(commands::BLUETOOTHCTL, &["info", address])
            .await;
        
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.contains("Connected: yes");
            }
        }
        
        false
    }
}

impl Default for BluetoothService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkService for BluetoothService {
    async fn get_actions(&self, _context: &ActionContext) -> Result<Vec<ActionType>> {
        let mut actions = Vec::new();
        
        match self.get_paired_devices().await {
            Ok(devices) => {
                for device in devices {
                    let is_connected = self.is_device_connected(&device.address).await;
                    
                    if is_connected {
                        actions.push(ActionType::Bluetooth(BluetoothAction::Disconnect(device.address.clone())));
                    } else {
                        actions.push(ActionType::Bluetooth(BluetoothAction::Connect(device.address.clone())));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to get Bluetooth devices: {}", e);
            }
        }
        
        // Add scan action
        actions.push(ActionType::Bluetooth(BluetoothAction::Scan));
        
        debug!("Bluetooth service generated {} actions", actions.len());
        Ok(actions)
    }
    
    async fn is_available(&self) -> bool {
        self.command_runner.is_command_available(commands::BLUETOOTHCTL).await
    }
    
    fn service_name(&self) -> &'static str {
        "Bluetooth"
    }
    
    async fn initialize(&mut self) -> Result<()> {
        info!("Bluetooth service initialized");
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
    async fn test_bluetooth_service() {
        let devices_output = create_mock_output(
            true, 
            "Device AA:BB:CC:DD:EE:FF My Headphones\nDevice 11:22:33:44:55:66 My Mouse", 
            ""
        );
        
        let runner = MockCommandRunner::new()
            .with_response("bluetoothctl", bluetooth_commands::PAIRED_DEVICES, devices_output)
            .with_available_command("bluetoothctl");
        
        let service = BluetoothService::new()
            .with_command_runner(Box::new(runner));
        
        assert!(service.is_available().await);
        
        let context = ActionContext::new(Config::default());
        let actions = service.get_actions(&context).await.unwrap();
        
        // Should have connect actions for devices + scan action
        assert!(actions.len() >= 3); // 2 devices + scan
        
        let has_scan_action = actions.iter().any(|action| {
            matches!(action, ActionType::Bluetooth(BluetoothAction::Scan))
        });
        assert!(has_scan_action);
    }
    
    #[test]
    fn test_device_parsing() {
        let service = BluetoothService::new();
        
        let device = service.parse_device_line("Device AA:BB:CC:DD:EE:FF My Headphones").unwrap();
        assert_eq!(device.name.as_ref(), "My Headphones");
        assert_eq!(device.address.as_ref(), "AA:BB:CC:DD:EE:FF");
        
        // Invalid lines should return None
        assert!(service.parse_device_line("Invalid line").is_none());
        assert!(service.parse_device_line("Device AA:BB:CC:DD:EE:FF").is_none());
    }
}