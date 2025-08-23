use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::fs;
use tokio::process::Command as AsyncCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfkillDevice {
    pub id: u32,
    #[serde(rename = "type")]
    pub device_type: String,
    pub device: String,
    pub soft: String,
    pub hard: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RfkillDevices {
    pub rfkilldevices: Vec<RfkillDevice>,
}

#[derive(Debug, Clone)]
pub enum BlockState {
    Blocked,
    Unblocked,
}

impl From<&str> for BlockState {
    fn from(s: &str) -> Self {
        match s {
            "blocked" => BlockState::Blocked,
            "unblocked" => BlockState::Unblocked,
            _ => BlockState::Unblocked, // Default to unblocked for unknown states
        }
    }
}

impl BlockState {
    pub fn is_blocked(&self) -> bool {
        matches!(self, BlockState::Blocked)
    }

    #[cfg(test)]
    pub fn is_unblocked(&self) -> bool {
        matches!(self, BlockState::Unblocked)
    }
}

impl RfkillDevice {
    /// Check if the device is soft blocked
    pub fn is_soft_blocked(&self) -> bool {
        BlockState::from(self.soft.as_str()).is_blocked()
    }

    /// Check if the device is hard blocked
    pub fn is_hard_blocked(&self) -> bool {
        BlockState::from(self.hard.as_str()).is_blocked()
    }

    /// Check if the device is blocked (either soft or hard)
    pub fn is_blocked(&self) -> bool {
        self.is_soft_blocked() || self.is_hard_blocked()
    }

    /// Check if the device is completely unblocked
    pub fn is_unblocked(&self) -> bool {
        !self.is_blocked()
    }

    /// Get the device type as a more user-friendly string
    pub fn device_type_display(&self) -> &str {
        match self.device_type.as_str() {
            "wlan" => "WiFi",
            "bluetooth" => "Bluetooth",
            "nfc" => "NFC",
            "uwb" => "UWB",
            "wimax" => "WiMAX",
            "wwan" => "WWAN",
            "gps" => "GPS",
            "fm" => "FM",
            _ => &self.device_type,
        }
    }
}

/// Get all rfkill devices
///
/// # Errors
///
/// Returns an error if the rfkill command fails to execute, returns a non-zero status,
/// or if the output cannot be parsed as JSON.
pub async fn get_rfkill_devices() -> Result<Vec<RfkillDevice>, Box<dyn std::error::Error>> {
    let output = AsyncCommand::new("rfkill")
        .arg("-J")
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "rfkill command failed: {stderr}",
            stderr = String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let rfkill_data: RfkillDevices = serde_json::from_str(&stdout)?;

    Ok(rfkill_data.rfkilldevices)
}

/// Get rfkill devices of a specific type
///
/// # Errors
///
/// Returns an error if fetching the rfkill devices fails.
pub async fn get_rfkill_devices_by_type(device_type: &str) -> Result<Vec<RfkillDevice>, Box<dyn std::error::Error>> {
    let devices = get_rfkill_devices().await?;
    Ok(devices.into_iter()
        .filter(|device| device.device_type == device_type)
        .collect())
}

/// Block a device by ID
///
/// # Errors
///
/// Returns an error if the rfkill block command fails to execute or returns a non-zero status.
pub async fn block_device(device_id: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output = AsyncCommand::new("rfkill")
        .arg("block")
        .arg(device_id.to_string())
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to block device {device_id}: {stderr}",
            device_id = device_id,
            stderr = String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    Ok(())
}

/// Unblock a device by ID
///
/// # Errors
///
/// Returns an error if the rfkill unblock command fails to execute or returns a non-zero status.
pub async fn unblock_device(device_id: u32) -> Result<(), Box<dyn std::error::Error>> {
    let output = AsyncCommand::new("rfkill")
        .arg("unblock")
        .arg(device_id.to_string())
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to unblock device {device_id}: {stderr}",
            device_id = device_id,
            stderr = String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    Ok(())
}

/// Block all devices of a specific type
///
/// # Errors
///
/// Returns an error if the rfkill block command fails to execute or returns a non-zero status.
pub async fn block_device_type(device_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = AsyncCommand::new("rfkill")
        .arg("block")
        .arg(device_type)
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to block {device_type} devices: {stderr}",
            device_type = device_type,
            stderr = String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    Ok(())
}

/// Unblock all devices of a specific type
///
/// # Errors
///
/// Returns an error if the rfkill unblock command fails to execute or returns a non-zero status.
pub async fn unblock_device_type(device_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = AsyncCommand::new("rfkill")
        .arg("unblock")
        .arg(device_type)
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to unblock {device_type} devices: {stderr}",
            device_type = device_type,
            stderr = String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    Ok(())
}

/// Check if rfkill command is available
pub fn is_rfkill_available() -> bool {
    Command::new("rfkill")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get a summary of device states by type
///
/// # Errors
///
/// Returns an error if fetching the rfkill devices fails.
pub async fn get_device_type_summary() -> Result<std::collections::HashMap<String, (usize, usize)>, Box<dyn std::error::Error>> {
    let devices = get_rfkill_devices().await?;
    let mut summary = std::collections::HashMap::new();

    for device in devices {
        let entry = summary.entry(device.device_type.clone()).or_insert((0, 0));
        if device.is_blocked() {
            entry.0 += 1; // blocked count
        } else {
            entry.1 += 1; // unblocked count
        }
    }

    Ok(summary)
}

/// Gets the path to the rfkill devices cache file.
pub fn get_rfkill_cache_path() -> String {
    // Try to use XDG_CACHE_HOME first
    if let Ok(cache_home) = std::env::var("XDG_CACHE_HOME") {
        let path = PathBuf::from(cache_home);
        let cache_dir = path.join("network-dmenu");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            eprintln!("Warning: Could not create cache directory: {e}");
            return String::from("/tmp/rfkill_devices_cache.json");
        }
        return cache_dir.join("rfkill_devices.json").to_string_lossy().to_string();
    }

    // Fall back to $HOME/.cache
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home);
        let cache_dir = path.join(".cache").join("network-dmenu");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            eprintln!("Warning: Could not create cache directory: {e}");
            return String::from("/tmp/rfkill_devices_cache.json");
        }
        return cache_dir.join("rfkill_devices.json").to_string_lossy().to_string();
    }

    // Last resort: use /tmp
    String::from("/tmp/rfkill_devices_cache.json")
}

