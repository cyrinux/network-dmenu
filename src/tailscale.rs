use crate::command::{execute_command, is_command_installed, read_output_lines, CommandRunner};
use crate::format_entry;
use notify_rust::Notification;
use regex::Regex;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::error::Error;

/// Enum representing various Tailscale actions.
#[derive(Debug)]
pub enum TailscaleAction {
    DisableExitNode,
    SetEnable(bool),
    SetExitNode(String),
    SetShields(bool),
}

/// Add a new parameter to pass the excluded exit nodes.
pub fn get_mullvad_actions(
    command_runner: &dyn CommandRunner,
    exclude_exit_nodes: &[String],
) -> Vec<String> {
    let output = command_runner
        .run_command("tailscale", &["exit-node", "list"])
        .expect("Failed to execute command");

    let active_exit_node = get_active_exit_node(command_runner);

    let exclude_set: HashSet<_> = exclude_exit_nodes.iter().collect();

    if output.status.success() {
        let reader = read_output_lines(&output).unwrap_or_default();
        let regex = Regex::new(r"\s{2,}").unwrap();

        let mut actions: Vec<String> = reader
            .into_iter()
            .filter(|line| line.contains("mullvad.ts.net"))
            .filter(|line| !exclude_set.contains(&extract_node_name(line)))
            .map(|line| parse_mullvad_line(&line, &regex, &active_exit_node))
            .collect();

        let reader = read_output_lines(&output).unwrap_or_default();
        actions.extend(
            reader
                .into_iter()
                .filter(|line| line.contains("ts.net") && !line.contains("mullvad.ts.net"))
                .filter(|line| !exclude_set.contains(&extract_node_name(line)))
                .map(|line| parse_exit_node_line(&line, &regex, &active_exit_node)),
        );

        actions.sort_by(|a, b| {
            a.split_whitespace()
                .next()
                .cmp(&b.split_whitespace().next())
        });
        actions
    } else {
        Vec::new()
    }
}

/// Helper function to extract node name from the action line.
fn extract_node_name(line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    parts.get(1).unwrap_or(&"").to_string()
}

/// Checks Mullvad connection status and sends a notification.
pub async fn check_mullvad() -> Result<(), Box<dyn Error>> {
    // Create a retry policy with exponential backoff
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    // Build a client with retry middleware
    let client: ClientWithMiddleware = ClientBuilder::new(Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    // Make a request and handle retries automatically
    let response = match client
        .get("https://am.i.mullvad.net/connected")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("Mullvad check request error: {}", e);
            return Ok(());
        }
    };

    let text = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("Mullvad check response error: {}", e);
            return Ok(());
        }
    };

    if let Err(e) = Notification::new()
        .summary("Connected Status")
        .body(text.trim())
        .show()
    {
        #[cfg(debug_assertions)]
        eprintln!("Mullvad notification error: {}", e);
    }

    Ok(())
}

/// Parses a Mullvad line from the Tailscale exit-node list output.
fn parse_mullvad_line(line: &str, regex: &Regex, active_exit_node: &str) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let node_ip = parts.first().unwrap_or(&"").trim();
    let node_name = parts.get(1).unwrap_or(&"").trim();
    let country = parts.get(2).unwrap_or(&"").trim();
    let is_active = active_exit_node == node_name;
    format_entry(
        "mullvad",
        if is_active { "✅" } else { get_flag(country) },
        &format!("{country:<15} - {node_ip:<16} {node_name}"),
    )
}

/// Extracts the short name from a node name.
fn extract_short_name(node_name: &str) -> &str {
    node_name.split('.').next().unwrap_or(node_name)
}

/// Parses an exit node line from the Tailscale exit-node list output.
fn parse_exit_node_line(line: &str, regex: &Regex, active_exit_node: &str) -> String {
    let parts: Vec<&str> = regex.split(line).collect();
    let node_ip = parts.first().unwrap_or(&"").trim();
    let node_name = parts.get(1).unwrap_or(&"").trim();
    let node_short_name = extract_short_name(node_name);
    let is_active = active_exit_node == node_name;
    format_entry(
        "exit-node",
        if is_active { "✅" } else { "🌿" },
        &format!("{node_short_name:<15} - {node_ip:<16} {node_name}"),
    )
}

/// Retrieves the currently active exit node for Tailscale.
fn get_active_exit_node(command_runner: &dyn CommandRunner) -> String {
    let output = command_runner
        .run_command("tailscale", &["status", "--json"])
        .expect("failed to execute process");

    let json: Value = serde_json::from_slice(&output.stdout).expect("failed to parse JSON");

    if let Some(peers) = json.get("Peer") {
        if let Some(peers_map) = peers.as_object() {
            for peer in peers_map.values() {
                if peer["Active"].as_bool() == Some(true)
                    && peer["ExitNode"].as_bool() == Some(true)
                {
                    if let Some(dns_name) = peer["DNSName"].as_str() {
                        return dns_name.trim_end_matches('.').to_string();
                    }
                }
            }
        }
    }

    String::new()
}

