//! Performance optimizations for geofencing operations
//!
//! Includes connection pooling, batch operations, intelligent caching,
//! and asynchronous processing for improved daemon performance.

use crate::geofencing::{GeofenceError, Result, LocationFingerprint, NetworkSignature};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::time::{sleep, timeout, Instant};

/// Network state cache for avoiding repeated queries
#[derive(Debug, Clone)]
pub struct NetworkStateCache {
    /// Current WiFi SSID
    current_ssid: Option<String>,
    /// Connected VPN profiles
    connected_vpns: Vec<String>,
    /// Bluetooth device states
    bluetooth_devices: HashMap<String, BluetoothDeviceState>,
    /// Network interface states
    interface_states: HashMap<String, NetworkInterfaceState>,
    /// Cache timestamp
    cache_time: DateTime<Utc>,
    /// Cache validity duration
    cache_ttl: ChronoDuration,
}

/// Bluetooth device connection state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BluetoothDeviceState {
    Connected,
    Disconnected,
    Connecting,
    Unknown,
}

/// Network interface state information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkInterfaceState {
    /// Whether interface is up
    pub is_up: bool,
    /// IP addresses assigned to interface
    pub ip_addresses: Vec<String>,
    /// Interface type (ethernet, wifi, etc.)
    pub interface_type: String,
    /// Link speed if available
    pub link_speed: Option<u64>,
}

/// Connection pool for managing command execution resources
pub struct ConnectionPool {
    /// Maximum concurrent operations
    max_concurrent: usize,
    /// Semaphore for limiting concurrent operations
    semaphore: Arc<Semaphore>,
    /// Active connection metrics
    active_connections: Arc<RwLock<ConnectionMetrics>>,
    /// Connection reuse pool for expensive operations
    reuse_pool: Arc<Mutex<HashMap<String, PooledConnection>>>,
}

/// Metrics for connection pool monitoring
#[derive(Debug, Default, Clone)]
struct ConnectionMetrics {
    total_connections: u64,
    active_connections: u32,
    peak_connections: u32,
    connection_timeouts: u64,
    connection_errors: u64,
}

/// Pooled connection for reuse
struct PooledConnection {
    connection_type: String,
    created_at: Instant,
    last_used: Instant,
    usage_count: u32,
}

/// Batch operations processor for efficient bulk actions
pub struct BatchProcessor {
    /// Batch size for WiFi operations
    wifi_batch_size: usize,
    /// Batch size for Bluetooth operations
    bluetooth_batch_size: usize,
    /// Maximum batch wait time
    max_batch_wait: Duration,
    /// Pending WiFi operations
    pending_wifi_ops: Arc<Mutex<Vec<WiFiOperation>>>,
    /// Pending Bluetooth operations
    pending_bluetooth_ops: Arc<Mutex<Vec<BluetoothOperation>>>,
}

/// WiFi operation for batching
#[derive(Debug, Clone)]
pub struct WiFiOperation {
    pub operation_type: WiFiOperationType,
    pub ssid: String,
    pub priority: u8,
    pub created_at: Instant,
}

/// Bluetooth operation for batching
#[derive(Debug, Clone)]
pub struct BluetoothOperation {
    pub operation_type: BluetoothOperationType,
    pub device_name: String,
    pub device_address: Option<String>,
    pub priority: u8,
    pub created_at: Instant,
}

/// WiFi operation types
#[derive(Debug, Clone, PartialEq)]
pub enum WiFiOperationType {
    Scan,
    Connect(String), // SSID
    Disconnect,
    GetStatus,
}

/// Bluetooth operation types
#[derive(Debug, Clone, PartialEq)]
pub enum BluetoothOperationType {
    Scan,
    Connect(String), // Device address
    Disconnect(String), // Device address
    GetDevices,
}

/// Intelligent cache manager with TTL and invalidation strategies
pub struct CacheManager {
    /// WiFi fingerprint cache
    fingerprint_cache: Arc<RwLock<HashMap<String, CachedFingerprint>>>,
    /// Network state cache
    network_state_cache: Arc<RwLock<NetworkStateCache>>,
    /// Zone match cache
    zone_match_cache: Arc<RwLock<HashMap<String, CachedZoneMatch>>>,
    /// Cache statistics
    cache_stats: Arc<RwLock<CacheStatistics>>,
    /// Cache configuration
    config: CacheConfig,
}

