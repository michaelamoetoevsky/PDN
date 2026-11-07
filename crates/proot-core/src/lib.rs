//! PRoot Core - Rust Implementation
//!
//! A ptrace-based chroot, mount --bind, and binfmt_misc implementation
//! without privilege/setup for Linux. Compatible with Termux Android 16+.

/// Trace-only logging, off unless `PN_DEBUG` is set in the environment.
///
/// These lines were unconditional upstream and flooded real output — a single
/// `backup create` emitted ~280 kB of DEBUG for one alpine container, and every
/// command printed a DEBUG banner, burying the actual result. Enable with
/// `PN_DEBUG=1` when diagnosing.
macro_rules! pn_trace {
    ($($arg:tt)*) => {
        if ::std::env::var_os("PN_DEBUG").is_some() {
            ::std::eprintln!($($arg)*);
        }
    };
}
pub mod arch;
pub mod cli;
pub mod compat;
pub mod extension;
pub mod identity;
pub mod path;
pub mod ptrace;
pub mod seccomp;
pub mod selective;
pub mod syscall;
pub mod tracee;
pub mod tracee_state;
pub mod vfs;

// `ExitStatus::signal()`, for the exit-status triple below. This crate is
// ptrace/PRoot-only, so the unix extension is always available.
use std::os::unix::process::ExitStatusExt;

pub use arch::{get_arch, sysnum, Arch};
pub use cli::{BindMount, ProotConfig};
pub use extension::{Extension, ExtensionManager};
pub use path::PathTranslator;
pub use ptrace::{PtraceController, PtraceEvent};
pub use seccomp::SeccompFilter;
pub use syscall::{SyscallConfig, SyscallHandler};
/// A child process exited with a code the caller should exit with itself.
///
/// Returning this as `Err` loses the code: `fn main() -> Result<()>` prints the
/// error and exits 1, so `podnext login c -- sh -c 'exit 7'` reported 1 for every
/// non-zero code. `main` downcasts to this and exits with the real one.
#[derive(Debug)]
pub struct ChildExit(pub i32);

impl std::fmt::Display for ChildExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "child exited with code {}", self.0)
    }
}

impl std::error::Error for ChildExit {}

pub use tracee::{
    FdEntry, FdTable, MountInfo, PathInfo, RawSyscall, Registers, SyscallInfo, Tracee, TraceeId,
    TraceeState, VfsContext,
};
pub use tracee_state::TraceeStateManager;
pub use vfs::VfsManager;

use anyhow::Result;
use parking_lot::Mutex;
use std::io::IsTerminal;
use std::sync::Arc;
use tokio::process::Command;

/// Resolve the proot binary to an absolute path.
///
/// Required once the child environment is cleared: Rust's Unix `Command`
/// searches a bare program name in the *child's* PATH, so the plain name
/// "proot" would no longer be found once PATH is the guest's.
fn proot_binary() -> String {
    if let Ok(explicit) = std::env::var("PROOT_BIN") {
        if !explicit.is_empty() {
            return explicit;
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let candidate = std::path::Path::new(dir).join("proot");
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    "proot".to_string()
}

/// POSIX single-quote a string so an inner `/bin/sh -c` sees it as one word.
///
/// Naive `args.join(" ")` corrupts quoting: `login alpine -- /bin/sh -c 'echo X'`
/// became `/bin/sh -c /bin/sh -c echo X`, and the inner `-c` ate a word.
/// Mirrors Python's `shlex.quote`.
fn sh_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'-' | b'.' | b'/' | b':' | b'@' | b'%' | b'+' | b',' | b'='
                )
        });
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Main PRoot engine
#[derive(Clone)]
pub struct ProotEngine {
    pub config: ProotConfig,
    ptrace_ctrl: Arc<Mutex<PtraceController>>,
    syscall_handler: Arc<Mutex<SyscallHandler>>,
    path_translator: Arc<Mutex<PathTranslator>>,
    tracee_manager: Arc<Mutex<TraceeStateManager>>,
    vfs_manager: Arc<Mutex<VfsManager>>,
    seccomp_filter: Arc<Mutex<SeccompFilter>>,
}

