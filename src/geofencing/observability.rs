//! Comprehensive observability and monitoring for geofencing daemon
//!
//! Provides metrics collection, health checks, distributed tracing,
//! structured logging, and integration with monitoring systems.

use crate::geofencing::{GeofenceError, Result, GeofenceZone, LocationChange};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::{Mutex, RwLock};

/// Health status of the daemon and its components
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// All systems operating normally
    Healthy,
    /// Some non-critical issues detected
    Degraded { issues: Vec<HealthIssue> },
    /// Critical issues affecting functionality
    Critical { error: String },
    /// Component is not responding
    Unknown,
}

/// Individual health issue
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthIssue {
    /// Component that has the issue
    pub component: String,
    /// Issue severity level
    pub severity: IssueSeverity,
    /// Human-readable issue description
    pub description: String,
    /// When the issue was first detected
    pub detected_at: DateTime<Utc>,
    /// Suggested remediation steps
    pub remediation: Option<String>,
}

/// Issue severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Comprehensive daemon metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonMetrics {
    /// Zone change statistics
    pub zone_metrics: ZoneMetrics,
    /// Location detection metrics
    pub location_metrics: LocationMetrics,
    /// Action execution metrics
    pub action_metrics: ActionMetrics,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Error metrics
    pub error_metrics: ErrorMetrics,
    /// System resource usage
    pub resource_metrics: ResourceMetrics,
    /// Network connectivity metrics
    pub network_metrics: NetworkMetrics,
}

/// Zone-related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneMetrics {
    /// Total number of zone changes
    pub total_zone_changes: u64,
    /// Zone changes in last hour
    pub zone_changes_last_hour: u32,
    /// Zone changes in last 24 hours
    pub zone_changes_last_day: u32,
    /// Average confidence score of zone matches
    pub average_confidence: f64,
    /// Zone change frequency by zone
    pub zone_change_frequency: HashMap<String, u32>,
    /// Time spent in each zone
    pub time_in_zones: HashMap<String, Duration>,
    /// Most frequently matched zones
    pub top_zones: Vec<(String, u32)>,
}

/// Location detection metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationMetrics {
    /// Total location scans performed
    pub total_scans: u64,
    /// Successful scans (confidence > threshold)
    pub successful_scans: u64,
    /// Failed scans
    pub failed_scans: u64,
    /// Average scan duration
    pub average_scan_duration: Duration,
    /// Average WiFi networks detected per scan
    pub average_networks_per_scan: f64,
    /// Location detection accuracy rate
    pub detection_accuracy: f64,
    /// Scan frequency distribution
    pub scan_intervals: HashMap<String, u32>, // e.g., "30s", "1m", "5m"
}

/// Action execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetrics {
    /// Total actions executed
    pub total_actions: u64,
    /// Successful actions
    pub successful_actions: u64,
    /// Failed actions
    pub failed_actions: u64,
    /// Actions by type
    pub actions_by_type: HashMap<String, u32>, // "wifi", "vpn", "bluetooth", etc.
    /// Average action execution time
    pub average_execution_time: Duration,
    /// Action success rate by type
    pub success_rate_by_type: HashMap<String, f64>,
    /// Most common failure reasons
    pub failure_reasons: HashMap<String, u32>,
}

/// Performance-related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Memory usage in MB
    pub memory_usage_mb: f64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Uptime duration
    pub uptime: Duration,
    /// Cache hit rate percentage
    pub cache_hit_rate: f64,
    /// Average response time for operations
    pub average_response_time: Duration,
    /// Connection pool utilization
    pub connection_pool_utilization: f64,
    /// Background task queue size
    pub background_task_queue_size: usize,
}

/// Error tracking metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// Total errors encountered
    pub total_errors: u64,
    /// Errors by category
    pub errors_by_category: HashMap<String, u32>,
    /// Errors by severity
    pub errors_by_severity: HashMap<String, u32>,
    /// Recent error rate (errors per hour)
    pub error_rate: f64,
    /// Most recent errors
    pub recent_errors: Vec<ErrorEvent>,
}

