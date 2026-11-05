//! Volume commands: `podnext volume …`.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::volume::*`, so
//! the dispatch and the `--help` surface are unchanged (`scripts/dump_cli_help.py` proves it).

use anyhow::Result;
use serde::Serialize;
use std::io::{self, IsTerminal, Write};
use tabled::Tabled;

use proot_distro_core::storage::VolumeConfig;

use crate::{output_json, output_table, output_yaml, OutputFormat, VolumeCommands};

pub async fn execute_volume_command(cmd: VolumeCommands) -> Result<()> {
    match cmd {
        VolumeCommands::Create { name, driver, opts } => {
            cmd_volume_create(name, driver, opts).await
        }
        VolumeCommands::Ls { format } => cmd_volume_ls(format).await,
        VolumeCommands::Inspect { volumes, format } => cmd_volume_inspect(volumes, format).await,
        VolumeCommands::Rm {
            volumes,
            force,
            yes,
        } => cmd_volume_rm(volumes, force || yes).await,
        VolumeCommands::Prune { force, yes } => cmd_volume_prune(force || yes).await,
    }
}

pub async fn cmd_volume_create(name: String, driver: String, opts: Vec<String>) -> Result<()> {
    proot_distro_core::NameValidator::new().validate_volume_name(&name)?;
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let labels = std::collections::HashMap::new();
    let mut options = std::collections::HashMap::new();

    for opt in opts {
        if let Some((k, v)) = opt.split_once('=') {
            options.insert(k.to_string(), v.to_string());
        }
    }

    let config = VolumeConfig {
        name: name.clone(),
        driver,
        labels,
        options,
    };
    core.storage_manager().create_volume(&config)?;
    println!(
        "Volume '{}' created successfully",
        proot_distro_core::escape_name(&name)
    );
    Ok(())
}

pub async fn cmd_volume_ls(format: OutputFormat) -> Result<()> {
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let volumes = core.storage_manager().list_volumes()?;

    #[derive(Tabled, Serialize)]
    struct VolumeRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DRIVER")]
        driver: String,
        #[tabled(rename = "MOUNTPOINT")]
        mountpoint: String,
    }

    let rows: Vec<VolumeRow> = volumes
        .iter()
        .map(|v| VolumeRow {
            name: v.name.clone(),
            driver: v.driver.clone(),
            mountpoint: v.mountpoint.display().to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&volumes)?,
        OutputFormat::Yaml => output_yaml(&volumes)?,
        OutputFormat::Text => {
            for v in volumes {
                println!("{} {} {}", v.name, v.driver, v.mountpoint.display());
            }
        }
    }
    Ok(())
}

pub async fn cmd_volume_inspect(volumes: Vec<String>, format: OutputFormat) -> Result<()> {
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let mut results = Vec::new();

    for name in volumes {
        match core.storage_manager().inspect_volume(&name) {
            Ok(info) => results.push(info),
            Err(e) => eprintln!("Volume '{}': {}", proot_distro_core::escape_name(&name), e),
        }
    }

    #[derive(Tabled, Serialize)]
    struct VolumeRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DRIVER")]
        driver: String,
        #[tabled(rename = "MOUNTPOINT")]
        mountpoint: String,
        #[tabled(rename = "SIZE")]
        size: String,
    }

    let rows: Vec<VolumeRow> = results
        .iter()
        .map(|v| VolumeRow {
            name: v.name.clone(),
            driver: v.driver.clone(),
            mountpoint: v.mountpoint.display().to_string(),
            size: v.size.to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Yaml => output_yaml(&results)?,
        OutputFormat::Text => {
            for v in results {
                println!("{} {} {}", v.name, v.driver, v.mountpoint.display());
            }
        }
    }
    Ok(())
}

pub async fn cmd_volume_rm(volumes: Vec<String>, force: bool) -> Result<()> {
    use std::io::IsTerminal;
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let mut errors = 0;
    let validator = proot_distro_core::NameValidator::new();

    // Check existence and validate first (Finding 04-4, 04-5, 05-2)
    for name in &volumes {
        validator.validate_volume_name(name)?;
        core.storage_manager().inspect_volume(name)?;
    }

    if !force {
        use std::io::IsTerminal;
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "stdin is not a tty: use -f/--force or -y/--yes to confirm removal of volume(s)"
            );
        }
        for name in &volumes {
            eprint!(
                "Remove volume '{}'? [y/N]: ",
                proot_distro_core::escape_name(name)
            );
            io::stderr().flush()?;
            let mut input = String::new();
            let bytes = io::stdin().read_line(&mut input)?;
            if bytes == 0 || !input.trim().eq_ignore_ascii_case("y") {
                eprintln!("Cancelled.");
                std::process::exit(1);
            }
        }
    }

    for name in &volumes {
        match core.storage_manager().remove_volume(name) {
            Ok(_) => println!("Removed volume: {}", proot_distro_core::escape_name(name)),
            Err(e) => {
                eprintln!(
                    "Failed to remove volume {}: {}",
                    proot_distro_core::escape_name(name),
                    e
                );
                errors += 1;
            }
        }
    }
    if errors > 0 {
        anyhow::bail!("Failed to remove {} volume(s)", errors);
    }
    Ok(())
}

pub async fn cmd_volume_prune(force: bool) -> Result<()> {
    use proot_distro_core::ContainerManager;
    use std::path::PathBuf;

    // A volume is "in use" when some container mounts a path inside it. The
    // container config is the only place that records this, so gather the mount
    // sources before touching anything.
    let container_mgr = ContainerManager::new()?;
    let mut mounted: Vec<PathBuf> = Vec::new();
    for info in container_mgr.list().await? {
        if let Ok(c) = container_mgr.get(&info.name).await {
            for b in &c.config.bind_mounts {
                mounted.push(PathBuf::from(&b.source));
            }
        }
    }

    let mut core = proot_distro_core::ProotDistroCore::new()?;

    // Decide the work set first, so the confirmation prompt lists exactly what
    // will be deleted rather than re-scanning afterwards.
    let mut unused: Vec<String> = Vec::new();
    for v in core.storage_manager().list_volumes()? {
        let used = mounted.iter().any(|m| m.starts_with(&v.mountpoint));
        if used {
            println!("Skipping in-use volume: {}", v.name);
        } else {
            unused.push(v.name);
        }
    }

    if unused.is_empty() {
        println!("No unused volumes to prune.");
        return Ok(());
    }

    if !force {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            anyhow::bail!("stdin is not a tty: use -f/--force or -y/--yes to confirm volume prune");
        }
        eprintln!(
            "Will remove {} volume(s): {}",
            unused.len(),
            unused.join(", ")
        );
        eprint!("Continue? [y/N]: ");
        std::io::Write::flush(&mut std::io::stderr())?;
        let mut input = String::new();
        let bytes = std::io::stdin().read_line(&mut input)?;
        if bytes == 0 || !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("Cancelled.");
            std::process::exit(1);
        }
    }

    let mut pruned = 0usize;
    for name in &unused {
        match core.storage_manager().remove_volume(name) {
            Ok(_) => {
                println!("Pruned volume: {}", name);
                pruned += 1;
            }
            Err(e) => eprintln!("Failed to prune volume {}: {}", name, e),
        }
    }
    println!("Pruned {} volume(s)", pruned);
    Ok(())
}
