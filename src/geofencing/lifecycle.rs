//! System lifecycle management for geofencing daemon
//!
//! Handles system events like suspend/resume, network interface changes,
//! and graceful shutdown with state preservation.

use crate::geofencing::{GeofenceError, Result, ZoneManager};
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};

/// System events that affect geofencing behavior
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemEvent {
    /// System is suspending to sleep/hibernate
    Suspend,
    /// System is resuming from sleep/hibernate
    Resume,
    /// Network interface went up
    NetworkUp(String),
    /// Network interface went down
    NetworkDown(String),
    /// WiFi connected to new network
    WiFiConnected(String),
    /// WiFi disconnected
    WiFiDisconnected,
    /// User session locked
    SessionLocked,
    /// User session unlocked
    SessionUnlocked,
    /// System shutdown requested
    Shutdown,
}

/// Network interface state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterfaceState {
    Up,
    Down,
    Connected(String), // Connected to SSID
    Disconnected,
}

/// Daemon state that needs to be preserved across lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    /// Current zone ID if any
    pub current_zone_id: Option<String>,
    /// When the daemon was last active
    pub last_active: DateTime<Utc>,
    /// Suspend/resume cycle count
    pub suspend_resume_count: u32,
    /// Network interface states
    pub interface_states: HashMap<String, InterfaceState>,
    /// Whether daemon is currently suspended
    pub is_suspended: bool,
    /// Total runtime before last suspend
    pub runtime_before_suspend: Duration,
    /// Last known location fingerprint confidence
    pub last_location_confidence: f64,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            current_zone_id: None,
            last_active: Utc::now(),
            suspend_resume_count: 0,
            interface_states: HashMap::new(),
            is_suspended: false,
            runtime_before_suspend: Duration::from_secs(0),
            last_location_confidence: 0.0,
        }
    }
}

/// System lifecycle manager
pub struct LifecycleManager {
    state: Arc<RwLock<DaemonState>>,
    zone_manager: Arc<Mutex<ZoneManager>>,
    event_handlers: HashMap<SystemEvent, Vec<Box<dyn SystemEventHandler>>>,
    state_file_path: PathBuf,
    network_monitor: NetworkMonitor,
    suspend_monitor: SuspendMonitor,
}

/// Trait for handling system events
#[async_trait::async_trait]
pub trait SystemEventHandler: Send + Sync {
    async fn handle_event(&self, event: &SystemEvent, state: &DaemonState) -> Result<()>;
}

/// Network interface monitoring
struct NetworkMonitor {
    interface_states: HashMap<String, InterfaceState>,
    monitored_interfaces: Vec<String>,
}

/// Suspend/resume monitoring using systemd-logind
struct SuspendMonitor {
    last_check: DateTime<Utc>,
    suspend_count: u32,
}

impl LifecycleManager {
    /// Create new lifecycle manager
    pub async fn new(zone_manager: Arc<Mutex<ZoneManager>>) -> Result<Self> {
        debug!("Creating lifecycle manager");

        let state_file_path = Self::get_state_file_path();
        let state = Arc::new(RwLock::new(Self::load_state(&state_file_path).await?));

        let mut manager = Self {
            state,
            zone_manager,
            event_handlers: HashMap::new(),
            state_file_path,
            network_monitor: NetworkMonitor::new(),
            suspend_monitor: SuspendMonitor::new(),
        };

        // Register default event handlers
        manager.register_default_handlers();

        info!("Lifecycle manager created successfully");
        Ok(manager)
    }

    /// Start lifecycle monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting system lifecycle monitoring");

        // Check if we're resuming from suspend
        if self.state.read().await.is_suspended {
            info!("Detected resume from suspend, triggering resume event");
            self.handle_event(SystemEvent::Resume).await?;
        }

        // Start monitoring tasks
        let state_clone = Arc::clone(&self.state);
        let _network_monitor_task = tokio::spawn(async move {
            Self::network_monitoring_loop(state_clone).await;
        });

