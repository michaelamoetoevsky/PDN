//! Runtime management for PN Next

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_containers: usize,
    pub default_resources: ResourceLimits,
    pub auto_cleanup: bool,
    pub cleanup_interval: std::time::Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_containers: 100,
            default_resources: ResourceLimits::default(),
            auto_cleanup: true,
            cleanup_interval: std::time::Duration::from_secs(3600),
        }
    }
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>,
    pub disk_limit: Option<u64>,
    pub pids_limit: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_limit: Some(1.0),
            memory_limit: Some(1024 * 1024 * 1024),
            disk_limit: Some(10 * 1024 * 1024 * 1024),
            pids_limit: Some(1000),
        }
    }
}

/// Runtime state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeState {
    Initializing,
    Running,
    Degraded,
    ShuttingDown,
    Stopped,
}

/// Runtime manager
pub struct Runtime {
    config: RuntimeConfig,
    state: tokio::sync::RwLock<RuntimeState>,
    containers: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, ContainerInfo>>>,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            state: tokio::sync::RwLock::new(RuntimeState::Initializing),
            containers: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
    
    pub async fn initialize(&self) -> anyhow::Result<()> {
        *self.state.write().await = RuntimeState::Running;
        Ok(())
    }
    
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        *self.state.write().await = RuntimeState::ShuttingDown;
        Ok(())
    }
    
    pub async fn get_state(&self) -> RuntimeState {
        *self.state.read().await
    }
    
    pub async fn add_container(&self, name: String, info: ContainerInfo) {
        self.containers.write().await.insert(name, info);
    }
    
    pub async fn remove_container(&self, name: &str) -> bool {
        self.containers.write().await.remove(name).is_some()
    }
    
    pub async fn get_container(&self, name: &str) -> Option<ContainerInfo> {
        self.containers.read().await.get(name).cloned()
    }
    
    pub async fn list_containers(&self) -> Vec<ContainerInfo> {
        self.containers.read().await.values().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub state: String,
}