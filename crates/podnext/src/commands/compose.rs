//! `podnext compose` — export PDN service configuration to a Docker Compose file.
//!
//! Loading reuses `ServiceSupervisor::load_service_from_file`, so a file that is
//! not a valid supervised service (missing name or command) is rejected here
//! before export. The exporter itself is in `pn_next::supervisor`.

use anyhow::{Context, Result};
use pn_next::supervisor::export_services_to_compose;
use pn_next::{ServiceConfig, ServiceSupervisor};
use std::path::PathBuf;

/// Export the given service definition file(s) as one Compose document.
pub async fn execute_compose_command(
    files: Vec<PathBuf>,
    image: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    let mut services: Vec<ServiceConfig> = Vec::with_capacity(files.len());
    for f in &files {
        let cfg = ServiceSupervisor::load_service_from_file(f)
            .await
            .with_context(|| format!("loading service config {}", f.display()))?;
        services.push(cfg);
    }

    let doc = export_services_to_compose(&services, image.as_deref());

    match output {
        Some(path) => {
            std::fs::write(&path, &doc).with_context(|| format!("writing {}", path.display()))?;
            println!("Wrote {} service(s) to {}", services.len(), path.display());
        }
        None => print!("{doc}"),
    }
    Ok(())
}
