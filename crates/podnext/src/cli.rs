//! Clap command surface for the podnext CLI.
//!
//! Holds the two top-level types - `Cli` and the `Commands` enum every
//! sub-command hangs off. The per-domain sub-command enums and the shared value
//! enums live in the sibling modules under `cli/` and are re-exported here, so
//! the rest of the crate keeps naming them as `crate::<Type>`.
//!
//! | module | holds |
//! |---|---|
//! | `common` | `OutputFormat` and the option families several domains accept |
//! | `image` / `network` / `volume` | the container-storage sub-commands |
//! | `service` | service, daemon, watchdog, quota and log |
//! | `system` | D-Bus, socket, boot, wakelock, policy, config, GitOps, dashboard |
//! | `backup` / `build` | snapshot management and the image builder |

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

mod backup;
mod build;
mod common;
mod image;
mod network;
mod service;
mod system;
mod volume;

pub(crate) use backup::*;
pub(crate) use build::*;
pub(crate) use common::*;
pub(crate) use image::*;
pub(crate) use network::*;
pub(crate) use service::*;
pub(crate) use system::*;
pub(crate) use volume::*;

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

    // SHORT FORM CORRECTED 2026-11-09 to follow the reference (measured from the installed
    // `proot --help`): there, `--link2symlink` is `-l`, and `-0` belongs to `--root-id`. PDN had
    // `-0` on this flag, so a reader of the proot documentation asked for root identity and silently
    // got hardlink emulation instead. `-0` now carries the reference's meaning and refuses by name
    // in `main.rs`, because the guest identity is already mapped.
    // No short form: the reference's `-l` is taken by `daemon logs --lines`, and adding it globally
    // makes clap PANIC for that command only - measured: `podnext daemon logs --help` exited 101 with
    // "Short option names must be unique for each argument, but '-l' is in use by both 'lines' and
    // 'link2symlink'". The help-dump gate caught it as a screen that printed <<exit 101>>, which is
    // why that gate exists. `-0` was removed above because the reference gives it to `--root-id`.
    #[arg(
        long = "link2symlink",
        global = true,
        help = "Enable hardlink emulation via symlinks"
    )]
    pub(crate) link2symlink: bool,

    // The reference's `-0` / `--root-id`: "make current user appear as root and fake its
    // privileges". Accepted here so the refusal can name it and give the reason, and short = '0'
    // matches the reference spelling. Measured with a decoy PROOT_BIN: the generated proot argv is
    // 139 arguments and byte-identical with and without `-0`, and `proot -0` on this device reports
    // uid 0 where proot alone reports uid 10396 - which is what PDN already does everywhere.
    #[arg(
        short = '0',
        long = "root-id",
        global = true,
        help = "Make the guest appear as root (refused: already mapped)"
    )]
    pub(crate) root_id: bool,

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

    // `proot` (default) runs the host `proot` binary, unchanged; `native` runs the guest under
    // podnext's own in-tree ptrace engine and never touches a `proot` binary. The detail lives in this
    // plain comment on purpose: as a doc comment it became clap's `long_help`, and because this option
    // is global a multi-line `long_help` switches EVERY screen to next-line layout (measured:
    // research/cli-help-baseline.txt went 4733 -> 10168 lines for an 8-line option). One line keeps the
    // published surface reviewable.
    /// Container engine: `native` (in-tree ptrace engine, the default) or `proot` (host binary)
    #[arg(
        long = "engine",
        global = true,
        value_enum,
        default_value = "native",
        help = "Container engine: native (default) or proot"
    )]
    pub(crate) engine: EngineChoice,
}

/// Parse the command line, answering a rejected invocation with the page of the
/// command that owns it.
///
/// clap's own answer to an unknown flag is a one-line `Usage:` blob: it names no
/// option and describes nothing, so the reader cannot tell what the command they
/// got wrong actually accepts. A bad flag on `image ls` is answered by
/// `podnext image ls --help` plus the reason, on stderr, and exits 2 (the
/// documented code for invalid arguments).
///
/// `--help` and `--version` are not failures and keep clap's own handling: their
/// page goes to stdout and the exit status is 0.
pub(crate) fn parse_or_exit() -> Cli {
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) {
                err.exit();
            }
            let page = owning_page();
            let _ = err.print();
            if !page.is_empty() {
                eprintln!();
                eprint!("{page}");
            }
            std::process::exit(2);
        }
    }
}

