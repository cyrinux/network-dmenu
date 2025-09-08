//! Custom notification system for zone actions
//!
//! Provides a consistent notification interface using notify-rust crate
//! to replace manual notify-send commands in custom_commands.

use log::{debug, error, info, warn};
use notify_rust::{Notification, Timeout, Urgency};
use serde::{Deserialize, Serialize};

/// Notification configuration for zone actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationConfig {
    /// Whether to show notifications
    pub enabled: bool,
    /// Notification timeout in seconds (0 = no timeout)
    pub timeout_seconds: u32,
    /// Default notification urgency level
    pub default_urgency: NotificationUrgency,
    /// Application name for notifications
    pub app_name: String,
    /// Icon to use for notifications
    pub icon: Option<String>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_seconds: 5,
            default_urgency: NotificationUrgency::Normal,
            app_name: "Network DMenu".to_string(),
            icon: Some("network-wireless".to_string()),
        }
    }
}

/// Notification urgency levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

impl From<NotificationUrgency> for Urgency {
    fn from(urgency: NotificationUrgency) -> Self {
        match urgency {
            NotificationUrgency::Low => Urgency::Low,
            NotificationUrgency::Normal => Urgency::Normal,
            NotificationUrgency::Critical => Urgency::Critical,
        }
    }
}

/// Custom notification system for zone actions
pub struct NotificationManager {
    config: NotificationConfig,
}

impl NotificationManager {
    /// Create new notification manager with configuration
    pub fn new(config: NotificationConfig) -> Self {
        debug!("Creating notification manager with config: enabled={}, app_name={}", 
               config.enabled, config.app_name);
        Self { config }
    }

    /// Create default notification manager
    pub fn default() -> Self {
        Self::new(NotificationConfig::default())
    }

    /// Send a notification with title and message
    pub fn notify(&self, title: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.notify_with_urgency(title, message, self.config.default_urgency)
    }

    /// Send a notification with custom urgency
    pub fn notify_with_urgency(
        &self,
        title: &str,
        message: &str,
        urgency: NotificationUrgency,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config.enabled {
            debug!("Notifications disabled, skipping: {} - {}", title, message);
            return Ok(());
        }

        debug!("Sending notification: '{}' - '{}'", title, message);

        let mut notification = Notification::new();
        notification
            .summary(title)
            .body(message)
            .appname(&self.config.app_name)
            .urgency(urgency.into());

        // Set timeout
        if self.config.timeout_seconds > 0 {
            notification.timeout(Timeout::Milliseconds(self.config.timeout_seconds * 1000));
        } else {
            notification.timeout(Timeout::Never);
        }

        // Set icon if configured
        if let Some(ref icon) = self.config.icon {
            notification.icon(icon);
        }

        match notification.show() {
            Ok(_) => {
                info!("Notification sent: {} - {}", title, message);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send notification '{}': {}", title, e);
                Err(Box::new(e))
            }
        }
    }

    /// Send zone enter notification
    pub fn notify_zone_enter(&self, zone_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let title = "Zone Entered";
        let message = &format!("📍 Entered zone: {}", zone_name);
        self.notify(title, message)
    }

    /// Send zone exit notification  
    pub fn notify_zone_exit(&self, zone_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let title = "Zone Exited";
        let message = &format!("📍 Left zone: {}", zone_name);
        self.notify(title, message)
    }

    /// Send security alert notification
    pub fn notify_security_alert(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let title = "Security Alert";
        self.notify_with_urgency(title, message, NotificationUrgency::Critical)
    }

    /// Send unknown zone protection notification
    pub fn notify_unknown_zone_protection(&self) -> Result<(), Box<dyn std::error::Error>> {
        let title = "Security Alert";
        let message = "🛡️ Unknown location detected - Security mode activated";
        self.notify_with_urgency(title, message, NotificationUrgency::Critical)
    }

    /// Send VPN connection notification
    pub fn notify_vpn_connected(&self, vpn_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let title = "VPN Connected";
        let message = &format!("🔐 Connected to VPN: {}", vpn_name);
        self.notify(title, message)
    }

    /// Send WiFi connection notification
    pub fn notify_wifi_connected(&self, ssid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let title = "WiFi Connected";
        let message = &format!("📶 Connected to WiFi: {}", ssid);
        self.notify(title, message)
    }

