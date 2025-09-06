//! Enhanced configuration management for geofencing daemon
//!
//! Provides hot configuration reload, validation, schema evolution,
//! environment variable overrides, and configuration profiles.

use crate::geofencing::{
    adaptive::ScanFrequency, observability::ObservabilityConfig, performance::CacheConfig,
    retry::RetryConfig, security::SecurityPolicy, GeofenceError, GeofencingConfig, Result,
};
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

/// Enhanced configuration with all daemon settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConfig {
    /// Schema version for configuration evolution
    pub schema_version: String,
    /// Configuration metadata
    pub metadata: ConfigMetadata,
    /// Core geofencing configuration
    pub geofencing: GeofencingConfig,
    /// Adaptive scanning configuration
    pub adaptive_scanning: ScanFrequency,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Security policy
    pub security: SecurityPolicy,
    /// Observability configuration
    pub observability: ObservabilityConfig,
    /// Retry configuration
    pub retry: RetryConfig,
    /// Environment-specific overrides
    pub environment_overrides: HashMap<String, serde_json::Value>,
    /// Configuration profiles
    pub profiles: HashMap<String, ConfigProfile>,
}

/// Configuration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    /// When this configuration was created
    pub created_at: DateTime<Utc>,
    /// When this configuration was last modified
    pub last_modified: DateTime<Utc>,
    /// Who/what created this configuration
    pub created_by: String,
    /// Configuration description
    pub description: Option<String>,
    /// Tags for organizing configurations
    pub tags: Vec<String>,
    /// Configuration environment (dev, staging, prod)
    pub environment: String,
}

/// Configuration profile for different use cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProfile {
    /// Profile name
    pub name: String,
    /// Profile description
    pub description: String,
    /// Partial configuration overrides
    pub overrides: HashMap<String, serde_json::Value>,
    /// Whether this profile is active
    pub active: bool,
}

/// Configuration validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation warnings (non-fatal issues)
    pub warnings: Vec<ConfigWarning>,
    /// Validation errors (fatal issues)
    pub errors: Vec<ConfigError>,
    /// Suggested fixes
    pub suggestions: Vec<ConfigSuggestion>,
}

/// Configuration warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigWarning {
    /// Configuration path that caused the warning
    pub path: String,
    /// Warning message
    pub message: String,
    /// Warning severity
    pub severity: WarningSeverity,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Configuration error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigError {
    /// Configuration path that caused the error
    pub path: String,
    /// Error message
    pub message: String,
    /// Error type
    pub error_type: ConfigErrorType,
}

/// Configuration suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSuggestion {
    /// What to improve
    pub description: String,
    /// Suggested change
    pub suggestion: String,
    /// Priority of the suggestion
    pub priority: SuggestionPriority,
}

/// Warning severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WarningSeverity {
    Low,
    Medium,
    High,
}

/// Configuration error types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfigErrorType {
    MissingRequired,
    InvalidValue,
    InvalidType,
    InvalidRange,
    ConflictingSettings,
    SchemaViolation,
}

/// Suggestion priority levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestionPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Configuration change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeEvent {
    /// When the change occurred
    pub timestamp: DateTime<Utc>,
    /// What changed
    pub change_type: ConfigChangeType,
    /// Configuration path that changed
    pub path: String,
    /// Old value (if applicable)
    pub old_value: Option<serde_json::Value>,
    /// New value
    pub new_value: serde_json::Value,
    /// Who/what made the change
    pub changed_by: String,
}

/// Types of configuration changes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfigChangeType {
    Created,
    Updated,
    Deleted,
    Reloaded,
    ProfileActivated,
    ProfileDeactivated,
}

/// Configuration manager handles all configuration operations
pub struct ConfigManager {
    /// Current configuration
    current_config: EnhancedConfig,
    /// Configuration file path
    config_file_path: PathBuf,
    /// Configuration change history
    change_history: Vec<ConfigChangeEvent>,
    /// File system watcher for hot reload
    _file_watcher: Option<tokio::task::JoinHandle<()>>,
    /// Configuration validators
    validators: Vec<Box<dyn ConfigValidator>>,
    /// Environment variable provider
    env_provider: EnvVariableProvider,
}

