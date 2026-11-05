//! CLI command implementations for service orchestration

/// Trace-only logging, off unless `PN_DEBUG` is set in the environment.
///
/// These lines were unconditional upstream and flooded real output — a single
/// `backup create` emitted ~280 kB of DEBUG for one alpine container, burying
/// the result. Enable with `PN_DEBUG=1` when diagnosing.
macro_rules! pn_trace {
    ($($arg:tt)*) => {
        if ::std::env::var_os("PN_DEBUG").is_some() {
            ::std::eprintln!($($arg)*);
        }
    };
}
use anyhow::{bail, Context, Result};
use pn_next::{
    BackupConfig,
    // Phase 7 modules
    BackupManager,
    BackupType,
    ContainerSocketConfig,
    ContainerSocketManager,
    DBusBroker,
    DBusBrokerConfig,
    DaemonRuntime,
    DaemonRuntimeConfig,
    EvaluationContext,
    GitOpsConfig,
    GitOpsManager,
    GitOpsResourceStatus,
    HealthStatus,
    LogLevel,
    LogQuery,
    PolicyEngine,
    PolicyEngineConfig,
    PolicyHealthStatus,
    PolicyResourceType,
    ResourceQuotaConfig,
    ResourceQuotaManager,
    ResourceQuotas,
    RestoreOptions,
    ServiceState,
    ServiceSupervisor,
    SocketActivationConfig,
    SocketActivationManager,
    StructuredLogging,
    StructuredLoggingConfig,
    SupervisorBuilder,
    SyncStatus,
    TermuxBootConfig,
    TermuxBootManager,
    WakeLockConfig,
    WakeLockManager,
    WakeLockType,
    WatchdogConfig,
    WatchdogManager,
    WebDashboard,
    WebDashboardConfig,
};
use serde::Serialize;
use serde_json;
use serde_yaml;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tabled::{Table, Tabled};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::utils::format_size;

mod compose;
pub use compose::execute_compose_command;

// Daemon socket path
const DAEMON_SOCKET: &str = "/data/data/com.termux/files/home/.local/run/pn-next/daemon.sock";

/// Send a command to the daemon and get response
async fn send_daemon_command(cmd: serde_json::Value) -> Result<serde_json::Value> {
    let stream = UnixStream::connect(DAEMON_SOCKET)
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
async fn is_daemon_running() -> bool {
    UnixStream::connect(DAEMON_SOCKET).await.is_ok()
}

// Output formatters
fn output_table<T: Tabled>(items: &[T]) {
    println!("{}", Table::new(items).to_string());
}

fn output_json<T: Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(items)?);
    Ok(())
}

fn output_yaml<T: Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_yaml::to_string(items)?);
    Ok(())
}

// ============ Service Commands ============

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

async fn cmd_service_start(name: String) -> Result<()> {
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

async fn cmd_service_stop(name: String, force: bool) -> Result<()> {
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

async fn cmd_service_restart(name: String) -> Result<()> {
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

async fn cmd_service_list(format: crate::OutputFormat) -> Result<()> {
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

async fn cmd_service_status(name: String, format: crate::OutputFormat) -> Result<()> {
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

async fn get_supervisor() -> Result<ServiceSupervisor> {
    // Derived from the home directory rather than a hardcoded Termux path, so the
    // supervisor also works when PDN is run from a different prefix.
    let home = dirs::home_dir().unwrap_or_default();
    let env_or = |key: &str, fallback: std::path::PathBuf| {
        std::env::var(key).unwrap_or_else(|_| fallback.to_string_lossy().into_owned())
    };
    let services_dir = env_or(
        "PN_NEXT_SERVICES_DIR",
        home.join(".config/pn-next/services"),
    );
    let runtime_dir = env_or("PN_NEXT_RUNTIME_DIR", home.join(".local/run/pn-next"));
    let log_dir = env_or("PN_NEXT_LOG_DIR", home.join(".local/log/pn-next"));

    let supervisor = SupervisorBuilder::new()
        .services_dir(services_dir)
        .runtime_dir(runtime_dir)
        .log_dir(log_dir)
        .build()?;

    // Initialize to load services from directory
    supervisor.initialize().await?;

    Ok(supervisor)
}

/// Print services from daemon response
fn print_services(services: Vec<serde_json::Value>, format: crate::OutputFormat) -> Result<()> {
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
fn print_service_status(data: &serde_json::Value, format: crate::OutputFormat) -> Result<()> {
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
fn print_services_local(
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
fn print_service_status_local(
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

async fn cmd_service_logs(name: String, _follow: bool, lines: usize) -> Result<()> {
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

    println!("Service '{}' logs (last {} lines):", name, lines);
    println!(
        "(Log viewing not yet implemented - use 'podnext log query --service {}')",
        name
    );
    Ok(())
}

async fn cmd_service_enable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.initialize().await?;
    boot_manager.add_auto_start_service(&name).await?;
    println!("Service '{}' enabled for auto-start at boot", name);
    Ok(())
}

async fn cmd_service_disable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.remove_auto_start_service(&name).await?;
    println!("Service '{}' disabled for auto-start at boot", name);
    Ok(())
}

async fn cmd_service_load(name: String) -> Result<()> {
    eprintln!("Loading service '{}' from TOML config...", name);
    let services_dir = std::env::var("PN_NEXT_SERVICES_DIR").unwrap_or_else(|_| {
        "/data/data/com.termux/files/home/.config/pn-next/services".to_string()
    });
    let config_path = PathBuf::from(services_dir).join(format!("{}.toml", name));
    let _service_config = ServiceSupervisor::load_service_from_file(&config_path).await?;
    println!("Service '{}' loaded from {}", name, config_path.display());
    Ok(())
}

async fn cmd_service_create(name: String, config: PathBuf) -> Result<()> {
    eprintln!(
        "Creating service '{}' from config {}",
        name,
        config.display()
    );
    let _service_config = ServiceSupervisor::load_service_from_file(&config).await?;
    println!("Service '{}' created from config", name);
    Ok(())
}

// ============ Daemon Commands ============

pub async fn execute_daemon_command(cmd: crate::DaemonCommands) -> Result<()> {
    match cmd {
        crate::DaemonCommands::Start => cmd_daemon_start().await,
        crate::DaemonCommands::Stop => cmd_daemon_stop().await,
        crate::DaemonCommands::Status { format } => cmd_daemon_status(format).await,
        crate::DaemonCommands::Reload => cmd_daemon_reload().await,
        crate::DaemonCommands::Logs { follow, lines } => cmd_daemon_logs(follow, lines).await,
    }
}

async fn cmd_daemon_start() -> Result<()> {
    eprintln!("Starting PDN daemon...");
    let config = DaemonRuntimeConfig::default();
    let runtime = DaemonRuntime::new(config);
    runtime.start().await?;
    println!("PDN daemon started");
    Ok(())
}

async fn cmd_daemon_stop() -> Result<()> {
    eprintln!("Stopping PDN daemon...");
    println!("PDN daemon stopped");
    Ok(())
}

async fn cmd_daemon_status(format: crate::OutputFormat) -> Result<()> {
    #[derive(Tabled, Serialize)]
    struct DaemonStatusRow {
        #[tabled(rename = "RUNNING")]
        running: bool,
        #[tabled(rename = "TOTAL SERVICES")]
        total_services: usize,
        #[tabled(rename = "RUNNING SERVICES")]
        running_services: usize,
        #[tabled(rename = "UPTIME")]
        uptime_secs: u64,
    }

    let status = DaemonStatusRow {
        running: true,
        total_services: 0,
        running_services: 0,
        uptime_secs: 0,
    };

    match format {
        crate::OutputFormat::Table => output_table(&[status]),
        crate::OutputFormat::Json => output_json(&status)?,
        crate::OutputFormat::Yaml => output_yaml(&status)?,
        crate::OutputFormat::Text => println!("Daemon running: true"),
    }
    Ok(())
}

async fn cmd_daemon_reload() -> Result<()> {
    println!("Reloading daemon configuration...");
    Ok(())
}

async fn cmd_daemon_logs(_follow: bool, lines: usize) -> Result<()> {
    println!("Daemon logs (last {} lines):", lines);
    Ok(())
}

// ============ Watchdog Commands ============

pub async fn execute_watchdog_command(cmd: crate::WatchdogCommands) -> Result<()> {
    match cmd {
        crate::WatchdogCommands::Status { format } => cmd_watchdog_status(format).await,
        crate::WatchdogCommands::SetPolicy {
            name,
            policy,
            max_restarts,
            window_secs,
        } => cmd_watchdog_set_policy(name, policy, max_restarts, window_secs).await,
        crate::WatchdogCommands::Reset { name } => cmd_watchdog_reset(name).await,
        crate::WatchdogCommands::Events { limit } => cmd_watchdog_events(limit).await,
    }
}

async fn cmd_watchdog_status(format: crate::OutputFormat) -> Result<()> {
    let watchdog = WatchdogManager::new(WatchdogConfig::default());
    let states = watchdog.get_all_states().await;

    #[derive(Tabled, Serialize)]
    struct WatchdogRow {
        #[tabled(rename = "SERVICE")]
        service: String,
        #[tabled(rename = "POLICY")]
        policy: String,
        #[tabled(rename = "RESTARTS")]
        restart_count: u32,
        #[tabled(rename = "ESCALATION")]
        escalation_level: u32,
        #[tabled(rename = "QUARANTINED")]
        quarantined: bool,
        #[tabled(rename = "STOPPED")]
        stopped_by_watchdog: bool,
    }

    let rows: Vec<WatchdogRow> = states
        .iter()
        .map(|s| WatchdogRow {
            service: s.service.clone(),
            policy: format!("{:?}", s.policy),
            restart_count: s.restart_count,
            escalation_level: s.escalation_level,
            quarantined: s.quarantined,
            stopped_by_watchdog: s.stopped_by_watchdog,
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&states)?,
        crate::OutputFormat::Yaml => output_yaml(&states)?,
        crate::OutputFormat::Text => {
            for s in rows {
                println!(
                    "{} {} {} {} {} {}",
                    s.service,
                    s.policy,
                    s.restart_count,
                    s.escalation_level,
                    s.quarantined,
                    s.stopped_by_watchdog
                );
            }
        }
    }
    Ok(())
}

async fn cmd_watchdog_set_policy(
    name: String,
    policy: crate::RestartPolicyArg,
    max_restarts: u32,
    window_secs: u64,
) -> Result<()> {
    let mut watchdog = WatchdogManager::new(WatchdogConfig::default());
    let pn_policy = match policy {
        crate::RestartPolicyArg::Never => pn_next::watchdog::RestartPolicy::Never,
        crate::RestartPolicyArg::OnFailure => pn_next::watchdog::RestartPolicy::OnFailure,
        crate::RestartPolicyArg::Always => pn_next::watchdog::RestartPolicy::Always,
        crate::RestartPolicyArg::UnlessStopped => pn_next::watchdog::RestartPolicy::UnlessStopped,
        crate::RestartPolicyArg::OnFailureOrCrash => {
            pn_next::watchdog::RestartPolicy::OnFailureOrCrash
        }
    };
    watchdog.set_policy(&name, pn_policy);

    println!(
        "Watchdog policy set for '{}': {:?} (max restarts: {}, window: {}s)",
        name, policy, max_restarts, window_secs
    );
    Ok(())
}

async fn cmd_watchdog_reset(name: String) -> Result<()> {
    let watchdog = WatchdogManager::new(WatchdogConfig::default());
    watchdog.reset_service(&name).await?;
    println!("Watchdog state reset for '{}'", name);
    Ok(())
}

async fn cmd_watchdog_events(limit: usize) -> Result<()> {
    eprintln!("warning: 'watchdog events' is not yet implemented (stub)");
    println!("Watchdog events (last {}):", limit);
    println!("(Event streaming not yet implemented)");
    Ok(())
}

// ============ Quota Commands ============

pub async fn execute_quota_command(cmd: crate::QuotaCommands) -> Result<()> {
    match cmd {
        crate::QuotaCommands::Set {
            name,
            resource,
            limit,
        } => cmd_quota_set(name, resource, limit).await,
        crate::QuotaCommands::Get { name, format } => cmd_quota_get(name, format).await,
        crate::QuotaCommands::Usage { name, format } => cmd_quota_usage(name, format).await,
        crate::QuotaCommands::List { format } => cmd_quota_list(format).await,
    }
}

async fn cmd_quota_set(
    name: String,
    resource: crate::ResourceTypeArg,
    limit: String,
) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let limit_bytes = parse_quota_limit(&limit, resource)?;

    let mut quota_manager = ResourceQuotaManager::new(ResourceQuotaConfig::default());
    // Load before mutating: setting one resource must not drop the service's
    // other quotas on the floor.
    quota_manager.load_quotas()?;
    // Start from whatever the service already has and set only the requested
    // field. Building a fresh ResourceQuotas here meant `quota set cpu` wiped
    // the memory limit that had just been set, because every other field came
    // out as None.
    let mut quotas = quota_manager.stored_quotas(&name);
    match resource {
        crate::ResourceTypeArg::Cpu => quotas.cpu_quota = Some(limit_bytes),
        crate::ResourceTypeArg::Memory => quotas.memory_limit = Some(limit_bytes),
        crate::ResourceTypeArg::MemorySwap => quotas.memory_swap_limit = Some(limit_bytes),
        crate::ResourceTypeArg::IoReadBps => quotas.io_read_bps = Some(limit_bytes),
        crate::ResourceTypeArg::IoWriteBps => quotas.io_write_bps = Some(limit_bytes),
        crate::ResourceTypeArg::IoReadIops => quotas.io_read_iops = Some(limit_bytes),
        crate::ResourceTypeArg::IoWriteIops => quotas.io_write_iops = Some(limit_bytes),
        crate::ResourceTypeArg::Pids => quotas.pids_max = Some(limit_bytes),
        crate::ResourceTypeArg::Nofile => quotas.nofile_limit = Some(limit_bytes),
        crate::ResourceTypeArg::Network => quotas.network_bps = Some(limit_bytes),
    }
    quota_manager.set_quotas(&name, quotas)?;

    let shown = if matches!(resource, crate::ResourceTypeArg::Cpu) {
        format!("{} (hundredths of a core)", limit_bytes)
    } else if matches!(
        resource,
        crate::ResourceTypeArg::Pids | crate::ResourceTypeArg::Nofile
    ) {
        limit_bytes.to_string()
    } else {
        format_size(limit_bytes)
    };
    println!(
        "Resource quota set for '{}': {:?} = {}",
        name, resource, shown
    );
    Ok(())
}

async fn cmd_quota_get(name: String, format: crate::OutputFormat) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut quota_manager = ResourceQuotaManager::new(ResourceQuotaConfig::default());
    quota_manager.load_quotas()?;
    let quotas = quota_manager.get_quotas(&name);

    #[derive(Tabled, Serialize)]
    struct QuotaRow {
        #[tabled(rename = "RESOURCE")]
        resource: String,
        #[tabled(rename = "LIMIT")]
        limit: String,
    }

    let rows = vec![
        QuotaRow {
            resource: "CPU".to_string(),
            limit: quotas
                .cpu_quota
                .map(|v| v.to_string())
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "Memory".to_string(),
            limit: quotas
                .memory_limit
                .map(|v| format_size(v))
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "Memory Swap".to_string(),
            limit: quotas
                .memory_swap_limit
                .map(|v| format_size(v))
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "I/O Read BPS".to_string(),
            limit: quotas
                .io_read_bps
                .map(|v| format_size(v))
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "I/O Write BPS".to_string(),
            limit: quotas
                .io_write_bps
                .map(|v| format_size(v))
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "PIDs Max".to_string(),
            limit: quotas
                .pids_max
                .map(|v| v.to_string())
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "Open Files".to_string(),
            limit: quotas
                .nofile_limit
                .map(|v| v.to_string())
                .unwrap_or("-".to_string()),
        },
        QuotaRow {
            resource: "Network BPS".to_string(),
            limit: quotas
                .network_bps
                .map(|v| format_size(v))
                .unwrap_or("-".to_string()),
        },
    ];

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&quotas)?,
        crate::OutputFormat::Yaml => output_yaml(&quotas)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{} = {}", r.resource, r.limit);
            }
        }
    }
    Ok(())
}

/// Live usage for one service, next to the limit configured for it.
///
/// `collect_usage` was already real; the only thing missing was a pid, which
/// only the supervisor knows.
async fn cmd_quota_usage(name: String, _format: crate::OutputFormat) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let status = supervisor.get_status(&name).await?;

    let mut quota_manager = ResourceQuotaManager::new(ResourceQuotaConfig::default());
    quota_manager.load_quotas()?;
    let quotas = quota_manager.get_quotas(&name);

    // `ServiceStatus.resources` is the *supervisor's* snapshot type
    // (cpu_percent/memory_bytes/pids), not this crate's ResourceUsage, so it
    // cannot be substituted here. Live usage needs a live pid.
    let Some(pid) = status.pid else {
        anyhow::bail!(
            "Service '{}' is not running (state: {:?}); live usage needs a pid",
            name,
            status.state
        );
    };
    let usage = quota_manager.collect_usage(&name, pid).await?;

    let show = |value: u64, limit: Option<u64>| match limit {
        Some(l) => format!("{} / {}", value, l),
        None => format!("{} (no limit)", value),
    };

    println!(
        "Resource usage for '{}' (state: {:?}, pid: {:?})",
        name, status.state, status.pid
    );
    // CPU values are hundredths of a core on both sides, so they compare directly.
    println!(
        "  CPU (100 = 1 core) : {}",
        show(usage.cpu_usage, quotas.cpu_quota)
    );
    println!(
        "  Memory             : {}",
        show(usage.memory_usage, quotas.memory_limit)
    );
    println!(
        "  Memory swap        : {}",
        show(usage.memory_swap_usage, quotas.memory_swap_limit)
    );
    println!(
        "  I/O read B/s       : {}",
        show(usage.io_read_bps, quotas.io_read_bps)
    );
    println!(
        "  I/O write B/s      : {}",
        show(usage.io_write_bps, quotas.io_write_bps)
    );
    println!(
        "  I/O read IOPS      : {}",
        show(usage.io_read_iops, quotas.io_read_iops)
    );
    println!(
        "  I/O write IOPS     : {}",
        show(usage.io_write_iops, quotas.io_write_iops)
    );
    println!(
        "  Processes          : {}",
        show(usage.pids_count, quotas.pids_max)
    );
    println!(
        "  Open file descr.   : {}",
        show(usage.open_fds, quotas.nofile_limit)
    );
    println!(
        "  Network RX B/s     : {}",
        show(usage.network_rx_bps, quotas.network_bps)
    );
    println!(
        "  Network TX B/s     : {}",
        show(usage.network_tx_bps, quotas.network_bps)
    );
    Ok(())
}

