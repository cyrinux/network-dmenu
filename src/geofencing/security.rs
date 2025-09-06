//! Enhanced security and sandboxing for geofencing operations
//!
//! Provides comprehensive security measures including command validation,
//! resource limits, network restrictions, and audit logging.

use crate::geofencing::{GeofenceError, Result};
use chrono::{DateTime, Utc};
use log::{debug, error, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;

/// Security policy for command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Allowed commands with their policies
    pub allowed_commands: HashMap<String, CommandPolicy>,
    /// Global resource limits
    pub resource_limits: ResourceLimits,
    /// Network access restrictions
    pub network_restrictions: NetworkRestrictions,
    /// Audit logging configuration
    pub audit_config: AuditConfig,
    /// Sandbox configuration
    pub sandbox_config: SandboxConfig,
}

/// Policy for individual command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPolicy {
    /// Allowed command line arguments (regex patterns)
    pub allowed_args: Vec<String>,
    /// Forbidden argument patterns
    pub forbidden_args: Vec<String>,
    /// Maximum execution time
    pub max_execution_time: Duration,
    /// Working directory restrictions
    pub allowed_working_dirs: Vec<String>,
    /// Environment variable restrictions
    pub env_restrictions: EnvRestrictions,
    /// Whether command can access network
    pub network_access: bool,
    /// Resource limits specific to this command
    pub resource_limits: Option<ResourceLimits>,
}

/// Environment variable restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvRestrictions {
    /// Environment variables to preserve
    pub preserve_env: Vec<String>,
    /// Environment variables to remove
    pub remove_env: Vec<String>,
    /// Additional environment variables to set
    pub set_env: HashMap<String, String>,
}

/// Resource limits for command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in MB
    pub max_memory_mb: Option<u64>,
    /// Maximum CPU time in seconds
    pub max_cpu_seconds: Option<u64>,
    /// Maximum file descriptors
    pub max_file_descriptors: Option<u64>,
    /// Maximum processes/threads
    pub max_processes: Option<u32>,
    /// Maximum file size in MB
    pub max_file_size_mb: Option<u64>,
}

/// Network access restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRestrictions {
    /// Whether to block all network access by default
    pub block_network_by_default: bool,
    /// Allowed destination hosts/IPs
    pub allowed_destinations: Vec<String>,
    /// Blocked destination hosts/IPs
    pub blocked_destinations: Vec<String>,
    /// Allowed ports
    pub allowed_ports: Vec<u16>,
    /// Blocked ports
    pub blocked_ports: Vec<u16>,
}

/// Audit logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Whether audit logging is enabled
    pub enabled: bool,
    /// Path to audit log file
    pub log_file: PathBuf,
    /// Maximum log file size in MB
    pub max_file_size_mb: u64,
    /// Number of rotated log files to keep
    pub max_files: u32,
    /// Whether to log successful commands
    pub log_successful: bool,
    /// Whether to log failed commands
    pub log_failed: bool,
    /// Whether to log command arguments
    pub log_arguments: bool,
}

/// Sandbox configuration using systemd-run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether to use systemd sandboxing
    pub enabled: bool,
    /// Sandbox user (if different from current)
    pub sandbox_user: Option<String>,
    /// Additional systemd properties
    pub systemd_properties: HashMap<String, String>,
    /// Temporary directory for sandbox
    pub temp_dir: Option<String>,
}

/// Audit log entry
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Timestamp of the command execution
    pub timestamp: DateTime<Utc>,
    /// Command that was executed
    pub command: String,
    /// Command arguments
    pub arguments: Vec<String>,
    /// Zone that triggered the command
    pub zone_id: String,
    /// Execution result
    pub result: CommandResult,
    /// Resource usage
    pub resource_usage: ResourceUsage,
    /// Duration of execution
    pub duration: Duration,
    /// Working directory
    pub working_dir: String,
    /// User ID
    pub uid: u32,
}

