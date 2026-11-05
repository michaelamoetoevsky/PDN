//! podnext CLI - Advanced container and service management for Termux/Android
//!
//! A comprehensive CLI that works with proot-distro-core, pn-next, and pn-gateway
//! to provide full container lifecycle management, service orchestration, and monitoring.

#![allow(deprecated)]
#![allow(future_incompatible)]

/// Trace-only logging, off unless `PN_DEBUG` is set in the environment.
///
/// These lines were unconditional upstream and flooded real output — a single
/// `backup create` emitted ~280 kB of DEBUG for one alpine container, burying
/// the result. Enable with `PN_DEBUG=1` when diagnosing.
macro_rules! pn_trace {
    ($($arg:tt)*) => {
        if ::std::env::var_os("PN_DEBUG").is_some() {
            ::std::eprintln!($($arg)*);
        }
    };
}
use anyhow::{bail, Result};
use clap::{CommandFactory, Parser};
use serde_yaml;
use std::io::{self, Write};
use std::path::PathBuf;
use tabled::{Table, Tabled};
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod doctor;
mod native;
mod utils;

pub(crate) use cli::*;

use native::{
    install_image_native, kill_container_native, list_containers_native, list_sessions_native,
    list_sessions_native_full, login_container_native, parse_mount_specs, remove_container_native,
    run_container_native, search_docker_hub, verify_image_available,
};
use utils::format_size;

use commands::container::*;
use commands::image::*;
use commands::network::*;
use commands::sync::*;
use commands::volume::*;
use commands::*;
use proot_distro_core::network::NetworkConfig;
use proot_distro_core::storage::VolumeConfig;