/// Cached location fingerprint with metadata
#[derive(Debug, Clone)]
struct CachedFingerprint {
    fingerprint: LocationFingerprint,
    created_at: Instant,
    access_count: u32,
    last_accessed: Instant,
}

/// Cached zone match result
#[derive(Debug, Clone)]
struct CachedZoneMatch {
    zone_id: Option<String>,
    confidence: f64,
    created_at: Instant,
    fingerprint_hash: String,
}

/// Cache statistics for monitoring
#[derive(Debug, Default, Clone)]
struct CacheStatistics {
    fingerprint_hits: u64,
    fingerprint_misses: u64,
    zone_match_hits: u64,
    zone_match_misses: u64,
    cache_evictions: u64,
    cache_invalidations: u64,
}

/// Cache configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheConfig {
    /// Fingerprint cache TTL in seconds
    #[serde(with = "duration_seconds")]
    pub fingerprint_ttl: Duration,
    /// Network state cache TTL in seconds
    #[serde(with = "duration_seconds")]
    pub network_state_ttl: Duration,
    /// Zone match cache TTL in seconds
    #[serde(with = "duration_seconds")]
    pub zone_match_ttl: Duration,
    /// Maximum cache entries
    pub max_entries: usize,
    /// Cache cleanup interval in seconds
    #[serde(with = "duration_seconds")]
    pub cleanup_interval: Duration,
}

mod duration_seconds {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            fingerprint_ttl: Duration::from_secs(60),        // 1 minute
            network_state_ttl: Duration::from_secs(30),      // 30 seconds
            zone_match_ttl: Duration::from_secs(120),        // 2 minutes
            max_entries: 1000,
            cleanup_interval: Duration::from_secs(300),      // 5 minutes
        }
    }
}

/// Asynchronous task manager for background operations
pub struct AsyncTaskManager {
    /// Task executor pool
    executor_pool: Arc<tokio::task::JoinSet<TaskResult>>,
    /// Active tasks tracking
    active_tasks: Arc<RwLock<HashMap<String, TaskMetadata>>>,
    /// Task completion notifications
    task_notifications: Arc<tokio::sync::broadcast::Sender<TaskNotification>>,
    /// Maximum concurrent background tasks
    max_concurrent_tasks: usize,
}

/// Background task result
#[derive(Debug)]
pub enum TaskResult {
    Success(String),
    Failed(String, String), // task_id, error
    Timeout(String),        // task_id
}

/// Task metadata for tracking
#[derive(Debug, Clone)]
struct TaskMetadata {
    task_id: String,
    task_type: String,
    started_at: Instant,
    priority: u8,
}

/// Task completion notification
#[derive(Debug, Clone)]
pub struct TaskNotification {
    pub task_id: String,
    pub result: String,
    pub completed_at: DateTime<Utc>,
}

impl NetworkStateCache {
    /// Create new network state cache
    pub fn new(cache_ttl: ChronoDuration) -> Self {
        Self {
            current_ssid: None,
            connected_vpns: Vec::new(),
            bluetooth_devices: HashMap::new(),
            interface_states: HashMap::new(),
            cache_time: Utc::now(),
            cache_ttl,
        }
    }

    /// Check if cache is still valid
    pub fn is_valid(&self) -> bool {
        Utc::now().signed_duration_since(self.cache_time) < self.cache_ttl
    }

    /// Update WiFi SSID in cache
    pub fn update_wifi_ssid(&mut self, ssid: Option<String>) {
        debug!("Updating WiFi SSID in cache: {:?}", ssid);
        self.current_ssid = ssid;
        self.cache_time = Utc::now();
    }

    /// Update VPN connections in cache
    pub fn update_vpn_connections(&mut self, vpns: Vec<String>) {
        debug!("Updating VPN connections in cache: {:?}", vpns);
        self.connected_vpns = vpns;
        self.cache_time = Utc::now();
    }

    /// Update Bluetooth device state
    pub fn update_bluetooth_device(&mut self, device_name: String, state: BluetoothDeviceState) {
        debug!("Updating Bluetooth device '{}' state: {:?}", device_name, state);
        self.bluetooth_devices.insert(device_name, state);
        self.cache_time = Utc::now();
    }

