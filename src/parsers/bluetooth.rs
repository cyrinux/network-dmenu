use crate::errors::{NetworkMenuError, Result};
use crate::parsers::{NetworkParser, ParsingPipeline, utils};
use crate::types::BluetoothDevice;
use std::sync::Arc;
use tracing::{debug, trace};

/// Parser for Bluetooth device information from bluetoothctl
#[derive(Debug)]
pub struct BluetoothParser;

impl BluetoothParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BluetoothParser {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkParser<BluetoothDevice> for BluetoothParser {
    fn parse(&self, input: &str) -> Result<BluetoothDevice> {
        let lines: Vec<&str> = input.lines().collect();
        if lines.is_empty() {
            return Err(NetworkMenuError::parse_error("Empty Bluetooth input"));
        }
        
        // For multi-line input, parse the first valid line
        for line in lines {
            if let Some(device) = self.parse_line(line)? {
                return Ok(device);
            }
        }
        
        Err(NetworkMenuError::parse_error("No valid Bluetooth device found in input"))
    }
    
    fn parse_line(&self, line: &str) -> Result<Option<BluetoothDevice>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        
        // Expected format: "Device AA:BB:CC:DD:EE:FF Device Name"
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 || parts[0] != "Device" {
            trace!("Skipping Bluetooth line with unexpected format: {}", line);
            return Ok(None);
        }
        
        let address = parts[1];
        let name = parts[2..].join(" ");
        
        if name.is_empty() || !is_valid_mac_address(address) {
            return Ok(None);
        }
        
        let device = BluetoothDevice::new(name, address);
        
        debug!("Parsed Bluetooth device: {} ({})", device.name, device.address);
        Ok(Some(device))
    }
    
    fn validate(&self, device: &BluetoothDevice) -> Result<()> {
        if device.name.is_empty() {
            return Err(NetworkMenuError::validation_error("Bluetooth device name cannot be empty"));
        }
        
        if device.address.is_empty() {
            return Err(NetworkMenuError::validation_error("Bluetooth device address cannot be empty"));
        }
        
        if !is_valid_mac_address(&device.address) {
            return Err(NetworkMenuError::validation_error(
                format!("Invalid MAC address: {}", device.address)
            ));
        }
        
        Ok(())
    }
}

/// Check if a string is a valid MAC address
fn is_valid_mac_address(addr: &str) -> bool {
    use regex::Regex;
    
    let mac_regex = Regex::new(r"^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$").unwrap();
    mac_regex.is_match(addr)
}

/// Create a parsing pipeline for Bluetooth devices
pub fn create_bluetooth_pipeline() -> ParsingPipeline<BluetoothDevice> {
    ParsingPipeline::new(Box::new(BluetoothParser::new()))
        .filter(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("Device"))
        .transform(|line| utils::strip_ansi_codes(&line))
        .transform(|line| utils::normalize_whitespace(&line))
}

/// Convenience function to parse bluetoothctl output
pub fn parse_bluetooth_devices(output: &str) -> Result<Vec<BluetoothDevice>> {
    create_bluetooth_pipeline().process(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_device_parsing() {
        let input = r#"Device AA:BB:CC:DD:EE:FF My Headphones
Device 11:22:33:44:55:66 My Mouse
Device FF-EE-DD-CC-BB-AA Bluetooth Speaker"#;
        
        let devices = parse_bluetooth_devices(input).unwrap();
        assert_eq!(devices.len(), 3);
        
        let headphones = &devices[0];
        assert_eq!(headphones.name.as_ref(), "My Headphones");
        assert_eq!(headphones.address.as_ref(), "AA:BB:CC:DD:EE:FF");
        
        let mouse = &devices[1];
        assert_eq!(mouse.name.as_ref(), "My Mouse");
        assert_eq!(mouse.address.as_ref(), "11:22:33:44:55:66");
        
        let speaker = &devices[2];
        assert_eq!(speaker.name.as_ref(), "Bluetooth Speaker");
        assert_eq!(speaker.address.as_ref(), "FF-EE-DD-CC-BB-AA");
    }

    #[test]
    fn test_bluetooth_validation() {
        let parser = BluetoothParser::new();
        
        let valid_device = BluetoothDevice::new("Test Device", "AA:BB:CC:DD:EE:FF");
        assert!(parser.validate(&valid_device).is_ok());
        
        let empty_name_device = BluetoothDevice::new("", "AA:BB:CC:DD:EE:FF");
        assert!(parser.validate(&empty_name_device).is_err());
        
        let empty_address_device = BluetoothDevice::new("Test", "");
        assert!(parser.validate(&empty_address_device).is_err());
        
        let invalid_address_device = BluetoothDevice::new("Test", "invalid-address");
        assert!(parser.validate(&invalid_address_device).is_err());
    }

    #[test]
    fn test_mac_address_validation() {
        assert!(is_valid_mac_address("AA:BB:CC:DD:EE:FF"));
        assert!(is_valid_mac_address("aa:bb:cc:dd:ee:ff"));
        assert!(is_valid_mac_address("AA-BB-CC-DD-EE-FF"));
        assert!(is_valid_mac_address("11:22:33:44:55:66"));
        
        assert!(!is_valid_mac_address("invalid"));
        assert!(!is_valid_mac_address("AA:BB:CC:DD:EE"));
        assert!(!is_valid_mac_address("AA:BB:CC:DD:EE:FF:GG"));
        assert!(!is_valid_mac_address(""));
    }

    #[test]
    fn test_skip_invalid_lines() {
        let input = r#"Invalid line
Device AA:BB:CC:DD:EE:FF Valid Device
Device invalid-address Invalid Device
Device"#;
        
        let devices = parse_bluetooth_devices(input).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name.as_ref(), "Valid Device");
    }

    #[test]
    fn test_complex_device_names() {
        let input = "Device AA:BB:CC:DD:EE:FF My Complex Device Name With Spaces";
        
        let devices = parse_bluetooth_devices(input).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name.as_ref(), "My Complex Device Name With Spaces");
    }
}