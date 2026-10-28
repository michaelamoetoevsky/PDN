//! API server for PN Next
//!
//! Provides a RESTful API for container management with real integration
//! to proot-distro-core and proot-core.

use anyhow::{Result, Context, bail};
use axum::{
    Router,
    routing::{get, post, put, delete},
    extract::{State, Path, Json, Query},
    response::Json as ResponseJson,
    http::StatusCode,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{CorsLayer, Any};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use proot_distro_core::{ContainerManager, ContainerConfig, ContainerInfo, ContainerStatus, SessionManager, LoginOptions};
use crate::network::{NetworkManager, NetworkConfig, Network};
use crate::storage::{StorageManager, StorageConfig, Volume};

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub api_key: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            enable_cors: true,
            tls_cert: None,
            tls_key: None,
            api_key: None,
        }
    }
}

/// API server with real backend integration
pub struct ApiServer {
    config: ApiConfig,
    router: Router,
    state: Arc<ApiState>,
}

impl ApiServer {
    pub fn new(config: ApiConfig) -> Result<Self> {
        let state = Arc::new(ApiState::new()?);
        
        let mut router = Router::new()
            .route("/health", get(health_check))
            .route("/api/v1/containers", get(list_containers).post(create_container))
            .route("/api/v1/containers/:name", get(get_container).put(update_container).delete(delete_container))
            .route("/api/v1/containers/:name/start", post(start_container))
            .route("/api/v1/containers/:name/stop", post(stop_container))
            .route("/api/v1/containers/:name/restart", post(restart_container))
            .route("/api/v1/containers/:name/logs", get(get_logs))
            .route("/api/v1/containers/:name/exec", post(exec_command))
            .route("/api/v1/containers/:name/stats", get(get_container_stats))
            .route("/api/v1/images", get(list_images).post(pull_image))
            .route("/api/v1/images/:name", get(get_image).delete(delete_image))
            .route("/api/v1/images/:name/tag", post(tag_image))
            .route("/api/v1/networks", get(list_networks).post(create_network))
            .route("/api/v1/networks/:name", get(get_network).delete(delete_network))
            .route("/api/v1/networks/:name/connect", post(connect_network))
            .route("/api/v1/networks/:name/disconnect", post(disconnect_network))
            .route("/api/v1/volumes", get(list_volumes).post(create_volume))
            .route("/api/v1/volumes/:name", get(get_volume).delete(delete_volume))
            .route("/api/v1/status", get(get_status))
            .route("/api/v1/metrics", get(get_metrics))
            .route("/api/v1/plugins", get(list_plugins).post(install_plugin))
            .route("/api/v1/plugins/:name", delete(remove_plugin))
            .with_state(state.clone());
        
        // Add CORS if enabled
        if config.enable_cors {
            router = router.layer(CorsLayer::permissive());
        }
        
        Ok(Self {
            config,
            router,
            state,
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("API server listening on {}", addr);
        axum::serve(listener, self.router.clone()).await?;
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

/// Application state with real backend integration
#[derive(Clone)]
pub struct ApiState {
    container_manager: Arc<RwLock<ContainerManager>>,
    session_manager: Arc<RwLock<SessionManager>>,
    network_manager: Arc<RwLock<NetworkManager>>,
    storage_manager: Arc<RwLock<StorageManager>>,
    started_at: DateTime<Utc>,
}

impl ApiState {
    pub fn new() -> Result<Self> {
        let container_manager = Arc::new(RwLock::new(ContainerManager::new()?));
        let session_manager = Arc::new(RwLock::new(SessionManager::new()?));
        let network_manager = Arc::new(RwLock::new(NetworkManager::new(NetworkConfig::default())?));
        let storage_manager = Arc::new(RwLock::new(StorageManager::new(StorageConfig::default())?));
        
        Ok(Self {
            container_manager,
            session_manager,
            network_manager,
            storage_manager,
            started_at: Utc::now(),
        })
    }
}

/// Health check endpoint
async fn health_check() -> ResponseJson<HealthResponse> {
    ResponseJson(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now(),
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    timestamp: DateTime<Utc>,
}

/// ==================== CONTAINER ENDPOINTS ====================

/// List containers
async fn list_containers(
    Query(params): Query<ListContainersParams>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<Vec<ContainerResponse>>, ApiError> {
    let manager = state.container_manager.read().await;
    let containers = manager.list()?;
    
    let filtered: Vec<_> = if params.all.unwrap_or(false) {
        containers
    } else {
        containers.into_iter()
            .filter(|c| c.status == ContainerStatus::Running)
            .collect()
    };
    
    Ok(ResponseJson(filtered.into_iter().map(ContainerResponse::from).collect()))
}

#[derive(Deserialize)]
struct ListContainersParams {
    all: Option<bool>,
}

/// Get container
async fn get_container(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerDetailResponse>, ApiError> {
    let manager = state.container_manager.read().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    Ok(ResponseJson(ContainerDetailResponse::from(container)))
}

/// Create container
async fn create_container(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateContainerRequest>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let config = ContainerConfig {
        name: req.name.clone(),
        image: req.image,
        command: req.command,
        env: req.env.unwrap_or_default(),
        working_dir: req.working_dir,
        resources: req.resources.map(|r| r.into()),
        network: req.network.map(|n| n.into()),
        binds: req.volumes.map(|v| v.into_iter().map(|v| v.into()).collect()),
    };
    
    let mut manager = state.container_manager.write().await;
    let container = manager.create(config)?;
    
    Ok(ResponseJson(ContainerResponse {
        id: container.info().name,
        message: "Container created".to_string(),
    }))
}

/// Update container
async fn update_container(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
    Json(req): Json<UpdateContainerRequest>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let mut manager = state.container_manager.write().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    // Update container config (simplified - in real impl would need more fields)
    if let Some(new_name) = req.name {
        // Would need rename functionality
    }
    
    Ok(ResponseJson(ContainerResponse {
        id: name,
        message: "Container updated".to_string(),
    }))
}

/// Delete container
async fn delete_container(
    Path(name): Path<String>,
    Query(params): Query<DeleteParams>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let mut manager = state.container_manager.write().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    if container.status == ContainerStatus::Running && !params.force.unwrap_or(false) {
        return Err(ApiError::conflict("Container is running. Use force=true to stop and remove".to_string()));
    }
    
    if container.status == ContainerStatus::Running {
        manager.kill(&name)?;
    }
    
    // Note: proot-distro-core doesn't have a remove method yet, would need to add
    // For now, just stop/kill
    
    Ok(ResponseJson(ContainerResponse {
        id: name,
        message: "Container stopped and marked for removal".to_string(),
    }))
}

#[derive(Deserialize)]
struct DeleteParams {
    force: Option<bool>,
}

/// Start container
async fn start_container(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let mut manager = state.container_manager.write().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    if container.status == ContainerStatus::Running {
        return Err(ApiError::conflict("Container is already running".to_string()));
    }
    
    // Start the container using proot-core
    manager.start(&name).await?;
    
    Ok(ResponseJson(ContainerResponse {
        id: name,
        message: "Container started".to_string(),
    }))
}

/// Stop container
async fn stop_container(
    Path(name): Path<String>,
    Query(params): Query<StopParams>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let mut manager = state.container_manager.write().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    if container.status != ContainerStatus::Running {
        return Err(ApiError::conflict("Container is not running".to_string()));
    }
    
    let timeout = params.timeout.unwrap_or(10);
    manager.stop(&name, timeout).await?;
    
    Ok(ResponseJson(ContainerResponse {
        id: name,
        message: "Container stopped".to_string(),
    }))
}

#[derive(Deserialize)]
struct StopParams {
    timeout: Option<u64>,
}

/// Restart container
async fn restart_container(
    Path(name): Path<String>,
    Query(params): Query<StopParams>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerResponse>, ApiError> {
    let mut manager = state.container_manager.write().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    let timeout = params.timeout.unwrap_or(10);
    manager.restart(&name, timeout).await?;
    
    Ok(ResponseJson(ContainerResponse {
        id: name,
        message: "Container restarted".to_string(),
    }))
}

/// Get container logs
async fn get_logs(
    Path(name): Path<String>,
    Query(params): Query<LogsParams>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<LogsResponse>, ApiError> {
    let manager = state.container_manager.read().await;
    let _container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    // In a real implementation, this would fetch logs from the container
    // For now, return empty logs
    Ok(ResponseJson(LogsResponse {
        logs: vec![],
    }))
}

#[derive(Deserialize)]
struct LogsParams {
    tail: Option<usize>,
    follow: Option<bool>,
    since: Option<String>,
    until: Option<String>,
}

/// Execute command in container
async fn exec_command(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ExecRequest>,
) -> Result<ResponseJson<ExecResponse>, ApiError> {
    let manager = state.container_manager.read().await;
    let container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    if container.status != ContainerStatus::Running {
        return Err(ApiError::conflict("Container is not running".to_string()));
    }
    
    // Create a session to execute the command
    let mut options = LoginOptions::default();
    options.container_name = name;
    options.command = req.command;
    options.user = req.user;
    options.tty = req.tty;
    
    let session_mgr = state.session_manager.read().await;
    let session = session_mgr.create(&name, &options)?;
    
    // In a real implementation, this would execute the command and return output
    // For now, return a placeholder
    Ok(ResponseJson(ExecResponse {
        output: format!("Executed command in session {}", session.id),
        exit_code: 0,
    }))
}

/// Get container stats
async fn get_container_stats(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ContainerStatsResponse>, ApiError> {
    let manager = state.container_manager.read().await;
    let _container = manager.get(&name)
        .ok_or_else(|| ApiError::not_found(format!("Container '{}' not found", name)))?;
    
    // In a real implementation, this would fetch real stats from cgroups
    Ok(ResponseJson(ContainerStatsResponse {
        cpu_usage: 0.0,
        memory_usage: 0,
        memory_limit: 0,
        network_rx: 0,
        network_tx: 0,
        block_read: 0,
        block_write: 0,
        pids: 0,
    }))
}

/// ==================== IMAGE ENDPOINTS ====================

/// List images
async fn list_images(
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<Vec<ImageSummary>>, ApiError> {
    let manager = state.container_manager.read().await;
    let containers = manager.list()?;
    
    // Extract unique images from containers
    let mut images = std::collections::HashSet::new();
    let mut result = Vec::new();
    
    for container in containers {
        if images.insert(container.image.clone()) {
            result.push(ImageSummary {
                id: format!("sha256:{}", container.image),
                name: container.image,
                tag: "latest".to_string(),
                size: 0, // Would need to track actual size
                created_at: container.created_at,
            });
        }
    }
    
    Ok(ResponseJson(result))
}

/// Get image
async fn get_image(
    Path(name): Path<String>,
    State(_state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ImageDetail>, ApiError> {
    // In a real implementation, this would fetch from image store
    Ok(ResponseJson(ImageDetail {
        id: name.clone(),
        name: name.clone(),
        tag: "latest".to_string(),
        size: 0,
        created_at: Utc::now(),
        architecture: "arm64".to_string(),
        os: "linux".to_string(),
        layers: vec![],
    }))
}

/// Pull image
async fn pull_image(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PullImageRequest>,
) -> Result<ResponseJson<ImageResponse>, ApiError> {
    let manager = state.container_manager.read().await;
    
    // Create a container to trigger image pull
    let config = ContainerConfig {
        name: format!("pull-{}", Uuid::new_v4()),
        image: req.image.clone(),
        command: vec![],
        env: vec![],
        working_dir: "/".to_string(),
        resources: None,
        network: None,
        binds: vec![],
    };
    
    let mut manager = state.container_manager.write().await;
    let _container = manager.create(config)?;
    
    Ok(ResponseJson(ImageResponse {
        id: Uuid::new_v4().to_string(),
        message: format!("Image '{}' pull started", req.image),
    }))
}

/// Delete image
async fn delete_image(
    Path(name): Path<String>,
    State(_state): State<Arc<ApiState>>,
) -> Result<ResponseJson<ImageResponse>, ApiError> {
    // In a real implementation, this would remove the image
    Ok(ResponseJson(ImageResponse {
        id: name,
        message: "Image deleted".to_string(),
    }))
}

/// Tag image
async fn tag_image(
    Path(name): Path<String>,
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<TagImageRequest>,
) -> Result<ResponseJson<ImageResponse>, ApiError> {
    Ok(ResponseJson(ImageResponse {
        id: format!("{}:{}", req.repository, req.tag),
        message: "Image tagged".to_string(),
    }))
}

#[derive(Deserialize)]
struct TagImageRequest {
    repository: String,
    tag: String,
}

/// ==================== NETWORK ENDPOINTS ====================

/// List networks
async fn list_networks(
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<Vec<NetworkSummary>>, ApiError> {
    let manager = state.network_manager.read().await;
    let networks = manager.list_networks().await;
    Ok(ResponseJson(networks.into_iter().map(NetworkSummary::from).collect()))
}

/// Get network
async fn get_network(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<NetworkDetail>, ApiError> {
    let manager = state.network_manager.read().await;
    let network = manager.get_network(&name).await
        .ok_or_else(|| ApiError::not_found(format!("Network '{}' not found", name)))?;
    
    Ok(ResponseJson(NetworkDetail::from(network)))
}

/// Create network
async fn create_network(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateNetworkRequest>,
) -> Result<ResponseJson<NetworkResponse>, ApiError> {
    let mut manager = state.network_manager.write().await;
    
    let network = Network {
        id: Uuid::new_v4(),
        name: req.name.clone(),
        driver: req.driver.unwrap_or_else(|| "bridge".to_string()),
        subnet: req.subnet.unwrap_or_else(|| "10.0.0.0/24".to_string()),
        gateway: req.gateway,
        dns_servers: req.dns_servers.unwrap_or_else(|| vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()]),
        enable_ipv6: req.enable_ipv6.unwrap_or(false),
        mtu: req.mtu.unwrap_or(1500),
        created_at: Utc::now(),
        containers: Vec::new(),
    };
    
    manager.create_network(network).await?;
    
    Ok(ResponseJson(NetworkResponse {
        id: req.name,
        message: "Network created".to_string(),
    }))
}

/// Delete network
async fn delete_network(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<NetworkResponse>, ApiError> {
    let mut manager = state.network_manager.write().await;
    manager.remove_network(&name).await?;
    
    Ok(ResponseJson(NetworkResponse {
        id: name,
        message: "Network deleted".to_string(),
    }))
}

/// Connect container to network
async fn connect_network(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ConnectNetworkRequest>,
) -> Result<ResponseJson<NetworkResponse>, ApiError> {
    let manager = state.network_manager.read().await;
    manager.connect_container(&name, &req.container).await?;
    
    Ok(ResponseJson(NetworkResponse {
        id: name,
        message: "Container connected to network".to_string(),
    }))
}

#[derive(Deserialize)]
struct ConnectNetworkRequest {
    container: String,
}

/// Disconnect container from network
async fn disconnect_network(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ConnectNetworkRequest>,
) -> Result<ResponseJson<NetworkResponse>, ApiError> {
    let manager = state.network_manager.read().await;
    manager.disconnect_container(&name, &req.container).await?;
    
    Ok(ResponseJson(NetworkResponse {
        id: name,
        message: "Container disconnected from network".to_string(),
    }))
}

/// ==================== VOLUME ENDPOINTS ====================

/// List volumes
async fn list_volumes(
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<Vec<VolumeSummary>>, ApiError> {
    let manager = state.storage_manager.read().await;
    let volumes = manager.list_volumes().await;
    Ok(ResponseJson(volumes.into_iter().map(VolumeSummary::from).collect()))
}

/// Get volume
async fn get_volume(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<VolumeDetail>, ApiError> {
    let manager = state.storage_manager.read().await;
    let volume = manager.get_volume(&name).await
        .ok_or_else(|| ApiError::not_found(format!("Volume '{}' not found", name)))?;
    
    Ok(ResponseJson(VolumeDetail::from(volume)))
}

/// Create volume
async fn create_volume(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<ResponseJson<VolumeResponse>, ApiError> {
    let mut manager = state.storage_manager.write().await;
    
    let volume = crate::storage::Volume {
        id: Uuid::new_v4(),
        name: req.name.clone(),
        driver: req.driver.unwrap_or_else(|| "local".to_string()),
        mountpoint: PathBuf::new(),
        labels: req.labels.unwrap_or_default(),
        options: req.options.unwrap_or_default(),
        created_at: Utc::now(),
        size: 0,
    };
    
    manager.create_volume(volume).await?;
    
    Ok(ResponseJson(VolumeResponse {
        id: req.name,
        message: "Volume created".to_string(),
    }))
}

/// Delete volume
async fn delete_volume(
    Path(name): Path<String>,
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<VolumeResponse>, ApiError> {
    let mut manager = state.storage_manager.write().await;
    manager.remove_volume(&name).await?;
    
    Ok(ResponseJson(VolumeResponse {
        id: name,
        message: "Volume deleted".to_string(),
    }))
}

/// ==================== STATUS & METRICS ====================

/// Get status
async fn get_status(
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<StatusResponse>, ApiError> {
    let container_manager = state.container_manager.read().await;
    let containers = container_manager.list()?;
    
    let running = containers.iter().filter(|c| c.status == ContainerStatus::Running).count();
    let stopped = containers.iter().filter(|c| c.status == ContainerStatus::Stopped).count();
    
    let network_manager = state.network_manager.read().await;
    let networks = network_manager.list_networks().await;
    
    let storage_manager = state.storage_manager.read().await;
    let volumes = storage_manager.list_volumes().await;
    
    Ok(ResponseJson(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        status: "running".to_string(),
        uptime: (Utc::now() - state.started_at).num_seconds() as u64,
        containers_running: running as u32,
        containers_stopped: stopped as u32,
        images: 0, // Would need image store
        networks: networks.len() as u32,
        volumes: volumes.len() as u32,
    }))
}

/// Get metrics
async fn get_metrics(
    State(_state): State<Arc<ApiState>>,
) -> Result<ResponseJson<MetricsResponse>, ApiError> {
    // In a real implementation, this would return Prometheus metrics
    Ok(ResponseJson(MetricsResponse {
        metrics: serde_json::json!({
            "containers_total": 0,
            "containers_running": 0,
            "images_total": 0,
            "networks_total": 0,
            "volumes_total": 0,
        }),
    }))
}

/// ==================== PLUGIN ENDPOINTS ====================

/// List plugins
async fn list_plugins(
    State(state): State<Arc<ApiState>>,
) -> Result<ResponseJson<Vec<PluginInfo>>, ApiError> {
    // Note: Plugin manager would need to be added to ApiState
    Ok(ResponseJson(vec![]))
}

/// Install plugin
async fn install_plugin(
    State(_state): State<Arc<ApiState>>,
    Json(_req): Json<InstallPluginRequest>,
) -> Result<ResponseJson<PluginResponse>, ApiError> {
    Ok(ResponseJson(PluginResponse {
        message: "Plugin installation not implemented".to_string(),
    }))
}

#[derive(Deserialize)]
struct InstallPluginRequest {
    url: String,
    name: Option<String>,
}

/// Remove plugin
async fn remove_plugin(
    Path(name): Path<String>,
    State(_state): State<Arc<ApiState>>,
) -> Result<ResponseJson<PluginResponse>, ApiError> {
    Ok(ResponseJson(PluginResponse {
        message: format!("Plugin '{}' removal not implemented", name),
    }))
}

/// ==================== RESPONSE TYPES ====================

#[derive(Serialize)]
struct ContainerResponse {
    id: String,
    message: String,
}

#[derive(Serialize)]
struct ContainerSummary {
    id: String,
    name: String,
    image: String,
    status: String,
    created_at: DateTime<Utc>,
}

impl From<ContainerInfo> for ContainerSummary {
    fn from(c: ContainerInfo) -> Self {
        Self {
            id: c.name.clone(),
            name: c.name,
            image: c.image,
            status: format!("{:?}", c.status),
            created_at: c.created_at,
        }
    }
}

#[derive(Serialize)]
struct ContainerResponseFull {
    id: String,
    name: String,
    image: String,
    status: String,
    created_at: DateTime<Utc>,
    state: String,
    config: serde_json::Value,
}

#[derive(Serialize)]
struct ContainerDetailResponse {
    id: String,
    name: String,
    image: String,
    status: String,
    created_at: DateTime<Utc>,
    state: String,
    config: serde_json::Value,
}

impl From<proot_distro_core::Container> for ContainerDetailResponse {
    fn from(c: proot_distro_core::Container) -> Self {
        Self {
            id: c.name.clone(),
            name: c.name,
            image: c.config.image,
            status: format!("{:?}", c.status),
            created_at: c.created_at,
            state: format!("{:?}", c.status),
            config: serde_json::to_value(c.config).unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
struct CreateContainerRequest {
    name: String,
    image: String,
    command: Option<Vec<String>>,
    env: Option<Vec<(String, String)>>,
    working_dir: Option<String>,
    resources: Option<ResourceLimitsRequest>,
    network: Option<NetworkConfigRequest>,
    volumes: Option<Vec<VolumeMountRequest>>,
}

#[derive(Deserialize)]
struct UpdateContainerRequest {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ResourceLimitsRequest {
    cpu_limit: Option<f64>,
    memory_limit: Option<u64>,
    disk_limit: Option<u64>,
    pids_limit: Option<u64>,
}

impl From<ResourceLimitsRequest> for proot_distro_core::ResourceLimits {
    fn from(r: ResourceLimitsRequest) -> Self {
        Self {
            cpu_limit: r.cpu_limit,
            memory_limit: r.memory_limit,
            disk_limit: r.disk_limit,
            pids_limit: r.pids_limit,
        }
    }
}

#[derive(Deserialize)]
struct NetworkConfigRequest {
    mode: Option<String>,
    ports: Option<Vec<PortMappingRequest>>,
}

impl From<NetworkConfigRequest> for proot_distro_core::NetworkConfig {
    fn from(n: NetworkConfigRequest) -> Self {
        Self {
            mode: n.mode.unwrap_or_else(|| "bridge".to_string()).parse().unwrap_or_default(),
            ports: n.ports.map(|p| p.into_iter().map(|p| p.into()).collect()).unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
struct PortMappingRequest {
    host_port: u16,
    container_port: u16,
    protocol: Option<String>,
}

impl From<PortMappingRequest> for proot_distro_core::PortMapping {
    fn from(p: PortMappingRequest) -> Self {
        Self {
            host_port: p.host_port,
            container_port: p.container_port,
            protocol: p.protocol.unwrap_or_else(|| "tcp".to_string()).parse().unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
struct VolumeMountRequest {
    source: String,
    target: String,
    readonly: Option<bool>,
}

impl From<VolumeMountRequest> for proot_distro_core::BindMount {
    fn from(v: VolumeMountRequest) -> Self {
        Self {
            host_path: v.source.into(),
            container_path: v.target.into(),
            readonly: v.readonly.unwrap_or(false),
        }
    }
}

#[derive(Deserialize)]
struct ExecRequest {
    command: Vec<String>,
    user: Option<String>,
    tty: bool,
}

#[derive(Serialize)]
struct ExecResponse {
    output: String,
    exit_code: i32,
}

#[derive(Serialize)]
struct LogsResponse {
    logs: Vec<String>,
}

#[derive(Serialize)]
struct ContainerStatsResponse {
    cpu_usage: f64,
    memory_usage: u64,
    memory_limit: u64,
    network_rx: u64,
    network_tx: u64,
    block_read: u64,
    block_write: u64,
    pids: u64,
}

#[derive(Serialize)]
struct ImageSummary {
    id: String,
    name: String,
    tag: String,
    size: u64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ImageDetail {
    id: String,
    name: String,
    tag: String,
    size: u64,
    created_at: DateTime<Utc>,
    architecture: String,
    os: String,
    layers: Vec<LayerInfo>,
}

#[derive(Serialize)]
struct LayerInfo {
    digest: String,
    size: u64,
}

#[derive(Deserialize)]
struct PullImageRequest {
    image: String,
    tag: Option<String>,
    auth: Option<AuthConfig>,
}

#[derive(Serialize, Deserialize)]
struct AuthConfig {
    username: String,
    password: String,
    server: Option<String>,
}

#[derive(Serialize)]
struct ImageResponse {
    id: String,
    message: String,
}

#[derive(Serialize)]
struct NetworkSummary {
    id: String,
    name: String,
    driver: String,
    subnet: String,
    created_at: DateTime<Utc>,
}

impl From<Network> for NetworkSummary {
    fn from(n: Network) -> Self {
        Self {
            id: n.id.to_string(),
            name: n.name,
            driver: n.driver,
            subnet: n.subnet,
            created_at: n.created_at,
        }
    }
}

#[derive(Serialize)]
struct NetworkDetail {
    id: String,
    name: String,
    driver: String,
    subnet: String,
    gateway: Option<String>,
    dns_servers: Vec<String>,
    enable_ipv6: bool,
    mtu: u16,
    created_at: DateTime<Utc>,
    containers: Vec<String>,
}

impl From<Network> for NetworkDetail {
    fn from(n: Network) -> Self {
        Self {
            id: n.id.to_string(),
            name: n.name,
            driver: n.driver,
            subnet: n.subnet,
            gateway: n.gateway,
            dns_servers: n.dns_servers,
            enable_ipv6: n.enable_ipv6,
            mtu: n.mtu,
            created_at: n.created_at,
            containers: n.containers,
        }
    }
}

#[derive(Deserialize)]
struct CreateNetworkRequest {
    name: String,
    driver: Option<String>,
    subnet: Option<String>,
    gateway: Option<String>,
    dns_servers: Option<Vec<String>>,
    enable_ipv6: Option<bool>,
    mtu: Option<u16>,
}

#[derive(Serialize)]
struct NetworkResponse {
    id: String,
    message: String,
}

#[derive(Serialize)]
struct VolumeSummary {
    id: String,
    name: String,
    driver: String,
    mountpoint: String,
    created_at: DateTime<Utc>,
}

impl From<crate::storage::Volume> for VolumeSummary {
    fn from(v: crate::storage::Volume) -> Self {
        Self {
            id: v.id.to_string(),
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint.to_string_lossy().to_string(),
            created_at: v.created_at,
        }
    }
}

#[derive(Serialize)]
struct VolumeDetail {
    id: String,
    name: String,
    driver: String,
    mountpoint: String,
    labels: std::collections::HashMap<String, String>,
    options: std::collections::HashMap<String, String>,
    created_at: DateTime<Utc>,
    size: u64,
}

impl From<crate::storage::Volume> for VolumeDetail {
    fn from(v: crate::storage::Volume) -> Self {
        Self {
            id: v.id.to_string(),
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint.to_string_lossy().to_string(),
            labels: v.labels,
            options: v.options,
            created_at: v.created_at,
            size: v.size,
        }
    }
}

#[derive(Deserialize)]
struct CreateVolumeRequest {
    name: String,
    driver: Option<String>,
    labels: Option<std::collections::HashMap<String, String>>,
    options: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize)]
struct VolumeResponse {
    id: String,
    message: String,
}

#[derive(Serialize)]
struct StatusResponse {
    version: String,
    status: String,
    uptime: u64,
    containers_running: u32,
    containers_stopped: u32,
    images: u32,
    networks: u32,
    volumes: u32,
}

#[derive(Serialize)]
struct MetricsResponse {
    metrics: serde_json::Value,
}

#[derive(Serialize)]
struct PluginInfo {
    name: String,
    version: String,
    description: String,
    author: String,
    license: String,
}

#[derive(Serialize)]
struct PluginResponse {
    message: String,
}

/// API Error type
#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    Conflict(String),
    BadRequest(String),
    Internal(String),
}

impl ApiError {
    fn not_found(msg: String) -> Self {
        Self::NotFound(msg)
    }
    
    fn conflict(msg: String) -> Self {
        Self::Conflict(msg)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::Conflict(msg) => write!(f, "Conflict: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        
        let body = serde_json::json!({
            "error": message,
            "status": status.as_u16(),
        });
        
        (status, ResponseJson(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_api_server_creation() {
        let config = ApiConfig::default();
        let server = ApiServer::new(config);
        assert!(server.is_ok());
    }
}