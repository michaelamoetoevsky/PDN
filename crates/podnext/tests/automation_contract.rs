//! The CLI's automation contract: JSON on stdout, notes on stderr, the exit-code
//! taxonomy, and the `--yes` volume path.
//!
//! Every case here spawns the real binary, and several of them mutate state
//! (`remove`, `kill`, `volume create`, `volume rm`). Nothing in this file used to
//! redirect the store, so the binary resolved the operator's real `~/.podnext`
//! from `PN_TEST_ROOT -> HOME` (`crates/proot-distro-core/src/paths.rs:27-53`).
//! The `volume create` / `volume rm` pair in `test_yes_flag_on_volume_rm` wrote
//! one there, and the `kill`/`remove` arms resolved it too. CI is unaffected
//! (runners have no `~/.podnext`), which is exactly why the leak survived locally.
//!
//! Every spawned process is now pinned to a fixture home under `$TMPDIR` /
//! `$PREFIX/tmp`: `HOME`, `PN_TEST_ROOT`, `TERMUX__HOME` and `TMPDIR` all point
//! there, and `XDG_RUNTIME_DIR` is removed (it outranks `HOME` for the daemon
//! socket). Before any mutating arm runs, `assert_binary_honours_store` proves the
//! binary actually resolves that fixture: a read-only `list` must name exactly the
//! fixture's seeded container. The probe points `HOME` at a separate decoy store,
//! so a binary that ignored `PN_TEST_ROOT` would list the decoy and fail the guard
//! rather than quietly rewrite the operator's store.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

/// A writable temp base. `/tmp` is not writable on Android, and
/// `std::env::temp_dir()` falls back to it when `TMPDIR` is unset, so never use it
/// alone. Order: `$TMPDIR` -> `$PREFIX/tmp` -> `std::env::temp_dir()`.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

/// One test's private store, plus a decoy store for the guard probe.
struct Fixture {
    _tmp: tempfile::TempDir,
    /// The fixture home: `<tmp>/home`. The store is `<home>/.podnext`.
    root: PathBuf,
    /// A second home holding a *different* store, used only by the guard probe so
    /// that a dropped `PN_TEST_ROOT` is visible as the decoy's listing.
    decoy: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let base = temp_base();
        std::fs::create_dir_all(&base).expect("temp base");
        let tmp = tempfile::Builder::new()
            .prefix(&format!("pdn-auto-{label}-"))
            .tempdir_in(&base)
            .expect("fixture tempdir");

        let root = tmp.path().join("home");
        let decoy = tmp.path().join("decoy");
        std::fs::create_dir_all(root.join(".podnext").join("containers"))
            .expect("fixture containers");
        std::fs::create_dir_all(root.join(".podnext").join("sessions")).expect("fixture sessions");
        std::fs::create_dir_all(root.join("tmp")).expect("fixture tmp");
        // The decoy is a *different* store: one container the fixture does not have.
        let decoy_container = decoy
            .join(".podnext")
            .join("containers")
            .join("pdn-decoy-9666");
        std::fs::create_dir_all(decoy_container.join("rootfs")).expect("decoy rootfs");
        std::fs::write(
            decoy_container.join("metadata.json"),
            r#"{"name":"pdn-decoy-9666","image":"alpine","rootfs":"rootfs"}"#,
        )
        .expect("decoy metadata");

