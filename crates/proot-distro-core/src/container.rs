//! Container management for PRoot Distro
//!
//! Provides container lifecycle management with proot-core integration.

use crate::gpu::GpuConfig;
use crate::locking::LockManager;
use crate::paths::ContainerPaths;
use crate::runtime::{ProotBackend, RuntimeBackend, RuntimeKind, UmlBackend};
use anyhow::Result;
use chrono::{DateTime, Utc};
use proot_core::{ProotConfig, ProotEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Container manager
pub struct ContainerManager {
    containers: Arc<Mutex<HashMap<String, Container>>>,
    paths: ContainerPaths,
    lock_manager: Arc<LockManager>,
    proot_engine: Arc<Mutex<Option<ProotEngine>>>,
}

impl ContainerManager {
    pub fn new() -> Result<Self> {
        let manager = Self {
            containers: Arc::new(Mutex::new(HashMap::new())),
            paths: ContainerPaths::new()?,
            lock_manager: Arc::new(LockManager::new()?),
            proot_engine: Arc::new(Mutex::new(None)),
        };

        // Load existing containers from disk
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(manager.load_containers())
        })?;

        Ok(manager)
    }

    /// Load existing containers from disk, inspecting metadata.json and /etc/os-release
    async fn load_containers(&self) -> Result<()> {
        let containers_dir = self.paths.containers_dir.clone();
        if !containers_dir.exists() {
            return Ok(());
        }
        let cache_path = self
            .paths
            .cache_dir
            .join(crate::container_index::INDEX_FILE);

        let mut containers = self.containers.lock().await;

        // Fast path: a current index is served without reading a single metadata
        // file. A stale or absent one falls through to the full scan, which then
        // rewrites the index. Serving stale data would be worse than no cache, so
        // the cache is keyed on the mtime+size of every metadata file (see
        // `container_index`).
        if crate::container_index::enabled() {
            if let Some(fresh) = crate::container_index::load_if_fresh(&containers_dir, &cache_path)
            {
                for c in fresh {
                    containers.insert(c.name.clone(), c);
                }
                return Ok(());
            }
        }

        let loaded = scan_store(&containers_dir)?;
        for c in &loaded {
            containers.insert(c.name.clone(), c.clone());
        }
        if crate::container_index::enabled() {
            // A refused index write (B11's `ESTALE` identity gate included) must reach
            // the caller rather than being swallowed: silently leaving a stale index
            // behind is the failure this project has already paid for twice.
            crate::container_index::store(&containers_dir, &cache_path, &loaded)?;
        }

        Ok(())
    }

    async fn get_engine(&self) -> Result<Arc<Mutex<ProotEngine>>> {
        let mut engine_guard = self.proot_engine.lock().await;
        if engine_guard.is_none() {
            let config = ProotConfig::default();
            *engine_guard = Some(ProotEngine::new(config)?);
        }
        // Return a new Arc<Mutex<ProotEngine>> wrapping the same engine
        let engine = engine_guard.as_ref().unwrap();
        Ok(Arc::new(Mutex::new(engine.clone())))
    }

    pub async fn create(&self, name: &str, config: &ContainerConfig) -> Result<Container> {
        let mut containers = self.containers.lock().await;

        if containers.contains_key(name) {
            return Err(anyhow::anyhow!("Container '{}' already exists", name));
        }

        let container = Container::new(name.to_string(), config.clone())?;
        containers.insert(name.to_string(), container.clone());

        Ok(container)
    }

    pub async fn get(&self, name: &str) -> Result<Container> {
        let containers = self.containers.lock().await;
        containers.get(name).cloned().ok_or_else(|| {
            anyhow::anyhow!("Container '{}' not found", crate::names::escape_name(name))
        })
    }

    pub async fn remove(&self, name: &str) -> Result<()> {
        let mut containers = self.containers.lock().await;
        containers.remove(name).ok_or_else(|| {
            anyhow::anyhow!("Container '{}' not found", crate::names::escape_name(name))
        })?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<ContainerInfo>> {
        let containers = self.containers.lock().await;
        Ok(containers.values().map(|c| c.info()).collect())
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        // For now, return empty - sessions are managed by SessionManager
        Ok(Vec::new())
    }

    pub async fn kill(&self, name: &str) -> Result<()> {
        let mut found_session = false;
        // Terminate matching active disk sessions via SIGQUIT (graceful) then SIGKILL
        if let Ok(active) = crate::session::active_sessions() {
            for s in active {
                if s.container_name == name || name == "all" || name == "--all" {
                    // Liveness came from the descriptor (C8); the pid is only an
                    // address, and a recycled one belongs to someone else.
                    if crate::session::signal_target(&s).is_none() {
                        continue;
                    }
                    let _ = crate::session::kill_session(s.pid, libc::SIGQUIT);
                    found_session = true;
                }
            }
        }

        let mut containers = self.containers.lock().await;
        if name == "all" || name == "--all" {
            for container in containers.values_mut() {
                if let Some(pid) = container.pid {
                    let _ = terminate_group(pid, KILL_TIMEOUT_SECS).await;
                    container.pid = None;
                }
                container.status = ContainerStatus::Stopped;
            }
            Ok(())
        } else if let Some(container) = containers.get_mut(name) {
            if let Some(pid) = container.pid {
                // Only claim Stopped once the whole group is verifiably gone.
                if let Err(e) = terminate_group(pid, KILL_TIMEOUT_SECS).await {
                    container.status = ContainerStatus::Error;
                    return Err(e);
                }
                container.pid = None;
            }
            container.status = ContainerStatus::Stopped;
            Ok(())
        } else if name == "all" || name == "--all" || found_session {
            Ok(())
        } else {
            anyhow::bail!("Container '{}' not found", name);
        }
    }

    pub async fn start(&self, name: &str) -> Result<()> {
        let mut containers = self.containers.lock().await;
        let container = containers
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Container '{}' not found", name))?;

        if container.status == ContainerStatus::Running {
            return Err(anyhow::anyhow!("Container '{}' is already running", name));
        }

        // Initialize proot engine if needed
        let engine = self.get_engine().await?;
        {
            let mut engine_guard = engine.lock().await;

            // Configure proot from container config
            engine_guard.config.rootfs = Some(container.rootfs_path.clone());
            engine_guard.config.bind = container
                .config
                .bind_mounts
                .iter()
                // `to_argument` refuses a source PRoot cannot express, instead of
                // handing it a string it would silently drop (#35).
                .map(|b| b.to_argument())
                .collect::<anyhow::Result<Vec<String>>>()?;

            // Same fake /proc stubs as the interactive path (see execute).
            let _ = crate::network::ensure_resolv_conf(&container.rootfs_path);
            if let Some(container_dir) = container.rootfs_path.parent() {
                // The synthetic binds below are derived from this directory, so a
                // colon anywhere in it would silently lose /dev/shm and the /proc
                // stubs (#35). Fail loudly before that can happen.
                crate::bindings::ensure_bindable(&container_dir.to_string_lossy())?;
                if crate::sysdata::ensure(container_dir).is_ok() {
                    engine_guard
                        .config
                        .bind
                        .extend(crate::sysdata::bindings(container_dir));
                }
                if crate::guestdirs::ensure(container_dir).is_ok() {
                    engine_guard
                        .config
                        .bind
                        .extend(crate::guestdirs::bindings(container_dir));
                }
            }
            // The Android bind set: same decision as the interactive path (see
            // `android_binds`), so a started container is not bound differently
            // from a `login`.
            crate::android_binds::extend_binds(
                &mut engine_guard.config.bind,
                &container.rootfs_path,
                container.config.isolated,
            );
            engine_guard.config.cwd = Some(
                container
                    .config
                    .working_dir
                    .clone()
                    .unwrap_or_else(|| "/".to_string())
                    .into(),
            );
            engine_guard.config.env = container
                .config
                .env_vars
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            engine_guard.config.command = if container.config.command.is_empty() {
                vec!["/bin/sh".to_string()]
            } else {
                container.config.command.clone()
            };
            // A profile recorded in the container metadata cannot be enforced (issue #60): no BPF
            // filter is built, so refuse instead of starting the container with the field dropped.
            crate::security::assert_seccomp_profile_supported(
                &container.config.security.seccomp_profile,
            )?;
            engine_guard.config.seccomp = false;
            engine_guard.config.kernel_release = container.config.kernel_release.clone();
            engine_guard.config.hostname = container.config.hostname.clone();
        }

        // Start the container
        let pid = engine.lock().await.start().await?;

        container.status = ContainerStatus::Running;
        container.pid = Some(pid);

        Ok(())
    }

    pub async fn stop(&self, name: &str, timeout: u64) -> Result<()> {
        let mut containers = self.containers.lock().await;
        let container = containers
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Container '{}' not found", name))?;

        if container.status != ContainerStatus::Running {
            return Err(anyhow::anyhow!("Container '{}' is not running", name));
        }

        if let Some(pid) = container.pid {
            // SIGTERM the group, escalate to SIGKILL, and verify. A survivor is
            // reported as an error rather than silently marked Stopped.
            if let Err(e) = terminate_group(pid, timeout).await {
                container.status = ContainerStatus::Error;
                return Err(e);
            }
        }

        container.status = ContainerStatus::Stopped;
        container.pid = None;

        Ok(())
    }

    pub async fn restart(&self, name: &str, timeout: u64) -> Result<()> {
        self.stop(name, timeout).await?;
        self.start(name).await
    }

    /// Returns the execution backend corresponding to container's configured runtime
    pub fn get_backend(&self, container: &Container) -> Box<dyn RuntimeBackend> {
        match container.config.runtime {
            RuntimeKind::Proot => Box::new(ProotBackend::with_engine(self.proot_engine.clone())),
            RuntimeKind::Uml => Box::new(UmlBackend::new()),
        }
    }

    /// Execute a command in the container and wait for completion (foreground execution)
    pub async fn execute(&self, name: &str, command: Vec<String>) -> Result<i32> {
        let container = self.get(name).await?;
        let backend = self.get_backend(&container);
        backend.execute(&container, command).await
    }

    pub async fn execute_container(
        &self,
        container: &Container,
        command: Vec<String>,
    ) -> Result<i32> {
        let backend = self.get_backend(container);
        backend.execute(container, command).await
    }

    pub fn backup(&self, _container: &Container, _dest: &Path) -> Result<()> {
        // Implementation for backing up container
        Ok(())
    }

    pub fn restore(&mut self, _name: &str, _src: &Path) -> Result<()> {
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
    pub command: Vec<String>,
    pub working_dir: Option<String>,
    pub kernel_release: Option<String>,
    pub hostname: Option<String>,
    pub resources: ResourceLimits,
    pub network: NetworkConfig,
    pub security: SecurityConfig,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub gpu: Option<GpuConfig>,
    #[serde(default)]
    pub runtime: RuntimeKind,
    #[serde(default)]
    pub isolated: bool,
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
            command: Vec::new(),
            working_dir: None,
            kernel_release: None,
            hostname: None,
            resources: ResourceLimits::default(),
            network: NetworkConfig::default(),
            security: SecurityConfig::default(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            gpu: Some(GpuConfig::default()),
            runtime: RuntimeKind::default(),
            isolated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub source: String,
    pub target: String,
    pub readonly: bool,
}

impl BindMount {
    /// Render this mount as the `--bind` argument PRoot expects.
    ///
    /// A source containing ':' cannot be expressed and would be silently dropped
    /// by PRoot, taking `/dev/shm` and the `/proc` stubs with it (#35), so it is
    /// an error here rather than a warning nobody reads. A read-only mount is an
    /// error too: see [`crate::bindings::argument`] for the measurement.
    pub fn to_argument(&self) -> anyhow::Result<String> {
        crate::bindings::argument(&self.source, &self.target, self.readonly)
    }
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
    /// Set when the directory is present but its `metadata.json` is not a regular
    /// file that can be read; the value names the offending file type (WORKLIST A1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrupt: Option<String>,
}

impl Container {
    pub fn new(name: String, config: ContainerConfig) -> Result<Self> {
        let rootfs_path = config.rootfs_path.clone().unwrap_or_else(|| {
            // A19: the store root has exactly one resolver. Composing it from
            // `dirs::home_dir()` here ignored `PN_TEST_ROOT`, so a redirected
            // store still placed rootfs paths under the operator's real home.
            crate::paths::resolve_home()
                .unwrap_or_else(|_| PathBuf::from("/home/user"))
                .join(".pn-next")
                .join("containers")
                .join(&name)
        });

        Ok(Self {
            name,
            config,
            created_at: Utc::now(),
            status: ContainerStatus::Stopped,
            pid: None,
            rootfs_path,
            corrupt: None,
        })
    }
}

impl Default for Container {
    fn default() -> Self {
        Self {
            name: String::new(),
            config: ContainerConfig::default(),
            created_at: Utc::now(),
            status: ContainerStatus::Stopped,
            pid: None,
            rootfs_path: PathBuf::new(),
            corrupt: None,
        }
    }
}

impl Container {
    pub fn info(&self) -> ContainerInfo {
        ContainerInfo {
            name: self.name.clone(),
            distro: self.config.distro.clone(),
            version: self.config.version.clone(),
            arch: self.config.arch.clone(),
            status: self.status,
            created_at: self.created_at,
            rootfs_path: self.rootfs_path.clone(),
            corrupt: self.corrupt.clone(),
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
    /// See [`Container::corrupt`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrupt: Option<String>,
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

/// Detect Linux distribution ID and Version from rootfs (/etc/os-release or /usr/lib/os-release)
fn detect_os_release(rootfs: &Path) -> Option<(String, String)> {
    let candidates = [
        rootfs.join("etc/os-release"),
        rootfs.join("usr/lib/os-release"),
    ];

    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            let mut id = None;
            let mut version_id = None;

            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if k == "ID" && id.is_none() {
                        id = Some(v.to_string());
                    } else if (k == "VERSION_ID" || k == "VERSION_CODENAME") && version_id.is_none()
                    {
                        version_id = Some(v.to_string());
                    }
                }
            }

            if let Some(id) = id {
                return Some((id, version_id.unwrap_or_else(|| "rolling".to_string())));
            }
        }
    }
    None
}

/// How long `kill()` waits for a group to die before escalating / giving up.
const KILL_TIMEOUT_SECS: u64 = 10;

/// Signal a whole process group and only return once it is verifiably gone.
///
/// proot traces the guest's descendants, so signalling the leader pid alone
/// orphans the tracees that hold the guest filesystem. The child was spawned
/// with `process_group(0)`, so its pgid equals its pid and `kill(-pgid, ..)`
/// reaches every descendant.
///
/// `kill(pid, 0)` is NOT a liveness test — it also succeeds for a zombie, which
/// made the old loop sleep the entire timeout and then SIGKILL a corpse. We reap
/// with waitpid and read process state from /proc instead.
async fn terminate_group(pgid: u32, timeout: u64) -> Result<()> {
    let group = -(pgid as i32);
    // SIGTERM the group (falls back to the bare pid for a non-leader/gone group;
    // ESRCH just means it already vanished and the poll below returns at once).
    unsafe { libc::kill(group, libc::SIGTERM) };

    if !wait_group_gone(pgid, timeout).await {
        unsafe { libc::kill(group, libc::SIGKILL) };
        if !wait_group_gone(pgid, timeout).await {
            anyhow::bail!("process group {} survived SIGKILL", pgid);
        }
    }
    Ok(())
}

/// Poll until the group has no live (non-zombie) member, up to `timeout` secs.
async fn wait_group_gone(pgid: u32, timeout: u64) -> bool {
    let start = std::time::Instant::now();
    loop {
        if !group_alive(pgid) {
            return true;
        }
        if start.elapsed().as_secs() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Is process group `pgid` still running (zombies excluded)?
fn group_alive(pgid: u32) -> bool {
    // Reap the leader if we are its parent and it already exited, so a zombie
    // does not read as alive.
    unsafe { libc::waitpid(pgid as i32, std::ptr::null_mut(), libc::WNOHANG) };
    live_pgrps().contains(&(pgid as i32))
}

/// pgrp of every live (non-zombie) process, parsed from /proc/<pid>/stat.
fn live_pgrps() -> Vec<i32> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", name)) else {
            continue;
        };
        // field 2 (comm) may contain spaces/parens; fields resume after the last ')'.
        // Fields: state ppid pgrp session ...
        let Some(close) = stat.rfind(')') else {
            continue;
        };
        let mut fields = stat[close + 1..].split_whitespace();
        let state = fields.next().unwrap_or("");
        if state.starts_with('Z') {
            continue;
        }
        let _ppid = fields.next();
        if let Some(pgrp) = fields.next().and_then(|s| s.parse::<i32>().ok()) {
            out.push(pgrp);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    fn count_group(pgid: u32) -> usize {
        live_pgrps()
            .into_iter()
            .filter(|p| *p == pgid as i32)
            .count()
    }

    /// A container whose process tree forks children must leave NOTHING behind
    /// after kill(). Observable assertion: no pid is left in the group.
    #[tokio::test]
    async fn kill_leaves_no_surviving_descendant() {
        // Stand-in for proot: a shell that forks two long sleepers and waits.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 300 & sleep 300 & wait")
            .process_group(0)
            .spawn()
            .expect("spawn sh");
        let pgid = child.id();

        // Wait (bounded) until the descendants have actually forked.
        let mut members = 0;
        for _ in 0..100 {
            members = count_group(pgid);
            if members >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            members >= 2,
            "expected >=2 descendants in group {pgid}, saw {members}"
        );

        terminate_group(pgid, 5).await.expect("terminate group");

        let left = count_group(pgid);
        assert_eq!(
            left, 0,
            "{left} descendant(s) survived kill in group {pgid}"
        );
        let _ = child.wait(); // reap our own handle if not already reaped
    }

    /// A zombie satisfies kill(pid, 0), so the old wait loop could not break
    /// early and burned the full timeout. The new predicate reports it gone.
    #[test]
    fn zombie_is_not_reported_alive() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .process_group(0)
            .spawn()
            .expect("spawn sh");
        let pid = child.id();

        // Wait until it has exited but is not reaped: a zombie.
        for _ in 0..200 {
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(close) = stat.rfind(')') {
                    if stat[close + 1..].trim_start().starts_with('Z') {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        // Observable fact 1: the OLD liveness test calls a zombie "alive".
        assert_eq!(
            unsafe { libc::kill(pid as i32, 0) },
            0,
            "a zombie should satisfy kill(pid, 0)"
        );

        // Evidence (printed, never asserted): the old loop cannot break on a
        // zombie, so it runs the whole timeout; the new predicate returns now.
        let timeout = 1u64;
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < timeout {
            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let old_elapsed = t0.elapsed();

        // Observable fact 2: the NEW predicate reaps/ignores zombies.
        let t1 = std::time::Instant::now();
        let alive = group_alive(pid);
        let new_elapsed = t1.elapsed();
        assert!(!alive, "a zombie must not count as a live process");

        eprintln!(
            "zombie pid {pid}: old kill(pid,0) loop = {old_elapsed:?} (full {timeout}s timeout), new predicate = {new_elapsed:?}"
        );
        let _ = child.wait();
    }
}

/// Read the whole container store from disk: `read_dir` plus a full read of every
/// container's `metadata.json` and `/etc/os-release`.
///
/// Extracted from `ContainerManager::load_containers` so the index cache can wrap
/// it: this is the uncached algorithm, and the cache must return exactly what this
/// returns.
pub fn scan_store(containers_dir: &Path) -> Result<Vec<Container>> {
    let mut out = Vec::new();

    for entry in std::fs::read_dir(containers_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let rootfs_path = entry.path().join("rootfs");
        if !rootfs_path.is_dir() {
            continue;
        }

        // Only consider directories that describe a container (see
        // `is_container_dir`: probing for etc/bin/usr alone missed every
        // legitimate `FROM scratch` image a build had just reported as
        // runnable, so `run`/`list`/`remove` could not see it at all).
        if !is_container_dir(&entry.path()) {
            continue;
        }

        let mut meta_distro = None;
        let mut meta_version = None;
        let mut created_at = None;
        let mut corrupt = None;

        // Classify before reading: a `metadata.json` that is not a regular file
        // is reported, never opened, so it can never block this scan (WORKLIST A1).
        let (mut metadata_file, kind) = open_metadata(&entry.path());
        match kind {
            MetadataKind::Regular => {
                let mut metadata_str = String::new();
                if let Some(file) = metadata_file.as_mut() {
                    if std::io::Read::read_to_string(file, &mut metadata_str).is_ok() {
                        if let Ok(metadata) =
                            serde_json::from_str::<serde_json::Value>(&metadata_str)
                        {
                            meta_distro = metadata
                                .get("image")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            meta_version = metadata
                                .get("tag")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            if let Some(ts_str) =
                                metadata.get("created_at").and_then(|v| v.as_str())
                            {
                                if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
                                    created_at = Some(dt.with_timezone(&Utc));
                                }
                            }
                        }
                    }
                }
            }
            MetadataKind::Hostile(kind) => {
                corrupt = Some(format!(
                    "metadata.json is not a regular file (found: {kind})"
                ));
            }
            MetadataKind::Absent => {}
        }

        // Inspect /etc/os-release in rootfs so DISTRO matches the real Linux distribution
        // (e.g. debian, ubuntu, alpine) rather than arbitrary container/target names.
        let (distro, version) = if let Some((os_id, os_ver)) = detect_os_release(&rootfs_path) {
            (os_id, os_ver)
        } else {
            (
                meta_distro.unwrap_or_else(|| "unknown".to_string()),
                meta_version.unwrap_or_else(|| "latest".to_string()),
            )
        };

        let config = ContainerConfig {
            name: name.clone(),
            distro,
            version,
            arch: std::env::consts::ARCH.to_string(),
            rootfs_path: Some(rootfs_path.clone()),
            ..Default::default()
        };

        if let Ok(mut container) = Container::new(name.clone(), config) {
            if let Some(created) = created_at {
                container.created_at = created;
            } else if let Ok(dir_meta) = entry.metadata() {
                if let Ok(ctime) = dir_meta.modified() {
                    container.created_at = DateTime::<Utc>::from(ctime);
                }
            }
            // A corrupt entry is still a container in the store: it is listed
            // (so `remove` can find and clear it) but flagged, never hidden.
            if corrupt.is_some() {
                container.status = ContainerStatus::Error;
            }
            container.corrupt = corrupt;
            out.push(container);
        }
    }

    Ok(out)
}

/// The prefix the install path gives a staging directory
/// (`native/image.rs`: `format!(".tmp-install-{name}-{uuid}")`). A staging
/// directory is a container being assembled, not a container: it must never be
/// listed or `remove`d as one (WORKLIST A3a).
pub const STAGING_PREFIX: &str = ".tmp-install-";

/// The kind of thing sitting at a container's `metadata.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    /// A regular file: safe to open and read.
    Regular,
    /// Not a regular file; the payload names the offending type (`"fifo"`,
    /// `"symlink"`, `"directory"`, ...). Never opened for reading, so it cannot
    /// block the caller (WORKLIST A1).
    Hostile(&'static str),
    /// Nothing there: a legacy container, which is allowed.
    Absent,
}

/// Classify `<dir>/metadata.json` without ever blocking on it.
///
/// A FIFO `metadata.json` used to stall `list`/`remove` forever: `read_to_string`
/// blocks in `openat` waiting for a writer, and that wait outlived its caller
/// (two `podnext list` orphans were still alive at PPID 1 after >2700 s). The
/// file is opened with `O_NONBLOCK | O_NOFOLLOW` and the descriptor is then
/// `fstat`ed, so any non-regular file is named and skipped without a read.
pub fn metadata_kind(dir: &Path) -> MetadataKind {
    open_metadata(dir).1
}

/// Open `<dir>/metadata.json` if — and only if — it is a regular file, returning
/// the open descriptor and its classified kind.
fn open_metadata(dir: &Path) -> (Option<std::fs::File>, MetadataKind) {
    use std::os::unix::ffi::OsStrExt;
    let path = dir.join("metadata.json");
    let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return (None, MetadataKind::Absent);
    };
    let fd = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) | Some(libc::ENOTDIR) => (None, MetadataKind::Absent),
            // `O_NOFOLLOW` rejects a symlink on the final component with ELOOP.
            Some(libc::ELOOP) => (None, MetadataKind::Hostile("symlink")),
            _ => (None, MetadataKind::Hostile("unreadable")),
        };
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        unsafe { libc::close(fd) };
        return (None, MetadataKind::Hostile("unreadable"));
    }
    let kind = match st.st_mode & libc::S_IFMT {
        libc::S_IFREG => MetadataKind::Regular,
        libc::S_IFIFO => MetadataKind::Hostile("fifo"),
        libc::S_IFDIR => MetadataKind::Hostile("directory"),
        libc::S_IFSOCK => MetadataKind::Hostile("socket"),
        libc::S_IFCHR => MetadataKind::Hostile("character device"),
        libc::S_IFBLK => MetadataKind::Hostile("block device"),
        libc::S_IFLNK => MetadataKind::Hostile("symlink"),
        _ => MetadataKind::Hostile("special file"),
    };
    if kind != MetadataKind::Regular {
        unsafe { libc::close(fd) };
        return (None, kind);
    }
    use std::os::unix::io::FromRawFd;
    (Some(unsafe { std::fs::File::from_raw_fd(fd) }), kind)
}

