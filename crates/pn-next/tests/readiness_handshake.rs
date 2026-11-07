//! Start coordination acceptance tests for WORKLIST C10: readiness is one byte
//! on a pipe (runc's `fifoFile.Write([]byte("0"))`), and a start that never gets
//! that byte is a NAMED failure rather than a silent success.
//!
//! The service config is built by parsing TOML rather than by a struct literal,
//! so this exact test source also runs against the base tree, where
//! `readiness_timeout_ms` is an unknown key: there the supervisor spawns the
//! child and returns immediately, which is what the base control quotes.
//!
//! Ordering is asserted, not eventual success: the signalling child writes its
//! byte after `SIGNAL_DELAY_MS` and then lives for 30 s, so a start that returns
//! only after the delay *and* while the child is still alive can only have been
//! released by the byte. Every test bounds its own wait and kills its own child.

use pn_next::{ServiceSupervisor, SupervisorBuilder};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// The bound the service is configured with.
const TIMEOUT_MS: u64 = 400;
/// How long the signalling child takes before it writes its byte.
const SIGNAL_DELAY_MS: u64 = 250;
/// A start that has not returned in this long has failed the test, whatever its
/// answer: every wait below is bounded.
const HARD_BOUND: Duration = Duration::from_secs(5);

/// A supervisor whose service files and runtime state live under `root`.
fn supervisor(root: &Path) -> ServiceSupervisor {
    SupervisorBuilder::new()
        .services_dir(root.join("services"))
        .runtime_dir(root.join("runtime"))
        .log_dir(root.join("logs"))
        .supervision_interval(Duration::from_millis(1000))
        .shutdown_timeout(Duration::from_secs(2))
        .enable_cgroups(false)
        .build()
        .expect("build supervisor")
}

/// A service whose `command` is `sh -c <script>`, with the readiness bound under
/// test set through the config file's own key.
fn service_toml(name: &str, script: &str) -> String {
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"
name = "{name}"
description = "C10 readiness handshake"
version = "1.0.0"
service_type = "daemon"
readiness_timeout_ms = {TIMEOUT_MS}

[process]
command = ["sh", "-c", "{escaped}"]
"#
    )
}

fn config(name: &str, script: &str) -> pn_next::ServiceConfig {
    toml::from_str(&service_toml(name, script)).expect("parse the service config")
}

fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// (b) A child that signals readiness: the start returns once the byte arrives,
/// while the child is still running.
#[tokio::test(flavor = "multi_thread")]
async fn start_returns_when_the_readiness_byte_arrives() {
    let tmp = tempdir().unwrap();
    let sup = supervisor(tmp.path());
    let name = "ready-signal";
    sup.create_service(config(
        name,
        r#"sleep 0.25; printf 0 >&$PN_READY_FD; exec sleep 30"#,
    ))
    .await
    .expect("create");

    let t0 = Instant::now();
    let started = tokio::time::timeout(HARD_BOUND, sup.start_service(name)).await;
    let elapsed = t0.elapsed();
    started
        .expect("start must return, never hang")
        .expect("a child that signals readiness must start");
    // Ordering, not eventual success: the byte is written at 250 ms, so a start
    // that returned earlier would have reported Ready before the child said so.
    assert!(
        elapsed >= Duration::from_millis(SIGNAL_DELAY_MS),
        "start returned in {elapsed:?}, before the readiness byte at {SIGNAL_DELAY_MS}ms"
    );
    let status = sup.get_status(name).await.expect("status");
    assert_eq!(status.state, pn_next::ServiceState::Running);
    let pid = status.pid.expect("a started service has a pid");
    assert!(
        pid_alive(pid),
        "start returned after the child (pid {pid}) was gone: the byte is not what released it"
    );
    eprintln!("C10: start returned {elapsed:?} after the byte, child {pid} still alive");

    let _ = sup.stop_service(name, true).await;
}

/// (b) A child that never signals: the bound elapses and the start fails with a
/// NAMED timeout, inside the hard bound, leaving no process behind.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_that_never_signals_is_a_named_readiness_timeout() {
    let tmp = tempdir().unwrap();
    let sup = supervisor(tmp.path());
    let name = "ready-silent";
    sup.create_service(config(name, "exec sleep 30"))
        .await
        .unwrap();

    let t0 = Instant::now();
    let started = tokio::time::timeout(HARD_BOUND, sup.start_service(name)).await;
    let elapsed = t0.elapsed();

    let err = started
        .expect("the readiness wait must be bounded")
        .expect_err("a start with no readiness byte must fail, never report success");
    let msg = format!("{err:#}");
    assert!(
        msg.contains(&format!("readiness timeout after {TIMEOUT_MS}ms")),
        "the failure is not the named readiness timeout: {msg}"
    );
    assert!(
        msg.contains("PN_READY_FD"),
        "the failure does not name the pipe: {msg}"
    );
    assert!(
        elapsed >= Duration::from_millis(TIMEOUT_MS) && elapsed < HARD_BOUND,
        "the wait was not bounded by the configured {TIMEOUT_MS}ms: {elapsed:?}"
    );
    eprintln!("C10: silent child failed after {elapsed:?} with: {msg}");

    let status = sup.get_status(name).await.expect("status");
    assert_eq!(status.state, pn_next::ServiceState::Failed);
    assert!(status.pid.is_none(), "the failed child must be reaped");
}

/// (b) A child that dies without signalling closes the pipe: the start fails at
/// once with the named "closed" error, instead of waiting out the bound.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_that_exits_without_signalling_fails_before_the_bound() {
    let tmp = tempdir().unwrap();
    let sup = supervisor(tmp.path());
    let name = "ready-dies";
    sup.create_service(config(name, "exit 3")).await.unwrap();

    let t0 = Instant::now();
    let started = tokio::time::timeout(HARD_BOUND, sup.start_service(name)).await;
    let elapsed = t0.elapsed();

    let err = started
        .expect("the readiness wait must be bounded")
        .expect_err("a child that exits without signalling is not a start");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("exited without signalling readiness"),
        "not the named closed-pipe failure: {msg}"
    );
    assert!(
        elapsed < Duration::from_millis(TIMEOUT_MS),
        "the closed pipe should resolve before the {TIMEOUT_MS}ms bound: {elapsed:?}"
    );
    eprintln!("C10: child dying unsignalled failed after {elapsed:?} with: {msg}");
}
