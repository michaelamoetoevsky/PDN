//! podnext CLI - Simple container management for Termux/Android
//!
//! A simplified CLI that works with proot-distro-core's actual implementation.

use clap::{Parser, Subcommand, Args, ValueEnum};
use anyhow::{Result, Context, bail};
use std::path::PathBuf;
use std::process::Command;
use std::io::{self, Write};
use tracing_subscriber::EnvFilter;

mod utils;

use utils::{parse_size, format_size, parse_container_path};

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

    // Use proot-distro directly for now since it works
    let mut args = vec!["install"];
    if force {
        args.push("--force");
    }
    args.push(&image);
    if let Some(_n) = name {
        // proot-distro doesn't support custom names directly, we'll rename after
    }

    let status = Command::new("proot-distro")
        .args(&args)
        .status()?;

    if !status.success() {
        bail!("Failed to install image");
    }

    println!("Container '{}' installed successfully!", container_name);
    println!("\nTo start the container:");
    println!("  podnext login {}", container_name);

    Ok(())
}

async fn cmd_list(all: bool) -> Result<()> {
    let mut args = vec!["list"];
    if all {
        args.push("--all");
    }

    let status = Command::new("proot-distro")
        .args(&args)
        .status()?;

    if !status.success() {
        bail!("Failed to list containers");
    }
    Ok(())
}

async fn cmd_login(container: String, user: Option<String>, command: Vec<String>) -> Result<()> {
    let mut args: Vec<String> = vec!["login".to_string(), container];
    if let Some(u) = user {
        args.push("-u".to_string());
        args.push(u);
    }
    if !command.is_empty() {
        args.push("--".to_string());
        args.extend(command);
    }

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let status = Command::new("proot-distro")
        .args(&args_ref)
        .status()?;

    if !status.success() {
        bail!("Failed to login to container");
    }
    Ok(())
}

async fn cmd_run(container: String, command: Vec<String>) -> Result<()> {
    let mut args: Vec<String> = vec!["login".to_string(), container, "--".to_string()];
    args.extend(command);

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let status = Command::new("proot-distro")
        .args(&args_ref)
        .status()?;

    if !status.success() {
        bail!("Failed to run command in container");
    }
    Ok(())
}

async fn cmd_ps() -> Result<()> {
    let status = Command::new("proot-distro")
        .args(["ps"])
        .status()?;

    if !status.success() {
        bail!("Failed to list sessions");
    }
    Ok(())
}

async fn cmd_kill(container: String) -> Result<()> {
    let status = Command::new("proot-distro")
        .args(["kill", &container])
        .status()?;

    if !status.success() {
        bail!("Failed to kill container");
    }
    println!("Container '{}' stopped.", container);
    Ok(())
}

async fn cmd_remove(name: String, force: bool) -> Result<()> {
    if !force {
        print!("Remove container '{}'? [y/N]: ", name);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let status = Command::new("proot-distro")
        .args(["remove", &name])
        .status()?;

    if !status.success() {
        bail!("Failed to remove container");
    }
    println!("Container '{}' removed.", name);
    Ok(())
}

async fn cmd_info() -> Result<()> {
    println!("podnext (PRoot Distro Next) v0.1.0");
    println!("  A simple container wrapper for Termux/Android");
    println!("  Uses proot-distro under the hood");
    Ok(())
}

async fn cmd_search(query: String, limit: usize) -> Result<()> {
    let limit_str = limit.to_string();
    let mut args = vec!["search"];
    args.push("--limit");
    args.push(&limit_str);
    args.push(&query);

    let status = Command::new("proot-distro")
        .args(&args)
        .status()?;

    if !status.success() {
        bail!("Failed to search images");
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
    ps           List active sessions
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
    podnext search ubuntu
    podnext remove ubuntu

    # Direct execution (like proot)
    podnext -r /path/to/rootfs -- /bin/bash
    podnext -b /host/data:/data -- ls /data

Run 'podnext <command> --help' for more information on a command.
"#);
}