    /// Get current WiFi SSID if cached
    pub fn get_current_ssid(&self) -> Option<&String> {
        if self.is_valid() {
            self.current_ssid.as_ref()
        } else {
            None
        }
    }

    /// Get connected VPN profiles if cached
    pub fn get_connected_vpns(&self) -> Option<&Vec<String>> {
        if self.is_valid() {
            Some(&self.connected_vpns)
        } else {
            None
        }
    }

    /// Get Bluetooth device state if cached
    pub fn get_bluetooth_device_state(&self, device_name: &str) -> Option<&BluetoothDeviceState> {
        if self.is_valid() {
            self.bluetooth_devices.get(device_name)
        } else {
            None
        }
    }

    /// Clear cache (force refresh)
    pub fn invalidate(&mut self) {
        debug!("Invalidating network state cache");
        self.cache_time = Utc::now() - self.cache_ttl - ChronoDuration::seconds(1);
    }
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(max_concurrent: usize) -> Self {
        debug!("Creating connection pool with max_concurrent: {}", max_concurrent);
        
        Self {
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_connections: Arc::new(RwLock::new(ConnectionMetrics::default())),
            reuse_pool: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute operation with connection pool management
    pub async fn execute_with_pool<F, T>(&self, operation_type: &str, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        debug!("Executing operation '{}' with connection pool", operation_type);

        // Acquire semaphore permit
        let _permit = match timeout(Duration::from_secs(30), self.semaphore.acquire()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return Err(GeofenceError::ActionExecution(
                    "Connection pool semaphore closed".to_string()
                ));
            }
            Err(_) => {
                // Update timeout metrics
                {
                    let mut metrics = self.active_connections.write().await;
                    metrics.connection_timeouts += 1;
                }
                
                return Err(GeofenceError::ActionExecution(
                    "Connection pool timeout waiting for available slot".to_string()
                ));
            }
        };

        // Check for connection reuse opportunity
        let connection_key = format!("{}_{}", operation_type, std::process::id());
        let reuse_info = {
            let mut reuse_pool = self.reuse_pool.lock().await;
            if let Some(connection) = reuse_pool.get_mut(&connection_key) {
                // Update existing connection usage
                connection.last_used = Instant::now();
                connection.usage_count += 1;
                debug!("Reusing connection for '{}', usage count: {}", operation_type, connection.usage_count);
                Some(("reused", connection.usage_count))
            } else {
                // Create new pooled connection
                let new_connection = PooledConnection {
                    connection_type: operation_type.to_string(),
                    created_at: Instant::now(),
                    last_used: Instant::now(),
                    usage_count: 1,
                };
                reuse_pool.insert(connection_key.clone(), new_connection);
                debug!("Created new pooled connection for '{}'", operation_type);
                Some(("new", 1))
            }
        };

        // Update active connection metrics
        {
            let mut metrics = self.active_connections.write().await;
            metrics.total_connections += 1;
            metrics.active_connections += 1;
            if metrics.active_connections > metrics.peak_connections {
                metrics.peak_connections = metrics.active_connections;
            }
        }

        // Execute operation
        let start_time = Instant::now();
        let result = operation.await;
        let duration = start_time.elapsed();
        
        // Log connection performance
        if let Some((reuse_type, usage_count)) = reuse_info {
            debug!("Operation '{}' completed in {:?} (connection: {}, usage: {})", 
                   operation_type, duration, reuse_type, usage_count);
        }

        // Update metrics
        {
            let mut metrics = self.active_connections.write().await;
            metrics.active_connections -= 1;
            
            if result.is_err() {
                metrics.connection_errors += 1;
            }
        }

        debug!("Operation '{}' completed in {:?} with result: {}", 
               operation_type, duration, if result.is_ok() { "success" } else { "error" });

        result
    }

    /// Get connection pool metrics
    pub async fn get_metrics(&self) -> ConnectionMetrics {
        self.active_connections.read().await.clone()
    }

    /// Get pool utilization as percentage
    pub async fn get_utilization(&self) -> f64 {
        let metrics = self.active_connections.read().await;
        (metrics.active_connections as f64 / self.max_concurrent as f64) * 100.0
    }

    /// Cleanup stale connections from reuse pool
    pub async fn cleanup_stale_connections(&self) {
        let mut reuse_pool = self.reuse_pool.lock().await;
        let now = Instant::now();
        let stale_threshold = Duration::from_secs(300); // 5 minutes
        
        let initial_count = reuse_pool.len();
        reuse_pool.retain(|connection_key, connection| {
            let is_stale = now.duration_since(connection.last_used) > stale_threshold;
            if is_stale {
                debug!("Removing stale connection: {} (unused for {:?})", 
                       connection_key, now.duration_since(connection.last_used));
            }
            !is_stale
        });
        
        let removed_count = initial_count - reuse_pool.len();
        if removed_count > 0 {
            debug!("Cleaned up {} stale connections from reuse pool", removed_count);
        }
    }
}

impl BatchProcessor {
    /// Create new batch processor
    pub fn new() -> Self {
        Self {
            wifi_batch_size: 5,
            bluetooth_batch_size: 3,
            max_batch_wait: Duration::from_millis(500),
            pending_wifi_ops: Arc::new(Mutex::new(Vec::new())),
            pending_bluetooth_ops: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add WiFi operation to batch queue
    pub async fn queue_wifi_operation(&self, operation: WiFiOperation) {
        debug!("Queuing WiFi operation: {:?}", operation.operation_type);
        
        let mut pending_ops = self.pending_wifi_ops.lock().await;
        pending_ops.push(operation);

        // Process batch if it's full
        if pending_ops.len() >= self.wifi_batch_size {
            let batch = pending_ops.drain(..).collect();
            drop(pending_ops);
            
            tokio::spawn(Self::process_wifi_batch(batch));
        }
    }

    /// Add Bluetooth operation to batch queue
    pub async fn queue_bluetooth_operation(&self, operation: BluetoothOperation) {
        debug!("Queuing Bluetooth operation: {:?}", operation.operation_type);
        
        let mut pending_ops = self.pending_bluetooth_ops.lock().await;
        pending_ops.push(operation);

        // Process batch if it's full
        if pending_ops.len() >= self.bluetooth_batch_size {
            let batch = pending_ops.drain(..).collect();
            drop(pending_ops);
            
            tokio::spawn(Self::process_bluetooth_batch(batch));
        }
    }

    /// Process pending batches (called periodically)
    pub async fn process_pending_batches(&self) {
        debug!("Processing pending batches");

        // Process WiFi batch
        {
            let mut pending_wifi = self.pending_wifi_ops.lock().await;
            if !pending_wifi.is_empty() {
                let batch = pending_wifi.drain(..).collect();
                drop(pending_wifi);
                tokio::spawn(Self::process_wifi_batch(batch));
            }
        }

        // Process Bluetooth batch
        {
            let mut pending_bluetooth = self.pending_bluetooth_ops.lock().await;
            if !pending_bluetooth.is_empty() {
                let batch = pending_bluetooth.drain(..).collect();
                drop(pending_bluetooth);
                tokio::spawn(Self::process_bluetooth_batch(batch));
            }
        }
    }

    /// Process a batch of WiFi operations
    async fn process_wifi_batch(operations: Vec<WiFiOperation>) {
        debug!("Processing WiFi batch with {} operations", operations.len());
        
        use crate::command::{CommandRunner, RealCommandRunner};
        let command_runner = RealCommandRunner;

        // Group operations by type for efficiency
        let mut scan_ops = Vec::new();
        let mut connect_ops = Vec::new();
        let mut status_ops = Vec::new();

        for op in operations {
            match op.operation_type {
                WiFiOperationType::Scan => scan_ops.push(op),
                WiFiOperationType::Connect(_) => connect_ops.push(op),
                WiFiOperationType::GetStatus => status_ops.push(op),
                _ => {}
            }
        }

        // Execute scans first (can be batched into single nmcli call)
        if !scan_ops.is_empty() {
            debug!("Executing batch WiFi scan for {} requests", scan_ops.len());
            if let Ok(output) = command_runner.run_command("nmcli", &["dev", "wifi", "list"]) {
                if output.status.success() {
                    debug!("Batch WiFi scan completed successfully");
                }
            }
        }

        // Execute connections sequentially (can't be easily batched)
        for op in connect_ops {
            if let WiFiOperationType::Connect(ssid) = op.operation_type {
                debug!("Executing WiFi connection to '{}'", ssid);
                let _ = command_runner.run_command("nmcli", &["dev", "wifi", "connect", &ssid]);
            }
        }

        // Execute status checks (can be batched into single call)
        if !status_ops.is_empty() {
            debug!("Executing batch WiFi status check for {} requests", status_ops.len());
            let _ = command_runner.run_command("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"]);
        }
    }

    /// Process a batch of Bluetooth operations
    async fn process_bluetooth_batch(operations: Vec<BluetoothOperation>) {
        debug!("Processing Bluetooth batch with {} operations", operations.len());
        
        use crate::command::{CommandRunner, RealCommandRunner};
        let command_runner = RealCommandRunner;

        // Group by operation type
        let mut scan_ops = Vec::new();
        let mut connect_ops = Vec::new();
        let mut device_ops = Vec::new();

        for op in operations {
            match op.operation_type {
                BluetoothOperationType::Scan => scan_ops.push(op),
                BluetoothOperationType::Connect(_) => connect_ops.push(op),
                BluetoothOperationType::GetDevices => device_ops.push(op),
                _ => {}
            }
        }

        // Execute device list first (single call serves all device requests)
        if !device_ops.is_empty() {
            debug!("Executing batch Bluetooth device list for {} requests", device_ops.len());
            let _ = command_runner.run_command("bluetoothctl", &["devices"]);
        }

        // Execute connections
        for op in connect_ops {
            if let BluetoothOperationType::Connect(address) = op.operation_type {
                debug!("Executing Bluetooth connection to '{}'", address);
                let _ = command_runner.run_command("bluetoothctl", &["connect", &address]);
            }
        }
    }

    /// Start batch processor background task
    pub async fn start_background_processing(&self) {
        debug!("Starting batch processor background task");

        let wifi_ops = Arc::clone(&self.pending_wifi_ops);
        let bluetooth_ops = Arc::clone(&self.pending_bluetooth_ops);
        let max_wait = self.max_batch_wait;

        tokio::spawn(async move {
            loop {
                sleep(max_wait).await;
                
                // Process WiFi batch if it has operations waiting too long
                {
                    let mut pending_wifi = wifi_ops.lock().await;
                    if !pending_wifi.is_empty() {
                        let oldest = &pending_wifi[0];
                        if oldest.created_at.elapsed() > max_wait {
                            let batch = pending_wifi.drain(..).collect();
                            drop(pending_wifi);
                            tokio::spawn(Self::process_wifi_batch(batch));
                        }
                    }
                }

                // Process Bluetooth batch if it has operations waiting too long
                {
                    let mut pending_bluetooth = bluetooth_ops.lock().await;
                    if !pending_bluetooth.is_empty() {
                        let oldest = &pending_bluetooth[0];
                        if oldest.created_at.elapsed() > max_wait {
                            let batch = pending_bluetooth.drain(..).collect();
                            drop(pending_bluetooth);
                            tokio::spawn(Self::process_bluetooth_batch(batch));
                        }
                    }
                }
            }
        });
    }
}

impl CacheManager {
    /// Create new cache manager
    pub fn new(config: CacheConfig) -> Self {
        debug!("Creating cache manager with config: {:?}", config);

        Self {
            fingerprint_cache: Arc::new(RwLock::new(HashMap::new())),
            network_state_cache: Arc::new(RwLock::new(NetworkStateCache::new(
                ChronoDuration::from_std(config.network_state_ttl).unwrap_or(ChronoDuration::seconds(30))
            ))),
            zone_match_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_stats: Arc::new(RwLock::new(CacheStatistics::default())),
            config,
        }
    }

