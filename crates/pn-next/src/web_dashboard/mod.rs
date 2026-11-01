//! Web UI Dashboard Module
//! Real-time monitoring and management dashboard for PDN

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::{debug, error, info, warn};

/// Web dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDashboardConfig {
    /// Enable web dashboard
    pub enabled: bool,
    /// Bind address
    pub bind_addr: String,
    /// Port
    pub port: u16,
    /// Static files directory
    pub static_dir: Option<String>,
    /// Enable WebSocket for real-time updates
    pub websocket_enabled: bool,
    /// Update interval (seconds)
    pub update_interval_secs: u64,
    /// Enable authentication
    pub auth_enabled: bool,
    /// JWT secret for auth
    pub jwt_secret: Option<String>,
    /// API rate limit (requests per minute)
    pub rate_limit_rpm: u32,
}

impl Default for WebDashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_addr: "0.0.0.0".to_string(),
            port: 8080,
            static_dir: None,
            websocket_enabled: true,
            update_interval_secs: 5,
            auth_enabled: false,
            jwt_secret: None,
            rate_limit_rpm: 1000,
        }
    }
}

/// Dashboard state shared across handlers
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub containers: Arc<RwLock<HashMap<String, ContainerInfo>>>,
    pub services: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    pub system_stats: Arc<RwLock<SystemStats>>,
    pub events: Arc<RwLock<Vec<DashboardEvent>>>,
    pub ws_sender: Option<broadcast::Sender<String>>,
    pub start_time: Instant,
}

impl DashboardState {
    pub fn new() -> Self {
        let (ws_sender, _) = broadcast::channel(1000);
        Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
            services: Arc::new(RwLock::new(HashMap::new())),
            system_stats: Arc::new(RwLock::new(SystemStats::default())),
            events: Arc::new(RwLock::new(Vec::new())),
            ws_sender: Some(ws_sender),
            start_time: Instant::now(),
        }
    }

    pub fn add_event(&self, event: DashboardEvent) {
        let event_json = serde_json::to_string(&event).unwrap_or_default();
        let mut events = self.events.blocking_write();
        events.push(event);
        if events.len() > 1000 {
            let drain_end = events.len() - 1000;
            events.drain(0..drain_end);
        }

        if let Some(sender) = &self.ws_sender {
            let _ = sender.send(event_json);
        }
    }
}

impl Default for DashboardState {
    fn default() -> Self {
        Self::new()
    }
}

/// Container information for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub labels: HashMap<String, String>,
    pub ports: Vec<PortInfo>,
}

/// Service information for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub status: String,
    pub health: String,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub started_at: Option<String>,
    pub uptime_seconds: u64,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub restart_policy: String,
    pub dependencies: Vec<String>,
}

/// System statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemStats {
    pub cpu_percent: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_available: u64,
    pub disk_total: u64,
    pub disk_used: u64,
    pub disk_available: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub container_count: usize,
    pub running_containers: usize,
    pub service_count: usize,
    pub running_services: usize,
    pub uptime_seconds: u64,
}

/// Dashboard event for real-time updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEvent {
    pub timestamp: String,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: String,
    pub message: String,
    pub severity: String,
}

/// Port information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub container_port: u16,
    pub host_port: Option<u16>,
    pub protocol: String,
}