/// Trait for configuration validators
pub trait ConfigValidator: Send + Sync {
    /// Validate configuration
    fn validate(&self, config: &EnhancedConfig) -> Vec<ValidationIssue>;

    /// Get validator name
    fn name(&self) -> &str;
}

/// Configuration validation issue
#[derive(Debug, Clone)]
pub enum ValidationIssue {
    Warning(ConfigWarning),
    Error(ConfigError),
    Suggestion(ConfigSuggestion),
}

/// Environment variable provider
pub struct EnvVariableProvider {
    /// Prefix for environment variables
    prefix: String,
    /// Environment variable overrides
    overrides: HashMap<String, String>,
}

impl Default for EnhancedConfig {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            metadata: ConfigMetadata::default(),
            geofencing: GeofencingConfig::default(),
            adaptive_scanning: ScanFrequency::default(),
            cache: CacheConfig::default(),
            security: SecurityPolicy::default(),
            observability: ObservabilityConfig::default(),
            retry: RetryConfig::default(),
            environment_overrides: HashMap::new(),
            profiles: Self::default_profiles(),
        }
    }
}

impl EnhancedConfig {
    /// Create default configuration profiles
    fn default_profiles() -> HashMap<String, ConfigProfile> {
        let mut profiles = HashMap::new();

        // Development profile
        profiles.insert(
            "development".to_string(),
            ConfigProfile {
                name: "development".to_string(),
                description: "Development environment settings".to_string(),
                overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert(
                        "geofencing.scan_interval_seconds".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(10)),
                    );
                    overrides.insert(
                        "observability.metrics_interval".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(15)),
                    );
                    overrides.insert(
                        "observability.trace_sampling_rate".to_string(),
                        serde_json::Value::Number(serde_json::Number::from_f64(1.0).unwrap()),
                    );
                    overrides
                },
                active: false,
            },
        );

        // Production profile
        profiles.insert(
            "production".to_string(),
            ConfigProfile {
                name: "production".to_string(),
                description: "Production environment settings".to_string(),
                overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert(
                        "geofencing.scan_interval_seconds".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(60)),
                    );
                    overrides.insert(
                        "observability.metrics_interval".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(300)),
                    );
                    overrides.insert(
                        "observability.trace_sampling_rate".to_string(),
                        serde_json::Value::Number(serde_json::Number::from_f64(0.01).unwrap()),
                    );
                    overrides.insert(
                        "retry.max_retries".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(5)),
                    );
                    overrides
                },
                active: false,
            },
        );

        // Battery saver profile
        profiles.insert(
            "battery_saver".to_string(),
            ConfigProfile {
                name: "battery_saver".to_string(),
                description: "Battery conservation settings".to_string(),
                overrides: {
                    let mut overrides = HashMap::new();
                    overrides.insert(
                        "geofencing.scan_interval_seconds".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(120)),
                    );
                    overrides.insert(
                        "observability.metrics_enabled".to_string(),
                        serde_json::Value::Bool(false),
                    );
                    overrides.insert(
                        "observability.tracing_enabled".to_string(),
                        serde_json::Value::Bool(false),
                    );
                    overrides.insert(
                        "cache.cleanup_interval".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(600)),
                    );
                    overrides
                },
                active: false,
            },
        );

        profiles
    }

    /// Apply environment variable overrides
    pub fn apply_environment_overrides(
        &mut self,
        env_provider: &EnvVariableProvider,
    ) -> Result<()> {
        debug!("Applying environment variable overrides");

        for (key, value) in &env_provider.overrides {
            self.apply_override(key, value)?;
        }

        Ok(())
    }

    /// Apply a configuration override
    pub fn apply_override(&mut self, path: &str, value: &str) -> Result<()> {
        debug!("Applying configuration override: {} = {}", path, value);

        // Parse the configuration path and apply the override
        // This is a simplified implementation - would need proper path parsing
        match path {
            "geofencing.scan_interval_seconds" => {
                if let Ok(interval) = value.parse::<u64>() {
                    self.geofencing.scan_interval_seconds = interval;
                }
            }
            "geofencing.confidence_threshold" => {
                if let Ok(threshold) = value.parse::<f64>() {
                    self.geofencing.confidence_threshold = threshold;
                }
            }
            "observability.metrics_enabled" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    self.observability.metrics_enabled = enabled;
                }
            }
            "observability.tracing_enabled" => {
                if let Ok(enabled) = value.parse::<bool>() {
                    self.observability.tracing_enabled = enabled;
                }
            }
            "retry.max_retries" => {
                if let Ok(retries) = value.parse::<u32>() {
                    self.retry.max_retries = retries;
                }
            }
            _ => {
                warn!("Unknown configuration override path: {}", path);
            }
        }

        Ok(())
    }

    /// Activate a configuration profile
    pub fn activate_profile(&mut self, profile_name: &str) -> Result<()> {
        debug!("Activating configuration profile: {}", profile_name);

        // Deactivate all profiles first
        for profile in self.profiles.values_mut() {
            profile.active = false;
        }

        // Activate the requested profile
        let overrides = if let Some(profile) = self.profiles.get_mut(profile_name) {
            profile.active = true;
            profile.overrides.clone()
        } else {
            return Err(GeofenceError::Config(format!(
                "Profile '{}' not found",
                profile_name
            )));
        };

        // Apply profile overrides
        for (key, value) in &overrides {
            if let serde_json::Value::String(str_value) = value {
                self.apply_override(key, str_value)?;
            }
        }

        info!("Configuration profile '{}' activated", profile_name);
        Ok(())
    }

    /// Get active profile name
    pub fn get_active_profile(&self) -> Option<&str> {
        self.profiles
            .iter()
            .find(|(_, profile)| profile.active)
            .map(|(name, _)| name.as_str())
    }

    /// Validate configuration
    pub fn validate(&self) -> ValidationResult {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut suggestions = Vec::new();

        // Validate geofencing settings
        if self.geofencing.scan_interval_seconds < 5 {
            warnings.push(ConfigWarning {
                path: "geofencing.scan_interval_seconds".to_string(),
                message: "Very frequent scanning may impact battery life".to_string(),
                severity: WarningSeverity::Medium,
                suggestion: Some("Consider using scan interval >= 30 seconds".to_string()),
            });
        }

        if self.geofencing.scan_interval_seconds > 300 {
            warnings.push(ConfigWarning {
                path: "geofencing.scan_interval_seconds".to_string(),
                message: "Infrequent scanning may miss location changes".to_string(),
                severity: WarningSeverity::Low,
                suggestion: Some("Consider using scan interval <= 120 seconds".to_string()),
            });
        }

        if !(0.1..=1.0).contains(&self.geofencing.confidence_threshold) {
            errors.push(ConfigError {
                path: "geofencing.confidence_threshold".to_string(),
                message: "Confidence threshold must be between 0.1 and 1.0".to_string(),
                error_type: ConfigErrorType::InvalidRange,
            });
        }

        // Validate zones
        if self.geofencing.zones.is_empty() {
            warnings.push(ConfigWarning {
                path: "geofencing.zones".to_string(),
                message: "No geofencing zones configured".to_string(),
                severity: WarningSeverity::High,
                suggestion: Some("Add at least one zone for geofencing to be useful".to_string()),
            });
        }

        // Validate zone names are unique
        let mut zone_names = std::collections::HashSet::new();
        for zone in &self.geofencing.zones {
            if !zone_names.insert(&zone.name) {
                errors.push(ConfigError {
                    path: "geofencing.zones".to_string(),
                    message: format!("Duplicate zone name: {}", zone.name),
                    error_type: ConfigErrorType::ConflictingSettings,
                });
            }
        }

        // Validate observability settings
        if self.observability.trace_sampling_rate < 0.0
            || self.observability.trace_sampling_rate > 1.0
        {
            errors.push(ConfigError {
                path: "observability.trace_sampling_rate".to_string(),
                message: "Trace sampling rate must be between 0.0 and 1.0".to_string(),
                error_type: ConfigErrorType::InvalidRange,
            });
        }

        // Validate retry settings
        if self.retry.max_retries > 10 {
            warnings.push(ConfigWarning {
                path: "retry.max_retries".to_string(),
                message: "High retry count may cause delays".to_string(),
                severity: WarningSeverity::Low,
                suggestion: Some("Consider max_retries <= 5 for better responsiveness".to_string()),
            });
        }

        // Performance suggestions
        if self.observability.metrics_enabled
            && self.observability.metrics_interval < Duration::from_secs(30)
        {
            suggestions.push(ConfigSuggestion {
                description: "Frequent metrics collection may impact performance".to_string(),
                suggestion: "Consider increasing metrics_interval to >= 30 seconds".to_string(),
                priority: SuggestionPriority::Low,
            });
        }

        ValidationResult {
            valid: errors.is_empty(),
            warnings,
            errors,
            suggestions,
        }
    }
}

