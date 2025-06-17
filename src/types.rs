use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Type alias for efficient string handling
pub type SharedString = Arc<str>;
pub type DisplayString = Cow<'static, str>;

/// Represents the security type of a WiFi network
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityType {
    None,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Enterprise,
    Unknown(String),
}

impl From<&str> for SecurityType {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "" | "--" | "NONE" => SecurityType::None,
            "WEP" => SecurityType::Wep,
            "WPA" => SecurityType::Wpa,
            "WPA2" => SecurityType::Wpa2,
            "WPA3" => SecurityType::Wpa3,
            "WPA-ENTERPRISE" | "WPA2-ENTERPRISE" => SecurityType::Enterprise,
            other => SecurityType::Unknown(other.to_string()),
        }
    }
}

impl std::fmt::Display for SecurityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityType::None => write!(f, "Open"),
            SecurityType::Wep => write!(f, "WEP"),
            SecurityType::Wpa => write!(f, "WPA"),
            SecurityType::Wpa2 => write!(f, "WPA2"),
            SecurityType::Wpa3 => write!(f, "WPA3"),
            SecurityType::Enterprise => write!(f, "Enterprise"),
            SecurityType::Unknown(s) => write!(f, "{}", s),
        }
    }
}

/// Represents the connection state of a network device
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Connecting,
    Disconnecting,
    Failed,
    Unknown,
}

impl From<&str> for ConnectionState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "connected" | "activated" | "yes" => ConnectionState::Connected,
            "disconnected" | "deactivated" | "no" => ConnectionState::Disconnected,
            "connecting" | "activating" => ConnectionState::Connecting,
            "disconnecting" | "deactivating" => ConnectionState::Disconnecting,
            "failed" => ConnectionState::Failed,
            _ => ConnectionState::Unknown,
        }
    }
}

/// Represents a WiFi network entry
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: SharedString,
    pub security: SecurityType,
    pub signal_strength: Option<u8>,
    pub is_connected: bool,
    pub is_known: bool,
}

impl WifiNetwork {
    pub fn new(ssid: impl Into<SharedString>) -> Self {
        Self {
            ssid: ssid.into(),
            security: SecurityType::None,
            signal_strength: None,
            is_connected: false,
            is_known: false,
        }
    }

    pub fn with_security(mut self, security: SecurityType) -> Self {
        self.security = security;
        self
    }

    pub fn with_signal_strength(mut self, strength: u8) -> Self {
        self.signal_strength = Some(strength);
        self
    }

    pub fn with_connection_state(mut self, connected: bool) -> Self {
        self.is_connected = connected;
        self
    }

    pub fn with_known_state(mut self, known: bool) -> Self {
        self.is_known = known;
        self
    }
}

/// Represents a VPN connection entry
#[derive(Debug, Clone)]
pub struct VpnConnection {
    pub name: SharedString,
    pub connection_type: SharedString,
    pub is_active: bool,
    pub uuid: Option<SharedString>,
}

impl VpnConnection {
    pub fn new(name: impl Into<SharedString>, connection_type: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            connection_type: connection_type.into(),
            is_active: false,
            uuid: None,
        }
    }

    pub fn with_active_state(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }

    pub fn with_uuid(mut self, uuid: impl Into<SharedString>) -> Self {
        self.uuid = Some(uuid.into());
        self
    }
}

/// Represents a Bluetooth device
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub name: SharedString,
    pub address: SharedString,
    pub is_connected: bool,
    pub is_paired: bool,
    pub is_trusted: bool,
    pub device_type: Option<SharedString>,
}

impl BluetoothDevice {
    pub fn new(name: impl Into<SharedString>, address: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
            is_connected: false,
            is_paired: false,
            is_trusted: false,
            device_type: None,
        }
    }

    pub fn with_connection_state(mut self, connected: bool) -> Self {
        self.is_connected = connected;
        self
    }

    pub fn with_paired_state(mut self, paired: bool) -> Self {
        self.is_paired = paired;
        self
    }

    pub fn with_trusted_state(mut self, trusted: bool) -> Self {
        self.is_trusted = trusted;
        self
    }

    pub fn with_device_type(mut self, device_type: impl Into<SharedString>) -> Self {
        self.device_type = Some(device_type.into());
        self
    }
}

/// Represents a Tailscale exit node
#[derive(Debug, Clone)]
pub struct TailscaleExitNode {
    pub name: SharedString,
    pub hostname: SharedString,
    pub location: Option<SharedString>,
    pub is_active: bool,
    pub is_mullvad: bool,
}

impl TailscaleExitNode {
    pub fn new(name: impl Into<SharedString>, hostname: impl Into<SharedString>) -> Self {
        let hostname_str = hostname.into();
        let is_mullvad = hostname_str.contains("mullvad.ts.net");
        
        Self {
            name: name.into(),
            hostname: hostname_str,
            location: None,
            is_active: false,
            is_mullvad,
        }
    }

