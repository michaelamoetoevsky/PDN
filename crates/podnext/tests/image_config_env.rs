//! The image's own config, applied at `login` (WORKLIST B5, issue #76), and the
//! kill ladder (WORKLIST B8).
//!
//! **What was measured before this file existed.** `podnext build local` records
//! `env`/`working_dir`/`cmd` in the image's `metadata.json` (#29), and
//! `podnext run` reinstates them; `podnext login` on the same image did not, so
//! the guest logged into an image built with `ENV GREETING=hi` saw `GREETING`
//! unset while `run` saw `hi`. In the other direction `podnext kill` sent
//! `SIGQUIT` and then escalated straight to `SIGKILL`, taking neither the
//! reference's first rung (`SIGTERM`, `commands/proot-distro/kill.py:234-236`)
//! nor a signal the caller named.
//!
//! **How each half is evidenced here, without a network and without depending on
//! a healthy guest.**
//!
//! * Persistence is driven through the real binary: an image is built with
//!   `FROM scratch` (no registry pull) carrying `ENV`, `WORKDIR` and `CMD`, and
//!   the `metadata.json` the build wrote is asserted field by field.
//! * Application at `login` needs a guest that can report its own environment,
//!   so the fixture pattern is used: a candidate container is probed with
//!   `run <c> -- /bin/true`, the first that works is named on stderr, and its
//!   rootfs is copied into a temp store (never linked, never written) carrying
//!   the image config under test. When no candidate can run a trivial command,
//!   one loud `FIXTURE-UNAVAILABLE:` line says what was probed and what came
//!   back, and the test returns: an unrunnable test is not a pass.
//! * The ladder is exercised on real processes: the session registry is pointed
//!   at a temp root with `PN_TEST_ROOT`, the record is written exactly as the
//!   runtime writes it, and the process behind it is a child this test started -
//!   one that ignores the signals it must ignore, so no rung can be passed by
//!   accident.
//!
//! Nothing here reads or writes `~/.podnext`.

use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

fn podnext(args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(bin());
    c.args(args);
    for (k, v) in env {
        c.env(k, v);
    }
    c
}

fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    podnext(args, env)
        .output()
        .unwrap_or_else(|e| panic!("spawn podnext {args:?}: {e}"))
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or_else(|| {
        panic!(
            "podnext was killed by a signal: {:?}; stderr: {}",
            out.status,
            stderr(out)
        )
    })
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Kernel start time of `pid` (field 22 of `/proc/<pid>/stat`), read here
/// independently of the code under test.
fn proc_start_time(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("read /proc stat");
    let rest = &stat[stat.rfind(')').expect("comm is parenthesised") + 1..];
    rest.split_whitespace()
        .nth(19)
        .expect("field 22")
        .parse()
        .expect("start time is numeric")
}

/// The process state field of `/proc/<pid>/stat`, or `None` when the process is
/// gone. `T` is a stopped process, which is what a delivered `SIGSTOP` leaves.
fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat[stat.rfind(')')? + 1..]
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// A hermetic store: everything `podnext` touches lives under this temp root.
///
/// `PN_TEST_ROOT` is what the container store resolves through, and `HOME` is
/// what `build local` resolves its output directory through, so both are pointed
/// at the same temp root - the operator's `~/.podnext` is never read or written.
struct TestRoot {
    tmp: tempfile::TempDir,
}

impl TestRoot {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join(".podnext").join("containers"))
            .expect("containers");
        std::fs::create_dir_all(tmp.path().join(".podnext").join("sessions")).expect("sessions");
        Self { tmp }
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        let v = self.tmp.path().to_string_lossy().into_owned();
        vec![("PN_TEST_ROOT", v.clone()), ("HOME", v)]
    }

    fn run(&self, args: &[&str]) -> Output {
        let env = self.env();
        let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        run(args, &refs)
    }

    fn sessions(&self) -> PathBuf {
        self.tmp.path().join(".podnext").join("sessions")
    }

    /// The container directory for `name` inside this temp store.
    fn container_dir(&self, name: &str) -> PathBuf {
        self.tmp
            .path()
            .join(".podnext")
            .join("containers")
            .join(name)
    }

    /// The session files still present, sorted.
    fn session_files(&self) -> Vec<String> {
        let mut left: Vec<String> = std::fs::read_dir(self.sessions())
            .expect("sessions dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        left
    }
}