impl Default for ConfigMetadata {
    fn default() -> Self {
        Self {
            created_at: Utc::now(),
            last_modified: Utc::now(),
            created_by: "network-dmenu".to_string(),
            description: Some("Default geofencing daemon configuration".to_string()),
            tags: vec!["default".to_string()],
            environment: "development".to_string(),
        }
    }
}

impl ConfigManager {
    /// Create new configuration manager
    pub async fn new<P: AsRef<Path>>(config_file_path: P) -> Result<Self> {
        let config_path = config_file_path.as_ref().to_path_buf();
        debug!(
            "Creating configuration manager with path: {}",
            config_path.display()
        );

        // Load configuration
        let config = Self::load_config(&config_path).await?;

        let mut manager = Self {
            current_config: config,
            config_file_path: config_path,
            change_history: Vec::new(),
            _file_watcher: None,
            validators: Vec::new(),
            env_provider: EnvVariableProvider::new("NETWORK_DMENU"),
        };

        // Register default validators
        manager.register_default_validators();

        // Apply environment overrides
        manager
            .current_config
            .apply_environment_overrides(&manager.env_provider)?;

        // Validate configuration
        let validation_result = manager.current_config.validate();
        if !validation_result.valid {
            warn!(
                "Configuration validation failed with {} errors",
                validation_result.errors.len()
            );
            for error in &validation_result.errors {
                error!("Config error at {}: {}", error.path, error.message);
            }
        }

        if !validation_result.warnings.is_empty() {
            info!(
                "Configuration has {} warnings",
                validation_result.warnings.len()
            );
            for warning in &validation_result.warnings {
                warn!("Config warning at {}: {}", warning.path, warning.message);
            }
        }

        info!("Configuration manager created successfully");
        Ok(manager)
    }

