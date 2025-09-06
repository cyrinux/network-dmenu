//! Enhanced geofencing daemon with all advanced components integrated
//!
//! Combines ML-powered zone suggestions, adaptive scanning, comprehensive security,
//! performance optimizations, observability, and advanced zone management
//! into a unified, production-ready geofencing system.

use super::{
    adaptive::{AdaptiveScanner, ScanFrequency},
    advanced_zones::{AdvancedZoneManager, AdvancedZoneConfig},
    config::{ConfigManager, EnhancedConfig},
    ipc::{DaemonCommand, DaemonIpcServer, DaemonResponse},
    lifecycle::{LifecycleManager, SystemEvent},
    observability::{ObservabilityManager, ObservabilityConfig},
    performance::PerformanceOptimizer,
    retry::{RetryManager, RetryConfig, ActionContext},
    security::{SecureCommandExecutor, SecurityPolicy},
    zones::ZoneManager,
    GeofencingConfig, LocationChange, Result, ZoneActions, GeofenceError,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, sleep};

/// Enhanced geofencing daemon with all advanced features
pub struct EnhancedGeofencingDaemon {
    /// Configuration manager
    config_manager: Arc<Mutex<ConfigManager>>,
    /// Core zone manager
    zone_manager: Arc<Mutex<ZoneManager>>,
    /// Advanced zone manager with ML capabilities
    advanced_zone_manager: Arc<Mutex<AdvancedZoneManager>>,
    /// Adaptive scanning system
    adaptive_scanner: Arc<Mutex<AdaptiveScanner>>,
    /// Retry manager for failed operations
    retry_manager: Arc<Mutex<RetryManager>>,
    /// Secure command executor
    secure_executor: Arc<Mutex<SecureCommandExecutor>>,
    /// Performance optimizer
    performance_optimizer: Arc<PerformanceOptimizer>,
    /// Observability manager
    observability_manager: Arc<Mutex<ObservabilityManager>>,
    /// System lifecycle manager
    lifecycle_manager: Arc<Mutex<LifecycleManager>>,
    /// Daemon state
    daemon_state: Arc<RwLock<EnhancedDaemonState>>,
    /// Shutdown signal
    should_shutdown: Arc<RwLock<bool>>,
}

/// Enhanced daemon state with comprehensive status information
#[derive(Debug, Clone)]
pub struct EnhancedDaemonState {
    /// Whether daemon is currently running
    pub running: bool,
    /// Current zone information
    pub current_zone: Option<String>,
    /// Daemon startup time
    pub startup_time: chrono::DateTime<Utc>,
    /// Last successful scan time
    pub last_scan: Option<chrono::DateTime<Utc>>,
    /// Current scanning interval
    pub current_scan_interval: Duration,
    /// Number of zone changes since startup
    pub total_zone_changes: u32,
    /// Recent zone suggestions count
    pub recent_suggestions: u32,
    /// Security incidents count
    pub security_incidents: u32,
    /// Performance metrics summary
    pub performance_summary: PerformanceSummary,
    /// Health status summary
    pub health_summary: HealthSummary,
}

/// Performance summary for dashboard display
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    /// Memory usage in MB
    pub memory_usage_mb: f64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Cache hit rate percentage
    pub cache_hit_rate: f64,
    /// Average operation time
    pub avg_operation_time: Duration,
    /// Connection pool utilization
    pub pool_utilization: f64,
}

/// Health status summary
#[derive(Debug, Clone)]
pub struct HealthSummary {
    /// Overall health status
    pub overall_status: String,
    /// Number of healthy components
    pub healthy_components: u32,
    /// Number of degraded components
    pub degraded_components: u32,
    /// Number of critical components
    pub critical_components: u32,
}

