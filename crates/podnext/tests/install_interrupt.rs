//! Install/remove lifecycle (WORKLIST A3b, A4b, B12).
//!
//! Five measured properties of the install path:
//!
//! (a) staging residue that no process owns is reclaimed by the next install of
//!     the same name, which then commits a whole container;
//! (b) `remove --force` clears staging residue too, not only the committed
//!     container - including for a name that was never installed;
//! (c) an install that was interrupted leaves no orphan `podnext` process, no
//!     torn `metadata.json` and no residue the next install cannot clear; the
//!     census is by program identity (`ps -eo comm=`), never a command-line grep;
//! (d) a concurrent same-name install can never destroy a committed container:
//!     exactly one install wins and the survivor is a whole rootfs;
//! (e) a staging tree whose sidecar lock another process holds is live and
//!     survives a reaper running in a third process; once the lock is released the
//!     same tree is residue and is reaped.
//!
//! Residue is PLANTED, never raced. The old fixture slept a fixed `KILL_AFTER`,
//! killed the install and then asserted that the kill had left a staging
//! directory to reclaim. That window is narrow in BOTH directions: measured on
//! this device, a kill 3 s in leaves ZERO residue (the staging tree does not exist
//! yet - the first seconds go to the manifest), and on a CI runner the whole
//! install finishes before the kill (runs 42187850363 and ac62ada: `the killed
//! --force reinstall must leave a staging residue to reclaim: []` with
//! `status=ExitStatus(unix_wait_status(0))`, then `this host completed it faster
//! than the fixture's window`).
//!
//! So the tests now assert the reaper's own contract instead of a race
//! (`crates/podnext/src/native/image.rs:170-256`): a `.tmp-install-*` directory is
//! residue when its sidecar `<staging>.lock` is absent or can be `flock`ed by the
//! observer, and live when another process holds that lock. The fixture creates
//! that state directly, so every assertion is the same on every host.
//!
//! The one remaining SIGKILL test (c) is driven by an observed event - the staging
//! tree appearing - never a clock, PRINTS which branch it took, and asserts only
//! the post-condition that holds whether or not the kill landed mid-flight. No
//! test requires the kill to have produced residue.
//!
//! Every test runs against a private store (`HOME`/`PN_TEST_ROOT` point at a
//! scratch directory), so the operator's real `~/.podnext` is never touched; that
//! the real store is unchanged is asserted at the end of each test.
//!
//! All five share one process, so a global lock serialises them: the process
//! census in (c) and the store scan in (d) are meaningless if another test in
//! this file is mid-install.

use std::fs;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());

/// The image every real install in this file fetches.
const IMAGE: &str = "debian:stable-slim";

/// How long the interruption fixture in (c) waits for a staging tree to appear
/// before it concludes the host is too fast to interrupt.
///
/// This is NOT a kill delay: the kill fires on the observed event (the staging
/// directory appearing). The window is only the fallback for a host that commits
/// the whole install before the tree is ever seen.
const STAGING_WINDOW: Duration = Duration::from_secs(3);

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

fn force_remove(sb: &Sandbox, name: &str) -> Output {
    let mut c = sb.cmd();
    c.arg("remove").arg(name).arg("--force");
    c.output().expect("podnext remove")
}