#[tokio::main]
async fn main() {
    // Exits with the child's code when a command inside a container failed, so
    // `podnext login c -- sh -c 'exit 7'` reports 7 like every other tool. The
    // rest of the errors stay ordinary failures and exit 1.
    if let Err(err) = run().await {
        if let Some(exit) = err.downcast_ref::<proot_core::ChildExit>() {
            std::process::exit(exit.0);
        }
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Check for direct execution mode early (before clap parsing)
    // Direct execution: podnext [flags...] -r /rootfs [flags...] -- <cmd> ...
    // Direct execution only applies if NO subcommand is used and -r/--rootfs is BEFORE '--'
    let raw_args: Vec<String> = std::env::args_os()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let pn_debug = std::env::var_os("PN_NEXT_DEBUG").is_some();
    if pn_debug {
        pn_trace!("[DEBUG main] Raw args: {:?}", raw_args);
    }

    let dashdash_idx = raw_args
        .iter()
        .position(|a| a == "--")
        .unwrap_or(raw_args.len());
    let pre_dashdash_args = &raw_args[..dashdash_idx];

    const SUBCOMMANDS: &[&str] = &[
        "run",
        "login",
        "list",
        "ls",
        "ps",
        "kill",
        "copy",
        "sync",
        "rename",
        "reset",
        "install",
        "remove",
        "rm",
        "search",
        "build",
        "docker",
        "image",
        "images",
        "network",
        "volume",
        "backup",
        "restore",
        "plugin",
        "help",
        "completion",
        "status",
        "info",
        "exec",
        "logs",
        "stop",
        "start",
        "restart",
        "attach",
        "commit",
    ];
    let has_subcommand = pre_dashdash_args
        .iter()
        .skip(1)
        .any(|a| SUBCOMMANDS.contains(&a.as_str()));
    let is_direct_execution = !has_subcommand
        && pre_dashdash_args
            .iter()
            .skip(1)
            .any(|a| a == "-r" || a == "--rootfs");

    if is_direct_execution {
        if pn_debug {
            pn_trace!("[DEBUG main] Direct execution mode detected");
        }

        // Parse all flags manually for direct execution
        // Format: podnext [flags...] -r <rootfs> [flags...] -- <cmd...>

        let mut rootfs: Option<String> = None;
        let mut binds = Vec::new();
        let mut pwd: Option<String> = None;
        let mut qemu: Option<String> = None;
        let mut link2symlink = false;
        let mut no_sysvipc = false;
        let mut sysvipc = false;
        let mut kernel: Option<String> = None;
        let mut hostname: Option<String> = None;
        let mut uid: Option<u32> = None;
        let mut gid: Option<u32> = None;
        let mut seccomp = false;
        let mut no_seccomp = false;
        let mut trace = false;
        let mut kill_on_exit = false;
        let mut no_kill_on_exit = false;
        let mut env = Vec::new();
        let mut verbose = false;
        let mut cmd_args: Vec<String> = Vec::new();

        let mut i = 1; // Skip program name

        // Parse all flags until --
        while i < raw_args.len() {
            let arg = &raw_args[i];
            if arg == "--" {
                // Remaining args are the command
                cmd_args = raw_args[i + 1..].to_vec();
                break;
            }

            match arg.as_str() {
                "-r" | "--rootfs" => {
                    i += 1;
                    if i < raw_args.len() {
                        rootfs = Some(raw_args[i].clone());
                    }
                }
                "-b" | "--bind" => {
                    i += 1;
                    if i < raw_args.len() {
                        binds.push(raw_args[i].clone());
                    }
                }
                "-w" | "--pwd" => {
                    i += 1;
                    if i < raw_args.len() {
                        pwd = Some(raw_args[i].clone());
                    }
                }
                "-q" | "--qemu" => {
                    i += 1;
                    if i < raw_args.len() {
                        qemu = Some(raw_args[i].clone());
                    }
                }
                "-0" | "--link2symlink" => {
                    link2symlink = true;
                }
                "--no-sysvipc" => {
                    no_sysvipc = true;
                }
                "--sysvipc" => {
                    sysvipc = true;
                }
                "--kernel" => {
                    i += 1;
                    if i < raw_args.len() {
                        kernel = Some(raw_args[i].clone());
                    }
                }
                "--hostname" => {
                    i += 1;
                    if i < raw_args.len() {
                        hostname = Some(raw_args[i].clone());
                    }
                }
                "--uid" => {
                    i += 1;
                    if i < raw_args.len() {
                        uid = raw_args[i].parse().ok();
                    }
                }
                "--gid" => {
                    i += 1;
                    if i < raw_args.len() {
                        gid = raw_args[i].parse().ok();
                    }
                }
                "--seccomp" => {
                    seccomp = true;
                }
                "--no-seccomp" => {
                    no_seccomp = true;
                }
                "--trace" => {
                    trace = true;
                }
                "--kill-on-exit" => {
                    kill_on_exit = true;
                }
                "--no-kill-on-exit" => {
                    no_kill_on_exit = true;
                }
                "-e" | "--env" => {
                    i += 1;
                    if i < raw_args.len() {
                        env.push(raw_args[i].clone());
                    }
                }
                "-v" | "--verbose" => {
                    verbose = true;
                }
                _ => {
                    // Unknown flag, skip
                }
            }
            i += 1;
        }

        if rootfs.is_none() {
            eprintln!("Error: -r/--rootfs is required for direct execution mode");
            std::process::exit(1);
        }

        if cmd_args.is_empty() {
            eprintln!("Error: Direct execution mode requires a command after --");
            std::process::exit(1);
        }

        if pn_debug {
            pn_trace!("[DEBUG main] Calling execute_direct_manual with all flags");
        }
        // For now, just call execute_direct_manual with basic args
        // TODO: Pass all parsed flags to a new execute_direct_manual_full function
        return execute_direct_manual(rootfs.unwrap(), &cmd_args, verbose).await;
    }

    // Parse trailing arguments for direct execution mode (-- <cmd> ...) for clap-parsed case
    let args: Vec<String> = std::env::args_os()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let cli_trailing_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        Vec::new()
    };

    let cli = Cli::parse();

    // Direct execution mode (like proot) - clap parsed case
    if cli.command.is_none() && cli.rootfs.is_some() {
        return execute_direct(&cli, &cli_trailing_args).await;
    }

    // Subcommand mode
    if let Some(cmd) = cli.command {
        pn_trace!("DEBUG main: Executing command: {:?}", cmd);
        // `-b/--bind` is a global option, so `login` and `run` must see it too; it used to
        // reach only the direct-execution path below.
        return execute_command(cmd, cli.bind.clone()).await;
    }

    print_help();
    Ok(())
}

