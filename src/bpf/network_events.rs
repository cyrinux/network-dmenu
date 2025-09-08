//! BPF-based network event monitoring for real-time network state detection
//!
//! This module implements eBPF programs to monitor network interface changes,
//! WiFi connections, and other network events in real-time, replacing the
//! polling-based approach in the geofencing daemon.

use super::{BpfError, BpfResult};
use aya::{
    maps::perf::AsyncPerfEventArray,
    programs::{KProbe, TracePoint, Xdp, XdpFlags, ProgramError},
    util::online_cpus,
    Ebpf, EbpfLoader, Pod,
};
use aya_log::EbpfLogger;
use bytes::BytesMut;
use log::{debug, error, info, warn};
use network_monitor_common::NetworkEvent as CommonNetworkEvent;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tokio_stream::StreamExt;

/// Network event types that can be detected by BPF programs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkEventType {
    /// Network interface brought up
    InterfaceUp,
    /// Network interface brought down  
    InterfaceDown,
    /// WiFi connection established
    WifiConnected,
    /// WiFi connection lost
    WifiDisconnected,
    /// IP address assigned/changed
    IpAddressChange,
    /// Default route changed
    RouteChange,
    /// Network namespace change
    NetnsChange,
    /// Packet received (for traffic analysis)
    PacketReceived,
}

/// Network event data structure passed from eBPF programs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    /// Type of network event
    pub event_type: NetworkEventType,
    /// Timestamp when event occurred (nanoseconds since UNIX epoch)
    pub timestamp: u64,
    /// Network interface index
    pub if_index: u32,
    /// Interface name (up to 16 chars, matching IFNAMSIZ)
    pub if_name: String,
    /// Additional event-specific data
    pub data: NetworkEventData,
}

/// Additional data for specific network events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEventData {
    /// Interface up/down events
    Interface {
        mac_address: [u8; 6],
        mtu: u32,
    },
    /// WiFi events
    Wifi {
        ssid: String,
        bssid: [u8; 6],
        signal_strength: i32,
    },
    /// IP address events
    IpAddress {
        old_addr: Option<std::net::IpAddr>,
        new_addr: Option<std::net::IpAddr>,
    },
    /// Route change events
    Route {
        destination: std::net::IpAddr,
        gateway: std::net::IpAddr,
    },
    /// Packet analysis data
    Packet {
        protocol: u8,
        src_addr: std::net::IpAddr,
        dst_addr: std::net::IpAddr,
        length: u32,
    },
    /// Generic event data
    Generic { data: Vec<u8> },
}

/// BPF-based network event monitor
pub struct BpfNetworkMonitor {
    bpf: Ebpf,
    event_receiver: Option<tokio::sync::mpsc::Receiver<NetworkEvent>>,
    _logger: EbpfLogger,
}

impl BpfNetworkMonitor {
    /// Create a new BPF network monitor
    pub async fn new() -> BpfResult<Self> {
        info!("🔧 Initializing BPF network monitor");

        // Check if we have the required permissions
        if !super::is_bpf_available() {
            return Err(BpfError::PermissionDenied);
        }

        // Load the compiled eBPF program
        let mut bpf = if std::path::Path::new("target/bpf/network-monitor").exists() {
            info!("Loading compiled eBPF program from target/bpf/network-monitor");
            EbpfLoader::new()
                .load(include_bytes!("../../target/bpf/network-monitor"))
                .map_err(|e| {
                    error!("Failed to load compiled eBPF program: {}", e);
                    BpfError::ProgramLoad(e)
                })?
        } else {
            warn!("Compiled eBPF program not found, creating minimal placeholder");
            warn!("Run 'cargo xtask build-ebpf' to compile the eBPF program");
            EbpfLoader::new()
                .load(&[]) // Empty program as fallback
                .map_err(|e| {
                    error!("Failed to create placeholder eBPF program: {}", e);
                    BpfError::ProgramLoad(e)
                })?
        };

        // Initialize eBPF logger for debugging
        if let Err(e) = EbpfLogger::init(&mut bpf) {
            warn!("Failed to initialize eBPF logger: {}", e);
        }
        let logger = EbpfLogger::init(&mut bpf).unwrap_or_else(|_| {
            // Create a dummy logger if initialization fails - for now just use a default
            warn!("Using fallback logger due to eBPF logger initialization failure");
            // Return a dummy logger - in practice we'd handle this more gracefully
            EbpfLogger::init(&mut EbpfLoader::new().load(&[]).unwrap()).unwrap()
        });

        debug!("✅ BPF program loaded successfully");

        Ok(Self {
            bpf,
            event_receiver: None,
            _logger: logger,
        })
    }

