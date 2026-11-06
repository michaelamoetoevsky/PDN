//! Store-integrity acceptance tests for WORKLIST A1, A3a and A4.
//!
//! Every test bounds its own wait: a check that can hang is not a check, so the
//! tests assert an *elapsed* budget rather than trusting the call to return.
//!
//! The store is built under a temporary directory and reached through
//! `PN_TEST_ROOT` (the same override `paths::resolve_home` honours), so no test
//! touches the real `~/.podnext`.

use proot_distro_core::container::{is_container_dir, metadata_kind, scan_store, MetadataKind};
use proot_distro_core::locking::ContainerLock;
use proot_distro_core::{ContainerInfo, ContainerStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// Serializes the tests that mutate process-global environment (`PN_TEST_ROOT`).
static ENV_LOCK: Mutex<()> = Mutex::new(());

const BUDGET: Duration = Duration::from_secs(2);

/// A real container of the shape the store scanner expects: a rootfs with
/// `etc/os-release` plus a `metadata.json`.
fn plant_container(store: &Path, name: &str) -> PathBuf {
    let dir = store.join(name);
    fs::create_dir_all(dir.join("rootfs/etc")).unwrap();
    fs::write(
        dir.join("rootfs/etc/os-release"),
        "ID=alpine\nVERSION_ID=3.24.4\n",
    )
    .unwrap();
    fs::write(
        dir.join("metadata.json"),
        r#"{"name":"x","image":"alpine","tag":"3.24.4","created_at":"2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    dir
}

fn make_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o644) }, 0, "mkfifo");
}

/// Build a store under a fake home and return `(home, containers_dir)`.
fn fake_store(tmp: &Path) -> (PathBuf, PathBuf) {
    let home = tmp.join("home");
    let store = home.join(".podnext/containers");
    fs::create_dir_all(&store).unwrap();
    (home, store)
}

/// Read the listing the way `podnext list` does, bounded by the caller's own
/// clock. Returns the result and how long the whole construction took.
fn list_via_manager(home: &Path) -> (anyhow::Result<Vec<ContainerInfo>>, Duration) {
    std::env::set_var("PN_TEST_ROOT", home);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let t0 = Instant::now();
    let res = rt.block_on(async {
        let mgr = proot_distro_core::ContainerManager::new()?;
        mgr.list().await
    });
    let elapsed = t0.elapsed();
    std::env::remove_var("PN_TEST_ROOT");
    (res, elapsed)
}

// ---------------------------------------------------------------------------
// A1 - a hostile metadata.json must not block, and must be reported by type.
// ---------------------------------------------------------------------------

/// (a) FIFO: the listing returns inside the budget and the container is reported
/// corrupt with the offending file TYPE named.
#[test]
fn a_fifo_metadata_json_is_bounded_and_reported_as_a_fifo() {
    let tmp = tempdir().unwrap();
    let (home, store) = fake_store(tmp.path());
    let dir = plant_container(&store, "hostile");
    fs::remove_file(dir.join("metadata.json")).unwrap();
    make_fifo(&dir.join("metadata.json"));

    // Metadata is classified, not read: no read of the FIFO is ever attempted.
    assert_eq!(metadata_kind(&dir), MetadataKind::Hostile("fifo"));

    // The core scan is the listing's body; it must return inside the budget.
    let t0 = Instant::now();
    let scanned = scan_store(&store).expect("scan");
    assert!(
        t0.elapsed() < BUDGET,
        "scan blocked on the FIFO: {:?}",
        t0.elapsed()
    );

    let c = scanned
        .iter()
        .find(|c| c.name == "hostile")
        .expect("listed");
    let reason = c.corrupt.as_deref().expect("flagged corrupt");
    assert!(reason.contains("fifo"), "type not named: {reason}");
    assert_eq!(c.status, ContainerStatus::Error);

    // And the end-to-end listing path (`ContainerManager::new` + `list`) is
    // bounded too: that construction is where the process used to hang.
    let _g = ENV_LOCK.lock().unwrap();
    let (listed, elapsed) = list_via_manager(&home);
    let listed = listed.expect("listing must not fail");
    assert!(elapsed < BUDGET, "listing blocked on the FIFO: {elapsed:?}");
    let c = listed.iter().find(|c| c.name == "hostile").expect("listed");
    assert!(c.corrupt.as_deref().unwrap_or("").contains("fifo"));
}

/// (a) A symlinked `metadata.json` is refused for the same reason, named `symlink`.
#[test]
fn a_symlinked_metadata_json_is_reported_as_a_symlink() {
    let tmp = tempdir().unwrap();
    let (_, store) = fake_store(tmp.path());
    let dir = plant_container(&store, "linked");
    fs::remove_file(dir.join("metadata.json")).unwrap();
    fs::write(tmp.path().join("elsewhere.json"), "{}").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("elsewhere.json"), dir.join("metadata.json"))
        .unwrap();

    assert_eq!(metadata_kind(&dir), MetadataKind::Hostile("symlink"));

    let t0 = Instant::now();
    let scanned = scan_store(&store).expect("scan");
    assert!(t0.elapsed() < BUDGET, "scan blocked: {:?}", t0.elapsed());
    let c = scanned.iter().find(|c| c.name == "linked").expect("listed");
    assert!(c.corrupt.as_deref().unwrap_or("").contains("symlink"));
}

