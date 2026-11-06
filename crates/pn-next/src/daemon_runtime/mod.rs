//! Daemon Runtime - Independent lifecycle manager for services
//!
//! Provides a standalone daemon process that manages service lifecycle
//! independently of the main PDN process. Supports:
//! - Service registration and discovery
//! - Independent lifecycle (survives container restarts)
//! - State persistence and recovery
//! - Health monitoring and auto-recovery
//! - IPC communication via Unix sockets

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::fd::OwnedFd;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command as TokioCommand;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Daemon runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonRuntimeConfig {
    /// Socket path for daemon IPC
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,

    /// State directory for persistence
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,

    /// Log directory
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,

    /// Health check interval (seconds)
    #[serde(default = "default_health_interval")]
    pub health_interval_secs: u64,

    /// Max restart attempts per service
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    /// Restart delay (seconds)
    #[serde(default = "default_restart_delay")]
    pub restart_delay_secs: u64,

    /// Enable auto-recovery
    #[serde(default = "default_true")]
    pub auto_recovery: bool,

    /// PID file path
    #[serde(default = "default_pid_file")]
    pub pid_file: PathBuf,
}

fn default_socket_path() -> PathBuf {
    PathBuf::from("/run/pn-next/daemon.sock")
}
fn default_state_dir() -> PathBuf {
    PathBuf::from("/var/lib/pn-next/daemon")
}
fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/pn-next/daemon")
}
fn default_health_interval() -> u64 {
    30
}
fn default_max_restarts() -> u32 {
    5
}
fn default_restart_delay() -> u64 {
    10
}
fn default_true() -> bool {
    true
}
fn default_pid_file() -> PathBuf {
    PathBuf::from("/run/pn-next/daemon.pid")
}

impl Default for DaemonRuntimeConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            state_dir: default_state_dir(),
            log_dir: default_log_dir(),
            health_interval_secs: default_health_interval(),
            max_restarts: default_max_restarts(),
            restart_delay_secs: default_restart_delay(),
            auto_recovery: default_true(),
            pid_file: default_pid_file(),
        }
    }
}

/// Managed service entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedService {
    pub name: String,
    pub command: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub user: Option<String>,
    pub restart_policy: DaemonRestartPolicy,
    pub health_check: Option<DaemonHealthCheckConfig>,
    pub dependencies: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Restart policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DaemonRestartPolicy {
    Never,
    OnFailure,
    Always,
    UnlessStopped,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealthCheckConfig {
    pub command: Option<Vec<String>>,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub retries: u32,
    pub start_period_secs: u64,
}

impl Default for DaemonHealthCheckConfig {
    fn default() -> Self {
        Self {
            command: None,
            interval_secs: 30,
            timeout_secs: 10,
            retries: 3,
            start_period_secs: 10,
        }
    }
}

/// Service runtime state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonServiceState {
    pub name: String,
    pub status: DaemonServiceStatus,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub last_restart: Option<DateTime<Utc>>,
    pub health: DaemonHealthStatus,
    pub exit_code: Option<i32>,
    pub last_check: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DaemonServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Restarting,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DaemonHealthStatus {
    Healthy,
    Unhealthy,
    Starting,
    Unknown,
}

/// Daemon runtime events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonEvent {
    ServiceRegistered {
        name: String,
    },
    ServiceUnregistered {
        name: String,
    },
    ServiceStarted {
        name: String,
        pid: u32,
    },
    ServiceStopped {
        name: String,
        exit_code: Option<i32>,
    },
    ServiceFailed {
        name: String,
        error: String,
    },
    ServiceRestarting {
        name: String,
        attempt: u32,
    },
    ServiceHealthChanged {
        name: String,
        health: DaemonHealthStatus,
    },
    DaemonStarted,
    DaemonStopping,
    DaemonStopped,
    HealthCheckCompleted {
        name: String,
        healthy: bool,
    },
    ConfigReloaded,
}

