//! The session registry: one reader, three renderings, and the liveness/exit
//! rules behind them (WORKLIST A2, A6, A9, B13).
//!
//! Measured before this file existed (parent-confirmed, one detached session,
//! three commands at the same instant):
//!
//! ```text
//! ps          -> sess-8449 | pdn-pv-sess | /bin/sleep 40 | 8449 | Running
//! sessions    -> only the attach hint; no session listed
//! ps -f json  -> []
//! ```
//!
//! and after `kill <c>` + `remove <c> --force` both returned 0, the store still
//! held `~/.podnext/sessions/8449.log` (0 B) and `8449.sock` (a unix socket)
//! with no process owning them.
//!
//! Every test here drives the real binary (`CARGO_BIN_EXE_podnext`). The
//! hermetic ones point the whole store at a temp root with `PN_TEST_ROOT`, so
//! they neither read nor write the real `~/.podnext`. The one that needs a
//! guest skips with a printed reason when no container is installed: an
//! unrunnable test is not a pass.

use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

fn podnext(args: &[&str]) -> Command {
    let mut c = Command::new(bin());
    c.args(args);
    c
}

/// Run `podnext` with extra environment (the store levers: `PN_TEST_ROOT`,
/// `RUNTIME_DIR`).
fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = podnext(args);
    for (k, v) in env {
        c.env(k, v);
    }
    c.output()
        .unwrap_or_else(|e| panic!("spawn podnext {args:?}: {e}"))
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or_else(|| {
        panic!(
            "podnext was killed by a signal: {:?}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The `id` of every row in a `ps -f json` / `sessions -f json` array.
fn ids(json: &str) -> Vec<String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("stdout was not json: {e}\n{json}"));
    parsed
        .as_array()
        .unwrap_or_else(|| panic!("the session list must be a JSON array: {json}"))
        .iter()
        .map(|row| {
            row.get("id")
                .and_then(|id| id.as_str())
                .unwrap_or_else(|| panic!("a session row has no id: {row}"))
                .to_string()
        })
        .collect()
}

/// Kernel start time of `pid` (field 22 of `/proc/<pid>/stat`), read here
/// independently of the code under test, or `None` when there is no such process.
fn proc_start_time_opt(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn proc_start_time(pid: u32) -> u64 {
    proc_start_time_opt(pid).expect("read /proc stat")
}

fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat[stat.rfind(')')? + 1..]
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// Hold `LOCK_EX` on a record, the way a live session's own process does: the
/// registry reads a record as live off this lock, so a test that wants the
/// liveness *rule* to be the only thing under test must hold it.
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

/// Write a session record exactly as the runtime does.
fn write_record(sessions: &Path, container: &str, pid: u32, start_time: u64) -> PathBuf {
    let record = sessions.join(format!("{pid}.json"));
    let json = serde_json::json!({
        "container_name": container,
        "pid": pid,
        "started_at": 1_700_000_000u64,
        "login_uid": "0",
        "cmdline": ["/bin/sleep", "30"],
        "start_time": start_time,
    });
    std::fs::write(&record, json.to_string()).expect("write record");
    record
}

/// A hermetic store: everything `podnext` touches lives under this temp root
/// (`PN_TEST_ROOT`).
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

    fn env(&self) -> String {
        self.tmp.path().to_string_lossy().into_owned()
    }

    fn sessions(&self) -> PathBuf {
        self.tmp.path().join(".podnext").join("sessions")
    }

    fn container_dir(&self, name: &str) -> PathBuf {
        self.tmp
            .path()
            .join(".podnext")
            .join("containers")
            .join(name)
    }

    /// Run `podnext` against this root.
    fn run(&self, args: &[&str]) -> Output {
        run(args, &[("PN_TEST_ROOT", &self.env())])
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

/// Removes a container directory when a test ends, however it ends.
struct ContainerGuard(PathBuf);

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A process a test started: killed and reaped on drop.
struct ChildGuard {
    child: Option<Child>,
    pid: u32,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        let pid = child.id();
        Self {
            child: Some(child),
            pid,
        }
    }

    /// For a process this test did not spawn (the detached supervisor is the
    /// launcher's child): only the pid is needed to signal it.
    fn from_pid(pid: u32) -> Self {
        Self { child: None, pid }
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

/// A container to run a guest in, or `None` with the reason printed.
///
/// The container has to be able to run the command, so it must have the
/// binaries: several containers in this store are partial (`alpine` has no
/// `/bin/sh` at all), and picking one of those would fail the guest, not the
/// rule under test.
fn installed_container() -> Option<String> {
    let out = podnext(&["list", "-f", "json"]).output().ok()?;
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let mut candidates: Vec<(String, std::path::PathBuf)> = parsed
        .as_array()?
        .iter()
        .filter_map(|c| {
            Some((
                c.get("name")?.as_str()?.to_string(),
                std::path::PathBuf::from(c.get("rootfs_path")?.as_str()?),
            ))
        })
        .collect();
    // Small, complete containers first.
    let preferred = ["alpine", "test-alpine", "debian", "ubuntu", "deb-pdn"];
    candidates.sort_by_key(|(name, _)| {
        preferred
            .iter()
            .position(|p| name == p)
            .unwrap_or(preferred.len())
    });
    candidates
        .into_iter()
        .find(|(_, rootfs)| rootfs.join("bin/sh").exists() && rootfs.join("bin/sleep").exists())
        .map(|(name, _)| name)
}

// ===== A2: the three renderings agree =====

#[test]
fn the_three_renderings_agree_while_a_detached_session_runs() {
    let Some(container) = installed_container() else {
        eprintln!(
            "skipped: no container installed on this host; the renderings need a real detached \
             session (install one with `podnext install alpine` to exercise A2)"
        );
        return;
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let sessions = tmp.path().join("sessions");
    std::fs::create_dir_all(&sessions).expect("sessions dir");
    let runtime = tmp.path().to_string_lossy().into_owned();
    let env: [(&str, &str); 1] = [("RUNTIME_DIR", &runtime)];

    // A real detached session, with its registry redirected into the temp dir
    // (the supervisor inherits the environment) so the real store is untouched.
    let started = run(
        &["run", "--detach", &container, "--", "/bin/sleep", "30"],
        &env,
    );
    assert_eq!(
        code(&started),
        0,
        "run --detach failed: {}",
        stderr(&started)
    );
    let supervisor: u32 = text(&started).trim().parse().unwrap_or_else(|_| {
        panic!(
            "`run --detach` must print the session pid, got {:?}",
            text(&started)
        )
    });
    let _guard = ChildGuard::from_pid(supervisor);

    let id = format!("sess-{supervisor}");
    let ps_table = run(&["ps"], &env);
    let sessions_table = run(&["sessions"], &env);
    let ps_json = run(&["ps", "-f", "json"], &env);
    let sessions_json = run(&["sessions", "-f", "json"], &env);

    for (name, out) in [
        ("ps", &ps_table),
        ("sessions", &sessions_table),
        ("ps -f json", &ps_json),
        ("sessions -f json", &sessions_json),
    ] {
        assert_eq!(code(out), 0, "`{name}` failed: {}", stderr(out));
    }

    // The table rendering of `ps` and of `sessions` both list the session...
    assert!(
        text(&ps_table).contains(&id),
        "`ps` did not list {id}: {}",
        text(&ps_table)
    );
    assert!(
        text(&sessions_table).contains(&id),
        "`sessions` did not list {id}: {}",
        text(&sessions_table)
    );
    // ...and the JSON is non-empty and carries the same ids, from the same
    // reader (the acceptance criterion: `ps -f json` used to print `[]`).
    let ps_ids = ids(&text(&ps_json));
    let sessions_ids = ids(&text(&sessions_json));
    assert!(
        !ps_ids.is_empty(),
        "`ps -f json` printed an empty list while a guest is running"
    );
    assert!(
        ps_ids.contains(&id),
        "`ps -f json` did not contain {id}: {ps_ids:?}"
    );
    assert_eq!(
        ps_ids, sessions_ids,
        "the two JSON renderings disagree about the same instant"
    );
    assert!(
        text(&sessions_json).contains(&id),
        "`sessions -f json` did not contain {id}: {}",
        text(&sessions_json)
    );

    // Teardown: no file of this session may outlive it (A9) - not because a
    // later reader happened to prune it, but as part of the kill.
    let killed = run(&["kill", &container], &env);
    assert_eq!(code(&killed), 0, "kill failed: {}", stderr(&killed));
    let left: Vec<String> = std::fs::read_dir(&sessions)
        .expect("sessions dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&supervisor.to_string()))
        .collect();
    assert!(
        left.is_empty(),
        "the killed session left {left:?} in {}",
        sessions.display()
    );
}

// ===== A6: the guest's own exit status =====

/// The proot process podnext spawned: its direct child whose `comm` names proot.
fn proot_child_of(parent: u32) -> Option<u32> {
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let comm = &stat[open + 1..close];
        let ppid = stat[close + 1..]
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u32>().ok());
        if ppid == Some(parent) && comm.contains("proot") {
            return Some(pid);
        }
    }
    None
}

/// A guest whose proot process is killed by SIGKILL from outside.
///
/// This is the case the runtime cannot report as an exit code: `wait()` yields
/// no code at all, and collapsing that to -1 is how `podnext` came to exit 255
/// for the guest's own 137 (measured on base 45476fa: 255; with the fix: 137).
#[test]
fn a_guest_killed_by_sigkill_from_outside_surfaces_as_137() {
    let Some(container) = installed_container() else {
        eprintln!("skipped: no container installed; A6 needs a guest to kill");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().to_string_lossy().into_owned();

    let mut child = Command::new(bin())
        .args(["run", &container, "--", "/bin/sleep", "6"])
        .env("RUNTIME_DIR", &runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn podnext run");
    let podnext_pid = child.id();

    let mut proot = None;
    for _ in 0..200 {
        proot = proot_child_of(podnext_pid);
        if proot.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let proot = proot.expect("podnext must have spawned a proot process");
    std::thread::sleep(std::time::Duration::from_millis(300));
    unsafe {
        libc::kill(proot as libc::pid_t, libc::SIGKILL);
    }

    let status = child.wait().expect("wait for podnext");
    assert_eq!(
        status.code(),
        Some(137),
        "a guest killed by SIGKILL must surface as 128+9, got {:?}",
        status
    );
}

/// A guest that SIGKILLs itself: the shell is the tracee, and PRoot propagates
/// `128 + N` as a normal exit code. This held before the fix too - it is the
/// regression guard on the way past.
#[test]
fn a_guest_sigkilling_itself_keeps_137() {
    let Some(container) = installed_container() else {
        eprintln!("skipped: no container installed; A6 needs a guest to kill");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().to_string_lossy().into_owned();
    let out = run(
        &["run", &container, "--", "/bin/sh", "-c", "kill -9 $$"],
        &[("RUNTIME_DIR", &runtime)],
    );
    assert_eq!(
        code(&out),
        137,
        "a guest that SIGKILLs itself must surface as 128+9; stderr: {}",
        stderr(&out)
    );
}

#[test]
fn a_guest_exit_code_passes_through() {
    let Some(container) = installed_container() else {
        eprintln!("skipped: no container installed; A6 needs a guest to exit with 3");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().to_string_lossy().into_owned();
    let out = run(
        &["run", &container, "--", "/bin/sh", "-c", "exit 3"],
        &[("RUNTIME_DIR", &runtime)],
    );
    assert_eq!(code(&out), 3, "stderr: {}", stderr(&out));
}

/// A signal forwarded from podnext itself: the guest is killed by the SIGTERM
/// podnext delivered, and the caller must see 143.
///
/// Measured: PRoot does not die of that signal here, it exits with code 255
/// (`raw wait status: code=Some(255) signal=None`), the same value as the
/// runtime's no-status sentinel - so this is the one case where the forwarded
/// signal is what identifies the status. Dropping it (as an interim version of
/// this fix did) made this case report 255 again.
#[test]
fn a_guest_killed_by_a_forwarded_signal_surfaces_as_143() {
    let Some(container) = installed_container() else {
        eprintln!("skipped: no container installed; A6 needs a guest to signal");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().to_string_lossy().into_owned();

    let mut child = Command::new(bin())
        .args(["run", &container, "--", "/bin/sleep", "20"])
        .env("RUNTIME_DIR", &runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn podnext run");
    let podnext_pid = child.id();

    // Wait for the guest to be running (its session is registered), then
    // deliver SIGTERM to podnext: it forwards it to the guest.
    for _ in 0..200 {
        let ps = run(&["ps", "-f", "json"], &[("RUNTIME_DIR", &runtime)]);
        if !ids(&text(&ps)).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    unsafe {
        libc::kill(podnext_pid as libc::pid_t, libc::SIGTERM);
    }

    let status = child.wait().expect("wait for podnext");
    assert_eq!(
        status.code(),
        Some(143),
        "a guest killed by the forwarded SIGTERM must surface as 128+15, got {:?}",
        status
    );
}

/// 255 belongs to a runtime failure, so it must not be explained away as a
/// signal: a guest that exits 255 gets 255 and no SIGILL hint (the hint used to
/// be printed for *every* 255).
#[test]
fn an_unrelated_255_gets_no_sigill_hint() {
    let Some(container) = installed_container() else {
        eprintln!("skipped: no container installed; A6 needs a guest to exit with 255");
        return;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().to_string_lossy().into_owned();
    let out = run(
        &["run", &container, "--", "/bin/sh", "-c", "exit 255"],
        &[("RUNTIME_DIR", &runtime)],
    );
    assert_eq!(code(&out), 255);
    let err = stderr(&out);
    assert!(
        !err.contains("SIGILL") && !err.contains("signal 4"),
        "a 255 that is the guest's own status must not be explained as SIGILL: {err}"
    );
}

// ===== A9: teardown leaves nothing behind =====

#[test]
fn kill_then_remove_leaves_no_session_files_behind() {
    let root = TestRoot::new();
    let sessions = root.sessions();

    // A container of this test's own, so `remove` can be exercised for real
    // without touching an installed one. This is exactly the shape a
    // `FROM scratch` local build produces.
    let name = "pdn-sess-teardown";
    let cdir = root.container_dir(name);
    std::fs::create_dir_all(cdir.join("rootfs")).expect("rootfs");
    std::fs::write(
        cdir.join("metadata.json"),
        r#"{"name":"pdn-sess-teardown:latest"}"#,
    )
    .expect("metadata");
    let _container = ContainerGuard(cdir.clone());

    // A real process stands in for the session's supervisor. `exec sleep` so the
    // recorded pid *is* the process, not a shell that would leave an orphan.
    let child = Command::new("/bin/sh")
        .args(["-c", "exec sleep 30"])
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn the session process");
    let guard = ChildGuard::new(child);
    let pid = guard.pid;
    let start = proc_start_time(pid);
    let _child = guard;

    let record = write_record(&sessions, name, pid, start);
    std::fs::write(sessions.join(format!("{pid}.sock")), b"").expect("socket file");
    std::fs::write(sessions.join(format!("{pid}.log")), b"").expect("log file");
    // The lock is what the registry reads as "a live session".
    let _lock = lock_ex(&record);

    // Residue from a supervisor that was killed by a signal and never reaped:
    // no record, no owner. `remove` must reclaim it too.
    let dead = 0x7fff_ffe0u32;
    std::fs::write(sessions.join(format!("{dead}.sock")), b"").expect("stale socket");
    std::fs::write(sessions.join(format!("{dead}.log")), b"").expect("stale log");

    let killed = root.run(&["kill", name]);
    assert_eq!(code(&killed), 0, "kill failed: {}", stderr(&killed));
    assert!(
        text(&killed).contains(&pid.to_string()),
        "kill did not report the session it killed: {}",
        text(&killed)
    );

    let removed = root.run(&["remove", name, "--force"]);
    assert_eq!(code(&removed), 0, "remove failed: {}", stderr(&removed));

    assert!(
        root.session_files().is_empty(),
        "kill + remove returned 0 but {} still holds {:?}",
        sessions.display(),
        root.session_files()
    );
}

// ===== B13/C8: liveness is the inherited descriptor, not the pid =====

/// (a) A record whose descriptor nobody holds is dead, however alive the pid it
/// names is. `kill(pid, 0)` calls this session running; the descriptor says it
/// ended, and the descriptor is the evidence that cannot be transferred to an
/// unrelated process. The record is pruned, and says why (C8).
#[test]
fn a_free_descriptor_is_death_even_when_the_pid_is_alive() {
    let root = TestRoot::new();
    let sessions = root.sessions();

    let live = ChildGuard::new(
        Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn the session process"),
    );
    let pid = live.pid;
    let record = write_record(&sessions, "pdn-sess-freed", pid, proc_start_time(pid));

    // The pid is real and running: a pid probe (or the old `process_is_alive`)
    // would report this session as live.
    assert!(
        matches!(proc_state(pid), Some(s) if s != 'Z'),
        "the stand-in process must be running for this test to mean anything"
    );
    // ...and no process holds the descriptor, because the registering process
    // is gone.

    let out = run(
        &["ps", "-f", "json"],
        &[("PN_TEST_ROOT", &root.env()), ("PN_DEBUG", "1")],
    );
    assert_eq!(code(&out), 0, "ps failed: {}", stderr(&out));
    let listed = ids(&text(&out));
    assert!(
        !listed.contains(&format!("sess-{pid}")),
        "a free descriptor is death, whatever the pid looks like: {listed:?}"
    );
    assert!(
        !record.exists(),
        "the stale record was not pruned: {} still exists",
        record.display()
    );
    assert!(
        stderr(&out).contains("the descriptor for") && stderr(&out).contains("lock free"),
        "pruning must say it was decided by the descriptor: {}",
        stderr(&out)
    );
}

/// (b) Liveness is the descriptor, not the pid it names. This test holds the
/// record's `LOCK_EX` - the way the session's own process does - while the
/// record names pid 1: a real, live process that is *not* the lock holder, with
/// a start time that is not its own. A pid probe calls that dead. The descriptor
/// calls it live, and the descriptor is right: a held lock cannot be acquired by
/// an unrelated process, while a pid is handed out again by the kernel.
///
/// The same record is then released and read again: nothing about the record
/// changed, only the descriptor, and now it is death.
#[test]
fn a_held_descriptor_decides_liveness_not_the_pid_it_names() {
    let root = TestRoot::new();
    let sessions = root.sessions();

    // Positive control: a live process, recorded as itself.
    let live = ChildGuard::new(
        Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn the live session process"),
    );
    let live_pid = live.pid;
    let live_record = write_record(
        &sessions,
        "pdn-sess-live",
        live_pid,
        proc_start_time(live_pid),
    );
    let _live_lock = lock_ex(&live_record);

    // The subject: pid 1, alive, and not the process holding this lock. Its
    // start time is deliberately not pid 1's, so nothing here relies on the pid
    // naming the recorded process.
    let foreign = 1u32;
    let foreign_start = proc_start_time_opt(foreign).map_or(1, |t| t + 1);
    let record = write_record(&sessions, "pdn-sess-foreign-pid", foreign, foreign_start);
    let held = lock_ex(&record);

    let out = root.run(&["ps", "-f", "json"]);
    assert_eq!(code(&out), 0, "ps failed: {}", stderr(&out));
    let listed = ids(&text(&out));
    assert!(
        listed.contains(&format!("sess-{live_pid}")),
        "a live session with a matching start time must be listed: {listed:?}"
    );
    assert!(
        listed.contains(&format!("sess-{foreign}")),
        "a held descriptor is liveness even though pid {foreign} is not the lock holder: \
         {listed:?}"
    );
    assert!(
        record.exists(),
        "a record whose descriptor is held must not be pruned: {}",
        record.display()
    );

    // Drop the descriptor: same record, same pid, and now it is dead.
    drop(held);
    let after = root.run(&["ps", "-f", "json"]);
    assert_eq!(code(&after), 0);
    let listed_after = ids(&text(&after));
    assert!(
        !listed_after.contains(&format!("sess-{foreign}")),
        "the session survived the release of its descriptor: {listed_after:?}"
    );
    assert!(!record.exists(), "the freed record was not pruned");
    assert!(
        listed_after.contains(&format!("sess-{live_pid}")),
        "the still-held record in this store must be unaffected: {listed_after:?}"
    );
}

/// A record whose pid is gone is still a live session while its descriptor is
/// held. That is the case the pid probe got exactly backwards: the launcher
/// exits (or is killed), a descendant that inherited the lock keeps running the
/// guest, and the old rule pruned the record and lost a running session. The pid
/// is not evidence, so it is not consulted - and `kill` refuses to signal a pid
/// that no longer names the registrant rather than hit whoever holds it now.
#[test]
fn a_record_whose_pid_is_gone_survives_while_the_descriptor_is_held() {
    let root = TestRoot::new();
    let dead = 0x7fff_ffe1u32;
    let record = write_record(&root.sessions(), "pdn-sess-orphan", dead, 12345);
    let _lock = lock_ex(&record);

    let out = root.run(&["ps", "-f", "json"]);
    assert_eq!(code(&out), 0, "ps failed: {}", stderr(&out));
    let listed = ids(&text(&out));
    assert!(
        listed.contains(&format!("sess-{dead}")),
        "a held descriptor is liveness even when pid {dead} does not exist: {listed:?}"
    );
    assert!(record.exists(), "a held record was pruned");

    // The pid cannot be signalled safely: nothing may be sent to that number.
    let killed = root.run(&["kill", "pdn-sess-orphan"]);
    assert!(
        stderr(&killed).contains("nothing can be signalled safely there"),
        "kill must refuse a pid that is not the registrant: {}",
        stderr(&killed)
    );
}