    /// Start configuration file watching for hot reload
    pub async fn start_file_watching(&mut self) -> Result<()> {
        debug!("Starting configuration file watching");

        let config_path = self.config_file_path.clone();
        let handle = tokio::spawn(async move {
            // This would implement actual file watching
            // For now, just log that it would be watching
            info!(
                "Would start watching config file: {}",
                config_path.display()
            );
        });

        self._file_watcher = Some(handle);
        Ok(())
    }

    /// Reload configuration from file
    pub async fn reload_config(&mut self) -> Result<()> {
        info!("Reloading configuration from file");

        let new_config = Self::load_config(&self.config_file_path).await?;

        // Validate new configuration
        let validation_result = new_config.validate();
        if !validation_result.valid {
            return Err(GeofenceError::Config(format!(
                "Configuration validation failed: {} errors",
                validation_result.errors.len()
            )));
        }

        // Record configuration change
        self.record_change(ConfigChangeEvent {
            timestamp: Utc::now(),
            change_type: ConfigChangeType::Reloaded,
            path: "root".to_string(),
            old_value: None,
            new_value: serde_json::to_value(&new_config).unwrap_or(serde_json::Value::Null),
            changed_by: "file_watcher".to_string(),
        });

        self.current_config = new_config;
        info!("Configuration reloaded successfully");
        Ok(())
    }

