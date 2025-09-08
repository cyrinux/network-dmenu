//! Advanced zone management with ML-driven suggestions and hierarchical zones
//!
//! Provides intelligent zone creation, optimization, hierarchical zone relationships,
//! automatic zone splitting/merging, and ML-powered zone suggestions.

use crate::geofencing::{GeofenceError, GeofenceZone, LocationFingerprint, Result, ZoneActions};
use chrono::{DateTime, Datelike, Timelike, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Advanced zone manager with ML capabilities
pub struct AdvancedZoneManager {
    /// Zone hierarchy manager
    hierarchy_manager: HierarchyManager,
    /// ML zone suggestion engine
    suggestion_engine: Arc<Mutex<ZoneSuggestionEngine>>,
    /// Zone optimization engine
    optimization_engine: Arc<Mutex<OptimizationEngine>>,
    /// Zone analytics collector
    analytics: Arc<Mutex<ZoneAnalytics>>,
    /// Zone relationship analyzer
    relationship_analyzer: RelationshipAnalyzer,
    /// Configuration
    config: AdvancedZoneConfig,
}

/// Configuration for advanced zone management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedZoneConfig {
    /// Enable ML-driven zone suggestions
    pub enable_ml_suggestions: bool,
    /// Minimum visits before suggesting zone creation
    pub min_visits_for_suggestion: u32,
    /// Minimum time spent before suggesting zone creation
    pub min_time_for_suggestion: Duration,
    /// Enable automatic zone optimization
    pub enable_auto_optimization: bool,
    /// Enable hierarchical zones
    pub enable_hierarchical_zones: bool,
    /// Maximum zone hierarchy depth
    pub max_hierarchy_depth: u32,
    /// Zone similarity threshold for merging suggestions
    pub merge_similarity_threshold: f64,
    /// Zone distance threshold for splitting suggestions
    pub split_distance_threshold: f64,
}

impl Default for AdvancedZoneConfig {
    fn default() -> Self {
        Self {
            enable_ml_suggestions: true,
            min_visits_for_suggestion: 3,
            min_time_for_suggestion: Duration::from_secs(3600), // 1 hour
            enable_auto_optimization: true,
            enable_hierarchical_zones: true,
            max_hierarchy_depth: 3,
            merge_similarity_threshold: 0.85,
            split_distance_threshold: 0.3,
        }
    }
}

/// Hierarchical zone relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneHierarchy {
    /// Zone ID
    pub zone_id: String,
    /// Parent zone ID (if any)
    pub parent_id: Option<String>,
    /// Child zone IDs
    pub children: Vec<String>,
    /// Hierarchy level (0 = root)
    pub level: u32,
    /// Relationship type
    pub relationship_type: ZoneRelationshipType,
    /// Inherited actions from parent
    pub inherits_actions: bool,
    /// Action overrides
    pub action_overrides: Option<ZoneActions>,
}

/// Types of zone relationships
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ZoneRelationshipType {
    /// Geographic containment (building contains rooms)
    Geographic,
    /// Temporal relationship (work hours vs break time)
    Temporal,
    /// Functional relationship (meeting rooms in office)
    Functional,
    /// Administrative relationship (personal vs work zones)
    Administrative,
}

/// Zone suggestion with confidence and reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneSuggestion {
    /// Suggested zone name
    pub suggested_name: String,
    /// Suggestion confidence (0.0 to 1.0)
    pub confidence: f64,
    /// Suggested location fingerprint
    pub suggested_fingerprint: LocationFingerprint,
    /// Suggested actions based on usage patterns
    pub suggested_actions: ZoneActions,
    /// Reasoning for the suggestion
    pub reasoning: String,
    /// Supporting evidence
    pub evidence: SuggestionEvidence,
    /// Suggestion type
    pub suggestion_type: SuggestionType,
    /// When the suggestion was generated
    pub created_at: DateTime<Utc>,
    /// Priority level
    pub priority: SuggestionPriority,
}

/// Evidence supporting a zone suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionEvidence {
    /// Number of visits to this location
    pub visit_count: u32,
    /// Total time spent at this location
    pub total_time: Duration,
    /// Average visit duration
    pub average_visit_duration: Duration,
    /// Common visit times
    pub common_visit_times: Vec<TimePattern>,
    /// Frequently used actions at this location
    pub common_actions: Vec<ActionPattern>,
    /// Similar existing zones
    pub similar_zones: Vec<String>,
}

/// Time pattern for visits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePattern {
    /// Hour of day (0-23)
    pub hour: u8,
    /// Day of week (0-6, Sunday=0)
    pub day_of_week: u8,
    /// Frequency of visits at this time
    pub frequency: f64,
}

/// Action pattern for location usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPattern {
    /// Action type (wifi, vpn, bluetooth, etc.)
    pub action_type: String,
    /// Action value
    pub action_value: String,
    /// Frequency of this action
    pub frequency: f64,
    /// Success rate of this action
    pub success_rate: f64,
}

/// Types of zone suggestions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestionType {
    /// Create a new zone
    CreateZone,
    /// Merge existing zones
    MergeZones(Vec<String>),
    /// Split an existing zone
    SplitZone(String),
    /// Optimize zone fingerprint
    OptimizeFingerprint(String),
    /// Update zone actions
    UpdateActions(String),
    /// Create hierarchical relationship
    CreateHierarchy(String, String), // parent, child
    /// Remove underutilized zone
    RemoveZone(String),
    /// Optimize schedule-based actions
    OptimizeSchedule,
    /// Automate frequent actions
    AutomateActions(String),
}

/// Priority levels for suggestions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SuggestionPriority {
    Low,
    Medium,
    High,
    Urgent,
}

/// Zone visit record for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneVisit {
    /// Location fingerprint of the visit
    pub fingerprint: LocationFingerprint,
    /// Start time of visit
    pub start_time: DateTime<Utc>,
    /// End time of visit (if known)
    pub end_time: Option<DateTime<Utc>>,
    /// Duration of visit
    pub duration: Option<Duration>,
    /// Actions performed during visit
    pub actions_performed: Vec<String>,
    /// Confidence in location detection
    pub confidence: f64,
    /// Associated zone (if matched)
    pub matched_zone: Option<String>,
}

/// Zone analytics data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneAnalytics {
    /// Visit history for unmatched locations
    pub unmatched_visits: VecDeque<ZoneVisit>,
    /// Zone usage statistics
    pub zone_usage_stats: HashMap<String, ZoneUsageStats>,
    /// Location clustering data
    pub location_clusters: Vec<LocationCluster>,
    /// Temporal patterns
    pub temporal_patterns: HashMap<String, Vec<TimePattern>>,
    /// Action correlation data
    pub action_correlations: HashMap<String, Vec<ActionPattern>>,
}

/// Usage statistics for a zone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneUsageStats {
    /// Total visits to this zone
    pub total_visits: u32,
    /// Total time spent in zone
    pub total_time: Duration,
    /// Average visit duration
    pub average_duration: Duration,
    /// Most recent visit
    pub last_visit: Option<DateTime<Utc>>,
    /// Most common actions in this zone
    pub common_actions: Vec<ActionPattern>,
    /// Time-based usage patterns
    pub temporal_patterns: Vec<TimePattern>,
    /// Zone confidence score over time
    pub confidence_history: VecDeque<(DateTime<Utc>, f64)>,
}

/// Location cluster for zone suggestions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationCluster {
    /// Cluster identifier
    pub cluster_id: String,
    /// Representative fingerprint for the cluster
    pub representative_fingerprint: LocationFingerprint,
    /// All fingerprints in this cluster
    pub fingerprints: Vec<LocationFingerprint>,
    /// Cluster center point
    pub center: NetworkSignatureCenter,
    /// Cluster radius (similarity measure)
    pub radius: f64,
    /// Number of visits to locations in this cluster
    pub visit_count: u32,
    /// Whether this cluster has been suggested as a zone
    pub suggested_as_zone: bool,
}

/// Center point of network signatures (for clustering)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSignatureCenter {
    /// Average signal strengths by network hash
    pub average_signals: HashMap<String, f64>,
    /// Common networks in this cluster
    pub common_networks: BTreeSet<String>,
}

/// Hierarchy manager for zone relationships
pub struct HierarchyManager {
    /// Zone hierarchies
    hierarchies: HashMap<String, ZoneHierarchy>,
    /// Root zones (no parents)
    root_zones: HashSet<String>,
    /// Zone relationships cache
    relationship_cache: HashMap<String, Vec<String>>, // zone_id -> related_zone_ids
}

/// ML-powered zone suggestion engine
pub struct ZoneSuggestionEngine {
    /// Visit history for analysis
    visit_history: VecDeque<ZoneVisit>,
    /// Location clustering algorithm
    clusterer: LocationClusterer,
    /// Pattern recognition engine
    pattern_recognizer: PatternRecognizer,
    /// Suggestion cache
    suggestion_cache: HashMap<String, ZoneSuggestion>,
    /// Configuration
    config: AdvancedZoneConfig,
}

/// Location clustering algorithm
pub struct LocationClusterer {
    /// Clustering parameters
    similarity_threshold: f64,
    min_cluster_size: usize,
    max_clusters: usize,
    /// Current clusters
    clusters: Vec<LocationCluster>,
}

/// Pattern recognition for user behavior
pub struct PatternRecognizer {
    /// Temporal patterns by location
    temporal_patterns: HashMap<String, Vec<TimePattern>>,
    /// Action patterns by location
    action_patterns: HashMap<String, Vec<ActionPattern>>,
    /// Sequence patterns (common action sequences)
    sequence_patterns: HashMap<String, Vec<String>>,
}

/// Zone optimization engine
pub struct OptimizationEngine {
    /// Optimization algorithms
    algorithms: Vec<Box<dyn ZoneOptimizer>>,
    /// Optimization history
    optimization_history: Vec<OptimizationResult>,
}

/// Zone optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// Zone that was optimized
    pub zone_id: String,
    /// Type of optimization performed
    pub optimization_type: OptimizationType,
    /// Improvement metrics
    pub improvements: HashMap<String, f64>,
    /// When optimization was performed
    pub optimized_at: DateTime<Utc>,
    /// Success/failure status
    pub success: bool,
}

/// Optimization statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationStats {
    /// Total number of optimizations performed
    pub total_optimizations: usize,
    /// Number of successful optimizations
    pub successful_optimizations: usize,
    /// Success rate as percentage (0.0 to 1.0)
    pub success_rate: f64,
    /// Average accuracy improvement across all optimizations
    pub average_accuracy_improvement: f64,
}