    /// Get cached location fingerprint
    pub async fn get_cached_fingerprint(&self, cache_key: &str) -> Option<LocationFingerprint> {
        let cache = self.fingerprint_cache.read().await;
        
        if let Some(cached) = cache.get(cache_key) {
            if cached.created_at.elapsed() < self.config.fingerprint_ttl {
                // Update access metrics
                {
                    let mut stats = self.cache_stats.write().await;
                    stats.fingerprint_hits += 1;
                }
                
                debug!("Cache hit for fingerprint key: {}", cache_key);
                return Some(cached.fingerprint.clone());
            }
        }

        // Cache miss
        {
            let mut stats = self.cache_stats.write().await;
            stats.fingerprint_misses += 1;
        }
        
        debug!("Cache miss for fingerprint key: {}", cache_key);
        None
    }

    /// Cache location fingerprint
    pub async fn cache_fingerprint(&self, cache_key: String, fingerprint: LocationFingerprint) {
        debug!("Caching fingerprint for key: {}", cache_key);
        
        let cached_fp = CachedFingerprint {
            fingerprint,
            created_at: Instant::now(),
            access_count: 0,
            last_accessed: Instant::now(),
        };

        let mut cache = self.fingerprint_cache.write().await;
        cache.insert(cache_key, cached_fp);

        // Cleanup if cache is too large
        if cache.len() > self.config.max_entries {
            self.cleanup_fingerprint_cache(&mut cache).await;
        }
    }

