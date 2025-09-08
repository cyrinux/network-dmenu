//! Error recovery and retry logic for geofencing operations
//!
//! Provides resilient execution of network actions with exponential backoff
//! and intelligent failure handling.

use crate::geofencing::{GeofenceError, Result, ZoneActions};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay before first retry
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Whether to enable jitter to avoid thundering herd
    pub enable_jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            enable_jitter: true,
        }
    }
}

/// Types of actions that can be retried
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetryableAction {
    WiFiConnection(String),
    VpnConnection(String),
    BluetoothConnection(String),
    TailscaleExitNode(String),
    TailscaleShields(bool),
    CustomCommand(String),
}

/// Status of a retry attempt
#[derive(Debug, Clone, PartialEq)]
pub enum RetryStatus {
    Success,
    Failed(String),
    MaxRetriesExceeded,
}

/// Failed action awaiting retry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedAction {
    /// The action that failed
    pub action: RetryableAction,
    /// Number of attempts made
    pub attempt_count: u32,
    /// When to attempt the next retry
    pub next_retry: DateTime<Utc>,
    /// Last error message
    pub last_error: String,
    /// When the action was first attempted
    pub first_attempt: DateTime<Utc>,
    /// Zone that triggered this action
    pub zone_id: String,
}

/// Action execution context
#[derive(Debug)]
pub struct ActionContext {
    pub zone_id: String,
    pub zone_name: String,
    pub confidence: f64,
}

/// Type alias for success callback
type SuccessCallback = Box<dyn Fn(&RetryableAction) + Send + Sync>;
/// Type alias for failure callback
type FailureCallback = Box<dyn Fn(&RetryableAction, &str) + Send + Sync>;

/// Retry manager for handling failed actions
pub struct RetryManager {
    config: RetryConfig,
    failed_actions: VecDeque<FailedAction>,
    success_callback: Option<SuccessCallback>,
    failure_callback: Option<FailureCallback>,
}

impl RetryManager {
    /// Create new retry manager
    pub fn new(config: RetryConfig) -> Self {
        debug!(
            "Creating retry manager with config: max_retries={}, base_delay={:?}",
            config.max_retries, config.base_delay
        );

        Self {
            config,
            failed_actions: VecDeque::new(),
            success_callback: None,
            failure_callback: None,
        }
    }

    /// Set callback for successful retries
    pub fn on_success<F>(&mut self, callback: F)
    where
        F: Fn(&RetryableAction) + Send + Sync + 'static,
    {
        self.success_callback = Some(Box::new(callback));
    }

    /// Set callback for failed retries
    pub fn on_failure<F>(&mut self, callback: F)
    where
        F: Fn(&RetryableAction, &str) + Send + Sync + 'static,
    {
        self.failure_callback = Some(Box::new(callback));
    }