/// Loads rfkill devices from cache file.
pub fn load_rfkill_devices_from_cache() -> Vec<RfkillDevice> {
    match std::fs::read_to_string(get_rfkill_cache_path()) {
        Ok(cache) => match serde_json::from_str::<Vec<RfkillDevice>>(&cache) {
            Ok(devices) => devices,
            Err(e) => {
                eprintln!("Warning: Could not parse rfkill cache: {e}");
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    }
}

/// Saves rfkill devices to cache file.
///
/// # Errors
///
/// Returns an error if creating the cache directory fails, if serializing the devices to JSON fails,
/// or if writing to the cache file fails.
pub fn save_rfkill_devices_to_cache(devices: &[RfkillDevice]) -> Result<(), Box<dyn std::error::Error>> {
    let cache_path = get_rfkill_cache_path();

    // Ensure parent directory exists
    if let Some(parent) = PathBuf::from(&cache_path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let json = serde_json::to_string(devices)?;
    fs::write(cache_path, json)?;
    Ok(())
}

/// Caches all rfkill devices for use in non-async functions.
///
/// # Errors
///
/// Returns an error if saving the rfkill devices to the cache file fails.
pub async fn cache_rfkill_devices() -> Result<(), Box<dyn std::error::Error>> {
    let all_devices = get_rfkill_devices().await.unwrap_or_default();

    if !all_devices.is_empty() {
        save_rfkill_devices_to_cache(&all_devices)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_state_from_str() {
        assert!(matches!(BlockState::from("blocked"), BlockState::Blocked));
        assert!(matches!(BlockState::from("unblocked"), BlockState::Unblocked));
        assert!(matches!(BlockState::from("unknown"), BlockState::Unblocked));
    }

    #[test]
    fn test_block_state_methods() {
        let blocked = BlockState::Blocked;
        let unblocked = BlockState::Unblocked;

        assert!(blocked.is_blocked());
        assert!(!blocked.is_unblocked());
        assert!(!unblocked.is_blocked());
        assert!(unblocked.is_unblocked());
    }

    #[test]
    fn test_rfkill_device_methods() {
        let device = RfkillDevice {
            id: 0,
            device_type: "bluetooth".to_string(),
            device: "hci0".to_string(),
            soft: "blocked".to_string(),
            hard: "unblocked".to_string(),
        };

        assert!(device.is_soft_blocked());
        assert!(!device.is_hard_blocked());
        assert!(device.is_blocked());
        assert!(!device.is_unblocked());
    }

    #[test]
    fn test_device_type_display() {
        let wifi_device = RfkillDevice {
            id: 1,
            device_type: "wlan".to_string(),
            device: "phy0".to_string(),
            soft: "unblocked".to_string(),
            hard: "unblocked".to_string(),
        };

        let bt_device = RfkillDevice {
            id: 0,
            device_type: "bluetooth".to_string(),
            device: "hci0".to_string(),
            soft: "unblocked".to_string(),
            hard: "unblocked".to_string(),
        };

        assert_eq!(wifi_device.device_type_display(), "WiFi");
        assert_eq!(bt_device.device_type_display(), "Bluetooth");
    }

    #[test]
    fn test_json_parsing() {
        let json_data = r#"
        {
            "rfkilldevices": [
                {
                    "id": 0,
                    "type": "bluetooth",
                    "device": "hci0",
                    "soft": "unblocked",
                    "hard": "unblocked"
                },
                {
                    "id": 1,
                    "type": "wlan",
                    "device": "phy0",
                    "soft": "blocked",
                    "hard": "unblocked"
                }
            ]
        }
        "#;

        let parsed: RfkillDevices = serde_json::from_str(json_data).unwrap();
        assert_eq!(parsed.rfkilldevices.len(), 2);

        let bt_device = &parsed.rfkilldevices[0];
        assert_eq!(bt_device.id, 0);
        assert_eq!(bt_device.device_type, "bluetooth");
        assert_eq!(bt_device.device, "hci0");
        assert!(!bt_device.is_blocked());

        let wifi_device = &parsed.rfkilldevices[1];
        assert_eq!(wifi_device.id, 1);
        assert_eq!(wifi_device.device_type, "wlan");
        assert_eq!(wifi_device.device, "phy0");
        assert!(wifi_device.is_blocked());
    }
}
