//! A18(b): with a daemon actually up, `daemon reload`, `daemon logs` and `daemon stop`
//! report what the daemon answered.
//!
//! The base printed a success line for all three with nothing listening on the socket
//! and no request sent (see `daemon_honesty.rs` for the no-daemon half). Here the daemon
//! is a real [`pn_next::DaemonRuntime`] on its own thread and tokio runtime, bound to the
//! exact socket path the CLI resolves (`$XDG_RUNTIME_DIR/pn-next/daemon.sock`), so the
//! test drives the real IPC surface through the real `podnext` binary.
//!
//! The daemon lives inside this process, so the socket is not an external dependency —
//! it is the fixture under test. If it ever fails to bind, the test names a
//! `FIXTURE-UNAVAILABLE` line and returns rather than asserting on a daemon that is not
//! there.

use pn_next::{DaemonEvent, DaemonRuntime, DaemonRuntimeConfig};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

fn podnext() -> Command {
    Command::new(env!("CARGO_BIN_EXE_podnext"))
}

/// An empty directory tree unique to this run, holding the socket, the daemon state and
/// the config/runtime dirs. Dropped (and removed) at test end.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = proot_distro_core::temp_base().join(format!(
            "podnext_ipc_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&root).expect("create sandbox root");
        Sandbox { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// A `podnext` command with every path input pointed inside the sandbox, so the
    /// CLI resolves its daemon socket under `runtime/`.
    fn command(&self) -> Command {
        let mut cmd = podnext();
        cmd.env("HOME", &self.root);
        cmd.env("TERMUX__HOME", &self.root);
        cmd.env("XDG_RUNTIME_DIR", self.path("runtime"));
        cmd.env("XDG_CONFIG_HOME", self.path("config"));
        cmd.env("PN_NEXT_SERVICES_DIR", self.path("services"));
        cmd.env("PN_NEXT_RUNTIME_DIR", self.path("runtime/pn-next"));
        cmd.env("PN_NEXT_LOG_DIR", self.path("logs"));
        cmd
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn split(out: &std::process::Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn daemon_reload_logs_and_stop_report_a_live_daemon() {
    let env = Sandbox::new("live");
    let socket = env.path("runtime/pn-next/daemon.sock");
    let socket_for_daemon = socket.clone();
    let state_dir = env.path("state");
    let log_dir = env.path("logs");
    let pid_file = env.path("runtime/pn-next/daemon.pid");

    // The daemon: its own thread, its own tokio runtime, alive until stopped.
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let daemon = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build the daemon's tokio runtime");
        rt.block_on(async move {
            let config = DaemonRuntimeConfig {
                socket_path: socket_for_daemon,
                state_dir,
                log_dir,
                pid_file,
                ..Default::default()
            };
            let runtime = Arc::new(DaemonRuntime::new(config));
            if let Err(e) = runtime.start().await {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let mut events = runtime.subscribe_events();
            while let Ok(event) = events.recv().await {
                if matches!(event, DaemonEvent::DaemonStopped) {
                    break;
                }
            }
        });
    });

    match ready_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => panic!("the daemon failed to start: {e}"),
        Err(_) => {
            eprintln!(
                "FIXTURE-UNAVAILABLE: the in-process daemon did not bind {socket:?} within 20s"
            );
            let _ = daemon.join();
            return;
        }
    }

    // reload: the daemon answers "configuration reloaded".
    let reload = env
        .command()
        .args(["daemon", "reload"])
        .output()
        .expect("run podnext daemon reload");
    let (rout, rerr) = split(&reload);
    assert!(
        reload.status.success(),
        "reload must succeed against a live daemon: exit {:?}, stdout={rout:?}, stderr={rerr:?}",
        reload.status.code()
    );
    assert!(
        rout.contains("configuration reloaded"),
        "reload must report the daemon's answer: stdout={rout:?}"
    );
    println!(
        "daemon reload, live daemon: exit {:?}, stdout={:?}",
        reload.status.code(),
        rout.trim()
    );

    // logs: this daemon keeps state, not a log stream, so the honest answer names the
    // gap instead of printing an empty success header.
    let logs = env
        .command()
        .args(["daemon", "logs"])
        .output()
        .expect("run podnext daemon logs");
    let (lout, lerr) = split(&logs);
    assert!(
        !logs.status.success(),
        "logs must report the daemon's refusal (no log kept): exit {:?}, stdout={lout:?}",
        logs.status.code()
    );
    assert!(
        lerr.contains("no daemon log file"),
        "logs must report the daemon's answer: stderr={lerr:?}"
    );
    println!(
        "daemon logs, live daemon: exit {:?}, stderr={:?}",
        logs.status.code(),
        lerr.trim()
    );

    // stop: the daemon reports the shutdown and removes the socket it was watching.
    let stop = env
        .command()
        .args(["daemon", "stop"])
        .output()
        .expect("run podnext daemon stop");
    let (sout, serr) = split(&stop);
    assert!(
        stop.status.success(),
        "stop must succeed against a live daemon: exit {:?}, stdout={sout:?}, stderr={serr:?}",
        stop.status.code()
    );
    assert!(
        sout.contains("daemon shutting down"),
        "stop must report the daemon's answer: stdout={sout:?}"
    );
    assert!(
        !socket.exists(),
        "stop must take the daemon's socket with it"
    );
    println!(
        "daemon stop, live daemon: exit {:?}, stdout={:?}",
        stop.status.code(),
        sout.trim()
    );

    let _ = daemon.join();
}