        Self {
            _tmp: tmp,
            root,
            decoy,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn store(&self) -> PathBuf {
        self.root.join(".podnext")
    }

    /// `podnext <args>` with every store lever pointed at `home`, except that
    /// `PN_TEST_ROOT` is always the fixture. The guard passes `home = decoy` so a
    /// binary that ignores `PN_TEST_ROOT` falls through to the decoy instead of the
    /// operator's store.
    fn command_in(&self, home: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(bin());
        c.args(args)
            .env("HOME", home)
            .env("PN_TEST_ROOT", &self.root)
            .env("TERMUX__HOME", home)
            .env("TMPDIR", self.root.join("tmp"))
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("PN_DEBUG")
            .env_remove("PN_GATEWAY_CONFIG");
        c
    }

    /// Run against the fixture home.
    fn run(&self, args: &[&str]) -> Output {
        self.command_in(&self.root, args)
            .output()
            .unwrap_or_else(|e| panic!("spawn podnext {args:?}: {e}"))
    }
}

/// Container names from `podnext list -f json`, sorted.
fn container_names(stdout: &[u8]) -> Vec<String> {
    let parsed: serde_json::Value = serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "`list -f json` was not JSON ({e}): {}",
            String::from_utf8_lossy(stdout)
        )
    });
    let mut names: Vec<String> = parsed
        .as_array()
        .expect("`list -f json` must be a JSON array")
        .iter()
        .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    names.sort();
    names
}

/// Prove the binary resolves the fixture store BEFORE any mutating arm runs.
///
/// Read-only `list`: what it returns must be exactly the fixture's seeded
/// container. The probe points `HOME` at the decoy, so a binary that ignored
/// `PN_TEST_ROOT` would list `pdn-decoy-9666` and this assertion would name the
/// store it actually resolved - instead of letting `remove`/`kill`/`volume rm`
/// rewrite the operator's `~/.podnext`.
fn assert_binary_honours_store(fixture: &Fixture) {
    let sentinel = fixture
        .store()
        .join("containers")
        .join("pdn-guard-sentinel");
    std::fs::create_dir_all(sentinel.join("rootfs")).expect("guard sentinel rootfs");
    std::fs::write(
        sentinel.join("metadata.json"),
        r#"{"name":"pdn-guard-sentinel","image":"alpine","rootfs":"rootfs"}"#,
    )
    .expect("guard sentinel metadata");

    let expected = vec!["pdn-guard-sentinel".to_string()];
    let out = fixture
        .command_in(&fixture.decoy, &["list", "-f", "json"])
        .output()
        .expect("guard: run `podnext list`");

    let seen = container_names(&out.stdout);
    assert_eq!(
        seen,
        expected,
        "fixture guard FAILED: `podnext list` did not resolve PN_TEST_ROOT={}. \
         It listed {:?}, i.e. it resolved HOME={} instead. A binary that ignores \
         PN_TEST_ROOT acts on the real `~/.podnext`, so this suite refuses.",
        fixture.root().display(),
        seen,
        fixture.decoy.display()
    );
    eprintln!(
        "fixture guard OK: PN_TEST_ROOT={} listed exactly {:?}; decoy HOME={} was not consulted",
        fixture.root().display(),
        seen,
        fixture.decoy.display()
    );
}

