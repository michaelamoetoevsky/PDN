//! Container commands: install, list, login, run, ps, sessions, attach, docker, kill, remove,
//! info.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::container::*`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use tabled::Tabled;

use crate::native::{
    install_image_native, kill_container_native, list_containers_native, list_sessions_native,
    list_sessions_native_full, login_container_native, parse_mount_specs, remove_container_native,
    run_container_native, search_docker_hub, verify_image_available,
};
use crate::utils::format_size;
use crate::{output_json, output_table, output_yaml, OutputFormat, SystemInfo};

pub async fn cmd_install(image: String, name: Option<String>, force: bool) -> Result<()> {
    let container_name = match &name {
        Some(explicit) => explicit.clone(),
        None => default_container_name(&image),
    };

    // An explicit `--name ""` used to be accepted: the container directory then
    // collapsed onto the containers root itself, creating an orphan that `list`
    // showed with a blank name and `remove` could never match. Derived names are
    // validated too, so no install can create an unusable path.
    proot_distro_core::NameValidator::new().validate_container_name(&container_name)?;

    let _rootfs: PathBuf = install_image_native(&image, &container_name, force, None).await?;
    Ok(())
}

/// Derive a container name from an image reference.
///
/// `debian:13` and `ghcr.io:5000/acme/web:1.2` both yield a single path component
/// (`debian`, `web`): a registry host, port, or namespace must never become a
/// directory component under `~/.podnext/containers/`.
pub fn default_container_name(image: &str) -> String {
    // The rule lives in proot-distro-core so build, run and install agree on it;
    // a divergence here is exactly how `build local` ended up naming a directory
    // `c2env:latest` and losing /dev/shm (#35).
    proot_distro_core::names::store_key(image)
}

/// Ask the running daemon for a read-only metadata query (roadmap section 9.2).
///
/// One connect, one request, one response: the round-trip count is a property of
/// the probe, not of the container count. `None` means the daemon is not running
/// (or cannot answer) and the caller reads the store in-process instead. Nothing is
/// printed on that path, so a stopped daemon changes nothing the CLI emits.
async fn daemon_query<T: serde::de::DeserializeOwned>(cmd: &str) -> Option<T> {
    let resp = super::service::send_daemon_command(serde_json::json!({ "cmd": cmd }))
        .await
        .ok()?;
    if resp.get("type").and_then(|t| t.as_str()) != Some("ok") {
        return None;
    }
    serde_json::from_value(resp.get("data")?.clone()).ok()
}

pub async fn cmd_list(_all: bool, no_all: bool, format: OutputFormat) -> Result<()> {
    let show_all = !no_all;
    // The metadata query — the daemon answers it when it runs, otherwise the store
    // is read in-process. Timed here, not in the library, so the benchmark harness
    // can read an in-process number instead of one inflated by process start.
    let t0 = std::time::Instant::now();
    let via_daemon = daemon_query::<Vec<proot_distro_core::ContainerInfo>>("containers").await;
    let round_trips = u32::from(via_daemon.is_some());
    let mut containers = match via_daemon {
        Some(all) => all,
        None => list_containers_native(true).await?,
    };
    if !show_all {
        containers.retain(|c| c.status == proot_distro_core::ContainerStatus::Running);
    }
    // `ContainerManager` holds containers in a `HashMap`, so the scan order is not
    // stable across processes. Sorting by name is what makes the daemon path and
    // the in-process path byte-identical (the acceptance criterion) and makes `list`
    // deterministic run to run.
    containers.sort_by(|a, b| a.name.cmp(&b.name));
    if std::env::var_os("PODNEXT_BENCH").is_some() {
        eprintln!(
            "PODNEXT_BENCH_MS={:.4}",
            t0.elapsed().as_secs_f64() * 1000.0
        );
        // Round trips for this query: 1 when the daemon answered, 0 on the
        // in-process path. Constant in the container count either way.
        eprintln!("PODNEXT_BENCH_RT={}", round_trips);
    }

    #[derive(Tabled, Serialize)]
    struct ContainerRow {
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "DISTRO")]
        distro: String,
        #[tabled(rename = "VERSION")]
        version: String,
        #[tabled(rename = "ARCH")]
        arch: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "CREATED")]
        created_at: String,
    }

    let rows: Vec<ContainerRow> = containers
        .iter()
        .map(|c| ContainerRow {
            name: c.name.clone(),
            distro: c.distro.clone(),
            version: c.version.clone(),
            arch: c.arch.clone(),
            status: format!("{:?}", c.status),
            created_at: c.created_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&containers)?,
        OutputFormat::Yaml => output_yaml(&containers)?,
        OutputFormat::Text => {
            for c in containers {
                println!(
                    "{} ({}:{}) [{}]",
                    c.name,
                    c.distro,
                    c.version,
                    format!("{:?}", c.status)
                );
            }
        }
    }
    Ok(())
}

