//! `podnext backup` - container snapshot management.

use clap::Subcommand;

use super::common::{BackupTypeArg, OutputFormat};

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
