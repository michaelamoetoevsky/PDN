//! The `service` and `daemon` supervision commands.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split. `get_supervisor` and the
//! `output_*` formatters stay in the parent module and are reached through `super::`, because
//! the quota, policy, boot and other domains use them too.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tabled::Tabled;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{get_supervisor, output_json, output_table, output_yaml};
use pn_next::{
    DaemonEvent, DaemonRuntime, DaemonRuntimeConfig, ServiceState, ServiceSupervisor,
    TermuxBootConfig, TermuxBootManager,
};

/// The daemon socket, resolved from the environment rather than hard-coded: a home that
/// moves (second profile, relocated install, `TERMUX__HOME`) must take the socket with it.
/// `XDG_RUNTIME_DIR` wins when set, then `$HOME/.local/run`, then the prefix's tmp.
pub fn daemon_socket() -> std::path::PathBuf {
    proot_distro_core::paths::host::runtime_dir().join("pn-next/daemon.sock")
}

/// Send a command to the daemon and get response
pub async fn send_daemon_command(cmd: serde_json::Value) -> Result<serde_json::Value> {
    let stream = UnixStream::connect(daemon_socket())
        .await
        .context("Failed to connect to daemon. Is podnext-daemon running?")?;
    let mut stream = stream;
    stream
        .write_all(serde_json::to_string(&cmd)?.as_bytes())
        .await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let resp: serde_json::Value = serde_json::from_str(line.trim())?;
    Ok(resp)
}

/// Check if daemon is running
pub async fn is_daemon_running() -> bool {
    UnixStream::connect(daemon_socket()).await.is_ok()
}

pub async fn execute_service_command(cmd: crate::ServiceCommands) -> Result<()> {
    match cmd {
        crate::ServiceCommands::Start { name } => cmd_service_start(name).await,
        crate::ServiceCommands::Stop { name, force } => cmd_service_stop(name, force).await,
        crate::ServiceCommands::Restart { name } => cmd_service_restart(name).await,
        crate::ServiceCommands::List { format } => cmd_service_list(format).await,
        crate::ServiceCommands::Status { name, format } => cmd_service_status(name, format).await,
        crate::ServiceCommands::Logs {
            name,
            follow,
            lines,
        } => cmd_service_logs(name, follow, lines).await,
        crate::ServiceCommands::Enable { name } => cmd_service_enable(name).await,
        crate::ServiceCommands::Disable { name } => cmd_service_disable(name).await,
        crate::ServiceCommands::Load { name } => cmd_service_load(name).await,
        crate::ServiceCommands::Create { name, config } => cmd_service_create(name, config).await,
    }
}

pub async fn cmd_service_start(name: String) -> Result<()> {
    eprintln!("Starting service '{}'...", name);

    // Try daemon first
    if is_daemon_running().await {
        let resp = send_daemon_command(serde_json::json!({"cmd": "start", "name": name})).await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
        println!("Service '{}' start initiated via daemon", name);
        return Ok(());
    }

    // Fallback to local supervisor
    let supervisor = get_supervisor().await?;
    supervisor.start_service(&name).await?;

    // Wait for service to reach Running state
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let status = supervisor.get_status(&name).await?;
        if status.state == ServiceState::Running {
            println!("Service '{}' started (PID: {:?})", name, status.pid);
            return Ok(());
        }
    }

    let status = supervisor.get_status(&name).await?;
    println!(
        "Service '{}' start initiated (current state: {:?})",
        name, status.state
    );
    Ok(())
}

pub async fn cmd_service_stop(name: String, force: bool) -> Result<()> {
    eprintln!("Stopping service '{}'...", name);

    if is_daemon_running().await {
        let resp =
            send_daemon_command(serde_json::json!({"cmd": "stop", "name": name, "force": force}))
                .await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
        println!("Service '{}' stopped via daemon", name);
        return Ok(());
    }

    let supervisor = get_supervisor().await?;
    supervisor.stop_service(&name, force).await?;
    println!("Service '{}' stopped", name);
    Ok(())
}