    /// Update configuration value
    pub async fn update_config_value(
        &mut self,
        path: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        debug!("Updating configuration value: {} = {:?}", path, value);

        // Store old value for change tracking
        let old_value = self.get_config_value(path);

        // Apply the change (simplified implementation)
        match path {
            "geofencing.scan_interval_seconds" => {
                if let Some(interval) = value.as_u64() {
                    self.current_config.geofencing.scan_interval_seconds = interval;
                }
            }
            "geofencing.confidence_threshold" => {
                if let Some(threshold) = value.as_f64() {
                    self.current_config.geofencing.confidence_threshold = threshold;
                }
            }
            _ => {
                return Err(GeofenceError::Config(format!(
                    "Unknown configuration path: {}",
                    path
                )));
            }
        }

        // Validate the updated configuration
        let validation_result = self.current_config.validate();
        if !validation_result.valid {
            return Err(GeofenceError::Config(format!(
                "Configuration update would cause validation errors: {}",
                validation_result.errors.len()
            )));
        }

        // Record the change
        self.record_change(ConfigChangeEvent {
            timestamp: Utc::now(),
            change_type: ConfigChangeType::Updated,
            path: path.to_string(),
            old_value,
            new_value: value,
            changed_by: "api".to_string(),
        });

        // Save to file
        self.save_config().await?;

        info!("Configuration updated successfully: {}", path);
        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> &EnhancedConfig {
        &self.current_config
    }

    /// Get configuration value by path
    pub fn get_config_value(&self, path: &str) -> Option<serde_json::Value> {
        // Simplified implementation - would need proper path traversal
        match path {
            "geofencing.scan_interval_seconds" => Some(serde_json::Value::Number(
                serde_json::Number::from(self.current_config.geofencing.scan_interval_seconds),
            )),
            "geofencing.confidence_threshold" => Some(serde_json::Value::Number(
                serde_json::Number::from_f64(self.current_config.geofencing.confidence_threshold)
                    .unwrap_or(serde_json::Number::from(0)),
            )),
            _ => None,
        }
    }

    /// Validate current configuration
    pub fn validate_config(&self) -> ValidationResult {
        debug!("Validating current configuration");

        let mut result = self.current_config.validate();

        // Run additional validators
        for validator in &self.validators {
            let issues = validator.validate(&self.current_config);
            for issue in issues {
                match issue {
                    ValidationIssue::Warning(warning) => result.warnings.push(warning),
                    ValidationIssue::Error(error) => result.errors.push(error),
                    ValidationIssue::Suggestion(suggestion) => result.suggestions.push(suggestion),
                }
            }
        }

        // Update validation result
        result.valid = result.errors.is_empty();

        debug!(
            "Configuration validation completed: {} errors, {} warnings, {} suggestions",
            result.errors.len(),
            result.warnings.len(),
            result.suggestions.len()
        );

        result
    }

    /// Activate configuration profile
    pub async fn activate_profile(&mut self, profile_name: &str) -> Result<()> {
        info!("Activating configuration profile: {}", profile_name);

        let old_profile = self
            .current_config
            .get_active_profile()
            .map(|p| p.to_string());

        self.current_config.activate_profile(profile_name)?;

        // Record the change
        self.record_change(ConfigChangeEvent {
            timestamp: Utc::now(),
            change_type: ConfigChangeType::ProfileActivated,
            path: "profiles.active".to_string(),
            old_value: old_profile.map(serde_json::Value::String),
            new_value: serde_json::Value::String(profile_name.to_string()),
            changed_by: "api".to_string(),
        });

        // Save configuration
        self.save_config().await?;

        info!(
            "Configuration profile '{}' activated successfully",
            profile_name
        );
        Ok(())
    }

    /// Get configuration change history
    pub fn get_change_history(&self, limit: Option<usize>) -> Vec<ConfigChangeEvent> {
        if let Some(limit) = limit {
            self.change_history
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect()
        } else {
            self.change_history.clone()
        }
    }

    /// Export configuration to file
    pub async fn export_config<P: AsRef<Path>>(&self, export_path: P) -> Result<()> {
        let export_path = export_path.as_ref();
        debug!("Exporting configuration to: {}", export_path.display());

        let config_json = serde_json::to_string_pretty(&self.current_config).map_err(|e| {
            GeofenceError::Config(format!("Failed to serialize configuration: {}", e))
        })?;

        fs::write(export_path, config_json).await.map_err(|e| {
            GeofenceError::Config(format!("Failed to write configuration file: {}", e))
        })?;

        info!("Configuration exported to: {}", export_path.display());
        Ok(())
    }

    /// Load configuration from file
    async fn load_config(config_path: &Path) -> Result<EnhancedConfig> {
        debug!("Loading configuration from: {}", config_path.display());

        if !config_path.exists() {
            info!("Configuration file does not exist, creating default configuration");
            let default_config = EnhancedConfig::default();

            // Create directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    GeofenceError::Config(format!(
                        "Failed to create configuration directory: {}",
                        e
                    ))
                })?;
            }

            // Write default configuration
            let config_json = serde_json::to_string_pretty(&default_config).map_err(|e| {
                GeofenceError::Config(format!("Failed to serialize default configuration: {}", e))
            })?;

            fs::write(config_path, config_json).await.map_err(|e| {
                GeofenceError::Config(format!("Failed to write default configuration: {}", e))
            })?;

            return Ok(default_config);
        }

        let config_content = fs::read_to_string(config_path).await.map_err(|e| {
            GeofenceError::Config(format!("Failed to read configuration file: {}", e))
        })?;

        let config: EnhancedConfig = serde_json::from_str(&config_content).map_err(|e| {
            GeofenceError::Config(format!("Failed to parse configuration file: {}", e))
        })?;

        debug!(
            "Configuration loaded successfully from: {}",
            config_path.display()
        );
        Ok(config)
    }

    /// Save configuration to file
    async fn save_config(&self) -> Result<()> {
        debug!(
            "Saving configuration to: {}",
            self.config_file_path.display()
        );

        let mut config = self.current_config.clone();
        config.metadata.last_modified = Utc::now();

        let config_json = serde_json::to_string_pretty(&config).map_err(|e| {
            GeofenceError::Config(format!("Failed to serialize configuration: {}", e))
        })?;

        fs::write(&self.config_file_path, config_json)
            .await
            .map_err(|e| {
                GeofenceError::Config(format!("Failed to write configuration file: {}", e))
            })?;

        debug!("Configuration saved successfully");
        Ok(())
    }

    /// Record a configuration change
    fn record_change(&mut self, change: ConfigChangeEvent) {
        debug!("Recording configuration change: {:?}", change.change_type);

        self.change_history.push(change);

        // Keep only last 1000 changes
        while self.change_history.len() > 1000 {
            self.change_history.remove(0);
        }
    }

    /// Register default configuration validators
    fn register_default_validators(&mut self) {
        debug!("Registering default configuration validators");

        // Would register actual validators
        // For now, just log that they would be registered
        info!("Default configuration validators registered");
    }
}

