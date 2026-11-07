//! A29: race the four mechanisms `research/RACE_SURFACES.md` (section 4c) says are
//! single-writer **by construction** but which the tree has only ever proven
//! sequentially. The value here is evidence, not new code: turn "should hold"
//! into "was raced and held". No mechanism is added.
//!
//! The four, each with the citation the item already established:
//!
//! 1. `blobstore::put_reader` two-phase ingest - the temp name carries `pid` and
//!    a counter (`blobstore.rs:161`), publication is `rename_if_unchanged` with an
//!    identity check (`:196`). Two processes ingest the SAME digest at once.
//! 2. `refcount::sweep` vs a live install - both take the same sidecar `flock`
//!    (`refcount.rs:168-170`). A sweep races an install that holds it.
//! 3. store `list` during `install` - staging trees are named `.tmp-install-*`
//!    (`container.rs:899`). `scan_store` runs under install load.
//! 4. session register vs attach - registration takes `LOCK_EX`
//!    (`session.rs:251`) and liveness is the inherited descriptor (C8). Attach
//!    (the registry scan) races registration.
//!
//! Every root is redirected: the blob store and store root are explicit paths
//! under a `tempdir`, the session registry is a private `RUNTIME_DIR`. Nothing
//! here touches the operator's real `~/.podnext` (10 containers) or its session
//! registry. Every wait is bounded; no test can hang the suite.