pub async fn cmd_service_restart(name: String) -> Result<()> {
    eprintln!("Restarting service '{}'...", name);

    if is_daemon_running().await {
        let resp = send_daemon_command(serde_json::json!({"cmd": "restart", "name": name})).await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
        println!("Service '{}' restarted via daemon", name);
        return Ok(());
    }

    let supervisor = get_supervisor().await?;
    supervisor.restart_service(&name).await?;
    println!("Service '{}' restarted", name);
    Ok(())
}

pub async fn cmd_service_list(format: crate::OutputFormat) -> Result<()> {
    if is_daemon_running().await {
        let resp = send_daemon_command(serde_json::json!({"cmd": "list"})).await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
        if let Some(data) = resp.get("data") {
            let services: Vec<serde_json::Value> = serde_json::from_value(data.clone())?;
            print_services(services, format)?;
        }
        return Ok(());
    }

    let supervisor = get_supervisor().await?;
    let services = supervisor.list_services().await;
    print_services_local(services, format)?;
    Ok(())
}

pub async fn cmd_service_status(name: String, format: crate::OutputFormat) -> Result<()> {
    if is_daemon_running().await {
        let resp = send_daemon_command(serde_json::json!({"cmd": "status", "name": name})).await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
        if let Some(data) = resp.get("data") {
            print_service_status(data, format)?;
        }
        return Ok(());
    }

    let supervisor = get_supervisor().await?;
    let status = supervisor.get_status(&name).await?;
    print_service_status_local(status, format)?;
    Ok(())
}

/// Print services from daemon response
pub fn print_services(services: Vec<serde_json::Value>, format: crate::OutputFormat) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct ServiceRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "HEALTH")]
        health: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "RESTART COUNT")]
        restart_count: u32,
    }

    let rows: Vec<ServiceRow> = services
        .iter()
        .map(|s| ServiceRow {
            name: s
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: s
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            health: s
                .get("health")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            pid: s
                .get("pid")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
                .unwrap_or("-".to_string()),
            restart_count: s.get("restart_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&services)?,
        crate::OutputFormat::Yaml => output_yaml(&services)?,
        crate::OutputFormat::Text => {
            for s in services {
                println!(
                    "{} {} {} {:?} {}",
                    s.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    s.get("status").and_then(|v| v.as_str()).unwrap_or(""),
                    s.get("health")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown"),
                    s.get("pid").and_then(|v| v.as_u64()),
                    s.get("restart_count").and_then(|v| v.as_u64()).unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}

/// Print service status from daemon response
pub fn print_service_status(data: &serde_json::Value, format: crate::OutputFormat) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct StatusRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "STATE")]
        state: String,
        #[tabled(rename = "HEALTH")]
        health: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "RESTART COUNT")]
        restart_count: u32,
        #[tabled(rename = "TOTAL RESTARTS")]
        total_restarts: u64,
    }

    let row = StatusRow {
        name: data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        state: data
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        health: data
            .get("health")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string(),
        pid: data
            .get("pid")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or("-".to_string()),
        restart_count: data
            .get("restart_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_restarts: data
            .get("total_restarts")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    };

    match format {
        crate::OutputFormat::Table => output_table(&[row]),
        crate::OutputFormat::Json => output_json(data)?,
        crate::OutputFormat::Yaml => output_yaml(data)?,
        crate::OutputFormat::Text => {
            println!(
                "{} {} {} {:?} {} {}",
                data.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                data.get("state").and_then(|v| v.as_str()).unwrap_or(""),
                data.get("health")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown"),
                data.get("pid").and_then(|v| v.as_u64()),
                data.get("restart_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                data.get("total_restarts")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            );
        }
    }
    Ok(())
}

/// Print services from local supervisor
pub fn print_services_local(
    services: Vec<pn_next::ServiceStatus>,
    format: crate::OutputFormat,
) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct ServiceRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "HEALTH")]
        health: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "RESTART COUNT")]
        restart_count: u32,
    }

    let rows: Vec<ServiceRow> = services
        .iter()
        .map(|s| ServiceRow {
            name: s.name.clone(),
            status: format!("{:?}", s.state),
            health: s
                .health
                .map(|h| format!("{:?}", h))
                .unwrap_or("Unknown".to_string()),
            pid: s.pid.map_or("-".to_string(), |p| p.to_string()),
            restart_count: s.restart_count,
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&services)?,
        crate::OutputFormat::Yaml => output_yaml(&services)?,
        crate::OutputFormat::Text => {
            for s in services {
                println!(
                    "{} {} {} {:?} {}",
                    s.name,
                    format!("{:?}", s.state),
                    s.health
                        .map(|h| format!("{:?}", h))
                        .unwrap_or("Unknown".to_string()),
                    s.pid.map_or("-".to_string(), |p| p.to_string()),
                    s.restart_count
                );
            }
        }
    }
    Ok(())
}