async fn cmd_quota_list(_format: crate::OutputFormat) -> Result<()> {
    let mut quota_manager = ResourceQuotaManager::new(ResourceQuotaConfig::default());
    quota_manager.load_quotas()?;
    let all = quota_manager.all_quotas();

    if all.is_empty() {
        println!("No per-service quotas set. (Defaults still apply to every service.)");
        return Ok(());
    }

    let mut names: Vec<&String> = all.keys().collect();
    names.sort();

    println!(
        "{:<24} {:>10} {:>12} {:>12} {:>10}",
        "SERVICE", "CPU", "MEMORY", "SWAP", "PIDS"
    );
    for name in names {
        let q = &all[name];
        println!(
            "{:<24} {:>10} {:>12} {:>12} {:>10}",
            name,
            q.cpu_quota
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            q.memory_limit
                .map(format_size)
                .unwrap_or_else(|| "-".into()),
            q.memory_swap_limit
                .map(format_size)
                .unwrap_or_else(|| "-".into()),
            q.pids_max
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        );
    }
    Ok(())
}

fn parse_quota_limit(limit: &str, resource: crate::ResourceTypeArg) -> Result<u64> {
    match resource {
        crate::ResourceTypeArg::Cpu => Ok(limit.parse()?),
        crate::ResourceTypeArg::Pids | crate::ResourceTypeArg::Nofile => Ok(limit.parse()?),
        _ => {
            let limit = limit.trim();
            let multiplier = if limit.ends_with("K") || limit.ends_with("k") {
                1024
            } else if limit.ends_with("M") || limit.ends_with("m") {
                1024 * 1024
            } else if limit.ends_with("G") || limit.ends_with("g") {
                1024 * 1024 * 1024
            } else {
                1
            };

            let num_str = if multiplier > 1 {
                &limit[..limit.len() - 1]
            } else {
                limit
            };
            Ok(num_str.parse::<u64>()? * multiplier)
        }
    }
}

// ============ Log Commands ============

pub async fn execute_log_command(cmd: crate::LogCommands) -> Result<()> {
    match cmd {
        crate::LogCommands::Query {
            service,
            min_level,
            max_level,
            grep,
            limit,
            format,
        } => cmd_log_query(service, min_level, max_level, grep, limit, format).await,
        crate::LogCommands::Tail { service, lines } => cmd_log_tail(service, lines).await,
        crate::LogCommands::Stats { format } => cmd_log_stats(format).await,
        crate::LogCommands::Flush => cmd_log_flush().await,
    }
}

async fn cmd_log_query(
    service: Option<String>,
    min_level: Option<crate::LogLevelArg>,
    max_level: Option<crate::LogLevelArg>,
    grep: Option<String>,
    limit: usize,
    format: crate::OutputFormat,
) -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;

    let query = LogQuery {
        service,
        min_level: min_level.map(|l| match l {
            crate::LogLevelArg::Emergency => LogLevel::Emergency,
            crate::LogLevelArg::Alert => LogLevel::Alert,
            crate::LogLevelArg::Critical => LogLevel::Critical,
            crate::LogLevelArg::Error => LogLevel::Error,
            crate::LogLevelArg::Warning => LogLevel::Warning,
            crate::LogLevelArg::Notice => LogLevel::Notice,
            crate::LogLevelArg::Info => LogLevel::Info,
            crate::LogLevelArg::Debug => LogLevel::Debug,
        }),
        max_level: max_level.map(|l| match l {
            crate::LogLevelArg::Emergency => LogLevel::Emergency,
            crate::LogLevelArg::Alert => LogLevel::Alert,
            crate::LogLevelArg::Critical => LogLevel::Critical,
            crate::LogLevelArg::Error => LogLevel::Error,
            crate::LogLevelArg::Warning => LogLevel::Warning,
            crate::LogLevelArg::Notice => LogLevel::Notice,
            crate::LogLevelArg::Info => LogLevel::Info,
            crate::LogLevelArg::Debug => LogLevel::Debug,
        }),
        grep,
        limit: Some(limit),
        ..Default::default()
    };

    let results = logging.query(query).await?;

    #[derive(Tabled, Serialize)]
    struct LogRow {
        #[tabled(rename = "ID")]
        id: u64,
        #[tabled(rename = "LEVEL")]
        level: String,
        #[tabled(rename = "SERVICE")]
        service: String,
        #[tabled(rename = "MESSAGE")]
        message: String,
    }

    let rows: Vec<LogRow> = results
        .iter()
        .map(|e| LogRow {
            id: e.id,
            level: format!("{:?}", e.level),
            service: e.service.clone(),
            message: e.message.clone(),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&results)?,
        crate::OutputFormat::Yaml => output_yaml(&results)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{} [{}] {}: {}", r.id, r.level, r.service, r.message);
            }
        }
    }
    Ok(())
}

async fn cmd_log_tail(service: Option<String>, lines: usize) -> Result<()> {
    // The log store already backs `log query`; tailing is the same query with
    // the newest entries and no filters beyond the service.
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;

    let mut entries = logging
        .query(LogQuery {
            service: service.clone(),
            limit: Some(lines),
            ..Default::default()
        })
        .await?;

    entries.sort_by_key(|e| e.id);
    let start = entries.len().saturating_sub(lines);

    for e in &entries[start..] {
        println!("{:>6} [{:?}] {}: {}", e.id, e.level, e.service, e.message);
    }

    // One-shot: `tail -f` semantics would need a subscription, and saying so is
    // better than appearing to follow.
    match service {
        Some(s) => println!(
            "--- {} entr{} for service '{}' (snapshot, not following) ---",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" },
            s
        ),
        None => println!(
            "--- {} entr{} (snapshot, not following) ---",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        ),
    }
    Ok(())
}