use proot_distro_core::blobstore::BlobStore;
use proot_distro_core::container::{scan_store, STAGING_PREFIX};
use proot_distro_core::refcount::{sidecar_lock_path, staging_units, sweep, Eviction};
use proot_distro_core::session::{active_sessions, register_session};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Serialises the one test that mutates a process-global env var (`RUNTIME_DIR`).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Bounded wait for `path` to appear. Never an unbounded sleep.
fn wait_for(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Deterministic, position-dependent payload: a truncated or shifted copy is
/// byte-detectable, not merely shorter.
fn payload(size: usize, seed: u8) -> Vec<u8> {
    (0..size)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Regular files directly under `<root>/sha256`, by name.
fn shard_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("sha256"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ===========================================================================
// 1. blobstore::put_reader - two PROCESSES ingest one digest at once
// ===========================================================================

/// Child half: re-executed by the parent via `current_exe() --exact`, so the two
/// ingests genuinely come from two processes (two `pid`s, two temp names). Runs
/// as a no-op in the normal suite (the env is unset).
#[test]
fn child_ingests_one_digest_concurrently() {
    let Ok(root) = std::env::var("W11_BLOB_ROOT") else {
        return;
    };
    let digest = std::env::var("W11_BLOB_DIGEST").unwrap();
    let size: usize = std::env::var("W11_BLOB_SIZE").unwrap().parse().unwrap();
    let seed: u8 = std::env::var("W11_BLOB_SEED").unwrap().parse().unwrap();
    let ready = PathBuf::from(std::env::var("W11_BLOB_READY").unwrap());
    let go = PathBuf::from(std::env::var("W11_BLOB_GO").unwrap());

    let bytes = payload(size, seed);
    // Rendezvous: tell the parent we are up, then start the moment it opens the
    // gate - so both processes enter `put_reader` within a hair of each other.
    fs::write(&ready, b"ready").unwrap();
    assert!(
        wait_for(&go, Duration::from_secs(20)),
        "the go gate never opened"
    );

    let store = BlobStore::at(root);
    store
        .put_reader(&digest, &bytes[..])
        .expect("child ingest must not fail");
}

#[test]
fn two_processes_ingesting_one_digest_leave_one_correct_blob() {
    for round in 0..4u8 {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("blobs");
        fs::create_dir_all(&root).unwrap();

        let size = 4 << 20;
        let bytes = payload(size, round);
        let hex = sha256_hex(&bytes);
        let digest = format!("sha256:{hex}");

        let ready = tmp.path().join("ready");
        let go = tmp.path().join("go");
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "child_ingests_one_digest_concurrently",
                "--nocapture",
            ])
            .env("W11_BLOB_ROOT", root.as_os_str())
            .env("W11_BLOB_DIGEST", &digest)
            .env("W11_BLOB_SIZE", size.to_string())
            .env("W11_BLOB_SEED", round.to_string())
            .env("W11_BLOB_READY", ready.as_os_str())
            .env("W11_BLOB_GO", go.as_os_str())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the second ingest process");

        assert!(
            wait_for(&ready, Duration::from_secs(30)),
            "round {round}: the second process never came up"
        );

        // Open the gate, then ingest the SAME digest here - the race.
        fs::write(&go, b"go").unwrap();
        let store = BlobStore::at(root.clone());
        let _ = store
            .put_reader(&digest, &bytes[..])
            .expect("parent ingest must not fail");

        let out = child.wait_with_output().expect("reap the second process");
        assert!(
            out.status.success(),
            "round {round}: the second process failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );

        // INVARIANT: the content address holds exactly the one verified blob -
        // no second file, no partial blob, no leftover ingest temp.
        assert_eq!(
            shard_names(&root),
            vec![hex.clone()],
            "round {round}: the store holds something other than the one blob"
        );
        let stored = fs::read(root.join("sha256").join(&hex)).unwrap();
        assert_eq!(
            stored.len(),
            size,
            "round {round}: the published blob is a partial file"
        );
        assert_eq!(
            stored, bytes,
            "round {round}: the published blob is not the bytes that were hashed"
        );
        // Every entry is a regular file: a content address is never a partial or
        // special file.
        for e in fs::read_dir(root.join("sha256")).unwrap().flatten() {
            assert!(
                e.file_type().unwrap().is_file(),
                "round {round}: a non-regular entry sits at a content address"
            );
        }
    }
}

// ===========================================================================
// 2. refcount::sweep vs a live install (same sidecar flock)
// ===========================================================================

#[test]
fn a_sweep_racing_a_live_install_never_deletes_the_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let containers = tmp.path().join("containers");
    fs::create_dir_all(&containers).unwrap();

    let tree = containers.join(format!(".tmp-install-raced-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(tree.join("rootfs")).unwrap();
    fs::write(tree.join("rootfs/sentinel"), b"half-written").unwrap();
    let tree_key = tree.file_name().unwrap().to_string_lossy().into_owned();
    let lock_path = sidecar_lock_path(&tree);

    // The install takes the sidecar `flock` and holds it for its whole run.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0,
        "the install must hold the sidecar lock"
    );
    fs::write(&lock_path, std::process::id().to_string()).unwrap();

    // ...and keeps writing into the tree while the sweep runs.
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let tree = tree.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            let mut i = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let _ = fs::write(tree.join(format!("rootfs/chunk-{}", i % 64)), b"x");
                i = i.wrapping_add(1);
            }
        })
    };

    // The sweep races the live install, many passes.
    let mut refusals = 0;
    for pass in 0..500 {
        let units = staging_units(&containers);
        for ev in sweep(&units) {
            if ev.key() == tree_key {
                match ev {
                    Eviction::Refused { message, .. } => {
                        assert!(
                            message.contains("install-holder"),
                            "pass {pass}: the refusal must name the holder: {message}"
                        );
                        refusals += 1;
                    }
                    other => panic!("pass {pass}: a sweep touched a live staging tree: {other:?}"),
                }
            }
        }
        assert!(
            tree.exists(),
            "pass {pass}: the live staging tree was deleted"
        );
        assert!(
            tree.join("rootfs/sentinel").exists(),
            "pass {pass}: the install's bytes were reclaimed out from under it"
        );
        assert!(
            lock_path.exists(),
            "pass {pass}: the live sidecar was deleted"
        );
    }
    assert!(refusals > 0, "the sweep never refused a live install");

    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();

    // Release: the same sweep now legitimately reclaims it. This half exists so
    // the test above cannot pass merely because the sweep never deletes.
    unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_UN) };
    drop(lock);
    let report = sweep(&staging_units(&containers));
    assert!(
        report.iter().any(|e| matches!(e, Eviction::Evicted { .. })),
        "a released staging tree must be reclaimed: {report:?}"
    );
    assert!(!tree.exists(), "the released tree must be gone");
}

