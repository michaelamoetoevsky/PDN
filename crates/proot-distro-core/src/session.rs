//! Session management for PRoot Distro

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Persistent session record for active container instances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub container_name: String,
    pub pid: u32,
    pub started_at: u64,
    pub login_uid: String,
    pub cmdline: Vec<String>,
    /// Kernel start time of `pid` (field 22 of `/proc/<pid>/stat`, in clock
    /// ticks), read when the record was written.
    ///
    /// Liveness is `(pid, start-time, not-a-zombie)`, not a bare PID: the kernel
    /// reuses PIDs, so a recycled PID can make a dead session look live, while a
    /// PID *plus* its start time cannot (B13, runc's `hasInit()`). `0` in a
    /// record written before this field existed means "not recorded" and is
    /// accepted as-is, so an upgrade does not discard live sessions.
    #[serde(default)]
    pub start_time: u64,
}

/// Directory storing runtime session files.
pub fn sessions_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RUNTIME_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir).join("sessions");
        }
    }
    let home = crate::paths::resolve_home().unwrap_or_else(|_| crate::paths::temp_base());
    home.join(".podnext").join("sessions")
}

/// Session file path for a given PID.
pub fn session_file_path(pid: u32) -> PathBuf {
    sessions_dir().join(format!("{}.json", pid))
}

/// Rendezvous socket a detached session's supervisor listens on (PDN#57).
///
/// Keyed by the supervisor's pid, beside the session record it answers for.
/// `active_sessions()` reads only `*.json`, so this extra file in the same
/// directory is invisible to the registry scan.
pub fn session_socket_path(pid: u32) -> PathBuf {
    sessions_dir().join(format!("{}.sock", pid))
}

/// Transcript the detached supervisor appends the guest's output to (PDN#57).
///
/// Kept beside the record so a reattach can be diagnosed even when nobody was
/// connected; same `active_sessions()` caveat as the socket.
pub fn session_log_path(pid: u32) -> PathBuf {
    sessions_dir().join(format!("{}.log", pid))
}

/// `(state, start-time)` from `/proc/<pid>/stat`, or `None` when the process is
/// gone.
///
/// `comm` (field 2) is parenthesised and may contain spaces or parentheses, so
/// the remaining fields only start reliably after the last `)`. After `comm`,
/// `start-time` is the 22nd field overall, i.e. the 19th after the state.
pub fn process_state_and_start_time(pid: u32) -> Option<(char, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let start_time = fields.nth(18)?.parse::<u64>().ok()?;
    Some((state, start_time))
}

/// The start time of a live process, for the record that will describe it.
pub fn process_start_time(pid: u32) -> Option<u64> {
    process_state_and_start_time(pid).map(|(_, start)| start)
}

/// Liveness of a recorded session: the process exists, its kernel start time is
/// the recorded one, and it is not a zombie.
///
/// A bare `kill(pid, 0)` answers "some process has this pid", which is not the
/// question: the kernel reuses PIDs, and a reaped-but-not-collected child still
/// answers the signal probe. `Z`/`X` states and a mismatched start time both
/// mean the recorded process is gone (B13).
pub fn process_is_alive(pid: u32, start_time: u64) -> bool {
    let Some((state, current)) = process_state_and_start_time(pid) else {
        return false;
    };
    if state == 'Z' || state == 'X' {
        return false;
    }
    // A record written before the field existed (`0`) cannot be checked; the
    // flock probe remains its evidence.
    start_time == 0 || current == start_time
}