async fn execute_direct(cli: &Cli, trailing_args: &[String]) -> Result<()> {
    if trailing_args.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", cli.rootfs);
    pn_trace!("[DEBUG direct] Binds: {:?}", cli.bind);
    pn_trace!("[DEBUG direct] Working dir: {:?}", cli.pwd);
    pn_trace!("[DEBUG direct] QEMU: {:?}", cli.qemu);
    pn_trace!("[DEBUG direct] Link2symlink: {}", cli.link2symlink);
    pn_trace!("[DEBUG direct] No sysvipc: {}", cli.no_sysvipc);
    pn_trace!("[DEBUG direct] Sysvipc: {}", cli.sysvipc);
    pn_trace!("[DEBUG direct] Kernel: {:?}", cli.kernel);
    pn_trace!("[DEBUG direct] Hostname: {:?}", cli.hostname);
    pn_trace!("[DEBUG direct] UID: {:?}", cli.uid);
    pn_trace!("[DEBUG direct] GID: {:?}", cli.gid);
    pn_trace!("[DEBUG direct] Seccomp: {}", cli.seccomp);
    pn_trace!("[DEBUG direct] No seccomp: {}", cli.no_seccomp);
    pn_trace!("[DEBUG direct] Trace: {}", cli.trace);
    pn_trace!("[DEBUG direct] Kill on exit: {}", cli.kill_on_exit);
    pn_trace!("[DEBUG direct] No kill on exit: {}", cli.no_kill_on_exit);
    pn_trace!("[DEBUG direct] Env: {:?}", cli.env);
    pn_trace!("[DEBUG direct] Command: {:?}", trailing_args);

    // Create a temporary container config for direct execution
    let rootfs_path = cli.rootfs.as_ref().unwrap();

    // Use proot-core directly for direct execution
    let mut config = proot_core::ProotConfig::default();
    config.verbose = cli.verbose;
    let mut engine = proot_core::ProotEngine::new(config)?;

    engine.config.rootfs = Some(rootfs_path.into());

    // Process bind mounts
    for bind in &cli.bind {
        engine.config.bind.push(bind.clone());
    }

    // Set working directory
    if let Some(pwd) = &cli.pwd {
        engine.config.cwd = Some(pwd.into());
    }

    // Set environment variables
    for env in &cli.env {
        engine.config.env.push(env.clone());
    }

    // Set kernel release
    if let Some(kernel) = &cli.kernel {
        engine.config.kernel_release = Some(kernel.clone());
    }

    // Set hostname
    if let Some(hostname) = &cli.hostname {
        engine.config.hostname = Some(hostname.clone());
    }

    // Set UID/GID
    if let Some(uid) = cli.uid {
        engine.config.uid = Some(uid);
    }
    if let Some(gid) = cli.gid {
        engine.config.gid = Some(gid);
    }

    // Asking to switch seccomp filtering OFF stays a no-op: it requests nothing. Asking to switch
    // it ON is refused before dispatch, where the global flag is read (see `refuse_unenforceable`).
    if cli.no_seccomp {
        engine.config.seccomp = false;
    }

    // Set trace
    if cli.trace {
        engine.config.trace = true;
    }

    // Set kill-on-exit
    if cli.kill_on_exit {
        engine.config.kill_on_exit = true;
    }
    if cli.no_kill_on_exit {
        engine.config.kill_on_exit = false;
    }

    // Set sysvipc
    if cli.no_sysvipc {
        engine.config.sysvipc = false;
    }
    if cli.sysvipc {
        engine.config.sysvipc = true;
    }

    // Set link2symlink
    if cli.link2symlink {
        engine.config.link2symlink = true;
    }

    // Set command
    engine.config.command = trailing_args.to_vec();

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

async fn execute_direct_manual(rootfs: String, cmd_args: &[String], verbose: bool) -> Result<()> {
    if cmd_args.is_empty() {
        bail!("Direct execution mode requires a command. Use 'podnext -r /path/to/rootfs -- <command>'");
    }

    pn_trace!("[DEBUG direct] Rootfs: {:?}", rootfs);
    pn_trace!("[DEBUG direct] Command: {:?}", cmd_args);
    pn_trace!("[DEBUG direct] Verbose: {}", verbose);

    // Use proot-core directly for direct execution
    let mut config = proot_core::ProotConfig::default();
    config.verbose = if verbose { 1 } else { 0 };
    let mut engine = proot_core::ProotEngine::new(config)?;

    engine.config.rootfs = Some(rootfs.into());
    engine.config.command = cmd_args.to_vec();

    // Execute and wait for completion
    let exit_code = engine.execute().await? as i32;
    if exit_code != 0 {
        return Err(anyhow::Error::new(proot_core::ChildExit(exit_code)));
    }

    Ok(())
}

async fn execute_command(cmd: Commands, global_binds: Vec<String>) -> Result<()> {
    pn_trace!("DEBUG execute_command: START {:?}", cmd);
    match cmd {
        // Container commands
        Commands::Install { image, name, force } => cmd_install(image, name, force).await,
        Commands::List {
            all,
            no_all,
            format,
        } => cmd_list(all, no_all, format).await,
        Commands::Login {
            container,
            user,
            isolated,
            command,
        } => cmd_login(container, user, isolated, command, global_binds).await,
        Commands::Run {
            container,
            command,
            rm,
            detach,
            name,
            network,
            volume,
            publish,
            entrypoint,
            tty,
            interactive,
            user,
            workdir,
            isolated,
            ..   // the global seccomp/trace flags are handled where the engine is configured
        } => {
            cmd_run(
                container,
                command,
                rm,
                detach,
                name,
                network,
                volume,
                publish,
                entrypoint,
                tty,
                interactive,
                user,
                workdir,
                isolated,
                global_binds,
            )
            .await
        }
        Commands::Ps { format } => cmd_ps(format).await,
        Commands::Sessions { format } => cmd_sessions(format).await,
        Commands::Attach {
            container,
            user,
            command,
        } => cmd_attach(container, user, command).await,
        Commands::Kill { container } => cmd_kill(container).await,
        Commands::Remove { name, force, yes } => cmd_remove(name, force || yes).await,
        Commands::Rename { oldname, newname } => cmd_rename(oldname, newname).await,
        Commands::Copy {
            source,
            destination,
            recursive,
            move_,
        } => cmd_copy(source, destination, recursive, move_).await,
        Commands::Reset { name } => cmd_reset(name).await,
        Commands::Sync {
            source,
            destination,
            delete,
        } => cmd_sync(source, destination, delete).await,
        Commands::Search {
            query,
            limit,
            format,
        } => cmd_search(query, limit, format).await,
        Commands::Image(cmd) => execute_image_command(cmd).await,
        Commands::Network(cmd) => execute_network_command(cmd).await,
        Commands::Volume(cmd) => execute_volume_command(cmd).await,
        Commands::Info { format } => cmd_info(format).await,
        Commands::Version { format } => cmd_version(format).await,
        Commands::Doctor { container, format } => cmd_doctor(container, format).await,

        // Service orchestration commands
        Commands::Service(cmd) => execute_service_command(cmd).await,
        Commands::Daemon(cmd) => execute_daemon_command(cmd).await,
        Commands::Watchdog(cmd) => execute_watchdog_command(cmd).await,
        Commands::Quota(cmd) => execute_quota_command(cmd).await,
        Commands::Log(cmd) => execute_log_command(cmd).await,
        Commands::Dbus(cmd) => execute_dbus_command(cmd).await,
        Commands::Socket(cmd) => execute_socket_command(cmd).await,
        Commands::Boot(cmd) => execute_boot_command(cmd).await,
        Commands::Wakelock(cmd) => execute_wakelock_command(cmd).await,
        Commands::Policy(cmd) => execute_policy_command(cmd).await,

        // Phase 7: Platform Maturity commands
        Commands::Backup(cmd) => execute_backup_command(cmd).await,
        Commands::Build(cmd) => execute_build_command(cmd).await,
        Commands::Config(cmd) => execute_config_command(cmd),
        Commands::Gitops(cmd) => execute_gitops_command(cmd).await,
        Commands::Dashboard(cmd) => execute_dashboard_command(cmd).await,
        Commands::Docker { container, args } => cmd_docker(container, args).await,
        Commands::Compose {
            files,
            image,
            output,
        } => execute_compose_command(files, image, output).await,

        // Utilities
        Commands::Completion { shell } => cmd_completion(shell).await,
    }
}

// Output formatting helpers
pub(crate) fn output_table<T: Tabled>(items: &[T]) {
    println!("{}", Table::new(items).to_string());
}

pub(crate) fn output_json<T: serde::Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(items)?);
    Ok(())
}

