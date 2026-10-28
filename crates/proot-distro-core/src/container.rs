//! Container management for PRoot Distro

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::paths::ContainerPaths;
use crate::locking::LockManager;

/// Container manager
pub struct ContainerManager {
    containers: Arc<Mutex<HashMap<String, Container>>>,
    paths: ContainerPaths,
    lock_manager: Arc<LockManager>,
}

impl ContainerManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            containers: Arc::new(Mutex::new(HashMap::new())),
            paths: ContainerPaths::new()?,
            lock_manager: Arc::new(LockManager::new()?),
        })
    }
    
    pub fn create(&self, name: &str, config: &ContainerConfig) -> Result<Container> {
        let mut containers = self.containers.lock().unwrap();
        
        if containers.contains_key(name) {
            return Err(anyhow::anyhow!("Container '{}' already exists", name));
        }
        
        let container = Container::new(name.to_string(), config.clone())?;
        containers.insert(name.to_string(), container.clone());
        
        Ok(container)
    }
    
    pub fn get(&self, name: &str) -> Result<Container> {
        let containers = self.containers.lock().unwrap();
        containers.get(name).cloned().ok_or_else(|| anyhow::anyhow!("Container '{}' not found", name))
    }
    
    pub fn remove(&self, name: &str) -> Result<()> {
        let mut containers = self.containers.lock().unwrap();
        containers.remove(name).ok_or_else(|| anyhow::anyhow!("Container '{}' not found", name))?;
        Ok(())
    }
    
    pub fn list(&self) -> Result<Vec<ContainerInfo>> {
        let containers = self.containers.lock().unwrap();
        Ok(containers.values().map(|c| c.info()).collect())
    }
    
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        // For now, return empty - sessions are managed by SessionManager
        Ok(Vec::new())
    }
    
    pub fn kill(&self, name: &str) -> Result<()> {
        let mut containers = self.containers.lock().unwrap();
        if let Some(container) = containers.get_mut(name) {
            container.status = ContainerStatus::Stopped;
            container.pid = None;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Container '{}' not found", name))
        }
    }
    
    pub fn backup(&self, container: &Container, dest: &Path) -> Result<()> {
        // Implementation for backing up container
        Ok(())
    }
    
    pub fn restore(&mut self, name: &str, src: &Path) -> Result<()> {
        // Implementation for restoring container
        Ok(())
    }
}

/// Container configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub name: String,
    pub distro: String,
    pub version: String,
    pub arch: String,
    pub rootfs_path: Option<PathBuf>,
    pub bind_mounts: Vec<BindMount>,
    pub env_vars: Vec<(String, String)>,
    pub resources: ResourceLimits,
    pub network: NetworkConfig,
    pub security: SecurityConfig,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            distro: "ubuntu".to_string(),
            version: "22.04".to_string(),
            arch: "aarch64".to_string(),
            rootfs_path: None,
            bind_mounts: Vec::new(),
            env_vars: Vec::new(),
            resources: ResourceLimits::default(),
            network: NetworkConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub source: String,
    pub target: String,
    pub readonly: bool,
}

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
            cpu_limit: None,
            memory_limit: None,
            disk_limit: None,
            pids_limit: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub bridge: Option<String>,
    pub ip_address: Option<String>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub port_mappings: Vec<PortMapping>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Bridge,
            bridge: Some("pn0".to_string()),
            ip_address: None,
            gateway: None,
            dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            port_mappings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    Bridge,
    Host,
    None,
    Container(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub privileged: bool,
    pub capabilities: Vec<String>,
    pub seccomp_profile: Option<String>,
    pub apparmor_profile: Option<String>,
    pub selinux_label: Option<String>,
    pub no_new_privileges: bool,
    pub read_only_rootfs: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            privileged: false,
            capabilities: Vec::new(),
            seccomp_profile: None,
            apparmor_profile: None,
            selinux_label: None,
            no_new_privileges: true,
            read_only_rootfs: false,
        }
    }
}

/// Container representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub name: String,
    pub config: ContainerConfig,
    pub created_at: DateTime<Utc>,
    pub status: ContainerStatus,
    pub pid: Option<u32>,
    pub rootfs_path: PathBuf,
}

impl Container {
    pub fn new(name: String, config: ContainerConfig) -> Result<Self> {
        let rootfs_path = config.rootfs_path.clone().unwrap_or_else(|| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/user")).join(".pn-next").join("containers").join(&name)
        });
        
        Ok(Self {
            name,
            config,
            created_at: Utc::now(),
            status: ContainerStatus::Stopped,
            pid: None,
            rootfs_path,
        })
    }
    
    pub fn info(&self) -> ContainerInfo {
        ContainerInfo {
            name: self.name.clone(),
            distro: self.config.distro.clone(),
            version: self.config.version.clone(),
            arch: self.config.arch.clone(),
            status: self.status,
            created_at: self.created_at,
            rootfs_path: self.rootfs_path.clone(),
        }
    }
}

/// Container info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub name: String,
    pub distro: String,
    pub version: String,
    pub arch: String,
    pub status: ContainerStatus,
    pub created_at: DateTime<Utc>,
    pub rootfs_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    Stopped,
    Running,
    Paused,
    Error,
}

/// Session info for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub container_name: String,
    pub user: String,
    pub pid: Option<u32>,
    pub status: String,
    pub started_at: DateTime<Utc>,
}