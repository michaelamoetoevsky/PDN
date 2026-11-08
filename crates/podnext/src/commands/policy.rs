//! Policy-engine commands: `podnext policy eval`, `list`, `load` and `presets`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use policy::*;` in `commands/mod.rs`.

use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use tabled::Tabled;

use pn_next::{
    EvaluationContext, HealthStatus, PolicyEngine, PolicyEngineConfig, PolicyHealthStatus,
    PolicyResourceType, ResourceQuotaConfig, ResourceQuotaManager,
};

use super::{get_supervisor, output_json, output_table, output_yaml};
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
pub async fn cmd_policy_eval(name: String, format: crate::OutputFormat) -> Result<()> {
    let config = PolicyEngineConfig::default();
    let policy_dir = config.policy_dir.display().to_string();
    let engine = PolicyEngine::new(config);

    engine.load_policies().await?;
    let rules = engine.get_rules().await;

    if rules.is_empty() {
        // B9: both lines are messages. The data channel answers per format, and a count is a datum.
        eprintln!("No policy rules found in {}", policy_dir);
        eprintln!("Add .yaml/.yml/.toml rule files there, then re-run.");
        match format {
            crate::OutputFormat::Json | crate::OutputFormat::Yaml => println!("[]"),
            _ => println!("rules: 0"),
        }
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
    // The engine decides; nothing applies the decision. Without this line `action : Deny` reads as
    // "the action that was taken" (ASEC2-ARCH-008, measured 2026-11-08: `PolicyAction` is defined in
    // pn-next/src/security.rs and re-exported, but nothing outside this engine consumes it).
    println!("  applied by : nothing - this command evaluates; no component enforces the action");
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

pub async fn cmd_policy_list(tag: Option<String>, format: crate::OutputFormat) -> Result<()> {
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

pub async fn cmd_policy_load(file: PathBuf) -> Result<()> {
    println!("Loading policy from {}...", file.display());
    Ok(())
}

pub async fn cmd_policy_presets() -> Result<()> {
    println!("Available policy presets:");
    println!("  production-safety - Production safety policies (restart limits, OOM quarantine, high CPU warning)");
    println!("  development - Development environment policies (unlimited restarts)");
    Ok(())
}