/// IPC message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    RegisterService {
        service: ManagedService,
    },
    UnregisterService {
        name: String,
    },
    StartService {
        name: String,
    },
    StopService {
        name: String,
        force: bool,
    },
    RestartService {
        name: String,
    },
    GetServiceStatus {
        name: String,
    },
    ListServices,
    GetDaemonStatus,
    /// The last `lines` lines of the daemon's own log, if it keeps one.
    GetLogs {
        lines: usize,
    },
    ShutdownDaemon,
    ReloadConfig,
    Ping,
}

/// The answer to one IPC request, in the same shape the CLI decodes (and the same
/// shape `podnext-daemon` writes): a `type` tag with `ok`/`error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok { data: Option<serde_json::Value> },
    Error { message: String },
}

impl DaemonResponse {
    fn ok(data: Option<serde_json::Value>) -> Self {
        DaemonResponse::Ok { data }
    }

    fn error(message: impl Into<String>) -> Self {
        DaemonResponse::Error {
            message: message.into(),
        }
    }
}

/// Daemon runtime
pub struct DaemonRuntime {
    config: DaemonRuntimeConfig,
    services: Arc<RwLock<HashMap<String, DaemonServiceState>>>,
    service_configs: Arc<RwLock<HashMap<String, ManagedService>>>,
    event_tx: broadcast::Sender<DaemonEvent>,
    command_tx: mpsc::Sender<(DaemonMessage, mpsc::Sender<DaemonResponse>)>,
    /// The receiving half of `command_tx`. Held (not dropped) so `start` can hand it to
    /// the dispatcher that answers IPC requests; only `start` may take it.
    command_rx: Mutex<Option<mpsc::Receiver<(DaemonMessage, mpsc::Sender<DaemonResponse>)>>>,
    health_tasks: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    running: Arc<RwLock<bool>>,
}

