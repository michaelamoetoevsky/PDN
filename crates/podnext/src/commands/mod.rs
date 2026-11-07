//! CLI command implementations for service orchestration

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
use anyhow::{bail, Context, Result};
use pn_next::{
    BackupConfig,
    // Phase 7 modules
    BackupManager,
    BackupType,
    DaemonRuntime,
    DaemonRuntimeConfig,
    GitOpsConfig,
    GitOpsManager,
    GitOpsResourceStatus,
    RestoreOptions,
    ServiceState,
    ServiceSupervisor,
    SupervisorBuilder,
    SyncStatus,
    TermuxBootConfig,
    TermuxBootManager,
    WakeLockConfig,
    WakeLockManager,
    WakeLockType,
    WatchdogConfig,
    WatchdogManager,
    WebDashboard,
    WebDashboardConfig,
};
use serde::Serialize;
use serde_json;
use serde_yaml;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tabled::{Table, Tabled};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

mod build;
mod compose;
pub mod container;
pub mod dbus;
mod gitops;
pub mod image;
pub mod log;
pub mod network;
pub mod policy;
pub mod power;
pub mod quota;
pub mod service;
pub mod socket;
pub mod sync;
pub mod volume;
pub mod watchdog;
pub use build::execute_build_command;
pub use compose::execute_compose_command;
pub use dbus::*;
pub use gitops::{execute_dashboard_command, execute_gitops_command};
pub use log::*;
pub use policy::*;
pub use power::{execute_boot_command, execute_wakelock_command};
pub use quota::*;
pub use service::{execute_daemon_command, execute_service_command};
pub use socket::*;
pub use watchdog::execute_watchdog_command;

// Output formatters
fn output_table<T: Tabled>(items: &[T]) {
    println!("{}", Table::new(items).to_string());
}

fn output_json<T: Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(items)?);
    Ok(())
}

fn output_yaml<T: Serialize>(items: &T) -> Result<()> {
    println!("{}", serde_yaml::to_string(items)?);
    Ok(())
}

// ============ Service Commands ============

async fn get_supervisor() -> Result<ServiceSupervisor> {
    // Derived from the home directory rather than a hardcoded Termux path, so the
    // supervisor also works when PDN is run from a different prefix.
    // A19: one resolver for the store root, so a redirected root (`PN_TEST_ROOT`)
    // is followed instead of the raw `dirs::home_dir()`.
    let home = proot_distro_core::paths::resolve_home().unwrap_or_default();
    let env_or = |key: &str, fallback: std::path::PathBuf| {
        std::env::var(key).unwrap_or_else(|_| fallback.to_string_lossy().into_owned())
    };
    let services_dir = env_or(
        "PN_NEXT_SERVICES_DIR",
        home.join(".config/pn-next/services"),
    );
    let runtime_dir = env_or("PN_NEXT_RUNTIME_DIR", home.join(".local/run/pn-next"));
    let log_dir = env_or("PN_NEXT_LOG_DIR", home.join(".local/log/pn-next"));

    let supervisor = SupervisorBuilder::new()
        .services_dir(services_dir)
        .runtime_dir(runtime_dir)
        .log_dir(log_dir)
        .build()?;

    // Initialize to load services from directory
    supervisor.initialize().await?;

    Ok(supervisor)
}

// ============ Daemon Commands ============

// ============ Watchdog Commands ============

// ============ Boot Commands ============

// ============ Wake Lock Commands ============

// ============ Backup Commands ============

pub async fn execute_backup_command(cmd: crate::BackupCommands) -> Result<()> {
    // Initialize backup manager with default config
    pn_trace!("DEBUG: execute_backup_command START");
    pn_trace!("DEBUG: Creating BackupConfig");
    let config = BackupConfig::default();
    pn_trace!("DEBUG: backup_dir = {:?}", config.backup_dir);
    pn_trace!("DEBUG: containers_dir = {:?}", config.containers_dir);
    pn_trace!("DEBUG: Creating BackupManager");
    let manager = BackupManager::new(config);
    pn_trace!("DEBUG: Starting backup manager");
    manager.start().await?;
    pn_trace!("DEBUG: Backup manager started");

    pn_trace!("DEBUG: Matching backup command");
    match cmd {
        crate::BackupCommands::Create {
            container,
            backup_type,
            format,
        } => cmd_backup_create(&manager, container, backup_type, format).await,
        crate::BackupCommands::List { container, format } => {
            cmd_backup_list(&manager, container, format).await
        }
        crate::BackupCommands::Restore { backup_id, target } => {
            cmd_backup_restore(&manager, backup_id, target).await
        }
        crate::BackupCommands::Delete { backup_id } => cmd_backup_delete(&manager, backup_id).await,
        crate::BackupCommands::Status { backup_id } => cmd_backup_status(&manager, backup_id).await,
        crate::BackupCommands::Config => cmd_backup_config(&manager).await,
    }
}