/// Print service status from local supervisor
pub fn print_service_status_local(
    status: pn_next::ServiceStatus,
    format: crate::OutputFormat,
) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct StatusRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "STATE")]
        state: String,
        #[tabled(rename = "HEALTH")]
        health: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "RESTART COUNT")]
        restart_count: u32,
        #[tabled(rename = "TOTAL RESTARTS")]
        total_restarts: u64,
    }

    let row = StatusRow {
        name: status.name.clone(),
        state: format!("{:?}", status.state),
        health: status
            .health
            .map(|h| format!("{:?}", h))
            .unwrap_or("Unknown".to_string()),
        pid: status.pid.map_or("-".to_string(), |p| p.to_string()),
        restart_count: status.restart_count,
        total_restarts: status.total_restarts,
    };

    match format {
        crate::OutputFormat::Table => output_table(&[row]),
        crate::OutputFormat::Json => output_json(&status)?,
        crate::OutputFormat::Yaml => output_yaml(&status)?,
        crate::OutputFormat::Text => {
            println!(
                "{} {} {} {:?} {} {}",
                status.name,
                format!("{:?}", status.state),
                status
                    .health
                    .map(|h| format!("{:?}", h))
                    .unwrap_or("Unknown".to_string()),
                status.pid.map_or("-".to_string(), |p| p.to_string()),
                status.restart_count,
                status.total_restarts
            );
        }
    }
    Ok(())
}

pub async fn cmd_service_logs(name: String, _follow: bool, lines: usize) -> Result<()> {
    if is_daemon_running().await {
        let resp = send_daemon_command(serde_json::json!({"cmd": "status", "name": name})).await?;
        if let Some(_err) = resp
            .get("type")
            .and_then(|t| t.as_str())
            .filter(|&t| t == "error")
        {
            let msg = resp
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("Daemon error: {}", msg);
        }
    } else {
        let supervisor = get_supervisor().await?;
        let _ = supervisor.get_status(&name).await?;
    }

    // A stub that printed a header and a not-implemented note, then returned Ok(()):
    // exit 0 told a script the log had been read.
    Err(anyhow::anyhow!(
        "'service logs' is not implemented: the supervisor does not keep a per-service log, so the \
         last {lines} lines cannot be read. Use `podnext log query --service {name}` for the log \
         the supervisor writes, or `podnext log tail --service {name}` to follow it."
    ))
}

pub async fn cmd_service_enable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.initialize().await?;
    boot_manager.add_auto_start_service(&name).await?;
    println!("Service '{}' enabled for auto-start at boot", name);
    Ok(())
}

pub async fn cmd_service_disable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.remove_auto_start_service(&name).await?;
    println!("Service '{}' disabled for auto-start at boot", name);
    Ok(())
}

