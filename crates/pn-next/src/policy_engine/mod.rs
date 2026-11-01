//! Policy Engine Module
//! Declarative policy evaluation for service management decisions

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Policy engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEngineConfig {
    /// Enable policy engine
    pub enabled: bool,
    /// Policy directory
    pub policy_dir: std::path::PathBuf,
    /// Evaluation interval (seconds)
    pub eval_interval_secs: u64,
    /// Default policy action
    pub default_action: PolicyAction,
    /// Policy version
    pub version: String,
}

impl Default for PolicyEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            policy_dir: std::path::PathBuf::from("~/.config/pdn/policies").expand(),
            eval_interval_secs: 60,
            default_action: PolicyAction::Allow,
            version: "1.0".to_string(),
        }
    }
}

/// Trait for path expansion
trait ExpandPath {
    fn expand(self) -> std::path::PathBuf;
}

impl ExpandPath for std::path::PathBuf {
    fn expand(self) -> std::path::PathBuf {
        let expanded = shellexpand::tilde(&self.to_string_lossy()).into_owned();
        std::path::PathBuf::from(expanded)
    }
}

/// Policy actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    Warn,
    RequireApproval,
    Quarantine,
    Restart,
    Stop,
}

/// Policy effect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEffect {
    pub action: PolicyAction,
    pub reason: String,
    pub metadata: HashMap<String, String>,
}

/// Policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Rule priority (higher = evaluated first)
    pub priority: i32,
    /// Conditions (all must match)
    pub conditions: Vec<PolicyCondition>,
    /// Action if conditions match
    pub action: PolicyAction,
    /// Reason for action
    pub reason: String,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Whether rule is enabled
    pub enabled: bool,
    /// Rule tags
    pub tags: Vec<String>,
}

/// Policy conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyCondition {
    /// Service name matches pattern
    ServiceName { pattern: String },
    /// Service label matches
    Label { key: String, value: String },
    /// Resource usage exceeds threshold
    ResourceUsage {
        resource: ResourceType,
        operator: ComparisonOperator,
        threshold: f64,
    },
    /// Time of day matches
    TimeOfDay {
        start: String,
        end: String,
        timezone: String,
    },
    /// Day of week matches
    DayOfWeek { days: Vec<String> },
    /// Service health status
    HealthStatus { status: HealthStatus },
    /// Restart count exceeds
    RestartCount {
        operator: ComparisonOperator,
        threshold: u32,
    },
    /// Uptime exceeds
    Uptime {
        operator: ComparisonOperator,
        threshold_secs: u64,
    },
    /// Custom expression (CEL-like)
    Expression { expr: String },
}

/// Comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    Matches, // Regex
}

/// Resource types for policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    CpuPercent,
    MemoryBytes,
    MemoryPercent,
    DiskReadBps,
    DiskWriteBps,
    NetworkRxBps,
    NetworkTxBps,
    PidsCount,
    OpenFiles,
    RestartCount,
    UptimeSecs,
}

/// Health status for policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Starting,
    Stopping,
}

/// Policy evaluation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationContext {
    /// Service name
    pub service: String,
    /// Service labels
    pub labels: HashMap<String, String>,
    /// Current resource usage
    pub resources: HashMap<ResourceType, f64>,
    /// Health status
    pub health: HealthStatus,
    /// Restart count
    pub restart_count: u32,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Current timestamp
    pub timestamp: DateTime<Utc>,
    /// Custom attributes
    pub attributes: HashMap<String, String>,
}

/// Policy evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Matched rules
    pub matched_rules: Vec<String>,
    /// Final action
    pub action: PolicyAction,
    /// Combined reason
    pub reason: String,
    /// Individual rule effects
    pub effects: Vec<PolicyEffect>,
    /// Evaluation timestamp
    pub timestamp: DateTime<Utc>,
}