/// Printed once by `run`/`login`, on **stderr** so the machine-readable stdout stream is
/// untouched, before the guest takes the terminal (roadmap section 9.3: the build/run output
/// must state which user the guest runs as; `build local` already does).
///
/// There is exactly one answer: proot-core's runtime passes `--change-id=0:0:0`
/// (`crates/proot-core/src/lib.rs`), so every guest id is the single host uid and the guest is
/// root. A request for any other user changes nothing, which is why `run`/`login` refuse `--user`.
pub(crate) const MAPPED_USER_NOTE: &str =
    "NOTE: the guest runs as root (uid 0): PRoot maps every guest id to the single host uid via \
     `--change-id=0:0:0`, so there is no privilege separation";

pub async fn cmd_login(
    container: String,
    user: Option<String>,
    isolated: bool,
    command: Vec<String>,
    binds: Vec<String>,
) -> Result<()> {
    // `--user` was parsed and then dropped: the value reached `LoginOptions.user`, nothing read it,
    // and the runtime hard-codes `--change-id=0:0:0` (proot-core), so `podnext login alpine
    // --user 1000 -- id` printed `uid=0(root)` — byte-identical to the same login without the flag
    // (measured inside the guest, PDN#66). `run` refuses the same flag; a flag the caller supplied
    // that changes nothing, with no signal from the exit status, is the class this project refuses
    // (#62/#63). Refuse rather than silently discard.
    if let Some(u) = user {
        anyhow::bail!(
            "--user {u} is not implemented for `login`: the guest would run as the mapped root. \
             PRoot maps every guest id to the single host uid via `--change-id=0:0:0`, so the \
             requested user changes nothing (measured: `podnext login <container> --user 1000 \
             -- id` prints `uid=0(root)`). `run` refuses the same flag for the same reason. Drop \
             `--user` so the login does what it says"
        );
    }

    let cmd = if command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        command
    };

    // A bad spec must stop the login, not be dropped: it used to be accepted and ignored.
    let mounts = parse_mount_specs(&binds)?;

    eprintln!("{MAPPED_USER_NOTE}");
    login_container_native(&container, isolated, cmd, mounts).await
}

pub async fn cmd_run(
    container: String,
    command: Vec<String>,
    _rm: bool,
    detach: bool,
    _name: Option<String>,
    _network: Option<String>,
    volume: Vec<String>,
    _publish: Vec<String>,
    _entrypoint: Option<String>,
    _tty: bool,
    _interactive: bool,
    _user: Option<String>,
    _workdir: Option<String>,
    isolated: bool,
    binds: Vec<String>,
) -> Result<()> {
    // Flags that were parsed and then dropped. Each one changes what the command DOES, so
    // accepting it silently means the caller gets a run that is not the one they asked for
    // and no way to tell from the exit status. They are named together so one run reports
    // every reason instead of one per attempt.
    let mut ignored: Vec<&str> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    if _rm {
        ignored.push("--rm");
    }
    if _name.is_some() {
        ignored.push("--name");
    }
    if _network.is_some() {
        ignored.push("--network");
    }
    if !_publish.is_empty() {
        ignored.push("--publish");
    }
    if _entrypoint.is_some() {
        ignored.push("--entrypoint");
    }
    if _user.is_some() {
        ignored.push("--user");
    }
    if _workdir.is_some() {
        ignored.push("--workdir");
    }
    if !ignored.is_empty() {
        reasons.push(format!(
            "{} not implemented for `run`; the guest would run without it. \
             --workdir and --user are part of the login polygon (v0.2.0), --rm needs ephemeral \
             containers, --name/--network/--publish need the network and naming work that \
             `podnext create` will own. Use `podnext login` for an interactive shell, or drop \
             the flag so the run does what it says",
            ignored.join(", ")
        ));
    }
    if !reasons.is_empty() {
        anyhow::bail!("{}", reasons.join("; "));
    }

    // `--volume` (run) and `--bind` (global) are the same thing; both were parsed and
    // dropped, so the guest never saw them. A malformed spec is a hard error.
    let mut specs = binds;
    specs.extend(volume);
    let mounts = parse_mount_specs(&specs)?;

    // State the user the guest runs as (roadmap 9.3). Build/run output is otherwise silent about
    // it; the flag checks above have already refused any request for a different user.
    eprintln!("{MAPPED_USER_NOTE}");

    // `--detach` starts the guest in the background under a supervised PTY and returns; the
    // session is then reachable with `podnext attach <container>` (PDN#57). The mount specs and
    // the refused-flag checks above run first, so a detached run is the run that was asked for.
    if detach {
        return crate::native::spawn_detached_session(&container, command, isolated, mounts).await;
    }

    run_container_native(&container, command, isolated, mounts).await
}