    /// Execute zone actions with retry logic
    pub async fn execute_zone_actions_with_retry(
        &mut self,
        actions: &ZoneActions,
        context: &ActionContext,
    ) -> Result<()> {
        info!(
            "🎯 ZONE CHANGE: Starting action execution for zone '{}' (confidence: {:.2})",
            context.zone_name, context.confidence
        );

        debug!("Zone action details: WiFi={:?}, VPN={:?}, Tailscale Exit Node={:?}, Tailscale Shields={:?}, {} Bluetooth devices, {} custom commands",
               actions.wifi, actions.vpn, actions.tailscale_exit_node, actions.tailscale_shields,
               actions.bluetooth.len(), actions.custom_commands.len());

        let action_start_time = std::time::Instant::now();
        let mut partial_failures = Vec::new();
        let mut success_count = 0;
        let total_actions = self.count_zone_actions(actions);

        info!(
            "📋 Zone '{}' has {} total actions to execute",
            context.zone_name, total_actions
        );

        // Execute WiFi action
        if let Some(ref wifi_ssid) = actions.wifi {
            info!(
                "📶 [1/{}] WiFi Action: Checking connection to SSID '{}'",
                total_actions, wifi_ssid
            );
            debug!(
                "WiFi connection check starting for zone '{}' to network '{}'",
                context.zone_name, wifi_ssid
            );

            // Check if already connected to avoid unnecessary reconnection
            if self.is_wifi_connected_to(wifi_ssid).await {
                success_count += 1;
                info!(
                    "✅ Already connected to WiFi '{}', skipping connection",
                    wifi_ssid
                );
                debug!(
                    "WiFi action skipped for zone '{}' - already connected to network '{}'",
                    context.zone_name, wifi_ssid
                );
            } else {
                info!(
                    "📶 [1/{}] WiFi Action: Connecting to SSID '{}'",
                    total_actions, wifi_ssid
                );
                let wifi_start = std::time::Instant::now();
                let action = RetryableAction::WiFiConnection(wifi_ssid.clone());
                match self.execute_with_retry(action.clone(), context).await {
                    RetryStatus::Success => {
                        success_count += 1;
                        info!(
                            "✅ WiFi connection to '{}' successful in {:?}",
                            wifi_ssid,
                            wifi_start.elapsed()
                        );
                        debug!("WiFi action completed successfully for zone '{}' - network '{}' is now active", 
                               context.zone_name, wifi_ssid);
                    }
                    RetryStatus::Failed(error) => {
                        warn!(
                            "❌ WiFi connection to '{}' failed after {:?}: {}",
                            wifi_ssid,
                            wifi_start.elapsed(),
                            error
                        );
                        debug!(
                            "WiFi action failed for zone '{}' - network '{}' connection unsuccessful",
                            context.zone_name, wifi_ssid
                        );
                        partial_failures.push((action, error));
                    }
                    RetryStatus::MaxRetriesExceeded => {
                        error!(
                            "⏰ WiFi connection to '{}' exceeded max retries after {:?}",
                            wifi_ssid,
                            wifi_start.elapsed()
                        );
                        debug!("WiFi action retry exhausted for zone '{}' - network '{}' connection abandoned", 
                               context.zone_name, wifi_ssid);
                        partial_failures.push((action, "Max retries exceeded".to_string()));
                    }
                }
            }
        }

        // Execute VPN action
        if let Some(ref vpn_name) = actions.vpn {
            let vpn_action_num = if actions.wifi.is_some() { 2 } else { 1 };
            info!(
                "🔐 [{}/{}] VPN Action: Connecting to VPN '{}'",
                vpn_action_num, total_actions, vpn_name
            );
            debug!(
                "VPN connection attempt starting for zone '{}' to provider '{}'",
                context.zone_name, vpn_name
            );

            let vpn_start = std::time::Instant::now();
            let action = RetryableAction::VpnConnection(vpn_name.clone());
            match self.execute_with_retry(action.clone(), context).await {
                RetryStatus::Success => {
                    success_count += 1;
                    info!(
                        "✅ VPN connection to '{}' successful in {:?}",
                        vpn_name,
                        vpn_start.elapsed()
                    );
                    debug!("VPN action completed successfully for zone '{}' - provider '{}' is now active", 
                           context.zone_name, vpn_name);
                }
                RetryStatus::Failed(error) => {
                    warn!(
                        "❌ VPN connection to '{}' failed after {:?}: {}",
                        vpn_name,
                        vpn_start.elapsed(),
                        error
                    );
                    debug!(
                        "VPN action failed for zone '{}' - provider '{}' connection unsuccessful",
                        context.zone_name, vpn_name
                    );
                    partial_failures.push((action, error));
                }
                RetryStatus::MaxRetriesExceeded => {
                    error!(
                        "⏰ VPN connection to '{}' exceeded max retries after {:?}",
                        vpn_name,
                        vpn_start.elapsed()
                    );
                    debug!("VPN action retry exhausted for zone '{}' - provider '{}' connection abandoned", 
                           context.zone_name, vpn_name);
                    partial_failures.push((action, "Max retries exceeded".to_string()));
                }
            }
        }

        // Execute Tailscale exit node action
        if let Some(ref exit_node) = actions.tailscale_exit_node {
            let mut action_num = 1;
            if actions.wifi.is_some() {
                action_num += 1;
            }
            if actions.vpn.is_some() {
                action_num += 1;
            }

            info!(
                "🌐 [{}/{}] Tailscale Exit Node: Setting to '{}'",
                action_num, total_actions, exit_node
            );
            debug!(
                "Tailscale exit node configuration starting for zone '{}' - switching to node '{}'",
                context.zone_name, exit_node
            );

            let exit_start = std::time::Instant::now();
            let action = RetryableAction::TailscaleExitNode(exit_node.clone());
            match self.execute_with_retry(action.clone(), context).await {
                RetryStatus::Success => {
                    success_count += 1;
                    info!(
                        "✅ Tailscale exit node '{}' configured successfully in {:?}",
                        exit_node,
                        exit_start.elapsed()
                    );
                    debug!("Tailscale exit node action completed for zone '{}' - now routing through '{}'", 
                           context.zone_name, exit_node);
                }
                RetryStatus::Failed(error) => {
                    warn!(
                        "❌ Tailscale exit node '{}' failed after {:?}: {}",
                        exit_node,
                        exit_start.elapsed(),
                        error
                    );
                    debug!("Tailscale exit node action failed for zone '{}' - node '{}' configuration unsuccessful", 
                           context.zone_name, exit_node);
                    partial_failures.push((action, error));
                }
                RetryStatus::MaxRetriesExceeded => {
                    error!(
                        "⏰ Tailscale exit node '{}' exceeded max retries after {:?}",
                        exit_node,
                        exit_start.elapsed()
                    );
                    debug!("Tailscale exit node retry exhausted for zone '{}' - node '{}' configuration abandoned", 
                           context.zone_name, exit_node);
                    partial_failures.push((action, "Max retries exceeded".to_string()));
                }
            }
        }

        // Execute Tailscale shields action
        if let Some(shields_up) = actions.tailscale_shields {
            let mut action_num = 1;
            if actions.wifi.is_some() {
                action_num += 1;
            }
            if actions.vpn.is_some() {
                action_num += 1;
            }
            if actions.tailscale_exit_node.is_some() {
                action_num += 1;
            }

            let shield_status = if shields_up { "ENABLING" } else { "DISABLING" };
            info!(
                "🛡️  [{}/{}] Tailscale Shields: {} for zone '{}'",
                action_num, total_actions, shield_status, context.zone_name
            );
            debug!(
                "Tailscale shields configuration starting for zone '{}' - setting shields_up={}",
                context.zone_name, shields_up
            );

            let shields_start = std::time::Instant::now();
            let action = RetryableAction::TailscaleShields(shields_up);
            match self.execute_with_retry(action.clone(), context).await {
                RetryStatus::Success => {
                    success_count += 1;
                    let status_msg = if shields_up { "ENABLED" } else { "DISABLED" };
                    info!(
                        "✅ Tailscale shields {} successfully in {:?}",
                        status_msg,
                        shields_start.elapsed()
                    );
                    debug!(
                        "Tailscale shields action completed for zone '{}' - shields are now {}",
                        context.zone_name,
                        if shields_up {
                            "active (blocking connections)"
                        } else {
                            "inactive (allowing connections)"
                        }
                    );
                }
                RetryStatus::Failed(error) => {
                    let status_msg = if shields_up { "enable" } else { "disable" };
                    warn!(
                        "❌ Tailscale shields {} failed after {:?}: {}",
                        status_msg,
                        shields_start.elapsed(),
                        error
                    );
                    debug!("Tailscale shields action failed for zone '{}' - shields configuration unsuccessful", 
                           context.zone_name);
                    partial_failures.push((action, error));
                }
                RetryStatus::MaxRetriesExceeded => {
                    let status_msg = if shields_up { "enable" } else { "disable" };
                    error!(
                        "⏰ Tailscale shields {} exceeded max retries after {:?}",
                        status_msg,
                        shields_start.elapsed()
                    );
                    debug!("Tailscale shields retry exhausted for zone '{}' - shields configuration abandoned", 
                           context.zone_name);
                    partial_failures.push((action, "Max retries exceeded".to_string()));
                }
            }
        }

        // Execute Bluetooth actions
        if !actions.bluetooth.is_empty() {
            let mut bluetooth_action_num = 1;
            if actions.wifi.is_some() {
                bluetooth_action_num += 1;
            }
            if actions.vpn.is_some() {
                bluetooth_action_num += 1;
            }
            if actions.tailscale_exit_node.is_some() {
                bluetooth_action_num += 1;
            }
            if actions.tailscale_shields.is_some() {
                bluetooth_action_num += 1;
            }

            info!(
                "📱 [{}/{}] Bluetooth Actions: Connecting {} devices",
                bluetooth_action_num,
                total_actions,
                actions.bluetooth.len()
            );
            debug!(
                "Bluetooth connections starting for zone '{}' - devices: {:?}",
                context.zone_name, actions.bluetooth
            );
        }

        for (bt_index, device_name) in actions.bluetooth.iter().enumerate() {
            // Check if already connected to avoid unnecessary reconnection
            if self.is_bluetooth_connected_to(device_name).await {
                success_count += 1;
                info!(
                    "✅ Already connected to Bluetooth device '{}', skipping connection",
                    device_name
                );
                debug!(
                    "Bluetooth device {} ({}/{}) already connected for zone '{}'",
                    device_name,
                    bt_index + 1,
                    actions.bluetooth.len(),
                    context.zone_name
                );
                continue;
            }

            let bt_start = std::time::Instant::now();
            let action = RetryableAction::BluetoothConnection(device_name.clone());
            match self.execute_with_retry(action.clone(), context).await {
                RetryStatus::Success => {
                    success_count += 1;
                    info!(
                        "✅ Bluetooth device '{}' connected successfully in {:?}",
                        device_name,
                        bt_start.elapsed()
                    );
                    debug!(
                        "Bluetooth device {} ({}/{}) connected for zone '{}'",
                        device_name,
                        bt_index + 1,
                        actions.bluetooth.len(),
                        context.zone_name
                    );
                }
                RetryStatus::Failed(error) => {
                    warn!(
                        "❌ Bluetooth device '{}' connection failed after {:?}: {}",
                        device_name,
                        bt_start.elapsed(),
                        error
                    );
                    debug!("Bluetooth device {} ({}/{}) failed for zone '{}' - connection unsuccessful", 
                           device_name, bt_index + 1, actions.bluetooth.len(), context.zone_name);
                    partial_failures.push((action, error));
                }
                RetryStatus::MaxRetriesExceeded => {
                    error!(
                        "⏰ Bluetooth device '{}' connection exceeded max retries after {:?}",
                        device_name,
                        bt_start.elapsed()
                    );
                    debug!(
                        "Bluetooth device {} retry exhausted for zone '{}' - connection abandoned",
                        device_name, context.zone_name
                    );
                    partial_failures.push((action, "Max retries exceeded".to_string()));
                }
            }
        }

        // Execute custom commands
        if !actions.custom_commands.is_empty() {
            let mut custom_action_num = 1;
            if actions.wifi.is_some() {
                custom_action_num += 1;
            }
            if actions.vpn.is_some() {
                custom_action_num += 1;
            }
            if actions.tailscale_exit_node.is_some() {
                custom_action_num += 1;
            }
            if actions.tailscale_shields.is_some() {
                custom_action_num += 1;
            }
            if !actions.bluetooth.is_empty() {
                custom_action_num += 1;
            }

            info!(
                "⚙️  [{}/{}] Custom Commands: Executing {} commands",
                custom_action_num,
                total_actions,
                actions.custom_commands.len()
            );
            debug!(
                "Custom commands starting for zone '{}' - commands: {:?}",
                context.zone_name, actions.custom_commands
            );
        }

        for (cmd_index, command) in actions.custom_commands.iter().enumerate() {
            let cmd_start = std::time::Instant::now();
            let action = RetryableAction::CustomCommand(command.clone());
            match self.execute_with_retry(action.clone(), context).await {
                RetryStatus::Success => {
                    success_count += 1;
                    info!(
                        "✅ Custom command '{}' executed successfully in {:?}",
                        command,
                        cmd_start.elapsed()
                    );
                    debug!(
                        "Custom command {} ({}/{}) executed for zone '{}'",
                        command,
                        cmd_index + 1,
                        actions.custom_commands.len(),
                        context.zone_name
                    );
                }
                RetryStatus::Failed(error) => {
                    warn!(
                        "❌ Custom command '{}' failed after {:?}: {}",
                        command,
                        cmd_start.elapsed(),
                        error
                    );
                    debug!(
                        "Custom command {} ({}/{}) failed for zone '{}' - execution unsuccessful",
                        command,
                        cmd_index + 1,
                        actions.custom_commands.len(),
                        context.zone_name
                    );
                    partial_failures.push((action, error));
                }
                RetryStatus::MaxRetriesExceeded => {
                    error!(
                        "⏰ Custom command '{}' exceeded max retries after {:?}",
                        command,
                        cmd_start.elapsed()
                    );
                    debug!(
                        "Custom command {} retry exhausted for zone '{}' - execution abandoned",
                        command, context.zone_name
                    );
                    partial_failures.push((action, "Max retries exceeded".to_string()));
                }
            }
        }

        // Generate comprehensive execution summary
        let total_execution_time = action_start_time.elapsed();
        let failure_count = partial_failures.len();

        if failure_count == 0 {
            info!("🎉 ZONE CHANGE COMPLETE: All {} actions for zone '{}' executed successfully in {:?}", 
                  success_count, context.zone_name, total_execution_time);
            debug!(
                "Zone action execution perfect success for '{}' - all systems configured",
                context.zone_name
            );
        } else if success_count > 0 {
            warn!("⚠️  ZONE CHANGE PARTIAL: {}/{} actions succeeded for zone '{}' in {:?} - {} failed", 
                  success_count, total_actions, context.zone_name, total_execution_time, failure_count);
            debug!("Zone action execution partially completed for '{}' - some systems may need manual intervention", 
                   context.zone_name);
        } else {
            error!(
                "❌ ZONE CHANGE FAILED: All {} actions for zone '{}' failed in {:?}",
                total_actions, context.zone_name, total_execution_time
            );
            debug!(
                "Zone action execution completely failed for '{}' - zone change unsuccessful",
                context.zone_name
            );
        }

        // Handle partial failures
        if !partial_failures.is_empty() {
            warn!(
                "📋 Failed actions summary for zone '{}':",
                context.zone_name
            );
            for (i, (action, error)) in partial_failures.iter().enumerate() {
                warn!("  {}: {:?} - {}", i + 1, action, error);
            }

            // Add failed actions to retry queue
            for (action, error) in partial_failures {
                debug!(
                    "Scheduling retry for failed action {:?} in zone '{}'",
                    action, context.zone_name
                );
                self.schedule_retry(action, error, &context.zone_id);
            }

            info!("Failed actions have been queued for automatic retry");
        }

        // Final debug summary
        debug!("Zone action execution completed for zone '{}' - Total: {}, Success: {}, Failed: {}, Duration: {:?}", 
               context.zone_name, total_actions, success_count, failure_count, total_execution_time);
        Ok(())
    }