/// Policy engine
pub struct PolicyEngine {
    config: PolicyEngineConfig,
    rules: Arc<RwLock<Vec<PolicyRule>>>,
    rule_index: Arc<RwLock<HashMap<String, usize>>>, // name -> index
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(config: PolicyEngineConfig) -> Self {
        Self {
            config,
            rules: Arc::new(RwLock::new(Vec::new())),
            rule_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load policies from directory
    pub async fn load_policies(&self) -> Result<()> {
        let policy_dir = self.config.policy_dir.clone();
        if !policy_dir.exists() {
            info!("Policy directory does not exist: {}", policy_dir.display());
            return Ok(());
        }

        let mut rules = Vec::new();
        let mut index = HashMap::new();

        for entry in std::fs::read_dir(&policy_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path
                .extension()
                .map(|e| e == "yaml" || e == "yml" || e == "toml")
                .unwrap_or(false)
            {
                let content = std::fs::read_to_string(&path)?;

                let policy_rules: Vec<PolicyRule> =
                    if path.extension().map(|e| e == "toml").unwrap_or(false) {
                        toml::from_str(&content)?
                    } else {
                        serde_yaml::from_str(&content)?
                    };

                for (i, rule) in policy_rules.into_iter().enumerate() {
                    if rule.enabled {
                        index.insert(rule.name.clone(), rules.len());
                        rules.push(rule);
                    }
                }
            }
        }

        // Sort by priority (highest first)
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut rules_lock = self.rules.write().await;
        *rules_lock = rules;

        let mut index_lock = self.rule_index.write().await;
        *index_lock = index;

        info!("Loaded {} policy rules", rules_lock.len());
        Ok(())
    }

    /// Add a policy rule
    pub async fn add_rule(&self, rule: PolicyRule) -> Result<()> {
        let mut rules = self.rules.write().await;
        let mut index = self.rule_index.write().await;

        if index.contains_key(&rule.name) {
            return Err(anyhow::anyhow!("Rule already exists: {}", rule.name));
        }

        index.insert(rule.name.clone(), rules.len());
        rules.push(rule);

        // Re-sort by priority
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        // Rebuild index
        index.clear();
        for (i, rule) in rules.iter().enumerate() {
            index.insert(rule.name.clone(), i);
        }

        Ok(())
    }

    /// Remove a policy rule
    pub async fn remove_rule(&self, name: &str) -> Result<()> {
        let mut rules = self.rules.write().await;
        let mut index = self.rule_index.write().await;

        if let Some(pos) = index.remove(name) {
            rules.remove(pos);
            // Rebuild index
            index.clear();
            for (i, rule) in rules.iter().enumerate() {
                index.insert(rule.name.clone(), i);
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Rule not found: {}", name))
        }
    }

    /// Enable/disable a rule
    pub async fn set_rule_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut rules = self.rules.write().await;
        if let Some(rule) = rules.iter_mut().find(|r| r.name == name) {
            rule.enabled = enabled;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Rule not found: {}", name))
        }
    }

    /// Evaluate policies for a context
    pub async fn evaluate(&self, context: &EvaluationContext) -> EvaluationResult {
        if !self.config.enabled {
            return EvaluationResult {
                matched_rules: Vec::new(),
                action: self.config.default_action,
                reason: "Policy engine disabled".to_string(),
                effects: Vec::new(),
                timestamp: Utc::now(),
            };
        }

        let rules = self.rules.read().await;
        let mut matched_rules = Vec::new();
        let mut effects = Vec::new();
        let mut final_action = self.config.default_action;
        let mut final_reason = "Default policy".to_string();

        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }

            if self.match_conditions(&rule.conditions, context) {
                matched_rules.push(rule.name.clone());
                effects.push(PolicyEffect {
                    action: rule.action,
                    reason: rule.reason.clone(),
                    metadata: rule.metadata.clone(),
                });

                // Higher priority rules override (since sorted by priority desc)
                // But we continue to collect all matches for audit
                if matched_rules.len() == 1 {
                    final_action = rule.action;
                    final_reason = rule.reason.clone();
                }
            }
        }

        EvaluationResult {
            matched_rules,
            action: final_action,
            reason: final_reason,
            effects,
            timestamp: Utc::now(),
        }
    }

    /// Match conditions against context
    fn match_conditions(
        &self,
        conditions: &[PolicyCondition],
        context: &EvaluationContext,
    ) -> bool {
        conditions
            .iter()
            .all(|cond| self.match_condition(cond, context))
    }

    /// Match a single condition
    fn match_condition(&self, condition: &PolicyCondition, context: &EvaluationContext) -> bool {
        match condition {
            PolicyCondition::ServiceName { pattern } => {
                Self::match_pattern(&context.service, pattern)
            }
            PolicyCondition::Label { key, value } => context.labels.get(key) == Some(value),
            PolicyCondition::ResourceUsage {
                resource,
                operator,
                threshold,
            } => {
                if let Some(&current) = context.resources.get(resource) {
                    Self::compare(current, *operator, *threshold)
                } else {
                    false
                }
            }
            PolicyCondition::TimeOfDay {
                start,
                end,
                timezone,
            } => Self::match_time_of_day(start, end, timezone, context.timestamp),
            PolicyCondition::DayOfWeek { days } => {
                let weekday = context.timestamp.weekday().to_string().to_lowercase();
                days.iter().any(|d| d.to_lowercase() == weekday)
            }
            PolicyCondition::HealthStatus { status } => context.health == *status,
            PolicyCondition::RestartCount {
                operator,
                threshold,
            } => Self::compare(context.restart_count as f64, *operator, *threshold as f64),
            PolicyCondition::Uptime {
                operator,
                threshold_secs,
            } => Self::compare(
                context.uptime_secs as f64,
                *operator,
                *threshold_secs as f64,
            ),
            PolicyCondition::Expression { expr } => {
                // Simplified: would use CEL or similar in production
                self.eval_expression(expr, context)
            }
        }
    }

