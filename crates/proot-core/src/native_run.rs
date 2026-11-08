//! Running a program without the external `proot` binary - step 3 of the native-engine plan.
//!
//! FINDING (measured 2026-11-08 14:58 WIB, PDN master 19df16b):
//! `ProotEngine::run()` (`lib.rs:173-184`) installs the seccomp filter and enters `run_loop`
//! without ever spawning a child, so it can never see a tracee; `ProotEngine::start()` (`:187`)
//! spawns the EXTERNAL `proot` binary with six argv features
//! (`--kill-on-exit --link2symlink --sysvipc -L --change-id=0:0:0 --rootfs=.`). "Run containers
//! with podnext's own binary" is therefore a subsystem (spawn path, those six features, execve
//! interception + path translation), not a wiring job. This module is the first increment of it.
//!
//! SCOPE OF THIS INCREMENT - small on purpose, so that capability is MEASURABLE rather than claimed:
//!
//!   * spawn a child that parks in a pipe `read`, `PTRACE_SEIZE` + `PTRACE_INTERRUPT` it so the
//!     engine owns the tracee before it can exec, and no external binary is involved (2026-11-08,
//!     p4 step 3b: `PTRACE_TRACEME` was dropped - its stop is delivered only at `execve`);
//!   * trace syscall stops with `PTRACE_SYSCALL` and report the number the tracee is stopped on
//!     through the existing `Tracee` (p4 step 1, commit 771b817, made that number real rather than
//!     the constant 0 it used to be);
//!   * translate the path argument of ONE syscall - `execve` - from a guest path to
//!     `<rootfs>/<path>`, and prove which binary ran by reading `/proc/<pid>/exe` at the
//!     `PTRACE_EVENT_EXEC` stop;
//!   * record a NAMED refusal for every other syscall: observed, passed through to the kernel,
//!     NOT emulated. In `strict` mode the run stops at the first refusal instead of going on to
//!     look like parity.
//!
//! MEASUREMENT THAT SHAPED THE SYNC (2026-11-08, first run of this module's test): the engine
//! prepared the path at its own mapping of the shared page and compared that address against
//! `execve`'s first argument, refusing with "path pointer is 0x74dd35da60 but the engine prepared
//! 0x776eb42000". The two addresses are page-aligned differently and differ by ~24 GB, while the
//! PAGE is the same physical memory in both processes (mapped before the fork with `MAP_SHARED`).
//! So the engine must not assume that its own address is the tracee's address: the child publishes
//! the pointer and length it is about to pass, in a header inside that shared page, and the engine
//! believes the child. Writing the translated bytes still lands in the child's view, because it is
//! one page of shared memory - only the ADDRESS is per-process. Refusing on the address alone made
//! arm A of the test execute the HOST's `/bin/true` (Termux has `/bin` -> `$PREFIX/bin`), which is
//! exactly the false positive the module exists to prevent.
//!
//! SECOND MEASUREMENT THAT SHAPED THE TRACE (2026-11-08, p4 step 3b): `PTRACE_TRACEME`'s initial
//! stop is delivered at the child's `execve`, not before it, so the trace began on the STARTUP of
//! an already-exec'd program (`faccessat`, `openat`, `read`) and the child's own pre-exec syscalls
//! - the markers and the `execve` whose path must be rewritten - were never observed. The engine now
//! does NOT use `PTRACE_TRACEME`: the child's FIRST syscall is a blocking `read` on a pipe, the
//! engine `PTRACE_SEIZE`s it (options in force from the first stop) and `PTRACE_INTERRUPT`s it,
//! consumes the `PTRACE_EVENT_STOP` group-stop, then releases the child by writing one byte into the
//! pipe. No SIGSTOP is raised (a group-stop raised with SIGSTOP and then continued with
//! `PTRACE_SYSCALL` does not resume syscall-stops), and there is no fork race: the child cannot pass
//! its `execve` until the engine owns it.
//!
//! NOT DONE HERE, named so the gap is explicit rather than implied (all of it is p4 step 3b+):
//! bind mounts and the other five argv features; traces of tracees that fork (`CLONE`/`FORK` are
//! refusals, not followed); argument translation for any syscall other than `execve`; the guest's
//! environment (an empty `envp` is passed on purpose - see `child_main`); and a tracee the engine
//! did NOT fork. That last one is the real difference from proot: proot attaches to a process whose
//! memory it does not own, so it needs `PTRACE_SETREGSET` plus a writable scratch address inside
//! the tracee. Here the string lives in a page the engine and the child share, which is why the
//! rewrite is a plain memory write.

