//! Store-integrity acceptance tests for WORKLIST S3: a FIFO at a container's
//! `metadata.json` is *metadata* - classified by type and refused by name - and
//! never a device node the store opens.
//!
//! A1 bounded the symptom: the scan classified the path with `O_NONBLOCK` so it
//! could not block, and then listed the container as a corrupt entry. The
//! campaign's planted FIFO was still a *pseudo-container* in the listing. S3 is
//! the structural form: the store reads metadata only from a regular file, and
//! anything else at that path (FIFO, socket, device, directory, symlink) fails
//! the store operation with an error naming the path and the file type, raised
//! before any read is attempted.
//!
//! The bound is the assertion, not a hope: a read of a FIFO blocks in `openat`
//! until a writer appears, so a test that returns inside its budget has proved
//! nothing was opened.
//!
//! The assertion runs against the *whole* store operation, not a helper, so the
//! base tree is a real control: there the scan returns `Ok` (the FIFO container
//! listed as corrupt) and the `expect_err` below fails, which is what this test
//! is for. Nothing here touches the real `~/.podnext`; the store lives in a
//! `tempdir` that unlinks the planted FIFO on drop even when a test fails.

use proot_distro_core::container::scan_store;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// The acceptance bound: a store operation that has not returned in 5 s has hung
/// on the FIFO, which is the failure this test exists to catch.
const BUDGET: Duration = Duration::from_secs(5);

/// A real container of the shape the store scanner expects.
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

/// A store under a temporary home.
fn fake_store(tmp: &Path) -> PathBuf {
    let store = tmp.join("home/.podnext/containers");
    fs::create_dir_all(&store).unwrap();
    store
}

/// (a) The store operation returns a NAMED error - the path and the file type -
/// well inside the budget, because the FIFO was never opened.
#[test]
fn a_fifo_metadata_json_is_a_named_store_error_and_never_blocks() {
    let tmp = tempdir().unwrap();
    let store = fake_store(tmp.path());
    let dir = plant_container(&store, "hostile");
    fs::remove_file(dir.join("metadata.json")).unwrap();
    make_fifo(&dir.join("metadata.json"));

    let t0 = Instant::now();
    let result = scan_store(&store);
    let elapsed = t0.elapsed();

    // Bounded: a read of the FIFO would have blocked here until a writer turned
    // up, so returning at all is the proof the path was never opened.
    assert!(
        elapsed < BUDGET,
        "the store blocked on the FIFO metadata path: {elapsed:?}"
    );

    let err = result.expect_err("a FIFO metadata path must fail the store operation, not list");
    let msg = format!("{err}");
    assert!(
        msg.contains("store error:"),
        "not a named store error: {msg}"
    );
    let fifo_path = dir.join("metadata.json").display().to_string();
    assert!(msg.contains(&fifo_path), "the path is not named: {msg}");
    assert!(msg.contains("fifo"), "the file type is not named: {msg}");
    assert!(
        msg.contains("not a regular file"),
        "the refusal does not say what is wrong: {msg}"
    );

    // Evidence, never asserted: how fast the refusal came back.
    eprintln!("S3: FIFO metadata refused in {elapsed:?} by: {msg}");
}

/// The counter-case that keeps the refusal honest: a store whose metadata is a
/// regular file is still scanned and listed. A refusal that rejected everything
/// would "pass" the test above for the wrong reason.
#[test]
fn a_regular_metadata_file_still_scans() {
    let tmp = tempdir().unwrap();
    let store = fake_store(tmp.path());
    plant_container(&store, "good");

    let scanned = scan_store(&store).expect("a regular metadata.json must scan");
    assert!(
        scanned.iter().any(|c| c.name == "good"),
        "the container is listed"
    );
}