#[test]
fn test_sessions_json_output_clean() {
    let fx = Fixture::new("sessions");
    assert_binary_honours_store(&fx);

    let output = fx.run(&["sessions", "-f", "json"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout must be strictly parseable JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout was not valid json: {}\nerror: {}", stdout, e));
    assert!(parsed.is_array());

    // The human note must be on stderr, not stdout, and it must describe what `attach` actually
    // does: reconnect to a session started with `run --detach`. It never creates one, so the note
    // must not claim otherwise.
    assert!(!stdout.contains("podnext attach"));
    assert!(
        stderr.contains("podnext attach") && stderr.contains("reconnect"),
        "the sessions note belongs on stderr and must say attach reconnects; stderr was: {stderr:?}"
    );
    assert!(
        !stderr.contains("create a new session"),
        "the sessions note must not tell the caller attach creates a session; stderr was: {stderr:?}"
    );
}

#[test]
fn test_gitops_status_json_output_clean() {
    let fx = Fixture::new("gitops");
    assert_binary_honours_store(&fx);

    let output = fx.run(&["gitops", "status", "-f", "json"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout must be strictly parseable JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout was not valid json: {}\nerror: {}", stdout, e));
    assert!(parsed.is_array());
    assert!(!stdout.contains("No applications found"));
}

#[test]
fn test_non_tty_stdin_refused_without_hang() {
    use std::process::Stdio;

    let fx = Fixture::new("nontty");
    assert_binary_honours_store(&fx);

    let mut child = fx
        .command_in(fx.root(), &["remove", "nonexistent_target_12345"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn podnext");

    let status = child.wait().expect("failed to wait on child");
    assert_eq!(status.code(), Some(1));
}

#[test]
fn test_unknown_targets_exit_1() {
    let fx = Fixture::new("unknown");
    assert_binary_honours_store(&fx);

    for args in [
        vec!["kill", "nosuch_container_99999"],
        vec!["service", "logs", "nosuch_service_99999"],
        vec!["quota", "set", "nosuch_service_99999", "memory", "512M"],
        vec!["boot", "enable", "nosuch_service_99999"],
        vec!["remove", "nosuch_container_99999"],
        vec!["volume", "rm", "nosuch_volume_99999"],
        vec!["image", "rm", "nosuch_image_99999"],
    ] {
        let output = fx.run(&args);

        assert_eq!(
            output.status.code(),
            Some(1),
            "expected exit 1 for args {:?}, got {:?}",
            args,
            output.status.code()
        );
    }
}

#[test]
fn test_exit_code_unification() {
    let fx = Fixture::new("exitcodes");
    assert_binary_honours_store(&fx);

    // 0: success
    let ok = fx.run(&["--version"]);
    assert_eq!(ok.status.code(), Some(0));

    // 1: application / entity not found
    let not_found = fx.run(&["kill", "nonexistent_container"]);
    assert_eq!(not_found.status.code(), Some(1));

    // 2: clap usage error
    let clap_err = fx.run(&["list", "--bogus-flag-12345"]);
    assert_eq!(clap_err.status.code(), Some(2));

    // 2: an unloadable configuration file is the same usage-error class as a missing argument.
    // There is no separate "config error" code: the taxonomy (roadmap section 9.1, asserted in
    // tests/exit_codes.rs) is 0 success / 1 documented refusal or domain error / 2 usage error,
    // plus the guest's own status. cli-reference.md documents 2 for an invalid file and
    // tests/cli_reference_contract.rs forbids documenting any code above 2.
    let tmp = tempfile::Builder::new()
        .tempfile_in(fx.root().join("tmp"))
        .expect("temp config file");
    std::fs::write(tmp.path(), "invalid_toml = = =").unwrap();
    let config_err = fx.run(&["config", "validate", &tmp.path().to_string_lossy()]);
    assert_eq!(config_err.status.code(), Some(2));
}

#[test]
fn test_yes_flag_on_volume_rm() {
    // The mutating arm. The guard runs first and refuses if the binary would
    // resolve anywhere but the fixture, and the volume's mountpoint is asserted
    // inside the fixture store - so this proves the write landed on the fixture.
    let fx = Fixture::new("volrm");
    assert_binary_honours_store(&fx);

    let vol_name = "test_yes_vol_auto";
    let created = fx.run(&["volume", "create", vol_name]);
    assert_eq!(
        created.status.code(),
        Some(0),
        "volume create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let mountpoint = fx.store().join("volumes").join(vol_name);
    assert!(
        mountpoint.is_dir(),
        "`volume create` did not land in the fixture store: {} is missing",
        mountpoint.display()
    );
    eprintln!(
        "volume landed in the fixture store: {}",
        mountpoint.display()
    );

    // Remove with --yes
    let rm = fx.run(&["volume", "rm", vol_name, "--yes"]);
    assert_eq!(rm.status.code(), Some(0));
    assert!(
        !mountpoint.exists(),
        "`volume rm --yes` left {} behind",
        mountpoint.display()
    );
    eprintln!(
        "volume rm removed the fixture mountpoint: {}",
        mountpoint.display()
    );
}
