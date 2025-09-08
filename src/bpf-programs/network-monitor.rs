//! eBPF program for network interface and connection monitoring
//!
//! This program runs in kernel space to monitor network events and send
//! them to userspace for processing by the network-dmenu daemon.

#![no_std]
#![no_main]

use aya_bpf::{
    bindings::{TC_ACT_PIPE, __sk_buff},
    helpers::{bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::{classifier, tracepoint, xdp},
    maps::{PerfEventArray, HashMap},
    programs::{TcContext, TracePointContext, XdpContext},
    BpfContext,
};
use aya_log_ebpf::info;
use core::mem;
use network_monitor_common::{NetworkEvent, NetworkEventType, NetworkEventData};

/// Map for sending network events to userspace
#[map(name = "NETWORK_EVENTS")]
static mut NETWORK_EVENTS: PerfEventArray<NetworkEvent> = PerfEventArray::with_max_entries(1024, 0);

/// Map to track interface states
#[map(name = "INTERFACE_STATES")]
static mut INTERFACE_STATES: HashMap<u32, u8> = HashMap::with_max_entries(256, 0);

/// XDP program for packet-level network monitoring
#[xdp(name = "packet_monitor")]
pub fn packet_monitor(ctx: XdpContext) -> u32 {
    match try_packet_monitor(ctx) {
        Ok(ret) => ret,
        Err(_) => aya_bpf::bindings::xdp_action::XDP_ABORTED,
    }
}

fn try_packet_monitor(ctx: XdpContext) -> Result<u32, u32> {
    // Get packet data
    let data = ctx.data();
    let data_end = ctx.data_end();

    if data + mem::size_of::<ethhdr>() > data_end {
        return Ok(aya_bpf::bindings::xdp_action::XDP_PASS);
    }

    // Parse Ethernet header
    let ethhdr = unsafe { &*(data as *const ethhdr) };
    
    // For now, just pass all packets - in a full implementation,
    // we could analyze specific packet types for network state detection
    
    // Create a packet received event (rate-limited)
    let event = NetworkEvent {
        event_type: NetworkEventType::PacketReceived,
        timestamp: unsafe { bpf_ktime_get_ns() },
        if_index: ctx.ingress_ifindex(),
        if_name: [0; 16], // Would be filled by userspace
        data: NetworkEventData::Packet {
            protocol: u16::from_be(ethhdr.h_proto) as u8,
            length: (data_end - data) as u32,
        },
    };

    // Send event to userspace (with rate limiting)
    if should_send_packet_event() {
        unsafe {
            NETWORK_EVENTS.output(&ctx, &event, 0);
        }
    }

    Ok(aya_bpf::bindings::xdp_action::XDP_PASS)
}

/// Tracepoint for network device receive events
#[tracepoint(name = "netif_receive_skb")]
pub fn netif_receive_skb(ctx: TracePointContext) -> u32 {
    match try_netif_receive_skb(ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

fn try_netif_receive_skb(_ctx: TracePointContext) -> Result<u32, u32> {
    // This tracepoint is called when a network interface receives a packet
    // We can use this to detect network activity and interface state changes
    
    // In a full implementation, we would:
    // 1. Extract interface information from the tracepoint args
    // 2. Track interface activity
    // 3. Detect interface state changes
    // 4. Send appropriate events to userspace

    info!(&_ctx, "netif_receive_skb tracepoint triggered");
    Ok(0)
}

/// Tracepoint for network device queue events  
#[tracepoint(name = "net_dev_queue")]
pub fn net_dev_queue(ctx: TracePointContext) -> u32 {
    match try_net_dev_queue(ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

fn try_net_dev_queue(_ctx: TracePointContext) -> Result<u32, u32> {
    // This tracepoint is called when packets are queued for transmission
    // Can be used to detect outbound network activity
    
    info!(&_ctx, "net_dev_queue tracepoint triggered");
    Ok(0)
}

/// TC classifier for monitoring network traffic at the interface level
#[classifier(name = "network_classifier")]
pub fn network_classifier(ctx: TcContext) -> i32 {
    match try_network_classifier(ctx) {
        Ok(ret) => ret,
        Err(_) => TC_ACT_PIPE,
    }
}

fn try_network_classifier(_ctx: TcContext) -> Result<i32, u32> {
    // TC classifier can monitor both ingress and egress traffic
    // This gives us more detailed network flow information
    
    Ok(TC_ACT_PIPE)
}

/// Rate limiting for packet events to avoid overwhelming userspace
fn should_send_packet_event() -> bool {
    // Simple rate limiting - only send every 1000th packet event
    static mut PACKET_COUNTER: u64 = 0;
    unsafe {
        PACKET_COUNTER += 1;
        PACKET_COUNTER % 1000 == 0
    }
}

/// Simple Ethernet header structure
#[repr(C)]
struct ethhdr {
    h_dest: [u8; 6],
    h_source: [u8; 6], 
    h_proto: u16,
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}