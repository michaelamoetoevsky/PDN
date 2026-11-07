//! C6 acceptance: reference-counted eviction, proven against the real binary.
//!
//! Two halves, each able to fail on its own:
//!
//! (1) a store unit that is still referenced is **never** deleted, and the
//!     refusal names the count and the holder;
//! (2) the same unit, once its references reach zero, **is** deleted.
//!
//! The store unit is the staging tree an interrupted install leaves behind — the
//! `.tmp-install-*` directory the campaign measured as a permanent leak. Its
//! reference is the live install that holds the sidecar `<tree>.lock`; that is a
//! real reference today, not a planted counter.
//!
//! The reference is planted deterministically (this process takes the `flock`),
//! never raced for: `flock` excludes across processes, so the reaper under test
//! is the real `podnext remove` in a separate process — the production
//! arrangement.
//!
//! Every store path is redirected (`HOME` and `PN_TEST_ROOT`), so no test can
//! write to the operator's real `~/.podnext`.

use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> String {
    env!("CARGO_BIN_EXE_podnext").to_string()
}

/// Writable scratch base: `/tmp` is not writable for an app uid on Android.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

/// A private store plus the helpers to drive `podnext` against it.
struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("pdn-refcount-")
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

    fn remove_force(&self, name: &str) -> Output {
        Command::new(bin())
            .args(["remove", name, "--force"])
            .env("HOME", self.home())
            .env("PN_TEST_ROOT", self.home())
            .output()
            .expect("podnext remove")
    }
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// The sidecar lock path for a staging directory: `<staging dir>.lock`, exactly
/// as the install path builds it.
fn staging_lock_path(dir: &Path) -> PathBuf {
    let mut name = dir.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

/// Plant a `.tmp-install-<name>-<uuid>` tree with a half-written rootfs, plus the
/// sidecar lock an install would have taken before creating it.
fn plant_staging(containers: &Path, name: &str) -> (PathBuf, PathBuf) {
    fs::create_dir_all(containers).expect("containers dir");
    let tree = containers.join(format!(".tmp-install-{name}-{}", unique_suffix()));
    fs::create_dir_all(tree.join("rootfs")).expect("planted rootfs");
    fs::write(tree.join("rootfs/half-written"), b"partial").expect("half-written file");
    fs::write(tree.join("pdn-liveness-sentinel"), b"live").expect("sentinel");
    let lock = staging_lock_path(&tree);
    fs::write(&lock, b"").expect("planted sidecar lock");
    (tree, lock)
}

/// The exact refusal `refcount::refusal_message` produces for one holder.
fn expected_refusal(key: &str, holder_pid: u32) -> String {
    format!("refusing to evict '{key}': 1 reference(s) still hold it (install-holder(pid={holder_pid}))")
}

/// (1) A live install's staging tree is refused and survives; (2) once its lock is
/// released it is residue and the next sweep deletes it.
///
/// Both halves run against the same unit, so "still referenced" and "references
/// reached zero" are the same object at two points in its life.
#[test]
fn a_referenced_staging_unit_survives_until_its_reference_is_released() {
    let sb = Sandbox::new();
    let containers = sb.containers();
    let (tree, lock_path) = plant_staging(&containers, "live");
    let key = tree.file_name().unwrap().to_string_lossy().into_owned();

    // The reference: an exclusive flock, held by this process, with the pid the
    // install path records.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("open the sidecar lock");
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0,
        "the fixture could not take the sidecar lock"
    );
    let holder_pid = std::process::id();
    fs::write(&lock_path, holder_pid.to_string()).expect("record the holder pid");

    // The reaper under test is a different process: `remove --force` reaps before
    // it checks whether the name exists, so a never-installed name is enough.
    let out = sb.remove_force("ghost");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    println!(
        "(1) reaper rc={:?}; unit survived: {}; sidecar survived: {}; stderr: {}",
        out.status.code(),
        tree.exists(),
        lock_path.exists(),
        stderr.trim()
    );

    let expected = expected_refusal(&key, holder_pid);
    assert!(
        stderr.contains(&expected),
        "an eviction of a referenced unit must be refused with the exact line\n  expected: {expected}\n  stderr:   {stderr}"
    );
    assert!(
        tree.join("pdn-liveness-sentinel").exists(),
        "a referenced staging unit was deleted"
    );
    assert!(
        lock_path.exists(),
        "a referenced staging unit's sidecar lock was deleted"
    );

    // Release the reference: the same unit is now residue.
    drop(lock);
    let out = sb.remove_force("ghost");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    println!(
        "(2) after releasing the reference: rc={:?}; unit gone: {}; sidecar gone: {}; stderr: {}",
        out.status.code(),
        !tree.exists(),
        !lock_path.exists(),
        stderr.trim()
    );
    assert!(
        !tree.exists(),
        "a staging unit at zero references must be deleted"
    );
    assert!(
        !lock_path.exists(),
        "a deleted staging unit's sidecar lock must go with it"
    );
    assert!(
        !stderr.contains("refusing to evict"),
        "nothing holds the unit, so nothing may be refused: {stderr}"
    );
}

/// A staging unit nobody ever claimed has zero references and is deleted on the
/// first sweep — with no refusal, because there is nothing to refuse.
#[test]
fn an_unreferenced_staging_unit_is_deleted() {
    let sb = Sandbox::new();
    let containers = sb.containers();
    let (tree, lock_path) = plant_staging(&containers, "dead");
    assert!(tree.is_dir(), "the fixture must plant a staging tree");

    let out = sb.remove_force("ghost");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    println!(
        "unowned unit: rc={:?}; tree gone: {}; sidecar gone: {}; stderr: {}",
        out.status.code(),
        !tree.exists(),
        !lock_path.exists(),
        stderr.trim()
    );

    assert!(
        !tree.exists(),
        "a staging unit at zero references must be deleted"
    );
    assert!(!lock_path.exists(), "its sidecar lock must go with it");
    assert!(
        !stderr.contains("refusing to evict"),
        "an unreferenced unit must not be refused: {stderr}"
    );
}