/// Remove every artifact of `pid`'s session: the record, the rendezvous socket
/// and the transcript. Idempotent, and safe when some are already gone.
pub fn clear_session_files(pid: u32) {
    for path in [
        session_file_path(pid),
        session_socket_path(pid),
        session_log_path(pid),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

/// Reclaim a container's session artifacts once it is killed or removed (A9).
///
/// The records still naming the container are cleared; then any rendezvous file
/// whose supervisor is gone is reaped. Without this, `kill` + `remove` returned
/// 0 while `~/.podnext/sessions` kept the session's socket and log.
pub fn clear_container_sessions(container: &str) {
    for record in active_sessions().unwrap_or_default() {
        if record.container_name == container {
            clear_session_files(record.pid);
        }
    }
    reap_stale_rendezvous();
}

/// Wait, bounded, for `pid` (identified by `start_time`) to be gone, then remove
/// its artifacts. `false` means it was still running when `timeout` elapsed and
/// nothing was removed - the caller must say so rather than claim the session
/// was cleaned up (A9).
pub fn wait_and_clear(pid: u32, start_time: u64, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while process_is_alive(pid, start_time) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    clear_session_files(pid);
    true
}

/// Remove rendezvous files whose supervisor is gone (PDN#57).
///
/// A supervisor that exits normally unlinks its own socket; one killed by a
/// signal cannot, so its `{pid}.sock`/`{pid}.log` would accumulate. Called when
/// a new supervisor starts: any such file whose pid names no live process is
/// dead weight. A live supervisor's own pid is skipped, and the file for *this*
/// pid is left to the caller's reclaim step.
pub fn reap_stale_rendezvous() {
    let dir = sessions_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("sock") && ext != Some("log") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        // `kill(pid, 0)` is the existence probe: 0 means alive.
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Register a new running session, taking an inherited exclusive lock.
pub fn register_session(
    container: &str,
    pid: u32,
    uid: &str,
    cmd: &[String],
) -> Result<SessionGuard> {
    register_session_in(&sessions_dir(), container, pid, uid, cmd)
}

/// Owns a session file's `LOCK_EX` and unlinks the file when dropped, so a run
/// that exits normally leaves no stale `{pid}.json` behind. A hard kill
/// (SIGKILL) still leaves the file; `active_sessions()` prunes those lazily.
#[derive(Debug)]
pub struct SessionGuard {
    /// Kept alive so the advisory lock is held for the whole run.
    _file: std::fs::File,
    path: PathBuf,
    dev: u64,
    ino: u64,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        // Unlink only if the path still names *our* inode: a reused pid may
        // have created a fresh file at the same path after we opened it.
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if meta.dev() == self.dev && meta.ino() == self.ino {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

/// Register a session in an explicit directory. Split from [`register_session`]
/// so tests use a private dir instead of mutating the process-wide `RUNTIME_DIR`.
fn register_session_in(
    dir: &Path,
    container: &str,
    pid: u32,
    uid: &str,
    cmd: &[String],
) -> Result<SessionGuard> {
    std::fs::create_dir_all(dir)?;

    let path = dir.join(format!("{}.json", pid));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;

    let fd = file.as_raw_fd();

    // Acquire exclusive advisory lock
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        let _ = std::fs::remove_file(&path);
        return Err(anyhow::anyhow!(
            "Failed to acquire LOCK_EX on session file '{}': {}",
            path.display(),
            err
        ));
    }

    // Unset FD_CLOEXEC so child process inherits descriptor
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0 {
            return Err(anyhow::anyhow!(
                "fcntl(F_GETFD) failed on session fd: {}",
                std::io::Error::last_os_error()
            ));
        }
        if libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
            return Err(anyhow::anyhow!(
                "fcntl(F_SETFD) failed to clear FD_CLOEXEC: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let record = SessionRecord {
        container_name: container.to_string(),
        pid,
        started_at,
        login_uid: uid.to_string(),
        cmdline: cmd.to_vec(),
        // The registering process's own kernel start time: `(pid, start-time)`
        // is what later identifies this process, not the pid alone (B13).
        start_time: process_start_time(std::process::id()).unwrap_or(0),
    };

    use std::io::Write;
    let data = serde_json::to_vec_pretty(&record)?;
    (&file).write_all(&data)?;
    (&file).flush()?;

    let meta = file.metadata()?;
    Ok(SessionGuard {
        _file: file,
        path,
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

/// Query active sessions by probing non-blocking shared locks on session files.
///
/// A record counts as a live session only when both hold: the registering
/// process still holds its `LOCK_EX`, **and** the recorded `(pid, start_time)`
/// names that same live, non-zombie process (B13). The lock alone was the old
/// evidence, and it answers "some process still has this file open" - a PID the
/// kernel has since reused is not a session, and a zombie is not running.
pub fn active_sessions() -> Result<Vec<SessionRecord>> {
    let dir = sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(_) => match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            },
        };

        let fd = file.as_raw_fd();
        let ret = unsafe { libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) };
        if ret == 0 {
            // Lock succeeded: process holding LOCK_EX has terminated. Unlink file.
            unsafe {
                libc::flock(fd, libc::LOCK_UN);
            }
            drop(file);
            let _ = std::fs::remove_file(&path);
        } else {
            let err = std::io::Error::last_os_error();
            let errno = err.raw_os_error().unwrap_or(0);
            if errno == libc::EWOULDBLOCK || errno == libc::EAGAIN {
                // A lock is held: the registering process is still alive. The
                // record must agree that it is the *same* process, and that it
                // is not a zombie (B13), or it is not a running session.
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(record) = serde_json::from_str::<SessionRecord>(&content) else {
                    continue;
                };
                if process_is_alive(record.pid, record.start_time) {
                    sessions.push(record);
                } else {
                    pn_trace!(
                        "[DEBUG] session liveness: pid {} start-time {} is not the recorded \
                         process; dropping the record",
                        record.pid,
                        record.start_time
                    );
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    Ok(sessions)
}

/// Send a signal to a session process.
pub fn kill_session(pid: u32, signal: i32) -> Result<()> {
    let ret = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if ret != 0 {
        return Err(anyhow::anyhow!(
            "kill_session({}, {}) failed: {}",
            pid,
            signal,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Session manager
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn create(
        &self,
        _container_name: &str,
        options: &crate::login::LoginOptions,
    ) -> Result<Session> {
        let id = Uuid::new_v4().to_string();
        let session = Session {
            id: id.clone(),
            container_name: options.container_name.clone(),
            user: options.user.clone().unwrap_or_else(|| "root".to_string()),
            command: options.command.clone(),
            env: options.env.iter().cloned().collect(),
            cwd: options.cwd.clone().unwrap_or_else(|| "/".to_string()),
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            status: SessionStatus::Created,
            pid: None,
        };

        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(id.clone(), session.clone());

        Ok(session)
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<Session> {
        self.sessions.lock().unwrap().values().cloned().collect()
    }

    pub fn list_all(&self) -> Vec<crate::container::SessionInfo> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .values()
            .map(|s| crate::container::SessionInfo {
                container_name: s.container_name.clone(),
                user: s.user.clone(),
                pid: s.pid,
                status: format!("{:?}", s.status),
                started_at: s.started_at.unwrap_or(s.created_at),
            })
            .collect()
    }

    pub fn list_all_full(&self) -> Vec<Session> {
        let sessions = self.sessions.lock().unwrap();
        sessions.values().cloned().collect()
    }

    pub fn end(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.status = SessionStatus::Ended;
            session.ended_at = Some(chrono::Utc::now());
        }
        Ok(())
    }
}

/// Session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub container_name: String,
    pub user: String,
    pub command: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub cwd: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: SessionStatus,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Created,
    Running,
    Completed,
    Failed,
    Terminated,
    Ended,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_record_serialization() {
        let record = SessionRecord {
            container_name: "test-box".to_string(),
            pid: 12345,
            started_at: 1700000000,
            login_uid: "1000".to_string(),
            cmdline: vec!["/bin/sh".to_string(), "-l".to_string()],
            start_time: 42,
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: SessionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, parsed);
    }

    /// A record written before `start_time` existed (no field at all) still
    /// parses, so an upgrade does not make existing sessions unreadable.
    #[test]
    fn a_record_without_a_start_time_still_parses() {
        let json = r#"{"container_name":"c1","pid":7,"started_at":1,"login_uid":"0","cmdline":[]}"#;
        let parsed: SessionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.start_time, 0);
        // `0` means "not recorded", not "start time zero": an old record for a
        // live process stays live rather than being declared dead on upgrade.
        let me = std::process::id();
        assert!(process_is_alive(me, 0));
    }

    /// The kernel start time is read from `/proc/<pid>/stat`, and it is stable
    /// for the life of the process.
    #[test]
    fn the_start_time_of_a_live_process_is_read_and_stable() {
        let me = std::process::id();
        let first = process_start_time(me).expect("own start time");
        assert!(first > 0, "a real process has a non-zero start time");
        assert_eq!(process_start_time(me), Some(first));
        assert!(process_is_alive(me, first));
    }

    /// A PID that has been reused by another process must not resurrect a dead
    /// session: the start time of the process now holding that PID differs from
    /// the one recorded (B13, runc's `hasInit()`).
    #[test]
    fn a_reused_pid_is_not_a_live_session() {
        let me = std::process::id();
        let real = process_start_time(me).expect("own start time");
        assert!(!process_is_alive(me, real + 1));
        // A PID that names nothing is not alive either.
        assert!(!process_is_alive(0x7fff_fffe, 1));
        assert_eq!(process_start_time(0x7fff_fffe), None);
    }

    /// A zombie is not "running": it kept its PID and its `/proc` entry, but the
    /// work is over. `kill(pid, 0)` answers for it; liveness must not.
    #[test]
    fn a_zombie_is_not_running() {
        // `sh -c exit 0` reaps itself; the parent never calls `wait()` in this
        // test, so the child stays a zombie with a live `/proc/<pid>/stat`.
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .expect("spawn a zombie");
        let pid = child.id();
        let start = (0..200)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                match process_state_and_start_time(pid) {
                    Some((state, start)) if state == 'Z' => Some(start),
                    _ => None,
                }
            })
            .expect("the child did not become a zombie");
        assert_eq!(process_state_and_start_time(pid).map(|(s, _)| s), Some('Z'));
        assert!(
            !process_is_alive(pid, start),
            "a zombie must not be reported as a running session"
        );
        let _ = child.kill();
    }

    #[test]
    fn session_registration_and_probing() {
        let tmp = std::env::temp_dir().join(format!("pdn-sess-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::env::set_var("RUNTIME_DIR", &tmp);

        let my_pid = std::process::id();
        let cmd = vec!["bash".to_string()];
        let file = register_session("c1", my_pid, "1000", &cmd).expect("register session");

        // The session file is locked by `file`
        let active = active_sessions().expect("active sessions");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].container_name, "c1");
        assert_eq!(active[0].pid, my_pid);

        // Drop the file to release the lock
        drop(file);

        // Now probing should find the lock free, prune the file, and return empty
        let active_after = active_sessions().expect("active sessions after drop");
        assert_eq!(active_after.len(), 0);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn session_file_removed_on_drop() {
        let tmp = crate::paths::temp_base().join(format!("pdn-sess-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let json_count = || {
            std::fs::read_dir(&tmp)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .map(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                        .unwrap_or(false)
                })
                .count()
        };

        {
            let _guard = register_session_in(&tmp, "c1", 4242, "1000", &["bash".to_string()])
                .expect("register session");
            assert_eq!(
                json_count(),
                1,
                "session file exists while the guard is held"
            );
        }

        assert_eq!(
            json_count(),
            0,
            "session file unlinked when the guard drops"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
