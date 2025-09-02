//! Usage patterns module for personalized menu ordering and action prediction
//!
//! This module learns from user behavior to:
//! - Reorder menu items based on frequency and context
//! - Predict likely next actions
//! - Suggest workflow automations
//! - Adapt to usage patterns over time

use super::{
    MlError, NetworkContext, NetworkType, PredictionResult, TrainingData, ModelPersistence,
    cosine_similarity, normalize_features,
};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};
use log::debug;
use chrono::{Timelike, Datelike};

/// User action types that can be tracked
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserAction {
    ConnectWifi(String),
    DisconnectWifi,
    ConnectBluetooth(String),
    DisconnectBluetooth,
    EnableTailscale,
    DisableTailscale,
    SelectExitNode(String),
    DisableExitNode,
    RunDiagnostic(String),
    ToggleAirplaneMode,
    CustomAction(String),
}

/// Action sequence for pattern detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSequence {
    pub actions: Vec<UserAction>,
    pub context: NetworkContext,
    pub timestamp: i64,  // Unix timestamp
}

/// Usage statistics for an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStats {
    pub total_count: u32,
    pub recent_count: u32,  // Last 7 days
    pub hourly_distribution: [u32; 24],
    pub daily_distribution: [u32; 7],
    pub last_used: Option<i64>,  // Unix timestamp
    pub average_time_between_uses: f32,  // In hours
    pub contexts: Vec<NetworkContext>,
}

impl Default for ActionStats {
    fn default() -> Self {
        Self {
            total_count: 0,
            recent_count: 0,
            hourly_distribution: [0; 24],
            daily_distribution: [0; 7],
            last_used: None,
            average_time_between_uses: 0.0,
            contexts: Vec::new(),
        }
    }
}

/// Usage pattern learner for menu personalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePatternLearner {
    action_stats: HashMap<UserAction, ActionStats>,
    action_sequences: VecDeque<ActionSequence>,
    frequent_workflows: Vec<Vec<UserAction>>,
    context_associations: HashMap<u64, Vec<UserAction>>,  // location_hash -> common actions
    training_data: TrainingData<UserAction>,
    config: UsageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageConfig {
    pub max_sequence_length: usize,
    pub max_history_size: usize,
    pub workflow_threshold: u32,  // Min occurrences to consider a workflow
    pub recency_weight: f32,
    pub frequency_weight: f32,
    pub context_weight: f32,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            max_sequence_length: 5,
            max_history_size: 1000,
            workflow_threshold: 3,
            recency_weight: 0.4,
            frequency_weight: 0.35,
            context_weight: 0.25,
        }
    }
}

impl UsagePatternLearner {
    pub fn new() -> Self {
        Self {
            action_stats: HashMap::new(),
            action_sequences: VecDeque::new(),
            frequent_workflows: Vec::new(),
            context_associations: HashMap::new(),
            training_data: TrainingData::default(),
            config: UsageConfig::default(),
        }
    }