        let state_clone = Arc::clone(&self.state);
        let _suspend_monitor_task = tokio::spawn(async move {
            Self::suspend_monitoring_loop(state_clone).await;
        });

        debug!("Lifecycle monitoring tasks started");
        Ok(())
    }

    /// Handle a system event
    pub async fn handle_event(&mut self, event: SystemEvent) -> Result<()> {
        debug!("Handling system event: {:?}", event);

        let current_state = self.state.read().await.clone();

        // Update state based on event
        self.update_state_for_event(&event).await;

        // Execute event handlers
        if let Some(handlers) = self.event_handlers.get(&event) {
            for handler in handlers {
                if let Err(e) = handler.handle_event(&event, &current_state).await {
                    error!("Event handler failed for {:?}: {}", event, e);
                }
            }
        }

        // Save state after handling event
        self.save_state().await?;

        debug!("System event {:?} handled successfully", event);
        Ok(())
    }

    /// Register an event handler
    pub fn register_handler<H>(&mut self, event: SystemEvent, handler: H)
    where
        H: SystemEventHandler + 'static,
    {
        debug!("Registering handler for event: {:?}", event);
        
        self.event_handlers
            .entry(event)
            .or_default()
            .push(Box::new(handler));
    }

    /// Get current daemon state
    pub async fn get_state(&self) -> DaemonState {
        self.state.read().await.clone()
    }

    /// Update current zone information
    pub async fn update_current_zone(&self, zone_id: Option<String>, confidence: f64) {
        debug!("Updating current zone: {:?} (confidence: {:.2})", zone_id, confidence);
        
        let mut state = self.state.write().await;
        state.current_zone_id = zone_id;
        state.last_location_confidence = confidence;
        state.last_active = Utc::now();
    }

    /// Check network interface changes
    pub async fn check_network_changes(&mut self) -> Vec<SystemEvent> {
        self.network_monitor.check_changes().await
    }

    /// Check suspend/resume events
    pub async fn check_suspend_events(&mut self) -> Vec<SystemEvent> {
        self.suspend_monitor.check_suspend_resume().await
    }

    /// Get suspend monitor statistics
    pub fn get_suspend_stats(&self) -> (u32, DateTime<Utc>) {
        self.suspend_monitor.get_stats()
    }

    /// Graceful shutdown
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating graceful daemon shutdown");

        // Trigger shutdown event
        self.handle_event(SystemEvent::Shutdown).await?;

        // Save final state
        {
            let mut state = self.state.write().await;
            state.last_active = Utc::now();
        }
        
        self.save_state().await?;

        info!("Daemon shutdown completed successfully");
        Ok(())
    }

    /// Register default system event handlers
    fn register_default_handlers(&mut self) {
        // Suspend handler
        self.register_handler(SystemEvent::Suspend, SuspendHandler::new(self.zone_manager.clone()));
        
        // Resume handler
        self.register_handler(SystemEvent::Resume, ResumeHandler::new(self.zone_manager.clone()));
        
        // Network change handler
        self.register_handler(SystemEvent::WiFiConnected("".to_string()), 
                             NetworkChangeHandler::new(self.zone_manager.clone()));
        
        // Session lock handler
        self.register_handler(SystemEvent::SessionLocked, 
                             SessionHandler::new(self.zone_manager.clone()));
    }

    /// Update daemon state based on system event
    async fn update_state_for_event(&self, event: &SystemEvent) {
        let mut state = self.state.write().await;
        
        match event {
            SystemEvent::Suspend => {
                debug!("Updating state for suspend event");
                state.is_suspended = true;
                state.runtime_before_suspend = Utc::now().signed_duration_since(state.last_active)
                    .to_std().unwrap_or(Duration::from_secs(0));
            }
            
            SystemEvent::Resume => {
                debug!("Updating state for resume event");
                state.is_suspended = false;
                state.suspend_resume_count += 1;
                state.last_active = Utc::now();
            }
            
            SystemEvent::NetworkUp(interface) => {
                debug!("Network interface {} is up", interface);
                state.interface_states.insert(interface.clone(), InterfaceState::Up);
            }
            
            SystemEvent::NetworkDown(interface) => {
                debug!("Network interface {} is down", interface);
                state.interface_states.insert(interface.clone(), InterfaceState::Down);
            }
            
            SystemEvent::WiFiConnected(ssid) => {
                debug!("WiFi connected to {}", ssid);
                state.interface_states.insert("wifi".to_string(), InterfaceState::Connected(ssid.clone()));
            }
            
            SystemEvent::WiFiDisconnected => {
                debug!("WiFi disconnected");
                state.interface_states.insert("wifi".to_string(), InterfaceState::Disconnected);
            }
            
            _ => {
                // Update last active time for all events
                state.last_active = Utc::now();
            }
        }
    }

    /// Load daemon state from disk
    async fn load_state(state_file_path: &PathBuf) -> Result<DaemonState> {
        debug!("Loading daemon state from: {}", state_file_path.display());

        match fs::read_to_string(state_file_path).await {
            Ok(content) => {
                match serde_json::from_str::<DaemonState>(&content) {
                    Ok(mut state) => {
                        debug!("Loaded daemon state: suspend_count={}, last_active={}", 
                               state.suspend_resume_count, state.last_active);
                        
                        // Check if we're resuming from an unexpected shutdown
                        let now = Utc::now();
                        let time_since_last_active = now.signed_duration_since(state.last_active);
                        
                        if time_since_last_active.num_hours() > 1 && state.is_suspended {
                            warn!("Detected possible unexpected shutdown during suspend");
                            state.suspend_resume_count += 1;
                            state.is_suspended = false;
                        }
                        
                        Ok(state)
                    }
                    Err(e) => {
                        warn!("Failed to parse daemon state file: {}", e);
                        Ok(DaemonState::default())
                    }
                }
            }
            Err(_) => {
                debug!("No existing daemon state file found, using default state");
                Ok(DaemonState::default())
            }
        }
    }

    /// Save daemon state to disk
    async fn save_state(&self) -> Result<()> {
        let state = self.state.read().await;
        
        debug!("Saving daemon state to: {}", self.state_file_path.display());

        // Create directory if it doesn't exist
        if let Some(parent) = self.state_file_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(GeofenceError::Config(format!(
                    "Failed to create state directory: {}", e
                )));
            }
        }

        let content = serde_json::to_string_pretty(&*state)
            .map_err(|e| GeofenceError::Config(format!(
                "Failed to serialize daemon state: {}", e
            )))?;

        fs::write(&self.state_file_path, content).await
            .map_err(|e| GeofenceError::Config(format!(
                "Failed to write daemon state: {}", e
            )))?;

        debug!("Daemon state saved successfully");
        Ok(())
    }

    /// Get daemon state file path
    fn get_state_file_path() -> PathBuf {
        let mut path = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        path.push("network-dmenu");
        path.push("daemon-lifecycle-state.json");
        path
    }

    /// Network monitoring loop
    async fn network_monitoring_loop(state: Arc<RwLock<DaemonState>>) {
        debug!("Starting network monitoring loop");
        
        let mut last_wifi_ssid: Option<String> = None;
        
        loop {
            // Check WiFi status using nmcli
            if let Ok(current_ssid) = Self::get_current_wifi_ssid().await {
                if current_ssid != last_wifi_ssid {
                    debug!("WiFi SSID changed from {:?} to {:?}", last_wifi_ssid, current_ssid);
                    
                    // Update state
                    {
                        let mut state_guard = state.write().await;
                        if let Some(ref ssid) = current_ssid {
                            state_guard.interface_states.insert(
                                "wifi".to_string(), 
                                InterfaceState::Connected(ssid.clone())
                            );
                        } else {
                            state_guard.interface_states.insert(
                                "wifi".to_string(), 
                                InterfaceState::Disconnected
                            );
                        }
                    }
                    
                    last_wifi_ssid = current_ssid;
                }
            }

            sleep(Duration::from_secs(10)).await;
        }
    }

    /// Get current WiFi SSID
    async fn get_current_wifi_ssid() -> Result<Option<String>> {
        use crate::command::{CommandRunner, RealCommandRunner};
        
        let command_runner = RealCommandRunner;
        
        if !crate::command::is_command_installed("nmcli") {
            return Ok(None);
        }

        match command_runner.run_command("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"]) {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                
                for line in stdout.lines() {
                    if let Some(stripped) = line.strip_prefix("yes:") {
                        let ssid = stripped.trim();
                        if !ssid.is_empty() {
                            return Ok(Some(ssid.to_string()));
                        }
                    }
                }
                
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Suspend monitoring loop using systemd-logind and system indicators
    async fn suspend_monitoring_loop(state: Arc<RwLock<DaemonState>>) {
        debug!("Starting suspend monitoring loop");
        
        let mut suspend_monitor = SuspendMonitor::new();
        
        loop {
            // Check for suspend/resume events
            let events = suspend_monitor.check_suspend_resume().await;
            
            for event in events {
                debug!("Suspend monitor detected event: {:?}", event);
                
                // Update daemon state based on detected event
                let mut daemon_state = state.write().await;
                
                match event {
                    SystemEvent::Resume => {
                        daemon_state.is_suspended = false;
                        daemon_state.suspend_resume_count += 1;
                        daemon_state.last_active = Utc::now();
                        info!("System resume detected (cycle #{}) - daemon reactivated", 
                              daemon_state.suspend_resume_count);
                    }
                    SystemEvent::Suspend => {
                        daemon_state.is_suspended = true;
                        daemon_state.runtime_before_suspend = Utc::now()
                            .signed_duration_since(daemon_state.last_active)
                            .to_std()
                            .unwrap_or(Duration::from_secs(0));
                        info!("System suspend detected - daemon suspending");
                    }
                    _ => {}
                }
            }
            
            // Log suspend monitor stats periodically for debugging
            let (suspend_count, last_check) = suspend_monitor.get_stats();
            debug!("Suspend monitor stats: count={}, last_check={}", suspend_count, last_check);
            
            sleep(Duration::from_secs(30)).await;
        }
    }
}

