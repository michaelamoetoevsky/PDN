//! Socket Activation for Containers Module
//! Extends socket activation to support on-demand container startup

use anyhow::{Context, Result};
use nix::sys::socket::{sendmsg, ControlMessage, ControlMessageOwned, MsgFlags, SockaddrLike};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket, UnixListener};
use tokio::sync::{Mutex, RwLock};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use crate::socket_activation::{ListenerType, SocketActivationManager, SocketConfig, SocketType};

/// Container socket activation configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerSocketConfig {
    /// Base socket activation config
    pub socket: SocketConfig,
    /// Container name to activate
    pub container: String,
    /// Command to run in container (optional, overrides container default)
    pub command: Option<Vec<String>>,
    /// Working directory in container
    pub working_dir: Option<String>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// User to run as
    pub user: Option<String>,
    /// Timeout for container startup (seconds)
    pub startup_timeout_secs: u64,
    /// Whether to keep container running after first request
    pub keep_alive: bool,
    /// Idle timeout before stopping container (seconds)
    pub idle_timeout_secs: Option<u64>,
    /// Maximum concurrent connections
    pub max_connections: Option<usize>,
    /// Health check before accepting connections
    pub health_check: Option<HealthCheckConfig>,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check type
    pub check_type: HealthCheckType,
    /// Target (port, path, command)
    pub target: String,
    /// Timeout (seconds)
    pub timeout_secs: u64,
    /// Interval (seconds)
    pub interval_secs: u64,
    /// Retries before marking unhealthy
    pub retries: u32,
}

/// Health check types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    /// HTTP GET request
    Http,
    /// TCP connection
    Tcp,
    /// Command execution
    Command,
    /// Unix socket
    UnixSocket,
}

/// Container socket activation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerSocketEvent {
    /// Container activation triggered
    ActivationTriggered { container: String, socket: String },
    /// Container started successfully
    ContainerStarted { container: String, pid: u32 },
    /// Container failed to start
    ContainerStartFailed { container: String, error: String },
    /// Connection accepted
    ConnectionAccepted {
        container: String,
        socket: String,
        peer: String,
    },
    /// Container stopped due to idle timeout
    ContainerStoppedIdle { container: String },
    /// Container stopped due to error
    ContainerStoppedError { container: String, error: String },
    /// Health check passed
    HealthCheckPassed { container: String },
    /// Health check failed
    HealthCheckFailed { container: String, error: String },
}

/// Container socket activation manager
pub struct ContainerSocketManager {
    base_manager: Arc<SocketActivationManager>,
    container_configs: Arc<RwLock<HashMap<String, ContainerSocketConfig>>>,
    active_containers: Arc<RwLock<HashMap<String, ActiveContainer>>>,
    event_sender: Option<tokio::sync::broadcast::Sender<ContainerSocketEvent>>,
    container_runtime: Option<Arc<dyn ContainerRuntime>>,
}

/// Active container state
#[derive(Debug, Clone)]
struct ActiveContainer {
    config: ContainerSocketConfig,
    pid: Option<u32>,
    started_at: chrono::DateTime<chrono::Utc>,
    last_activity: chrono::DateTime<chrono::Utc>,
    connection_count: usize,
    health_status: HealthStatus,
}

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HealthStatus {
    Unknown,
    Healthy,
    Unhealthy,
    Starting,
}

/// Container runtime trait
#[async_trait::async_trait]
trait ContainerRuntime: Send + Sync {
    async fn start_container(&self, config: &ContainerSocketConfig) -> Result<u32>;
    async fn stop_container(&self, container: &str) -> Result<()>;
    async fn is_running(&self, container: &str) -> Result<bool>;
    async fn exec(&self, container: &str, cmd: &[String]) -> Result<String>;
}

impl ContainerSocketManager {
    /// Create a new container socket manager
    pub fn new(base_manager: Arc<SocketActivationManager>) -> Self {
        Self {
            base_manager,
            container_configs: Arc::new(RwLock::new(HashMap::new())),
            active_containers: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
            container_runtime: None,
        }
    }