    /// Execute a single action with retry logic
    async fn execute_with_retry(
        &self,
        action: RetryableAction,
        context: &ActionContext,
    ) -> RetryStatus {
        let mut attempt = 0;
        debug!(
            "Starting retry execution for action {:?} in zone '{}' with confidence {:.2}",
            action, context.zone_name, context.confidence
        );

        while attempt <= self.config.max_retries {
            attempt += 1;
            debug!(
                "Attempting action {:?} (attempt {}/{}) for zone '{}' with confidence {:.2}",
                action,
                attempt,
                self.config.max_retries + 1,
                context.zone_name,
                context.confidence
            );

            match self.execute_action(&action).await {
                Ok(()) => {
                    debug!("Action {:?} succeeded on attempt {}", action, attempt);
                    if let Some(ref callback) = self.success_callback {
                        callback(&action);
                    }
                    return RetryStatus::Success;
                }
                Err(error) => {
                    let error_msg = error.to_string();
                    debug!(
                        "Action {:?} failed on attempt {} in zone '{}': {}",
                        action, attempt, context.zone_name, error_msg
                    );

                    if attempt > self.config.max_retries {
                        error!(
                            "Action {:?} exceeded max retries in zone '{}' (confidence: {:.2})",
                            action, context.zone_name, context.confidence
                        );
                        if let Some(ref callback) = self.failure_callback {
                            callback(
                                &action,
                                &format!(
                                    "Zone: {}, Confidence: {:.2}, Error: {}",
                                    context.zone_name, context.confidence, error_msg
                                ),
                            );
                        }
                        return RetryStatus::MaxRetriesExceeded;
                    }

                    // Calculate delay for next attempt
                    let delay = self.calculate_retry_delay(attempt);
                    debug!("Waiting {:?} before retry attempt {}", delay, attempt + 1);
                    sleep(delay).await;
                }
            }
        }

        RetryStatus::Failed("Unexpected retry loop exit".to_string())
    }