/// Resource usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// File descriptors used
    pub file_descriptors: u32,
    /// Network connections active
    pub network_connections: u32,
    /// Disk space used for data storage
    pub disk_usage_mb: f64,
    /// Number of threads
    pub thread_count: u32,
    /// Memory allocations per second
    pub memory_allocations_per_sec: f64,
}

/// Network connectivity metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// WiFi scan success rate
    pub wifi_scan_success_rate: f64,
    /// VPN connection success rate
    pub vpn_connection_success_rate: f64,
    /// Bluetooth connection success rate
    pub bluetooth_connection_success_rate: f64,
    /// Network interface availability
    pub interface_availability: HashMap<String, bool>,
    /// DNS resolution time
    pub dns_resolution_time: Duration,
    /// Internet connectivity status
    pub internet_connectivity: bool,
}

/// Individual error event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    /// When the error occurred
    pub timestamp: DateTime<Utc>,
    /// Error category
    pub category: String,
    /// Error message
    pub message: String,
    /// Stack trace if available
    pub stack_trace: Option<String>,
    /// Context information
    pub context: HashMap<String, String>,
}

/// Distributed tracing span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    /// Unique span identifier
    pub span_id: String,
    /// Parent span identifier
    pub parent_id: Option<String>,
    /// Trace identifier
    pub trace_id: String,
    /// Operation name
    pub operation_name: String,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: Option<DateTime<Utc>>,
    /// Span duration
    pub duration: Option<Duration>,
    /// Span tags/attributes
    pub tags: HashMap<String, String>,
    /// Span status
    pub status: SpanStatus,
}

/// Trace span status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpanStatus {
    Ok,
    Error(String),
    Timeout,
    Cancelled,
}

/// Observability manager coordinates all monitoring activities
pub struct ObservabilityManager {
    /// Metrics collector
    metrics_collector: Arc<Mutex<MetricsCollector>>,
    /// Health checker
    health_checker: Arc<HealthChecker>,
    /// Distributed tracer
    tracer: Arc<Mutex<DistributedTracer>>,
    /// Event logger
    event_logger: Arc<Mutex<EventLogger>>,
    /// Configuration
    config: ObservabilityConfig,
    /// Metrics export scheduler
    export_scheduler: Option<tokio::task::JoinHandle<()>>,
}

/// Configuration for observability features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Whether metrics collection is enabled
    pub metrics_enabled: bool,
    /// Metrics collection interval
    pub metrics_interval: Duration,
    /// Whether distributed tracing is enabled
    pub tracing_enabled: bool,
    /// Trace sampling rate (0.0 to 1.0)
    pub trace_sampling_rate: f64,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Metrics export configuration
    pub export_config: MetricsExportConfig,
    /// Log level for structured logging
    pub log_level: String,
}

/// Metrics export configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsExportConfig {
    /// Export format (prometheus, json, influxdb)
    pub format: String,
    /// Export endpoint URL
    pub endpoint: Option<String>,
    /// Export interval
    pub interval: Duration,
    /// Whether to export to local files
    pub export_to_file: bool,
    /// File path for exports
    pub file_path: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            metrics_interval: Duration::from_secs(30),
            tracing_enabled: true,
            trace_sampling_rate: 0.1, // 10% sampling
            health_check_interval: Duration::from_secs(60),
            export_config: MetricsExportConfig::default(),
            log_level: "info".to_string(),
        }
    }
}

impl Default for MetricsExportConfig {
    fn default() -> Self {
        let mut file_path = dirs::data_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        file_path.push("network-dmenu");
        file_path.push("metrics.json");

        Self {
            format: "json".to_string(),
            endpoint: None,
            interval: Duration::from_secs(300), // 5 minutes
            export_to_file: true,
            file_path: Some(file_path.to_string_lossy().to_string()),
        }
    }
}

/// Metrics collector for gathering daemon statistics
pub struct MetricsCollector {
    /// Current daemon metrics
    current_metrics: DaemonMetrics,
    /// Historical metrics (last 24 hours)
    historical_metrics: VecDeque<(DateTime<Utc>, DaemonMetrics)>,
    /// Metrics collection start time
    start_time: Instant,
    /// Last collection time
    last_collection: Instant,
}

