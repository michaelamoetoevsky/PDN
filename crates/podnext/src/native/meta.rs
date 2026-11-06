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

/// One session, as the registry reports it.
///
/// The single reader behind `podnext ps`, `podnext ps -f json` and
/// `podnext sessions` (A2). The three used to be derived from three different
/// lists -- the in-process `SessionManager`, which is empty in every fresh
/// process, and the on-disk registry reached separately by `ps` -- so at one
/// instant `ps` listed a session that `sessions` and `ps -f json` did not. The
/// disk registry is the only source that crosses process boundaries; deriving
/// all three from it is what makes them agree.
#[derive(Debug, Clone, serde::Serialize, tabled::Tabled)]
pub struct SessionRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "CONTAINER")]
    pub container: String,
    #[tabled(rename = "USER")]
    pub user: String,
    #[tabled(rename = "COMMAND")]
    pub command: String,
    #[tabled(rename = "CWD")]
    pub cwd: String,
    #[tabled(rename = "PID")]
    pub pid: String,
    #[tabled(rename = "STATUS")]
    pub status: String,
    #[tabled(rename = "CREATED")]
    pub created: String,
    #[tabled(rename = "STARTED")]
    pub started: String,
}

/// Every live session, in one deterministic order.
///
/// `id` is `sess-<pid>`: the name `ps` printed before this reader existed, kept
/// so scripts that matched it keep working. Liveness is the registry's own
/// `(pid, start-time, not-a-zombie)` rule (B13), so a dead or recycled PID is
/// not listed. The `cwd` and `created` columns are not recorded by the runtime
/// and stay `-` rather than being invented.
pub fn session_registry() -> Vec<SessionRow> {
    let mut rows: Vec<SessionRow> = proot_distro_core::session::active_sessions()
        .unwrap_or_default()
        .into_iter()
        .map(|r| SessionRow {
            id: format!("sess-{}", r.pid),
            container: r.container_name,
            user: r.login_uid,
            command: r.cmdline.join(" "),
            cwd: "-".to_string(),
            pid: r.pid.to_string(),
            status: "Running".to_string(),
            created: "-".to_string(),
            started: format_started(r.started_at),
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows
}

/// The registry's wall-clock start time, in the same UTC format the other tables use.
fn format_started(unix_secs: u64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(unix_secs as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub async fn list_sessions_native() -> Result<Vec<SessionRow>> {
    Ok(session_registry())
}

/// The caller that used the full in-memory list now reads the same registry:
/// two sources is exactly how the renderings diverged (A2).
pub async fn list_sessions_native_full() -> Result<Vec<SessionRow>> {
    Ok(session_registry())
}

pub async fn kill_container_native(container_name: &str) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    container_mgr.kill(container_name).await
}

pub async fn remove_container_native(name: String, force: bool) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    // Reclaim staging residue a killed install left behind (A3b, from dept/w1-lifecycle):
    // `remove` is the other command that must be able to clear the store, and it used to leave
    // every `.tmp-install-*` directory untouched. It runs BEFORE the existence check so that
    // residue is cleared even when no committed container matches the name.
    //
    // Re-inserted by hand while resolving the `native/meta.rs` collision: both departments
    // changed this file (this one inserted the reaper, the other rewrote the session registry and
    // this function's root resolution), and taking either side wholesale would have silently
    // dropped the other's change. It uses the same resolved root as the code below, so the reaper
    // and the removal cannot look at two different stores.
    if let Ok(paths) = proot_distro_core::paths::ContainerPaths::new() {
        crate::native::reap_stale_staging(&paths.containers_dir);
    }
    // The container store's own resolved root. `dirs::home_dir()` is a second,
    // unrelated answer - it ignores `PN_TEST_ROOT` and a relocated `HOME` - so
    // `remove` could find the container in the index and then look for its
    // directory under a different root entirely.
    let container_dir = proot_distro_core::paths::ContainerPaths::new()
        .ok()
        .map(|paths| paths.containers_dir.join(&name));
    let dir_exists = container_dir.as_ref().map(|d| d.exists()).unwrap_or(false);
    let in_mgr = container_mgr.get(&name).await.is_ok();

    if !in_mgr && !dir_exists {
        bail!("Container '{}' not found", name);
    }

    // Check existence first (Finding 04-4)
    container_mgr.get(&name).await?;

    // A5: a live session owns the container's directory, and that directory cannot
    // be deleted from under it (measured: `rm -rf` of a running container's store
    // directory fails with EACCES). Refuse BEFORE touching the index, naming the
    // session, so the state can never be "the index lost the record while the
    // directory - and the guest - stayed". Deciding to stop the session belongs to
    // the operator (or an explicit `podnext kill`), not to a silent `remove`.
    let live: Vec<u32> = proot_distro_core::session::active_sessions()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.container_name == name)
        .map(|s| s.pid)
        .collect();
    if !live.is_empty() {
        let ids: Vec<String> = live.iter().map(|pid| format!("pid {pid}")).collect();
        bail!(
            "Container '{}' has a live session ({}); refusing to remove it. Stop the session \
             first (e.g. `podnext kill {}`), then remove it.",
            name,
            ids.join(", "),
            name
        );
    }

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

    // A5: delete the directory FIRST and drop the index record only once that
    // succeeded, so the index and the directory move together or not at all. The
    // old order dropped the record first, so a failed delete (EACCES because a
    // process still held the directory) left the record gone and the directory
    // behind - the half-applied state.
    if let Some(dir) = container_dir {
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .map_err(|e| anyhow::anyhow!("failed to delete {}: {}", dir.display(), e))?;
        }
    }

    if in_mgr {
        let _ = container_mgr.remove(&name).await;
    }

    // A removed container must not leave its session's rendezvous socket or
    // transcript behind (A9): `kill` + `remove` returned 0 while
    // `~/.podnext/sessions` kept both, with no process owning them.
    proot_distro_core::session::clear_container_sessions(&name);

    println!("Container '{}' removed.", name);
    Ok(())
}