    /// Get cached zone match result
    pub async fn get_cached_zone_match(&self, fingerprint_hash: &str) -> Option<(Option<String>, f64)> {
        let cache = self.zone_match_cache.read().await;
        
        if let Some(cached) = cache.get(fingerprint_hash) {
            if cached.created_at.elapsed() < self.config.zone_match_ttl {
                {
                    let mut stats = self.cache_stats.write().await;
                    stats.zone_match_hits += 1;
                }
                
                debug!("Cache hit for zone match: {}", fingerprint_hash);
                return Some((cached.zone_id.clone(), cached.confidence));
            }
        }

        {
            let mut stats = self.cache_stats.write().await;
            stats.zone_match_misses += 1;
        }
        
        debug!("Cache miss for zone match: {}", fingerprint_hash);
        None
    }

    /// Cache zone match result
    pub async fn cache_zone_match(&self, fingerprint_hash: String, zone_id: Option<String>, confidence: f64) {
        debug!("Caching zone match for fingerprint: {} -> {:?}", fingerprint_hash, zone_id);
        
        let cached_match = CachedZoneMatch {
            zone_id,
            confidence,
            created_at: Instant::now(),
            fingerprint_hash: fingerprint_hash.clone(),
        };

        let mut cache = self.zone_match_cache.write().await;
        cache.insert(fingerprint_hash, cached_match);
    }