pub async fn cmd_ps(format: OutputFormat) -> Result<()> {
    let mut sessions = match daemon_query::<Vec<proot_distro_core::Session>>("sessions").await {
        Some(s) => s,
        None => list_sessions_native_full().await?,
    };
    // Same `HashMap`-order caveat as `list`: sorted so both paths (and repeated
    // runs) render the same bytes.
    sessions.sort_by(|a, b| a.id.cmp(&b.id));
    let active_disk_sessions = proot_distro_core::session::active_sessions().unwrap_or_default();

    #[derive(Tabled, Serialize)]
    struct SessionRow {
        #[tabled(rename = "ID")]
        id: String,
        #[tabled(rename = "CONTAINER")]
        container: String,
        #[tabled(rename = "USER")]
        user: String,
        #[tabled(rename = "COMMAND")]
        command: String,
        #[tabled(rename = "CWD")]
        cwd: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "CREATED")]
        created: String,
        #[tabled(rename = "STARTED")]
        started: String,
    }

    let mut rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            id: s.id.clone(),
            container: s.container_name.clone(),
            user: s.user.clone(),
            command: s.command.join(" "),
            cwd: s.cwd.clone(),
            pid: s.pid.map_or("-".to_string(), |p| p.to_string()),
            status: format!("{:?}", s.status),
            created: s.created_at.format("%Y-%m-%d %H:%M").to_string(),
            started: s.started_at.map_or("-".to_string(), |dt| {
                dt.format("%Y-%m-%d %H:%M").to_string()
            }),
        })
        .collect();

    for act in active_disk_sessions {
        if !rows.iter().any(|r| r.pid == act.pid.to_string()) {
            rows.push(SessionRow {
                id: format!("sess-{}", act.pid),
                container: act.container_name,
                user: act.login_uid,
                command: act.cmdline.join(" "),
                cwd: "-".to_string(),
                pid: act.pid.to_string(),
                status: "Running".to_string(),
                created: "-".to_string(),
                started: "-".to_string(),
            });
        }
    }

    // The disk rows are appended after the store rows and `active_sessions()` reads a
    // directory, so its order is not guaranteed either: one more sort keeps the table
    // and text output deterministic between the two paths.
    rows.sort_by(|a, b| a.id.cmp(&b.id));

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in sessions {
                println!(
                    "{} {} {} {} {} {}",
                    s.id,
                    s.container_name,
                    s.user,
                    s.pid.map_or("-".to_string(), |p| p.to_string()),
                    format!("{:?}", s.status),
                    s.command.join(" ")
                );
            }
        }
    }
    Ok(())
}

pub async fn cmd_sessions(format: OutputFormat) -> Result<()> {
    let sessions = list_sessions_native().await?;

    #[derive(Tabled, Serialize)]
    struct SessionRow {
        #[tabled(rename = "CONTAINER")]
        container: String,
        #[tabled(rename = "USER")]
        user: String,
        #[tabled(rename = "PID")]
        pid: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "STARTED")]
        started: String,
    }

    let rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            container: s.container_name.clone(),
            user: s.user.clone(),
            pid: s.pid.map_or("-".to_string(), |p| p.to_string()),
            status: s.status.clone(),
            started: s.started_at.format("%Y-%m-%d %H:%M").to_string(),
        })
        .collect();

    match format {
        OutputFormat::Table => output_table(&rows),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in sessions {
                println!(
                    "{} {} {} {}",
                    s.container_name,
                    s.user,
                    s.pid.map_or("-".to_string(), |p| p.to_string()),
                    s.status
                );
            }
        }
    }
    eprintln!("\nNote: Use 'podnext attach <container>' to create a new session.");
    Ok(())
}

/// `podnext attach <container>`: reconnect to a running detached session (PDN#57).
///
/// The same argv is how `run --detach` re-invokes this binary to run the
/// supervisor; the environment marker switches between the two sides of the
/// round trip, so no new subcommand is needed.
pub async fn cmd_attach(
    container: String,
    user: Option<String>,
    command: Vec<String>,
) -> Result<()> {
    if crate::native::is_session_host() {
        return crate::native::run_session_host(&container).await;
    }
    crate::native::attach_session(&container, user, command).await
}

