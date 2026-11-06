//! WORKLIST A10: `daemon status` must not claim a running daemon when there is none,
//! and `service create` must not exit 0 while persisting nothing.
//!
//! WORKLIST A14 (extended here): `daemon stop`, `daemon reload` and `daemon logs`
//! had the same defect one layer down — all three printed a success line and exited 0
//! with nothing listening. They now share `daemon status`'s socket probe and refuse.
//!
//! Both commands are run as the real binary with `HOME`, `XDG_RUNTIME_DIR`,
//! `XDG_CONFIG_HOME` and every `PN_NEXT_*` path redirected into a throwaway directory
//! under the process temp dir. No fixed path, no container name, and no assumption
//! about whether a daemon happens to run on the host: the socket these commands would
//! answer on lives inside the sandbox, so nothing can be listening there.

use std::path::PathBuf;
use std::process::Command;

fn podnext() -> Command {
    Command::new(env!("CARGO_BIN_EXE_podnext"))
}

/// An empty directory tree unique to this test run, holding the socket, the service
/// store and the config/log/runtime dirs. Dropped (and removed) at test end.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "podnext_honesty_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&root).expect("create sandbox root");
        Sandbox { root }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// A `podnext` command with every path input pointed inside the sandbox.
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

/// The dishonest shape is `running: true`. The assertion is branch-independent: the
/// command may exit non-zero, or it may exit 0 and say not-running, but it must never
/// report a running daemon when nothing answers on the socket.
#[test]
fn daemon_status_never_claims_a_daemon_that_is_not_there() {
    let env = Sandbox::new("daemon");
    let out = env
        .command()
        .args(["daemon", "status", "-f", "json"])
        .output()
        .expect("run podnext daemon status");
    let (stdout, stderr) = split(&out);

    let json_running = serde_json::from_str::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|v| v.get("running").and_then(|r| r.as_bool()));
    let claims_running = json_running == Some(true)
        || stdout.contains("\"running\": true")
        || stdout.contains("running: true");
    assert!(
        !claims_running,
        "daemon status claimed a running daemon with no socket reachable: \
         exit {:?}, stdout={stdout:?}, stderr={stderr:?}",
        out.status.code()
    );

    let says_not_running = json_running == Some(false)
        || stdout.contains("\"running\": false")
        || stdout.contains("running: false")
        || stderr.to_lowercase().contains("not running");
    assert!(
        !out.status.success() || says_not_running,
        "with no daemon reachable the command must exit non-zero or report not-running: \
         exit {:?}, stdout={stdout:?}, stderr={stderr:?}",
        out.status.code()
    );

    let observed = if out.status.success() {
        "exit 0, reported not-running"
    } else {
        "exited non-zero"
    };
    println!(
        "daemon status with no daemon reachable: observed form = {observed} (exit {:?})",
        out.status.code()
    );
}

/// `service create` either persists a service that the next CLI call can read back, or
/// it refuses non-zero with a reason. Exiting 0 while storing nothing is the defect.
#[test]
fn service_create_persists_a_retrievable_service_or_refuses() {
    let env = Sandbox::new("service");
    let cfg = env.path("honesty_svc.toml");
    std::fs::write(
        &cfg,
        "name = \"honesty_svc\"\n\n[process]\ncommand = [\"/bin/sleep\", \"100\"]\n",
    )
    .expect("write service config");

    let created = env
        .command()
        .args(["service", "create", "honesty_svc", "--config"])
        .arg(&cfg)
        .output()
        .expect("run podnext service create");
    let (cstdout, cstderr) = split(&created);

    if created.status.success() {
        // It claimed success, so the service must be retrievable by a later call.
        let shown = env
            .command()
            .args(["service", "status", "honesty_svc"])
            .output()
            .expect("run podnext service status");
        let (sstdout, sstderr) = split(&shown);
        assert!(
            shown.status.success(),
            "service create exited 0 but the service could not be read back: \
             status exit {:?}, stdout={sstdout:?}, stderr={sstderr:?}",
            shown.status.code()
        );
        println!(
            "service create persisted a retrievable service (exit {:?}); \
             `service status` reads it back",
            created.status.code()
        );
    } else {
        assert!(
            !cstderr.trim().is_empty() || !cstdout.trim().is_empty(),
            "service create failed without a reason: stdout={cstdout:?}, stderr={cstderr:?}"
        );
        println!(
            "service create refused with a reason: exit {:?}, stderr={cstderr:?}",
            created.status.code()
        );
    }
}

/// A14: `daemon stop`, `daemon reload` and `daemon logs` must refuse when no daemon
/// answers on the socket — the same honesty `daemon status` was given in A10. The
/// base printed a success line to stdout and exited 0 for all three; that line is
/// quoted below as the negative control, and the test fails if it still appears.
#[test]
fn daemon_stop_reload_and_logs_refuse_with_no_daemon() {
    let env = Sandbox::new("daemon_refuse");

    // What each command printed, and exited 0 on, with nothing listening (base).
    let cases: [(&[&str], &str); 3] = [
        (&["daemon", "stop"], "PDN daemon stopped"),
        (&["daemon", "reload"], "Reloading daemon configuration..."),
        (&["daemon", "logs"], "Daemon logs (last 100 lines):"),
    ];

    for (args, base_success_line) in cases {
        let out = env.command().args(args).output().expect("run podnext");
        let (stdout, stderr) = split(&out);

        assert!(
            !out.status.success(),
            "`{}` must refuse with no daemon; exit {:?}, stdout={stdout:?}, stderr={stderr:?}",
            args.join(" "),
            out.status.code()
        );

        let names_reason =
            stderr.contains("not running") || stderr.contains("nothing is listening");
        assert!(
            names_reason,
            "`{}` refused without a named reason: stderr={stderr:?}",
            args.join(" ")
        );

        assert!(
            !stdout.contains(base_success_line),
            "`{}` still printed the base success line {base_success_line:?}: stdout={stdout:?}",
            args.join(" ")
        );

        println!(
            "`{}` with no daemon: exit {:?}, stderr={:?} \
             (base printed {base_success_line:?} to stdout and exited 0)",
            args.join(" "),
            out.status.code(),
            stderr.trim()
        );
    }
}