pub(crate) fn output_yaml<T: serde::Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_yaml::to_string(items)?);
    Ok(())
}

// Container command implementations (existing)
/// `podnext config validate`: the load site for the gateway configuration.
///
/// Before this existed the loader was reachable only from tests, so the only way
/// to discover that a documented value could not be loaded was to read the code.
/// The command uses the same entry points the gateway will, and exits non-zero on
/// a configuration the gateway would refuse to start with.
fn execute_config_command(cmd: ConfigCommands) -> Result<()> {
    use pn_gateway::config::GatewayConfig;

    match cmd {
        ConfigCommands::Validate { path, format } => {
            let path = match path.or_else(|| {
                // Same variable the gateway reads, so the check and the start-up
                // path cannot disagree about which file is in play.
                std::env::var(GatewayConfig::CONFIG_ENV)
                    .ok()
                    .filter(|p| !p.trim().is_empty())
                    .map(PathBuf::from)
            }) {
                Some(path) => path,
                None => {
                    eprintln!(
                        "no configuration file given: pass a path or set {}",
                        GatewayConfig::CONFIG_ENV
                    );
                    std::process::exit(3);
                }
            };

            let config = match GatewayConfig::from_toml_file(&path) {
                Ok(config) => config,
                Err(err) => {
                    // Exit code 3 keeps "invalid configuration" distinguishable
                    // from generic application errors (1) and clap usage (2).
                    eprintln!("invalid gateway configuration: {err}");
                    std::process::exit(3);
                }
            };

            match format {
                OutputFormat::Json => {
                    output_json(&config)?;
                    return Ok(());
                }
                OutputFormat::Yaml => {
                    output_yaml(&config)?;
                    return Ok(());
                }
                _ => {}
            }

            use pn_gateway::duration_fmt::format_duration;
            println!("{} is valid", path.display());
            println!("  name                {}", config.name);
            println!("  id                  {}", config.gateway_id);
            println!(
                "  proot timeout       {}",
                format_duration(config.proot_config.timeout)
            );
            println!(
                "  retries             {} (delay {})",
                config.fail_safe.max_retries,
                format_duration(config.fail_safe.retry_delay)
            );
            println!(
                "  circuit breaker     {} (threshold {}, timeout {})",
                config.fail_safe.circuit_breaker.enabled,
                config.fail_safe.circuit_breaker.failure_threshold,
                format_duration(config.fail_safe.circuit_breaker.timeout)
            );
            println!(
                "  health check        {} every {}",
                config.health.enabled,
                format_duration(config.health.check_interval)
            );
            println!(
                "  metrics             {} every {}, retained {}",
                config.metrics.enabled,
                format_duration(config.metrics.interval),
                format_duration(config.metrics.retention)
            );
            println!(
                "  storage cleanup     {} / backup {}",
                format_duration(config.storage.cleanup_interval),
                format_duration(config.storage.backup_interval)
            );
            Ok(())
        }
    }
}

