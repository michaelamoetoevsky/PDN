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
    install_archive_native, install_image_native, kill_container_native, list_containers_native,
    list_sessions_native, list_sessions_native_full, parse_mount_specs, remove_container_native,
    run_container_native, search_docker_hub, verify_image_available,
};
use crate::utils::format_size;
use crate::{output_json, output_table, output_yaml, OutputFormat, SystemInfo};

pub async fn cmd_install(
    image: String,
    name: Option<String>,
    force: bool,
    checksum: Option<String>,
) -> Result<()> {
    let container_name = resolve_install_name(&image, name.as_deref())?;

    // An explicit `--name ""` used to be accepted: the container directory then
    // collapsed onto the containers root itself, creating an orphan that `list`
    // showed with a blank name and `remove` could never match. Derived names are
    // validated too, so no install can create an unusable path.
    proot_distro_core::NameValidator::new().validate_container_name(&container_name)?;

    let checksum = checksum.as_deref().map(parse_checksum).transpose()?;

    if proot_distro_core::names::is_local_source(&image)
        || proot_distro_core::names::is_url_source(&image)
    {
        install_archive_native(&image, &container_name, force, checksum.as_deref()).await?;
    } else {
        // A checksum pins an archive's bytes; a registry reference is already
        // content-addressed by its manifest, so the option does not apply here.
        if checksum.is_some() {
            bail!(
                "--checksum applies to a local archive or a URL, not to the registry \
                 reference '{image}'"
            );
        }
        install_image_native(&image, &container_name, force, None).await?;
    }
    Ok(())
}

/// Decide the container name from the source, following the reference's rules.
///
/// An explicit `--name` always wins. Otherwise a local path or URL derives its
/// name from the filename (`_derive_local_name`), and a registry reference from
/// its alias (`derive_alias`). Every refusal names the reason: the source that
/// could not be read, or the basename a valid name could not be derived from.
fn resolve_install_name(source: &str, custom: Option<&str>) -> Result<String> {
    if proot_distro_core::names::is_local_source(source) {
        let path = expand_home(source);
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_file() => {}
            _ => bail!(
                "cannot install '{source}': local file '{}' does not exist or is not a \
                 regular file",
                path.display()
            ),
        }
        if let Some(name) = custom {
            return Ok(name.to_string());
        }
        let derived = proot_distro_core::names::derive_local_name(&path.to_string_lossy());
        if derived.is_empty() {
            bail!(
                "cannot determine a valid container name from '{}'. Specify the name \
                 with '--name NAME'",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| source.to_string())
            );
        }
        return Ok(derived);
    }

    if proot_distro_core::names::is_url_source(source) {
        if let Some(name) = custom {
            return Ok(name.to_string());
        }
        // The query string and fragment are not part of the filename.
        let url_path = source.split(['?', '#']).next().unwrap_or(source);
        let derived = proot_distro_core::names::derive_local_name(url_path);
        if derived.is_empty() {
            bail!(
                "cannot determine a valid container name from '{source}'. Specify \
                 the name with '--name NAME'"
            );
        }
        return Ok(derived);
    }

    Ok(match custom {
        Some(name) => name.to_string(),
        None => default_container_name(source),
    })
}

/// Expand a leading `~`/`~/` against `$HOME` (the reference's `os.path.expanduser`).
fn expand_home(source: &str) -> PathBuf {
    if let Some(rest) = source.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    } else if source == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(source)
}

/// Normalise a `--checksum` value to bare lowercase hex, or refuse it by name.
fn parse_checksum(value: &str) -> Result<String> {
    let hex = value.strip_prefix("sha256:").unwrap_or(value);
    let well_formed = hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if well_formed {
        Ok(hex.to_string())
    } else {
        bail!("invalid --checksum '{value}': expected sha256:<64 lowercase hex>")
    }
}