pub async fn cmd_service_load(name: String) -> Result<()> {
    eprintln!("Loading service '{}' from TOML config...", name);
    // Same reason as `daemon_socket`: the services directory is the daemon's default, so
    // it is resolved the same way instead of naming one machine's home directory.
    let services_dir = std::env::var("PN_NEXT_SERVICES_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            proot_distro_core::paths::host::config_dir().map(|c| c.join("pn-next/services"))
        })
        .unwrap_or_else(|| proot_distro_core::paths::host::prefix().join("etc/pn-next/services"))
        .display()
        .to_string();
    let config_path = PathBuf::from(services_dir).join(format!("{}.toml", name));
    let _service_config = ServiceSupervisor::load_service_from_file(&config_path).await?;
    println!("Service '{}' loaded from {}", name, config_path.display());
    Ok(())
}

pub async fn cmd_service_create(name: String, config: PathBuf) -> Result<()> {
    eprintln!(
        "Creating service '{}' from config {}",
        name,
        config.display()
    );

    // Validate the file first, then refuse if it names a different service than the
    // one on the command line: otherwise the persisted service is not the one the
    // caller asked for, and every later `service status <name>` says "not found".
    let service_config = ServiceSupervisor::load_service_from_file(&config).await?;
    if service_config.meta.name != name {
        bail!(
            "config {} declares service name '{}', which does not match '{}'",
            config.display(),
            service_config.meta.name,
            name
        );
    }

    // Persist through the supervisor so the file lands in the same services directory
    // that `service list`, `status`, `start` and `logs` all read back from. Creating
    // without persisting was the defect: the command exited 0 and stored nothing.
    let supervisor = get_supervisor().await?;
    supervisor.create_service(service_config).await?;
    println!("Service '{}' created from {}", name, config.display());
    Ok(())
}

pub async fn execute_daemon_command(cmd: crate::DaemonCommands) -> Result<()> {
    match cmd {
        crate::DaemonCommands::Start => cmd_daemon_start().await,
        crate::DaemonCommands::Stop => cmd_daemon_stop().await,
        crate::DaemonCommands::Status { format } => cmd_daemon_status(format).await,
        crate::DaemonCommands::Reload => cmd_daemon_reload().await,
        crate::DaemonCommands::Logs { follow, lines } => cmd_daemon_logs(follow, lines).await,
    }
}

/// The daemon runtime's configuration, resolved from the environment exactly like
/// `daemon_socket()` so the socket it *binds* is the one every other command *probes*.
/// The base used `DaemonRuntimeConfig::default()`, whose `/run` and `/var` paths are
/// unwritable on Termux: `daemon start` died before printing anything (os error 30),
/// and had it not, it would have watched a socket nobody probes.
fn daemon_runtime_config() -> DaemonRuntimeConfig {
    let socket = daemon_socket();
    let run_dir = socket
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| proot_distro_core::paths::host::runtime_dir());
    let state_dir = proot_distro_core::paths::host::state_dir()
        .unwrap_or_else(|| run_dir.clone())
        .join("pn-next/daemon");
    DaemonRuntimeConfig {
        socket_path: socket,
        pid_file: run_dir.join("daemon.pid"),
        log_dir: state_dir.join("logs"),
        state_dir,
        ..DaemonRuntimeConfig::default()
    }
}

