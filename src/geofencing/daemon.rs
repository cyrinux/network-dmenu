//! Geofencing daemon for background location monitoring
//!
//! Runs in the background to continuously monitor location changes
//! and automatically execute zone-based actions.

use super::{
    ipc::{DaemonCommand, DaemonIpcServer, DaemonResponse, DaemonStatus},
    zones::ZoneManager,
    GeofencingConfig, LocationChange, Result, ZoneActions,
};

#[cfg(feature = "ml")]
use crate::ml_integration::MlManager;
use chrono::Utc;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
#[cfg(not(feature = "ml"))]
use tokio::time::interval;

// Import network functions from the main codebase
use crate::command::{CommandRunner, RealCommandRunner};

/// Main geofencing daemon
pub struct GeofencingDaemon {
    zone_manager: Arc<Mutex<ZoneManager>>,
    config: GeofencingConfig,
    status: Arc<RwLock<DaemonStatusData>>,
    should_shutdown: Arc<RwLock<bool>>,
    #[cfg(feature = "ml")]
    ml_manager: Arc<Mutex<MlManager>>,
}

/// Internal daemon status data
#[derive(Debug)]
struct DaemonStatusData {
    monitoring: bool,
    last_scan: Option<chrono::DateTime<chrono::Utc>>,
    total_zone_changes: u32,
    startup_time: Instant,
    current_zone_id: Option<String>,
    #[cfg(feature = "ml")]
    ml_suggestions_generated: u32,
    #[cfg(feature = "ml")]
    ml_auto_suggestions_processed: u32,
    #[cfg(feature = "ml")]
    adaptive_scan_interval: Duration,
    #[cfg(feature = "ml")]
    last_ml_update: Option<chrono::DateTime<chrono::Utc>>,
}

impl GeofencingDaemon {
    /// Create new geofencing daemon
    pub fn new(config: GeofencingConfig) -> Self {
        debug!("Creating new geofencing daemon with config: enabled={}, privacy_mode={:?}, scan_interval={}s",
            config.enabled, config.privacy_mode, config.scan_interval_seconds);

        let zone_manager = Arc::new(Mutex::new(ZoneManager::new(config.clone())));

        #[cfg(feature = "ml")]
        let ml_manager = Arc::new(Mutex::new(MlManager::new()));

        let status = Arc::new(RwLock::new(DaemonStatusData {
            monitoring: false,
            last_scan: None,
            total_zone_changes: 0,
            startup_time: Instant::now(),
            current_zone_id: None,
            #[cfg(feature = "ml")]
            ml_suggestions_generated: 0,
            #[cfg(feature = "ml")]
            ml_auto_suggestions_processed: 0,
            #[cfg(feature = "ml")]
            adaptive_scan_interval: Duration::from_secs(config.scan_interval_seconds),
            #[cfg(feature = "ml")]
            last_ml_update: None,
        }));

        debug!("Geofencing daemon created successfully");
        Self {
            zone_manager,
            config,
            status,
            should_shutdown: Arc::new(RwLock::new(false)),
            #[cfg(feature = "ml")]
            ml_manager,
        }
    }

    /// Start the daemon
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting geofencing daemon");
        debug!(
            "Daemon configuration: scan_interval={}s, confidence_threshold={}, notifications={}",
            self.config.scan_interval_seconds,
            self.config.confidence_threshold,
            self.config.notifications
        );

        // Update status
        {
            let mut status = self.status.write().await;
            status.monitoring = true;
            debug!("Daemon monitoring status set to true");
        }

        // Start IPC server
        debug!("Starting IPC server");
        let mut ipc_server = DaemonIpcServer::new().await?;
        debug!("IPC server started successfully");

        // Clone references for tasks
        debug!("Setting up daemon tasks");
        let zone_manager = Arc::clone(&self.zone_manager);
        let status = Arc::clone(&self.status);
        let should_shutdown = Arc::clone(&self.should_shutdown);
        let scan_interval = Duration::from_secs(self.config.scan_interval_seconds);
        #[cfg(feature = "ml")]
        let ml_manager = Arc::clone(&self.ml_manager);
        debug!("Scan interval set to {:?}", scan_interval);

        // Start location monitoring task
        let monitor_task = {
            let zone_manager = Arc::clone(&zone_manager);
            let status = Arc::clone(&status);
            let should_shutdown = Arc::clone(&should_shutdown);
            #[cfg(feature = "ml")]
            let ml_manager = Arc::clone(&ml_manager);

            tokio::spawn(async move {
                #[cfg(feature = "ml")]
                {
                    Self::enhanced_location_monitoring_loop(
                        zone_manager,
                        status,
                        should_shutdown,
                        ml_manager,
                        scan_interval,
                    )
                    .await;
                }
                #[cfg(not(feature = "ml"))]
                {
                    Self::location_monitoring_loop(
                        zone_manager,
                        status,
                        should_shutdown,
                        scan_interval,
                    )
                    .await;
                }
            })
        };

        // Handle IPC commands
        let ipc_task = {
            let zone_manager = Arc::clone(&self.zone_manager);
            let status = Arc::clone(&self.status);
            let should_shutdown = Arc::clone(&self.should_shutdown);
            #[cfg(feature = "ml")]
            let ml_manager = Arc::clone(&self.ml_manager);

            tokio::spawn(async move {
                #[cfg(feature = "ml")]
                let command_handler = move |cmd| {
                    Self::handle_ipc_command(
                        Arc::clone(&zone_manager),
                        Arc::clone(&status),
                        Arc::clone(&should_shutdown),
                        Arc::clone(&ml_manager),
                        cmd,
                    )
                };

                #[cfg(not(feature = "ml"))]
                let command_handler = move |cmd| {
                    Self::handle_ipc_command(
                        Arc::clone(&zone_manager),
                        Arc::clone(&status),
                        Arc::clone(&should_shutdown),
                        cmd,
                    )
                };

                if let Err(e) = ipc_server.handle_connections(command_handler).await {
                    error!("IPC server error: {}", e);
                }
            })
        };