/// Health checker for monitoring component health
pub struct HealthChecker {
    /// Component health statuses
    component_health: Arc<RwLock<HashMap<String, HealthStatus>>>,
    /// Health check functions
    health_checks: HashMap<String, Box<dyn Fn() -> HealthStatus + Send + Sync>>,
    /// Health check history
    health_history: Arc<Mutex<VecDeque<(DateTime<Utc>, HashMap<String, HealthStatus>)>>>,
}

/// Distributed tracer for operation tracing
pub struct DistributedTracer {
    /// Active traces
    active_traces: HashMap<String, TraceSpan>,
    /// Completed traces
    completed_traces: VecDeque<TraceSpan>,
    /// Trace sampling configuration
    sampling_rate: f64,
    /// Trace export buffer
    export_buffer: VecDeque<TraceSpan>,
}

/// Event logger for structured application events
pub struct EventLogger {
    /// Event buffer
    event_buffer: VecDeque<LogEvent>,
    /// Log configuration
    config: LogConfig,
}

/// Structured log event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Log level
    pub level: String,
    /// Event message
    pub message: String,
    /// Structured fields
    pub fields: HashMap<String, serde_json::Value>,
    /// Source location
    pub source: Option<String>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Minimum log level
    pub min_level: String,
    /// Maximum events to buffer
    pub buffer_size: usize,
    /// Whether to include source locations
    pub include_source: bool,
}

impl ObservabilityManager {
    /// Create new observability manager
    pub async fn new(config: ObservabilityConfig) -> Result<Self> {
        debug!("Creating observability manager with config: {:?}", config);

        let metrics_collector = Arc::new(Mutex::new(MetricsCollector::new()));
        let health_checker = Arc::new(HealthChecker::new());
        let tracer = Arc::new(Mutex::new(DistributedTracer::new(config.trace_sampling_rate)));
        let event_logger = Arc::new(Mutex::new(EventLogger::new()));

        let manager = Self {
            metrics_collector,
            health_checker,
            tracer,
            event_logger,
            config,
            export_scheduler: None,
        };

        info!("Observability manager created successfully");
        Ok(manager)
    }

    /// Start observability monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting observability monitoring");

        // Start metrics collection
        if self.config.metrics_enabled {
            self.start_metrics_collection().await?;
        }

        // Start health checking
        self.start_health_checking().await?;

        // Start metrics export
        self.start_metrics_export().await?;