    /// Check if a command string is a notify-send command and extract its content
    pub fn parse_notify_send_command(&self, command: &str) -> Option<(String, String, NotificationUrgency)> {
        let command = command.trim();
        
        // Check if it's a notify-send command
        if !command.starts_with("notify-send") {
            return None;
        }

        debug!("Parsing notify-send command: {}", command);

        // Parse basic notify-send command formats:
        // notify-send 'title' 'message'
        // notify-send 'title' 'message' --urgency=critical
        // notify-send "title" "message" --urgency=normal
        
        let mut title = String::new();
        let mut message = String::new();
        let mut urgency = NotificationUrgency::Normal;
        
        // Split command and extract parts
        let parts: Vec<&str> = command.split_whitespace().collect();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut current_part = String::new();
        let mut parsed_parts = Vec::new();

        // Simple parser for quoted strings
        for part in parts.iter().skip(1) { // Skip "notify-send"
            if part.starts_with('"') || part.starts_with('\'') {
                if !in_quotes {
                    in_quotes = true;
                    quote_char = part.chars().next().unwrap();
                    current_part = part[1..].to_string();
                    
                    if part.ends_with(quote_char) && part.len() > 1 {
                        // Single-word quoted string
                        parsed_parts.push(current_part[..current_part.len()-1].to_string());
                        current_part.clear();
                        in_quotes = false;
                    }
                } else {
                    current_part.push(' ');
                    current_part.push_str(part);
                }
            } else if in_quotes {
                if part.ends_with(quote_char) {
                    current_part.push(' ');
                    current_part.push_str(&part[..part.len()-1]);
                    parsed_parts.push(current_part.clone());
                    current_part.clear();
                    in_quotes = false;
                } else {
                    current_part.push(' ');
                    current_part.push_str(part);
                }
            } else if part.starts_with("--urgency=") {
                let urgency_str = &part[10..];
                urgency = match urgency_str.to_lowercase().as_str() {
                    "low" => NotificationUrgency::Low,
                    "normal" => NotificationUrgency::Normal,
                    "critical" => NotificationUrgency::Critical,
                    _ => NotificationUrgency::Normal,
                };
            } else {
                parsed_parts.push(part.to_string());
            }
        }

        // Extract title and message
        if parsed_parts.len() >= 1 {
            title = parsed_parts[0].clone();
        }
        if parsed_parts.len() >= 2 {
            message = parsed_parts[1].clone();
        }

        if !title.is_empty() {
            debug!("Parsed notify-send: title='{}', message='{}', urgency={:?}", 
                   title, message, urgency);
            Some((title, message, urgency))
        } else {
            warn!("Failed to parse notify-send command: {}", command);
            None
        }
    }

    /// Execute a custom notification (replaces notify-send commands)
    pub fn execute_notification_command(&self, command: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some((title, message, urgency)) = self.parse_notify_send_command(command) {
            debug!("Converting notify-send to custom notification: {} - {}", title, message);
            self.notify_with_urgency(&title, &message, urgency)
        } else {
            Err(format!("Invalid notification command: {}", command).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_config_default() {
        let config = NotificationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.timeout_seconds, 5);
        assert_eq!(config.default_urgency, NotificationUrgency::Normal);
        assert_eq!(config.app_name, "Network DMenu");
    }

    #[test]
    fn test_parse_notify_send_basic() {
        let manager = NotificationManager::default();
        
        let result = manager.parse_notify_send_command("notify-send 'Hello' 'World'");
        assert!(result.is_some());
        
        let (title, message, urgency) = result.unwrap();
        assert_eq!(title, "Hello");
        assert_eq!(message, "World");
        assert_eq!(urgency, NotificationUrgency::Normal);
    }

    #[test]
    fn test_parse_notify_send_with_urgency() {
        let manager = NotificationManager::default();
        
        let result = manager.parse_notify_send_command(
            "notify-send 'Security Alert' 'Entered unknown location - protection mode activated' --urgency=critical"
        );
        assert!(result.is_some());
        
        let (title, message, urgency) = result.unwrap();
        assert_eq!(title, "Security Alert");
        assert_eq!(message, "Entered unknown location - protection mode activated");
        assert_eq!(urgency, NotificationUrgency::Critical);
    }

    #[test]
    fn test_parse_notify_send_double_quotes() {
        let manager = NotificationManager::default();
        
        let result = manager.parse_notify_send_command("notify-send \"Zone Change\" \"Welcome Home!\"");
        assert!(result.is_some());
        
        let (title, message, urgency) = result.unwrap();
        assert_eq!(title, "Zone Change");
        assert_eq!(message, "Welcome Home!");
        assert_eq!(urgency, NotificationUrgency::Normal);
    }

    #[test]
    fn test_parse_non_notify_send() {
        let manager = NotificationManager::default();
        
        let result = manager.parse_notify_send_command("echo 'not a notification'");
        assert!(result.is_none());
    }

    #[test]
    fn test_urgency_conversion() {
        assert_eq!(Urgency::Low, NotificationUrgency::Low.into());
        assert_eq!(Urgency::Normal, NotificationUrgency::Normal.into());
        assert_eq!(Urgency::Critical, NotificationUrgency::Critical.into());
    }
}