    /// Pattern matching (glob-style)
    fn match_pattern(text: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.contains('*') {
            // Simple glob matching
            let regex = pattern
                .replace(".", r"\.")
                .replace("*", ".*")
                .replace("?", ".");
            regex::Regex::new(&format!("^{}$", regex))
                .map(|re| re.is_match(text))
                .unwrap_or(false)
        } else {
            text == pattern
        }
    }

    /// Compare values
    fn compare(left: f64, op: ComparisonOperator, right: f64) -> bool {
        match op {
            ComparisonOperator::Eq => (left - right).abs() < f64::EPSILON,
            ComparisonOperator::Ne => (left - right).abs() >= f64::EPSILON,
            ComparisonOperator::Gt => left > right,
            ComparisonOperator::Gte => left >= right,
            ComparisonOperator::Lt => left < right,
            ComparisonOperator::Lte => left <= right,
            ComparisonOperator::Contains => false, // Not applicable for numbers
            ComparisonOperator::Matches => false,  // Not applicable for numbers
        }
    }

    /// Time of day matching
    fn match_time_of_day(
        start: &str,
        end: &str,
        _timezone: &str,
        timestamp: DateTime<Utc>,
    ) -> bool {
        // Parse HH:MM format
        let parse_time = |s: &str| -> Option<(u32, u32)> {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() == 2 {
                parts[0].parse().ok().zip(parts[1].parse().ok())
            } else {
                None
            }
        };

        let (start_h, start_m) = match parse_time(start) {
            Some(t) => t,
            None => return false,
        };
        let (end_h, end_m) = match parse_time(end) {
            Some(t) => t,
            None => return false,
        };

        let current_h = timestamp.hour();
        let current_m = timestamp.minute();

        let start_min = start_h * 60 + start_m;
        let end_min = end_h * 60 + end_m;
        let current_min = current_h * 60 + current_m;

        if start_min <= end_min {
            current_min >= start_min && current_min <= end_min
        } else {
            // Overnight range
            current_min >= start_min || current_min <= end_min
        }
    }

    /// Evaluate expression (simplified)
    fn eval_expression(&self, _expr: &str, _context: &EvaluationContext) -> bool {
        // In production, use CEL (Common Expression Language)
        // For now, return false
        false
    }

    /// Get all rules
    pub async fn get_rules(&self) -> Vec<PolicyRule> {
        self.rules.read().await.clone()
    }

    /// Get rule by name
    pub async fn get_rule(&self, name: &str) -> Option<PolicyRule> {
        let rules = self.rules.read().await;
        rules.iter().find(|r| r.name == name).cloned()
    }

    /// List rules by tag
    pub async fn list_rules_by_tag(&self, tag: &str) -> Vec<PolicyRule> {
        let rules = self.rules.read().await;
        rules
            .iter()
            .filter(|r| r.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// Start policy evaluation loop
    pub async fn start_evaluation_loop(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let rules = self.rules.clone();
        let interval_secs = self.config.eval_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;
                // Periodic policy reload or evaluation would go here
                debug!("Policy evaluation tick");
            }
        });

        Ok(())
    }

    /// Export policies to file
    pub async fn export_policies(&self, path: &std::path::Path) -> Result<()> {
        let rules = self.rules.read().await;
        let content = serde_yaml::to_string(&*rules)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }
}

/// Built-in policy presets
pub mod presets {
    use super::*;

