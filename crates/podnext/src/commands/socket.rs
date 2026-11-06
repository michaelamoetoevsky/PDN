//! Socket-activation commands: `podnext socket register`, `unregister` and `list`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use socket::*;` in `commands/mod.rs`.

use anyhow::Result;
use std::path::PathBuf;

use pn_next::{
    ContainerSocketConfig, ContainerSocketManager, SocketActivationConfig, SocketActivationManager,
};
pub async fn execute_socket_command(cmd: crate::SocketCommands) -> Result<()> {
    match cmd {
        crate::SocketCommands::Register { config } => cmd_socket_register(config).await,
        crate::SocketCommands::Unregister { name } => cmd_socket_unregister(name).await,
        crate::SocketCommands::List { format } => cmd_socket_list(format).await,
    }
}

pub async fn cmd_socket_register(config: PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(&config)?;
    let socket_config: ContainerSocketConfig = toml::from_str(&content)?;

    let base_manager = SocketActivationManager::new(SocketActivationConfig::default());
    let manager = ContainerSocketManager::new(std::sync::Arc::new(base_manager));
    manager.register_container_socket(socket_config).await?;

    println!("Socket activation registered");
    Ok(())
}

pub async fn cmd_socket_unregister(name: String) -> Result<()> {
    println!("Unregistering socket '{}'...", name);
    Ok(())
}

pub async fn cmd_socket_list(format: crate::OutputFormat) -> Result<()> {
    // B9: "Socket activations:" is a heading, not a message, but a machine caller asked for data.
    match format {
        crate::OutputFormat::Json | crate::OutputFormat::Yaml => println!("[]"),
        _ => println!("Socket activations:"),
    }
    Ok(())
}