use anyhow::{anyhow, Result};
use nix::sys::ptrace::{self, Options};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::path::{Path, PathBuf};

use crate::arch::Arch;
use crate::tracee::Tracee;

/// One page shared between the engine and the child, laid out as four regions. The page is mapped
/// BEFORE the fork; its physical memory is shared, while its ADDRESS is per-process (measured
/// 2026-11-08, see the module docs).
///
///   [   0 ..  512) the path string the kernel will read at `execve` - the engine rewrites this
///   [ 512 .. 1024) argv[0], the guest's own name, never rewritten (the guest should see its own
///                  path, not the host path the engine substituted)
///   [1024 .. 1536) the argv pointer array, terminated by a null pointer
///   [1536 .. 1600) the child's self-report: magic, the pointer it will pass to `execve`, and the
///                  length of the path it wrote
const PAGE_BYTES: usize = 4096;
const PATH_OFFSET: usize = 0;
const ARGV0_OFFSET: usize = 512;
const ARGV_OFFSET: usize = 1024;
const HEADER_OFFSET: usize = 1536;
/// A translated path must fit below `ARGV0_OFFSET`; anything longer is refused by name rather than
/// written over the guest's argv[0].
const PATH_CAPACITY: usize = ARGV0_OFFSET - 1;
/// `"P4NR"` + version, so a header that was never written (or was written by something else) is
/// visible as such instead of read as zeroes.
const HEADER_MAGIC: u64 = 0x5034_4E52_0000_0001;

/// Upper bound on syscall stops before the engine gives up. `PTRACE_SYSCALL` stops twice per
/// syscall, and a dynamically linked program (a libc start-up plus `mmap`/`mprotect` traffic) uses
/// a few hundred. A run that exceeds this is mis-tracing, and the error says so instead of looping
/// forever.
const STOP_BUDGET: usize = 200_000;

/// How many entry stops the report keeps for inspection.
pub const OBSERVED_CAP: usize = 24;

/// The single place the engine's trace options are set - now at `PTRACE_SEIZE` time rather than at a
/// `PTRACE_TRACEME` stop, so they are in force from the engine's first stop. `TRACESYSGOOD` is what
/// distinguishes a syscall stop from a signal stop; `TRACEEXEC` gives the `PTRACE_EVENT_EXEC` stop
/// used to read `/proc/<pid>/exe`; `EXITKILL` kills the tracee if the engine dies (2026-11-08,
/// p4 step 3b).
const TRACE_OPTIONS: Options = Options::PTRACE_O_TRACESYSGOOD
    .union(Options::PTRACE_O_TRACEEXEC)
    .union(Options::PTRACE_O_EXITKILL);

/// `PTRACE_SEIZE`/`PTRACE_INTERRUPT` are NOT declared by the Android/AArch64 `libc` crate (it
/// declares only up to `PTRACE_SETREGSET`, `PTRACE_SECCOMP_GET_METADATA` and `PTRACE_EVENT_STOP`),
/// and `nix 0.27`'s `ptrace::seize`/`ptrace::interrupt` are gated `#[cfg(target_os = "linux")]`, so
/// they are absent on Android (measured 2026-11-08: `cannot find function seize in module ptrace`).
/// The request numbers are fixed by the Linux ptrace ABI (`include/uapi/linux/ptrace.h`), so they
/// are named here rather than spelled at the call site, exactly as `NT_PRSTATUS` is in `tracee.rs`.
const PTRACE_SEIZE: nix::libc::c_int = 0x4206;
const PTRACE_INTERRUPT: nix::libc::c_int = 0x4207;

/// What the child says about itself, read from the shared header after its first stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildView {
    /// The pointer the child will pass to `execve`. Kept as the child sees it - the engine's own
    /// mapping of the same page can have a different address.
    pub path_ptr: u64,
    /// Bytes the child wrote at `PATH_OFFSET`, including the terminator that is not counted.
    pub path_len: usize,
}

