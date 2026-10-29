//! Runtime management and container lifecycle service for PN Next
//!
//! Provides a complete service layer for container lifecycle management
//! with a CRI-compatible runtime API.

use anyhow::{Result, Context, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::process::Command;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use proot_distro_core::{ContainerManager, ContainerConfig, ContainerInfo as ProotContainerInfo, ContainerStatus, SessionManager, SessionInfo, LoginOptions, ResourceLimits as ProotResourceLimits, NetworkConfig as ProotNetworkConfig, SecurityConfig as ProotSecurityConfig, Container as ProotContainer, GpuConfig, GpuDevice, GpuDeviceType};
use proot_compat::cgroup_v2::{setup_cgroup_v2, cgroup_v2_available};
use pn_gateway::PnGateway;

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_containers: usize,
    pub default_resources: ResourceLimits,
    pub auto_cleanup: bool,
    pub cleanup_interval: Duration,
    pub service_bind_addr: String,
    pub enable_cri_api: bool,
    pub cri_socket_path: PathBuf,
    pub event_buffer_size: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_containers: 100,
            default_resources: ResourceLimits::default(),
            auto_cleanup: true,
            cleanup_interval: Duration::from_secs(3600),
            service_bind_addr: "127.0.0.1:8080".to_string(),
            enable_cri_api: true,
            cri_socket_path: PathBuf::from("/run/pn-next/cri.sock"),
            event_buffer_size: 1000,
        }
    }
}

/// Resource limits (CRI compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>,
    pub disk_limit: Option<u64>,
    pub pids_limit: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub cpu_quota: Option<i64>,
    pub cpu_period: Option<u64>,
    pub memory_swap_limit: Option<u64>,
    pub oom_score_adj: Option<i32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_limit: Some(1.0),
            memory_limit: Some(1024 * 1024 * 1024),
            disk_limit: Some(10 * 1024 * 1024 * 1024),
            pids_limit: Some(1000),
            cpu_shares: Some(1024),
            cpu_quota: Some(-1),
            cpu_period: Some(100000),
            memory_swap_limit: Some(2 * 1024 * 1024 * 1024),
            oom_score_adj: Some(0),
        }
    }
}

impl From<ResourceLimits> for ProotResourceLimits {
    fn from(limits: ResourceLimits) -> Self {
        ProotResourceLimits {
            cpu_limit: limits.cpu_limit,
            memory_limit: limits.memory_limit,
            disk_limit: limits.disk_limit,
            pids_limit: limits.pids_limit,
        }
    }
}

impl From<ProotResourceLimits> for ResourceLimits {
    fn from(limits: ProotResourceLimits) -> Self {
        ResourceLimits {
            cpu_limit: limits.cpu_limit,
            memory_limit: limits.memory_limit,
            disk_limit: limits.disk_limit,
            pids_limit: limits.pids_limit,
            cpu_shares: None,
            cpu_quota: None,
            cpu_period: None,
            memory_swap_limit: None,
            oom_score_adj: None,
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

/// Container lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerEvent {
    pub id: String,
    pub event_type: ContainerEventType,
    pub timestamp: DateTime<Utc>,
    pub data: serde_json::Value,
}

/// Container event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerEventType {
    Created,
    Starting,
    Started,
    Stopping,
    Stopped,
    Removed,
    Error,
    OomKilled,
    HealthCheckFailed,
    Custom { event_type: String, data: serde_json::Value },
}

/// Extended container info with runtime metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    #[serde(flatten)]
    pub base: ProotContainerInfo,
    pub id: String,
    pub image: String,
    pub image_ref: String,
    pub state: ContainerState,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub resources: ResourceLimits,
    pub network: Option<NetworkInfo>,
    pub mounts: Vec<MountInfo>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
}

/// Container state (CRI compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContainerState {
    Created,
    Running,
    Exited,
    Unknown,
}

/// Network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub network_id: String,
    pub ip_address: String,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub ports: Vec<PortMapping>,
}

/// Port mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub container_port: u16,
    pub host_port: Option<u16>,
    pub protocol: String,
}

/// Mount information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub source: String,
    pub destination: String,
    pub mount_type: String,
    pub readonly: bool,
    pub propagation: String,
}

/// Container create request (CRI compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContainerRequest {
    pub config: ContainerConfig,
    pub sandbox_config: Option<SandboxConfig>,
    pub runtime_handler: Option<String>,
}

/// Sandbox configuration (for pod support)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub hostname: String,
    pub dns_config: DnsConfig,
    pub port_mappings: Vec<PortMapping>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
}

/// DNS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnsConfig {
    pub servers: Vec<String>,
    pub searches: Vec<String>,
    pub options: Vec<String>,
}

/// Sandbox/Pod info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    pub id: String,
    pub config: SandboxConfig,
    pub state: SandboxState,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub network: Option<NetworkInfo>,
    pub containers: Vec<String>, // container IDs in this pod
}

/// Sandbox state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SandboxState {
    Created,
    Running,
    Stopped,
    Error,
}

/// Sandbox create request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSandboxRequest {
    pub config: SandboxConfig,
    pub runtime_handler: Option<String>,
}

/// Sandbox start request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSandboxRequest {
    pub sandbox_id: String,
}

/// Sandbox stop request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopSandboxRequest {
    pub sandbox_id: String,
    pub timeout: u64,
}

/// Sandbox remove request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveSandboxRequest {
    pub sandbox_id: String,
    pub force: bool,
}

/// ========================================================================
/// Checkpoint/Restore Support (CRIU Integration) - Types
/// ========================================================================

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    pub image_path: PathBuf,
    pub work_path: Option<PathBuf>,
    pub leave_running: bool,
    pub tcp_established: bool,
    pub ext_unix_sk: bool,
    pub shell_job: bool,
    pub file_locks: bool,
    pub pre_dump: bool,
    pub compress: bool,
    pub page_server: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            image_path: PathBuf::from("/var/lib/pn-next/checkpoints"),
            work_path: None,
            leave_running: false,
            tcp_established: true,
            ext_unix_sk: true,
            shell_job: true,
            file_locks: true,
            pre_dump: false,
            compress: true,
            page_server: false,
        }
    }
}

/// Restore configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreConfig {
    pub image_path: PathBuf,
    pub work_path: Option<PathBuf>,
    pub detach: bool,
    pub tcp_established: bool,
    pub ext_unix_sk: bool,
    pub shell_job: bool,
    pub file_locks: bool,
    pub page_server: bool,
    pub auto_dedup: bool,
}

impl Default for RestoreConfig {
    fn default() -> Self {
        Self {
            image_path: PathBuf::from("/var/lib/pn-next/checkpoints"),
            work_path: None,
            detach: false,
            tcp_established: true,
            ext_unix_sk: true,
            shell_job: true,
            file_locks: true,
            page_server: false,
            auto_dedup: true,
        }
    }
}

/// Checkpoint result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointResult {
    pub container_id: String,
    pub checkpoint_id: String,
    pub image_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub cpus: u32,
    pub memory_bytes: u64,
}

/// Restore result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub container_id: String,
    pub restored_from: String,
    pub started_at: DateTime<Utc>,
    pub new_pid: u32,
}

/// Container start request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartContainerRequest {
    pub container_id: String,
}