    pub fn with_location(mut self, location: impl Into<SharedString>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn with_active_state(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
}

/// Represents the state of a network interface
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: SharedString,
    pub interface_type: InterfaceType,
    pub state: ConnectionState,
    pub ip_address: Option<SharedString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceType {
    Wifi,
    Ethernet,
    Vpn,
    Bluetooth,
    Unknown,
}

impl From<&str> for InterfaceType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "wifi" | "wireless" | "802-11-wireless" => InterfaceType::Wifi,
            "ethernet" | "wired" => InterfaceType::Ethernet,
            "vpn" | "tun" | "tap" => InterfaceType::Vpn,
            "bluetooth" => InterfaceType::Bluetooth,
            _ => InterfaceType::Unknown,
        }
    }
}

/// Configuration for custom actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAction {
    pub display: String,
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub confirm: bool,
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub actions: Vec<CustomAction>,
    
    #[serde(default)]
    pub exclude_exit_node: Vec<String>,
    
    #[serde(default = "default_dmenu_cmd")]
    pub dmenu_cmd: String,
    
    #[serde(default)]
    pub dmenu_args: Vec<String>,
    
    #[serde(default)]
    pub wifi: WifiConfig,
    
    #[serde(default)]
    pub vpn: VpnConfig,
    
    #[serde(default)]
    pub tailscale: TailscaleConfig,
    
    #[serde(default)]
    pub bluetooth: BluetoothConfig,
    
    #[serde(default)]
    pub notifications: NotificationConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            exclude_exit_node: Vec::new(),
            dmenu_cmd: default_dmenu_cmd(),
            dmenu_args: Vec::new(),
            wifi: WifiConfig::default(),
            vpn: VpnConfig::default(),
            tailscale: TailscaleConfig::default(),
            bluetooth: BluetoothConfig::default(),
            notifications: NotificationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    #[serde(default = "default_true")]
    pub auto_scan: bool,
    
    #[serde(default = "default_true")]
    pub show_signal_strength: bool,
    
    #[serde(default)]
    pub preferred_networks: Vec<String>,
}

impl Default for WifiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_scan: true,
            show_signal_strength: true,
            preferred_networks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    #[serde(default = "default_true")]
    pub show_status: bool,
}

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_status: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    #[serde(default = "default_true")]
    pub check_captive_portal: bool,
    
    #[serde(default)]
    pub preferred_exit_nodes: Vec<String>,
}

impl Default for TailscaleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_captive_portal: true,
            preferred_exit_nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    #[serde(default = "default_true")]
    pub auto_trust: bool,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_trust: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    #[serde(default = "default_notification_timeout")]
    pub timeout_ms: i32,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: default_notification_timeout(),
        }
    }
}

/// Context passed to action handlers
#[derive(Debug, Clone)]
pub struct ActionContext {
    pub wifi_interface: Option<SharedString>,
    pub config: Config,
    pub connected_devices: Vec<SharedString>,
}

impl ActionContext {
    pub fn new(config: Config) -> Self {
        Self {
            wifi_interface: None,
            config,
            connected_devices: Vec::new(),
        }
    }

    pub fn with_wifi_interface(mut self, interface: impl Into<SharedString>) -> Self {
        self.wifi_interface = Some(interface.into());
        self
    }

    pub fn with_connected_devices(mut self, devices: Vec<SharedString>) -> Self {
        self.connected_devices = devices;
        self
    }
}

/// Menu entry for display
#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub display_text: SharedString,
    pub action_key: SharedString,
    pub icon: Option<SharedString>,
    pub description: Option<SharedString>,
}

impl MenuEntry {
    pub fn new(display_text: impl Into<SharedString>, action_key: impl Into<SharedString>) -> Self {
        Self {
            display_text: display_text.into(),
            action_key: action_key.into(),
            icon: None,
            description: None,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

// Helper functions for serde defaults
fn default_dmenu_cmd() -> String {
    crate::constants::DEFAULT_DMENU_CMD.to_string()
}

fn default_true() -> bool {
    true
}

fn default_notification_timeout() -> i32 {
    crate::constants::NOTIFICATION_TIMEOUT_MS
}

/// Trait for converting types to SharedString efficiently
pub trait IntoSharedString {
    fn into_shared_string(self) -> SharedString;
}

impl IntoSharedString for String {
    fn into_shared_string(self) -> SharedString {
        self.into()
    }
}

impl IntoSharedString for &str {
    fn into_shared_string(self) -> SharedString {
        self.into()
    }
}

impl IntoSharedString for &String {
    fn into_shared_string(self) -> SharedString {
        self.as_str().into()
    }
}