/// What the engine should do with paths. `rootfs: None` means "run the program the guest named,
/// resolved on the host filesystem" - the arm that proves the translation below is load-bearing.
#[derive(Debug, Clone, Default)]
pub struct RunPlan {
    pub rootfs: Option<PathBuf>,
    /// Extra guest->host prefixes (bind mounts). Empty in this increment: only the rootfs rule is
    /// implemented, and `binds` exists so the shape of the next step is visible.
    pub binds: Vec<(String, PathBuf)>,
}

/// A path the engine rewrote at a syscall stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub guest: String,
    pub host: PathBuf,
    pub syscall: &'static str,
}

/// A syscall the engine OBSERVED but does not emulate or translate. Not an error: for a
/// proot-style engine, letting the kernel do the work is the design. It is recorded so that no
/// reader can mistake a passed-through syscall for a handled one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub number: i64,
    /// `None` when the number is not in the table below. A number with no name is reported as such
    /// rather than given an invented one.
    pub name: Option<&'static str>,
    /// Why this syscall was not handled, in the engine's own words. A refusal without a reason is
    /// an admission the engine cannot explain, so every construction site states one.
    pub reason: &'static str,
}

impl Refusal {
    /// How a refusal appears in a report or an error: `SYS_MMAP (222) [passed through, not
    /// emulated]`, or `number 443 (no name in this engine's table) [...]`.
    pub fn label(&self) -> String {
        let head = match self.name {
            Some(n) => format!("{} ({})", n, self.number),
            None => format!("number {} (no name in this engine's table)", self.number),
        };
        format!("{} [{}]", head, self.reason)
    }
}

/// The outcome of one native run.
#[derive(Debug, Clone)]
pub struct NativeRunReport {
    /// The child's exit code, or `128 + signal` when it was killed.
    pub exit_code: i32,
    /// Total syscall stops seen (entry plus exit).
    pub stops: usize,
    /// Entry stops seen - the count of syscalls actually requested.
    pub entries: usize,
    /// Paths the engine rewrote, in order.
    pub translations: Vec<Translation>,
    /// Syscalls observed but not handled, in order.
    pub refusals: Vec<Refusal>,
    /// The first `OBSERVED_CAP` entry stops as `(syscall number, first argument)`. This is the
    /// diagnostic that answers "what did the engine actually see?" - written after a run whose
    /// `execve` argument did not match the address the engine had prepared, because a report can
    /// only be argued from measurements it carries.
    pub observed: Vec<(i64, u64)>,
    /// What the child reported about itself, or `None` when the header never appeared (which is
    /// itself a finding: the engine and the child disagree about the page).
    pub child_view: Option<ChildView>,
    /// The binary the kernel actually executed, read from `/proc/<pid>/exe` at the exec event.
    /// `None` means no exec event was seen, which is itself a finding about the run.
    pub executed: Option<PathBuf>,
    /// Set when `strict` stopped the run: the refusal that stopped it, by name.
    pub strict_failure: Option<String>,
}

