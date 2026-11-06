//! WORKLIST A5: `remove` must not half-apply while a session is live.
//!
//! Measured on the base commit (three independent agents): `remove <c> --force`
//! with a live session printed
//!
//! ```text
//! Error: removed '<c>' from the index but failed to delete <dir>: Permission
//! denied (os error 13)
//! ```
//!
//! and exited 1, leaving the guest alive and the directory on disk - while the
//! index had already dropped the record. `rm -rf` then succeeded. The two halves
//! of the operation disagreed.
//!
//! The semantics this file pins down (A5): `remove` REFUSES BEFORE it touches the
//! index when a live session still names the container, naming that session; and
//! otherwise deletes the directory before it drops the record - the index and the
//! directory move together or not at all. A command that cannot remove must change
//! nothing; a command that succeeds must leave neither half behind.
//!
//! A real detached session is racy on a fast host and needs an installed guest, so
//! liveness is planted the way the registry itself defines it: a `{pid}.json`
//! record in `RUNTIME_DIR/sessions` whose pid is a live, non-zombie process with a
//! matching kernel start time, and whose `LOCK_EX` this test holds for its whole
//! lifetime. That is exactly the state `active_sessions()` reads as "a live
//! session" (see `crates/proot-distro-core/src/session.rs`), so it cannot race -
//! the session is live until this test says otherwise.
//!
//! The directory cannot be deleted, the way a live guest makes it: the store's
//! parent directory is made read-only, so `remove_dir_all` unlinks the container's
//! contents and then fails on the final `rmdir` with EACCES - the same error the
//! live session produced. That is order-independent and identical on every run,
//! which is what makes the base half-apply deterministic: the container's marker
//! file goes first, so the store stops recognising the directory although the
//! directory is still there.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_podnext")
}

/// Writable scratch base: `/tmp` is not writable for an app uid on Android.
fn temp_base() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PREFIX").map(|p| PathBuf::from(p).join("tmp")))
        .unwrap_or_else(std::env::temp_dir)
}

