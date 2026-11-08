//! Guardrail manager for PN Gateway

use super::config::{GuardrailAction, GuardrailConfig, GuardrailRule, Severity};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Guardrail manager for enforcing safety rules
pub struct GuardrailManager {
    config: GuardrailConfig,
    rule_evaluators: HashMap<String, Box<dyn RuleEvaluator + Send + Sync>>,
    violation_log: Arc<tokio::sync::Mutex<Vec<ViolationRecord>>>,
}

impl GuardrailManager {
    pub fn new(config: GuardrailConfig) -> Self {
        let mut evaluators: HashMap<String, Box<dyn RuleEvaluator + Send + Sync>> = HashMap::new();

        // Register default evaluators
        evaluators.insert(
            "container_count".to_string(),
            Box::new(ContainerCountEvaluator),
        );
        evaluators.insert("memory_usage".to_string(), Box::new(MemoryUsageEvaluator));
        evaluators.insert("cpu_usage".to_string(), Box::new(CpuUsageEvaluator));
        evaluators.insert("disk_usage".to_string(), Box::new(DiskUsageEvaluator));
        evaluators.insert(
            "privileged_access".to_string(),
            Box::new(PrivilegedAccessEvaluator),
        );
        evaluators.insert("network_usage".to_string(), Box::new(NetworkUsageEvaluator));
        evaluators.insert(
            "container_lifetime".to_string(),
            Box::new(ContainerLifetimeEvaluator),
        );

        Self {
            config,
            rule_evaluators: evaluators,
            violation_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn check_all(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        for rule in &self.config.rules {
            if let Some(evaluator) = self.rule_evaluators.get(&rule.name) {
                let result = evaluator.evaluate(&rule.condition)?;

                if result.triggered {
                    self.record_violation(&rule, &result)?;

                    match rule.action {
                        GuardrailAction::Deny => {
                            return Err(anyhow::anyhow!(
                                "Guardrail '{}' denied: {}",
                                rule.name,
                                result.message
                            ));
                        }
                        GuardrailAction::Throttle => {
                            // Nothing in this engine throttles a container: there is no enforcement
                            // point here. Measured 2026-11-08, this arm logged
                            // "Throttling due to guardrail", which reads as an applied limit. The
                            // message now says what happened (a violation was recorded) and what did
                            // not (no throttle was applied by this engine).
                            warn!(
                                "Guardrail '{}' triggered (throttle requested, NOT applied by this \
                                 engine): {}",
                                rule.name, result.message
                            );
                        }
                        GuardrailAction::Warn => {
                            warn!("Guardrail '{}' warning: {}", rule.name, result.message);
                        }
                        GuardrailAction::Quarantine => {
                            warn!(
                                "Guardrail '{}' triggered (quarantine requested, NOT applied by this \
                                 engine): {}",
                                rule.name, result.message
                            );
                        }
                        GuardrailAction::Alert => {
                            warn!("Guardrail '{}' alert: {}", rule.name, result.message);
                        }
                        GuardrailAction::Allow => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Condition evaluation is not implemented here, so this refuses instead of answering `false`.
    ///
    /// It used to return `Ok(false)` unconditionally, which reads as "the condition does not hold" -
    /// a claim about the condition, made without evaluating it. Nothing in the tree called it
    /// (measured 2026-11-08: the only `evaluate` call sites are the registered evaluators in
    /// `check_all` and the separate `pn-next` policy engine), so failing closed costs nothing and
    /// stops a future caller from trusting an answer that was never computed.
    pub async fn check_condition(
        &self,
        condition: &str,
        _context: &HashMap<String, serde_json::Value>,
    ) -> Result<bool> {
        anyhow::bail!(
            "no condition language is implemented for '{}'; this engine evaluates rules through the \
             evaluators registered by rule name in check_all, and a silent `false` would be a claim \
             it never made",
            condition
        )
    }

    fn record_violation(&self, rule: &GuardrailRule, result: &EvaluationResult) -> Result<()> {
        let record = ViolationRecord {
            rule_name: rule.name.clone(),
            rule_severity: rule.severity.clone(),
            message: result.message.clone(),
            timestamp: chrono::Utc::now(),
            action_taken: rule.action.clone(),
        };

        // Log based on severity
        match rule.severity {
            Severity::Critical => {
                error!(
                    "GUARDRAIL VIOLATION [CRITICAL]: {} - {}",
                    rule.name, result.message
                );
            }
            Severity::Warning => {
                warn!(
                    "GUARDRAIL VIOLATION [WARNING]: {} - {}",
                    rule.name, result.message
                );
            }
            Severity::Info => {
                info!(
                    "GUARDRAIL VIOLATION [INFO]: {} - {}",
                    rule.name, result.message
                );
            }
        }

        // Log to violation log
        tokio::spawn({
            let log = self.violation_log.clone();
            let record = record.clone();
            async move {
                let mut log = log.lock().await;
                log.push(record);
                // Keep only last 10000 entries
                if log.len() > 10000 {
                    let drain_start = log.len() - 10000;
                    log.drain(0..drain_start);
                }
            }
        });

        Ok(())
    }

    pub async fn get_violations(&self, limit: Option<usize>) -> Vec<ViolationRecord> {
        let log = self.violation_log.lock().await;
        let limit = limit.unwrap_or(100);
        log.iter().rev().take(limit).cloned().collect()
    }

    pub async fn clear_violations(&self) {
        let mut log = self.violation_log.lock().await;
        log.clear();
    }
}

#[derive(Debug, Clone)]
struct EvaluationResult {
    triggered: bool,
    message: String,
    value: Option<serde_json::Value>,
}

trait RuleEvaluator: Send + Sync {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult>;
}

struct ContainerCountEvaluator;

impl RuleEvaluator for ContainerCountEvaluator {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult> {
        // Parse condition like "container_count > 100"
        let parts: Vec<&str> = condition.split_whitespace().collect();
        if parts.len() >= 3 {
            let metric = parts[0];
            let op = parts[1];
            let threshold: usize = parts[2].parse().unwrap_or(0);

            if metric == "container_count" {
                let current = get_container_count()?;
                let triggered = match op {
                    ">" => current > threshold,
                    ">=" => current >= threshold,
                    "<" => current < threshold,
                    "<=" => current <= threshold,
                    "==" => current == threshold,
                    "!=" => current != threshold,
                    _ => false,
                };

                return Ok(EvaluationResult {
                    triggered,
                    message: format!(
                        "Container count {} {} (threshold: {})",
                        current, op, threshold
                    ),
                    value: Some(serde_json::json!(current)),
                });
            }
        }
        Ok(EvaluationResult {
            triggered: false,
            message: "Unknown condition".to_string(),
            value: None,
        })
    }
}

struct MemoryUsageEvaluator;

impl RuleEvaluator for MemoryUsageEvaluator {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult> {
        let parts: Vec<&str> = condition.split_whitespace().collect();
        if parts.len() >= 3 {
            let metric = parts[0];
            let op = parts[1];
            let threshold_str = parts[2].trim_end_matches('%');
            let threshold: f64 = threshold_str.parse().unwrap_or(0.0);

            if metric == "memory_usage" {
                let current = get_memory_usage_percent()?;
                let triggered = match op {
                    ">" => current > threshold,
                    ">=" => current >= threshold,
                    "<" => current < threshold,
                    "<=" => current <= threshold,
                    "==" => (current - threshold).abs() < f64::EPSILON,
                    "!=" => (current - threshold).abs() >= f64::EPSILON,
                    _ => false,
                };

                return Ok(EvaluationResult {
                    triggered,
                    message: format!(
                        "Memory usage {:.1}% {} {}% (threshold: {}%)",
                        current, op, threshold, threshold
                    ),
                    value: Some(serde_json::json!(current)),
                });
            }
        }
        Ok(EvaluationResult {
            triggered: false,
            message: "Unknown condition".to_string(),
            value: None,
        })
    }
}

struct CpuUsageEvaluator;

impl RuleEvaluator for CpuUsageEvaluator {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult> {
        let parts: Vec<&str> = condition.split_whitespace().collect();
        if parts.len() >= 3 {
            let metric = parts[0];
            let op = parts[1];
            let threshold_str = parts[2].trim_end_matches('%');
            let threshold: f64 = threshold_str.parse().unwrap_or(0.0);

            if metric == "cpu_usage" {
                let current = get_cpu_usage_percent()?;
                let triggered = match op {
                    ">" => current > threshold,
                    ">=" => current >= threshold,
                    "<" => current < threshold,
                    "<=" => current <= threshold,
                    "==" => (current - threshold).abs() < f64::EPSILON,
                    "!=" => (current - threshold).abs() >= f64::EPSILON,
                    _ => false,
                };

                return Ok(EvaluationResult {
                    triggered,
                    message: format!(
                        "CPU usage {:.1}% {} {}% (threshold: {}%)",
                        current, op, threshold, threshold
                    ),
                    value: Some(serde_json::json!(current)),
                });
            }
        }
        Ok(EvaluationResult {
            triggered: false,
            message: "Unknown condition".to_string(),
            value: None,
        })
    }
}

struct DiskUsageEvaluator;

impl RuleEvaluator for DiskUsageEvaluator {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult> {
        let parts: Vec<&str> = condition.split_whitespace().collect();
        if parts.len() >= 3 {
            let metric = parts[0];
            let op = parts[1];
            let threshold_str = parts[2].trim_end_matches('%');
            let threshold: f64 = threshold_str.parse().unwrap_or(0.0);

            if metric == "disk_usage" {
                let current = get_disk_usage_percent("/")?;
                let triggered = match op {
                    ">" => current > threshold,
                    ">=" => current >= threshold,
                    "<" => current < threshold,
                    "<=" => current <= threshold,
                    "==" => (current - threshold).abs() < f64::EPSILON,
                    "!=" => (current - threshold).abs() >= f64::EPSILON,
                    _ => false,
                };

                return Ok(EvaluationResult {
                    triggered,
                    message: format!(
                        "Disk usage {:.1}% {} {}% (threshold: {}%)",
                        current, op, threshold, threshold
                    ),
                    value: Some(serde_json::json!(current)),
                });
            }
        }
        Ok(EvaluationResult {
            triggered: false,
            message: "Unknown condition".to_string(),
            value: None,
        })
    }
}

struct PrivilegedAccessEvaluator;

impl RuleEvaluator for PrivilegedAccessEvaluator {
    fn evaluate(&self, condition: &str) -> Result<EvaluationResult> {
        if condition == "privileged == true" {
            let has_privileged = check_privileged_containers()?;
            return Ok(EvaluationResult {
                triggered: has_privileged,
                message: if has_privileged {
                    "Privileged container detected".to_string()
                } else {
                    "No privileged containers".to_string()
                },
                value: Some(serde_json::json!(has_privileged)),
            });
        }
        Ok(EvaluationResult {
            triggered: false,
            message: "Unknown condition".to_string(),
            value: None,
        })
    }
}

struct NetworkUsageEvaluator;

impl RuleEvaluator for NetworkUsageEvaluator {
    fn evaluate(&self, _condition: &str) -> Result<EvaluationResult> {
        // Simplified implementation
        Ok(EvaluationResult {
            triggered: false,
            message: "Network usage check not implemented".to_string(),
            value: None,
        })
    }
}

struct ContainerLifetimeEvaluator;

impl RuleEvaluator for ContainerLifetimeEvaluator {
    fn evaluate(&self, _condition: &str) -> Result<EvaluationResult> {
        // Simplified implementation
        Ok(EvaluationResult {
            triggered: false,
            message: "Container lifetime check not implemented".to_string(),
            value: None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ViolationRecord {
    pub rule_name: String,
    pub rule_severity: Severity,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action_taken: GuardrailAction,
}

// Mock functions - in production these would query actual system metrics
fn get_container_count() -> Result<usize> {
    Ok(0) // Mock
}

fn get_memory_usage_percent() -> Result<f64> {
    Ok(0.0) // Mock
}

fn get_cpu_usage_percent() -> Result<f64> {
    Ok(0.0) // Mock
}

fn get_disk_usage_percent(_path: &str) -> Result<f64> {
    Ok(0.0) // Mock
}

fn check_privileged_containers() -> Result<bool> {
    Ok(false) // Mock
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GuardrailConfig, GuardrailRule};

    fn rule(action: GuardrailAction) -> GuardrailRule {
        GuardrailRule {
            // The evaluator is looked up BY RULE NAME, so the name has to be the metric it evaluates.
            name: "container_count".to_string(),
            description: "test rule".to_string(),
            // `container_count >= 0` is always true, so the evaluator really triggers and the arm
            // under test runs. A condition that can never fire would make every arm look correct.
            // Note the metric itself is a mock (`get_container_count() -> Ok(0)`), which is why the
            // honest statement about this engine is that it cannot judge anything real yet.
            condition: "container_count >= 0".to_string(),
            action,
            severity: Severity::Warning,
        }
    }

    fn manager_for(action: GuardrailAction) -> GuardrailManager {
        GuardrailManager::new(GuardrailConfig {
            enabled: true,
            rules: vec![rule(action)],
            strict_mode: false,
            audit_log: false,
        })
    }

    /// `Deny` must still stop the check, and no other action may fail it. The engine has no
    /// enforcement point, so its arms used to log "Throttling due to guardrail" for a throttle
    /// nothing applied (ASEC2-ARCH-006/007/009, measured 2026-11-08).
    #[tokio::test(flavor = "multi_thread")]
    async fn deny_stops_and_the_other_actions_do_not_fail_the_check() {
        let error = manager_for(GuardrailAction::Deny)
            .check_all()
            .expect_err("a triggered Deny must stop the check");
        assert!(
            format!("{error}").contains("denied"),
            "the refusal must name the rule: {error}"
        );

        for action in [
            GuardrailAction::Throttle,
            GuardrailAction::Quarantine,
            GuardrailAction::Alert,
            GuardrailAction::Warn,
            GuardrailAction::Allow,
        ] {
            manager_for(action.clone())
                .check_all()
                .unwrap_or_else(|error| panic!("{action:?} must not fail the check: {error}"));
        }
    }

    /// An unimplemented condition must refuse instead of answering `false`, which reads as "the
    /// condition does not hold" - a claim about a condition the engine never evaluated.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_unimplemented_condition_refuses() {
        let error = GuardrailManager::new(GuardrailConfig::default())
            .check_condition("container_count >= 0", &HashMap::new())
            .await
            .expect_err("no condition language is implemented");
        assert!(
            format!("{error}").contains("no condition language"),
            "the refusal must name the reason: {error}"
        );
    }
}
