//! podnext CLI - Simple container management for Termux/Android
//!
//! A simplified CLI that works with proot-distro-core's actual implementation.

use clap::{Parser, Subcommand, Args, ValueEnum};
use anyhow::{Result, Context, bail};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod utils;
mod native;

use utils::{parse_size, format_size, parse_container_path};
use native::{
    search_docker_hub, install_image_native, list_containers_native,
    login_container_native, run_container_native, list_sessions_native,
    kill_container_native, remove_container_native,
};
use proot_distro_core::{ContainerInfo, SessionInfo};

#[derive(Parser)]
#[command(name = "podnext")]
#[command(about = "PRoot Distro Next - Simple container management for Termux/Android")]
#[command(version = "0.1.0")]
#[command(
    long_about = "podnext (PRoot Distro Next) - Simple container management for Termux/Android.\n\n\
    EXAMPLES:\n    podnext install ubuntu:22.04              # Install Ubuntu 22.04\n    podnext login ubuntu                        # Interactive shell\n    podnext run ubuntu -- apt update            # Run command in container\n    podnext list                                # List containers\n    podnext remove ubuntu                       # Delete container\n\n\
    DIRECT EXECUTION (like proot):\n    podnext -r /path/to/rootfs -- /bin/bash     # Execute in proot environment\n    podnext -b /host/path:/guest/path -- ls     # Bind mount and execute\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    // Direct execution mode (like proot)
    #[arg(short = 'r', long = "rootfs", global = true, help = "Use PATH as guest root filesystem")]
    rootfs: Option<String>,

    #[arg(short = 'b', long = "bind", global = true, value_name = "HOST_PATH:GUEST_PATH", help = "Bind mount host path into guest (repeatable)")]
    bind: Vec<String>,

    #[arg(short = 'w', long = "pwd", global = true, help = "Set initial working directory")]
    pwd: Option<String>,

    #[arg(short = 'q', long = "qemu", global = true, help = "Execute through QEMU user-mode emulation")]
    qemu: Option<String>,

    #[arg(short = '0', long = "link2symlink", global = true, help = "Enable hardlink emulation via symlinks")]
    link2symlink: bool,

    #[arg(long = "no-sysvipc", global = true, help = "Disable System V IPC emulation")]
    no_sysvipc: bool,

    #[arg(long = "kernel", global = true, help = "Override kernel release string")]
    kernel: Option<String>,

    #[arg(long = "hostname", global = true, help = "Set hostname")]
    hostname: Option<String>,

    #[arg(short = 'e', long = "env", global = true, value_name = "VAR=VALUE", help = "Set environment variable (repeatable)")]
    env: Vec<String>,

    #[arg(last = true, global = true, help = "Command to execute in proot environment")]
    cmd_args: Vec<String>,
}

#[derive(Subcommand)]
#[command(rename_all = "lower")]
enum Commands {
    /// Install distribution from OCI image
    Install {
        /// Image reference (e.g., ubuntu:22.04, debian:12, alpine:latest)
        image: String,
        /// Container name (defaults to image name without tag)
        #[arg(short, long)]
        name: Option<String>,
        /// Force reinstall if container exists
        #[arg(short, long)]
        force: bool,
    },

    /// List created containers
    List {
        /// Show all (including stopped)
        #[arg(short = 'a', long)]
        all: bool,
    },

    /// Start interactive shell inside a container
    Login {
        /// Container name
        container: String,
        /// User to run as
        #[arg(short, long)]
        user: Option<String>,
        /// Command to run instead of shell (after --)
        #[arg(last = true)]
        command: Vec<String>,
    },

    /// Run command in container
    Run {
        /// Container name
        container: String,
        /// Command to run (after --)
        #[arg(last = true)]
        command: Vec<String>,
    },

    /// List active sessions
    Ps,

    /// Stop container
    Kill {
        /// Container name
        container: String,
    },

