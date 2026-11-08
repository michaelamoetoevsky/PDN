//! Native implementations for podnext
//!
//! This module provides native implementations of all podnext commands
//! using the Rust crates directly instead of wrapping proot-distro.
//!
//! Split by subsystem: [`image`] holds the OCI/registry install path, [`meta`]
//! the container/session listing and mount helpers, [`session`] the run/login
//! path and the detached-session supervisor.

/// Trace-only logging, off unless `PN_DEBUG` is set in the environment.
///
/// These lines were unconditional upstream and flooded real output — a single
/// `backup create` emitted ~280 kB of DEBUG for one alpine container, and every
/// command printed a DEBUG banner, burying the actual result. Enable with
/// `PN_DEBUG=1` when diagnosing.
macro_rules! pn_trace {
    ($($arg:tt)*) => {
        if ::std::env::var_os("PN_DEBUG").is_some() {
            ::std::eprintln!($($arg)*);
        }
    };
}

mod digest;
mod image;
mod meta;
mod retry;
mod session;

pub use image::*;
pub use meta::*;
pub use session::*;

// Re-exports for this module's tests (below), which name these by `super::`.
#[cfg(test)]
pub(crate) use image::smoke_test_rootfs;
#[cfg(test)]
pub(crate) use meta::apply_image_runtime_config;
#[cfg(test)]
pub(crate) use session::{direct_child_pid, forward_to_guest};
#[cfg(test)]
use std::path::PathBuf;

/// `podnext run` must behave as the image was built: the ENV/WORKDIR recorded in
/// the image's `metadata.json` must reach the container's runtime config. These
/// exercise the loader gap directly (offline, no proot).
#[cfg(test)]
mod mount_spec_tests {
    use super::{parse_mount_spec, parse_mount_specs};

    fn scratch(label: &str) -> std::path::PathBuf {
        let base = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("PREFIX").map(|p| std::path::PathBuf::from(p).join("tmp")))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("pdn-mountspec-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_plain_spec_renders_the_only_form_proot_understands() {
        let host = scratch("rw");
        let m = parse_mount_spec(&format!("{}:/probe", host.display())).expect("parses");
        assert_eq!(m.source, host.display().to_string());
        assert_eq!(m.target, "/probe");
        assert!(!m.readonly);
        // Measured: proot 5.1.107.101 splits the argument once, so a suffix would become
        // part of the guest name. The rendered form must be exactly `<host>:<target>`.
        assert_eq!(
            m.to_argument().expect("expressible"),
            format!("{}:/probe", host.display())
        );
        let _ = std::fs::remove_dir_all(&host);
    }

    #[test]
    fn the_ro_suffix_is_refused_because_proot_cannot_express_it() {
        let host = scratch("ro");
        let err = parse_mount_spec(&format!("{}:/probe:ro", host.display()))
            .expect_err("must refuse instead of mounting at '/probe:ro'")
            .to_string();
        assert!(err.contains("read-only mounts are not supported"), "{err}");
        assert!(
            err.contains("/probe:ro"),
            "the reason names where it would land: {err}"
        );
        let _ = std::fs::remove_dir_all(&host);
    }

    #[test]
    fn a_spec_without_a_colon_names_the_expected_shape() {
        let err = parse_mount_spec("/tmp")
            .expect_err("must refuse")
            .to_string();
        assert!(err.contains("expected HOST:GUEST"), "{err}");
    }

    #[test]
    fn a_relative_guest_path_is_refused() {
        let host = scratch("rel");
        let err = parse_mount_spec(&format!("{}:relative", host.display()))
            .expect_err("must refuse")
            .to_string();
        assert!(err.contains("absolute"), "{err}");
        let _ = std::fs::remove_dir_all(&host);
    }