    /// Get network state cache
    pub async fn get_network_state_cache(&self) -> NetworkStateCache {
        self.network_state_cache.read().await.clone()
    }

    /// Update network state cache
    pub async fn update_network_state_cache<F>(&self, update_fn: F)
    where
        F: FnOnce(&mut NetworkStateCache),
    {
        let mut cache = self.network_state_cache.write().await;
        update_fn(&mut cache);
    }

    /// Invalidate all caches
    pub async fn invalidate_all(&self) {
        debug!("Invalidating all caches");
        
        self.fingerprint_cache.write().await.clear();
        self.zone_match_cache.write().await.clear();
        self.network_state_cache.write().await.invalidate();

        {
            let mut stats = self.cache_stats.write().await;
            stats.cache_invalidations += 1;
        }
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStatistics {
        self.cache_stats.read().await.clone()
    }

    /// Start background cache cleanup task
    pub async fn start_cleanup_task(&self) {
        debug!("Starting cache cleanup background task");

        let fingerprint_cache = Arc::clone(&self.fingerprint_cache);
        let zone_match_cache = Arc::clone(&self.zone_match_cache);
        let cache_stats = Arc::clone(&self.cache_stats);
        let cleanup_interval = self.config.cleanup_interval;
        let fingerprint_ttl = self.config.fingerprint_ttl;
        let zone_match_ttl = self.config.zone_match_ttl;

        tokio::spawn(async move {
            loop {
                sleep(cleanup_interval).await;
                debug!("Running cache cleanup");

                // Cleanup fingerprint cache
                {
                    let mut cache = fingerprint_cache.write().await;
                    let initial_size = cache.len();
                    
                    cache.retain(|_, cached| cached.created_at.elapsed() < fingerprint_ttl);
                    
                    let evicted = initial_size - cache.len();
                    if evicted > 0 {
                        debug!("Evicted {} expired fingerprint cache entries", evicted);
                        let mut stats = cache_stats.write().await;
                        stats.cache_evictions += evicted as u64;
                    }
                }

                // Cleanup zone match cache
                {
                    let mut cache = zone_match_cache.write().await;
                    let initial_size = cache.len();
                    
                    cache.retain(|_, cached| cached.created_at.elapsed() < zone_match_ttl);
                    
                    let evicted = initial_size - cache.len();
                    if evicted > 0 {
                        debug!("Evicted {} expired zone match cache entries", evicted);
                        let mut stats = cache_stats.write().await;
                        stats.cache_evictions += evicted as u64;
                    }
                }
            }
        });
    }

    /// Cleanup fingerprint cache (LRU eviction)
    async fn cleanup_fingerprint_cache(&self, cache: &mut HashMap<String, CachedFingerprint>) {
        debug!("Cleaning up fingerprint cache (size: {})", cache.len());

        // Remove oldest entries (simple cleanup strategy)
        let target_size = (self.config.max_entries as f64 * 0.8) as usize;
        let mut entries: Vec<_> = cache.iter().map(|(k, v)| (k.clone(), v.last_accessed)).collect();
        
        // Sort by last accessed time
        entries.sort_by_key(|(_, last_accessed)| *last_accessed);
        
        // Remove oldest entries
        let to_remove = cache.len().saturating_sub(target_size);
        let keys_to_remove: Vec<_> = entries.iter().take(to_remove).map(|(k, _)| k.clone()).collect();
        
        for key in keys_to_remove {
            cache.remove(&key);
        }

        debug!("Cleaned up fingerprint cache, removed {} entries", to_remove);
        
        {
            let mut stats = self.cache_stats.write().await;
            stats.cache_evictions += to_remove as u64;
        }
    }
}

impl AsyncTaskManager {
    /// Create new async task manager
    pub fn new(max_concurrent_tasks: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(100);
        
        Self {
            executor_pool: Arc::new(tokio::task::JoinSet::new()),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            task_notifications: Arc::new(tx),
            max_concurrent_tasks,
        }
    }