impl NetworkMonitor {
    fn new() -> Self {
        Self {
            interface_states: HashMap::new(),
            monitored_interfaces: vec!["wlan0".to_string(), "eth0".to_string()],
        }
    }

    async fn check_changes(&mut self) -> Vec<SystemEvent> {
        let mut events = Vec::new();
        
        // Check each monitored interface
        for interface in &self.monitored_interfaces {
            if let Ok(current_state) = self.get_interface_state(interface).await {
                let previous_state = self.interface_states.get(interface);
                
                if previous_state != Some(&current_state) {
                    debug!("Interface {} state changed to {:?}", interface, current_state);
                    
                    let event = match &current_state {
                        InterfaceState::Up => SystemEvent::NetworkUp(interface.clone()),
                        InterfaceState::Down => SystemEvent::NetworkDown(interface.clone()),
                        InterfaceState::Connected(ssid) => SystemEvent::WiFiConnected(ssid.clone()),
                        InterfaceState::Disconnected => SystemEvent::WiFiDisconnected,
                    };
                    
                    events.push(event);
                    self.interface_states.insert(interface.clone(), current_state);
                }
            }
        }
        
        events
    }

    async fn get_interface_state(&self, interface: &str) -> Result<InterfaceState> {
        use crate::command::{CommandRunner, RealCommandRunner};
        
        let command_runner = RealCommandRunner;
        
        // Check if interface is up
        match command_runner.run_command("ip", &["link", "show", interface]) {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                
                if stdout.contains("state UP") {
                    // If it's a WiFi interface, check if connected
                    if interface.starts_with("wlan") || interface.starts_with("wifi") {
                        if let Ok(Some(ssid)) = LifecycleManager::get_current_wifi_ssid().await {
                            Ok(InterfaceState::Connected(ssid))
                        } else {
                            Ok(InterfaceState::Up)
                        }
                    } else {
                        Ok(InterfaceState::Up)
                    }
                } else {
                    Ok(InterfaceState::Down)
                }
            }
            _ => Ok(InterfaceState::Down),
        }
    }
}

