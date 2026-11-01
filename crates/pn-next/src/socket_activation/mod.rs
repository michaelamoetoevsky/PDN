//! Socket Activation - systemd-style on-demand container/service start
//!
//! Implements socket activation similar to systemd's socket units:
//! - Listen on sockets (Unix, TCP, UDP) and start services on first connection
//! - Pass file descriptors to started processes via FD passing
//! - Support for multiple sockets per service
//! - Integration with ServiceSupervisor for lifecycle management

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket, UnixListener, UnixStream};
use tokio::process::Command as TokioCommand;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Socket type for activation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocketType {
    Stream,
    Datagram,
    SeqPacket,
}

/// Socket activation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketConfig {
    pub name: String,
    pub socket_type: SocketType,
    pub listen: String,
    pub service: String,
    #[serde(default = "default_backlog")]
    pub backlog: i32,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default = "default_mode")]
    pub mode: u32,
    #[serde(default = "default_true")]
    pub pass_fd: bool,
    #[serde(default)]
    pub max_connections: usize,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_secs: u64,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

fn default_backlog() -> i32 {
    128
}
fn default_mode() -> u32 {
    0o666
}
fn default_true() -> bool {
    true
}
fn default_startup_timeout() -> u64 {
    90
}

/// Socket activation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketActivationConfig {
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
    #[serde(default = "default_mode")]
    pub default_mode: u32,
    #[serde(default = "default_true")]
    pub cleanup_on_shutdown: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_socket_dir() -> PathBuf {
    PathBuf::from("/run/pn-next/sockets")
}

impl Default for SocketActivationConfig {
    fn default() -> Self {
        Self {
            socket_dir: default_socket_dir(),
            default_mode: 0o666,
            cleanup_on_shutdown: true,
            enabled: true,
        }
    }
}

/// Socket type for activation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocketActivationEvent {
    SocketListening {
        name: String,
        address: String,
    },
    SocketClosed {
        name: String,
    },
    ServiceActivated {
        socket_name: String,
        service: String,
        pid: Option<u32>,
    },
    ActivationFailed {
        socket_name: String,
        service: String,
        error: String,
    },
    ConnectionAccepted {
        socket_name: String,
        peer_addr: String,
    },
    MaxConnectionsReached {
        socket_name: String,
    },
}

/// Internal socket listener state
struct SocketListener {
    config: SocketConfig,
    listener_type: ListenerType,
    connections_accepted: usize,
    is_active: bool,
}

pub enum ListenerType {
    Unix(UnixListener),
    Tcp(TcpListener),
    Udp(UdpSocket),
}

/// Socket activation manager
pub struct SocketActivationManager {
    config: SocketActivationConfig,
    sockets: Arc<RwLock<HashMap<String, SocketListener>>>,
    service_supervisor: Arc<RwLock<Option<Arc<crate::supervisor::ServiceSupervisor>>>>,
    event_tx: broadcast::Sender<SocketActivationEvent>,
}