/// Whether a directory in the container store describes a container.
///
/// The store is keyed by directory, and a directory is only a container if it has
/// a rootfs. The original test for that was "has `etc`, `bin` or `usr`", which is
/// a guess about the *contents* of an image and is wrong for a legitimate
/// `FROM scratch` build: `podnext build local` reported success, printed
/// "Run it with: podnext run <name>", and then `run`, `list` and `remove` could
/// not find the container at all, because its rootfs held none of those three
/// directories.
///
/// A successful build records `metadata.json`, so that file is the marker. The
/// old probe is kept as a fallback for containers that predate it or were placed
/// there by other means.
///
/// Two exclusions: a staging directory ([`STAGING_PREFIX`]) is never a container,
/// and a `metadata.json` that exists but is not a regular file still marks the
/// directory — as a corrupt container to be reported and cleared, not read.
pub fn is_container_dir(dir: &std::path::Path) -> bool {
    if dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(STAGING_PREFIX))
    {
        return false;
    }
    let rootfs = dir.join("rootfs");
    if !rootfs.is_dir() {
        return false;
    }
    match metadata_kind(dir) {
        MetadataKind::Regular | MetadataKind::Hostile(_) => return true,
        MetadataKind::Absent => {}
    }
    rootfs.join("etc").exists() || rootfs.join("bin").exists() || rootfs.join("usr").exists()
}

