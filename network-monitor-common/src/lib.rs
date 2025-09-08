//! Common data structures shared between eBPF programs and userspace
//!
//! This crate defines the data structures used to communicate network events
//! between the eBPF programs running in kernel space and the userspace daemon.

#![no_std]

#[cfg(feature = "user")]
use aya::Pod;

/// Network event types that can be detected by BPF programs
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "user", derive(Pod))]
pub enum NetworkEventType {
    /// Network interface brought up
    InterfaceUp = 1,
    /// Network interface brought down  
    InterfaceDown = 2,
    /// WiFi connection established
    WifiConnected = 3,
    /// WiFi connection lost
    WifiDisconnected = 4,
    /// IP address assigned/changed
    IpAddressChange = 5,
    /// Default route changed
    RouteChange = 6,
    /// Network namespace change
    NetnsChange = 7,
    /// Packet received (for traffic analysis)
    PacketReceived = 8,
}

/// Maximum length for interface names (matches Linux IFNAMSIZ)
pub const IFNAMSIZ: usize = 16;
/// Maximum length for SSID
pub const MAX_SSID_LEN: usize = 32;
/// Maximum length for BSSID
pub const ETH_ALEN: usize = 6;

/// Network event data structure passed from eBPF programs
/// Simplified structure for better compatibility between kernel and user space
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "user", derive(Pod))]
pub struct NetworkEvent {
    /// Type of network event
    pub event_type: NetworkEventType,
    /// Timestamp when event occurred (nanoseconds since boot)
    pub timestamp: u64,
    /// Network interface index
    pub if_index: u32,
    /// Interface name (up to 16 chars, matching IFNAMSIZ)
    pub if_name: [u8; IFNAMSIZ],
    /// MAC address (for interface events)
    pub mac_addr: [u8; ETH_ALEN],
    /// WiFi SSID (for WiFi events) 
    pub ssid: [u8; MAX_SSID_LEN],
    /// SSID length
    pub ssid_len: u8,
    /// BSSID (for WiFi events)
    pub bssid: [u8; ETH_ALEN], 
    /// Signal strength (for WiFi events)
    pub signal_strength: i32,
    /// IP address (for IP change events) - supports both IPv4 and IPv6
    pub ip_addr: [u8; 16],
    /// Address family (AF_INET or AF_INET6)
    pub addr_family: u8,
    /// Protocol (for packet events)
    pub protocol: u8,
    /// Packet length (for packet events)
    pub length: u32,
    /// MTU (for interface events)
    pub mtu: u32,
    /// Generic flags/state
    pub flags: u32,
}

impl NetworkEvent {
    /// Create a new network event with default values
    pub const fn new(event_type: NetworkEventType) -> Self {
        Self {
            event_type,
            timestamp: 0,
            if_index: 0,
            if_name: [0; IFNAMSIZ],
            mac_addr: [0; ETH_ALEN],
            ssid: [0; MAX_SSID_LEN],
            ssid_len: 0,
            bssid: [0; ETH_ALEN],
            signal_strength: 0,
            ip_addr: [0; 16],
            addr_family: 0,
            protocol: 0,
            length: 0,
            mtu: 0,
            flags: 0,
        }
    }
}

/// Helper function to convert C string to Rust string
pub fn cstring_to_str(cstr: &[u8]) -> &str {
    // Find the null terminator
    let end = cstr.iter().position(|&c| c == 0).unwrap_or(cstr.len());
    core::str::from_utf8(&cstr[..end]).unwrap_or("")
}

/// Helper function to convert Rust string to C string
pub fn str_to_cstring(s: &str, buf: &mut [u8]) -> usize {
    let bytes = s.as_bytes();
    let len = core::cmp::min(bytes.len(), buf.len() - 1);
    buf[..len].copy_from_slice(&bytes[..len]);
    if len < buf.len() {
        buf[len] = 0; // Null terminator
    }
    len
}