    #[test]
    fn a_missing_source_is_refused() {
        let err = parse_mount_spec("/definitely/not/here:/probe")
            .expect_err("must refuse")
            .to_string();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn one_bad_spec_fails_the_whole_list() {
        let host = scratch("list");
        let ok = format!("{}:/probe", host.display());
        assert!(parse_mount_specs(&[ok.clone(), ok.clone()]).is_ok());
        assert!(parse_mount_specs(&[ok, "garbage".to_string()]).is_err());
        let _ = std::fs::remove_dir_all(&host);
    }
}

#[cfg(test)]
mod image_runtime_config_tests {
    use super::*;
    use proot_distro_core::{Container, ContainerConfig};

    fn container_at(rootfs: PathBuf) -> Container {
        let config = ContainerConfig {
            name: "img:latest".to_string(),
            rootfs_path: Some(rootfs),
            ..Default::default()
        };
        Container::new("img:latest".to_string(), config).expect("container")
    }

    fn scratch_dir(metadata: &str) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();
        std::fs::write(tmp.path().join("metadata.json"), metadata).unwrap();
        (tmp, rootfs)
    }

    #[test]
    fn run_reinstates_env_and_working_dir_from_metadata() {
        let (_tmp, rootfs) = scratch_dir(
            r#"{"name":"img:latest","env":{"GREETING":"hello-from-image","APP_MODE":"prod"},"working_dir":"/srv/app"}"#,
        );
        let mut c = container_at(rootfs);
        assert!(
            c.config.env_vars.is_empty() && c.config.working_dir.is_none(),
            "the loader must leave these unset, which is the bug"
        );

        apply_image_runtime_config(&mut c);

        assert_eq!(
            c.config.env_vars,
            vec![
                ("APP_MODE".to_string(), "prod".to_string()),
                ("GREETING".to_string(), "hello-from-image".to_string()),
            ]
        );
        assert_eq!(c.config.working_dir.as_deref(), Some("/srv/app"));
    }

    #[test]
    fn explicit_config_is_not_clobbered_by_metadata() {
        let (_tmp, rootfs) =
            scratch_dir(r#"{"env":{"FROM_IMAGE":"1"},"working_dir":"/from/image"}"#);
        let mut c = container_at(rootfs);
        c.config.env_vars = vec![("EXPLICIT".to_string(), "1".to_string())];
        c.config.working_dir = Some("/explicit".to_string());

        apply_image_runtime_config(&mut c);

        assert_eq!(
            c.config.env_vars,
            vec![("EXPLICIT".to_string(), "1".to_string())]
        );
        assert_eq!(c.config.working_dir.as_deref(), Some("/explicit"));
    }

    #[test]
    fn missing_metadata_is_a_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();
        let mut c = container_at(rootfs);

        apply_image_runtime_config(&mut c);

        assert!(c.config.env_vars.is_empty());
        assert!(c.config.working_dir.is_none());
    }
}