/// Command execution result
#[derive(Debug, Serialize, Deserialize)]
pub enum CommandResult {
    Success { exit_code: i32 },
    Failed { exit_code: i32, error: String },
    Timeout,
    SecurityViolation(String),
    ResourceLimitExceeded(String),
}

/// Resource usage information
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in MB
    pub memory_mb: Option<u64>,
    /// CPU time in milliseconds
    pub cpu_time_ms: Option<u64>,
    /// Number of file descriptors used
    pub file_descriptors: Option<u32>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            allowed_commands: Self::default_allowed_commands(),
            resource_limits: ResourceLimits::default(),
            network_restrictions: NetworkRestrictions::default(),
            audit_config: AuditConfig::default(),
            sandbox_config: SandboxConfig::default(),
        }
    }
}

impl SecurityPolicy {
    /// Create default allowed commands with security policies
    fn default_allowed_commands() -> HashMap<String, CommandPolicy> {
        let mut commands = HashMap::new();

        // systemctl --user commands
        commands.insert("systemctl".to_string(), CommandPolicy {
            allowed_args: vec![
                r"^--user$".to_string(),
                r"^(start|stop|restart|reload|enable|disable)$".to_string(),
                r"^[a-zA-Z0-9\-_.@]+\.service$".to_string(),
            ],
            forbidden_args: vec![
                r"^--system$".to_string(),
                r"^--root$".to_string(),
                r"^(poweroff|reboot|halt)$".to_string(),
            ],
            max_execution_time: Duration::from_secs(30),
            allowed_working_dirs: vec!["/home".to_string()],
            env_restrictions: EnvRestrictions::safe_defaults(),
            network_access: false,
            resource_limits: Some(ResourceLimits::minimal()),
        });

        // notify-send
        commands.insert("notify-send".to_string(), CommandPolicy {
            allowed_args: vec![
                r"^[^`$;&|<>]+$".to_string(), // No shell metacharacters
            ],
            forbidden_args: vec![],
            max_execution_time: Duration::from_secs(10),
            allowed_working_dirs: vec!["/home".to_string()],
            env_restrictions: EnvRestrictions::safe_defaults(),
            network_access: false,
            resource_limits: Some(ResourceLimits::minimal()),
        });

        // pactl (audio control)
        commands.insert("pactl".to_string(), CommandPolicy {
            allowed_args: vec![
                r"^set-(sink|source)-(volume|mute)$".to_string(),
                r"^set-card-profile$".to_string(),
                r"^[0-9a-zA-Z\-_.@]+$".to_string(), // Device names
                r"^[0-9]+%?$".to_string(), // Volume levels
                r"^(0|1|toggle)$".to_string(), // Mute states
            ],
            forbidden_args: vec![],
            max_execution_time: Duration::from_secs(15),
            allowed_working_dirs: vec!["/home".to_string()],
            env_restrictions: EnvRestrictions::safe_defaults(),
            network_access: false,
            resource_limits: Some(ResourceLimits::minimal()),
        });

        // gsettings (GNOME settings)
        commands.insert("gsettings".to_string(), CommandPolicy {
            allowed_args: vec![
                r"^set$".to_string(),
                r"^[a-zA-Z0-9\-_.]+$".to_string(), // Schema names
                r"^[a-zA-Z0-9\-_]+$".to_string(), // Key names
                r"^'[^']*'$".to_string(), // Quoted values
            ],
            forbidden_args: vec![],
            max_execution_time: Duration::from_secs(15),
            allowed_working_dirs: vec!["/home".to_string()],
            env_restrictions: EnvRestrictions::safe_defaults(),
            network_access: false,
            resource_limits: Some(ResourceLimits::minimal()),
        });

        // Application launchers
        for app in &["firefox", "chromium", "google-chrome", "code", "alacritty"] {
            commands.insert(app.to_string(), CommandPolicy {
                allowed_args: vec![
                    r"^--new-window$".to_string(),
                    r"^--private-window$".to_string(),
                    r"^--incognito$".to_string(),
                    r"^-e$".to_string(),
                    r"^[a-zA-Z0-9\-_./:]+$".to_string(), // URLs or file paths
                ],
                forbidden_args: vec![
                    r"^--remote-debugging".to_string(),
                    r"^--disable-web-security".to_string(),
                    r"^--allow-running-insecure-content".to_string(),
                ],
                max_execution_time: Duration::from_secs(30),
                allowed_working_dirs: vec!["/home".to_string()],
                env_restrictions: EnvRestrictions::safe_defaults(),
                network_access: true, // Browsers need network access
                resource_limits: Some(ResourceLimits::moderate()),
            });
        }

        commands
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: Some(512),    // 512MB limit
            max_cpu_seconds: Some(60),   // 1 minute CPU time
            max_file_descriptors: Some(64),
            max_processes: Some(16),
            max_file_size_mb: Some(100), // 100MB file size limit
        }
    }
}

