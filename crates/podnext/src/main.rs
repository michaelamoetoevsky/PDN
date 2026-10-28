//! podnext CLI - Command line interface for PRoot Distro Next
//!
//! This CLI mimics the simplicity of proot and proot-distro while providing
//! next-generation container management capabilities.

use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "podnext")]
#[command(about = "PRoot Distro Next - Next generation container management for Termux")]
#[command(version = "0.1.0")]
#[command(
    long_about = "podnext (PRoot Distro Next) - A unified container platform combining PRoot, PRoot-distro, and proot-compat into a single Rust codebase with gateway layer, fail-safe mechanisms, and guardrails.\n\n\
    Like proot-distro, it manages Linux containers on Termux/Android without root.\n\
    Like proot, it can execute commands directly in a proot environment.\n\n\
    EXAMPLES:\n    podnext install ubuntu:22.04          # Install Ubuntu 22.04\n    podnext login ubuntu                    # Interactive shell\n    podnext run ubuntu -- apt update        # Run command in container\n    podnext list                            # List containers\n    podnext remove ubuntu                   # Delete container\n\n\
    DIRECT EXECUTION (like proot):\n    podnext -r /path/to/rootfs -- /bin/bash # Execute in proot environment\n    podnext -b /host/path:/guest/path -- ls # Bind mount and execute\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    // Direct execution mode (like proot)
    #[arg(short = 'r', long = "rootfs", global = true, help = "Use PATH as guest root filesystem")]
    rootfs: Option<String>,

    #[arg(short = 'b', long = "bind", global = true, value_name = "HOST_PATH:GUEST_PATH", help = "Bind mount host path into guest (can be used multiple times)")]
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

    #[arg(short = 'e', long = "env", global = true, value_name = "VAR=VALUE", help = "Set environment variable")]
    env: Vec<String>,

    // Command to execute (when using direct execution mode) - trailing arguments
    #[arg(last = true, global = true, help = "Command to execute in proot environment")]
    cmd_args: Vec<String>,
}

#[derive(Subcommand)]
#[command(rename_all = "lower")]
enum Commands {
    /// Install distribution from OCI image or rootfs archive
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

    /// List created containers or cached images
    List {
        /// Show images instead of containers
        #[arg(short = 'i', long)]
        images: bool,

        /// Show all (including stopped)
        #[arg(short = 'a', long)]
        all: bool,
    },

    /// Start interactive shell inside a container
    Login {
        /// Container name
        container: String,

        /// User to run as (name, name:group, uid, or uid:gid)
        #[arg(short, long)]
        user: Option<String>,

        /// Enable isolated mode (minimal host bindings)
        #[arg(long)]
        isolated: bool,

        /// Minimal mode (only /dev, /proc, /sys bound)
        #[arg(long)]
        minimal: bool,

        /// Bind host home directory
        #[arg(long)]
        shared_home: bool,

        /// Bind host tmp directory
        #[arg(long)]
        shared_tmp: bool,

        /// Custom bind mount (can be used multiple times)
        #[arg(short = 'b', long = "bind", value_name = "SRC:DEST")]
        bind: Vec<String>,

        /// Working directory
        #[arg(long = "work-dir")]
        work_dir: Option<String>,

        /// Environment variables
        #[arg(short = 'e', long = "env", value_name = "VAR=VALUE")]
        env: Vec<String>,

        /// Detach and run in background
        #[arg(short = 'd', long)]
        detach: bool,

        /// Print proot command and exit
        #[arg(long = "get-proot-cmd")]
        get_proot_cmd: bool,

        /// Command to run instead of shell (after --)
        command: Vec<String>,
    },

    /// Run container entrypoint or custom command
    Run {
        /// Container name
        container: String,

        /// Redirect privileged ports (22->2022, 80->2080, etc.)
        #[arg(long = "redirect-ports")]
        redirect_ports: bool,

        /// Command to run (after --)
        command: Vec<String>,
    },

    /// List active container sessions
    Ps {
        /// Show all sessions
        #[arg(short = 'a', long)]
        all: bool,
    },

    /// Stop active container sessions
    Kill {
        /// Container name or PID
        target: String,

        /// Signal to send
        #[arg(short, long, default_value = "TERM")]
        signal: String,
    },