#[cfg(test)]
mod is_container_dir_tests {
    use super::is_container_dir;

    fn dir_with(name: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(name).join("rootfs")).unwrap();
        tmp
    }

    #[test]
    fn a_scratch_image_with_metadata_is_a_container() {
        // Exactly the shape `podnext build local` produces from `FROM scratch`.
        let tmp = dir_with("demo");
        let dir = tmp.path().join("demo");
        std::fs::write(dir.join("rootfs/app.txt"), "hi").unwrap();
        std::fs::write(dir.join("metadata.json"), r#"{"name":"demo:latest"}"#).unwrap();
        assert!(is_container_dir(&dir));
    }

    #[test]
    fn a_legacy_container_without_metadata_still_counts() {
        let tmp = dir_with("legacy");
        let dir = tmp.path().join("legacy");
        std::fs::create_dir_all(dir.join("rootfs/etc")).unwrap();
        assert!(is_container_dir(&dir));
    }

    #[test]
    fn a_directory_without_a_rootfs_or_without_any_marker_is_not_a_container() {
        let tmp = tempfile::tempdir().unwrap();
        // No rootfs at all.
        std::fs::create_dir_all(tmp.path().join("junk")).unwrap();
        assert!(!is_container_dir(&tmp.path().join("junk")));

        // A rootfs, but nothing that identifies it: a half-finished extraction.
        std::fs::create_dir_all(tmp.path().join("half/rootfs")).unwrap();
        assert!(!is_container_dir(&tmp.path().join("half")));
    }
}
