//! Network Menu Library
//! 
//! A refactored network management system with improved error handling,
//! service architecture, and async support.

pub mod command;
pub mod config;
pub mod constants;
pub mod errors;
pub mod parsers;
pub mod services;
pub mod types;
pub mod utils;

// Re-export commonly used types
pub use errors::{NetworkMenuError, Result};
pub use types::{Config, ActionContext};
pub use services::{ActionType, NetworkServiceManager};
pub use command::{CommandRunner, RealCommandRunner};

// Legacy modules for backward compatibility
pub mod bluetooth;
pub mod iwd;
pub mod networkmanager;
pub mod tailscale;

use tracing::{debug, info};

/// Initialize the network menu system
pub async fn initialize() -> Result<NetworkServiceManager> {
    info!("Initializing network-dmenu system");
    
    let mut service_manager = NetworkServiceManager::new();
    
    // Add core services
    service_manager = service_manager
        .add_service(Box::new(services::wifi_service::WifiService::new()))
        .add_service(Box::new(services::vpn_service::VpnService::new()))
        .add_service(Box::new(services::tailscale_service::TailscaleService::new()))
        .add_service(Box::new(services::bluetooth_service::BluetoothService::new()))
        .add_service(Box::new(services::system_service::SystemService::new()));
    
    service_manager.initialize().await?;
    
    debug!("Network menu system initialized successfully");
    Ok(service_manager)
}

/// Get version information
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get application name
pub fn app_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}