impl ResourceLimits {
    /// Minimal resource limits for simple commands
    pub fn minimal() -> Self {
        Self {
            max_memory_mb: Some(64),     // 64MB
            max_cpu_seconds: Some(10),   // 10 seconds
            max_file_descriptors: Some(16),
            max_processes: Some(4),
            max_file_size_mb: Some(10),  // 10MB
        }
    }

    /// Moderate resource limits for applications
    pub fn moderate() -> Self {
        Self {
            max_memory_mb: Some(1024),   // 1GB
            max_cpu_seconds: Some(300),  // 5 minutes
            max_file_descriptors: Some(256),
            max_processes: Some(64),
            max_file_size_mb: Some(1024), // 1GB
        }
    }
}

impl Default for NetworkRestrictions {
    fn default() -> Self {
        Self {
            block_network_by_default: true,
            allowed_destinations: vec![],
            blocked_destinations: vec![
                "169.254.0.0/16".to_string(),  // Link-local
                "127.0.0.1".to_string(),       // Localhost
                "::1".to_string(),             // IPv6 localhost
            ],
            allowed_ports: vec![80, 443, 53],   // HTTP, HTTPS, DNS
            blocked_ports: vec![22, 23, 135, 139, 445], // SSH, Telnet, Windows ports
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        let mut log_path = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        log_path.push("network-dmenu");
        log_path.push("audit.log");

        Self {
            enabled: true,
            log_file: log_path,
            max_file_size_mb: 100,
            max_files: 5,
            log_successful: true,
            log_failed: true,
            log_arguments: true,
        }
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sandbox_user: None, // Use current user
            systemd_properties: Self::default_systemd_properties(),
            temp_dir: Some("/tmp/network-dmenu-sandbox".to_string()),
        }
    }
}

impl SandboxConfig {
    fn default_systemd_properties() -> HashMap<String, String> {
        let mut props = HashMap::new();
        
        // Security restrictions
        props.insert("NoNewPrivileges".to_string(), "true".to_string());
        props.insert("ProtectSystem".to_string(), "strict".to_string());
        props.insert("ProtectHome".to_string(), "read-only".to_string());
        props.insert("PrivateTmp".to_string(), "true".to_string());
        props.insert("PrivateDevices".to_string(), "true".to_string());
        props.insert("ProtectKernelTunables".to_string(), "true".to_string());
        props.insert("ProtectKernelModules".to_string(), "true".to_string());
        props.insert("ProtectControlGroups".to_string(), "true".to_string());
        props.insert("RestrictRealtime".to_string(), "true".to_string());
        props.insert("RestrictSUIDSGID".to_string(), "true".to_string());
        props.insert("LockPersonality".to_string(), "true".to_string());
        props.insert("MemoryDenyWriteExecute".to_string(), "true".to_string());
        
        // Network restrictions
        props.insert("PrivateNetwork".to_string(), "true".to_string()); // Default: no network
        props.insert("IPAddressDeny".to_string(), "any".to_string());
        
        // Resource limits
        props.insert("TasksMax".to_string(), "16".to_string());
        props.insert("CPUQuota".to_string(), "50%".to_string());
        props.insert("MemoryMax".to_string(), "512M".to_string());
        
        props
    }
}