/// A private store: `HOME`/`PN_TEST_ROOT` hold the containers and `RUNTIME_DIR`
/// the session registry, so the operator's real `~/.podnext` is never touched.
struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("pdn-remove-atomic-")
            .tempdir_in(temp_base())
            .expect("scratch store");
        Sandbox { dir }
    }

    fn home(&self) -> &Path {
        self.dir.path()
    }

    fn containers(&self) -> PathBuf {
        self.home().join(".podnext").join("containers")
    }

    fn sessions(&self) -> PathBuf {
        self.home().join("sessions")
    }

    fn container_dir(&self, name: &str) -> PathBuf {
        self.containers().join(name)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(bin());
        c.env("HOME", self.home())
            .env("PN_TEST_ROOT", self.home())
            .env("RUNTIME_DIR", self.home())
            .stdin(Stdio::null());
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd().args(args).output().expect("spawn podnext")
    }

    /// Whether `podnext list -f json` reports `name` as a container in this store.
    fn listed(&self, name: &str) -> bool {
        let out = self.run(&["list", "-f", "json"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "`list -f json` failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let parsed: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("list -f json must be JSON");
        parsed
            .as_array()
            .expect("the container list must be a JSON array")
            .iter()
            .any(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
    }
}

/// A container directory of the shape the store recognises.
fn plant_container(sb: &Sandbox, name: &str) -> PathBuf {
    let dir = sb.container_dir(name);
    std::fs::create_dir_all(dir.join("rootfs/etc")).expect("rootfs/etc");
    std::fs::write(dir.join("metadata.json"), format!(r#"{{"name":"{name}"}}"#))
        .expect("metadata.json");
    dir
}

/// Kernel start time of `pid` (field 22 of `/proc/<pid>/stat`), read independently
/// of the code under test.
fn proc_start_time(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("read /proc stat");
    let rest = &stat[stat.rfind(')').expect("comm is parenthesised") + 1..];
    rest.split_whitespace()
        .nth(19)
        .expect("field 22")
        .parse()
        .expect("start time is numeric")
}

/// A live session, as the registry defines one: a live non-zombie process plus a
/// record naming it and holding `LOCK_EX`. Dropped at the end of the test.
struct LiveSession {
    child: Child,
    _lock: std::fs::File,
}

impl LiveSession {
    fn start(sb: &Sandbox, container: &str) -> Self {
        std::fs::create_dir_all(sb.sessions()).expect("sessions dir");
        // `exec sleep` so the recorded pid *is* the live process.
        let child = Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn the session process");
        let pid = child.id();
        let record = sb.sessions().join(format!("{pid}.json"));
        std::fs::write(
            &record,
            serde_json::json!({
                "container_name": container,
                "pid": pid,
                "started_at": 1_700_000_000u64,
                "login_uid": "0",
                "cmdline": ["/bin/sleep", "30"],
                "start_time": proc_start_time(pid),
            })
            .to_string(),
        )
        .expect("write the record");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&record)
            .expect("open the record");
        let ret = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) };
        assert_eq!(ret, 0, "flock LOCK_EX on the record");
        LiveSession { child, _lock: lock }
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Makes the store read-only for the duration of a test (the directory a live
/// session holds cannot be removed), restoring it so the sandbox can be deleted.
struct ReadOnlyStore(PathBuf);

impl ReadOnlyStore {
    fn new(containers: PathBuf) -> Self {
        std::fs::set_permissions(&containers, std::fs::Permissions::from_mode(0o555))
            .expect("chmod the store read-only");
        ReadOnlyStore(containers)
    }
}

impl Drop for ReadOnlyStore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn remove_while_a_session_is_live_leaves_the_index_and_the_directory_agreeing() {
    let sb = Sandbox::new();
    let name = "pdn-a5-live";
    let dir = plant_container(&sb, name);

    // A live session that cannot race: it is live until this test drops it.
    let session = LiveSession::start(&sb, name);
    // The directory cannot be deleted, exactly as a live guest makes it.
    let _store = ReadOnlyStore::new(sb.containers());

    let out = sb.run(&["remove", name, "--force"]);
    let exit = out.status.code().expect("podnext was killed by a signal");
    let listed = sb.listed(name);
    let dir_present = dir.exists();

    if exit == 0 {
        eprintln!("branch (b): `remove` reported success (exit 0)");
        assert!(
            !listed,
            "`remove` succeeded but the index still lists '{name}': {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            !dir_present,
            "`remove` succeeded but the directory {} is still on disk",
            dir.display()
        );
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        eprintln!("branch (a): `remove` refused (exit {exit}); stderr: {err}");
        assert!(
            dir_present,
            "`remove` failed but the directory {} is already gone",
            dir.display()
        );
        assert!(
            listed,
            "`remove` failed but the index lost the record for '{name}' while the directory \
             {} stayed - the half-applied state",
            dir.display()
        );
        // The refusal names the container and the live session it protects.
        assert!(
            err.contains(name),
            "the refusal did not name the container '{name}': {err}"
        );
        assert!(
            err.contains(&session.child.id().to_string()),
            "the refusal did not name the live session pid {}: {err}",
            session.child.id()
        );
    }

    // The invariant, whatever branch was taken: the index and the directory agree.
    assert_eq!(
        listed,
        dir_present,
        "half-applied: the index says listed={listed} while the directory {} says present={dir_present}",
        dir.display()
    );
}

#[test]
fn remove_without_a_session_removes_both_halves() {
    let sb = Sandbox::new();
    let name = "pdn-a5-plain";
    let dir = plant_container(&sb, name);

    let out = sb.run(&["remove", name, "--force"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "`remove` of an idle container must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !dir.exists(),
        "the directory {} survived a successful `remove`",
        dir.display()
    );
    assert!(
        !sb.listed(name),
        "the index still lists '{name}' after a successful `remove`"
    );
}