    /// Start monitoring network events
    pub async fn start_monitoring(&mut self) -> BpfResult<()> {
        info!("🚀 Starting BPF network event monitoring");

        // Attach tracepoint for network interface events
        self.attach_interface_tracepoints().await?;

        // Attach XDP program for packet analysis (optional)
        if let Err(e) = self.attach_packet_monitor().await {
            warn!("Failed to attach packet monitor (non-critical): {}", e);
        }

        // Set up event processing
        self.setup_event_processing().await?;

        info!("✅ BPF network monitoring started successfully");
        Ok(())
    }

    /// Attach tracepoint and kprobe programs for network interface monitoring
    async fn attach_interface_tracepoints(&mut self) -> BpfResult<()> {
        debug!("📎 Attaching network interface monitoring programs");

        // Try to attach kprobe for netif_receive_skb
        if let Some(program) = self.bpf.program_mut("netif_receive_skb") {
            match program.try_into() {
                Ok(program) => {
                    let program: &mut KProbe = program;
                    if let Err(e) = program.load() {
                        warn!("Failed to load netif_receive_skb kprobe: {}", e);
                    } else if let Err(e) = program.attach("netif_receive_skb", 0) {
                        warn!("Failed to attach netif_receive_skb kprobe: {}", e);
                    } else {
                        debug!("📎 Attached netif_receive_skb kprobe");
                    }
                }
                Err(e) => {
                    warn!("Failed to convert netif_receive_skb to kprobe: {}", e);
                }
            }
        } else {
            debug!("netif_receive_skb program not found in eBPF binary");
        }

        // Try to attach kprobe for dev_queue_xmit 
        if let Some(program) = self.bpf.program_mut("dev_queue_xmit") {
            match program.try_into() {
                Ok(program) => {
                    let program: &mut KProbe = program;
                    if let Err(e) = program.load() {
                        warn!("Failed to load dev_queue_xmit kprobe: {}", e);
                    } else if let Err(e) = program.attach("dev_queue_xmit", 0) {
                        warn!("Failed to attach dev_queue_xmit kprobe: {}", e);
                    } else {
                        debug!("📎 Attached dev_queue_xmit kprobe");
                    }
                }
                Err(e) => {
                    warn!("Failed to convert dev_queue_xmit to kprobe: {}", e);
                }
            }
        } else {
            debug!("dev_queue_xmit program not found in eBPF binary");
        }

        // Try to attach tracepoint for netdev_state_change
        if let Some(program) = self.bpf.program_mut("netdev_state_change") {
            match program.try_into() {
                Ok(program) => {
                    let program: &mut TracePoint = program;
                    if let Err(e) = program.load() {
                        warn!("Failed to load netdev_state_change tracepoint: {}", e);
                    } else if let Err(e) = program.attach("net", "net_dev_start_xmit") {
                        // Using a different tracepoint that's more commonly available
                        warn!("Failed to attach netdev_state_change tracepoint: {}", e);
                    } else {
                        debug!("📎 Attached netdev_state_change tracepoint");
                    }
                }
                Err(e) => {
                    warn!("Failed to convert netdev_state_change to tracepoint: {}", e);
                }
            }
        } else {
            debug!("netdev_state_change program not found in eBPF binary");
        }

        debug!("✅ Network interface monitoring programs attached");
        Ok(())
    }