    /// Submit background task for execution
    pub async fn submit_task<F>(&self, task_id: String, task_type: String, task: F) -> Result<()>
    where
        F: std::future::Future<Output = Result<String>> + Send + 'static,
    {
        debug!("Submitting background task: {} ({})", task_id, task_type);

        // Check if we're at capacity
        {
            let active_tasks = self.active_tasks.read().await;
            if active_tasks.len() >= self.max_concurrent_tasks {
                return Err(GeofenceError::ActionExecution(
                    "Task manager at capacity".to_string()
                ));
            }
        }

        // Add task metadata
        let task_metadata = TaskMetadata {
            task_id: task_id.clone(),
            task_type,
            started_at: Instant::now(),
            priority: 5, // Default priority
        };

        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(task_id.clone(), task_metadata);
        }

        // Submit task
        let active_tasks_clone = Arc::clone(&self.active_tasks);
        let notifications_clone = Arc::clone(&self.task_notifications);
        let task_id_clone = task_id.clone();

        tokio::spawn(async move {
            let result = task.await;
            
            // Remove from active tasks
            {
                let mut active_tasks = active_tasks_clone.write().await;
                active_tasks.remove(&task_id);
            }

            // Send notification
            let notification = TaskNotification {
                task_id: task_id_clone.clone(),
                result: match &result {
                    Ok(msg) => format!("Success: {}", msg),
                    Err(e) => format!("Failed: {}", e),
                },
                completed_at: Utc::now(),
            };

            let _ = notifications_clone.send(notification);

            match result {
                Ok(msg) => TaskResult::Success(msg),
                Err(e) => TaskResult::Failed(task_id_clone, e.to_string()),
            }
        });

        Ok(())
    }

    /// Get active task count
    pub async fn get_active_task_count(&self) -> usize {
        self.active_tasks.read().await.len()
    }

    /// Subscribe to task notifications
    pub fn subscribe_to_notifications(&self) -> tokio::sync::broadcast::Receiver<TaskNotification> {
        self.task_notifications.subscribe()
    }