#[cfg(all(test, unix))]
mod signal_tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    /// Poll `f` until it is true or `deadline` passes; returns whether it became
    /// true. Asserts on the observable state change, not on a fixed sleep.
    fn eventually(deadline: std::time::Duration, mut f: impl FnMut() -> bool) -> bool {
        let end = std::time::Instant::now() + deadline;
        loop {
            if f() {
                return true;
            }
            if std::time::Instant::now() >= end {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The run path discovers the proot child from /proc and signals it; the
    /// guest must actually die. The child is spawned with `process_group(0)`,
    /// the same way proot-core will.
    #[test]
    fn forward_to_guest_terminates_child_group() {
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 60")
            .process_group(0)
            .spawn()
            .expect("spawn child");
        let pid = child.id() as i32;

        // Observable: the child becomes visible to the run path's discovery.
        assert!(
            eventually(std::time::Duration::from_secs(5), || direct_child_pid()
                == Some(pid)),
            "child pid {pid} never showed up as this process's direct child"
        );

        forward_to_guest(libc::SIGTERM);

        // Observable: the child is reaped (gone), not a time elapsed.
        assert!(
            eventually(std::time::Duration::from_secs(5), || child
                .try_wait()
                .expect("try_wait")
                .is_some()),
            "child survived SIGTERM forwarded to its process group"
        );
    }

    #[tokio::test]
    async fn smoke_test_rootfs_skips_when_no_sh() {
        let tmp = tempfile::tempdir().unwrap();
        let rootfs = tmp.path().join("rootfs");
        std::fs::create_dir_all(&rootfs).unwrap();
        assert!(smoke_test_rootfs(&rootfs).await.is_ok());
    }
}

/// The `--engine` wiring: which runner a choice selects, and that a native path refusal is a NAMED
/// error and never a fallback. These are the assertions that go red if `--engine native` were made
/// to fall through to the proot path, or a refusal were made silent.
#[cfg(test)]
mod engine_wiring_tests {
    use super::{native_verdict, uses_native_engine};
    use crate::EngineChoice;
    use proot_core::native_run::{NativeRunReport, Refusal};

    fn refusal(number: i64, name: &'static str, reason: &'static str) -> Refusal {
        Refusal {
            number,
            name: Some(name),
            reason,
        }
    }

    fn report(refusals: Vec<Refusal>, exit_code: i32) -> NativeRunReport {
        NativeRunReport {
            exit_code,
            stops: refusals.len(),
            entries: refusals.len(),
            translations: Vec::new(),
            refusals,
            observed: Vec::new(),
            child_view: None,
            executed: None,
            strict_failure: None,
        }
    }

    /// The `--engine` choice is what selects the runner. A named assertion about WHICH engine ran:
    /// `native` must route to the in-tree engine, `proot` (and the default) must not. Making
    /// `native` fall through to the proot path turns this red with this message.
    #[test]
    fn native_routes_to_the_native_engine_and_proot_does_not() {
        assert!(
            uses_native_engine(EngineChoice::Native),
            "`--engine native` must route to the native runner, not the proot path"
        );
        assert!(
            !uses_native_engine(EngineChoice::Proot),
            "`--engine proot` (and the default) must stay on the proot path"
        );
    }

    /// A path syscall the engine could not translate is a NAMED, hard error: the guest path would
    /// have been resolved on the HOST, and the engine must NOT fall back to proot. `SYS_chdir` is a
    /// path-taking syscall with no translator (`native_run::unhandled_path_syscall`), the cheapest
    /// such case - the same one the engine's own test names.
    #[test]
    fn an_untranslated_path_is_a_named_error_and_not_a_fallback_to_proot() {
        let report = report(
            vec![refusal(
                libc::SYS_chdir as i64,
                "SYS_chdir",
                "takes a path but this engine has no translation for it; the kernel resolves it \
                 on the HOST filesystem",
            )],
            0,
        );
        let err = native_verdict(&report)
            .expect_err("an untranslated path must fail the run")
            .to_string();
        assert!(
            err.contains("SYS_chdir"),
            "the refusal must name the syscall: {err}"
        );
        assert!(
            err.contains("does NOT fall back to proot"),
            "the refusal must say there is no fallback: {err}"
        );
    }

    /// A pass-through (mmap, brk, ...) is recorded but does not fail the run: letting the kernel do
    /// that work is the engine's design, and it is the only mode a dynamic guest survives.
    #[test]
    fn a_pass_through_refusal_does_not_fail_the_run() {
        let report = report(
            vec![refusal(
                libc::SYS_mmap as i64,
                "SYS_mmap",
                "passed through, not emulated",
            )],
            0,
        );
        assert_eq!(native_verdict(&report).unwrap(), 0);
    }

    /// The guest's own non-zero status is passed through as the run's status.
    #[test]
    fn a_guests_own_exit_code_is_passed_through() {
        assert_eq!(native_verdict(&report(Vec::new(), 7)).unwrap(), 7);
    }
}
