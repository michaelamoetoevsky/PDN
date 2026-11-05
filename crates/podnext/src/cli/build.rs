//! `podnext build` - the local image builder and the build-service sub-commands.

use clap::Subcommand;
use std::path::PathBuf;

use super::common::OutputFormat;

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