/// The sidecar lock path for a staging directory: `<staging dir>.lock`, exactly as
/// `native::image::staging_lock_path` builds it.
fn staging_lock_path(dir: &Path) -> PathBuf {
    let mut name = dir.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

/// A unique-enough `<uuid>` for a planted staging directory, without pulling in a
/// dependency the test does not already have.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// Plant the residue state the reaper's contract describes
/// (`native::image::reap_stale_staging`): a `.tmp-install-<name>-<uuid>` directory
/// whose sidecar `.lock` file exists but is held by NO process, so a reaper that
/// takes the lock finds it free.
///
/// Racing a SIGKILL cannot produce this reliably - measured on this device a 3 s
/// kill leaves zero residue, and a fast runner finishes the install first - so the
/// fixture plants it and the assertion becomes host-independent.
fn plant_residue(containers: &Path, name: &str) -> PathBuf {
    fs::create_dir_all(containers).expect("containers dir");
    let dir = containers.join(format!(".tmp-install-{}-{}", name, unique_suffix()));
    let rootfs = dir.join("rootfs");
    fs::create_dir_all(&rootfs).expect("planted rootfs");
    fs::write(rootfs.join("half-written"), b"partial").expect("planted half-written file");
    fs::write(staging_lock_path(&dir), b"").expect("planted sidecar lock");
    dir
}

/// Which branch the interruption fixture in (c) took.
///
/// Printed so a CI log names the case that ran, and matched on so the test never
/// silently converts "could not interrupt" into a pass.
#[derive(Debug, Clone, Copy)]
enum Interrupt {
    /// The staging tree was observed and the install was `SIGKILL`ed mid-flight.
    Killed(std::process::ExitStatus),
    /// The install exited before its staging tree could be observed, so there was
    /// nothing to interrupt.
    NotInterrupted(std::process::ExitStatus),
}

impl Interrupt {
    fn status(&self) -> std::process::ExitStatus {
        match self {
            Interrupt::Killed(s) | Interrupt::NotInterrupted(s) => s.clone(),
        }
    }

    /// True when the `SIGKILL` really landed mid-install.
    fn was_killed(&self) -> bool {
        matches!(self, Interrupt::Killed(_))
    }

    fn label(&self) -> &'static str {
        match self {
            Interrupt::Killed(_) => "INTERRUPTED (SIGKILL landed mid-install)",
            Interrupt::NotInterrupted(_) => {
                "NOT INTERRUPTED (staging tree never observed in the fixture window; \
                 the host committed the install first)"
            }
        }
    }
}

/// Interrupt an install on the observed event - its staging tree appearing - and
/// never on a clock, so the kill cannot land before the tree exists (measured: a
/// fixed 3 s delay does exactly that on this device) nor after the install has
/// finished (a fast runner).
///
/// A host too fast to ever show the tree has nothing to interrupt; that is
/// reported as [`Interrupt::NotInterrupted`], not as a failure.
fn interrupt_on_staging(child: Child, containers: &Path, window: Duration) -> Interrupt {
    let mut child = child;
    let deadline = Instant::now() + window;
    loop {
        // The install committed and exited before its tree was ever observed.
        if let Ok(Some(status)) = child.try_wait() {
            return Interrupt::NotInterrupted(status);
        }
        if !staging_dirs(containers).is_empty() {
            let _ = child.kill();
            let status = child.wait().expect("wait for killed install");
            // `kill` can lose a race it did not run: if the process exited on its
            // own in the microseconds after `try_wait`, no `SIGKILL` landed (the
            // status carries no signal) and there was nothing to interrupt.
            return match status.signal() {
                Some(_) => Interrupt::Killed(status),
                None => Interrupt::NotInterrupted(status),
            };
        }
        let now = Instant::now();
        if now >= deadline {
            // No tree seen within the window: let the install finish and report the
            // branch instead of failing on a host-dependent precondition.
            return Interrupt::NotInterrupted(
                child.wait().expect("wait for un-interrupted install"),
            );
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(5)));
    }
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

/// Sidecar staging lock files (`<staging dir>.lock`) still on disk: the other half
/// of the residue, and the signal the reaper reads for liveness.
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

/// (a) The reaper reclaims a staging residue that no process owns: the next
/// install of the same name deletes the `.tmp-install-*` tree AND its sidecar
/// `.lock`, then commits a whole container.
#[test]
fn staging_residue_is_reclaimed_by_the_next_install() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let planted = plant_residue(&containers, "reapme");
    println!(
        "(a) planted residue: {} (sidecar lock present, held by nobody)",
        planted.file_name().unwrap().to_string_lossy()
    );
    assert!(
        planted.is_dir(),
        "the fixture must plant a residue directory"
    );
    assert_eq!(staging_dirs(&containers).len(), 1);

    let out = install(&sb, "reapme", &[]);
    let after = staging_dirs(&containers);
    println!(
        "(a) install rc={:?}; staging dirs after it: {} {after:?}; sidecars left: {}",
        out.status.code(),
        after.len(),
        stray_staging_locks(&containers).len()
    );
    assert!(
        out.status.success(),
        "the install must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !planted.exists(),
        "the reaper must delete the planted residue directory"
    );
    assert!(
        !staging_lock_path(&planted).exists(),
        "the reaper must delete the residue's sidecar lock too"
    );
    assert_eq!(
        after.len(),
        0,
        "no staging residue may survive the install: {after:?}"
    );
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(
        containers.join("reapme").join("rootfs").is_dir(),
        "the install must commit a whole container"
    );

    let _ = fs::remove_dir_all(containers.join("reapme"));
    assert_real_store_intact();
}