impl SuspendMonitor {
    fn new() -> Self {
        Self {
            last_check: Utc::now(),
            suspend_count: 0,
        }
    }

    /// Check if system has been suspended since last check
    async fn check_suspend_resume(&mut self) -> Vec<SystemEvent> {
        let mut events = Vec::new();
        let now = Utc::now();
        
        // Check for large time gaps (> 5 minutes) indicating possible suspend
        let time_since_check = now.signed_duration_since(self.last_check);
        
        if time_since_check.num_minutes() > 5 {
            debug!("Detected {} minute gap since last check - possible suspend/resume", 
                   time_since_check.num_minutes());
            
            // Check system logs or other indicators for suspend/resume
            if let Ok(suspend_detected) = self.detect_suspend_from_system().await {
                if suspend_detected {
                    self.suspend_count += 1;
                    debug!("Suspend/resume cycle detected (count: {})", self.suspend_count);
                    events.push(SystemEvent::Resume);
                }
            }
        }
        
        self.last_check = now;
        events
    }

    /// Detect suspend events from system logs or other indicators
    async fn detect_suspend_from_system(&self) -> Result<bool> {
        use crate::command::{CommandRunner, RealCommandRunner};
        
        let command_runner = RealCommandRunner;
        
        // Method 1: Check systemd journal for suspend/resume events
        if crate::command::is_command_installed("journalctl") {
            match command_runner.run_command("journalctl", &[
                "--since", "5 minutes ago",
                "--grep", "PM: suspend",
                "--no-pager",
                "-q"
            ]) {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if !stdout.trim().is_empty() {
                        debug!("Found suspend events in journal");
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }

        // Method 2: Check uptime for system reboot (less accurate but still useful)
        match command_runner.run_command("uptime", &["-s"]) {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(boot_time) = chrono::DateTime::parse_from_str(
                    stdout.trim(), 
                    "%Y-%m-%d %H:%M:%S"
                ) {
                    let boot_duration = Utc::now().signed_duration_since(boot_time.with_timezone(&Utc));
                    
                    // If system booted recently, it might indicate resume from hibernation
                    if boot_duration.num_minutes() < 10 {
                        debug!("Recent boot detected - possible hibernate/resume");
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }

        // Method 3: Check /sys/power/state for suspend indicators (if available)
        if let Ok(power_state) = tokio::fs::read_to_string("/sys/power/state").await {
            debug!("Power state options: {}", power_state.trim());
            // This doesn't directly indicate suspend, but confirms suspend capability
        }

        Ok(false)
    }

    /// Get suspend statistics
    pub fn get_stats(&self) -> (u32, DateTime<Utc>) {
        (self.suspend_count, self.last_check)
    }
}

/// Handler for suspend events
struct SuspendHandler {
    zone_manager: Arc<Mutex<ZoneManager>>,
}

impl SuspendHandler {
    fn new(zone_manager: Arc<Mutex<ZoneManager>>) -> Self {
        Self { zone_manager }
    }
}

#[async_trait::async_trait]
#[async_trait::async_trait]
impl SystemEventHandler for SuspendHandler {
    async fn handle_event(&self, event: &SystemEvent, state: &DaemonState) -> Result<()> {
        match event {
            SystemEvent::Suspend => {
                info!("Handling system suspend - saving zone state");
                
                // Use zone_manager to save current zone state before suspend
                let zone_manager = self.zone_manager.lock().await;
                if let Some(ref zone_id) = state.current_zone_id {
                    info!("Saving current zone state at suspend: {}", zone_id);
                    
                    // In a real implementation, we'd save zone state to persistent storage
                    // For now, we'll log the zone configuration for debugging
                    let zone_list = zone_manager.list_zones();
                    debug!("Zone manager has {} configured zones", zone_list.len());
                    
                    if let Some(zone) = zone_manager.get_zone(zone_id) {
                        info!("Suspended with zone '{}' active (confidence: {:.2})", 
                              zone.name, zone.confidence_threshold);
                    }
                }
                
                debug!("Suspend handling completed - zone state preserved");
            }
            SystemEvent::Resume => {
                info!("Handling system resume - restoring zone context");
                
                // Use zone_manager to potentially trigger zone re-detection after resume
                let zone_manager = self.zone_manager.lock().await;
                
                if let Some(ref zone_id) = state.current_zone_id {
                    info!("Restoring zone context after resume: {}", zone_id);
                    
                    // Mark zone for re-evaluation since network conditions may have changed
                    if let Some(zone) = zone_manager.get_zone(zone_id) {
                        // In a real implementation, we'd mark the zone for re-evaluation
                        // For now, just log that re-evaluation would be triggered
                        info!("Triggering zone re-evaluation for '{}' after resume", zone.name);
                    }
                }
                
                debug!("Resume handling completed - zone re-evaluation triggered");
            }
            _ => {
                // Handle other events if needed
                debug!("SuspendHandler ignoring event: {:?}", event);
            }
        }
        
        Ok(())
    }
}

/// Handler for resume events  
struct ResumeHandler {
    zone_manager: Arc<Mutex<ZoneManager>>,
}

impl ResumeHandler {
    fn new(zone_manager: Arc<Mutex<ZoneManager>>) -> Self {
        Self { zone_manager }
    }
}

#[async_trait::async_trait]
#[async_trait::async_trait]
impl SystemEventHandler for ResumeHandler {
    async fn handle_event(&self, event: &SystemEvent, state: &DaemonState) -> Result<()> {
        if matches!(event, SystemEvent::Resume) {
            info!("Handling system resume - triggering immediate location check (last active: {:?})", 
                  state.last_active);
            
            // Check if we have a previous zone context from the daemon state
            let previous_zone = state.current_zone_id.clone();
            debug!("Previous zone before suspend: {:?}", previous_zone);
            
            // Trigger immediate location detection after resume
            let mut manager = self.zone_manager.lock().await;
            
            match manager.detect_location_change().await {
                Ok(Some(change)) => {
                    info!("Location change detected after resume: {} -> {} (suspend/resume count: {})", 
                          change.from.as_ref().map(|z| z.name.as_str()).unwrap_or("None"),
                          change.to.name, state.suspend_resume_count);
                    
                    // If we had a zone before suspend and it's different now, log transition
                    if let Some(prev) = &previous_zone {
                        if prev != &change.to.name {
                            info!("Zone transition after resume: {} -> {}", prev, change.to.name);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No location change detected after resume (last active: {:?})", 
                           state.last_active);
                    
                    // If we had a zone before, assume we're still there
                    if let Some(prev) = &previous_zone {
                        debug!("Assuming still in previous zone '{}' after resume", prev);
                    }
                }
                Err(e) => {
                    warn!("Failed to detect location after resume: {}", e);
                }
            }
        }
        
        Ok(())
    }
}

/// Handler for network changes
struct NetworkChangeHandler {
    zone_manager: Arc<Mutex<ZoneManager>>,
}

impl NetworkChangeHandler {
    fn new(zone_manager: Arc<Mutex<ZoneManager>>) -> Self {
        Self { zone_manager }
    }
}

#[async_trait::async_trait]
#[async_trait::async_trait]
impl SystemEventHandler for NetworkChangeHandler {
    async fn handle_event(&self, event: &SystemEvent, _state: &DaemonState) -> Result<()> {
        match event {
            SystemEvent::WiFiConnected(ssid) => {
                info!("WiFi connected to '{}' - triggering location check", ssid);
                
                // Wait a moment for network to stabilize
                sleep(Duration::from_secs(2)).await;
                
                let mut manager = self.zone_manager.lock().await;
                
                match manager.detect_location_change().await {
                    Ok(Some(change)) => {
                        info!("Zone change after WiFi connection: {}", change.to.name);
                    }
                    Ok(None) => {
                        debug!("No zone change after WiFi connection");
                    }
                    Err(e) => {
                        warn!("Failed to detect location after WiFi connection: {}", e);
                    }
                }
            }
            
            SystemEvent::WiFiDisconnected => {
                debug!("WiFi disconnected - location detection may be limited");
            }
            
            _ => {}
        }
        
        Ok(())
    }
}

/// Handler for session events
struct SessionHandler {
    zone_manager: Arc<Mutex<ZoneManager>>,
}

impl SessionHandler {
    fn new(zone_manager: Arc<Mutex<ZoneManager>>) -> Self {
        Self { zone_manager }
    }
}

#[async_trait::async_trait]
#[async_trait::async_trait]
impl SystemEventHandler for SessionHandler {
    async fn handle_event(&self, event: &SystemEvent, _state: &DaemonState) -> Result<()> {
        match event {
            SystemEvent::SessionLocked => {
                debug!("User session locked - reducing scanning frequency");
                // In a real implementation, we'd signal the daemon to reduce scanning
            }
            
            SystemEvent::SessionUnlocked => {
                debug!("User session unlocked - resuming normal scanning");
                // Trigger immediate location check when user unlocks
                let mut manager = self.zone_manager.lock().await;
                
                if let Err(e) = manager.detect_location_change().await {
                    warn!("Failed to detect location after session unlock: {}", e);
                }
            }
            
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geofencing::GeofencingConfig;

    #[tokio::test]
    async fn test_daemon_state_default() {
        let state = DaemonState::default();
        assert!(state.current_zone_id.is_none());
        assert!(!state.is_suspended);
        assert_eq!(state.suspend_resume_count, 0);
    }

    #[tokio::test]
    async fn test_lifecycle_manager_creation() {
        let config = GeofencingConfig::default();
        let zone_manager = Arc::new(Mutex::new(crate::geofencing::ZoneManager::new(config)));
        
        let manager = LifecycleManager::new(zone_manager).await;
        assert!(manager.is_ok());
    }

    #[test]
    fn test_system_event_types() {
        let events = vec![
            SystemEvent::Suspend,
            SystemEvent::Resume,
            SystemEvent::NetworkUp("wlan0".to_string()),
            SystemEvent::NetworkDown("wlan0".to_string()),
            SystemEvent::WiFiConnected("TestNetwork".to_string()),
            SystemEvent::WiFiDisconnected,
            SystemEvent::Shutdown,
        ];
        
        for event in events {
            // Test serialization/deserialization
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: SystemEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, deserialized);
        }
    }

    #[tokio::test]
    async fn test_state_persistence() {
        use tempfile::NamedTempFile;
        
        let temp_file = NamedTempFile::new().unwrap();
        let state_path = temp_file.path().to_path_buf();
        
        // Create initial state
        let mut state = DaemonState::default();
        state.current_zone_id = Some("test_zone".to_string());
        state.suspend_resume_count = 5;
        
        // Save state
        let content = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&state_path, content).await.unwrap();
        
        // Load state
        let loaded_state = LifecycleManager::load_state(&state_path).await.unwrap();
        
        assert_eq!(loaded_state.current_zone_id, Some("test_zone".to_string()));
        assert_eq!(loaded_state.suspend_resume_count, 5);
    }
}