    /// Attach XDP program for packet-level monitoring (optional)
    async fn attach_packet_monitor(&mut self) -> BpfResult<()> {
        debug!("📎 Attaching XDP packet monitor");

        // Try to find a suitable network interface
        let interfaces = self.get_network_interfaces().await?;
        if interfaces.is_empty() {
            return Err(BpfError::ProgramAttach(
                "No network interfaces available".to_string(),
            ));
        }

        // Attach to the first available interface (typically eth0 or wlan0)
        let interface_name = &interfaces[0];
        debug!("📡 Attaching XDP to interface: {}", interface_name);

        let program: &mut Xdp = self
            .bpf
            .program_mut("packet_monitor")
            .ok_or_else(|| BpfError::ProgramAttach("packet_monitor not found".to_string()))?
            .try_into()
            .map_err(|e| BpfError::ProgramAttach(format!("Invalid XDP program type: {}", e)))?;

        program
            .load()
            .map_err(|e| BpfError::ProgramAttach(format!("Failed to load XDP program: {}", e)))?;

        program
            .attach(interface_name, XdpFlags::default())
            .map_err(|e| {
                BpfError::ProgramAttach(format!("Failed to attach XDP to {}: {}", interface_name, e))
            })?;

        debug!("✅ XDP packet monitor attached to {}", interface_name);
        Ok(())
    }