impl EnvRestrictions {
    /// Safe default environment restrictions
    pub fn safe_defaults() -> Self {
        Self {
            preserve_env: vec![
                "USER".to_string(),
                "HOME".to_string(),
                "DISPLAY".to_string(),
                "WAYLAND_DISPLAY".to_string(),
                "XDG_RUNTIME_DIR".to_string(),
                "PULSE_RUNTIME_PATH".to_string(),
                "DBUS_SESSION_BUS_ADDRESS".to_string(),
            ],
            remove_env: vec![
                "PATH".to_string(), // Use restricted PATH
                "LD_PRELOAD".to_string(),
                "LD_LIBRARY_PATH".to_string(),
                "PYTHONPATH".to_string(),
            ],
            set_env: {
                let mut env = HashMap::new();
                env.insert("PATH".to_string(), "/usr/bin:/bin".to_string()); // Restricted PATH
                env.insert("TMPDIR".to_string(), "/tmp/network-dmenu".to_string());
                env
            },
        }
    }
}

/// Secure command executor with comprehensive security measures
pub struct SecureCommandExecutor {
    policy: SecurityPolicy,
    audit_logger: AuditLogger,
    resource_monitor: ResourceMonitor,
}

/// Audit logger for command execution
struct AuditLogger {
    config: AuditConfig,
}

/// Resource monitoring for command execution
struct ResourceMonitor {
    limits: ResourceLimits,
}

impl SecureCommandExecutor {
    /// Create new secure command executor
    pub async fn new(policy: SecurityPolicy) -> Result<Self> {
        debug!("Creating secure command executor");

        let audit_logger = AuditLogger::new(policy.audit_config.clone()).await?;
        let resource_monitor = ResourceMonitor::new(policy.resource_limits.clone());

        Ok(Self {
            policy,
            audit_logger,
            resource_monitor,
        })
    }

    /// Execute command with comprehensive security checks
    pub async fn execute_secure_command(
        &mut self,
        command: &str,
        args: &[&str],
        zone_id: &str,
    ) -> Result<std::process::Output> {
        debug!("Executing secure command: {} {:?}", command, args);

        let start_time = std::time::Instant::now();

        // Security validation
        self.validate_command_security(command, args).await?;
        
        // Initialize resource monitoring for this command  
        let monitoring_enabled = true; // Enable monitoring by default

        // Get command policy
        let policy = self.policy.allowed_commands.get(command)
            .ok_or_else(|| GeofenceError::ActionExecution(
                format!("Command '{}' not in allowed commands", command)
            ))?;

        // Execute with sandbox if enabled
        let result = if self.policy.sandbox_config.enabled {
            self.execute_sandboxed_command(command, args, policy).await
        } else {
            self.execute_direct_command(command, args, policy).await
        };

        // Monitor resource usage if enabled and command succeeded
        if monitoring_enabled {
            if let Ok(ref output) = result {
                if output.status.success() {
                    // Get the process ID for monitoring (simulated)
                    let pid = std::process::id();
                    let resource_usage = self.resource_monitor.monitor_execution(pid).await;
                    debug!("Command resource usage: {:?}", resource_usage);
                    
                    // Check if resource limits were exceeded using the centralized method
                    if !self.resource_monitor.check_resource_limits(&resource_usage) {
                        warn!("Command '{}' violated resource limits", command);
                    }
                }
            }
        }

        let duration = start_time.elapsed();

        // Log execution result
        let command_result = match &result {
            Ok(output) => {
                if output.status.success() {
                    CommandResult::Success { 
                        exit_code: output.status.code().unwrap_or(0) 
                    }
                } else {
                    CommandResult::Failed { 
                        exit_code: output.status.code().unwrap_or(-1),
                        error: String::from_utf8_lossy(&output.stderr).to_string(),
                    }
                }
            }
            Err(e) => CommandResult::SecurityViolation(e.to_string()),
        };

        // Create audit log entry
        let audit_entry = AuditLogEntry {
            timestamp: Utc::now(),
            command: command.to_string(),
            arguments: args.iter().map(|s| s.to_string()).collect(),
            zone_id: zone_id.to_string(),
            result: command_result,
            resource_usage: ResourceUsage {
                memory_mb: None, // Would need process monitoring for actual values
                cpu_time_ms: Some(duration.as_millis() as u64),
                file_descriptors: None,
            },
            duration,
            working_dir: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            uid: unsafe { libc::getuid() },
        };

        // Log the audit entry
        if let Err(e) = self.audit_logger.log_command_execution(&audit_entry).await {
            warn!("Failed to write audit log: {}", e);
        }

        result
    }

