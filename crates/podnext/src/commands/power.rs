//! The `boot` and `wakelock` supervision commands, extracted from `commands/mod.rs`.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use super::{get_supervisor, output_json, output_table, output_yaml};
use pn_next::{TermuxBootConfig, TermuxBootManager, WakeLockConfig, WakeLockManager, WakeLockType};

pub async fn execute_boot_command(cmd: crate::BootCommands) -> Result<()> {
    match cmd {
        crate::BootCommands::Enable { name } => cmd_boot_enable(name).await,
        crate::BootCommands::Disable { name } => cmd_boot_disable(name).await,
        crate::BootCommands::Status { format } => cmd_boot_status(format).await,
    }
}

pub async fn cmd_boot_enable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.initialize().await?;
    boot_manager.add_auto_start_service(&name).await?;
    println!("Service '{}' enabled for Termux:Boot auto-start", name);
    Ok(())
}

pub async fn cmd_boot_disable(name: String) -> Result<()> {
    let supervisor = get_supervisor().await?;
    let _ = supervisor.get_status(&name).await?;

    let mut boot_manager = TermuxBootManager::new(TermuxBootConfig::default());
    boot_manager.remove_auto_start_service(&name).await?;
    println!("Service '{}' disabled for Termux:Boot auto-start", name);
    Ok(())
}

pub async fn cmd_boot_status(format: crate::OutputFormat) -> Result<()> {
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

pub async fn execute_wakelock_command(cmd: crate::WakelockCommands) -> Result<()> {
    match cmd {
        crate::WakelockCommands::Acquire { name, lock_type } => {
            cmd_wakelock_acquire(name, lock_type).await
        }
        crate::WakelockCommands::Release { name } => cmd_wakelock_release(name).await,
        crate::WakelockCommands::Status { format } => cmd_wakelock_status(format).await,
    }
}

pub async fn cmd_wakelock_acquire(name: String, lock_type: crate::WakeLockTypeArg) -> Result<()> {
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

pub async fn cmd_wakelock_release(name: String) -> Result<()> {
    let wake_manager = WakeLockManager::new(WakeLockConfig::default());
    wake_manager.release(&name).await?;
    println!("Wake lock released for '{}'", name);
    Ok(())
}

pub async fn cmd_wakelock_status(format: crate::OutputFormat) -> Result<()> {
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