pub async fn cmd_docker(container_name: Option<String>, args: Vec<String>) -> Result<()> {
    let container_mgr = proot_distro_core::ContainerManager::new()?;
    let backend: Box<dyn proot_distro_core::RuntimeBackend> = if let Some(ref name) = container_name
    {
        let c = container_mgr.get(name).await?;
        container_mgr.get_backend(&c)
    } else {
        Box::new(proot_distro_core::UmlBackend::new())
    };

    if !backend.supports_kernel_features() {
        anyhow::bail!(
            "The configured runtime backend '{}' does not support kernel features required by Docker.\n\
             Reason: Docker requires namespaces (mount, pid, net) and cgroups v2, which PRoot cannot provide.\n\
             Resolution: Start the UML backend with 'uml4termux up' and use '--runtime uml'.",
            format!("{:?}", backend.kind()).to_lowercase()
        );
    }

    let status = backend.status().await?;
    if !status.running {
        anyhow::bail!(
            "UML kernel runtime is not running (status: {}). Please start it with 'uml4termux up'.",
            status.details
        );
    }

    let mut docker_cmd = vec!["docker".to_string()];
    docker_cmd.extend(args);

    let dummy_container = proot_distro_core::Container::default();
    let exit_code = backend.execute(&dummy_container, docker_cmd).await?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

pub async fn cmd_kill(container: String) -> Result<()> {
    let mut killed_any = false;
    if let Ok(active) = proot_distro_core::session::active_sessions() {
        for s in active {
            if s.container_name == container || container == "all" || container == "--all" {
                if let Ok(()) = proot_distro_core::session::kill_session(s.pid, libc::SIGQUIT) {
                    println!(
                        "Sent SIGQUIT to session PID {} ({})",
                        s.pid, s.container_name
                    );
                    killed_any = true;
                }
            }
        }
        if killed_any {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
    kill_container_native(&container).await?;
    if container == "all" || container == "--all" {
        if killed_any {
            println!("Killed all active container sessions.");
        } else {
            println!("No active container sessions found.");
        }
    } else if killed_any {
        println!("Container '{}' killed.", container);
    } else {
        println!("Container '{}' is not running.", container);
    }
    Ok(())
}

pub async fn cmd_remove(name: String, force: bool) -> Result<()> {
    remove_container_native(name, force).await
}

pub async fn cmd_info(format: OutputFormat) -> Result<()> {
    // The daemon answers this while it runs; the literals are the in-process fallback
    // and must stay identical to `podnext-daemon`'s `info` handler.
    let payload = daemon_query::<SerializableInfo>("info")
        .await
        .unwrap_or_else(|| SerializableInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: "Termux/Android".to_string(),
            rust_version: "1.80+".to_string(),
            backend: "proot-distro-core".to_string(),
            isolation: "proot (unmapped host tree, masked /proc)".to_string(),
        });

    let info = SystemInfo {
        version: payload.version,
        platform: payload.platform,
        rust_version: payload.rust_version,
        backend: payload.backend,
        isolation: payload.isolation,
    };

    #[derive(Tabled, Serialize)]
    struct InfoRow {
        #[tabled(rename = "VERSION")]
        version: String,
        #[tabled(rename = "PLATFORM")]
        platform: String,
        #[tabled(rename = "RUST VERSION")]
        rust_version: String,
        #[tabled(rename = "BACKEND")]
        backend: String,
        #[tabled(rename = "ISOLATION")]
        isolation: String,
    }

    let row = InfoRow {
        version: info.version.clone(),
        platform: info.platform.clone(),
        rust_version: info.rust_version.clone(),
        backend: info.backend.clone(),
        isolation: info.isolation.clone(),
    };

    let serializable = SerializableInfo {
        version: info.version.clone(),
        platform: info.platform.clone(),
        rust_version: info.rust_version.clone(),
        backend: info.backend.clone(),
        isolation: info.isolation.clone(),
    };

    match format {
        OutputFormat::Table => output_table(&[row]),
        OutputFormat::Json => output_json(&serializable)?,
        OutputFormat::Yaml => output_yaml(&serializable)?,
        OutputFormat::Text => {
            println!(
                "podnext {} - PRoot Distro Next for Termux/Android",
                env!("CARGO_PKG_VERSION")
            );
            println!("Backend: {}", info.backend);
            println!("Isolation: {}", info.isolation);
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct SerializableInfo {
    version: String,
    platform: String,
    rust_version: String,
    backend: String,
    isolation: String,
}