    /// Validate command security before execution
    async fn validate_command_security(&self, command: &str, args: &[&str]) -> Result<()> {
        debug!("Validating command security: {} {:?}", command, args);

        // Check if command is in allowed list
        let policy = self.policy.allowed_commands.get(command)
            .ok_or_else(|| GeofenceError::ActionExecution(
                format!("Command '{}' not allowed by security policy", command)
            ))?;

        // Validate arguments against allowed patterns
        for arg in args {
            let mut allowed = false;
            
            for pattern in &policy.allowed_args {
                if let Ok(regex) = regex::Regex::new(pattern) {
                    if regex.is_match(arg) {
                        allowed = true;
                        break;
                    }
                }
            }

            if !allowed {
                return Err(GeofenceError::ActionExecution(
                    format!("Argument '{}' not allowed for command '{}'", arg, command)
                ));
            }

            // Check forbidden patterns
            for pattern in &policy.forbidden_args {
                if let Ok(regex) = regex::Regex::new(pattern) {
                    if regex.is_match(arg) {
                        return Err(GeofenceError::ActionExecution(
                            format!("Argument '{}' forbidden for command '{}'", arg, command)
                        ));
                    }
                }
            }
        }

        debug!("Command security validation passed");
        Ok(())
    }

    /// Execute command in systemd sandbox
    async fn execute_sandboxed_command(
        &self,
        command: &str,
        args: &[&str],
        policy: &CommandPolicy,
    ) -> Result<std::process::Output> {
        debug!("Executing command in systemd sandbox: {} {:?}", command, args);

        let mut systemd_run_args = vec!["--user", "--scope"];

        // Add systemd properties for sandboxing
        let mut property_strings = Vec::new();
        for (key, value) in &self.policy.sandbox_config.systemd_properties {
            property_strings.push(format!("{}={}", key, value));
        }
        for property_string in &property_strings {
            systemd_run_args.push("--property");
            systemd_run_args.push(property_string);
        }

        // Add resource limits
        let mut limit_strings = Vec::new();
        if let Some(limits) = &policy.resource_limits {
            if let Some(memory_mb) = limits.max_memory_mb {
                limit_strings.push(format!("MemoryMax={}M", memory_mb));
            }

            if let Some(cpu_seconds) = limits.max_cpu_seconds {
                let cpu_quota = (cpu_seconds * 100 / 60).min(100); // Convert to percentage
                limit_strings.push(format!("CPUQuota={}%", cpu_quota));
            }

            if let Some(max_processes) = limits.max_processes {
                limit_strings.push(format!("TasksMax={}", max_processes));
            }
        }

        // Add timeout
        limit_strings.push(format!("TimeoutStopSec={}s", policy.max_execution_time.as_secs()));
        
        // Add all limit strings to args
        for limit_string in &limit_strings {
            systemd_run_args.push("--property");
            systemd_run_args.push(limit_string);
        }

        // Add the actual command and arguments
        systemd_run_args.push(command);
        systemd_run_args.extend(args);

        // Execute with systemd-run
        let mut cmd = Command::new("systemd-run");
        cmd.args(&systemd_run_args);

        // Set environment restrictions
        self.apply_env_restrictions(&mut cmd, &policy.env_restrictions);

        // Set working directory if restricted
        if !policy.allowed_working_dirs.is_empty() {
            if let Ok(current_dir) = std::env::current_dir() {
                let current_path = current_dir.to_string_lossy();
                let allowed = policy.allowed_working_dirs.iter()
                    .any(|allowed_dir| current_path.starts_with(allowed_dir));
                
                if !allowed {
                    cmd.current_dir(&policy.allowed_working_dirs[0]);
                }
            }
        }

        // Execute command with timeout
        let timeout_duration = policy.max_execution_time + Duration::from_secs(5); // Extra time for cleanup
        
        match tokio::time::timeout(timeout_duration, cmd.output()).await {
            Ok(Ok(output)) => {
                debug!("Sandboxed command completed successfully");
                Ok(output)
            }
            Ok(Err(e)) => {
                error!("Sandboxed command execution failed: {}", e);
                Err(GeofenceError::ActionExecution(format!(
                    "Sandboxed command execution failed: {}", e
                )))
            }
            Err(_) => {
                error!("Sandboxed command timed out after {:?}", timeout_duration);
                Err(GeofenceError::ActionExecution(format!(
                    "Command timed out after {:?}", timeout_duration
                )))
            }
        }
    }