impl DaemonRuntime {
    pub fn new(config: DaemonRuntimeConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let (command_tx, command_rx) = mpsc::channel(1024);

        Self {
            config,
            services: Arc::new(RwLock::new(HashMap::new())),
            service_configs: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            command_tx,
            command_rx: Mutex::new(Some(command_rx)),
            health_tasks: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn command_sender(&self) -> mpsc::Sender<(DaemonMessage, mpsc::Sender<DaemonResponse>)> {
        self.command_tx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// Start every daemon task. Takes `self: &Arc<Self>` because the IPC listener and the
    /// command dispatcher both outlive this call and need to keep the runtime alive.
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        *self.running.write().await = true;

        // Create directories
        tokio::fs::create_dir_all(&self.config.state_dir).await?;
        tokio::fs::create_dir_all(&self.config.log_dir).await?;
        if let Some(parent) = self.config.socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if let Some(parent) = self.config.pid_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write PID file
        let pid = std::process::id();
        tokio::fs::write(&self.config.pid_file, pid.to_string()).await?;

        // Load persisted state
        self.load_state().await?;

        // Start health monitor
        self.start_health_monitor().await?;

        // Answer IPC requests before the listener can accept any: a connection that
        // arrives before there is a consumer would sit unanswered forever.
        self.start_command_dispatcher().await;

        // Start IPC listener
        self.start_ipc_listener().await?;

        let _ = self.event_tx.send(DaemonEvent::DaemonStarted);
        info!("Daemon runtime started");
        Ok(())
    }

    /// Take the command receiver and spawn the loop that turns a [`DaemonMessage`] into a
    /// [`DaemonResponse`]. Without this the channel had no consumer and every request
    /// would hang or fail.
    async fn start_command_dispatcher(self: &Arc<Self>) {
        let Some(mut rx) = self.command_rx.lock().await.take() else {
            return;
        };
        let this = Arc::clone(self);
        tokio::spawn(async move {
            while let Some((msg, reply)) = rx.recv().await {
                let resp = this.dispatch(msg).await;
                let _ = reply.send(resp).await;
            }
        });
    }

    async fn start_health_monitor(&self) -> Result<()> {
        let services = self.services.clone();
        let service_configs = self.service_configs.clone();
        let event_tx = self.event_tx.clone();
        let interval_secs = self.config.health_interval_secs;
        let auto_recovery = self.config.auto_recovery;
        let restart_delay = self.config.restart_delay_secs;
        let max_restarts = self.config.max_restarts;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Get snapshot of running services
                let running_services: Vec<(String, DaemonServiceState, DaemonHealthCheckConfig)> = {
                    let services_read = services.read().await;
                    let configs_read = service_configs.read().await;
                    services_read
                        .iter()
                        .filter(|(_, state)| matches!(state.status, DaemonServiceStatus::Running))
                        .filter_map(|(name, state)| {
                            let health_check =
                                configs_read.get(name).and_then(|c| c.health_check.as_ref());
                            health_check.map(|hc| (name.clone(), state.clone(), hc.clone()))
                        })
                        .collect()
                };

                for (name, state, health_check) in running_services {
                    let healthy = Self::run_health_check(&health_check).await;
                    let new_health = if healthy {
                        DaemonHealthStatus::Healthy
                    } else {
                        DaemonHealthStatus::Unhealthy
                    };

                    if new_health != state.health {
                        let mut services_write = services.write().await;
                        if let Some(state) = services_write.get_mut(&name) {
                            state.health = new_health;
                            state.last_check = Some(Utc::now());
                        }

                        let _ = event_tx.send(DaemonEvent::ServiceHealthChanged {
                            name: name.clone(),
                            health: new_health,
                        });

                        // Auto-recovery
                        if !healthy && auto_recovery {
                            let state_read = services.read().await;
                            if let Some(state) = state_read.get(&name) {
                                if matches!(state.status, DaemonServiceStatus::Running)
                                    && state.restart_count < max_restarts
                                {
                                    info!(
                                        "Service {} unhealthy, restarting (attempt {}/{})",
                                        name,
                                        state.restart_count + 1,
                                        max_restarts
                                    );

                                    let _ = event_tx.send(DaemonEvent::ServiceRestarting {
                                        name: name.clone(),
                                        attempt: state.restart_count + 1,
                                    });

                                    tokio::time::sleep(Duration::from_secs(restart_delay)).await;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn start_ipc_listener(&self) -> Result<()> {
        let socket_path = self.config.socket_path.clone();
        let _ = tokio::fs::remove_file(&socket_path).await;

        let listener = UnixListener::bind(&socket_path).context("Failed to bind daemon socket")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&socket_path)?.permissions();
            perms.set_mode(0o666);
            std::fs::set_permissions(&socket_path, perms)?;
        }

        info!("Daemon IPC listening on {:?}", socket_path);

        // Accept connections for the life of the daemon. The listener is moved into the
        // task (the base bound it and dropped it, so nothing ever accepted) and each
        // connection is served on its own task against the command channel.
        let commands = self.command_sender();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let commands = commands.clone();
                        tokio::spawn(serve_connection(stream, commands));
                    }
                    Err(e) => warn!("Daemon IPC accept error: {}", e),
                }
            }
        });

        Ok(())
    }

    /// Answer one parsed request. The daemon-side half of A18(b): every verb the CLI can
    /// send has an answer here, including the daemon-level stop/reload/logs the base
    /// never delivered.
    async fn dispatch(&self, msg: DaemonMessage) -> DaemonResponse {
        match msg {
            DaemonMessage::Ping => DaemonResponse::ok(None),
            DaemonMessage::ListServices => {
                let data = services_json(&self.list_services().await);
                DaemonResponse::ok(Some(data))
            }
            DaemonMessage::GetServiceStatus { name } => {
                match self.get_service_status(&name).await {
                    Ok(Some(state)) => DaemonResponse::ok(Some(service_json(&state))),
                    Ok(None) => DaemonResponse::error(format!("Service not found: {name}")),
                    Err(e) => DaemonResponse::error(e.to_string()),
                }
            }
            DaemonMessage::StartService { name } => match self.start_service(&name).await {
                Ok(()) => DaemonResponse::ok(Some(
                    serde_json::json!({ "message": format!("Service '{name}' start initiated") }),
                )),
                Err(e) => DaemonResponse::error(format!("Failed to start service: {e}")),
            },
            DaemonMessage::StopService { name, force } => {
                match self.stop_service(&name, force).await {
                    Ok(()) => DaemonResponse::ok(Some(
                        serde_json::json!({ "message": format!("Service '{name}' stopped") }),
                    )),
                    Err(e) => DaemonResponse::error(format!("Failed to stop service: {e}")),
                }
            }
            DaemonMessage::RestartService { name } => match self.restart_service(&name).await {
                Ok(()) => DaemonResponse::ok(Some(
                    serde_json::json!({ "message": format!("Service '{name}' restarted") }),
                )),
                Err(e) => DaemonResponse::error(format!("Failed to restart service: {e}")),
            },
            DaemonMessage::RegisterService { service } => {
                let name = service.name.clone();
                match self.register_service(service).await {
                    Ok(()) => DaemonResponse::ok(Some(
                        serde_json::json!({ "message": format!("Service '{name}' registered") }),
                    )),
                    Err(e) => DaemonResponse::error(format!("Failed to register service: {e}")),
                }
            }
            DaemonMessage::UnregisterService { name } => {
                match self.unregister_service(&name).await {
                    Ok(()) => DaemonResponse::ok(Some(
                        serde_json::json!({ "message": format!("Service '{name}' unregistered") }),
                    )),
                    Err(e) => DaemonResponse::error(format!("Failed to unregister service: {e}")),
                }
            }
            DaemonMessage::GetDaemonStatus => {
                let status = self.get_daemon_status().await;
                match serde_json::to_value(status) {
                    Ok(data) => DaemonResponse::ok(Some(data)),
                    Err(e) => DaemonResponse::error(format!("Failed to encode status: {e}")),
                }
            }
            DaemonMessage::ReloadConfig => match self.load_state().await {
                Ok(()) => {
                    let _ = self.event_tx.send(DaemonEvent::ConfigReloaded);
                    DaemonResponse::ok(Some(
                        serde_json::json!({ "message": "configuration reloaded" }),
                    ))
                }
                Err(e) => DaemonResponse::error(format!("Reload failed: {e}")),
            },
            DaemonMessage::GetLogs { lines } => self.daemon_logs(lines).await,
            DaemonMessage::ShutdownDaemon => match self.shutdown().await {
                Ok(()) => DaemonResponse::ok(Some(
                    serde_json::json!({ "message": "daemon shutting down" }),
                )),
                Err(e) => DaemonResponse::error(format!("Shutdown failed: {e}")),
            },
        }
    }

    /// The daemon's own log, if it keeps one. This runtime records state, not a log
    /// stream, so the honest answer when no log file exists is a refusal naming the gap
    /// rather than an empty success.
    async fn daemon_logs(&self, lines: usize) -> DaemonResponse {
        let path = self.config.log_dir.join("daemon.log");
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => {
                let tail: Vec<&str> = text.lines().rev().take(lines).collect();
                let body = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
                DaemonResponse::ok(Some(serde_json::json!({ "message": body })))
            }
            Err(_) => DaemonResponse::error(format!(
                "no daemon log file at {}: this daemon keeps state only (under {}), not a log \
                 stream; use `podnext log query` for service logs",
                path.display(),
                self.config.state_dir.display()
            )),
        }
    }