async fn cmd_log_stats(format: crate::OutputFormat) -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;
    let stats = logging.get_stats().await?;

    #[derive(Tabled, Serialize)]
    struct StatsRow {
        #[tabled(rename = "METRIC")]
        metric: String,
        #[tabled(rename = "VALUE")]
        value: String,
    }

    let rows = vec![
        StatsRow {
            metric: "Total Entries".to_string(),
            value: stats.total_entries.to_string(),
        },
        StatsRow {
            metric: "Total Size".to_string(),
            value: format_size(stats.total_size_bytes),
        },
        StatsRow {
            metric: "File Count".to_string(),
            value: stats.file_count.to_string(),
        },
    ];

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&stats)?,
        crate::OutputFormat::Yaml => output_yaml(&stats)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{}: {}", r.metric, r.value);
            }
        }
    }
    Ok(())
}

async fn cmd_log_flush() -> Result<()> {
    let config = StructuredLoggingConfig::default();
    let logging = StructuredLogging::new(config).await?;
    logging.flush().await?;
    println!("Log buffer flushed");
    Ok(())
}

// ============ D-Bus Commands ============

pub async fn execute_dbus_command(cmd: crate::DbusCommands) -> Result<()> {
    match cmd {
        crate::DbusCommands::Serve { socket } => cmd_dbus_serve(socket).await,
        crate::DbusCommands::List { format } => cmd_dbus_list(format).await,
        crate::DbusCommands::Call {
            bus,
            socket,
            path,
            interface,
            method,
            args,
        } => cmd_dbus_call(bus, socket, path, interface, method, args).await,
        crate::DbusCommands::Monitor { bus } => cmd_dbus_monitor(bus).await,
    }
}

/// Listen on the broker socket until interrupted.
async fn cmd_dbus_serve(socket: Option<String>) -> Result<()> {
    let mut config = DBusBrokerConfig::default();
    if let Some(path) = socket {
        config.socket_path = path;
    }
    let path = config.socket_path.clone();

    let broker = DBusBroker::new(config);
    println!("D-Bus broker listening on {}", path);
    println!("Press Ctrl-C to stop.");

    // The signature is `&self`, so the handle has to outlive the call.
    tokio::select! {
        result = broker.start() => result,
        _ = tokio::signal::ctrl_c() => {
            println!("Stopped.");
            Ok(())
        }
    }
}

async fn cmd_dbus_list(format: crate::OutputFormat) -> Result<()> {
    let config = DBusBrokerConfig::default();
    let broker = DBusBroker::new(config);
    let services = broker.list_services(None).await;

    #[derive(Tabled, Serialize)]
    struct DbusRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "PID")]
        pid: u32,
        #[tabled(rename = "CONTAINER")]
        container: String,
    }

    let rows: Vec<DbusRow> = services
        .iter()
        .map(|s| DbusRow {
            name: s.name.clone(),
            pid: s.pid,
            container: s.container.clone().unwrap_or("-".to_string()),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&services)?,
        crate::OutputFormat::Yaml => output_yaml(&services)?,
        crate::OutputFormat::Text => {
            for s in rows {
                println!("{} {} {}", s.name, s.pid, s.container);
            }
        }
    }
    Ok(())
}

/// Speak PDN's own broker framing: a 4-byte little-endian length prefix
/// followed by one JSON message. This is *not* the binary D-Bus protocol, so
/// `ZBusClient` cannot talk to this broker.
async fn dbus_roundtrip(
    socket_path: &str,
    message: &serde_json::Value,
) -> Result<Vec<serde_json::Value>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .with_context(|| {
            format!(
                "Cannot reach the D-Bus broker at {}. Start it first (it is started by \
                 'podnext daemon', or run 'podnext dbus serve' in another shell).",
                socket_path
            )
        })?;

    let payload = serde_json::to_vec(message)?;
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;

    // The broker pushes an unsolicited Hello on connect, then one message per
    // request. Read until we have seen a reply to something other than the
    // handshake, or the peer closes.
    let mut replies = Vec::new();
    // The broker pushes an unsolicited Hello before it reads anything. Hello
    // carries a destination (the client's unique name) just like a real reply
    // does, so it cannot be told apart that way — it is matched by member.
    for _ in 0..5 {
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 16 * 1024 * 1024 {
            anyhow::bail!("Broker sent a nonsensical frame length ({})", len);
        }
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;
        let value: serde_json::Value = serde_json::from_slice(&buf).context(
            "The broker's reply is not the JSON framing this client speaks. A real \
             D-Bus daemon answers in the binary protocol; 'podnext dbus call' targets \
             PDN's own broker only.",
        )?;

        if value["header"]["member"].as_str() == Some("Hello") {
            continue;
        }

        replies.push(value);
        // One request, one reply: anything past this is unsolicited and is
        // reported to the caller rather than looped on forever.
        break;
    }

    Ok(replies)
}

async fn cmd_dbus_call(
    bus: String,
    socket: Option<String>,
    path: String,
    interface: String,
    method: String,
    args: Vec<String>,
) -> Result<()> {
    // Without this the client always dialled the default path, so a broker
    // started with `--socket` was unreachable even though it was running.
    let socket_path = socket.unwrap_or_else(|| DBusBrokerConfig::default().socket_path);
    let _ = &bus;

    // The broker decodes the body as a single JSON value, so `args` are joined
    // into one and passed through as-is when they already parse as JSON.
    let raw = args.join(" ");
    let body: serde_json::Value = if raw.is_empty() {
        serde_json::Value::String(String::new())
    } else {
        serde_json::from_str(&raw)
            .unwrap_or(serde_json::Value::String(raw.trim_matches('"').to_string()))
    };

    let message = serde_json::json!({
        "header": {
            // MessageType derives `rename_all = "lowercase"`, so MethodCall
            // serialises to "methodcall" — "method_call" fails to parse at the
            // broker, which then closes the connection with no reply.
            "message_type": "methodcall",
            "flags": 0,
            "version": 1,
            "serial": 1,
            "path": path,
            "interface": interface,
            "member": method,
            "destination": serde_json::Value::Null,
            "sender": ":1.client",
            "signature": serde_json::Value::Null,
            "unix_fds": 0,
        },
        "body": serde_json::to_vec(&body)?,
    });

    let replies = dbus_roundtrip(&socket_path, &message).await?;
    let reply = replies
        .last()
        .ok_or_else(|| anyhow::anyhow!("Broker closed without answering"))?;

    let msg_type = reply["header"]["message_type"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // The body is JSON bytes inside JSON; unwrap it so the caller sees the
    // payload rather than an array of numbers.
    let decoded = reply["body"]
        .as_array()
        .and_then(|bytes| {
            let bytes: Vec<u8> = bytes
                .iter()
                .filter_map(|b| b.as_u64().map(|n| n as u8))
                .collect();
            serde_json::from_slice::<serde_json::Value>(&bytes).ok()
        })
        .unwrap_or_else(|| reply["body"].clone());

    match msg_type.as_str() {
        "error" => {
            let (name, text) = match &decoded {
                serde_json::Value::Array(items) => (
                    items.first().and_then(|v| v.as_str()).unwrap_or("Error"),
                    items.get(1).and_then(|v| v.as_str()).unwrap_or(""),
                ),
                _ => ("Error", ""),
            };
            anyhow::bail!("{}: {}", name, text);
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&decoded)?);
        }
    }
    Ok(())
}

async fn cmd_dbus_monitor(bus: String) -> Result<()> {
    let _ = bus;
    anyhow::bail!(
        "'dbus monitor' is not implemented: PDN's broker logs signals but never \
         forwards them to a watcher, so there is nothing to stream yet. Watching \
         would need a BecomeMonitor verb in the broker protocol; it does not exist. \
         Use 'podnext log tail' for service output instead."
    )
}

// ============ Socket Commands ============

pub async fn execute_socket_command(cmd: crate::SocketCommands) -> Result<()> {
    match cmd {
        crate::SocketCommands::Register { config } => cmd_socket_register(config).await,
        crate::SocketCommands::Unregister { name } => cmd_socket_unregister(name).await,
        crate::SocketCommands::List { format } => cmd_socket_list(format).await,
    }
}

async fn cmd_socket_register(config: PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(&config)?;
    let socket_config: ContainerSocketConfig = toml::from_str(&content)?;

    let base_manager = SocketActivationManager::new(SocketActivationConfig::default());
    let manager = ContainerSocketManager::new(std::sync::Arc::new(base_manager));
    manager.register_container_socket(socket_config).await?;

    println!("Socket activation registered");
    Ok(())
}

async fn cmd_socket_unregister(name: String) -> Result<()> {
    println!("Unregistering socket '{}'...", name);
    Ok(())
}

async fn cmd_socket_list(_format: crate::OutputFormat) -> Result<()> {
    println!("Socket activations:");
    Ok(())
}

// ============ Boot Commands ============

pub async fn execute_boot_command(cmd: crate::BootCommands) -> Result<()> {
    match cmd {
        crate::BootCommands::Enable { name } => cmd_boot_enable(name).await,
        crate::BootCommands::Disable { name } => cmd_boot_disable(name).await,
        crate::BootCommands::Status { format } => cmd_boot_status(format).await,
    }
}

async fn cmd_boot_enable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.initialize().await?;
    boot_manager.add_auto_start_service(&name).await?;
    println!("Service '{}' enabled for Termux:Boot auto-start", name);
    Ok(())
}

async fn cmd_boot_disable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.remove_auto_start_service(&name).await?;
    println!("Service '{}' disabled for Termux:Boot auto-start", name);
    Ok(())
}

async fn cmd_boot_status(format: crate::OutputFormat) -> Result<()> {
    let boot_manager = TermuxBootManager::new(TermuxBootConfig::default());

    #[derive(Tabled, Serialize)]
    struct BootRow {
        #[tabled(rename = "SERVICE")]
        service: String,
        #[tabled(rename = "ENABLED")]
        enabled: bool,
    }

    let auto_start_services = boot_manager.get_auto_start_services();
    let rows: Vec<BootRow> = auto_start_services
        .iter()
        .map(|s| BootRow {
            service: s.clone(),
            enabled: true,
        })
        .collect();

    let services_vec: Vec<String> = auto_start_services.to_vec();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&services_vec)?,
        crate::OutputFormat::Yaml => output_yaml(&services_vec)?,
        crate::OutputFormat::Text => {
            for s in rows {
                println!(
                    "{} - {}",
                    s.service,
                    if s.enabled { "enabled" } else { "disabled" }
                );
            }
        }
    }
    Ok(())
}

// ============ Wake Lock Commands ============

pub async fn execute_wakelock_command(cmd: crate::WakelockCommands) -> Result<()> {
    match cmd {
        crate::WakelockCommands::Acquire { name, lock_type } => {
            cmd_wakelock_acquire(name, lock_type).await
        }
        crate::WakelockCommands::Release { name } => cmd_wakelock_release(name).await,
        crate::WakelockCommands::Status { format } => cmd_wakelock_status(format).await,
    }
}

async fn cmd_wakelock_acquire(name: String, lock_type: crate::WakeLockTypeArg) -> Result<()> {
    let mut wake_manager = WakeLockManager::new(WakeLockConfig::default());
    wake_manager.initialize_android().await?;

    let lock_type = match lock_type {
        crate::WakeLockTypeArg::Partial => WakeLockType::Partial,
        crate::WakeLockTypeArg::Full => WakeLockType::Full,
        crate::WakeLockTypeArg::DozeExemption => WakeLockType::DozeExemption,
    };

    wake_manager.acquire(&name, Some(lock_type)).await?;
    println!("Wake lock acquired for '{}': {:?}", name, lock_type);
    Ok(())
}

async fn cmd_wakelock_release(name: String) -> Result<()> {
    let wake_manager = WakeLockManager::new(WakeLockConfig::default());
    wake_manager.release(&name).await?;
    println!("Wake lock released for '{}'", name);
    Ok(())
}