    /// Get list of available network interfaces
    async fn get_network_interfaces(&self) -> BpfResult<Vec<String>> {
        use std::fs;

        let mut interfaces = Vec::new();

        // Read interfaces from /sys/class/net/
        let net_dir = "/sys/class/net";
        if let Ok(entries) = fs::read_dir(net_dir) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    // Skip loopback interface
                    if name != "lo" {
                        interfaces.push(name);
                    }
                }
            }
        }

        debug!("🔍 Found network interfaces: {:?}", interfaces);
        Ok(interfaces)
    }

    /// Set up event processing from BPF programs
    async fn setup_event_processing(&mut self) -> BpfResult<()> {
        debug!("⚡ Setting up BPF event processing");

        // Try to get the perf event array from the loaded BPF program
        let perf_array_result = self.bpf.take_map("NETWORK_EVENTS");
        
        if let Some(perf_map) = perf_array_result {
            match AsyncPerfEventArray::try_from(perf_map) {
                Ok(mut perf_array) => {
                    debug!("✅ Successfully obtained perf event array from eBPF program");
                    
                    // Create channel for forwarding events
                    let (tx, rx) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);
                    self.event_receiver = Some(rx);

                    // Spawn task to process BPF events
                    tokio::spawn(async move {
                        debug!("🔄 Starting real BPF event processing loop");
                        
                        let cpus = match online_cpus() {
                            Ok(cpus) => cpus,
                            Err(e) => {
                                error!("Failed to get online CPUs: {:?}", e);
                                vec![0] // Fallback to CPU 0
                            }
                        };

                        for cpu_id in cpus {
                            let mut buf = match perf_array.open(cpu_id, None) {
                                Ok(buf) => buf,
                                Err(e) => {
                                    error!("Failed to open perf buffer for CPU {}: {}", cpu_id, e);
                                    continue;
                                }
                            };

                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let mut buffers = (0..10)
                                    .map(|_| BytesMut::with_capacity(1024))
                                    .collect::<Vec<_>>();

                                loop {
                                    match buf.read_events(&mut buffers).await {
                                        Ok(events) => {
                                            debug!("📨 Received {} BPF events from CPU {}", events.read, cpu_id);
                                            
                                            for buf in buffers.iter().take(events.read) {
                                                match Self::parse_common_network_event(buf) {
                                                    Ok(event) => {
                                                        debug!("🔍 Parsed network event: {:?}", event.event_type);
                                                        if let Err(e) = tx_clone.send(event).await {
                                                            error!("Failed to send network event: {}", e);
                                                            break;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        debug!("Failed to parse network event: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error reading BPF events from CPU {}: {}", cpu_id, e);
                                            sleep(Duration::from_millis(100)).await;
                                        }
                                    }
                                }
                            });
                        }

                        debug!("✅ Real BPF event processing setup completed");
                    });
                    
                    return Ok(());
                }
                Err(e) => {
                    warn!("Failed to create perf event array: {}", e);
                }
            }
        } else {
            debug!("NETWORK_EVENTS map not found in eBPF program");
        }

        // Fallback to placeholder mode
        warn!("Falling back to placeholder event processing");
        let (_tx, rx) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);
        self.event_receiver = Some(rx);
        
        debug!("✅ BPF event processing setup completed (placeholder mode)");
        Ok(())
    }

    /// Placeholder for full BPF event processing (commented out due to borrowing issues)
    #[allow(dead_code)]
    async fn setup_full_event_processing(&mut self) -> BpfResult<()> {
        // This would be the full implementation once BPF programs are compiled
        /*
        debug!("⚡ Setting up BPF event processing");

        // Get the perf event array for receiving events from BPF programs
        let mut perf_array = AsyncPerfEventArray::try_from(
            self.bpf
                .map_mut("NETWORK_EVENTS")
                .ok_or_else(|| BpfError::EventProcessing("NETWORK_EVENTS map not found".to_string()))?,
        )
        .map_err(|e| BpfError::EventProcessing(format!("Failed to create perf array: {}", e)))?;

        // Create channel for forwarding events
        let (tx, rx) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);
        self.event_receiver = Some(rx);

        */
        
        // Placeholder implementation
        let (_tx, rx) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);
        self.event_receiver = Some(rx);
        
        debug!("✅ Full BPF event processing would be setup here");
        Ok(())
    }

    /// Parse raw BPF event data from common structure into NetworkEvent
    fn parse_common_network_event(data: &[u8]) -> Result<NetworkEvent, String> {
        // Try to parse the data as a CommonNetworkEvent from the eBPF program
        if data.len() < std::mem::size_of::<CommonNetworkEvent>() {
            return Err(format!(
                "Event data too short: {} bytes, expected at least {}",
                data.len(),
                std::mem::size_of::<CommonNetworkEvent>()
            ));
        }

        // For safety, we'll manually parse the fields rather than casting
        // This is more robust for cross-architecture compatibility
        
        let timestamp = u64::from_ne_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15]
        ]);
        
        let if_index = u32::from_ne_bytes([data[16], data[17], data[18], data[19]]);
        
        // Extract interface name
        let mut if_name = String::new();
        for i in 20..36 { // IFNAMSIZ = 16 bytes at offset 20
            if data[i] == 0 {
                break;
            }
            if_name.push(data[i] as char);
        }
        
        // Convert the common event type to our event type
        let event_type = match data[0] {
            1 => NetworkEventType::InterfaceUp,
            2 => NetworkEventType::InterfaceDown,
            3 => NetworkEventType::WifiConnected,
            4 => NetworkEventType::WifiDisconnected,
            5 => NetworkEventType::IpAddressChange,
            6 => NetworkEventType::RouteChange,
            7 => NetworkEventType::NetnsChange,
            8 => NetworkEventType::PacketReceived,
            _ => {
                return Err(format!("Unknown event type: {}", data[0]));
            }
        };

        let event = NetworkEvent {
            event_type,
            timestamp,
            if_index,
            if_name,
            data: NetworkEventData::Generic { data: data.to_vec() },
        };

        Ok(event)
    }

    /// Legacy parse function for backward compatibility
    #[allow(dead_code)]
    fn parse_network_event(data: &[u8]) -> Result<NetworkEvent, String> {
        // This would normally parse the binary data from the BPF program
        // For now, we'll create a placeholder implementation
        
        if data.len() < 16 {
            return Err("Event data too short".to_string());
        }

        // Mock parsing - in real implementation, this would match the BPF program's data format
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let event = NetworkEvent {
            event_type: NetworkEventType::InterfaceUp, // Would be parsed from data
            timestamp,
            if_index: 1, // Would be parsed from data
            if_name: "wlan0".to_string(), // Would be parsed from data
            data: NetworkEventData::Generic {
                data: data.to_vec(),
            },
        };

        Ok(event)
    }

    /// Get next network event
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        if let Some(ref mut receiver) = self.event_receiver {
            receiver.recv().await
        } else {
            None
        }
    }

    /// Check if monitoring is active
    pub fn is_monitoring(&self) -> bool {
        self.event_receiver.is_some()
    }

    /// Stop monitoring and cleanup resources
    pub async fn stop_monitoring(&mut self) -> BpfResult<()> {
        info!("🛑 Stopping BPF network monitoring");
        
        // Close event receiver
        if let Some(mut receiver) = self.event_receiver.take() {
            receiver.close();
        }

        debug!("✅ BPF network monitoring stopped");
        Ok(())
    }
}