/// API request/response types
#[derive(Debug, Deserialize)]
pub struct ContainerActionRequest {
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct ServiceActionRequest {
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateContainerRequest {
    pub name: String,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub ports: Option<Vec<PortMapping>>,
    pub volumes: Option<Vec<VolumeMount>>,
    pub labels: Option<HashMap<String, String>>,
    pub restart_policy: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PortMapping {
    pub container_port: u16,
    pub host_port: Option<u16>,
    pub protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VolumeMount {
    pub host_path: String,
    pub container_path: String,
    pub read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceRequest {
    pub name: String,
    pub command: Vec<String>,
    pub working_dir: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub restart_policy: Option<String>,
    pub health_check: Option<HealthCheckConfig>,
    pub dependencies: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheckConfig {
    pub check_type: String,
    pub target: String,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub retries: u32,
}

/// Web dashboard server
pub struct WebDashboard {
    config: WebDashboardConfig,
    state: Arc<DashboardState>,
    router: Router,
}

impl WebDashboard {
    pub fn new(config: WebDashboardConfig) -> Self {
        let state = Arc::new(DashboardState::new());
        let router = Self::create_router(state.clone());

        Self {
            config,
            state,
            router,
        }
    }

    fn create_router(state: Arc<DashboardState>) -> Router {
        let mut router = Router::new()
            .route("/api/health", get(health_check))
            .route("/api/system", get(get_system_stats))
            .route("/api/containers", get(list_containers))
            .route("/api/containers/:id", get(get_container))
            .route("/api/containers/:id", delete(delete_container))
            .route("/api/containers/:id/action", post(container_action))
            .route("/api/containers", post(create_container))
            .route("/api/services", get(list_services))
            .route("/api/services/:name", get(get_service))
            .route("/api/services/:name", delete(delete_service))
            .route("/api/services/:name/action", post(service_action))
            .route("/api/services", post(create_service))
            .route("/api/events", get(get_events))
            .route("/api/logs/:container_id", get(get_container_logs))
            .route("/api/stats/:container_id", get(get_container_stats))
            .route("/api/metrics", get(get_metrics))
            .route("/ws", get(websocket_handler))
            .with_state(state.clone());

        // Add CORS
        if true {
            router = router.layer(CorsLayer::permissive());
        }

        // Serve static files if configured
        // router = router.fallback_service(ServeDir::new("static"));

        router
    }

    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Web dashboard disabled");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.bind_addr, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .context("Failed to bind to address")?;

        info!("Web dashboard starting on {}", addr);

        // Start background tasks
        self.start_background_tasks().await;

        axum::serve(listener, self.router.clone())
            .await
            .context("Web dashboard server error")?;

        Ok(())
    }

    async fn start_background_tasks(&self) {
        let state = self.state.clone();
        let interval_secs = self.config.update_interval_secs;

        // System stats updater
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                update_system_stats(state.clone()).await;
            }
        });

        // Event cleanup
        let state2 = self.state.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut events = state2.events.write().await;
                let len = events.len();
                if len > 1000 {
                    events.drain(0..len - 1000);
                }
            }
        });
    }

    pub fn get_state(&self) -> Arc<DashboardState> {
        self.state.clone()
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "pdn-web-dashboard",
        "version": "0.1.0"
    }))
}

/// Get system statistics
async fn get_system_stats(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    let stats = state.system_stats.read().await;
    Json(stats.clone())
}

/// List all containers
async fn list_containers(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    let containers = state.containers.read().await;
    let containers: Vec<ContainerInfo> = containers.values().cloned().collect();
    Json(containers)
}

/// Get container details
async fn get_container(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let containers = state.containers.read().await;
    if let Some(container) = containers.get(&id) {
        Json(container.clone()).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Container not found"})),
        )
            .into_response()
    }
}

/// Delete container
async fn delete_container(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut containers = state.containers.write().await;
    if containers.remove(&id).is_some() {
        state.add_event(DashboardEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: "delete".to_string(),
            resource_type: "container".to_string(),
            resource_id: id.clone(),
            message: format!("Container {} deleted", id),
            severity: "info".to_string(),
        });
        (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Container not found"})),
        )
            .into_response()
    }
}

/// Container action (start, stop, restart, pause, unpause)
async fn container_action(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
    Json(req): Json<ContainerActionRequest>,
) -> impl IntoResponse {
    let mut containers = state.containers.write().await;

    if let Some(container) = containers.get_mut(&id) {
        match req.action.as_str() {
            "start" => container.status = "running".to_string(),
            "stop" => container.status = "stopped".to_string(),
            "restart" => {
                container.status = "restarting".to_string();
                // Would actually restart
                container.status = "running".to_string();
            }
            "pause" => container.status = "paused".to_string(),
            "unpause" => container.status = "running".to_string(),
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Invalid action"})),
                )
                    .into_response()
            }
        }

        state.add_event(DashboardEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: "action".to_string(),
            resource_type: "container".to_string(),
            resource_id: id.clone(),
            message: format!("Container {} action: {}", id, req.action),
            severity: "info".to_string(),
        });

        Json(container.clone()).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Container not found"})),
        )
            .into_response()
    }
}