    /// Delete a container
    Remove {
        /// Container name
        name: String,
        /// Force remove without confirmation
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Search images on Docker Hub
    Search {
        /// Search query
        query: String,
        /// Limit results
        #[arg(short, long, default_value = "25")]
        limit: usize,
    },

    /// List active sessions (better formatted)
    Sessions,

    /// Attach to a container session (creates new session)
    Attach {
        /// Container name
        container: String,
        /// User to run as
        #[arg(short, long)]
        user: Option<String>,
        /// Command to run instead of shell (after --)
        command: Vec<String>,
    },

    /// Show system information
    #[command(name = "info")]
    Info,

    /// Show version
    #[command(name = "version")]
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Direct execution mode (like proot)
    if cli.command.is_none() && (cli.rootfs.is_some() || !cli.cmd_args.is_empty()) {
        return execute_direct(&cli).await;
    }

    // Subcommand mode
    if let Some(cmd) = cli.command {
        return execute_command(cmd).await;
    }

    print_help();
    Ok(())
}

async fn execute_direct(cli: &Cli) -> Result<()> {
    eprintln!("podnext direct execution mode (like proot)");
    eprintln!("  Rootfs: {:?}", cli.rootfs);
    eprintln!("  Binds: {:?}", cli.bind);
    eprintln!("  Working dir: {:?}", cli.pwd);
    eprintln!("  QEMU: {:?}", cli.qemu);
    eprintln!("  Link2symlink: {}", cli.link2symlink);
    eprintln!("  No sysvipc: {}", cli.no_sysvipc);
    eprintln!("  Kernel: {:?}", cli.kernel);
    eprintln!("  Hostname: {:?}", cli.hostname);
    eprintln!("  Env: {:?}", cli.env);
    eprintln!("  Command: {:?}", cli.cmd_args);

    if cli.cmd_args.is_empty() {
        bail!("No command specified for direct execution");
    }
    Ok(())
}

async fn execute_command(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Install { image, name, force } => cmd_install(image, name, force).await,
        Commands::List { all } => cmd_list(all).await,
        Commands::Login { container, user, command } => cmd_login(container, user, command).await,
        Commands::Run { container, command } => cmd_run(container, command).await,
        Commands::Ps => cmd_ps().await,
        Commands::Sessions => cmd_sessions().await,
        Commands::Attach { container, user, command } => cmd_attach(container, user, command).await,
        Commands::Kill { container } => cmd_kill(container).await,
        Commands::Remove { name, force } => cmd_remove(name, force).await,
        Commands::Search { query, limit } => cmd_search(query, limit).await,
        Commands::Info => cmd_info().await,
        Commands::Version => cmd_version().await,
    }
}

async fn cmd_install(image: String, name: Option<String>, force: bool) -> Result<()> {
    let container_name = name.clone().unwrap_or_else(|| {
        image.split(':').next().unwrap_or(&image).to_string()
    });

    println!("Installing '{}' as '{}'...", image, container_name);

    // Native installation using proot-distro-core + direct Docker Hub API
    let rootfs: PathBuf = install_image_native(&image, &container_name, force, None).await?;

    println!("\nContainer '{}' installed successfully!", container_name);
    println!("Rootfs: {}", rootfs.display());
    println!("\nTo start the container:");
    println!("  podnext login {}", container_name);

    Ok(())
}

async fn cmd_list(all: bool) -> Result<()> {
    let containers = list_containers_native(all).await?;

    println!("{:<20} {:<15} {:<15} {:<12} {}", "NAME", "DISTRO", "VERSION", "STATUS", "CREATED");
    println!("{}", "-".repeat(80));
    for c in containers {
        println!("{:<20} {:<15} {:<15} {:<12} {}", 
            c.name, c.distro, c.version, format!("{:?}", c.status), c.created_at.format("%Y-%m-%d %H:%M"));
    }
    Ok(())
}

async fn cmd_login(container: String, user: Option<String>, command: Vec<String>) -> Result<()> {
    let cmd = if command.is_empty() {
        vec!["/bin/bash".to_string()]
    } else {
        command
    };
    
    login_container_native(&container, user, cmd).await
}

async fn cmd_run(container: String, command: Vec<String>) -> Result<()> {
    run_container_native(&container, command).await
}

async fn cmd_ps() -> Result<()> {
    let sessions = list_sessions_native().await?;
    
    println!("{:<20} {:<15} {:<15} {:<10} {}", "CONTAINER", "USER", "PID", "STATUS", "STARTED");
    println!("{}", "-".repeat(75));
    for s in sessions {
        println!("{:<20} {:<15} {:<15} {:<10} {}", 
            s.container_name, s.user, 
            s.pid.map_or("-".to_string(), |p| p.to_string()), 
            s.status, s.started_at.format("%Y-%m-%d %H:%M"));
    }
    Ok(())
}