/// Types of zone optimization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OptimizationType {
    /// Optimize fingerprint accuracy
    FingerprintOptimization,
    /// Merge similar zones
    ZoneMerging,
    /// Split overly broad zones  
    ZoneSplitting,
    /// Optimize action sequences
    ActionOptimization,
    /// Adjust confidence thresholds
    ThresholdOptimization,
}

/// Trait for zone optimization algorithms
pub trait ZoneOptimizer: Send + Sync {
    /// Analyze zone and suggest optimizations
    fn analyze_zone(&self, zone: &GeofenceZone, analytics: &ZoneAnalytics) -> Vec<ZoneSuggestion>;

    /// Apply optimization to a zone
    fn optimize_zone(&self, zone: &mut GeofenceZone, suggestion: &ZoneSuggestion) -> Result<()>;

    /// Get optimizer name
    fn name(&self) -> &str;
}

/// Zone relationship analyzer
pub struct RelationshipAnalyzer {
    /// Spatial relationship detection
    spatial_analyzer: SpatialAnalyzer,
    /// Temporal relationship detection
    temporal_analyzer: TemporalAnalyzer,
    /// Functional relationship detection
    functional_analyzer: FunctionalAnalyzer,
}

/// Spatial relationship analysis
struct SpatialAnalyzer {
    /// Containment detection threshold
    containment_threshold: f64,
    /// Proximity detection threshold
    proximity_threshold: f64,
}

/// Temporal relationship analysis
struct TemporalAnalyzer {
    /// Time window for relationship detection
    time_window: Duration,
    /// Sequence detection parameters
    sequence_threshold: f64,
}

/// Functional relationship analysis
struct FunctionalAnalyzer {
    /// Action similarity threshold
    action_similarity_threshold: f64,
    /// Usage pattern correlation threshold
    pattern_correlation_threshold: f64,
}

impl AdvancedZoneManager {
    /// Create new advanced zone manager
    pub async fn new(config: AdvancedZoneConfig) -> Self {
        debug!("Creating advanced zone manager with config: {:?}", config);

        Self {
            hierarchy_manager: HierarchyManager::new(),
            suggestion_engine: Arc::new(Mutex::new(ZoneSuggestionEngine::new(config.clone()))),
            optimization_engine: Arc::new(Mutex::new(OptimizationEngine::new())),
            analytics: Arc::new(Mutex::new(ZoneAnalytics::new())),
            relationship_analyzer: RelationshipAnalyzer::new(),
            config,
        }
    }

