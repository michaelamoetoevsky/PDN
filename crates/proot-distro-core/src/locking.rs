//! Locking mechanism for PRoot Distro

use anyhow::Result;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Lock manager for containers
pub struct LockManager {
    locks: std::collections::HashMap<String, Arc<Mutex<()>>>,
}

impl LockManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            locks: std::collections::HashMap::new(),
        })
    }

    pub fn acquire(&mut self, name: &str) -> Result<()> {
        let lock = self
            .locks
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())));

        // Try to acquire lock with timeout
        let guard = lock.try_lock();
        if guard.is_err() {
            // Wait with timeout
            let start = std::time::Instant::now();
            while start.elapsed() < std::time::Duration::from_secs(30) {
                if let Ok(_) = lock.try_lock() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        Ok(())
    }

    pub fn release(&mut self, name: &str) {
        self.locks.remove(name);
    }

    pub fn try_acquire(&mut self, name: &str, timeout: std::time::Duration) -> bool {
        let _lock = self
            .locks
            .entry(name.to_string())
            .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(())));

        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(_) = self.locks.get(name).unwrap().try_lock() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }
}

/// File-based lock for cross-process synchronization
pub struct FileLock {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
}

impl FileLock {
    pub fn new(path: &std::path::Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: None,
        }
    }

    pub fn lock(&mut self) -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.path)?;

        // flock the descriptor we actually opened. The previous version flock'd
        // `self.file` — always `None` here — falling through to `unwrap_or(0)`,
        // i.e. file descriptor 0 (stdin), and then panicked unwrapping that same
        // `None`. Neither excluded anything.
        let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            return Err(anyhow::anyhow!("Failed to acquire lock"));
        }

        // Keep the descriptor alive for as long as the lock is held: flock is
        // released when the last descriptor referring to the file is closed.
        self.file = Some(file);
        Ok(())
    }

    pub fn unlock(&mut self) -> Result<()> {
        if let Some(file) = self.file.take() {
            unsafe {
                libc::flock(file.as_raw_fd(), libc::LOCK_UN);
            }
        }
        Ok(())
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.unlock();
    }
}

/// Where the exclusive lock for a container `name` lives.
///
/// Under the store's own `.podnext` root, next to `containers/`, so every process
/// that shares the store also shares the lock directory. It used to be `.pn-next`,
/// a directory nothing else in the store used, while the install path took no lock
/// at all: `~/.podnext/locks/` stayed empty through every race (WORKLIST A4).
pub fn lock_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/home/user"))
        .join(".podnext")
        .join("locks")
        .join(format!("{}.lock", name))
}

/// The PID recorded in a lock file, if the holder wrote one. `None` for a lock
/// held by an older build, or one taken but not yet stamped.
///
/// Counting is not the same as ownership: the PID is only a hint for the refusal
/// message and is re-read from the file each time, never trusted to decide
/// anything on its own.
fn holder_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// `" (holder PID 123)"`, or empty when the holder did not record one.
fn holder_suffix(path: &Path) -> String {
    match holder_pid(path) {
        Some(pid) => format!(" (holder PID {pid})"),
        None => String::new(),
    }
}

/// Container lock for exclusive access.
///
/// flock is advisory and descriptor-scoped: the lock is held while the
/// [`std::fs::File`] returned by `open` is alive, so the descriptor is kept for
/// the guard's lifetime. The holder writes its PID into the lock file so a
/// refused contender can name it (WORKLIST B4).
pub struct ContainerLock {
    lock_file: PathBuf,
    file: Option<fs::File>,
    exclusive: bool,
}

impl ContainerLock {
    /// The lock for `name` at its default location ([`lock_path`]).
    pub fn new(name: &str, exclusive: bool) -> Self {
        Self::at(&lock_path(name), exclusive)
    }

    /// A lock on an explicit path, for callers working outside the default store
    /// (tests, an alternate `$HOME`).
    pub fn at(lock_file: &Path, exclusive: bool) -> Self {
        Self {
            lock_file: lock_file.to_path_buf(),
            file: None,
            exclusive,
        }
    }

    pub fn acquire(&mut self) -> Result<()> {
        use std::fs::OpenOptions;

        if let Some(parent) = self.lock_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.lock_file)?;

        // Try to acquire lock.
        //
        // The descriptor to lock is `file`, just opened. This used to be
        // `self.file.as_ref().unwrap()`, but `self.file` is only assigned below
        // — so the first call panicked on `Option::unwrap()` and no container
        // operation could ever take a lock.
        let result = unsafe {
            if self.exclusive {
                libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB)
            } else {
                libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB)
            }
        };

        if result != 0 {
            return Err(anyhow::anyhow!(
                "Failed to acquire lock: {} is held by another process{}",
                self.lock_file.display(),
                holder_suffix(&self.lock_file)
            ));
        }

        // Record the holder, so a contender refused on the next line can say who
        // it was refused by. Best-effort: a lock that could not be stamped still
        // excludes, it just names no one.
        let _ = fs::write(&self.lock_file, format!("{}", std::process::id()));

        self.file = Some(file);
        Ok(())
    }

    pub fn release(&mut self) -> Result<()> {
        if let Some(file) = self.file.take() {
            unsafe {
                libc::flock(file.as_raw_fd(), libc::LOCK_UN);
            }
        }
        Ok(())
    }
}

impl Drop for ContainerLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_lock_exclusive_contention() {
        let mut lock1 = ContainerLock::new("test_contention_box", true);
        assert!(lock1.acquire().is_ok());

        // Second exclusive lock attempt on same container must fail with non-blocking contention
        let mut lock2 = ContainerLock::new("test_contention_box", true);
        let res2 = lock2.acquire();
        assert!(res2.is_err());
        assert!(res2
            .unwrap_err()
            .to_string()
            .contains("held by another process"));

        // Release first lock
        drop(lock1);

        // Now lock2 should succeed
        assert!(lock2.acquire().is_ok());
    }

    #[test]
    fn test_container_lock_shared_vs_exclusive() {
        let mut shared1 = ContainerLock::new("test_sh_ex_box", false);
        assert!(shared1.acquire().is_ok());

        // Second shared lock should succeed
        let mut shared2 = ContainerLock::new("test_sh_ex_box", false);
        assert!(shared2.acquire().is_ok());

        // Exclusive lock must fail while shared locks are held
        let mut exclusive = ContainerLock::new("test_sh_ex_box", true);
        assert!(exclusive.acquire().is_err());

        drop(shared1);
        drop(shared2);

        // Exclusive lock now succeeds
        assert!(exclusive.acquire().is_ok());
    }

    #[test]
    fn test_file_lock_acquires_and_excludes_a_second_locker() {
        // Resolve the temp base explicitly: Android has no writable `/tmp`, and
        // `std::env::temp_dir()` falls back to it when TMPDIR is unset.
        let base = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("PREFIX").map(|p| std::path::PathBuf::from(p).join("tmp")))
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&base).expect("temp base");
        let path = base.join(format!("pdn-filelock-{}.lock", std::process::id()));
        let _ = std::fs::remove_file(&path);

        // First locker takes the lock and keeps the descriptor alive.
        let mut first = FileLock::new(&path);
        first.lock().expect("first lock must succeed");

        // A second locker on the same path must be refused while the first holds it.
        let mut second = FileLock::new(&path);
        assert!(
            second.lock().is_err(),
            "a second locker must be refused while the first holds the lock"
        );

        // Releasing the first makes the path lockable again.
        first.unlock().expect("unlock");
        second.lock().expect("lock after release must succeed");
        second.unlock().expect("unlock");

        let _ = std::fs::remove_file(&path);
    }
}
