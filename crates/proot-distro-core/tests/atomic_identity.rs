//! B11: an atomic state write refuses (`ESTALE`) rather than clobbering when the
//! temp file is replaced or removed between the write and the rename.
//!
//! The swap is PLANTED, never raced: each test builds the temp file itself,
//! replaces or removes it, then drives the identity gate (`rename_if_unchanged`)
//! directly — the same step `atomic_write` performs around its rename. No test
//! wins a timing window; the condition is deterministic.

use proot_distro_core::storage::{atomic_write, file_id, rename_if_unchanged};
use std::fs;
use std::path::PathBuf;

/// Scratch dir under `temp_base()` (never `/tmp`, which is not writable on
/// Android) — the same helper the crate's own paths use.
fn scratch(tag: &str) -> PathBuf {
    let base =
        proot_distro_core::temp_base().join(format!("pdn-b11-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}

/// Replace `path`'s inode with a freshly written file's, deterministically: the
/// impostor is a *separate* file `rename`d over the entry, so its `(st_dev,
/// st_ino)` cannot coincide with the one captured for the original (unlike a
/// remove-then-write, where the freed inode number may be reused).
fn swap_in_impostor(path: &std::path::Path, bytes: &[u8]) {
    let impostor = path.with_extension("impostor");
    fs::write(&impostor, bytes).unwrap();
    fs::rename(&impostor, path).unwrap();
}

#[test]
fn refuses_estale_when_the_temp_was_replaced() {
    let dir = scratch("swap");
    let dest = dir.join("state.json");
    let tmp = dir.join("state.json.tmp");
    fs::write(&dest, b"OLD").unwrap();

    // The temp file exactly as the writer left it.
    fs::write(&tmp, b"NEW").unwrap();
    let mine = file_id(&tmp).unwrap();

    // Somebody other than us (a second writer, a cleanup sweep) swaps it out.
    swap_in_impostor(&tmp, b"IMPOSTOR");
    let theirs = file_id(&tmp).unwrap();
    assert_ne!(mine, theirs, "planted swap must change the identity");

    let res = rename_if_unchanged(&tmp, mine, &dest);
    assert!(
        res.is_err(),
        "the identity gate must refuse a replaced temp; dest now holds {:?}",
        String::from_utf8_lossy(&fs::read(&dest).unwrap())
    );
    let msg = res.unwrap_err().to_string();
    eprintln!("refusal message: {msg}");
    assert!(msg.contains("ESTALE"), "refusal must name ESTALE: {msg}");
    assert!(
        msg.contains(&mine.to_string()) && msg.contains(&theirs.to_string()),
        "refusal must carry both identity pairs ({mine} / {theirs}): {msg}"
    );

    // The destination keeps the OLD content, byte for byte.
    assert_eq!(fs::read(&dest).unwrap(), b"OLD");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refuses_estale_when_the_temp_vanished() {
    let dir = scratch("gone");
    let dest = dir.join("state.json");
    let tmp = dir.join("state.json.tmp");
    fs::write(&dest, b"OLD").unwrap();

    fs::write(&tmp, b"NEW").unwrap();
    let mine = file_id(&tmp).unwrap();

    // A cleanup sweep removes the temp before our rename.
    fs::remove_file(&tmp).unwrap();

    let res = rename_if_unchanged(&tmp, mine, &dest);
    assert!(
        res.is_err(),
        "the identity gate must refuse a vanished temp; dest now holds {:?}",
        String::from_utf8_lossy(&fs::read(&dest).unwrap())
    );
    let msg = res.unwrap_err().to_string();
    assert!(msg.contains("ESTALE"), "refusal must name ESTALE: {msg}");
    assert_eq!(fs::read(&dest).unwrap(), b"OLD");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proceeds_when_the_identity_is_intact() {
    let dir = scratch("intact");
    let dest = dir.join("state.json");
    let tmp = dir.join("state.json.tmp");
    fs::write(&dest, b"OLD").unwrap();
    fs::write(&tmp, b"NEW").unwrap();
    let mine = file_id(&tmp).unwrap();

    rename_if_unchanged(&tmp, mine, &dest).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"NEW");
    assert!(!tmp.exists(), "temp must be consumed by the rename");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_installs_the_new_bytes() {
    let dir = scratch("write");
    let dest = dir.join("state.json");
    fs::write(&dest, b"OLD").unwrap();

    atomic_write(&dest, b"NEW").unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"NEW");
    // No temp sibling left behind.
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name() != "state.json")
        .collect();
    assert!(leftovers.is_empty(), "temp left behind: {leftovers:?}");
    let _ = fs::remove_dir_all(&dir);
}
