//! podnext CLI - Advanced container and service management for Termux/Android
//!
//! A comprehensive CLI that works with proot-distro-core, pn-next, and pn-gateway
//! to provide full container lifecycle management, service orchestration, and monitoring.

#![allow(deprecated)]
#![allow(future_incompatible)]

/// Trace-only logging, off unless `PN_DEBUG` is set in the environment.
///
/// These lines were unconditional upstream and flooded real output — a single
/// `backup create` emitted ~280 kB of DEBUG for one alpine container, burying
/// the result. Enable with `PN_DEBUG=1` when diagnosing.
macro_rules! pn_trace {
    ($($arg:tt)*) => {
        if ::std::env::var_os("PN_DEBUG").is_some() {
            ::std::eprintln!($($arg)*);
        }
    };
}
use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_yaml;
use std::io::{self, Write};
use std::path::PathBuf;
use tabled::{Table, Tabled};
use tracing_subscriber::EnvFilter;

mod commands;
mod doctor;
mod native;
mod utils;

use native::{
    install_image_native, kill_container_native, list_containers_native, list_sessions_native,
    list_sessions_native_full, login_container_native, parse_mount_specs, remove_container_native,
    run_container_native, search_docker_hub, verify_image_available,
};
use utils::format_size;

#[derive(Serialize, Clone)]
struct SerializableInfo {
    version: String,
    platform: String,
    rust_version: String,
    backend: String,
    isolation: String,
}

use commands::image::*;
use commands::network::*;
use commands::sync::*;
use commands::volume::*;
use commands::*;
use proot_distro_core::network::NetworkConfig;
use proot_distro_core::storage::VolumeConfig;