/// Derive a container name from an image reference.
///
/// `debian:13` and `ghcr.io:5000/acme/web:1.2` both yield a single path component
/// (`debian`, `web`): a registry host, port, or namespace must never become a
/// directory component under `~/.podnext/containers/`.
pub fn default_container_name(image: &str) -> String {
    // The reference's `derive_alias`: the last path component with its tag - and
    // any `@digest` pin - dropped. For every tagged reference it is the same
    // string `store_key` produces for `build` and `run`, which is what keeps
    // `build local`, `run` and `install` naming one directory the same way (#35).
    proot_distro_core::names::derive_alias(image)
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

/// The proot-distro `login` flags PDN does NOT implement, each with the reason and the way out.
///
/// Added 2026-11-09 after real use reported `podnext login <ctr> --shared-home` answering clap's
/// `error: unexpected argument '--shared-home' found`, which reads as a typo and tells the caller
/// nothing. The flags are accepted by the parser (see the hidden fields on `Commands::Login`) and
/// refused here BY NAME, so the message names the flag, says why, and gives the command that works.
/// Every remedy below is a command that was actually run on this device.
const REFUSED_LOGIN_FLAGS: &[(&str, &str)] = &[
    (
        "--fix-low-ports",
        "the reference maps this to `-P/--redirect-ports`, which rewrites the guest's listening \
         ports through a port_switch shim; PDN has no such shim yet, so the guest would keep the \
         host's port numbers unchanged. `--engine proot` is where the reference flag belongs",
    ),
    (
        "--no-link2symlink",
        "hardlink emulation cannot be switched off; only `-0/--link2symlink` is accepted",
    ),
    (
        "--change-id",
        "the runtime already maps every guest id to one host uid, so there is no per-login mapping \
         to set; `--engine proot` is where that flag belongs",
    ),
];

pub async fn cmd_login(
    container: String,
    user: Option<String>,
    isolated: bool,
    command: Vec<String>,
    binds: Vec<String>,
    engine: crate::EngineChoice,
    shared_home: bool,
    shared_tmp: bool,
    shared_x11: bool,
    detach: bool,
    refused_flags: &[&str],
) -> Result<()> {
    // Refuse the proot-distro login polygon by name, with the remedy, BEFORE any work happens: a flag
    // the caller supplied that changes nothing is the class this project refuses (#62/#63), and an
    // operator who typed a real proot-distro flag deserves a sentence, not a parse error.
    if let Some(flag) = refused_flags.first() {
        let reason = REFUSED_LOGIN_FLAGS
            .iter()
            .find(|(name, _)| name == flag)
            .map(|(_, why)| *why)
            .unwrap_or("this flag is not implemented for `login`");
        anyhow::bail!(
            "{flag} is not implemented for `login`: {reason}. Drop it, or run \
             `podnext login --help` for the flags that are supported"
        );
    }

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

    // The reference's OWN binds, read from the installed proot-distro package
    // (commands/login/proot_cmd.py:205-216), which names the paths rather than inventing them:
    //   --shared-home / --termux-home -> --bind=<host home>:/root   (the reference's login_uid == "0"
    //   branch; PDN's guest is always root through --change-id=0:0:0)
    //   --shared-tmp                 -> --bind=<prefix>/tmp:/tmp
    //   --shared-x11                 -> --bind=<prefix>/tmp/.X11-unix:/tmp/.X11-unix
    let mut bind_specs = binds;
    if shared_home {
        // `paths::host::home()` answers `Option<PathBuf>` (the compiler caught my first version
        // assuming `PathBuf`); a missing home is refused by name rather than bound as an empty path.
        let home = proot_distro_core::paths::host::home().ok_or_else(|| {
            anyhow::anyhow!(
                "--shared-home was asked for, but the host home directory could not be resolved \
                 (HOME and TERMUX__HOME are both unset)"
            )
        })?;
        bind_specs.push(format!("{}:/root", home.display()));
    }
    if shared_tmp {
        bind_specs.push(format!(
            "{}/tmp:/tmp",
            proot_distro_core::paths::host::prefix().display()
        ));
    }
    if shared_x11 {
        let x11 = proot_distro_core::paths::host::prefix().join("tmp/.X11-unix");
        if !x11.is_dir() {
            // Refuse by name rather than hand proot a bind source that does not exist: the reference
            // binds the same path, and proot fails on a missing source, so the honest place to say so
            // is here, with the path.
            anyhow::bail!(
                "--shared-x11 was asked for, but there is no X11 socket directory at {}. Start an X \
                 server in Termux first (the reference binds this same path).",
                x11.display()
            );
        }
        bind_specs.push(format!("{}:/tmp/.X11-unix", x11.display()));
    }

    // A bad spec must stop the login, not be dropped: it used to be accepted and ignored.
    let mounts = parse_mount_specs(&bind_specs)?;

    eprintln!("{MAPPED_USER_NOTE}");
    // The same applied path `run` takes, so the image's own `Env`/`WorkingDir`
    // reach the guest (WORKLIST B5, issue #76). `login` used to call
    // `login_container_native`, which reloads the container through the store
    // loader that leaves `env_vars`/`working_dir` at their defaults: `podnext run`
    // on an image built with `ENV GREETING=hi` saw `GREETING=hi` and `podnext
    // login` on the same image saw nothing, so the image did not behave as built.
    // `-d/--detach` exists on the reference's login (parser.py:158) and starts the session in the
    // background, reachable with `podnext attach <container>`; the engine decision is shared with
    // `run --detach` so the two say the same thing.
    if detach {
        let engine = engine_for_detached_session(engine)?;
        return crate::native::spawn_detached_session(&container, cmd, isolated, mounts).await;
    }

    // One applied path instead of two that disagree.
    run_container_native(&container, cmd, isolated, mounts, engine).await
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
    engine: crate::EngineChoice,
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
        // The detached supervisor runs the guest through the proot path
        // (`login_container_native` -> `ProotDistroCore::login`); the native engine has no
        // detached-session form yet.
        //
        // 2026-11-09: the engine default flipped to `native`, which made `podnext run --detach
        // <container>` fail - it exited 1 with the refusal below for every caller who had not named
        // an engine, and the documented example in docs/src/cli-reference.md is exactly that command
        // line (the doc-example checker lists `run` under CANNOT_RUN, so CI could not see it). The
        // detached path therefore selects proot FOR the caller and says so in one line, while an
        // EXPLICIT `--engine native` is still refused: an operator who named the engine gets the
        // truth instead of silently being given a different one.
        let engine = engine_for_detached_session(engine)?;
        return crate::native::spawn_detached_session(&container, command, isolated, mounts).await;
    }

    /// Whether the caller named `--engine` on the command line.
    run_container_native(&container, command, isolated, mounts, engine).await
}