async fn cmd_wakelock_status(format: crate::OutputFormat) -> Result<()> {
    let wake_manager = WakeLockManager::new(WakeLockConfig::default());
    let states = wake_manager.get_all_states().await;

    #[derive(Tabled, Serialize)]
    struct WakeRow {
        #[tabled(rename = "SERVICE")]
        service: String,
        #[tabled(rename = "TYPE")]
        lock_type: String,
        #[tabled(rename = "HELD")]
        held: bool,
        #[tabled(rename = "REF COUNT")]
        ref_count: u32,
        #[tabled(rename = "PID")]
        pid: String,
    }

    let rows: Vec<WakeRow> = states
        .iter()
        .map(|s| WakeRow {
            service: s.service_name.clone(),
            lock_type: format!("{:?}", s.lock_type),
            held: s.held,
            ref_count: s.ref_count,
            pid: s.holder_pid.map_or("-".to_string(), |p| p.to_string()),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&states)?,
        crate::OutputFormat::Yaml => output_yaml(&states)?,
        crate::OutputFormat::Text => {
            for s in rows {
                println!(
                    "{} {} {} {} {:?}",
                    s.service, s.lock_type, s.held, s.ref_count, s.pid
                );
            }
        }
    }
    Ok(())
}

// ============ Policy Commands ============

pub async fn execute_policy_command(cmd: crate::PolicyCommands) -> Result<()> {
    match cmd {
        crate::PolicyCommands::Eval { name, format } => cmd_policy_eval(name, format).await,
        crate::PolicyCommands::List { tag, format } => cmd_policy_list(tag, format).await,
        crate::PolicyCommands::Load { file } => cmd_policy_load(file).await,
        crate::PolicyCommands::Presets => cmd_policy_presets().await,
    }
}

/// Evaluate the configured policy rules against one service.
///
/// The engine itself was always real — a 190-line condition matcher. Two things
/// were missing: `load_policies()` was never called, so `rules` was empty and
/// every evaluation was a no-op whatever the user had configured; and nothing
/// built an `EvaluationContext`, which needs live counters only the supervisor
/// has.
async fn cmd_policy_eval(name: String, _format: crate::OutputFormat) -> Result<()> {
    let config = PolicyEngineConfig::default();
    let policy_dir = config.policy_dir.display().to_string();
    let engine = PolicyEngine::new(config);

    engine.load_policies().await?;
    let rules = engine.get_rules().await;

    if rules.is_empty() {
        println!("No policy rules found in {}", policy_dir);
        println!("Add .yaml/.yml/.toml rule files there, then re-run.");
        return Ok(());
    }

    let supervisor = get_supervisor().await?;
    let status = supervisor.get_status(&name).await?;

    let mut resources = std::collections::HashMap::new();
    match status.pid {
        // A live process gives true per-second rates and descriptor counts.
        Some(pid) => {
            let usage = ResourceQuotaManager::new(ResourceQuotaConfig::default())
                .collect_usage(&name, pid)
                .await?;
            // CPU is hundredths of a core on the usage side; policies talk percent.
            resources.insert(
                PolicyResourceType::CpuPercent,
                usage.cpu_usage as f64 / 100.0,
            );
            resources.insert(PolicyResourceType::MemoryBytes, usage.memory_usage as f64);
            resources.insert(PolicyResourceType::DiskReadBps, usage.io_read_bps as f64);
            resources.insert(PolicyResourceType::DiskWriteBps, usage.io_write_bps as f64);
            resources.insert(
                PolicyResourceType::NetworkRxBps,
                usage.network_rx_bps as f64,
            );
            resources.insert(
                PolicyResourceType::NetworkTxBps,
                usage.network_tx_bps as f64,
            );
            resources.insert(PolicyResourceType::PidsCount, usage.pids_count as f64);
            resources.insert(PolicyResourceType::OpenFiles, usage.open_fds as f64);
        }
        // Stopped: only the supervisor's snapshot is available, and only the
        // fields whose meaning carries over are usable. Read/write counters
        // there are cumulative bytes, not rates, so they are left out rather
        // than fed to a policy as if they were per-second.
        None => {
            if let Some(snapshot) = &status.resources {
                resources.insert(PolicyResourceType::CpuPercent, snapshot.cpu_percent);
                resources.insert(
                    PolicyResourceType::MemoryBytes,
                    snapshot.memory_bytes as f64,
                );
                resources.insert(PolicyResourceType::PidsCount, snapshot.pids as f64);
            }
        }
    }

    let uptime_secs = status
        .started_at
        .map(|started| (chrono::Utc::now() - started).num_seconds().max(0) as u64)
        .unwrap_or(0);

    let context = EvaluationContext {
        service: name.clone(),
        labels: Default::default(),
        resources,
        health: match status.health {
            Some(HealthStatus::Healthy) => PolicyHealthStatus::Healthy,
            Some(HealthStatus::Unhealthy) => PolicyHealthStatus::Unhealthy,
            Some(HealthStatus::Starting) => PolicyHealthStatus::Starting,
            _ => PolicyHealthStatus::Unknown,
        },
        restart_count: status.restart_count,
        uptime_secs,
        timestamp: chrono::Utc::now(),
        attributes: Default::default(),
    };

    let result = engine.evaluate(&context).await;

    println!(
        "Policy evaluation for '{}' ({} rule(s) loaded)",
        name,
        rules.len()
    );
    println!("  state      : {:?} (pid {:?})", status.state, status.pid);
    println!("  action     : {:?}", result.action);
    println!("  reason     : {}", result.reason);
    if result.matched_rules.is_empty() {
        println!("  matched    : (no rule matched)");
    } else {
        println!("  matched    : {}", result.matched_rules.join(", "));
    }
    for effect in &result.effects {
        println!("  effect     : {:?}", effect);
    }
    Ok(())
}

async fn cmd_policy_list(tag: Option<String>, format: crate::OutputFormat) -> Result<()> {
    let engine = PolicyEngine::new(PolicyEngineConfig::default());
    // Without this the rule set is always empty and `policy list` shows nothing.
    engine.load_policies().await?;
    let rules = if let Some(tag) = tag {
        engine.list_rules_by_tag(&tag).await
    } else {
        engine.get_rules().await
    };

    #[derive(Tabled, Serialize)]
    struct PolicyRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "PRIORITY")]
        priority: i32,
        #[tabled(rename = "ACTION")]
        action: String,
        #[tabled(rename = "REASON")]
        reason: String,
        #[tabled(rename = "ENABLED")]
        enabled: bool,
    }

    let rows: Vec<PolicyRow> = rules
        .iter()
        .map(|r| PolicyRow {
            name: r.name.clone(),
            priority: r.priority,
            action: format!("{:?}", r.action),
            reason: r.reason.clone(),
            enabled: r.enabled,
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&rules)?,
        crate::OutputFormat::Yaml => output_yaml(&rules)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{} {} {} {}", r.name, r.priority, r.action, r.reason);
            }
        }
    }
    Ok(())
}

async fn cmd_policy_load(file: PathBuf) -> Result<()> {
    println!("Loading policy from {}...", file.display());
    Ok(())
}

async fn cmd_policy_presets() -> Result<()> {
    println!("Available policy presets:");
    println!("  production-safety - Production safety policies (restart limits, OOM quarantine, high CPU warning)");
    println!("  development - Development environment policies (unlimited restarts)");
    Ok(())
}

// ============ Backup Commands ============

pub async fn execute_backup_command(cmd: crate::BackupCommands) -> Result<()> {
    // Initialize backup manager with default config
    pn_trace!("DEBUG: execute_backup_command START");
    pn_trace!("DEBUG: Creating BackupConfig");
    let config = BackupConfig::default();
    pn_trace!("DEBUG: backup_dir = {:?}", config.backup_dir);
    pn_trace!("DEBUG: containers_dir = {:?}", config.containers_dir);
    pn_trace!("DEBUG: Creating BackupManager");
    let manager = BackupManager::new(config);
    pn_trace!("DEBUG: Starting backup manager");
    manager.start().await?;
    pn_trace!("DEBUG: Backup manager started");

    pn_trace!("DEBUG: Matching backup command");
    match cmd {
        crate::BackupCommands::Create {
            container,
            backup_type,
            format,
        } => cmd_backup_create(&manager, container, backup_type, format).await,
        crate::BackupCommands::List { container, format } => {
            cmd_backup_list(&manager, container, format).await
        }
        crate::BackupCommands::Restore { backup_id, target } => {
            cmd_backup_restore(&manager, backup_id, target).await
        }
        crate::BackupCommands::Delete { backup_id } => cmd_backup_delete(&manager, backup_id).await,
        crate::BackupCommands::Status { backup_id } => cmd_backup_status(&manager, backup_id).await,
        crate::BackupCommands::Config => cmd_backup_config(&manager).await,
    }
}

async fn cmd_backup_create(
    manager: &BackupManager,
    container: String,
    backup_type: crate::BackupTypeArg,
    _format: crate::OutputFormat,
) -> Result<()> {
    let bt = match backup_type {
        crate::BackupTypeArg::Full => BackupType::Full,
        crate::BackupTypeArg::Incremental => BackupType::Incremental,
        crate::BackupTypeArg::Differential => BackupType::Differential,
        crate::BackupTypeArg::ConfigOnly => BackupType::Manual,
    };

    eprintln!("Creating {:?} backup for container '{}'...", bt, container);
    let backup_id = manager.create_backup(&container, &container, bt).await?;
    println!("Backup created with ID: {}", backup_id);
    Ok(())
}

async fn cmd_backup_list(
    manager: &BackupManager,
    container: Option<String>,
    format: crate::OutputFormat,
) -> Result<()> {
    let backups = manager.list_backups(container.as_deref()).await;

    #[derive(Tabled, Serialize)]
    struct BackupRow {
        #[tabled(rename = "ID")]
        id: String,
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "CONTAINER")]
        container_id: String,
        #[tabled(rename = "TYPE")]
        backup_type: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "SIZE")]
        size: String,
        #[tabled(rename = "CREATED")]
        created_at: String,
    }

    let rows: Vec<BackupRow> = backups
        .iter()
        .map(|b| BackupRow {
            id: b.id.clone(),
            name: b.name.clone(),
            container_id: b.container_id.clone(),
            backup_type: format!("{:?}", b.backup_type),
            status: format!("{:?}", b.status),
            size: crate::utils::format_size(b.size_bytes),
            created_at: chrono::DateTime::<chrono::Utc>::from(b.created_at)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&backups)?,
        crate::OutputFormat::Yaml => output_yaml(&backups)?,
        crate::OutputFormat::Text => {
            for b in backups {
                println!(
                    "{} ({:?}) [{:?}] {}",
                    b.id,
                    b.backup_type,
                    b.status,
                    crate::utils::format_size(b.size_bytes)
                );
            }
        }
    }
    Ok(())
}

async fn cmd_backup_restore(
    manager: &BackupManager,
    backup_id: String,
    target: Option<String>,
) -> Result<()> {
    println!("Restoring backup '{}'...", backup_id);
    let options = RestoreOptions {
        target_name: target,
        target_id: None,
        restore_volumes: true,
        restore_config: true,
        restore_logs: true,
        overwrite: false,
        auto_start: false,
        network: None,
        labels: HashMap::new(),
    };
    let container_id = manager.restore_backup(&backup_id, options).await?;
    println!("Restored to container: {}", container_id);
    Ok(())
}

async fn cmd_backup_delete(manager: &BackupManager, backup_id: String) -> Result<()> {
    eprintln!("Deleting backup '{}'...", backup_id);
    manager.delete_backup(&backup_id).await?;
    println!("Backup deleted");
    Ok(())
}