    /// Execute a single action
    async fn execute_action(&self, action: &RetryableAction) -> Result<()> {
        use crate::command::{CommandRunner, RealCommandRunner};

        let command_runner = RealCommandRunner;

        match action {
            RetryableAction::WiFiConnection(ssid) => {
                debug!("Executing WiFi connection to '{}'", ssid);

                if crate::command::is_command_installed("nmcli") {
                    let result =
                        command_runner.run_command("nmcli", &["device", "wifi", "connect", ssid]);

                    match result {
                        Ok(output) if output.status.success() => {
                            debug!("WiFi connection successful");
                            Ok(())
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(GeofenceError::ActionExecution(format!(
                                "WiFi connection failed: {}",
                                stderr
                            )))
                        }
                        Err(e) => Err(GeofenceError::ActionExecution(format!(
                            "Failed to execute nmcli: {}",
                            e
                        ))),
                    }
                } else {
                    Err(GeofenceError::ActionExecution(
                        "nmcli not available for WiFi connection".to_string(),
                    ))
                }
            }

            RetryableAction::VpnConnection(vpn_name) => {
                debug!("Executing VPN connection to '{}'", vpn_name);

                if crate::command::is_command_installed("nmcli") {
                    let result =
                        command_runner.run_command("nmcli", &["connection", "up", vpn_name]);

                    match result {
                        Ok(output) if output.status.success() => {
                            debug!("VPN connection successful");
                            Ok(())
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(GeofenceError::ActionExecution(format!(
                                "VPN connection failed: {}",
                                stderr
                            )))
                        }
                        Err(e) => Err(GeofenceError::ActionExecution(format!(
                            "Failed to execute nmcli: {}",
                            e
                        ))),
                    }
                } else {
                    Err(GeofenceError::ActionExecution(
                        "nmcli not available for VPN connection".to_string(),
                    ))
                }
            }

            RetryableAction::BluetoothConnection(device_name) => {
                debug!("Executing Bluetooth connection to '{}'", device_name);

                if crate::command::is_command_installed("bluetoothctl") {
                    // First get device address
                    let devices_result = command_runner.run_command("bluetoothctl", &["devices"]);

                    match devices_result {
                        Ok(output) if output.status.success() => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let mut device_address: Option<String> = None;

                            for line in stdout.lines() {
                                if line.contains(device_name) {
                                    if let Some(address_start) = line.find("Device ") {
                                        if let Some(address_end) =
                                            line[address_start + 7..].find(' ')
                                        {
                                            device_address = Some(
                                                line[address_start + 7
                                                    ..address_start + 7 + address_end]
                                                    .to_string(),
                                            );
                                            break;
                                        }
                                    }
                                }
                            }

                            if let Some(address) = device_address {
                                let connect_result = command_runner
                                    .run_command("bluetoothctl", &["connect", &address]);

                                match connect_result {
                                    Ok(output) if output.status.success() => {
                                        debug!("Bluetooth connection successful");
                                        Ok(())
                                    }
                                    Ok(output) => {
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        Err(GeofenceError::ActionExecution(format!(
                                            "Bluetooth connection failed: {}",
                                            stderr
                                        )))
                                    }
                                    Err(e) => Err(GeofenceError::ActionExecution(format!(
                                        "Failed to execute bluetoothctl connect: {}",
                                        e
                                    ))),
                                }
                            } else {
                                Err(GeofenceError::ActionExecution(format!(
                                    "Bluetooth device '{}' not found",
                                    device_name
                                )))
                            }
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(GeofenceError::ActionExecution(format!(
                                "Failed to list Bluetooth devices: {}",
                                stderr
                            )))
                        }
                        Err(e) => Err(GeofenceError::ActionExecution(format!(
                            "Failed to execute bluetoothctl devices: {}",
                            e
                        ))),
                    }
                } else {
                    Err(GeofenceError::ActionExecution(
                        "bluetoothctl not available for Bluetooth connection".to_string(),
                    ))
                }
            }

            RetryableAction::TailscaleExitNode(exit_node) => {
                debug!(
                    "Executing Tailscale exit node configuration: '{}'",
                    exit_node
                );

                if crate::command::is_command_installed("tailscale") {
                    let exit_node_arg = format!("--exit-node={}", exit_node);
                    let result = command_runner.run_command("tailscale", &["set", &exit_node_arg]);

                    match result {
                        Ok(output) if output.status.success() => {
                            debug!("Tailscale exit node configuration successful");
                            Ok(())
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(GeofenceError::ActionExecution(format!(
                                "Tailscale exit node configuration failed: {}",
                                stderr
                            )))
                        }
                        Err(e) => Err(GeofenceError::ActionExecution(format!(
                            "Failed to execute tailscale: {}",
                            e
                        ))),
                    }
                } else {
                    Err(GeofenceError::ActionExecution(
                        "tailscale not available for exit node configuration".to_string(),
                    ))
                }
            }

            RetryableAction::TailscaleShields(shields_up) => {
                let action_desc = if *shields_up { "ENABLING" } else { "DISABLING" };
                debug!(
                    "🛡️  Executing Tailscale shields configuration: {} (shields_up={})",
                    action_desc, shields_up
                );

                if crate::command::is_command_installed("tailscale") {
                    debug!("Tailscale command found - proceeding with shields configuration");

                    let shield_arg = if *shields_up {
                        "--shields-up=true"
                    } else {
                        "--shields-up=false"
                    };

                    debug!("Executing command: tailscale set {}", shield_arg);
                    let cmd_start = std::time::Instant::now();
                    let result = command_runner.run_command("tailscale", &["set", shield_arg]);
                    let cmd_duration = cmd_start.elapsed();

                    match result {
                        Ok(output) if output.status.success() => {
                            let status_msg = if *shields_up { "ENABLED" } else { "DISABLED" };
                            debug!("✅ Tailscale shields configuration successful - shields are now {} (took {:?})", status_msg, cmd_duration);

                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if !stdout.trim().is_empty() {
                                debug!("Tailscale command output: {}", stdout.trim());
                            }
                            Ok(())
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            warn!("❌ Tailscale shields command failed after {:?} with exit code: {:?}", 
                                 cmd_duration, output.status.code());

                            if !stdout.trim().is_empty() {
                                warn!("Tailscale stdout: {}", stdout.trim());
                            }
                            if !stderr.trim().is_empty() {
                                warn!("Tailscale stderr: {}", stderr.trim());
                            }

                            Err(GeofenceError::ActionExecution(format!(
                                "Tailscale shields configuration failed (exit code: {:?}): {}",
                                output.status.code(),
                                stderr
                            )))
                        }
                        Err(e) => {
                            error!(
                                "❌ Failed to execute tailscale command after {:?}: {}",
                                cmd_duration, e
                            );
                            Err(GeofenceError::ActionExecution(format!(
                                "Failed to execute tailscale: {}",
                                e
                            )))
                        }
                    }
                } else {
                    warn!("⚠️  Tailscale command not found - cannot configure shields");
                    Err(GeofenceError::ActionExecution(
                        "tailscale not available for shields configuration".to_string(),
                    ))
                }
            }

            RetryableAction::CustomCommand(command) => {
                debug!("Executing custom command: '{}'", command);

                // Use enhanced security check
                if !self.is_safe_command(command) {
                    return Err(GeofenceError::ActionExecution(format!(
                        "Command failed security check: {}",
                        command
                    )));
                }

                match tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                {
                    Ok(output) if output.status.success() => {
                        debug!("Custom command executed successfully");
                        Ok(())
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(GeofenceError::ActionExecution(format!(
                            "Custom command failed: {}",
                            stderr
                        )))
                    }
                    Err(e) => Err(GeofenceError::ActionExecution(format!(
                        "Failed to execute custom command: {}",
                        e
                    ))),
                }
            }
        }
    }

    /// Enhanced security check for custom commands
    fn is_safe_command(&self, command: &str) -> bool {
        let command_lower = command.to_lowercase();

        // Enhanced dangerous patterns detection
        let dangerous_patterns = [
            // File system operations
            "rm -rf",
            "rm -r",
            "sudo rm",
            "format",
            "mkfs",
            "dd if=",
            "fdisk",
            "parted",
            "> /dev/",
            "truncate",
            "shred",
            // System control
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "systemctl reboot",
            "systemctl poweroff",
            // Network security
            "iptables -F",
            "ufw --force",
            "ufw disable",
            "systemctl stop firewalld",
            // Permissions
            "chmod 777",
            "chmod -R 777",
            "chown -R root",
            "chmod u+s",
            // Network access
            "curl",
            "wget",
            "nc ",
            "netcat",
            "telnet",
            "ssh ",
            "scp ",
            "rsync",
            // Code execution
            "python -c",
            "perl -e",
            "ruby -e",
            "node -e",
            "eval",
            "exec",
            "`",
            "$(",
            // User management
            "passwd",
            "su ",
            "sudo su",
            "usermod",
            "userdel",
            "useradd",
            // System files
            "/etc/shadow",
            "/etc/passwd",
            "/etc/sudoers",
            "crontab",
            // Sensitive directories
            "/var/",
            "/etc/",
            "/root/",
            "/boot/",
            "/sys/",
            "/proc/kernel",
            // Package management
            "apt install",
            "yum install",
            "dnf install",
            "pacman -S",
            "pkg install",
            // Process manipulation
            "kill -9",
            "killall -9",
            "pkill -9",
        ];

        // Check for dangerous patterns
        for pattern in &dangerous_patterns {
            if command_lower.contains(pattern) {
                debug!(
                    "Command blocked by dangerous pattern '{}': {}",
                    pattern, command
                );
                return false;
            }
        }

        // Enhanced safe command whitelist
        let safe_prefixes = [
            "systemctl --user start",
            "systemctl --user stop",
            "systemctl --user restart",
            "systemctl --user reload",
            "systemctl --user enable",
            "systemctl --user disable",
            "notify-send",
            "zenity",
            "kdialog",
            "echo ",
            "printf ",
            "logger ",
            "touch /tmp/",
            "mkdir -p /tmp/",
            "rm /tmp/",
            "gsettings set",
            "dconf write",
            "xrandr --output",
            "brightnessctl set",
            "amixer set",
            "amixer sset",
        ];

        // Check for safe command prefixes
        for prefix in &safe_prefixes {
            if command_lower.starts_with(prefix) {
                debug!("Command allowed by safe prefix '{}': {}", prefix, command);
                return true;
            }
        }

        // Additional checks for specific safe patterns
        let safe_patterns = [
            r"^firefox --new-window",
            r"^chromium --new-window",
            r"^google-chrome --new-window",
            r"^code --new-window",
            r"^alacritty -e",
            r"^gnome-terminal --",
            r"^konsole -e",
        ];

        for pattern in &safe_patterns {
            if regex::Regex::new(pattern).unwrap().is_match(&command_lower) {
                debug!("Command allowed by safe pattern '{}': {}", pattern, command);
                return true;
            }
        }

        debug!("Command rejected by security policy: {}", command);
        false
    }

    /// Schedule a failed action for retry
    fn schedule_retry(&mut self, action: RetryableAction, error: String, zone_id: &str) {
        let now = Utc::now();
        let next_retry = now + ChronoDuration::seconds(self.config.base_delay.as_secs() as i64);

        let failed_action = FailedAction {
            action,
            attempt_count: 0,
            next_retry,
            last_error: error,
            first_attempt: now,
            zone_id: zone_id.to_string(),
        };

        debug!("Scheduling action for retry: {:?}", failed_action);
        self.failed_actions.push_back(failed_action);
    }

    /// Process retry queue
    pub async fn process_retries(&mut self) -> Vec<RetryStatus> {
        let now = Utc::now();
        let mut results = Vec::new();
        let mut remaining_actions = VecDeque::new();

        debug!(
            "Processing {} actions in retry queue",
            self.failed_actions.len()
        );

        while let Some(mut failed_action) = self.failed_actions.pop_front() {
            if failed_action.next_retry <= now {
                debug!("Retrying action: {:?}", failed_action.action);

                let context = ActionContext {
                    zone_id: failed_action.zone_id.clone(),
                    zone_name: "Retry".to_string(),
                    confidence: 1.0,
                };

                let status = self
                    .execute_with_retry(failed_action.action.clone(), &context)
                    .await;
                results.push(status.clone());

                match status {
                    RetryStatus::Success => {
                        info!("Retry successful for action: {:?}", failed_action.action);
                    }
                    RetryStatus::Failed(_) => {
                        failed_action.attempt_count += 1;
                        if failed_action.attempt_count < self.config.max_retries {
                            // Schedule for next retry
                            let delay = self.calculate_retry_delay(failed_action.attempt_count);
                            failed_action.next_retry = now
                                + ChronoDuration::from_std(delay)
                                    .unwrap_or(ChronoDuration::seconds(60));
                            remaining_actions.push_back(failed_action);
                        } else {
                            warn!("Action exceeded max retries: {:?}", failed_action.action);
                        }
                    }
                    RetryStatus::MaxRetriesExceeded => {
                        warn!("Action max retries exceeded: {:?}", failed_action.action);
                    }
                }
            } else {
                // Not yet time to retry
                remaining_actions.push_back(failed_action);
            }
        }

        self.failed_actions = remaining_actions;
        debug!(
            "Retry processing complete. {} actions remain in queue",
            self.failed_actions.len()
        );

        results
    }

    /// Calculate retry delay with exponential backoff and jitter
    fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        let delay_secs = (self.config.base_delay.as_secs_f64()
            * self.config.backoff_multiplier.powi(attempt as i32))
        .min(self.config.max_delay.as_secs_f64());

        let delay = Duration::from_secs_f64(delay_secs);

        if self.config.enable_jitter {
            // Add up to 25% jitter to prevent thundering herd
            let jitter_range = delay.as_secs_f64() * 0.25;
            let jitter = fastrand::f64() * jitter_range;
            Duration::from_secs_f64(delay.as_secs_f64() + jitter)
        } else {
            delay
        }
    }

    /// Count total number of actions in zone configuration
    fn count_zone_actions(&self, actions: &ZoneActions) -> usize {
        let mut count = 0;
        if actions.wifi.is_some() {
            count += 1;
        }
        if actions.vpn.is_some() {
            count += 1;
        }
        if actions.tailscale_exit_node.is_some() {
            count += 1;
        }
        if actions.tailscale_shields.is_some() {
            count += 1;
        }
        count += actions.bluetooth.len();
        count += actions.custom_commands.len();
        count
    }

    /// Get current retry queue status
    pub fn get_retry_status(&self) -> RetryQueueStatus {
        RetryQueueStatus {
            pending_retries: self.failed_actions.len(),
            actions: self.failed_actions.iter().cloned().collect(),
        }
    }

    /// Clear retry queue
    pub fn clear_retry_queue(&mut self) {
        debug!(
            "Clearing retry queue with {} pending actions",
            self.failed_actions.len()
        );
        self.failed_actions.clear();
    }

    /// Check if already connected to a specific WiFi network
    async fn is_wifi_connected_to(&self, target_ssid: &str) -> bool {
        debug!("Checking if already connected to WiFi: '{}'", target_ssid);

        // Use daemon's function to get current WiFi SSID
        if let Some(current_ssid) =
            crate::geofencing::daemon::GeofencingDaemon::get_current_wifi_ssid().await
        {
            if current_ssid == target_ssid {
                debug!(
                    "Already connected to WiFi '{}' (current: '{}')",
                    target_ssid, current_ssid
                );
                return true;
            }
        }

        debug!("Not currently connected to WiFi '{}'", target_ssid);
        false
    }

    /// Check if already connected to a specific Bluetooth device
    async fn is_bluetooth_connected_to(&self, target_device: &str) -> bool {
        use crate::command::{CommandRunner, RealCommandRunner};
        let command_runner = RealCommandRunner;

        debug!(
            "Checking if already connected to Bluetooth device: '{}'",
            target_device
        );

        if !crate::command::is_command_installed("bluetoothctl") {
            debug!("bluetoothctl not available, skipping Bluetooth connection check");
            return false;
        }

        // Use bluetoothctl to check connected devices
        if let Ok(output) = command_runner.run_command("bluetoothctl", &["info", target_device]) {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout);
                if info.contains("Connected: yes") {
                    debug!("Already connected to Bluetooth device '{}'", target_device);
                    return true;
                }
            }
        }

        debug!(
            "Not currently connected to Bluetooth device '{}'",
            target_device
        );
        false
    }
}

