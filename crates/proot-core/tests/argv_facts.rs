//! What `proot-core` actually puts in the `proot` argument vector.
//!
//! Workbench WORKLIST B6/B7. The argv builder is inline in
//! `ProotEngine::{start,execute}` (`crates/proot-core/src/lib.rs:200-299`,
//! `:338-457`); there is no function returning an argv the code also reports on
//! itself. So this test observes the argv the way `proot` receives it:
//! `proot_binary()` honours `PROOT_BIN` (`crates/proot-core/src/lib.rs:76-90`), a
//! capture script is installed there, and the real `execute()` path is run. The
//! script records `"$@"` and the guest environment; the test reads both back.
//! Nothing here is a boolean the builder returns about its own behaviour.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct Captured {
    argv: Vec<String>,
    env: Vec<String>,
}

/// An absolute path to `env`, resolved the way the child's `PATH` would.
///
/// The child runs with a cleared environment (`lib.rs:412`), so `env` is looked
/// up once here, on the host, and baked into the capture script.
fn env_binary() -> String {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':').filter(|d| !d.is_empty()) {
            let candidate = Path::new(dir).join("env");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    // Android's toybox `env`, reached through the `/bin` symlink to `/system/bin`.
    "/system/bin/env".to_string()
}

/// Run `execute()` against a capture script and return what the script saw.
async fn capture(label: &str) -> Captured {
    let dir = std::env::temp_dir().join(format!("pdn-argv-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // `execute()` sets `--rootfs=.` and chdirs into the rootfs, so it must exist.
    std::fs::create_dir_all(dir.join("rootfs")).expect("scratch rootfs");

    let argv_file = dir.join("argv");
    let env_file = dir.join("env");
    let script = dir.join("proot-capture.sh");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{} > '{}'\n",
        argv_file.display(),
        env_binary(),
        env_file.display()
    );
    let mut file = std::fs::File::create(&script).expect("create capture script");
    file.write_all(body.as_bytes())
        .expect("write capture script");
    drop(file);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod 0755");

    // `proot_binary()` reads this before spawning, so the built argv lands here.
    std::env::set_var("PROOT_BIN", &script);

    let config = proot_core::ProotConfig {
        rootfs: Some(dir.join("rootfs")),
        command: vec!["true".to_string()],
        ..Default::default()
    };
    let mut engine = proot_core::ProotEngine::new(config).expect("engine");
    let code = engine.execute().await.expect("execute");
    assert_eq!(code, 0, "the capture script exits 0");

    let lines = |path: &PathBuf| -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    };
    Captured {
        argv: lines(&argv_file),
        env: lines(&env_file),
    }
}

/// B7: `--link2symlink` is always in the argv, and `PROOT_NO_MOUNTINFO` never is.
#[tokio::test]
async fn the_built_argv_carries_link2symlink_and_never_proot_no_mountinfo() {
    let cap = capture("b7").await;
    println!("proot argv ({} tokens): {:#?}", cap.argv.len(), cap.argv);

    assert!(
        cap.argv.iter().any(|a| a == "--link2symlink"),
        "--link2symlink is passed unconditionally \
         (crates/proot-core/src/lib.rs:212, :351); argv was {:?}",
        cap.argv
    );
    assert!(
        !cap.argv.iter().any(|a| a.contains("PROOT_NO_MOUNTINFO")),
        "PROOT_NO_MOUNTINFO must never reach the argv; argv was {:?}",
        cap.argv
    );
    assert!(
        !cap.env.iter().any(|e| e.starts_with("PROOT_NO_MOUNTINFO")),
        "the guest environment must not set PROOT_NO_MOUNTINFO; env was {:?}",
        cap.env
    );
}

/// B6: the `/dev/null` -> `/etc/ld.so.preload` bind is proot's own default
/// (`cli/proot.c:142`), inherited by PDN rather than built by it, so PDN's argv
/// must not spend a token on it. The positive control below pins that this
/// absence is about ld.so.preload, not about bind handling being broken.
#[tokio::test]
async fn the_ld_so_preload_bind_is_proots_default_not_pdns() {
    let cap = capture("b6").await;
    println!("proot argv ({} tokens): {:#?}", cap.argv.len(), cap.argv);

    assert!(
        cap.argv.iter().any(|a| a == "--bind=/dev"),
        "--bind=/dev is one of PDN's own binds \
         (crates/proot-core/src/lib.rs:256, :395); argv was {:?}",
        cap.argv
    );
    assert!(
        !cap.argv.iter().any(|a| a.contains("/etc/ld.so.preload")),
        "PDN's argv must not carry the ld.so.preload bind, which proot applies \
         itself; argv was {:?}",
        cap.argv
    );
}