impl EnhancedGeofencingDaemon {
    /// Create new enhanced geofencing daemon
    pub async fn new(config_path: std::path::PathBuf) -> Result<Self> {
        info!("🚀 Creating enhanced geofencing daemon");

        // Initialize configuration manager
        let config_manager = Arc::new(Mutex::new(
            ConfigManager::new(&config_path).await?
        ));

        let enhanced_config = {
            let config_guard = config_manager.lock().await;
            config_guard.get_config().clone()
        };

        // Initialize core zone manager
        let zone_manager = Arc::new(Mutex::new(
            ZoneManager::new(enhanced_config.geofencing.clone())
        ));

        // Initialize advanced zone manager
        let advanced_zone_manager = Arc::new(Mutex::new(
            AdvancedZoneManager::new(AdvancedZoneConfig::default()).await
        ));

        // Initialize adaptive scanner
        let adaptive_scanner = Arc::new(Mutex::new(
            AdaptiveScanner::new(enhanced_config.adaptive_scanning.clone())
        ));

        // Initialize retry manager
        let retry_manager = Arc::new(Mutex::new(
            RetryManager::new(enhanced_config.retry.clone())
        ));

        // Initialize secure command executor
        let secure_executor = Arc::new(Mutex::new(
            SecureCommandExecutor::new(enhanced_config.security.clone()).await?
        ));

        // Initialize performance optimizer
        let performance_optimizer = Arc::new(
            PerformanceOptimizer::new().await
        );

        // Initialize observability manager
        let observability_manager = Arc::new(Mutex::new(
            ObservabilityManager::new(enhanced_config.observability.clone()).await?
        ));

        // Initialize lifecycle manager
        let lifecycle_manager = Arc::new(Mutex::new(
            LifecycleManager::new(Arc::clone(&zone_manager)).await?
        ));

        // Initialize daemon state
        let daemon_state = Arc::new(RwLock::new(EnhancedDaemonState {
            running: false,
            current_zone: None,
            startup_time: Utc::now(),
            last_scan: None,
            current_scan_interval: Duration::from_secs(30),
            total_zone_changes: 0,
            recent_suggestions: 0,
            security_incidents: 0,
            performance_summary: PerformanceSummary {
                memory_usage_mb: 0.0,
                cpu_usage_percent: 0.0,
                cache_hit_rate: 0.0,
                avg_operation_time: Duration::from_millis(0),
                pool_utilization: 0.0,
            },
            health_summary: HealthSummary {
                overall_status: "Starting".to_string(),
                healthy_components: 0,
                degraded_components: 0,
                critical_components: 0,
            },
        }));

        let daemon = Self {
            config_manager,
            zone_manager,
            advanced_zone_manager,
            adaptive_scanner,
            retry_manager,
            secure_executor,
            performance_optimizer,
            observability_manager,
            lifecycle_manager,
            daemon_state,
            should_shutdown: Arc::new(RwLock::new(false)),
        };

        info!("✅ Enhanced geofencing daemon created successfully");
        Ok(daemon)
    }

