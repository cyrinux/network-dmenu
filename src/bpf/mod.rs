//! BPF integration for real-time network monitoring
//!
//! This module provides eBPF-based network monitoring capabilities using Aya,
//! replacing polling-based approaches with event-driven network state detection.

#[cfg(feature = "bpf")]
pub mod network_events;

#[cfg(feature = "bpf")]
pub use network_events::*;

/// BPF error types
#[cfg(feature = "bpf")]
#[derive(Debug, thiserror::Error)]
pub enum BpfError {
    #[error("Failed to load BPF program: {0}")]
    ProgramLoad(#[from] aya::EbpfError),
    
    #[error("Failed to attach BPF program: {0}")]
    ProgramAttach(String),
    
    #[error("Network event processing error: {0}")]
    EventProcessing(String),
    
    #[error("Permission denied - BPF requires CAP_SYS_ADMIN")]
    PermissionDenied,
    
    #[error("BPF feature not available")]
    NotAvailable,
}

/// Result type for BPF operations
#[cfg(feature = "bpf")]
pub type BpfResult<T> = Result<T, BpfError>;

/// Check if BPF functionality is available and properly configured
#[cfg(feature = "bpf")]
pub fn is_bpf_available() -> bool {
    // Check if running as root or with appropriate capabilities
    unsafe {
        libc::getuid() == 0 || 
        // Could also check for CAP_SYS_ADMIN capability more precisely
        std::env::var("BPF_FORCE_ENABLE").is_ok()
    }
}

/// Stub implementations for when BPF feature is disabled
#[cfg(not(feature = "bpf"))]
pub fn is_bpf_available() -> bool {
    false
}

#[cfg(not(feature = "bpf"))]
#[derive(Debug)]
pub struct BpfError;

#[cfg(not(feature = "bpf"))]
impl std::fmt::Display for BpfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BPF feature not compiled")
    }
}

#[cfg(not(feature = "bpf"))]
impl std::error::Error for BpfError {}

#[cfg(not(feature = "bpf"))]
pub type BpfResult<T> = Result<T, BpfError>;