/// The process group the child should get.
///
/// A child in its own process group is not the terminal's foreground group, so its first read from
/// the controlling tty stops on `SIGTTIN`: no prompt and no interactive stdin, while writes and
/// ioctls keep working, so the failure looks partial (`stty size` still answers). Measured against
/// v0.1.7, which had no such group and worked (issue #68). An interactive foreground run therefore
/// stays in the caller's group, which is the foreground group and also receives `SIGWINCH`;
/// anything else gets its own, because group-signalling is the point of it.
fn child_owns_process_group(stdin_is_tty: bool) -> bool {
    !stdin_is_tty
}

/// The `TERM` the guest should see: the caller's.
///
/// The caller's terminal is the one the guest draws to, so telling it anything else makes it ask
/// for capabilities nothing has: measured, a caller at `TERM=footest` used to arrive as
/// `xterm-256color`. An empty value counts as absent, because a guest told `TERM=` cannot render at
/// all, and the fallback then applies.
fn guest_term(caller: Option<&str>) -> String {
    caller
        .filter(|v| !v.is_empty())
        .unwrap_or("xterm-256color")
        .to_string()
}

impl ProotEngine {
    pub fn new(config: ProotConfig) -> Result<Self> {
        let ptrace_ctrl = Arc::new(Mutex::new(PtraceController::new()?));
        let syscall_config = SyscallConfig::default();
        let syscall_handler = Arc::new(Mutex::new(SyscallHandler::new(syscall_config)?));
        let path_translator = Arc::new(Mutex::new(PathTranslator::new(&config)?));
        let tracee_manager = Arc::new(Mutex::new(TraceeStateManager::new()));
        let vfs_manager = Arc::new(Mutex::new(VfsManager::new(&config)?));
        let seccomp_filter = Arc::new(Mutex::new(SeccompFilter::new(&config)?));

        Ok(Self {
            config,
            ptrace_ctrl,
            syscall_handler,
            path_translator,
            tracee_manager,
            vfs_manager,
            seccomp_filter,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        // Initialize seccomp filter
        self.seccomp_filter.lock().install()?;

        // Start ptrace loop
        self.ptrace_ctrl.lock().run_loop(
            &mut self.syscall_handler.lock(),
            &mut self.path_translator.lock(),
            &mut self.tracee_manager.lock(),
            &mut self.vfs_manager.lock(),
        )
    }

    /// Start a daemon process (background mode)
    pub async fn start(&mut self) -> Result<u32> {
        // Initialize seccomp filter
        self.seccomp_filter.lock().install()?;

        // Build the proot command for proper isolation
        // Note: Use only options supported by the system proot binary (5.1.107.101)
        // IMPORTANT: proot requires --rootfs=. with cwd set to rootfs for proper isolation
        let proot_bin = proot_binary();
        let mut cmd = Command::new(&proot_bin);

        // Proot extensions for Termux/Android (supported by this proot version)
        cmd.arg("--kill-on-exit");
        cmd.arg("--link2symlink");
        cmd.arg("--sysvipc");
        cmd.arg("-L"); // Fix lstat for dpkg symlink warnings

        // Present uid/gid 0 inside the guest.
        //
        // Without this the guest keeps the real Android app uid and every
        // privileged operation fails at the first check: `apt-get install`
        // aborts with `dpkg: error: requested operation requires superuser
        // privilege`, and `chown`/`passwd`/`useradd` refuse too, which is what
        // blocked node, playwright and any package install in a Debian
        // container. proot-distro passes exactly this for non-termux
        // containers (proot_cmd.py:105, `--change-id={login_uid}:{login_gid}`,
        // defaulting to 0:0).
        //
        // It is still a user-space illusion: proot maps ids for the traced
        // process only, no real privilege is gained.
        cmd.arg("--change-id=0:0:0");

        if let Some(hostname) = &self.config.hostname {
            cmd.arg(format!(
                "--kernel-release=\\Linux\\{}\\6.6.0\\6.6.0\\aarch64\\localdomain\\-1\\",
                hostname
            ));
        } else if let Some(krel) = &self.config.kernel_release {
            cmd.arg(format!("--kernel-release={}", krel));
        }

        // Root filesystem - critical for isolation
        // Use --rootfs=. with cwd set to rootfs (like proot-distro does)
        let rootfs = self
            .config
            .rootfs
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Rootfs not configured"))?;
        cmd.arg("--rootfs=.");
        cmd.current_dir(rootfs);

        // Working directory inside guest rootfs
        if let Some(cwd) = &self.config.cwd {
            cmd.arg(format!("--cwd={}", cwd.display()));
        }

        // Essential virtual filesystem binds for isolation
        cmd.arg("--bind=/dev");
        cmd.arg("--bind=/proc");
        cmd.arg("--bind=/sys");

        // Environment variables — allow-listed guest env (see execute()).
        cmd.env_clear();
        cmd.env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );
        cmd.env("HOME", "/root");
        // TERM is forwarded rather than invented: the caller's terminal is the one the guest draws
        // to, and a guest that asks for capabilities it does not have is worse than one told the
        // truth. Measured (scripts/shell_fidelity.py): with the caller at `TERM=footest`, the guest
        // reported `xterm-256color` while COLORTERM passed through untouched. The default applies
        // only when the caller has no TERM at all, so a bare non-tty run still gets something sane.
        cmd.env("TERM", guest_term(std::env::var("TERM").ok().as_deref()));
        for passthrough in ["LANG", "LC_ALL", "COLORTERM"] {
            if let Ok(v) = std::env::var(passthrough) {
                cmd.env(passthrough, v);
            }
        }
        for env in &self.config.env {
            if let Some((k, v)) = env.split_once('=') {
                cmd.env(k, v);
            }
        }

        // Additional bind mounts from config
        for bind in &self.config.bind {
            cmd.arg(format!("--bind={}", bind));
        }

        // Command to execute
        if self.config.command.is_empty() {
            return Err(anyhow::anyhow!("No command specified"));
        }
        cmd.arg(&self.config.command[0]);
        if self.config.command.len() > 1 {
            cmd.args(&self.config.command[1..]);
        }

        // Debug output. Reachable when PN_DEBUG is set, not only under `verbose`: the
        // container paths never set `verbose`, so the argv a failing bind actually
        // produced could not be printed at all.
        if self.config.verbose > 0 || std::env::var_os("PN_DEBUG").is_some() {
            pn_trace!("[DEBUG] Starting proot command: {:?}", cmd);
            pn_trace!("[DEBUG] Rootfs: {:?}", rootfs);
            pn_trace!("[DEBUG] Env: {:?}", self.config.env);
        }

        // Own process group: proot traces the guest's descendants, so a stop
        // must be able to signal the whole tree, not one pid. pgid == child pid.
        cmd.process_group(0);
        unsafe {
            cmd.pre_exec(|| {
                libc::umask(0o022);
                #[cfg(not(target_os = "android"))]
                {
                    let gid: libc::gid_t = 0;
                    let _ = libc::setgroups(1, &gid);
                }
                Ok(())
            });
        }
        // Spawn the process (daemon mode)
        let child = cmd.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;

        if self.config.verbose > 0 {
            pn_trace!("[DEBUG] Started daemon PID: {}", pid);
        }

        Ok(pid)
    }

    /// Execute a command in the foreground and wait for completion, returning exit code
    pub async fn execute(&mut self) -> Result<i32> {
        // The opt-in second backend (S1). When selected, the command runs under
        // the selective-interception filter and tracer instead of PRoot.
        if self.config.seccomp_selective {
            return self.execute_selective().await;
        }

        // Initialize seccomp filter
        self.seccomp_filter.lock().install()?;

        // Build the proot command for proper isolation
        // Note: Use only options supported by the system proot binary (5.1.107.101)
        // IMPORTANT: proot requires --rootfs=. with cwd set to rootfs for proper isolation
        // Also, run commands through the shell like proot-distro does
        let proot_bin = proot_binary();
        let mut cmd = Command::new(&proot_bin);

        // Proot extensions for Termux/Android (supported by this proot version)
        cmd.arg("--kill-on-exit");
        cmd.arg("--link2symlink");
        cmd.arg("--sysvipc");
        cmd.arg("-L"); // Fix lstat for dpkg symlink warnings

        // Present uid/gid 0 inside the guest.
        //
        // Without this the guest keeps the real Android app uid and every
        // privileged operation fails at the first check: `apt-get install`
        // aborts with `dpkg: error: requested operation requires superuser
        // privilege`, and `chown`/`passwd`/`useradd` refuse too, which is what
        // blocked node, playwright and any package install in a Debian
        // container. proot-distro passes exactly this for non-termux
        // containers (proot_cmd.py:105, `--change-id={login_uid}:{login_gid}`,
        // defaulting to 0:0).
        //
        // It is still a user-space illusion: proot maps ids for the traced
        // process only, no real privilege is gained.
        cmd.arg("--change-id=0:0:0");

        if let Some(hostname) = &self.config.hostname {
            cmd.arg(format!(
                "--kernel-release=\\Linux\\{}\\6.6.0\\6.6.0\\aarch64\\localdomain\\-1\\",
                hostname
            ));
        } else if let Some(krel) = &self.config.kernel_release {
            cmd.arg(format!("--kernel-release={}", krel));
        }

        // Root filesystem - critical for isolation
        // Use --rootfs=. with cwd set to rootfs (like proot-distro does)
        let rootfs = self
            .config
            .rootfs
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Rootfs not configured"))?;
        cmd.arg("--rootfs=.");
        cmd.current_dir(rootfs);

        // Working directory inside guest rootfs
        if let Some(cwd) = &self.config.cwd {
            cmd.arg(format!("--cwd={}", cwd.display()));
        }

        // Essential virtual filesystem binds for isolation
        cmd.arg("--bind=/dev");
        cmd.arg("--bind=/proc");
        cmd.arg("--bind=/sys");

        // Environment variables
        //
        // The container gets a clean, allow-listed environment — not the host's.
        // Passing the host env through (the previous behaviour) leaked live
        // secrets (`*_API_KEY`) into the guest and broke every musl binary:
        // Termux's LD_PRELOAD (libtermux-exec) and LD_LIBRARY_PATH are host-ABI
        // and cause "relocation error" + exit 127 for apk/busybox/getent.
        // proot-distro builds the guest env from scratch (env -i) for the same
        // reasons. Verified: after this, `login alpine -- apk --version` prints
        // "apk-tools 3.0.8-r0" and the guest no longer sees host secrets.
        cmd.env_clear();
        cmd.env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        );
        cmd.env("HOME", "/root");
        // TERM is forwarded rather than invented: the caller's terminal is the one the guest draws
        // to, and a guest that asks for capabilities it does not have is worse than one told the
        // truth. Measured (scripts/shell_fidelity.py): with the caller at `TERM=footest`, the guest
        // reported `xterm-256color` while COLORTERM passed through untouched. The default applies
        // only when the caller has no TERM at all, so a bare non-tty run still gets something sane.
        cmd.env("TERM", guest_term(std::env::var("TERM").ok().as_deref()));
        for passthrough in ["LANG", "LC_ALL", "COLORTERM"] {
            if let Ok(v) = std::env::var(passthrough) {
                cmd.env(passthrough, v);
            }
        }
        for env in &self.config.env {
            if let Some((k, v)) = env.split_once('=') {
                cmd.env(k, v);
            }
        }

        // Additional bind mounts from config
        for bind in &self.config.bind {
            cmd.arg(format!("--bind={}", bind));
        }

        // Command to execute - run through shell like proot-distro does
        // This ensures PATH resolution and symlink handling works correctly.
        // Each argv element is quoted individually so the inner shell sees
        // exactly the words the caller passed.
        if self.config.command.is_empty() {
            return Err(anyhow::anyhow!("No command specified"));
        }
        let shell = "/bin/sh";
        let command_str = self
            .config
            .command
            .iter()
            .map(|a| sh_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        cmd.arg(shell);
        cmd.arg("-c");
        cmd.arg(command_str);

        // Debug output. Reachable when PN_DEBUG is set, not only under `verbose`: the
        // container paths never set `verbose`, so the argv a failing bind actually
        // produced could not be printed at all.
        if self.config.verbose > 0 || std::env::var_os("PN_DEBUG").is_some() {
            pn_trace!("[DEBUG] Executing proot command: {:?}", cmd);
            pn_trace!("[DEBUG] Rootfs: {:?}", rootfs);
            pn_trace!("[DEBUG] Env: {:?}", self.config.env);
        }

        unsafe {
            cmd.pre_exec(|| {
                libc::umask(0o022);
                #[cfg(not(target_os = "android"))]
                {
                    let gid: libc::gid_t = 0;
                    let _ = libc::setgroups(1, &gid);
                }
                Ok(())
            });
        }

        // Spawn and wait for completion
        let stdin_is_tty = std::io::stdin().is_terminal();
        let mut child = {
            let mut c = cmd;
            c.stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .stdin(std::process::Stdio::inherit());
            // A child in its own process group is not the terminal's foreground group, so the first
            // read from the controlling tty stops it with SIGTTIN: no prompt, no interactive stdin,
            // while writes and ioctls keep working and the failure looks partial (issue #68, measured
            // - v0.1.7 had no such group and worked; /proc showed the proot child `tpgid=<caller>`).
            // The group is given only when stdin is not a terminal. An interactive foreground run
            // then stays in the caller's group, which IS the foreground group and also receives
            // SIGWINCH from the kernel. `start()` keeps its own group: a supervised or detached run
            // has no terminal to own, and group-signalling it is the point.
            if child_owns_process_group(stdin_is_tty) {
                c.process_group(0);
            }
            c.spawn()?
        };
        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;
        if self.config.verbose > 0 {
            pn_trace!("[DEBUG] Spawned PID: {}", pid);
        }

        let exit_status = child.wait().await?;
        if self.config.verbose > 0 {
            pn_trace!("[DEBUG] Exit status: {:?}", exit_status);
        }
        // The raw wait status, at the point the exit code is derived from it:
        // `code()` is `None` exactly when the child was killed by a signal, and
        // the whole triple below turns on that distinction.
        pn_trace!(
            "[DEBUG] raw wait status: code={:?} signal={:?}",
            exit_status.code(),
            exit_status.signal()
        );
        // The exit status is a triple (NEXT_DEV section 4: runc's contract, the
        // shell convention): a child that died of signal N is reported as
        // `128 + N`, a clean exit passes its own status through unchanged, and
        // 255 is reserved for a runtime failure - a wait that reported neither.
        // `code()` is `None` exactly when the child was killed by a signal, so
        // collapsing that to -1 (which the CLI then exits as 255) lost the
        // guest's own 137/139/143 and made a signal death
        // indistinguishable from a genuine runtime failure (A6).
        let exit_code = exit_status_code(exit_status);
        if self.config.verbose > 0 {
            pn_trace!("[DEBUG] Exit code: {}", exit_code);
        }

        Ok(exit_code)
    }

    /// Run the configured command through the selective-interception backend
    /// (S1) instead of PRoot.
    ///
    /// The command is `execve`'d native: no guest root, no path translation and
    /// no bind mounts, so this is the interception mechanism, not a drop-in for
    /// the PRoot path. It exists so the backend can be measured against PRoot on
    /// the same workload (see `tests/selective_seccomp.rs`).
    pub async fn execute_selective(&mut self) -> Result<i32> {
        if self.config.command.is_empty() {
            return Err(anyhow::anyhow!("No command specified"));
        }
        let selected = selective::selected_syscalls();
        let run = selective::run_selected_cmd(&self.config.command, &selected)?;
        pn_trace!(
            "[DEBUG] selective backend: {} intercepted, {} ms, exit {}",
            run.intercepted(),
            run.elapsed.as_millis(),
            run.exit_code
        );
        Ok(run.exit_code)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.ptrace_ctrl.lock().shutdown()
    }
}

/// Initialize PRoot core with default configuration
pub fn init() -> Result<ProotEngine> {
    let config = ProotConfig::default();
    ProotEngine::new(config)
}

/// The exit code a completed wait maps onto (NEXT_DEV section 4: runc's
/// contract). See the call site for why the signal case cannot be collapsed
/// into the code case.
pub fn exit_status_code(status: std::process::ExitStatus) -> i32 {
    exit_status_code_parts(status.code(), status.signal())
}

/// The rule over its two inputs, so all three branches - including the 255
/// runtime-failure branch, which no real wait on this device produces - are
/// testable directly.
fn exit_status_code_parts(code: Option<i32>, signal: Option<i32>) -> i32 {
    match (code, signal) {
        // A clean exit: the child's own status, passed through unchanged.
        (Some(code), _) => code,
        // A signal death: `128 + N`, the shell convention. Never invented - the
        // signal comes from the wait status itself, not from a guess about what
        // was delivered.
        (None, Some(signal)) => 128 + signal,
        // Neither: 255, reserved for a runtime failure.
        (None, None) => 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let engine = init();
        assert!(engine.is_ok());
    }

    #[test]
    fn test_proot_engine_new() {
        let config = ProotConfig::default();
        let engine = ProotEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_proot_config_default() {
        let config = ProotConfig::default();
        assert!(config.command.is_empty());
        assert!(config.bind.is_empty());
        assert!(config.env.is_empty());
    }

    /// Issue #68: an interactive child must not be moved out of the terminal's foreground group,
    /// or its reads stop on SIGTTIN. Inverting either assertion breaks real interactive use.
    #[test]
    fn an_interactive_run_does_not_take_its_own_process_group() {
        assert!(
            !child_owns_process_group(true),
            "a child with a tty on stdin must stay in the caller's group"
        );
        assert!(
            child_owns_process_group(false),
            "a child without a tty should get its own group for group-signalling"
        );
    }

    /// The guest inherits the caller's terminal, and an empty value is treated as absent.
    #[test]
    fn the_guest_inherits_the_callers_term() {
        assert_eq!(guest_term(Some("footest")), "footest");
        assert_eq!(guest_term(Some("xterm-kitty")), "xterm-kitty");
        assert_eq!(guest_term(Some("")), "xterm-256color");
        assert_eq!(guest_term(None), "xterm-256color");
    }

    #[test]
    fn test_proot_config_custom() {
        let config = ProotConfig {
            command: vec!["/bin/bash".to_string()],
            bind: vec!["/host:/guest".to_string()],
            env: vec!["TEST=value".to_string()],
            ..Default::default()
        };
        assert_eq!(config.command, vec!["/bin/bash".to_string()]);
        assert_eq!(config.bind, vec!["/host:/guest".to_string()]);
        assert_eq!(config.env, vec!["TEST=value".to_string()]);
    }

    /// All three branches of the triple rule (A6). The 255 branch is not
    /// reachable from a real wait on this device, so it is asserted here over
    /// the rule itself rather than only through a guest.
    #[test]
    fn the_exit_status_triple() {
        // Signal death: `128 + N`. 9 = SIGKILL (137), 15 = SIGTERM (143).
        assert_eq!(exit_status_code_parts(None, Some(9)), 137);
        assert_eq!(exit_status_code_parts(None, Some(15)), 143);
        assert_eq!(exit_status_code_parts(None, Some(11)), 139);
        // The child's own status, including 255 as the child's own choice.
        assert_eq!(exit_status_code_parts(Some(3), None), 3);
        assert_eq!(exit_status_code_parts(Some(255), None), 255);
        // Neither: a runtime failure.
        assert_eq!(exit_status_code_parts(None, None), 255);
    }

    /// The rule was read from `ExitStatus`, not assumed: a real child that
    /// SIGKILLs itself must map to 137 (it used to become -1 and exit 255).
    #[test]
    fn a_real_sigkilled_child_maps_to_137() {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("kill -9 $$")
            .status()
            .expect("spawn /bin/sh");
        assert_eq!(status.code(), None, "a signal death carries no exit code");
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert_eq!(exit_status_code(status), 137);
    }

    /// And a real clean exit passes its own code through.
    #[test]
    fn a_real_exit_passes_its_code_through() {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 3")
            .status()
            .expect("spawn /bin/sh");
        assert_eq!(exit_status_code(status), 3);
    }
}