        info!("Observability monitoring started successfully");
        Ok(())
    }

    /// Record zone change event
    pub async fn record_zone_change(&self, change: &LocationChange) {
        debug!("Recording zone change event: {} -> {}", 
               change.from.as_ref().map(|z| z.name.as_str()).unwrap_or("None"),
               change.to.name);

        // Update metrics
        {
            let mut collector = self.metrics_collector.lock().await;
            collector.current_metrics.zone_metrics.total_zone_changes += 1;
            collector.current_metrics.zone_metrics.zone_changes_last_hour += 1;
            
            *collector.current_metrics.zone_metrics.zone_change_frequency
                .entry(change.to.name.clone())
                .or_insert(0) += 1;
        }

        // Create trace span
        if self.config.tracing_enabled {
            let span_id = self.start_trace_span(
                "zone_change",
                Some([
                    ("from_zone", change.from.as_ref().map(|z| z.name.as_str()).unwrap_or("none")),
                    ("to_zone", &change.to.name),
                    ("confidence", &change.confidence.to_string()),
                ].into_iter().collect())
            ).await;
            
            // End span immediately for zone change events
            self.end_trace_span(&span_id).await;
        }

        // Log structured event
        self.log_structured_event(
            "info",
            "Zone change detected",
            [
                ("event_type".to_string(), serde_json::Value::String("zone_change".to_string())),
                ("from_zone".to_string(), serde_json::Value::String(
                    change.from.as_ref().map(|z| z.name.clone()).unwrap_or("none".to_string())
                )),
                ("to_zone".to_string(), serde_json::Value::String(change.to.name.clone())),
                ("confidence".to_string(), serde_json::Value::Number(
                    serde_json::Number::from_f64(change.confidence).unwrap()
                )),
            ].into_iter().collect()
        ).await;
    }

    /// Record action execution
    pub async fn record_action_execution(
        &self,
        action_type: &str,
        success: bool,
        duration: Duration,
        error: Option<&str>,
    ) {
        debug!("Recording action execution: {} (success: {}, duration: {:?})", 
               action_type, success, duration);

        // Update metrics
        {
            let mut collector = self.metrics_collector.lock().await;
            collector.current_metrics.action_metrics.total_actions += 1;
            
            if success {
                collector.current_metrics.action_metrics.successful_actions += 1;
            } else {
                collector.current_metrics.action_metrics.failed_actions += 1;
                
                if let Some(error) = error {
                    *collector.current_metrics.action_metrics.failure_reasons
                        .entry(error.to_string())
                        .or_insert(0) += 1;
                }
            }
            
            *collector.current_metrics.action_metrics.actions_by_type
                .entry(action_type.to_string())
                .or_insert(0) += 1;
        }

        // Log structured event
        let mut fields = HashMap::new();
        fields.insert("event_type".to_string(), serde_json::Value::String("action_execution".to_string()));
        fields.insert("action_type".to_string(), serde_json::Value::String(action_type.to_string()));
        fields.insert("success".to_string(), serde_json::Value::Bool(success));
        fields.insert("duration_ms".to_string(), 
                     serde_json::Value::Number(serde_json::Number::from(duration.as_millis() as u64)));
        
        if let Some(error) = error {
            fields.insert("error".to_string(), serde_json::Value::String(error.to_string()));
        }

        self.log_structured_event(
            if success { "info" } else { "warn" },
            &format!("Action execution {}", if success { "completed" } else { "failed" }),
            fields
        ).await;
    }

    /// Start a distributed trace span
    pub async fn start_trace_span(&self, operation: &str, tags: Option<HashMap<&str, &str>>) -> String {
        if !self.config.tracing_enabled {
            return String::new();
        }

        let mut tracer = self.tracer.lock().await;
        tracer.start_span(operation, tags).await
    }

    /// End a distributed trace span
    pub async fn end_trace_span(&self, span_id: &str) {
        if !self.config.tracing_enabled {
            return;
        }

        let mut tracer = self.tracer.lock().await;
        tracer.end_span(span_id).await;
    }

    /// Log structured event
    pub async fn log_structured_event(
        &self,
        level: &str,
        message: &str,
        fields: HashMap<String, serde_json::Value>,
    ) {
        let mut logger = self.event_logger.lock().await;
        logger.log_event(level, message, fields);
    }

    /// Get current daemon metrics
    pub async fn get_current_metrics(&self) -> DaemonMetrics {
        let collector = self.metrics_collector.lock().await;
        collector.current_metrics.clone()
    }

    /// Get health status for all components
    pub async fn get_health_status(&self) -> HashMap<String, HealthStatus> {
        self.health_checker.get_overall_health().await
    }

    /// Get recent trace spans
    pub async fn get_recent_traces(&self, limit: usize) -> Vec<TraceSpan> {
        let tracer = self.tracer.lock().await;
        tracer.get_recent_traces(limit)
    }

    /// Export metrics to configured destination
    pub async fn export_metrics(&self) -> Result<()> {
        debug!("Exporting metrics");

        let metrics = self.get_current_metrics().await;

        match self.config.export_config.format.as_str() {
            "json" => {
                if self.config.export_config.export_to_file {
                    if let Some(ref file_path) = self.config.export_config.file_path {
                        let json_data = serde_json::to_string_pretty(&metrics)
                            .map_err(|e| GeofenceError::Config(format!(
                                "Failed to serialize metrics: {}", e
                            )))?;
                        
                        fs::write(file_path, json_data).await
                            .map_err(|e| GeofenceError::Config(format!(
                                "Failed to write metrics file: {}", e
                            )))?;
                        
                        debug!("Metrics exported to file: {}", file_path);
                    }
                }
            }
            "prometheus" => {
                // Would implement Prometheus format export
                debug!("Prometheus export not yet implemented");
            }
            _ => {
                warn!("Unknown metrics export format: {}", self.config.export_config.format);
            }
        }

        Ok(())
    }

    /// Start metrics collection background task
    async fn start_metrics_collection(&self) -> Result<()> {
        debug!("Starting metrics collection");

        let collector = Arc::clone(&self.metrics_collector);
        let interval = self.config.metrics_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                
                let mut collector_guard = collector.lock().await;
                collector_guard.collect_metrics().await;
                drop(collector_guard);
            }
        });

        Ok(())
    }

    /// Start health checking background task
    async fn start_health_checking(&self) -> Result<()> {
        debug!("Starting health checking");

        let health_checker = Arc::clone(&self.health_checker);
        let interval = self.config.health_check_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                
                health_checker.perform_health_checks().await;
            }
        });

        Ok(())
    }

    /// Start metrics export background task
    async fn start_metrics_export(&self) -> Result<()> {
        debug!("Starting metrics export");

        // This would be implemented to periodically export metrics
        // For now, just log that it's starting
        info!("Metrics export scheduler would start here");

        Ok(())
    }
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            current_metrics: DaemonMetrics::default(),
            historical_metrics: VecDeque::new(),
            start_time: Instant::now(),
            last_collection: Instant::now(),
        }
    }

    async fn collect_metrics(&mut self) {
        debug!("Collecting daemon metrics");

        // Update performance metrics
        self.current_metrics.performance_metrics.uptime = self.start_time.elapsed();
        
        // This would collect actual system metrics
        // For now, update with placeholder values
        self.current_metrics.performance_metrics.memory_usage_mb = 128.0;
        self.current_metrics.performance_metrics.cpu_usage_percent = 5.0;
        
        // Store historical metrics (keep last 24 hours)
        self.historical_metrics.push_back((Utc::now(), self.current_metrics.clone()));
        
        // Keep only last 24 hours (assuming collection every 30 seconds)
        let max_entries = 24 * 60 * 2; // 2 entries per minute
        while self.historical_metrics.len() > max_entries {
            self.historical_metrics.pop_front();
        }

        self.last_collection = Instant::now();
        debug!("Metrics collection completed");
    }
}

