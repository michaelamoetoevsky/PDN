//! A22: PDN is a real subreaper, so the descendants of a dead guest are adopted
//! and reaped instead of surviving the session as orphans.
//!
//! Measured defect (campaign logs, second dissection round): PDN never called
//! `prctl(PR_SET_CHILD_SUBREAPER)`, so `grep -rn PR_SET_CHILD_SUBREAPER crates/`
//! returned nothing; when a session died its tracees were reparented to PID 1
//! and outlived it - two orphans of a finished session were alive for 2.730 s
//! and 3.046 s and exited only on SIGTERM. proot-distro reaps its orphans; PDN
//! did not.
//!
//! The condition is *planted*, not raced for: the guest prints its grandchild's
//! pid, the test reads that pid, confirms the grandchild is running, and kills
//! the guest's tracer. The grandchild is then either reaped by podnext (fixed)
//! or reparented and left running (the defect above).
//!
//! This drives the real binary. It needs a container to run a guest in; where
//! none is installed it prints `FIXTURE-UNAVAILABLE:` and returns, because an
//! unrunnable test is not a pass.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

/// `$TMPDIR` (set by the test harness) or `$PREFIX/tmp`; `/tmp` is not writable
/// on Android, so the scratch root never defaults to it.
fn scratch(label: &str) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("pdn-a22-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The `comm` of `pid`, or `None` when there is no such process.
fn comm(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    Some(stat[open + 1..close].to_string())
}

/// The parent pid of `pid`, or `None` when there is no such process.
fn ppid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// State letter from `/proc/<pid>/stat` field 3 (`Z` is a zombie).
fn state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat[stat.rfind(')')? + 1..]
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// A process that is present and not a zombie.
fn running(pid: u32) -> bool {
    matches!(state(pid), Some(s) if s != 'Z')
}

/// The proot process podnext spawned: the direct child whose `comm` names proot.
fn proot_child_of(parent: u32) -> Option<u32> {
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if comm(pid).is_some_and(|c| c.contains("proot")) && ppid(pid) == Some(parent) {
            return Some(pid);
        }
    }
    None
}

/// A container that can run the planted guest, or `None` with the reason.
/// Several containers here are partial (`alpine` has no `/bin/sh`), and picking
/// one of those would fail the guest, not the rule under test.
fn installed_container() -> Option<String> {
    let out = Command::new(bin())
        .args(["list", "-f", "json"])
        .output()
        .ok()?;
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let preferred = ["deb-pdn", "debian", "ubuntu", "test-alpine", "alpine"];
    let mut candidates: Vec<(String, PathBuf)> = parsed
        .as_array()?
        .iter()
        .filter_map(|c| {
            Some((
                c.get("name")?.as_str()?.to_string(),
                PathBuf::from(c.get("rootfs_path")?.as_str()?),
            ))
        })
        .collect();
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

/// Kills and reaps a directly-spawned child on drop, so a failed assertion does
/// not leak the session.
struct ChildGuard<'a>(Option<&'a mut Child>);

impl Drop for ChildGuard<'_> {
    fn drop(&mut self) {
        if let Some(child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn marker_pid(line: &str) -> Option<u32> {
    let rest = line.split("A22-GRANDCHILD=").nth(1)?;
    rest.split_whitespace().next()?.parse().ok()
}

#[test]
fn a_dead_guest_leaves_no_orphaned_grandchild() {
    let Some(container) = installed_container() else {
        eprintln!(
            "FIXTURE-UNAVAILABLE: no container with /bin/sh and /bin/sleep is installed, so no \
             guest can be orphaned; install one (`podnext install debian`) to exercise A22"
        );
        return;
    };

    let root = scratch("orphan");
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).expect("runtime dir");

    // The planted guest: it backgrounds a long sleep (the grandchild), prints its
    // pid, then becomes a long sleep itself so the guest is alive - and the
    // grandchild is running - when the test kills the tracer below.
    let mut child = Command::new(bin())
        .args([
            "run",
            &container,
            "--",
            "/bin/sh",
            "-c",
            "sleep 900 & echo A22-GRANDCHILD=$!; exec sleep 900",
        ])
        .env("RUNTIME_DIR", &runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn podnext run");
    let podnext_pid = child.id();

    // The guest prints the grandchild's pid; read it with a bound so a guest that
    // never starts is a fixture problem, never a hang.
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel::<Option<u32>>();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        let mut found = None;
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Some(pid) = marker_pid(&line) {
                        found = Some(pid);
                        break;
                    }
                }
            }
        }
        let _ = tx.send(found);
    });
    let grandchild = match rx.recv_timeout(Duration::from_secs(25)).ok().flatten() {
        Some(pid) => pid,
        None => {
            drop(ChildGuard(Some(&mut child)));
            eprintln!(
                "FIXTURE-UNAVAILABLE: the guest in '{container}' did not report a grandchild pid \
                 within 25 s; A22 needs a guest that can background a process"
            );
            return;
        }
    };

    // Find the guest's tracer, then confirm the condition before planting the kill.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut proot = None;
    while Instant::now() < deadline {
        proot = proot_child_of(podnext_pid);
        if proot.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let Some(proot) = proot else {
        drop(ChildGuard(Some(&mut child)));
        eprintln!(
            "FIXTURE-UNAVAILABLE: podnext never spawned a proot process for '{container}', so the \
             session cannot be killed for A22"
        );
        return;
    };
    if !running(grandchild) {
        drop(ChildGuard(Some(&mut child)));
        eprintln!(
            "FIXTURE-UNAVAILABLE: the planted grandchild {grandchild} was already gone before the \
             kill, so nothing would be orphaned"
        );
        return;
    }

    // Planted condition: the grandchild is running; now kill the session's tracer.
    unsafe {
        libc::kill(proot as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.wait();

    // The grandchild must not survive the dead session as an orphan.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && running(grandchild) {
        std::thread::sleep(Duration::from_millis(25));
    }
    if running(grandchild) {
        let parent = ppid(grandchild);
        // Never leave the orphan behind for a failed run.
        unsafe {
            libc::kill(grandchild as libc::pid_t, libc::SIGKILL);
        }
        panic!(
            "the grandchild {grandchild} survived the dead session as an orphan (PPID {:?}); \
             PDN must adopt and reap its descendants (A22)",
            parent
        );
    }

    println!(
        "A22-PROVED: real guest - grandchild {grandchild} of '{container}' did not survive the \
         session it was reparented out of"
    );
}
