//! `login --shared-home` / `--termux-home` / `--shared-tmp`: the bind mapping is the REFERENCE's, read
//! from the installed proot-distro package (`commands/login/proot_cmd.py:205-216`):
//!
//! ```text
//! --shared-home / --termux-home -> --bind=<host home>:/root   (the reference's login_uid == "0" branch)
//! --shared-tmp                  -> --bind=<termux prefix>/tmp:/tmp
//! ```
//!
//! Before this, all of them were refused by name with a suggested `-b` command, so `podnext login` could
//! not behave like `proot-distro login` without the caller spelling the binds out. The assertion is on the
//! argv the runtime builds, captured by a decoy `PROOT_BIN`, because that is the same string proot would
//! receive - no guest, no rootfs and no kernel needed, and the no-flag arm is the control that proves the
//! flag is what put the bind there.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

struct Store {
    tmp: tempfile::TempDir,
}

impl Store {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let containers = tmp.path().join(".podnext").join("containers");
        std::fs::create_dir_all(&containers).expect("containers");
        std::fs::create_dir_all(tmp.path().join(".podnext").join("sessions")).expect("sessions");
        // The `--shared-tmp` bind source must exist or the runtime refuses by name (that refusal is what the
        // runner saw as an empty argv), so the fixture provides `<fixture>/usr/tmp`.
        std::fs::create_dir_all(tmp.path().join("usr").join("tmp")).expect("fixture prefix tmp");
        let dir = containers.join("shared");
        std::fs::create_dir_all(dir.join("rootfs")).expect("rootfs");
        std::fs::write(
            dir.join("metadata.json"),
            serde_json::json!({
                "created_at": "2026-11-09T00:00:00Z",
                "image": "debian:13",
                "name": "shared",
                "rootfs": dir.join("rootfs").to_string_lossy(),
                "status": "Stopped",
                "tag": "13",
            })
            .to_string(),
        )
        .expect("metadata");
        Self { tmp }
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().to_path_buf()
    }

    fn env(&self) -> Vec<(String, String)> {
        // The RESOLVERS are `paths::host::home()` = TERMUX__HOME then HOME, and `paths::host::prefix()` =
        // TERMUX__PREFIX then PREFIX. Asserting a host path while the child inherits a different environment is
        // how these two tests passed here (where TERMUX__HOME/TERMUX__PREFIX are set) and failed on the CI
        // runner (`--shared-home` expected `/home/runner`, argv carried the fixture; `--shared-tmp` got no bind
        // at all). Pinning both variables in the child makes the expected value a function of THIS fixture, so
        // it is the same string on every host - and the test still measures the flag, because the no-flag arm
        // asserts the bind's absence.
        let root = self.tmp.path().to_string_lossy().into_owned();
        vec![
            ("PN_TEST_ROOT".to_string(), root.clone()),
            ("HOME".to_string(), root.clone()),
            ("TERMUX__HOME".to_string(), root.clone()),
            ("TERMUX__PREFIX".to_string(), format!("{root}/usr")),
        ]
    }

    /// The argv the runtime would hand to proot, captured by a decoy `PROOT_BIN`.
    fn proot_argv(&self, flags: &[&str]) -> Vec<String> {
        let decoy = self.tmp.path().join("decoy-proot");
        let out = self.tmp.path().join("decoy-argv");
        std::fs::write(
            &decoy,
            format!(
                "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} > {}\nexit 127\n",
                out.display()
            ),
        )
        .expect("decoy");
        let mut perms = std::fs::metadata(&decoy).expect("stat").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&decoy, perms).expect("chmod");

        let mut cmd = Command::new(bin());
        cmd.arg("--engine")
            .arg("proot")
            .arg("login")
            .arg("shared")
            .args(flags)
            .arg("--")
            .arg("/bin/sh");
        for (k, v) in self.env() {
            cmd.env(k, v);
        }
        cmd.env("PROOT_BIN", &decoy);
        cmd.env_remove("LD_LIBRARY_PATH");
        let _ = cmd.output().expect("spawn podnext");

        let text = std::fs::read_to_string(&out).unwrap_or_default();
        text.lines().map(str::to_string).collect()
    }
}

fn binds_ending_with(argv: &[String], target: &str) -> Vec<String> {
    argv.iter()
        .filter(|a| a.starts_with("--bind=") && a.ends_with(target))
        .cloned()
        .collect()
}

#[test]
fn login_binds_the_host_home_at_root_the_way_the_reference_does() {
    let store = Store::new();
    // The source is the SHARED resolver's answer, not this fixture's HOME: the first version of this
    // test asserted the fixture root and the run told it otherwise
    // (`--bind=/data/data/com.termux/files/home:/root`), because `paths::host::home()` prefers
    // TERMUX__HOME over HOME. Asserting the resolver keeps the test honest about the contract - "the
    // flag uses the same resolver as everything else" - without pretending the fixture can move it.
    // Derived from the environment the CHILD is given (see `env()`), not from this process's own.
    let expected = format!("--bind={}:/root", store.home().display());

    for flag in ["--shared-home", "--termux-home"] {
        let argv = store.proot_argv(&[flag]);
        assert!(
            argv.contains(&expected),
            "{flag} must produce {expected}; argv was {:?}",
            binds_ending_with(&argv, ":/root")
        );
    }

    // The control: with no flag the bind is absent, so the assertion above is measuring the flag and
    // not something the runtime always does.
    let plain = store.proot_argv(&[]);
    assert!(
        binds_ending_with(&plain, ":/root").is_empty(),
        "without the flag there must be no /root bind: {:?}",
        binds_ending_with(&plain, ":/root")
    );
}

#[test]
fn login_binds_the_host_tmp_at_tmp_the_way_the_reference_does() {
    let store = Store::new();
    let expected = format!("--bind={}/usr/tmp:/tmp", store.home().display());

    let argv = store.proot_argv(&["--shared-tmp"]);
    assert!(
        argv.contains(&expected),
        "--shared-tmp must produce {expected}; argv was {:?}",
        binds_ending_with(&argv, ":/tmp")
    );

    // The control has to look at the SOURCE, not at the target: the runtime always binds the
    // container's own `tmp` directory at `/tmp` (guestdirs), which is how the first version of this
    // test failed - `without the flag there must be no /tmp bind:
    // [--bind=<store>/containers/shared/tmp:/tmp]`.
    let plain = store.proot_argv(&[]);
    let sources: Vec<String> = binds_ending_with(&plain, ":/tmp");
    assert!(
        !sources.iter().any(|b| b == &expected),
        "without the flag the PREFIX tmp must not be bound: {sources:?}"
    );
    assert!(
        !sources.is_empty(),
        "the container's own tmp is still bound at /tmp, which is what makes the control meaningful"
    );
}
