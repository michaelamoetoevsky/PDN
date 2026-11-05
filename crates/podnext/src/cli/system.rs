//! The platform command families: D-Bus and socket IPC, Termux boot and wakelock,
//! the policy engine, gateway configuration, GitOps reconciliation and the
//! dashboard server.

use clap::Subcommand;
use std::path::PathBuf;

use super::common::{OutputFormat, WakeLockTypeArg};

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