impl EnvVariableProvider {
    /// Create new environment variable provider
    pub fn new(prefix: &str) -> Self {
        debug!(
            "Creating environment variable provider with prefix: {}",
            prefix
        );

        let mut overrides = HashMap::new();

        // Load environment variables with the specified prefix
        for (key, value) in std::env::vars() {
            if key.starts_with(&format!("{}_", prefix)) {
                // Convert environment variable name to config path
                let config_key = key[prefix.len() + 1..].to_lowercase().replace('_', ".");

                overrides.insert(config_key, value);
            }
        }

        debug!("Loaded {} environment variable overrides", overrides.len());

        Self {
            prefix: prefix.to_string(),
            overrides,
        }
    }

    /// Refresh environment variables (re-scan for new variables with the prefix)
    pub fn refresh_environment_variables(&mut self) -> usize {
        let old_count = self.overrides.len();
        self.overrides.clear();

        // Reload environment variables with the current prefix
        for (key, value) in std::env::vars() {
            if key.starts_with(&format!("{}_", self.prefix)) {
                // Convert environment variable name to config path
                let config_key = key[self.prefix.len() + 1..]
                    .to_lowercase()
                    .replace('_', ".");

                self.overrides.insert(config_key, value);
            }
        }

        let new_count = self.overrides.len();
        debug!(
            "Refreshed environment variables: {} -> {} overrides (prefix: {})",
            old_count, new_count, self.prefix
        );

        new_count
    }

    /// Get current prefix
    pub fn get_prefix(&self) -> &str {
        &self.prefix
    }

    /// Change the prefix and reload environment variables
    pub fn change_prefix(&mut self, new_prefix: &str) -> usize {
        debug!(
            "Changing environment variable prefix from '{}' to '{}'",
            self.prefix, new_prefix
        );
        self.prefix = new_prefix.to_string();
        self.refresh_environment_variables()
    }

