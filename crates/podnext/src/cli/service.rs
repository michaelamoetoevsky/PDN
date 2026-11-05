//! Service supervision: `podnext service` and the daemon, watchdog, quota and log
//! command families that share its shape.

use clap::Subcommand;
use std::path::PathBuf;

use super::common::{LogLevelArg, OutputFormat, ResourceTypeArg, RestartPolicyArg};

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