impl NativeRunReport {
    /// Refusals grouped by name, largest first - the shape a capability report wants.
    pub fn refusal_summary(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for r in &self.refusals {
            let label = r.label();
            match counts.iter_mut().find(|(l, _)| *l == label) {
                Some((_, c)) => *c += 1,
                None => counts.push((label, 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts
    }
}

/// Names for the syscall numbers this engine knows. Deliberately partial: a missing name reports
/// as `None`, which is the honest answer, instead of a plausible-looking string.
///
/// The numbers are compared against `libc`'s per-architecture constants (`libc::SYS_execve`), not
/// the aarch64-only table in `arch::sysnum`, because this module must behave on the x86_64 CI
/// runner as well - the same class of mistake as the `PTRACE_GETREGSET` cast that broke the runner
/// build on run 42449343311.
pub fn syscall_name(number: i64) -> Option<&'static str> {
    // Only constants this engine's TARGET libc actually declares. Measured 2026-11-08: an earlier
    // version of this table also named `SYS_fstat`, `SYS_newfstatat`, `SYS_rseq` and `SYS_fork`,
    // and the crate did not compile on Android/aarch64 at all (`cannot find value SYS_fstat in
    // crate nix::libc`, and the same for the other three) - aarch64 has no `fork` syscall, and the
    // Android libc crate does not export the other three. A name table is a claim about the
    // target's ABI, so it may only contain constants the target declares; the runner would accept
    // the wider table, which is exactly how a device-only compile error hides until CI explains it.
    Some(match number {
        x if x == nix::libc::SYS_execve as i64 => "SYS_execve",
        x if x == nix::libc::SYS_openat as i64 => "SYS_openat",
        x if x == nix::libc::SYS_read as i64 => "SYS_read",
        x if x == nix::libc::SYS_write as i64 => "SYS_write",
        x if x == nix::libc::SYS_close as i64 => "SYS_close",
        x if x == nix::libc::SYS_mmap as i64 => "SYS_mmap",
        x if x == nix::libc::SYS_mprotect as i64 => "SYS_mprotect",
        x if x == nix::libc::SYS_munmap as i64 => "SYS_munmap",
        x if x == nix::libc::SYS_brk as i64 => "SYS_brk",
        x if x == nix::libc::SYS_getpid as i64 => "SYS_getpid",
        x if x == nix::libc::SYS_exit as i64 => "SYS_exit",
        x if x == nix::libc::SYS_exit_group as i64 => "SYS_exit_group",
        x if x == nix::libc::SYS_prlimit64 as i64 => "SYS_prlimit64",
        x if x == nix::libc::SYS_set_tid_address as i64 => "SYS_set_tid_address",
        x if x == nix::libc::SYS_clone as i64 => "SYS_clone",
        x if x == nix::libc::SYS_readlinkat as i64 => "SYS_readlinkat",
        x if x == nix::libc::SYS_getrandom as i64 => "SYS_getrandom",
        x if x == nix::libc::SYS_ioctl as i64 => "SYS_ioctl",
        _ => return None,
    })
}

/// Run `argv` under this engine: fork, trace, translate `execve`'s path, and report what happened.
///
/// `strict` turns the first unhandled syscall into a named failure instead of a recorded refusal -
/// the honest answer to "can this engine host this program yet?".
pub fn run_native(argv: &[String], plan: &RunPlan, strict: bool) -> Result<NativeRunReport> {
    if argv.is_empty() {
        return Err(anyhow!("run_native needs at least one argv entry"));
    }
    let guest_path = argv[0].clone();
    if guest_path.len() > PATH_CAPACITY {
        return Err(anyhow!(
            "guest path of {} bytes does not fit the {} byte translation area: {}",
            guest_path.len(),
            PATH_CAPACITY,
            guest_path
        ));
    }

    // Mapped BEFORE the fork, so the engine and the child share the physical page. (The ADDRESS is
    // per-process and is never assumed to match - see `ChildView`.)
    let page = unsafe {
        nix::libc::mmap(
            std::ptr::null_mut(),
            PAGE_BYTES,
            nix::libc::PROT_READ | nix::libc::PROT_WRITE,
            nix::libc::MAP_SHARED | nix::libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if page == nix::libc::MAP_FAILED {
        return Err(anyhow!(
            "mmap for the translation page failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // (2026-11-08, p4 step 3b) A pipe the child blocks on as its FIRST syscall. It is what lets the
    // engine attach before the child can exec: the child cannot pass its `execve` until the engine
    // writes one byte, and the engine writes only after it owns the tracee.
    let (pipe_r, pipe_w) = nix::unistd::pipe()?;

    match unsafe { fork() }? {
        ForkResult::Child => {
            // Close the write end we inherited: only the engine releases the child.
            let _ = nix::unistd::close(pipe_w);
            child_main(page as *mut u8, &guest_path, argv, pipe_r)
        }
        ForkResult::Parent { child } => {
            // The engine only writes the release byte; the child owns the read end.
            let _ = nix::unistd::close(pipe_r);
            let report = parent_loop(child, page as *mut u8, plan, strict, pipe_w);
            let _ = nix::unistd::close(pipe_w);
            unsafe { nix::libc::munmap(page, PAGE_BYTES) };
            report
        }
    }
}

/// The child half: park in a blocking `read` so the engine can own the tracee before any exec,
/// publish the path, argv and a self-report into the shared page, then call `execve`.
///
/// (2026-11-08, p4 step 3b) This used to call `ptrace(PTRACE_TRACEME)`; that stop is delivered at
/// the child's `execve`, so the engine's trace began on the startup of an already-exec'd program and
/// the child's own pre-exec syscalls were invisible. The child now does NOT trace itself: its first
/// syscall is the `read` below, and the engine `PTRACE_SEIZE`s + `PTRACE_INTERRUPT`s it there.
fn child_main(page: *mut u8, guest_path: &str, argv: &[String], pipe_r: nix::libc::c_int) -> ! {
    // SAFETY: the child is a fresh fork of a single-threaded test/engine process, and every pointer
    // below is derived from the page mapped by `run_native` before the fork.
    unsafe {
        // FIRST syscall, before anything else: block until the engine releases us. The engine
        // seizes and interrupts the child while it is parked in this read, so the trace is live
        // before the execve below (2026-11-08, p4 step 3b). EINTR is retried: a PTRACE_INTERRUPT
        // group-stop resumes this read rather than failing it.
        let mut release = 0u8;
        loop {
            let n = nix::libc::read(pipe_r, &mut release as *mut u8 as *mut nix::libc::c_void, 1);
            if n >= 0 {
                break;
            }
            if *nix::libc::__errno() != nix::libc::EINTR {
                nix::libc::_exit(126);
            }
        }
        nix::libc::close(pipe_r);

        write_cstr(page.add(PATH_OFFSET), guest_path);
        write_cstr(page.add(ARGV0_OFFSET), &argv[0]);
        // argv as the guest should see it: argv[0] is the program's own (guest) name. Only the
        // syscall's path argument is translated, not what the program is told it was called.
        let argv_slots = page.add(ARGV_OFFSET) as *mut *const nix::libc::c_char;
        *argv_slots = page.add(ARGV0_OFFSET) as *const nix::libc::c_char;
        *argv_slots.add(1) = std::ptr::null();
        // The self-report the engine believes instead of its own address arithmetic.
        let header = page.add(HEADER_OFFSET) as *mut u64;
        *header = HEADER_MAGIC;
        *header.add(1) = page.add(PATH_OFFSET) as u64;
        *header.add(2) = guest_path.len() as u64;

        // The child's own marker before its `execve`, issued as a RAW syscall on purpose: Android's
        // bionic CACHES `getpid()`, so `libc::getpid` is often not a syscall at all and would leave
        // the engine no stop to anchor the trace on (measured 2026-11-08: the `getpid` marker never
        // appeared as a stop, while a raw `syscall(SYS_getppid)` did). The raw `getpid` is a real
        // syscall whose entry stop is the one immediately before the translated `execve`.
        let _sync_marker = nix::libc::syscall(nix::libc::SYS_getpid);
        std::hint::black_box(_sync_marker);

        // An EMPTY environment, deliberately: `nix::libc::environ` is not declared for the Android
        // target (measured 2026-11-08: `cannot find value environ in crate nix::libc`), and handing
        // the guest the engine's own environment would be a claim about isolation this increment
        // has not measured. Passing no environment is the honest version of that, and it is named
        // here as a limitation: p4-3b decides the guest's environment on purpose.
        let empty_envp: [*const nix::libc::c_char; 1] = [std::ptr::null()];

        // `execve` is issued as a RAW syscall, NOT through `libc::execve`, because a preloaded
        // library can interpose the libc symbol and then the tracee's execve never surfaces as a
        // syscall stop the engine can rewrite. Measured 2026-11-08 on this device, where the
        // environment sets `LD_PRELOAD=libtermux-exec-ld-preload.so`: with `libc::execve` the child
        // exec'd the HOST `/bin/true` (exit 0, `executed=.../usr/bin/coreutils`) and the engine saw
        // NO `SYS_execve` entry stop at all, so nothing was translated; with the raw syscall the
        // entry stop appeared, the path was rewritten, and the fixture ran (`executed` named
        // `rootfs-a/bin/true`). A proot-style engine must not let an interposer sit between it and
        // the exec it translates, so the syscall is made directly.
        let _exec_rc = nix::libc::syscall(
            nix::libc::SYS_execve,
            page.add(PATH_OFFSET) as *const nix::libc::c_char,
            page.add(ARGV_OFFSET) as *const *const nix::libc::c_char,
            empty_envp.as_ptr(),
        );
        std::hint::black_box(_exec_rc);
        // execve only returns on failure; 127 is the conventional "could not execute".
        nix::libc::_exit(127)
    }
}

/// The tracer half.
fn parent_loop(
    child: Pid,
    page: *mut u8,
    plan: &RunPlan,
    strict: bool,
    pipe_w: nix::libc::c_int,
) -> Result<NativeRunReport> {
    let mut report = NativeRunReport {
        exit_code: -1,
        stops: 0,
        entries: 0,
        translations: Vec::new(),
        refusals: Vec::new(),
        observed: Vec::new(),
        child_view: None,
        executed: None,
        strict_failure: None,
    };

    // (2026-11-08, p4 step 3b) Own the tracee BEFORE its execve. `PTRACE_SEIZE` attaches without
    // stopping it and carries the options, so they are in force from the engine's first stop;
    // `PTRACE_INTERRUPT` then brings the child - parked in its opening `read()` - to a group-stop
    // the engine owns (reported as a `PTRACE_EVENT_STOP`). `PTRACE_TRACEME` is deliberately NOT
    // used: its stop is delivered at `execve`, after the syscalls the engine must see. The requests
    // go through `libc` because neither `PTRACE_SEIZE`/`PTRACE_INTERRUPT` nor `nix`'s wrappers
    // exist on Android (see the constants above); the request is cast `as _` so the same source
    // compiles where `libc` declares `ptrace`'s request as `c_int` and where it is `c_uint`.
    let seized = unsafe {
        nix::libc::ptrace(
            PTRACE_SEIZE as _,
            child.as_raw(),
            std::ptr::null_mut::<nix::libc::c_void>(),
            TRACE_OPTIONS.bits() as *mut nix::libc::c_void,
        )
    };
    if seized < 0 {
        let _ = ptrace::kill(child);
        return Err(anyhow!(
            "PTRACE_SEIZE failed for pid {}: {}",
            child,
            std::io::Error::last_os_error()
        ));
    }
    let interrupted = unsafe {
        nix::libc::ptrace(
            PTRACE_INTERRUPT as _,
            child.as_raw(),
            std::ptr::null_mut::<nix::libc::c_void>(),
            std::ptr::null_mut::<nix::libc::c_void>(),
        )
    };
    if interrupted < 0 {
        let _ = ptrace::kill(child);
        return Err(anyhow!(
            "PTRACE_INTERRUPT failed for pid {}: {}",
            child,
            std::io::Error::last_os_error()
        ));
    }
    match waitpid(child, None)? {
        WaitStatus::PtraceEvent(_, _, event) if event == nix::libc::PTRACE_EVENT_STOP => {}
        other => {
            let _ = ptrace::kill(child);
            return Err(anyhow!(
                "PTRACE_INTERRUPT did not produce a PTRACE_EVENT_STOP group-stop for the tracee; \
                 got {:?}",
                other
            ));
        }
    }
    // Release the child's blocked `read()`. The child is still stopped here, so no exec can slip
    // past the engine; the loop below resumes it with `PTRACE_SYSCALL`. The write is checked: a
    // failed one would leave the tracee parked forever.
    let release = [1u8];
    let wrote =
        unsafe { nix::libc::write(pipe_w, release.as_ptr() as *const nix::libc::c_void, 1) };
    if wrote != 1 {
        let _ = ptrace::kill(child);
        return Err(anyhow!(
            "releasing the tracee's pipe failed: wrote {} byte(s), expected 1",
            wrote
        ));
    }

    let mut tracee = Tracee::new(child, Arch::from_host());
    // TRUE: the trace resumes from a PTRACE_INTERRUPT group-stop, where the child is parked in its
    // opening `read()`, and the kernel re-reports that `read` as a fresh syscall ENTRY. So the first
    // stop this loop sees is the ENTRY of the child's release `read` (measured 2026-11-08, p4 step
    // 3b: `observed` begins with the child's own `close`/`read`/`close`, then its raw `getpid`
    // marker, then its `execve`). A `false` here shifts every entry/exit pair by one, so return
    // values are read as arguments and the real `execve` entry stop is never examined.
    let mut at_entry = true;
    let mut execve_handled = false;

    loop {
        if report.stops > STOP_BUDGET {
            let _ = ptrace::kill(child);
            return Err(anyhow!(
                "stopped tracing after {} syscall stops without the tracee exiting",
                STOP_BUDGET
            ));
        }
        ptrace::syscall(child, None)?;
        match waitpid(child, None) {
            Ok(WaitStatus::PtraceSyscall(_)) => {
                report.stops += 1;
                if at_entry {
                    report.entries += 1;
                    // The child writes its self-report after its opening `read()` returns, i.e. only
                    // after the engine releases it, so the header is read on the first entry stop at
                    // which it exists (2026-11-08, p4 step 3b). Reading it before the release would
                    // see unwritten memory.
                    if report.child_view.is_none() {
                        report.child_view = read_child_view(page);
                    }
                    tracee.observe_syscall()?;
                    let (number, arg0) = match tracee.syscall_info.as_ref() {
                        Some(info) => (info.number, info.args[0]),
                        None => {
                            let _ = ptrace::kill(child);
                            return Err(anyhow!(
                                "observe_syscall reported success but left no syscall_info"
                            ));
                        }
                    };
                    if report.observed.len() < OBSERVED_CAP {
                        report.observed.push((number, arg0));
                    }
                    if number == nix::libc::SYS_execve as i64 {
                        if execve_handled {
                            // A second execve is how a script's interpreter is started. Translating
                            // it here would be guesswork about which path is a guest path, so it is
                            // recorded instead.
                            report.refusals.push(Refusal {
                                number,
                                name: syscall_name(number),
                                reason: "a later execve (an interpreter for a script) is not translated",
                            });
                        } else if !path_pointer_is_ours(&report, arg0) {
                            // The engine only rewrites the path it published, and it learns where
                            // that is from the CHILD, not from its own mapping of the shared page
                            // (measured 2026-11-08: the two addresses differ). Anything else needs
                            // PTRACE_SETREGSET plus a writable address inside the tracee's memory,
                            // so it is recorded with the observed pointer and the run continues
                            // exactly as the kernel would have run it.
                            report.refusals.push(Refusal {
                                number,
                                name: syscall_name(number),
                                reason: "execve's path pointer is not the one this engine prepared, so it cannot be rewritten yet",
                            });
                        } else {
                            execve_handled = true;
                            if let Err(e) = translate_execve(page, plan, &mut report) {
                                let _ = ptrace::kill(child);
                                return Err(e);
                            }
                        }
                    } else {
                        let refusal = Refusal {
                            number,
                            name: syscall_name(number),
                            reason: "passed through, not emulated",
                        };
                        if strict {
                            report.strict_failure = Some(refusal.label());
                            report.refusals.push(refusal);
                            // The program was NOT run: no translation happened, so leave nothing
                            // that could be mistaken for a partial success.
                            report.translations.clear();
                            let _ = ptrace::kill(child);
                            break;
                        }
                        report.refusals.push(refusal);
                    }
                }
                // `execve` succeeds without an exit stop (the image is replaced), so the parity
                // between entry and exit stops is re-established by the exec event below.
                at_entry = !at_entry;
            }
            Ok(WaitStatus::PtraceEvent(_, _, event)) if event == nix::libc::PTRACE_EVENT_EXEC => {
                // Direct evidence of WHICH binary ran, rather than an inference from an exit code.
                report.executed = std::fs::read_link(format!("/proc/{}/exe", child.as_raw())).ok();
                // FALSE, not true: the next `PTRACE_SYSCALL` stop is the execve's own syscall-EXIT
                // (the syscall has already succeeded and the image is replaced), so the stop AFTER
                // that is the first ENTRY of the new program. Measured 2026-11-08, p4 step 3b:
                // `PTRACE_GET_SYSCALL_INFO` reported op=EXIT on the stop right after the exec event.
                at_entry = false;
            }
            Ok(WaitStatus::Exited(_, code)) => {
                report.exit_code = code;
                break;
            }
            Ok(WaitStatus::Signaled(_, sig, _)) => {
                report.exit_code = 128 + sig as i32;
                break;
            }
            Ok(_) => {}
            Err(nix::Error::ECHILD) => break,
            Err(e) => {
                let _ = ptrace::kill(child);
                return Err(anyhow!(
                    "waitpid failed while tracing the native run: {}",
                    e
                ));
            }
        }
    }

    Ok(report)
}

/// Read the child's self-report out of the shared header, or `None` when it has not been written
/// yet (magic mismatch). The engine believes this instead of its own address for the shared page.
///
/// SAFETY (of the caller's `page`): HEADER_OFFSET..HEADER_OFFSET+24 is inside the page `run_native`
/// mapped before the fork.
fn read_child_view(page: *mut u8) -> Option<ChildView> {
    unsafe {
        let header = page.add(HEADER_OFFSET) as *const u64;
        if *header == HEADER_MAGIC {
            Some(ChildView {
                path_ptr: *header.add(1),
                path_len: *header.add(2) as usize,
            })
        } else {
            None
        }
    }
}

/// Did the tracee call `execve` on the path this engine published? Answered from the child's own
/// report, so the engine's per-process address for the shared page is irrelevant.
fn path_pointer_is_ours(report: &NativeRunReport, arg0: u64) -> bool {
    match report.child_view.as_ref() {
        Some(view) => view.path_ptr == arg0,
        // No header: the engine has no idea what the child prepared, so it must not rewrite it.
        None => false,
    }
}

/// The one translation this increment implements: the `execve` path argument.
///
/// The caller has already established that the syscall is `execve` and that its path pointer is the
/// one the child published; anything else is recorded as a refusal rather than reaching here.
fn translate_execve(page: *mut u8, plan: &RunPlan, report: &mut NativeRunReport) -> Result<()> {
    let Some(rootfs) = plan.rootfs.as_ref() else {
        // No plan: the guest path is used as-is by the kernel. Not an error - it is the control arm
        // showing that the translation is what makes a rootfs-relative run work.
        return Ok(());
    };

    // SAFETY: `page` is the mapping `run_native` made before the fork; PATH_OFFSET..PATH_CAPACITY
    // is inside it, and `read_cstr` stays within that capacity.
    let path_at = unsafe { page.add(PATH_OFFSET) };
    let guest = read_cstr(path_at)?;
    if guest.is_empty() {
        return Err(anyhow!("execve was called with an empty path"));
    }
    let host = resolve(rootfs, &guest, &plan.binds);
    if !host.exists() {
        // The honest failure: no fallback to the host filesystem, because a silent fallback is
        // exactly how a container would appear to work while running a host binary.
        return Err(anyhow!(
            "translated path does not exist: {} -> {} (this engine does not fall back to the host \
             filesystem)",
            guest,
            host.display()
        ));
    }
    let bytes = host.as_os_str().as_encoded_bytes();
    if bytes.len() > PATH_CAPACITY {
        return Err(anyhow!(
            "translated path of {} bytes does not fit the {} byte translation area: {}",
            bytes.len(),
            PATH_CAPACITY,
            host.display()
        ));
    }
    unsafe { write_cstr(path_at, &host.to_string_lossy()) };
    report.translations.push(Translation {
        guest,
        host,
        syscall: "SYS_execve",
    });
    Ok(())
}

/// Guest path -> host path. Longest bind prefix first, then the rootfs rule. A guest path that is
/// not absolute is joined under the rootfs as well, so a relative exec cannot escape it.
pub fn resolve(rootfs: &Path, guest: &str, binds: &[(String, PathBuf)]) -> PathBuf {
    for (from, to) in binds {
        if let Some(rest) = guest.strip_prefix(from.as_str()) {
            return to.join(rest.trim_start_matches('/'));
        }
    }
    rootfs.join(guest.trim_start_matches('/'))
}

unsafe fn write_cstr(dst: *mut u8, s: &str) {
    let bytes = s.as_bytes();
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
    *dst.add(bytes.len()) = 0;
}

/// Read a NUL-terminated string from the engine's own shared page (bounded: a run that never
/// terminates its path is a bug in the child, not a reason to walk the page forever).
fn read_cstr(src: *const u8) -> Result<String> {
    let mut out = Vec::new();
    for i in 0..PATH_CAPACITY {
        let b = unsafe { *src.add(i) };
        if b == 0 {
            return Ok(String::from_utf8_lossy(&out).into_owned());
        }
        out.push(b);
    }
    Err(anyhow!(
        "the guest path in the shared page is not NUL-terminated within {} bytes",
        PATH_CAPACITY
    ))
}