    pub fn with_config(config: UsageConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Record a user action
    pub fn record_action(&mut self, action: UserAction, context: NetworkContext) {
        let now = chrono::Utc::now();
        let now_timestamp = now.timestamp();

        // Update action statistics
        let stats = self.action_stats.entry(action.clone()).or_default();
        stats.total_count += 1;

        // Update recent count (last 7 days)
        if let Some(last_used_ts) = stats.last_used {
            let days_since = (now_timestamp - last_used_ts) / 86400;  // seconds in a day
            if days_since <= 7 {
                stats.recent_count += 1;
            } else {
                stats.recent_count = 1;  // Reset if it's been more than 7 days
            }

            // Update average time between uses
            let hours_since = ((now_timestamp - last_used_ts) as f32) / 3600.0;  // seconds in an hour
            stats.average_time_between_uses =
                (stats.average_time_between_uses * (stats.total_count - 1) as f32 + hours_since)
                / stats.total_count as f32;
        }

        stats.last_used = Some(now_timestamp);
        stats.hourly_distribution[now.hour() as usize] += 1;
        stats.daily_distribution[now.weekday().num_days_from_monday() as usize] += 1;
        stats.contexts.push(context.clone());

        // Limit context history
        if stats.contexts.len() > 100 {
            stats.contexts.remove(0);
        }

        // Update context associations
        self.context_associations
            .entry(context.location_hash)
            .or_default()
            .push(action.clone());

        // Add to sequences
        if let Some(last_seq) = self.action_sequences.back_mut() {
            if last_seq.actions.len() < self.config.max_sequence_length
                && (now_timestamp - last_seq.timestamp) < 300 {  // 5 minutes in seconds
                // Add to current sequence if recent enough
                last_seq.actions.push(action.clone());
            } else {
                // Start new sequence
                self.action_sequences.push_back(ActionSequence {
                    actions: vec![action.clone()],
                    context: context.clone(),
                    timestamp: now_timestamp,
                });
            }
        } else {
            // First sequence
            self.action_sequences.push_back(ActionSequence {
                actions: vec![action.clone()],
                context: context.clone(),
                timestamp: now_timestamp,
            });
        }

        // Limit sequence history
        while self.action_sequences.len() > self.config.max_history_size {
            self.action_sequences.pop_front();
        }

        // Update frequent workflows
        self.update_frequent_workflows();

        // Add training sample
        let features = self.extract_action_features(&action, &context);
        self.training_data.add_sample(features, action, context);
    }

    /// Update frequently used workflows
    fn update_frequent_workflows(&mut self) {
        let mut workflow_counts: HashMap<Vec<UserAction>, u32> = HashMap::new();

        for sequence in &self.action_sequences {
            if sequence.actions.len() >= 2 {
                // Check all subsequences
                for window_size in 2..=sequence.actions.len().min(self.config.max_sequence_length) {
                    for i in 0..=sequence.actions.len() - window_size {
                        let workflow = sequence.actions[i..i + window_size].to_vec();
                        *workflow_counts.entry(workflow).or_insert(0) += 1;
                    }
                }
            }
        }

        // Keep workflows that occur frequently
        self.frequent_workflows = workflow_counts
            .into_iter()
            .filter(|(_, count)| *count >= self.config.workflow_threshold)
            .map(|(workflow, _)| workflow)
            .collect();
    }

    /// Extract features for an action
    fn extract_action_features(&self, action: &UserAction, context: &NetworkContext) -> Vec<f32> {
        let mut features = Vec::new();

        // Time-based features
        features.push(context.time_of_day as f32 / 24.0);
        features.push(context.day_of_week as f32 / 7.0);

        // Context features
        features.push(if context.network_type == NetworkType::WiFi { 1.0 } else { 0.0 });
        features.push(context.signal_strength.unwrap_or(0.0));

        // Action statistics features
        if let Some(stats) = self.action_stats.get(action) {
            features.push(stats.total_count as f32 / 100.0);  // Normalized
            features.push(stats.recent_count as f32 / 10.0);   // Normalized
            features.push(stats.average_time_between_uses / 168.0);  // Normalized to weeks

            // Hour distribution entropy (measure of time-specific usage)
            let hour_entropy = Self::calculate_entropy(&stats.hourly_distribution);
            features.push(hour_entropy);
        } else {
            features.extend(vec![0.0; 4]);
        }

        normalize_features(&features)
    }

    /// Calculate entropy of a distribution
    fn calculate_entropy(distribution: &[u32]) -> f32 {
        let total: u32 = distribution.iter().sum();
        if total == 0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &count in distribution {
            if count > 0 {
                let p = count as f32 / total as f32;
                entropy -= p * p.log2();
            }
        }
        entropy
    }

    /// Get personalized menu ordering
    pub fn get_personalized_menu_order(
        &self,
        available_actions: Vec<String>,
        context: &NetworkContext,
    ) -> Vec<String> {
        let mut scored_actions: Vec<(String, f32)> = available_actions
            .into_iter()
            .map(|action_str| {
                let score = self.calculate_action_score(&action_str, context);
                (action_str, score)
            })
            .collect();

        // Sort by score (descending)
        scored_actions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored_actions.into_iter()
            .map(|(action, _)| action)
            .collect()
    }

    /// Calculate score for an action based on usage patterns
    fn calculate_action_score(&self, action_str: &str, context: &NetworkContext) -> f32 {
        // Try to match action string to UserAction
        let action = self.parse_action_string(action_str);

        if let Some(action) = action {
            if let Some(stats) = self.action_stats.get(&action) {
                // Recency score
                let recency_score = if let Some(last_used_ts) = stats.last_used {
                    let hours_since = ((chrono::Utc::now().timestamp() - last_used_ts) as f32) / 3600.0;
                    1.0 / (1.0 + hours_since / 24.0)  // Decay over days
                } else {
                    0.0
                };

                // Frequency score
                let frequency_score = (stats.total_count as f32 / 100.0).min(1.0);

                // Context score
                let context_score = self.calculate_context_similarity(&action, context);

                // Time-based score
                let time_score = {
                    let hour_weight = stats.hourly_distribution[context.time_of_day as usize] as f32
                        / stats.total_count.max(1) as f32;
                    let day_weight = stats.daily_distribution[context.day_of_week as usize] as f32
                        / stats.total_count.max(1) as f32;
                    (hour_weight + day_weight) / 2.0
                };

                // Weighted combination
                recency_score * self.config.recency_weight +
                frequency_score * self.config.frequency_weight +
                context_score * self.config.context_weight +
                time_score * 0.1  // Small time bonus
            } else {
                0.1  // Base score for unseen actions
            }
        } else {
            0.05  // Minimal score for unrecognized actions
        }
    }

    /// Calculate context similarity
    fn calculate_context_similarity(&self, action: &UserAction, context: &NetworkContext) -> f32 {
        if let Some(stats) = self.action_stats.get(action) {
            if stats.contexts.is_empty() {
                return 0.5;
            }

            // Find most similar historical context
            let context_vec = vec![
                context.time_of_day as f32 / 24.0,
                context.day_of_week as f32 / 7.0,
                if context.network_type == NetworkType::WiFi { 1.0 } else { 0.0 },
                context.signal_strength.unwrap_or(0.0),
            ];

            let max_similarity = stats.contexts.iter()
                .map(|hist_context| {
                    let hist_vec = vec![
                        hist_context.time_of_day as f32 / 24.0,
                        hist_context.day_of_week as f32 / 7.0,
                        if hist_context.network_type == NetworkType::WiFi { 1.0 } else { 0.0 },
                        hist_context.signal_strength.unwrap_or(0.0),
                    ];
                    cosine_similarity(&context_vec, &hist_vec)
                })
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);

            max_similarity
        } else {
            0.0
        }
    }