pub async fn cmd_ps(format: OutputFormat) -> Result<()> {
    // One reader for all three renderings (A2). The disk registry is the only
    // session list that crosses process boundaries, so `ps`, `ps -f json` and
    // `sessions` cannot disagree about the same instant. The daemon used to
    // answer this query from its own view, which is empty while a detached
    // guest runs: that is how `ps` listed a session that `ps -f json` reported
    // as `[]`.
    let sessions = list_sessions_native_full().await?;

    match format {
        OutputFormat::Table => output_table(&sessions),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in &sessions {
                println!(
                    "{} {} {} {} {} {}",
                    s.id, s.container, s.user, s.pid, s.status, s.command
                );
            }
        }
    }
    Ok(())
}

pub async fn cmd_sessions(format: OutputFormat) -> Result<()> {
    // The same reader `ps` renders, so the table and the JSON are the same
    // answer to the same question (A2). Both used to read the in-process
    // `SessionManager`, which is empty in a fresh process, so `sessions`
    // printed a header with no rows while a guest was running.
    let sessions = list_sessions_native().await?;

    match format {
        OutputFormat::Table => output_table(&sessions),
        OutputFormat::Json => output_json(&sessions)?,
        OutputFormat::Yaml => output_yaml(&sessions)?,
        OutputFormat::Text => {
            for s in &sessions {
                println!("{} {} {} {}", s.container, s.user, s.pid, s.status);
            }
        }
    }
    // `attach` never creates a session: it reconnects to one started with `run --detach`, and
    // with none running for the container it exits 1 and names the command that starts one. The
    // old note told the caller the opposite.
    eprintln!(
        "\nNote: 'podnext attach <container>' reconnects to a session started with \
         'podnext run --detach <container>'; it does not create one."
    );
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
             Measured here by 'podnext doctor' (S5): {}\n\
             Gated at dockerd's daemon/daemon.go:1130-1132 (the Devices-cgroup check), which refuses to start without them.\n\
             Resolution: Start the UML backend with 'uml4termux up' and use '--runtime uml'.",
            format!("{:?}", backend.kind()).to_lowercase(),
            crate::doctor::capability_line()
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

/// How long each rung of the kill ladder gets before the next one is tried.
const KILL_LADDER_GRACE: std::time::Duration = std::time::Duration::from_millis(750);

/// The escalation order: `SIGTERM`, then `SIGQUIT` if the guest ignored it, then
/// `SIGKILL` if it ignored that too (the reference's ladder,
/// `commands/proot-distro/kill.py:234-236`). `SIGKILL` cannot be ignored, so a
/// session that survives the first two rungs is gone after the third or not at
/// all - which is why its rung is what reports the failure.
const KILL_LADDER: [i32; 3] = [libc::SIGTERM, libc::SIGQUIT, libc::SIGKILL];

/// Resolve a signal `--signal`/`-s` named: a name (`STOP`, `TERM`, with or
/// without the `SIG` prefix) or a number.
///
/// The table is short on purpose. An unknown name is refused by name rather than
/// guessed at: the number that lands is not something the caller can check from
/// an exit status afterwards.
fn parse_signal(spec: &str) -> Result<i32> {
    let upper = spec.trim().to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    Ok(match name {
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "QUIT" => libc::SIGQUIT,
        "KILL" => libc::SIGKILL,
        "TERM" => libc::SIGTERM,
        "CONT" => libc::SIGCONT,
        "STOP" => libc::SIGSTOP,
        "USR1" => libc::SIGUSR1,
        "USR2" => libc::SIGUSR2,
        other => match other.parse::<i32>() {
            Ok(n) if (1..=64).contains(&n) => n,
            _ => bail!(
                "unknown signal '{spec}': expected a name (HUP, INT, QUIT, KILL, TERM, CONT, STOP, \
                 USR1, USR2) or a number from 1 to 64"
            ),
        },
    })
}

/// The name a signal is reported under. A bare number arrives only when the
/// caller asked for one, so it is reported as a number rather than invented.
fn signal_label(sig: i32) -> String {
    match sig {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGKILL => "SIGKILL",
        libc::SIGTERM => "SIGTERM",
        libc::SIGCONT => "SIGCONT",
        libc::SIGSTOP => "SIGSTOP",
        libc::SIGUSR1 => "SIGUSR1",
        libc::SIGUSR2 => "SIGUSR2",
        _ => return format!("signal {sig}"),
    }
    .to_string()
}

/// Wait for a session process to actually go, without touching its files.
///
/// The ladder decides the next rung off this, and the store sweep that follows
/// still has to see the session's record, so nothing may be cleared here; the
/// teardown of A9 happens once the whole kill is done.
fn wait_until_gone(pid: u32, start_time: u64, grace: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + grace;
    while proot_distro_core::session::process_is_alive(pid, start_time) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    true
}

pub async fn cmd_kill(container: String, signal: Option<String>) -> Result<()> {
    // A signal named on the command line is the whole instruction: it is
    // delivered once and never escalated (WORKLIST B8). `kill -s STOP` must
    // stop the guest and stop there - a ladder that then sent TERM/QUIT/KILL
    // would kill what the caller asked to pause, and nothing in the exit status
    // would say so.
    let requested = match signal {
        Some(ref spec) => Some(parse_signal(spec)?),
        None => None,
    };
    let all = container == "all" || container == "--all";

    let mut killed: Vec<proot_distro_core::session::SessionRecord> = Vec::new();
    if let Ok(active) = proot_distro_core::session::active_sessions() {
        for s in active {
            if s.container_name != container && !all {
                continue;
            }
            // C8: the registry proved this session live from its inherited
            // descriptor, so the record's pid is only an address. If the kernel
            // has since handed that pid to another process, signalling it would
            // hit a stranger - refuse and say so instead.
            if proot_distro_core::session::signal_target(&s).is_none() {
                eprintln!(
                    "Skipping session PID {} ({}): its pid no longer names the process that \
                     registered it, so nothing can be signalled safely there.",
                    s.pid, s.container_name
                );
                continue;
            }
            match requested {
                Some(sig) => {
                    if proot_distro_core::session::kill_session(s.pid, sig).is_ok() {
                        println!(
                            "Sent {} to session PID {} ({})",
                            signal_label(sig),
                            s.pid,
                            s.container_name
                        );
                        killed.push(s);
                    }
                }
                // No signal named: escalate, but only past a rung the guest
                // ignored. `wait_until_gone` asks the kernel, not a stopwatch,
                // so a rung is skipped the moment the process is really gone.
                None => {
                    for sig in KILL_LADDER {
                        if proot_distro_core::session::kill_session(s.pid, sig).is_err() {
                            break;
                        }
                        println!(
                            "Sent {} to session PID {} ({})",
                            signal_label(sig),
                            s.pid,
                            s.container_name
                        );
                        if wait_until_gone(s.pid, s.start_time, KILL_LADDER_GRACE) {
                            break;
                        }
                    }
                    killed.push(s);
                }
            }
        }
    }
    // `ContainerManager::kill` also sweeps the matching sessions with a SIGQUIT
    // of its own, and it reads the registry the same way this command does. Once
    // a session has been signalled it has nothing left to add - and for a named
    // signal its sweep would be exactly the escalation that must not happen - so
    // it runs only when no session matched. That is also where its "Container
    // '<name>' not found" belongs: the domain error for an address that names
    // nothing at all (automation_contract asserts exit 1 for it), which a
    // matching session can never be.
    if killed.is_empty() {
        kill_container_native(&container).await?;
    }
    // The session is over, so its files go with it (A9) - for the ladder, whose
    // whole job is that "killed" means the session is gone. A supervisor killed
    // by a signal it cannot act on - SIG_IGN inherited from the parent, which
    // the cargo test harness does set for SIGQUIT - cannot unlink its own socket
    // or transcript, and neither can one killed by SIGKILL, so the killer waits
    // for the process to go and then removes them. A named signal gets no
    // teardown: `kill -s STOP` names a pause, not a removal.
    if requested.is_none() {
        for s in &killed {
            if proot_distro_core::session::wait_and_clear(s.pid, s.start_time, KILL_LADDER_GRACE) {
                continue;
            }
            eprintln!(
                "Warning: session PID {} ({}) is still running after {}; its files in {} were \
                 left in place.",
                s.pid,
                s.container_name,
                KILL_LADDER
                    .iter()
                    .map(|s| signal_label(*s))
                    .collect::<Vec<_>>()
                    .join(", "),
                proot_distro_core::session::sessions_dir().display()
            );
        }
    }
    if all {
        if killed.is_empty() {
            println!("No active container sessions found.");
        } else if let Some(sig) = requested {
            println!(
                "Sent {} to all active container sessions.",
                signal_label(sig)
            );
        } else {
            println!("Killed all active container sessions.");
        }
    } else if killed.is_empty() {
        println!("Container '{}' is not running.", container);
    } else if let Some(sig) = requested {
        println!("Sent {} to container '{}'.", signal_label(sig), container);
    } else {
        println!("Container '{}' killed.", container);
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

/// Whether the caller named `--engine` on the command line.
///
/// clap's `default_value` makes the default indistinguishable from an explicit choice in the
/// parsed struct, and the detached path must tell "the default sent me here" (choose proot and
/// say so) from "the operator asked for this engine" (refuse by name). Scanning argv is the
/// cheap, local way to ask; `--engine` is a GLOBAL argument, so it always appears verbatim.
fn engine_was_requested_on_the_command_line() -> bool {
    // `args_os`, NEVER the lossy panicking reader: issue #41 fixed a panic on a non-UTF-8
    // argument, and the ledger check "the panicking reader is gone from the tree" greps this crate
    // for that reader's call syntax - comments included, which is why this sentence does not spell
    // it out. It caught this very helper on the first gate run after it was written.
    std::env::args_os().any(|a| {
        let a = a.to_string_lossy();
        a == "--engine" || a.starts_with("--engine=")
    })
}

/// Choose the engine for a DETACHED session and say which one it is.
///
/// `run --detach` and `login --detach` (the reference has `-d, --detach` on login too,
/// `parser.py:158`) both need the same decision, so it lives in one place. The detached supervisor
/// runs the guest through the proot path; an engine the caller did not name is chosen FOR them and
/// printed, while an explicit `--engine native` is refused by name.
fn engine_for_detached_session(engine: crate::EngineChoice) -> Result<crate::EngineChoice> {
    if engine != crate::EngineChoice::Native {
        return Ok(engine);
    }
    if engine_was_requested_on_the_command_line() {
        anyhow::bail!(
            "--engine native is not implemented for `--detach`: the detached session \
             supervisor runs the guest through the proot path. Drop `--detach` for a foreground \
             session on the native engine, or use `--engine proot`."
        );
    }
    eprintln!(
        "NOTE: --detach runs the guest through the proot path: the detached session supervisor \
         is not ported to the native engine yet. Pass `--engine proot` to make that explicit, or \
         drop `--detach` for a foreground session on the native engine."
    );
    Ok(crate::EngineChoice::Proot)
}