/// A process a test started: killed and reaped on drop.
struct ChildGuard {
    child: Option<Child>,
    pid: u32,
}

impl ChildGuard {
    /// `/bin/sh -c <script>`, with `exec` so the recorded pid *is* the process
    /// rather than a shell that would leave a survivor behind.
    fn shell(script: &str) -> Self {
        let child = Command::new("/bin/sh")
            .args(["-c", script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the session process");
        let pid = child.id();
        Self {
            child: Some(child),
            pid,
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.pid as libc::pid_t, libc::SIGKILL);
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.wait();
        }
    }
}

/// Write a session record exactly as the runtime does.
fn write_record(sessions: &Path, container: &str, pid: u32, start_time: u64) -> PathBuf {
    let record = sessions.join(format!("{pid}.json"));
    let json = serde_json::json!({
        "container_name": container,
        "pid": pid,
        "started_at": 1_700_000_000u64,
        "login_uid": "0",
        "cmdline": ["/bin/sleep", "60"],
        "start_time": start_time,
    });
    std::fs::write(&record, json.to_string()).expect("write record");
    record
}

/// Hold `LOCK_EX` on a record, the way a live session's own process does: the
/// registry reads a record as live off this lock.
fn lock_ex(path: &Path) -> std::fs::File {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("open record");
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    assert_eq!(ret, 0, "flock LOCK_EX on {}", path.display());
    file
}

/// A live record for `pid`, with the lock that makes the registry see it.
fn live_session(root: &TestRoot, container: &str, pid: u32) -> std::fs::File {
    let record = write_record(&root.sessions(), container, pid, proc_start_time(pid));
    lock_ex(&record)
}

// ===== B5: the image config, and `Env` at login =====

/// The metadata a container's build wrote, as the run/login path reads it.
fn read_metadata(container_dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(container_dir.join("metadata.json")).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}",
            container_dir.join("metadata.json").display()
        )
    });
    serde_json::from_str(&text).expect("metadata.json is json")
}

#[test]
fn the_image_config_is_persisted_by_a_build() {
    // Offline: `FROM scratch` pulls nothing, and the config the image states
    // (`Env`, `WorkingDir`, `Cmd`) must be the config its metadata carries -
    // before #76 nothing in a container's metadata was read back at login.
    let root = TestRoot::new();
    let ctx = tempfile::tempdir().expect("build context");
    std::fs::write(
        ctx.path().join("Dockerfile"),
        "FROM scratch\n\
         ENV PDN_IMG_ENV=hello-from-image\n\
         WORKDIR /srv/app\n\
         CMD [\"/bin/sh\", \"-c\", \"true\"]\n",
    )
    .expect("Dockerfile");

    let built = root.run(&[
        "build",
        "local",
        "-t",
        "pdn-imgenv:latest",
        "-f",
        &ctx.path().join("Dockerfile").to_string_lossy(),
        &ctx.path().to_string_lossy(),
    ]);
    assert_eq!(code(&built), 0, "build failed: {}", stderr(&built));

    let meta = read_metadata(&root.container_dir("pdn-imgenv"));
    assert_eq!(
        meta.get("env")
            .and_then(|e| e.get("PDN_IMG_ENV"))
            .and_then(|v| v.as_str()),
        Some("hello-from-image"),
        "the image's Env was not persisted: {meta}"
    );
    assert_eq!(
        meta.get("working_dir").and_then(|v| v.as_str()),
        Some("/srv/app"),
        "the image's WorkingDir was not persisted: {meta}"
    );
    assert_eq!(
        meta.get("cmd").and_then(|v| v.as_array()).cloned(),
        Some(vec![
            serde_json::json!("/bin/sh"),
            serde_json::json!("-c"),
            serde_json::json!("true")
        ]),
        "the image's Cmd was not persisted: {meta}"
    );
}

