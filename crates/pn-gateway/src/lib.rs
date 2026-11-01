//! PN Gateway - Bridge layer between PRoot, PRoot-distro, and proot-compat
//!
//! This crate provides the gateway layer that connects PRoot core,
//! PRoot-distro container management, and proot-compat compatibility layer.
//! It provides fail-safe mechanisms and guardrails for robust operation.

pub mod config;
pub mod fail_safe;
pub mod gateway;
pub mod guardrails;
pub mod health;
pub mod metrics;

pub use config::{
    CircuitBreakerState, FailSafeAction, FailSafeConfig, GatewayConfig, GatewayConfigBuilder,
    GatewayState, GuardrailConfig, GuardrailRule, HealthConfig, MetricsConfig, MetricsExporter,
};
pub use fail_safe::FailSafeManager;
pub use gateway::PnGateway;
pub use guardrails::GuardrailManager;
pub use health::HealthMonitor;
pub use metrics::MetricsCollector;

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
