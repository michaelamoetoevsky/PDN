//! Network commands: `podnext network …`.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::network::*`.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use proot_distro_core::network::NetworkConfig;

use crate::{output_json, output_table, output_yaml, NetworkCommands, OutputFormat};

pub async fn execute_network_command(cmd: NetworkCommands) -> Result<()> {
    match cmd {
        NetworkCommands::Create {
            name,
            driver,
            subnet,
            gateway,
        } => cmd_network_create(name, driver, subnet, gateway).await,
        NetworkCommands::Ls { format } => cmd_network_ls(format).await,
        NetworkCommands::Inspect { networks, format } => {
            cmd_network_inspect(networks, format).await
        }
        NetworkCommands::Rm { networks } => cmd_network_rm(networks).await,
        NetworkCommands::Connect {
            network,
            container,
            ip,
        } => cmd_network_connect(network, container, ip).await,
        NetworkCommands::Disconnect { network, container } => {
            cmd_network_disconnect(network, container).await
        }
    }
}

pub async fn cmd_network_create(
    name: String,
    driver: String,
    subnet: Option<String>,
    gateway: Option<String>,
) -> Result<()> {
    proot_distro_core::NameValidator::new().validate_network_name(&name)?;
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let config = NetworkConfig {
        name: name.clone(),
        driver,
        subnet,
        gateway,
        dns_servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
        enable_ipv6: false,
        mtu: None,
        ..Default::default()
    };
    core.network_manager().create(&config)?;
    println!(
        "Network '{}' created successfully",
        proot_distro_core::escape_name(&name)
    );
    Ok(())
}

pub async fn cmd_network_ls(format: OutputFormat) -> Result<()> {
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let networks = core.network_manager().list()?;

    #[derive(Tabled, Serialize)]
    struct NetworkRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DRIVER")]
        driver: String,
        #[tabled(rename = "SUBNET")]
        subnet: String,
        #[tabled(rename = "GATEWAY")]
        gateway: String,
    }

    let rows: Vec<NetworkRow> = networks
        .iter()
        .map(|n| NetworkRow {
            name: n.name.clone(),
            driver: n.driver.clone(),
            subnet: n.subnet.clone().unwrap_or_default(),
            gateway: n.gateway.clone().unwrap_or_default(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&networks)?,
        OutputFormat::Yaml => output_yaml(&networks)?,
        OutputFormat::Text => {
            for n in networks {
                println!(
                    "{} {} {} {}",
                    n.name,
                    n.driver,
                    n.subnet.unwrap_or_default(),
                    n.gateway.unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}

pub async fn cmd_network_inspect(networks: Vec<String>, format: OutputFormat) -> Result<()> {
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let mut results = Vec::new();

    for name in networks {
        match core.network_manager().inspect(&name) {
            Ok(info) => results.push(info),
            Err(e) => eprintln!("Network '{}': {}", proot_distro_core::escape_name(&name), e),
        }
    }

    #[derive(Tabled, Serialize)]
    struct NetworkRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DRIVER")]
        driver: String,
        #[tabled(rename = "SUBNET")]
        subnet: String,
        #[tabled(rename = "GATEWAY")]
        gateway: String,
        #[tabled(rename = "IPV6")]
        ipv6: String,
        #[tabled(rename = "MTU")]
        mtu: String,
    }

    let rows: Vec<NetworkRow> = results
        .iter()
        .map(|n| NetworkRow {
            name: n.name.clone(),
            driver: n.driver.clone(),
            subnet: n.subnet.clone().unwrap_or_default(),
            gateway: n.gateway.clone().unwrap_or_default(),
            ipv6: n.enable_ipv6.to_string(),
            mtu: n.mtu.map_or("".to_string(), |v| v.to_string()),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Yaml => output_yaml(&results)?,
        OutputFormat::Text => {
            for n in results {
                println!(
                    "{} {} {} {}",
                    n.name,
                    n.driver,
                    n.subnet.unwrap_or_default(),
                    n.gateway.unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}

pub async fn cmd_network_rm(networks: Vec<String>) -> Result<()> {
    let mut core = proot_distro_core::ProotDistroCore::new()?;
    let mut errors = 0;
    let validator = proot_distro_core::NameValidator::new();

    for name in &networks {
        if let Err(e) = validator.validate_network_name(name) {
            eprintln!(
                "Failed to remove network {}: {}",
                proot_distro_core::escape_name(name),
                e
            );
            errors += 1;
            continue;
        }
        match core.network_manager().remove(name) {
            Ok(_) => println!("Removed network: {}", proot_distro_core::escape_name(name)),
            Err(e) => {
                eprintln!(
                    "Failed to remove network {}: {}",
                    proot_distro_core::escape_name(name),
                    e
                );
                errors += 1;
            }
        }
    }
    if errors > 0 {
        anyhow::bail!("Failed to remove {} network(s)", errors);
    }
    Ok(())
}

pub async fn cmd_network_connect(
    network: String,
    container: String,
    ip: Option<String>,
) -> Result<()> {
    // Resolve the container first: an unknown name must fail before any state
    // is written, and we need its rootfs to apply the network's DNS.
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let target = container_mgr.get(&container).await?;

    let mut core = proot_distro_core::ProotDistroCore::new()?;
    core.network_manager().connect(
        &network,
        &container,
        ip.as_deref(),
        Some(target.rootfs_path.as_path()),
    )?;

    println!("Connected container {} to network {}", container, network);
    if let Some(ip) = ip {
        println!("IP: {}", ip);
    }
    Ok(())
}

pub async fn cmd_network_disconnect(network: String, container: String) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let target = container_mgr.get(&container).await?;

    let mut core = proot_distro_core::ProotDistroCore::new()?;
    core.network_manager()
        .disconnect(&network, &container, Some(target.rootfs_path.as_path()))?;

    println!(
        "Disconnected container {} from network {}",
        container, network
    );
    Ok(())
}