async fn cmd_backup_status(manager: &BackupManager, backup_id: String) -> Result<()> {
    if let Some(backup) = manager.get_backup(&backup_id).await {
        println!("Backup ID: {}", backup.id);
        println!("Name: {}", backup.name);
        println!("Container: {}", backup.container_name);
        println!("Type: {:?}", backup.backup_type);
        println!("Status: {:?}", backup.status);
        println!("Size: {}", crate::utils::format_size(backup.size_bytes));
        println!(
            "Compressed: {}",
            crate::utils::format_size(backup.compressed_size)
        );
        println!("Checksum: {}", backup.checksum);
        if let Some(completed) = backup.completed_at {
            println!(
                "Completed: {} seconds ago",
                completed
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
        }
    } else {
        println!("Backup '{}' not found", backup_id);
    }
    Ok(())
}

async fn cmd_backup_config(_manager: &BackupManager) -> Result<()> {
    // Access config directly since it's not exposed via get_config
    println!("Backup Configuration:");
    println!("  (Config access not directly available via API)");
    Ok(())
}

// ============ Build Commands ============

pub async fn execute_build_command(cmd: crate::BuildCommands) -> Result<()> {
    match cmd {
        crate::BuildCommands::Local { tag, file, path } => cmd_build_local(tag, file, path).await,
        crate::BuildCommands::Submit { spec, format } => cmd_build_submit(spec, format).await,
        crate::BuildCommands::Status { build_id, format } => {
            cmd_build_status(build_id, format).await
        }
        crate::BuildCommands::Cancel { build_id } => cmd_build_cancel(build_id).await,
        crate::BuildCommands::List { status, format } => cmd_build_list(status, format).await,
        crate::BuildCommands::Artifacts { build_id } => cmd_build_artifacts(build_id).await,
        crate::BuildCommands::Logs { build_id, follow } => cmd_build_logs(build_id, follow).await,
    }
}

async fn cmd_build_local(tag: Option<String>, file: Option<PathBuf>, path: PathBuf) -> Result<()> {
    let target_tag = tag.unwrap_or_else(|| "custom:latest".to_string());
    // `build_local_image` performs the build and only returns once the image is
    // on disk. The success line below is unreachable on failure, so a printed
    // "Successfully built" is now always backed by a real image (#29).
    let image_dir = build_local_image(&target_tag, file, &path, None).await?;

    // The runnable name is the store key, not the tag: `NameValidator` rejects a
    // colon, so the advice for `custom:latest` used to be a command that could
    // never work.
    let key = proot_distro_core::names::store_key(&target_tag);
    println!("\nSuccessfully built image '{}'", target_tag);
    println!("  Image stored at {}", image_dir.display());
    if key != target_tag {
        println!("  Stored as container '{key}' (a tag is not a path component)");
    }
    let bin_name = std::env::args()
        .next()
        .unwrap_or_else(|| "podnext".to_string());
    let rootfs = image_dir.join("rootfs");
    let has_sh = rootfs.join("bin/sh").symlink_metadata().is_ok()
        || rootfs.join("usr/bin/sh").symlink_metadata().is_ok();
    if has_sh {
        println!("  Run it with: {bin_name} run {key}");
    } else {
        let meta_cmd: Vec<String> = std::fs::read_to_string(image_dir.join("metadata.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("cmd").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if !meta_cmd.is_empty() {
            println!(
                "  Run it with: {bin_name} run {key} -- {}",
                meta_cmd.join(" ")
            );
        } else {
            println!("  Run it with: {bin_name} run {key} -- <command>");
        }
    }
    Ok(())
}

/// Materialise a Dockerfile build into the runtime's image store.
///
/// A "built image" in this runtime *is* a container directory under
/// `~/.podnext/containers/<store-key>/rootfs` — what `ContainerManager::load_containers`
/// scans, and what `podnext run <tag>` / `podnext image ls` read. This function
/// produces exactly that and returns its directory, so a returned `Ok` means the
/// image is really runnable. `containers_dir` overrides the store root for tests.
///
/// The previous implementation diffed the raw build context against an empty
/// base, printed the blob digest, deleted the blob and then claimed success while
/// writing nothing — a silent false success. It also printed that success line
/// even when the context path did not exist, i.e. after doing no work at all.
async fn build_local_image(
    tag: &str,
    file: Option<PathBuf>,
    path: &std::path::Path,
    containers_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    let dockerfile_path = file.unwrap_or_else(|| path.join("Dockerfile"));
    // The old code guarded the only work it did with `if path.exists()` and then
    // printed success regardless; a missing context must be a hard error. Checked
    // first so it is reported ahead of the derived Dockerfile path.
    if !path.exists() {
        bail!(
            "build context '{}' does not exist; pass a directory containing the Dockerfile",
            path.display()
        );
    }
    if !dockerfile_path.exists() {
        bail!("Dockerfile not found at '{}'", dockerfile_path.display());
    }

    println!("Parsing Dockerfile: {}", dockerfile_path.display());
    let content = std::fs::read_to_string(&dockerfile_path)?;
    // The Dockerfile parser (owned by proot-distro-core) has no ARG variant, so
    // consume ARG instructions here: declare them and substitute their values
    // into the instructions that follow.
    let (content, build_args) = preprocess_build_args(&content);
    for (name, value) in &build_args {
        if value.is_empty() {
            println!("Build arg: {} (no default)", name);
        } else {
            println!("Build arg: {}={}", name, value);
        }
    }
    // Before parsing: the parser drops `--` flags and uses only the last FROM, so
    // a multi-stage Dockerfile would build something subtly different without
    // saying so. Refuse those shapes by name.
    refuse_unsupported_build_features(&content)?;
    let instructions = proot_distro_core::dockerfile::parse_dockerfile(&content)?;
    println!("Parsed {} instruction(s):", instructions.len());
    for (idx, inst) in instructions.iter().enumerate() {
        println!("  [{}/{}] {:?}", idx + 1, instructions.len(), inst);
    }

    // Resolve the base image from the last FROM. Without one there is nothing to
    // build on, so refuse rather than fabricate a result.
    let base_image = instructions
        .iter()
        .rev()
        .find_map(|inst| match inst {
            proot_distro_core::dockerfile::DockerfileInstruction::From { image, tag, .. } => {
                Some(match tag {
                    Some(t) if !t.is_empty() => format!("{}:{}", image, t),
                    _ => image.clone(),
                })
            }
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Dockerfile has no FROM instruction; cannot build"))?;
    println!("\nTarget image tag: {}", tag);
    println!("Base image: {}", base_image);

    let containers_dir = containers_dir.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".podnext")
            .join("containers")
    });
    // The store key is derived, never the raw tag. `c2env:latest` is a legal
    // directory name, but it puts a colon in the host path and PRoot splits bind
    // arguments on ':' - the container then silently loses /dev/shm and the /proc
    // stubs (#35). `store_key` is the rule `podnext install` already used, and
    // `podnext run <store_key>` resolves the result.
    let key = proot_distro_core::names::store_key(tag);
    // An empty or otherwise unusable key would mean a container the CLI can never
    // address. Fail here rather than create it.
    proot_distro_core::NameValidator::new()
        .validate_container_name(&key)
        .with_context(|| {
            format!("tag '{tag}' does not yield a usable container name (derived '{key}')")
        })?;
    let container_dir = containers_dir.join(&key);
    if container_dir.exists() {
        // Rebuilding the same tag replaces its image, which is what repeat builds
        // expect. A *different* tag that derives to the same key is not the same
        // thing: clearing it would destroy an image the user still refers to by
        // another name (`app:v1` then `app:v2`). The recorded tag is what tells the
        // two cases apart, so refuse when they disagree instead of deleting.
        let recorded = std::fs::read_to_string(container_dir.join("metadata.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|meta| {
                meta.get("name")
                    .and_then(|n| n.as_str())
                    .map(str::to_string)
            });
        if let Some(recorded) = recorded {
            if recorded != tag {
                bail!(
                    "container '{key}' already holds image '{recorded}' and tag '{tag}' \
                     derives to the same name; rebuilding would destroy that image. \
                     Remove it first (podnext remove {key}) or choose another tag"
                );
            }
        }
        std::fs::remove_dir_all(&container_dir).with_context(|| {
            format!(
                "failed to clear existing image '{}'",
                container_dir.display()
            )
        })?;
    }
    let rootfs = container_dir.join("rootfs");

    if base_image == "scratch" {
        // `FROM scratch` is Docker's empty base: the Dockerfile supplies the
        // whole filesystem. No registry pull (and, as in Docker, no shell for RUN).
        std::fs::create_dir_all(&rootfs)?;
    } else {
        println!("Pulling base image '{}'...", base_image);
        // `key`, not `tag`: this installs the base INTO the container directory
        // that `podnext run <key>` will resolve, and a colon in that name would
        // put a colon in every bind path derived from it (#35).
        crate::native::install_image_native(&base_image, &key, true, Some(rootfs.clone())).await?;
    }

    // Apply the Dockerfile on top of the base rootfs, in order. `env` and
    // `workdir` accumulate the way Docker scopes them for later RUNs.
    let mut env: Vec<(String, String)> = Vec::new();
    let mut workdir: Option<String> = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut labels: Vec<(String, String)> = Vec::new();
    // EXPOSE/USER are image metadata, not filesystem changes. `EXPOSE` in Docker
    // only *documents* a port (it never binds one) and this runtime has no host
    // port-forwarding daemon, so recording it is the faithful behaviour. `USER`
    // is recorded but not enforced: proot's rootless model maps every guest id
    // to the single host uid and `ContainerConfig` carries no user/uid field
    // (adding one means changing the shared crate, outside this branch's scope).
    let mut exposed_ports: Vec<u16> = Vec::new();
    let mut user: Option<String> = None;
    // Constructed lazily: only RUN needs the proot engine, which needs a
    // multi-thread runtime. A metadata-only (scratch) build must not require it.
    let mut mgr: Option<proot_distro_core::ContainerManager> = None;

    for inst in &instructions {
        use proot_distro_core::dockerfile::DockerfileInstruction as Instr;
        match inst {
            Instr::From { .. } => {}
            Instr::Copy { src, dest } | Instr::Add { src, dest } => {
                println!("Copy {} -> {}", src, dest);
                copy_into_rootfs(path, src, dest, &rootfs)?;
            }
            Instr::Env { key, value } => {
                env.retain(|(k, _)| k != key);
                env.push((key.clone(), value.clone()));
            }
            Instr::Workdir(dir) => {
                let dir = dir.trim_matches('"').to_string();
                let abs = if dir.starts_with('/') {
                    dir
                } else {
                    format!("/{}", dir)
                };
                std::fs::create_dir_all(rootfs.join(abs.trim_start_matches('/')))?;
                workdir = Some(abs);
            }
            Instr::Run(run_cmd) => {
                println!("Running: {}", run_cmd);
                if mgr.is_none() {
                    mgr = Some(proot_distro_core::ContainerManager::new()?);
                }
                let mgr = mgr.as_ref().unwrap();
                let mut c = mgr.get(&key).await?;
                c.config.env_vars = env.clone();
                if let Some(wd) = &workdir {
                    c.config.working_dir = Some(wd.clone());
                }
                let code = mgr
                    .execute_container(
                        &c,
                        vec!["/bin/sh".to_string(), "-c".to_string(), run_cmd.clone()],
                    )
                    .await?;
                if code != 0 {
                    bail!("RUN instruction failed (exit {}): '{}'", code, run_cmd);
                }
            }
            Instr::Cmd(args) | Instr::Entrypoint(args) => {
                if !args.is_empty() {
                    cmd = args.clone();
                }
            }
            Instr::Label { key, value } => {
                labels.retain(|(k, _)| k != key);
                labels.push((key.clone(), value.clone()));
            }
            Instr::Expose(port) => {
                if !exposed_ports.contains(port) {
                    println!("Expose: {} (recorded in the image metadata)", port);
                    if exposed_ports.is_empty() {
                        // Docker's EXPOSE only documents a port; this runtime has
                        // no port-forwarding daemon, so say so rather than let a
                        // reader assume traffic will arrive.
                        println!(
                            "NOTE: EXPOSE is metadata here. This runtime does not forward host \
                             ports, so publishing them is a no-op."
                        );
                    }
                    exposed_ports.push(*port);
                }
            }
            Instr::User(u) => {
                let u = u.trim().to_string();
                if !u.is_empty() {
                    println!("User: {} (recorded in the image metadata)", u);
                    println!(
                        "NOTE: USER is not enforced here. PRoot maps every guest id to the single \
                         host uid, so RUN and `podnext run` both execute as the host user. Do not \
                         rely on this image's USER for privilege separation."
                    );
                    user = Some(u);
                }
            }
        }
    }

    // Record image metadata so `podnext image ls` / `inspect` can see the result.
    let (target_repo, target_tag) = if let Some((r, t)) = tag.split_once(':') {
        (r, t)
    } else {
        (tag, "latest")
    };
    let metadata = serde_json::json!({
        "name": tag,
        "image": target_repo,
        "tag": target_tag,
        "base_image": base_image,
        "rootfs": rootfs.to_string_lossy(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "status": "Stopped",
        "cmd": cmd,
        "env": env.iter().cloned().collect::<HashMap<_, _>>(),
        "working_dir": workdir,
        "labels": labels.iter().cloned().collect::<HashMap<_, _>>(),
        "exposed_ports": exposed_ports,
        "user": user,
    });
    std::fs::write(
        container_dir.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    // Success is claimed only once the image is actually on disk.
    if !rootfs.is_dir() {
        bail!(
            "build finished but image rootfs '{}' is missing",
            rootfs.display()
        );
    }
    Ok(container_dir)
}

/// Copy a build-context path into the image rootfs (Dockerfile COPY/ADD).
///
/// Handles a single source path (file or directory) and, since a wildcard in the
/// source is a common Dockerfile idiom, `*`/`?` globs expanded against the build
/// context. `--from=` stage copies are still not supported.
fn copy_into_rootfs(
    ctx: &std::path::Path,
    src: &str,
    dest: &str,
    rootfs: &std::path::Path,
) -> Result<()> {
    if src.contains('*') || src.contains('?') {
        // ponytail: `*`/`?` within a path component only — no `**`, `[...]`
        // classes or brace expansion. Add when a real Dockerfile needs them.
        let mut matched = expand_glob(ctx, src)?;
        if matched.is_empty() {
            bail!(
                "COPY/ADD source '{}' matched no files in the build context",
                src
            );
        }
        let dest_is_dir = dest.ends_with('/') || dest == "." || dest == "/";
        if matched.len() > 1 && !dest_is_dir {
            bail!(
                "COPY/ADD with multiple sources needs a directory destination ending in '/', got '{}'",
                dest
            );
        }
        matched.sort();
        for path in matched {
            copy_one(&path, dest, rootfs)?;
        }
        return Ok(());
    }

    let src_path = ctx.join(src);
    if !src_path.exists() {
        bail!("COPY/ADD source '{}' not found in build context", src);
    }
    copy_one(&src_path, dest, rootfs)
}

/// Copy one already-resolved source path into the rootfs.
fn copy_one(src_path: &std::path::Path, dest: &str, rootfs: &std::path::Path) -> Result<()> {
    let mut dest_path = if let Some(stripped) = dest.strip_prefix('/') {
        rootfs.join(stripped)
    } else {
        rootfs.join(dest)
    };
    // A directory (or `.`/`/`) destination means "keep the source's name".
    if dest.ends_with('/') || dest == "." || dest == "/" {
        let base = src_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid COPY/ADD source '{}'", src_path.display()))?;
        dest_path = dest_path.join(base);
    }
    if src_path.is_dir() {
        copy_dir_all(src_path, &dest_path)
    } else {
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src_path, &dest_path)
            .with_context(|| format!("failed to copy '{}' into image", src_path.display()))?;
        Ok(())
    }
}

/// Refuse Dockerfile shapes this builder cannot honour.
///
/// Both cases below would otherwise build something other than what the file
/// asks for: the parser drops every `--` flag (so `COPY --from=` would copy from
/// the context instead of a stage) and only the last `FROM` supplies the base
/// (so earlier stages vanish). A loud refusal is the only honest option until
/// per-stage rootfs support exists.
fn refuse_unsupported_build_features(content: &str) -> Result<()> {
    let mut stages = 0usize;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("FROM ") || upper == "FROM" {
            stages += 1;
        }
        if (upper.starts_with("COPY ") || upper.starts_with("ADD ")) && trimmed.contains("--from=")
        {
            bail!(
                "line {}: 'COPY/ADD --from=' (multi-stage copy) is not supported: only the last \
                 FROM is used as the base and there is no per-stage rootfs. Build each stage \
                 separately, or copy the files from the build context",
                idx + 1
            );
        }
    }
    if stages > 1 {
        bail!(
            "this Dockerfile declares {stages} FROM stages, which this builder cannot honour: \
             only the last FROM becomes the base and the earlier stages are discarded. Build \
             each stage separately or remove the unused stages"
        );
    }
    Ok(())
}

/// Expand a COPY/ADD source pattern against the build context.
///
/// Components without a wildcard must exist literally; wildcard components are
/// matched against the directory listing. Returns the accumulated matches (empty
/// when nothing matches).
fn expand_glob(ctx: &std::path::Path, pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut current = vec![ctx.to_path_buf()];
    for comp in pattern.split('/').filter(|c| !c.is_empty() && *c != ".") {
        if comp == "**" {
            bail!(
                "'**' is not supported in a COPY/ADD source ('{pattern}'): this builder matches \
                 one path component at a time, so '**' would silently match nothing. List the \
                 files explicitly or use '*' once per component"
            );
        }
        let mut next = Vec::new();
        if comp.contains('*') || comp.contains('?') || comp.contains('[') {
            for dir in &current {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if glob_match(comp, &name) {
                        next.push(entry.path());
                    }
                }
            }
        } else {
            for dir in &current {
                let path = dir.join(comp);
                if path.exists() {
                    next.push(path);
                }
            }
        }
        next.sort();
        current = next;
    }
    Ok(current)
}

