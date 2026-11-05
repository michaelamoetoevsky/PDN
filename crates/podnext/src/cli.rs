//! Clap command surface for the podnext CLI.
//!
//! Holds the whole argument surface - `Cli`, `Commands`, the per-domain
//! sub-command enums and the shared value enums. `main.rs` re-exports these
//! so `crate::<Type>` keeps resolving for the other modules.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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
    podnext dbus serve --socket $PREFIX/tmp/pn-dbus.sock  # Run IPC D-Bus broker\n\
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
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    // Direct execution mode (like proot)
    #[arg(
        short = 'r',
        long = "rootfs",
        global = true,
        help = "Use PATH as guest root filesystem"
    )]
    pub(crate) rootfs: Option<String>,

    #[arg(
        short = 'b',
        long = "bind",
        global = true,
        value_name = "HOST_PATH:GUEST_PATH",
        help = "Bind mount host path into guest (repeatable)"
    )]
    pub(crate) bind: Vec<String>,

    #[arg(
        short = 'w',
        long = "pwd",
        global = true,
        help = "Set initial working directory"
    )]
    pub(crate) pwd: Option<String>,

    #[arg(
        short = 'q',
        long = "qemu",
        global = true,
        help = "Execute through QEMU user-mode emulation (path to qemu binary)"
    )]
    pub(crate) qemu: Option<String>,

    #[arg(
        short = '0',
        long = "link2symlink",
        global = true,
        help = "Enable hardlink emulation via symlinks"
    )]
    pub(crate) link2symlink: bool,

    #[arg(
        long = "no-sysvipc",
        global = true,
        conflicts_with = "sysvipc",
        help = "Disable System V IPC emulation"
    )]
    pub(crate) no_sysvipc: bool,

    #[arg(
        long = "sysvipc",
        global = true,
        conflicts_with = "no_sysvipc",
        help = "Enable System V IPC emulation"
    )]
    pub(crate) sysvipc: bool,

    #[arg(
        long = "kernel",
        global = true,
        help = "Override kernel release string"
    )]
    pub(crate) kernel: Option<String>,

    #[arg(long = "hostname", global = true, help = "Set hostname")]
    pub(crate) hostname: Option<String>,

    #[arg(long = "uid", global = true, help = "User ID to run as")]
    pub(crate) uid: Option<u32>,

    #[arg(long = "gid", global = true, help = "Group ID to run as")]
    pub(crate) gid: Option<u32>,

    #[arg(
        long = "seccomp",
        global = true,
        conflicts_with = "no_seccomp",
        help = "Enable seccomp filtering"
    )]
    pub(crate) seccomp: bool,

    #[arg(
        long = "no-seccomp",
        global = true,
        conflicts_with = "seccomp",
        help = "Disable seccomp filtering"
    )]
    pub(crate) no_seccomp: bool,

    #[arg(long = "trace", global = true, help = "Enable syscall tracing")]
    pub(crate) trace: bool,

    #[arg(
        long = "kill-on-exit",
        global = true,
        conflicts_with = "no_kill_on_exit",
        help = "Kill processes on exit"
    )]
    pub(crate) kill_on_exit: bool,

    #[arg(
        long = "no-kill-on-exit",
        global = true,
        conflicts_with = "kill_on_exit",
        help = "Don't kill processes on exit"
    )]
    pub(crate) no_kill_on_exit: bool,

    #[arg(
        short = 'e',
        long = "env",
        global = true,
        value_name = "VAR=VALUE",
        help = "Set environment variable (repeatable)"
    )]
    pub(crate) env: Vec<String>,

    /// Enable verbose debug output
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count, help = "Enable verbose debug output (repeatable)")]
    pub(crate) verbose: u8,

    /// Output format
    #[arg(
        long = "format",
        global = true,
        value_enum,
        help = "Output format (table, json, yaml, text)"
    )]
    pub(crate) format: Option<OutputFormat>,
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum Commands {
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
        podnext login debian --isolated"
    )]
    Login {
        /// Container name
        container: String,
        /// User to run as (not implemented: `login` refuses it; the guest runs as the mapped root)
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
        /// Run in the background under a supervised PTY; reattach with `podnext attach <container>` (Ctrl-P Ctrl-Q detaches again)
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

    /// [experimental] [Container] Attach to a running detached session (reconnects; does not start one)
    #[command(
        long_about = "Reconnect to a session started with `podnext run --detach <container>`. `attach` never starts a session: with none running for the container it exits 1 and names the command that starts one. The argument is the container name, not a session id.\n\n\
        Ctrl-P Ctrl-Q detaches again and leaves the guest running; the same guest shell answers the next attach. Two declared simplifications: a resize of this terminal while attached is not re-forwarded to the guest, and output produced while detached is appended to the session log but not replayed on reconnect."
    )]
    Attach {
        /// Container name
        container: String,
        /// User to run as (not implemented: `attach` refuses it; a session's user is fixed when it
        /// starts, and the guest runs as the mapped root)
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
pub(crate) enum ServiceCommands {
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
pub(crate) enum DaemonCommands {
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
pub(crate) enum WatchdogCommands {
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
pub(crate) enum QuotaCommands {
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
pub(crate) enum LogCommands {
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
pub(crate) enum DbusCommands {
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
pub(crate) enum SocketCommands {
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
pub(crate) enum BootCommands {
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
pub(crate) enum WakelockCommands {
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
pub(crate) enum PolicyCommands {
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
pub(crate) enum BackupCommands {
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
pub(crate) enum BackupTypeArg {
    Full,
    Incremental,
    Differential,
    ConfigOnly,
}

/// Gateway configuration checks
#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum ConfigCommands {
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
pub(crate) enum BuildCommands {
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
pub(crate) enum GitopsCommands {
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
pub(crate) enum DashboardCommands {
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
pub(crate) enum RestartPolicyArg {
    Never,
    OnFailure,
    Always,
    UnlessStopped,
    OnFailureOrCrash,
}

#[derive(ValueEnum, Clone, Copy, Debug, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum ResourceTypeArg {
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
pub(crate) enum LogLevelArg {
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
pub(crate) enum WakeLockTypeArg {
    Partial,
    Full,
    DozeExemption,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}