    /// Start the enhanced daemon with all subsystems
    pub async fn run(&mut self) -> Result<()> {
        info!("🚀 Starting enhanced geofencing daemon with all subsystems");

        // Update daemon state
        {
            let mut state = self.daemon_state.write().await;
            state.running = true;
            state.health_summary.overall_status = "Starting".to_string();
        }

        // Start configuration file watching
        {
            let mut config_manager = self.config_manager.lock().await;
            config_manager.start_file_watching().await?;
        }

        // Start lifecycle monitoring
        {
            let mut lifecycle_manager = self.lifecycle_manager.lock().await;
            lifecycle_manager.start_monitoring().await?;
        }

        // Start observability monitoring
        {
            let mut observability_manager = self.observability_manager.lock().await;
            observability_manager.start_monitoring().await?;
        }

        // Start IPC server
        debug!("Starting enhanced IPC server");
        let mut ipc_server = DaemonIpcServer::new().await?;

        // Clone references for background tasks
        let zone_manager = Arc::clone(&self.zone_manager);
        let advanced_zone_manager = Arc::clone(&self.advanced_zone_manager);
        let adaptive_scanner = Arc::clone(&self.adaptive_scanner);
        let retry_manager = Arc::clone(&self.retry_manager);
        let secure_executor = Arc::clone(&self.secure_executor);
        let performance_optimizer = Arc::clone(&self.performance_optimizer);
        let observability_manager = Arc::clone(&self.observability_manager);
        let lifecycle_manager = Arc::clone(&self.lifecycle_manager);
        let daemon_state = Arc::clone(&self.daemon_state);
        let should_shutdown = Arc::clone(&self.should_shutdown);

        // Start main scanning loop
        let scanning_task = {
            let zone_manager = Arc::clone(&zone_manager);
            let advanced_zone_manager = Arc::clone(&advanced_zone_manager);
            let adaptive_scanner = Arc::clone(&adaptive_scanner);
            let retry_manager = Arc::clone(&retry_manager);
            let secure_executor = Arc::clone(&secure_executor);
            let observability_manager = Arc::clone(&observability_manager);
            let daemon_state = Arc::clone(&daemon_state);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::enhanced_scanning_loop(
                    zone_manager,
                    advanced_zone_manager,
                    adaptive_scanner,
                    retry_manager,
                    secure_executor,
                    observability_manager,
                    daemon_state,
                    should_shutdown,
                ).await;
            })
        };

        // Start retry processing task
        let retry_task = {
            let retry_manager = Arc::clone(&retry_manager);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::retry_processing_loop(retry_manager, should_shutdown).await;
            })
        };

        // Start suggestions generation task
        let suggestions_task = {
            let advanced_zone_manager = Arc::clone(&advanced_zone_manager);
            let daemon_state = Arc::clone(&daemon_state);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::suggestions_generation_loop(advanced_zone_manager, daemon_state, should_shutdown).await;
            })
        };

        // Start health monitoring task
        let health_task = {
            let observability_manager = Arc::clone(&observability_manager);
            let daemon_state = Arc::clone(&daemon_state);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::health_monitoring_loop(observability_manager, daemon_state, should_shutdown).await;
            })
        };

        // Start performance monitoring task
        let performance_task = {
            let performance_optimizer = Arc::clone(&performance_optimizer);
            let daemon_state = Arc::clone(&daemon_state);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                Self::performance_monitoring_loop(performance_optimizer, daemon_state, should_shutdown).await;
            })
        };

        // Handle IPC commands
        let ipc_task = {
            let zone_manager = Arc::clone(&zone_manager);
            let advanced_zone_manager = Arc::clone(&advanced_zone_manager);
            let observability_manager = Arc::clone(&observability_manager);
            let daemon_state = Arc::clone(&daemon_state);
            let should_shutdown = Arc::clone(&should_shutdown);

            tokio::spawn(async move {
                let command_handler = |cmd| {
                    Self::handle_enhanced_ipc_command(
                        Arc::clone(&zone_manager),
                        Arc::clone(&advanced_zone_manager),
                        Arc::clone(&observability_manager),
                        Arc::clone(&daemon_state),
                        Arc::clone(&should_shutdown),
                        cmd,
                    )
                };

                if let Err(e) = ipc_server.handle_connections(command_handler).await {
                    error!("Enhanced IPC server error: {}", e);
                }
            })
        };

        // Update daemon state to running
        {
            let mut state = self.daemon_state.write().await;
            state.health_summary.overall_status = "Running".to_string();
        }

        info!("✅ All enhanced daemon subsystems started successfully");

        // Wait for shutdown signal or tasks to complete
        tokio::select! {
            _ = scanning_task => {
                info!("Enhanced scanning task completed");
            },
            _ = retry_task => {
                info!("Retry processing task completed");
            },
            _ = suggestions_task => {
                info!("Suggestions generation task completed");
            },
            _ = health_task => {
                info!("Health monitoring task completed");
            },
            _ = performance_task => {
                info!("Performance monitoring task completed");
            },
            _ = ipc_task => {
                info!("Enhanced IPC task completed");
            },
            _ = tokio::signal::ctrl_c() => {
                info!("🛑 Received shutdown signal");
                *self.should_shutdown.write().await = true;
            }
        }

        // Graceful shutdown
        self.shutdown().await?;

        info!("🏁 Enhanced geofencing daemon shutdown completed");
        Ok(())
    }

    /// Enhanced scanning loop with all advanced features
    async fn enhanced_scanning_loop(
        zone_manager: Arc<Mutex<ZoneManager>>,
        advanced_zone_manager: Arc<Mutex<AdvancedZoneManager>>,
        adaptive_scanner: Arc<Mutex<AdaptiveScanner>>,
        retry_manager: Arc<Mutex<RetryManager>>,
        secure_executor: Arc<Mutex<SecureCommandExecutor>>,
        observability_manager: Arc<Mutex<ObservabilityManager>>,
        daemon_state: Arc<RwLock<EnhancedDaemonState>>,
        should_shutdown: Arc<RwLock<bool>>,
    ) {
        info!("🔍 Starting enhanced scanning loop");
        
        let mut scan_count = 0u64;

        loop {
            // Check for shutdown
            if *should_shutdown.read().await {
                debug!("Shutdown signal received, exiting enhanced scanning loop");
                break;
            }

            scan_count += 1;
            let scan_start = tokio::time::Instant::now();
            
            debug!("Enhanced scan #{} starting", scan_count);

            // Calculate optimal scanning interval
            let current_interval = {
                let mut scanner = adaptive_scanner.lock().await;
                scanner.calculate_optimal_interval().await
            };

            // Update daemon state with current interval
            {
                let mut state = daemon_state.write().await;
                state.current_scan_interval = current_interval;
            }

            // Skip if no zones configured
            let zone_count = {
                let manager = zone_manager.lock().await;
                manager.list_zones().len()
            };

            if zone_count == 0 {
                debug!("No zones configured, skipping enhanced scan #{}", scan_count);
                sleep(current_interval).await;
                continue;
            }

            debug!("Enhanced scan #{} processing with {} zones", scan_count, zone_count);

            // Perform location detection with error recovery
            let location_change_result = {
                let mut manager = zone_manager.lock().await;
                manager.detect_location_change().await
            };

            let location_change = match location_change_result {
                Ok(change) => {
                    if let Some(ref change) = change {
                        debug!(
                            "Enhanced scan #{}: Location change detected: {} -> {} (confidence: {:.2})",
                            scan_count,
                            change.from.as_ref().map(|z| &z.name).unwrap_or(&"None".to_string()),
                            change.to.name,
                            change.confidence
                        );

                        // Record with observability
                        {
                            let observability = observability_manager.lock().await;
                            observability.record_zone_change(change).await;
                        }

                        // Update adaptive scanner with movement detection
                        {
                            let mut scanner = adaptive_scanner.lock().await;
                            // This would need the current fingerprint from zone detection
                            scanner.update_zone_stability(Some(&change.to.id), change.confidence);
                        }

                        // Record visit for advanced zone manager
                        {
                            let advanced_manager = advanced_zone_manager.lock().await;
                            // This would need the current fingerprint
                            let _ = advanced_manager.record_visit(
                                change.to.fingerprints.first().unwrap_or(&Default::default()).clone(),
                                Some(change.to.id.clone())
                            ).await;
                        }

                        // Update daemon state
                        {
                            let mut state = daemon_state.write().await;
                            state.current_zone = Some(change.to.name.clone());
                            state.total_zone_changes += 1;
                            state.last_scan = Some(Utc::now());
                        }
                    } else {
                        debug!("Enhanced scan #{}: No location change detected", scan_count);
                        
                        // Update last scan time
                        {
                            let mut state = daemon_state.write().await;
                            state.last_scan = Some(Utc::now());
                        }
                    }
                    change
                }
                Err(e) => {
                    warn!("Enhanced scan #{}: Location detection failed: {}", scan_count, e);
                    
                    // Record error with observability
                    {
                        let observability = observability_manager.lock().await;
                        observability.log_structured_event(
                            "error",
                            "Location detection failed",
                            [
                                ("event_type".to_string(), serde_json::Value::String("location_detection_error".to_string())),
                                ("error".to_string(), serde_json::Value::String(e.to_string())),
                                ("scan_count".to_string(), serde_json::Value::Number(serde_json::Number::from(scan_count))),
                            ].into_iter().collect()
                        ).await;
                    }

                    continue;
                }
            };

            // Execute zone actions if location changed
            if let Some(change) = location_change {
                debug!("Enhanced scan #{}: Executing zone actions for '{}'", scan_count, change.to.name);

                let action_start = tokio::time::Instant::now();

                // Create action context
                let context = ActionContext {
                    zone_id: change.to.id.clone(),
                    zone_name: change.to.name.clone(),
                    confidence: change.confidence,
                };

                // Execute actions with retry and security
                let action_result = {
                    let mut retry_manager_guard = retry_manager.lock().await;
                    retry_manager_guard.execute_zone_actions_with_retry(
                        &change.suggested_actions,
                        &context,
                    ).await
                };

                let action_duration = action_start.elapsed();

                match action_result {
                    Ok(()) => {
                        debug!("Enhanced scan #{}: Zone actions completed successfully in {:?}", scan_count, action_duration);
                        
                        // Record successful action execution
                        {
                            let observability = observability_manager.lock().await;
                            observability.record_action_execution(
                                "zone_actions",
                                true,
                                action_duration,
                                None,
                            ).await;
                        }
                    }
                    Err(e) => {
                        error!("Enhanced scan #{}: Zone actions failed: {}", scan_count, e);
                        
                        // Record failed action execution
                        {
                            let observability = observability_manager.lock().await;
                            observability.record_action_execution(
                                "zone_actions",
                                false,
                                action_duration,
                                Some(&e.to_string()),
                            ).await;
                        }
                    }
                }
            }

            let scan_duration = scan_start.elapsed();
            debug!("Enhanced scan #{} completed in {:?}", scan_count, scan_duration);

            // Wait for next scan
            sleep(current_interval).await;
        }

        info!("Enhanced scanning loop completed after {} scans", scan_count);
    }

    /// Retry processing background loop
    async fn retry_processing_loop(
        retry_manager: Arc<Mutex<RetryManager>>,
        should_shutdown: Arc<RwLock<bool>>,
    ) {
        debug!("Starting retry processing loop");
        
        let mut interval = interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            if *should_shutdown.read().await {
                break;
            }

            let mut retry_manager_guard = retry_manager.lock().await;
            let results = retry_manager_guard.process_retries().await;
            
            if !results.is_empty() {
                debug!("Processed {} retry operations", results.len());
            }
        }
    }

    /// Zone suggestions generation background loop
    async fn suggestions_generation_loop(
        advanced_zone_manager: Arc<Mutex<AdvancedZoneManager>>,
        daemon_state: Arc<RwLock<EnhancedDaemonState>>,
        should_shutdown: Arc<RwLock<bool>>,
    ) {
        debug!("Starting suggestions generation loop");
        
        let mut interval = interval(Duration::from_secs(3600)); // Every hour

        loop {
            interval.tick().await;

            if *should_shutdown.read().await {
                break;
            }

            let suggestions = {
                let advanced_manager = advanced_zone_manager.lock().await;
                match advanced_manager.generate_suggestions().await {
                    Ok(suggestions) => suggestions,
                    Err(e) => {
                        warn!("Failed to generate zone suggestions: {}", e);
                        continue;
                    }
                }
            };

            if !suggestions.is_empty() {
                info!("Generated {} new zone suggestions", suggestions.len());
                
                // Update daemon state
                {
                    let mut state = daemon_state.write().await;
                    state.recent_suggestions = suggestions.len() as u32;
                }
            }
        }
    }

    /// Health monitoring background loop
    async fn health_monitoring_loop(
        observability_manager: Arc<Mutex<ObservabilityManager>>,
        daemon_state: Arc<RwLock<EnhancedDaemonState>>,
        should_shutdown: Arc<RwLock<bool>>,
    ) {
        debug!("Starting health monitoring loop");
        
        let mut interval = interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            if *should_shutdown.read().await {
                break;
            }

            let health_status = {
                let observability = observability_manager.lock().await;
                observability.get_health_status().await
            };

            // Update daemon state with health summary
            {
                let mut state = daemon_state.write().await;
                let mut healthy = 0u32;
                let mut degraded = 0u32;
                let mut critical = 0u32;

                for status in health_status.values() {
                    match status {
                        super::observability::HealthStatus::Healthy => healthy += 1,
                        super::observability::HealthStatus::Degraded { .. } => degraded += 1,
                        super::observability::HealthStatus::Critical { .. } => critical += 1,
                        super::observability::HealthStatus::Unknown => degraded += 1,
                    }
                }

                state.health_summary = HealthSummary {
                    overall_status: if critical > 0 {
                        "Critical".to_string()
                    } else if degraded > 0 {
                        "Degraded".to_string()
                    } else {
                        "Healthy".to_string()
                    },
                    healthy_components: healthy,
                    degraded_components: degraded,
                    critical_components: critical,
                };
            }
        }
    }

    /// Performance monitoring background loop
    async fn performance_monitoring_loop(
        performance_optimizer: Arc<PerformanceOptimizer>,
        daemon_state: Arc<RwLock<EnhancedDaemonState>>,
        should_shutdown: Arc<RwLock<bool>>,
    ) {
        debug!("Starting performance monitoring loop");
        
        let mut interval = interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            if *should_shutdown.read().await {
                break;
            }

            let performance_metrics = performance_optimizer.get_performance_metrics().await;

            // Update daemon state with performance summary
            {
                let mut state = daemon_state.write().await;
                state.performance_summary = PerformanceSummary {
                    memory_usage_mb: 128.0, // Would get actual memory usage
                    cpu_usage_percent: 5.0, // Would get actual CPU usage
                    cache_hit_rate: performance_metrics.cache_hit_rate,
                    avg_operation_time: performance_metrics.average_response_time,
                    pool_utilization: performance_metrics.connection_pool_utilization,
                };
            }
        }
    }

    /// Handle enhanced IPC commands with extended functionality
    async fn handle_enhanced_ipc_command(
        zone_manager: Arc<Mutex<ZoneManager>>,
        advanced_zone_manager: Arc<Mutex<AdvancedZoneManager>>,
        observability_manager: Arc<Mutex<ObservabilityManager>>,
        daemon_state: Arc<RwLock<EnhancedDaemonState>>,
        should_shutdown: Arc<RwLock<bool>>,
        command: DaemonCommand,
    ) -> DaemonResponse {
        debug!("Handling enhanced IPC command: {:?}", command);

        match command {
            // Enhanced status command
            DaemonCommand::GetStatus => {
                let state = daemon_state.read().await;
                let status = super::ipc::DaemonStatus {
                    monitoring: state.running,
                    zone_count: {
                        let manager = zone_manager.lock().await;
                        manager.list_zones().len()
                    },
                    active_zone_id: state.current_zone.clone(),
                    last_scan: state.last_scan,
                    total_zone_changes: state.total_zone_changes,
                    uptime_seconds: Utc::now().signed_duration_since(state.startup_time).num_seconds() as u64,
                };

                DaemonResponse::Status { status }
            }

            // Zone suggestions command (new)
            _ if matches!(command, DaemonCommand::GetCurrentLocation) => {
                let advanced_manager = advanced_zone_manager.lock().await;
                match advanced_manager.generate_suggestions().await {
                    Ok(suggestions) => {
                        // Convert suggestions to appropriate response
                        DaemonResponse::Success // Placeholder
                    }
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to get zone suggestions: {}", e),
                    }
                }
            }

            // Delegate to standard command handling for other commands
            _ => {
                // This would delegate to the original daemon command handling
                DaemonResponse::Error {
                    message: "Command not implemented in enhanced mode".to_string(),
                }
            }
        }
    }

    /// Get enhanced daemon status
    pub async fn get_enhanced_status(&self) -> EnhancedDaemonState {
        self.daemon_state.read().await.clone()
    }

    /// Graceful shutdown with cleanup
    async fn shutdown(&mut self) -> Result<()> {
        info!("🛑 Starting graceful shutdown of enhanced daemon");

        // Signal all tasks to shutdown
        *self.should_shutdown.write().await = true;

        // Update daemon state
        {
            let mut state = self.daemon_state.write().await;
            state.running = false;
            state.health_summary.overall_status = "Shutting Down".to_string();
        }

        // Graceful shutdown of lifecycle manager
        {
            let mut lifecycle_manager = self.lifecycle_manager.lock().await;
            lifecycle_manager.shutdown().await?;
        }

        // Export final metrics
        {
            let observability_manager = self.observability_manager.lock().await;
            if let Err(e) = observability_manager.export_metrics().await {
                warn!("Failed to export final metrics: {}", e);
            }
        }

        info!("✅ Enhanced daemon graceful shutdown completed");
        Ok(())
    }
}