/// Sets the exit node for Tailscale.
fn set_exit_node(action: &str) -> bool {
    let Some(node_ip) = extract_node_ip(action) else {
        return false;
    };

    #[cfg(debug_assertions)]
    println!("Exit-node ip address: {node_ip}");

    if !execute_command("tailscale", &["up"]) {
        return false;
    }

    execute_command(
        "tailscale",
        &[
            "set",
            "--exit-node",
            node_ip,
            "--exit-node-allow-lan-access=true",
        ],
    )
}

/// Extracts the IP address from the action string.
fn extract_node_ip(action: &str) -> Option<&str> {
    Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
        .ok()?
        .captures(action)
        .and_then(|caps| caps.get(0))
        .map(|m| m.as_str())
}

/// Returns the flag emoji for a given country.
fn get_flag(country: &str) -> &'static str {
    let country_flags: HashMap<&str, &str> = [
        ("Albania", "🇦🇱"),
        ("Australia", "🇦🇺"),
        ("Austria", "🇦🇹"),
        ("Belgium", "🇧🇪"),
        ("Brazil", "🇧🇷"),
        ("Bulgaria", "🇧🇬"),
        ("Canada", "🇨🇦"),
        ("Chile", "🇨🇱"),
        ("Colombia", "🇨🇴"),
        ("Croatia", "🇭🇷"),
        ("Czech Republic", "🇨🇿"),
        ("Denmark", "🇩🇰"),
        ("Estonia", "🇪🇪"),
        ("Finland", "🇫🇮"),
        ("France", "🇫🇷"),
        ("Germany", "🇩🇪"),
        ("Greece", "🇬🇷"),
        ("Hong Kong", "🇭🇰"),
        ("Hungary", "🇭🇺"),
        ("Indonesia", "🇮🇩"),
        ("Ireland", "🇮🇪"),
        ("Israel", "🇮🇱"),
        ("Italy", "🇮🇹"),
        ("Japan", "🇯🇵"),
        ("Latvia", "🇱🇻"),
        ("Mexico", "🇲🇽"),
        ("Netherlands", "🇳🇱"),
        ("New Zealand", "🇳🇿"),
        ("Norway", "🇳🇴"),
        ("Poland", "🇵🇱"),
        ("Portugal", "🇵🇹"),
        ("Romania", "🇷🇴"),
        ("Serbia", "🇷🇸"),
        ("Singapore", "🇸🇬"),
        ("Slovakia", "🇸🇰"),
        ("Slovenia", "🇸🇮"),
        ("South Africa", "🇿🇦"),
        ("Spain", "🇪🇸"),
        ("Sweden", "🇸🇪"),
        ("Switzerland", "🇨🇭"),
        ("Thailand", "🇹🇭"),
        ("Turkey", "🇹🇷"),
        ("UK", "🇬🇧"),
        ("Ukraine", "🇺🇦"),
        ("USA", "🇺🇸"),
    ]
    .iter()
    .cloned()
    .collect();

    country_flags.get(country).unwrap_or(&"❓")
}