async fn cmd_backup_create(
    manager: &BackupManager,
    container: String,
    backup_type: crate::BackupTypeArg,
    _format: crate::OutputFormat,
) -> Result<()> {
    let bt = match backup_type {
        crate::BackupTypeArg::Full => BackupType::Full,
        crate::BackupTypeArg::Incremental => BackupType::Incremental,
        crate::BackupTypeArg::Differential => BackupType::Differential,
        crate::BackupTypeArg::ConfigOnly => BackupType::Manual,
    };

    eprintln!("Creating {:?} backup for container '{}'...", bt, container);
    let backup_id = manager.create_backup(&container, &container, bt).await?;
    println!("Backup created with ID: {}", backup_id);
    Ok(())
}

async fn cmd_backup_list(
    manager: &BackupManager,
    container: Option<String>,
    format: crate::OutputFormat,
) -> Result<()> {
    let backups = manager.list_backups(container.as_deref()).await;

    #[derive(Tabled, Serialize)]
    struct BackupRow {
        #[tabled(rename = "ID")]
        id: String,
        #[tabled(rename = "NAME")]
        name: String,
        #[tabled(rename = "CONTAINER")]
        container_id: String,
        #[tabled(rename = "TYPE")]
        backup_type: String,
        #[tabled(rename = "STATUS")]
        status: String,
        #[tabled(rename = "SIZE")]
        size: String,
        #[tabled(rename = "CREATED")]
        created_at: String,
    }

    let rows: Vec<BackupRow> = backups
        .iter()
        .map(|b| BackupRow {
            id: b.id.clone(),
            name: b.name.clone(),
            container_id: b.container_id.clone(),
            backup_type: format!("{:?}", b.backup_type),
            status: format!("{:?}", b.status),
            size: crate::utils::format_size(b.size_bytes),
            created_at: chrono::DateTime::<chrono::Utc>::from(b.created_at)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        })
        .collect();

    match format {
        crate::OutputFormat::Table => output_table(&rows),
        crate::OutputFormat::Json => output_json(&backups)?,
        crate::OutputFormat::Yaml => output_yaml(&backups)?,
        crate::OutputFormat::Text => {
            for b in backups {
                println!(
                    "{} ({:?}) [{:?}] {}",
                    b.id,
                    b.backup_type,
                    b.status,
                    crate::utils::format_size(b.size_bytes)
                );
            }
        }
    }
    Ok(())
}

async fn cmd_backup_restore(
    manager: &BackupManager,
    backup_id: String,
    target: Option<String>,
) -> Result<()> {
    println!("Restoring backup '{}'...", backup_id);
    let options = RestoreOptions {
        target_name: target,
        target_id: None,
        restore_volumes: true,
        restore_config: true,
        restore_logs: true,
        overwrite: false,
        auto_start: false,
        network: None,
        labels: HashMap::new(),
    };
    let container_id = manager.restore_backup(&backup_id, options).await?;
    println!("Restored to container: {}", container_id);
    Ok(())
}

async fn cmd_backup_delete(manager: &BackupManager, backup_id: String) -> Result<()> {
    eprintln!("Deleting backup '{}'...", backup_id);
    manager.delete_backup(&backup_id).await?;
    println!("Backup deleted");
    Ok(())
}