    async fn run_health_check(config: &DaemonHealthCheckConfig) -> bool {
        if let Some(cmd) = &config.command {
            let timeout_duration = Duration::from_secs(config.timeout_secs);
            let result = tokio::time::timeout(
                timeout_duration,
                TokioCommand::new(&cmd[0]).args(&cmd[1..]).output(),
            )
            .await;

            match result {
                Ok(Ok(output)) => output.status.success(),
                _ => false,
            }
        } else {
            true // No health check configured = healthy
        }
    }

    pub async fn register_service(&self, service: ManagedService) -> Result<()> {
        let name = service.name.clone();

        // Validate dependencies exist
        let configs = self.service_configs.read().await;
        for dep in &service.dependencies {
            if !configs.contains_key(dep) {
                bail!("Dependency '{}' not found for service '{}'", dep, name);
            }
        }
        drop(configs);

        // Register service config
        self.service_configs
            .write()
            .await
            .insert(name.clone(), service);

        // Initialize service state
        let state = DaemonServiceState {
            name: name.clone(),
            status: DaemonServiceStatus::Stopped,
            pid: None,
            started_at: None,
            restart_count: 0,
            last_restart: None,
            health: DaemonHealthStatus::Unknown,
            exit_code: None,
            last_check: None,
        };

        self.services.write().await.insert(name.clone(), state);

        // Persist
        self.persist_state().await?;

        let _ = self
            .event_tx
            .send(DaemonEvent::ServiceRegistered { name: name.clone() });
        info!("Service registered: {}", name);
        Ok(())
    }