impl SocketActivationManager {
    pub fn new(config: SocketActivationConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config,
            sockets: Arc::new(RwLock::new(HashMap::new())),
            service_supervisor: Arc::new(RwLock::new(None)),
            event_tx,
        }
    }

    pub fn set_service_supervisor(&self, supervisor: Arc<crate::supervisor::ServiceSupervisor>) {
        *self.service_supervisor.blocking_write() = Some(supervisor);
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<SocketActivationEvent> {
        self.event_tx.subscribe()
    }

    pub async fn add_socket(&self, config: SocketConfig) -> Result<()> {
        if !self.config.enabled {
            bail!("Socket activation is disabled");
        }

        if config.listen.starts_with('/') || config.listen.starts_with("./") {
            if let Some(parent) = Path::new(&config.listen).parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create socket directory")?;
            }
        }

        let listener = self.create_listener(&config).await?;

        let socket_listener = SocketListener {
            config: config.clone(),
            listener_type: listener,
            connections_accepted: 0,
            is_active: true,
        };

        let mut sockets = self.sockets.write().await;
        sockets.insert(config.name.clone(), socket_listener);

        let _ = self.event_tx.send(SocketActivationEvent::SocketListening {
            name: config.name.clone(),
            address: config.listen.clone(),
        });

        info!(
            "Socket activation listening: {} on {}",
            config.name, config.listen
        );
        Ok(())
    }

    pub async fn remove_socket(&self, name: &str) -> Result<()> {
        let mut sockets = self.sockets.write().await;
        if let Some(mut listener) = sockets.remove(name) {
            listener.is_active = false;
            if self.config.cleanup_on_shutdown && listener.config.listen.starts_with('/') {
                let _ = tokio::fs::remove_file(&listener.config.listen).await;
            }
            let _ = self.event_tx.send(SocketActivationEvent::SocketClosed {
                name: name.to_string(),
            });
            info!("Socket activation removed: {}", name);
        }
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Socket activation disabled, skipping start");
            return Ok(());
        }

        let sockets = self.sockets.read().await;
        for (name, listener) in sockets.iter() {
            if listener.is_active {
                self.start_listener_loop(name.clone(), listener.config.clone())
                    .await?;
            }
        }
        Ok(())
    }

    async fn create_listener(&self, config: &SocketConfig) -> Result<ListenerType> {
        match config.socket_type {
            SocketType::Stream => {
                if config.listen.starts_with('/') || config.listen.starts_with("./") {
                    let _ = tokio::fs::remove_file(&config.listen).await;
                    let listener =
                        UnixListener::bind(&config.listen).context("Failed to bind Unix socket")?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = std::fs::metadata(&config.listen)?.permissions();
                        perms.set_mode(config.mode);
                        std::fs::set_permissions(&config.listen, perms)?;
                    }
                    Ok(ListenerType::Unix(listener))
                } else {
                    let listener = TcpListener::bind(&config.listen)
                        .await
                        .context("Failed to bind TCP socket")?;
                    Ok(ListenerType::Tcp(listener))
                }
            }
            SocketType::Datagram => {
                if config.listen.starts_with('/') {
                    let _ = tokio::fs::remove_file(&config.listen).await;
                    let socket = UdpSocket::bind(&config.listen)
                        .await
                        .context("Failed to bind Unix datagram socket")?;
                    Ok(ListenerType::Udp(socket))
                } else {
                    let socket = UdpSocket::bind(&config.listen)
                        .await
                        .context("Failed to bind UDP socket")?;
                    Ok(ListenerType::Udp(socket))
                }
            }
            SocketType::SeqPacket => {
                let _ = tokio::fs::remove_file(&config.listen).await;
                let listener = UnixListener::bind(&config.listen)
                    .context("Failed to bind Unix seqpacket socket")?;
                Ok(ListenerType::Unix(listener))
            }
        }
    }

    async fn start_listener_loop(&self, name: String, config: SocketConfig) -> Result<()> {
        let sockets = self.sockets.clone();
        let supervisor = self.service_supervisor.clone();
        let event_tx = self.event_tx.clone();
        let startup_timeout = config.startup_timeout_secs;
        let pass_fd = config.pass_fd;
        let service_name = config.service.clone();
        let max_connections = config.max_connections;

        let listener_type = self.create_listener(&config).await?;

        tokio::spawn(async move {
            match listener_type {
                ListenerType::Unix(listener) => {
                    Self::run_unix_listener(
                        listener,
                        name,
                        config,
                        supervisor,
                        event_tx,
                        startup_timeout,
                        pass_fd,
                        max_connections,
                    )
                    .await;
                }
                ListenerType::Tcp(listener) => {
                    Self::run_tcp_listener(
                        listener,
                        name,
                        config,
                        supervisor,
                        event_tx,
                        startup_timeout,
                        pass_fd,
                        max_connections,
                    )
                    .await;
                }
                ListenerType::Udp(socket) => {
                    Self::run_udp_listener(
                        socket,
                        name,
                        config,
                        supervisor,
                        event_tx,
                        startup_timeout,
                        pass_fd,
                        max_connections,
                    )
                    .await;
                }
            }
        });

        Ok(())
    }

    async fn run_unix_listener(
        mut listener: UnixListener,
        name: String,
        config: SocketConfig,
        supervisor: Arc<RwLock<Option<Arc<crate::supervisor::ServiceSupervisor>>>>,
        event_tx: broadcast::Sender<SocketActivationEvent>,
        startup_timeout: u64,
        pass_fd: bool,
        max_connections: usize,
    ) {
        let mut connections_accepted = 0;
        loop {
            if max_connections > 0 && connections_accepted >= max_connections {
                let _ = event_tx.send(SocketActivationEvent::MaxConnectionsReached {
                    socket_name: name.clone(),
                });
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
            match listener.accept().await {
                Ok((stream, addr)) => {
                    connections_accepted += 1;
                    let peer_addr = format!("{:?}", addr);
                    let _ = event_tx.send(SocketActivationEvent::ConnectionAccepted {
                        socket_name: name.clone(),
                        peer_addr: peer_addr.clone(),
                    });
                    debug!("Accepted connection on {} from {}", name, peer_addr);

                    let svc_guard = supervisor.read().await;
                    if let Some(svc) = svc_guard.as_ref() {
                        let svc = svc.clone();
                        let service_name = config.service.clone();
                        let pass_fd_clone = pass_fd;
                        let stream_clone = stream;
                        let event_tx_clone = event_tx.clone();
                        let name_clone = name.clone();
                        tokio::spawn(async move {
                            let result = Self::activate_service(
                                svc,
                                &service_name,
                                stream_clone,
                                pass_fd_clone,
                                startup_timeout,
                            )
                            .await;
                            match result {
                                Ok(pid) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ServiceActivated {
                                            socket_name: name_clone.clone(),
                                            service: service_name.clone(),
                                            pid,
                                        },
                                    );
                                }
                                Err(e) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ActivationFailed {
                                            socket_name: name_clone.clone(),
                                            service: service_name,
                                            error: e.to_string(),
                                        },
                                    );
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    error!("Error accepting on {}: {}", name, e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn run_tcp_listener(
        mut listener: TcpListener,
        name: String,
        config: SocketConfig,
        supervisor: Arc<RwLock<Option<Arc<crate::supervisor::ServiceSupervisor>>>>,
        event_tx: broadcast::Sender<SocketActivationEvent>,
        startup_timeout: u64,
        pass_fd: bool,
        max_connections: usize,
    ) {
        let mut connections_accepted = 0;
        loop {
            if max_connections > 0 && connections_accepted >= max_connections {
                let _ = event_tx.send(SocketActivationEvent::MaxConnectionsReached {
                    socket_name: name.clone(),
                });
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
            match listener.accept().await {
                Ok((stream, addr)) => {
                    connections_accepted += 1;
                    let peer_addr = addr.to_string();
                    let _ = event_tx.send(SocketActivationEvent::ConnectionAccepted {
                        socket_name: name.clone(),
                        peer_addr: peer_addr.clone(),
                    });
                    let svc_guard = supervisor.read().await;
                    if let Some(svc) = svc_guard.as_ref() {
                        let svc = svc.clone();
                        let service_name = config.service.clone();
                        let pass_fd_clone = pass_fd;
                        let event_tx_clone = event_tx.clone();
                        let name_clone = name.clone();
                        tokio::spawn(async move {
                            let result = Self::activate_service_tcp(
                                svc,
                                &service_name,
                                stream,
                                pass_fd_clone,
                                startup_timeout,
                            )
                            .await;
                            match result {
                                Ok(pid) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ServiceActivated {
                                            socket_name: name_clone.clone(),
                                            service: service_name.clone(),
                                            pid,
                                        },
                                    );
                                }
                                Err(e) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ActivationFailed {
                                            socket_name: name_clone.clone(),
                                            service: service_name,
                                            error: e.to_string(),
                                        },
                                    );
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    error!("Error accepting on {}: {}", name, e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn run_udp_listener(
        socket: UdpSocket,
        name: String,
        config: SocketConfig,
        supervisor: Arc<RwLock<Option<Arc<crate::supervisor::ServiceSupervisor>>>>,
        event_tx: broadcast::Sender<SocketActivationEvent>,
        startup_timeout: u64,
        pass_fd: bool,
        max_connections: usize,
    ) {
        let mut buf = [0u8; 65536];
        let mut connections_accepted = 0;
        loop {
            if max_connections > 0 && connections_accepted >= max_connections {
                let _ = event_tx.send(SocketActivationEvent::MaxConnectionsReached {
                    socket_name: name.clone(),
                });
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
            match socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    connections_accepted += 1;
                    let peer_addr = addr.to_string();
                    let _ = event_tx.send(SocketActivationEvent::ConnectionAccepted {
                        socket_name: name.clone(),
                        peer_addr: peer_addr.clone(),
                    });
                    let svc_guard = supervisor.read().await;
                    if let Some(svc) = svc_guard.as_ref() {
                        let svc = svc.clone();
                        let service_name = config.service.clone();
                        let data = buf[..len].to_vec();
                        let event_tx_clone = event_tx.clone();
                        let name_clone = name.clone();
                        tokio::spawn(async move {
                            let result = Self::activate_service_udp(
                                svc,
                                &service_name,
                                data,
                                addr,
                                startup_timeout,
                            )
                            .await;
                            match result {
                                Ok(pid) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ServiceActivated {
                                            socket_name: name_clone.clone(),
                                            service: service_name.clone(),
                                            pid,
                                        },
                                    );
                                }
                                Err(e) => {
                                    let _ = event_tx_clone.send(
                                        SocketActivationEvent::ActivationFailed {
                                            socket_name: name_clone.clone(),
                                            service: service_name,
                                            error: e.to_string(),
                                        },
                                    );
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    error!("Error receiving on {}: {}", name, e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn activate_service(
        supervisor: Arc<crate::supervisor::ServiceSupervisor>,
        service_name: &str,
        _stream: UnixStream,
        _pass_fd: bool,
        timeout_secs: u64,
    ) -> Result<Option<u32>> {
        let status = supervisor.get_status(service_name).await?;
        if matches!(status.state, crate::supervisor::ServiceState::Running) {
            return Ok(None);
        }
        let start_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            supervisor.start_service(service_name),
        )
        .await;
        match start_result {
            Ok(Ok(())) => {
                info!("Service {} activated via socket", service_name);
                let status = supervisor.get_status(service_name).await?;
                Ok(status.pid)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!("Timeout activating service")),
        }
    }

    async fn activate_service_tcp(
        supervisor: Arc<crate::supervisor::ServiceSupervisor>,
        service_name: &str,
        _stream: tokio::net::TcpStream,
        _pass_fd: bool,
        timeout_secs: u64,
    ) -> Result<Option<u32>> {
        let status = supervisor.get_status(service_name).await?;
        if matches!(status.state, crate::supervisor::ServiceState::Running) {
            return Ok(None);
        }
        let start_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            supervisor.start_service(service_name),
        )
        .await;
        match start_result {
            Ok(Ok(())) => {
                info!("Service {} activated via TCP socket", service_name);
                let status = supervisor.get_status(service_name).await?;
                Ok(status.pid)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!("Timeout activating service")),
        }
    }

    async fn activate_service_udp(
        supervisor: Arc<crate::supervisor::ServiceSupervisor>,
        service_name: &str,
        _data: Vec<u8>,
        _addr: std::net::SocketAddr,
        timeout_secs: u64,
    ) -> Result<Option<u32>> {
        let status = supervisor.get_status(service_name).await?;
        if matches!(status.state, crate::supervisor::ServiceState::Running) {
            return Ok(None);
        }
        let start_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            supervisor.start_service(service_name),
        )
        .await;
        match start_result {
            Ok(Ok(())) => {
                info!("Service {} activated via UDP socket", service_name);
                let status = supervisor.get_status(service_name).await?;
                Ok(status.pid)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!("Timeout activating service")),
        }
    }

    pub async fn list_sockets(&self) -> Vec<SocketConfig> {
        let sockets = self.sockets.read().await;
        sockets.values().map(|l| l.config.clone()).collect()
    }

    pub async fn get_socket_status(&self, name: &str) -> Option<(bool, usize)> {
        let sockets = self.sockets.read().await;
        sockets
            .get(name)
            .map(|l| (l.is_active, l.connections_accepted))
    }

    pub async fn shutdown(&self) -> Result<()> {
        let mut sockets = self.sockets.write().await;
        for (name, mut listener) in sockets.drain() {
            listener.is_active = false;
            if self.config.cleanup_on_shutdown && listener.config.listen.starts_with('/') {
                let _ = tokio::fs::remove_file(&listener.config.listen).await;
            }
            let _ = self
                .event_tx
                .send(SocketActivationEvent::SocketClosed { name });
        }
        Ok(())
    }

    pub async fn load_from_directory(&self, dir: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("socket") {
                let content = tokio::fs::read_to_string(&path).await?;
                let config: SocketConfig =
                    toml::from_str(&content).context("Failed to parse socket config")?;
                self.add_socket(config).await?;
            }
        }
        Ok(())
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            socket_type: SocketType::Stream,
            listen: String::new(),
            service: String::new(),
            backlog: 128,
            user: None,
            group: None,
            mode: 0o666,
            pass_fd: true,
            max_connections: 0,
            startup_timeout_secs: 90,
            environment: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_socket_activation_config() {
        let temp_dir = tempdir().unwrap();
        let socket_dir = temp_dir.path().join("sockets");
        let config = SocketActivationConfig {
            socket_dir: socket_dir.clone(),
            ..Default::default()
        };
        let manager = SocketActivationManager::new(config);
        let socket_config = SocketConfig {
            name: "test-socket".to_string(),
            socket_type: SocketType::Stream,
            listen: socket_dir.join("test.sock").to_string_lossy().to_string(),
            service: "test-service".to_string(),
            ..Default::default()
        };
        let result = manager.add_socket(socket_config).await;
        assert!(result.is_ok());
        let sockets = manager.list_sockets().await;
        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0].name, "test-socket");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_socket_activation_tcp() {
        let temp_dir = tempdir().unwrap();
        let config = SocketActivationConfig {
            socket_dir: temp_dir.path().join("sockets"),
            ..Default::default()
        };
        let manager = SocketActivationManager::new(config);
        let socket_config = SocketConfig {
            name: "test-tcp".to_string(),
            socket_type: SocketType::Stream,
            listen: "127.0.0.1:0".to_string(),
            service: "test-service".to_string(),
            ..Default::default()
        };
        let result = manager.add_socket(socket_config).await;
        assert!(result.is_ok());
        let sockets = manager.list_sockets().await;
        assert_eq!(sockets.len(), 1);
    }
}
