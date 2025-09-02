use crate::command::CommandRunner;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Struct to hold Tailscale preferences from `tailscale debug prefs`
#[derive(Debug, Serialize, Deserialize, Default)]
#[allow(non_snake_case)]
pub struct TailscalePrefs {
    /// Want tailscale enabled or not
    pub WantRunning: bool,
    /// Whether shields are up (blocks incoming connections)
    pub ShieldsUp: bool,
    /// Whether to accept advertised routes
    pub RouteAll: bool,
    /// Whether to allow LAN access when using an exit node
    pub ExitNodeAllowLANAccess: bool,
    /// Current exit node IP address if set
    pub ExitNodeIP: Option<String>,
    /// Current exit node ID if set
    pub ExitNodeID: Option<String>,
    /// Whether to advertise routes
    pub AdvertiseRoutes: Option<Value>,
    /// Whether to advertise tags
    pub AdvertiseTags: Option<Vec<String>>,
    /// Whether DNS is managed by Tailscale
    pub CorpDNS: Option<bool>,
}

/// Parse the output of `tailscale debug prefs` to get current state
pub fn parse_tailscale_prefs(command_runner: &dyn CommandRunner) -> Option<TailscalePrefs> {
    let output = command_runner
        .run_command("tailscale", &["debug", "prefs"])
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON output
    serde_json::from_str(&output_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandRunner;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    /// Mock command runner for testing
    struct MockCommandRunner {
        expected_cmd: String,
        expected_args: Vec<String>,
        output: Output,
    }

    impl MockCommandRunner {
        fn new(cmd: &str, args: &[&str], output: Output) -> Self {
            Self {
                expected_cmd: cmd.to_string(),
                expected_args: args.iter().map(|a| a.to_string()).collect(),
                output,
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run_command(&self, cmd: &str, args: &[&str]) -> Result<Output, std::io::Error> {
            assert_eq!(cmd, self.expected_cmd);
            assert_eq!(
                args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                self.expected_args
            );
            Ok(Output {
                status: self.output.status,
                stdout: self.output.stdout.clone(),
                stderr: self.output.stderr.clone(),
            })
        }
    }

    #[test]
    fn test_parse_tailscale_prefs() {
        let json = r#"{
            "ShieldsUp": false,
            "RouteAll": true,
            "ExitNodeAllowLANAccess": true,
            "ExitNodeIP": "100.101.102.103",
            "AdvertiseRoutes": ["10.0.0.0/24"]
        }"#;

        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: json.as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["debug", "prefs"], output);

        let prefs = parse_tailscale_prefs(&mock_runner);
        assert!(prefs.is_some());

        let prefs = prefs.unwrap();
        assert_eq!(prefs.ShieldsUp, false);
        assert_eq!(prefs.RouteAll, true);
        assert_eq!(prefs.ExitNodeAllowLANAccess, true);
        assert_eq!(prefs.ExitNodeIP, Some("100.101.102.103".to_string()));
    }

    #[test]
    fn test_parse_tailscale_prefs_command_failure() {
        let output = Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["debug", "prefs"], output);

        let prefs = parse_tailscale_prefs(&mock_runner);
        assert!(prefs.is_none());
    }

    #[test]
    fn test_parse_tailscale_prefs_invalid_json() {
        let json = r#"{ invalid json }"#;

        let output = Output {
            status: ExitStatus::from_raw(0),
            stdout: json.as_bytes().to_vec(),
            stderr: vec![],
        };

        let mock_runner = MockCommandRunner::new("tailscale", &["debug", "prefs"], output);

        let prefs = parse_tailscale_prefs(&mock_runner);
        assert!(prefs.is_none());
    }
}