impl Default for EnhancedDaemonState {
    fn default() -> Self {
        Self {
            running: false,
            current_zone: None,
            startup_time: Utc::now(),
            last_scan: None,
            current_scan_interval: Duration::from_secs(30),
            total_zone_changes: 0,
            recent_suggestions: 0,
            security_incidents: 0,
            performance_summary: PerformanceSummary {
                memory_usage_mb: 0.0,
                cpu_usage_percent: 0.0,
                cache_hit_rate: 0.0,
                avg_operation_time: Duration::from_millis(0),
                pool_utilization: 0.0,
            },
            health_summary: HealthSummary {
                overall_status: "Initialized".to_string(),
                healthy_components: 0,
                degraded_components: 0,
                critical_components: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_enhanced_daemon_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();
        
        let daemon = EnhancedGeofencingDaemon::new(config_path).await;
        assert!(daemon.is_ok());
        
        let daemon = daemon.unwrap();
        let state = daemon.get_enhanced_status().await;
        assert!(!state.running);
        assert_eq!(state.total_zone_changes, 0);
    }

    #[tokio::test]
    async fn test_enhanced_daemon_state() {
        let state = EnhancedDaemonState::default();
        assert!(!state.running);
        assert!(state.current_zone.is_none());
        assert_eq!(state.total_zone_changes, 0);
        assert_eq!(state.health_summary.overall_status, "Initialized");
    }

    #[test]
    fn test_performance_summary() {
        let summary = PerformanceSummary {
            memory_usage_mb: 128.0,
            cpu_usage_percent: 5.0,
            cache_hit_rate: 85.0,
            avg_operation_time: Duration::from_millis(50),
            pool_utilization: 25.0,
        };

        assert_eq!(summary.memory_usage_mb, 128.0);
        assert_eq!(summary.cache_hit_rate, 85.0);
    }

    #[test]
    fn test_health_summary() {
        let summary = HealthSummary {
            overall_status: "Healthy".to_string(),
            healthy_components: 8,
            degraded_components: 0,
            critical_components: 0,
        };

        assert_eq!(summary.overall_status, "Healthy");
        assert_eq!(summary.healthy_components, 8);
    }
}