/// A container that can actually run a command, in a store of this test's own.
///
/// Picked by capability, never by presence in `podnext list`: this device has a
/// container named `alpine` that is a stub with no `/bin/sh`, and a
/// presence-only guard would fail *inside the guest* instead of skipping. Each
/// candidate is probed with `run <c> -- /bin/true` and the first that exits 0
/// wins; its rootfs is then copied - never linked, never written - into the temp
/// store this test owns, so the operator's `~/.podnext/containers` is untouched.
/// `None`, with one loud named line, means no candidate can run a trivial
/// command here: the test is UNRUNNABLE, which is neither a pass nor a product
/// failure.
fn guest_container(root: &TestRoot) -> Option<PathBuf> {
    let candidates = std::env::var("PODNEXT_FIXTURE_CANDIDATES")
        .unwrap_or_else(|_| "test-alpine,alpine,deb-pdn,ubuntu".to_string());
    let mut chosen = None;
    let mut results = Vec::new();
    for name in candidates
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        let out = run(&["run", name, "--", "/bin/true"], &[]);
        if code(&out) == 0 {
            eprintln!("fixture: using `{name}` - `run {name} -- /bin/true` exited 0");
            chosen = Some(name.to_string());
            break;
        }
        let why = stderr(&out);
        let why = why
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .unwrap_or("");
        eprintln!(
            "fixture: rejected `{name}` - `run {name} -- /bin/true` exited {:?} ({})",
            out.status.code(),
            why.trim()
        );
        results.push(format!("{name}={:?}", out.status.code()));
    }
    let Some(name) = chosen else {
        eprintln!(
            "FIXTURE-UNAVAILABLE: no container that can run a trivial command was found, so this \
             test is UNRUNNABLE here (not a pass, not a product failure). probed: [{}]; results: \
             [{}]",
            candidates,
            results.join(", ")
        );
        return None;
    };

    // Its rootfs, from the store's own listing (the fixture lives in the real
    // store; the copy lands in the temp one this test owns).
    let listed = run(&["list", "-f", "json"], &[]);
    let parsed: serde_json::Value =
        serde_json::from_slice(&listed.stdout).expect("`list -f json` is json");
    let src: PathBuf = parsed
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
        })
        .and_then(|c| c.get("rootfs_path").and_then(|p| p.as_str()))
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("`{name}` was probed but has no rootfs in `list -f json`"));

    let dst = root.container_dir("pdn-imgenv");
    let dst_rootfs = dst.join("rootfs");
    std::fs::create_dir_all(&dst_rootfs).expect("temp container dir");
    // `src/.` copies the *contents* into a directory this test owns. A fixture's
    // rootfs carries the masked placeholder directories a guest is given
    // (`apex`, `system`, ... are mode `d---------`), which proot binds over from
    // the host anyway, so `cp` cannot read them and reports them as errors while
    // the copy that matters - the guest's own `/bin/sh` and libraries - is
    // complete. Whether it is usable is not assumed from the exit status: the
    // copy is probed the same way the fixture was.
    let copied = Command::new("cp")
        .arg("-a")
        .arg(src.join("."))
        .arg(&dst_rootfs)
        .status();
    let probe = root.run(&["run", "pdn-imgenv", "--", "/bin/true"]);
    if code(&probe) != 0 {
        eprintln!(
            "FIXTURE-UNAVAILABLE: the fixture `{name}` cannot be reproduced in a temp store - \
             `cp -a {} {}/` reported {copied:?} and the copy cannot run `/bin/true` (exited {}: \
             {}). This test is UNRUNNABLE here (not a pass, not a product failure); the fixture's \
             rootfs was not modified.",
            src.display(),
            dst.display(),
            code(&probe),
            stderr(&probe).lines().last().unwrap_or("").trim()
        );
        let _ = std::fs::remove_dir_all(&dst);
        return None;
    }
    eprintln!(
        "fixture: `{name}` copied into the temp store ({} -> {}) and runs `/bin/true`",
        src.display(),
        dst.display()
    );
    Some(dst)
}