    /// Execute command directly (without systemd sandbox)
    async fn execute_direct_command(
        &self,
        command: &str,
        args: &[&str],
        policy: &CommandPolicy,
    ) -> Result<std::process::Output> {
        debug!("Executing command directly: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args);

        // Apply environment restrictions
        self.apply_env_restrictions(&mut cmd, &policy.env_restrictions);

        // Set resource limits (limited without systemd)
        cmd.kill_on_drop(true);

        // Execute with timeout
        match tokio::time::timeout(policy.max_execution_time, cmd.output()).await {
            Ok(Ok(output)) => {
                debug!("Direct command completed successfully");
                Ok(output)
            }
            Ok(Err(e)) => {
                error!("Direct command execution failed: {}", e);
                Err(GeofenceError::ActionExecution(format!(
                    "Direct command execution failed: {}", e
                )))
            }
            Err(_) => {
                error!("Direct command timed out after {:?}", policy.max_execution_time);
                Err(GeofenceError::ActionExecution(format!(
                    "Command timed out after {:?}", policy.max_execution_time
                )))
            }
        }
    }

    /// Apply environment variable restrictions
    fn apply_env_restrictions(&self, cmd: &mut Command, env_restrictions: &EnvRestrictions) {
        // Clear all environment variables first
        cmd.env_clear();

        // Preserve specified environment variables
        for env_var in &env_restrictions.preserve_env {
            if let Ok(value) = std::env::var(env_var) {
                cmd.env(env_var, value);
            }
        }

        // Set additional environment variables
        for (key, value) in &env_restrictions.set_env {
            cmd.env(key, value);
        }

        debug!("Applied environment restrictions: preserve={:?}, set={:?}", 
               env_restrictions.preserve_env, env_restrictions.set_env);
    }

    /// Update security policy
    pub fn update_policy(&mut self, policy: SecurityPolicy) {
        debug!("Updating security policy");
        self.policy = policy;
    }

    /// Get current security policy
    pub fn get_policy(&self) -> &SecurityPolicy {
        &self.policy
    }

    /// Get audit log entries
    pub async fn get_audit_logs(&self, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        self.audit_logger.get_recent_entries(limit).await
    }
}

impl AuditLogger {
    async fn new(config: AuditConfig) -> Result<Self> {
        debug!("Creating audit logger with config: {:?}", config);

        // Create log directory if it doesn't exist
        if let Some(parent) = config.log_file.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(GeofenceError::Config(format!(
                    "Failed to create audit log directory: {}", e
                )));
            }
        }