        // Wait for shutdown signal or tasks to complete
        tokio::select! {
            _ = monitor_task => {
                info!("Location monitoring task completed");
            },
            _ = ipc_task => {
                info!("IPC task completed");
            },
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                *self.should_shutdown.write().await = true;
            }
        }

        info!("Geofencing daemon shutting down");
        Ok(())
    }

    /// Main location monitoring loop (non-ML version)
    #[cfg(not(feature = "ml"))]
    async fn location_monitoring_loop(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        scan_interval: Duration,
    ) {
        debug!(
            "Starting location monitoring loop with interval {:?}",
            scan_interval
        );
        let mut interval = interval(scan_interval);
        let mut scan_count = 0u64;

        loop {
            interval.tick().await;
            scan_count += 1;
            debug!("Location scan #{} starting", scan_count);

            // Check for shutdown
            if *should_shutdown.read().await {
                debug!("Shutdown signal received, exiting monitoring loop");
                break;
            }

            // Skip if no zones configured
            {
                let manager = zone_manager.lock().await;
                let zone_count = manager.list_zones().len();
                if zone_count == 0 {
                    debug!("No zones configured, skipping scan #{}", scan_count);
                    continue;
                }
                debug!("Scanning with {} configured zones", zone_count);
            }

            // Check for location change
            debug!("Detecting location change for scan #{}", scan_count);
            let location_change = {
                let mut manager = zone_manager.lock().await;
                match manager.detect_location_change().await {
                    Ok(change) => {
                        if let Some(ref change) = change {
                            debug!(
                                "Location change detected: from {:?} to {} (confidence: {:.2})",
                                change.from.as_ref().map(|z| &z.name),
                                change.to.name,
                                change.confidence
                            );
                        } else {
                            debug!("No location change detected in scan #{}", scan_count);
                        }
                        change
                    }
                    Err(e) => {
                        warn!("Location detection failed in scan #{}: {}", scan_count, e);
                        continue;
                    }
                }
            };

            // Update status
            {
                let mut status_data = status.write().await;
                status_data.last_scan = Some(Utc::now());
                debug!(
                    "Updated last scan time to {}",
                    status_data.last_scan.unwrap()
                );

                if let Some(ref change) = location_change {
                    status_data.total_zone_changes += 1;
                    status_data.current_zone_id = Some(change.to.id.clone());
                    debug!(
                        "Total zone changes now: {}, current zone: {}",
                        status_data.total_zone_changes, change.to.id
                    );
                }
            }

            // Handle zone change
            if let Some(change) = location_change {
                info!(
                    "🌍 ZONE CHANGE DETECTED: {} -> {} (confidence: {:.2}%)",
                    change
                        .from
                        .as_ref()
                        .map(|z| z.name.as_str())
                        .unwrap_or("None"),
                    change.to.name,
                    change.confidence * 100.0
                );

                debug!("🔍 Zone change analysis:");
                debug!(
                    "  • From Zone: {}",
                    change
                        .from
                        .as_ref()
                        .map(|z| format!("'{}' (ID: {})", z.name, z.id))
                        .unwrap_or("None".to_string())
                );
                debug!("  • To Zone: '{}' (ID: {})", change.to.name, change.to.id);
                debug!("  • Confidence Score: {:.2}%", change.confidence * 100.0);
                debug!("  • Threshold: {:.2}", change.to.confidence_threshold);

                debug!("📋 Zone '{}' action summary:", change.to.name);
                debug!(
                    "  • WiFi: {}",
                    change.suggested_actions.wifi.as_deref().unwrap_or("None")
                );
                debug!(
                    "  • VPN: {}",
                    change.suggested_actions.vpn.as_deref().unwrap_or("None")
                );
                debug!(
                    "  • Tailscale Exit Node: {}",
                    change
                        .suggested_actions
                        .tailscale_exit_node
                        .as_deref()
                        .unwrap_or("None")
                );
                debug!(
                    "  • Tailscale Shields: {}",
                    match change.suggested_actions.tailscale_shields {
                        Some(true) => "Enable",
                        Some(false) => "Disable",
                        None => "No change",
                    }
                );
                debug!(
                    "  • Bluetooth Devices: {} ({})",
                    change.suggested_actions.bluetooth.len(),
                    if change.suggested_actions.bluetooth.is_empty() {
                        "none".to_string()
                    } else {
                        change.suggested_actions.bluetooth.join(", ")
                    }
                );
                debug!(
                    "  • Custom Commands: {} ({})",
                    change.suggested_actions.custom_commands.len(),
                    if change.suggested_actions.custom_commands.is_empty() {
                        "none".to_string()
                    } else {
                        change.suggested_actions.custom_commands.join("; ")
                    }
                );
                debug!(
                    "  • Notifications: {}",
                    if change.suggested_actions.notifications {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                );

                // Execute zone actions
                debug!("Executing zone actions for zone '{}'", change.to.name);
                if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await {
                    error!(
                        "Failed to execute zone actions for zone '{}': {}",
                        change.to.name, e
                    );
                } else {
                    debug!(
                        "Successfully executed all zone actions for zone '{}'",
                        change.to.name
                    );
                }

                // Send notification if enabled
                if change.suggested_actions.notifications {
                    debug!(
                        "Sending zone change notification for zone '{}'",
                        change.to.name
                    );
                    Self::send_zone_change_notification(&change);
                } else {
                    debug!(
                        "Notifications disabled for zone '{}', skipping notification",
                        change.to.name
                    );
                }
            }
        }

        info!(
            "Location monitoring loop stopped after {} scans",
            scan_count
        );
    }

    /// Enhanced location monitoring loop with ML-powered intelligence
    #[cfg(feature = "ml")]
    async fn enhanced_location_monitoring_loop(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        ml_manager: Arc<Mutex<MlManager>>,
        base_scan_interval: Duration,
    ) {
        info!("🧠 Starting ML-enhanced location monitoring with adaptive scanning");
        let mut scan_count = 0u32;
        let mut current_scan_interval = base_scan_interval;
        let mut ml_update_counter = 0u32;

        // Initialize ML system
        {
            let mut ml = ml_manager.lock().await;
            ml.initialize();
            info!("ML system initialized successfully");
        }

        loop {
            let scan_start_time = std::time::Instant::now();
            scan_count += 1;
            ml_update_counter += 1;

            debug!("🧠 Enhanced scan #{} (interval: {:?})", scan_count, current_scan_interval);

            // Sleep for adaptive interval
            tokio::time::sleep(current_scan_interval).await;

            // Check for shutdown
            if *should_shutdown.read().await {
                debug!("Shutdown signal received, exiting enhanced monitoring loop");
                
                // Save ML models before shutdown
                {
                    let ml = ml_manager.lock().await;
                    if let Err(e) = ml.save_models() {
                        warn!("Failed to save ML models on shutdown: {}", e);
                    } else {
                        info!("ML models saved successfully");
                    }
                }
                break;
            }

            // Skip if no zones configured
            {
                let manager = zone_manager.lock().await;
                let zone_count = manager.list_zones().len();
                if zone_count == 0 {
                    debug!("No zones configured, skipping enhanced scan #{}", scan_count);
                    continue;
                }
                debug!("Enhanced scanning with {} configured zones", zone_count);
            }

            // Generate ML-powered zone suggestions every 5 scans
            if ml_update_counter >= 5 {
                debug!("🧠 Generating ML-powered zone suggestions");
                ml_update_counter = 0;
                
                let (suggestions_generated, auto_suggestions_processed) = {
                    let mut ml = ml_manager.lock().await;
                    let manager = zone_manager.lock().await;
                    
                    // Generate suggestions with auto-acceptance evaluation
                    let enhanced_suggestions = ml.generate_enhanced_zone_suggestions(&manager.list_zones());
                    let suggestion_count = enhanced_suggestions.len();
                    
                    // Process automatic suggestions
                    let auto_results = ml.process_auto_suggestions(&manager.list_zones());
                    let auto_accepted_count = auto_results.iter()
                        .filter(|r| r.action == crate::ml_integration::AutoSuggestionAction::CreateZone)
                        .count();
                    
                    if !enhanced_suggestions.is_empty() {
                        info!("🧠 Generated {} zone suggestions ({} auto-accepted, {} require manual approval)", 
                              suggestion_count, auto_accepted_count, suggestion_count - auto_accepted_count);
                        
                        for (suggestion, should_auto_accept) in &enhanced_suggestions {
                            let acceptance_status = if *should_auto_accept { "🤖 AUTO-ACCEPT" } else { "👤 MANUAL" };
                            debug!("  📍 {}: {} (confidence: {:.1}%, priority: {:?})",
                                   acceptance_status,
                                   suggestion.suggested_name, 
                                   suggestion.confidence * 100.0,
                                   suggestion.priority);
                        }
                        
                        // Log auto-accepted suggestions
                        for result in &auto_results {
                            if result.action == crate::ml_integration::AutoSuggestionAction::CreateZone {
                                info!("🤖 ✨ Auto-suggestion: '{}' would be created automatically (confidence: {:.1}%)",
                                      result.suggestion_name, result.confidence * 100.0);
                                debug!("   Reasoning: {}", result.reasoning);
                            }
                        }
                    }
                    
                    (suggestion_count as u32, auto_accepted_count as u32)
                };

                // Update status with ML metrics
                {
                    let mut status_data = status.write().await;
                    status_data.ml_suggestions_generated += suggestions_generated;
                    status_data.ml_auto_suggestions_processed += auto_suggestions_processed;
                    status_data.last_ml_update = Some(Utc::now());
                }
            }

            // Check for location change with ML learning
            debug!("🧠 Detecting location change with ML learning for scan #{}", scan_count);
            let location_change = {
                let mut manager = zone_manager.lock().await;
                manager.detect_location_change().await
            };

            // Extract values for ML tracking before match consumes location_change
            let zone_changed = matches!(location_change, Ok(Some(_)));
            let confidence_score = match &location_change {
                Ok(Some(change)) => change.confidence,
                _ => 0.5, // Default confidence for stable/error states
            };

            match location_change {
                Ok(Some(change)) => {
                    // Enhanced confidence calculation with ML
                    let ml_confidence = {
                        let ml = ml_manager.lock().await;
                        let _manager = zone_manager.lock().await;
                        if let Ok(fingerprint) = super::fingerprinting::create_wifi_fingerprint(super::PrivacyMode::High).await {
                            ml.calculate_zone_confidence(&change.to.id, &fingerprint)
                        } else {
                            change.confidence // Fallback to original confidence
                        }
                    };

                    // Get dynamic confidence threshold
                    let confidence_threshold = {
                        let ml = ml_manager.lock().await;
                        ml.get_confidence_threshold(&change.to.id)
                    };

                    info!("🧠 Enhanced zone change detected: {} -> {} (original: {:.2}%, ML-enhanced: {:.2}%, threshold: {:.2}%)", 
                         change.from.as_ref().map(|z| z.name.as_str()).unwrap_or("None"),
                         change.to.name, 
                         change.confidence * 100.0,
                         ml_confidence * 100.0,
                         confidence_threshold * 100.0);

                    // Only proceed if confidence meets dynamic threshold
                    if ml_confidence >= confidence_threshold {
                        // Track prediction accuracy if we had a previous prediction
                        if let Some(previous_zone_id) = change.from.as_ref().map(|z| &z.id) {
                            let mut ml = ml_manager.lock().await;
                            ml.track_prediction_accuracy(previous_zone_id, &change.to.id, ml_confidence);
                        }

                        // Record zone change for ML learning
                        {
                            let mut ml = ml_manager.lock().await;
                            ml.record_zone_change(
                                change.from.as_ref().map(|z| z.id.as_str()),
                                &change.to.id
                            );
                        }

                        // Update status
                        {
                            let mut status_data = status.write().await;
                            status_data.last_scan = Some(Utc::now());
                            status_data.total_zone_changes += 1;
                            status_data.current_zone_id = Some(change.to.id.clone());
                        }

                        // Execute zone actions
                        debug!("🧠 Executing enhanced zone actions for zone '{}'", change.to.name);
                        if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await {
                            error!("Failed to execute zone actions for zone '{}': {}", change.to.name, e);
                        } else {
                        debug!("Successfully executed all zone actions for zone '{}'", change.to.name);
                    }

                    // Send notification if enabled
                    if change.suggested_actions.notifications {
                        debug!("Sending enhanced zone change notification for zone '{}'", change.to.name);
                        Self::send_zone_change_notification(&change);
                    }

                    // Adjust scan interval based on zone change
                    current_scan_interval = Self::calculate_adaptive_scan_interval(
                        base_scan_interval,
                        true, // zone changed
                        scan_start_time.elapsed(),
                    );
                    
                    {
                        let mut status_data = status.write().await;
                        status_data.adaptive_scan_interval = current_scan_interval;
                    }
                    
                    debug!("🧠 Adjusted scan interval to {:?} after zone change", current_scan_interval);
                    } else {
                        // Confidence below threshold - reject zone change
                        warn!("🧠 Zone change rejected due to low confidence: {:.2}% < {:.2}% threshold", 
                              ml_confidence * 100.0, confidence_threshold * 100.0);
                        
                        // Still update last scan time
                        {
                            let mut status_data = status.write().await;
                            status_data.last_scan = Some(Utc::now());
                        }
                        
                        // Use more frequent scanning when confidence is low
                        current_scan_interval = Self::calculate_adaptive_scan_interval(
                            base_scan_interval,
                            false, // no zone change executed
                            scan_start_time.elapsed(),
                        );
                        
                        // Reduce interval further for low confidence situations
                        current_scan_interval = Duration::from_millis(
                            (current_scan_interval.as_millis() as f64 * 0.7) as u64
                        );
                        
                        {
                            let mut status_data = status.write().await;
                            status_data.adaptive_scan_interval = current_scan_interval;
                        }
                        
                        debug!("🧠 Increased scan frequency to {:?} due to low confidence", current_scan_interval);
                    }
                }
                Ok(None) => {
                    debug!("🧠 No zone change detected in enhanced scan #{}", scan_count);
                    
                    // Update status
                    {
                        let mut status_data = status.write().await;
                        status_data.last_scan = Some(Utc::now());
                    }

                    // Adjust scan interval for stable location
                    current_scan_interval = Self::calculate_adaptive_scan_interval(
                        base_scan_interval,
                        false, // no zone change
                        scan_start_time.elapsed(),
                    );
                    
                    {
                        let mut status_data = status.write().await;
                        status_data.adaptive_scan_interval = current_scan_interval;
                    }
                }
                Err(e) => {
                    error!("🧠 Enhanced location detection failed in scan #{}: {}", scan_count, e);
                    
                    // Use exponential backoff on errors
                    current_scan_interval = std::cmp::min(
                        current_scan_interval * 2,
                        Duration::from_secs(300) // Max 5 minutes
                    );
                    debug!("🧠 Increased scan interval to {:?} due to error", current_scan_interval);
                }
            }

            // Update ML system with enhanced scan performance data every scan
            {
                let scan_duration = scan_start_time.elapsed();
                let mut ml = ml_manager.lock().await;
                let manager = zone_manager.lock().await;
                let zones_detected = manager.list_zones().len();
                
                ml.record_enhanced_scan_performance(
                    scan_duration, 
                    current_scan_interval, 
                    zones_detected, 
                    zone_changed,
                    confidence_score as f64
                );
                
                // Use ML-enhanced adaptive interval calculation
                let context = crate::ml_integration::get_current_context();
                let recent_changes = status.read().await.total_zone_changes % 10; // Recent changes in window
                current_scan_interval = ml.get_adaptive_scan_interval(
                    base_scan_interval,
                    recent_changes as u32,
                    &context
                );
                
                // Update status with ML-calculated interval
                {
                    let mut status_data = status.write().await;
                    status_data.adaptive_scan_interval = current_scan_interval;
                }
                
                debug!("📊 ML-enhanced scan interval updated: {:?} (zone_changed={}, confidence={:.1}%)", 
                       current_scan_interval, zone_changed, confidence_score * 100.0);
            }
        }

        info!("🧠 Enhanced location monitoring loop stopped after {} scans", scan_count);
    }

    /// Calculate adaptive scan interval based on recent activity
    fn calculate_adaptive_scan_interval(
        base_interval: Duration,
        zone_changed: bool,
        last_scan_duration: Duration,
    ) -> Duration {
        let base_seconds = base_interval.as_secs();
        
        let adjusted_seconds = if zone_changed {
            // Increase frequency after zone change to catch rapid transitions
            std::cmp::max(base_seconds / 2, 10) // Minimum 10 seconds
        } else {
            // Decrease frequency when stable, but consider scan performance
            let performance_factor = if last_scan_duration > Duration::from_secs(5) {
                1.5 // Slower scans if previous scan took long
            } else {
                1.2 // Normal backoff
            };
            
            (base_seconds as f64 * performance_factor) as u64
        };
        
        // Clamp to reasonable bounds
        let clamped_seconds = std::cmp::min(std::cmp::max(adjusted_seconds, 10), 300);
        Duration::from_secs(clamped_seconds)
    }

    /// Handle IPC commands from clients
    async fn handle_ipc_command(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        #[cfg(feature = "ml")]
        ml_manager: Arc<Mutex<crate::ml_integration::MlManager>>,
        command: DaemonCommand,
    ) -> DaemonResponse {
        debug!("Handling IPC command: {:?}", command);

        match command {
            DaemonCommand::GetCurrentLocation => {
                debug!("Processing GetCurrentLocation command");
                match super::fingerprinting::create_wifi_fingerprint(super::PrivacyMode::High).await
                {
                    Ok(fingerprint) => {
                        debug!("Successfully created location fingerprint with {} WiFi networks, confidence: {:.2}",
                            fingerprint.wifi_networks.len(), fingerprint.confidence_score);
                        DaemonResponse::LocationUpdate { fingerprint }
                    }
                    Err(e) => {
                        warn!("Failed to create location fingerprint: {}", e);
                        DaemonResponse::Error {
                            message: format!("Failed to get location: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::GetActiveZone => {
                debug!("Processing GetActiveZone command");
                let manager = zone_manager.lock().await;
                let zone = manager.get_current_zone().cloned();
                if let Some(ref zone) = zone {
                    debug!("Current active zone: {} (ID: {})", zone.name, zone.id);
                } else {
                    debug!("No active zone currently detected");
                }
                DaemonResponse::ActiveZone { zone }
            }

            DaemonCommand::ListZones => {
                debug!("Processing ListZones command");
                let manager = zone_manager.lock().await;
                let zones = manager.list_zones();
                debug!("Returning {} configured zones", zones.len());
                for zone in &zones {
                    debug!(
                        "  Zone: {} (ID: {}, {} fingerprints)",
                        zone.name,
                        zone.id,
                        zone.fingerprints.len()
                    );
                }
                DaemonResponse::ZoneList { zones }
            }

            DaemonCommand::CreateZone { name, actions } => {
                let mut manager = zone_manager.lock().await;
                match manager
                    .create_zone_from_current_location(name, actions)
                    .await
                {
                    Ok(zone) => DaemonResponse::ZoneCreated { zone },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to create zone: {}", e),
                    },
                }
            }

            DaemonCommand::RemoveZone { zone_id } => {
                let mut manager = zone_manager.lock().await;
                match manager.remove_zone(&zone_id) {
                    Ok(_) => DaemonResponse::Success,
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to remove zone: {}", e),
                    },
                }
            }

            DaemonCommand::ActivateZone { zone_id } => {
                debug!("Processing ActivateZone command for zone ID '{}'", zone_id);
                let mut manager = zone_manager.lock().await;
                match manager.activate_zone(&zone_id) {
                    Ok(change) => {
                        debug!(
                            "Zone '{}' activated successfully, executing actions",
                            change.to.name
                        );
                        // Execute zone actions
                        if let Err(e) = Self::execute_zone_actions(&change.suggested_actions).await
                        {
                            warn!(
                                "Zone '{}' activated but actions failed: {}",
                                change.to.name, e
                            );
                            return DaemonResponse::Error {
                                message: format!("Zone activated but actions failed: {}", e),
                            };
                        }
                        debug!(
                            "Successfully executed all actions for zone '{}'",
                            change.to.name
                        );

                        DaemonResponse::ZoneChanged {
                            from_zone_id: change.from.map(|z| z.id),
                            to_zone: change.to,
                            confidence: change.confidence,
                        }
                    }
                    Err(e) => {
                        warn!("Failed to activate zone '{}': {}", zone_id, e);
                        DaemonResponse::Error {
                            message: format!("Failed to activate zone: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::AddFingerprint { zone_name } => {
                debug!("Processing AddFingerprint command for zone '{}'", zone_name);
                let mut manager = zone_manager.lock().await;
                match manager.add_fingerprint_to_zone(&zone_name).await {
                    Ok(true) => {
                        debug!("Successfully added new fingerprint to zone '{}'", zone_name);
                        DaemonResponse::FingerprintAdded {
                            success: true,
                            message: format!("Added new fingerprint to zone '{}'", zone_name),
                        }
                    }
                    Ok(false) => {
                        debug!(
                            "Fingerprint too similar to existing ones in zone '{}', not added",
                            zone_name
                        );
                        DaemonResponse::FingerprintAdded {
                            success: false,
                            message: format!(
                                "Fingerprint too similar to existing ones in zone '{}'",
                                zone_name
                            ),
                        }
                    }
                    Err(e) => {
                        warn!("Failed to add fingerprint to zone '{}': {}", zone_name, e);
                        DaemonResponse::FingerprintAdded {
                            success: false,
                            message: format!("Failed to add fingerprint: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::ExecuteActions { actions } => {
                debug!(
                    "Processing ExecuteActions command with {} custom commands",
                    actions.custom_commands.len()
                );
                debug!("Actions details: WiFi={:?}, VPN={:?}, Tailscale Exit={:?}, Shields={:?}, Bluetooth={:?}",
                    actions.wifi, actions.vpn, actions.tailscale_exit_node, actions.tailscale_shields, actions.bluetooth);
                match Self::execute_zone_actions(&actions).await {
                    Ok(_) => {
                        debug!("Successfully executed all requested actions");
                        DaemonResponse::Success
                    }
                    Err(e) => {
                        warn!("Failed to execute requested actions: {}", e);
                        DaemonResponse::Error {
                            message: format!("Failed to execute actions: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::GetStatus => {
                debug!("Processing GetStatus command");
                let status_data = status.read().await;
                let daemon_status = DaemonStatus {
                    monitoring: status_data.monitoring,
                    zone_count: {
                        let manager = zone_manager.lock().await;
                        manager.list_zones().len()
                    },
                    active_zone_id: {
                        let manager = zone_manager.lock().await;
                        manager.get_current_zone().map(|z| z.id.clone())
                    },
                    last_scan: {
                        let manager = zone_manager.lock().await;
                        manager.get_last_scan()
                    },
                    total_zone_changes: {
                        let manager = zone_manager.lock().await;
                        manager.get_total_zone_changes()
                    },
                    uptime_seconds: status_data.startup_time.elapsed().as_secs(),
                    #[cfg(feature = "ml")]
                    ml_suggestions_generated: status_data.ml_suggestions_generated,
                    #[cfg(feature = "ml")]
                    adaptive_scan_interval_seconds: status_data.adaptive_scan_interval.as_secs(),
                    #[cfg(feature = "ml")]
                    last_ml_update: status_data.last_ml_update,
                };
                debug!("Daemon status: monitoring={}, {} zones, active_zone={:?}, {} zone changes, uptime={}s", daemon_status.monitoring, daemon_status.zone_count, daemon_status.active_zone_id, daemon_status.total_zone_changes, daemon_status.uptime_seconds);
                DaemonResponse::Status {
                    status: daemon_status,
                }
            }

            DaemonCommand::Shutdown => {
                debug!("Processing Shutdown command");
                info!("Received shutdown command from client");
                *should_shutdown.write().await = true;
                debug!("Shutdown flag set to true");
                DaemonResponse::Success
            }

            #[cfg(feature = "ml")]
            DaemonCommand::GetZoneSuggestions => {
                debug!("Processing GetZoneSuggestions command");
                let ml_manager = ml_manager.lock().await;
                let manager = zone_manager.lock().await;
                let suggestions = ml_manager.generate_zone_suggestions(&manager.list_zones());
                debug!("Generated {} zone suggestions", suggestions.len());
                DaemonResponse::ZoneSuggestions { suggestions }
            }

            #[cfg(feature = "ml")]
            DaemonCommand::GetMlMetrics => {
                debug!("Processing GetMlMetrics command");
                let ml_manager = ml_manager.lock().await;
                let status_data = status.read().await;
                let metrics = ml_manager.get_ml_metrics(
                    status_data.ml_suggestions_generated,
                    status_data.ml_auto_suggestions_processed
                );
                debug!("Retrieved ML metrics: model_version={}, suggestions={}, auto_processed={}", 
                       metrics.ml_model_version, 
                       status_data.ml_suggestions_generated,
                       status_data.ml_auto_suggestions_processed);
                DaemonResponse::MlMetrics { metrics }
            }
        }
    }

    /// Handle IPC commands from clients (non-ML version)
    #[cfg(not(feature = "ml"))]
    async fn handle_ipc_command(
        zone_manager: Arc<Mutex<ZoneManager>>,
        status: Arc<RwLock<DaemonStatusData>>,
        should_shutdown: Arc<RwLock<bool>>,
        command: DaemonCommand,
    ) -> DaemonResponse {
        debug!("Handling IPC command: {:?}", command);

        match command {
            DaemonCommand::GetCurrentLocation => {
                debug!("Processing GetCurrentLocation command");
                match super::fingerprinting::create_wifi_fingerprint(super::PrivacyMode::High).await
                {
                    Ok(fingerprint) => {
                        debug!("Successfully created location fingerprint with {} WiFi networks, confidence: {:.2}",
                            fingerprint.wifi_networks.len(), fingerprint.confidence_score);
                        DaemonResponse::LocationUpdate { fingerprint }
                    }
                    Err(e) => {
                        warn!("Failed to create location fingerprint: {}", e);
                        DaemonResponse::Error {
                            message: format!("Failed to get location: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::GetActiveZone => {
                debug!("Processing GetActiveZone command");
                let manager = zone_manager.lock().await;
                let zone = manager.get_current_zone().cloned();
                if let Some(ref zone) = zone {
                    debug!("Current active zone: {} (ID: {})", zone.name, zone.id);
                } else {
                    debug!("No active zone currently detected");
                }
                DaemonResponse::ActiveZone { zone }
            }

            DaemonCommand::ListZones => {
                debug!("Processing ListZones command");
                let manager = zone_manager.lock().await;
                let zones = manager.list_zones();
                debug!("Returning {} configured zones", zones.len());
                DaemonResponse::ZoneList { zones }
            }

            DaemonCommand::CreateZone { name, actions } => {
                debug!("Processing CreateZone command for zone: {}", name);
                let mut manager = zone_manager.lock().await;
                match manager.create_zone(name, actions) {
                    Ok(zone) => {
                        debug!("Successfully created zone: {} (ID: {})", zone.name, zone.id);
                        DaemonResponse::ZoneCreated { zone }
                    }
                    Err(e) => {
                        warn!("Failed to create zone: {}", e);
                        DaemonResponse::Error {
                            message: format!("Failed to create zone: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::AddFingerprint { zone_name } => {
                debug!("Processing AddFingerprint command for zone: {}", zone_name);
                let mut manager = zone_manager.lock().await;
                match manager.add_fingerprint_to_zone(&zone_name).await {
                    Ok(_) => {
                        debug!("Successfully added fingerprint to zone: {}", zone_name);
                        DaemonResponse::FingerprintAdded {
                            success: true,
                            message: "Fingerprint added successfully".to_string(),
                        }
                    }
                    Err(e) => {
                        warn!("Failed to add fingerprint to zone {}: {}", zone_name, e);
                        DaemonResponse::FingerprintAdded {
                            success: false,
                            message: format!("Failed to add fingerprint: {}", e),
                        }
                    }
                }
            }

            DaemonCommand::GetStatus => {
                debug!("Processing GetStatus command");
                let status_data = status.read().await;
                let daemon_status = DaemonStatus {
                    monitoring: status_data.monitoring,
                    zone_count: {
                        let manager = zone_manager.lock().await;
                        manager.list_zones().len()
                    },
                    active_zone_id: {
                        let manager = zone_manager.lock().await;
                        manager.get_current_zone().map(|z| z.id.clone())
                    },
                    last_scan: {
                        let manager = zone_manager.lock().await;
                        manager.get_last_scan()
                    },
                    total_zone_changes: {
                        let manager = zone_manager.lock().await;
                        manager.get_total_zone_changes()
                    },
                    uptime_seconds: status_data.startup_time.elapsed().as_secs(),
                };
                debug!("Daemon status: monitoring={}, {} zones, active_zone={:?}, {} zone changes, uptime={}s", daemon_status.monitoring, daemon_status.zone_count, daemon_status.active_zone_id, daemon_status.total_zone_changes, daemon_status.uptime_seconds);
                DaemonResponse::Status {
                    status: daemon_status,
                }
            }

            DaemonCommand::Shutdown => {
                debug!("Processing Shutdown command");
                info!("Received shutdown command from client");
                *should_shutdown.write().await = true;
                debug!("Shutdown flag set to true");
                DaemonResponse::Success
            }
        }
    }

    /// Execute zone actions (connect to WiFi, VPN, etc.)
    async fn execute_zone_actions(actions: &ZoneActions) -> Result<()> {
        debug!("Starting zone action execution");
        info!("Executing zone actions: {:?}", actions);
        debug!("Action details: WiFi={:?}, VPN={:?}, Tailscale Exit Node={:?}, Tailscale Shields={:?}, {} Bluetooth devices, {} custom commands",
            actions.wifi, actions.vpn, actions.tailscale_exit_node, actions.tailscale_shields,
            actions.bluetooth.len(), actions.custom_commands.len());

        // Connect to WiFi
        if let Some(ref wifi_ssid) = actions.wifi {
            debug!(
                "Processing WiFi connection action for SSID: '{}'",
                wifi_ssid
            );
            info!("Connecting to WiFi: {}", wifi_ssid);

            let command_runner = RealCommandRunner;

            // Try NetworkManager first, then fall back to IWD
            let success = if crate::command::is_command_installed("nmcli") {
                debug!("Using NetworkManager (nmcli) for WiFi connection");
                // Use NetworkManager - attempt connection without password first
                debug!("Executing nmcli command: device wifi connect {}", wifi_ssid);
                let result =
                    command_runner.run_command("nmcli", &["device", "wifi", "connect", wifi_ssid]);

                match result {
                    Ok(output) if output.status.success() => {
                        debug!("NetworkManager connection successful: {:?}", output);
                        info!("Successfully connected to WiFi: {}", wifi_ssid);
                        true
                    }
                    Ok(output) => {
                        debug!(
                            "NetworkManager connection failed with status: {:?}, stderr: {:?}",
                            output.status,
                            String::from_utf8_lossy(&output.stderr)
                        );
                        warn!(
                            "Failed to connect to WiFi {} (may need password)",
                            wifi_ssid
                        );
                        false
                    }
                    Err(e) => {
                        debug!("NetworkManager command execution error: {}", e);
                        warn!(
                            "Failed to execute nmcli command for WiFi {}: {}",
                            wifi_ssid, e
                        );
                        false
                    }
                }
            } else if crate::command::is_command_installed("iwctl") {
                debug!("Using IWD (iwctl) for WiFi connection as NetworkManager not available");
                debug!(
                    "Executing iwctl command: station wlan0 connect {}",
                    wifi_ssid
                );
                // Use IWD - attempt connection
                let result = command_runner
                    .run_command("iwctl", &["station", "wlan0", "connect", wifi_ssid]);

                match result {
                    Ok(output) if output.status.success() => {
                        debug!("IWD connection successful: {:?}", output);
                        info!("Successfully connected to WiFi: {}", wifi_ssid);
                        true
                    }
                    Ok(output) => {
                        debug!(
                            "IWD connection failed with status: {:?}, stderr: {:?}",
                            output.status,
                            String::from_utf8_lossy(&output.stderr)
                        );
                        warn!(
                            "Failed to connect to WiFi {} (may need password)",
                            wifi_ssid
                        );
                        false
                    }
                    Err(e) => {
                        debug!("IWD command execution error: {}", e);
                        warn!(
                            "Failed to execute iwctl command for WiFi {}: {}",
                            wifi_ssid, e
                        );
                        false
                    }
                }
            } else {
                debug!("No WiFi management tools found (nmcli or iwctl)");
                warn!("No WiFi manager (nmcli/iwctl) available");
                false
            };

            if !success {
                debug!(
                    "WiFi connection attempt to '{}' was unsuccessful",
                    wifi_ssid
                );
                error!(
                    "WiFi connection to {} failed - geofencing may not work as expected",
                    wifi_ssid
                );
            }
        }

        // Connect to VPN
        if let Some(ref vpn_name) = actions.vpn {
            debug!("Processing VPN connection action for: '{}'", vpn_name);
            info!("Connecting to VPN: {}", vpn_name);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("nmcli") {
                debug!("Using NetworkManager to connect to VPN");
                debug!("Executing nmcli command: connection up {}", vpn_name);
                let result = command_runner.run_command("nmcli", &["connection", "up", vpn_name]);

                match result {
                    Ok(output) if output.status.success() => {
                        debug!("VPN connection successful: {:?}", output);
                        info!("Successfully connected to VPN: {}", vpn_name);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        debug!(
                            "VPN connection failed with status: {:?}, stderr: {}",
                            output.status, stderr
                        );
                        error!("Failed to connect to VPN {}: {}", vpn_name, stderr);
                    }
                    Err(e) => {
                        debug!("VPN command execution error: {}", e);
                        error!("Error connecting to VPN {}: {}", vpn_name, e);
                    }
                }
            } else {
                debug!("NetworkManager (nmcli) not available for VPN connection");
                warn!("NetworkManager (nmcli) not available for VPN connection");
            }
        }

        // Configure Tailscale exit node
        if let Some(ref exit_node) = actions.tailscale_exit_node {
            debug!("Processing Tailscale exit node action: '{}'", exit_node);
            info!("Setting Tailscale exit node: {}", exit_node);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("tailscale") {
                debug!("Using Tailscale CLI to set exit node");
                let exit_node_arg = format!("--exit-node={}", exit_node);
                debug!("Executing tailscale command: set {}", exit_node_arg);
                let result = command_runner.run_command("tailscale", &["set", &exit_node_arg]);

                match result {
                    Ok(output) if output.status.success() => {
                        debug!("Tailscale exit node set successfully: {:?}", output);
                        info!("Successfully set Tailscale exit node: {}", exit_node);
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        debug!(
                            "Tailscale exit node failed with status: {:?}, stderr: {}",
                            output.status, stderr
                        );
                        error!(
                            "Failed to set Tailscale exit node {}: {}",
                            exit_node, stderr
                        );
                    }
                    Err(e) => {
                        debug!("Tailscale command execution error: {}", e);
                        error!("Error setting Tailscale exit node {}: {}", exit_node, e);
                    }
                }
            } else {
                debug!("Tailscale CLI not available for exit node configuration");
                warn!("Tailscale not available for exit node configuration");
            }
        }

        // Configure Tailscale shields
        if let Some(shields_up) = actions.tailscale_shields {
            debug!("Processing Tailscale shields action: {}", shields_up);
            info!(
                "Setting Tailscale shields: {}",
                if shields_up { "up" } else { "down" }
            );

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("tailscale") {
                let shield_arg = if shields_up {
                    "--shields-up=true"
                } else {
                    "--shields-up=false"
                };
                debug!("Executing tailscale command: set {}", shield_arg);
                let result = command_runner.run_command("tailscale", &["set", shield_arg]);

                match result {
                    Ok(output) if output.status.success() => {
                        debug!("Tailscale shields set successfully: {:?}", output);
                        info!(
                            "Successfully set Tailscale shields: {}",
                            if shields_up { "up" } else { "down" }
                        );
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        debug!(
                            "Tailscale shields failed with status: {:?}, stderr: {}",
                            output.status, stderr
                        );
                        error!("Failed to set Tailscale shields: {}", stderr);
                    }
                    Err(e) => {
                        debug!("Tailscale shields command execution error: {}", e);
                        error!("Error setting Tailscale shields: {}", e);
                    }
                }
            } else {
                debug!("Tailscale CLI not available for shields configuration");
                warn!("Tailscale not available for shields configuration");
            }
        }

        // Connect Bluetooth devices
        for device_name in &actions.bluetooth {
            debug!(
                "Processing Bluetooth connection action for device: '{}'",
                device_name
            );
            info!("Connecting Bluetooth device: {}", device_name);

            let command_runner = RealCommandRunner;

            if crate::command::is_command_installed("bluetoothctl") {
                debug!("Using bluetoothctl to connect device");
                // First, try to find the device by name to get its address
                debug!("Executing bluetoothctl devices command to find device address");
                match command_runner.run_command("bluetoothctl", &["devices"]) {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let mut device_address: Option<String> = None;

                        // Look for the device by name in the output
                        for line in stdout.lines() {
                            if line.contains(device_name) {
                                // Extract MAC address from line format: "Device AA:BB:CC:DD:EE:FF Device Name"
                                if let Some(address_start) = line.find("Device ") {
                                    if let Some(address_end) = line[address_start + 7..].find(' ') {
                                        device_address = Some(
                                            line[address_start + 7
                                                ..address_start + 7 + address_end]
                                                .to_string(),
                                        );
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(address) = device_address {
                            debug!("Found device '{}' with address '{}'", device_name, address);
                            // Try to connect using the address
                            debug!("Executing bluetoothctl command: connect {}", address);
                            match command_runner.run_command("bluetoothctl", &["connect", &address])
                            {
                                Ok(output) if output.status.success() => {
                                    debug!("Bluetooth connection successful: {:?}", output);
                                    info!(
                                        "Successfully connected to Bluetooth device: {} ({})",
                                        device_name, address
                                    );
                                }
                                Ok(output) => {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    debug!(
                                        "Bluetooth connection failed with status: {:?}, stderr: {}",
                                        output.status, stderr
                                    );
                                    warn!(
                                        "Failed to connect to Bluetooth device: {} ({}): {}",
                                        device_name, address, stderr
                                    );
                                }
                                Err(e) => {
                                    debug!("Bluetooth command execution error: {}", e);
                                    warn!(
                                        "Error connecting to Bluetooth device: {} ({}): {}",
                                        device_name, address, e
                                    );
                                }
                            }
                        } else {
                            debug!(
                                "Device '{}' not found in bluetoothctl devices output",
                                device_name
                            );
                            warn!(
                                "Bluetooth device '{}' not found in paired devices",
                                device_name
                            );
                        }
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        debug!(
                            "Bluetooth devices listing failed with status: {:?}, stderr: {}",
                            output.status, stderr
                        );
                        error!(
                            "Failed to list Bluetooth devices for {}: {}",
                            device_name, stderr
                        );
                    }
                    Err(e) => {
                        debug!("Bluetooth devices command execution error: {}", e);
                        error!(
                            "Failed to execute bluetoothctl devices command for {}: {}",
                            device_name, e
                        );
                    }
                }
            } else {
                debug!("bluetoothctl not available for Bluetooth connection");
                warn!("bluetoothctl not available for Bluetooth connection");
            }
        }

        // Execute custom commands
        debug!(
            "Processing {} custom commands",
            actions.custom_commands.len()
        );
        for (idx, command) in actions.custom_commands.iter().enumerate() {
            debug!("Processing custom command #{}: '{}'", idx + 1, command);
            info!("Executing custom command: {}", command);

            // Security: Only allow predefined safe commands or whitelist patterns
            if Self::is_safe_command(command) {
                debug!("Command passed security check, executing...");
                match tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                {
                    Ok(output) => {
                        if output.status.success() {
                            info!("Successfully executed custom command: {}", command);
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            if !stdout.trim().is_empty() {
                                debug!("Command output: {}", stdout.trim());
                            } else {
                                debug!("Command executed successfully with no output");
                            }
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            debug!(
                                "Custom command failed with status: {:?}, stderr: {}",
                                output.status,
                                stderr.trim()
                            );
                            error!(
                                "Custom command failed: {} - Error: {}",
                                command,
                                stderr.trim()
                            );
                        }
                    }
                    Err(e) => {
                        debug!("Custom command execution error: {}", e);
                        error!("Failed to execute custom command '{}': {}", command, e);
                    }
                }
            } else {
                debug!("Command failed security check: '{}'", command);
                warn!("Skipped potentially unsafe custom command: {}", command);
            }
        }
        debug!("Finished processing all custom commands");

        Ok(())
    }

    /// Check if a custom command is safe to execute
    /// This is a security measure to prevent dangerous commands
    fn is_safe_command(command: &str) -> bool {
        let command_lower = command.to_lowercase();

        // Block dangerous commands and patterns
        let dangerous_patterns = [
            "rm -rf",
            "rm -r",
            "sudo rm",
            "format",
            "mkfs",
            "dd if=",
            "fdisk",
            "parted",
            "> /dev/",
            "shutdown",
            "reboot",
            "halt",
            "iptables -F",
            "ufw --force",
            "chmod 777",
            "chmod -R 777",
            "curl",
            "wget",
            "nc ",
            "netcat",
            "telnet",
            "ssh ",
            "scp ",
            "python -c",
            "perl -e",
            "eval",
            "exec",
            "`",
            "$(",
            "passwd",
            "su ",
            "sudo su",
            "/etc/shadow",
            "/etc/passwd",
            "crontab",
            "/var/",
            "/etc/",
            "/root/",
            "/boot/",
        ];

        // Check for dangerous patterns
        for pattern in &dangerous_patterns {
            if command_lower.contains(pattern) {
                return false;
            }
        }

        // Allow safe commands (whitelist approach would be more secure)
        let safe_prefixes = [
            "systemctl --user start",
            "systemctl --user stop",
            "systemctl --user restart",
            "notify-send",
            "echo ",
            "printf ",
            "logger ",
            "touch /tmp/",
            "mkdir -p /tmp/",
        ];

        // Check for safe command prefixes
        for prefix in &safe_prefixes {
            if command_lower.starts_with(prefix) {
                return true;
            }
        }

        // For now, be conservative and reject unknown commands
        // In production, you might want a more sophisticated whitelist
        false
    }

    /// Send zone change notification
    fn send_zone_change_notification(change: &LocationChange) {
        debug!("Preparing to send zone change notification");
        let title = "Network Zone Changed";
        let body = if let Some(ref from_zone) = change.from {
            debug!(
                "Zone change from '{}' to '{}' with confidence {:.2}",
                from_zone.name, change.to.name, change.confidence
            );
            format!(
                "Switched from {} to {} zone\nConfidence: {:.0}%",
                from_zone.name,
                change.to.name,
                change.confidence * 100.0
            )
        } else {
            debug!(
                "Initial zone entry to '{}' with confidence {:.2}",
                change.to.name, change.confidence
            );
            format!(
                "Entered {} zone\nConfidence: {:.0}%",
                change.to.name,
                change.confidence * 100.0
            )
        };
        debug!(
            "Notification content prepared: title='{}', body='{}'",
            title,
            body.replace("\n", " | ")
        );

        // Send desktop notification
        debug!("Sending desktop notification via notify-rust");
        if let Err(e) = notify_rust::Notification::new()
            .summary(title)
            .body(&body)
            .icon("network-wireless")
            .timeout(notify_rust::Timeout::Milliseconds(5000))
            .show()
        {
            debug!("Desktop notification failed: {}", e);
            warn!("Failed to send notification: {}", e);
        } else {
            debug!("Desktop notification sent successfully");
        }

        info!("Zone change notification sent: {} - {}", title, body);
    }
}

/// Check if daemon is running
pub fn is_daemon_running() -> bool {
    std::path::Path::new("/tmp/network-dmenu-daemon.sock").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_creation() {
        let config = GeofencingConfig::default();
        let daemon = GeofencingDaemon::new(config);

        // Basic creation test
        assert!(!*daemon.should_shutdown.try_read().unwrap());
    }

    #[test]
    fn test_daemon_running_check() {
        // Should be false in test environment
        assert!(!is_daemon_running());
    }

    #[tokio::test]
    async fn test_daemon_status_creation() {
        let config = GeofencingConfig::default();
        let daemon = GeofencingDaemon::new(config);

        let status_data = daemon.status.read().await;
        assert!(!status_data.monitoring);
        assert!(status_data.last_scan.is_none());
        assert_eq!(status_data.total_zone_changes, 0);
    }
}