/// Status of the retry queue
#[derive(Debug, Serialize, Deserialize)]
pub struct RetryQueueStatus {
    pub pending_retries: usize,
    pub actions: Vec<FailedAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = RetryConfig {
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            enable_jitter: false,
            ..Default::default()
        };

        let manager = RetryManager::new(config);

        let delay1 = manager.calculate_retry_delay(1);
        let delay2 = manager.calculate_retry_delay(2);
        let delay3 = manager.calculate_retry_delay(3);

        assert_eq!(delay1.as_secs(), 2);
        assert_eq!(delay2.as_secs(), 4);
        assert_eq!(delay3.as_secs(), 8);
    }

    #[test]
    fn test_safe_command_validation() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);

        // Safe commands
        assert!(manager.is_safe_command("systemctl --user start syncthing"));
        assert!(manager.is_safe_command("notify-send 'Hello World'"));
        assert!(manager.is_safe_command("echo 'test'"));

        // Dangerous commands
        assert!(!manager.is_safe_command("rm -rf /"));
        assert!(!manager.is_safe_command("sudo shutdown now"));
        assert!(!manager.is_safe_command("curl http://malicious.com"));
        assert!(!manager.is_safe_command("python -c 'import os; os.system(\"rm -rf /\")'"));
    }

    #[tokio::test]
    async fn test_retry_manager_creation() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);

        assert_eq!(manager.failed_actions.len(), 0);
        assert!(manager.success_callback.is_none());
        assert!(manager.failure_callback.is_none());
    }
}