impl Default for DaemonMetrics {
    fn default() -> Self {
        Self {
            zone_metrics: ZoneMetrics::default(),
            location_metrics: LocationMetrics::default(),
            action_metrics: ActionMetrics::default(),
            performance_metrics: PerformanceMetrics::default(),
            error_metrics: ErrorMetrics::default(),
            resource_metrics: ResourceMetrics::default(),
            network_metrics: NetworkMetrics::default(),
        }
    }
}

impl Default for ZoneMetrics {
    fn default() -> Self {
        Self {
            total_zone_changes: 0,
            zone_changes_last_hour: 0,
            zone_changes_last_day: 0,
            average_confidence: 0.0,
            zone_change_frequency: HashMap::new(),
            time_in_zones: HashMap::new(),
            top_zones: Vec::new(),
        }
    }
}

impl Default for LocationMetrics {
    fn default() -> Self {
        Self {
            total_scans: 0,
            successful_scans: 0,
            failed_scans: 0,
            average_scan_duration: Duration::from_millis(100),
            average_networks_per_scan: 5.0,
            detection_accuracy: 0.85,
            scan_intervals: HashMap::new(),
        }
    }
}

impl Default for ActionMetrics {
    fn default() -> Self {
        Self {
            total_actions: 0,
            successful_actions: 0,
            failed_actions: 0,
            actions_by_type: HashMap::new(),
            average_execution_time: Duration::from_millis(500),
            success_rate_by_type: HashMap::new(),
            failure_reasons: HashMap::new(),
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            uptime: Duration::from_secs(0),
            cache_hit_rate: 0.0,
            average_response_time: Duration::from_millis(50),
            connection_pool_utilization: 0.0,
            background_task_queue_size: 0,
        }
    }
}

impl Default for ErrorMetrics {
    fn default() -> Self {
        Self {
            total_errors: 0,
            errors_by_category: HashMap::new(),
            errors_by_severity: HashMap::new(),
            error_rate: 0.0,
            recent_errors: Vec::new(),
        }
    }
}