/// `*` matches any run of characters, `?` exactly one, `[...]` one character from
/// a set or range (`[abc]`, `[a-z]`, `[!0-9]`). `**` is handled (refused) by
/// [`expand_glob`] before reaching here, because it is a path-level feature.
fn glob_match(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = name.chars().collect();
    matches_from(&pat, &txt)
}

fn matches_from(pat: &[char], txt: &[char]) -> bool {
    let Some(&head) = pat.first() else {
        return txt.is_empty();
    };
    match head {
        '*' => (0..=txt.len()).any(|n| matches_from(&pat[1..], &txt[n..])),
        '?' => !txt.is_empty() && matches_from(&pat[1..], &txt[1..]),
        '[' => {
            if txt.is_empty() {
                return false;
            }
            match parse_class(pat) {
                // A malformed class is a literal '[' - the shell behaviour, and
                // the least surprising thing for a COPY pattern.
                Some((set, negated, rest)) => {
                    (set.contains(&txt[0]) != negated) && matches_from(rest, &txt[1..])
                }
                None => txt[0] == '[' && matches_from(&pat[1..], &txt[1..]),
            }
        }
        c => !txt.is_empty() && txt[0] == c && matches_from(&pat[1..], &txt[1..]),
    }
}

/// Parse a `[...]` class at the start of `pat`, returning its characters, whether
/// it is negated (`[!...]` or `[^...]`) and the pattern after the class.
fn parse_class(pat: &[char]) -> Option<(Vec<char>, bool, &[char])> {
    let mut i = 1;
    let negated = matches!(pat.get(i), Some('!') | Some('^'));
    if negated {
        i += 1;
    }
    let mut set = Vec::new();
    let mut at_start = true;
    loop {
        let c = *pat.get(i)?;
        if c == ']' && !at_start {
            return Some((set, negated, &pat[i + 1..]));
        }
        // `a-z` is a range; a '-' first or last inside the class is literal.
        if pat.get(i + 1) == Some(&'-') && !matches!(pat.get(i + 2), None | Some(&']')) {
            let hi = *pat.get(i + 2)?;
            if c > hi {
                return None;
            }
            set.extend(c..=hi);
            i += 3;
        } else {
            set.push(c);
            i += 1;
        }
        at_start = false;
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Consume `ARG` instructions from Dockerfile content.
///
/// Returns the content with `ARG` lines removed and the declared args in order.
/// Values are substituted into the remaining instructions so later lines can
/// use `$NAME` / `${NAME}`. `proot-distro-core`'s parser has no `ARG` variant,
/// so this keeps `build local` working without changing that crate.
fn preprocess_build_args(content: &str) -> (String, Vec<(String, String)>) {
    let mut args: Vec<(String, String)> = Vec::new();
    let mut kept = String::with_capacity(content.len());
    for line in content.lines() {
        let trimmed = line.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let verb = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if verb.eq_ignore_ascii_case("ARG") {
            if let Some((name, default)) = rest.split_once('=') {
                args.push((name.trim().to_string(), strip_arg_quotes(default.trim())));
            } else if !rest.is_empty() {
                args.push((rest.to_string(), String::new()));
            }
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    let substituted = substitute_build_args(&kept, &args);
    (substituted, args)
}

fn strip_arg_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Replace `$NAME` / `${NAME}` with the value of a declared build arg.
fn substitute_build_args(content: &str, args: &[(String, String)]) -> String {
    if args.is_empty() {
        return content.to_string();
    }
    let lookup = |name: &str| {
        args.iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0;
    while i < content.len() {
        if bytes[i] == b'$' {
            if i + 1 < content.len() && bytes[i + 1] == b'{' {
                if let Some(close) = content[i + 2..].find('}') {
                    let name = &content[i + 2..i + 2 + close];
                    if let Some(v) = lookup(name) {
                        out.push_str(v);
                        i += 2 + close + 1;
                        continue;
                    }
                }
            } else {
                let start = i + 1;
                let mut end = start;
                while end < content.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if end > start {
                    if let Some(v) = lookup(&content[start..end]) {
                        out.push_str(v);
                        i = end;
                        continue;
                    }
                }
            }
        }
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

async fn cmd_build_submit(spec: PathBuf, _format: crate::OutputFormat) -> Result<()> {
    eprintln!("warning: 'build submit' is not yet implemented (stub)");
    println!("Submitting build from spec '{}'...", spec.display());
    println!("(Build submission not yet implemented)");
    Ok(())
}

async fn cmd_build_status(build_id: String, _format: crate::OutputFormat) -> Result<()> {
    eprintln!("warning: 'build status' is not yet implemented (stub)");
    println!("Build '{}' status: (not yet implemented)", build_id);
    Ok(())
}

async fn cmd_build_cancel(build_id: String) -> Result<()> {
    eprintln!("warning: 'build cancel' is not yet implemented (stub)");
    println!("Cancelling build '{}'...", build_id);
    println!("(Build cancellation not yet implemented)");
    Ok(())
}

async fn cmd_build_list(status: Option<String>, _format: crate::OutputFormat) -> Result<()> {
    eprintln!("warning: 'build list' is not yet implemented (stub)");
    println!(
        "Listing builds{}...",
        status
            .as_ref()
            .map(|s| format!(" with status '{}'", s))
            .unwrap_or_default()
    );
    println!("(Build listing not yet implemented)");
    Ok(())
}

async fn cmd_build_artifacts(build_id: String) -> Result<()> {
    eprintln!("warning: 'build artifacts' is not yet implemented (stub)");
    println!("Build '{}' artifacts: (not yet implemented)", build_id);
    Ok(())
}

async fn cmd_build_logs(build_id: String, follow: bool) -> Result<()> {
    eprintln!("warning: 'build logs' is not yet implemented (stub)");
    println!(
        "Showing logs for build '{}'{}...",
        build_id,
        if follow { " (follow)" } else { "" }
    );
    println!("(Build logs not yet implemented)");
    Ok(())
}

// ============ GitOps Commands ============

pub async fn execute_gitops_command(cmd: crate::GitopsCommands) -> Result<()> {
    match cmd {
        crate::GitopsCommands::Sync { force } => cmd_gitops_sync(force).await,
        crate::GitopsCommands::Status { format } => cmd_gitops_status(format).await,
        crate::GitopsCommands::Resources { kind, format } => {
            cmd_gitops_resources(kind, format).await
        }
        crate::GitopsCommands::Drift { format } => cmd_gitops_drift(format).await,
        crate::GitopsCommands::Reconcile { name } => cmd_gitops_reconcile(name).await,
    }
}

/// Build a manager pointed at the configured repo and load its state.
///
/// Deliberately does *not* call `start()`: that clones a git repository, and
/// these commands operate on local manifests.
async fn gitops_manager() -> Result<GitOpsManager> {
    let mut manager = GitOpsManager::new(GitOpsConfig::default());
    let loaded = manager.load_from_disk().await?;
    // After load_from_disk: the stored status must win over the manifest's
    // default, or every report shows Unknown again.
    manager.load_state().await?;
    if loaded == 0 {
        eprintln!(
            "No applications found in {}.",
            manager.applications_dir().display()
        );
    }
    Ok(manager)
}

async fn cmd_gitops_sync(force: bool) -> Result<()> {
    let manager = gitops_manager().await?;
    let revision = manager.sync_now().await?;
    manager.save_state().await?;

    let total: usize = manager
        .list_applications()
        .await
        .iter()
        .map(|a| a.status.resources.len())
        .sum();

    println!(
        "Synced {} application(s), {} resource(s) applied{}.",
        revision,
        total,
        if force { " (forced)" } else { "" }
    );
    Ok(())
}

async fn cmd_gitops_status(format: crate::OutputFormat) -> Result<()> {
    let manager = gitops_manager().await?;
    let apps = manager.list_applications().await;

    #[derive(Tabled, Serialize)]
    struct StatusRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "NAMESPACE")]
        namespace: String,
        #[tabled(rename = "SYNC")]
        sync: String,
        #[tabled(rename = "HEALTH")]
        health: String,
        #[tabled(rename = "REVISION")]
        revision: String,
        #[tabled(rename = "RESOURCES")]
        resources: usize,
        #[tabled(rename = "OBSERVED")]
        observed: String,
    }

    let rows: Vec<StatusRow> = apps
        .iter()
        .map(|a| StatusRow {
            name: a.name.clone(),
            namespace: a.namespace.clone(),
            sync: format!("{:?}", a.status.sync.status),
            health: format!("{:?}", a.status.health.status),
            revision: if a.status.sync.revision.is_empty() {
                "-".to_string()
            } else {
                a.status.sync.revision.clone()
            },
            resources: a.status.resources.len(),
            observed: if a.status.observed_at.is_empty() {
                "-".to_string()
            } else {
                a.status.observed_at.clone()
            },
        })
        .collect();

    match format {
        crate::OutputFormat::Table => {
            if rows.is_empty() {
                println!("No GitOps applications loaded.");
            } else {
                output_table(&rows);
            }
        }
        crate::OutputFormat::Json => output_json(&apps)?,
        crate::OutputFormat::Yaml => output_yaml(&apps)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!(
                    "{} {} {} {} {} {}",
                    r.name, r.namespace, r.sync, r.health, r.revision, r.resources
                );
            }
        }
    }
    Ok(())
}

async fn cmd_gitops_resources(kind: Option<String>, format: crate::OutputFormat) -> Result<()> {
    let manager = gitops_manager().await?;
    let apps = manager.list_applications().await;

    let wanted = kind.as_ref().map(|k| k.to_lowercase());
    let flattened: Vec<(String, GitOpsResourceStatus)> = apps
        .iter()
        .flat_map(|a| {
            a.status
                .resources
                .iter()
                .cloned()
                .map(move |r| (a.name.clone(), r))
        })
        .filter(|(_, r)| {
            wanted
                .as_ref()
                .map(|k| r.kind.to_lowercase() == *k)
                .unwrap_or(true)
        })
        .collect();

    #[derive(Tabled, Serialize)]
    struct ResourceRow {
        #[tabled(rename = "APPLICATION")]
        application: String,
        #[tabled(rename = "KIND")]
        kind: String,
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "NAMESPACE")]
        namespace: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "HEALTH")]
        health: String,
    }

    let rows: Vec<ResourceRow> = flattened
        .iter()
        .map(|(app, r)| ResourceRow {
            application: app.clone(),
            kind: r.kind.clone(),
            name: r.name.clone(),
            namespace: r.namespace.clone(),
            status: format!("{:?}", r.sync_status),
            health: format!("{:?}", r.health),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => {
            if rows.is_empty() {
                println!(
                    "No resources recorded for {} application(s). Run 'podnext gitops sync' \
                     first - resources are filled in by a sync.",
                    apps.len()
                );
            } else {
                output_table(&rows);
            }
        }
        crate::OutputFormat::Json => output_json(&rows)?,
        crate::OutputFormat::Yaml => output_yaml(&rows)?,
        crate::OutputFormat::Text => {
            for r in rows {
                println!("{} {} {} {}", r.kind, r.name, r.namespace, r.status);
            }
        }
    }
    Ok(())
}

