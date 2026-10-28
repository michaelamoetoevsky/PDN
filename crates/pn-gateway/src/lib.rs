//! PN Gateway - Bridge layer between PRoot, PRoot-distro, and proot-compat
//!
//! This crate provides the gateway layer that connects PRoot core,
//! PRoot-distro container management, and proot-compat compatibility layer.
//! It provides fail-safe mechanisms and guardrails for robust operation.

pub mod gateway;
pub mod fail_safe;
pub mod guardrails;
pub mod health;
pub mod metrics;
pub mod config;

pub use gateway::PnGateway;
pub use fail_safe::FailSafeManager;
pub use guardrails::GuardrailManager;
pub use health::HealthMonitor;
pub use metrics::MetricsCollector;
pub use config::{GatewayConfig, GatewayConfigBuilder, GatewayState, FailSafeConfig, FailSafeAction, GuardrailConfig, GuardrailRule, HealthConfig, MetricsConfig, MetricsExporter, CircuitBreakerState};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_creation() {
        let config = crate::config::GatewayConfig::default();
        let gateway = PnGateway::new(config).await;
        assert!(gateway.is_ok());
    }
}