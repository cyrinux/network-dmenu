use crate::constants::{CONFIG_DIR_NAME, CONFIG_FILE_NAME, DEFAULT_DMENU_CMD};
use crate::errors::{NetworkMenuError, Result};
use crate::types::{Config, CustomAction};
use dirs::config_dir;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Configuration manager responsible for loading and saving configuration
pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new() -> Result<Self> {
        let config_path = Self::get_config_path()?;
        Ok(Self { config_path })
    }

    /// Get the path to the configuration file
    fn get_config_path() -> Result<PathBuf> {
        let config_dir = config_dir()
            .ok_or_else(|| NetworkMenuError::config_error("Could not determine config directory"))?;
        
        let app_config_dir = config_dir.join(CONFIG_DIR_NAME);
        Ok(app_config_dir.join(CONFIG_FILE_NAME))
    }

    /// Load configuration from file, creating default if it doesn't exist
    pub async fn load(&self) -> Result<Config> {
        if !self.config_path.exists() {
            info!("Configuration file not found, creating default configuration");
            self.create_default_config().await?;
        }

        self.load_from_file().await
    }

    /// Load configuration from file
    async fn load_from_file(&self) -> Result<Config> {
        debug!("Loading configuration from: {:?}", self.config_path);
        
        let content = tokio::fs::read_to_string(&self.config_path)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to read config file: {}", e)))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to parse config file: {}", e)))?;

        self.validate_config(&config)?;
        
        debug!("Configuration loaded successfully");
        Ok(config)
    }

    /// Create default configuration file
    async fn create_default_config(&self) -> Result<()> {
        let default_config = self.get_default_config();
        
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NetworkMenuError::config_error(format!("Failed to create config directory: {}", e)))?;
        }

        let toml_content = toml::to_string_pretty(&default_config)
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to serialize default config: {}", e)))?;

        tokio::fs::write(&self.config_path, toml_content)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to write default config: {}", e)))?;

        info!("Default configuration created at: {:?}", self.config_path);
        Ok(())
    }

    /// Get default configuration
    fn get_default_config(&self) -> Config {
        Config {
            actions: vec![
                CustomAction {
                    display: "🌐 Open Network Settings".to_string(),
                    cmd: "nm-connection-editor".to_string(),
                    args: vec![],
                    confirm: false,
                },
                CustomAction {
                    display: "🔄 Restart NetworkManager".to_string(),
                    cmd: "systemctl".to_string(),
                    args: vec!["restart".to_string(), "NetworkManager".to_string()],
                    confirm: true,
                },
            ],
            exclude_exit_node: vec![
                "example-node".to_string(),
            ],
            dmenu_cmd: DEFAULT_DMENU_CMD.to_string(),
            dmenu_args: vec![
                "-i".to_string(), // case insensitive
                "-l".to_string(), // vertical list
                "10".to_string(), // show 10 lines
            ],
            ..Default::default()
        }
    }

    /// Validate configuration
    fn validate_config(&self, config: &Config) -> Result<()> {
        // Validate dmenu command
        if config.dmenu_cmd.is_empty() {
            return Err(NetworkMenuError::validation_error("dmenu_cmd cannot be empty"));
        }

        // Validate custom actions
        for (index, action) in config.actions.iter().enumerate() {
            if action.display.is_empty() {
                return Err(NetworkMenuError::validation_error(
                    format!("Custom action at index {} has empty display text", index)
                ));
            }
            if action.cmd.is_empty() {
                return Err(NetworkMenuError::validation_error(
                    format!("Custom action '{}' has empty command", action.display)
                ));
            }
        }

        // Validate notification timeout
        if config.notifications.timeout_ms < 0 {
            return Err(NetworkMenuError::validation_error("Notification timeout cannot be negative"));
        }

        debug!("Configuration validation passed");
        Ok(())
    }

    /// Save configuration to file
    pub async fn save(&self, config: &Config) -> Result<()> {
        self.validate_config(config)?;

        let toml_content = toml::to_string_pretty(config)
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to serialize config: {}", e)))?;

        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NetworkMenuError::config_error(format!("Failed to create config directory: {}", e)))?;
        }

        tokio::fs::write(&self.config_path, toml_content)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to write config file: {}", e)))?;

        info!("Configuration saved to: {:?}", self.config_path);
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Check if configuration file exists
    pub fn config_exists(&self) -> bool {
        self.config_path.exists()
    }

    /// Backup current configuration
    pub async fn backup(&self) -> Result<PathBuf> {
        if !self.config_path.exists() {
            return Err(NetworkMenuError::config_error("No configuration file to backup"));
        }

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = self.config_path.with_extension(format!("toml.backup_{}", timestamp));

        tokio::fs::copy(&self.config_path, &backup_path)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to create backup: {}", e)))?;

        info!("Configuration backed up to: {:?}", backup_path);
        Ok(backup_path)
    }

    /// Restore configuration from backup
    pub async fn restore_from_backup(&self, backup_path: &Path) -> Result<()> {
        if !backup_path.exists() {
            return Err(NetworkMenuError::config_error("Backup file does not exist"));
        }

        // Validate backup file first
        let backup_content = tokio::fs::read_to_string(backup_path)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to read backup file: {}", e)))?;

        let backup_config: Config = toml::from_str(&backup_content)
            .map_err(|e| NetworkMenuError::config_error(format!("Invalid backup file format: {}", e)))?;

        self.validate_config(&backup_config)?;

        // Create current backup before restoring
        if self.config_path.exists() {
            let _ = self.backup().await; // Don't fail if backup fails
        }

        tokio::fs::copy(backup_path, &self.config_path)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to restore from backup: {}", e)))?;

        info!("Configuration restored from: {:?}", backup_path);
        Ok(())
    }

    /// Get available backup files
    pub async fn list_backups(&self) -> Result<Vec<PathBuf>> {
        let config_dir = self.config_path.parent()
            .ok_or_else(|| NetworkMenuError::config_error("Invalid config path"))?;

        if !config_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();
        let mut entries = tokio::fs::read_dir(config_dir)
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to read config directory: {}", e)))?;

        while let Some(entry) = entries.next_entry()
            .await
            .map_err(|e| NetworkMenuError::config_error(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("config.toml.backup_") {
                    backups.push(path);
                }
            }
        }

        backups.sort();
        Ok(backups)
    }

    /// Reset configuration to default
    pub async fn reset_to_default(&self) -> Result<()> {
        // Create backup first
        if self.config_path.exists() {
            let _ = self.backup().await; // Don't fail if backup fails
        }

        self.create_default_config().await?;
        info!("Configuration reset to default");
        Ok(())
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().expect("Failed to create configuration manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_creation_and_loading() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        let manager = ConfigManager {
            config_path: config_path.clone(),
        };

        // Test default config creation
        manager.create_default_config().await.unwrap();
        assert!(config_path.exists());

        // Test loading
        let config = manager.load_from_file().await.unwrap();
        assert!(!config.actions.is_empty());
        assert_eq!(config.dmenu_cmd, DEFAULT_DMENU_CMD);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let manager = ConfigManager::new().unwrap();
        
        let mut invalid_config = Config::default();
        invalid_config.dmenu_cmd = String::new();
        
        assert!(manager.validate_config(&invalid_config).is_err());
        
        let valid_config = Config::default();
        assert!(manager.validate_config(&valid_config).is_ok());
    }

    #[tokio::test]
    async fn test_backup_and_restore() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        let manager = ConfigManager {
            config_path: config_path.clone(),
        };

        // Create initial config
        manager.create_default_config().await.unwrap();
        
        // Create backup
        let backup_path = manager.backup().await.unwrap();
        assert!(backup_path.exists());
        
        // Modify config
        let mut config = manager.load_from_file().await.unwrap();
        config.dmenu_cmd = "modified_dmenu".to_string();
        manager.save(&config).await.unwrap();
        
        // Restore from backup
        manager.restore_from_backup(&backup_path).await.unwrap();
        
        // Verify restoration
        let restored_config = manager.load_from_file().await.unwrap();
        assert_eq!(restored_config.dmenu_cmd, DEFAULT_DMENU_CMD);
    }
}