    /// Get active tasks
    pub async fn get_active_tasks(&self) -> HashMap<String, TaskMetadata> {
        self.active_tasks.read().await.clone()
    }
}

/// Performance optimizer that combines all optimization strategies
pub struct PerformanceOptimizer {
    pub connection_pool: ConnectionPool,
    pub batch_processor: BatchProcessor,
    pub cache_manager: CacheManager,
    pub task_manager: AsyncTaskManager,
}

impl PerformanceOptimizer {
    /// Create new performance optimizer with default configuration
    pub async fn new() -> Self {
        let connection_pool = ConnectionPool::new(10); // 10 concurrent operations
        let batch_processor = BatchProcessor::new();
        let cache_manager = CacheManager::new(CacheConfig::default());
        let task_manager = AsyncTaskManager::new(5); // 5 background tasks

        // Start background tasks
        batch_processor.start_background_processing().await;
        cache_manager.start_cleanup_task().await;

        Self {
            connection_pool,
            batch_processor,
            cache_manager,
            task_manager,
        }
    }

    /// Get comprehensive performance metrics
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        let connection_metrics = self.connection_pool.get_metrics().await;
        let cache_stats = self.cache_manager.get_cache_stats().await;
        let active_tasks = self.task_manager.get_active_task_count().await;
        let pool_utilization = self.connection_pool.get_utilization().await;

        PerformanceMetrics {
            connection_pool_utilization: pool_utilization,
            active_background_tasks: active_tasks,
            cache_hit_rate: Self::calculate_cache_hit_rate(&cache_stats),
            total_operations: connection_metrics.total_connections,
            failed_operations: connection_metrics.connection_errors,
            average_response_time: Duration::from_millis(50), // Would be calculated from actual metrics
        }
    }

    /// Calculate overall cache hit rate
    fn calculate_cache_hit_rate(stats: &CacheStatistics) -> f64 {
        let total_requests = stats.fingerprint_hits + stats.fingerprint_misses 
                          + stats.zone_match_hits + stats.zone_match_misses;
        
        if total_requests == 0 {
            return 0.0;
        }
        
        let total_hits = stats.fingerprint_hits + stats.zone_match_hits;
        (total_hits as f64 / total_requests as f64) * 100.0
    }
}

/// Comprehensive performance metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub connection_pool_utilization: f64,
    pub active_background_tasks: usize,
    pub cache_hit_rate: f64,
    pub total_operations: u64,
    pub failed_operations: u64,
    pub average_response_time: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_state_cache_validity() {
        let mut cache = NetworkStateCache::new(ChronoDuration::seconds(10));
        assert!(cache.is_valid());
        
        // Simulate cache expiry
        cache.cache_time = Utc::now() - ChronoDuration::seconds(15);
        assert!(!cache.is_valid());
    }

    #[tokio::test]
    async fn test_connection_pool_creation() {
        let pool = ConnectionPool::new(5);
        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connections, 0);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.fingerprint_ttl, Duration::from_secs(60));
        assert_eq!(config.network_state_ttl, Duration::from_secs(30));
        assert_eq!(config.max_entries, 1000);
    }

    #[tokio::test]
    async fn test_cache_manager_creation() {
        let config = CacheConfig::default();
        let cache_manager = CacheManager::new(config);
        
        let stats = cache_manager.get_cache_stats().await;
        assert_eq!(stats.fingerprint_hits, 0);
        assert_eq!(stats.fingerprint_misses, 0);
    }

    #[tokio::test]
    async fn test_batch_processor_creation() {
        let processor = BatchProcessor::new();
        assert_eq!(processor.wifi_batch_size, 5);
        assert_eq!(processor.bluetooth_batch_size, 3);
    }

    #[tokio::test]
    async fn test_async_task_manager() {
        let task_manager = AsyncTaskManager::new(3);
        assert_eq!(task_manager.get_active_task_count().await, 0);
        
        let result = task_manager.submit_task(
            "test_task".to_string(),
            "test".to_string(),
            async { Ok("Test completed".to_string()) }
        ).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_performance_optimizer() {
        let optimizer = PerformanceOptimizer::new().await;
        let metrics = optimizer.get_performance_metrics().await;
        
        assert_eq!(metrics.connection_pool_utilization, 0.0);
        assert_eq!(metrics.active_background_tasks, 0);
        assert_eq!(metrics.cache_hit_rate, 0.0);
    }
}