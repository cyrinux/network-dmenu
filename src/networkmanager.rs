use crate::command::{read_output_lines, CommandRunner};
use crate::utils::{convert_network_strength, prompt_for_password};
use crate::{parse_vpn_action, parse_wifi_action, VpnAction, WifiAction};
use regex::Regex;
use std::error::Error;
use std::io::{BufRead, BufReader};

/// Retrieves available Wi-Fi networks using NetworkManager.
pub fn get_nm_wifi_networks(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<WifiAction>, Box<dyn Error>> {
    let mut actions = Vec::new();

    if let Some(lines) = fetch_wifi_lines(command_runner)? {
        let has_in_use = lines.iter().any(|line| line.starts_with('*'));

        if !has_in_use {
            let rescan_output = command_runner.run_command(
                "nmcli",
                &["--colors", "no", "dev", "wifi", "list", "--rescan", "auto"],
            )?;

            if rescan_output.status.success() {
                if let Some(rescan_lines) = fetch_wifi_lines(command_runner)? {
                    parse_wifi_lines(&mut actions, rescan_lines);
                }
            }
        } else {
            parse_wifi_lines(&mut actions, lines);
        }
    }

    Ok(actions)
}

/// Retrieves available VPN networks using NetworkManager.
pub fn get_nm_vpn_networks(
    command_runner: &dyn CommandRunner,
) -> Result<Vec<VpnAction>, Box<dyn Error>> {
    let mut actions = Vec::new();

    if let Some(lines) = fetch_vpn_lines(command_runner)? {
        parse_vpn_lines(&mut actions, lines);
    }

    Ok(actions)
}

/// Fetches raw VPN network data from NetworkManager.
fn fetch_vpn_lines(
    command_runner: &dyn CommandRunner,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "ACTIVE,TYPE,NAME",
            "connection",
            "show",
        ],
    )?;

    if output.status.success() {
        let reader = read_output_lines(&output)?;
        Ok(Some(reader))
    } else {
        Ok(None)
    }
}

/// Fetches raw Wi-Fi network data from NetworkManager.
fn fetch_wifi_lines(
    command_runner: &dyn CommandRunner,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "IN-USE,SSID,BARS,SECURITY",
            "device",
            "wifi",
        ],
    )?;

    if output.status.success() {
        let reader = read_output_lines(&output)?;
        Ok(Some(reader))
    } else {
        Ok(None)
    }
}

/// Parses the raw VPN network data into a structured format.
fn parse_vpn_lines(actions: &mut Vec<VpnAction>, vpn_lines: Vec<String>) {
    vpn_lines.into_iter().for_each(|line| {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 3 {
            let in_use = parts[0].trim();
            let typ = parts[1].trim();
            let name = parts[2].trim();
            if !name.is_empty() && (typ == "vpn" || typ == "wireguard") {
                let display = format!("{} {}", if in_use == "yes" { "✅" } else { "📶" }, name);
                if in_use == "yes" {
                    actions.push(VpnAction::Disconnect(display));
                } else {
                    actions.push(VpnAction::Connect(display));
                }
            }
        }
    });
}

/// Parses the raw Wi-Fi network data into a structured format.
fn parse_wifi_lines(actions: &mut Vec<WifiAction>, wifi_lines: Vec<String>) {
    wifi_lines.into_iter().for_each(|line| {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 4 {
            let in_use = parts[0].trim();
            let ssid = parts[1].trim();
            let signal = parts[2].trim();
            let security = parts[3].trim();
            if !ssid.is_empty() {
                let display = format!(
                    "{} {:<25}\t{:<11}\t{}",
                    if in_use == "*" { "✅" } else { "📶" },
                    ssid,
                    security.to_uppercase(),
                    convert_network_strength(signal),
                );
                actions.push(WifiAction::Network(display));
            }
        }
    });
}

/// Connects to a Wi-Fi network using NetworkManager.
pub fn connect_to_nm_vpn(
    action: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let name = parse_vpn_action(action)?;
    #[cfg(debug_assertions)]
    println!("Connecting to VPN network: {name}");

    attempt_vpn_connection(name, command_runner)
}
/// Connects to a Wi-Fi network using NetworkManager.
pub fn connect_to_nm_wifi(
    action: &str,
    hidden: bool,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let (ssid, security) = parse_wifi_action(action)?;
    #[cfg(debug_assertions)]
    println!("Connecting to Wi-Fi network: {ssid} with security {security}");

    // Helper function for attempting connection with or without a password
    let connect = |hidden: bool| -> Result<bool, Box<dyn Error>> {
        if is_known_network(ssid, command_runner)? || security.is_empty() {
            attempt_wifi_connection(ssid, hidden, None, command_runner)
        } else {
            let password = prompt_for_password(ssid)?;
            attempt_wifi_connection(ssid, hidden, Some(password), command_runner)
        }
    };

    // Main connection logic
    match connect(hidden) {
        Ok(true) => Ok(true),
        Err(_) if hidden => {
            // Retry with password if the first attempt failed and the network is hidden
            let password = prompt_for_password(ssid)?;
            attempt_wifi_connection(ssid, true, Some(password), command_runner)
        }
        result => result, // Return the original result for non-hidden networks
    }
}