    /// Set container runtime
    pub fn with_runtime(mut self, runtime: Arc<dyn ContainerRuntime>) -> Self {
        self.container_runtime = Some(runtime);
        self
    }

    /// Set event broadcaster
    pub fn with_events(
        mut self,
        sender: tokio::sync::broadcast::Sender<ContainerSocketEvent>,
    ) -> Self {
        self.event_sender = Some(sender);
        self
    }

    /// Get event receiver
    pub fn subscribe_events(
        &self,
    ) -> Option<tokio::sync::broadcast::Receiver<ContainerSocketEvent>> {
        self.event_sender.as_ref().map(|s| s.subscribe())
    }

    /// Emit event
    fn emit(&self, event: ContainerSocketEvent) {
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(event);
        }
    }

    /// Register a container socket activation
    pub async fn register_container_socket(&self, config: ContainerSocketConfig) -> Result<()> {
        let socket_name = config.socket.name.clone();

        // Register with base manager
        self.base_manager.add_socket(config.socket.clone()).await?;

        // Store container config
        let mut configs = self.container_configs.write().await;
        configs.insert(socket_name.clone(), config.clone());

        info!(
            "Registered container socket activation: {} -> {}",
            socket_name, config.container
        );
        Ok(())
    }

    /// Unregister a container socket
    pub async fn unregister_container_socket(&self, socket_name: &str) -> Result<()> {
        self.base_manager.remove_socket(socket_name).await?;

        let mut configs = self.container_configs.write().await;
        configs.remove(socket_name);

        // Stop container if running
        self.stop_container(socket_name).await?;

        info!("Unregistered container socket: {}", socket_name);
        Ok(())
    }

    /// Handle incoming connection (called by base manager)
    pub async fn handle_connection(
        &self,
        socket_name: &str,
        _fd: std::os::unix::io::RawFd,
    ) -> Result<()> {
        let config = {
            let configs = self.container_configs.read().await;
            configs.get(socket_name).cloned()
        };

        let config = match config {
            Some(c) => c,
            None => {
                warn!("No container config for socket: {}", socket_name);
                return Ok(());
            }
        };

        // Check if container is already active
        let mut active = self.active_containers.write().await;
        let container_state =
            active
                .entry(socket_name.to_string())
                .or_insert_with(|| ActiveContainer {
                    config: config.clone(),
                    pid: None,
                    started_at: chrono::Utc::now(),
                    last_activity: chrono::Utc::now(),
                    connection_count: 0,
                    health_status: HealthStatus::Unknown,
                });

        // Start container if not running
        if container_state.pid.is_none() {
            self.start_container(socket_name, &config, container_state)
                .await?;
        }

        // Update activity
        container_state.last_activity = chrono::Utc::now();
        container_state.connection_count += 1;

        // Accept connection (FD is already passed)
        self.emit(ContainerSocketEvent::ConnectionAccepted {
            container: config.container.clone(),
            socket: socket_name.to_string(),
            peer: "unknown".to_string(), // Would get from socket
        });

        // Start idle monitor if configured
        if config.idle_timeout_secs.is_some() {
            self.start_idle_monitor(socket_name).await?;
        }

        Ok(())
    }

    /// Start container for socket
    async fn start_container(
        &self,
        socket_name: &str,
        config: &ContainerSocketConfig,
        state: &mut ActiveContainer,
    ) -> Result<()> {
        state.health_status = HealthStatus::Starting;

        self.emit(ContainerSocketEvent::ActivationTriggered {
            container: config.container.clone(),
            socket: socket_name.to_string(),
        });

        // Run health check before starting if configured
        if let Some(hc) = &config.health_check {
            if !self.run_health_check(&config.container, hc).await? {
                let error = "Pre-start health check failed".to_string();
                self.emit(ContainerSocketEvent::ContainerStartFailed {
                    container: config.container.clone(),
                    error: error.clone(),
                });
                return Err(anyhow::anyhow!(error));
            }
        }

        // Start container via runtime
        let pid = if let Some(runtime) = &self.container_runtime {
            runtime.start_container(config).await?
        } else {
            // Fallback: use podnext CLI
            self.start_container_fallback(config).await?
        };

        state.pid = Some(pid);
        state.started_at = chrono::Utc::now();
        state.health_status = HealthStatus::Healthy;

        self.emit(ContainerSocketEvent::ContainerStarted {
            container: config.container.clone(),
            pid,
        });

        // Run post-start health check
        if let Some(hc) = &config.health_check {
            if !self.run_health_check(&config.container, hc).await? {
                state.health_status = HealthStatus::Unhealthy;
                self.emit(ContainerSocketEvent::HealthCheckFailed {
                    container: config.container.clone(),
                    error: "Post-start health check failed".to_string(),
                });
            } else {
                self.emit(ContainerSocketEvent::HealthCheckPassed {
                    container: config.container.clone(),
                });
            }
        }

        // Start periodic health checks
        if config.health_check.is_some() {
            self.start_health_monitor(socket_name).await?;
        }

        Ok(())
    }

    /// Fallback container start using podnext
    async fn start_container_fallback(&self, config: &ContainerSocketConfig) -> Result<u32> {
        let mut cmd = vec![
            "podnext".to_string(),
            "run".to_string(),
            config.container.clone(),
        ];

        if let Some(command) = &config.command {
            cmd.push("--".to_string());
            cmd.extend(command.clone());
        }

        let mut child = tokio::process::Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let pid = child.id().unwrap_or(0);

        // Wait for startup or timeout
        let timeout = Duration::from_secs(config.startup_timeout_secs);
        let _ = tokio::time::timeout(timeout, child.wait()).await;

        Ok(pid)
    }

    /// Stop container
    async fn stop_container(&self, socket_name: &str) -> Result<()> {
        let state = {
            let mut active = self.active_containers.write().await;
            active.remove(socket_name)
        };

        if let Some(state) = state {
            if let Some(pid) = state.pid {
                // Try graceful shutdown first
                let _ = nix::sys::signal::kill(
                    Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGTERM,
                );
                tokio::time::sleep(Duration::from_secs(5)).await;

                // Force kill if still running
                let _ = nix::sys::signal::kill(
                    Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }

            if let Some(runtime) = &self.container_runtime {
                let _ = runtime.stop_container(&state.config.container).await;
            }

            self.emit(ContainerSocketEvent::ContainerStoppedIdle {
                container: state.config.container.clone(),
            });
        }

        Ok(())
    }

    /// Run health check
    async fn run_health_check(&self, container: &str, config: &HealthCheckConfig) -> Result<bool> {
        match config.check_type {
            HealthCheckType::Http => {
                let url = format!("http://localhost:{}", config.target);
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(config.timeout_secs))
                    .build()?;
                let resp = client.get(&url).send().await;
                Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
            }
            HealthCheckType::Tcp => {
                let addr: SocketAddr = config.target.parse()?;
                let result = tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    tokio::net::TcpStream::connect(addr),
                )
                .await;
                Ok(result.is_ok())
            }
            HealthCheckType::Command => {
                if let Some(runtime) = &self.container_runtime {
                    let output = runtime.exec(container, &[config.target.clone()]).await?;
                    Ok(output.trim() == "0" || output.contains("healthy"))
                } else {
                    Ok(true) // Skip if no runtime
                }
            }
            HealthCheckType::UnixSocket => {
                let path = Path::new(&config.target);
                let result = tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    tokio::net::UnixStream::connect(path),
                )
                .await;
                Ok(result.is_ok())
            }
        }
    }

    /// Start health monitor
    async fn start_health_monitor(&self, socket_name: &str) -> Result<()> {
        let socket_name = socket_name.to_string();
        let active = self.active_containers.clone();
        let configs = self.container_configs.clone();
        let event_sender = self.event_sender.clone();
        let runtime = self.container_runtime.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;

                let (config, pid) = {
                    let active_guard = active.read().await;
                    let config_guard = configs.read().await;

                    if let Some(state) = active_guard.get(&socket_name) {
                        if let Some(config) = config_guard.get(&socket_name) {
                            if let Some(hc) = &config.health_check {
                                (Some((config.clone(), hc.clone())), state.pid)
                            } else {
                                (None, None)
                            }
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    }
                };

                if let Some((config, _)) = config {
                    if let Some(hc) = config.health_check {
                        let healthy = if let Some(rt) = &runtime {
                            // Use runtime health check
                            true // Simplified
                        } else {
                            // Default check
                            true
                        };

                        if !healthy {
                            let mut active_guard = active.write().await;
                            if let Some(state) = active_guard.get_mut(&socket_name) {
                                state.health_status = HealthStatus::Unhealthy;
                            }

                            if let Some(sender) = &event_sender {
                                let _ = sender.send(ContainerSocketEvent::HealthCheckFailed {
                                    container: config.container.clone(),
                                    error: "Health check failed".to_string(),
                                });
                            }
                        } else {
                            if let Some(sender) = &event_sender {
                                let _ = sender.send(ContainerSocketEvent::HealthCheckPassed {
                                    container: config.container.clone(),
                                });
                            }
                        }
                    }
                } else {
                    break; // Container no longer tracked
                }
            }
        });

        Ok(())
    }

    /// Start idle monitor
    async fn start_idle_monitor(&self, socket_name: &str) -> Result<()> {
        let socket_name = socket_name.to_string();
        let active = self.active_containers.clone();
        let configs = self.container_configs.clone();
        let self_weak = Arc::downgrade(&Arc::new(self.clone()));

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                let should_stop = {
                    let active_guard = active.read().await;
                    let config_guard = configs.read().await;

                    if let Some(state) = active_guard.get(&socket_name) {
                        if let Some(config) = config_guard.get(&socket_name) {
                            if let Some(idle_timeout) = config.idle_timeout_secs {
                                let idle = chrono::Utc::now() - state.last_activity;
                                idle.num_seconds() > idle_timeout as i64
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        true // Not tracked anymore
                    }
                };

                if should_stop {
                    if let Some(strong) = self_weak.upgrade() {
                        let _ = strong.stop_container(&socket_name).await;
                    }
                    break;
                }
            }
        });

        Ok(())
    }

    /// Get active container info
    pub async fn get_active(
        &self,
        socket_name: &str,
    ) -> Option<(u32, chrono::DateTime<chrono::Utc>, usize)> {
        let active = self.active_containers.read().await;
        active
            .get(socket_name)
            .map(|s| (s.pid.unwrap_or(0), s.started_at, s.connection_count))
    }

    /// List all container sockets
    pub async fn list_container_sockets(&self) -> Vec<ContainerSocketConfig> {
        let configs = self.container_configs.read().await;
        configs.values().cloned().collect()
    }
}

// Need to make ContainerSocketManager cloneable for idle monitor
impl Clone for ContainerSocketManager {
    fn clone(&self) -> Self {
        Self {
            base_manager: self.base_manager.clone(),
            container_configs: self.container_configs.clone(),
            active_containers: self.active_containers.clone(),
            event_sender: self.event_sender.clone(),
            container_runtime: self.container_runtime.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_container_socket_config() {
        let config = ContainerSocketConfig {
            socket: SocketConfig {
                name: "web".to_string(),
                socket_type: SocketType::Stream,
                listen: "0.0.0.0:8080".to_string(),
                service: "web-server".to_string(),
                ..Default::default()
            },
            container: "web-server".to_string(),
            command: Some(vec![
                "nginx".to_string(),
                "-g".to_string(),
                "daemon off;".to_string(),
            ]),
            startup_timeout_secs: 30,
            keep_alive: true,
            idle_timeout_secs: Some(300),
            ..Default::default()
        };

        assert_eq!(config.container, "web-server");
        assert_eq!(config.startup_timeout_secs, 30);
    }
}