async fn cmd_sessions() -> Result<()> {
    let sessions = list_sessions_native().await?;
    
    println!("Active Sessions:");
    println!("{:<20} {:<15} {:<15} {:<10} {}", "CONTAINER", "USER", "PID", "STATUS", "STARTED");
    println!("{}", "-".repeat(75));
    for s in sessions {
        println!("{:<20} {:<15} {:<15} {:<10} {}", 
            s.container_name, s.user, 
            s.pid.map_or("-".to_string(), |p| p.to_string()), 
            s.status, s.started_at.format("%Y-%m-%d %H:%M"));
    }
    println!("\nNote: Use 'podnext attach <container>' to create a new session.");
    Ok(())
}

async fn cmd_attach(container: String, user: Option<String>, command: Vec<String>) -> Result<()> {
    let cmd = if command.is_empty() {
        vec!["/bin/bash".to_string()]
    } else {
        command
    };
    
    login_container_native(&container, user, cmd).await
}

async fn cmd_kill(container: String) -> Result<()> {
    kill_container_native(&container).await
}

async fn cmd_remove(name: String, force: bool) -> Result<()> {
    remove_container_native(name, force).await
}

async fn cmd_info() -> Result<()> {
    println!("podnext (PRoot Distro Next) v0.1.0");
    println!("  A simple container wrapper for Termux/Android");
    println!("  Uses proot-distro under the hood");
    Ok(())
}

async fn cmd_search(query: String, limit: usize) -> Result<()> {
    let images = search_docker_hub(&query, limit).await?;

    println!("{:<40} {:<50} {:<8} {:<10} {}", "NAME", "DESCRIPTION", "STARS", "PULLS", "OFFICIAL");
    println!("{}", "-".repeat(120));
    for img in images {
        let desc = if img.short_description.len() > 48 {
            format!("{}...", &img.short_description[..48])
        } else {
            img.short_description
        };
        println!("{:<40} {:<50} {:<8} {:<10} {}", 
            img.repo_name, desc, img.star_count, img.pull_count, 
            if img.is_official { "[OK]" } else { "" });
    }
    Ok(())
}

async fn cmd_version() -> Result<()> {
    println!("podnext 0.1.0");
    Ok(())
}

fn print_help() {
    println!(r#"podnext (PRoot Distro Next) - Simple container management for Termux/Android

USAGE:
    podnext [SUBCOMMAND] [OPTIONS]
    podnext [OPTIONS] -- COMMAND [ARGS]...    # Direct execution mode (like proot)

SUBCOMMANDS:
    install      Install distribution from OCI image
    list         List containers
    login        Start interactive shell in container
    run          Run command in container
    ps           List active sessions (raw)
    sessions     List active sessions (formatted)
    attach       Attach to container (creates new session)
    kill         Stop container
    remove       Delete container
    search       Search images on Docker Hub
    info         Show system information
    version      Show version
    help         Show this help

DIRECT EXECUTION OPTIONS (like proot):
    -r, --rootfs PATH       Guest root filesystem
    -b, --bind HOST:GUEST   Bind mount (repeatable)
    -w, --pwd PATH          Working directory
    -q, --qemu CMD          QEMU user-mode emulator
    -0, --link2symlink      Enable hardlink emulation
    --no-sysvipc            Disable System V IPC
    --kernel TEXT           Override kernel release
    --hostname TEXT         Set hostname
    -e, --env VAR=VALUE     Set environment variable

EXAMPLES:
    # Container management (simple and familiar)
    podnext install ubuntu:22.04
    podnext login ubuntu
    podnext run ubuntu -- apt update
    podnext list
    podnext sessions
    podnext attach ubuntu
    podnext search ubuntu
    podnext remove ubuntu

    # Direct execution (like proot)
    podnext -r /path/to/rootfs -- /bin/bash
    podnext -b /host/data:/data -- ls /data

Run 'podnext <command> --help' for more information on a command.
"#);
}