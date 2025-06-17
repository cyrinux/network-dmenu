use std::time::Duration;

// Command names
pub mod commands {
    pub const TAILSCALE: &str = "tailscale";
    pub const NMCLI: &str = "nmcli";
    pub const IWCTL: &str = "iwctl";
    pub const BLUETOOTHCTL: &str = "bluetoothctl";
    pub const RFKILL: &str = "rfkill";
    pub const NM_CONNECTION_EDITOR: &str = "nm-connection-editor";
    pub const PINENTRY_GNOME3: &str = "pinentry-gnome3";
}

// Tailscale command arguments
pub mod tailscale_commands {
    pub const STATUS: &[&str] = &["status"];
    pub const STATUS_JSON: &[&str] = &["status", "--json"];
    pub const EXIT_NODE_LIST: &[&str] = &["exit-node", "list"];
    pub const UP: &[&str] = &["up"];
    pub const DOWN: &[&str] = &["down"];
    pub const SET_EXIT_NODE: &[&str] = &["set", "--exit-node"];
    pub const UNSET_EXIT_NODE: &[&str] = &["set", "--exit-node="];
    pub const SET_SHIELDS_UP: &[&str] = &["set", "--shields-up"];
    pub const SET_SHIELDS_DOWN: &[&str] = &["set", "--shields-up=false"];
    pub const NETCHECK: &[&str] = &["netcheck"];
}

// NetworkManager command arguments
pub mod nmcli_commands {
    pub const WIFI_LIST: &[&str] = &["--colors", "no", "-t", "-f", "IN-USE,SSID,BARS,SECURITY", "device", "wifi"];
    pub const WIFI_LIST_RESCAN: &[&str] = &["--colors", "no", "dev", "wifi", "list", "--rescan", "auto"];
    pub const VPN_LIST: &[&str] = &["--colors", "no", "-t", "-f", "ACTIVE,TYPE,NAME", "connection", "show"];
    pub const DEVICE_STATUS: &[&str] = &["--colors", "no", "-t", "-f", "DEVICE,STATE", "device", "status"];
    pub const CONNECTION_UP: &[&str] = &["connection", "up"];
    pub const CONNECTION_DOWN: &[&str] = &["connection", "down"];
    pub const DEVICE_WIFI_CONNECT: &[&str] = &["device", "wifi", "connect"];
    pub const DEVICE_WIFI_DISCONNECT: &[&str] = &["device", "wifi", "disconnect"];
}

// IWD command arguments
pub mod iwd_commands {
    pub const STATION_SCAN: &[&str] = &["station", "scan"];
    pub const STATION_GET_NETWORKS: &[&str] = &["station", "get-networks"];
    pub const STATION_CONNECT: &[&str] = &["station", "connect"];
    pub const STATION_DISCONNECT: &[&str] = &["station", "disconnect"];
    pub const KNOWN_NETWORKS_LIST: &[&str] = &["known-networks", "list"];
}

// Bluetooth command arguments
pub mod bluetooth_commands {
    pub const DEVICES: &[&str] = &["devices"];
    pub const PAIRED_DEVICES: &[&str] = &["paired-devices"];
    pub const CONNECT: &[&str] = &["connect"];
    pub const DISCONNECT: &[&str] = &["disconnect"];
    pub const TRUST: &[&str] = &["trust"];
    pub const UNTRUST: &[&str] = &["untrust"];
}

// RFKill command arguments
pub mod rfkill_commands {
    pub const BLOCK_WIFI: &[&str] = &["block", "wifi"];
    pub const UNBLOCK_WIFI: &[&str] = &["unblock", "wifi"];
}

// WiFi strength symbols
pub const WIFI_STRENGTH_SYMBOLS: &[&str] = &["_", "▂", "▄", "▆", "█"];

// Network detection
pub const DETECT_CAPTIVE_PORTAL_URL: &str = "http://detectportal.firefox.com/";
pub const EXPECTED_PORTAL_RESPONSE: &str = "success";

// Timeouts and retry settings
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
pub const CAPTIVE_PORTAL_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_RETRY_ATTEMPTS: usize = 3;
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

// Configuration
pub const CONFIG_FILE_NAME: &str = "config.toml";
pub const DEFAULT_DMENU_CMD: &str = "dmenu";

// Environment variables
pub const LC_ALL_ENV: &str = "LC_ALL";
pub const LC_ALL_VALUE: &str = "C";

// Separators and delimiters
pub const FIELD_SEPARATOR: char = ':';
pub const MENU_SEPARATOR: &str = " | ";
pub const ACTION_SEPARATOR: &str = " → ";

// Display formatting
pub const CONNECTED_INDICATOR: &str = "●";
pub const DISCONNECTED_INDICATOR: &str = "○";
pub const ACTIVE_INDICATOR: &str = "*";

// Notification settings
pub const NOTIFICATION_TIMEOUT_MS: i32 = 3000;

// Regular expressions
pub const TAILSCALE_EXIT_NODE_REGEX: &str = r"\s{2,}";
pub const IP_ADDRESS_REGEX: &str = r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b";
pub const MAC_ADDRESS_REGEX: &str = r"^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$";

// Menu labels
pub mod menu_labels {
    pub const WIFI_CONNECT: &str = "📶 Connect to WiFi";
    pub const WIFI_DISCONNECT: &str = "📶 Disconnect WiFi";
    pub const WIFI_CONNECT_HIDDEN: &str = "🔒 Connect to Hidden Network";
    pub const VPN_CONNECT: &str = "🔒 Connect VPN";
    pub const VPN_DISCONNECT: &str = "🔒 Disconnect VPN";
    pub const TAILSCALE_ENABLE: &str = "🟢 Enable Tailscale";
    pub const TAILSCALE_DISABLE: &str = "🔴 Disable Tailscale";
    pub const TAILSCALE_SHIELDS_UP: &str = "🛡️ Shields Up";
    pub const TAILSCALE_SHIELDS_DOWN: &str = "🛡️ Shields Down";
    pub const TAILSCALE_DISABLE_EXIT_NODE: &str = "🚪 Disable Exit Node";
    pub const BLUETOOTH_CONNECT: &str = "🔵 Connect Bluetooth";
    pub const BLUETOOTH_DISCONNECT: &str = "🔵 Disconnect Bluetooth";
    pub const EDIT_CONNECTIONS: &str = "⚙️ Edit Connections";
    pub const RFKILL_BLOCK: &str = "📵 Block WiFi";
    pub const RFKILL_UNBLOCK: &str = "📶 Unblock WiFi";
}

// File extensions and paths
pub const CONFIG_DIR_NAME: &str = "network-dmenu";
pub const LOG_FILE_NAME: &str = "network-dmenu.log";

// Network types
pub const NETWORK_TYPE_VPN: &str = "vpn";
pub const NETWORK_TYPE_WIFI: &str = "802-11-wireless";
pub const SECURITY_NONE: &str = "--";
pub const SECURITY_WEP: &str = "WEP";
pub const SECURITY_WPA: &str = "WPA";
pub const SECURITY_WPA2: &str = "WPA2";

// Tailscale specific
pub const MULLVAD_DOMAIN: &str = "mullvad.ts.net";
pub const TAILNET_DOMAIN: &str = "ts.net";

// Exit codes
pub const EXIT_CODE_SUCCESS: i32 = 0;
pub const EXIT_CODE_ERROR: i32 = 1;
pub const EXIT_CODE_USER_CANCELLED: i32 = 2;