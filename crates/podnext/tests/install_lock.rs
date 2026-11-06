//! WORKLIST A4, end to end: two concurrent installs of ONE name cannot both run.
//!
//! Measured before this (POC-EXTREME-TESTING.md F9, WORKLIST A4): both processes printed
//! `Container '<name>' installed successfully!` and `~/.podnext/locks/` stayed EMPTY through every
//! race, because the lock primitive existed in `proot_distro_core::locking` and nothing on the install
//! path ever took it. This asserts the CLI contract: exactly one install succeeds, the other is refused
//! BEFORE doing any work, and the refusal names the holder's PID - which is the difference between a
//! dead end and a diagnosis.
//!
//! The store lives in a scratch directory (`HOME` and `PN_TEST_ROOT` both point at it), so the
//! operator's real `~/.podnext` is untouched. It asserts its own sandbox condition at the end.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const IMAGE: &str = "alpine:latest";
const NAME: &str = "a4race";

fn bin() -> String {
    // One shared CARGO_TARGET_DIR across worktrees means a sibling's build can silently replace
    // `target/debug/podnext`; PODNEXT_BIN pins the exercised binary to a provenanced copy.
    if let Ok(pinned) = std::env::var("PODNEXT_BIN") {
        if !pinned.is_empty() {
            return pinned;
        }
    }
    env!("CARGO_BIN_EXE_podnext").to_string()
}

/// `/tmp` is not writable for an app uid on Android.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

fn cmd(home: &Path) -> Command {
    let mut c = Command::new(bin());
    c.env("HOME", home)
        .env("PN_TEST_ROOT", home)
        .stdin(Stdio::null());
    c
}

fn staging_dirs(containers: &Path) -> Vec<String> {
    fs::read_dir(containers)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(".tmp-install-") && e.path().is_dir()
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn concurrent_same_name_install_is_refused_with_the_holder_named() {
    let dir = tempfile::Builder::new()
        .prefix("pdn-install-lock-")
        .tempdir_in(temp_base())
        .expect("scratch store");
    let home = dir.path();
    let locks = home.join(".podnext").join("locks");
    let containers = home.join(".podnext").join("containers");

    // Racer A: takes the name lock for the whole install (an install spends seconds downloading).
    let mut a_cmd = cmd(home);
    a_cmd
        .args(["install", IMAGE, "--name", NAME])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let a = a_cmd.spawn().expect("spawn install A");
    let a_pid = a.id();
    std::thread::sleep(Duration::from_millis(2000));

    // The lock must exist and must record the holder while it installs.
    let held: Vec<String> = fs::read_dir(&locks)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let lock_body = fs::read_to_string(locks.join(format!("{NAME}.lock"))).unwrap_or_default();

    // Racer B: must be refused, and must not have done any work.
    let b = cmd(home)
        .args(["install", IMAGE, "--name", NAME])
        .output()
        .expect("run install B");

    let a = a.wait_with_output().expect("wait for install A");
    let a_stdout = String::from_utf8_lossy(&a.stdout).into_owned();
    let b_stdout = String::from_utf8_lossy(&b.stdout).into_owned();
    let b_stderr = String::from_utf8_lossy(&b.stderr).into_owned();

    // 1. the holder leaves a lock, and it names the holder.
    assert!(
        !held.is_empty(),
        "the holder must leave a lock file while it installs; {locks:?} was empty"
    );
    assert!(
        lock_body.contains(&a_pid.to_string()),
        "the lock file must record the holder's PID {a_pid}; it held {lock_body:?}"
    );

    // 2. exactly one of the two installs succeeded.
    let successes = u8::from(a.status.success()) + u8::from(b.status.success());
    assert_eq!(
        successes, 1,
        "exactly one concurrent same-name install must win.\nA rc={:?} stdout={a_stdout:?}\nB rc={:?} stdout={b_stdout:?} stderr={b_stderr:?}",
        a.status.code(),
        b.status.code()
    );

    // 3. the refused one did not claim success, and its refusal names the holder.
    let refused_stdout = if a.status.success() {
        &b_stdout
    } else {
        &a_stdout
    };
    let refused_stderr = if a.status.success() {
        &b_stderr
    } else {
        &String::from_utf8_lossy(&a.stderr).into_owned()
    };
    assert!(
        !refused_stdout.contains("installed successfully"),
        "the refused install must not report success: {refused_stdout:?}"
    );
    assert!(
        refused_stderr.contains("held by another process"),
        "the refusal must say the name is held by another process: {refused_stderr:?}"
    );
    assert!(
        refused_stderr.contains(&a_pid.to_string()),
        "the refusal must name the holder's PID {a_pid}: {refused_stderr:?}"
    );

    // 4. the store ends with exactly one whole container and no staging residue.
    let staging = staging_dirs(&containers);
    assert!(
        staging.is_empty(),
        "no staging residue may survive a lock refusal: {staging:?}"
    );
    assert!(
        containers.join(NAME).is_dir(),
        "the winner's container must be committed"
    );
    let _ = fs::remove_dir_all(home);
}