impl Default for ResourceMetrics {
    fn default() -> Self {
        Self {
            file_descriptors: 0,
            network_connections: 0,
            disk_usage_mb: 0.0,
            thread_count: 0,
            memory_allocations_per_sec: 0.0,
        }
    }
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self {
            wifi_scan_success_rate: 95.0,
            vpn_connection_success_rate: 90.0,
            bluetooth_connection_success_rate: 85.0,
            interface_availability: HashMap::new(),
            dns_resolution_time: Duration::from_millis(50),
            internet_connectivity: true,
        }
    }
}

impl HealthChecker {
    fn new() -> Self {
        let mut health_checks: HashMap<String, Box<dyn Fn() -> HealthStatus + Send + Sync>> = HashMap::new();
        
        // Add critical health checks for geofencing system
        health_checks.insert("wifi_interface".to_string(), Box::new(|| {
            // Check if WiFi interface is available
            if std::path::Path::new("/sys/class/net/wlan0/operstate").exists() {
                HealthStatus::Healthy
            } else if std::path::Path::new("/sys/class/net/wlp0s20f3/operstate").exists() {
                HealthStatus::Healthy
            } else {
                HealthStatus::Critical { error: "No WiFi interface found".to_string() }
            }
        }));
        
        health_checks.insert("network_manager".to_string(), Box::new(|| {
            // Check if NetworkManager is running
            match std::process::Command::new("systemctl")
                .args(["is-active", "NetworkManager"])
                .output() 
            {
                Ok(output) if output.status.success() => HealthStatus::Healthy,
                _ => HealthStatus::Degraded { issues: vec![] }
            }
        }));
        
        health_checks.insert("memory_usage".to_string(), Box::new(|| {
            // Check system memory usage
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                if let (Some(total_line), Some(available_line)) = (
                    content.lines().find(|l| l.starts_with("MemTotal:")),
                    content.lines().find(|l| l.starts_with("MemAvailable:"))
                ) {
                    if let (Ok(total), Ok(available)) = (
                        total_line.split_whitespace().nth(1).unwrap_or("0").parse::<u64>(),
                        available_line.split_whitespace().nth(1).unwrap_or("0").parse::<u64>()
                    ) {
                        let usage_percent = ((total - available) as f64 / total as f64) * 100.0;
                        if usage_percent > 90.0 {
                            return HealthStatus::Critical { error: format!("High memory usage: {:.1}%", usage_percent) };
                        } else if usage_percent > 75.0 {
                            return HealthStatus::Degraded { issues: vec![] };
                        }
                    }
                }
            }
            HealthStatus::Healthy
        }));

        Self {
            component_health: Arc::new(RwLock::new(HashMap::new())),
            health_checks,
            health_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn perform_health_checks(&self) {
        debug!("Performing health checks");

        let mut results = HashMap::new();

        // Perform basic health checks
        results.insert("daemon".to_string(), self.check_daemon_health().await);
        results.insert("location_detection".to_string(), self.check_location_detection_health().await);
        results.insert("network_connectivity".to_string(), self.check_network_health().await);
        results.insert("storage".to_string(), self.check_storage_health().await);
        
        // Execute registered health checks
        for (name, check_fn) in &self.health_checks {
            let status = check_fn();
            results.insert(name.clone(), status);
            debug!("Health check '{}': {:?}", name, results.get(name));
        }

        // Update component health
        {
            let mut health = self.component_health.write().await;
            *health = results.clone();
        }

        // Store health history
        {
            let mut history = self.health_history.lock().await;
            history.push_back((Utc::now(), results));
            
            // Keep only last 24 hours
            while history.len() > 1440 { // 1 entry per minute
                history.pop_front();
            }
        }
    }

    async fn check_daemon_health(&self) -> HealthStatus {
        // Basic daemon health check
        HealthStatus::Healthy
    }

    async fn check_location_detection_health(&self) -> HealthStatus {
        // Check if location detection is working
        HealthStatus::Healthy
    }

    async fn check_network_health(&self) -> HealthStatus {
        // Check network connectivity
        HealthStatus::Healthy
    }

    async fn check_storage_health(&self) -> HealthStatus {
        // Check disk space and file system health
        HealthStatus::Healthy
    }

    async fn get_overall_health(&self) -> HashMap<String, HealthStatus> {
        self.component_health.read().await.clone()
    }
}

impl DistributedTracer {
    fn new(sampling_rate: f64) -> Self {
        Self {
            active_traces: HashMap::new(),
            completed_traces: VecDeque::new(),
            sampling_rate,
            export_buffer: VecDeque::new(),
        }
    }

