//! Start coordination as one byte on a pipe (WORKLIST C10).
//!
//! runc's init process tells its parent it has finished setting up by writing a
//! single byte to a pipe the parent holds the read end of
//! (`libcontainer/standard_init_linux.go`: `fifoFile.Write([]byte("0"))`), and
//! the parent's start waits for exactly that
//! (`libcontainer/container_linux.go`). The child's readiness is an *event*: a
//! parent that never sees it knows the start failed.
//!
//! PDN had the other shape. `podnext service start` spawned the process and then
//! polled the reported state for up to 30 x 500 ms, printing
//! `start initiated (current state: ...)` and exiting 0 when the poll ran out -
//! a service that never came up was reported as a start. This module is the
//! port: the child sees the write end at [`READY_FD_NUM`] as `$PN_READY_FD`, the
//! parent blocks on the read end for a bounded time, and running out of that
//! time is a named failure ([`ReadinessError::Timeout`]) rather than silence.
//!
//! The write end is installed with `dup2` in the child (a `pre_exec` step) and
//! stays close-on-exec in the parent, so it is not inherited by any *other*
//! child the same process spawns. Clearing `FD_CLOEXEC` instead would leak the
//! write end into a sibling spawn, which would then hold the pipe open and hide
//! the EOF that says "the child died before signalling".

use std::io;
use std::time::{Duration, Instant};

/// Environment variable carrying the write end's descriptor to the child.
pub const READY_FD_ENV: &str = "PN_READY_FD";

/// The descriptor the child sees the pipe's write end at: 0/1/2 are stdio, so
/// the first free number is where runc's own extra files land too.
pub const READY_FD_NUM: i32 = 3;

/// Why a readiness handshake did not complete.
///
/// Both variants are named failures: nothing here reports a start whose
/// readiness byte never arrived.
#[derive(Debug, thiserror::Error)]
pub enum ReadinessError {
    /// No byte arrived within the caller's bound.
    #[error(
        "readiness timeout after {ms}ms: the child wrote no byte to the readiness pipe \
         (fd {fd}, ${env}); the service did not signal that it is ready"
    )]
    Timeout { ms: u64, fd: i32, env: &'static str },
    /// The write end was closed without a byte: the child exited first.
    #[error(
        "readiness pipe closed before a byte was written (fd {fd}, ${env}): the child exited \
         without signalling readiness"
    )]
    Closed { fd: i32, env: &'static str },
    /// The pipe itself failed (poll/read).
    #[error("readiness pipe error (fd {fd}): {source}")]
    Io {
        fd: i32,
        #[source]
        source: io::Error,
    },
}

/// The parent's read end and the child's write end (both close-on-exec until
/// [`ReadinessPipe::install`] dup's the write end into the child).
pub struct ReadinessPipe {
    read_fd: i32,
    write_fd: i32,
    child_end_open: bool,
}

impl ReadinessPipe {
    /// Create the pipe, close-on-exec on both ends.
    pub fn new() -> io::Result<Self> {
        let mut fds = [0i32; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            read_fd: fds[0],
            write_fd: fds[1],
            child_end_open: true,
        })
    }

    /// Give `cmd`'s child the write end at [`READY_FD_NUM`] and the matching
    /// `$PN_READY_FD`. Call before spawning; call
    /// [`ReadinessPipe::close_child_end`] after.
    pub fn install(&self, cmd: &mut tokio::process::Command) {
        let write_fd = self.write_fd;
        // Runs between fork and exec in the child, after std has set up stdio;
        // `dup2` is async-signal-safe, which is all `pre_exec` permits. The new
        // descriptor is not close-on-exec, so it survives the exec; the
        // parent's copy closes with the exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::dup2(write_fd, READY_FD_NUM) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd.env(READY_FD_ENV, READY_FD_NUM.to_string());
    }

    /// Close the parent's copy of the write end, after the spawn.
    pub fn close_child_end(&mut self) {
        if self.child_end_open {
            unsafe { libc::close(self.write_fd) };
            self.child_end_open = false;
        }
    }

    /// Wait, bounded, for the one readiness byte.
    ///
    /// `Ok(())` means the byte arrived. [`ReadinessError::Timeout`] means the
    /// bound elapsed first; [`ReadinessError::Closed`] means the child exited
    /// without writing. Interrupted polls and reads are retried against the same
    /// deadline, so a signal cannot silently shorten the bound.
    pub fn wait(&self, timeout: Duration) -> Result<(), ReadinessError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.timeout(timeout));
            }
            let ms = remaining.as_millis().min(i32::MAX as u128) as i32;
            let mut pollfd = libc::pollfd {
                fd: self.read_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let n = unsafe { libc::poll(&mut pollfd, 1, ms) };
            if n < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(self.io(err));
            }
            if n == 0 {
                return Err(self.timeout(timeout));
            }

            let mut byte = [0u8; 1];
            let n = unsafe { libc::read(self.read_fd, byte.as_mut_ptr() as *mut libc::c_void, 1) };
            if n == 1 {
                // Any byte is the signal: runc's child writes "0", and the
                // handshake is about arrival, not content.
                return Ok(());
            }
            if n == 0 {
                return Err(ReadinessError::Closed {
                    fd: self.read_fd,
                    env: READY_FD_ENV,
                });
            }
            let err = io::Error::last_os_error();
            match err.kind() {
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => continue,
                _ => return Err(self.io(err)),
            }
        }
    }

    fn timeout(&self, timeout: Duration) -> ReadinessError {
        ReadinessError::Timeout {
            ms: timeout.as_millis() as u64,
            fd: self.read_fd,
            env: READY_FD_ENV,
        }
    }

    fn io(&self, source: io::Error) -> ReadinessError {
        ReadinessError::Io {
            fd: self.read_fd,
            source,
        }
    }
}

impl Drop for ReadinessPipe {
    fn drop(&mut self) {
        if self.child_end_open {
            unsafe { libc::close(self.write_fd) };
        }
        unsafe { libc::close(self.read_fd) };
    }
}