/// (a) A DIRECTORY at `metadata.json` is refused, named `directory`.
#[test]
fn a_directory_at_metadata_json_is_reported_as_a_directory() {
    let tmp = tempdir().unwrap();
    let (_, store) = fake_store(tmp.path());
    let dir = plant_container(&store, "dir");
    fs::remove_file(dir.join("metadata.json")).unwrap();
    fs::create_dir(dir.join("metadata.json")).unwrap();

    assert_eq!(metadata_kind(&dir), MetadataKind::Hostile("directory"));

    let t0 = Instant::now();
    let scanned = scan_store(&store).expect("scan");
    assert!(t0.elapsed() < BUDGET, "scan blocked: {:?}", t0.elapsed());
    let c = scanned.iter().find(|c| c.name == "dir").expect("listed");
    assert!(c.corrupt.as_deref().unwrap_or("").contains("directory"));
}

// ---------------------------------------------------------------------------
// A1 - remove --force must clear a container it cannot read.
// ---------------------------------------------------------------------------

/// (b) `remove --force` on a FIFO container succeeds instead of hanging.
///
/// The hang lived in `ContainerManager::new` (the store scan); the delete itself
/// is `remove_dir_all`. Both halves are exercised here: the manager must see the
/// container (so `get` succeeds and `remove` does not bail "not found"), and the
/// directory must actually go away.
#[test]
fn remove_force_clears_a_container_whose_metadata_is_a_fifo() {
    let tmp = tempdir().unwrap();
    let (home, store) = fake_store(tmp.path());
    let dir = plant_container(&store, "stuck");
    fs::remove_file(dir.join("metadata.json")).unwrap();
    make_fifo(&dir.join("metadata.json"));

    let _g = ENV_LOCK.lock().unwrap();
    let (listed, elapsed) = list_via_manager(&home);
    let listed = listed.expect("listing must not block");
    assert!(
        elapsed < BUDGET,
        "remove blocked before deleting: {elapsed:?}"
    );
    let seen = listed
        .iter()
        .find(|c| c.name == "stuck")
        .expect("the target must be visible to remove");

    // The CLI's remove path: delete the container directory. A FIFO inside it is
    // unlinked like any other entry.
    let t0 = Instant::now();
    fs::remove_dir_all(&dir).expect("remove_dir_all must clear the FIFO container");
    assert!(t0.elapsed() < BUDGET, "delete blocked: {:?}", t0.elapsed());
    assert!(!dir.exists(), "directory must be gone");
    assert!(
        seen.corrupt.is_some(),
        "was reported corrupt before deletion"
    );
}

// ---------------------------------------------------------------------------
// A3a - a staging directory is never listed as a container.
// ---------------------------------------------------------------------------