/// Attempts to connect to a VPN network, optionally using a password.
fn attempt_vpn_connection(
    name: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    if name.is_empty() {
        #[cfg(debug_assertions)]
        eprintln!("Network name is empty");
        return Ok(false);
    }

    let status = command_runner
        .run_command("nmcli", &["connection", "up", name])?
        .status;

    if status.success() {
        // Connection successful
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to VPN network: {name}");
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// Mock command runner for testing
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
    fn test_get_nm_wifi_networks_success() {
        let stdout = "*:TestNetwork1:****:WPA2\n :TestNetwork2:***:WPA2\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new(
            "nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "-f",
                "IN-USE,SSID,BARS,SECURITY",
                "device",
                "wifi",
            ],
            output,
        );
        let result = get_nm_wifi_networks(&mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(!networks.is_empty());
    }

    #[test]
    fn test_get_nm_wifi_networks_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new(
            "nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "-f",
                "IN-USE,SSID,BARS,SECURITY",
                "device",
                "wifi",
            ],
            output,
        );
        let result = get_nm_wifi_networks(&mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(networks.is_empty());
    }

    #[test]
    fn test_get_nm_vpn_networks_success() {
        let stdout = "yes:vpn:TestVPN1\nno:vpn:TestVPN2\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new(
            "nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "-f",
                "ACTIVE,TYPE,NAME",
                "connection",
                "show",
            ],
            output,
        );
        let result = get_nm_vpn_networks(&mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(!networks.is_empty());
    }

    #[test]
    fn test_get_nm_vpn_networks_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new(
            "nmcli",
            &[
                "--colors",
                "no",
                "-t",
                "-f",
                "ACTIVE,TYPE,NAME",
                "connection",
                "show",
            ],
            output,
        );
        let result = get_nm_vpn_networks(&mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(networks.is_empty());
    }

    #[test]
    fn test_disconnect_nm_wifi_success() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("nmcli", &["device", "disconnect", "wlan0"], output);

        let result = disconnect_nm_wifi("wlan0", &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_connect_to_nm_vpn_success() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("nmcli", &["connection", "up", "TestVPN"], output);

        let result = connect_to_nm_vpn("vpn       - 📶 TestVPN", &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_disconnect_nm_vpn_success() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("nmcli", &["connection", "down", "TestVPN"], output);

        let result = disconnect_nm_vpn("TestVPN", &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}

/// Attempts to connect to a Wi-Fi network, optionally using a password.

fn attempt_wifi_connection(
    ssid: &str,
    hidden: bool,
    password: Option<String>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let mut command = match password {
        Some(ref pwd) => vec!["device", "wifi", "connect", ssid, "password", pwd],
        None => vec!["device", "wifi", "connect", ssid],
    };

    // Add hidden parameter only if needed
    if hidden {
        command.push("hidden");
        command.push("yes");
    }

    let status = command_runner.run_command("nmcli", &command)?.status;

    if status.success() {
        // Connection successful
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {ssid}");
        Ok(false)
    }
}

/// Disconnects from a VPN network.
pub fn disconnect_nm_vpn(
    name: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let status = command_runner
        .run_command("nmcli", &["connection", "down", name])?
        .status;
    Ok(status.success())
}
/// Disconnects from a Wi-Fi network.
pub fn disconnect_nm_wifi(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let status = command_runner
        .run_command("nmcli", &["device", "disconnect", interface])?
        .status;
    Ok(status.success())
}

/// Checks if NetworkManager is currently connected to a network.
pub fn is_nm_connected(
    command_runner: &dyn CommandRunner,
    interface: &str,
) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command(
        "nmcli",
        &[
            "--colors",
            "no",
            "-t",
            "-f",
            "DEVICE,STATE",
            "device",
            "status",
        ],
    )?;
    let reader = read_output_lines(&output)?;
    for line in reader {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 2 && parts[0].trim() == interface && parts[1].trim() == "connected" {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Checks if a Wi-Fi network is known (i.e., previously connected).
pub fn is_known_network(
    ssid: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    // Run the `nmcli connection show` command
    let output = command_runner.run_command("nmcli", &["--colors", "no", "connection", "show"])?;

    // Check if the command executed successfully
    if output.status.success() {
        // Create a buffered reader for the command output
        let reader = BufReader::new(output.stdout.as_slice());

        // Create a regex pattern to match the SSID exactly
        let ssid_pattern = format!(r"^\s*{}\s+", regex::escape(ssid));
        let re = Regex::new(&ssid_pattern)?;

        // Iterate over each line in the output
        for line in reader.lines() {
            let line = line?;

            // Check if the line matches the SSID pattern
            if re.is_match(&line) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}