/// Create new container
async fn create_container(
    State(state): State<Arc<DashboardState>>,
    Json(req): Json<CreateContainerRequest>,
) -> impl IntoResponse {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let container = ContainerInfo {
        id: id.clone(),
        name: req.name,
        image: req.image,
        status: "created".to_string(),
        state: "created".to_string(),
        created_at: now.clone(),
        started_at: None,
        cpu_percent: 0.0,
        memory_usage: 0,
        memory_limit: 0,
        network_rx: 0,
        network_tx: 0,
        labels: req.labels.unwrap_or_default(),
        ports: req
            .ports
            .unwrap_or_default()
            .into_iter()
            .map(|p| PortInfo {
                container_port: p.container_port,
                host_port: p.host_port,
                protocol: p.protocol.unwrap_or_else(|| "tcp".to_string()),
            })
            .collect(),
    };

    state
        .containers
        .write()
        .await
        .insert(id.clone(), container.clone());

    state.add_event(DashboardEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event_type: "create".to_string(),
        resource_type: "container".to_string(),
        resource_id: id.clone(),
        message: format!("Container created"),
        severity: "info".to_string(),
    });

    (StatusCode::CREATED, Json(container)).into_response()
}

/// List all services
async fn list_services(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    let services = state.services.read().await;
    let services: Vec<ServiceInfo> = services.values().cloned().collect();
    Json(services)
}

/// Get service details
async fn get_service(
    State(state): State<Arc<DashboardState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let services = state.services.read().await;
    if let Some(service) = services.get(&name) {
        Json(service.clone()).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Service not found"})),
        )
            .into_response()
    }
}

/// Delete service
async fn delete_service(
    State(state): State<Arc<DashboardState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut services = state.services.write().await;
    if services.remove(&name).is_some() {
        state.add_event(DashboardEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: "delete".to_string(),
            resource_type: "service".to_string(),
            resource_id: name.clone(),
            message: format!("Service {} deleted", name),
            severity: "info".to_string(),
        });
        (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Service not found"})),
        )
            .into_response()
    }
}

/// Service action (start, stop, restart, enable, disable)
async fn service_action(
    State(state): State<Arc<DashboardState>>,
    Path(name): Path<String>,
    Json(req): Json<ServiceActionRequest>,
) -> impl IntoResponse {
    let mut services = state.services.write().await;

    if let Some(service) = services.get_mut(&name) {
        let result = match req.action.as_str() {
            "start" => {
                service.status = "running".to_string();
                Ok(())
            }
            "stop" => {
                service.status = "stopped".to_string();
                Ok(())
            }
            "restart" => {
                service.status = "restarting".to_string();
                service.status = "running".to_string();
                Ok(())
            }
            "enable" => {
                service.status = "enabled".to_string();
                Ok(())
            }
            "disable" => {
                service.status = "disabled".to_string();
                Ok(())
            }
            _ => Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid action"})),
            )),
        };

        match result {
            Ok(_) => {
                state.add_event(DashboardEvent {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    event_type: "action".to_string(),
                    resource_type: "service".to_string(),
                    resource_id: name.clone(),
                    message: format!("Service {} action: {}", name, req.action),
                    severity: "info".to_string(),
                });

                Json(service.clone()).into_response()
            }
            Err(e) => e.into_response(),
        }
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Service not found"})),
        )
            .into_response()
    }
}

/// Create new service
async fn create_service(
    State(state): State<Arc<DashboardState>>,
    Json(req): Json<CreateServiceRequest>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();

    let service = ServiceInfo {
        name: req.name.clone(),
        status: "created".to_string(),
        health: "unknown".to_string(),
        pid: None,
        restart_count: 0,
        started_at: None,
        uptime_seconds: 0,
        cpu_percent: 0.0,
        memory_usage: 0,
        memory_limit: 0,
        restart_policy: req
            .restart_policy
            .unwrap_or_else(|| "OnFailure".to_string()),
        dependencies: req.dependencies.unwrap_or_default(),
    };

    state
        .services
        .write()
        .await
        .insert(req.name.clone(), service.clone());

    state.add_event(DashboardEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event_type: "create".to_string(),
        resource_type: "service".to_string(),
        resource_id: req.name.clone(),
        message: format!("Service created"),
        severity: "info".to_string(),
    });

    (StatusCode::CREATED, Json(service)).into_response()
}

