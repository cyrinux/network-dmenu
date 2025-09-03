use crate::command::CommandRunner;
use crate::constants::ICON_CHECK;
use crate::format_entry;
use log::{debug, error};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// SSH SOCKS proxy action types
#[derive(Debug, Clone)]
pub enum SshAction {
    StartProxy(SshProxyConfig),
    StopProxy(SshProxyConfig),
}

/// SSH SOCKS proxy configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SshProxyConfig {
    pub name: String,
    pub server: String,
    pub port: u16,
    pub socket_path: String,
    pub ssh_options: Vec<String>,
}

impl SshProxyConfig {
    pub fn new(name: String, server: String, port: u16) -> Self {
        Self {
            socket_path: format!("/tmp/{}.sock", name),
            name,
            server,
            port,
            ssh_options: vec!["-f".to_string(), "-q".to_string(), "-N".to_string()],
        }
    }

    /// Check if the SSH SOCKS proxy is currently active
    pub fn is_active(&self) -> bool {
        // Check if socket file exists and is active
        if Path::new(&self.socket_path).exists() {
            // Try to check if the socket is actually being used
            // We can check if there's a process listening on the SOCKS port
            self.is_port_listening()
        } else {
            false
        }
    }

    fn is_port_listening(&self) -> bool {
        // Check if a process is listening on the SOCKS port
        match std::process::Command::new("lsof")
            .args(["-i", &format!("tcp:{}", self.port)])
            .output()
        {
            Ok(output) => !output.stdout.is_empty(),
            Err(_) => {
                // Fallback: check with netstat if lsof is not available
                match std::process::Command::new("netstat")
                    .args(["-tln"])
                    .output()
                {
                    Ok(output) => {
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        output_str.lines().any(|line| {
                            line.contains(&format!(":{}", self.port)) && line.contains("LISTEN")
                        })
                    }
                    Err(_) => false,
                }
            }
        }
    }

    /// Start the SSH SOCKS proxy
    pub fn start(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if self.is_active() {
            return Ok(()); // Already active
        }

        let mut cmd_args = vec![
            "-S".to_string(),
            self.socket_path.clone(),
            "-D".to_string(),
            self.port.to_string(),
        ];
        cmd_args.extend(self.ssh_options.iter().cloned());
        cmd_args.push(self.server.clone());

        debug!("Starting SSH SOCKS proxy: ssh {}", cmd_args.join(" "));

        let cmd_args_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
        match command_runner.run_command("ssh", &cmd_args_refs) {
            Ok(output) => {
                if output.status.success() {
                    debug!("SSH SOCKS proxy started successfully for {}", self.name);
                    Ok(())
                } else {
                    let error_msg = format!(
                        "Failed to start SSH SOCKS proxy: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    error!("{}", error_msg);
                    Err(error_msg)
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to execute ssh command: {}", e);
                error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    /// Stop the SSH SOCKS proxy
    pub fn stop(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if !self.is_active() {
            return Ok(()); // Already stopped
        }

        // Kill the SSH process using the control socket
        let kill_args = vec![
            "-S".to_string(),
            self.socket_path.clone(),
            "-O".to_string(),
            "exit".to_string(),
            self.server.clone(),
        ];

        debug!("Stopping SSH SOCKS proxy: ssh {}", kill_args.join(" "));

        let kill_args_refs: Vec<&str> = kill_args.iter().map(|s| s.as_str()).collect();
        match command_runner.run_command("ssh", &kill_args_refs) {
            Ok(output) => {
                if output.status.success() || output.status.code() == Some(255) {
                    // SSH returns 255 when connection is terminated, which is expected
                    debug!("SSH SOCKS proxy stopped successfully for {}", self.name);
                    
                    // Clean up socket file if it still exists
                    if Path::new(&self.socket_path).exists() {
                        if let Err(e) = fs::remove_file(&self.socket_path) {
                            debug!("Failed to remove socket file: {}", e);
                        }
                    }
                    
                    Ok(())
                } else {
                    let error_msg = format!(
                        "Failed to stop SSH SOCKS proxy: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    error!("{}", error_msg);
                    Err(error_msg)
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to execute ssh command: {}", e);
                error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }
}

/// Get SSH SOCKS proxy actions from configuration
pub fn get_ssh_proxy_actions(ssh_configs: &HashMap<String, SshProxyConfig>) -> Vec<SshAction> {
    let mut actions = Vec::new();

    for (_, config) in ssh_configs {
        if config.is_active() {
            actions.push(SshAction::StopProxy(config.clone()));
        } else {
            actions.push(SshAction::StartProxy(config.clone()));
        }
    }

    actions
}

/// Convert SSH action to display string
pub fn ssh_action_to_string(action: &SshAction) -> String {
    match action {
        SshAction::StartProxy(config) => {
            format_entry(
                "ssh-proxy",
                "🔌",
                &format!("Start SOCKS proxy {} ({}:{})", config.name, config.server, config.port),
            )
        }
        SshAction::StopProxy(config) => {
            format_entry(
                "ssh-proxy",
                ICON_CHECK,
                &format!("Stop SOCKS proxy {} ({}:{})", config.name, config.server, config.port),
            )
        }
    }
}

/// Handle SSH SOCKS proxy action
pub fn handle_ssh_action(action: &SshAction, command_runner: &dyn CommandRunner) -> Result<(), String> {
    match action {
        SshAction::StartProxy(config) => {
            debug!("Starting SSH SOCKS proxy: {}", config.name);
            config.start(command_runner)
        }
        SshAction::StopProxy(config) => {
            debug!("Stopping SSH SOCKS proxy: {}", config.name);
            config.stop(command_runner)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::MockCommandRunner;
    use std::process::{ExitStatus, Output};

    #[test]
    fn test_ssh_proxy_config_creation() {
        let config = SshProxyConfig::new("server1".to_string(), "example.com".to_string(), 1081);
        
        assert_eq!(config.name, "server1");
        assert_eq!(config.server, "example.com");
        assert_eq!(config.port, 1081);
        assert_eq!(config.socket_path, "/tmp/server1.sock");
        assert_eq!(config.ssh_options, vec!["-f", "-q", "-N"]);
    }

    #[test]
    fn test_ssh_action_to_string() {
        let config = SshProxyConfig::new("server1".to_string(), "example.com".to_string(), 1081);
        
        let start_action = SshAction::StartProxy(config.clone());
        let stop_action = SshAction::StopProxy(config);
        
        let start_str = ssh_action_to_string(&start_action);
        let stop_str = ssh_action_to_string(&stop_action);
        
        assert!(start_str.contains("Start SOCKS proxy server1"));
        assert!(stop_str.contains("Stop SOCKS proxy server1"));
    }
}