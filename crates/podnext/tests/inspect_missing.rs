//! Names that do not resolve must fail the command, not report success with empty output.
//!
//! Measured before this fix (WORKLIST A8): `podnext volume inspect <missing>` and
//! `podnext network inspect <missing>` exited 0 with `[]` on stdout, while `podnext image inspect`
//! correctly exited non-zero. This asserts ONE convention across all three: a non-zero exit, no
//! empty JSON array presented as a result, and the missing name reported on stderr.
//!
//! The store is redirected to a temporary EMPTY directory for the duration of the test. PDN resolves
//! its store from TERMUX__HOME/PREFIX in preference to HOME (measured during the extreme test, when a
//! sibling's HOME override did not move the store), so both are set here. Nothing writes to the real
//! store: these commands only read.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

fn isolated_home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdn-inspect-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp home");
    dir
}

fn run(home: &PathBuf, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .env("TERMUX__HOME", home)
        .env("HOME", home)
        .output()
        .expect("run podnext");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn inspect_of_a_missing_object_fails_with_one_convention() {
    let home = isolated_home();
    let cases: [(&str, &[&str]); 3] = [
        ("volume", &["volume", "inspect", "pdn-no-such-volume"]),
        ("network", &["network", "inspect", "pdn-no-such-network"]),
        ("image", &["image", "inspect", "pdn-no-such-image"]),
    ];
    for (what, args) in cases {
        let (code, stdout, stderr) = run(&home, args);
        assert_ne!(
            code, 0,
            "{what} inspect of a missing object must exit non-zero; stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            !stdout.trim().starts_with('['),
            "{what} inspect must not print an empty JSON array as if it were a result: {stdout:?}"
        );
        assert!(
            stderr.contains("no such") || stderr.contains("not found"),
            "{what} inspect must name what was missing on stderr: {stderr:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}
