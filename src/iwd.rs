use crate::command::{read_output_lines, CommandRunner};
use crate::utils::{convert_network_strength, prompt_for_password};
use crate::{notify_connection, parse_wifi_action, WifiAction};
use regex::Regex;
use std::error::Error;
use std::io::{BufRead, BufReader};

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

fn parse_iwd_networks(
    actions: &mut Vec<WifiAction>,
    networks: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let ansi_escape = Regex::new(r"\x1B\[[0-9;]*m.*?\x1B\[0m")?;

    networks.into_iter().for_each(|network| {
        let line = ansi_escape.replace_all(&network, "").to_string();
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let connected = network.starts_with("\u{1b}[0m");
            let signal = parts.pop().unwrap().trim();
            let security = parts.pop().unwrap().trim();
            let ssid = line[..line.find(security).unwrap()].trim();
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

pub fn connect_to_iwd_wifi(
    interface: &str,
    action: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let (ssid, security) = parse_wifi_action(action)?;
    #[cfg(debug_assertions)]
    println!("Connecting to Wi-Fi network: {ssid} with security {security}");

    if is_known_network(ssid, command_runner)? || security.is_empty() {
        attempt_connection(interface, ssid, None, command_runner)
    } else {
        let password = prompt_for_password(ssid)?;
        attempt_connection(interface, ssid, Some(&password), command_runner)
    }
}

fn attempt_connection(
    interface: &str,
    ssid: &str,
    passphrase: Option<&str>,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let mut command_args: Vec<&str> = vec!["station", interface, "connect", ssid];

    if let Some(pwd) = passphrase {
        command_args.push("--passphrase");
        command_args.push(pwd);
    }

    let status = command_runner.run_command("iwctl", &command_args)?.status;

    if status.success() {
        notify_connection(ssid)?;
        Ok(true)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("Failed to connect to Wi-Fi network: {ssid}");
        Ok(false)
    }
}

pub fn disconnect_iwd_wifi(
    interface: &str,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    let status = command_runner
        .run_command("iwctl", &["station", interface, "disconnect"])?
        .status;
    Ok(status.success())
}

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