#[derive(Parser)]
#[command(name = "podnext")]
#[command(
    about = "PRoot Distro Next (PDN) - Fast, isolated container and service orchestration for Termux/Android"
)]
#[command(version)]
#[command(
    long_about = "podnext (PRoot Distro Next) - Next-generation container & service orchestration platform.\n\n\
    ========================================================================\n\
    1. SIMPLE & CORE DISTRO USAGE (Everyday proot-distro replacement):\n\
    ========================================================================\n\
    podnext install debian:13                 # Install Debian 13 trixie distro\n\
    podnext login debian                      # Enter interactive container shell\n\
    podnext run debian -- apt update          # Run command directly in container\n\
    podnext list                              # List installed containers\n\
    podnext copy debian:/etc/hosts ./hosts    # Copy files/dirs host <-> container\n\
    podnext sync ./src debian:/root/src -d    # Sync directories (sha256 hashing + prune)\n\
    podnext rename debian deb-prod            # Rename installed container\n\
    podnext reset deb-prod                    # Reset container to initial clean state\n\
    podnext remove deb-prod                   # Remove container and its data\n\
    podnext search debian                     # Search OCI images on Docker Hub\n\
    podnext info                              # Show system & host kernel info\n\n\
    ========================================================================\n\
    2. INTERMEDIATE: IMAGES, STORAGE, VOLUMES & BACKUP:\n\
    ========================================================================\n\
    podnext image ls                          # List local image cache\n\
    podnext image pull alpine:latest          # Pull OCI image from registry\n\
    podnext volume create my-vol              # Create persistent storage volume\n\
    podnext volume ls                         # List storage volumes\n\
    podnext volume prune                      # Remove unused volumes\n\
    podnext network create isolated-net       # Create isolated bridge network\n\
    podnext network ls                        # List container networks\n\
    podnext backup create debian              # Create container snapshot archive (tar.gz)\n\
    podnext backup list                       # List backup snapshots\n\
    podnext backup restore <id> --target deb2 # Restore container from backup\n\n\
    ========================================================================\n\
    3. ADVANCED: SYSTEM SERVICES, DAEMON & SUPERVISOR:\n\
    ========================================================================\n\
    podnext daemon start                      # Start PDN supervisor daemon\n\
    podnext daemon status                     # Check supervisor daemon status\n\
    podnext service start web-srv             # Start managed service (with restart policy)\n\
    podnext service status web-srv            # Show service status & PID\n\
    podnext service logs web-srv              # View service logs\n\
    podnext quota set web-srv memory 512M     # Set memory cgroup quota\n\
    podnext quota set web-srv cpu 200         # Set CPU quota limit (2 cores)\n\
    podnext quota usage web-srv               # Monitor real-time resource usage\n\
    podnext watchdog status                   # Monitor circuit breaker & watchdog\n\
    podnext log tail web-srv                  # Stream logs in real-time\n\
    podnext dbus serve --socket /tmp/d.sock   # Run IPC D-Bus broker\n\
    podnext dbus call ...                     # Send D-Bus method call\n\
    podnext gitops sync                       # Synchronize declarative GitOps manifests\n\
    podnext gitops status                     # Check GitOps app sync status\n\
    podnext boot enable web-srv               # Enable auto-start on Termux boot\n\
    podnext wakelock acquire web-srv partial  # Acquire Android wake lock to keep active\n\n\
    ========================================================================\n\
    4. DIRECT EXECUTION MODE (Pure compatibility like proot binary):\n\
    ========================================================================\n\
    podnext -r /path/to/rootfs -- /bin/bash    # Run custom rootfs directly\n\
    podnext -b /host/dir:/guest/dir -- ls      # Directly bind mount host directory\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    // Direct execution mode (like proot)
    #[arg(
        short = 'r',
        long = "rootfs",
        global = true,
        help = "Use PATH as guest root filesystem"
    )]
    rootfs: Option<String>,

    #[arg(
        short = 'b',
        long = "bind",
        global = true,
        value_name = "HOST_PATH:GUEST_PATH",
        help = "Bind mount host path into guest (repeatable)"
    )]
    bind: Vec<String>,

    #[arg(
        short = 'w',
        long = "pwd",
        global = true,
        help = "Set initial working directory"
    )]
    pwd: Option<String>,

    #[arg(
        short = 'q',
        long = "qemu",
        global = true,
        help = "Execute through QEMU user-mode emulation (path to qemu binary)"
    )]
    qemu: Option<String>,

    #[arg(
        short = '0',
        long = "link2symlink",
        global = true,
        help = "Enable hardlink emulation via symlinks"
    )]
    link2symlink: bool,

    #[arg(
        long = "no-sysvipc",
        global = true,
        conflicts_with = "sysvipc",
        help = "Disable System V IPC emulation"
    )]
    no_sysvipc: bool,

    #[arg(
        long = "sysvipc",
        global = true,
        conflicts_with = "no_sysvipc",
        help = "Enable System V IPC emulation"
    )]
    sysvipc: bool,

    #[arg(
        long = "kernel",
        global = true,
        help = "Override kernel release string"
    )]
    kernel: Option<String>,

    #[arg(long = "hostname", global = true, help = "Set hostname")]
    hostname: Option<String>,

    #[arg(long = "uid", global = true, help = "User ID to run as")]
    uid: Option<u32>,

    #[arg(long = "gid", global = true, help = "Group ID to run as")]
    gid: Option<u32>,

    #[arg(
        long = "seccomp",
        global = true,
        conflicts_with = "no_seccomp",
        help = "Enable seccomp filtering"
    )]
    seccomp: bool,

    #[arg(
        long = "no-seccomp",
        global = true,
        conflicts_with = "seccomp",
        help = "Disable seccomp filtering"
    )]
    no_seccomp: bool,

    #[arg(long = "trace", global = true, help = "Enable syscall tracing")]
    trace: bool,

    #[arg(
        long = "kill-on-exit",
        global = true,
        conflicts_with = "no_kill_on_exit",
        help = "Kill processes on exit"
    )]
    kill_on_exit: bool,

    #[arg(
        long = "no-kill-on-exit",
        global = true,
        conflicts_with = "kill_on_exit",
        help = "Don't kill processes on exit"
    )]
    no_kill_on_exit: bool,

    #[arg(
        short = 'e',
        long = "env",
        global = true,
        value_name = "VAR=VALUE",
        help = "Set environment variable (repeatable)"
    )]
    env: Vec<String>,

    /// Enable verbose debug output
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count, help = "Enable verbose debug output (repeatable)")]
    verbose: u8,

    /// Output format
    #[arg(
        long = "format",
        global = true,
        value_enum,
        help = "Output format (table, json, yaml, text)"
    )]
    format: Option<OutputFormat>,
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum Commands {
    // ------------------------------------------------------------------------
    // TIER 1: CORE DISTRO COMMANDS (Simple & Daily Usage - proot-distro parity)
    // ------------------------------------------------------------------------
    /// [experimental] [Core] Install distribution from OCI image
    #[command(
        long_about = "Download and unpack an OCI/Docker container image into a clean, isolated rootfs.\n\n\
        EXAMPLES:\n  \
        podnext install debian:13\n  \
        podnext install ubuntu:22.04 --name my-ubuntu\n  \
        podnext install alpine:latest --force"
    )]
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

    /// [stable] [Core] List created containers
    #[command(
        visible_alias = "ls",
        long_about = "List all installed distributions and containers with their status, real Linux OS, and architecture.\n\n\
        EXAMPLES:\n  \
        podnext list\n  \
        podnext list --format json\n  \
        podnext list --no-all"
    )]
    List {
        /// Show all (including stopped)
        #[arg(short = 'a', long, conflicts_with = "no_all")]
        all: bool,
        /// Hide stopped containers
        #[arg(long = "no-all", action = clap::ArgAction::SetTrue, conflicts_with = "all")]
        no_all: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// [experimental] [Core] Start interactive shell inside a container
    #[command(
        long_about = "Start an interactive root/user shell inside an installed container with isolated environment.\n\n\
        EXAMPLES:\n  \
        podnext login debian\n  \
        podnext login ubuntu -- /bin/bash\n  \
        podnext login debian --user 1000"
    )]
    Login {
        /// Container name
        container: String,
        /// User to run as
        #[arg(short, long)]
        user: Option<String>,
        /// Run in isolated mode (do not mount host storage or Termux home directory)
        #[arg(long = "isolated")]
        isolated: bool,
        /// Command to run instead of shell (after --)
        #[arg(last = true)]
        command: Vec<String>,
    },

    /// [experimental] [Core] Run command in container
    #[command(
        long_about = "Run a command inside an installed container directly from the host without opening an interactive shell.\n\n\
        EXAMPLES:\n  \
        podnext run debian -- apt update && apt upgrade -y\n  \
        podnext run debian -- curl -fsSL https://example.com\n  \
        podnext run ubuntu -v /sdcard:/mnt/sdcard -- ls -la /mnt/sdcard"
    )]
    Run {
        /// Container name
        container: String,
        /// Command to run (after --)
        #[arg(last = true)]
        command: Vec<String>,
        /// Remove the container when it exits (not implemented: `run` refuses it)
        #[arg(long = "rm", global = false)]
        rm: bool,
        /// Run in the background (not implemented: `run` refuses it rather than running in the foreground)
        #[arg(long = "detach", short = 'd', global = false)]
        detach: bool,
        /// Assign a name to the container (not implemented: `run` refuses it)
        #[arg(long = "name", global = false)]
        name: Option<String>,
        /// Connect a container to a network (not implemented: `run` refuses it)
        #[arg(long = "network", global = false)]
        network: Option<String>,
        /// Bind mount a host directory into the guest as HOST:GUEST (repeatable)
        #[arg(
            long = "volume",
            global = false,
            value_name = "HOST_PATH:CONTAINER_PATH"
        )]
        volume: Vec<String>,
        /// Publish a container's port(s) to the host (not implemented: `run` refuses it)
        #[arg(
            long = "publish",
            short = 'p',
            global = false,
            value_name = "HOST_PORT:CONTAINER_PORT[/PROTOCOL]"
        )]
        publish: Vec<String>,
        /// Overwrite the default ENTRYPOINT of the image (not implemented: `run` refuses it)
        #[arg(long = "entrypoint", global = false)]
        entrypoint: Option<String>,
        /// Accepted for compatibility: stdio is inherited, so a TTY is used when the caller has one
        #[arg(long = "tty", short = 't', global = false)]
        tty: bool,
        /// Accepted for compatibility: stdin is inherited unless the caller redirects it
        #[arg(long = "interactive", short = 'i', global = false)]
        interactive: bool,
        /// User to run as (not implemented: `run` refuses it; the guest runs as the mapped root)
        #[arg(long = "user", short = 'u', global = false)]
        user: Option<String>,
        /// Working directory inside the container (not implemented: `run` refuses it)
        #[arg(long = "workdir", global = false)]
        workdir: Option<String>,
        /// Run in isolated mode (do not mount host storage or Termux home directory)
        #[arg(long = "isolated", global = false)]
        isolated: bool,
    },

    /// [experimental] [Container] List active sessions
    Ps {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// [stable] [Container] Stop container
    Kill {
        /// Container name
        container: String,
    },

    /// [stable] [Core] Delete a container
    #[command(
        visible_alias = "rm",
        long_about = "Permanently remove an installed container and its rootfs from disk.\n\n\
        EXAMPLES:\n  \
        podnext remove debian\n  \
        podnext remove my-ubuntu --force"
    )]
    Remove {
        /// Container name
        name: String,
        /// Force remove without confirmation
        #[arg(short = 'f', long)]
        force: bool,
        /// Confirm removal without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// [experimental] [Core] Rename an installed container
    #[command(
        long_about = "Safely rename an existing container in its directory structure and metadata.\n\n\
        EXAMPLES:\n  \
        podnext rename debian deb-prod\n  \
        podnext rename ubuntu test-ub"
    )]
    Rename {
        /// Current container name
        oldname: String,
        /// New container name
        newname: String,
    },

    /// [experimental] [Core] Copy files or directories between host and containers
    ///
    /// Paths are `[CONTAINER:]PATH`; a path without a container prefix is the
    /// host's.
    #[command(
        long_about = "Copy files or directories between the host and containers safely (strictly rejects path traversal escapes).\n\n\
        EXAMPLES:\n  \
        podnext copy debian:/etc/resolv.conf ./resolv.conf\n  \
        podnext copy ./my-app debian:/opt/my-app -R\n  \
        podnext copy debian:/var/log/app.log ./app.log -m"
    )]
    Copy {
        /// `[CONTAINER:]SRC`
        source: String,
        /// `[CONTAINER:]DEST`
        destination: String,
        /// Copy directories recursively (`-R`, since `-r` is the global --rootfs)
        #[arg(short = 'R', long)]
        recursive: bool,
        /// Remove the source after a successful copy
        #[arg(short = 'm', long = "move")]
        move_: bool,
    },

    /// [experimental] [Core] Reset a container to a clean install of the image it came from
    ///
    /// Deletes the container directory and reinstalls it. All data inside the
    /// container is lost, as with proot-distro's `reset`.
    #[command(
        long_about = "Reinstall a container cleanly from its original OCI image, discarding all user modifications.\n\n\
        EXAMPLES:\n  \
        podnext reset debian"
    )]
    Reset {
        /// Container name
        name: String,
    },

    /// [experimental] [Core] Mirror files between host and containers (content hashing sha256)
    ///
    /// Like `copy`, but only the differences move: a file is transferred when its
    /// content differs, so a second run is a no-op. Paths are `[CONTAINER:]PATH`.
    #[command(
        long_about = "Fast delta directory synchronization between host and containers based on SHA256 content hashing.\n\n\
        EXAMPLES:\n  \
        podnext sync ./src debian:/root/src\n  \
        podnext sync ./src debian:/root/src --delete"
    )]
    Sync {
        /// `[CONTAINER:]SRC`
        source: String,
        /// `[CONTAINER:]DEST`
        destination: String,
        /// Remove destination entries that have no counterpart in the source
        #[arg(short = 'd', long)]
        delete: bool,
    },

    /// [experimental] [Core] Search images on Docker Hub
    #[command(
        long_about = "Search for container images on Docker Hub, or list verified official distributions when run without arguments.\n\n\
        EXAMPLES:\n  \
        podnext search\n  \
        podnext search debian\n  \
        podnext search ubuntu --limit 5\n  \
        podnext search alpine --format json"
    )]
    Search {
        /// Search query (omit to display popular verified distributions)
        #[arg(default_value = "")]
        query: String,
        /// Limit results
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// [experimental] [Container] Manage OCI images and local cache
    #[command(subcommand)]
    Image(ImageCommands),

    /// [stable] [Container] List active sessions (detailed)
    Sessions {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// [experimental] [Container] Attach to a container session (creates new session)
    Attach {
        /// Container name
        container: String,
        /// User to run as
        #[arg(short, long)]
        user: Option<String>,
        /// Command to run instead of shell (after --)
        command: Vec<String>,
    },

    /// [experimental] [Core] Show system information
    #[command(name = "info")]
    #[command(
        long_about = "Display host kernel, system architecture, CPU cores, memory limits, and PDN capabilities.\n\n\
        EXAMPLES:\n  \
        podnext info\n  \
        podnext info --format json"
    )]
    Info {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },

    /// [experimental] [Core] Show version
    #[command(name = "version")]
    #[command(long_about = "Display version and build information for PDN.\n\n\
        EXAMPLES:\n  \
        podnext version\n  \
        podnext version --format json")]
    Version {
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    /// [stable] Report host and guest diagnostics, with the value behind every verdict
    #[command(
        long_about = "Run host checks (proot binary, temporary directory, shared memory, DNS, \
kernel, cgroups) and, when a container is named, guest checks (os-release, architecture, \
/dev/shm, mount count, resolv.conf). Every line carries the value or command it came from; a \
check that cannot run reports `unknown` rather than passing.\n\n\
EXIT STATUS:\n  0  no failing check\n  1  at least one check failed\n  2  invalid arguments\n\n\
EXAMPLES:\n  \
podnext doctor\n  \
podnext doctor --container alpine\n  \
podnext doctor --format json",
        after_help = "A failing check is the only thing that changes the exit status; warnings and \
unknowns are reported without failing the command."
    )]
    Doctor {
        /// Container to probe from the inside (guest checks are skipped without it)
        #[arg(short, long)]
        container: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    // ------------------------------------------------------------------------
    // TIER 2 & 3: ADVANCED SYSTEM SERVICES & ORCHESTRATION
    // ------------------------------------------------------------------------
    /// [experimental] [System] Service management commands (supervision & lifecycle)
    #[command(subcommand)]
    Service(ServiceCommands),

    /// [experimental] [System] Daemon management (background supervisor process)
    #[command(subcommand)]
    Daemon(DaemonCommands),

    /// [experimental] [System] Watchdog management & circuit breaker
    #[command(subcommand)]
    Watchdog(WatchdogCommands),

    /// [experimental] [System] Resource quota management (memory/cpu cgroups)
    #[command(subcommand)]
    Quota(QuotaCommands),

    /// [experimental] [System] Structured logging & real-time streaming
    #[command(subcommand, visible_alias = "logs")]
    Log(LogCommands),

    /// [experimental] [System] D-Bus IPC & service brokering
    #[command(subcommand)]
    Dbus(DbusCommands),

    /// [experimental] [System] Socket activation (listen on host, wake container on demand)
    #[command(subcommand)]
    Socket(SocketCommands),

    /// [experimental] [System] Termux:Boot integration for background autostart
    #[command(subcommand)]
    Boot(BootCommands),

    /// [experimental] [System] Wake lock management (prevent Android sleep)
    #[command(subcommand)]
    Wakelock(WakelockCommands),

    /// [experimental] [System] Policy engine (RBAC & capability limits)
    #[command(subcommand)]
    Policy(PolicyCommands),

    /// [experimental] [Storage] Backup/Restore management (container snapshots)
    #[command(subcommand)]
    Backup(BackupCommands),

    /// [experimental] [Advanced] Multi-arch build service
    #[command(subcommand)]
    Build(BuildCommands),
    /// [stable] Check a gateway configuration file before deploying it
    #[command(
        long_about = "Check a pn-gateway configuration file and print what it resolves to.\n\n\
        The file is read with the same loader the gateway uses, so an unknown key, a malformed \
        value or an unreadable duration fails here instead of at startup.\n\n\
        EXAMPLES:\n  \
        podnext config validate ./gateway.toml\n  \
        PN_GATEWAY_CONFIG=./gateway.toml podnext config validate"
    )]
    #[command(subcommand)]
    Config(ConfigCommands),

    /// [stable] [System] GitOps integration (declarative YAML sync & reconciliation)
    #[command(subcommand)]
    Gitops(GitopsCommands),

    /// [experimental] [System] Web dashboard management
    #[command(subcommand)]
    Dashboard(DashboardCommands),

    /// [experimental] [Storage] Network management (isolated bridge networks)
    #[command(subcommand)]
    Network(NetworkCommands),

    /// [stable] [Storage] Volume management (persistent data volumes)
    #[command(subcommand)]
    Volume(VolumeCommands),

    /// [experimental] [Advanced] Run Docker CLI commands transparently via kernel-capable runtime (UML)
    #[command(
        long_about = "Execute Docker CLI commands transparently through User-Mode Linux (UML) runtime.\n\n\
        EXAMPLES:\n  \
        podnext docker run --rm hello-world\n  \
        podnext docker ps\n  \
        podnext docker images"
    )]
    Docker {
        /// Optional container or instance name
        #[arg(short, long)]
        container: Option<String>,

        /// Arguments passed through to the docker CLI
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// [experimental] [Interop] Export PDN service configuration to a Docker Compose file
    #[command(
        long_about = "Export one or more PDN service definition files to a single Docker Compose document.\n\n\
        EXAMPLES:\n  \
        podnext compose svc.toml\n  \
        podnext compose svc-a.toml svc-b.toml -o compose.yml\n  \
        podnext compose svc.toml --image my-app:latest"
    )]
    Compose {
        /// Service definition file(s) to export (TOML, as the supervisor loads)
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Image for the exported service(s); Compose needs an image or a build context
        #[arg(short, long)]
        image: Option<String>,

        /// Write to this file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// [experimental] [Core] Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum ImageCommands {
    /// Pull an image from a registry
    Pull {
        /// Image reference (e.g., ubuntu:22.04, alpine:latest)
        image: String,
        /// Platform (e.g., linux/arm64, linux/amd64)
        #[arg(long)]
        platform: Option<String>,
        /// Download all tagged images
        #[arg(long)]
        all_tags: bool,
    },
    /// Push an image to a registry
    Push {
        /// Image reference (e.g., ubuntu:22.04)
        image: String,
    },
    /// Remove one or more images
    #[command(visible_alias = "remove")]
    Rm {
        /// Image references to remove
        images: Vec<String>,
        /// Force removal without confirmation
        #[arg(short = 'f', long)]
        force: bool,
        /// Confirm removal without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Tag an image
    Tag {
        /// Source image reference
        source: String,
        /// Target image reference
        target: String,
    },
    /// Show image history
    History {
        /// Image reference
        image: String,
        /// Show full history without truncation
        #[arg(long)]
        no_trunc: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Display detailed information on one or more images
    Inspect {
        /// Image references to inspect
        images: Vec<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// List images
    // `docs/src/cli-reference.md` documents `podnext image list`, which did not
    // exist: the documented command failed with "unrecognized subcommand 'list'".
    // The alias makes the documented form work instead of changing the doc to
    // match a name nobody types.
    #[command(visible_alias = "list")]
    Ls {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
        /// Show all images (including intermediate)
        #[arg(short = 'a', long)]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum NetworkCommands {
    /// Create a network
    Create {
        /// Network name
        name: String,
        /// Network driver
        #[arg(short = 'd', long, default_value = "bridge")]
        driver: String,
        /// Network subnet (CIDR)
        #[arg(short = 's', long)]
        subnet: Option<String>,
        /// Network gateway
        #[arg(short = 'g', long)]
        gateway: Option<String>,
    },
    /// List networks
    #[command(visible_alias = "list")]
    Ls {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Inspect a network
    Inspect {
        /// Network name(s)
        networks: Vec<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Remove one or more networks
    #[command(visible_alias = "remove")]
    Rm {
        /// Network name(s)
        networks: Vec<String>,
    },
    /// Connect a container to a network
    Connect {
        /// Network name
        network: String,
        /// Container name
        container: String,
        /// Container IP address
        #[arg(short = 'i', long)]
        ip: Option<String>,
    },
    /// Disconnect a container from a network
    Disconnect {
        /// Network name
        network: String,
        /// Container name
        container: String,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum VolumeCommands {
    /// Create a volume
    Create {
        /// Volume name
        name: String,
        /// Volume driver
        #[arg(short = 'd', long, default_value = "local")]
        driver: String,
        /// Driver options
        #[arg(short = 'o', long, value_name = "KEY=VALUE")]
        opts: Vec<String>,
    },
    /// List volumes
    #[command(visible_alias = "list")]
    Ls {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Inspect a volume
    Inspect {
        /// Volume name(s)
        volumes: Vec<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Remove one or more volumes
    #[command(visible_alias = "remove")]
    Rm {
        /// Volume name(s)
        volumes: Vec<String>,
        /// Force removal
        #[arg(short = 'f', long)]
        force: bool,
        /// Confirm removal without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Remove all unused volumes
    Prune {
        /// Force prune without confirmation
        #[arg(short = 'f', long)]
        force: bool,
        /// Confirm removal without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum ServiceCommands {
    /// Start a service
    Start {
        /// Service name
        name: String,
    },
    /// Stop a service
    Stop {
        /// Service name
        name: String,
        /// Force stop (SIGKILL)
        #[arg(short, long)]
        force: bool,
    },
    /// Restart a service
    Restart {
        /// Service name
        name: String,
    },
    /// List all services
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Show service status
    Status {
        /// Service name
        name: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// View service logs
    Logs {
        /// Service name
        name: String,
        /// Follow logs
        #[arg(short, long)]
        follow: bool,
        /// Number of lines
        #[arg(short, long, default_value = "100")]
        lines: usize,
    },
    /// Enable auto-start at boot
    Enable {
        /// Service name
        name: String,
    },
    /// Disable auto-start at boot
    Disable {
        /// Service name
        name: String,
    },
    /// Load service from TOML config
    Load {
        /// Service name
        name: String,
    },
    /// Create service from TOML config
    Create {
        /// Service name
        name: String,
        /// TOML config file
        #[arg(short, long)]
        config: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum DaemonCommands {
    /// Start daemon
    Start,
    /// Stop daemon
    Stop,
    /// Show daemon status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Reload daemon configuration
    Reload,
    /// Show daemon logs
    Logs {
        /// Follow logs
        #[arg(short, long)]
        follow: bool,
        /// Number of lines
        #[arg(short, long, default_value = "100")]
        lines: usize,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum WatchdogCommands {
    /// Show watchdog status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Set watchdog policy for a service
    #[command(visible_alias = "set-policy")]
    SetPolicy {
        /// Service name
        name: String,
        /// Restart policy
        #[arg(value_enum)]
        policy: RestartPolicyArg,
        /// Max restarts per window
        max_restarts: u32,
        /// Window in seconds
        window_secs: u64,
    },
    /// Reset watchdog state for a service
    Reset {
        /// Service name
        name: String,
    },
    /// Show watchdog events
    Events {
        /// Number of events
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum QuotaCommands {
    /// Set resource quota
    Set {
        /// Service name
        name: String,
        /// Resource type
        #[arg(value_enum)]
        resource: ResourceTypeArg,
        /// Limit value (e.g., 512M, 200, 1000)
        limit: String,
    },
    /// Get resource quotas
    Get {
        /// Service name
        name: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Show current resource usage
    Usage {
        /// Service name
        name: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// List all quotas
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum LogCommands {
    /// Query logs
    Query {
        /// Service name
        #[arg(short, long)]
        service: Option<String>,
        /// Minimum log level
        #[arg(long, value_enum)]
        min_level: Option<LogLevelArg>,
        /// Maximum log level
        #[arg(long, value_enum)]
        max_level: Option<LogLevelArg>,
        /// Search text
        #[arg(long)]
        grep: Option<String>,
        /// Limit results
        #[arg(short, long, default_value = "100")]
        limit: usize,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Tail logs (follow)
    Tail {
        /// Service name
        #[arg(short, long)]
        service: Option<String>,
        /// Number of lines
        #[arg(short, long, default_value = "100")]
        lines: usize,
    },
    /// Show log statistics
    Stats {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Flush log buffer
    Flush,
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum DbusCommands {
    /// Run the D-Bus broker in the foreground
    ///
    /// Needed because nothing else starts it: the daemon does not, and `list`
    /// only constructs a broker without listening (so it always reports none).
    Serve {
        /// Socket path (defaults to the broker's own default)
        #[arg(long)]
        socket: Option<String>,
    },
    /// List D-Bus services
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Call D-Bus method
    Call {
        /// Bus name
        bus: String,
        /// Broker socket path (defaults to the broker's own default)
        #[arg(long)]
        socket: Option<String>,
        /// Object path
        path: String,
        /// Interface
        interface: String,
        /// Method
        method: String,
        /// Arguments (JSON)
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Monitor D-Bus signals
    Monitor {
        /// Bus name
        bus: String,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum SocketCommands {
    /// Register socket activation
    Register {
        /// Config file
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Unregister socket
    Unregister {
        /// Socket name
        name: String,
    },
    /// List socket activations
    List {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum BootCommands {
    /// Enable auto-start at boot
    Enable {
        /// Service name
        name: String,
    },
    /// Disable auto-start at boot
    Disable {
        /// Service name
        name: String,
    },
    /// Show boot configuration
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum WakelockCommands {
    /// Acquire wake lock
    Acquire {
        /// Service name
        name: String,
        /// Lock type
        #[arg(value_enum, ignore_case = true)]
        lock_type: WakeLockTypeArg,
    },
    /// Release wake lock
    Release {
        /// Service name
        name: String,
    },
    /// Show wake lock status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum PolicyCommands {
    /// Evaluate policies for a service
    Eval {
        /// Service name
        name: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// List policy rules
    List {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Load policy from file
    Load {
        /// Policy file
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Show policy presets
    Presets,
}

/// Backup/Restore management
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum BackupCommands {
    /// Create a backup
    Create {
        /// Container name or ID
        container: String,
        /// Backup type
        #[arg(short = 't', long, value_enum, default_value = "full")]
        backup_type: BackupTypeArg,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// List backups
    List {
        /// Filter by container
        #[arg(short, long)]
        container: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Restore a backup
    Restore {
        /// Backup ID
        backup_id: String,
        /// Target container name (optional)
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Delete a backup
    Delete {
        /// Backup ID
        backup_id: String,
    },
    /// Show backup status
    Status {
        /// Backup ID
        backup_id: String,
    },
    /// Show backup configuration
    Config,
}

/// Backup type
#[derive(ValueEnum, Clone, Copy, Debug)]
#[clap(rename_all = "lowercase")]
enum BackupTypeArg {
    Full,
    Incremental,
    Differential,
    ConfigOnly,
}

/// Gateway configuration checks
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum ConfigCommands {
    /// Validate a gateway configuration file and print the values it resolves to
    Validate {
        /// Configuration file to check (defaults to $PN_GATEWAY_CONFIG)
        path: Option<PathBuf>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
}

/// Multi-arch build service
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum BuildCommands {
    /// [stable] Build an OCI container image from a Dockerfile context
    #[command(
        long_about = "Build an OCI container image from a Dockerfile context directory.\n\n\
        EXAMPLES:\n  \
        podnext build local -t my-app:latest .\n  \
        podnext build local -f Dockerfile.custom -t my-app:v1 ./context"
    )]
    Local {
        /// Name and optionally a tag in the 'name:tag' format
        #[arg(short, long)]
        tag: Option<String>,

        /// Path to the Dockerfile (defaults to 'PATH/Dockerfile')
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Build context path (directory containing Dockerfile and assets)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// [experimental] Submit a build (refuses: no build service)
    Submit {
        /// Build specification file
        #[arg(short, long)]
        spec: PathBuf,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// [experimental] Show build status (refuses: no build service)
    Status {
        /// Build ID
        build_id: String,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// [experimental] Cancel a build (refuses: no build service)
    Cancel {
        /// Build ID
        build_id: String,
    },
    /// [experimental] List builds (refuses: no build service)
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// [experimental] Show build artifacts (refuses: no build service)
    Artifacts {
        /// Build ID
        build_id: String,
    },
    /// [experimental] Show build logs (refuses: no build service)
    Logs {
        /// Build ID
        build_id: String,
        /// Follow logs
        #[arg(short, long)]
        follow: bool,
    },
}

/// GitOps integration
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum GitopsCommands {
    /// Sync with git repository
    Sync {
        /// Force sync
        #[arg(short, long)]
        force: bool,
    },
    /// Show GitOps status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// List managed resources
    Resources {
        /// Filter by kind
        #[arg(long)]
        kind: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Check for drift
    Drift {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
    /// Reconcile resources
    Reconcile {
        /// Resource name (optional, reconciles all if not specified)
        name: Option<String>,
    },
}

/// Web dashboard management
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
enum DashboardCommands {
    /// Start the dashboard server
    Start {
        /// Port to bind
        #[arg(short, long, default_value = "8800")]
        port: u16,
        /// Bind address
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// Stop the dashboard server
    Stop,
    /// Show dashboard status
    Status,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Table,
    Json,
    Yaml,
    Text,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum RestartPolicyArg {
    Never,
    OnFailure,
    Always,
    UnlessStopped,
    OnFailureOrCrash,
}

#[derive(ValueEnum, Clone, Copy, Debug, strum::Display)]
#[strum(serialize_all = "lowercase")]
enum ResourceTypeArg {
    Cpu,
    Memory,
    MemorySwap,
    IoReadBps,
    IoWriteBps,
    IoReadIops,
    IoWriteIops,
    Pids,
    Nofile,
    Network,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum LogLevelArg {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum WakeLockTypeArg {
    Partial,
    Full,
    DozeExemption,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

#[tokio::main]
async fn main() {
    // Exits with the child's code when a command inside a container failed, so
    // `podnext login c -- sh -c 'exit 7'` reports 7 like every other tool. The
    // rest of the errors stay ordinary failures and exit 1.
    if let Err(err) = run().await {
        if let Some(exit) = err.downcast_ref::<proot_core::ChildExit>() {
            std::process::exit(exit.0);
        }
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Check for direct execution mode early (before clap parsing)
    // Direct execution: podnext [flags...] -r /rootfs [flags...] -- <cmd> ...
    // Direct execution only applies if NO subcommand is used and -r/--rootfs is BEFORE '--'
    let raw_args: Vec<String> = std::env::args_os()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let pn_debug = std::env::var_os("PN_NEXT_DEBUG").is_some();
    if pn_debug {
        pn_trace!("[DEBUG main] Raw args: {:?}", raw_args);
    }

    let dashdash_idx = raw_args
        .iter()
        .position(|a| a == "--")
        .unwrap_or(raw_args.len());
    let pre_dashdash_args = &raw_args[..dashdash_idx];

    const SUBCOMMANDS: &[&str] = &[
        "run",
        "login",
        "list",
        "ls",
        "ps",
        "kill",
        "copy",
        "sync",
        "rename",
        "reset",
        "install",
        "remove",
        "rm",
        "search",
        "build",
        "docker",
        "image",
        "images",
        "network",
        "volume",
        "backup",
        "restore",
        "plugin",
        "help",
        "completion",
        "status",
        "info",
        "exec",
        "logs",
        "stop",
        "start",
        "restart",
        "attach",
        "commit",
    ];
    let has_subcommand = pre_dashdash_args
        .iter()
        .skip(1)
        .any(|a| SUBCOMMANDS.contains(&a.as_str()));
    let is_direct_execution = !has_subcommand
        && pre_dashdash_args
            .iter()
            .skip(1)
            .any(|a| a == "-r" || a == "--rootfs");

    if is_direct_execution {
        if pn_debug {
            pn_trace!("[DEBUG main] Direct execution mode detected");
        }

        // Parse all flags manually for direct execution
        // Format: podnext [flags...] -r <rootfs> [flags...] -- <cmd...>

        let mut rootfs: Option<String> = None;
        let mut binds = Vec::new();
        let mut pwd: Option<String> = None;
        let mut qemu: Option<String> = None;
        let mut link2symlink = false;
        let mut no_sysvipc = false;
        let mut sysvipc = false;
        let mut kernel: Option<String> = None;
        let mut hostname: Option<String> = None;
        let mut uid: Option<u32> = None;
        let mut gid: Option<u32> = None;
        let mut seccomp = false;
        let mut no_seccomp = false;
        let mut trace = false;
        let mut kill_on_exit = false;
        let mut no_kill_on_exit = false;
        let mut env = Vec::new();
        let mut verbose = false;
        let mut cmd_args: Vec<String> = Vec::new();

        let mut i = 1; // Skip program name

        // Parse all flags until --
        while i < raw_args.len() {
            let arg = &raw_args[i];
            if arg == "--" {
                // Remaining args are the command
                cmd_args = raw_args[i + 1..].to_vec();
                break;
            }

            match arg.as_str() {
                "-r" | "--rootfs" => {
                    i += 1;
                    if i < raw_args.len() {
                        rootfs = Some(raw_args[i].clone());
                    }
                }
                "-b" | "--bind" => {
                    i += 1;
                    if i < raw_args.len() {
                        binds.push(raw_args[i].clone());
                    }
                }
                "-w" | "--pwd" => {
                    i += 1;
                    if i < raw_args.len() {
                        pwd = Some(raw_args[i].clone());
                    }
                }
                "-q" | "--qemu" => {
                    i += 1;
                    if i < raw_args.len() {
                        qemu = Some(raw_args[i].clone());
                    }
                }
                "-0" | "--link2symlink" => {
                    link2symlink = true;
                }
                "--no-sysvipc" => {
                    no_sysvipc = true;
                }
                "--sysvipc" => {
                    sysvipc = true;
                }
                "--kernel" => {
                    i += 1;
                    if i < raw_args.len() {
                        kernel = Some(raw_args[i].clone());
                    }
                }
                "--hostname" => {
                    i += 1;
                    if i < raw_args.len() {
                        hostname = Some(raw_args[i].clone());
                    }
                }
                "--uid" => {
                    i += 1;
                    if i < raw_args.len() {
                        uid = raw_args[i].parse().ok();
                    }
                }
                "--gid" => {
                    i += 1;
                    if i < raw_args.len() {
                        gid = raw_args[i].parse().ok();
                    }
                }
                "--seccomp" => {
                    seccomp = true;
                }
                "--no-seccomp" => {
                    no_seccomp = true;
                }
                "--trace" => {
                    trace = true;
                }
                "--kill-on-exit" => {
                    kill_on_exit = true;
                }
                "--no-kill-on-exit" => {
                    no_kill_on_exit = true;
                }
                "-e" | "--env" => {
                    i += 1;
                    if i < raw_args.len() {
                        env.push(raw_args[i].clone());
                    }
                }
                "-v" | "--verbose" => {
                    verbose = true;
                }
                _ => {
                    // Unknown flag, skip
                }
            }
            i += 1;
        }

        if rootfs.is_none() {
            eprintln!("Error: -r/--rootfs is required for direct execution mode");
            std::process::exit(1);
        }

        if cmd_args.is_empty() {
            eprintln!("Error: Direct execution mode requires a command after --");
            std::process::exit(1);
        }

        if pn_debug {
            pn_trace!("[DEBUG main] Calling execute_direct_manual with all flags");
        }
        // For now, just call execute_direct_manual with basic args
        // TODO: Pass all parsed flags to a new execute_direct_manual_full function
        return execute_direct_manual(rootfs.unwrap(), &cmd_args, verbose).await;
    }

    // Parse trailing arguments for direct execution mode (-- <cmd> ...) for clap-parsed case
    let args: Vec<String> = std::env::args_os()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let cli_trailing_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        Vec::new()
    };

    let cli = Cli::parse();

    // Direct execution mode (like proot) - clap parsed case
    if cli.command.is_none() && cli.rootfs.is_some() {
        return execute_direct(&cli, &cli_trailing_args).await;
    }

    // Subcommand mode
    if let Some(cmd) = cli.command {
        pn_trace!("DEBUG main: Executing command: {:?}", cmd);
        // `-b/--bind` is a global option, so `login` and `run` must see it too; it used to
        // reach only the direct-execution path below.
        return execute_command(cmd, cli.bind.clone()).await;
    }

    print_help();
    Ok(())
}

async fn execute_direct(cli: &Cli, trailing_args: &[String]) -> Result<()> {
    if trailing_args.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", cli.rootfs);
    pn_trace!("[DEBUG direct] Binds: {:?}", cli.bind);
    pn_trace!("[DEBUG direct] Working dir: {:?}", cli.pwd);
    pn_trace!("[DEBUG direct] QEMU: {:?}", cli.qemu);
    pn_trace!("[DEBUG direct] Link2symlink: {}", cli.link2symlink);
    pn_trace!("[DEBUG direct] No sysvipc: {}", cli.no_sysvipc);
    pn_trace!("[DEBUG direct] Sysvipc: {}", cli.sysvipc);
    pn_trace!("[DEBUG direct] Kernel: {:?}", cli.kernel);
    pn_trace!("[DEBUG direct] Hostname: {:?}", cli.hostname);
    pn_trace!("[DEBUG direct] UID: {:?}", cli.uid);
    pn_trace!("[DEBUG direct] GID: {:?}", cli.gid);
    pn_trace!("[DEBUG direct] Seccomp: {}", cli.seccomp);
    pn_trace!("[DEBUG direct] No seccomp: {}", cli.no_seccomp);
    pn_trace!("[DEBUG direct] Trace: {}", cli.trace);
    pn_trace!("[DEBUG direct] Kill on exit: {}", cli.kill_on_exit);
    pn_trace!("[DEBUG direct] No kill on exit: {}", cli.no_kill_on_exit);
    pn_trace!("[DEBUG direct] Env: {:?}", cli.env);
    pn_trace!("[DEBUG direct] Command: {:?}", trailing_args);

    // Create a temporary container config for direct execution
    let rootfs_path = cli.rootfs.as_ref().unwrap();

    // Use proot-core directly for direct execution
    let mut config = proot_core::ProotConfig::default();
    config.verbose = cli.verbose;
    let mut engine = proot_core::ProotEngine::new(config)?;

    engine.config.rootfs = Some(rootfs_path.into());

    // Process bind mounts
    for bind in &cli.bind {
        engine.config.bind.push(bind.clone());
    }

    // Set working directory
    if let Some(pwd) = &cli.pwd {
        engine.config.cwd = Some(pwd.into());
    }

    // Set environment variables
    for env in &cli.env {
        engine.config.env.push(env.clone());
    }

    // Set kernel release
    if let Some(kernel) = &cli.kernel {
        engine.config.kernel_release = Some(kernel.clone());
    }

    // Set hostname
    if let Some(hostname) = &cli.hostname {
        engine.config.hostname = Some(hostname.clone());
    }

    // Set UID/GID
    if let Some(uid) = cli.uid {
        engine.config.uid = Some(uid);
    }
    if let Some(gid) = cli.gid {
        engine.config.gid = Some(gid);
    }

    // Set seccomp
    if cli.seccomp {
        engine.config.seccomp = true;
    }
    if cli.no_seccomp {
        engine.config.seccomp = false;
    }

    // Set trace
    if cli.trace {
        engine.config.trace = true;
    }

    // Set kill-on-exit
    if cli.kill_on_exit {
        engine.config.kill_on_exit = true;
    }
    if cli.no_kill_on_exit {
        engine.config.kill_on_exit = false;
    }

    // Set sysvipc
    if cli.no_sysvipc {
        engine.config.sysvipc = false;
    }
    if cli.sysvipc {
        engine.config.sysvipc = true;
    }

    // Set link2symlink
    if cli.link2symlink {
        engine.config.link2symlink = true;
    }

    // Set command
    engine.config.command = trailing_args.to_vec();

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

async fn execute_direct_manual(rootfs: String, cmd_args: &[String], verbose: bool) -> Result<()> {
    if cmd_args.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", rootfs);
    pn_trace!("[DEBUG direct] Command: {:?}", cmd_args);
    pn_trace!("[DEBUG direct] Verbose: {}", verbose);

    // Use proot-core directly for direct execution
    let mut config = proot_core::ProotConfig::default();
    config.verbose = if verbose { 1 } else { 0 };
    let mut engine = proot_core::ProotEngine::new(config)?;

    engine.config.rootfs = Some(rootfs.into());
    engine.config.command = cmd_args.to_vec();

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

async fn execute_command(cmd: Commands, global_binds: Vec<String>) -> Result<()> {
    pn_trace!("DEBUG execute_command: START {:?}", cmd);
    match cmd {
        // Container commands
        Commands::Install { image, name, force } => cmd_install(image, name, force).await,
        Commands::List {
            all,
            no_all,
            format,
        } => cmd_list(all, no_all, format).await,
        Commands::Login {
            container,
            user,
            isolated,
            command,
        } => cmd_login(container, user, isolated, command, global_binds).await,
        Commands::Run {
            container,
            command,
            rm,
            detach,
            name,
            network,
            volume,
            publish,
            entrypoint,
            tty,
            interactive,
            user,
            workdir,
            isolated,
        } => {
            cmd_run(
                container,
                command,
                rm,
                detach,
                name,
                network,
                volume,
                publish,
                entrypoint,
                tty,
                interactive,
                user,
                workdir,
                isolated,
                global_binds,
            )
            .await
        }
        Commands::Ps { format } => cmd_ps(format).await,
        Commands::Sessions { format } => cmd_sessions(format).await,
        Commands::Attach {
            container,
            user,
            command,
        } => cmd_attach(container, user, command).await,
        Commands::Kill { container } => cmd_kill(container).await,
        Commands::Remove { name, force, yes } => cmd_remove(name, force || yes).await,
        Commands::Rename { oldname, newname } => cmd_rename(oldname, newname).await,
        Commands::Copy {
            source,
            destination,
            recursive,
            move_,
        } => cmd_copy(source, destination, recursive, move_).await,
        Commands::Reset { name } => cmd_reset(name).await,
        Commands::Sync {
            source,
            destination,
            delete,
        } => cmd_sync(source, destination, delete).await,
        Commands::Search {
            query,
            limit,
            format,
        } => cmd_search(query, limit, format).await,
        Commands::Image(cmd) => execute_image_command(cmd).await,
        Commands::Network(cmd) => execute_network_command(cmd).await,
        Commands::Volume(cmd) => execute_volume_command(cmd).await,
        Commands::Info { format } => cmd_info(format).await,
        Commands::Version { format } => cmd_version(format).await,
        Commands::Doctor { container, format } => cmd_doctor(container, format).await,

        // Service orchestration commands
        Commands::Service(cmd) => execute_service_command(cmd).await,
        Commands::Daemon(cmd) => execute_daemon_command(cmd).await,
        Commands::Watchdog(cmd) => execute_watchdog_command(cmd).await,
        Commands::Quota(cmd) => execute_quota_command(cmd).await,
        Commands::Log(cmd) => execute_log_command(cmd).await,
        Commands::Dbus(cmd) => execute_dbus_command(cmd).await,
        Commands::Socket(cmd) => execute_socket_command(cmd).await,
        Commands::Boot(cmd) => execute_boot_command(cmd).await,
        Commands::Wakelock(cmd) => execute_wakelock_command(cmd).await,
        Commands::Policy(cmd) => execute_policy_command(cmd).await,

        // Phase 7: Platform Maturity commands
        Commands::Backup(cmd) => execute_backup_command(cmd).await,
        Commands::Build(cmd) => execute_build_command(cmd).await,
        Commands::Config(cmd) => execute_config_command(cmd),
        Commands::Gitops(cmd) => execute_gitops_command(cmd).await,
        Commands::Dashboard(cmd) => execute_dashboard_command(cmd).await,
        Commands::Docker { container, args } => cmd_docker(container, args).await,
        Commands::Compose {
            files,
            image,
            output,
        } => execute_compose_command(files, image, output).await,

        // Utilities
        Commands::Completion { shell } => cmd_completion(shell).await,
    }
}

// Output formatting helpers
pub(crate) fn output_table<T: Tabled>(items: &[T]) {
    println!("{}", Table::new(items).to_string());
}

pub(crate) fn output_json<T: serde::Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(items)?);
    Ok(())
}

pub(crate) fn output_yaml<T: serde::Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_yaml::to_string(items)?);
    Ok(())
}

// Container command implementations (existing)
/// `podnext config validate`: the load site for the gateway configuration.
///
/// Before this existed the loader was reachable only from tests, so the only way
/// to discover that a documented value could not be loaded was to read the code.
/// The command uses the same entry points the gateway will, and exits non-zero on
/// a configuration the gateway would refuse to start with.
fn execute_config_command(cmd: ConfigCommands) -> Result<()> {
    use pn_gateway::config::GatewayConfig;

    match cmd {
        ConfigCommands::Validate { path, format } => {
            let path = match path.or_else(|| {
                // Same variable the gateway reads, so the check and the start-up
                // path cannot disagree about which file is in play.
                std::env::var(GatewayConfig::CONFIG_ENV)
                    .ok()
                    .filter(|p| !p.trim().is_empty())
                    .map(PathBuf::from)
            }) {
                Some(path) => path,
                None => {
                    eprintln!(
                        "no configuration file given: pass a path or set {}",
                        GatewayConfig::CONFIG_ENV
                    );
                    std::process::exit(3);
                }
            };

            let config = match GatewayConfig::from_toml_file(&path) {
                Ok(config) => config,
                Err(err) => {
                    // Exit code 3 keeps "invalid configuration" distinguishable
                    // from generic application errors (1) and clap usage (2).
                    eprintln!("invalid gateway configuration: {err}");
                    std::process::exit(3);
                }
            };

            match format {
                OutputFormat::Json => {
                    output_json(&config)?;
                    return Ok(());
                }
                OutputFormat::Yaml => {
                    output_yaml(&config)?;
                    return Ok(());
                }
                _ => {}
            }

            use pn_gateway::duration_fmt::format_duration;
            println!("{} is valid", path.display());
            println!("  name                {}", config.name);
            println!("  id                  {}", config.gateway_id);
            println!(
                "  proot timeout       {}",
                format_duration(config.proot_config.timeout)
            );
            println!(
                "  retries             {} (delay {})",
                config.fail_safe.max_retries,
                format_duration(config.fail_safe.retry_delay)
            );
            println!(
                "  circuit breaker     {} (threshold {}, timeout {})",
                config.fail_safe.circuit_breaker.enabled,
                config.fail_safe.circuit_breaker.failure_threshold,
                format_duration(config.fail_safe.circuit_breaker.timeout)
            );
            println!(
                "  health check        {} every {}",
                config.health.enabled,
                format_duration(config.health.check_interval)
            );
            println!(
                "  metrics             {} every {}, retained {}",
                config.metrics.enabled,
                format_duration(config.metrics.interval),
                format_duration(config.metrics.retention)
            );
            println!(
                "  storage cleanup     {} / backup {}",
                format_duration(config.storage.cleanup_interval),
                format_duration(config.storage.backup_interval)
            );
            Ok(())
        }
    }
}

async fn cmd_install(image: String, name: Option<String>, force: bool) -> Result<()> {
    let container_name = match &name {
        Some(explicit) => explicit.clone(),
        None => default_container_name(&image),
    };

    // An explicit `--name ""` used to be accepted: the container directory then
    // collapsed onto the containers root itself, creating an orphan that `list`
    // showed with a blank name and `remove` could never match. Derived names are
    // validated too, so no install can create an unusable path.
    proot_distro_core::NameValidator::new().validate_container_name(&container_name)?;

    let _rootfs: PathBuf = install_image_native(&image, &container_name, force, None).await?;
    Ok(())
}

/// Derive a container name from an image reference.
///
/// `debian:13` and `ghcr.io:5000/acme/web:1.2` both yield a single path component
/// (`debian`, `web`): a registry host, port, or namespace must never become a
/// directory component under `~/.podnext/containers/`.
fn default_container_name(image: &str) -> String {
    // The rule lives in proot-distro-core so build, run and install agree on it;
    // a divergence here is exactly how `build local` ended up naming a directory
    // `c2env:latest` and losing /dev/shm (#35).
    proot_distro_core::names::store_key(image)
}

async fn cmd_list(_all: bool, no_all: bool, format: OutputFormat) -> Result<()> {
    let show_all = !no_all;
    let containers = list_containers_native(show_all).await?;

    #[derive(Tabled, Serialize)]
    struct ContainerRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DISTRO")]
        distro: String,
        #[tabled(rename = "VERSION")]
        version: String,
        #[tabled(rename = "ARCH")]
        arch: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "CREATED")]
        created_at: String,
    }

    let rows: Vec<ContainerRow> = containers
        .iter()
        .map(|c| ContainerRow {
            name: c.name.clone(),
            distro: c.distro.clone(),
            version: c.version.clone(),
            arch: c.arch.clone(),
            status: format!("{:?}", c.status),
            created_at: c.created_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&containers)?,
        OutputFormat::Yaml => output_yaml(&containers)?,
        OutputFormat::Text => {
            for c in containers {
                println!(
                    "{} ({}:{}) [{}]",
                    c.name,
                    c.distro,
                    c.version,
                    format!("{:?}", c.status)
                );
            }
        }
    }
    Ok(())
}

async fn cmd_login(
    container: String,
    user: Option<String>,
    isolated: bool,
    command: Vec<String>,
    binds: Vec<String>,
) -> Result<()> {
    let cmd = if command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        command
    };

    // A bad spec must stop the login, not be dropped: it used to be accepted and ignored.
    let mounts = parse_mount_specs(&binds)?;
    login_container_native(&container, user, isolated, cmd, mounts).await
}

async fn cmd_run(
    container: String,
    command: Vec<String>,
    _rm: bool,
    detach: bool,
    _name: Option<String>,
    _network: Option<String>,
    volume: Vec<String>,
    _publish: Vec<String>,
    _entrypoint: Option<String>,
    _tty: bool,
    _interactive: bool,
    _user: Option<String>,
    _workdir: Option<String>,
    isolated: bool,
    binds: Vec<String>,
) -> Result<()> {
    if detach {
        // Refusing beats the previous behaviour, which printed a warning and ran in the
        // foreground anyway: a caller that asked for background got a blocking process and
        // no way to tell from the exit status.
        anyhow::bail!(
            "--detach is not implemented: a detached session cannot be re-entered yet, so the \
             command would run in the foreground while reporting otherwise. Run it in the \
             foreground, or run the command under `podnext service` supervision."
        );
    }

    // Flags that were parsed and then dropped. Each one changes what the command DOES, so
    // accepting it silently means the caller gets a run that is not the one they asked for
    // and no way to tell from the exit status. They are named together so one run reports
    // every reason instead of one per attempt.
    let mut ignored: Vec<&str> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    if _rm {
        ignored.push("--rm");
    }
    if _name.is_some() {
        ignored.push("--name");
    }
    if _network.is_some() {
        ignored.push("--network");
    }
    if !_publish.is_empty() {
        ignored.push("--publish");
    }
    if _entrypoint.is_some() {
        ignored.push("--entrypoint");
    }
    if _user.is_some() {
        ignored.push("--user");
    }
    if _workdir.is_some() {
        ignored.push("--workdir");
    }
    if !ignored.is_empty() {
        reasons.push(format!(
            "{} not implemented for `run`; the guest would run without it. \
             --workdir and --user are part of the login polygon (v0.2.0), --rm needs ephemeral \
             containers, --name/--network/--publish need the network and naming work that \
             `podnext create` will own. Use `podnext login` for an interactive shell, or drop \
             the flag so the run does what it says",
            ignored.join(", ")
        ));
    }
    if !reasons.is_empty() {
        anyhow::bail!("{}", reasons.join("; "));
    }

    // `--volume` (run) and `--bind` (global) are the same thing; both were parsed and
    // dropped, so the guest never saw them. A malformed spec is a hard error.
    let mut specs = binds;
    specs.extend(volume);
    let mounts = parse_mount_specs(&specs)?;

    run_container_native(&container, command, isolated, mounts).await
}

async fn cmd_ps(format: OutputFormat) -> Result<()> {
    let sessions = list_sessions_native_full().await?;
    let active_disk_sessions = proot_distro_core::session::active_sessions().unwrap_or_default();

    #[derive(Tabled, Serialize)]
    struct SessionRow {
        #[tabled(rename = "ID")]
        id: String,
        #[tabled(rename = "CONTAINER")]
        container: String,
        #[tabled(rename = "USER")]
        user: String,
        #[tabled(rename = "COMMAND")]
        command: String,
        #[tabled(rename = "CWD")]
        cwd: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "CREATED")]
        created: String,
        #[tabled(rename = "STARTED")]
        started: String,
    }

    let mut rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            id: s.id.clone(),
            container: s.container_name.clone(),
            user: s.user.clone(),
            command: s.command.join(" "),
            cwd: s.cwd.clone(),
            pid: s.pid.map_or("-".to_string(), |p| p.to_string()),
            status: format!("{:?}", s.status),
            created: s.created_at.format("%Y-%m-%d %H:%M").to_string(),
            started: s.started_at.map_or("-".to_string(), |dt| {
                dt.format("%Y-%m-%d %H:%M").to_string()
            }),
        })
        .collect();

    for act in active_disk_sessions {
        if !rows.iter().any(|r| r.pid == act.pid.to_string()) {
            rows.push(SessionRow {
                id: format!("sess-{}", act.pid),
                container: act.container_name,
                user: act.login_uid,
                command: act.cmdline.join(" "),
                cwd: "-".to_string(),
                pid: act.pid.to_string(),
                status: "Running".to_string(),
                created: "-".to_string(),
                started: "-".to_string(),
            });
        }
    }

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in sessions {
                println!(
                    "{} {} {} {} {} {}",
                    s.id,
                    s.container_name,
                    s.user,
                    s.pid.map_or("-".to_string(), |p| p.to_string()),
                    format!("{:?}", s.status),
                    s.command.join(" ")
                );
            }
        }
    }
    Ok(())
}

async fn cmd_sessions(format: OutputFormat) -> Result<()> {
    let sessions = list_sessions_native().await?;

    #[derive(Tabled, Serialize)]
    struct SessionRow {
        #[tabled(rename = "CONTAINER")]
        container: String,
        #[tabled(rename = "USER")]
        user: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "STARTED")]
        started: String,
    }

    let rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            container: s.container_name.clone(),
            user: s.user.clone(),
            pid: s.pid.map_or("-".to_string(), |p| p.to_string()),
            status: s.status.clone(),
            started: s.started_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in sessions {
                println!(
                    "{} {} {} {}",
                    s.container_name,
                    s.user,
                    s.pid.map_or("-".to_string(), |p| p.to_string()),
                    s.status
                );
            }
        }
    }
    eprintln!("\nNote: Use 'podnext attach <container>' to create a new session.");
    Ok(())
}

async fn cmd_attach(container: String, user: Option<String>, command: Vec<String>) -> Result<()> {
    let cmd = if command.is_empty() {
        vec!["/bin/bash".to_string()]
    } else {
        command
    };

    login_container_native(&container, user, false, cmd, Vec::new()).await
}

async fn cmd_docker(container_name: Option<String>, args: Vec<String>) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let backend: Box<dyn proot_distro_core::RuntimeBackend> = if let Some(ref name) = container_name
    {
        let c = container_mgr.get(name).await?;
        container_mgr.get_backend(&c)
    } else {
        Box::new(proot_distro_core::UmlBackend::new())
    };

    if !backend.supports_kernel_features() {
        anyhow::bail!(
            "The configured runtime backend '{}' does not support kernel features required by Docker.\n\
             Reason: Docker requires namespaces (mount, pid, net) and cgroups v2, which PRoot cannot provide.\n\
             Resolution: Start the UML backend with 'uml4termux up' and use '--runtime uml'.",
            format!("{:?}", backend.kind()).to_lowercase()
        );
    }

    let status = backend.status().await?;
    if !status.running {
        anyhow::bail!(
            "UML kernel runtime is not running (status: {}). Please start it with 'uml4termux up'.",
            status.details
        );
    }

    let mut docker_cmd = vec!["docker".to_string()];
    docker_cmd.extend(args);

    let dummy_container = proot_distro_core::Container::default();
    let exit_code = backend.execute(&dummy_container, docker_cmd).await?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

async fn cmd_kill(container: String) -> Result<()> {
    let mut killed_any = false;
    if let Ok(active) = proot_distro_core::session::active_sessions() {
        for s in active {
            if s.container_name == container || container == "all" || container == "--all" {
                if let Ok(()) = proot_distro_core::session::kill_session(s.pid, libc::SIGQUIT) {
                    println!(
                        "Sent SIGQUIT to session PID {} ({})",
                        s.pid, s.container_name
                    );
                    killed_any = true;
                }
            }
        }
        if killed_any {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
    kill_container_native(&container).await?;
    if container == "all" || container == "--all" {
        if killed_any {
            println!("Killed all active container sessions.");
        } else {
            println!("No active container sessions found.");
        }
    } else if killed_any {
        println!("Container '{}' killed.", container);
    } else {
        println!("Container '{}' is not running.", container);
    }
    Ok(())
}

async fn cmd_remove(name: String, force: bool) -> Result<()> {
    remove_container_native(name, force).await
}

async fn cmd_info(format: OutputFormat) -> Result<()> {
    let info = SystemInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: "Termux/Android".to_string(),
        rust_version: "1.80+".to_string(),
        backend: "proot-distro-core".to_string(),
        isolation: "proot (unmapped host tree, masked /proc)".to_string(),
    };

    #[derive(Tabled, Serialize)]
    struct InfoRow {
        #[tabled(rename = "VERSION")]
        version: String,
        #[tabled(rename = "PLATFORM")]
        platform: String,
        #[tabled(rename = "RUST VERSION")]
        rust_version: String,
        #[tabled(rename = "BACKEND")]
        backend: String,
        #[tabled(rename = "ISOLATION")]
        isolation: String,
    }

    let row = InfoRow {
        version: info.version.clone(),
        platform: info.platform.clone(),
        rust_version: info.rust_version.clone(),
        backend: info.backend.clone(),
        isolation: info.isolation.clone(),
    };

    let serializable = SerializableInfo {
        version: info.version.clone(),
        platform: info.platform.clone(),
        rust_version: info.rust_version.clone(),
        backend: info.backend.clone(),
        isolation: info.isolation.clone(),
    };

    match format {
        OutputFormat::Table => output_table(&[row]),
        OutputFormat::Json => output_json(&serializable)?,
        OutputFormat::Yaml => output_yaml(&serializable)?,
        OutputFormat::Text => {
            println!(
                "podnext {} - PRoot Distro Next for Termux/Android",
                env!("CARGO_PKG_VERSION")
            );
            println!("Backend: {}", info.backend);
            println!("Isolation: {}", info.isolation);
        }
    }
    Ok(())
}

/// `podnext doctor`: report what the host and the guest actually look like.
///
/// Exit status is driven by the worst verdict: a failing check exits 1. Warnings and
/// unknowns are printed without failing the command, because "not measured" is not the
/// same as "broken".
async fn cmd_doctor(container: Option<String>, format: OutputFormat) -> Result<()> {
    let checks = doctor::collect(container.as_deref()).await;
    match format {
        OutputFormat::Table | OutputFormat::Text => doctor::render_text(&checks),
        OutputFormat::Json => output_json(&checks)?,
        OutputFormat::Yaml => output_yaml(&checks)?,
    }
    if doctor::worst_verdict(&checks) == doctor::Verdict::Fail {
        anyhow::bail!("one or more checks failed; the rows marked `fail` name what and why");
    }
    Ok(())
}

async fn cmd_version(format: OutputFormat) -> Result<()> {
    let version_info = VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: option_env!("GIT_COMMIT").unwrap_or("unknown").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
        rust_version: "1.80+".to_string(),
    };

    match format {
        OutputFormat::Table => output_table(&[version_info]),
        OutputFormat::Json => output_json(&version_info)?,
        OutputFormat::Yaml => output_yaml(&version_info)?,
        OutputFormat::Text => println!("podnext {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

async fn cmd_completion(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let name = "podnext".to_string();

    match shell {
        Shell::Bash => clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Zsh => clap_complete::generate(
            clap_complete::Shell::Zsh,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Fish => clap_complete::generate(
            clap_complete::Shell::Fish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::PowerShell => clap_complete::generate(
            clap_complete::Shell::PowerShell,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Elvish => clap_complete::generate(
            clap_complete::Shell::Elvish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
    }
    Ok(())
}

fn print_help() {
    let mut cmd = Cli::command();
    cmd.print_help().unwrap();
    println!();
}

// Output format helpers
#[derive(Tabled)]
struct SystemInfo {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Platform")]
    platform: String,
    #[tabled(rename = "Rust Version")]
    rust_version: String,
    #[tabled(rename = "Backend")]
    backend: String,
    #[tabled(rename = "Isolation")]
    isolation: String,
}

#[derive(Tabled, serde::Serialize)]
struct VersionInfo {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Git Commit")]
    git_commit: String,
    #[tabled(rename = "Build Date")]
    build_date: String,
    #[tabled(rename = "Rust Version")]
    rust_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guest path in `container:[CONTAINER:]PATH` comes from the user, so it
    /// is a trust boundary. A plain `join` let `c1:../../../../etc/shadow` read a
    /// host file; these cases pin that shut.
    #[test]
    fn guest_to_host_refuses_traversal() {
        let rootfs = std::path::Path::new("/data/podnext/containers/c1/rootfs");

        // Ordinary paths resolve under the rootfs.
        assert_eq!(
            guest_to_host(rootfs, "/etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );
        assert_eq!(
            guest_to_host(rootfs, "etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );
        // "." is a no-op, and a trailing slash is fine.
        assert_eq!(
            guest_to_host(rootfs, "/./etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );

        // Anything that climbs out is refused, however it is spelled.
        for evil in [
            "/../../../../etc/shadow",
            "../etc/shadow",
            "etc/../../..",
            "/etc/../../../etc/shadow",
            "..",
            "/..",
        ] {
            assert!(
                guest_to_host(rootfs, evil).is_err(),
                "expected {:?} to be refused",
                evil
            );
        }
    }

    fn tmp_tree(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdn-sync-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sync_transfers_only_what_differs() {
        let root = tmp_tree("diff");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "one").unwrap();
        std::fs::write(src.join("sub/b.txt"), "two").unwrap();

        let mut report = SyncReport::default();
        mirror_dir(&src, &dst, &mut report).unwrap();
        assert_eq!(report.copied, 2, "first pass copies everything");
        assert_eq!(
            std::fs::read_to_string(dst.join("sub/b.txt")).unwrap(),
            "two"
        );

        // Second pass: nothing changed, nothing moves.
        let mut second = SyncReport::default();
        mirror_dir(&src, &dst, &mut second).unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.unchanged, 2);

        // Change one file and only that one moves.
        std::fs::write(src.join("a.txt"), "ONE!").unwrap();
        let mut third = SyncReport::default();
        mirror_dir(&src, &dst, &mut third).unwrap();
        assert_eq!(third.copied, 1);
        assert_eq!(third.unchanged, 1);
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "ONE!");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_removes_only_orphans() {
        let root = tmp_tree("prune");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(dst.join("stale-dir")).unwrap();
        std::fs::write(src.join("keep.txt"), "k").unwrap();
        std::fs::write(dst.join("keep.txt"), "k").unwrap();
        std::fs::write(dst.join("drop.txt"), "d").unwrap();
        std::fs::write(dst.join("stale-dir/inner.txt"), "d").unwrap();

        let mut report = SyncReport::default();
        prune_extras(&src, &dst, &mut report).unwrap();

        assert!(dst.join("keep.txt").exists(), "a live file must survive");
        assert!(!dst.join("drop.txt").exists(), "an orphan must go");
        assert!(
            !dst.join("stale-dir").exists(),
            "an orphan directory must go"
        );
        assert_eq!(report.removed, 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn same_content_compares_bytes_not_names() {
        let root = tmp_tree("same");
        std::fs::write(root.join("a"), "abcdef").unwrap();
        std::fs::write(root.join("b"), "abcdef").unwrap();
        std::fs::write(root.join("c"), "abcde").unwrap();

        assert!(same_content(&root.join("a"), &root.join("b")).unwrap());
        assert!(!same_content(&root.join("a"), &root.join("c")).unwrap());
        assert!(!same_content(&root.join("a"), &root.join("missing")).unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_keeps_sparse_files_sparse() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::unix::fs::MetadataExt;

        let root = tmp_tree("sparse");
        let src = root.join("sparse.bin");
        let dst = root.join("copy.bin");

        // 64 MiB apparent, one 4 KiB page of real data: dense would be ~131072
        // blocks; the bug made exactly that (issue #18).
        let f = std::fs::File::create(&src).unwrap();
        f.set_len(64 * 1024 * 1024).unwrap();
        drop(f);
        let mut w = std::fs::OpenOptions::new().write(true).open(&src).unwrap();
        w.seek(SeekFrom::Start(1024 * 1024)).unwrap();
        w.write_all(&[0xAB; 4096]).unwrap();
        drop(w);

        copy_preserving_holes(&src, &dst).unwrap();

        let s = std::fs::metadata(&src).unwrap();
        let d = std::fs::metadata(&dst).unwrap();
        assert_eq!(s.len(), d.len(), "apparent size must match");
        assert_eq!(
            std::fs::read(&src).unwrap(),
            std::fs::read(&dst).unwrap(),
            "content must be byte-identical"
        );

        // A filesystem with no hole support would densify the source too; there
        // is nothing to assert there.
        if s.blocks() * 512 >= 1024 * 1024 {
            let _ = std::fs::remove_dir_all(&root);
            return;
        }
        assert!(
            d.blocks() * 512 < 1024 * 1024,
            "sparse copy allocated {} bytes, expected far less than 1 MiB",
            d.blocks() * 512
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn split_spec_only_treats_a_path_free_prefix_as_a_container() {
        assert_eq!(split_spec("c1:/etc/x"), (Some("c1"), "/etc/x"));
        assert_eq!(split_spec("/etc/x"), (None, "/etc/x"));
        // A host path containing a colon must not be read as a container name.
        assert_eq!(split_spec("/data/a:b/c"), (None, "/data/a:b/c"));
        assert_eq!(split_spec("./rel/x"), (None, "./rel/x"));
    }

    /// `image inspect` must find what `build local`'s store holds, under the tag
    /// or under the derived store key (#29 / #35), and must not invent an answer
    /// for a reference neither source knows.
    #[test]
    fn store_metadata_lookup_accepts_tag_and_key_and_reports_absence() {
        let store = tempfile::tempdir().unwrap();
        let dir = store.path().join("demoinspect");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metadata.json"),
            r#"{"name":"demoinspect:latest","image":"scratch","exposed_ports":[8080]}"#,
        )
        .unwrap();

        // By tag: the store key is the colon-free form.
        let (found_dir, meta) = read_store_metadata(store.path(), "demoinspect:latest")
            .expect("the tag must resolve to its store key");
        assert_eq!(found_dir.file_name().unwrap(), "demoinspect");
        assert_eq!(meta["exposed_ports"][0], 8080);

        // By store key.
        assert!(read_store_metadata(store.path(), "demoinspect").is_some());

        // Unknown reference: nothing, so the caller reports it instead of exiting 0
        // with no output.
        assert!(read_store_metadata(store.path(), "nosuchimg").is_none());

        // A reference that could escape the store is not a path lookup.
        assert!(read_store_metadata(store.path(), "../demoinspect").is_none());

        // A directory without metadata.json is not an image.
        std::fs::create_dir_all(store.path().join("empty")).unwrap();
        assert!(read_store_metadata(store.path(), "empty").is_none());
    }

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn format_accepted_globally() {
        use clap::Parser;
        let cli = Cli::try_parse_from(&["podnext", "--format", "json", "list"]).unwrap();
        assert_eq!(cli.format, Some(OutputFormat::Json));
    }

    #[test]
    fn conflicting_flags_error() {
        use clap::Parser;
        assert!(Cli::try_parse_from(&["podnext", "list", "--all", "--no-all"]).is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--seccomp",
            "--no-seccomp",
            "--",
            "ls"
        ])
        .is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--sysvipc",
            "--no-sysvipc",
            "--",
            "ls"
        ])
        .is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--kill-on-exit",
            "--no-kill-on-exit",
            "--",
            "ls"
        ])
        .is_err());
    }
}