// ===========================================================================
// 3. store `list` (scan_store) during `install`
// ===========================================================================

/// The shape the store scanner expects: a rootfs with `etc/os-release` plus a
/// `metadata.json`.
fn plant_container(store: &Path, name: &str) {
    let dir = store.join(name);
    fs::create_dir_all(dir.join("rootfs/etc")).unwrap();
    fs::write(dir.join("rootfs/etc/os-release"), "ID=alpine\n").unwrap();
    fs::write(
        dir.join("metadata.json"),
        r#"{"image":"alpine","tag":"3.24.4"}"#,
    )
    .unwrap();
}

#[test]
fn a_list_racing_an_install_never_shows_a_staging_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join(".podnext/containers");
    fs::create_dir_all(&store).unwrap();
    plant_container(&store, "real");

    // The staging tree an install is building in: it has everything that could
    // make it look like a container (rootfs + etc + metadata.json).
    let staging = store.join(format!(".tmp-install-demo-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(staging.join("rootfs/etc")).unwrap();
    fs::write(staging.join("rootfs/etc/os-release"), "ID=alpine\n").unwrap();
    let staging_name = staging.file_name().unwrap().to_string_lossy().into_owned();

    // The install keeps writing into the tree while the listing runs.
    let stop = Arc::new(AtomicBool::new(false));
    let installer = {
        let staging = staging.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            let mut i = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let _ = fs::write(
                    staging.join("metadata.json"),
                    format!(r#"{{"image":"alpine","tag":"3.24.4","n":{i}}}"#),
                );
                let _ = fs::write(staging.join(format!("rootfs/etc/f-{}", i % 64)), b"y");
                i = i.wrapping_add(1);
            }
        })
    };

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut scans = 0u64;
    let mut real_seen = 0u64;
    while Instant::now() < deadline {
        let listed = scan_store(&store).expect("scan under install load");
        for c in &listed {
            assert!(
                !c.name.starts_with(STAGING_PREFIX),
                "a staging tree was listed as a container: {}",
                c.name
            );
            assert_ne!(c.name, staging_name, "the staging tree was listed by name");
        }
        if listed.iter().any(|c| c.name == "real") {
            real_seen += 1;
        }
        scans += 1;
    }
    stop.store(true, Ordering::Relaxed);
    installer.join().unwrap();

    assert!(scans > 0, "no scan ran");
    assert!(
        real_seen > 0,
        "the real container was never listed: the check is vacuous"
    );
}

// ===========================================================================
// 4. session register vs attach (the registry scan)
// ===========================================================================

/// The liveness contract (C8), raced: while a session's record holds its
/// inherited `LOCK_EX`, a concurrent attach scan (`active_sessions`, the same
/// scan `list-sessions`/`kill`/`remove` use) must never lose it - even under a
/// steady churn of other registrations coming and going beside it.
#[test]
fn an_attach_racing_a_register_never_loses_a_live_session() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("run");
    fs::create_dir_all(&runtime).unwrap();
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("RUNTIME_DIR", &runtime);

    // The live session: registered and held for the whole test.
    let live = register_session("live-box", std::process::id(), "1000", &["sh".to_string()])
        .expect("register the live session");

    // Churn: many other registrations coming and going while the scan runs.
    let stop = Arc::new(AtomicBool::new(false));
    let next_pid = Arc::new(AtomicU64::new(1_000_000));
    let churn = {
        let stop = stop.clone();
        let next_pid = next_pid.clone();
        thread::spawn(move || {
            let mut made = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let pid = next_pid.fetch_add(1, Ordering::Relaxed) as u32;
                if let Ok(g) = register_session("churn-box", pid, "1000", &["sh".to_string()]) {
                    made += 1;
                    drop(g);
                }
            }
            made
        })
    };

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut scans = 0u64;
    let mut live_missing = 0u64;
    while Instant::now() < deadline {
        let active = active_sessions().expect("attach scan");
        if !active.iter().any(|s| s.container_name == "live-box") {
            live_missing += 1;
        }
        scans += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let made = churn.join().unwrap();

    drop(live);
    std::env::remove_var("RUNTIME_DIR");

    eprintln!(
        "W11 RACE 4 (liveness): scans={scans} churn-registrations={made} live-missing={live_missing}"
    );
    assert!(
        scans > 0 && made > 0,
        "the race did not run (scans={scans}, made={made})"
    );
    assert_eq!(
        live_missing, 0,
        "a live (LOCK_EX-held) session was not attachable in {live_missing}/{scans} scans"
    );
}

