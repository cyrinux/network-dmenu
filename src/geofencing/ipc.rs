//! Inter-process communication for daemon-client communication
//! 
//! Provides Unix domain socket communication between the daemon and client
//! for geofencing operations.

use super::{GeofenceError, GeofenceZone, LocationFingerprint, Result, ZoneActions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Socket path for daemon communication
const DAEMON_SOCKET_PATH: &str = "/tmp/network-dmenu-daemon.sock";

/// Commands that can be sent to the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonCommand {
    /// Get current location fingerprint
    GetCurrentLocation,
    /// Get currently active geofence zone
    GetActiveZone,
    /// List all configured zones
    ListZones,
    /// Create a new geofence zone
    CreateZone {
        name: String,
        actions: ZoneActions,
    },
    /// Remove a geofence zone
    RemoveZone {
        zone_id: String,
    },
    /// Manually activate a zone
    ActivateZone {
        zone_id: String,
    },
    /// Execute specific actions
    ExecuteActions {
        actions: ZoneActions,
    },
    /// Get daemon status and statistics
    GetStatus,
    /// Shutdown the daemon gracefully
    Shutdown,
}

/// Responses from the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Current location fingerprint
    LocationUpdate {
        fingerprint: LocationFingerprint,
    },
    /// Zone change notification
    ZoneChanged {
        from_zone_id: Option<String>,
        to_zone: GeofenceZone,
        confidence: f64,
    },
    /// List of zones
    ZoneList {
        zones: Vec<GeofenceZone>,
    },
    /// Currently active zone
    ActiveZone {
        zone: Option<GeofenceZone>,
    },
    /// Zone creation result
    ZoneCreated {
        zone: GeofenceZone,
    },
    /// Daemon status information
    Status {
        status: DaemonStatus,
    },
    /// Simple success response
    Success,
    /// Error response
    Error {
        message: String,
    },
}

/// Daemon status information
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Whether the daemon is actively monitoring
    pub monitoring: bool,
    /// Number of configured zones
    pub zone_count: usize,
    /// Current active zone ID
    pub active_zone_id: Option<String>,
    /// Last scan timestamp
    pub last_scan: Option<chrono::DateTime<chrono::Utc>>,
    /// Total zone changes detected
    pub total_zone_changes: u32,
    /// Daemon uptime in seconds
    pub uptime_seconds: u64,
}

/// Client for communicating with the daemon
pub struct DaemonClient {
    socket_path: PathBuf,
}

impl DaemonClient {
    /// Create new daemon client
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(DAEMON_SOCKET_PATH),
        }
    }
    
    /// Check if daemon is running
    pub fn is_daemon_running(&self) -> bool {
        self.socket_path.exists()
    }
    
    /// Send command to daemon and receive response
    pub async fn send_command(&self, command: DaemonCommand) -> Result<DaemonResponse> {
        if !self.is_daemon_running() {
            return Err(GeofenceError::Ipc(
                "Daemon is not running".to_string()
            ));
        }
        
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| GeofenceError::Ipc(format!("Failed to connect to daemon: {}", e)))?;
        
        // Serialize and send command
        let command_json = serde_json::to_string(&command)
            .map_err(|e| GeofenceError::Ipc(format!("Failed to serialize command: {}", e)))?;
            
        let message = format!("{}\n", command_json);
        stream.write_all(message.as_bytes())
            .await
            .map_err(|e| GeofenceError::Ipc(format!("Failed to send command: {}", e)))?;
        
        // Read response
        let mut buffer = Vec::new();
        let mut temp_buffer = [0; 1024];
        
        loop {
            let n = stream.read(&mut temp_buffer)
                .await
                .map_err(|e| GeofenceError::Ipc(format!("Failed to read response: {}", e)))?;
                
            if n == 0 {
                break;
            }
            
            buffer.extend_from_slice(&temp_buffer[..n]);
            
            // Check if we have a complete message (ends with newline)
            if buffer.ends_with(&[b'\n']) {
                break;
            }
        }
        
        // Parse response
        let response_str = String::from_utf8(buffer)
            .map_err(|e| GeofenceError::Ipc(format!("Invalid UTF-8 in response: {}", e)))?;
            
        let response: DaemonResponse = serde_json::from_str(response_str.trim())
            .map_err(|e| GeofenceError::Ipc(format!("Failed to parse response: {}", e)))?;
            
        Ok(response)
    }
    
    /// Convenience method to get current location
    pub async fn get_current_location(&self) -> Result<LocationFingerprint> {
        match self.send_command(DaemonCommand::GetCurrentLocation).await? {
            DaemonResponse::LocationUpdate { fingerprint } => Ok(fingerprint),
            DaemonResponse::Error { message } => Err(GeofenceError::Ipc(message)),
            _ => Err(GeofenceError::Ipc("Unexpected response".to_string())),
        }
    }
    
    /// Convenience method to get active zone
    pub async fn get_active_zone(&self) -> Result<Option<GeofenceZone>> {
        match self.send_command(DaemonCommand::GetActiveZone).await? {
            DaemonResponse::ActiveZone { zone } => Ok(zone),
            DaemonResponse::Error { message } => Err(GeofenceError::Ipc(message)),
            _ => Err(GeofenceError::Ipc("Unexpected response".to_string())),
        }
    }
    
    /// Convenience method to list zones
    pub async fn list_zones(&self) -> Result<Vec<GeofenceZone>> {
        match self.send_command(DaemonCommand::ListZones).await? {
            DaemonResponse::ZoneList { zones } => Ok(zones),
            DaemonResponse::Error { message } => Err(GeofenceError::Ipc(message)),
            _ => Err(GeofenceError::Ipc("Unexpected response".to_string())),
        }
    }
    
    /// Convenience method to create zone
    pub async fn create_zone(&self, name: String, actions: ZoneActions) -> Result<GeofenceZone> {
        match self.send_command(DaemonCommand::CreateZone { name, actions }).await? {
            DaemonResponse::ZoneCreated { zone } => Ok(zone),
            DaemonResponse::Error { message } => Err(GeofenceError::Ipc(message)),
            _ => Err(GeofenceError::Ipc("Unexpected response".to_string())),
        }
    }
    
    /// Convenience method to get daemon status
    pub async fn get_status(&self) -> Result<DaemonStatus> {
        match self.send_command(DaemonCommand::GetStatus).await? {
            DaemonResponse::Status { status } => Ok(status),
            DaemonResponse::Error { message } => Err(GeofenceError::Ipc(message)),
            _ => Err(GeofenceError::Ipc("Unexpected response".to_string())),
        }
    }
}