    pub async fn unregister_service(&self, name: &str) -> Result<()> {
        // Stop if running
        self.stop_service(name, true).await?;

        let name_owned = name.to_string();

        // Remove from configs
        self.service_configs.write().await.remove(&name_owned);

        // Remove from state
        self.services.write().await.remove(&name_owned);

        // Persist
        self.persist_state().await?;

        let _ = self
            .event_tx
            .send(DaemonEvent::ServiceUnregistered { name: name_owned });
        info!("Service unregistered: {}", name);
        Ok(())
    }

    pub async fn start_service(&self, name: &str) -> Result<()> {
        let configs = self.service_configs.read().await;
        let config = configs
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Service not found: {}", name))?;
        drop(configs);

        // Check dependencies
        for dep in &config.dependencies {
            let services = self.services.read().await;
            let dep_state = services.get(dep);
            if !matches!(
                dep_state.map(|s| s.status),
                Some(DaemonServiceStatus::Running)
            ) {
                // Start dependency first
                drop(services);
                Box::pin(self.start_service(dep)).await?;
            }
        }

        // Update state
        {
            let mut services = self.services.write().await;
            if let Some(state) = services.get_mut(name) {
                state.status = DaemonServiceStatus::Starting;
            }
        }

        // Spawn process
        let mut cmd = TokioCommand::new(&config.command[0]);
        cmd.args(&config.command[1..]);

        if let Some(dir) = &config.working_dir {
            cmd.current_dir(dir);
        }

        for (k, v) in &config.environment {
            cmd.env(k, v);
        }

        if let Some(user) = &config.user {
            cmd.env("USER", user);
        }

        let child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to start service")?;

        let pid = child.id().unwrap_or(0);

        // Update state with PID
        {
            let mut services = self.services.write().await;
            if let Some(state) = services.get_mut(name) {
                state.status = DaemonServiceStatus::Running;
                state.pid = Some(pid);
                state.started_at = Some(Utc::now());
                state.health = DaemonHealthStatus::Starting;
            }
        }

        // Persist
        self.persist_state().await?;

        let _ = self.event_tx.send(DaemonEvent::ServiceStarted {
            name: name.to_string(),
            pid,
        });
        info!("Service started: {} (PID: {})", name, pid);

        // Monitor process
        self.monitor_process(name.to_string(), child).await?;

        Ok(())
    }