/// Checks if an exit node is currently active for Tailscale.
pub fn is_exit_node_active(command_runner: &dyn CommandRunner) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command("tailscale", &["status"])?;

    if output.status.success() {
        let reader = read_output_lines(&output)?;
        for line in reader {
            if line.contains("active; exit node;") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Handles a Tailscale action.
pub async fn handle_tailscale_action(
    action: &TailscaleAction,
    command_runner: &dyn CommandRunner,
) -> Result<bool, Box<dyn Error>> {
    if !is_command_installed("tailscale") {
        return Ok(false);
    }

    match action {
        TailscaleAction::DisableExitNode => {
            let status = command_runner
                .run_command("tailscale", &["set", "--exit-node="])?
                .status;
            // Ignore errors from mullvad check
            let _ = check_mullvad().await;
            Ok(status.success())
        }
        TailscaleAction::SetEnable(enable) => {
            let status = command_runner
                .run_command("tailscale", &[if *enable { "up" } else { "down" }])?
                .status;
            Ok(status.success())
        }
        TailscaleAction::SetExitNode(node) => {
            if set_exit_node(node) {
                // Ignore errors from mullvad check
                let _ = check_mullvad().await;
                Ok(true)
            } else {
                // Ignore errors from mullvad check
                let _ = check_mullvad().await;
                Ok(false)
            }
        }
        TailscaleAction::SetShields(enable) => {
            let status = command_runner
                .run_command(
                    "tailscale",
                    &[
                        "set",
                        "--shields-up",
                        if *enable { "true" } else { "false" },
                    ],
                )?
                .status;
            Ok(status.success())
        }
    }
}

/// Checks if Tailscale is currently enabled.
pub fn is_tailscale_enabled(command_runner: &dyn CommandRunner) -> Result<bool, Box<dyn Error>> {
    let output = command_runner.run_command("tailscale", &["status"])?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(!stdout.contains("Tailscale is stopped"));
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// Mock command runner for testing with multiple command support
    struct MockCommandRunner {
        responses: Vec<(String, Vec<String>, Output)>,
        call_count: std::cell::RefCell<usize>,
    }

    impl MockCommandRunner {
        fn new(command: &str, args: &[&str], output: Output) -> Self {
            Self {
                responses: vec![(
                    command.to_string(),
                    args.iter().map(|s| s.to_string()).collect(),
                    output,
                )],
                call_count: std::cell::RefCell::new(0),
            }
        }

        fn with_multiple_calls(responses: Vec<(&str, &[&str], Output)>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(cmd, args, output)| {
                        (
                            cmd.to_string(),
                            args.iter().map(|s| s.to_string()).collect(),
                            output,
                        )
                    })
                    .collect(),
                call_count: std::cell::RefCell::new(0),
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, command: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            let mut count = self.call_count.borrow_mut();
            if *count < self.responses.len() {
                let (expected_cmd, expected_args, output) = &self.responses[*count];
                assert_eq!(command, expected_cmd);
                assert_eq!(args, expected_args.as_slice());
                *count += 1;
                Ok(Output {
                    status: output.status,
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                })
            } else {
                panic!("Unexpected command call: {} {:?}", command, args);
            }
        }
    }

    #[test]
    fn test_extract_node_name() {
        let line = "100.100.100.100  node-name.ts.net  active; exit node;";
        let result = extract_node_name(line);
        assert_eq!(result, "node-name.ts.net");
    }

    #[test]
    fn test_extract_node_name_empty() {
        let line = "";
        let result = extract_node_name(line);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_node_name_single_word() {
        let line = "single";
        let result = extract_node_name(line);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_short_name() {
        let node_name = "test-node.mullvad.ts.net";
        let result = extract_short_name(node_name);
        assert_eq!(result, "test-node");
    }

    #[test]
    fn test_extract_short_name_no_dots() {
        let node_name = "simple-name";
        let result = extract_short_name(node_name);
        assert_eq!(result, "simple-name");
    }

    #[test]
    fn test_extract_node_ip_valid() {
        let action = "mullvad   - Germany        - 192.168.1.1    node.mullvad.ts.net";
        let result = extract_node_ip(action);
        assert_eq!(result, Some("192.168.1.1"));
    }

    #[test]
    fn test_extract_node_ip_invalid() {
        let action = "no ip address here";
        let result = extract_node_ip(action);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_node_ip_multiple_ips() {
        let action = "192.168.1.1 and 10.0.0.1";
        let result = extract_node_ip(action);
        assert_eq!(result, Some("192.168.1.1")); // Should return first match
    }

    #[test]
    fn test_get_flag_known_country() {
        assert_eq!(get_flag("Germany"), "🇩🇪");
        assert_eq!(get_flag("USA"), "🇺🇸");
        assert_eq!(get_flag("Japan"), "🇯🇵");
    }

    #[test]
    fn test_get_flag_unknown_country() {
        assert_eq!(get_flag("Unknown Country"), "❓");
        assert_eq!(get_flag(""), "❓");
    }

    #[test]
    fn test_parse_mullvad_line() {
        let regex = Regex::new(r"\s{2,}").unwrap();
        let line = "192.168.1.1  node.mullvad.ts.net  Germany  offline";
        let active_exit_node = "";

        let result = parse_mullvad_line(line, &regex, active_exit_node);
        assert!(result.contains("mullvad"));
        assert!(result.contains("Germany"));
        assert!(result.contains("192.168.1.1"));
        assert!(result.contains("node.mullvad.ts.net"));
    }

    #[test]
    fn test_parse_mullvad_line_active() {
        let regex = Regex::new(r"\s{2,}").unwrap();
        let line = "192.168.1.1  node.mullvad.ts.net  Germany  active";
        let active_exit_node = "node.mullvad.ts.net";

        let result = parse_mullvad_line(line, &regex, active_exit_node);
        assert!(result.contains("✅"));
    }

    #[test]
    fn test_parse_exit_node_line() {
        let regex = Regex::new(r"\s{2,}").unwrap();
        let line = "10.0.0.1  test-node.ts.net  offline";
        let active_exit_node = "";

        let result = parse_exit_node_line(line, &regex, active_exit_node);
        assert!(result.contains("exit-node"));
        assert!(result.contains("test-node"));
        assert!(result.contains("10.0.0.1"));
        assert!(result.contains("🌿"));
    }

    #[test]
    fn test_parse_exit_node_line_active() {
        let regex = Regex::new(r"\s{2,}").unwrap();
        let line = "10.0.0.1  test-node.ts.net  active";
        let active_exit_node = "test-node.ts.net";

        let result = parse_exit_node_line(line, &regex, active_exit_node);
        assert!(result.contains("✅"));
    }

    #[test]
    fn test_get_mullvad_actions_success() {
        let stdout = "100.65.216.68       au-adl-wg-301.mullvad.ts.net               Australia          Adelaide               -\n100.110.43.2        raspberrypi.allosaurus-godzilla.ts.net     -                  -                      -\n";
        let exit_nodes_output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
        };

        let status_output = Output {
            status: ExitStatus::from_raw(0),
            stdout: "{\"Peer\":{}}".as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::with_multiple_calls(vec![
            ("tailscale", &["exit-node", "list"], exit_nodes_output),
            ("tailscale", &["status", "--json"], status_output),
        ]);
        let exclude_nodes = vec![];

        let result = get_mullvad_actions(&mock_runner, &exclude_nodes);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_get_mullvad_actions_with_exclusions() {
        let stdout = "100.65.216.68       au-adl-wg-301.mullvad.ts.net               Australia          Adelaide               -\n100.110.43.2        excluded.ts.net                            -                  -                      -\n";
        let exit_nodes_output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
        };

        let status_output = Output {
            status: ExitStatus::from_raw(0),
            stdout: "{\"Peer\":{}}".as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::with_multiple_calls(vec![
            ("tailscale", &["exit-node", "list"], exit_nodes_output),
            ("tailscale", &["status", "--json"], status_output),
        ]);
        let exclude_nodes = vec!["excluded.ts.net".to_string()];

        let result = get_mullvad_actions(&mock_runner, &exclude_nodes);
        assert_eq!(result.len(), 1); // Only the non-excluded node should be present
        assert!(result[0].contains("au-adl-wg-301.mullvad.ts.net"));
    }

    #[test]
    fn test_get_mullvad_actions_command_failure() {
        let exit_nodes_output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let status_output = Output {
            status: ExitStatus::from_raw(0),
            stdout: "{\"Peer\":{}}".as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::with_multiple_calls(vec![
            ("tailscale", &["exit-node", "list"], exit_nodes_output),
            ("tailscale", &["status", "--json"], status_output),
        ]);
        let exclude_nodes = vec![];

        let result = get_mullvad_actions(&mock_runner, &exclude_nodes);
        assert!(result.is_empty());
    }

    #[test]
    fn test_is_exit_node_active_true() {
        let stdout = b"100.100.100.100  active; exit node;";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_exit_node_active(&mock_runner);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_is_exit_node_active_false() {
        let stdout = b"100.100.100.100  active;";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_exit_node_active(&mock_runner);

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_is_exit_node_active_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_exit_node_active(&mock_runner);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false on command failure
    }

    #[test]
    fn test_is_tailscale_enabled_true() {
        let stdout = b"Tailscale is running normally";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_tailscale_enabled(&mock_runner);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_is_tailscale_enabled_false() {
        let stdout = b"Tailscale is stopped";
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_tailscale_enabled(&mock_runner);

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_is_tailscale_enabled_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["status"], output);
        let result = is_tailscale_enabled(&mock_runner);

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false on command failure
    }

    #[tokio::test]
    async fn test_check_mullvad_success() {
        // This test verifies the function doesn't panic
        // In a real test environment, we'd mock the HTTP client
        let result = check_mullvad().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_tailscale_action_disable_exit_node() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["set", "--exit-node="], output);
        let action = TailscaleAction::DisableExitNode;

        let result = handle_tailscale_action(&action, &mock_runner).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_handle_tailscale_action_set_enable_true() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["up"], output);
        let action = TailscaleAction::SetEnable(true);

        let result = handle_tailscale_action(&action, &mock_runner).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_handle_tailscale_action_set_enable_false() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["down"], output);
        let action = TailscaleAction::SetEnable(false);

        let result = handle_tailscale_action(&action, &mock_runner).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_handle_tailscale_action_set_shields_true() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("tailscale", &["set", "--shields-up", "true"], output);
        let action = TailscaleAction::SetShields(true);

        let result = handle_tailscale_action(&action, &mock_runner).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_handle_tailscale_action_set_shields_false() {
        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner =
            MockCommandRunner::new("tailscale", &["set", "--shields-up", "false"], output);
        let action = TailscaleAction::SetShields(false);

        let result = handle_tailscale_action(&action, &mock_runner).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
