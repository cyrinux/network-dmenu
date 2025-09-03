use crate::command::{CommandRunner, is_command_installed};
use crate::constants::{ICON_CHECK, ICON_CROSS};
use crate::format_entry;
use log::{debug, error, warn};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Tor proxy action types
#[derive(Debug, Clone)]
pub enum TorAction {
    StartTor,
    StopTor,
    RestartTor,
    StartTorsocks(TorsocksConfig),
    StopTorsocks(TorsocksConfig),
}

/// Torsocks configuration for specific applications
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TorsocksConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub description: String,
}

/// Tor service manager
pub struct TorManager {
    tor_data_dir: String,
    control_port: u16,
    socks_port: u16,
}

impl Default for TorManager {
    fn default() -> Self {
        Self {
            tor_data_dir: "/tmp/network-dmenu-tor".to_string(),
            control_port: 9051,
            socks_port: 9050,
        }
    }
}

impl TorManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if Tor daemon is running
    pub fn is_tor_running(&self) -> bool {
        // Check if Tor is listening on the control port
        self.is_port_listening(self.control_port) || 
        // Also check SOCKS port as fallback
        self.is_port_listening(self.socks_port)
    }

    fn is_port_listening(&self, port: u16) -> bool {
        // Check if a process is listening on the specified port
        match std::process::Command::new("lsof")
            .args(["-i", &format!("tcp:{}", port)])
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
                            line.contains(&format!(":{}", port)) && line.contains("LISTEN")
                        })
                    }
                    Err(_) => false,
                }
            }
        }
    }

    /// Start Tor daemon
    pub fn start_tor(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if self.is_tor_running() {
            debug!("Tor is already running");
            return Ok(());
        }

        // Create data directory
        if let Err(e) = fs::create_dir_all(&self.tor_data_dir) {
            return Err(format!("Failed to create Tor data directory: {}", e));
        }

        // Start Tor with custom configuration
        let tor_args = [
            "--DataDirectory", &self.tor_data_dir,
            "--ControlPort", &self.control_port.to_string(),
            "--SocksPort", &self.socks_port.to_string(),
            "--RunAsDaemon", "1",
            "--Log", "notice file /tmp/network-dmenu-tor.log",
        ];

        debug!("Starting Tor: tor {}", tor_args.join(" "));

        match command_runner.run_command("tor", &tor_args) {
            Ok(output) => {
                if output.status.success() {
                    debug!("Tor started successfully");
                    // Give Tor a moment to establish connections
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    Ok(())
                } else {
                    let error_msg = format!(
                        "Failed to start Tor: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    error!("{}", error_msg);
                    Err(error_msg)
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to execute tor command: {}", e);
                error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    /// Stop Tor daemon
    pub fn stop_tor(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if !self.is_tor_running() {
            debug!("Tor is not running");
            return Ok(());
        }

        // Try graceful shutdown via control port first
        if self.control_shutdown().is_err() {
            warn!("Graceful shutdown failed, attempting force kill");
            
            // Force kill Tor processes
            let killall_args = ["tor"];
            match command_runner.run_command("killall", &killall_args) {
                Ok(_) => debug!("Tor processes killed"),
                Err(e) => warn!("Failed to kill Tor processes: {}", e),
            }
        }

        // Clean up data directory
        if Path::new(&self.tor_data_dir).exists() {
            if let Err(e) = fs::remove_dir_all(&self.tor_data_dir) {
                warn!("Failed to clean up Tor data directory: {}", e);
            }
        }

        Ok(())
    }

    fn control_shutdown(&self) -> Result<(), String> {
        // Send SHUTDOWN signal via control port using telnet/nc
        let shutdown_cmd = format!("echo 'SIGNAL SHUTDOWN' | nc localhost {}", self.control_port);
        match std::process::Command::new("sh")
            .args(["-c", &shutdown_cmd])
            .output()
        {
            Ok(_) => {
                debug!("Sent shutdown signal to Tor");
                Ok(())
            }
            Err(e) => Err(format!("Failed to send shutdown signal: {}", e)),
        }
    }

    /// Restart Tor daemon
    pub fn restart_tor(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        debug!("Restarting Tor");
        self.stop_tor(command_runner)?;
        std::thread::sleep(std::time::Duration::from_secs(2));
        self.start_tor(command_runner)
    }
}

impl TorsocksConfig {
    /// Create a new torsocks configuration
    pub fn new(name: String, command: String, args: Vec<String>, description: String) -> Self {
        Self {
            name,
            command,
            args,
            description,
        }
    }

    /// Check if the application is running with torsocks
    pub fn is_running(&self) -> bool {
        // Check if there are processes matching our command
        match std::process::Command::new("pgrep")
            .args(["-f", &format!("torsocks {}", self.command)])
            .output()
        {
            Ok(output) => !output.stdout.is_empty(),
            Err(_) => false,
        }
    }

    /// Start application with torsocks
    pub fn start(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if self.is_running() {
            return Ok(());
        }

        if !is_command_installed("torsocks") {
            return Err("torsocks command not found. Please install torsocks package".to_string());
        }

        let mut torsocks_args = vec!["torsocks".to_string(), self.command.clone()];
        torsocks_args.extend(self.args.iter().cloned());

        debug!("Starting torsocks: {}", torsocks_args.join(" "));

        match command_runner.run_command("sh", &["-c", &format!("{} &", torsocks_args.join(" "))]) {
            Ok(_) => {
                debug!("Started {} with torsocks", self.name);
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to start {} with torsocks: {}", self.name, e);
                error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    /// Stop application running with torsocks
    pub fn stop(&self, command_runner: &dyn CommandRunner) -> Result<(), String> {
        if !self.is_running() {
            return Ok(());
        }

        // Kill processes matching our torsocks command
        let pkill_args = ["-f", &format!("torsocks {}", self.command)];
        
        match command_runner.run_command("pkill", &pkill_args) {
            Ok(_) => {
                debug!("Stopped {} with torsocks", self.name);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to stop {} with torsocks: {}", self.name, e);
                Ok(()) // Don't fail if process is already gone
            }
        }
    }
}

/// Get Tor proxy actions based on current state
pub fn get_tor_actions(torsocks_configs: &HashMap<String, TorsocksConfig>) -> Vec<TorAction> {
    let mut actions = Vec::new();
    let tor_manager = TorManager::new();

    // Tor daemon management
    if tor_manager.is_tor_running() {
        actions.push(TorAction::StopTor);
        actions.push(TorAction::RestartTor);
    } else {
        actions.push(TorAction::StartTor);
    }

    // Torsocks application management (only if Tor is running and torsocks is available)
    if tor_manager.is_tor_running() && is_command_installed("torsocks") {
        for config in torsocks_configs.values() {
            if config.is_running() {
                actions.push(TorAction::StopTorsocks(config.clone()));
            } else {
                actions.push(TorAction::StartTorsocks(config.clone()));
            }
        }
    }

    actions
}

/// Convert Tor action to display string
pub fn tor_action_to_string(action: &TorAction) -> String {
    match action {
        TorAction::StartTor => {
            format_entry("tor", "🧅", "Start Tor daemon")
        }
        TorAction::StopTor => {
            format_entry("tor", ICON_CROSS, "Stop Tor daemon")
        }
        TorAction::RestartTor => {
            format_entry("tor", "🔄", "Restart Tor daemon")
        }
        TorAction::StartTorsocks(config) => {
            format_entry(
                "torsocks",
                "🧅",
                &format!("Start {} via Tor", config.description),
            )
        }
        TorAction::StopTorsocks(config) => {
            format_entry(
                "torsocks",
                ICON_CHECK,
                &format!("Stop {} via Tor", config.description),
            )
        }
    }
}

/// Handle Tor action
pub fn handle_tor_action(action: &TorAction, command_runner: &dyn CommandRunner) -> Result<(), String> {
    let tor_manager = TorManager::new();

    match action {
        TorAction::StartTor => {
            debug!("Starting Tor daemon");
            tor_manager.start_tor(command_runner)
        }
        TorAction::StopTor => {
            debug!("Stopping Tor daemon");
            tor_manager.stop_tor(command_runner)
        }
        TorAction::RestartTor => {
            debug!("Restarting Tor daemon");
            tor_manager.restart_tor(command_runner)
        }
        TorAction::StartTorsocks(config) => {
            debug!("Starting {} with torsocks", config.name);
            if !tor_manager.is_tor_running() {
                return Err("Tor daemon must be running to use torsocks".to_string());
            }
            if !is_command_installed("torsocks") {
                return Err("torsocks command not found. Please install torsocks package".to_string());
            }
            config.start(command_runner)
        }
        TorAction::StopTorsocks(config) => {
            debug!("Stopping {} with torsocks", config.name);
            config.stop(command_runner)
        }
    }
}

/// Get default torsocks configurations
pub fn get_default_torsocks_configs() -> HashMap<String, TorsocksConfig> {
    let mut configs = HashMap::new();

    configs.insert(
        "firefox".to_string(),
        TorsocksConfig::new(
            "firefox".to_string(),
            "firefox".to_string(),
            vec!["--private-window".to_string()],
            "Firefox Private Browsing".to_string(),
        ),
    );

    configs.insert(
        "curl".to_string(),
        TorsocksConfig::new(
            "curl".to_string(),
            "curl".to_string(),
            vec!["httpbin.org/ip".to_string()],
            "Test Tor Connection".to_string(),
        ),
    );

    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tor_manager_creation() {
        let manager = TorManager::new();
        assert_eq!(manager.control_port, 9051);
        assert_eq!(manager.socks_port, 9050);
        assert!(manager.tor_data_dir.contains("network-dmenu-tor"));
    }

    #[test]
    fn test_torsocks_config_creation() {
        let config = TorsocksConfig::new(
            "firefox".to_string(),
            "firefox".to_string(),
            vec!["--private".to_string()],
            "Firefox Private".to_string(),
        );
        
        assert_eq!(config.name, "firefox");
        assert_eq!(config.command, "firefox");
        assert_eq!(config.args, vec!["--private"]);
        assert_eq!(config.description, "Firefox Private");
    }

    #[test]
    fn test_tor_action_to_string() {
        let start_action = TorAction::StartTor;
        let stop_action = TorAction::StopTor;
        
        let start_str = tor_action_to_string(&start_action);
        let stop_str = tor_action_to_string(&stop_action);
        
        assert!(start_str.contains("Start Tor daemon"));
        assert!(stop_str.contains("Stop Tor daemon"));
    }

    #[test]
    fn test_default_torsocks_configs() {
        let configs = get_default_torsocks_configs();
        assert!(configs.contains_key("firefox"));
        assert!(configs.contains_key("curl"));
        
        let firefox_config = configs.get("firefox").unwrap();
        assert_eq!(firefox_config.command, "firefox");
    }
}