/// Report what is known about drift without pretending to know more.
///
/// Drift means "the live cluster differs from the manifests", and there is no
/// cluster adapter here, so the only honest signal is the recorded sync status.
async fn cmd_gitops_drift(format: crate::OutputFormat) -> Result<()> {
    let manager = gitops_manager().await?;
    let apps = manager.list_applications().await;

    let drifted: Vec<(String, String, String, String)> = apps
        .iter()
        .flat_map(|a| {
            a.status
                .resources
                .iter()
                .filter(|r| r.sync_status != SyncStatus::Synced)
                .map({
                    let app = a.name.clone();
                    move |r| {
                        (
                            app.clone(),
                            r.kind.clone(),
                            r.name.clone(),
                            format!("{:?}", r.sync_status),
                        )
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();

    if drifted.is_empty() {
        println!("No drift recorded: every synced resource last reported as Synced.");
    } else {
        #[derive(Tabled, Serialize)]
        struct DriftRow {
            #[tabled(rename = "APPLICATION")]
            application: String,
            #[tabled(rename = "KIND")]
            kind: String,
            #[tabled(rename = "NAME")]
            name: String,
            #[tabled(rename = "STATUS")]
            status: String,
        }
        let rows: Vec<DriftRow> = drifted
            .iter()
            .map(|(a, k, n, s)| DriftRow {
                application: a.clone(),
                kind: k.clone(),
                name: n.clone(),
                status: s.clone(),
            })
            .collect();
        match format {
            crate::OutputFormat::Json => output_json(&rows)?,
            crate::OutputFormat::Yaml => output_yaml(&rows)?,
            _ => output_table(&rows),
        }
    }

    // Stated every time, so nobody reads the line above as a live comparison.
    println!(
        "Note: this reports the last recorded sync status. Comparing against a live \
         cluster is not implemented - PDN has no cluster adapter, so real drift \
         cannot be detected yet."
    );
    Ok(())
}

async fn cmd_gitops_reconcile(name: Option<String>) -> Result<()> {
    let manager = gitops_manager().await?;

    match name {
        Some(ref target) => {
            // Allow naming either an application or a resource inside one.
            let known = manager.get_application(target).await.is_some()
                || manager
                    .list_applications()
                    .await
                    .iter()
                    .any(|a| a.status.resources.iter().any(|r| r.name == *target));

            if !known {
                anyhow::bail!(
                    "'{}' is not a known application or resource. Run 'podnext gitops status' \
                     to see what is loaded.",
                    target
                );
            }
            manager.sync_one(target).await?;
            manager.save_state().await?;
            println!("Re-synced '{}'.", target);
        }
        None => {
            let n = manager.sync_now().await?;
            manager.save_state().await?;
            println!("Re-synced {} application(s).", n);
        }
    }

    println!(
        "Scope: PDN's GitOps state only. No cluster write was attempted - resources are \
         recorded, not applied to a running cluster."
    );
    Ok(())
}

// ============ Dashboard Commands ============

pub async fn execute_dashboard_command(cmd: crate::DashboardCommands) -> Result<()> {
    let config = WebDashboardConfig {
        enabled: true,
        bind_addr: "0.0.0.0".to_string(),
        port: 8800,
        update_interval_secs: 30,
        ..Default::default()
    };
    let dashboard = WebDashboard::new(config);

    match cmd {
        crate::DashboardCommands::Start { port, bind } => {
            cmd_dashboard_start(dashboard, port, bind).await
        }
        crate::DashboardCommands::Stop => cmd_dashboard_stop(dashboard).await,
        crate::DashboardCommands::Status => cmd_dashboard_status(dashboard).await,
    }
}

async fn cmd_dashboard_start(dashboard: WebDashboard, port: u16, bind: String) -> Result<()> {
    let pid_path = dashboard_pid_path()?;

    // Refuse to start a second server rather than silently failing on bind.
    if let Some(pid) = read_pid(&pid_path) {
        if process_alive(pid) {
            anyhow::bail!(
                "Dashboard already running (pid {}); use 'podnext dashboard stop' first",
                pid
            );
        }
    }

    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&pid_path, std::process::id().to_string())?;

    eprintln!("Starting dashboard on {}:{}...", bind, port);
    eprintln!("PID file: {}", pid_path.display());

    // `WebDashboard::start` was never reached: it binds and serves until
    // killed, which is exactly what this command should do.
    let result = dashboard.start().await;

    // A clean shutdown (Ctrl-C included) must not leave a stale pid file that
    // makes `status` lie.
    let _ = std::fs::remove_file(&pid_path);
    result
}

async fn cmd_dashboard_stop(_dashboard: WebDashboard) -> Result<()> {
    let pid_path = dashboard_pid_path()?;
    let pid = match read_pid(&pid_path) {
        Some(pid) => pid,
        None => {
            println!(
                "Dashboard is not running (no pid file at {})",
                pid_path.display()
            );
            return Ok(());
        }
    };

    if !process_alive(pid) {
        println!(
            "Dashboard is not running (stale pid {}); removing pid file",
            pid
        );
        let _ = std::fs::remove_file(&pid_path);
        return Ok(());
    }

    eprintln!("Stopping dashboard server (pid {})...", pid);
    std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()?;

    // Give it a moment to exit, then escalate the way an operator would.
    for _ in 0..20 {
        if !process_alive(pid) {
            let _ = std::fs::remove_file(&pid_path);
            println!("Dashboard stopped.");
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    std::process::Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status()?;
    let _ = std::fs::remove_file(&pid_path);
    println!("Dashboard did not exit on SIGTERM; sent SIGKILL.");
    Ok(())
}

async fn cmd_dashboard_status(_dashboard: WebDashboard) -> Result<()> {
    let pid_path = dashboard_pid_path()?;

    let Some(pid) = read_pid(&pid_path) else {
        println!("Dashboard: stopped (no pid file)");
        return Ok(());
    };

    if !process_alive(pid) {
        println!("Dashboard: stopped (stale pid {})", pid);
        return Ok(());
    }

    // Port 8800 is the configured default; report whether it is actually held.
    let listening = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:8800".parse().unwrap(),
        std::time::Duration::from_millis(500),
    )
    .is_ok();

    println!("Dashboard: running");
    println!("  pid     : {}", pid);
    println!("  pid file: {}", pid_path.display());
    println!(
        "  127.0.0.1:8800 : {}",
        if listening {
            "accepting connections"
        } else {
            "not accepting"
        }
    );
    Ok(())
}

/// Where the dashboard records its pid, next to the other runtime state.
fn dashboard_pid_path() -> Result<std::path::PathBuf> {
    let home = proot_distro_core::paths::resolve_home()?;
    Ok(home.join(".podnext").join("dashboard.pid"))
}

fn read_pid(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn process_alive(pid: u32) -> bool {
    // `/proc/<pid>` disappears with the process; no signal needed, so nothing
    // is disturbed by a mere status check.
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

#[cfg(test)]
mod build_local_tests {
    use super::*;
    use proot_distro_core::dockerfile::{parse_dockerfile, DockerfileInstruction};

    /// The exec plan `cmd_build_local` derives from Dockerfile content: the
    /// `RUN` commands in order, plus the declared build args.
    fn run_plan(content: &str) -> (Vec<String>, Vec<(String, String)>) {
        let (cleaned, args) = preprocess_build_args(content);
        let cmds = parse_dockerfile(&cleaned)
            .expect("preprocessed Dockerfile must parse")
            .into_iter()
            .filter_map(|i| match i {
                DockerfileInstruction::Run(c) => Some(c),
                _ => None,
            })
            .collect();
        (cmds, args)
    }

    // Fails before the fix: `parse_dockerfile` bails on `ARG`.
    #[test]
    fn arg_is_accepted_and_substituted() {
        let content = "FROM alpine:3.19\nARG VERSION=1.2.3\nARG NAME=\"world\"\nRUN echo $VERSION ${NAME}\nCOPY a b\n";
        let (cmds, args) = run_plan(content);
        assert_eq!(
            args,
            vec![
                ("VERSION".to_string(), "1.2.3".to_string()),
                ("NAME".to_string(), "world".to_string()),
            ]
        );
        assert_eq!(cmds, vec!["echo 1.2.3 world".to_string()]);
    }

    #[test]
    fn arg_without_default_is_declared_empty() {
        let (cmds, args) = run_plan("FROM alpine\nARG TOKEN\nRUN echo ${TOKEN}\n");
        assert_eq!(args, vec![("TOKEN".to_string(), String::new())]);
        assert_eq!(cmds, vec!["echo".to_string()]);
    }

    // Regression for #29: a missing build context must fail hard and write
    // nothing. Before the fix `cmd_build_local` guarded all real work behind
    // `if path.exists()`, skipped it, printed "Successfully built image" and
    // exited 0. The contract asserted here is "Err returned AND the store is
    // untouched" — not one exact diagnostic string ("context missing" and
    // "Dockerfile missing" are both acceptable hard errors).
    #[tokio::test]
    async fn missing_context_is_a_hard_error_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-context");
        let result = build_local_image(
            "bse-missing:latest",
            None,
            &missing,
            Some(store.path().to_path_buf()),
        )
        .await;
        let err = result.expect_err("missing context must be an error, not a success");
        assert!(
            err.to_string().contains("does not exist")
                || err.to_string().contains("Dockerfile not found"),
            "unexpected error: {err}"
        );
        assert!(
            !store.path().join("bse-missing").exists(),
            "a failed build must not register an image"
        );
        assert!(
            !store.path().join("bse-missing:latest").exists(),
            "the store key must never be the raw tag (#35)"
        );
    }

    // Regression for #29: on success the built image must actually be present in
    // the image store as a runnable container (a container dir with a rootfs and
    // metadata.json) — not merely reported. `FROM scratch` keeps this offline.
    #[tokio::test]
    async fn successful_build_registers_a_runnable_image() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY app.txt /app.txt\n",
        )
        .unwrap();
        std::fs::write(ctx.path().join("app.txt"), "static app content\n").unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "bse-scratch:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        // The store key is the colon-free form of the tag (#35); the tag itself
        // stays in metadata below, so the image is still identifiable.
        assert_eq!(dir, store.path().join("bse-scratch"));
        assert!(
            !dir.to_string_lossy().contains(':'),
            "no host path under the store may contain ':': {}",
            dir.display()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/app.txt")).unwrap(),
            "static app content\n"
        );
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["name"], "bse-scratch:latest");
    }

    // Residual gap 2 (#30): `EXPOSE`/`USER` were parsed and then dropped. They are
    // image metadata (Docker's EXPOSE never binds a port), so the fix records them
    // in `metadata.json` where `run`/inspection can see what was built. Duplicate
    // EXPOSE of the same port is collapsed; `EXPOSE 8080/tcp` keeps the port only.
    // `FROM scratch` keeps this offline.
    #[tokio::test]
    async fn expose_and_user_are_recorded_in_metadata() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nEXPOSE 8080\nEXPOSE 8080/tcp\nEXPOSE 443\nUSER nobody\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-meta:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["exposed_ports"], serde_json::json!([8080, 443]));
        assert_eq!(meta["user"], "nobody");
    }

    // Residual gap 1 (#30) is fixed at `run`, which reinstates ENV/WORKDIR read
    // back from this file (`native::apply_image_runtime_config`). This asserts the
    // build half of that contract: the values must be present and named exactly
    // as the run path reads them.
    #[tokio::test]
    async fn env_and_workdir_are_recorded_for_the_run_path() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nENV GREETING=hello-from-image\nENV APP_MODE=prod\nWORKDIR /srv/app\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-env:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["env"]["GREETING"], "hello-from-image");
        assert_eq!(meta["env"]["APP_MODE"], "prod");
        assert_eq!(meta["working_dir"], "/srv/app");
    }

    // Residual gap 3 (#30), wildcard slice: `COPY src/*.rs` was unsupported.
    /// The glob matcher gained `[...]` classes; `**` stays unsupported but is now
    /// refused with a message instead of silently matching nothing.
    #[test]
    fn glob_match_handles_character_classes() {
        assert!(glob_match("file[0-9].txt", "file7.txt"));
        assert!(!glob_match("file[0-9].txt", "filex.txt"));
        assert!(glob_match("[abc]x", "bx"));
        assert!(!glob_match("[abc]x", "dx"));
        assert!(glob_match("v[!0-9]", "vx"));
        assert!(!glob_match("v[!0-9]", "v1"));
        assert!(glob_match("v[^0-9]", "vy"));
        assert!(glob_match("[a-c]1", "b1"));
        assert!(!glob_match("[a-c]1", "e1"));
        // A ']' as the first character of a class is literal, as in the shell.
        assert!(glob_match("[]]", "]"));

        // `*` and `?` keep working, including a match that needs backtracking.
        assert!(glob_match("*[0-9]", "abc7"));
        assert!(glob_match("a*z[0-9]", "abcz4"));
        assert!(glob_match("?[a-z]", "xq"));
        assert!(!glob_match("?[a-z]", "x1"));
    }

    #[test]
    fn double_star_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let err = expand_glob(tmp.path(), "src/**/*.rs")
            .expect_err("'**' must be refused, not silently match nothing");
        let msg = err.to_string();
        assert!(
            msg.contains("**"),
            "error must show the offending token: {msg}"
        );
        assert!(msg.contains("not supported"), "error must say so: {msg}");
    }

    /// Multi-stage and `--from=` would build something other than what the file
    /// asks for, so both are refused before any work happens.
    #[test]
    fn unsupported_dockerfile_shapes_are_refused() {
        let ok = "FROM scratch\nCOPY a /a\nRUN echo hi\n";
        assert!(refuse_unsupported_build_features(ok).is_ok());
        assert!(refuse_unsupported_build_features("FROM alpine:3.19 AS build\nRUN true\n").is_ok());

        // Two stages with no stage copy: refused on the stage count.
        let two_stages = "FROM alpine:3.19 AS build\nRUN true\nFROM scratch\nCOPY a /a\n";
        let err = refuse_unsupported_build_features(two_stages)
            .unwrap_err()
            .to_string();
        assert!(err.contains("2 FROM stages"), "unexpected error: {err}");

        // Two stages *with* a stage copy: the more specific refusal wins, because it
        // names the line the reader has to look at.
        let both = "FROM alpine:3.19 AS build\nRUN true\nFROM scratch\nCOPY --from=build /x /x\n";
        let err = refuse_unsupported_build_features(both)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--from="), "unexpected error: {err}");
        assert!(
            err.contains("line 4"),
            "error should point at the line: {err}"
        );

        let stage_copy = "FROM alpine:3.19\nCOPY --from=builder /app /app\n";
        let err = refuse_unsupported_build_features(stage_copy)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--from="), "unexpected error: {err}");
        assert!(
            err.contains("line 2"),
            "error should point at the line: {err}"
        );

        // A comment mentioning the feature must not trigger the refusal.
        assert!(refuse_unsupported_build_features("# COPY --from=x /a /b\nFROM scratch\n").is_ok());
    }

    #[test]
    fn glob_match_handles_star_and_question() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.txt"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abbc"));
        assert!(glob_match(".*", ".env"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("src/*.rs", "main.rs"));
    }

    #[tokio::test]
    async fn copy_wildcard_copies_every_match_into_a_directory() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(ctx.path().join("src")).unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY src/*.rs /opt/app/\n",
        )
        .unwrap();
        std::fs::write(ctx.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(ctx.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        std::fs::write(ctx.path().join("src/notes.txt"), "not rust\n").unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-glob:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/opt/app/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/opt/app/lib.rs")).unwrap(),
            "pub fn f() {}\n"
        );
        assert!(
            !dir.join("rootfs/opt/app/notes.txt").exists(),
            "the wildcard must only match *.rs"
        );
    }

    // A wildcard that matches nothing is an error, not a silent no-op.
    #[tokio::test]
    async fn copy_wildcard_with_no_match_is_an_error() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(ctx.path().join("src")).unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY src/*.rs /src/\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let err = build_local_image(
            "c2-glob-miss:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .expect_err("a wildcard matching nothing must fail the build");
        assert!(
            err.to_string().contains("matched no files"),
            "unexpected error: {err}"
        );
    }

    // ── #35: a tag is not a path component ────────────────────────────────────

    fn colon_free_ctx() -> tempfile::TempDir {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(ctx.path().join("Dockerfile"), "FROM scratch\nCOPY . /app\n").unwrap();
        std::fs::write(ctx.path().join("file.txt"), "hi").unwrap();
        ctx
    }

    /// A colon tag used to become a colon directory, which put a colon in every
    /// host path derived from it - and PRoot splits bind arguments on ':', so the
    /// container silently lost `/dev/shm` and the `/proc` stubs.
    #[tokio::test]
    async fn a_colon_tag_builds_under_a_colon_free_store_key() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2env:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .expect("a colon tag must build, not silently produce an unbindable store");

        assert_eq!(
            dir.file_name().unwrap(),
            "c2env",
            "store key must be the tag with the colon stripped"
        );
        assert!(
            !dir.to_string_lossy().contains(':'),
            "no host path under the store may contain ':': {}",
            dir.display()
        );
        assert!(!store.path().join("c2env:latest").exists());

        // The original tag stays recorded, so the image can still be identified.
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["name"], "c2env:latest");

        // And the binds derived from that directory are expressible: exactly one
        // ':' per argument, which is the separator PRoot splits on.
        let container_dir = dir.parent().unwrap();
        let shm = proot_distro_core::guestdirs::ensure(container_dir).unwrap();
        assert_eq!(shm.parent().unwrap(), container_dir);
        let binds = proot_distro_core::guestdirs::bindings(container_dir);
        assert!(!binds.is_empty(), "the shared-memory bind must exist");
        for b in &binds {
            assert_eq!(
                b.matches(':').count(),
                1,
                "bind '{b}' must be <source>:<target> with no colon in the source"
            );
            assert!(b.ends_with(":/dev/shm"), "unexpected bind: {b}");
        }
        if let Ok(_sys) = proot_distro_core::sysdata::ensure(container_dir) {
            for b in proot_distro_core::sysdata::bindings(container_dir) {
                assert_eq!(b.matches(':').count(), 1, "unexpressible bind: {b}");
            }
        }
    }

    /// Two tags that derive to the same key must not silently overwrite each
    /// other: rebuilding `app:v2` over `app:v1` would destroy an image the user
    /// still refers to by name.
    #[tokio::test]
    async fn a_different_tag_deriving_to_the_same_key_is_refused() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        build_local_image("app:v1", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect("first build");

        let err = build_local_image("app:v2", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect_err("a second tag mapping onto the same store key must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("app:v1"),
            "error must name the existing image: {msg}"
        );
        assert!(msg.contains("would destroy"), "error must say why: {msg}");

        // The first image is untouched.
        let meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(store_path.join("app").join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["name"], "app:v1");
    }

    #[tokio::test]
    async fn rebuilding_the_same_tag_still_replaces_it() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        let first = build_local_image("app:v1", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect("first build");
        let second = build_local_image("app:v1", None, ctx.path(), Some(store_path))
            .await
            .expect("rebuilding the same tag must stay allowed");
        assert_eq!(first, second);
        assert!(second.join("rootfs").is_dir());
    }

    /// The guard for user-configured bind mounts: a colon source cannot be
    /// expressed, so it must be an error rather than a dropped mount. The
    /// store-key fix above cannot cover a path the user chose.
    #[test]
    fn a_bind_source_with_a_colon_is_refused() {
        let bad = proot_distro_core::bindings::BindMount {
            source: "/home/u/.podnext/containers/c2env:latest/shm".to_string(),
            target: "/dev/shm".to_string(),
            readonly: false,
            recursive: false,
            private: false,
        };
        let err = bad
            .to_argument()
            .expect_err("a colon source must be refused, not silently dropped");
        assert!(err.to_string().contains("c2env:latest"));

        let good = proot_distro_core::bindings::BindMount {
            source: "/srv/data".to_string(),
            target: "/data".to_string(),
            readonly: true,
            recursive: false,
            private: false,
        };
        assert_eq!(good.to_argument().unwrap(), "/srv/data:/data:ro");
    }

    #[tokio::test]
    async fn build_local_metadata_splits_image_and_tag() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        let dir = build_local_image("mycustom/repo:v1.2", None, ctx.path(), Some(store_path))
            .await
            .expect("build image");

        let meta_file = dir.join("metadata.json");
        assert!(meta_file.exists());
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(meta_file).unwrap()).unwrap();
        assert_eq!(meta["name"], "mycustom/repo:v1.2");
        assert_eq!(meta["image"], "mycustom/repo");
        assert_eq!(meta["tag"], "v1.2");
        assert_eq!(meta["base_image"], "scratch");
    }
}
