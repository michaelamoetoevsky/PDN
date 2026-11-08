//! ASEc-001 / ASEC1-A07-001: a WORKDIR that escapes the image rootfs.
//!
//! `podnext build` joins a WORKDIR straight onto the image rootfs
//! (`rootfs.join(wd.trim_start_matches('/'))`), so a `..` component climbs out of
//! it: `WORKDIR ../../etc` created `<store>/containers/etc` -- a sibling of the
//! image directory -- while the build still exited 0. The chained variant climbs
//! out of the store and the build fixture entirely.
//!
//! The contract asserted here is the SAME rule COPY/ADD already enforces
//! (`copy_into_rootfs`, and `build_copy_escape.rs`): a `..` path component is
//! refused by name, before any filesystem write, rather than normalised. One rule
//! for every path a Dockerfile supplies.
//!
//! These drive the real CLI. The store is a scratch directory (`HOME` and
//! `PN_TEST_ROOT` both point at it), so the operator's real `~/.podnext` is never
//! touched.

use std::fs;
use std::path::{Path, PathBuf};
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

struct Fixture {
    _scratch: tempfile::TempDir,
    home: PathBuf,
    ctx: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let scratch = tempfile::Builder::new()
            .prefix(&format!("pdn-workdir-{label}-"))
            .tempdir_in(temp_base())
            .expect("scratch store");
        let home = scratch.path().join("home");
        let ctx = scratch.path().join("ctx");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&ctx).unwrap();
        Self {
            _scratch: scratch,
            home,
            ctx,
        }
    }

    fn dockerfile(&self, body: &str) {
        fs::write(self.ctx.join("Dockerfile"), body).unwrap();
    }

    fn build(&self, tag: &str) -> std::process::Output {
        Command::new(bin())
            .current_dir(&self.ctx)
            .args(["build", "local", "-t", tag, "--file", "Dockerfile", "."])
            .env("HOME", &self.home)
            .env("PN_TEST_ROOT", &self.home)
            .output()
            .expect("run podnext build")
    }

    fn rootfs(&self, tag: &str) -> PathBuf {
        self.home
            .join(".podnext/containers")
            .join(tag)
            .join("rootfs")
    }
}

fn escaped_listing(label: &str, paths: &[&Path]) -> String {
    let mut s = format!("{label}: nothing may be created outside the image root.\n");
    for p in paths {
        s.push_str(&format!("  {} exists={}\n", p.display(), p.exists()));
    }
    s
}

/// A `..` WORKDIR is refused by name, and no directory lands outside the root.
///
/// The pre-fix landing was `<store>/containers/etc` (two climbs from
/// `<store>/containers/t1/rootfs`). Asserted first, so removing the guard reddens
/// this on the escape rather than on the refusal wording.
#[test]
fn a_workdir_with_dotdot_is_refused_and_creates_nothing_outside_the_root() {
    let fx = Fixture::new("escape");
    fx.dockerfile("FROM scratch\nWORKDIR ../../etc\n");

    let container_etc = fx.home.join(".podnext/containers/etc");
    let store_etc = fx.home.join(".podnext/etc");
    let home_etc = fx.home.join("etc");

    let out = fx.build("t1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // 1. Nothing outside the image root. This is the escape ASEC1-A07-001.
    assert!(
        !container_etc.exists() && !store_etc.exists() && !home_etc.exists(),
        "a WORKDIR with '..' escaped the image root; build exit={:?}\n{}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status.code(),
        escaped_listing(
            "WORKDIR ../../etc",
            &[&container_etc, &store_etc, &home_etc]
        ),
    );

    // 2. Refused by name, in the style COPY/ADD already uses.
    assert!(
        !out.status.success(),
        "the build must be refused, not succeed\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("../../etc"),
        "the refusal must name the offending path '../../etc': {combined}"
    );
    assert!(
        combined.contains("WORKDIR"),
        "the refusal must name the WORKDIR instruction: {combined}"
    );
}

/// The chained climb: the 1000-level form from the ledger, plus a form sized to
/// leave the store and the fixture entirely.
///
/// 1000 `../` components from the rootfs saturate at the filesystem root, so the
/// pre-fix build walked far outside the store and still exited 0. Five climbs from
/// the rootfs (`containers/t1/rootfs` -> `home` -> scratch) is the store-edge form
/// the ledger recorded leaving `<fixture>/etc` behind; it is asserted as a landing
/// path, so it reddens if the guard is removed.
#[test]
fn a_chained_dotdot_workdir_cannot_reach_outside_the_store() {
    let fx = Fixture::new("chained");
    let store = fx.home.join(".podnext");

    for (label, dir) in [
        ("1000x chained", format!("{}etc", "../".repeat(1000))),
        ("store-edge chained", format!("{}etc", "../".repeat(5))),
    ] {
        fx.dockerfile(&format!("FROM scratch\nWORKDIR {dir}\n"));
        let out = fx.build("t1");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        let container_etc = store.join("containers/etc");
        let store_etc = store.join("etc");
        let home_etc = fx.home.join("etc");
        let scratch_etc = fx._scratch.path().join("etc");
        assert!(
            !container_etc.exists()
                && !store_etc.exists()
                && !home_etc.exists()
                && !scratch_etc.exists(),
            "a {label} WORKDIR reached outside the store; build exit={:?}\n{}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            out.status.code(),
            escaped_listing(
                &format!("WORKDIR {label}"),
                &[&container_etc, &store_etc, &home_etc, &scratch_etc]
            ),
        );
        assert!(
            !out.status.success(),
            "the build must be refused, not succeed ({label})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("WORKDIR"),
            "the refusal must name the WORKDIR instruction ({label}): {combined}"
        );
    }
}

/// POSITIVE CONTROL: an absolute WORKDIR is still created inside the image root
/// AND still recorded as the working_dir.
#[test]
fn an_absolute_workdir_lands_inside_the_rootfs_and_is_recorded() {
    let fx = Fixture::new("absolute");
    fx.dockerfile("FROM scratch\nWORKDIR /srv/app\n");

    let out = fx.build("t1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a normal WORKDIR must build\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let inside = fx.rootfs("t1").join("srv/app");
    assert!(
        inside.is_dir(),
        "the WORKDIR must be created inside the image root at {}",
        inside.display()
    );

    let meta = fs::read_to_string(fx.home.join(".podnext/containers/t1/metadata.json")).unwrap();
    assert!(
        meta.contains("\"working_dir\": \"/srv/app\""),
        "the absolute WORKDIR must be recorded: {meta}"
    );
}

/// POSITIVE CONTROL: Docker's relative chaining still works and still lands
/// inside the rootfs: `WORKDIR /app` then `WORKDIR src` is `/app/src`.
#[test]
fn a_relative_chained_workdir_lands_inside_the_rootfs_and_is_recorded() {
    let fx = Fixture::new("relative");
    fx.dockerfile("FROM scratch\nWORKDIR /app\nWORKDIR src\n");

    let out = fx.build("t1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a relative-chained WORKDIR must build\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let inside = fx.rootfs("t1").join("app/src");
    assert!(
        inside.is_dir(),
        "the chained WORKDIR must be created inside the image root at {}",
        inside.display()
    );

    let meta = fs::read_to_string(fx.home.join(".podnext/containers/t1/metadata.json")).unwrap();
    assert!(
        meta.contains("\"working_dir\": \"/app/src\""),
        "the chained WORKDIR must resolve to /app/src: {meta}"
    );
}