    async fn monitor_process(&self, name: String, mut child: tokio::process::Child) -> Result<()> {
        let services = self.services.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            let result = child.wait().await;

            let exit_code = match result {
                Ok(status) => status.code(),
                Err(e) => {
                    error!("Error waiting for service {}: {}", name, e);
                    Some(-1)
                }
            };

            let mut services = services.write().await;
            if let Some(state) = services.get_mut(&name) {
                state.status = if exit_code == Some(0) {
                    DaemonServiceStatus::Stopped
                } else {
                    DaemonServiceStatus::Failed
                };
                state.pid = None;
                state.exit_code = exit_code;
            }

            let _ = event_tx.send(DaemonEvent::ServiceStopped {
                name: name.clone(),
                exit_code,
            });

            info!("Service {} stopped with exit code {:?}", name, exit_code);
        });

        Ok(())
    }

    pub async fn stop_service(&self, name: &str, force: bool) -> Result<()> {
        let mut services = self.services.write().await;
        let state = services
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Service not found: {}", name))?;

        if matches!(
            state.status,
            DaemonServiceStatus::Stopped | DaemonServiceStatus::Stopping
        ) {
            return Ok(());
        }

        state.status = DaemonServiceStatus::Stopping;

        if let Some(pid) = state.pid {
            // Send SIGTERM
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            }

            if force {
                tokio::time::sleep(Duration::from_secs(5)).await;
                #[cfg(unix)]
                {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
                }
            }
        }

        state.status = DaemonServiceStatus::Stopped;
        state.pid = None;

        self.persist_state().await?;

        Ok(())
    }

    pub async fn restart_service(&self, name: &str) -> Result<()> {
        self.stop_service(name, true).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.start_service(name).await
    }

    pub async fn get_service_status(&self, name: &str) -> Result<Option<DaemonServiceState>> {
        let services = self.services.read().await;
        Ok(services.get(name).cloned())
    }

    pub async fn list_services(&self) -> Vec<DaemonServiceState> {
        let services = self.services.read().await;
        services.values().cloned().collect()
    }

    pub async fn get_daemon_status(&self) -> DaemonStatus {
        let services = self.services.read().await;
        let running = services
            .values()
            .filter(|s| matches!(s.status, DaemonServiceStatus::Running))
            .count();
        let stopped = services
            .values()
            .filter(|s| matches!(s.status, DaemonServiceStatus::Stopped))
            .count();
        let failed = services
            .values()
            .filter(|s| matches!(s.status, DaemonServiceStatus::Failed))
            .count();

        DaemonStatus {
            running: *self.running.read().await,
            total_services: services.len(),
            running_services: running,
            stopped_services: stopped,
            failed_services: failed,
            uptime_secs: 0, // Would track actual uptime
        }
    }

    async fn persist_state(&self) -> Result<()> {
        let services = self.services.read().await;
        let state_file = self.config.state_dir.join("services.json");
        let data = serde_json::to_vec_pretty(&*services)?;
        tokio::fs::create_dir_all(&self.config.state_dir).await?;
        tokio::fs::write(state_file, data).await?;
        Ok(())
    }

    async fn load_state(&self) -> Result<()> {
        let state_file = self.config.state_dir.join("services.json");
        if !state_file.exists() {
            return Ok(());
        }

        let data = tokio::fs::read(state_file).await?;
        let services: HashMap<String, DaemonServiceState> = serde_json::from_slice(&data)?;

        let mut services_lock = self.services.write().await;
        *services_lock = services;

        info!("Loaded {} persisted services", services_lock.len());
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        *self.running.write().await = false;

        let _ = self.event_tx.send(DaemonEvent::DaemonStopping);

        // Stop all services
        let services = self.services.read().await;
        let names: Vec<String> = services.keys().cloned().collect();
        drop(services);

        for name in names {
            let _ = self.stop_service(&name, true).await;
        }

        // Remove PID file
        let _ = tokio::fs::remove_file(&self.config.pid_file).await;

        // Remove socket
        let _ = tokio::fs::remove_file(&self.config.socket_path).await;

        let _ = self.event_tx.send(DaemonEvent::DaemonStopped);
        info!("Daemon runtime stopped");
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub total_services: usize,
    pub running_services: usize,
    pub stopped_services: usize,
    pub failed_services: usize,
    pub uptime_secs: u64,
}