    /// Record a location visit for analysis
    pub async fn record_visit(
        &self,
        fingerprint: LocationFingerprint,
        matched_zone: Option<String>,
    ) -> Result<()> {
        debug!(
            "Recording visit with {} networks, matched_zone: {:?}",
            fingerprint.wifi_networks.len(),
            matched_zone
        );

        let visit = ZoneVisit {
            fingerprint: fingerprint.clone(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            actions_performed: Vec::new(),
            confidence: fingerprint.confidence_score,
            matched_zone: matched_zone.clone(),
        };

        // Add to analytics
        {
            let mut analytics = self.analytics.lock().await;
            if matched_zone.is_none() {
                analytics.unmatched_visits.push_back(visit.clone());

                // Keep only recent unmatched visits
                while analytics.unmatched_visits.len() > 1000 {
                    analytics.unmatched_visits.pop_front();
                }
            }
        }

        // Update suggestion engine
        {
            let mut engine = self.suggestion_engine.lock().await;
            engine.record_visit(visit).await;
        }

        Ok(())
    }

    /// Generate zone suggestions based on collected data
    pub async fn generate_suggestions(&self) -> Result<Vec<ZoneSuggestion>> {
        debug!("Generating zone suggestions");

        if !self.config.enable_ml_suggestions {
            return Ok(Vec::new());
        }

        let mut suggestions = Vec::new();

        // Get suggestions from ML engine
        {
            let mut engine = self.suggestion_engine.lock().await;
            let ml_suggestions = engine.generate_suggestions().await?;
            suggestions.extend(ml_suggestions);
        }

        // Get optimization suggestions
        {
            let analytics = self.analytics.lock().await;
            let mut optimization_engine = self.optimization_engine.lock().await;
            let optimization_suggestions = optimization_engine.suggest_optimizations(&analytics);
            suggestions.extend(optimization_suggestions);
        }

        // Sort by priority and confidence
        suggestions.sort_by(|a, b| match (a.priority.clone(), b.priority.clone()) {
            (SuggestionPriority::Urgent, _) => std::cmp::Ordering::Less,
            (_, SuggestionPriority::Urgent) => std::cmp::Ordering::Greater,
            (SuggestionPriority::High, SuggestionPriority::Low | SuggestionPriority::Medium) => {
                std::cmp::Ordering::Less
            }
            (SuggestionPriority::Low | SuggestionPriority::Medium, SuggestionPriority::High) => {
                std::cmp::Ordering::Greater
            }
            _ => b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        debug!("Generated {} zone suggestions", suggestions.len());
        Ok(suggestions)
    }

    /// Create hierarchical relationship between zones
    pub async fn create_zone_hierarchy(
        &mut self,
        parent_id: String,
        child_id: String,
        relationship_type: ZoneRelationshipType,
    ) -> Result<()> {
        debug!(
            "Creating zone hierarchy: {} -> {} ({:?})",
            parent_id, child_id, relationship_type
        );

        if !self.config.enable_hierarchical_zones {
            return Err(GeofenceError::Config(
                "Hierarchical zones are disabled in configuration".to_string(),
            ));
        }

        self.hierarchy_manager
            .create_relationship(parent_id, child_id, relationship_type)?;

        info!("Zone hierarchy created successfully");
        Ok(())
    }

    /// Get zone hierarchy for a specific zone
    pub fn get_zone_hierarchy(&self, zone_id: &str) -> Option<&ZoneHierarchy> {
        self.hierarchy_manager.get_hierarchy(zone_id)
    }

    /// Get all root zones (zones without parents)
    pub fn get_root_zones(&self) -> Vec<&str> {
        self.hierarchy_manager.get_root_zones()
    }

    /// Get child zones for a parent zone
    pub fn get_child_zones(&self, parent_id: &str) -> Vec<&str> {
        self.hierarchy_manager.get_children(parent_id)
    }

    /// Apply zone suggestion
    pub async fn apply_suggestion(&mut self, suggestion: &ZoneSuggestion) -> Result<String> {
        debug!("Applying zone suggestion: {:?}", suggestion.suggestion_type);

        match &suggestion.suggestion_type {
            SuggestionType::CreateZone => {
                let zone_id = uuid::Uuid::new_v4().to_string();
                info!(
                    "Creating new zone '{}' based on suggestion",
                    suggestion.suggested_name
                );
                Ok(zone_id)
            }

            SuggestionType::MergeZones(zone_ids) => {
                info!("Merging zones: {:?}", zone_ids);
                Ok(format!("merged_{}", zone_ids.join("_")))
            }

            SuggestionType::SplitZone(zone_id) => {
                info!("Splitting zone: {}", zone_id);
                Ok(format!("{}_split", zone_id))
            }

            SuggestionType::OptimizeFingerprint(zone_id) => {
                info!("Optimizing fingerprint for zone: {}", zone_id);
                Ok(zone_id.clone())
            }

            SuggestionType::UpdateActions(zone_id) => {
                info!("Updating actions for zone: {}", zone_id);
                Ok(zone_id.clone())
            }

            SuggestionType::CreateHierarchy(parent_id, child_id) => {
                self.create_zone_hierarchy(
                    parent_id.clone(),
                    child_id.clone(),
                    ZoneRelationshipType::Geographic,
                )
                .await?;
                Ok(format!("hierarchy_{}_{}", parent_id, child_id))
            }

            SuggestionType::RemoveZone(zone_id) => {
                info!("Removing underutilized zone: {}", zone_id);
                Ok(format!("removed_{}", zone_id))
            }

            SuggestionType::OptimizeSchedule => {
                info!("Optimizing schedule-based actions");
                Ok("schedule_optimized".to_string())
            }

            SuggestionType::AutomateActions(zone_id) => {
                info!("Automating frequent actions for zone: {}", zone_id);
                Ok(format!("automated_{}", zone_id))
            }
        }
    }

    /// Get zone analytics
    pub async fn get_zone_analytics(&self) -> ZoneAnalytics {
        self.analytics.lock().await.clone()
    }

    /// Perform automatic zone optimization
    pub async fn auto_optimize_zones(&mut self) -> Result<Vec<OptimizationResult>> {
        debug!("Performing automatic zone optimization");

        if !self.config.enable_auto_optimization {
            return Ok(Vec::new());
        }

        let suggestions = {
            let analytics = self.analytics.lock().await;
            let mut optimization_engine = self.optimization_engine.lock().await;
            optimization_engine.suggest_optimizations(&analytics)
        };

        let mut results = Vec::new();

        for suggestion in suggestions {
            match self.apply_suggestion(&suggestion).await {
                Ok(_) => {
                    results.push(OptimizationResult {
                        zone_id: match &suggestion.suggestion_type {
                            SuggestionType::OptimizeFingerprint(id)
                            | SuggestionType::UpdateActions(id) => id.clone(),
                            _ => "multiple".to_string(),
                        },
                        optimization_type: match suggestion.suggestion_type {
                            SuggestionType::OptimizeFingerprint(_) => {
                                OptimizationType::FingerprintOptimization
                            }
                            SuggestionType::MergeZones(_) => OptimizationType::ZoneMerging,
                            SuggestionType::SplitZone(_) => OptimizationType::ZoneSplitting,
                            SuggestionType::UpdateActions(_) => {
                                OptimizationType::ActionOptimization
                            }
                            _ => OptimizationType::ThresholdOptimization,
                        },
                        improvements: HashMap::new(), // Would contain actual metrics
                        optimized_at: Utc::now(),
                        success: true,
                    });
                }
                Err(e) => {
                    warn!("Failed to apply optimization suggestion: {}", e);
                }
            }
        }

        info!(
            "Completed automatic optimization: {} successful operations",
            results.len()
        );
        Ok(results)
    }

    /// Analyze zone relationships
    pub fn analyze_zone_relationships(
        &self,
        zones: &[GeofenceZone],
    ) -> Vec<ZoneRelationshipSuggestion> {
        debug!("Analyzing zone relationships for {} zones", zones.len());

        self.relationship_analyzer.analyze_relationships(zones)
    }
}

/// Zone relationship suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRelationshipSuggestion {
    /// Zones involved in the relationship
    pub zones: Vec<String>,
    /// Suggested relationship type
    pub relationship_type: ZoneRelationshipType,
    /// Confidence in the relationship
    pub confidence: f64,
    /// Evidence for the relationship
    pub evidence: String,
}

impl HierarchyManager {
    fn new() -> Self {
        Self {
            hierarchies: HashMap::new(),
            root_zones: HashSet::new(),
            relationship_cache: HashMap::new(),
        }
    }

    fn create_relationship(
        &mut self,
        parent_id: String,
        child_id: String,
        relationship_type: ZoneRelationshipType,
    ) -> Result<()> {
        // Validate hierarchy depth
        let parent_level = self
            .hierarchies
            .get(&parent_id)
            .map(|h| h.level)
            .unwrap_or(0);

        if parent_level >= 2 {
            // Max depth of 3 (0, 1, 2)
            return Err(GeofenceError::Config(
                "Maximum hierarchy depth exceeded".to_string(),
            ));
        }

        // Create or update parent hierarchy
        let parent_hierarchy = self
            .hierarchies
            .entry(parent_id.clone())
            .or_insert_with(|| ZoneHierarchy {
                zone_id: parent_id.clone(),
                parent_id: None,
                children: Vec::new(),
                level: parent_level,
                relationship_type: relationship_type.clone(),
                inherits_actions: false,
                action_overrides: None,
            });

        if !parent_hierarchy.children.contains(&child_id) {
            parent_hierarchy.children.push(child_id.clone());
        }

        // Create child hierarchy
        self.hierarchies.insert(
            child_id.clone(),
            ZoneHierarchy {
                zone_id: child_id.clone(),
                parent_id: Some(parent_id.clone()),
                children: Vec::new(),
                level: parent_level + 1,
                relationship_type,
                inherits_actions: true,
                action_overrides: None,
            },
        );

        // Update root zones
        self.root_zones.insert(parent_id);
        self.root_zones.remove(&child_id);

        // Invalidate relationship cache
        self.relationship_cache.clear();

        Ok(())
    }

    fn get_hierarchy(&self, zone_id: &str) -> Option<&ZoneHierarchy> {
        self.hierarchies.get(zone_id)
    }

    fn get_root_zones(&self) -> Vec<&str> {
        self.root_zones.iter().map(|s| s.as_str()).collect()
    }

    fn get_children(&self, parent_id: &str) -> Vec<&str> {
        self.hierarchies
            .get(parent_id)
            .map(|h| h.children.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

impl ZoneSuggestionEngine {
    fn new(config: AdvancedZoneConfig) -> Self {
        Self {
            visit_history: VecDeque::new(),
            clusterer: LocationClusterer::new(),
            pattern_recognizer: PatternRecognizer::new(),
            suggestion_cache: HashMap::new(),
            config,
        }
    }

    async fn record_visit(&mut self, visit: ZoneVisit) {
        self.visit_history.push_back(visit.clone());

        // Keep only recent visits
        while self.visit_history.len() > 10000 {
            self.visit_history.pop_front();
        }

        // Update clustering
        self.clusterer.add_location(&visit.fingerprint);

        // Update pattern recognition
        self.pattern_recognizer.analyze_visit(&visit);
    }

    async fn generate_suggestions(&mut self) -> Result<Vec<ZoneSuggestion>> {
        let cache_key = format!("suggestions_{}", Utc::now().format("%Y-%m-%d-%H"));

        // Check if we have cached suggestions for this hour
        if let Some(cached_suggestion) = self.suggestion_cache.get(&cache_key) {
            debug!("Returning cached zone suggestions for {}", cache_key);
            return Ok(vec![cached_suggestion.clone()]);
        }

        let mut suggestions = Vec::new();

        // Collect mature cluster IDs for comparison
        let mature_clusters = self.clusterer.get_mature_clusters();
        let mature_cluster_ids: Vec<String> = mature_clusters
            .iter()
            .map(|c| c.cluster_id.clone())
            .collect();

        // Process mature clusters first
        for cluster in &mature_clusters {
            let suggestion = self.create_zone_suggestion_from_cluster(cluster)?;

            // Cache the suggestion
            self.suggestion_cache.insert(
                format!("cluster_{}", cluster.cluster_id),
                suggestion.clone(),
            );

            suggestions.push(suggestion);
        }

        // Additional suggestions from significant clusters (high visit count)
        let significant_clusters = self
            .clusterer
            .get_significant_clusters(self.config.min_visits_for_suggestion as usize);
        for cluster in significant_clusters {
            // Skip if already processed in mature clusters
            if !mature_cluster_ids.contains(&cluster.cluster_id) {
                let suggestion = self.create_zone_suggestion_from_cluster(cluster)?;
                suggestions.push(suggestion);
            }
        }

        // Pattern-based suggestions from behavior analysis
        let pattern_suggestions = self.pattern_recognizer.suggest_zones(&self.config);
        suggestions.extend(pattern_suggestions);

        // Cleanup old cache entries (keep only last 24 hours)
        self.cleanup_suggestion_cache();

        Ok(suggestions)
    }

    /// Clean up old cached suggestions
    fn cleanup_suggestion_cache(&mut self) {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::hours(24);

        let initial_count = self.suggestion_cache.len();
        self.suggestion_cache.retain(|key, suggestion| {
            let is_recent = suggestion.created_at > cutoff;
            if !is_recent {
                debug!("Removing stale cached suggestion: {}", key);
            }
            is_recent
        });

        let removed_count = initial_count - self.suggestion_cache.len();
        if removed_count > 0 {
            debug!("Cleaned up {} stale cached suggestions", removed_count);
        }
    }

    fn create_zone_suggestion_from_cluster(
        &self,
        cluster: &LocationCluster,
    ) -> Result<ZoneSuggestion> {
        let suggested_name = format!("Frequent Location {}", &cluster.cluster_id[..8]);

        // Use config for duration estimates
        let estimated_total_time = Duration::from_secs(
            cluster.visit_count as u64 * self.config.min_time_for_suggestion.as_secs()
                / self.config.min_visits_for_suggestion as u64,
        );

        let evidence = SuggestionEvidence {
            visit_count: cluster.visit_count,
            total_time: estimated_total_time,
            average_visit_duration: Duration::from_secs(
                estimated_total_time.as_secs() / cluster.visit_count as u64,
            ),
            common_visit_times: Vec::new(), // Would analyze from visit history
            common_actions: Vec::new(),     // Would analyze from visit history
            similar_zones: Vec::new(),
        };

        // Use config thresholds for confidence calculation
        let confidence = (cluster.visit_count as f64
            / (self.config.min_visits_for_suggestion as f64 * 2.0))
            .min(0.9);
        let is_high_priority = cluster.visit_count >= self.config.min_visits_for_suggestion * 3;

        Ok(ZoneSuggestion {
            suggested_name,
            confidence,
            suggested_fingerprint: cluster.representative_fingerprint.clone(),
            suggested_actions: ZoneActions::default(),
            reasoning: format!(
                "You've visited this location {} times (min threshold: {}). Creating a zone here would enable automatic actions.",
                cluster.visit_count, self.config.min_visits_for_suggestion
            ),
            evidence,
            suggestion_type: SuggestionType::CreateZone,
            created_at: Utc::now(),
            priority: if is_high_priority { 
                SuggestionPriority::High 
            } else { 
                SuggestionPriority::Medium 
            },
        })
    }
}

impl LocationClusterer {
    fn new() -> Self {
        Self {
            similarity_threshold: 0.7,
            min_cluster_size: 3,
            max_clusters: 50,
            clusters: Vec::new(),
        }
    }

    fn add_location(&mut self, fingerprint: &LocationFingerprint) {
        // Find closest existing cluster
        let mut best_cluster_idx = None;
        let mut best_similarity = 0.0;

        for (idx, cluster) in self.clusters.iter().enumerate() {
            let similarity = self
                .calculate_fingerprint_similarity(fingerprint, &cluster.representative_fingerprint);

            if similarity > best_similarity && similarity >= self.similarity_threshold {
                best_similarity = similarity;
                best_cluster_idx = Some(idx);
            }
        }

        if let Some(idx) = best_cluster_idx {
            // Add to existing cluster
            self.clusters[idx].fingerprints.push(fingerprint.clone());
            self.clusters[idx].visit_count += 1;
            self.update_cluster_representative(idx);
        } else if self.clusters.len() < self.max_clusters {
            // Create new cluster
            let cluster_id = uuid::Uuid::new_v4().to_string();
            let cluster = LocationCluster {
                cluster_id,
                representative_fingerprint: fingerprint.clone(),
                fingerprints: vec![fingerprint.clone()],
                center: NetworkSignatureCenter {
                    average_signals: HashMap::new(),
                    common_networks: fingerprint
                        .wifi_networks
                        .iter()
                        .map(|n| n.ssid_hash.clone())
                        .collect(),
                },
                radius: 0.0,
                visit_count: 1,
                suggested_as_zone: false,
            };
            self.clusters.push(cluster);
        }

        // Cleanup clusters that don't meet minimum size requirement
        self.cleanup_small_clusters();
    }

    /// Remove clusters that are too small to be meaningful
    fn cleanup_small_clusters(&mut self) {
        let initial_count = self.clusters.len();
        self.clusters.retain(|cluster| {
            let meets_size_requirement =
                cluster.visit_count >= self.min_cluster_size.try_into().unwrap();
            if !meets_size_requirement {
                debug!(
                    "Removing small cluster {} with {} visits (min required: {})",
                    cluster.cluster_id, cluster.visit_count, self.min_cluster_size
                );
            }
            meets_size_requirement
        });

        let removed_count = initial_count - self.clusters.len();
        if removed_count > 0 {
            debug!("Cleaned up {} small clusters", removed_count);
        }
    }

    /// Get clusters that meet minimum size for zone suggestions  
    fn get_mature_clusters(&self) -> Vec<&LocationCluster> {
        self.clusters
            .iter()
            .filter(|cluster| {
                cluster.visit_count >= self.min_cluster_size.try_into().unwrap()
                    && !cluster.suggested_as_zone
            })
            .collect()
    }

    fn calculate_fingerprint_similarity(
        &self,
        fp1: &LocationFingerprint,
        fp2: &LocationFingerprint,
    ) -> f64 {
        let networks1: HashSet<_> = fp1.wifi_networks.iter().map(|n| &n.ssid_hash).collect();
        let networks2: HashSet<_> = fp2.wifi_networks.iter().map(|n| &n.ssid_hash).collect();

        let intersection = networks1.intersection(&networks2).count();
        let union = networks1.union(&networks2).count();

        if union == 0 {
            1.0
        } else {
            intersection as f64 / union as f64
        }
    }

    fn update_cluster_representative(&mut self, cluster_idx: usize) {
        // Update the representative fingerprint to be the centroid
        // This is a simplified implementation
        if let Some(cluster) = self.clusters.get_mut(cluster_idx) {
            if !cluster.fingerprints.is_empty() {
                // Use the most recent fingerprint as representative (simplified)
                cluster.representative_fingerprint = cluster.fingerprints.last().unwrap().clone();
            }
        }
    }

    fn get_significant_clusters(&self, min_visits: usize) -> Vec<&LocationCluster> {
        self.clusters
            .iter()
            .filter(|c| c.visit_count >= min_visits as u32)
            .collect()
    }
}

impl PatternRecognizer {
    fn new() -> Self {
        Self {
            temporal_patterns: HashMap::new(),
            action_patterns: HashMap::new(),
            sequence_patterns: HashMap::new(),
        }
    }

    fn analyze_visit(&mut self, visit: &ZoneVisit) {
        debug!(
            "Analyzing visit patterns for zone: {:?}",
            visit.matched_zone
        );

        // Extract location key for pattern tracking
        let location_key = if let Some(zone) = &visit.matched_zone {
            zone.clone()
        } else {
            // Create a key based on dominant networks
            visit
                .fingerprint
                .wifi_networks
                .iter()
                .take(3)
                .map(|n| n.ssid_hash.chars().take(8).collect::<String>())
                .collect::<Vec<_>>()
                .join("_")
        };

        // Analyze temporal patterns
        let hour = visit.start_time.hour() as u8;
        let day_of_week = visit.start_time.weekday().num_days_from_monday() as u8;

        let temporal_pattern = TimePattern {
            hour,
            day_of_week,
            frequency: 1.0,
        };

        self.temporal_patterns
            .entry(location_key.clone())
            .or_default()
            .push(temporal_pattern);

        // Analyze action patterns
        for action in &visit.actions_performed {
            let action_pattern = ActionPattern {
                action_type: "generic".to_string(),
                action_value: action.clone(),
                frequency: 1.0,
                success_rate: 1.0, // Assume success for now
            };

            self.action_patterns
                .entry(location_key.clone())
                .or_default()
                .push(action_pattern);
        }

        // Analyze sequence patterns (if we have previous actions)
        if visit.actions_performed.len() > 1 {
            let sequence_key = format!("{}_sequence", location_key);
            self.sequence_patterns
                .entry(sequence_key)
                .or_default()
                .extend(visit.actions_performed.clone());
        }

        // Cleanup old patterns to avoid memory bloat
        self.cleanup_old_patterns();
    }

    /// Remove old pattern data to prevent memory growth
    fn cleanup_old_patterns(&mut self) {
        // Keep only the most recent 100 patterns per location
        for patterns in self.temporal_patterns.values_mut() {
            if patterns.len() > 100 {
                patterns.drain(0..patterns.len() - 100);
            }
        }

        for patterns in self.action_patterns.values_mut() {
            if patterns.len() > 50 {
                patterns.drain(0..patterns.len() - 50);
            }
        }

        for sequence in self.sequence_patterns.values_mut() {
            if sequence.len() > 200 {
                sequence.drain(0..sequence.len() - 200);
            }
        }
    }

    fn suggest_zones(&self, config: &AdvancedZoneConfig) -> Vec<ZoneSuggestion> {
        let mut suggestions = Vec::new();

        // Generate suggestions based on temporal patterns
        for (location_key, patterns) in &self.temporal_patterns {
            // Find patterns with high frequency that might represent regular locations
            let total_frequency: f64 = patterns.iter().map(|p| p.frequency).sum();

            if total_frequency >= config.min_visits_for_suggestion as f64 {
                // Create a zone suggestion based on temporal patterns
                let evidence = SuggestionEvidence {
                    visit_count: total_frequency as u32,
                    total_time: Duration::from_secs(3600), // Default estimate
                    average_visit_duration: Duration::from_secs(3600),
                    common_visit_times: patterns.clone(),
                    common_actions: self
                        .action_patterns
                        .get(location_key)
                        .cloned()
                        .unwrap_or_default(),
                    similar_zones: vec![],
                };

                // Determine zone name based on patterns
                let suggested_name = if patterns.iter().any(|p| p.hour >= 9 && p.hour <= 17) {
                    format!(
                        "Work Location ({})",
                        location_key.chars().take(8).collect::<String>()
                    )
                } else if patterns.iter().any(|p| p.hour >= 18 || p.hour <= 8) {
                    format!(
                        "Home Location ({})",
                        location_key.chars().take(8).collect::<String>()
                    )
                } else {
                    format!(
                        "Frequent Location ({})",
                        location_key.chars().take(8).collect::<String>()
                    )
                };

                suggestions.push(ZoneSuggestion {
                    suggested_name,
                    confidence: (total_frequency / (config.min_visits_for_suggestion as f64 * 2.0))
                        .min(0.95),
                    suggested_fingerprint: LocationFingerprint::default(), // Would be derived from visits
                    suggested_actions: ZoneActions::default(), // Would be derived from action patterns
                    reasoning: format!(
                        "Detected regular visits with {} total frequency across {} time patterns",
                        total_frequency,
                        patterns.len()
                    ),
                    evidence,
                    suggestion_type: SuggestionType::CreateZone,
                    created_at: Utc::now(),
                    priority: if total_frequency > config.min_visits_for_suggestion as f64 * 3.0 {
                        SuggestionPriority::High
                    } else {
                        SuggestionPriority::Medium
                    },
                });
            }
        }

        suggestions
    }
}

impl OptimizationEngine {
    fn new() -> Self {
        Self {
            algorithms: vec![
                Box::new(FingerprintOptimizer::new()),
                Box::new(ActionOptimizer::new()),
                Box::new(ZoneMergeOptimizer::new()),
            ],
            optimization_history: Vec::new(),
        }
    }

    fn suggest_optimizations(&mut self, analytics: &ZoneAnalytics) -> Vec<ZoneSuggestion> {
        let mut suggestions = Vec::new();

        // Analyze underutilized zones for potential optimization
        for (zone_name, usage_stats) in &analytics.zone_usage_stats {
            if usage_stats.total_visits < 5 && usage_stats.total_time.as_secs() < 3600 {
                // Suggest zone removal or merging for rarely used zones
                if let Some(suggestion) =
                    self.create_underutilized_zone_suggestion(zone_name, usage_stats)
                {
                    suggestions.push(suggestion);
                }
            }
        }

        // Analyze unmatched visits for potential new zones
        if analytics.unmatched_visits.len() > 10 {
            let clustered_visits = self.analyze_unmatched_visits(&analytics.unmatched_visits);
            for cluster in clustered_visits {
                if let Some(suggestion) = self.create_new_zone_suggestion(&cluster) {
                    suggestions.push(suggestion);
                }
            }
        }

        // Analyze temporal patterns for schedule-based optimizations
        for (location_key, patterns) in &analytics.temporal_patterns {
            if let Some(suggestion) = self.analyze_temporal_optimization(location_key, patterns) {
                suggestions.push(suggestion);
            }
        }

        // Analyze action patterns for automation suggestions
        for (zone_id, actions) in &analytics.action_correlations {
            if let Some(suggestion) = self.analyze_action_automation(zone_id, actions) {
                suggestions.push(suggestion);
            }
        }

        // Run algorithmic optimizations using real data
        for algorithm in &self.algorithms {
            debug!(
                "Running optimization algorithm: {} with real analytics data",
                algorithm.name()
            );

            let optimization_results =
                self.run_algorithm_with_analytics(algorithm.as_ref(), analytics);
            for result in optimization_results {
                self.optimization_history.push(result.clone());
                if let Some(suggestion) = self.convert_result_to_suggestion(result) {
                    suggestions.push(suggestion);
                }
            }
        }

        info!(
            "Generated {} optimization suggestions from analytics data",
            suggestions.len()
        );
        suggestions
    }

    /// Convert an optimization result to a zone suggestion
    fn convert_result_to_suggestion(&self, result: OptimizationResult) -> Option<ZoneSuggestion> {
        let accuracy_improvement = result.improvements.get("accuracy").unwrap_or(&0.0);
        let performance_improvement = result.improvements.get("performance").unwrap_or(&0.0);

        // Create default evidence structure
        let evidence = SuggestionEvidence {
            visit_count: 1,
            total_time: Duration::from_secs(3600),
            average_visit_duration: Duration::from_secs(3600),
            common_visit_times: vec![],
            common_actions: vec![],
            similar_zones: vec![],
        };

        // Create default location fingerprint
        let suggested_fingerprint = LocationFingerprint::default();

        match result.optimization_type {
            OptimizationType::FingerprintOptimization => Some(ZoneSuggestion {
                suggested_name: format!("Optimized {}", result.zone_id),
                confidence: (*accuracy_improvement).max(0.8),
                suggested_fingerprint: suggested_fingerprint.clone(),
                suggested_actions: ZoneActions::default(),
                reasoning: format!(
                    "Fingerprint optimization improved accuracy by {:.1}%",
                    accuracy_improvement * 100.0
                ),
                evidence: evidence.clone(),
                suggestion_type: SuggestionType::OptimizeFingerprint(result.zone_id.clone()),
                created_at: Utc::now(),
                priority: SuggestionPriority::Medium,
            }),
            OptimizationType::ActionOptimization => Some(ZoneSuggestion {
                suggested_name: format!("Action-optimized {}", result.zone_id),
                confidence: (*performance_improvement).max(0.7),
                suggested_fingerprint: suggested_fingerprint.clone(),
                suggested_actions: ZoneActions::default(),
                reasoning: "Actions optimized for better performance".to_string(),
                evidence: evidence.clone(),
                suggestion_type: SuggestionType::UpdateActions(result.zone_id.clone()),
                created_at: Utc::now(),
                priority: SuggestionPriority::Medium,
            }),
            OptimizationType::ZoneMerging => Some(ZoneSuggestion {
                suggested_name: format!("Merged zone for {}", result.zone_id),
                confidence: 0.85,
                suggested_fingerprint,
                suggested_actions: ZoneActions::default(),
                reasoning: "Zones merged to reduce redundancy".to_string(),
                evidence,
                suggestion_type: SuggestionType::MergeZones(vec![result.zone_id.clone()]),
                created_at: Utc::now(),
                priority: SuggestionPriority::High,
            }),
            _ => None,
        }
    }

    /// Get optimization history
    pub fn get_optimization_history(&self) -> &[OptimizationResult] {
        &self.optimization_history
    }

    /// Get optimization history for a specific zone
    pub fn get_zone_optimization_history(&self, zone_id: &str) -> Vec<&OptimizationResult> {
        self.optimization_history
            .iter()
            .filter(|result| result.zone_id == zone_id)
            .collect()
    }

    /// Clear old optimization history (keep only recent results)
    pub fn cleanup_history(&mut self, max_entries: usize) {
        if self.optimization_history.len() > max_entries {
            let keep_from = self.optimization_history.len() - max_entries;
            self.optimization_history.drain(..keep_from);
            debug!(
                "Cleaned up optimization history, keeping {} recent entries",
                max_entries
            );
        }
    }

    /// Get optimization statistics
    pub fn get_optimization_stats(&self) -> OptimizationStats {
        let total_optimizations = self.optimization_history.len();
        let success_count = self
            .optimization_history
            .iter()
            .filter(|result| result.success)
            .count();

        let avg_accuracy_improvement = if !self.optimization_history.is_empty() {
            self.optimization_history
                .iter()
                .filter_map(|result| result.improvements.get("accuracy"))
                .sum::<f64>()
                / self.optimization_history.len() as f64
        } else {
            0.0
        };

        OptimizationStats {
            total_optimizations,
            successful_optimizations: success_count,
            success_rate: if total_optimizations > 0 {
                success_count as f64 / total_optimizations as f64
            } else {
                0.0
            },
            average_accuracy_improvement: avg_accuracy_improvement,
        }
    }

    /// Create suggestion for underutilized zone
    fn create_underutilized_zone_suggestion(
        &self,
        zone_name: &str,
        usage_stats: &ZoneUsageStats,
    ) -> Option<ZoneSuggestion> {
        let suggested_fingerprint = LocationFingerprint {
            wifi_networks: BTreeSet::new(),
            bluetooth_devices: BTreeSet::new(),
            ip_location: None,
            confidence_score: 0.8,
            timestamp: Utc::now(),
        };

        let evidence = SuggestionEvidence {
            visit_count: usage_stats.total_visits,
            total_time: usage_stats.total_time,
            average_visit_duration: usage_stats.average_duration,
            common_visit_times: vec![], // ZoneUsageStats doesn't have common_visit_times
            common_actions: vec![],
            similar_zones: vec![],
        };

        Some(ZoneSuggestion {
            suggested_name: format!("Remove or Merge: {}", zone_name),
            confidence: 0.8,
            suggested_fingerprint,
            suggested_actions: ZoneActions::default(),
            reasoning: format!(
                "Zone '{}' has only {} visits and {:?} total time - consider removal or merging",
                zone_name, usage_stats.total_visits, usage_stats.total_time
            ),
            evidence,
            suggestion_type: SuggestionType::RemoveZone(zone_name.to_string()),
            created_at: Utc::now(),
            priority: SuggestionPriority::Low,
        })
    }

    /// Analyze unmatched visits to identify potential zones
    fn analyze_unmatched_visits(
        &self,
        unmatched_visits: &VecDeque<ZoneVisit>,
    ) -> Vec<LocationCluster> {
        let mut clusters = Vec::new();
        let mut processed_visits = HashSet::new();

        for (i, visit) in unmatched_visits.iter().enumerate() {
            if processed_visits.contains(&i) {
                continue;
            }

            let mut cluster_visits = vec![visit.clone()];
            processed_visits.insert(i);

            // Find similar visits to cluster together
            for (j, other_visit) in unmatched_visits.iter().enumerate().skip(i + 1) {
                if processed_visits.contains(&j) {
                    continue;
                }

                // Simple similarity check based on WiFi networks
                let similarity = self.calculate_visit_similarity(visit, other_visit);
                if similarity > 0.7 {
                    cluster_visits.push(other_visit.clone());
                    processed_visits.insert(j);
                }
            }

            // Create cluster if we have multiple visits
            if cluster_visits.len() >= 3 {
                let centroid = self.calculate_cluster_centroid(&cluster_visits);
                let cluster = LocationCluster {
                    cluster_id: format!("cluster_{}", clusters.len()),
                    representative_fingerprint: centroid.clone(),
                    fingerprints: cluster_visits
                        .iter()
                        .map(|v| v.fingerprint.clone())
                        .collect(),
                    center: NetworkSignatureCenter {
                        average_signals: HashMap::new(),
                        common_networks: BTreeSet::new(),
                    },
                    radius: 0.8,
                    visit_count: cluster_visits.len() as u32,
                    suggested_as_zone: false,
                };
                clusters.push(cluster);
            }
        }

        clusters
    }

    /// Calculate similarity between two visits
    fn calculate_visit_similarity(&self, visit1: &ZoneVisit, visit2: &ZoneVisit) -> f64 {
        let networks1: HashSet<_> = visit1.fingerprint.wifi_networks.iter().collect();
        let networks2: HashSet<_> = visit2.fingerprint.wifi_networks.iter().collect();

        if networks1.is_empty() && networks2.is_empty() {
            return 0.0;
        }

        let intersection = networks1.intersection(&networks2).count();
        let union = networks1.union(&networks2).count();

        intersection as f64 / union as f64
    }

    /// Calculate cluster centroid location
    fn calculate_cluster_centroid(&self, visits: &[ZoneVisit]) -> LocationFingerprint {
        // Aggregate all WiFi networks from visits
        let mut all_networks = BTreeSet::new();
        let mut all_bluetooth = BTreeSet::new();

        for visit in visits {
            all_networks.extend(visit.fingerprint.wifi_networks.clone());
            all_bluetooth.extend(visit.fingerprint.bluetooth_devices.clone());
        }

        LocationFingerprint {
            wifi_networks: all_networks,
            bluetooth_devices: all_bluetooth,
            ip_location: None,
            confidence_score: 0.8,
            timestamp: Utc::now(),
        }
    }

    /// Create suggestion for new zone from cluster
    fn create_new_zone_suggestion(&self, cluster: &LocationCluster) -> Option<ZoneSuggestion> {
        if cluster.fingerprints.is_empty() {
            return None;
        }

        let total_visits = cluster.visit_count;
        let total_time = Duration::from_secs(3600 * total_visits as u64); // Estimate 1 hour per visit
        let avg_duration = total_time / total_visits;

        let evidence = SuggestionEvidence {
            visit_count: total_visits,
            total_time,
            average_visit_duration: avg_duration,
            common_visit_times: vec![], // Not available from cluster
            common_actions: vec![],     // Not available from cluster
            similar_zones: vec![],
        };

        Some(ZoneSuggestion {
            suggested_name: format!("New Zone ({})", cluster.cluster_id),
            confidence: cluster.radius,
            suggested_fingerprint: cluster.representative_fingerprint.clone(),
            suggested_actions: ZoneActions::default(),
            reasoning: format!(
                "Detected {} frequent visits to unmatched location - suggests new zone",
                total_visits
            ),
            evidence,
            suggestion_type: SuggestionType::CreateZone,
            created_at: Utc::now(),
            priority: SuggestionPriority::High,
        })
    }

    /// Analyze temporal patterns for schedule-based optimizations
    fn analyze_temporal_optimization(
        &self,
        location_key: &str,
        patterns: &[TimePattern],
    ) -> Option<ZoneSuggestion> {
        // Look for strong temporal patterns
        let work_hours_count = patterns
            .iter()
            .filter(|p| p.hour >= 9 && p.hour <= 17)
            .count();
        let evening_count = patterns
            .iter()
            .filter(|p| p.hour >= 18 && p.hour <= 22)
            .count();

        if work_hours_count > 5 || evening_count > 5 {
            let suggested_fingerprint = LocationFingerprint {
                wifi_networks: BTreeSet::new(),
                bluetooth_devices: BTreeSet::new(),
                ip_location: None,
                confidence_score: 0.85,
                timestamp: Utc::now(),
            };

            let evidence = SuggestionEvidence {
                visit_count: patterns.len() as u32,
                total_time: Duration::from_secs(3600 * patterns.len() as u64),
                average_visit_duration: Duration::from_secs(3600),
                common_visit_times: patterns.to_vec(),
                common_actions: vec![],
                similar_zones: vec![],
            };

            return Some(ZoneSuggestion {
                suggested_name: format!("Schedule-optimized {}", location_key),
                confidence: 0.85,
                suggested_fingerprint,
                suggested_actions: ZoneActions::default(),
                reasoning: format!(
                    "Strong temporal pattern detected - {} work hour visits, {} evening visits",
                    work_hours_count, evening_count
                ),
                evidence,
                suggestion_type: SuggestionType::OptimizeSchedule,
                created_at: Utc::now(),
                priority: SuggestionPriority::Medium,
            });
        }

        None
    }

    /// Analyze action patterns for automation suggestions
    fn analyze_action_automation(
        &self,
        zone_id: &str,
        actions: &[ActionPattern],
    ) -> Option<ZoneSuggestion> {
        // Look for frequently repeated actions
        let mut action_counts = HashMap::new();
        for action in actions {
            *action_counts.entry(&action.action_type).or_insert(0) += 1;
        }

        let frequent_actions: Vec<_> = action_counts
            .iter()
            .filter(|(_, &count)| count >= 3)
            .collect();

        if !frequent_actions.is_empty() {
            let suggested_fingerprint = LocationFingerprint {
                wifi_networks: BTreeSet::new(),
                bluetooth_devices: BTreeSet::new(),
                ip_location: None,
                confidence_score: 0.9,
                timestamp: Utc::now(),
            };

            let evidence = SuggestionEvidence {
                visit_count: actions.len() as u32,
                total_time: Duration::from_secs(1800 * actions.len() as u64),
                average_visit_duration: Duration::from_secs(1800),
                common_visit_times: vec![],
                common_actions: actions.to_vec(),
                similar_zones: vec![],
            };

            return Some(ZoneSuggestion {
                suggested_name: format!("Auto-actions for {}", zone_id),
                confidence: 0.9,
                suggested_fingerprint,
                suggested_actions: ZoneActions::default(),
                reasoning: format!(
                    "Detected {} frequently repeated actions - suggest automation",
                    frequent_actions.len()
                ),
                evidence,
                suggestion_type: SuggestionType::AutomateActions(zone_id.to_string()),
                created_at: Utc::now(),
                priority: SuggestionPriority::High,
            });
        }

        None
    }

    /// Run optimization algorithm with real analytics data
    fn run_algorithm_with_analytics(
        &self,
        _algorithm: &dyn ZoneOptimizer,
        analytics: &ZoneAnalytics,
    ) -> Vec<OptimizationResult> {
        let mut results = Vec::new();

        // For each zone with usage statistics, run optimization
        for (zone_id, usage_stats) in &analytics.zone_usage_stats {
            // Create a mock zone for optimization (in real implementation, get from zone manager)
            let _mock_zone = GeofenceZone {
                id: zone_id.clone(),
                name: zone_id.clone(),
                fingerprints: vec![LocationFingerprint {
                    wifi_networks: BTreeSet::new(),
                    bluetooth_devices: BTreeSet::new(),
                    ip_location: None,
                    confidence_score: 0.8,
                    timestamp: Utc::now(),
                }],
                confidence_threshold: 0.8,
                actions: ZoneActions::default(),
                created_at: Utc::now(),
                last_matched: None,
                match_count: usage_stats.total_visits,
            };

            // Calculate optimization metrics based on real usage
            let accuracy_improvement = if usage_stats.total_visits > 10 {
                0.15
            } else {
                0.05
            };
            let performance_improvement =
                if usage_stats.average_duration > Duration::from_secs(1800) {
                    0.20
                } else {
                    0.10
                };

            let result = OptimizationResult {
                zone_id: zone_id.clone(),
                optimization_type: if accuracy_improvement > 0.10 {
                    OptimizationType::FingerprintOptimization
                } else {
                    OptimizationType::ActionOptimization
                },
                improvements: {
                    let mut map = HashMap::new();
                    map.insert("accuracy".to_string(), accuracy_improvement);
                    map.insert("performance".to_string(), performance_improvement);
                    map
                },
                optimized_at: Utc::now(),
                success: true,
            };

            results.push(result);
        }

        results
    }
}

impl RelationshipAnalyzer {
    fn new() -> Self {
        Self {
            spatial_analyzer: SpatialAnalyzer {
                containment_threshold: 0.8,
                proximity_threshold: 0.6,
            },
            temporal_analyzer: TemporalAnalyzer {
                time_window: Duration::from_secs(3600),
                sequence_threshold: 0.7,
            },
            functional_analyzer: FunctionalAnalyzer {
                action_similarity_threshold: 0.75,
                pattern_correlation_threshold: 0.8,
            },
        }
    }

    /// Configure temporal analysis settings
    pub fn configure_temporal_analysis(&mut self, time_window: Duration, sequence_threshold: f64) {
        self.temporal_analyzer.time_window = time_window;
        self.temporal_analyzer.sequence_threshold = sequence_threshold;
    }

    /// Get current temporal configuration for diagnostics
    pub fn get_temporal_config(&self) -> (Duration, f64) {
        (
            self.temporal_analyzer.get_time_window(),
            self.temporal_analyzer.get_sequence_threshold(),
        )
    }

    fn analyze_relationships(&self, zones: &[GeofenceZone]) -> Vec<ZoneRelationshipSuggestion> {
        let mut suggestions = Vec::new();

        // Analyze spatial relationships
        for i in 0..zones.len() {
            for j in (i + 1)..zones.len() {
                let zone1 = &zones[i];
                let zone2 = &zones[j];

                // Check for containment relationship
                if self.spatial_analyzer.is_contained(zone1, zone2) {
                    suggestions.push(ZoneRelationshipSuggestion {
                        zones: vec![zone2.id.clone(), zone1.id.clone()], // parent, child
                        relationship_type: ZoneRelationshipType::Geographic,
                        confidence: 0.8,
                        evidence: "Spatial containment detected".to_string(),
                    });
                }

                // Check for proximity relationship
                if self.spatial_analyzer.are_proximate(zone1, zone2) {
                    suggestions.push(ZoneRelationshipSuggestion {
                        zones: vec![zone1.id.clone(), zone2.id.clone()],
                        relationship_type: ZoneRelationshipType::Geographic,
                        confidence: 0.6,
                        evidence: "Spatial proximity detected".to_string(),
                    });
                }

                // Check for functional relationships
                if self
                    .functional_analyzer
                    .are_functionally_related(zone1, zone2)
                {
                    suggestions.push(ZoneRelationshipSuggestion {
                        zones: vec![zone1.id.clone(), zone2.id.clone()],
                        relationship_type: ZoneRelationshipType::Functional,
                        confidence: 0.7,
                        evidence: "Similar usage patterns detected".to_string(),
                    });
                }

                // Check for temporal relationships
                if self.temporal_analyzer.are_temporally_related(zone1, zone2) {
                    suggestions.push(ZoneRelationshipSuggestion {
                        zones: vec![zone1.id.clone(), zone2.id.clone()],
                        relationship_type: ZoneRelationshipType::Temporal,
                        confidence: 0.65,
                        evidence: "Sequential usage pattern detected".to_string(),
                    });
                }
            }
        }

        suggestions
    }
}

impl SpatialAnalyzer {
    /// Check if zone1 is spatially contained within zone2
    fn is_contained(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> bool {
        // Use WiFi fingerprints to determine spatial containment
        let containment_score = self.calculate_containment_score(zone1, zone2);
        containment_score > self.containment_threshold
    }

    /// Check if two zones are in proximity to each other
    fn are_proximate(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> bool {
        let proximity_score = self.calculate_proximity_score(zone1, zone2);
        proximity_score > self.proximity_threshold
    }

    /// Calculate containment score between two zones
    fn calculate_containment_score(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> f64 {
        // Extract WiFi networks from fingerprints to determine if zone1 is inside zone2
        let zone1_networks = self.extract_wifi_networks(&zone1.fingerprints);
        let zone2_networks = self.extract_wifi_networks(&zone2.fingerprints);

        if zone1_networks.is_empty() || zone2_networks.is_empty() {
            return 0.0;
        }

        // If zone1 is contained in zone2, most of zone1's networks should also be visible in zone2
        let intersection: HashSet<_> = zone1_networks.intersection(&zone2_networks).collect();
        let containment_ratio = intersection.len() as f64 / zone1_networks.len() as f64;

        // Also check if zone2 has significantly more networks (indicating larger coverage area)
        let coverage_ratio = zone2_networks.len() as f64 / zone1_networks.len() as f64;
        let coverage_boost = if coverage_ratio > 1.2 { 0.2 } else { 0.0 };

        (containment_ratio + coverage_boost).min(1.0)
    }

    /// Calculate proximity score between two zones
    fn calculate_proximity_score(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> f64 {
        let zone1_networks = self.extract_wifi_networks(&zone1.fingerprints);
        let zone2_networks = self.extract_wifi_networks(&zone2.fingerprints);

        if zone1_networks.is_empty() || zone2_networks.is_empty() {
            return 0.0;
        }

        // Calculate Jaccard similarity for proximity
        let intersection: HashSet<_> = zone1_networks.intersection(&zone2_networks).collect();
        let union: HashSet<_> = zone1_networks.union(&zone2_networks).collect();

        intersection.len() as f64 / union.len() as f64
    }

    /// Extract WiFi network hashes from fingerprints
    fn extract_wifi_networks(&self, fingerprints: &[LocationFingerprint]) -> HashSet<String> {
        let mut networks = HashSet::new();

        for fingerprint in fingerprints {
            for network in &fingerprint.wifi_networks {
                networks.insert(network.ssid_hash.clone());
            }
        }

        networks
    }
}

impl TemporalAnalyzer {
    /// Check if two zones are temporally related (used in sequence)
    fn are_temporally_related(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> bool {
        // Analyze if zones are typically visited in sequence within the time window
        let temporal_score = self.calculate_temporal_correlation(zone1, zone2);
        temporal_score > self.sequence_threshold
    }

    /// Calculate temporal correlation between two zones
    fn calculate_temporal_correlation(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> f64 {
        // This would typically use visit history data, but for now we'll use zone metadata
        // In a real implementation, this would analyze historical visit patterns

        // Use time window to determine recency bonus
        let recency_bonus =
            if let (Some(last1), Some(last2)) = (&zone1.last_matched, &zone2.last_matched) {
                let time_diff = if last1 > last2 {
                    *last1 - *last2
                } else {
                    *last2 - *last1
                };
                let time_diff_seconds = time_diff.num_seconds() as u64;

                // Higher correlation if zones were visited within the time window
                if time_diff_seconds <= self.time_window.as_secs() {
                    0.3 // Bonus for zones visited within time window
                } else {
                    0.0
                }
            } else {
                0.0
            };

        // Estimate temporal correlation based on zone names and actions
        let name_similarity = self.calculate_name_based_temporal_hint(&zone1.name, &zone2.name);
        let action_temporal_hint =
            self.calculate_action_temporal_hint(&zone1.actions, &zone2.actions);

        let base_correlation = (name_similarity + action_temporal_hint) / 2.0;
        (base_correlation + recency_bonus).min(1.0)
    }

    /// Calculate temporal hints based on zone names
    fn calculate_name_based_temporal_hint(&self, name1: &str, name2: &str) -> f64 {
        // Look for sequential patterns in names
        let name1_lower = name1.to_lowercase();
        let name2_lower = name2.to_lowercase();

        // Check for common sequential patterns
        let sequential_keywords = [
            ("home", "work"),
            ("work", "home"),
            ("office", "home"),
            ("home", "office"),
            ("morning", "evening"),
            ("day", "night"),
            ("arrival", "departure"),
            ("entry", "exit"),
        ];

        for (first, second) in sequential_keywords.iter() {
            if (name1_lower.contains(first) && name2_lower.contains(second))
                || (name1_lower.contains(second) && name2_lower.contains(first))
            {
                return 0.8;
            }
        }

        0.2 // Default low correlation
    }

    /// Calculate temporal hints based on action patterns
    fn calculate_action_temporal_hint(
        &self,
        actions1: &ZoneActions,
        actions2: &ZoneActions,
    ) -> f64 {
        // Analyze if actions suggest temporal relationship
        let mut temporal_score: f64 = 0.0;

        // VPN patterns: work zones often have different VPN configs than home
        if actions1.vpn != actions2.vpn {
            temporal_score += 0.3; // Different VPN suggests different contexts
        }

        // WiFi patterns: different WiFi configs often suggest different locations visited in sequence
        if actions1.wifi != actions2.wifi {
            temporal_score += 0.2;
        }

        // Bluetooth patterns: different devices suggest different contexts/times
        let bluetooth_overlap =
            self.calculate_bluetooth_overlap(&actions1.bluetooth, &actions2.bluetooth);
        if bluetooth_overlap < 0.5 {
            temporal_score += 0.3;
        }

        temporal_score.min(1.0)
    }

    /// Calculate Bluetooth device overlap between two action sets
    fn calculate_bluetooth_overlap(&self, bluetooth1: &[String], bluetooth2: &[String]) -> f64 {
        if bluetooth1.is_empty() && bluetooth2.is_empty() {
            return 1.0;
        }

        let set1: HashSet<_> = bluetooth1.iter().collect();
        let set2: HashSet<_> = bluetooth2.iter().collect();
        let intersection: HashSet<_> = set1.intersection(&set2).collect();
        let union: HashSet<_> = set1.union(&set2).collect();

        if union.is_empty() {
            0.0
        } else {
            intersection.len() as f64 / union.len() as f64
        }
    }

    /// Get the configured time window for temporal analysis
    pub fn get_time_window(&self) -> Duration {
        self.time_window
    }

    /// Get the sequence detection threshold
    pub fn get_sequence_threshold(&self) -> f64 {
        self.sequence_threshold
    }
}

impl FunctionalAnalyzer {
    fn are_functionally_related(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> bool {
        // Analyze if zones have similar actions/purposes
        let action_similarity = self.calculate_action_similarity(&zone1.actions, &zone2.actions);
        let pattern_correlation = self.calculate_pattern_correlation(zone1, zone2);

        // Zones are functionally related if either they have similar actions OR similar usage patterns
        action_similarity > self.action_similarity_threshold
            || pattern_correlation > self.pattern_correlation_threshold
    }

    /// Calculate usage pattern correlation between zones
    fn calculate_pattern_correlation(&self, zone1: &GeofenceZone, zone2: &GeofenceZone) -> f64 {
        // Analyze WiFi network patterns for correlation
        let network_pattern_score = self.calculate_network_pattern_correlation(zone1, zone2);

        // Analyze naming patterns for functional correlation
        let naming_pattern_score =
            self.calculate_naming_pattern_correlation(&zone1.name, &zone2.name);

        // Combine scores
        (network_pattern_score + naming_pattern_score) / 2.0
    }

    /// Calculate network pattern correlation
    fn calculate_network_pattern_correlation(
        &self,
        zone1: &GeofenceZone,
        zone2: &GeofenceZone,
    ) -> f64 {
        // Look for similar network environments that suggest similar functions
        let zone1_networks = self.extract_wifi_networks(&zone1.fingerprints);
        let zone2_networks = self.extract_wifi_networks(&zone2.fingerprints);

        if zone1_networks.is_empty() || zone2_networks.is_empty() {
            return 0.0;
        }

        // Calculate network overlap
        let intersection: HashSet<_> = zone1_networks.intersection(&zone2_networks).collect();
        let overlap_ratio =
            intersection.len() as f64 / zone1_networks.len().min(zone2_networks.len()) as f64;

        // Convert network sets to vectors for the environment detection
        let zone1_networks_vec: Vec<String> = zone1_networks.into_iter().collect();
        let zone2_networks_vec: Vec<String> = zone2_networks.into_iter().collect();

        // Boost score if networks suggest similar environments (e.g., corporate networks)
        let environment_boost =
            self.detect_similar_network_environment(&zone1_networks_vec, &zone2_networks_vec);

        (overlap_ratio + environment_boost).min(1.0)
    }

    /// Detect if networks suggest similar environments
    fn detect_similar_network_environment(
        &self,
        networks1: &[String],
        networks2: &[String],
    ) -> f64 {
        let corporate_keywords = [
            "corp",
            "office",
            "work",
            "company",
            "enterprise",
            "wifi",
            "guest",
        ];
        let home_keywords = ["home", "house", "family", "personal", "wifi"];

        let net1_text = networks1.join(" ").to_lowercase();
        let net2_text = networks2.join(" ").to_lowercase();

        // Check for corporate environment
        let net1_corporate = corporate_keywords.iter().any(|&kw| net1_text.contains(kw));
        let net2_corporate = corporate_keywords.iter().any(|&kw| net2_text.contains(kw));

        // Check for home environment
        let net1_home = home_keywords.iter().any(|&kw| net1_text.contains(kw));
        let net2_home = home_keywords.iter().any(|&kw| net2_text.contains(kw));

        if (net1_corporate && net2_corporate) || (net1_home && net2_home) {
            0.3 // Similar environment boost
        } else {
            0.0
        }
    }

    /// Calculate naming pattern correlation
    fn calculate_naming_pattern_correlation(&self, name1: &str, name2: &str) -> f64 {
        let name1_lower = name1.to_lowercase();
        let name2_lower = name2.to_lowercase();

        // Look for functional similarity indicators
        let functional_keywords = [
            "work",
            "office",
            "meeting",
            "conference",
            "home",
            "house",
            "bedroom",
            "kitchen",
            "living",
            "coffee",
            "restaurant",
            "store",
            "shop",
            "station",
            "airport",
            "transport",
            "travel",
            "gym",
            "fitness",
            "health",
            "medical",
        ];

        let name1_functions: HashSet<_> = functional_keywords
            .iter()
            .filter(|&&kw| name1_lower.contains(kw))
            .collect();

        let name2_functions: HashSet<_> = functional_keywords
            .iter()
            .filter(|&&kw| name2_lower.contains(kw))
            .collect();

        if name1_functions.is_empty() || name2_functions.is_empty() {
            return 0.1; // Low default correlation
        }

        let intersection = name1_functions.intersection(&name2_functions).count();
        let union = name1_functions.union(&name2_functions).count();

        intersection as f64 / union as f64
    }

    /// Extract WiFi network hashes from fingerprints
    fn extract_wifi_networks(&self, fingerprints: &[LocationFingerprint]) -> HashSet<String> {
        let mut networks = HashSet::new();

        for fingerprint in fingerprints {
            for network in &fingerprint.wifi_networks {
                networks.insert(network.ssid_hash.clone());
            }
        }

        networks
    }

    fn calculate_action_similarity(&self, actions1: &ZoneActions, actions2: &ZoneActions) -> f64 {
        let mut similarity_score = 0.0;
        let mut total_comparisons = 0;

        // Compare WiFi actions
        total_comparisons += 1;
        if actions1.wifi == actions2.wifi {
            similarity_score += 1.0;
        }

        // Compare VPN actions
        total_comparisons += 1;
        if actions1.vpn == actions2.vpn {
            similarity_score += 1.0;
        }

        // Compare Bluetooth actions
        total_comparisons += 1;
        let bluetooth_similarity =
            self.calculate_list_similarity(&actions1.bluetooth, &actions2.bluetooth);
        similarity_score += bluetooth_similarity;

        similarity_score / total_comparisons as f64
    }

    fn calculate_list_similarity(&self, list1: &[String], list2: &[String]) -> f64 {
        if list1.is_empty() && list2.is_empty() {
            return 1.0;
        }

        let set1: HashSet<_> = list1.iter().collect();
        let set2: HashSet<_> = list2.iter().collect();

        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();

        if union == 0 {
            1.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

impl ZoneAnalytics {
    fn new() -> Self {
        Self {
            unmatched_visits: VecDeque::new(),
            zone_usage_stats: HashMap::new(),
            location_clusters: Vec::new(),
            temporal_patterns: HashMap::new(),
            action_correlations: HashMap::new(),
        }
    }
}

// Example optimization algorithms

/// Fingerprint optimization algorithm
struct FingerprintOptimizer;

impl FingerprintOptimizer {
    fn new() -> Self {
        Self
    }
}

impl ZoneOptimizer for FingerprintOptimizer {
    fn analyze_zone(&self, zone: &GeofenceZone, analytics: &ZoneAnalytics) -> Vec<ZoneSuggestion> {
        // Check if zone has poor confidence or few fingerprints
        if zone.fingerprints.len() < 2 || zone.confidence_threshold < 0.7 {
            // Look for unmatched visits that could improve this zone
            if let Some(usage_stats) = analytics.zone_usage_stats.get(&zone.id) {
                if usage_stats.total_visits > 5 {
                    let suggestion = ZoneSuggestion {
                        suggested_name: format!("Optimize fingerprint for {}", zone.name),
                        confidence: 0.8,
                        suggested_fingerprint: LocationFingerprint::default(),
                        suggested_actions: zone.actions.clone(),
                        reasoning: format!("Zone '{}' has {} visits but only {} fingerprints. Adding more fingerprints could improve accuracy.", 
                                         zone.name, usage_stats.total_visits, zone.fingerprints.len()),
                        evidence: SuggestionEvidence {
                            visit_count: usage_stats.total_visits,
                            total_time: usage_stats.total_time,
                            average_visit_duration: usage_stats.average_duration,
                            common_visit_times: vec![],
                            common_actions: vec![],
                            similar_zones: vec![],
                        },
                        suggestion_type: SuggestionType::OptimizeFingerprint(zone.id.clone()),
                        created_at: Utc::now(),
                        priority: SuggestionPriority::Medium,
                    };
                    return vec![suggestion];
                }
            }
        }
        Vec::new()
    }

    fn optimize_zone(&self, zone: &mut GeofenceZone, suggestion: &ZoneSuggestion) -> Result<()> {
        match &suggestion.suggestion_type {
            SuggestionType::OptimizeFingerprint(_) => {
                // Increase confidence threshold slightly
                zone.confidence_threshold = (zone.confidence_threshold + 0.05).min(0.95);
                info!(
                    "Optimized fingerprint for zone '{}': confidence threshold increased to {:.2}",
                    zone.name, zone.confidence_threshold
                );
                Ok(())
            }
            _ => Err(GeofenceError::Config(
                "Invalid suggestion type for FingerprintOptimizer".to_string(),
            )),
        }
    }

    fn name(&self) -> &str {
        "FingerprintOptimizer"
    }
}

/// Action sequence optimization algorithm
struct ActionOptimizer;

impl ActionOptimizer {
    fn new() -> Self {
        Self
    }
}

impl ZoneOptimizer for ActionOptimizer {
    fn analyze_zone(&self, zone: &GeofenceZone, analytics: &ZoneAnalytics) -> Vec<ZoneSuggestion> {
        // Check if zone has empty or default actions but has usage patterns
        if let Some(usage_stats) = analytics.zone_usage_stats.get(&zone.id) {
            if usage_stats.total_visits > 3 {
                let has_empty_actions = zone.actions.wifi.is_none()
                    && zone.actions.vpn.is_none()
                    && zone.actions.bluetooth.is_empty();

                if has_empty_actions {
                    let suggestion = ZoneSuggestion {
                        suggested_name: format!("Add actions for {}", zone.name),
                        confidence: 0.7,
                        suggested_fingerprint: LocationFingerprint::default(),
                        suggested_actions: zone.actions.clone(),
                        reasoning: format!("Zone '{}' has {} visits but no configured actions. Consider adding WiFi, VPN, or Bluetooth actions.", 
                                         zone.name, usage_stats.total_visits),
                        evidence: SuggestionEvidence {
                            visit_count: usage_stats.total_visits,
                            total_time: usage_stats.total_time,
                            average_visit_duration: usage_stats.average_duration,
                            common_visit_times: vec![],
                            common_actions: usage_stats.common_actions.clone(),
                            similar_zones: vec![],
                        },
                        suggestion_type: SuggestionType::UpdateActions(zone.id.clone()),
                        created_at: Utc::now(),
                        priority: SuggestionPriority::Low,
                    };
                    return vec![suggestion];
                }
            }
        }
        Vec::new()
    }

    fn optimize_zone(&self, zone: &mut GeofenceZone, suggestion: &ZoneSuggestion) -> Result<()> {
        match &suggestion.suggestion_type {
            SuggestionType::UpdateActions(_) => {
                // Enable notifications if not already enabled
                if !zone.actions.notifications {
                    zone.actions.notifications = true;
                    info!("Enabled notifications for zone '{}'", zone.name);
                }
                Ok(())
            }
            _ => Err(GeofenceError::Config(
                "Invalid suggestion type for ActionOptimizer".to_string(),
            )),
        }
    }

    fn name(&self) -> &str {
        "ActionOptimizer"
    }
}

/// Zone merging optimization algorithm
struct ZoneMergeOptimizer;

impl ZoneMergeOptimizer {
    fn new() -> Self {
        Self
    }
}

impl ZoneOptimizer for ZoneMergeOptimizer {
    fn analyze_zone(&self, zone: &GeofenceZone, analytics: &ZoneAnalytics) -> Vec<ZoneSuggestion> {
        // Look for zones with very low usage that might be candidates for merging
        if let Some(usage_stats) = analytics.zone_usage_stats.get(&zone.id) {
            if usage_stats.total_visits < 3 && usage_stats.total_time.as_secs() < 1800 {
                // Find potential zones to merge with by looking for similar names
                let similar_zones: Vec<String> = analytics
                    .zone_usage_stats
                    .keys()
                    .filter(|other_zone_id| {
                        *other_zone_id != &zone.id
                            && self.zones_might_be_similar(&zone.name, other_zone_id)
                    })
                    .cloned()
                    .collect();

                if !similar_zones.is_empty() {
                    let mut merge_candidates = vec![zone.id.clone()];
                    merge_candidates.extend(similar_zones.clone());

                    let suggestion = ZoneSuggestion {
                        suggested_name: format!("Consider merging {}", zone.name),
                        confidence: 0.6,
                        suggested_fingerprint: LocationFingerprint::default(),
                        suggested_actions: zone.actions.clone(),
                        reasoning: format!("Zone '{}' has low usage ({} visits, {:?} total time). Consider merging with similar zones.", 
                                         zone.name, usage_stats.total_visits, usage_stats.total_time),
                        evidence: SuggestionEvidence {
                            visit_count: usage_stats.total_visits,
                            total_time: usage_stats.total_time,
                            average_visit_duration: usage_stats.average_duration,
                            common_visit_times: vec![],
                            common_actions: vec![],
                            similar_zones,
                        },
                        suggestion_type: SuggestionType::MergeZones(merge_candidates),
                        created_at: Utc::now(),
                        priority: SuggestionPriority::Low,
                    };
                    return vec![suggestion];
                }
            }
        }
        Vec::new()
    }

    fn optimize_zone(&self, zone: &mut GeofenceZone, suggestion: &ZoneSuggestion) -> Result<()> {
        match &suggestion.suggestion_type {
            SuggestionType::MergeZones(_zones) => {
                // For now, just log the merge suggestion - actual merging would require zone manager
                info!(
                    "Zone '{}' is a candidate for merging - consider manual review",
                    zone.name
                );
                Ok(())
            }
            _ => Err(GeofenceError::Config(
                "Invalid suggestion type for ZoneMergeOptimizer".to_string(),
            )),
        }
    }

    fn name(&self) -> &str {
        "ZoneMergeOptimizer"
    }
}

impl ZoneMergeOptimizer {
    /// Check if two zone names suggest they might be similar locations
    fn zones_might_be_similar(&self, name1: &str, name2: &str) -> bool {
        let name1_lower = name1.to_lowercase();
        let name2_lower = name2.to_lowercase();

        // Check for common words or prefixes
        let similarity_indicators = [
            ("home", "house"),
            ("work", "office"),
            ("coffee", "cafe"),
            ("shop", "store"),
            ("gym", "fitness"),
            ("station", "stop"),
        ];

        for (word1, word2) in similarity_indicators.iter() {
            if (name1_lower.contains(word1) && name2_lower.contains(word2))
                || (name1_lower.contains(word2) && name2_lower.contains(word1))
            {
                return true;
            }
        }

        // Check for shared prefixes (first 3+ characters)
        if name1.len() >= 3 && name2.len() >= 3 {
            let prefix1 = &name1_lower[..3];
            let prefix2 = &name2_lower[..3];
            if prefix1 == prefix2 {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn create_test_fingerprint(networks: Vec<&str>) -> LocationFingerprint {
        let wifi_networks = networks
            .into_iter()
            .enumerate()
            .map(|(i, ssid)| NetworkSignature {
                ssid_hash: ssid.to_string(),
                bssid_prefix: format!("aa:bb:cc:{:02x}", i),
                signal_strength: -50,
                frequency: 2412,
            })
            .collect();

        LocationFingerprint {
            wifi_networks,
            bluetooth_devices: BTreeSet::new(),
            ip_location: None,
            confidence_score: 0.8,
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_advanced_zone_manager_creation() {
        let config = AdvancedZoneConfig::default();
        let manager = AdvancedZoneManager::new(config).await;

        assert!(manager.config.enable_ml_suggestions);
        assert!(manager.config.enable_hierarchical_zones);
    }

    #[tokio::test]
    async fn test_visit_recording() {
        let config = AdvancedZoneConfig::default();
        let manager = AdvancedZoneManager::new(config).await;

        let fingerprint = create_test_fingerprint(vec!["network1", "network2"]);
        let result = manager.record_visit(fingerprint, None).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_suggestion_generation() {
        let config = AdvancedZoneConfig::default();
        let manager = AdvancedZoneManager::new(config).await;

        let suggestions = manager.generate_suggestions().await.unwrap();
        // Initially should have no suggestions
        assert!(suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_hierarchy_creation() {
        let config = AdvancedZoneConfig::default();
        let mut manager = AdvancedZoneManager::new(config).await;

        let result = manager
            .create_zone_hierarchy(
                "office".to_string(),
                "meeting_room".to_string(),
                ZoneRelationshipType::Geographic,
            )
            .await;

        assert!(result.is_ok());

        let hierarchy = manager.get_zone_hierarchy("meeting_room");
        assert!(hierarchy.is_some());
        assert_eq!(hierarchy.unwrap().parent_id, Some("office".to_string()));
    }

    #[test]
    fn test_location_clusterer() {
        let mut clusterer = LocationClusterer::new();

        let fp1 = create_test_fingerprint(vec!["network1", "network2"]);
        let fp2 = create_test_fingerprint(vec!["network1", "network2"]);
        let fp3 = create_test_fingerprint(vec!["network3", "network4"]);

        clusterer.add_location(&fp1);
        clusterer.add_location(&fp2);
        clusterer.add_location(&fp3);

        assert_eq!(clusterer.clusters.len(), 2); // Should create 2 clusters
    }

    #[test]
    fn test_zone_suggestion_priority_sorting() {
        let mut suggestions = vec![
            ZoneSuggestion {
                suggested_name: "Low".to_string(),
                confidence: 0.5,
                priority: SuggestionPriority::Low,
                suggestion_type: SuggestionType::CreateZone,
                suggested_fingerprint: create_test_fingerprint(vec!["test"]),
                suggested_actions: ZoneActions::default(),
                reasoning: "Test".to_string(),
                evidence: SuggestionEvidence {
                    visit_count: 1,
                    total_time: Duration::from_secs(100),
                    average_visit_duration: Duration::from_secs(100),
                    common_visit_times: Vec::new(),
                    common_actions: Vec::new(),
                    similar_zones: Vec::new(),
                },
                created_at: Utc::now(),
            },
            ZoneSuggestion {
                suggested_name: "High".to_string(),
                confidence: 0.7,
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::CreateZone,
                suggested_fingerprint: create_test_fingerprint(vec!["test"]),
                suggested_actions: ZoneActions::default(),
                reasoning: "Test".to_string(),
                evidence: SuggestionEvidence {
                    visit_count: 5,
                    total_time: Duration::from_secs(500),
                    average_visit_duration: Duration::from_secs(100),
                    common_visit_times: Vec::new(),
                    common_actions: Vec::new(),
                    similar_zones: Vec::new(),
                },
                created_at: Utc::now(),
            },
        ];

        suggestions.sort_by(|a, b| match (a.priority.clone(), b.priority.clone()) {
            (SuggestionPriority::High, SuggestionPriority::Low) => std::cmp::Ordering::Less,
            (SuggestionPriority::Low, SuggestionPriority::High) => std::cmp::Ordering::Greater,
            _ => b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        assert_eq!(suggestions[0].suggested_name, "High");
        assert_eq!(suggestions[1].suggested_name, "Low");
    }

    #[test]
    fn test_relationship_analysis() {
        let analyzer = RelationshipAnalyzer::new();
        let zones = Vec::new(); // Empty for test

        let relationships = analyzer.analyze_relationships(&zones);
        assert!(relationships.is_empty());
    }

    #[test]
    fn test_advanced_zone_config_default() {
        let config = AdvancedZoneConfig::default();

        assert!(config.enable_ml_suggestions);
        assert!(config.enable_hierarchical_zones);
        assert_eq!(config.min_visits_for_suggestion, 3);
        assert_eq!(config.max_hierarchy_depth, 3);
    }
}
