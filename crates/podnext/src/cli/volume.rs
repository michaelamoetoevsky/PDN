//! `podnext volume` - persistent data volumes.

use clap::Subcommand;

use super::common::OutputFormat;

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
