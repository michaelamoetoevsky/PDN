//! Resource quota commands: `podnext quota set`, `get`, `usage` and `list`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use quota::*;` in `commands/mod.rs`.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use pn_next::{ResourceQuotaConfig, ResourceQuotaManager};

use super::{get_supervisor, output_json, output_table, output_yaml};
use crate::utils::format_size;
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

pub async fn cmd_quota_set(
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

pub async fn cmd_quota_get(name: String, format: crate::OutputFormat) -> Result<()> {
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
pub async fn cmd_quota_usage(name: String, _format: crate::OutputFormat) -> Result<()> {
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

pub async fn cmd_quota_list(format: crate::OutputFormat) -> Result<()> {
    let mut quota_manager = ResourceQuotaManager::new(ResourceQuotaConfig::default());
    quota_manager.load_quotas()?;
    let all = quota_manager.all_quotas();

    if all.is_empty() {
        // B9: the sentence is a message, so it goes to stderr; the data channel still answers in the
        // requested format. `[]` is valid JSON and valid YAML, so one line serves both machine formats.
        eprintln!("No per-service quotas set. (Defaults still apply to every service.)");
        match format {
            crate::OutputFormat::Json | crate::OutputFormat::Yaml => println!("[]"),
            _ => println!(
                "{:<24} {:>10} {:>12} {:>12} {:>10}",
                "SERVICE", "CPU", "MEMORY", "SWAP", "PIDS"
            ),
        }
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

pub fn parse_quota_limit(limit: &str, resource: crate::ResourceTypeArg) -> Result<u64> {
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