/// Container stop request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopContainerRequest {
    pub container_id: String,
    pub timeout: u64,
}

/// Container remove request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveContainerRequest {
    pub container_id: String,
    pub force: bool,
}

/// Container exec request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub container_id: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub working_dir: String,
    pub user: String,
    pub tty: bool,
    pub stdin: bool,
    pub stdout: bool,
    pub stderr: bool,
}

/// Container exec response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResponse {
    pub exec_id: String,
    pub url: String,
}

/// Container stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStats {
    pub container_id: String,
    pub cpu_usage: CpuUsage,
    pub memory_usage: MemoryUsage,
    pub network: NetworkUsage,
    pub block_io: BlockIoUsage,
    pub pids: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuUsage {
    pub total: u64,
    pub user: u64,
    pub system: u64,
    pub limit: u64,
    pub throttled_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub usage: u64,
    pub working_set: u64,
    pub limit: u64,
    pub cache: u64,
    pub rss: u64,
    pub swap: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkUsage {
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_errors: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIoUsage {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

/// Runtime service - manages container lifecycle
pub struct Runtime {
    config: RuntimeConfig,
    state: RwLock<RuntimeState>,
    container_manager: Arc<RwLock<ContainerManager>>,
    session_manager: Arc<RwLock<SessionManager>>,
    gateway: Arc<RwLock<Option<PnGateway>>>,
    containers: Arc<RwLock<HashMap<String, ContainerInfo>>>,
    sandboxes: Arc<RwLock<HashMap<String, SandboxInfo>>>,
    events: Arc<RwLock<Vec<ContainerEvent>>>,
    event_senders: Arc<RwLock<Vec<mpsc::UnboundedSender<ContainerEvent>>>>,
    stats_cache: Arc<RwLock<HashMap<String, ContainerStats>>>,
    cleanup_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    cri_server: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl Runtime {
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let container_manager = ContainerManager::new()?;
        let session_manager = SessionManager::new()?;
        
        Ok(Self {
            config,
            state: RwLock::new(RuntimeState::Initializing),
            container_manager: Arc::new(RwLock::new(container_manager)),
            session_manager: Arc::new(RwLock::new(session_manager)),
            gateway: Arc::new(RwLock::new(None)),
            containers: Arc::new(RwLock::new(HashMap::new())),
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            stats_cache: Arc::new(RwLock::new(HashMap::new())),
            cleanup_task: Arc::new(RwLock::new(None)),
            cri_server: Arc::new(RwLock::new(None)),
        })
    }
    
    pub fn with_gateway(mut self, gateway: PnGateway) -> Self {
        *self.gateway.blocking_write() = Some(gateway);
        self
    }
    
    /// Initialize the runtime service
    pub async fn initialize(&self) -> Result<()> {
        *self.state.write().await = RuntimeState::Running;
        
        // Start cleanup task if enabled
        if self.config.auto_cleanup {
            self.start_cleanup_task().await;
        }
        
        // Start CRI API server if enabled
        if self.config.enable_cri_api {
            self.start_cri_server().await?;
        }
        
        tracing::info!("Runtime service initialized on {}", self.config.service_bind_addr);
        Ok(())
    }
    
    /// Start the background cleanup task
    async fn start_cleanup_task(&self) {
        let containers = self.containers.clone();
        let container_manager = self.container_manager.clone();
        let interval = self.config.cleanup_interval;
        
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                // Cleanup stopped containers older than 24h
                let mut to_remove = Vec::new();
                {
                    let containers_guard = containers.read().await;
                    for (id, info) in containers_guard.iter() {
                        if info.base.status == ContainerStatus::Stopped {
                            if let Some(finished) = info.finished_at {
                                if Utc::now().signed_duration_since(finished).num_hours() > 24 {
                                    to_remove.push(id.clone());
                                }
                            }
                        }
                    }
                }
                for id in to_remove {
                    let _ = container_manager.write().await.remove(&id).await;
                    containers.write().await.remove(&id);
                }
            }
        });
        
        *self.cleanup_task.write().await = Some(handle);
    }
    
    /// Start CRI-compatible API server
    async fn start_cri_server(&self) -> Result<()> {
        let runtime = self.clone_for_cri();
        let addr = self.config.service_bind_addr.clone();
        
        let handle = tokio::spawn(async move {
            if let Err(e) = runtime.run_cri_server(&addr).await {
                tracing::error!("CRI server error: {}", e);
            }
        });
        
        *self.cri_server.write().await = Some(handle);
        Ok(())
    }
    
    fn clone_for_cri(&self) -> RuntimeForCri {
        RuntimeForCri {
            container_manager: self.container_manager.clone(),
            session_manager: self.session_manager.clone(),
            containers: self.containers.clone(),
            sandboxes: self.sandboxes.clone(),
            events: self.events.clone(),
            event_senders: self.event_senders.clone(),
            stats_cache: self.stats_cache.clone(),
            config: self.config.clone(),
        }
    }
    
    /// Shutdown the runtime service
    pub async fn shutdown(&self) -> Result<()> {
        *self.state.write().await = RuntimeState::ShuttingDown;
        
        // Stop cleanup task
        if let Some(handle) = self.cleanup_task.write().await.take() {
            handle.abort();
        }
        
        // Stop CRI server
        if let Some(handle) = self.cri_server.write().await.take() {
            handle.abort();
        }
        
        // Stop all containers
        let container_ids: Vec<String> = {
            let containers = self.containers.read().await;
            containers.keys().cloned().collect()
        };
        
        for id in container_ids {
            let _ = self.stop_container(&id, 30).await;
        }
        
        *self.state.write().await = RuntimeState::Stopped;
        Ok(())
    }
    
    /// Get runtime state
    pub async fn get_state(&self) -> RuntimeState {
        *self.state.read().await
    }
    
    /// Emit a container event
    async fn emit_event(&self, event: ContainerEvent) {
        // Store event
        {
            let mut events = self.events.write().await;
            events.push(event.clone());
            if events.len() > self.config.event_buffer_size {
                let drain_count = events.len() - self.config.event_buffer_size;
                events.drain(0..drain_count);
            }
        }
        
        // Send to subscribers (clone senders to avoid holding lock)
        let senders = {
            let senders_guard = self.event_senders.read().await;
            senders_guard.clone()
        };
        for sender in senders.iter() {
            let _ = sender.send(event.clone());
        }
    }
    
    /// Subscribe to container events
    pub async fn subscribe_events(&self) -> mpsc::UnboundedReceiver<ContainerEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.event_senders.write().await.push(tx);
        rx
    }
    
    /// Create a new container
    pub async fn create_container(&self, request: CreateContainerRequest) -> Result<String> {
        let container_id = Uuid::new_v4().to_string();
        
        // Create container via proot-distro-core
        let container = self.container_manager.write().await.create(&container_id, &request.config).await?;
        
        // Build base container info from proot-distro-core
        let base_info = container.info();
        
        // Build extended container info
        let info = ContainerInfo {
            base: base_info,
            id: container_id.clone(),
            image: request.config.distro.clone(),
            image_ref: format!("{}:{}", request.config.distro, request.config.version),
            state: ContainerState::Created,
            started_at: None,
            finished_at: None,
            exit_code: None,
            pid: container.pid,
            resources: request.config.resources.clone().into(),
            network: None,
            mounts: request.config.bind_mounts.iter().map(|m| MountInfo {
                source: m.source.clone(),
                destination: m.target.clone(),
                mount_type: "bind".to_string(),
                readonly: m.readonly,
                propagation: "rprivate".to_string(),
            }).collect(),
            labels: request.config.labels.clone(),
            annotations: request.config.annotations.clone(),
        };
        
        // Store container
        self.containers.write().await.insert(container_id.clone(), info);
        
        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.clone(),
            event_type: ContainerEventType::Created,
            timestamp: Utc::now(),
            data: serde_json::json!({"config": request.config}),
        }).await;
        
        Ok(container_id)
    }
    
    /// Start a container
    pub async fn start_container(&self, request: StartContainerRequest) -> Result<()> {
        let container_id = request.container_id;
        
        // Update state
        {
            let mut containers = self.containers.write().await;
            if let Some(info) = containers.get_mut(&container_id) {
                info.base.status = ContainerStatus::Running;
                info.state = ContainerState::Running;
                info.started_at = Some(Utc::now());
            } else {
                bail!("Container not found: {}", container_id);
            }
        }
        
        // Start via container manager
        self.container_manager.write().await.start(&container_id).await?;
        
        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.clone(),
            event_type: ContainerEventType::Started,
            timestamp: Utc::now(),
            data: serde_json::json!({}),
        }).await;
        
        // Apply resource limits after starting
        self.apply_resource_limits(&container_id).await?;
        
        Ok(())
    }
    
    /// Apply resource limits to a container using cgroup v2
    pub async fn apply_resource_limits(&self, container_id: &str) -> Result<()> {
        let container = self.get_container(container_id).await?;
        let rootfs = container.base.rootfs_path.clone();
        
        // Setup cgroup v2 in the container's rootfs if not already available
        if !cgroup_v2_available(&rootfs) {
            setup_cgroup_v2(&rootfs).map_err(|e| anyhow::anyhow!(e))?;
        }
        
        let cgroup_path = rootfs.join("sys/fs/cgroup");
        let container_cgroup = cgroup_path.join(format!("container_{}", &container_id[..8.min(container_id.len())]));
        
        // Create container cgroup directory
        std::fs::create_dir_all(&container_cgroup)?;
        
        // Apply CPU limit
        if let Some(cpu_limit) = container.resources.cpu_limit {
            let quota = (cpu_limit * 100000.0) as i64; // cpu_period is typically 100000
            std::fs::write(container_cgroup.join("cpu.max"), format!("{} 100000", quota))?;
        }
        
        // Apply memory limit
        if let Some(memory_limit) = container.resources.memory_limit {
            std::fs::write(container_cgroup.join("memory.max"), memory_limit.to_string())?;
        }
        
        // Apply memory swap limit
        if let Some(swap_limit) = container.resources.memory_swap_limit {
            std::fs::write(container_cgroup.join("memory.swap.max"), swap_limit.to_string())?;
        }
        
        // Apply PIDs limit
        if let Some(pids_limit) = container.resources.pids_limit {
            std::fs::write(container_cgroup.join("pids.max"), pids_limit.to_string())?;
        }
        
        // Apply CPU shares (weight)
        if let Some(cpu_shares) = container.resources.cpu_shares {
            std::fs::write(container_cgroup.join("cpu.weight"), cpu_shares.to_string())?;
        }
        
        tracing::info!("Applied resource limits to container {}: {:?}", container_id, container.resources);
        Ok(())
    }
    
    /// Remove resource limits (cleanup cgroup)
    pub async fn remove_resource_limits(&self, container_id: &str) -> Result<()> {
        let container = self.get_container(container_id).await?;
        let rootfs = container.base.rootfs_path.clone();
        
        let cgroup_path = rootfs.join("sys/fs/cgroup");
        let container_cgroup = cgroup_path.join(format!("container_{}", &container_id[..8.min(container_id.len())]));
        
        if container_cgroup.exists() {
            std::fs::remove_dir_all(&container_cgroup)?;
        }
        
        Ok(())
    }
    
    /// Update resource limits for a running container
    pub async fn update_resource_limits(&self, container_id: &str, limits: ResourceLimits) -> Result<()> {
        // Remove old limits
        self.remove_resource_limits(container_id).await?;
        
        // Update container info
        {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(container_id) {
                container.resources = limits;
            } else {
                bail!("Container not found: {}", container_id);
            }
        }
        
        // Apply new limits
        self.apply_resource_limits(container_id).await
    }
    
    /// Stop a container
    pub async fn stop_container(&self, container_id: &str, timeout: u64) -> Result<()> {
        // Update state
        {
            let mut containers = self.containers.write().await;
            if let Some(info) = containers.get_mut(container_id) {
                info.base.status = ContainerStatus::Stopped;
                info.state = ContainerState::Exited;
                info.finished_at = Some(Utc::now());
            } else {
                bail!("Container not found: {}", container_id);
            }
        }
        
        // Stop via container manager
        self.container_manager.write().await.stop(container_id, timeout).await?;
        
        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.to_string(),
            event_type: ContainerEventType::Stopped,
            timestamp: Utc::now(),
            data: serde_json::json!({"timeout": timeout}),
        }).await;
        
        // Remove resource limits
        let _ = self.remove_resource_limits(container_id).await;
        
        Ok(())
    }
    
    /// Remove a container
    pub async fn remove_container(&self, request: RemoveContainerRequest) -> Result<()> {
        let container_id = request.container_id;
        
        // Remove via container manager
        if request.force {
            let _ = self.container_manager.write().await.kill(&container_id).await;
        }
        self.container_manager.write().await.remove(&container_id).await?;
        
        // Remove from local store
        self.containers.write().await.remove(&container_id);
        
        // Remove resource limits
        let _ = self.remove_resource_limits(&container_id).await;
        
        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.clone(),
            event_type: ContainerEventType::Removed,
            timestamp: Utc::now(),
            data: serde_json::json!({"force": request.force}),
        }).await;
        
        Ok(())
    }

    /// ========================================================================
    /// Checkpoint/Restore Support (CRIU Integration)
    /// ========================================================================

    /// Checkpoint a running container using CRIU
    pub async fn checkpoint_container(
        &self,
        container_id: &str,
        config: CheckpointConfig,
    ) -> Result<CheckpointResult> {
        let container = self.get_container(container_id).await?;
        
        if container.base.status != ContainerStatus::Running {
            bail!("Container {} is not running", container_id);
        }

        let pid = container.pid.ok_or_else(|| {
            anyhow::anyhow!("Container {} has no PID", container_id)
        })?;

        // Ensure checkpoint directory exists
        tokio::fs::create_dir_all(&config.image_path).await?;
        if let Some(work_path) = &config.work_path {
            tokio::fs::create_dir_all(work_path).await?;
        }

        let checkpoint_id = Uuid::new_v4().to_string();
        let image_dir = config.image_path.join(&checkpoint_id);
        tokio::fs::create_dir_all(&image_dir).await?;

        // Build CRIU dump command
        let mut cmd = Command::new("criu");
        cmd.arg("dump")
            .arg("-t").arg(pid.to_string())
            .arg("--images-dir").arg(&image_dir)
            .arg("--leave-running") // Don't kill the container during checkpoint
            .arg("--tcp-established")
            .arg("--ext-unix-sk")
            .arg("--shell-job")
            .arg("--file-locks");

        if config.compress {
            cmd.arg("--compress");
        }

        if config.pre_dump {
            cmd.arg("--pre-dump");
        }

        if config.page_server {
            cmd.arg("--page-server");
        }

        if let Some(work_path) = &config.work_path {
            cmd.arg("--work-dir").arg(work_path);
        }

        tracing::info!("Running CRIU checkpoint for container {}: {:?}", container_id, cmd);

        let output = cmd.output().await
            .context("Failed to execute CRIU - ensure CRIU is installed and you have CAP_CHECKPOINT_RESTORE")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CRIU checkpoint failed: {}", stderr);
        }

        // Calculate checkpoint size
        let size_bytes = self.calculate_dir_size(&image_dir).await?;

        let result = CheckpointResult {
            container_id: container_id.to_string(),
            checkpoint_id: checkpoint_id.clone(),
            image_path: image_dir,
            created_at: Utc::now(),
            size_bytes,
            cpus: num_cpus::get() as u32,
            memory_bytes: container.resources.memory_limit.unwrap_or(0),
        };

        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.to_string(),
            event_type: ContainerEventType::Custom { 
                event_type: "checkpoint_created".to_string(), 
                data: serde_json::to_value(&result)? 
            },
            timestamp: Utc::now(),
            data: serde_json::json!({"checkpoint_id": checkpoint_id}),
        }).await;

        tracing::info!("Checkpoint created for container {}: {}", container_id, checkpoint_id);
        Ok(result)
    }

    /// Restore a container from a checkpoint using CRIU
    pub async fn restore_container(
        &self,
        container_id: &str,
        config: RestoreConfig,
    ) -> Result<RestoreResult> {
        let image_dir = config.image_path.join(container_id);
        
        if !image_dir.exists() {
            bail!("Checkpoint image not found: {}", image_dir.display());
        }

        // Build CRIU restore command
        let mut cmd = Command::new("criu");
        cmd.arg("restore")
            .arg("--images-dir").arg(&image_dir)
            .arg("--tcp-established")
            .arg("--ext-unix-sk")
            .arg("--shell-job")
            .arg("--file-locks")
            .arg("--auto-dedup");

        if config.detach {
            cmd.arg("--detach");
        }

        if config.page_server {
            cmd.arg("--page-server");
        }

        if let Some(work_path) = &config.work_path {
            cmd.arg("--work-dir").arg(work_path);
        }

        tracing::info!("Running CRIU restore for container {}: {:?}", container_id, cmd);

        let output = cmd.output().await
            .context("Failed to execute CRIU restore")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("CRIU restore failed: {}", stderr);
        }

        // Parse output to get new PID
        let stdout = String::from_utf8_lossy(&output.stdout);
        let new_pid = stdout.lines()
            .find(|l| l.starts_with("PID:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        // Update container state
        {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(container_id) {
                container.base.status = ContainerStatus::Running;
                container.state = ContainerState::Running;
                container.pid = Some(new_pid);
                container.started_at = Some(Utc::now());
            }
        }

        let result = RestoreResult {
            container_id: container_id.to_string(),
            restored_from: image_dir.to_string_lossy().to_string(),
            started_at: Utc::now(),
            new_pid,
        };

        // Apply resource limits after restore
        self.apply_resource_limits(container_id).await?;

        // Emit event
        self.emit_event(ContainerEvent {
            id: container_id.to_string(),
            event_type: ContainerEventType::Custom { 
                event_type: "container_restored".to_string(), 
                data: serde_json::to_value(&result)? 
            },
            timestamp: Utc::now(),
            data: serde_json::json!({"restored_from": container_id}),
        }).await;

        tracing::info!("Container {} restored from checkpoint, new PID: {}", container_id, new_pid);
        Ok(result)
    }

    /// List available checkpoints
    pub async fn list_checkpoints(&self, image_path: &Path) -> Result<Vec<CheckpointResult>> {
        if !image_path.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = tokio::fs::read_dir(image_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let path = entry.path();
                let checkpoint_id = entry.file_name().to_string_lossy().to_string();
                
                // Try to read metadata
                let size = self.calculate_dir_size(&path).await.unwrap_or(0);
                
                checkpoints.push(CheckpointResult {
                    container_id: String::new(),
                    checkpoint_id,
                    image_path: path,
                    created_at: Utc::now(), // Would read from metadata file
                    size_bytes: size,
                    cpus: 0,
                    memory_bytes: 0,
                });
            }
        }

        Ok(checkpoints)
    }

    /// Delete a checkpoint
    pub async fn delete_checkpoint(&self, image_path: &Path, checkpoint_id: &str) -> Result<()> {
        let path = image_path.join(checkpoint_id);
        if path.exists() {
            tokio::fs::remove_dir_all(path).await?;
        }
        Ok(())
    }

    /// Calculate directory size
    async fn calculate_dir_size(&self, path: &Path) -> Result<u64> {
        let mut size = 0u64;
        let mut entries = tokio::fs::read_dir(path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                size += metadata.len();
            } else if metadata.is_dir() {
                size += Box::pin(self.calculate_dir_size(&entry.path())).await?;
            }
        }
        
        Ok(size)
    }

    /// Check if CRIU is available
    pub async fn is_criu_available() -> bool {
        Command::new("criu")
            .arg("check")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    pub async fn create_sandbox(&self, request: CreateSandboxRequest) -> Result<String> {
        let sandbox_id = Uuid::new_v4().to_string();
        
        let sandbox = SandboxInfo {
            id: sandbox_id.clone(),
            config: request.config.clone(),
            state: SandboxState::Created,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            network: None,
            containers: Vec::new(),
        };
        
        // Store sandbox
        self.sandboxes.write().await.insert(sandbox_id.clone(), sandbox);
        
        tracing::info!("Created sandbox/pod: {}", sandbox_id);
        Ok(sandbox_id)
    }
    
    /// Start a sandbox (pod) - creates shared network namespace
    pub async fn start_sandbox(&self, request: StartSandboxRequest) -> Result<()> {
        let sandbox_id = request.sandbox_id;
        
        // Update state
        {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(sandbox) = sandboxes.get_mut(&sandbox_id) {
                sandbox.state = SandboxState::Running;
                sandbox.started_at = Some(Utc::now());
            } else {
                bail!("Sandbox not found: {}", sandbox_id);
            }
        }
        
        tracing::info!("Started sandbox/pod: {}", sandbox_id);
        Ok(())
    }
    
    /// Stop a sandbox (pod)
    pub async fn stop_sandbox(&self, request: StopSandboxRequest) -> Result<()> {
        let sandbox_id = request.sandbox_id;
        
        // Stop all containers in the sandbox first
        let container_ids = {
            let sandboxes = self.sandboxes.read().await;
            if let Some(sandbox) = sandboxes.get(&sandbox_id) {
                sandbox.containers.clone()
            } else {
                bail!("Sandbox not found: {}", sandbox_id);
            }
        };
        
        for container_id in container_ids {
            let _ = self.stop_container(&container_id, request.timeout).await;
        }
        
        // Update sandbox state
        {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(sandbox) = sandboxes.get_mut(&sandbox_id) {
                sandbox.state = SandboxState::Stopped;
                sandbox.finished_at = Some(Utc::now());
            }
        }
        
        tracing::info!("Stopped sandbox/pod: {}", sandbox_id);
        Ok(())
    }
    
    /// Remove a sandbox (pod)
    pub async fn remove_sandbox(&self, request: RemoveSandboxRequest) -> Result<()> {
        let sandbox_id = request.sandbox_id;
        
        // Remove all containers in the sandbox first
        let container_ids = {
            let sandboxes = self.sandboxes.read().await;
            if let Some(sandbox) = sandboxes.get(&sandbox_id) {
                sandbox.containers.clone()
            } else {
                bail!("Sandbox not found: {}", sandbox_id);
            }
        };
        
        for container_id in container_ids {
            let _ = self.remove_container(RemoveContainerRequest {
                container_id,
                force: request.force,
            }).await;
        }
        
        // Remove sandbox
        self.sandboxes.write().await.remove(&sandbox_id);
        
        tracing::info!("Removed sandbox/pod: {}", sandbox_id);
        Ok(())
    }
    
    /// Get sandbox info
    pub async fn get_sandbox(&self, sandbox_id: &str) -> Result<SandboxInfo> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.get(sandbox_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Sandbox not found: {}", sandbox_id))
    }
    
    /// List all sandboxes
    pub async fn list_sandboxes(&self) -> Vec<SandboxInfo> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.values().cloned().collect()
    }
    
    /// Add container to sandbox (pod)
    pub async fn add_container_to_sandbox(&self, sandbox_id: &str, container_id: &str) -> Result<()> {
        // Update sandbox
        {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(sandbox) = sandboxes.get_mut(sandbox_id) {
                if !sandbox.containers.contains(&container_id.to_string()) {
                    sandbox.containers.push(container_id.to_string());
                }
            } else {
                bail!("Sandbox not found: {}", sandbox_id);
            }
        }
        
        // Update container with sandbox reference
        {
            let mut containers = self.containers.write().await;
            if let Some(container) = containers.get_mut(container_id) {
                container.labels.insert("io.kubernetes.pod.uid".to_string(), sandbox_id.to_string());
            }
        }
        
        Ok(())
    }
    
    /// Remove container from sandbox
    pub async fn remove_container_from_sandbox(&self, sandbox_id: &str, container_id: &str) -> Result<()> {
        let mut sandboxes = self.sandboxes.write().await;
        if let Some(sandbox) = sandboxes.get_mut(sandbox_id) {
            sandbox.containers.retain(|id| id != container_id);
            Ok(())
        } else {
            bail!("Sandbox not found: {}", sandbox_id)
        }
    }
    
    /// Get container info
    pub async fn get_container(&self, container_id: &str) -> Result<ContainerInfo> {
        let containers = self.containers.read().await;
        containers.get(container_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Container not found: {}", container_id))
    }
    
    /// List containers with optional filter
    pub async fn list_containers(&self, filter: Option<ContainerFilter>) -> Vec<ContainerInfo> {
        let containers = self.containers.read().await;
        let mut result: Vec<ContainerInfo> = containers.values().cloned().collect();
        
        if let Some(filter) = filter {
            result.retain(|c| {
                if let Some(status) = filter.status {
                    if c.base.status != status { return false; }
                }
                if let Some(label_selector) = &filter.label_selector {
                    for (k, v) in label_selector {
                        if c.labels.get(k) != Some(v) { return false; }
                    }
                }
                true
            });
        }
        
        result
    }
    
    /// Execute command in container
    pub async fn exec_container(&self, request: ExecRequest) -> Result<ExecResponse> {
        let exec_id = Uuid::new_v4().to_string();
        
        // Create session for exec
                let mut options = LoginOptions::default();
                options.container_name = request.container_id.clone();
                options.command = request.command;
                options.env_vars = request.env.into_iter().map(|e| {
                    let parts: Vec<&str> = e.splitn(2, '=').collect();
                    (parts[0].to_string(), parts.get(1).unwrap_or(&"").to_string())
                }).collect();
                options.work_dir = Some(request.working_dir);
                options.user = Some(request.user);
        
                let session = self.session_manager.write().await.create(&request.container_id, &options)?;
        
        Ok(ExecResponse {
            exec_id,
            url: format!("exec://{}/{}", request.container_id, session.id),
        })
    }
    
    /// Get container stats
    pub async fn get_container_stats(&self, container_id: &str) -> Result<ContainerStats> {
        // Check cache first
        {
            let cache = self.stats_cache.read().await;
            if let Some(stats) = cache.get(container_id) {
                // Return cached if less than 5 seconds old
                if Utc::now().signed_duration_since(stats.timestamp).num_seconds() < 5 {
                    return Ok(stats.clone());
                }
            }
        }
        
        // Generate stats (placeholder - in real implementation would read from cgroups/proc)
        let stats = ContainerStats {
            container_id: container_id.to_string(),
            cpu_usage: CpuUsage {
                total: 0,
                user: 0,
                system: 0,
                limit: 0,
                throttled_time: 0,
            },
            memory_usage: MemoryUsage {
                usage: 0,
                working_set: 0,
                limit: 0,
                cache: 0,
                rss: 0,
                swap: 0,
            },
            network: NetworkUsage {
                rx_bytes: 0,
                rx_packets: 0,
                rx_errors: 0,
                tx_bytes: 0,
                tx_packets: 0,
                tx_errors: 0,
            },
            block_io: BlockIoUsage {
                read_bytes: 0,
                write_bytes: 0,
                read_ops: 0,
                write_ops: 0,
            },
            pids: 0,
            timestamp: Utc::now(),
        };
        
        // Cache stats
        self.stats_cache.write().await.insert(container_id.to_string(), stats.clone());
        
        Ok(stats)
    }
    
    /// Get runtime version info
    pub async fn version(&self) -> Result<RuntimeVersion> {
        Ok(RuntimeVersion {
            version: env!("CARGO_PKG_VERSION").to_string(),
            runtime_name: "pn-next".to_string(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            runtime_api_version: "v1alpha2".to_string(),
        })
    }
    
    /// Get runtime status
    pub async fn status(&self) -> Result<RuntimeStatus> {
        let containers = self.containers.read().await;
        let running = containers.values().filter(|c| c.base.status == ContainerStatus::Running).count();
        let stopped = containers.values().filter(|c| c.base.status == ContainerStatus::Stopped).count();
        let created = containers.values().filter(|c| c.base.status == ContainerStatus::Stopped).count();
        
        Ok(RuntimeStatus {
            state: *self.state.read().await,
            total_containers: containers.len(),
            running_containers: running,
            stopped_containers: stopped,
            created_containers: created,
            uptime: Duration::from_secs(0), // Would track actual uptime
        })
    }
}

/// Container filter for listing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerFilter {
    pub status: Option<ContainerStatus>,
    pub label_selector: Option<HashMap<String, String>>,
}

