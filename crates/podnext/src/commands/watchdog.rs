//! The `watchdog` supervision commands, extracted from `commands/mod.rs`.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use super::{output_json, output_table, output_yaml};
use pn_next::{WatchdogConfig, WatchdogManager};

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

pub async fn cmd_watchdog_status(format: crate::OutputFormat) -> Result<()> {
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

pub async fn cmd_watchdog_set_policy(
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

pub async fn cmd_watchdog_reset(name: String) -> Result<()> {
    let watchdog = WatchdogManager::new(WatchdogConfig::default());
    watchdog.reset_service(&name).await?;
    println!("Watchdog state reset for '{}'", name);
    Ok(())
}

pub async fn cmd_watchdog_events(_limit: usize) -> Result<()> {
    Err(anyhow::anyhow!(
        "'watchdog events' is not implemented: the watchdog keeps its state in memory and records \
         transitions in the container log. Use `podnext watchdog status` for the current state and \
         `podnext log tail <service>` for the log."
    ))
}