/// IPC server for the daemon
pub struct DaemonIpcServer {
    listener: UnixListener,
}

impl DaemonIpcServer {
    /// Create new IPC server
    pub async fn new() -> Result<Self> {
        // Remove existing socket if it exists
        let socket_path = PathBuf::from(DAEMON_SOCKET_PATH);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .map_err(|e| GeofenceError::Ipc(format!("Failed to remove old socket: {}", e)))?;
        }
        
        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| GeofenceError::Ipc(format!("Failed to bind socket: {}", e)))?;
            
        Ok(Self { listener })
    }
    
    /// Accept incoming connections and handle commands
    pub async fn handle_connections<F, Fut>(&mut self, command_handler: F) -> Result<()>
    where
        F: Fn(DaemonCommand) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = DaemonResponse> + Send,
    {
        let handler = std::sync::Arc::new(command_handler);
        
        loop {
            match self.listener.accept().await {
                Ok((mut stream, _)) => {
                    let handler_clone = handler.clone();
                    // Handle connection in a separate task to avoid blocking
                    tokio::spawn(async move {
                        if let Err(e) = handle_client_connection(&mut stream, handler_clone).await {
                            eprintln!("Error handling client connection: {}", e);
                        }
                    });
                },
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    }
}

/// Handle a single client connection
async fn handle_client_connection<F, Fut>(
    stream: &mut UnixStream,
    command_handler: std::sync::Arc<F>,
) -> Result<()>
where
    F: Fn(DaemonCommand) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = DaemonResponse> + Send,
{
    // Read command
    let mut buffer = Vec::new();
    let mut temp_buffer = [0; 1024];
    
    loop {
        let n = stream.read(&mut temp_buffer)
            .await
            .map_err(|e| GeofenceError::Ipc(format!("Failed to read command: {}", e)))?;
            
        if n == 0 {
            break;
        }
        
        buffer.extend_from_slice(&temp_buffer[..n]);
        
        // Check if we have a complete message
        if buffer.ends_with(&[b'\n']) {
            break;
        }
    }
    
    let command_str = String::from_utf8(buffer)
        .map_err(|e| GeofenceError::Ipc(format!("Invalid UTF-8 in command: {}", e)))?;
        
    // Parse command
    let command: DaemonCommand = match serde_json::from_str(command_str.trim()) {
        Ok(cmd) => cmd,
        Err(e) => {
            // Send error response
            let error_response = DaemonResponse::Error {
                message: format!("Failed to parse command: {}", e),
            };
            send_response(stream, error_response).await?;
            return Ok(());
        }
    };
    
    // Handle command
    let response = command_handler(command).await;
    
    // Send response
    send_response(stream, response).await?;
    
    Ok(())
}

/// Send response to client
async fn send_response(stream: &mut UnixStream, response: DaemonResponse) -> Result<()> {
    let response_json = serde_json::to_string(&response)
        .map_err(|e| GeofenceError::Ipc(format!("Failed to serialize response: {}", e)))?;
        
    let message = format!("{}\n", response_json);
    stream.write_all(message.as_bytes())
        .await
        .map_err(|e| GeofenceError::Ipc(format!("Failed to send response: {}", e)))?;
        
    Ok(())
}

impl Drop for DaemonIpcServer {
    fn drop(&mut self) {
        // Clean up socket file
        let _ = std::fs::remove_file(DAEMON_SOCKET_PATH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_daemon_client_creation() {
        let client = DaemonClient::new();
        assert_eq!(client.socket_path.to_str().unwrap(), DAEMON_SOCKET_PATH);
    }

    #[tokio::test]
    async fn test_daemon_running_check() {
        let client = DaemonClient::new();
        // Should be false initially (no daemon running in test)
        assert!(!client.is_daemon_running());
    }

    #[test]
    fn test_command_serialization() {
        let command = DaemonCommand::GetCurrentLocation;
        let serialized = serde_json::to_string(&command).unwrap();
        let deserialized: DaemonCommand = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            DaemonCommand::GetCurrentLocation => {}, // Success
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let response = DaemonResponse::Success;
        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: DaemonResponse = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            DaemonResponse::Success => {}, // Success
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_daemon_status_creation() {
        let status = DaemonStatus {
            monitoring: true,
            zone_count: 3,
            active_zone_id: Some("home".to_string()),
            last_scan: Some(chrono::Utc::now()),
            total_zone_changes: 5,
            uptime_seconds: 3600,
        };
        
        assert!(status.monitoring);
        assert_eq!(status.zone_count, 3);
        assert!(status.active_zone_id.is_some());
    }
}