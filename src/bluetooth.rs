use crate::command::{read_output_lines, CommandRunner};
use crate::constants::{ICON_BLUETOOTH, ICON_CHECK};
use crate::format_entry;
use regex::Regex;
use std::error::Error;
use std::process::Output;

/// Represents actions that can be performed on Bluetooth devices.
#[derive(Debug)]
pub enum BluetoothAction {
    ToggleConnect(String),
}

/// Retrieves a list of paired Bluetooth devices and their connection status.
pub fn get_paired_bluetooth_devices(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let output = command_runner.run_command("bluetoothctl", &["devices"])?;
    let connected_devices = get_connected_devices(command_runner).unwrap_or_else(|_| vec![]);

    if output.status.success() {
        let devices =
            parse_bluetooth_devices(&output, &connected_devices).unwrap_or_else(|_| vec![]);
        Ok(devices)
    } else {
        // Instead of returning an error, return an empty list
        Ok(vec![])
    }
}

/// Parses the output of `bluetoothctl devices` command to retrieve a list of Bluetooth devices.
fn parse_bluetooth_devices(
    output: &Output,
    connected_devices: &[String],
) -> Result<Vec<BluetoothAction>, Box<dyn Error>> {
    let reader = read_output_lines(output)?;
    let devices = reader
        .into_iter()
        .filter_map(|line| parse_bluetooth_device(line, connected_devices))
        .collect();
    Ok(devices)
}

/// Parses a line of Bluetooth device information and returns a `BluetoothAction` if valid.
fn parse_bluetooth_device(line: String, connected_devices: &[String]) -> Option<BluetoothAction> {
    // Define a regex pattern for matching MAC addresses and device names
    // Check if the line matches the pattern and extract captures
    Regex::new(r"([0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5})\s+(.*)")
        .ok()?
        .captures(&line)
        .and_then(|caps| {
            // Extract the MAC address and device name from the captures
            let address = caps.get(1).map(|m| m.as_str().to_string());
            let name = caps.get(3).map(|m| m.as_str().to_string());

            // Check if we successfully extracted both the address and the name
            address.and_then(|addr| {
                name.map(|nm| {
                    // Check if the device is active
                    let is_active = connected_devices.contains(&addr);

                    // Return the appropriate BluetoothAction
                    BluetoothAction::ToggleConnect(format_entry(
                        "bluetooth",
                        if is_active {
                            ICON_CHECK
                        } else {
                            ICON_BLUETOOTH
                        },
                        &format!("{nm:<25} - {addr}"),
                    ))
                })
            })
        })
}

/// Handles a Bluetooth action, such as connecting or disconnecting a device.
pub fn handle_bluetooth_action(
    action: &BluetoothAction,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    match action {
        BluetoothAction::ToggleConnect(device) => {
            connect_to_bluetooth_device(device, connected_devices, command_runner)
        }
    }
}

