//! Roadmap section 9.2: the binary must honour `TERMUX__HOME`, `TERMUX__PREFIX` and the
//! `XDG_*` variables instead of hard-coding one machine's Termux paths. This drives the real
//! binary with a relocated home and prefix and checks that its defaults follow them.

use std::path::PathBuf;
use std::process::Command;

fn scratch(label: &str) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("pdn-relocated-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The defaults under test belong to the `podnext-daemon` binary; `podnext daemon` is the
/// CLI that supervises it and carries no path defaults of its own.
fn daemon_help(env: &[(&str, &str)], remove: &[&str]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_podnext-daemon"));
    cmd.args(["--help"]);
    for key in remove {
        cmd.env_remove(key);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("spawn podnext daemon --help");
    assert!(out.status.success(), "podnext-daemon --help must succeed");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_daemon_defaults_follow_termux_home() {
    let home = scratch("home");
    let help = daemon_help(
        &[("TERMUX__HOME", home.to_str().unwrap())],
        &["XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_STATE_HOME"],
    );
    let socket = home.join(".local/run/pn-next/daemon.sock");
    let services = home.join(".config/pn-next/services");
    let logs = home.join(".local/log/pn-next");
    assert!(
        help.contains(&socket.display().to_string()),
        "the default socket must follow TERMUX__HOME ({socket:?}): {help}"
    );
    assert!(
        help.contains(&services.display().to_string()),
        "the services directory must follow TERMUX__HOME ({services:?}): {help}"
    );
    assert!(
        help.contains(&logs.display().to_string()),
        "the log directory must follow TERMUX__HOME ({logs:?}): {help}"
    );
    assert!(
        !help.contains("/data/data/com.termux/files/home"),
        "no default may name the unrelocated home: {help}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn xdg_variables_win_when_they_are_set() {
    let root = scratch("xdg");
    let runtime = root.join("runtime");
    let config = root.join("config");
    let state = root.join("state");
    for dir in [&runtime, &config, &state] {
        std::fs::create_dir_all(dir).expect("xdg dir");
    }
    let help = daemon_help(
        &[
            ("HOME", root.join("home").to_str().unwrap()),
            ("XDG_RUNTIME_DIR", runtime.to_str().unwrap()),
            ("XDG_CONFIG_HOME", config.to_str().unwrap()),
            ("XDG_STATE_HOME", state.to_str().unwrap()),
        ],
        &["TERMUX__HOME"],
    );
    for expected in [
        runtime.join("pn-next/daemon.sock"),
        config.join("pn-next/services"),
        state.join("pn-next"),
    ] {
        assert!(
            help.contains(&expected.display().to_string()),
            "the default must follow XDG ({expected:?}): {help}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