/// (b) `remove --force` clears staging residue as well as the container, and it
/// reaps even for a name that was never installed (the reaper runs before the
/// existence check, so the residue is cleared and the command still reports
/// "not found").
#[test]
fn remove_force_leaves_zero_staging_residue() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    // (1) Residue for a name with no committed container.
    let ghost = plant_residue(&containers, "ghostname");
    let out = force_remove(&sb, "ghostname");
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!(
        "(b) remove --force, never-installed name: rc={:?}; residue gone: {}; stderr: {}",
        out.status.code(),
        !ghost.exists(),
        stderr.trim()
    );
    assert!(
        !ghost.exists(),
        "remove --force must reap residue even when no container of that name exists"
    );
    assert!(
        !staging_lock_path(&ghost).exists(),
        "the sidecar lock must go with the reaped residue"
    );
    assert!(
        staging_dirs(&containers).is_empty(),
        "no staging residue may survive remove --force"
    );
    assert!(
        !out.status.success(),
        "remove of a never-installed name must still report that it was not found"
    );
    assert!(
        stderr.contains("not found"),
        "remove --force must still say the container was not found: {stderr}"
    );

    // (2) Residue next to a committed container of the same name: both go.
    let ok = install(&sb, "removeme", &[]);
    assert!(
        ok.status.success(),
        "setup install must succeed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let planted = plant_residue(&containers, "removeme");
    let out = force_remove(&sb, "removeme");
    let after = staging_dirs(&containers);
    println!(
        "(b) remove --force, committed container: rc={:?}; residue after: {} {after:?}; container gone: {}",
        out.status.code(),
        after.len(),
        !containers.join("removeme").exists()
    );
    assert!(
        out.status.success(),
        "remove --force must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !planted.exists(),
        "remove must clear the planted residue, not only the container"
    );
    assert!(!staging_lock_path(&planted).exists());
    assert_eq!(after.len(), 0, "remove must clear the residue: {after:?}");
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(
        !containers.join("removeme").exists(),
        "the container must be gone"
    );

    assert_real_store_intact();
}

/// (c) An interrupted install leaves no orphan `podnext` process, no torn
/// `metadata.json`, and no residue the next install cannot clear - the invariant
/// that holds whether or not the kill landed mid-flight.
///
/// The kill is driven by the observed event (the staging tree appearing), never a
/// clock, and the test PRINTS which branch it took. It never asserts that the kill
/// produced residue: measured on this device a fixed 3 s kill leaves none.
#[test]
fn killed_install_leaves_no_orphan_and_no_half_written_state() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let before = podnext_process_count();
    let attempt = interrupt_on_staging(
        spawn_install(&sb, "killme", false),
        &containers,
        STAGING_WINDOW,
    );
    let after = podnext_process_count();
    println!(
        "(c) branch: {}; status={:?}; podnext before={before} after={after}; \
         residue left by the attempt: {:?}",
        attempt.label(),
        attempt.status(),
        staging_dirs(&containers)
    );
    // Branch-independent: interrupted (SIGKILLed and reaped) and not-interrupted
    // (exited on its own) both leave no `podnext` behind.
    assert_eq!(
        after, before,
        "an install must leave no orphan podnext process (before={before} after={after})"
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

    // The store must end clean and the install path must still work, whichever
    // branch ran. A plain install reclaims any residue when nothing was committed;
    // when the install already committed, the equivalent install is `--force`.
    let extra: &[&str] = if attempt.was_killed() {
        &[]
    } else {
        &["--force"]
    };
    let ok = install(&sb, "killme", extra);
    let after_residue = staging_dirs(&containers);
    println!(
        "(c) next install rc={:?}; staging residue after it: {} {after_residue:?}",
        ok.status.code(),
        after_residue.len()
    );
    assert!(
        ok.status.success(),
        "the next install must succeed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(
        after_residue.len(),
        0,
        "no staging residue may survive the next install: {after_residue:?}"
    );
    assert_eq!(stray_staging_locks(&containers).len(), 0);
    assert!(containers.join("killme").join("rootfs").is_dir());

    let _ = force_remove(&sb, "killme");
    assert_real_store_intact();
}