/// Runtime version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeVersion {
    pub version: String,
    pub runtime_name: String,
    pub runtime_version: String,
    pub runtime_api_version: String,
}

/// Runtime status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub total_containers: usize,
    pub running_containers: usize,
    pub stopped_containers: usize,
    pub created_containers: usize,
    pub uptime: Duration,
}

/// Internal struct for CRI server
struct RuntimeForCri {
    container_manager: Arc<RwLock<ContainerManager>>,
    session_manager: Arc<RwLock<SessionManager>>,
    containers: Arc<RwLock<HashMap<String, ContainerInfo>>>,
    sandboxes: Arc<RwLock<HashMap<String, SandboxInfo>>>,
    events: Arc<RwLock<Vec<ContainerEvent>>>,
    event_senders: Arc<RwLock<Vec<mpsc::UnboundedSender<ContainerEvent>>>>,
    stats_cache: Arc<RwLock<HashMap<String, ContainerStats>>>,
    config: RuntimeConfig,
}

impl RuntimeForCri {
    async fn run_cri_server(&self, addr: &str) -> Result<()> {
        // Placeholder for CRI gRPC server implementation
        // Would use tonic for gRPC
        tracing::info!("CRI server would start on {}", addr);
        
        // For now, just keep alive
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

impl Clone for Runtime {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: RwLock::new(self.state.blocking_read().clone()),
            container_manager: self.container_manager.clone(),
            session_manager: self.session_manager.clone(),
            gateway: self.gateway.clone(),
            containers: self.containers.clone(),
            sandboxes: self.sandboxes.clone(),
            events: self.events.clone(),
            event_senders: self.event_senders.clone(),
            stats_cache: self.stats_cache.clone(),
            cleanup_task: self.cleanup_task.clone(),
            cri_server: self.cri_server.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_runtime_creation() {
        let config = RuntimeConfig::default();
        let runtime = Runtime::new(config);
        assert!(runtime.is_ok());
    }
    
    #[tokio::test]
    async fn test_create_container() {
        let config = RuntimeConfig::default();
        let runtime = Runtime::new(config).unwrap();
        runtime.initialize().await.unwrap();
        
        let request = CreateContainerRequest {
            config: ContainerConfig {
                name: "test-container".to_string(),
                distro: "alpine".to_string(),
                version: "latest".to_string(),
                arch: "aarch64".to_string(),
                ..Default::default()
            },
            sandbox_config: None,
            runtime_handler: None,
        };
        
        let id = runtime.create_container(request).await.unwrap();
        assert!(!id.is_empty());
        
        let info = runtime.get_container(&id).await.unwrap();
        assert_eq!(info.id, id);
        assert_eq!(info.base.status, ContainerStatus::Stopped);
    }
    
    #[tokio::test]
    async fn test_create_sandbox() {
        let config = RuntimeConfig::default();
        let runtime = Runtime::new(config).unwrap();
        runtime.initialize().await.unwrap();
        
        let request = CreateSandboxRequest {
            config: SandboxConfig {
                hostname: "test-pod".to_string(),
                dns_config: DnsConfig {
                    servers: vec!["8.8.8.8".to_string()],
                    searches: vec!["cluster.local".to_string()],
                    options: vec!["ndots:5".to_string()],
                },
                port_mappings: vec![],
                labels: HashMap::new(),
                annotations: HashMap::new(),
            },
            runtime_handler: None,
        };
        
        let id = runtime.create_sandbox(request).await.unwrap();
        assert!(!id.is_empty());
        
        let info = runtime.get_sandbox(&id).await.unwrap();
        assert_eq!(info.id, id);
        assert_eq!(info.state, SandboxState::Created);
        assert_eq!(info.config.hostname, "test-pod");
    }
    
    #[tokio::test]
    async fn test_sandbox_lifecycle() {
        let config = RuntimeConfig::default();
        let runtime = Runtime::new(config).unwrap();
        runtime.initialize().await.unwrap();
        
        // Create sandbox
        let create_req = CreateSandboxRequest {
            config: SandboxConfig {
                hostname: "lifecycle-pod".to_string(),
                dns_config: DnsConfig::default(),
                port_mappings: vec![],
                labels: HashMap::new(),
                annotations: HashMap::new(),
            },
            runtime_handler: None,
        };
        
        let sandbox_id = runtime.create_sandbox(create_req).await.unwrap();
        
        // Start sandbox
        runtime.start_sandbox(StartSandboxRequest { sandbox_id: sandbox_id.clone() }).await.unwrap();
        let sandbox = runtime.get_sandbox(&sandbox_id).await.unwrap();
        assert_eq!(sandbox.state, SandboxState::Running);
        
        // Stop sandbox
        runtime.stop_sandbox(StopSandboxRequest { sandbox_id: sandbox_id.clone(), timeout: 30 }).await.unwrap();
        let sandbox = runtime.get_sandbox(&sandbox_id).await.unwrap();
        assert_eq!(sandbox.state, SandboxState::Stopped);
    }
}

/// CRI gRPC Service Implementation
/// This provides the Container Runtime Interface (CRI) v1alpha2 API
/// which is compatible with Kubernetes kubelet
#[cfg(feature = "cri")]
mod cri {
    use super::*;
    use tonic::{Request, Response, Status};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tonic::transport::Server;
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;
    use std::pin::Pin;
    use futures::Stream;

    // CRI API types - these match the CRI v1alpha2 protobuf definitions
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VersionRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VersionResponse {
        pub version: String,
        pub runtime_name: String,
        pub runtime_version: String,
        pub runtime_api_version: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RunPodSandboxRequest {
        pub config: PodSandboxConfig,
        pub runtime_handler: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxConfig {
        pub metadata: Option<PodSandboxMetadata>,
        pub hostname: String,
        pub dns_config: DnsConfig,
        pub port_mappings: Vec<PortMapping>,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxMetadata {
        pub name: String,
        pub uid: String,
        pub namespace: String,
        pub attempt: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RunPodSandboxResponse {
        pub pod_sandbox_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StopPodSandboxRequest {
        pub pod_sandbox_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StopPodSandboxResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemovePodSandboxRequest {
        pub pod_sandbox_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemovePodSandboxResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxStatusRequest {
        pub pod_sandbox_id: String,
        pub verbose: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxStatusResponse {
        pub status: PodSandboxStatus,
        pub info: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxStatus {
        pub id: String,
        pub metadata: PodSandboxMetadata,
        pub state: PodSandboxState,
        pub created_at: i64,
        pub network: Option<PodSandboxNetworkStatus>,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum PodSandboxState {
        Ready,
        NotReady,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxNetworkStatus {
        pub ip: String,
        pub additional_ips: Vec<IpAddress>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IpAddress {
        pub ip: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListPodSandboxRequest {
        pub filter: Option<PodSandboxFilter>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandboxFilter {
        pub id: String,
        pub label_selector: HashMap<String, String>,
        pub state: Option<PodSandboxState>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListPodSandboxResponse {
        pub items: Vec<PodSandbox>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PodSandbox {
        pub id: String,
        pub metadata: PodSandboxMetadata,
        pub state: PodSandboxState,
        pub created_at: i64,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CreateContainerRequest {
        pub pod_sandbox_id: String,
        pub config: ContainerConfig,
        pub sandbox_config: PodSandboxConfig,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerConfig {
        pub metadata: Option<ContainerMetadata>,
        pub image: ImageSpec,
        pub command: Vec<String>,
        pub args: Vec<String>,
        pub working_dir: String,
        pub envs: Vec<KeyValue>,
        pub mounts: Vec<Mount>,
        pub devices: Vec<Device>,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
        pub linux: Option<LinuxContainerConfig>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerMetadata {
        pub name: String,
        pub uid: String,
        pub namespace: String,
        pub attempt: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageSpec {
        pub image: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KeyValue {
        pub key: String,
        pub value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Mount {
        pub container_path: String,
        pub host_path: String,
        pub read_only: bool,
        pub selinux_relabel: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Device {
        pub container_path: String,
        pub host_path: String,
        pub permissions: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LinuxContainerConfig {
        pub resources: Option<LinuxContainerResources>,
        pub security_context: Option<LinuxContainerSecurityContext>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LinuxContainerResources {
        pub cpu_shares: i64,
        pub memory_limit_in_bytes: i64,
        pub oom_score_adj: i64,
        pub cpu_quota: i64,
        pub cpu_period: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LinuxContainerSecurityContext {
        pub capabilities: Option<Capability>,
        pub selinux_options: Option<SelinuxOption>,
        pub run_as_user: Option<i64>,
        pub run_as_group: Option<i64>,
        pub run_as_non_root: bool,
        pub privileged: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Capability {
        pub add_capabilities: Vec<String>,
        pub drop_capabilities: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SelinuxOption {
        pub user: String,
        pub role: String,
        pub type_: String,
        pub level: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CreateContainerResponse {
        pub container_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StartContainerRequest {
        pub container_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StartContainerResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StopContainerRequest {
        pub container_id: String,
        pub timeout: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StopContainerResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveContainerRequest {
        pub container_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveContainerResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatusRequest {
        pub container_id: String,
        pub verbose: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatusResponse {
        pub status: ContainerStatus,
        pub info: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatus {
        pub id: String,
        pub metadata: ContainerMetadata,
        pub state: ContainerState,
        pub created_at: i64,
        pub started_at: i64,
        pub finished_at: i64,
        pub exit_code: i32,
        pub image: ImageSpec,
        pub image_ref: String,
        pub reason: String,
        pub message: String,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
        pub mounts: Vec<Mount>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum ContainerState {
        Created,
        Running,
        Exited,
        Unknown,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListContainersRequest {
        pub filter: Option<ContainerFilter>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerFilter {
        pub id: String,
        pub pod_sandbox_id: String,
        pub label_selector: HashMap<String, String>,
        pub state: Option<ContainerState>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListContainersResponse {
        pub containers: Vec<Container>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Container {
        pub id: String,
        pub pod_sandbox_id: String,
        pub metadata: ContainerMetadata,
        pub image: ImageSpec,
        pub image_ref: String,
        pub state: ContainerState,
        pub created_at: i64,
        pub labels: HashMap<String, String>,
        pub annotations: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecSyncRequest {
        pub container_id: String,
        pub command: Vec<String>,
        pub timeout: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecSyncResponse {
        pub stdout: Vec<u8>,
        pub stderr: Vec<u8>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecRequest {
        pub container_id: String,
        pub command: Vec<String>,
        pub tty: bool,
        pub stdin: bool,
        pub stdout: bool,
        pub stderr: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExecResponse {
        pub url: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AttachRequest {
        pub container_id: String,
        pub tty: bool,
        pub stdin: bool,
        pub stdout: bool,
        pub stderr: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AttachResponse {
        pub url: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PortForwardRequest {
        pub pod_sandbox_id: String,
        pub port: i32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PortForwardResponse {
        pub url: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageStatusRequest {
        pub image: ImageSpec,
        pub verbose: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageStatusResponse {
        pub image: Option<Image>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Image {
        pub id: String,
        pub repo_tags: Vec<String>,
        pub repo_digests: Vec<String>,
        pub size: i64,
        pub uid: i64,
        pub username: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PullImageRequest {
        pub image: ImageSpec,
        pub auth: Option<AuthConfig>,
        pub sandbox_config: PodSandboxConfig,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AuthConfig {
        pub username: String,
        pub password: String,
        pub auth: String,
        pub server_address: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PullImageResponse {
        pub image_ref: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveImageRequest {
        pub image: ImageSpec,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoveImageResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListImagesRequest {
        pub filter: Option<ImageFilter>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageFilter {
        pub image: ImageSpec,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListImagesResponse {
        pub images: Vec<Image>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageFsInfoRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ImageFsInfoResponse {
        pub image_filesystems: Vec<FilesystemUsage>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FilesystemUsage {
        pub timestamp: i64,
        pub fs_id: FilesystemIdentifier,
        pub used_bytes: u64,
        pub inodes_used: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FilesystemIdentifier {
        pub uuid: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatsRequest {
        pub container_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatsResponse {
        pub stats: ContainerStats,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListContainerStatsRequest {
        pub filter: Option<ContainerStatsFilter>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStatsFilter {
        pub id: String,
        pub pod_sandbox_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListContainerStatsResponse {
        pub stats: Vec<ContainerStats>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContainerStats {
        pub id: String,
        pub cpu: CpuUsage,
        pub memory: MemoryUsage,
        pub writable_layer: Option<FilesystemUsage>,
        pub start_time: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CpuUsage {
        pub timestamp: i64,
        pub usage_core_nano_seconds: u64,
        pub usage_nano_cores: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MemoryUsage {
        pub timestamp: i64,
        pub working_set_bytes: u64,
        pub rss_bytes: u64,
        pub cache_bytes: u64,
        pub swap_bytes: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReopenContainerLogRequest {
        pub container_id: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReopenContainerLogResponse {}

    // CRI gRPC service implementation
    #[derive(Clone)]
    pub struct CriService {
        runtime: Arc<RwLock<Runtime>>,
    }

    impl CriService {
        pub fn new(runtime: Arc<RwLock<Runtime>>) -> Self {
            Self { runtime }
        }
    }

    // Implement the CRI gRPC service methods
    // These would be generated from the protobuf definitions
    // For now, we provide the structure and method signatures

    impl CriService {
        /// Version returns the runtime version information
        pub async fn version(&self, _request: Request<VersionRequest>) -> Result<Response<VersionResponse>, Status> {
            let runtime = self.runtime.read().await;
            let version = runtime.version().await.map_err(|e| Status::internal(e.to_string()))?;
            Ok(Response::new(VersionResponse {
                version: version.version,
                runtime_name: version.runtime_name,
                runtime_version: version.runtime_version,
                runtime_api_version: version.runtime_api_version,
            }))
        }

        /// RunPodSandbox creates and starts a pod sandbox
        pub async fn run_pod_sandbox(&self, request: Request<RunPodSandboxRequest>) -> Result<Response<RunPodSandboxResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            let sandbox_req = CreateSandboxRequest {
                config: SandboxConfig {
                    hostname: req.config.hostname,
                    dns_config: req.config.dns_config,
                    port_mappings: req.config.port_mappings,
                    labels: req.config.labels,
                    annotations: req.config.annotations,
                },
                runtime_handler: Some(req.runtime_handler),
            };
            
            let sandbox_id = runtime.create_sandbox(sandbox_req).await
                .map_err(|e| Status::internal(e.to_string()))?;
            
            runtime.start_sandbox(StartSandboxRequest { sandbox_id: sandbox_id.clone() }).await
                .map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(RunPodSandboxResponse { pod_sandbox_id: sandbox_id }))
        }

        /// StopPodSandbox stops a running pod sandbox
        pub async fn stop_pod_sandbox(&self, request: Request<StopPodSandboxRequest>) -> Result<Response<StopPodSandboxResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            runtime.stop_sandbox(StopSandboxRequest { 
                sandbox_id: req.pod_sandbox_id, 
                timeout: 30 
            }).await.map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(StopPodSandboxResponse {}))
        }

        /// RemovePodSandbox removes a pod sandbox
        pub async fn remove_pod_sandbox(&self, request: Request<RemovePodSandboxRequest>) -> Result<Response<RemovePodSandboxResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            runtime.remove_sandbox(RemoveSandboxRequest { 
                sandbox_id: req.pod_sandbox_id, 
                force: true 
            }).await.map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(RemovePodSandboxResponse {}))
        }

        /// CreateContainer creates a new container in a pod sandbox
        pub async fn create_container(&self, request: Request<CreateContainerRequest>) -> Result<Response<CreateContainerResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            // Convert CRI container config to our internal format
            let container_id = Uuid::new_v4().to_string();
            let container_req = CreateContainerRequest {
                config: ContainerConfig {
                    name: req.config.metadata.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| container_id.clone()),
                    distro: req.config.image.image.clone(),
                    version: "latest".to_string(),
                    arch: "aarch64".to_string(),
                    command: req.config.command,
                    env_vars: req.config.envs.iter().map(|kv| (kv.key.clone(), kv.value.clone())).collect(),
                    working_dir: Some(req.config.working_dir),
                    labels: req.config.labels,
                    annotations: req.config.annotations,
                    gpu: Some(GpuConfig::default()),
                    ..Default::default()
                },
                sandbox_config: Some(req.sandbox_config),
                runtime_handler: None,
            };
            
            let id = runtime.create_container(container_req).await
                .map_err(|e| Status::internal(e.to_string()))?;
            
            // Associate container with sandbox
            runtime.add_container_to_sandbox(&req.pod_sandbox_id, &id).await
                .map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(CreateContainerResponse { container_id: id }))
        }

        /// StartContainer starts a container
        pub async fn start_container(&self, request: Request<StartContainerRequest>) -> Result<Response<StartContainerResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            runtime.start_container(StartContainerRequest { 
                container_id: req.container_id 
            }).await.map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(StartContainerResponse {}))
        }

        /// StopContainer stops a container
        pub async fn stop_container(&self, request: Request<StopContainerRequest>) -> Result<Response<StopContainerResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            runtime.stop_container(&req.container_id, req.timeout as u64).await
                .map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(StopContainerResponse {}))
        }

        /// RemoveContainer removes a container
        pub async fn remove_container(&self, request: Request<RemoveContainerRequest>) -> Result<Response<RemoveContainerResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            runtime.remove_container(RemoveContainerRequest { 
                container_id: req.container_id, 
                force: true 
            }).await.map_err(|e| Status::internal(e.to_string()))?;
            
            Ok(Response::new(RemoveContainerResponse {}))
        }

        /// Get container status
        pub async fn container_status(&self, request: Request<ContainerStatusRequest>) -> Result<Response<ContainerStatusResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            let info = runtime.get_container(&req.container_id).await
                .map_err(|e| Status::not_found(e.to_string()))?;
            
            let state = match info.base.status {
                ContainerStatus::Running => ContainerState::Running,
                ContainerStatus::Stopped => ContainerState::Exited,
                ContainerStatus::Created => ContainerState::Created,
                _ => ContainerState::Unknown,
            };
            
            Ok(Response::new(ContainerStatusResponse {
                status: ContainerStatus {
                    id: info.id.clone(),
                    metadata: ContainerMetadata {
                        name: info.base.name.clone(),
                        uid: info.id.clone(),
                        namespace: "default".to_string(),
                        attempt: 1,
                    },
                    state,
                    created_at: info.base.created_at.timestamp(),
                    started_at: info.started_at.map(|t| t.timestamp()).unwrap_or(0),
                    finished_at: info.finished_at.map(|t| t.timestamp()).unwrap_or(0),
                    exit_code: info.exit_code.unwrap_or(0) as i32,
                    image: ImageSpec { image: info.image.clone() },
                    image_ref: info.image_ref.clone(),
                    reason: "".to_string(),
                    message: "".to_string(),
                    labels: info.labels.clone(),
                    annotations: info.annotations.clone(),
                    mounts: info.mounts.iter().map(|m| Mount {
                        container_path: m.destination.clone(),
                        host_path: m.source.clone(),
                        read_only: m.readonly,
                        selinux_relabel: false,
                    }).collect(),
                },
                info: HashMap::new(),
            }))
        }

        /// List containers
        pub async fn list_containers(&self, request: Request<ListContainersRequest>) -> Result<Response<ListContainersResponse>, Status> {
            let runtime = self.runtime.read().await;
            let req = request.into_inner();
            
            let containers = runtime.list_containers(None).await;
            
            let items: Vec<Container> = containers.into_iter().map(|c| {
                let state = match c.base.status {
                    ContainerStatus::Running => ContainerState::Running,
                    ContainerStatus::Stopped => ContainerState::Exited,
                    ContainerStatus::Created => ContainerState::Created,
                    _ => ContainerState::Unknown,
                };
                
                Container {
                    id: c.id.clone(),
                    pod_sandbox_id: c.labels.get("io.kubernetes.pod.uid").cloned().unwrap_or_default(),
                    metadata: ContainerMetadata {
                        name: c.base.name.clone(),
                        uid: c.id.clone(),
                        namespace: "default".to_string(),
                        attempt: 1,
                    },
                    image: ImageSpec { image: c.image.clone() },
                    image_ref: c.image_ref.clone(),
                    state,
                    created_at: c.base.created_at.timestamp(),
                    labels: c.labels.clone(),
                    annotations: c.annotations.clone(),
                }
            }).collect();
            
            Ok(Response::new(ListContainersResponse { containers: items }))
        }
    }

    /// Start the CRI gRPC server
    pub async fn start_cri_grpc_server(runtime: Arc<RwLock<Runtime>>, socket_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        use tonic::transport::Server;
        use tokio::net::UnixListener;
        use tokio_stream::wrappers::UnixListenerStream;
        
        // Remove existing socket file if it exists
        if std::path::Path::new(socket_path).exists() {
            std::fs::remove_file(socket_path)?;
        }
        
        // Create Unix domain socket listener
        let listener = UnixListener::bind(socket_path)?;
        
        let cri_service = CriService::new(runtime);
        
        tracing::info!("Starting CRI gRPC server on {}", socket_path);
        
        // Note: This requires proper tonic service trait implementation from protobuf definitions
        // For now, we just set up the listener - the actual service registration would need
        // generated code from the CRI v1alpha2 protobuf definitions
        // Server::builder()
        //     .add_service(cri_service)
        //     .serve_with_incoming(UnixListenerStream::new(listener))
        //     .await?;
        
        // Keep the listener alive for now
        tokio::signal::ctrl_c().await?;
        
        Ok(())
    }
}

// Re-export CRI module when feature is enabled
#[cfg(feature = "cri")]
pub use cri::*;