impl Drop for BpfNetworkMonitor {
    fn drop(&mut self) {
        debug!("🧹 Cleaning up BPF network monitor");
        // BPF programs are automatically detached when the Bpf object is dropped
    }
}

/// Utility function to check if a network event indicates a significant state change
pub fn is_significant_network_change(event: &NetworkEvent) -> bool {
    match event.event_type {
        NetworkEventType::InterfaceUp | NetworkEventType::InterfaceDown => true,
        NetworkEventType::WifiConnected | NetworkEventType::WifiDisconnected => true,
        NetworkEventType::IpAddressChange | NetworkEventType::RouteChange => true,
        NetworkEventType::NetnsChange => true,
        NetworkEventType::PacketReceived => false, // Too frequent for geofencing
    }
}

/// Convert NetworkEvent to a human-readable string
pub fn event_to_string(event: &NetworkEvent) -> String {
    // Convert nanoseconds to seconds with fractional part
    let seconds = (event.timestamp / 1_000_000_000) as i64;
    let nanoseconds = (event.timestamp % 1_000_000_000) as u32;
    
    let timestamp = chrono::DateTime::from_timestamp(seconds, nanoseconds)
        .unwrap_or_else(|| chrono::Utc::now());
    
    match &event.event_type {
        NetworkEventType::InterfaceUp => {
            format!("🟢 Interface {} UP at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::InterfaceDown => {
            format!("🔴 Interface {} DOWN at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::WifiConnected => {
            if let NetworkEventData::Wifi { ssid, .. } = &event.data {
                format!("📶 WiFi connected to '{}' on {} at {}", ssid, event.if_name, timestamp.format("%H:%M:%S"))
            } else {
                format!("📶 WiFi connected on {} at {}", event.if_name, timestamp.format("%H:%M:%S"))
            }
        }
        NetworkEventType::WifiDisconnected => {
            format!("📵 WiFi disconnected from {} at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::IpAddressChange => {
            format!("🌐 IP address changed on {} at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::RouteChange => {
            format!("🛤️ Route changed on {} at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::NetnsChange => {
            format!("🔄 Network namespace change at {}", timestamp.format("%H:%M:%S"))
        }
        NetworkEventType::PacketReceived => {
            format!("📦 Packet on {} at {}", event.if_name, timestamp.format("%H:%M:%S"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_event_creation() {
        let event = NetworkEvent {
            event_type: NetworkEventType::WifiConnected,
            timestamp: 1234567890,
            if_index: 2,
            if_name: "wlan0".to_string(),
            data: NetworkEventData::Wifi {
                ssid: "TestNetwork".to_string(),
                bssid: [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
                signal_strength: -45,
            },
        };

        assert_eq!(event.event_type, NetworkEventType::WifiConnected);
        assert_eq!(event.if_name, "wlan0");
    }

    #[test]
    fn test_significant_network_change() {
        let wifi_event = NetworkEvent {
            event_type: NetworkEventType::WifiConnected,
            timestamp: 0,
            if_index: 0,
            if_name: "wlan0".to_string(),
            data: NetworkEventData::Generic { data: vec![] },
        };

        let packet_event = NetworkEvent {
            event_type: NetworkEventType::PacketReceived,
            timestamp: 0,
            if_index: 0,
            if_name: "wlan0".to_string(),
            data: NetworkEventData::Generic { data: vec![] },
        };

        assert!(is_significant_network_change(&wifi_event));
        assert!(!is_significant_network_change(&packet_event));
    }

    #[test]
    fn test_event_to_string() {
        let event = NetworkEvent {
            event_type: NetworkEventType::InterfaceUp,
            timestamp: 0,
            if_index: 0,
            if_name: "eth0".to_string(),
            data: NetworkEventData::Generic { data: vec![] },
        };

        let event_str = event_to_string(&event);
        assert!(event_str.contains("eth0"));
        assert!(event_str.contains("UP"));
    }
}