async fn cmd_backup_status(manager: &BackupManager, backup_id: String) -> Result<()> {
    if let Some(backup) = manager.get_backup(&backup_id).await {
        println!("Backup ID: {}", backup.id);
        println!("Name: {}", backup.name);
        println!("Container: {}", backup.container_name);
        println!("Type: {:?}", backup.backup_type);
        println!("Status: {:?}", backup.status);
        println!("Size: {}", crate::utils::format_size(backup.size_bytes));
        println!(
            "Compressed: {}",
            crate::utils::format_size(backup.compressed_size)
        );
        println!("Checksum: {}", backup.checksum);
        if let Some(completed) = backup.completed_at {
            println!(
                "Completed: {} seconds ago",
                completed
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
        }
    } else {
        anyhow::bail!("no such backup: {}", backup_id);
    }
    Ok(())
}

async fn cmd_backup_config(_manager: &BackupManager) -> Result<()> {
    // Access config directly since it's not exposed via get_config
    println!("Backup Configuration:");
    println!("  (Config access not directly available via API)");
    Ok(())
}

#[cfg(test)]
mod build_local_tests {
    use super::build::*;
    use proot_distro_core::dockerfile::{parse_dockerfile, DockerfileInstruction};

    /// The exec plan `cmd_build_local` derives from Dockerfile content: the
    /// `RUN` commands in order, plus the declared build args.
    fn run_plan(content: &str) -> (Vec<String>, Vec<(String, String)>) {
        let (cleaned, args) = preprocess_build_args(content);
        let cmds = parse_dockerfile(&cleaned)
            .expect("preprocessed Dockerfile must parse")
            .into_iter()
            .filter_map(|i| match i {
                DockerfileInstruction::Run(c) => Some(c),
                _ => None,
            })
            .collect();
        (cmds, args)
    }

    // Fails before the fix: `parse_dockerfile` bails on `ARG`.
    #[test]
    fn arg_is_accepted_and_substituted() {
        let content = "FROM alpine:3.19\nARG VERSION=1.2.3\nARG NAME=\"world\"\nRUN echo $VERSION ${NAME}\nCOPY a b\n";
        let (cmds, args) = run_plan(content);
        assert_eq!(
            args,
            vec![
                ("VERSION".to_string(), "1.2.3".to_string()),
                ("NAME".to_string(), "world".to_string()),
            ]
        );
        assert_eq!(cmds, vec!["echo 1.2.3 world".to_string()]);
    }

    #[test]
    fn arg_without_default_is_declared_empty() {
        let (cmds, args) = run_plan("FROM alpine\nARG TOKEN\nRUN echo ${TOKEN}\n");
        assert_eq!(args, vec![("TOKEN".to_string(), String::new())]);
        assert_eq!(cmds, vec!["echo".to_string()]);
    }

    // Regression for #29: a missing build context must fail hard and write
    // nothing. Before the fix `cmd_build_local` guarded all real work behind
    // `if path.exists()`, skipped it, printed "Successfully built image" and
    // exited 0. The contract asserted here is "Err returned AND the store is
    // untouched" — not one exact diagnostic string ("context missing" and
    // "Dockerfile missing" are both acceptable hard errors).
    #[tokio::test]
    async fn missing_context_is_a_hard_error_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-context");
        let result = build_local_image(
            "bse-missing:latest",
            None,
            &missing,
            Some(store.path().to_path_buf()),
        )
        .await;
        let err = result.expect_err("missing context must be an error, not a success");
        assert!(
            err.to_string().contains("does not exist")
                || err.to_string().contains("Dockerfile not found"),
            "unexpected error: {err}"
        );
        assert!(
            !store.path().join("bse-missing").exists(),
            "a failed build must not register an image"
        );
        assert!(
            !store.path().join("bse-missing:latest").exists(),
            "the store key must never be the raw tag (#35)"
        );
    }

    // Regression for #29: on success the built image must actually be present in
    // the image store as a runnable container (a container dir with a rootfs and
    // metadata.json) — not merely reported. `FROM scratch` keeps this offline.
    #[tokio::test]
    async fn successful_build_registers_a_runnable_image() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY app.txt /app.txt\n",
        )
        .unwrap();
        std::fs::write(ctx.path().join("app.txt"), "static app content\n").unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "bse-scratch:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        // The store key is the colon-free form of the tag (#35); the tag itself
        // stays in metadata below, so the image is still identifiable.
        assert_eq!(dir, store.path().join("bse-scratch"));
        assert!(
            !dir.to_string_lossy().contains(':'),
            "no host path under the store may contain ':': {}",
            dir.display()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/app.txt")).unwrap(),
            "static app content\n"
        );
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["name"], "bse-scratch:latest");
    }

    // Residual gap 2 (#30): `EXPOSE`/`USER` were parsed and then dropped. They are
    // image metadata (Docker's EXPOSE never binds a port), so the fix records them
    // in `metadata.json` where `run`/inspection can see what was built. Duplicate
    // EXPOSE of the same port is collapsed; `EXPOSE 8080/tcp` keeps the port only.
    // `FROM scratch` keeps this offline.
    #[tokio::test]
    async fn expose_and_user_are_recorded_in_metadata() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nEXPOSE 8080\nEXPOSE 8080/tcp\nEXPOSE 443\nUSER nobody\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-meta:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["exposed_ports"], serde_json::json!([8080, 443]));
        assert_eq!(meta["user"], "nobody");
    }

    // Residual gap 1 (#30) is fixed at `run`, which reinstates ENV/WORKDIR read
    // back from this file (`native::apply_image_runtime_config`). This asserts the
    // build half of that contract: the values must be present and named exactly
    // as the run path reads them.
    #[tokio::test]
    async fn env_and_workdir_are_recorded_for_the_run_path() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nENV GREETING=hello-from-image\nENV APP_MODE=prod\nWORKDIR /srv/app\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-env:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["env"]["GREETING"], "hello-from-image");
        assert_eq!(meta["env"]["APP_MODE"], "prod");
        assert_eq!(meta["working_dir"], "/srv/app");
    }

    // C5/G3: the reference's `do_workdir` "creates the directory on disk ...
    // so the path still exists when the image is run"; its acceptance is "a
    // built image's WORKDIR exists at run time". PDN has no layer engine, so
    // it creates the ancestors directly in `build.rs`. Assert the on-disk half,
    // which the metadata assertion above does not cover.
    #[tokio::test]
    async fn workdir_directory_exists_in_the_built_rootfs() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nWORKDIR /srv/app/deep\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c5-wd-dir:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        assert!(
            dir.join("rootfs").join("srv/app/deep").is_dir(),
            "WORKDIR must exist in the built rootfs when the image is run"
        );
    }

    // C5/G3, the gap left after the directory is created: a *relative* WORKDIR.
    // Docker resolves it "relative to whatever the previous working directory
    // is" (`docs.docker.com/reference/build-checks/workdir-relative-path`), so
    // `WORKDIR /app` then `WORKDIR src` is `/app/src`. PDN prepended a slash
    // unconditionally and produced `/src` - not the scoping `build.rs:221`
    // claims to match.
    #[tokio::test]
    async fn relative_workdir_resolves_against_the_previous_workdir() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nWORKDIR /app\nWORKDIR src\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c5-wd-rel:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["working_dir"], "/app/src");
        assert!(
            dir.join("rootfs").join("app/src").is_dir(),
            "the chained directory must exist in the built rootfs"
        );
    }

    // Residual gap 3 (#30), wildcard slice: `COPY src/*.rs` was unsupported.
    /// The glob matcher gained `[...]` classes; `**` stays unsupported but is now
    /// refused with a message instead of silently matching nothing.
    #[test]
    fn glob_match_handles_character_classes() {
        assert!(glob_match("file[0-9].txt", "file7.txt"));
        assert!(!glob_match("file[0-9].txt", "filex.txt"));
        assert!(glob_match("[abc]x", "bx"));
        assert!(!glob_match("[abc]x", "dx"));
        assert!(glob_match("v[!0-9]", "vx"));
        assert!(!glob_match("v[!0-9]", "v1"));
        assert!(glob_match("v[^0-9]", "vy"));
        assert!(glob_match("[a-c]1", "b1"));
        assert!(!glob_match("[a-c]1", "e1"));
        // A ']' as the first character of a class is literal, as in the shell.
        assert!(glob_match("[]]", "]"));

        // `*` and `?` keep working, including a match that needs backtracking.
        assert!(glob_match("*[0-9]", "abc7"));
        assert!(glob_match("a*z[0-9]", "abcz4"));
        assert!(glob_match("?[a-z]", "xq"));
        assert!(!glob_match("?[a-z]", "x1"));
    }

    #[test]
    fn double_star_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let err = expand_glob(tmp.path(), "src/**/*.rs")
            .expect_err("'**' must be refused, not silently match nothing");
        let msg = err.to_string();
        assert!(
            msg.contains("**"),
            "error must show the offending token: {msg}"
        );
        assert!(msg.contains("not supported"), "error must say so: {msg}");
    }

    /// Multi-stage and `--from=` would build something other than what the file
    /// asks for, so both are refused before any work happens.
    #[test]
    fn unsupported_dockerfile_shapes_are_refused() {
        let ok = "FROM scratch\nCOPY a /a\nRUN echo hi\n";
        assert!(refuse_unsupported_build_features(ok).is_ok());
        assert!(refuse_unsupported_build_features("FROM alpine:3.19 AS build\nRUN true\n").is_ok());

        // Two stages with no stage copy: refused on the stage count.
        let two_stages = "FROM alpine:3.19 AS build\nRUN true\nFROM scratch\nCOPY a /a\n";
        let err = refuse_unsupported_build_features(two_stages)
            .unwrap_err()
            .to_string();
        assert!(err.contains("2 FROM stages"), "unexpected error: {err}");

        // Two stages *with* a stage copy: the more specific refusal wins, because it
        // names the line the reader has to look at.
        let both = "FROM alpine:3.19 AS build\nRUN true\nFROM scratch\nCOPY --from=build /x /x\n";
        let err = refuse_unsupported_build_features(both)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--from="), "unexpected error: {err}");
        assert!(
            err.contains("line 4"),
            "error should point at the line: {err}"
        );

        let stage_copy = "FROM alpine:3.19\nCOPY --from=builder /app /app\n";
        let err = refuse_unsupported_build_features(stage_copy)
            .unwrap_err()
            .to_string();
        assert!(err.contains("--from="), "unexpected error: {err}");
        assert!(
            err.contains("line 2"),
            "error should point at the line: {err}"
        );

        // A comment mentioning the feature must not trigger the refusal.
        assert!(refuse_unsupported_build_features("# COPY --from=x /a /b\nFROM scratch\n").is_ok());
    }

    #[test]
    fn glob_match_handles_star_and_question() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.txt"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "abbc"));
        assert!(glob_match(".*", ".env"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("src/*.rs", "main.rs"));
    }

    #[tokio::test]
    async fn copy_wildcard_copies_every_match_into_a_directory() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(ctx.path().join("src")).unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY src/*.rs /opt/app/\n",
        )
        .unwrap();
        std::fs::write(ctx.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(ctx.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        std::fs::write(ctx.path().join("src/notes.txt"), "not rust\n").unwrap();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2-glob:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/opt/app/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("rootfs/opt/app/lib.rs")).unwrap(),
            "pub fn f() {}\n"
        );
        assert!(
            !dir.join("rootfs/opt/app/notes.txt").exists(),
            "the wildcard must only match *.rs"
        );
    }

    // A wildcard that matches nothing is an error, not a silent no-op.
    #[tokio::test]
    async fn copy_wildcard_with_no_match_is_an_error() {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(ctx.path().join("src")).unwrap();
        std::fs::write(
            ctx.path().join("Dockerfile"),
            "FROM scratch\nCOPY src/*.rs /src/\n",
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();

        let err = build_local_image(
            "c2-glob-miss:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .expect_err("a wildcard matching nothing must fail the build");
        assert!(
            err.to_string().contains("matched no files"),
            "unexpected error: {err}"
        );
    }

    // ── #35: a tag is not a path component ────────────────────────────────────

    fn colon_free_ctx() -> tempfile::TempDir {
        let ctx = tempfile::tempdir().unwrap();
        std::fs::write(ctx.path().join("Dockerfile"), "FROM scratch\nCOPY . /app\n").unwrap();
        std::fs::write(ctx.path().join("file.txt"), "hi").unwrap();
        ctx
    }

    /// A colon tag used to become a colon directory, which put a colon in every
    /// host path derived from it - and PRoot splits bind arguments on ':', so the
    /// container silently lost `/dev/shm` and the `/proc` stubs.
    #[tokio::test]
    async fn a_colon_tag_builds_under_a_colon_free_store_key() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();

        let dir = build_local_image(
            "c2env:latest",
            None,
            ctx.path(),
            Some(store.path().to_path_buf()),
        )
        .await
        .expect("a colon tag must build, not silently produce an unbindable store");

        assert_eq!(
            dir.file_name().unwrap(),
            "c2env",
            "store key must be the tag with the colon stripped"
        );
        assert!(
            !dir.to_string_lossy().contains(':'),
            "no host path under the store may contain ':': {}",
            dir.display()
        );
        assert!(!store.path().join("c2env:latest").exists());

        // The original tag stays recorded, so the image can still be identified.
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(meta["name"], "c2env:latest");

        // And the binds derived from that directory are expressible: exactly one
        // ':' per argument, which is the separator PRoot splits on.
        let container_dir = dir.parent().unwrap();
        let shm = proot_distro_core::guestdirs::ensure(container_dir).unwrap();
        assert_eq!(shm.parent().unwrap(), container_dir);
        let binds = proot_distro_core::guestdirs::bindings(container_dir);
        assert!(!binds.is_empty(), "the shared-memory bind must exist");
        for b in &binds {
            assert_eq!(
                b.matches(':').count(),
                1,
                "bind '{b}' must be <source>:<target> with no colon in the source"
            );
            assert!(
                b.ends_with(":/dev/shm") || b.ends_with(":/tmp"),
                "unexpected bind: {b}"
            );
        }
        if let Ok(_sys) = proot_distro_core::sysdata::ensure(container_dir) {
            for b in proot_distro_core::sysdata::bindings(container_dir) {
                assert_eq!(b.matches(':').count(), 1, "unexpressible bind: {b}");
            }
        }
    }

    /// Two tags that derive to the same key must not silently overwrite each
    /// other: rebuilding `app:v2` over `app:v1` would destroy an image the user
    /// still refers to by name.
    #[tokio::test]
    async fn a_different_tag_deriving_to_the_same_key_is_refused() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        build_local_image("app:v1", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect("first build");

        let err = build_local_image("app:v2", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect_err("a second tag mapping onto the same store key must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("app:v1"),
            "error must name the existing image: {msg}"
        );
        assert!(msg.contains("would destroy"), "error must say why: {msg}");

        // The first image is untouched.
        let meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(store_path.join("app").join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["name"], "app:v1");
    }

    #[tokio::test]
    async fn rebuilding_the_same_tag_still_replaces_it() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        let first = build_local_image("app:v1", None, ctx.path(), Some(store_path.clone()))
            .await
            .expect("first build");
        let second = build_local_image("app:v1", None, ctx.path(), Some(store_path))
            .await
            .expect("rebuilding the same tag must stay allowed");
        assert_eq!(first, second);
        assert!(second.join("rootfs").is_dir());
    }

    /// The guard for user-configured bind mounts: a colon source cannot be
    /// expressed, so it must be an error rather than a dropped mount. The
    /// store-key fix above cannot cover a path the user chose.
    #[test]
    fn a_bind_source_with_a_colon_is_refused() {
        let bad = proot_distro_core::bindings::BindMount {
            source: "/home/u/.podnext/containers/c2env:latest/shm".to_string(),
            target: "/dev/shm".to_string(),
            readonly: false,
            recursive: false,
            private: false,
        };
        let err = bad
            .to_argument()
            .expect_err("a colon source must be refused, not silently dropped");
        assert!(err.to_string().contains("c2env:latest"));

        let good = proot_distro_core::bindings::BindMount {
            source: "/srv/data".to_string(),
            target: "/data".to_string(),
            readonly: false,
            recursive: false,
            private: false,
        };
        // Measured on proot 5.1.107.101: a `:ro` suffix becomes part of the guest name,
        // so `to_argument` renders plain and refuses the read-only request instead.
        assert_eq!(good.to_argument().unwrap(), "/srv/data:/data");

        let read_only = proot_distro_core::bindings::BindMount {
            source: "/srv/data".to_string(),
            target: "/data".to_string(),
            readonly: true,
            recursive: false,
            private: false,
        };
        let err = read_only
            .to_argument()
            .expect_err("a read-only bind must be refused, not mounted writable elsewhere");
        assert!(err.to_string().contains("read-only bind"), "{err}");
    }

    #[tokio::test]
    async fn build_local_metadata_splits_image_and_tag() {
        let ctx = colon_free_ctx();
        let store = tempfile::tempdir().unwrap();
        let store_path = store.path().to_path_buf();

        let dir = build_local_image("mycustom/repo:v1.2", None, ctx.path(), Some(store_path))
            .await
            .expect("build image");

        let meta_file = dir.join("metadata.json");
        assert!(meta_file.exists());
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(meta_file).unwrap()).unwrap();
        assert_eq!(meta["name"], "mycustom/repo:v1.2");
        assert_eq!(meta["image"], "mycustom/repo");
        assert_eq!(meta["tag"], "v1.2");
        assert_eq!(meta["base_image"], "scratch");
    }
}
