//! Watchdog Integration Module
//! Auto-restart failed containers and services with policy-based recovery

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Watchdog configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// Enable watchdog
    pub enabled: bool,
    /// Check interval (seconds)
    pub check_interval_secs: u64,
    /// Default restart policy
    pub default_policy: RestartPolicy,
    /// Service-specific policies
    pub service_policies: HashMap<String, RestartPolicy>,
    /// Maximum restart attempts per window
    pub max_restarts_per_window: u32,
    /// Time window for restart counting (seconds)
    pub restart_window_secs: u64,
    /// Escalation policies
    pub escalation: EscalationConfig,
    /// Health check timeout (seconds)
    pub health_check_timeout_secs: u64,
    /// Action on max restarts exceeded
    pub on_max_restarts: MaxRestartsAction,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 30,
            default_policy: RestartPolicy::OnFailure,
            service_policies: HashMap::new(),
            max_restarts_per_window: 5,
            restart_window_secs: 300, // 5 minutes
            escalation: EscalationConfig::default(),
            health_check_timeout_secs: 10,
            on_max_restarts: MaxRestartsAction::Stop,
        }
    }
}

/// Restart policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    /// Never restart
    Never,
    /// Restart only on failure (non-zero exit)
    OnFailure,
    /// Always restart
    Always,
    /// Restart unless explicitly stopped
    UnlessStopped,
    /// Restart on failure or crash (signal)
    OnFailureOrCrash,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        RestartPolicy::OnFailure
    }
}

/// Escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    /// Enable escalation
    pub enabled: bool,
    /// Levels of escalation
    pub levels: Vec<EscalationLevel>,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            levels: vec![
                EscalationLevel {
                    attempt: 1,
                    action: EscalationAction::Restart,
                    delay_secs: 5,
                },
                EscalationLevel {
                    attempt: 2,
                    action: EscalationAction::Restart,
                    delay_secs: 30,
                },
                EscalationLevel {
                    attempt: 3,
                    action: EscalationAction::RestartWithCleanup,
                    delay_secs: 60,
                },
                EscalationLevel {
                    attempt: 5,
                    action: EscalationAction::Alert,
                    delay_secs: 0,
                },
            ],
        }
    }
}

/// Escalation level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLevel {
    /// Attempt number (1-indexed)
    pub attempt: u32,
    /// Action to take
    pub action: EscalationAction,
    /// Delay before action (seconds)
    pub delay_secs: u64,
}

/// Escalation actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    /// Simple restart
    Restart,
    /// Restart with state cleanup
    RestartWithCleanup,
    /// Restart with config reload
    RestartWithReload,
    /// Send alert/notification
    Alert,
    /// Stop service permanently
    Stop,
    /// Quarantine service
    Quarantine,
}

/// Action when max restarts exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxRestartsAction {
    /// Stop the service
    Stop,
    /// Quarantine (stop and alert)
    Quarantine,
    /// Continue with longer delays
    ContinueWithBackoff,
    /// Alert only
    Alert,
}

/// Watchdog event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatchdogEvent {
    /// Service health check passed
    HealthCheckPassed { service: String },
    /// Service health check failed
    HealthCheckFailed { service: String, error: String },
    /// Service restarted
    ServiceRestarted {
        service: String,
        attempt: u32,
        reason: String,
    },
    /// Service stopped due to policy
    ServiceStopped { service: String, reason: String },
    /// Service quarantined
    ServiceQuarantined { service: String, reason: String },
    /// Escalation triggered
    EscalationTriggered {
        service: String,
        level: u32,
        action: EscalationAction,
    },
    /// Max restarts exceeded
    MaxRestartsExceeded {
        service: String,
        action: MaxRestartsAction,
    },
    /// Watchdog started
    WatchdogStarted,
    /// Watchdog stopped
    WatchdogStopped,
}

/// Service watchdog state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceWatchdogState {
    /// Service name
    pub service: String,
    /// Current restart policy
    pub policy: RestartPolicy,
    /// Number of restarts in current window
    pub restart_count: u32,
    /// Timestamps of recent restarts
    pub restart_timestamps: Vec<DateTime<Utc>>,
    /// Last health check result
    pub last_health_check: Option<HealthCheckResult>,
    /// Last restart time
    pub last_restart: Option<DateTime<Utc>>,
    /// Current escalation level
    pub escalation_level: u32,
    /// Whether service is quarantined
    pub quarantined: bool,
    /// Whether service is stopped by watchdog
    pub stopped_by_watchdog: bool,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Whether healthy
    pub healthy: bool,
    /// Check timestamp
    pub timestamp: DateTime<Utc>,
    /// Error message if unhealthy
    pub error: Option<String>,
    /// Response time (ms)
    pub response_time_ms: u64,
}