    /// Production safety policies
    pub fn production_safety() -> Vec<PolicyRule> {
        vec![
            PolicyRule {
                name: "deny-production-restart-loop".to_string(),
                description: "Prevent restart loops in production".to_string(),
                priority: 100,
                conditions: vec![
                    PolicyCondition::Label {
                        key: "environment".to_string(),
                        value: "production".to_string(),
                    },
                    PolicyCondition::RestartCount {
                        operator: ComparisonOperator::Gte,
                        threshold: 5,
                    },
                ],
                action: PolicyAction::RequireApproval,
                reason: "Production service restarting too frequently".to_string(),
                metadata: HashMap::new(),
                enabled: true,
                tags: vec!["production".to_string(), "safety".to_string()],
            },
            PolicyRule {
                name: "quarantine-oom-services".to_string(),
                description: "Quarantine services that exceed memory limits".to_string(),
                priority: 90,
                conditions: vec![PolicyCondition::ResourceUsage {
                    resource: ResourceType::MemoryPercent,
                    operator: ComparisonOperator::Gte,
                    threshold: 95.0,
                }],
                action: PolicyAction::Quarantine,
                reason: "Service exceeded 95% memory limit".to_string(),
                metadata: HashMap::new(),
                enabled: true,
                tags: vec!["resource".to_string(), "safety".to_string()],
            },
            PolicyRule {
                name: "warn-high-cpu".to_string(),
                description: "Warn on sustained high CPU usage".to_string(),
                priority: 50,
                conditions: vec![
                    PolicyCondition::ResourceUsage {
                        resource: ResourceType::CpuPercent,
                        operator: ComparisonOperator::Gte,
                        threshold: 80.0,
                    },
                    PolicyCondition::Uptime {
                        operator: ComparisonOperator::Gte,
                        threshold_secs: 300,
                    },
                ],
                action: PolicyAction::Warn,
                reason: "Service CPU usage above 80% for 5+ minutes".to_string(),
                metadata: HashMap::new(),
                enabled: true,
                tags: vec!["resource".to_string(), "monitoring".to_string()],
            },
        ]
    }

    /// Development policies
    pub fn development() -> Vec<PolicyRule> {
        vec![PolicyRule {
            name: "allow-dev-restarts".to_string(),
            description: "Allow unlimited restarts in development".to_string(),
            priority: 10,
            conditions: vec![PolicyCondition::Label {
                key: "environment".to_string(),
                value: "development".to_string(),
            }],
            action: PolicyAction::Allow,
            reason: "Development environment allows restarts".to_string(),
            metadata: HashMap::new(),
            enabled: true,
            tags: vec!["development".to_string()],
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_policy_engine_creation() {
        let config = PolicyEngineConfig::default();
        let engine = PolicyEngine::new(config);

        let context = EvaluationContext {
            service: "test".to_string(),
            labels: HashMap::new(),
            resources: HashMap::new(),
            health: HealthStatus::Healthy,
            restart_count: 0,
            uptime_secs: 0,
            timestamp: Utc::now(),
            attributes: HashMap::new(),
        };

        let result = engine.evaluate(&context).await;
        assert_eq!(result.action, PolicyAction::Allow);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_policy_rule_matching() {
        let config = PolicyEngineConfig::default();
        let engine = PolicyEngine::new(config);

        let rule = PolicyRule {
            name: "test-rule".to_string(),
            description: "Test".to_string(),
            priority: 100,
            conditions: vec![PolicyCondition::ServiceName {
                pattern: "web-*".to_string(),
            }],
            action: PolicyAction::Deny,
            reason: "Test deny".to_string(),
            metadata: HashMap::new(),
            enabled: true,
            tags: vec![],
        };

        engine.add_rule(rule).await.unwrap();

        let context = EvaluationContext {
            service: "web-server".to_string(),
            labels: HashMap::new(),
            resources: HashMap::new(),
            health: HealthStatus::Healthy,
            restart_count: 0,
            uptime_secs: 0,
            timestamp: Utc::now(),
            attributes: HashMap::new(),
        };

        let result = engine.evaluate(&context).await;
        assert_eq!(result.action, PolicyAction::Deny);
        assert!(result.matched_rules.contains(&"test-rule".to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_resource_condition() {
        let config = PolicyEngineConfig::default();
        let engine = PolicyEngine::new(config);

        let rule = PolicyRule {
            name: "high-memory".to_string(),
            description: "High memory".to_string(),
            priority: 100,
            conditions: vec![PolicyCondition::ResourceUsage {
                resource: ResourceType::MemoryPercent,
                operator: ComparisonOperator::Gte,
                threshold: 90.0,
            }],
            action: PolicyAction::Warn,
            reason: "High memory".to_string(),
            metadata: HashMap::new(),
            enabled: true,
            tags: vec![],
        };

        engine.add_rule(rule).await.unwrap();

        let mut resources = HashMap::new();
        resources.insert(ResourceType::MemoryPercent, 95.0);

        let context = EvaluationContext {
            service: "test".to_string(),
            labels: HashMap::new(),
            resources,
            health: HealthStatus::Healthy,
            restart_count: 0,
            uptime_secs: 0,
            timestamp: Utc::now(),
            attributes: HashMap::new(),
        };

        let result = engine.evaluate(&context).await;
        assert_eq!(result.action, PolicyAction::Warn);
    }
}