/// Get recent events
async fn get_events(
    State(state): State<Arc<DashboardState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let events = state.events.read().await;
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);
    let events: Vec<DashboardEvent> = events.iter().rev().take(limit).cloned().collect();
    Json(events)
}

/// Get container logs
async fn get_container_logs(
    State(_state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let lines = params
        .get("lines")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);
    let follow = params
        .get("follow")
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false);

    Json(serde_json::json!({
        "container_id": id,
        "lines": lines,
        "follow": follow,
        "logs": format!("Logs for container {} (last {} lines)", id, lines)
    }))
    .into_response()
}

/// Get container stats
async fn get_container_stats(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let containers = state.containers.read().await;
    if let Some(container) = containers.get(&id) {
        Json(serde_json::json!({
            "container_id": id,
            "cpu_percent": container.cpu_percent,
            "memory_usage": container.memory_usage,
            "memory_limit": container.memory_limit,
            "network_rx": container.network_rx,
            "network_tx": container.network_tx,
        }))
        .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Container not found"})),
        )
            .into_response()
    }
}

/// Get Prometheus metrics
async fn get_metrics(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    let stats = state.system_stats.read().await;
    let containers = state.containers.read().await;
    let services = state.services.read().await;

    let mut metrics = String::new();
    metrics.push_str(&format!("pdn_containers_total {}\n", containers.len()));
    metrics.push_str(&format!(
        "pdn_containers_running {}\n",
        containers
            .values()
            .filter(|c| c.status == "running")
            .count()
    ));
    metrics.push_str(&format!("pdn_services_total {}\n", services.len()));
    metrics.push_str(&format!(
        "pdn_services_running {}\n",
        services.values().filter(|s| s.status == "running").count()
    ));
    metrics.push_str(&format!("pdn_cpu_percent {:.2}\n", stats.cpu_percent));
    metrics.push_str(&format!("pdn_memory_used {}\n", stats.memory_used));
    metrics.push_str(&format!("pdn_memory_total {}\n", stats.memory_total));

    Response::builder()
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(metrics)
        .unwrap()
}

/// WebSocket handler for real-time updates
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DashboardState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

async fn handle_websocket(mut socket: axum::extract::ws::WebSocket, state: Arc<DashboardState>) {
    let mut rx = state.ws_sender.as_ref().unwrap().subscribe();

    while let Ok(msg) = rx.recv().await {
        if socket
            .send(axum::extract::ws::Message::Text(msg))
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Background task to update system stats
async fn update_system_stats(state: Arc<DashboardState>) {
    // In a real implementation, this would collect actual system stats
    // For now, we'll simulate

    let mut stats = state.system_stats.write().await;
    stats.uptime_seconds = state.start_time.elapsed().as_secs();
    stats.container_count = state.containers.read().await.len();
    stats.service_count = state.services.read().await.len();
    stats.running_containers = state
        .containers
        .read()
        .await
        .values()
        .filter(|c| c.status == "running")
        .count();
    stats.running_services = state
        .services
        .read()
        .await
        .values()
        .filter(|s| s.status == "running")
        .count();

    // Simulate CPU/memory
    stats.cpu_percent = 25.0 + (rand::random::<f64>() * 30.0);
    stats.memory_total = 8 * 1024 * 1024 * 1024; // 8GB
    stats.memory_used = (stats.memory_total as f64 * 0.4) as u64;
    stats.memory_available = stats.memory_total - stats.memory_used;
    stats.disk_total = 100 * 1024 * 1024 * 1024; // 100GB
    stats.disk_used = (stats.disk_total as f64 * 0.3) as u64;
    stats.disk_available = stats.disk_total - stats.disk_used;
}

use chrono;
use rand;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dashboard_config() {
        let config = WebDashboardConfig::default();
        assert!(config.enabled);
        assert_eq!(config.port, 8080);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dashboard_state() {
        let state = DashboardState::new();
        assert_eq!(state.containers.read().await.len(), 0);
    }
}