/// `podnext doctor`: report what the host and the guest actually look like.
///
/// Exit status is driven by the worst verdict: a failing check exits 1. Warnings and
/// unknowns are printed without failing the command, because "not measured" is not the
/// same as "broken".
async fn cmd_doctor(container: Option<String>, format: OutputFormat) -> Result<()> {
    let checks = doctor::collect(container.as_deref()).await;
    match format {
        OutputFormat::Table | OutputFormat::Text => doctor::render_text(&checks),
        OutputFormat::Json => output_json(&checks)?,
        OutputFormat::Yaml => output_yaml(&checks)?,
    }
    if doctor::worst_verdict(&checks) == doctor::Verdict::Fail {
        anyhow::bail!("one or more checks failed; the rows marked `fail` name what and why");
    }
    Ok(())
}

async fn cmd_version(format: OutputFormat) -> Result<()> {
    let version_info = VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: option_env!("GIT_COMMIT").unwrap_or("unknown").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
        rust_version: "1.80+".to_string(),
    };

    match format {
        OutputFormat::Table => output_table(&[version_info]),
        OutputFormat::Json => output_json(&version_info)?,
        OutputFormat::Yaml => output_yaml(&version_info)?,
        OutputFormat::Text => println!("podnext {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

async fn cmd_completion(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let name = "podnext".to_string();

    match shell {
        Shell::Bash => clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Zsh => clap_complete::generate(
            clap_complete::Shell::Zsh,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Fish => clap_complete::generate(
            clap_complete::Shell::Fish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::PowerShell => clap_complete::generate(
            clap_complete::Shell::PowerShell,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
        Shell::Elvish => clap_complete::generate(
            clap_complete::Shell::Elvish,
            &mut cmd,
            name,
            &mut std::io::stdout(),
        ),
    }
    Ok(())
}

fn print_help() {
    let mut cmd = Cli::command();
    cmd.print_help().unwrap();
    println!();
}

// Output format helpers
#[derive(Tabled)]
struct SystemInfo {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Platform")]
    platform: String,
    #[tabled(rename = "Rust Version")]
    rust_version: String,
    #[tabled(rename = "Backend")]
    backend: String,
    #[tabled(rename = "Isolation")]
    isolation: String,
}

#[derive(Tabled, serde::Serialize)]
struct VersionInfo {
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Git Commit")]
    git_commit: String,
    #[tabled(rename = "Build Date")]
    build_date: String,
    #[tabled(rename = "Rust Version")]
    rust_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guest path in `container:[CONTAINER:]PATH` comes from the user, so it
    /// is a trust boundary. A plain `join` let `c1:../../../../etc/shadow` read a
    /// host file; these cases pin that shut.
    #[test]
    fn guest_to_host_refuses_traversal() {
        let rootfs = std::path::Path::new("/data/podnext/containers/c1/rootfs");

        // Ordinary paths resolve under the rootfs.
        assert_eq!(
            guest_to_host(rootfs, "/etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );
        assert_eq!(
            guest_to_host(rootfs, "etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );
        // "." is a no-op, and a trailing slash is fine.
        assert_eq!(
            guest_to_host(rootfs, "/./etc/hosts").unwrap(),
            rootfs.join("etc/hosts")
        );

        // Anything that climbs out is refused, however it is spelled.
        for evil in [
            "/../../../../etc/shadow",
            "../etc/shadow",
            "etc/../../..",
            "/etc/../../../etc/shadow",
            "..",
            "/..",
        ] {
            assert!(
                guest_to_host(rootfs, evil).is_err(),
                "expected {:?} to be refused",
                evil
            );
        }
    }

    fn tmp_tree(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdn-sync-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sync_transfers_only_what_differs() {
        let root = tmp_tree("diff");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "one").unwrap();
        std::fs::write(src.join("sub/b.txt"), "two").unwrap();

        let mut report = SyncReport::default();
        mirror_dir(&src, &dst, &mut report).unwrap();
        assert_eq!(report.copied, 2, "first pass copies everything");
        assert_eq!(
            std::fs::read_to_string(dst.join("sub/b.txt")).unwrap(),
            "two"
        );

        // Second pass: nothing changed, nothing moves.
        let mut second = SyncReport::default();
        mirror_dir(&src, &dst, &mut second).unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.unchanged, 2);

        // Change one file and only that one moves.
        std::fs::write(src.join("a.txt"), "ONE!").unwrap();
        let mut third = SyncReport::default();
        mirror_dir(&src, &dst, &mut third).unwrap();
        assert_eq!(third.copied, 1);
        assert_eq!(third.unchanged, 1);
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "ONE!");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_removes_only_orphans() {
        let root = tmp_tree("prune");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(dst.join("stale-dir")).unwrap();
        std::fs::write(src.join("keep.txt"), "k").unwrap();
        std::fs::write(dst.join("keep.txt"), "k").unwrap();
        std::fs::write(dst.join("drop.txt"), "d").unwrap();
        std::fs::write(dst.join("stale-dir/inner.txt"), "d").unwrap();

        let mut report = SyncReport::default();
        prune_extras(&src, &dst, &mut report).unwrap();

        assert!(dst.join("keep.txt").exists(), "a live file must survive");
        assert!(!dst.join("drop.txt").exists(), "an orphan must go");
        assert!(
            !dst.join("stale-dir").exists(),
            "an orphan directory must go"
        );
        assert_eq!(report.removed, 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn same_content_compares_bytes_not_names() {
        let root = tmp_tree("same");
        std::fs::write(root.join("a"), "abcdef").unwrap();
        std::fs::write(root.join("b"), "abcdef").unwrap();
        std::fs::write(root.join("c"), "abcde").unwrap();

        assert!(same_content(&root.join("a"), &root.join("b")).unwrap());
        assert!(!same_content(&root.join("a"), &root.join("c")).unwrap());
        assert!(!same_content(&root.join("a"), &root.join("missing")).unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_keeps_sparse_files_sparse() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::unix::fs::MetadataExt;

        let root = tmp_tree("sparse");
        let src = root.join("sparse.bin");
        let dst = root.join("copy.bin");

        // 64 MiB apparent, one 4 KiB page of real data: dense would be ~131072
        // blocks; the bug made exactly that (issue #18).
        let f = std::fs::File::create(&src).unwrap();
        f.set_len(64 * 1024 * 1024).unwrap();
        drop(f);
        let mut w = std::fs::OpenOptions::new().write(true).open(&src).unwrap();
        w.seek(SeekFrom::Start(1024 * 1024)).unwrap();
        w.write_all(&[0xAB; 4096]).unwrap();
        drop(w);

        copy_preserving_holes(&src, &dst).unwrap();

        let s = std::fs::metadata(&src).unwrap();
        let d = std::fs::metadata(&dst).unwrap();
        assert_eq!(s.len(), d.len(), "apparent size must match");
        assert_eq!(
            std::fs::read(&src).unwrap(),
            std::fs::read(&dst).unwrap(),
            "content must be byte-identical"
        );

        // A filesystem with no hole support would densify the source too; there
        // is nothing to assert there.
        if s.blocks() * 512 >= 1024 * 1024 {
            let _ = std::fs::remove_dir_all(&root);
            return;
        }
        assert!(
            d.blocks() * 512 < 1024 * 1024,
            "sparse copy allocated {} bytes, expected far less than 1 MiB",
            d.blocks() * 512
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn split_spec_only_treats_a_path_free_prefix_as_a_container() {
        assert_eq!(split_spec("c1:/etc/x"), (Some("c1"), "/etc/x"));
        assert_eq!(split_spec("/etc/x"), (None, "/etc/x"));
        // A host path containing a colon must not be read as a container name.
        assert_eq!(split_spec("/data/a:b/c"), (None, "/data/a:b/c"));
        assert_eq!(split_spec("./rel/x"), (None, "./rel/x"));
    }

    /// `image inspect` must find what `build local`'s store holds, under the tag
    /// or under the derived store key (#29 / #35), and must not invent an answer
    /// for a reference neither source knows.
    #[test]
    fn store_metadata_lookup_accepts_tag_and_key_and_reports_absence() {
        let store = tempfile::tempdir().unwrap();
        let dir = store.path().join("demoinspect");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("metadata.json"),
            r#"{"name":"demoinspect:latest","image":"scratch","exposed_ports":[8080]}"#,
        )
        .unwrap();

        // By tag: the store key is the colon-free form.
        let (found_dir, meta) = read_store_metadata(store.path(), "demoinspect:latest")
            .expect("the tag must resolve to its store key");
        assert_eq!(found_dir.file_name().unwrap(), "demoinspect");
        assert_eq!(meta["exposed_ports"][0], 8080);

        // By store key.
        assert!(read_store_metadata(store.path(), "demoinspect").is_some());

        // Unknown reference: nothing, so the caller reports it instead of exiting 0
        // with no output.
        assert!(read_store_metadata(store.path(), "nosuchimg").is_none());

        // A reference that could escape the store is not a path lookup.
        assert!(read_store_metadata(store.path(), "../demoinspect").is_none());

        // A directory without metadata.json is not an image.
        std::fs::create_dir_all(store.path().join("empty")).unwrap();
        assert!(read_store_metadata(store.path(), "empty").is_none());
    }

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn format_accepted_globally() {
        use clap::Parser;
        let cli = Cli::try_parse_from(&["podnext", "--format", "json", "list"]).unwrap();
        assert_eq!(cli.format, Some(OutputFormat::Json));
    }

    #[test]
    fn conflicting_flags_error() {
        use clap::Parser;
        assert!(Cli::try_parse_from(&["podnext", "list", "--all", "--no-all"]).is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--seccomp",
            "--no-seccomp",
            "--",
            "ls"
        ])
        .is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--sysvipc",
            "--no-sysvipc",
            "--",
            "ls"
        ])
        .is_err());
        assert!(Cli::try_parse_from(&[
            "podnext",
            "-r",
            "/tmp",
            "--kill-on-exit",
            "--no-kill-on-exit",
            "--",
            "ls"
        ])
        .is_err());
    }
}