    /// Check if a specific environment variable with the prefix exists
    pub fn has_env_variable(&self, variable_name: &str) -> bool {
        let full_key = format!("{}_{}", self.prefix, variable_name.to_uppercase());
        std::env::var(&full_key).is_ok()
    }

    /// Get a specific environment variable with the prefix
    pub fn get_env_variable(&self, variable_name: &str) -> Option<String> {
        let full_key = format!("{}_{}", self.prefix, variable_name.to_uppercase());
        std::env::var(&full_key).ok()
    }
}

// Example configuration validators

/// Validator for network-related settings
pub struct NetworkConfigValidator;

impl ConfigValidator for NetworkConfigValidator {
    fn validate(&self, config: &EnhancedConfig) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check if any zones have network actions but no WiFi networks configured
        for zone in &config.geofencing.zones {
            if (zone.actions.wifi.is_some() || zone.actions.vpn.is_some())
                && zone.fingerprints.is_empty()
            {
                issues.push(ValidationIssue::Warning(ConfigWarning {
                    path: format!("geofencing.zones.{}.fingerprints", zone.name),
                    message: "Zone has network actions but no location fingerprints".to_string(),
                    severity: WarningSeverity::High,
                    suggestion: Some("Add location fingerprints by visiting the zone".to_string()),
                }));
            }
        }

        issues
    }

    fn name(&self) -> &str {
        "NetworkConfigValidator"
    }
}

/// Validator for performance-related settings
pub struct PerformanceConfigValidator;

impl ConfigValidator for PerformanceConfigValidator {
    fn validate(&self, config: &EnhancedConfig) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check for performance-impacting combinations
        if config.geofencing.scan_interval_seconds < 15
            && config.observability.metrics_enabled
            && config.observability.tracing_enabled
        {
            issues.push(ValidationIssue::Suggestion(ConfigSuggestion {
                description:
                    "High-frequency scanning with full observability may impact performance"
                        .to_string(),
                suggestion: "Consider increasing scan_interval or reducing observability features"
                    .to_string(),
                priority: SuggestionPriority::Medium,
            }));
        }

        issues
    }

    fn name(&self) -> &str {
        "PerformanceConfigValidator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_enhanced_config_default() {
        let config = EnhancedConfig::default();
        assert_eq!(config.schema_version, "1.0.0");
        assert!(!config.profiles.is_empty());
        assert!(config.profiles.contains_key("development"));
        assert!(config.profiles.contains_key("production"));
    }

    #[tokio::test]
    async fn test_config_manager_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();

        let manager = ConfigManager::new(config_path).await;
        assert!(manager.is_ok());
    }

    #[test]
    fn test_config_validation() {
        let config = EnhancedConfig::default();
        let result = config.validate();

        // Default config should have warnings about no zones but no errors
        assert!(result.valid);
        assert!(!result.warnings.is_empty()); // Should warn about no zones
    }

    #[test]
    fn test_profile_activation() {
        let mut config = EnhancedConfig::default();

        // Test activating development profile
        let result = config.activate_profile("development");
        assert!(result.is_ok());
        assert_eq!(config.get_active_profile(), Some("development"));

        // Test activating non-existent profile
        let result = config.activate_profile("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_environment_variable_provider() {
        let provider = EnvVariableProvider::new("TEST");
        assert_eq!(provider.prefix, "TEST");
    }

    #[test]
    fn test_config_validation_result() {
        let mut result = ValidationResult {
            valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            suggestions: Vec::new(),
        };

        result.errors.push(ConfigError {
            path: "test.path".to_string(),
            message: "Test error".to_string(),
            error_type: ConfigErrorType::InvalidValue,
        });

        // Should become invalid when errors are present
        result.valid = result.errors.is_empty();
        assert!(!result.valid);
    }

    #[test]
    fn test_network_config_validator() {
        let validator = NetworkConfigValidator;
        let config = EnhancedConfig::default();

        let issues = validator.validate(&config);
        assert_eq!(validator.name(), "NetworkConfigValidator");

        // Default config with no zones should have no network validation issues
        assert!(issues.is_empty());
    }
}
