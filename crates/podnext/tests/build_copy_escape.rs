//! WORKLIST A11 (second half): a COPY whose destination escapes the image root.
//!
//! `podnext build` joins a COPY destination straight onto the image rootfs
//! (`rootfs.join(dest)`), so a `..` component climbs out of it: `COPY x
//! ../../../../escaped.txt` wrote the file outside the image and into the store.
//! This drives the real CLI - the Dockerfile goes through the real parser and the
//! real COPY execution path - and then looks for the byte that landed outside the
//! root.
//!
//! The store is a scratch directory (`HOME` and `PN_TEST_ROOT` both point at it),
//! so the operator's real `~/.podnext` is never touched.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// `/tmp` is not writable for an app uid on Android.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

fn bin() -> String {
    // One shared CARGO_TARGET_DIR across worktrees means a sibling's build can
    // silently replace `target/debug/podnext`; PODNEXT_BIN pins the copy.
    if let Ok(pinned) = std::env::var("PODNEXT_BIN") {
        if !pinned.is_empty() {
            return pinned;
        }
    }
    env!("CARGO_BIN_EXE_podnext").to_string()
}

#[test]
fn a_copy_destination_with_dotdot_is_refused_and_writes_nothing_outside_the_root() {
    let scratch = tempfile::Builder::new()
        .prefix("pdn-copy-escape-")
        .tempdir_in(temp_base())
        .expect("scratch store");
    let home = scratch.path().join("home");
    let ctx = scratch.path().join("ctx");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&ctx).unwrap();
    fs::write(ctx.join("payload.txt"), b"top secret\n").unwrap();

    // Four levels out of `<home>/.podnext/containers/copytest/rootfs` lands
    // directly in the scratch home: containers -> .podnext -> home.
    let dest = "../../../../escaped.txt";
    fs::write(
        ctx.join("Dockerfile"),
        format!("FROM scratch\nCOPY payload.txt {dest}\n"),
    )
    .unwrap();
    let escaped = home.join("escaped.txt");

    let out = Command::new(bin())
        .current_dir(&ctx)
        .args([
            "build",
            "local",
            "-t",
            "copytest",
            "--file",
            "Dockerfile",
            ".",
        ])
        .env("HOME", &home)
        .env("PN_TEST_ROOT", &home)
        .output()
        .expect("run podnext build");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // 1. Nothing may be written outside the image root. This is the escape.
    assert!(
        !escaped.exists(),
        "A COPY with '..' in its destination escaped the image root: podnext wrote {}\n\
         build exit={:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        escaped.display(),
        out.status.code(),
    );

    // 2. The instruction must be refused by name, naming the offending path.
    assert!(
        !out.status.success(),
        "the build must be refused, not succeed\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains(dest),
        "the refusal must name the offending path '{dest}': {combined}"
    );
    assert!(
        combined.contains("COPY"),
        "the refusal must name the COPY instruction: {combined}"
    );

    let _ = fs::remove_dir_all(&home);
}

/// The legitimate shapes must keep working: a relative in-tree COPY and `.` as
/// destination.
#[test]
fn a_relative_in_tree_copy_with_a_dot_destination_still_builds() {
    let scratch = tempfile::Builder::new()
        .prefix("pdn-copy-ok-")
        .tempdir_in(temp_base())
        .expect("scratch store");
    let home = scratch.path().join("home");
    let ctx = scratch.path().join("ctx");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&ctx).unwrap();
    fs::write(ctx.join("payload.txt"), b"here\n").unwrap();
    fs::write(ctx.join("Dockerfile"), "FROM scratch\nCOPY payload.txt .\n").unwrap();

    let out = Command::new(bin())
        .current_dir(&ctx)
        .args([
            "build",
            "local",
            "-t",
            "copyok",
            "--file",
            "Dockerfile",
            ".",
        ])
        .env("HOME", &home)
        .env("PN_TEST_ROOT", &home)
        .output()
        .expect("run podnext build");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a legitimate in-tree COPY must build\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // `.` as destination keeps the source's name, inside the rootfs.
    let copied = home.join(".podnext/containers/copyok/rootfs/payload.txt");
    assert!(
        copied.is_file(),
        "the COPY must land inside the image root at {}",
        copied.display()
    );

    let _ = fs::remove_dir_all(&home);
}