/// The JSON shape `podnext list`/`service list`/`daemon status` read off the daemon:
/// `status` and `health` as the CLI's own `{:?}` names, so the daemon path and the
/// in-process fallback render the same bytes.
fn service_json(state: &DaemonServiceState) -> serde_json::Value {
    serde_json::json!({
        "name": state.name,
        "status": format!("{:?}", state.status),
        "health": format!("{:?}", state.health),
        "pid": state.pid,
        "restart_count": state.restart_count,
    })
}

fn services_json(states: &[DaemonServiceState]) -> serde_json::Value {
    serde_json::Value::Array(states.iter().map(service_json).collect())
}

/// Translate one wire request (`{"cmd": "...", ...}`, the same bytes the CLI writes)
/// into a [`DaemonMessage`]. `None` for a verb this daemon does not speak, which the
/// caller answers with a refusal naming it.
fn message_from_wire(request: &serde_json::Value) -> Option<DaemonMessage> {
    let cmd = request.get("cmd")?.as_str()?;
    let name = || {
        request
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Some(match cmd {
        "ping" => DaemonMessage::Ping,
        "list" => DaemonMessage::ListServices,
        "status" => DaemonMessage::GetServiceStatus { name: name() },
        "start" => DaemonMessage::StartService { name: name() },
        "stop" => DaemonMessage::StopService {
            name: name(),
            force: request
                .get("force")
                .and_then(|f| f.as_bool())
                .unwrap_or(false),
        },
        "restart" => DaemonMessage::RestartService { name: name() },
        "daemon-status" => DaemonMessage::GetDaemonStatus,
        "daemon-stop" => DaemonMessage::ShutdownDaemon,
        "daemon-reload" => DaemonMessage::ReloadConfig,
        "daemon-logs" => DaemonMessage::GetLogs {
            lines: request.get("lines").and_then(|l| l.as_u64()).unwrap_or(100) as usize,
        },
        _ => return None,
    })
}

/// Serve one IPC connection: read a single request line, ask the dispatcher for the
/// answer, write it back as one line and close. Any failure to parse or deliver is
/// reported to the client as an error response rather than dropped.
async fn serve_connection(
    stream: UnixStream,
    commands: mpsc::Sender<(DaemonMessage, mpsc::Sender<DaemonResponse>)>,
) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    let response = match reader.read_line(&mut line).await {
        Ok(0) => return, // client hung up without asking anything
        Ok(_) => match serde_json::from_str::<serde_json::Value>(line.trim()) {
            Ok(request) => match message_from_wire(&request) {
                Some(msg) => {
                    let (reply_tx, mut reply_rx) = mpsc::channel(1);
                    match commands.send((msg, reply_tx)).await {
                        Ok(()) => reply_rx
                            .recv()
                            .await
                            .unwrap_or_else(|| DaemonResponse::error("daemon dropped the request")),
                        Err(_) => DaemonResponse::error("daemon is shutting down"),
                    }
                }
                None => DaemonResponse::error(format!("unknown command: {}", line.trim())),
            },
            Err(e) => DaemonResponse::error(format!("invalid request: {e}")),
        },
        Err(_) => return,
    };

    let mut bytes = serde_json::to_vec(&response).unwrap_or_default();
    bytes.push(b'\n');
    let _ = write_half.write_all(&bytes).await;
    let _ = write_half.flush().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_daemon_runtime_creation() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonRuntimeConfig {
            socket_path: temp_dir.path().join("daemon.sock"),
            state_dir: temp_dir.path().join("state"),
            log_dir: temp_dir.path().join("logs"),
            pid_file: temp_dir.path().join("daemon.pid"),
            ..Default::default()
        };

        let runtime = DaemonRuntime::new(config);
        assert!(!*runtime.running.read().await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_service_registration() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonRuntimeConfig {
            socket_path: temp_dir.path().join("daemon.sock"),
            state_dir: temp_dir.path().join("state"),
            log_dir: temp_dir.path().join("logs"),
            pid_file: temp_dir.path().join("daemon.pid"),
            ..Default::default()
        };

        let runtime = DaemonRuntime::new(config);

        let service = ManagedService {
            name: "test-service".to_string(),
            command: vec!["/bin/sleep".to_string(), "100".to_string()],
            working_dir: None,
            environment: HashMap::new(),
            user: None,
            restart_policy: DaemonRestartPolicy::OnFailure,
            health_check: Some(DaemonHealthCheckConfig::default()),
            dependencies: vec![],
            metadata: HashMap::new(),
        };

        let result = runtime.register_service(service).await;
        assert!(result.is_ok());

        let services = runtime.list_services().await;
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "test-service");
    }

    /// A18(b): the IPC listener answers the CLI's requests. The base bound the socket
    /// and dropped the listener, and the command channel had no consumer — every
    /// request hung, so `daemon stop|reload|logs` could only print a success line. Each
    /// request below is the exact JSON the CLI's `send_daemon_command` writes; the
    /// answers are the exact bytes it decodes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ipc_answers_daemon_reload_logs_and_stop() {
        let temp_dir = tempdir().unwrap();
        let socket = temp_dir.path().join("daemon.sock");
        let config = DaemonRuntimeConfig {
            socket_path: socket.clone(),
            state_dir: temp_dir.path().join("state"),
            log_dir: temp_dir.path().join("logs"),
            pid_file: temp_dir.path().join("daemon.pid"),
            ..Default::default()
        };
        let runtime = Arc::new(DaemonRuntime::new(config));
        runtime.start().await.unwrap();
        assert!(
            socket.exists(),
            "the listener must bind the socket it watches"
        );

        let ask = |request: serde_json::Value| {
            let socket = socket.clone();
            async move {
                let stream = UnixStream::connect(&socket).await.expect("connect");
                let (read_half, mut write_half) = stream.into_split();
                write_half
                    .write_all(format!("{request}\n").as_bytes())
                    .await
                    .unwrap();
                write_half.flush().await.unwrap();
                let mut reader = BufReader::new(read_half);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                serde_json::from_str::<serde_json::Value>(line.trim()).unwrap()
            }
        };

        let reload = ask(serde_json::json!({"cmd": "daemon-reload"})).await;
        assert_eq!(reload["type"], "ok", "reload answer: {reload}");
        assert_eq!(reload["data"]["message"], "configuration reloaded");

        // This daemon keeps state, not a log stream, so the honest answer names the gap.
        let logs = ask(serde_json::json!({"cmd": "daemon-logs", "lines": 50})).await;
        assert_eq!(logs["type"], "error", "logs answer: {logs}");
        assert!(
            logs["message"]
                .as_str()
                .unwrap()
                .contains("no daemon log file"),
            "logs answer: {logs}"
        );

        // An unknown verb is refused, never answered as success.
        let unknown = ask(serde_json::json!({"cmd": "make-me-a-sandwich"})).await;
        assert_eq!(unknown["type"], "error", "unknown verb answer: {unknown}");

        let stop = ask(serde_json::json!({"cmd": "daemon-stop"})).await;
        assert_eq!(stop["type"], "ok", "stop answer: {stop}");
        assert_eq!(stop["data"]["message"], "daemon shutting down");
        assert!(!socket.exists(), "stop must remove the socket");
    }
}