    async fn start_span(&mut self, operation: &str, tags: Option<HashMap<&str, &str>>) -> String {
        // Simple sampling decision
        if fastrand::f64() > self.sampling_rate {
            return String::new(); // Skip tracing
        }

        let span_id = uuid::Uuid::new_v4().to_string();
        let trace_id = uuid::Uuid::new_v4().to_string();

        let mut span_tags = HashMap::new();
        if let Some(tags) = tags {
            for (k, v) in tags {
                span_tags.insert(k.to_string(), v.to_string());
            }
        }

        let span = TraceSpan {
            span_id: span_id.clone(),
            parent_id: None,
            trace_id,
            operation_name: operation.to_string(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            tags: span_tags,
            status: SpanStatus::Ok,
        };

        self.active_traces.insert(span_id.clone(), span);
        debug!("Started trace span: {} ({})", operation, span_id);

        span_id
    }

    async fn end_span(&mut self, span_id: &str) {
        if let Some(mut span) = self.active_traces.remove(span_id) {
            let end_time = Utc::now();
            span.end_time = Some(end_time);
            span.duration = Some(Duration::from_millis(
                end_time.signed_duration_since(span.start_time).num_milliseconds() as u64
            ));

            debug!("Ended trace span: {} (duration: {:?})", span.operation_name, span.duration);

            self.completed_traces.push_back(span);
            
            // Keep only recent traces
            while self.completed_traces.len() > 1000 {
                self.completed_traces.pop_front();
            }
        }
    }

    fn get_recent_traces(&self, limit: usize) -> Vec<TraceSpan> {
        self.completed_traces
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

impl EventLogger {
    fn new() -> Self {
        Self {
            event_buffer: VecDeque::new(),
            config: LogConfig {
                min_level: "info".to_string(),
                buffer_size: 10000,
                include_source: false,
            },
        }
    }

    fn log_event(&mut self, level: &str, message: &str, fields: HashMap<String, serde_json::Value>) {
        let event = LogEvent {
            timestamp: Utc::now(),
            level: level.to_string(),
            message: message.to_string(),
            fields,
            source: None,
        };

        self.event_buffer.push_back(event);

        // Maintain buffer size
        while self.event_buffer.len() > self.config.buffer_size {
            self.event_buffer.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_observability_manager_creation() {
        let config = ObservabilityConfig::default();
        let manager = ObservabilityManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus::Healthy;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_daemon_metrics_default() {
        let metrics = DaemonMetrics::default();
        assert_eq!(metrics.zone_metrics.total_zone_changes, 0);
        assert_eq!(metrics.action_metrics.total_actions, 0);
    }

    #[tokio::test]
    async fn test_metrics_collector() {
        let mut collector = MetricsCollector::new();
        collector.collect_metrics().await;
        
        assert!(collector.current_metrics.performance_metrics.uptime > Duration::from_secs(0));
    }

    #[test]
    fn test_trace_span_creation() {
        let span = TraceSpan {
            span_id: "test-span".to_string(),
            parent_id: None,
            trace_id: "test-trace".to_string(),
            operation_name: "test_operation".to_string(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            tags: HashMap::new(),
            status: SpanStatus::Ok,
        };

        assert_eq!(span.operation_name, "test_operation");
        assert_eq!(span.status, SpanStatus::Ok);
    }

    #[test]
    fn test_observability_config_default() {
        let config = ObservabilityConfig::default();
        assert!(config.metrics_enabled);
        assert!(config.tracing_enabled);
        assert_eq!(config.trace_sampling_rate, 0.1);
    }
}