/// A29 FINDING (the fourth mechanism does NOT hold): `register_session_in`
/// opens/creates the record - publishing it in the registry directory - and
/// *then* takes `LOCK_EX` (`session.rs:241-251`). An attach scan that runs in
/// that window takes `LOCK_SH`, reads the free lock as "the session is over",
/// and unlinks the record (`session.rs:354-368`). `register_session` then
/// returns `Ok` with a guard whose record is already gone: the session is
/// invisible to `active_sessions` for its whole life.
///
/// `#[ignore]`d because it is a *reproduction of a defect this tree still has*,
/// not a passing guarantee - it is committed so the claim above is backed by a
/// test that races the two operations, and run with
/// `cargo test -p proot-distro-core --test single_writer_races -- --ignored`.
#[test]
#[ignore = "reproduces an open defect (publish-before-lock in register_session_in); see RACE_SURFACES A29"]
fn a_register_publishes_its_record_before_it_locks_it() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("run");
    fs::create_dir_all(&runtime).unwrap();
    let _g = ENV_LOCK.lock().unwrap();
    std::env::set_var("RUNTIME_DIR", &runtime);

    let stop = Arc::new(AtomicBool::new(false));
    let next_pid = Arc::new(AtomicU64::new(1_000_000));
    // The registrar: many short-lived sessions, each a fresh record.
    let registrar = {
        let stop = stop.clone();
        let next_pid = next_pid.clone();
        thread::spawn(move || {
            let mut made = 0u64;
            let mut lost = 0u64;
            let mut lost_unlinked = 0u64;
            let attachable = |pid: u32| {
                active_sessions()
                    .map(|v| {
                        v.iter()
                            .any(|s| s.pid == pid && s.container_name == "churn-box")
                    })
                    .unwrap_or(false)
            };
            while !stop.load(Ordering::Relaxed) {
                let pid = next_pid.fetch_add(1, Ordering::Relaxed) as u32;
                let g = match register_session("churn-box", pid, "1000", &["sh".to_string()]) {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                made += 1;
                // Walk the record until the guard's own lock is held and its
                // bytes are written; then it must be attachable. It is checked
                // while `g` is alive, so the record's lock is held throughout.
                let deadline = Instant::now() + Duration::from_millis(100);
                let mut ok = false;
                while Instant::now() < deadline {
                    if attachable(pid) {
                        ok = true;
                        break;
                    }
                    thread::yield_now();
                }
                if !ok {
                    lost += 1;
                    // Pin the cause: unlinked (the reap during the open->flock
                    // window) or present-but-unlisted.
                    if !proot_distro_core::session::session_file_path(pid).exists() {
                        lost_unlinked += 1;
                    }
                }
                drop(g);
            }
            (made, lost, lost_unlinked)
        })
    };

    // The attach scan, racing the registrar.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut scans = 0u64;
    while Instant::now() < deadline {
        let _ = active_sessions().expect("attach scan");
        scans += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let (made, lost, lost_unlinked) = registrar.join().unwrap();
    std::env::remove_var("RUNTIME_DIR");

    eprintln!(
        "W11 RACE 4 (finding): scans={scans} registrations={made} \
         not-attachable-while-locked={lost} of-which-unlinked={lost_unlinked}"
    );
    assert!(scans > 0 && made > 0, "the race did not run");
    assert_eq!(
        lost, 0,
        "{lost}/{made} registrations were not attachable while their LOCK_EX guard was alive \
         ({lost_unlinked} of them had their record file unlinked)"
    );
}