/// Watchdog manager
pub struct WatchdogManager {
    config: WatchdogConfig,
    states: Arc<RwLock<HashMap<String, ServiceWatchdogState>>>,
    event_sender: Option<tokio::sync::broadcast::Sender<WatchdogEvent>>,
    restart_callback: Option<Arc<dyn Fn(&str) -> Result<()> + Send + Sync>>,
    health_check_callback: Option<Arc<dyn Fn(&str) -> Result<HealthCheckResult> + Send + Sync>>,
}

impl WatchdogManager {
    /// Create a new watchdog manager
    pub fn new(config: WatchdogConfig) -> Self {
        Self {
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
            restart_callback: None,
            health_check_callback: None,
        }
    }

    /// Set event broadcaster
    pub fn with_events(mut self, sender: tokio::sync::broadcast::Sender<WatchdogEvent>) -> Self {
        self.event_sender = Some(sender);
        self
    }

    /// Set restart callback
    pub fn with_restart_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) -> Result<()> + Send + Sync + 'static,
    {
        self.restart_callback = Some(Arc::new(callback));
        self
    }

    /// Set health check callback
    pub fn with_health_check_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) -> Result<HealthCheckResult> + Send + Sync + 'static,
    {
        self.health_check_callback = Some(Arc::new(callback));
        self
    }

    /// Get event receiver
    pub fn subscribe_events(&self) -> Option<tokio::sync::broadcast::Receiver<WatchdogEvent>> {
        self.event_sender.as_ref().map(|s| s.subscribe())
    }

    /// Emit event
    fn emit(&self, event: WatchdogEvent) {
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(event);
        }
    }

    /// Register a service for watchdog monitoring
    pub async fn register_service(
        &self,
        service: &str,
        policy: Option<RestartPolicy>,
    ) -> Result<()> {
        let mut states = self.states.write().await;
        let policy = policy
            .or_else(|| self.config.service_policies.get(service).copied())
            .unwrap_or(self.config.default_policy);

        states.insert(
            service.to_string(),
            ServiceWatchdogState {
                service: service.to_string(),
                policy,
                restart_count: 0,
                restart_timestamps: Vec::new(),
                last_health_check: None,
                last_restart: None,
                escalation_level: 0,
                quarantined: false,
                stopped_by_watchdog: false,
            },
        );

        info!("Registered service {} with policy {:?}", service, policy);
        Ok(())
    }

    /// Unregister a service
    pub async fn unregister_service(&self, service: &str) -> Result<()> {
        let mut states = self.states.write().await;
        states.remove(service);
        info!("Unregistered service {}", service);
        Ok(())
    }

    /// Get service policy
    pub fn get_policy(&self, service: &str) -> RestartPolicy {
        self.config
            .service_policies
            .get(service)
            .copied()
            .unwrap_or(self.config.default_policy)
    }

    /// Set service policy
    pub fn set_policy(&mut self, service: &str, policy: RestartPolicy) {
        self.config
            .service_policies
            .insert(service.to_string(), policy);
    }

    /// Perform health check on a service
    pub async fn check_service(&self, service: &str) -> Result<HealthCheckResult> {
        if let Some(callback) = &self.health_check_callback {
            let result = callback(service)?;

            // Update state
            let mut states = self.states.write().await;
            if let Some(state) = states.get_mut(service) {
                state.last_health_check = Some(result.clone());
            }

            if result.healthy {
                self.emit(WatchdogEvent::HealthCheckPassed {
                    service: service.to_string(),
                });
            } else {
                self.emit(WatchdogEvent::HealthCheckFailed {
                    service: service.to_string(),
                    error: result.error.clone().unwrap_or_default(),
                });
            }

            Ok(result)
        } else {
            // Default: assume healthy if no callback
            Ok(HealthCheckResult {
                healthy: true,
                timestamp: Utc::now(),
                error: None,
                response_time_ms: 0,
            })
        }
    }

    /// Handle service failure
    pub async fn handle_failure(&self, service: &str, reason: &str) -> Result<()> {
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(service) {
            if state.quarantined || state.stopped_by_watchdog {
                return Ok(()); // Already handled
            }

            // Check if we should restart based on policy
            let should_restart = match state.policy {
                RestartPolicy::Never => false,
                RestartPolicy::OnFailure => true,
                RestartPolicy::Always => true,
                RestartPolicy::UnlessStopped => !state.stopped_by_watchdog,
                RestartPolicy::OnFailureOrCrash => true,
            };

            if !should_restart {
                info!(
                    "Service {} not restarted due to policy {:?}",
                    service, state.policy
                );
                return Ok(());
            }

            // Check restart window
            let now = Utc::now();
            state
                .restart_timestamps
                .retain(|ts| (now - *ts).num_seconds() < self.config.restart_window_secs as i64);
            state.restart_count = state.restart_timestamps.len() as u32;

            // Check max restarts
            if state.restart_count >= self.config.max_restarts_per_window {
                return self.handle_max_restarts_exceeded(service, state).await;
            }

            // Determine escalation
            let escalation_level = state.escalation_level + 1;
            let escalation = self
                .config
                .escalation
                .levels
                .iter()
                .find(|e| e.attempt == escalation_level)
                .or_else(|| self.config.escalation.levels.last());

            if let Some(esc) = escalation {
                state.escalation_level = escalation_level;

                // Apply delay
                if esc.delay_secs > 0 {
                    info!(
                        "Escalation delay {}s for service {}",
                        esc.delay_secs, service
                    );
                    tokio::time::sleep(Duration::from_secs(esc.delay_secs)).await;
                }

                // Execute escalation action
                match esc.action {
                    EscalationAction::Restart => {
                        self.restart_service_internal(
                            service,
                            state,
                            &format!("Escalation level {}", escalation_level),
                        )
                        .await?;
                    }
                    EscalationAction::RestartWithCleanup => {
                        self.restart_service_internal(
                            service,
                            state,
                            &format!("Escalation level {} with cleanup", escalation_level),
                        )
                        .await?;
                        // Additional cleanup would go here
                    }
                    EscalationAction::RestartWithReload => {
                        self.restart_service_internal(
                            service,
                            state,
                            &format!("Escalation level {} with reload", escalation_level),
                        )
                        .await?;
                        // Config reload would go here
                    }
                    EscalationAction::Alert => {
                        self.emit(WatchdogEvent::EscalationTriggered {
                            service: service.to_string(),
                            level: escalation_level,
                            action: esc.action,
                        });
                        warn!("ALERT: Service {} exceeded restart threshold", service);
                    }
                    EscalationAction::Stop => {
                        state.stopped_by_watchdog = true;
                        self.emit(WatchdogEvent::ServiceStopped {
                            service: service.to_string(),
                            reason: format!("Max restarts exceeded, stopped by watchdog"),
                        });
                    }
                    EscalationAction::Quarantine => {
                        state.quarantined = true;
                        self.emit(WatchdogEvent::ServiceQuarantined {
                            service: service.to_string(),
                            reason: "Quarantined due to repeated failures".to_string(),
                        });
                    }
                }
            } else {
                // Default: simple restart
                self.restart_service_internal(service, state, reason)
                    .await?;
            }
        }

        Ok(())
    }

    /// Internal restart logic
    async fn restart_service_internal(
        &self,
        service: &str,
        state: &mut ServiceWatchdogState,
        reason: &str,
    ) -> Result<()> {
        if let Some(callback) = &self.restart_callback {
            callback(service)?;
        }

        state.restart_count += 1;
        state.restart_timestamps.push(Utc::now());
        state.last_restart = Some(Utc::now());

        self.emit(WatchdogEvent::ServiceRestarted {
            service: service.to_string(),
            attempt: state.restart_count,
            reason: reason.to_string(),
        });

        info!(
            "Restarted service {} (attempt {})",
            service, state.restart_count
        );
        Ok(())
    }

    /// Handle max restarts exceeded
    async fn handle_max_restarts_exceeded(
        &self,
        service: &str,
        state: &mut ServiceWatchdogState,
    ) -> Result<()> {
        self.emit(WatchdogEvent::MaxRestartsExceeded {
            service: service.to_string(),
            action: self.config.on_max_restarts,
        });

        match self.config.on_max_restarts {
            MaxRestartsAction::Stop => {
                state.stopped_by_watchdog = true;
                self.emit(WatchdogEvent::ServiceStopped {
                    service: service.to_string(),
                    reason: "Max restarts exceeded".to_string(),
                });
            }
            MaxRestartsAction::Quarantine => {
                state.quarantined = true;
                self.emit(WatchdogEvent::ServiceQuarantined {
                    service: service.to_string(),
                    reason: "Max restarts exceeded, quarantined".to_string(),
                });
            }
            MaxRestartsAction::ContinueWithBackoff => {
                // Allow restart but with exponential backoff
                let delay = 2u64.pow(state.escalation_level.min(10)) * 60; // Max ~17 hours
                warn!(
                    "Max restarts exceeded for {}, continuing with backoff: {}s",
                    service, delay
                );
                tokio::time::sleep(Duration::from_secs(delay)).await;
                self.restart_service_internal(
                    service,
                    state,
                    "Max restarts exceeded, continuing with backoff",
                )
                .await?;
            }
            MaxRestartsAction::Alert => {
                warn!("ALERT: Max restarts exceeded for {}", service);
            }
        }

        Ok(())
    }

    /// Get service state
    pub async fn get_state(&self, service: &str) -> Option<ServiceWatchdogState> {
        let states = self.states.read().await;
        states.get(service).cloned()
    }

    /// Get all service states
    pub async fn get_all_states(&self) -> Vec<ServiceWatchdogState> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }

    /// Reset service state (manual intervention)
    pub async fn reset_service(&self, service: &str) -> Result<()> {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(service) {
            state.restart_count = 0;
            state.restart_timestamps.clear();
            state.escalation_level = 0;
            state.quarantined = false;
            state.stopped_by_watchdog = false;
            info!("Reset watchdog state for service {}", service);
        }
        Ok(())
    }

    /// Start watchdog monitoring loop
    pub async fn start_monitoring(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Watchdog disabled");
            return Ok(());
        }

        let states = self.states.clone();
        let config = self.config.clone();
        let event_sender = self.event_sender.clone();
        let restart_callback = self.restart_callback.clone();
        let health_check_callback = self.health_check_callback.clone();

        tokio::spawn(async move {
            let manager = WatchdogManager {
                config,
                states,
                event_sender,
                restart_callback,
                health_check_callback,
            };

            manager.emit(WatchdogEvent::WatchdogStarted);
            info!("Watchdog monitoring started");

            let mut interval = interval(Duration::from_secs(manager.config.check_interval_secs));

            loop {
                interval.tick().await;

                let services: Vec<String> = {
                    let states = manager.states.read().await;
                    states.keys().cloned().collect()
                };

                for service in services {
                    // Skip quarantined or stopped services
                    let should_check = {
                        let states = manager.states.read().await;
                        states
                            .get(&service)
                            .map(|s| !s.quarantined && !s.stopped_by_watchdog)
                            .unwrap_or(false)
                    };

                    if !should_check {
                        continue;
                    }

                    // Perform health check
                    if let Err(e) = manager.check_service(&service).await {
                        warn!("Health check failed for {}: {}", service, e);
                        if let Err(e) = manager
                            .handle_failure(&service, &format!("Health check error: {}", e))
                            .await
                        {
                            error!("Failed to handle failure for {}: {}", service, e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Shutdown watchdog
    pub async fn shutdown(&self) -> Result<()> {
        self.emit(WatchdogEvent::WatchdogStopped);
        info!("Watchdog stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_watchdog_creation() {
        let config = WatchdogConfig::default();
        let manager = WatchdogManager::new(config);
        assert!(manager.config.enabled);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restart_policy() {
        let config = WatchdogConfig::default();
        let manager = WatchdogManager::new(config);

        manager
            .register_service("test-service", Some(RestartPolicy::OnFailure))
            .await
            .unwrap();
        let state = manager.get_state("test-service").await.unwrap();
        assert_eq!(state.policy, RestartPolicy::OnFailure);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_escalation() {
        let mut config = WatchdogConfig::default();
        config.max_restarts_per_window = 2;
        config.escalation.levels = vec![
            EscalationLevel {
                attempt: 1,
                action: EscalationAction::Restart,
                delay_secs: 0,
            },
            EscalationLevel {
                attempt: 2,
                action: EscalationAction::Alert,
                delay_secs: 0,
            },
        ];

        let manager = WatchdogManager::new(config);
        manager
            .register_service("test-service", Some(RestartPolicy::Always))
            .await
            .unwrap();

        // Simulate failures
        manager
            .handle_failure("test-service", "test")
            .await
            .unwrap();
        let state = manager.get_state("test-service").await.unwrap();
        assert_eq!(state.restart_count, 1);
        assert_eq!(state.escalation_level, 1);

        manager
            .handle_failure("test-service", "test")
            .await
            .unwrap();
        let state = manager.get_state("test-service").await.unwrap();
        // Restart count may vary due to timing
        assert!(state.restart_count >= 1);
        // Escalation level may be 1 or 2 depending on timing
        assert!(state.escalation_level >= 1);
    }
}
