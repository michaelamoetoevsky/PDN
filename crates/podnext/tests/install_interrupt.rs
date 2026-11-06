//! Install/remove lifecycle (WORKLIST A3b, A4b, B12).
//!
//! Four measured properties of the install path:
//!
//! (a) an install killed with `SIGKILL` leaves a staging directory that the next
//!     install of the same name reclaims, so no `.tmp-install-*` residue survives;
//! (b) `remove --force` clears staging residue too, not only the committed
//!     container;
//! (c) a killed install leaves no orphan `podnext` process and no half-written
//!     state - counted before and after, with the process census taken by program
//!     identity (`ps -eo comm=`), never a command-line grep;
//! (d) a concurrent same-name install can never destroy a committed container:
//!     exactly one install wins and the survivor is a whole rootfs.
//!
//! Every test runs against a private store (`HOME`/`PN_TEST_ROOT` point at a
//! scratch directory), so the operator's real `~/.podnext` is never touched; that
//! the real store is unchanged is asserted at the end of each test.
//!
//! All four share one process, so a global lock serialises them: the process
//! census in (c) and the store scan in (d) are meaningless if another test in
//! this file is mid-install.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::time::Duration;

static SERIAL: Mutex<()> = Mutex::new(());

/// A small, reliably published image; the same one the WORKLIST evidence used.
const IMAGE: &str = "alpine:latest";

/// Long enough that the staging tree exists and layers are still downloading
/// (the measured evidence kills at 2-5 s and always finds the residue).
const KILL_AFTER: Duration = Duration::from_secs(3);

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn bin() -> String {
    // The workspace shares one CARGO_TARGET_DIR across worktrees, so a sibling's
    // build can silently replace `target/debug/podnext`. `PODNEXT_BIN` lets the
    // run pin the binary to a copy provenanced by a grep of an added string; the
    // default stays the Cargo-built binary.
    if let Ok(pinned) = std::env::var("PODNEXT_BIN") {
        if !pinned.is_empty() {
            return pinned;
        }
    }
    env!("CARGO_BIN_EXE_podnext").to_string()
}

/// Writable scratch base: `/tmp` is not writable for an app uid on Android.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("pdn-install-interrupt-")
            .tempdir_in(temp_base())
            .expect("scratch store");
        Sandbox { dir }
    }

    fn home(&self) -> &Path {
        self.dir.path()
    }

    fn containers(&self) -> PathBuf {
        self.home().join(".podnext").join("containers")
    }

    /// A `podnext` process whose whole store lives in this sandbox.
    fn cmd(&self) -> Command {
        let mut c = Command::new(bin());
        c.env("HOME", self.home())
            .env("PN_TEST_ROOT", self.home())
            .stdin(Stdio::null());
        c
    }
}

fn install(sb: &Sandbox, name: &str, extra: &[&str]) -> Output {
    let mut c = sb.cmd();
    c.arg("install").arg(IMAGE).arg("--name").arg(name);
    c.args(extra);
    c.output().expect("podnext install")
}

fn spawn_install(sb: &Sandbox, name: &str, capture: bool) -> Child {
    let mut c = sb.cmd();
    c.arg("install").arg(IMAGE).arg("--name").arg(name);
    if capture {
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        c.stdout(Stdio::null()).stderr(Stdio::null());
    }
    c.spawn().expect("spawn podnext install")
}

/// A `--force` reinstall, which is the only install that stages while a
/// committed container of the same name is on disk.
fn spawn_force_install(sb: &Sandbox, name: &str) -> Child {
    let mut c = sb.cmd();
    c.arg("install")
        .arg(IMAGE)
        .arg("--name")
        .arg(name)
        .arg("--force")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c.spawn().expect("spawn podnext install --force")
}

fn force_remove(sb: &Sandbox, name: &str) -> Output {
    let mut c = sb.cmd();
    c.arg("remove").arg(name).arg("--force");
    c.output().expect("podnext remove")
}

/// `Child::kill` is `SIGKILL` on Unix: exactly the interruption the WORKLIST
/// measured (exit 137), not a graceful shutdown.
fn kill_after(mut child: Child, after: Duration) -> std::process::ExitStatus {
    std::thread::sleep(after);
    let _ = child.kill();
    child.wait().expect("wait for killed install")
}

/// Staging directories (the racing object) this path leaves under the store.
fn staging_dirs(containers: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(containers)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-install-"))
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Sidecar lock files whose staging tree is gone: the other half of the residue.
fn stray_staging_locks(containers: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(containers)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(".tmp-install-") && n.ends_with(".lock") && e.path().is_file()
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn count_files(root: &Path) -> usize {
    let mut n = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(entry.path()),
                Ok(_) => n += 1,
                Err(_) => {}
            }
        }
    }
    n
}

