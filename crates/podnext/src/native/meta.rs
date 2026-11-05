//! Container and session listing, the image metadata re-apply, and the
//! command-line mount-spec helpers.
//!
//! Split out of `native` in wave 7; behaviour unchanged.

use anyhow::{bail, Result};
use std::fs;
use std::io::{self, Write};

// ===== Native Container Operations =====

pub async fn list_containers_native(all: bool) -> Result<Vec<proot_distro_core::ContainerInfo>> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let containers = container_mgr.list().await?;

    if !all {
        // Filter to only running
        let containers: Vec<_> = containers
            .into_iter()
            .filter(|c| c.status == proot_distro_core::ContainerStatus::Running)
            .collect();
        return Ok(containers);
    }

    Ok(containers)
}

/// Parse a mount spec from the command line: `HOST:GUEST[:ro|rw]`.
///
/// PRoot splits a bind argument on `:`, so the guest half must contain none; the
/// host half is validated by [`proot_distro_core::bindings::argument`], the single
/// place the splitting rules live. The guest path must be absolute, because PRoot
/// resolves it inside the traced root.
pub fn parse_mount_spec(spec: &str) -> Result<proot_distro_core::container::BindMount> {
    let (body, readonly) = if let Some(rest) = spec.strip_suffix(":ro") {
        (rest, true)
    } else if let Some(rest) = spec.strip_suffix(":rw") {
        (rest, false)
    } else {
        (spec, false)
    };
    let Some((source, target)) = body.split_once(':') else {
        anyhow::bail!("invalid mount '{spec}': expected HOST:GUEST[:ro|rw]");
    };
    if source.is_empty() || target.is_empty() {
        anyhow::bail!("invalid mount '{spec}': the host and guest paths are both required");
    }
    if !target.starts_with('/') {
        anyhow::bail!("invalid mount '{spec}': the guest path must be absolute (got '{target}')");
    }
    if !std::path::Path::new(source).exists() {
        anyhow::bail!("mount source '{source}' does not exist");
    }
    if readonly {
        anyhow::bail!(
            "read-only mounts are not supported: proot 5.1.107.101 has no read-only bind form, \
             so the ':ro' would become part of the guest path name and the mount would land at \
             '{target}:ro' instead of '{target}'. Drop the ':ro' suffix to mount read-write, or \
             restrict access inside the guest"
        );
    }
    let _ = proot_distro_core::bindings::argument(source, target, readonly)?;
    Ok(proot_distro_core::container::BindMount {
        source: source.to_string(),
        target: target.to_string(),
        readonly,
    })
}

/// Parse every `--bind`/`--volume` spec, reporting the first bad one.
pub fn parse_mount_specs(specs: &[String]) -> Result<Vec<proot_distro_core::container::BindMount>> {
    specs.iter().map(|s| parse_mount_spec(s)).collect()
}

/// Reinstate the environment and working directory an image was built with.
///
/// `podnext build local` records `env`/`working_dir` in the image's
/// `metadata.json` (#29), but `ContainerManager::load_containers` keys a
/// container's config off its directory name and `os-release` only and leaves
/// `env_vars`/`working_dir` at their defaults. A later `podnext run` therefore
/// started with an empty environment and `cwd=/`, so the image did not behave as
/// built. Re-apply the recorded values here, immediately before the backend
/// builds the proot command (`proot-distro-core/src/runtime.rs` reads exactly
/// these two fields). Values already set on the container win, so a caller
/// override is never clobbered.
pub(crate) fn apply_image_runtime_config(container: &mut proot_distro_core::Container) {
    let Some(dir) = container.rootfs_path.parent() else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(dir.join("metadata.json")) else {
        return;
    };
    let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };

    if container.config.env_vars.is_empty() {
        if let Some(env) = meta.get("env").and_then(|v| v.as_object()) {
            let mut vars: Vec<(String, String)> = env
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect();
            // JSON objects carry no order; sort so repeated runs are identical.
            vars.sort();
            container.config.env_vars = vars;
        }
    }
    if container.config.working_dir.is_none() {
        if let Some(wd) = meta.get("working_dir").and_then(|v| v.as_str()) {
            if !wd.is_empty() {
                container.config.working_dir = Some(wd.to_string());
            }
        }
    }
}

pub async fn list_sessions_native() -> Result<Vec<proot_distro_core::SessionInfo>> {
    let session_mgr = proot_distro_core::SessionManager::new()?;
    Ok(session_mgr.list_all())
}

pub async fn list_sessions_native_full() -> Result<Vec<proot_distro_core::Session>> {
    let session_mgr = proot_distro_core::SessionManager::new()?;
    Ok(session_mgr.list_all_full())
}

pub async fn kill_container_native(container_name: &str) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    container_mgr.kill(container_name).await
}

pub async fn remove_container_native(name: String, force: bool) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let container_dir = dirs::home_dir().map(|h| h.join(".podnext").join("containers").join(&name));
    let dir_exists = container_dir.as_ref().map(|d| d.exists()).unwrap_or(false);
    let in_mgr = container_mgr.get(&name).await.is_ok();

    if !in_mgr && !dir_exists {
        bail!("Container '{}' not found", name);
    }

    // Check existence first (Finding 04-4)
    container_mgr.get(&name).await?;

    if !force {
        use std::io::IsTerminal;
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "stdin is not a tty: use -f/--force or -y/--yes to confirm removal of container '{}'",
                name
            );
        }

        eprint!("Remove container '{}'? [y/N]: ", name);
        io::stderr().flush()?;
        let mut input = String::new();
        let bytes = io::stdin().read_line(&mut input)?;
        if bytes == 0 || !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("Cancelled.");
            std::process::exit(1);
        }
    }

    if in_mgr {
        let _ = container_mgr.remove(&name).await;
    }

    // Delete the on-disk container directory (including orphan directories without metadata)
    if let Some(dir) = container_dir {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| {
                anyhow::anyhow!(
                    "removed '{}' from the index but failed to delete {}: {}",
                    name,
                    dir.display(),
                    e
                )
            })?;
        }
    }

    println!("Container '{}' removed.", name);
    Ok(())
}
