//! `podnext image` - the local OCI image cache.

use clap::Subcommand;

use super::common::OutputFormat;

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
