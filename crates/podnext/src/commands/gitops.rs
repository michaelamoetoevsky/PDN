//! `podnext gitops` and `podnext dashboard` command implementations.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split.

use super::{output_json, output_table, output_yaml};
use anyhow::Result;
use pn_next::{
    GitOpsConfig, GitOpsManager, GitOpsResourceStatus, SyncStatus, WebDashboard, WebDashboardConfig,
};
use serde::Serialize;
use tabled::Tabled;

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
pub async fn gitops_manager() -> Result<GitOpsManager> {
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

pub async fn cmd_gitops_sync(force: bool) -> Result<()> {
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

pub async fn cmd_gitops_status(format: crate::OutputFormat) -> Result<()> {
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

pub async fn cmd_gitops_resources(kind: Option<String>, format: crate::OutputFormat) -> Result<()> {
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
pub async fn cmd_gitops_drift(format: crate::OutputFormat) -> Result<()> {
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
        // B9: a message for a human, and an empty list for a machine.
        eprintln!("No drift recorded: every synced resource last reported as Synced.");
        match format {
            crate::OutputFormat::Json | crate::OutputFormat::Yaml => println!("[]"),
            _ => println!("drifted: 0"),
        }
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
    eprintln!(
        "Note: this reports the last recorded sync status. Comparing against a live \
         cluster is not implemented - PDN has no cluster adapter, so real drift \
         cannot be detected yet."
    );
    Ok(())
}

pub async fn cmd_gitops_reconcile(name: Option<String>) -> Result<()> {
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

pub async fn cmd_dashboard_start(dashboard: WebDashboard, port: u16, bind: String) -> Result<()> {
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

pub async fn cmd_dashboard_stop(_dashboard: WebDashboard) -> Result<()> {
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

pub async fn cmd_dashboard_status(_dashboard: WebDashboard) -> Result<()> {
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
pub fn dashboard_pid_path() -> Result<std::path::PathBuf> {
    let home = proot_distro_core::paths::resolve_home()?;
    Ok(home.join(".podnext").join("dashboard.pid"))
}

pub fn read_pid(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn process_alive(pid: u32) -> bool {
    // `/proc/<pid>` disappears with the process; no signal needed, so nothing
    // is disturbed by a mere status check.
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}