/// (d) Two concurrent same-name installs: exactly one wins, and the survivor is a
/// whole rootfs (file count equal to a solo install of the same image).
///
/// The claim is branch-independent: the loser is refused by the container-name
/// lock when the installs overlap and because the container is already committed
/// when they do not (both measured). The fixture prints which it saw instead of
/// assuming the two overlapped.
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
    let loser = if out_a.status.success() {
        &out_b
    } else {
        &out_a
    };
    let loser_err = String::from_utf8_lossy(&loser.stderr);
    let branch = if loser_err.contains("acquire lock") {
        "OVERLAPPING - the loser was refused by the container-name lock"
    } else if loser_err.contains("already exists") {
        "NOT OVERLAPPING - the loser was refused because the container was already committed"
    } else {
        "UNRECOGNISED refusal"
    };
    println!(
        "(d) race rc: {:?} / {:?}; successes={successes}; branch: {branch}",
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

/// (e) A staging tree whose sidecar lock another process HOLDS is live and must
/// survive a reaper running in yet another process; once the lock is released the
/// same tree is residue and must be reaped.
///
/// `flock` excludes across processes, not across descriptors inside one process,
/// so the owner here is this test process and the reaper is the real binary
/// (`install` / `remove --force`) - exactly the production arrangement. Planting
/// the tree and holding the lock makes the property deterministic on every host.
#[test]
fn a_live_installs_staging_tree_survives_another_installs_reaper() {
    let _guard = serial();
    let sb = Sandbox::new();
    let containers = sb.containers();

    let live = plant_residue(&containers, "live");
    let lock_path = staging_lock_path(&live);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("open the live tree's sidecar lock");
    let rc = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(rc, 0, "the test could not take the sidecar lock");
    let sentinel = live.join("pdn-liveness-sentinel");
    fs::write(&sentinel, b"live").expect("write sentinel");
    println!(
        "(e) live tree {} with its sidecar lock held by test process {}",
        live.file_name().unwrap().to_string_lossy(),
        std::process::id()
    );

    // The reaper is a DIFFERENT process: an install of an unrelated name reaps at
    // its start, before it stages anything of its own.
    let other = install(&sb, "other", &[]);
    println!(
        "(e) separate-process reaper (install other) rc={:?}; live tree survived: {}; \
         sentinel survived: {}",
        other.status.code(),
        live.exists(),
        sentinel.exists()
    );
    assert!(
        other.status.success(),
        "the second install must succeed: {}",
        String::from_utf8_lossy(&other.stderr)
    );
    assert!(
        live.exists(),
        "the reaper must not take a live install's staging tree"
    );
    assert!(
        sentinel.exists(),
        "the reaper deleted the live install's staging tree"
    );
    assert!(
        lock_path.exists(),
        "the live tree's sidecar lock must survive the reaper"
    );
    assert!(
        containers.join("other").join("rootfs").is_dir(),
        "the install that ran the reaper must commit"
    );

    // Release the lock: the tree is now unowned residue and the next reaper must
    // take it. Deterministic - the test chooses when the tree stops being live.
    drop(lock);
    let out = force_remove(&sb, "other");
    println!(
        "(e) after releasing the lock, separate-process reaper (remove --force other) \
         rc={:?}; tree gone: {}; sidecar gone: {}",
        out.status.code(),
        !live.exists(),
        !lock_path.exists()
    );
    assert!(
        out.status.success(),
        "remove --force must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!live.exists(), "an unowned staging tree must be reaped");
    assert!(
        !lock_path.exists(),
        "the sidecar lock must go with the reaped tree"
    );
    assert!(staging_dirs(&containers).is_empty());
    assert_eq!(stray_staging_locks(&containers).len(), 0);

    assert_real_store_intact();
}
