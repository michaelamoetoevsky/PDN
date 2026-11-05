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
use anyhow::Result;
use clap::{CommandFactory, Parser};
use serde_yaml;
use std::io::{self, Write};
use std::path::PathBuf;
use tabled::{Table, Tabled};
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;
mod direct;
mod dispatch;
mod doctor;
mod native;
mod subcommands;
mod utils;

pub(crate) use cli::*;
pub(crate) use direct::*;
pub(crate) use dispatch::*;
pub(crate) use subcommands::*;

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
        return scan_direct_execution(&raw_args, pn_debug).await;
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

    // A global flag can ask for something PDN cannot deliver, and the honest place to say so is
    // before any work happens: `--seccomp` promises syscall filtering, and this PRoot build applies
    // none. Measured on this device: the generated proot argv is byte-identical with and without the
    // flag (0 seccomp tokens), `proot --help` lists no seccomp option, and the field the flag used to
    // set is never read (`crates/proot-core/src/seccomp.rs` warns `field filter_program is never
    // read`). Refusing here rather than in the engine setup also means the answer does not depend on
    // the container existing, so a caller sees the real reason instead of "container not found"
    // (issue #62).
    if cli.seccomp {
        anyhow::bail!(
            "--seccomp is not implemented: this PRoot build applies no seccomp filter, so the flag \
             would be accepted and never enforced (the generated proot argv is byte-identical with \
             and without it). Drop the flag, or use a runtime that can enforce it."
        );
    }

    // Same class (issue #63), measured the same way: `--no-kill-on-exit` cannot turn off a
    // `--kill-on-exit` that `proot-core` passes unconditionally, so the flag changes nothing.
    if cli.no_kill_on_exit {
        anyhow::bail!(
            "--no-kill-on-exit is not implemented: proot-core passes `--kill-on-exit` to proot \
             unconditionally, so the flag would be accepted and never enforced (the generated proot \
             argv is byte-identical with and without it). Drop the flag."
        );
    }

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