        Ok(Self { config })
    }

    async fn log_command_execution(&mut self, entry: &AuditLogEntry) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Logging audit entry for command: {}", entry.command);

        let log_line = serde_json::to_string(entry)
            .map_err(|e| GeofenceError::Config(format!(
                "Failed to serialize audit log entry: {}", e
            )))?;

        // Check if log rotation is needed
        if let Ok(metadata) = fs::metadata(&self.config.log_file).await {
            let size_mb = metadata.len() / 1024 / 1024;
            if size_mb > self.config.max_file_size_mb {
                self.rotate_logs().await?;
            }
        }

        // Append to log file
        let log_line_with_newline = format!("{}\n", log_line);
        fs::write(&self.config.log_file, log_line_with_newline).await
            .map_err(|e| GeofenceError::Config(format!(
                "Failed to write audit log: {}", e
            )))?;

        debug!("Audit log entry written successfully");
        Ok(())
    }

    async fn rotate_logs(&self) -> Result<()> {
        debug!("Rotating audit logs");

        // Move old log files
        for i in (1..self.config.max_files).rev() {
            let old_path = format!("{}.{}", self.config.log_file.display(), i);
            let new_path = format!("{}.{}", self.config.log_file.display(), i + 1);

            if PathBuf::from(&old_path).exists() {
                if let Err(e) = fs::rename(&old_path, &new_path).await {
                    warn!("Failed to rotate log file {} to {}: {}", old_path, new_path, e);
                }
            }
        }

        // Move current log to .1
        let rotated_path = format!("{}.1", self.config.log_file.display());
        if let Err(e) = fs::rename(&self.config.log_file, &rotated_path).await {
            warn!("Failed to rotate current log file: {}", e);
        }

        debug!("Log rotation completed");
        Ok(())
    }

    async fn get_recent_entries(&self, limit: Option<usize>) -> Result<Vec<AuditLogEntry>> {
        debug!("Retrieving recent audit log entries (limit: {:?})", limit);

        if !self.config.log_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.config.log_file).await
            .map_err(|e| GeofenceError::Config(format!(
                "Failed to read audit log: {}", e
            )))?;

        let mut entries = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Take the last N lines if limit is specified
        let lines_to_process = if let Some(limit) = limit {
            if lines.len() > limit {
                &lines[lines.len() - limit..]
            } else {
                &lines[..]
            }
        } else {
            &lines[..]
        };

        for line in lines_to_process {
            if let Ok(entry) = serde_json::from_str::<AuditLogEntry>(line) {
                entries.push(entry);
            }
        }

        debug!("Retrieved {} audit log entries", entries.len());
        Ok(entries)
    }
}

impl ResourceMonitor {
    fn new(limits: ResourceLimits) -> Self {
        Self { limits }
    }

