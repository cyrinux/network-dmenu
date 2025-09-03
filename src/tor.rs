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
    RefreshCircuit,
    TestConnection,
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

    /// Check if Tor daemon is running (async version)
    pub async fn is_tor_running_async(&self) -> bool {
        // Check if Tor is listening on the control port
        self.is_port_listening_async(self.control_port).await || 
        // Also check SOCKS port as fallback
        self.is_port_listening_async(self.socks_port).await
    }

    /// Check if Tor daemon is running (sync version for backward compatibility)
    pub fn is_tor_running(&self) -> bool {
        // Use a simple heuristic check - look for tor processes
        // This is much faster than port checking
        match std::process::Command::new("pgrep")
            .args(["-x", "tor"])
            .output()
        {
            Ok(output) => !output.stdout.is_empty(),
            Err(_) => false,
        }
    }

    /// Check if Tor daemon is running (async version for streaming)
    pub async fn is_tor_running_async_fast(&self) -> bool {
        debug!("TOR_DEBUG: Starting is_tor_running_async_fast()");
        let start_time = std::time::Instant::now();
        
        // Use a simple heuristic check - look for tor processes (async version)
        let result = match tokio::process::Command::new("pgrep")
            .args(["-x", "tor"])
            .output()
            .await
        {
            Ok(output) => {
                let is_running = !output.stdout.is_empty();
                debug!("TOR_DEBUG: pgrep result: {} bytes stdout, running={}", output.stdout.len(), is_running);
                is_running
            },
            Err(e) => {
                debug!("TOR_DEBUG: pgrep error: {}", e);
                false
            }
        };
        
        let elapsed = start_time.elapsed();
        debug!("TOR_DEBUG: is_tor_running_async_fast() took {:?}, result={}", elapsed, result);
        result
    }

    async fn is_port_listening_async(&self, port: u16) -> bool {
        // Check if a process is listening on the specified port
        // Try ss first (modern and widely available)
        match tokio::process::Command::new("ss")
            .args(["-tln"])
            .output()
            .await
        {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.lines().any(|line| {
                    line.contains(&format!(":{}", port)) && line.contains("LISTEN")
                })
            }
            Err(_) => {
                // Fallback: try lsof
                match tokio::process::Command::new("lsof")
                    .args(["-i", &format!("tcp:{}", port)])
                    .output()
                    .await
                {
                    Ok(output) => !output.stdout.is_empty(),
                    Err(_) => {
                        // Last fallback: try netstat
                        match tokio::process::Command::new("netstat")
                            .args(["-tln"])
                            .output()
                            .await
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
            
            // Try multiple methods to kill Tor processes
            // Method 1: killall
            let killall_result = command_runner.run_command("killall", &["tor"]);
            if let Err(e) = killall_result {
                debug!("killall failed: {}", e);
                
                // Method 2: pkill
                let pkill_result = command_runner.run_command("pkill", &["-f", "^tor$"]);
                if let Err(e2) = pkill_result {
                    debug!("pkill failed: {}", e2);
                    
                    // Method 3: Find PIDs with pgrep and kill them
                    if let Ok(pgrep_output) = command_runner.run_command("pgrep", &["-x", "tor"]) {
                        let pids_str = String::from_utf8_lossy(&pgrep_output.stdout);
                        for pid in pids_str.lines() {
                            if !pid.is_empty() {
                                debug!("Trying to kill Tor PID: {}", pid);
                                let _ = command_runner.run_command("kill", &["-TERM", pid]);
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                let _ = command_runner.run_command("kill", &["-KILL", pid]);
                            }
                        }
                    }
                } else {
                    debug!("Tor processes killed with pkill");
                }
            } else {
                debug!("Tor processes killed with killall");
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
        // Send proper Tor control protocol commands
        // First authenticate (if no password set, authenticate with empty password)
        // Then send SIGNAL SHUTDOWN
        let shutdown_cmd = format!(
            r#"printf "AUTHENTICATE \"\"\r\nSIGNAL SHUTDOWN\r\nQUIT\r\n" | nc localhost {} -w 3"#,
            self.control_port
        );
        
        debug!("Attempting graceful Tor shutdown via control port");
        match std::process::Command::new("sh")
            .args(["-c", &shutdown_cmd])
            .output()
        {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                debug!("Control port response: {}", output_str.trim());
                
                // Check if authentication and shutdown were successful
                if output_str.contains("250 OK") {
                    debug!("Tor graceful shutdown successful");
                    // Wait a bit for Tor to actually shut down
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    Ok(())
                } else {
                    debug!("Tor control response didn't indicate success: {}", output_str.trim());
                    Err("Control port shutdown failed".to_string())
                }
            }
            Err(e) => Err(format!("Failed to send shutdown signal: {}", e)),
        }
    }

    /// Refresh Tor circuit by sending NEWNYM signal
    pub fn refresh_circuit(&self) -> Result<(), String> {
        if !self.is_tor_running() {
            return Err("Tor daemon is not running".to_string());
        }

        let newnym_cmd = format!(
            r#"printf "AUTHENTICATE \"\"\r\nSIGNAL NEWNYM\r\nQUIT\r\n" | nc localhost {} -w 3"#,
            self.control_port
        );
        
        debug!("Refreshing Tor circuit");
        match std::process::Command::new("sh")
            .args(["-c", &newnym_cmd])
            .output()
        {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                debug!("Control port response: {}", output_str.trim());
                
                // Check for both authentication success (250 OK) and signal acceptance (250 OK)
                // Some Tor versions might respond differently
                if output_str.contains("250 OK") || output_str.contains("250") {
                    debug!("Tor circuit refreshed successfully");
                    Ok(())
                } else if output_str.contains("514") {
                    // Authentication required
                    Err("Tor control authentication failed - check if ControlPort has authentication enabled".to_string())
                } else {
                    debug!("Tor control response didn't indicate success: {}", output_str.trim());
                    warn!("Full stderr: {}", String::from_utf8_lossy(&output.stderr));
                    
                    // If we got this far, the command executed, so it might have worked anyway
                    // Some Tor configurations don't give the expected response format
                    if output.status.success() && output_str.trim().is_empty() {
                        debug!("Command executed successfully despite empty response - assuming circuit refresh worked");
                        Ok(())
                    } else {
                        Err(format!("Failed to refresh circuit: {}", output_str.trim()))
                    }
                }
            }
            Err(e) => Err(format!("Failed to send NEWNYM signal: {}", e)),
        }
    }

    /// Test Tor connection by checking IP via SOCKS proxy
    pub fn test_connection(&self) -> Result<String, String> {
        if !self.is_tor_running() {
            return Err("Tor daemon is not running".to_string());
        }

        // Test connection by fetching IP through Tor SOCKS proxy
        let test_cmd = format!(
            "curl --silent --max-time 10 --socks5-hostname localhost:{} https://httpbin.org/ip",
            self.socks_port
        );

        match std::process::Command::new("sh")
            .args(["-c", &test_cmd])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let response = String::from_utf8_lossy(&output.stdout);
                    debug!("Tor connection test successful: {}", response.trim());
                    
                    // Parse JSON to extract IP
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                        if let Some(origin) = parsed.get("origin").and_then(|o| o.as_str()) {
                            return Ok(format!("✓ Tor working - Exit IP: {}", origin));
                        }
                    }
                    Ok("✓ Tor connection working".to_string())
                } else {
                    let error_msg = format!("Connection test failed: {}", String::from_utf8_lossy(&output.stderr));
                    warn!("{}", error_msg);
                    Err(error_msg)
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to test connection: {}", e);
                error!("{}", error_msg);
                Err(error_msg)
            }
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

    /// Check if the application is running with torsocks (async version)
    pub async fn is_running_async(&self) -> bool {
        debug!("TOR_DEBUG: Starting TorsocksConfig.is_running_async() for '{}'", self.name);
        let start_time = std::time::Instant::now();
        
        // Check if there are processes matching our command
        let pattern = format!("torsocks {}", self.command);
        debug!("TOR_DEBUG: Looking for pattern: '{}'", pattern);
        
        let result = match tokio::process::Command::new("pgrep")
            .args(["-f", &pattern])
            .output()
            .await
        {
            Ok(output) => {
                let is_running = !output.stdout.is_empty();
                debug!("TOR_DEBUG: pgrep -f result for '{}': {} bytes stdout, running={}", 
                       self.name, output.stdout.len(), is_running);
                is_running
            },
            Err(e) => {
                debug!("TOR_DEBUG: pgrep -f error for '{}': {}", self.name, e);
                false
            }
        };
        
        let elapsed = start_time.elapsed();
        debug!("TOR_DEBUG: TorsocksConfig.is_running_async() for '{}' took {:?}, result={}", 
               self.name, elapsed, result);
        result
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
        actions.push(TorAction::RefreshCircuit);
        actions.push(TorAction::TestConnection);
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

/// Get Tor proxy actions based on current state (async version for streaming)
pub async fn get_tor_actions_async(torsocks_configs: &HashMap<String, TorsocksConfig>) -> Vec<TorAction> {
    debug!("TOR_DEBUG: Starting get_tor_actions_async() with {} torsocks configs", torsocks_configs.len());
    let start_time = std::time::Instant::now();
    
    let mut actions = Vec::new();
    let tor_manager = TorManager::new();

    // Tor daemon management - use async process check for streaming
    debug!("TOR_DEBUG: Checking if Tor is running...");
    let is_running = tor_manager.is_tor_running_async_fast().await;
    debug!("TOR_DEBUG: Tor running status: {}", is_running);
    
    if is_running {
        debug!("TOR_DEBUG: Adding Tor daemon actions (running)");
        actions.push(TorAction::StopTor);
        actions.push(TorAction::RestartTor);
        actions.push(TorAction::RefreshCircuit);
        actions.push(TorAction::TestConnection);
    } else {
        debug!("TOR_DEBUG: Adding Tor daemon actions (not running)");
        actions.push(TorAction::StartTor);
    }

    // Torsocks application management (only if Tor is running and torsocks is available)
    debug!("TOR_DEBUG: Checking torsocks availability...");
    let torsocks_available = is_command_installed("torsocks");
    debug!("TOR_DEBUG: torsocks available: {}", torsocks_available);
    
    if is_running && torsocks_available {
        debug!("TOR_DEBUG: Processing {} torsocks configs...", torsocks_configs.len());
        for (config_name, config) in torsocks_configs {
            debug!("TOR_DEBUG: Checking torsocks config '{}'", config_name);
            let config_start = std::time::Instant::now();
            
            // Use async version of is_running check
            if config.is_running_async().await {
                debug!("TOR_DEBUG: Config '{}' is running, adding stop action", config_name);
                actions.push(TorAction::StopTorsocks(config.clone()));
            } else {
                debug!("TOR_DEBUG: Config '{}' is not running, adding start action", config_name);
                actions.push(TorAction::StartTorsocks(config.clone()));
            }
            
            let config_elapsed = config_start.elapsed();
            debug!("TOR_DEBUG: Processing config '{}' took {:?}", config_name, config_elapsed);
        }
        debug!("TOR_DEBUG: Finished processing all torsocks configs");
    } else {
        debug!("TOR_DEBUG: Skipping torsocks configs (tor_running={}, torsocks_available={})", 
               is_running, torsocks_available);
    }

    let total_elapsed = start_time.elapsed();
    debug!("TOR_DEBUG: get_tor_actions_async() completed in {:?}, returning {} actions", 
           total_elapsed, actions.len());
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
        TorAction::RefreshCircuit => {
            format_entry("tor", "🔃", "Refresh Tor circuit")
        }
        TorAction::TestConnection => {
            format_entry("tor", "🧪", "Test Tor connection")
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
        TorAction::RefreshCircuit => {
            debug!("Refreshing Tor circuit");
            tor_manager.refresh_circuit()
        }
        TorAction::TestConnection => {
            debug!("Testing Tor connection");
            match tor_manager.test_connection() {
                Ok(result) => {
                    println!("{}", result);
                    Ok(())
                }
                Err(e) => Err(e)
            }
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