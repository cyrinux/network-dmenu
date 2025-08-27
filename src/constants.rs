
//! This module contains all user-facing strings to facilitate easy translation and maintenance

// Action types
pub const ACTION_TYPE_ACTION: &str = "action";
pub const ACTION_TYPE_DIAGNOSTIC: &str = "diagnostic";
pub const ACTION_TYPE_NEXTDNS: &str = "nextdns";
pub const ACTION_TYPE_SYSTEM: &str = "system";
pub const ACTION_TYPE_TAILSCALE: &str = "tailscale";
pub const ACTION_TYPE_VPN: &str = "vpn";
pub const ACTION_TYPE_WIFI: &str = "wifi";

// Icons
pub const ICON_CROSS: &str = "❌";
pub const ICON_CHECK: &str = "✅";
pub const ICON_SIGNAL: &str = "📶";

pub const ICON_LOCK: &str = "🔒";
pub const ICON_LIST: &str = "📋";
pub const ICON_FIREWALL_BLOCK: &str = "🚫";
pub const ICON_FIREWALL_ALLOW: &str = "🔓";
pub const ICON_STAR: &str = "🌟";
pub const ICON_BLUETOOTH: &str = "";
pub const ICON_KEY: &str = "🔑";
pub const ICON_LEAF: &str = "🌿";

// Security types
pub const SECURITY_OPEN: &str = "OPEN";
pub const SECURITY_UNKNOWN: &str = "UNKNOWN";

// We use format strings directly in the code instead of constants
// to avoid issues with the format! macro

// System actions
pub const SYSTEM_AIRPLANE_MODE_ON: &str = "Turn ON airplane mode";
pub const SYSTEM_AIRPLANE_MODE_OFF: &str = "Turn OFF airplane mode";
pub const SYSTEM_EDIT_CONNECTIONS: &str = "Edit connections";

// Tailscale actions
pub const TAILSCALE_DISABLE_EXIT_NODE: &str = "Disable exit-node";
pub const TAILSCALE_ENABLE: &str = "Enable tailscale";
pub const TAILSCALE_DISABLE: &str = "Disable tailscale";
pub const TAILSCALE_SHIELDS_UP: &str = "Block incoming connections";
pub const TAILSCALE_SHIELDS_DOWN: &str = "Allow incoming connections";
pub const TAILSCALE_ALLOW_ADVERTISE_ROUTES: &str = "Allow advertise routes";
pub const TAILSCALE_DISALLOW_ADVERTISE_ROUTES: &str = "Disallow advertise routes";
pub const TAILSCALE_ALLOW_LAN_ACCESS_EXIT_NODE: &str = "Allow lan access while exit-node used";
pub const TAILSCALE_DISALLOW_LAN_ACCESS_EXIT_NODE: &str =
    "Disallow lan access while exit-node used";
pub const TAILSCALE_SHOW_LOCK_STATUS: &str = "Show Tailscale Lock Status";
pub const TAILSCALE_LIST_LOCKED_NODES: &str = "List Locked Nodes";
pub const TAILSCALE_SIGN_NODE: &str = "Sign Node: {}";
pub const TAILSCALE_SIGN_NODE_DETAILED: &str = "Sign Node: {flag} {hostname} - {machine} ({key})";

// WiFi actions
pub const WIFI_DISCONNECT: &str = "Disconnect";
pub const WIFI_CONNECT: &str = "Connect";
pub const WIFI_CONNECT_HIDDEN: &str = "Connect to hidden network";

// Suggested node format

pub const SUGGESTED_CHECK: &str = "(suggested";

// Network connection messages
// Error messages
pub const ERROR_CONFIG_READ: &str = "Failed to read config";

// Default config values
pub const DEFAULT_DMENU_CMD: &str = "dmenu";
pub const DEFAULT_DMENU_ARGS: &str = "--no-multi";

// Config file
pub const CONFIG_FILENAME: &str = "config.toml";
pub const CONFIG_DIR_NAME: &str = "network-dmenu";
