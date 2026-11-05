//! `podnext network` - isolated bridge networks.

use clap::Subcommand;

use super::common::OutputFormat;

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