    /// Delete a container or cached image
    Remove {
        /// Name of container or image to remove
        name: String,

        /// Remove image instead of container
        #[arg(short = 'i', long)]
        image: bool,

        /// Force remove without confirmation
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Rename a container
    Rename {
        /// Current container name
        old_name: String,

        /// New container name
        new_name: String,
    },

    /// Reinstall a container from scratch (destroys data!)
    Reset {
        /// Container name
        container: String,

        /// New image to install from (optional)
        #[arg(short, long)]
        image: Option<String>,
    },

    /// Save container as a TAR archive
    Backup {
        /// Container name
        container: String,

        /// Output file (defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Restore container from a TAR archive
    Restore {
        /// Container name
        container: String,

        /// Input archive file (defaults to stdin)
        #[arg(short, long)]
        input: Option<String>,
    },

    /// Delete cached downloads
    #[command(name = "clear-cache")]
    ClearCache,

    /// Copy files from/to container
    Copy {
        /// Source path (container:path or local path)
        src: String,

        /// Destination path (container:path or local path)
        dest: String,
    },

    /// Sync files from/to container
    Sync {
        /// Source path (container:path or local path)
        src: String,

        /// Destination path (container:path or local path)
        dest: String,
    },

    /// Build an OCI image from a Dockerfile
    Build {
        /// Dockerfile path
        #[arg(short, long, default_value = "Dockerfile")]
        file: String,

        /// Tag for the image
        #[arg(short, long)]
        tag: String,

        /// Build context directory
        context: String,
    },

    /// Push a locally built image to a registry
    Push {
        /// Image reference to push
        image: String,
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
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Direct execution mode (like proot) - when no subcommand but rootfs or cmd_args provided
    if cli.command.is_none() && (cli.rootfs.is_some() || !cli.cmd_args.is_empty()) {
        return execute_direct(&cli).await;
    }

    // Subcommand mode (like proot-distro)
    if let Some(cmd) = cli.command {
        return execute_command(cmd).await;
    }

    // Default: show help
    print_help();
    Ok(())
}

async fn execute_direct(cli: &Cli) -> Result<()> {
    println!("podnext direct execution mode (like proot)");
    println!("  Rootfs: {:?}", cli.rootfs);
    println!("  Binds: {:?}", cli.bind);
    println!("  Working dir: {:?}", cli.pwd);
    println!("  QEMU: {:?}", cli.qemu);
    println!("  Link2symlink: {}", cli.link2symlink);
    println!("  No sysvipc: {}", cli.no_sysvipc);
    println!("  Kernel: {:?}", cli.kernel);
    println!("  Hostname: {:?}", cli.hostname);
    println!("  Env: {:?}", cli.env);
    println!("  Command: {:?}", cli.cmd_args);

    // TODO: Implement actual proot execution via proot-core
    // This would use the proot-core crate to ptrace and execute the command

    if cli.cmd_args.is_empty() {
        eprintln!("Error: No command specified for direct execution");
        std::process::exit(1);
    }

    Ok(())
}

async fn execute_command(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Install { image, name, force } => {
            let container_name = name.unwrap_or_else(|| {
                image.split(':').next().unwrap_or(&image).to_string()
            });
            println!("Installing '{}' as '{}'{}", image, container_name, if force { " (force)" } else { "" });
            // TODO: Implement via proot-distro-core + pn-gateway
        }
        Commands::List { images, all } => {
            if images {
                println!("Listing cached images{}", if all { " (all)" } else { "" });
            } else {
                println!("Listing containers{}", if all { " (all)" } else { "" });
            }
            // TODO: Implement
        }
        Commands::Login { container, user, isolated, minimal, shared_home, shared_tmp, bind, work_dir, env, detach, get_proot_cmd, command } => {
            println!("Logging into '{}'", container);
            if let Some(u) = user { println!("  User: {}", u); }
            if isolated { println!("  Isolated mode: enabled"); }
            if minimal { println!("  Minimal mode: enabled"); }
            if shared_home { println!("  Shared home: enabled"); }
            if shared_tmp { println!("  Shared tmp: enabled"); }
            for b in bind { println!("  Bind: {}", b); }
            if let Some(w) = work_dir { println!("  Work dir: {}", w); }
            for e in env { println!("  Env: {}", e); }
            if detach { println!("  Detached: true"); }
            if get_proot_cmd { println!("  Get proot cmd: true"); }
            if !command.is_empty() { println!("  Command: {:?}", command); }
            // TODO: Implement via proot-distro-core + pn-gateway
        }
        Commands::Run { container, redirect_ports, command } => {
            println!("Running '{}'{}", container, if redirect_ports { " (redirect ports)" } else { "" });
            if !command.is_empty() { println!("  Command: {:?}", command); }
            // TODO: Implement
        }
        Commands::Ps { all } => {
            println!("Active sessions{}", if all { " (all)" } else { "" });
            // TODO: Implement
        }
        Commands::Kill { target, signal } => {
            println!("Killing '{}' with signal {}", target, signal);
            // TODO: Implement
        }
        Commands::Remove { name, image, force } => {
            println!("Removing {}{} (force: {})", if image { "image" } else { "container" }, name, force);
            // TODO: Implement
        }
        Commands::Rename { old_name, new_name } => {
            println!("Renaming '{}' -> '{}'", old_name, new_name);
            // TODO: Implement
        }
        Commands::Reset { container, image } => {
            println!("Resetting '{}'{}", container, image.map(|i| format!(" with image '{}'", i)).unwrap_or_default());
            // TODO: Implement
        }
        Commands::Backup { container, output } => {
            println!("Backing up '{}'{}", container, output.map(|o| format!(" to '{}'", o)).unwrap_or(" to stdout".to_string()));
            // TODO: Implement
        }
        Commands::Restore { container, input } => {
            println!("Restoring '{}'{}", container, input.map(|i| format!(" from '{}'", i)).unwrap_or(" from stdin".to_string()));
            // TODO: Implement
        }
        Commands::ClearCache => {
            println!("Clearing cache...");
            // TODO: Implement
        }
        Commands::Copy { src, dest } => {
            println!("Copying '{}' -> '{}'", src, dest);
            // TODO: Implement
        }
        Commands::Sync { src, dest } => {
            println!("Syncing '{}' -> '{}'", src, dest);
            // TODO: Implement
        }
        Commands::Build { file, tag, context } => {
            println!("Building '{}' from '{}' with context '{}'", tag, file, context);
            // TODO: Implement
        }
        Commands::Push { image } => {
            println!("Pushing '{}'", image);
            // TODO: Implement
        }
        Commands::Search { query, limit } => {
            println!("Searching for '{}' (limit: {})", query, limit);
            // TODO: Implement
        }
        Commands::Info => {
            println!("podnext (PRoot Distro Next) v0.1.0");
            println!("  A unified container platform for Termux/Android");
            println!("  Combining PRoot, PRoot-distro, and proot-compat");
        }
        Commands::Version => {
            println!("podnext 0.1.0");
        }
    }
    Ok(())
}

fn print_help() {
    println!(r#"podnext (PRoot Distro Next) - Next generation container management for Termux

USAGE:
    podnext [SUBCOMMAND] [OPTIONS]
    podnext [OPTIONS] -- COMMAND [ARGS]...    # Direct execution mode (like proot)

SUBCOMMANDS (like proot-distro):
    install      Install distribution from OCI image
    list         List containers or images
    login        Start interactive shell in container
    run          Run command in container
    ps           List active sessions
    kill         Stop container sessions
    remove       Delete container or image
    rename       Rename a container
    reset        Reinstall container from scratch
    backup       Save container as TAR archive
    restore      Restore container from TAR archive
    clear-cache  Delete cached downloads
    copy         Copy files from/to container
    sync         Sync files from/to container
    build        Build OCI image from Dockerfile
    push         Push image to registry
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
    # Container management (like proot-distro)
    podnext install ubuntu:22.04
    podnext login ubuntu
    podnext run ubuntu -- apt update
    podnext list
    podnext remove ubuntu

    # Direct execution (like proot)
    podnext -r /path/to/rootfs -- /bin/bash
    podnext -b /host/data:/data -- ls /data

Run 'podnext <command> --help' for more information on a command.
"#);
}