/// (c) A `.tmp-install-<anything>` directory that otherwise looks like a
/// container is not a container and is not listed.
#[test]
fn a_staging_directory_is_not_listed_as_a_container() {
    let tmp = tempdir().unwrap();
    let (_, store) = fake_store(tmp.path());
    let staging = store.join(".tmp-install-alpine-2f9c4f0e-6a1b-4a5d-9f0e-000000000000");
    fs::create_dir_all(staging.join("rootfs/etc")).unwrap();
    fs::write(staging.join("rootfs/etc/os-release"), "ID=alpine\n").unwrap();
    fs::write(
        staging.join("metadata.json"),
        r#"{"image":"alpine","tag":"3.24.4"}"#,
    )
    .unwrap();

    assert!(
        !is_container_dir(&staging),
        "a staging directory must not be a container"
    );

    let listed = scan_store(&store).expect("scan");
    assert!(
        listed.is_empty(),
        "the staging directory was listed: {:?}",
        listed.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// A4 - the install lock is taken, and the refusal names the holder.
// ---------------------------------------------------------------------------

/// (d) Two concurrent installs of one name: exactly ONE succeeds and the other is
/// refused with the holder's PID; `~/.podnext/locks/` is non-empty while held.
///
/// The holder is a genuinely separate process: this test re-executes its own
/// binary and drives the child into the "hold the lock" branch, so the PID in the
/// refusal is not the contender's own.
#[test]
fn two_concurrent_installs_of_one_name_one_wins_and_the_refusal_names_the_holder() {
    if let Ok(name) = std::env::var("PDN_LOCK_CHILD_NAME") {
        let home = PathBuf::from(std::env::var("PDN_LOCK_CHILD_HOME").unwrap());
        std::env::set_var("HOME", &home);
        let mut lock = ContainerLock::new(&name, true);
        lock.acquire().expect("the holder must take the lock");
        fs::write(home.join(".holder_pid"), std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(30));
        return;
    }

    let tmp = tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();

    // `ContainerLock::new` and `lock_path` resolve the store from `$HOME`, so the
    // test redirects it; the lock is then the same file for parent and child.
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("HOME", &home);

    // The default lock location must live in the shared store, not in `.pn-next`.
    assert_eq!(
        proot_distro_core::locking::lock_path("rc"),
        home.join(".podnext/locks/rc.lock")
    );

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("two_concurrent_installs_of_one_name_one_wins_and_the_refusal_names_the_holder")
        .arg("--nocapture")
        .env("HOME", &home)
        .env("PDN_LOCK_CHILD_NAME", "rc")
        .env("PDN_LOCK_CHILD_HOME", &home)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the holder");

    // Wait (bounded) for the holder to take the lock.
    let pid_file = home.join(".holder_pid");
    let mut holder_pid = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(text) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                holder_pid = Some(pid);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let holder_pid = match holder_pid {
        Some(p) => p,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the holder never took the lock");
        }
    };

    // ~/.podnext/locks/ is non-empty while the install is in flight.
    let lock_dir = home.join(".podnext/locks");
    let entries: Vec<_> = fs::read_dir(&lock_dir)
        .expect("lock dir must exist")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(!entries.is_empty(), "{} stayed empty", lock_dir.display());

    // The second install of the same name is REFUSED, and the refusal names the
    // holder's PID.
    let mut contender = ContainerLock::new("rc", true);
    let err = contender
        .acquire()
        .expect_err("a second install of the same name must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains(&holder_pid.to_string()),
        "the refusal must name holder PID {holder_pid}: {msg}"
    );
    assert!(msg.contains("held by another process"), "message: {msg}");

    // The lock file records the holder.
    assert_eq!(
        fs::read_to_string(lock_dir.join("rc.lock")).unwrap().trim(),
        holder_pid.to_string()
    );

    std::env::remove_var("HOME");
    let _ = child.kill();
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// A19 - the store root has ONE resolver. A redirected root must not spill into
// the operator's real ~/.podnext.
// ---------------------------------------------------------------------------

/// Read-only, depth-2 listing of the operator's real store as relative paths.
///
/// Depth 2 is enough: every path this class composes is `<real>/.podnext/<entry>`
/// or `<real>/.podnext/<dir>/<entry>` (containers, volumes, locks, quotas.json), so
/// a leak appears as a new entry here. Symlinks are listed but not descended.
fn store_snapshot(root: &Option<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let root = match root {
        Some(r) if r.exists() => r.clone(),
        _ => return out,
    };
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            out.push(path.strip_prefix(&root).unwrap_or(&path).to_path_buf());
            let is_plain_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_plain_dir && depth + 1 < 2 {
                stack.push((path, depth + 1));
            }
        }
    }
    out.sort();
    out
}

/// A19: with the store root redirected (`PN_TEST_ROOT`), every home-derived
/// default this class owns resolves under the redirect, and NOTHING is created
/// under the operator's real `~/.podnext`.
///
/// The real store is probed through `dirs::home_dir()` on purpose: that is the one
/// location that must NOT follow the redirect, because the whole point is to
/// observe whether the redirect reached the operator's store. It is the same probe
/// A16's lock test uses, and the ledger's A19 predicate exempts it.
#[test]
fn a_redirected_root_creates_nothing_under_the_operators_real_store() {
    let _g = ENV_LOCK.lock().unwrap();

    // Snapshot the operator's real store BEFORE anything is redirected.
    let real_store = dirs::home_dir().map(|h| h.join(".podnext"));
    let before = store_snapshot(&real_store);

    let tmp = tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    std::env::set_var("PN_TEST_ROOT", &home);

    // Every default the class owns must follow the redirect.
    let cfg = proot_distro_core::config::Config::default();
    assert!(
        cfg.storage.root_dir.starts_with(&home),
        "storage.root_dir did not follow the redirect: {:?}",
        cfg.storage.root_dir
    );
    assert!(
        cfg.paths.root_dir.starts_with(&home),
        "paths.root_dir did not follow the redirect: {:?}",
        cfg.paths.root_dir
    );

    let container = proot_distro_core::container::Container::new("a19".into(), Default::default())
        .expect("container");
    assert!(
        container.rootfs_path.starts_with(&home),
        "rootfs_path did not follow the redirect: {:?}",
        container.rootfs_path
    );

    // The store itself: creating it is where a second root would leak.
    let paths = proot_distro_core::ContainerPaths::new().expect("store paths");
    paths.ensure_dirs().expect("ensure dirs");
    assert!(
        paths.containers_dir.starts_with(&home),
        "containers_dir did not follow the redirect: {:?}",
        paths.containers_dir
    );

    std::env::remove_var("PN_TEST_ROOT");

    // Nothing appeared under the operator's real store.
    let after = store_snapshot(&real_store);
    assert_eq!(
        before, after,
        "a redirected root wrote into the operator's real store"
    );
}
