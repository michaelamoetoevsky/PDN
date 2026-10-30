//! PN Next - Advanced architecture for PRoot Distro Next
//!
//! This crate provides the advanced architecture layer that builds upon
//! PRoot, PRoot-distro, and proot-compat to provide a more sophisticated,
//! user-friendly, and organized system for managing containers on Termux.

pub mod runtime;
pub mod scheduler;
pub mod network;
pub mod storage;
pub mod security;
pub mod api;
pub mod workflow;
pub mod plugin;

pub use runtime::{Runtime, RuntimeConfig, RuntimeState};
pub use scheduler::{Scheduler, SchedulerConfig, Task, TaskPriority, TaskStatus};
pub use network::{NetworkManager, NetworkConfig, NetworkInterface};
pub use storage::{StorageManager, StorageConfig, Volume, VolumeDriver};
pub use security::{SecurityManager, SecurityConfig, Capability, Policy};
pub use api::{ApiServer, ApiConfig, ApiState};
pub use workflow::{WorkflowEngine, WorkflowConfig, Workflow, WorkflowStep as Step};
pub use plugin::{PluginManager, PluginConfig, Plugin};

use anyhow::Result;
use pn_gateway::PnGateway;
use std::sync::Arc;
use tokio::sync::RwLock;

/// PN Next - Advanced architecture for PRoot Distro Next
pub struct PnNext {
    gateway: Arc<RwLock<Option<pn_gateway::PnGateway>>>,
    runtime: Arc<RwLock<Runtime>>,
    scheduler: Arc<RwLock<Scheduler>>,
    network: Arc<RwLock<NetworkManager>>,
    storage: Arc<RwLock<StorageManager>>,
    security: Arc<RwLock<SecurityManager>>,
    api: Arc<RwLock<ApiServer>>,
    workflow: Arc<RwLock<WorkflowEngine>>,
    plugin_manager: Arc<RwLock<PluginManager>>,
    config: PnNextConfig,
    state: tokio::sync::RwLock<PnNextState>,
}

impl PnNext {
    pub async fn new(config: PnNextConfig) -> Result<Self> {
        let gateway_config = pn_gateway::GatewayConfig {
            name: "pn-next-gateway".to_string(),
            version: "0.1.0".to_string(),
            ..Default::default()
        };

        let gateway = pn_gateway::PnGateway::new(gateway_config).await?;

        let runtime = Runtime::new(config.runtime.clone())?;
        let scheduler = Scheduler::new(config.scheduler.clone())?;
        let network = NetworkManager::new(config.network.clone())?;
        let storage = StorageManager::new(config.storage.clone())?;
        let security = SecurityManager::new(config.security.clone())?;
        let api = ApiServer::new(config.api.clone())?;
        let workflow = WorkflowEngine::new(config.workflow.clone())?;
        let plugin_manager = PluginManager::new(config.plugin.clone())?;

        Ok(Self {
            gateway: Arc::new(RwLock::new(Some(gateway))),
            runtime: Arc::new(RwLock::new(runtime)),
            scheduler: Arc::new(RwLock::new(scheduler)),
            network: Arc::new(RwLock::new(network)),
            storage: Arc::new(RwLock::new(storage)),
            security: Arc::new(RwLock::new(security)),
            api: Arc::new(RwLock::new(api)),
            workflow: Arc::new(RwLock::new(workflow)),
            plugin_manager: Arc::new(RwLock::new(plugin_manager)),
            config,
            state: tokio::sync::RwLock::new(PnNextState::Initializing),
        })
    }

    pub async fn initialize(&self) -> Result<()> {
        *self.state.write().await = PnNextState::Initializing;

        // Initialize gateway
        if let Some(gateway) = &mut *self.gateway.write().await {
            gateway.initialize().await?;
        }

        // Initialize components
        self.runtime.write().await.initialize().await?;
        self.scheduler.write().await.start().await?;
        self.network.write().await.initialize().await?;
        self.storage.write().await.initialize().await?;
        self.security.write().await.initialize().await?;
        self.api.write().await.start().await?;
        self.workflow.write().await.start().await?;
        self.plugin_manager.write().await.load_plugins().await?;

        *self.state.write().await = PnNextState::Running;

        Ok(())
    }

    pub async fn run_container(&self, name: &str, image: &str, args: Vec<String>) -> Result<()> {
        // Validate with security manager
        self.security.read().await.validate_container_creation(&name, &args).await?;

        // Schedule the container creation
        let task = Task::new(
            format!("create_container_{}", name),
            serde_json::json!({
                "name": name,
                "image": image,
                "args": args,
            }),
            TaskPriority::Normal,
        );

        self.scheduler.write().await.submit(task).await?;

        Ok(())
    }

    pub async fn start_api(&self) -> Result<()> {
        self.api.write().await.start().await
    }

    pub async fn shutdown(&self) -> Result<()> {
        *self.state.write().await = PnNextState::ShuttingDown;

        // Shutdown in reverse order
        self.api.write().await.stop().await?;
        self.workflow.write().await.stop().await?;
        self.plugin_manager.write().await.unload_all().await?;
        self.security.write().await.shutdown().await?;
        self.storage.write().await.shutdown().await?;
        self.network.write().await.shutdown().await?;
        self.scheduler.write().await.stop().await?;
        self.runtime.write().await.shutdown().await?;

        if let Some(mut gateway) = self.gateway.write().await.take() {
            gateway.shutdown().await?;
        }

        *self.state.write().await = PnNextState::Stopped;

        Ok(())
    }

    pub async fn get_state(&self) -> PnNextState {
        self.state.read().await.clone()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PnNextConfig {
    pub runtime: RuntimeConfig,
    pub scheduler: SchedulerConfig,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
    pub security: SecurityConfig,
    pub api: ApiConfig,
    pub workflow: WorkflowConfig,
    pub plugin: PluginConfig,
}

impl Default for PnNextConfig {
    fn default() -> Self {
        Self {
            runtime: RuntimeConfig::default(),
            scheduler: SchedulerConfig::default(),
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
            security: SecurityConfig::default(),
            api: ApiConfig::default(),
            workflow: WorkflowConfig::default(),
            plugin: PluginConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PnNextState {
    Initializing,
    Running,
    Degraded,
    ShuttingDown,
    Stopped,
    Error,
}

impl Default for PnNextState {
    fn default() -> Self {
        PnNextState::Initializing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_pn_next_creation() {
        let config = PnNextConfig::default();
        let pn_next = PnNext::new(config).await;
        assert!(pn_next.is_ok());
    }
}