#[test]
fn a_login_delivers_the_images_env_to_the_guest() {
    let root = TestRoot::new();
    let Some(cdir) = guest_container(&root) else {
        return;
    };

    // The image config under test, in the exact shape the build path persists it
    // (asserted by the test above). It is written here rather than produced by a
    // pull because the acceptance is about what `login` delivers, and a pull is
    // not available offline.
    std::fs::write(
        cdir.join("metadata.json"),
        serde_json::json!({
            "name": "pdn-imgenv",
            "image": "pdn-imgenv",
            "tag": "latest",
            "env": { "PDN_IMG_ENV": "hello-from-image" },
            "working_dir": "/srv/app",
            "cmd": ["/bin/sh", "-c", "true"],
            "status": "Stopped",
        })
        .to_string(),
    )
    .expect("metadata.json");
    // WORKDIR is not materialised by a build (the C5 gap, not this change), and
    // proot cannot chdir into a directory that does not exist.
    std::fs::create_dir_all(cdir.join("rootfs").join("srv").join("app")).expect("workdir");

    let out = root.run(&[
        "login",
        "pdn-imgenv",
        "--",
        "/bin/sh",
        "-c",
        "printf 'ENV=%s CWD=%s\\n' \"$PDN_IMG_ENV\" \"$PWD\"",
    ]);
    assert_eq!(
        code(&out),
        0,
        "login into the container failed: {}",
        stderr(&out)
    );
    let seen = text(&out);
    eprintln!("guest reported: {}", seen.trim());
    assert!(
        seen.contains("ENV=hello-from-image"),
        "the guest did not see the image's Env: stdout={seen:?} stderr={:?}",
        stderr(&out)
    );
    assert!(
        seen.contains("CWD=/srv/app"),
        "the guest did not start in the image's WorkingDir: stdout={seen:?}"
    );
}

// ===== B8: the kill ladder =====

