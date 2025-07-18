use crate::command::{read_output_lines, CommandRunner};
use crate::utils::{convert_network_strength, prompt_for_password};
use crate::{parse_wifi_action, WifiAction};
use regex::Regex;
use std::error::Error;
use std::io::{BufRead, BufReader};

/// Retrieves available Wi-Fi networks using IWD.
pub fn get_iwd_networks(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<Vec<WifiAction>, Box<dyn Error>> {
    let mut actions = Vec::new();

    if let Some(networks) = fetch_iwd_networks(interface, command_runner)? {
        let has_connected = networks.iter().any(|network| network.starts_with('>'));

        if !has_connected {
            let rescan_output =
                command_runner.run_command("iwctl", &["station", interface, "scan"])?;

            if rescan_output.status.success() {
                if let Some(rescan_networks) = fetch_iwd_networks(interface, command_runner)? {
                    parse_iwd_networks(&mut actions, rescan_networks)?;
                }
            }
        } else {
            parse_iwd_networks(&mut actions, networks)?;
        }
    }

    Ok(actions)
}

/// Fetches raw Wi-Fi network data from IWD.
fn fetch_iwd_networks(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let output = command_runner.run_command("iwctl", &["station", interface, "get-networks"])?;

    if output.status.success() {
        let reader = read_output_lines(&output)?;
        let networks = reader
            .into_iter()
            .skip_while(|network| !network.contains("Available networks"))
            .skip(3)
            .collect();
        Ok(Some(networks))
    } else {
        Ok(None)
    }
}

/// Parses the raw Wi-Fi network data into a structured format.
fn parse_iwd_networks(
    actions: &mut Vec<WifiAction>,
    networks: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let ansi_escape = Regex::new(r"\x1B\[[0-9;]*m.*?\x1B\[0m")?;
    let full_ansi_escape = Regex::new(r"\x1B\[[0-?]*[ -/]*[@-~]")?;

    networks.into_iter().for_each(|network| {
        let line = ansi_escape.replace_all(&network, "").to_string();
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let connected = network.starts_with("\u{1b}[0m");
            let signal = parts.pop().unwrap().trim();
            let security = parts.pop().unwrap().trim();
            let ssid = line[..line.find(security).unwrap()].trim();
            let ssid = full_ansi_escape.replace_all(ssid, "").to_string();
            let display = format!(
                "{} {:<25}\t{:<11}\t{}",
                if connected { "✅" } else { "📶" },
                ssid,
                security.to_uppercase(),
                convert_network_strength(signal)
            );
            actions.push(WifiAction::Network(display));
        }
    });

    Ok(())
}

/// Connects to a Wi-Fi network using IWD.
pub fn connect_to_iwd_wifi(
    interface: &str,
    action: &str,
    hidden: bool,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let (ssid, security) = parse_wifi_action(action)?;

    #[cfg(debug_assertions)]
    println!("Connecting to Wi-Fi network: {ssid} with security {security}");

    if is_known_network(ssid, command_runner)? || security == "OPEN" || security == "UNKNOWN" {
        attempt_connection(interface, ssid, hidden, None, command_runner)
    } else {
        let password = prompt_for_password(ssid)?;
        attempt_connection(interface, ssid, hidden, Some(&password), command_runner)
    }
}

/// Attempts to connect to a Wi-Fi network, optionally using a password.
fn attempt_connection(
    interface: &str,
    ssid: &str,
    hidden: bool,
    passphrase: Option<&str>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let mut command_args: Vec<&str> = vec![
        "station",
        interface,
        if hidden { "connect-hidden" } else { "connect" },
        ssid,
    ];

    if let Some(pass) = passphrase {
        command_args.push(pass);
    }

    let status = command_runner.run_command("iwctl", &command_args)?.status;

    if status.success() {
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("NOOOOO Failed to connect to Wi-Fi network: {ssid}");
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
    fn test_get_iwd_networks_success() {
        let stdout = b"Available networks\n------\n\n> TestNetwork1       psk\n  TestNetwork2       psk    ****\n";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("iwctl", &["station", "wlan0", "get-networks"], output);
        let result = get_iwd_networks("wlan0", &mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(!networks.is_empty());
    }

    #[test]
    fn test_get_iwd_networks_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("iwctl", &["station", "wlan0", "get-networks"], output);
        let result = get_iwd_networks("wlan0", &mock_runner);

        assert!(result.is_ok());
        let networks = result.unwrap();
        assert!(networks.is_empty());
    }

    #[test]
    fn test_disconnect_iwd_wifi_success() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("iwctl", &["station", "wlan0", "disconnect"], output);

        let result = disconnect_iwd_wifi("wlan0", &mock_runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}

/// Disconnects from a Wi-Fi network.
pub fn disconnect_iwd_wifi(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let status = command_runner
        .run_command("iwctl", &["station", interface, "disconnect"])?
        .status;
    Ok(status.success())
}

/// Checks if IWD is currently connected to a network.
pub fn is_iwd_connected(
    command_runner: &dyn CommandRunner,
    interface: &str,
) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command("iwctl", &["station", interface, "show"])?;
    if output.status.success() {
        for line in read_output_lines(&output)? {
            if line.contains("Connected") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Checks if a Wi-Fi network is known (i.e., previously connected).
pub fn is_known_network(
    ssid: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command("iwctl", &["known-networks", "list"])?;
    if output.status.success() {
        let reader = BufReader::new(output.stdout.as_slice());
        let ssid_pattern = format!(r"\b{}\b", regex::escape(ssid));
        let re = Regex::new(&ssid_pattern)?;
        for line in reader.lines() {
            let line = line?;
            if re.is_match(&line) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