/// The page of the deepest command named on the command line, rendered by clap
/// itself (`podnext <path> --help`), so it carries the global options that command
/// inherits. Empty when no path resolves, which leaves the error alone.
///
/// Flag tokens are skipped, which is what lets a global option precede the
/// sub-command (`podnext --format json list --typo` still resolves to `list`).
/// Known ceiling: a *value* that happens to spell a sub-command name resolves one
/// level too deep, and a flag taking a value that names a sub-command
/// (`podnext -b list list --typo`) resolves to the wrong page.
fn owning_page() -> String {
    let mut argv: Vec<std::ffi::OsString> = vec!["podnext".into()];
    let mut cmd = Cli::command();
    for arg in std::env::args_os().skip(1) {
        // Issue #41: argv is bytes. `args()` panics re-reading argv when any argument is not valid
        // UTF-8, and this runs on the failure path - exactly where a bad argument is likely.
        let arg = arg.to_string_lossy();
        if arg.starts_with('-') {
            continue;
        }
        if let Some(sub) = cmd.find_subcommand(&*arg) {
            cmd = sub.clone();
            argv.push(arg.as_ref().into());
        }
    }
    argv.push("--help".into());
    match Cli::try_parse_from(argv) {
        Err(err) if err.kind() == ErrorKind::DisplayHelp => err.to_string(),
        _ => String::new(),
    }
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lower")]
pub(crate) enum Commands {
    // ------------------------------------------------------------------------
    // TIER 1: CORE DISTRO COMMANDS (Simple & Daily Usage - proot-distro parity)
    // ------------------------------------------------------------------------
    /// [experimental] [Core] Install a distribution from an image, archive or URL
    #[command(
        long_about = "Unpack a rootfs into a clean, isolated container. The source is a Docker/OCI\n\
        image reference, a local archive (a plain rootfs tarball or an OCI image-layout\n\
        tar), or an http(s) URL to one of those. A local archive and a URL are unpacked\n\
        offline; the container name is derived from the source's last component unless\n\
        --name is given.\n\n\
        EXAMPLES:\n  \
        podnext install debian:13\n  \
        podnext install ubuntu:22.04 --name my-ubuntu\n  \
        podnext install ./rootfs.tar.gz\n  \
        podnext install ./image.oci.tar --name deb\n  \
        podnext install https://example.org/rootfs.tar.xz --checksum sha256:<hex>"
    )]
    Install {
        /// Image reference (ubuntu:22.04), local archive (./rootfs.tar.gz), or http(s) URL
        image: String,
        /// Container name (defaults to the source's derived alias)
        #[arg(short, long)]
        name: Option<String>,
        /// Force reinstall if container exists
        #[arg(short, long)]
        force: bool,
        /// Verify the archive against this sha256 (as `sha256:<hex>` or bare hex)
        #[arg(long, value_name = "SHA256")]
        checksum: Option<String>,
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

        // REFUSED BY NAME (2026-11-09). Real use reported `podnext login <ctr> --shared-home` and friends
        // failing with clap's `error: unexpected argument ... found`, which reads as a typo and says
        // nothing about what to do. These flags are the proot-distro login polygon that PDN does not
        // implement; accepting them here lets `cmd_login` refuse each one by name with its remedy, and
        // `hide = true` keeps `--help` (and its pinned fixture) unchanged while doing so. A genuine typo
        // still fails at parse time, so this does not swallow mistakes.
        // IMPLEMENTED 2026-11-09 with the reference's OWN semantics, read from the installed
        // proot-distro package (commands/login/proot_cmd.py:205-216):
        //   --shared-home / --termux-home -> --bind=<TERMUX_HOME>:/root   (login_uid is always 0 here)
        //   --shared-tmp                 -> --bind=<TERMUX_PREFIX>/tmp:/tmp
        //   --shared-x11                 -> --bind=<TERMUX_PREFIX>/tmp/.X11-unix:/tmp/.X11-unix
        // and `proot-distro login` itself carries -d/--detach (parser.py:158), so `login` gets it too.
        /// Share the host home with the guest at /root (the reference's --termux-home does the same)
        #[arg(long = "shared-home")]
        shared_home: bool,
        /// Alias of --shared-home, spelled the way proot-distro spells it
        #[arg(long = "termux-home")]
        termux_home: bool,
        /// Share the host temp directory with the guest at /tmp
        #[arg(long = "shared-tmp")]
        shared_tmp: bool,
        /// Share the X11 socket directory with the guest at /tmp/.X11-unix
        #[arg(long = "shared-x11")]
        shared_x11: bool,
        /// Run the session in the background; reattach with `podnext attach <container>`
        #[arg(long = "detach", short = 'd')]
        detach: bool,
        #[arg(long = "fix-low-ports", hide = true)]
        fix_low_ports: bool,
        #[arg(long = "no-link2symlink", hide = true)]
        no_link2symlink: bool,
        #[arg(long = "change-id", hide = true)]
        change_id: Option<String>,
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

        /// Signal to deliver: a name (STOP, TERM, QUIT, KILL, ...) or a number.
        /// Naming one delivers exactly it, once, and never escalates.
        #[arg(short = 's', long = "signal", value_name = "SIGNAL")]
        signal: Option<String>,
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