/// Poll `f` until it is true or `deadline` passes; returns whether it became
/// true. Signals are queued, not applied synchronously, so a state change is
/// waited for rather than assumed to have already happened.
fn eventually(deadline: std::time::Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + deadline;
    loop {
        if f() {
            return true;
        }
        if std::time::Instant::now() >= end {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `Sent SIGTERM ...`, `Sent SIGQUIT ...`, `Sent SIGKILL ...` in that order.
fn assert_ladder_order(out: &str, pid: u32) {
    let positions: Vec<Option<usize>> = ["SIGTERM", "SIGQUIT", "SIGKILL"]
        .iter()
        .map(|sig| out.find(&format!("Sent {sig} to session PID {pid}")))
        .collect();
    assert!(
        positions.iter().all(|p| p.is_some()),
        "the ladder did not report every rung for pid {pid}: {out:?}"
    );
    let at: Vec<usize> = positions.into_iter().flatten().collect();
    assert!(
        at[0] < at[1] && at[1] < at[2],
        "the ladder was reported out of order (TERM, QUIT, KILL expected): {out:?}"
    );
}

#[test]
fn a_guest_ignoring_term_is_escalated_term_quit_kill() {
    let root = TestRoot::new();
    let name = "pdn-kill-escalation";

    // A process that ignores SIGTERM and SIGQUIT: an ignored disposition
    // survives `exec`, so the `sleep` inherits it and only SIGKILL can end it.
    // No rung of the ladder can be satisfied by accident.
    let child = ChildGuard::shell("trap '' TERM QUIT; exec sleep 60");
    let pid = child.pid;
    let _lock = live_session(&root, name, pid);

    let out = root.run(&["kill", name]);
    assert_eq!(code(&out), 0, "kill failed: {}", stderr(&out));
    assert_ladder_order(&text(&out), pid);
    assert!(
        text(&out).contains(&format!("Container '{name}' killed.")),
        "a completed ladder must say the container is gone: {}",
        text(&out)
    );
    assert!(
        eventually(std::time::Duration::from_secs(5), || matches!(
            proc_state(pid),
            None | Some('Z') | Some('X')
        )),
        "the process survived the ladder (state {:?})",
        proc_state(pid)
    );
    assert!(
        root.session_files().is_empty(),
        "the ladder killed the session but left {:?}",
        root.session_files()
    );
}

#[test]
fn a_named_signal_is_delivered_once_and_never_escalated() {
    let root = TestRoot::new();
    let name = "pdn-kill-named";

    // Ignores TERM but *dies* on QUIT and KILL, so a single unrequested
    // escalation - the second rung of the ladder, or the `SIGQUIT` the container
    // manager sends as it sweeps the sessions - would show up as a dead process.
    let child = ChildGuard::shell("trap '' TERM; exec sleep 60");
    let pid = child.pid;
    let _lock = live_session(&root, name, pid);

    let out = root.run(&["kill", name, "--signal", "TERM"]);
    assert_eq!(code(&out), 0, "kill -s TERM failed: {}", stderr(&out));
    let seen = text(&out);
    assert!(
        seen.contains(&format!("Sent SIGTERM to session PID {pid}")),
        "the asked-for signal was not reported: {seen:?}"
    );
    assert!(
        !seen.contains("SIGQUIT") && !seen.contains("SIGKILL"),
        "a signal the caller asked for was escalated: {seen:?}"
    );
    assert!(
        proc_state(pid).is_some(),
        "the process ignoring SIGTERM was gone, so something else was sent"
    );
}

#[test]
fn a_stop_asked_for_is_delivered_as_asked() {
    let root = TestRoot::new();
    let name = "pdn-kill-stop";

    // Dies on TERM and QUIT, and is *stopped* by STOP: the state it ends in says
    // which signal it actually got.
    let child = ChildGuard::shell("exec sleep 60");
    let pid = child.pid;
    let _lock = live_session(&root, name, pid);

    let out = root.run(&["kill", name, "--signal", "STOP"]);
    assert_eq!(code(&out), 0, "kill -s STOP failed: {}", stderr(&out));
    let seen = text(&out);
    assert!(
        seen.contains(&format!("Sent SIGSTOP to session PID {pid}")),
        "STOP was not reported as delivered: {seen:?}"
    );
    assert!(
        !seen.contains("SIGTERM") && !seen.contains("SIGQUIT") && !seen.contains("SIGKILL"),
        "a signal the caller asked for was escalated: {seen:?}"
    );
    assert!(
        eventually(std::time::Duration::from_secs(5), || proc_state(pid)
            == Some('T')),
        "the guest was not stopped by the SIGSTOP it was asked to receive (state {:?})",
        proc_state(pid)
    );
}

#[test]
fn an_unknown_signal_is_refused_by_name() {
    let root = TestRoot::new();
    let name = "pdn-kill-unknown";

    let child = ChildGuard::shell("exec sleep 60");
    let pid = child.pid;
    let _lock = live_session(&root, name, pid);

    let out = root.run(&["kill", name, "--signal", "NOSUCHSIGNAL"]);
    assert_eq!(
        code(&out),
        1,
        "an unknown signal is a refusal, not a usage error or a success: {:?}",
        out.status
    );
    let err = stderr(&out);
    assert!(
        err.contains("NOSUCHSIGNAL"),
        "the refusal must name the value it refused: {err}"
    );
    assert!(
        !text(&out).contains("Sent "),
        "a refused signal must not be delivered: {}",
        text(&out)
    );
    assert!(
        proc_state(pid).is_some(),
        "the session was signalled even though the signal was refused"
    );
}