    /// Parse action string to UserAction
    fn parse_action_string(&self, action_str: &str) -> Option<UserAction> {
        // Simple pattern matching - in real implementation, this would be more sophisticated
        if action_str.contains("WiFi") {
            if action_str.contains("Disconnect") {
                Some(UserAction::DisconnectWifi)
            } else {
                Some(UserAction::ConnectWifi("network".to_string()))
            }
        } else if action_str.contains("Bluetooth") {
            if action_str.contains("Disconnect") {
                Some(UserAction::DisconnectBluetooth)
            } else {
                Some(UserAction::ConnectBluetooth("device".to_string()))
            }
        } else if action_str.contains("Tailscale") {
            if action_str.contains("Disable") {
                Some(UserAction::DisableTailscale)
            } else {
                Some(UserAction::EnableTailscale)
            }
        } else if action_str.contains("Exit Node") {
            if action_str.contains("Disable") {
                Some(UserAction::DisableExitNode)
            } else {
                Some(UserAction::SelectExitNode("node".to_string()))
            }
        } else if action_str.contains("Airplane") {
            Some(UserAction::ToggleAirplaneMode)
        } else if action_str.contains("Diagnostic") {
            Some(UserAction::RunDiagnostic(action_str.to_string()))
        } else {
            Some(UserAction::CustomAction(action_str.to_string()))
        }
    }

    /// Predict next likely action
    pub fn predict_next_action(
        &self,
        recent_actions: &[UserAction],
        context: &NetworkContext,
    ) -> PredictionResult<UserAction> {
        let mut action_scores: HashMap<UserAction, f32> = HashMap::new();

        // Check workflow patterns
        for workflow in &self.frequent_workflows {
            if workflow.len() > recent_actions.len() {
                // Check if recent actions match the beginning of this workflow
                if workflow.starts_with(recent_actions) {
                    // Predict the next action in the workflow
                    let next_action = &workflow[recent_actions.len()];
                    *action_scores.entry(next_action.clone()).or_insert(0.0) += 1.0;
                }
            }
        }

        // Check context associations
        if let Some(context_actions) = self.context_associations.get(&context.location_hash) {
            for action in context_actions {
                *action_scores.entry(action.clone()).or_insert(0.0) += 0.5;
            }
        }

        // Add time-based predictions
        for (action, stats) in &self.action_stats {
            let time_weight = stats.hourly_distribution[context.time_of_day as usize] as f32
                / stats.total_count.max(1) as f32;
            if time_weight > 0.1 {
                *action_scores.entry(action.clone()).or_insert(0.0) += time_weight;
            }
        }

        // Find best prediction
        let mut sorted_actions: Vec<(UserAction, f32)> = action_scores.into_iter().collect();
        sorted_actions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if let Some((action, score)) = sorted_actions.first() {
            let alternatives: Vec<(UserAction, f32)> = sorted_actions.iter()
                .skip(1)
                .take(2)
                .map(|(a, s)| (a.clone(), *s))
                .collect();

            PredictionResult::new(action.clone(), score / 2.0)  // Normalize confidence
                .with_alternatives(alternatives)
        } else {
            // No prediction available
            PredictionResult::new(UserAction::CustomAction("".to_string()), 0.0)
        }
    }

    /// Suggest workflow automation
    pub fn suggest_automation(&self) -> Vec<Vec<UserAction>> {
        self.frequent_workflows.clone()
    }