/// Send one daemon-level request and return the message the daemon reported. A refusal
/// from the daemon (an unknown verb, a real failure) becomes an error here, so a
/// command never prints a success line over an answer the daemon did not give.
async fn daemon_request(cmd: serde_json::Value) -> Result<String> {
    let resp = send_daemon_command(cmd).await?;
    if resp.get("type").and_then(|t| t.as_str()) == Some("error") {
        bail!(
            "daemon refused: {}",
            resp.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
        );
    }
    Ok(resp
        .get("data")
        .and_then(|d| d.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("ok")
        .to_string())
}

pub async fn cmd_daemon_start() -> Result<()> {
    eprintln!("Starting PDN daemon...");
    let config = daemon_runtime_config();
    // Resolved, not hard-coded: printed so the operator can see which socket this
    // daemon will answer on — the one every `service`/`daemon` command probes.
    eprintln!("Daemon socket: {}", config.socket_path.display());
    let runtime = Arc::new(DaemonRuntime::new(config));
    runtime.start().await?;
    println!("PDN daemon started");

    // Stay up until something asks the daemon to stop (`daemon stop`) or it is
    // signalled. Returning here exited the process the instant it started, so no
    // daemon was ever left running to answer anything.
    let mut events = runtime.subscribe_events();
    while let Ok(event) = events.recv().await {
        if matches!(event, DaemonEvent::DaemonStopped) {
            break;
        }
    }
    println!("PDN daemon stopped");
    Ok(())
}

pub async fn cmd_daemon_stop() -> Result<()> {
    // Same handshake as `daemon status`: a stop that cannot be delivered is a
    // refusal, not a success line. Nothing to stop means a named non-zero exit.
    if !is_daemon_running().await {
        bail!(
            "daemon is not running: nothing is listening on {}",
            daemon_socket().display()
        );
    }
    eprintln!("Stopping PDN daemon...");
    let answer = daemon_request(serde_json::json!({ "cmd": "daemon-stop" })).await?;
    println!("PDN daemon stopped: {answer}");
    Ok(())
}

pub async fn cmd_daemon_status(format: crate::OutputFormat) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct DaemonStatusRow {
        #[tabled(rename = "RUNNING")]
        running: bool,
        #[tabled(rename = "TOTAL SERVICES")]
        total_services: usize,
        #[tabled(rename = "RUNNING SERVICES")]
        running_services: usize,
        #[tabled(rename = "UPTIME")]
        uptime_secs: String,
    }

    // The one fact that can be measured is whether something answers on the daemon
    // socket — the same handshake every `service` command uses to decide if a daemon
    // exists. Without it there is no daemon: reporting `running: true` was the defect.
    let (running, total_services, running_services) = if is_daemon_running().await {
        // A reachable daemon answers `list`, so the counts are read from it rather
        // than guessed. No daemon anywhere records a start time, so the uptime is
        // reported as unknown (null) instead of a fabricated zero.
        let resp = send_daemon_command(serde_json::json!({ "cmd": "list" })).await?;
        let services = resp
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let running_count = services
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("Running"))
            .count();
        (true, services.len(), running_count)
    } else {
        (false, 0, 0)
    };

    let status = DaemonStatusRow {
        running,
        total_services,
        running_services,
        // No daemon in this tree records a start time, so an uptime cannot be measured;
        // "unknown" is the honest value, not a fabricated zero.
        uptime_secs: "unknown".to_string(),
    };

    match format {
        crate::OutputFormat::Table => output_table(&[status]),
        crate::OutputFormat::Json => output_json(&status)?,
        crate::OutputFormat::Yaml => output_yaml(&status)?,
        crate::OutputFormat::Text => println!("Daemon running: {}", running),
    }

    if !running {
        bail!(
            "daemon is not running: nothing is listening on {}",
            daemon_socket().display()
        );
    }
    Ok(())
}

pub async fn cmd_daemon_reload() -> Result<()> {
    // A reload the daemon never receives is not a reload. Without a socket there is
    // nobody to reload, so refuse with the same named reason as `daemon status`.
    if !is_daemon_running().await {
        bail!(
            "daemon is not running: nothing is listening on {}",
            daemon_socket().display()
        );
    }
    let answer = daemon_request(serde_json::json!({ "cmd": "daemon-reload" })).await?;
    println!("Daemon reloaded: {answer}");
    Ok(())
}

pub async fn cmd_daemon_logs(_follow: bool, lines: usize) -> Result<()> {
    // There is no log stream without a daemon; the empty header this used to print
    // was a success line over nothing. Refuse instead, then report what the daemon
    // actually answers (its own log, or an honest refusal when it keeps none).
    if !is_daemon_running().await {
        bail!(
            "daemon is not running: nothing is listening on {}",
            daemon_socket().display()
        );
    }
    let answer =
        daemon_request(serde_json::json!({ "cmd": "daemon-logs", "lines": lines })).await?;
    println!("{answer}");
    Ok(())
}