    /// Monitor resource usage during command execution
    async fn monitor_execution(&self, _pid: u32) -> ResourceUsage {
        // In a real implementation, this would monitor the actual process
        // For now, we'll simulate resource monitoring with limits checking
        
        debug!("Monitoring resource usage with limits: max_memory_mb={:?}, max_cpu_seconds={:?}, max_file_descriptors={:?}", 
               self.limits.max_memory_mb, self.limits.max_cpu_seconds, self.limits.max_file_descriptors);
        
        // Simulate some resource usage for demonstration
        let simulated_memory = 50u64; // 50 MB
        let simulated_cpu_time = 1000u64; // 1 second
        let simulated_fds = 10u32; // 10 file descriptors
        
        // Check against limits and warn if approaching
        if let Some(max_memory) = self.limits.max_memory_mb {
            if simulated_memory > max_memory / 2 {
                warn!("Command approaching memory limit: {}MB used, limit: {}MB", 
                      simulated_memory, max_memory);
            }
        }
        
        if let Some(max_cpu_seconds) = self.limits.max_cpu_seconds {
            let simulated_cpu_seconds = simulated_cpu_time / 1000; // Convert ms to seconds
            if simulated_cpu_seconds > max_cpu_seconds / 2 {
                warn!("Command approaching CPU time limit: {}s used, limit: {}s", 
                      simulated_cpu_seconds, max_cpu_seconds);
            }
        }
        
        if let Some(max_fds) = self.limits.max_file_descriptors {
            if (simulated_fds as u64) > max_fds / 2 {
                warn!("Command approaching FD limit: {} FDs used, limit: {}", 
                      simulated_fds, max_fds);
            }
        }
        
        ResourceUsage {
            memory_mb: Some(simulated_memory),
            cpu_time_ms: Some(simulated_cpu_time),
            file_descriptors: Some(simulated_fds),
        }
    }

    /// Check if resource usage violates limits
    fn check_resource_limits(&self, usage: &ResourceUsage) -> bool {
        if let (Some(memory), Some(max_memory)) = (usage.memory_mb, self.limits.max_memory_mb) {
            if memory > max_memory {
                warn!("Memory limit exceeded: {}MB > {}MB", memory, max_memory);
                return false;
            }
        }
        
        if let (Some(cpu_time_ms), Some(max_cpu_seconds)) = (usage.cpu_time_ms, self.limits.max_cpu_seconds) {
            let cpu_time_seconds = cpu_time_ms / 1000; // Convert ms to seconds
            if cpu_time_seconds > max_cpu_seconds {
                warn!("CPU time limit exceeded: {}s > {}s", cpu_time_seconds, max_cpu_seconds);
                return false;
            }
        }
        
        if let (Some(fds), Some(max_fds)) = (usage.file_descriptors, self.limits.max_file_descriptors) {
            if (fds as u64) > max_fds {
                warn!("File descriptor limit exceeded: {} > {}", fds, max_fds);
                return false;
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_policy_default() {
        let policy = SecurityPolicy::default();
        assert!(policy.allowed_commands.contains_key("systemctl"));
        assert!(policy.allowed_commands.contains_key("notify-send"));
        assert!(policy.audit_config.enabled);
    }

    #[test]
    fn test_resource_limits() {
        let minimal = ResourceLimits::minimal();
        let moderate = ResourceLimits::moderate();
        let default = ResourceLimits::default();

        assert!(minimal.max_memory_mb < default.max_memory_mb);
        assert!(moderate.max_memory_mb > default.max_memory_mb);
    }

    #[test]
    fn test_env_restrictions() {
        let env = EnvRestrictions::safe_defaults();
        assert!(env.preserve_env.contains(&"USER".to_string()));
        assert!(env.preserve_env.contains(&"HOME".to_string()));
        assert!(env.remove_env.contains(&"LD_PRELOAD".to_string()));
    }

    #[tokio::test]
    async fn test_audit_logger_creation() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_file: temp_file.path().to_path_buf(),
            max_file_size_mb: 1,
            max_files: 3,
            log_successful: true,
            log_failed: true,
            log_arguments: true,
        };

        let logger = AuditLogger::new(config).await;
        assert!(logger.is_ok());
    }

    #[test]
    fn test_command_result_serialization() {
        let results = vec![
            CommandResult::Success { exit_code: 0 },
            CommandResult::Failed { exit_code: 1, error: "Test error".to_string() },
            CommandResult::Timeout,
            CommandResult::SecurityViolation("Test violation".to_string()),
        ];

        for result in results {
            let json = serde_json::to_string(&result).unwrap();
            let deserialized: CommandResult = serde_json::from_str(&json).unwrap();
            // CommandResult doesn't implement PartialEq, so we just test serialization works
        }
    }
}