    /// Get usage statistics for reporting
    pub fn get_usage_statistics(&self) -> HashMap<String, ActionStats> {
        self.action_stats.iter()
            .map(|(action, stats)| {
                let action_str = format!("{:?}", action);
                (action_str, stats.clone())
            })
            .collect()
    }
}

impl ModelPersistence for UsagePatternLearner {
    fn save(&self, path: &str) -> Result<(), MlError> {
        let model_path = Path::new(path);
        if let Some(parent) = model_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        fs::write(model_path, serialized)?;
        debug!("Usage pattern learner saved to {}", path);

        Ok(())
    }

    fn load(path: &str) -> Result<Self, MlError> {
        let model_path = Path::new(path);
        if !model_path.exists() {
            return Err(MlError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Model file not found",
            )));
        }

        let contents = fs::read_to_string(model_path)?;
        let learner: Self = serde_json::from_str(&contents)
            .map_err(|e| MlError::SerializationError(e.to_string()))?;

        debug!("Usage pattern learner loaded from {}", path);

        Ok(learner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> NetworkContext {
        NetworkContext {
            time_of_day: 14,
            day_of_week: 2,
            location_hash: 12345,
            network_type: NetworkType::WiFi,
            signal_strength: Some(0.8),
        }
    }

    #[test]
    fn test_record_action() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        learner.record_action(UserAction::EnableTailscale, context.clone());
        learner.record_action(UserAction::SelectExitNode("us-node".to_string()), context);

        assert_eq!(learner.action_stats.len(), 2);
        assert!(learner.action_stats.contains_key(&UserAction::EnableTailscale));
        assert_eq!(learner.action_sequences.len(), 1);
    }

    #[test]
    fn test_workflow_detection() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        // Simulate a repeated workflow
        for _ in 0..3 {
            learner.record_action(UserAction::EnableTailscale, context.clone());
            learner.record_action(UserAction::SelectExitNode("node".to_string()), context.clone());
        }

        assert!(!learner.frequent_workflows.is_empty());
    }

    #[test]
    fn test_personalized_menu_order() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        // Record some actions with different frequencies
        for _ in 0..5 {
            learner.record_action(UserAction::EnableTailscale, context.clone());
        }
        for _ in 0..2 {
            learner.record_action(UserAction::ConnectWifi("network".to_string()), context.clone());
        }
        learner.record_action(UserAction::RunDiagnostic("ping".to_string()), context.clone());

        let menu_items = vec![
            "Run Diagnostic".to_string(),
            "Enable Tailscale".to_string(),
            "Connect WiFi".to_string(),
        ];

        let ordered = learner.get_personalized_menu_order(menu_items, &context);

        // Most frequently used should be first
        assert!(ordered[0].contains("Tailscale"));
    }

    #[test]
    fn test_predict_next_action() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        // Train with a pattern
        for _ in 0..5 {
            learner.record_action(UserAction::EnableTailscale, context.clone());
            learner.record_action(UserAction::SelectExitNode("node".to_string()), context.clone());
        }

        let recent = vec![UserAction::EnableTailscale];
        let prediction = learner.predict_next_action(&recent, &context);

        assert!(matches!(prediction.value, UserAction::SelectExitNode(_)));
    }

    #[test]
    fn test_action_stats_update() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        learner.record_action(UserAction::EnableTailscale, context.clone());

        let stats = learner.action_stats.get(&UserAction::EnableTailscale).unwrap();
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.recent_count, 1);
        assert!(stats.last_used.is_some());
        assert_eq!(stats.hourly_distribution[14], 1);  // Hour 14
        assert_eq!(stats.daily_distribution[2], 1);    // Wednesday
    }

    #[test]
    fn test_context_similarity() {
        let mut learner = UsagePatternLearner::new();
        let context1 = create_test_context();
        let mut context2 = context1.clone();
        context2.time_of_day = 15;  // Slightly different time

        learner.record_action(UserAction::EnableTailscale, context1.clone());

        let similarity = learner.calculate_context_similarity(
            &UserAction::EnableTailscale,
            &context2
        );

        assert!(similarity > 0.8);  // Should be similar
    }

    #[test]
    fn test_model_persistence() {
        let mut learner = UsagePatternLearner::new();
        let context = create_test_context();

        learner.record_action(UserAction::EnableTailscale, context);

        let temp_dir = tempfile::tempdir().unwrap();
        let model_path = temp_dir.path().join("usage_model.json");

        // Save model
        learner.save(model_path.to_str().unwrap()).unwrap();
        assert!(model_path.exists());

        // Load model
        let loaded = UsagePatternLearner::load(model_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.action_stats.len(), learner.action_stats.len());
    }
}