/// Live `podnext` processes, by program identity (never a command-line grep):
/// `ps -eo comm= | grep -c '^podnext'`.
fn podnext_process_count() -> usize {
    let out = Command::new("sh")
        .arg("-c")
        .arg("ps -eo comm= | grep -c '^podnext'")
        .output()
        .expect("ps census");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

/// Every committed `metadata.json` under the store must parse: a half-written one
/// would not (B12).
fn assert_no_torn_metadata(containers: &Path) {
    for entry in fs::read_dir(containers).into_iter().flatten().flatten() {
        let meta = entry.path().join("metadata.json");
        if meta.is_file() {
            let text = fs::read_to_string(&meta).expect("read metadata.json");
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or_else(|e| panic!("torn metadata.json at {}: {e}", meta.display()));
        }
    }
}

/// Poll until `f` is true or `deadline` passes.
fn wait_until(deadline: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + deadline;
    loop {
        if f() {
            return true;
        }
        if std::time::Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The operator's real store must be byte-for-byte the set it started with.
fn assert_real_store_intact() {
    let home = std::env::var("HOME").expect("HOME for the real store");
    let containers = Path::new(&home).join(".podnext").join("containers");
    let mut names: Vec<String> = fs::read_dir(&containers)
        .expect("the real store must exist")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let expected: Vec<String> = [
        "almalinux",
        "alpine",
        "deb-pdn",
        "deb-renamed",
        "dev-renamed-target",
        "nousresearch",
        "pdn-bench",
        "test-alpine",
        "test-pdn-install",
        "ubuntu",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        names, expected,
        "the operator's real ~/.podnext/containers must be untouched"
    );
}

/// (a) SIGKILL then a fresh install of the same name: residue count before and
/// after, and it must reach zero.
#[test]
fn killed_install_residue_is_reclaimed_by_the_next_install() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let killed = kill_after(spawn_install(&sb, "reapme", false), KILL_AFTER);
    let before = staging_dirs(&containers);
    println!(
        "(a) killed install status={killed:?}; staging dirs after the kill: {} {before:?}",
        before.len()
    );
    assert_eq!(
        before.len(),
        1,
        "a SIGKILLed install must leave exactly one staging directory: {before:?}"
    );

    let out = install(&sb, "reapme", &[]);
    let after = staging_dirs(&containers);
    println!(
        "(a) fresh install rc={:?}; staging dirs after it: {} {after:?}",
        out.status.code(),
        after.len()
    );
    assert!(
        out.status.success(),
        "the fresh install must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        after.len(),
        0,
        "the reaper must delete the residue: {after:?}"
    );
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(
        containers.join("reapme").join("rootfs").is_dir(),
        "the fresh install must be committed"
    );

    let _ = fs::remove_dir_all(containers.join("reapme"));
    assert_real_store_intact();
}

/// (b) `remove --force` must clear staging residue as well as the container.
#[test]
fn remove_force_leaves_zero_staging_residue() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let ok = install(&sb, "removeme", &[]);
    assert!(
        ok.status.success(),
        "setup install must succeed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // A forced reinstall killed mid-flight: the container is still committed, and
    // its staging tree is now residue.
    let killed = kill_after(spawn_force_install(&sb, "removeme"), KILL_AFTER);
    let residue = staging_dirs(&containers);
    println!(
        "(b) killed --force reinstall status={killed:?}; residue before remove: {} {residue:?}",
        residue.len()
    );
    assert_eq!(
        residue.len(),
        1,
        "the killed --force reinstall must leave a staging residue to reclaim: {residue:?}"
    );
    assert!(
        containers.join("removeme").join("rootfs").is_dir(),
        "the committed container must survive the killed reinstall"
    );

    let out = force_remove(&sb, "removeme");
    let after = staging_dirs(&containers);
    println!(
        "(b) remove --force rc={:?}; residue after remove: {} {after:?}; container gone: {}",
        out.status.code(),
        after.len(),
        !containers.join("removeme").exists()
    );
    assert!(
        out.status.success(),
        "remove --force must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(after.len(), 0, "remove must clear the residue: {after:?}");
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(
        !containers.join("removeme").exists(),
        "the container must be gone"
    );

    assert_real_store_intact();
}

/// (c) A killed install leaves no orphan `podnext` process and no half-written
/// state; both counted before and after.
#[test]
fn killed_install_leaves_no_orphan_and_no_half_written_state() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let before = podnext_process_count();
    let killed = kill_after(spawn_install(&sb, "killme", false), KILL_AFTER);
    let after = podnext_process_count();
    println!("(c) killed status={killed:?}; podnext processes before={before} after={after}");
    assert_eq!(
        after, before,
        "a killed install must leave no orphan podnext process (before={before} after={after})"
    );

    // No torn committed state: nothing under the store may be a half-written
    // metadata.json, and no temp write may be left behind.
    assert_no_torn_metadata(&containers);
    let tmp_writes: Vec<String> = fs::read_dir(&containers)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("metadata.json.tmp")
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        tmp_writes.is_empty(),
        "half-written state left behind: {tmp_writes:?}"
    );

    // The killed run left a half-written staging tree. A fresh install of the same
    // name must be able to reclaim it and commit a whole container: that is the
    // "no half-written state" half of the property, counted before and after.
    let residue = staging_dirs(&containers);
    println!(
        "(c) half-written staging trees left by the kill: {} {residue:?}",
        residue.len()
    );
    assert_eq!(
        residue.len(),
        1,
        "the kill must leave exactly the half-written staging tree: {residue:?}"
    );

    let ok = install(&sb, "killme", &[]);
    assert!(
        ok.status.success(),
        "the fresh install must succeed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let after_residue = staging_dirs(&containers);
    println!(
        "(c) fresh install rc={:?}; staging residue after it: {} {after_residue:?}",
        ok.status.code(),
        after_residue.len()
    );
    assert_eq!(
        after_residue.len(),
        0,
        "the half-written state must be reclaimed: {after_residue:?}"
    );
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(containers.join("killme").join("rootfs").is_dir());

    let _ = force_remove(&sb, "killme");
    assert_real_store_intact();
}

/// (d) Two concurrent same-name installs: exactly one wins, and the survivor is a
/// whole rootfs (file count equal to a solo install of the same image).
#[test]
fn concurrent_same_name_install_never_destroys_the_survivor() {
    let _guard = serial();

    let solo = Sandbox::new();
    let solo_out = install(&solo, "solo", &[]);
    assert!(
        solo_out.status.success(),
        "solo install must succeed: {}",
        String::from_utf8_lossy(&solo_out.stderr)
    );
    let solo_files = count_files(&solo.containers().join("solo").join("rootfs"));
    println!("(d) solo install rootfs file count: {solo_files}");
    assert!(
        solo_files > 0,
        "a solo install must produce a non-empty rootfs"
    );

    let race = Sandbox::new();
    let containers = race.containers();
    let a = spawn_install(&race, "race", true);
    let b = spawn_install(&race, "race", true);
    let out_a = a.wait_with_output().expect("first install");
    let out_b = b.wait_with_output().expect("second install");

    let successes = [&out_a, &out_b]
        .iter()
        .filter(|o| o.status.success())
        .count();
    println!(
        "(d) race rc: {:?} / {:?}; successes={successes}",
        out_a.status.code(),
        out_b.status.code()
    );
    assert_eq!(
        successes,
        1,
        "exactly one concurrent same-name install must win: \
         stdout_a={:?} stdout_b={:?}",
        String::from_utf8_lossy(&out_a.stdout),
        String::from_utf8_lossy(&out_b.stdout)
    );

    let survivor_files = count_files(&containers.join("race").join("rootfs"));
    println!("(d) race survivor rootfs file count: {survivor_files}");
    assert_eq!(
        survivor_files, solo_files,
        "the surviving container must be as whole as a solo install"
    );
    assert_eq!(
        staging_dirs(&containers).len(),
        0,
        "no staging residue after the race"
    );
    assert_eq!(stray_staging_locks(&containers).len(), 0);

    let _ = fs::remove_dir_all(containers.join("race"));
    assert_real_store_intact();
}

/// The reaper must never take a staging tree a live install owns, or a concurrent
/// install would be corrupted the moment another install starts.
#[test]
fn a_live_installs_staging_tree_survives_another_installs_reaper() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let live = spawn_install(&sb, "live", true);
    // The sidecar lock is taken before the tree is created, so once the tree
    // exists its owner is holding the lock.
    assert!(
        wait_until(Duration::from_secs(30), || staging_dirs(&containers).len()
            == 1),
        "the live install never created its staging tree"
    );
    // A file that only this test writes: it survives only if the staging tree is
    // the very same directory, so its absence proves the tree was reaped (and
    // recreated by the install itself).
    let live_staging = containers.join(staging_dirs(&containers)[0].clone());
    let sentinel = live_staging.join("pdn-liveness-sentinel");
    fs::write(&sentinel, b"live").expect("write sentinel");

    // A second, unrelated install runs its reaper at start while the first is
    // still downloading.
    let other = spawn_install(&sb, "other", true);
    assert!(
        wait_until(Duration::from_secs(20), || staging_dirs(&containers)
            .iter()
            .any(|d| d.contains("-other-"))),
        "the second install never staged its tree"
    );
    assert!(
        sentinel.exists(),
        "the second install's reaper deleted the live install's staging tree"
    );

    let live_out = live.wait_with_output().expect("live install");
    let other_out = other.wait_with_output().expect("other install");
    println!(
        "(e) rc live={:?} other={:?}; staging residue after both: {}",
        live_out.status.code(),
        other_out.status.code(),
        staging_dirs(&containers).len()
    );
    assert!(
        live_out.status.success(),
        "the live install must still complete: {}",
        String::from_utf8_lossy(&live_out.stderr)
    );
    assert!(
        other_out.status.success(),
        "the second install must succeed: {}",
        String::from_utf8_lossy(&other_out.stderr)
    );
    assert_eq!(staging_dirs(&containers).len(), 0);
    assert_eq!(stray_staging_locks(&containers).len(), 0);

    let _ = force_remove(&sb, "live");
    let _ = force_remove(&sb, "other");
    assert_real_store_intact();
}