/// Connects or disconnects a Bluetooth device based on its current status.
fn connect_to_bluetooth_device(
    device: &str,
    connected_devices: &[String],
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    if let Some(address) = extract_device_address(device) {
        let is_active = connected_devices.contains(&address);
        let action = if is_active { "disconnect" } else { "connect" };
        #[cfg(debug_assertions)]
        println!("Connect to Bluetooth device: {address}");
        let status = command_runner
            .run_command("bluetoothctl", &[action, &address])?
            .status;

        if status.success() {
            Ok(true)
        } else {
            #[cfg(debug_assertions)]
            eprintln!("Failed to connect to Bluetooth device: {address}");
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

/// Extracts the MAC address from the given device string.
fn extract_device_address(device: &str) -> Option<String> {
    Regex::new(r"([0-9A-Fa-f]{2}(:[0-9A-Fa-f]{2}){5})$")
        .ok()?
        .captures(device)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Retrieves a list of currently connected Bluetooth devices.
pub fn get_connected_devices(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<String>, Box<dyn Error>> {
    let output = command_runner.run_command("bluetoothctl", &["info"])?;
    let mac_addresses = read_output_lines(&output)?
        .into_iter()
        .filter(|line| line.starts_with("Device "))
        .filter_map(|line| line.split_whitespace().nth(1).map(|s| s.to_string()))
        .collect();
    Ok(mac_addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// Mock command runner for testing
    #[derive(Debug)]
    struct MockCommandRunner {
        expected_command: String,
        expected_args: Vec<String>,
        return_output: Output,
    }

    impl MockCommandRunner {
        fn new(command: &str, args: &[&str], output: Output) -> Self {
            Self {
                expected_command: command.to_string(),
                expected_args: args.iter().map(|s| s.to_string()).collect(),
                return_output: output,
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            assert_eq!(command, self.expected_command);
            assert_eq!(args, self.expected_args.as_slice());
            Ok(Output {
                status: self.return_output.status,
                stdout: self.return_output.stdout.clone(),
                stderr: self.return_output.stderr.clone(),
            })
        }
    }

    #[test]
    fn test_extract_device_address_valid() {
        let device = format!(
            "bluetooth - {} Test Device Name         - AA:BB:CC:DD:EE:FF",
            ICON_CHECK
        );
        let result = extract_device_address(&device);
        assert_eq!(result, Some("AA:BB:CC:DD:EE:FF".to_string()));
    }

    #[test]
    fn test_extract_device_address_invalid() {
        let device = "invalid device string";
        let result = extract_device_address(device);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_device_address_no_address() {
        let device = "bluetooth - Device Name";
        let result = extract_device_address(device);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_bluetooth_device_valid() {
        let line = "Device AA:BB:CC:DD:EE:FF Test Device".to_string();
        let connected_devices = vec![];
        let result = parse_bluetooth_device(line, &connected_devices);

        assert!(result.is_some());
        if let Some(BluetoothAction::ToggleConnect(device_str)) = result {
            assert!(device_str.contains("Test Device"));
            assert!(device_str.contains("AA:BB:CC:DD:EE:FF"));
            assert!(device_str.contains("bluetooth"));
        }
    }

    #[test]
    fn test_parse_bluetooth_device_connected() {
        let line = "Device AA:BB:CC:DD:EE:FF Test Device".to_string();
        let connected_devices = vec!["AA:BB:CC:DD:EE:FF".to_string()];
        let result = parse_bluetooth_device(line, &connected_devices);

        assert!(result.is_some());
        if let Some(BluetoothAction::ToggleConnect(device_str)) = result {
            assert!(device_str.contains(ICON_CHECK));
        }
    }

    #[test]
    fn test_parse_bluetooth_device_invalid_format() {
        let line = "Invalid line format".to_string();
        let connected_devices = vec![];
        let result = parse_bluetooth_device(line, &connected_devices);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_bluetooth_device_no_name() {
        let line = "Device AA:BB:CC:DD:EE:FF".to_string();
        let connected_devices = vec![];
        let result = parse_bluetooth_device(line, &connected_devices);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_bluetooth_devices_success() {
        let stdout =
            b"Device AA:BB:CC:DD:EE:FF Test Device 1\nDevice 11:22:33:44:55:66 Test Device 2\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };
        let connected_devices = vec!["AA:BB:CC:DD:EE:FF".to_string()];

        let result = parse_bluetooth_devices(&output, &connected_devices);
        assert!(result.is_ok());

        let devices = result.unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn test_parse_bluetooth_devices_empty() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };
        let connected_devices = vec![];

        let result = parse_bluetooth_devices(&output, &connected_devices);
        assert!(result.is_ok());

        let devices = result.unwrap();
        assert_eq!(devices.len(), 0);
    }

    #[test]
    fn test_get_connected_devices_success() {
        let stdout = b"Device AA:BB:CC:DD:EE:FF\nDevice 11:22:33:44:55:66\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("bluetoothctl", &["info"], output);
        let result = get_connected_devices(&mock_runner);

        assert!(result.is_ok());
        let devices = result.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0], "AA:BB:CC:DD:EE:FF");
        assert_eq!(devices[1], "11:22:33:44:55:66");
    }

    #[test]
    fn test_get_connected_devices_no_devices() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("bluetoothctl", &["info"], output);
        let result = get_connected_devices(&mock_runner);

        assert!(result.is_ok());
        let devices = result.unwrap();
        assert_eq!(devices.len(), 0);
    }

    #[test]
    fn test_get_connected_devices_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("bluetoothctl", &["info"], output);
        let result = get_connected_devices(&mock_runner);

        // Function returns Ok with empty vec on failure, not an error
        assert!(result.is_ok());
        let devices = result.unwrap();
        assert_eq!(devices.len(), 0);
    }

    #[test]
    fn test_parse_bluetooth_devices_function() {
        let stdout =
            b"Device AA:BB:CC:DD:EE:FF Test Device 1\nDevice 11:22:33:44:55:66 Test Device 2\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };
        let connected_devices = vec!["AA:BB:CC:DD:EE:FF".to_string()];

        let result = parse_bluetooth_devices(&output, &connected_devices);
        assert!(result.is_ok());

        let devices = result.unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn test_parse_bluetooth_devices_with_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };
        let connected_devices = vec![];

        let result = parse_bluetooth_devices(&output, &connected_devices);
        assert!(result.is_ok());
        let devices = result.unwrap();
        assert_eq!(devices.len(), 0);
    }

    #[test]
    fn test_connect_to_bluetooth_device_connect() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let device = "bluetooth - Test Device           - AA:BB:CC:DD:EE:FF";
        let connected_devices = vec![];
        let mock_runner =
            MockCommandRunner::new("bluetoothctl", &["connect", "AA:BB:CC:DD:EE:FF"], output);

        let result = connect_to_bluetooth_device(device, &connected_devices, &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_connect_to_bluetooth_device_disconnect() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let device = format!(
            "bluetooth - {} Test Device           - AA:BB:CC:DD:EE:FF",
            ICON_CHECK
        );
        let connected_devices = vec!["AA:BB:CC:DD:EE:FF".to_string()];
        let mock_runner =
            MockCommandRunner::new("bluetoothctl", &["disconnect", "AA:BB:CC:DD:EE:FF"], output);

        let result = connect_to_bluetooth_device(&device, &connected_devices, &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_connect_to_bluetooth_device_no_address() {
        let device = "invalid device string";
        let connected_devices = vec![];

        // Create a mock that should never be called
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };
        let mock_runner = MockCommandRunner::new("never_called", &[], output);

        let result = connect_to_bluetooth_device(device, &connected_devices, &mock_runner);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_connect_to_bluetooth_device_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let device = "bluetooth - Test Device           - AA:BB:CC:DD:EE:FF";
        let connected_devices = vec![];
        let mock_runner =
            MockCommandRunner::new("bluetoothctl", &["connect", "AA:BB:CC:DD:EE:FF"], output);

        let result = connect_to_bluetooth_device(device, &connected_devices, &mock_runner);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_handle_bluetooth_action_toggle_connect() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let device_str = "bluetooth - Test Device           - AA:BB:CC:DD:EE:FF";
        let action = BluetoothAction::ToggleConnect(device_str.to_string());
        let connected_devices = vec![];
        let mock_runner =
            MockCommandRunner::new("bluetoothctl", &["connect", "AA:BB:CC:DD:EE:FF"], output);

        let result = handle_bluetooth_action(&action, &connected_devices, &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
