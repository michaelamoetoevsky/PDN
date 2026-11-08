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
//! REWRITE FOR A TRACEE THE ENGINE DID NOT FORK (2026-11-08, p4 step 4): the engine now follows
//! forks (`TRACEFORK`/`VFORK`/`CLONE`), so a process the tracee forks is a tracee too. Such a
//! descendant's `execve` path does NOT live in the shared page - the page was unmapped when the
//! tracee exec'd - so `translate_execve_remote` reads the path out of the descendant with
//! `PTRACE_PEEKDATA`, resolves it through the same `resolve` (binds first, then the rootfs), and
//! points `execve`'s argument register at a copy written onto the descendant's own stack through
//! `PTRACE_POKEDATA` + `PTRACE_SETREGSET`. This is the proot technique the shared page exists to
//! avoid for the engine's OWN child; the shared page is still the rewrite path for that child.
//!
//! BIND MOUNTS (2026-11-08, p4 step 4): `RunPlan::binds` is honoured by `translate_execve` and
//! `translate_execve_remote` through `resolve`: a bind whose guest prefix matches wins over the
//! rootfs, and the longest matching prefix wins. A guest path outside every bind resolves under
//! the rootfs exactly as before. This module has no caller that fills `binds`; a caller (or a
//! test) supplies them in the `RunPlan`.
//!
//! ENTRY-TIME PATH TRANSLATION FOR THE DYNAMIC LOADER (2026-11-08, p7): a dynamically linked guest
//! could not start, because the ELF interpreter runs INSIDE the guest and opens its `NEEDED`
//! libraries by absolute path - paths that name the HOST, not the rootfs. `execve` translation
//! alone is therefore not enough: the kernel starts the image, and the loader's own
//! `openat`/`newfstatat`/`readlinkat`/`faccessat` calls then reach the host filesystem. Those
//! syscalls are now translated AT ENTRY: the path is read out of the tracee with `peek_cstr`,
//! resolved through the same `resolve` (binds first, then the rootfs), and written back with the
//! same stack-scratch + `PTRACE_SETREGSET` technique `execve` uses - one helper,
//! `write_path_and_redirect_arg`, parameterised by the argument register instead of duplicated.
//! A guest path that has no counterpart in the rootfs is still REWRITTEN, so the kernel answers
//! `ENOENT`; it is never left pointing at the host file. `path_syscall` states which syscalls and
//! which argument index; `unhandled_path_syscall` names the path-taking syscalls that have NO
//! translator here, so they are refused by name with a reason that says the host will resolve
//! them - never passed through as if they were handled.
//!
//! GUEST WORKING DIRECTORY (2026-11-09, p7 cwd). `RunPlan::cwd` carries the guest's STARTING
//! directory as a guest-absolute path; the child `chdir`s into its TRANSLATED host path (through
//! the same `resolve` the execve rewrite uses) after the release read and before its `execve`, so
//! the guest starts in the container's configured working directory instead of wherever the host
//! engine process happened to be. The proot path does exactly this with
//! `container.config.working_dir.unwrap_or("/")` (`proot-distro-core/src/container.rs:234-240`,
//! `runtime.rs:152-158`). A cwd that is not absolute, or that contains a `..` component (which
//! `resolve` would join under the rootfs and the KERNEL would then collapse OUTSIDE the rootfs and
//! its binds), is refused BY NAME; a failing `chdir` exits `126` rather than run the guest in the
//! wrong directory. The child's own pre-exec `chdir` is engine setup on an already-translated host
//! path, not a guest path, so it is exempt from the named path-refusal rule below; a guest `chdir`
//! after the exec is still the unhandled path syscall it always was.
//!
//! NOT DONE HERE, named so the gap is explicit rather than implied:
//! the other five argv features; the guest's environment (an empty `envp` is passed on purpose -
//! see `child_main`); `dirfd`-relative path translation (a RELATIVE guest path is still refused by
//! name rather than guessed - only the STARTING cwd is modelled, not a per-syscall `dirfd`); and
//! the `PT_INTERP` path, which the KERNEL opens before the tracee runs a single instruction, so it
//! cannot be translated and must exist on the host.

use anyhow::{anyhow, Result};
use nix::sys::ptrace::{self, Options};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
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
///   [1600 .. 4096) the strings for `argv[1..]`, NUL-separated (2026-11-09, p7 argv). Before this
///                  region existed the engine published ONLY argv[0] and terminated the pointer
///                  array after it, so a guest invoked with arguments saw `argc == 1` and silently
///                  used its own defaults - measured, and undocumented.
const PAGE_BYTES: usize = 4096;
const PATH_OFFSET: usize = 0;
const ARGV0_OFFSET: usize = 512;
const ARGV_OFFSET: usize = 1024;
const HEADER_OFFSET: usize = 1536;
/// Where the strings for `argv[1..]` begin: immediately after the header, in the part of the page
/// that was unused. The capacity is derived from the page size rather than written down twice.
const ARGV_EXTRA_OFFSET: usize = 1600;
const ARGV_EXTRA_CAPACITY: usize = PAGE_BYTES - ARGV_EXTRA_OFFSET - 1;
/// Pointer slots the child's argv array can hold: the region between the array's start and the
/// header, divided by the pointer size. The terminating null occupies one of them.
const ARGV_SLOTS: usize =
    (HEADER_OFFSET - ARGV_OFFSET) / std::mem::size_of::<*const nix::libc::c_char>();
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
/// used to read `/proc/<pid>/exe`; `TRACEFORK`/`TRACEVFORK`/`TRACECLONE` make a process the tracee
/// forks a tracee too, so a grandchild's `execve` is translated like the child's; `EXITKILL` kills
/// the tracee if the engine dies (2026-11-08, p4 step 3b; fork tracing added 2026-11-08, p4 step 4).
const TRACE_OPTIONS: Options = Options::PTRACE_O_TRACESYSGOOD
    .union(Options::PTRACE_O_TRACEEXEC)
    .union(Options::PTRACE_O_TRACEFORK)
    .union(Options::PTRACE_O_TRACEVFORK)
    .union(Options::PTRACE_O_TRACECLONE)
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
    /// Extra guest->host prefixes (bind mounts): `(guest_prefix, host_path)` pairs. Honoured by
    /// `resolve` before the rootfs, longest matching prefix first (2026-11-08, p4 step 4).
    pub binds: Vec<(String, PathBuf)>,
    /// The guest's STARTING working directory, as a GUEST-ABSOLUTE path, or `None` to leave the
    /// child wherever the engine process is. Translated to its host path through `resolve` and
    /// `chdir`'d in the child before its `execve` (2026-11-09, p7 cwd), so a container's configured
    /// `working_dir` reaches the guest (proot uses `working_dir.unwrap_or("/")`). This models only
    /// the STARTING point: a per-syscall `dirfd` is still not modelled and a relative guest path is
    /// still refused by name. Not absolute, or containing `..`, is a NAMED refusal.
    pub cwd: Option<String>,
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

/// `SYS_newfstatat` is declared by the libc crate for linux-gnu (79 on arm64, 262 on x86_64) and
/// for android/x86_64, but NOT for android/aarch64 - measured 2026-11-08 on libc 0.2.191, whose
/// `android/b64/aarch64/mod.rs` has no such constant. The arm64 table (`asm-generic/unistd.h`)
/// fixes it at 79, the same number the gnu arm64 crate declares, so that one number is named here
/// rather than leaving `stat` untranslatable on the device.
#[cfg(any(not(target_os = "android"), target_arch = "x86_64"))]
const SYS_NEWFSTATAT: i64 = nix::libc::SYS_newfstatat as i64;
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const SYS_NEWFSTATAT: i64 = 79;

/// The syscalls whose path argument this engine rewrites at ENTRY, and which argument holds it.
///
/// Only numbers the TARGET's libc actually declares are named. arm64 has no legacy
/// `open`/`stat`/`lstat`/`access`/`readlink` syscalls at all (the table starts at `openat`), so
/// those arms are `x86_64`-only for the same reason the name table above is partial: a constant the
/// target does not declare does not compile, and the runner would accept a wider table - the
/// device-only compile error that CI then has to explain.
/// A path-taking syscall the engine CAN translate: its `name`, the `arg_index` holding the path, and
/// the `dirfd_arg` when the syscall resolves a RELATIVE path against a directory fd.
///
/// The dirfd is part of the entry on purpose (2026-11-09, p7 relative): a relative path is only
/// translatable against `RunPlan::cwd` when the syscall resolves it there, i.e. when `arg0` is
/// `AT_FDCWD` or when the syscall has no dirfd at all (the legacy `open`/`stat`/`access` family). For
/// any other fd the kernel resolves the name against THAT directory, so joining it onto the cwd would
/// silently point at a different file - worse than refusing. Keeping this in the same table is what
/// stops a second list from drifting away from the first.
struct PathSyscall {
    name: &'static str,
    arg_index: usize,
    dirfd_arg: Option<usize>,
}

fn path_syscall(number: i64) -> Option<PathSyscall> {
    if number == nix::libc::SYS_openat as i64 {
        return Some(PathSyscall {
            name: "SYS_openat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_openat2 as i64 {
        return Some(PathSyscall {
            name: "SYS_openat2",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == SYS_NEWFSTATAT {
        return Some(PathSyscall {
            name: "SYS_newfstatat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_statx as i64 {
        return Some(PathSyscall {
            name: "SYS_statx",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_readlinkat as i64 {
        return Some(PathSyscall {
            name: "SYS_readlinkat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_faccessat as i64 {
        return Some(PathSyscall {
            name: "SYS_faccessat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_faccessat2 as i64 {
        return Some(PathSyscall {
            name: "SYS_faccessat2",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    #[cfg(target_arch = "x86_64")]
    {
        if number == nix::libc::SYS_open as i64 {
            return Some(PathSyscall {
                name: "SYS_open",
                arg_index: 0,
                dirfd_arg: None,
            });
        }
        if number == nix::libc::SYS_stat as i64 {
            return Some(PathSyscall {
                name: "SYS_stat",
                arg_index: 0,
                dirfd_arg: None,
            });
        }
        if number == nix::libc::SYS_lstat as i64 {
            return Some(PathSyscall {
                name: "SYS_lstat",
                arg_index: 0,
                dirfd_arg: None,
            });
        }
        if number == nix::libc::SYS_readlink as i64 {
            return Some(PathSyscall {
                name: "SYS_readlink",
                arg_index: 0,
                dirfd_arg: None,
            });
        }
        if number == nix::libc::SYS_access as i64 {
            return Some(PathSyscall {
                name: "SYS_access",
                arg_index: 0,
                dirfd_arg: None,
            });
        }
    }
    None
}

/// Path-taking syscalls this engine has NO translator for. They are refused BY NAME, with a reason
/// that says the kernel will resolve the guest path on the HOST filesystem - the distinction the
/// module exists to keep: a path the engine cannot translate must never be passed through as if it
/// were handled. `strict` mode therefore stops on one instead of continuing to look like parity.
///
/// Only constants declared on every supported target are named (all of these are in the generic
/// non-legacy table, so arm64 has them); the legacy-only `chmod`/`chown`/`rename`/... are not,
/// because they do not exist on arm64 and referencing them would not compile there.
fn unhandled_path_syscall(number: i64) -> Option<&'static str> {
    Some(match number {
        x if x == nix::libc::SYS_chdir as i64 => "SYS_chdir",
        x if x == nix::libc::SYS_mkdirat as i64 => "SYS_mkdirat",
        x if x == nix::libc::SYS_unlinkat as i64 => "SYS_unlinkat",
        x if x == nix::libc::SYS_renameat as i64 => "SYS_renameat",
        x if x == nix::libc::SYS_renameat2 as i64 => "SYS_renameat2",
        x if x == nix::libc::SYS_symlinkat as i64 => "SYS_symlinkat",
        x if x == nix::libc::SYS_linkat as i64 => "SYS_linkat",
        x if x == nix::libc::SYS_fchmodat as i64 => "SYS_fchmodat",
        x if x == nix::libc::SYS_fchownat as i64 => "SYS_fchownat",
        x if x == nix::libc::SYS_utimensat as i64 => "SYS_utimensat",
        x if x == nix::libc::SYS_execveat as i64 => "SYS_execveat",
        x if x == nix::libc::SYS_inotify_add_watch as i64 => "SYS_inotify_add_watch",
        _ => return None,
    })
}

/// Whether a refusal whose syscall `number` is given is a PATH refusal: a path-taking syscall this
/// engine could not translate, so the kernel would resolve the guest's path on the HOST filesystem.
///
/// This is the class of refusal a caller can fail a run on by name. It is the union of the two
/// tables above, kept here so the caller and the engine cannot disagree about which refusals are
/// the dangerous ones - the same single-source rule the name table follows. A syscall that was
/// actually translated produces no refusal, so this predicate is only ever asked about refusals.
pub fn is_untranslated_path_refusal(number: i64) -> bool {
    path_syscall(number).is_some() || unhandled_path_syscall(number).is_some()
}

/// Names for the syscall numbers this engine knows. Deliberately partial: a missing name reports
/// as `None`, which is the honest answer, instead of a plausible-looking string.
///
/// The numbers are compared against `libc`'s per-architecture constants (`libc::SYS_execve`), not
/// the aarch64-only table in `arch::sysnum`, because this module must behave on the x86_64 CI
/// runner as well - the same class of mistake as the `PTRACE_GETREGSET` cast that broke the runner
/// build on run 42449343311.
pub fn syscall_name(number: i64) -> Option<&'static str> {
    // A path syscall has a name whether the engine translates it or refuses it; keeping the two
    // tables as the single source of those names stops the report and the dispatcher drifting.
    if let Some(PathSyscall { name, .. }) = path_syscall(number) {
        return Some(name);
    }
    if let Some(name) = unhandled_path_syscall(number) {
        return Some(name);
    }
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

    // The guest's argv (2026-11-09, p7 argv). Until this increment the engine published argv[0] ONLY
    // and terminated the pointer array after it, so a guest invoked with arguments saw `argc == 1`
    // and fell back to its own defaults - silently, and undocumented (measured: a probe passed
    // `argv[1]` printed its compiled-in default name). Both limits are checked HERE, before the
    // fork, so exceeding them is an `Err` from `run_native` rather than a child that exits 126 for
    // a reason the caller cannot read.
    let argv_slots_needed = argv.len() + 1; // the entries plus the terminating null
    if argv_slots_needed > ARGV_SLOTS {
        return Err(anyhow!(
            "argv of {} entries needs {} slots with the terminating null, and this engine publishes \
             {}: the extra arguments would be dropped rather than run",
            argv.len(),
            argv_slots_needed,
            ARGV_SLOTS
        ));
    }
    let argv_extra_bytes: usize = argv[1..].iter().map(|a| a.len() + 1).sum();
    if argv_extra_bytes > ARGV_EXTRA_CAPACITY {
        return Err(anyhow!(
            "argv strings after argv[0] total {} bytes and do not fit the {} byte region this engine \
             publishes for them",
            argv_extra_bytes,
            ARGV_EXTRA_CAPACITY
        ));
    }

    // The guest's starting directory, translated to its host path ONCE, here in the engine, so a
    // refusal is an `Err` from `run_native` (not a child exit code). `None` leaves the child in
    // whatever directory the engine process is in - the parity gap this increment closes.
    let cwd_host = match plan.cwd.as_deref() {
        Some(guest) => Some(resolve_cwd(plan, guest)?),
        None => None,
    };

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
            child_main(
                page as *mut u8,
                &guest_path,
                argv,
                pipe_r,
                cwd_host.as_deref(),
            )
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
fn child_main(
    page: *mut u8,
    guest_path: &str,
    argv: &[String],
    pipe_r: nix::libc::c_int,
    cwd: Option<&Path>,
) -> ! {
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
            // `std::io::Error::last_os_error()` rather than `*libc::__errno()`. Measured on run
            // 42472894579 (PDN 65b2b81): every CI job that builds the workspace failed within 90 s
            // with `error[E0425]: cannot find function __errno in crate nix::libc` at this line - the
            // libc crate declares `__errno` for android/bsd/newlib/nuttx but not for linux-gnu, where
            // the accessor is `__errno_location`. The device compiled and the runner did not: the
            // same device-vs-runner divergence as the `as c_int` incident. std reads errno on both.
            if std::io::Error::last_os_error().raw_os_error() != Some(nix::libc::EINTR) {
                nix::libc::_exit(126);
            }
        }
        nix::libc::close(pipe_r);

        // The guest's STARTING directory (2026-11-09, p7 cwd): `cwd` is already the TRANSLATED host
        // path (`run_native` resolved it through `resolve_cwd` before the fork). Issued as a RAW
        // syscall for the same reason `execve` below is - a preloaded library can interpose
        // `libc::chdir`, and this must reach the kernel as a `SYS_chdir` the engine can see. A
        // failure is FATAL: continuing would run the guest in the wrong directory, the exact parity
        // gap this increment closes. 126 is the same "could not proceed" code the release-read
        // failure above uses.
        if let Some(dir) = cwd {
            let mut path_bytes = dir.as_os_str().as_encoded_bytes().to_vec();
            path_bytes.push(0);
            let rc = nix::libc::syscall(
                nix::libc::SYS_chdir,
                path_bytes.as_ptr() as *const nix::libc::c_char,
            );
            if rc != 0 {
                nix::libc::_exit(126);
            }
        }

        write_cstr(page.add(PATH_OFFSET), guest_path);
        write_cstr(page.add(ARGV0_OFFSET), &argv[0]);
        // argv as the guest should see it: argv[0] is the program's own (guest) name. Only the
        // syscall's path argument is translated, not what the program is told it was called.
        // Every LATER entry is copied into the region after the header (2026-11-09, p7 argv). The
        // sizes were validated in `run_native` before this fork, so these writes cannot run off the
        // page; the terminating null goes into the slot after the last entry.
        let argv_slots = page.add(ARGV_OFFSET) as *mut *const nix::libc::c_char;
        *argv_slots = page.add(ARGV0_OFFSET) as *const nix::libc::c_char;
        let mut arg_cursor = page.add(ARGV_EXTRA_OFFSET);
        for (slot, arg) in argv.iter().skip(1).enumerate() {
            write_cstr(arg_cursor, arg);
            *argv_slots.add(slot + 1) = arg_cursor as *const nix::libc::c_char;
            arg_cursor = arg_cursor.add(arg.len() + 1);
        }
        *argv_slots.add(argv.len()) = std::ptr::null();
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

/// The tracer half. Drives EVERY tracee - the engine's own child and every process it forks - for
/// as long as at least one tracee is alive, translating each `execve` it sees (through the shared
/// page for the engine's own child, in the tracee's memory for a fork descendant).
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

    // One slot per tracee: the engine's own child and every fork descendant now traced. The
    // engine's child starts at a syscall ENTRY: the trace resumes from a PTRACE_INTERRUPT
    // group-stop where the child is parked in its opening `read()`, which the kernel re-reports as
    // a fresh ENTRY (measured 2026-11-08, p4 step 3b: `observed` begins with the child's own
    // close/read/close, then its raw `getpid` marker, then its `execve`). A fork descendant starts
    // at a syscall EXIT, because the kernel reports the new child's first syscall stop as the exit
    // of the `clone`/`fork` it was born in. `started` is false until a descendant's birth stop has
    // been consumed.
    struct TraceeSlot {
        pid: Pid,
        at_entry: bool,
        execve_handled: bool,
        started: bool,
    }
    let mut slots: Vec<TraceeSlot> = vec![TraceeSlot {
        pid: child,
        at_entry: true,
        execve_handled: false,
        started: true,
    }];
    let mut original_exit: Option<i32> = None;

    // Resume the engine's own child once; from here the loop is wait-then-resume for whichever
    // tracee stopped, so a fork descendant is driven exactly like the original child.
    ptrace::syscall(child, None)?;

    loop {
        if report.stops > STOP_BUDGET {
            for s in &slots {
                let _ = ptrace::kill(s.pid);
            }
            return Err(anyhow!(
                "stopped tracing after {} syscall stops without every tracee exiting",
                STOP_BUDGET
            ));
        }
        // `__WNOTHREAD`: wait only for children of THIS thread, so the engine never reaps a child
        // that another thread of the same process forked (cargo runs these tests concurrently in one
        // process, each forking its own tracees - measured 2026-11-08: a plain `waitpid(-1)`
        // cross-reaped them and every run failed with ESRCH). `__WALL` is the ptrace requirement
        // that stopped `clone` children are waited for too.
        let status = match waitpid(
            Pid::from_raw(-1),
            Some(WaitPidFlag::__WALL | WaitPidFlag::__WNOTHREAD),
        ) {
            Ok(s) => s,
            // ECHILD: every tracee has been reaped, so the run is over.
            Err(nix::Error::ECHILD) => break,
            Err(e) => {
                for s in &slots {
                    let _ = ptrace::kill(s.pid);
                }
                return Err(anyhow!(
                    "waitpid failed while tracing the native run: {}",
                    e
                ));
            }
        };
        let Some(pid) = status.pid() else { continue };

        // A pid with no slot is USUALLY a fork descendant whose event the engine has not recorded
        // yet: the parent's FORK event and the child's birth stop become ready together and either
        // may be returned first. Record it unstarted so its first stop is consumed as the birth
        // stop. But a pid that has ALREADY EXITED is a corpse left by an earlier run of this engine
        // that killed its tracee without reaping it (a killed, stopped tracee stays a zombie of this
        // thread) - ignore it, because resuming a dead pid is ESRCH (measured 2026-11-08).
        let idx = match slots.iter().position(|s| s.pid == pid) {
            Some(i) => i,
            None => match status {
                WaitStatus::Exited(..) | WaitStatus::Signaled(..) => continue,
                _ => {
                    slots.push(TraceeSlot {
                        pid,
                        at_entry: false,
                        execve_handled: false,
                        started: false,
                    });
                    slots.len() - 1
                }
            },
        };

        // Birth stop of a fork descendant: the kernel stops a freshly attached tracee, and leaving
        // it stopped hangs the workload. Consume it and resume with PTRACE_SYSCALL, exactly as the
        // engine consumed its own child's group-stop before this slice.
        if !slots[idx].started {
            slots[idx].started = true;
            ptrace::syscall(pid, None)?;
            continue;
        }

        match status {
            WaitStatus::PtraceSyscall(_) => {
                report.stops += 1;
                let mut tracee = Tracee::new(pid, Arch::from_host());
                tracee.observe_syscall()?;
                let (number, arg0, args) = match tracee.syscall_info.as_ref() {
                    Some(info) => (info.number, info.args[0], info.args),
                    None => {
                        let _ = ptrace::kill(pid);
                        return Err(anyhow!(
                            "observe_syscall reported success but left no syscall_info"
                        ));
                    }
                };
                let is_execve = number == nix::libc::SYS_execve as i64;

                // IDENTITY MAPPING, exit side (2026-11-09, p7 gap 3). podnext tells the operator,
                // for BOTH engines, "the guest runs as root (uid 0): PRoot maps every guest id to the
                // single host uid via `--change-id=0:0:0`, so there is no privilege separation"
                // (podnext/src/commands/container.rs:247-271) and the runtime really does hard-code
                // that flag for proot. The native path printed the same sentence while passing the
                // HOST ids straight through - measured uid=0/euid=0 under proot against
                // uid=10396/euid=10396 under native - i.e. it claimed a mapping it never performed.
                //
                // Rewriting the RESULT of the id-reporting syscalls makes the claim true: the guest
                // sees 0, exactly as it does under proot. The rewrite happens at the EXIT stop, after
                // the kernel has produced the host value, so no syscall is skipped and no privilege is
                // granted - this is a report of identity and nothing else. Named gaps, not silent ones:
                // getresuid/getresgid/getgroups write through a POINTER into guest memory and are not
                // covered here, and `st_uid`/`st_gid` inside stat() results are not rewritten either.
                if !slots[idx].at_entry && id_reporting(number) {
                    if let Err(e) = override_id_result(pid) {
                        let _ = ptrace::kill(pid);
                        return Err(e);
                    }
                }

                if slots[idx].at_entry {
                    report.entries += 1;
                    // The child writes its self-report after its opening `read()` returns, i.e.
                    // only after the engine releases it, so the header is read on the first entry
                    // stop at which it exists (2026-11-08, p4 step 3b). Reading it before the
                    // release would see unwritten memory.
                    if report.child_view.is_none() {
                        report.child_view = read_child_view(page);
                    }
                    if report.observed.len() < OBSERVED_CAP {
                        report.observed.push((number, arg0));
                    }
                }

                if is_execve && !slots[idx].execve_handled {
                    if path_pointer_is_ours(&report, arg0) {
                        // The engine's OWN child: the path lives in the shared page, so the
                        // rewrite is a plain memory write through `translate_execve`. Only
                        // meaningful at an entry stop, where the published pointer appears as arg0.
                        if slots[idx].at_entry {
                            slots[idx].execve_handled = true;
                            if let Err(e) = translate_execve(page, plan, &mut report) {
                                let _ = ptrace::kill(pid);
                                return Err(e);
                            }
                        }
                    } else {
                        // A path the engine did NOT publish: a fork descendant's own `execve`. The
                        // string lives in the tracee's memory, so it is read and rewritten there
                        // (PTRACE_PEEKDATA / POKEDATA + SETREGSET). A non-entry stop, where arg0 is
                        // a return value instead of a pointer, is not readable and is `false`.
                        match translate_execve_remote(pid, arg0, plan, &mut report) {
                            Ok(true) => slots[idx].execve_handled = true,
                            Ok(false) => {
                                if slots[idx].at_entry {
                                    report.refusals.push(Refusal {
                                        number,
                                        name: syscall_name(number),
                                        reason: "execve's path could not be read from the tracee, so it was not translated",
                                    });
                                }
                            }
                            Err(e) => {
                                let _ = ptrace::kill(pid);
                                return Err(e);
                            }
                        }
                    }
                } else if slots[idx].at_entry {
                    // ENTRY-TIME PATH TRANSLATION (p7). A syscall that takes a guest path is
                    // translated here, at the stop BEFORE the kernel reads the argument, so the
                    // dynamic loader's own opens land inside the rootfs. `Ok(false)` is a path the
                    // engine could not translate (unreadable, empty, relative, or no rootfs plan);
                    // it is recorded as a named refusal rather than left to the host silently.
                    if let Some(PathSyscall {
                        name,
                        arg_index,
                        dirfd_arg,
                    }) = path_syscall(number)
                    {
                        match translate_path_remote(
                            pid,
                            arg_index,
                            dirfd_arg,
                            name,
                            number,
                            &args,
                            plan,
                            &mut report,
                        ) {
                            Ok(true) => {}
                            Ok(false) => {
                                let refusal = Refusal {
                                    number,
                                    name: Some(name),
                                    reason:
                                        "its path could not be translated (unreadable, empty or \
                                             relative); the HOST path is not used as a fallback",
                                };
                                if strict {
                                    report.strict_failure = Some(refusal.label());
                                    report.refusals.push(refusal);
                                    // The program was NOT run as the guest asked: leave nothing
                                    // that could be mistaken for a partial success.
                                    report.translations.clear();
                                    for s in &slots {
                                        let _ = ptrace::kill(s.pid);
                                    }
                                    break;
                                }
                                report.refusals.push(refusal);
                            }
                            Err(e) => {
                                let _ = ptrace::kill(pid);
                                return Err(e);
                            }
                        }
                    } else if number == nix::libc::SYS_chdir as i64
                        && pid == child
                        && !slots[idx].execve_handled
                    {
                        // The engine's OWN pre-exec `chdir` into `RunPlan::cwd` (2026-11-09, p7 cwd).
                        // The path the child was handed is ALREADY the translated host path
                        // (`resolve_cwd`), so the kernel resolves it correctly with no rewrite and
                        // there is nothing for the engine to translate. It is engine setup on a host
                        // path, NOT a guest path, so it is exempt from the named path-refusal rule -
                        // recording it would fail every `--engine native` run on its own bootstrap. A
                        // guest `chdir` AFTER the exec (`execve_handled`) is unaffected: it is still
                        // the unhandled path syscall it always was, and still gets its named refusal.
                    } else {
                        // Not a first execve and not a path this engine translates: a syscall the
                        // engine observed but does not handle.
                        let refusal = if is_execve {
                            // A second execve is how a script's interpreter is started.
                            // Translating it here would be guesswork about which path is a guest
                            // path, so it is recorded instead.
                            Refusal {
                                number,
                                name: syscall_name(number),
                                reason: "a later execve (an interpreter for a script) is not \
                                         translated",
                            }
                        } else if unhandled_path_syscall(number).is_some() {
                            // It takes a path and has no translator: say so by name and say that
                            // the kernel will resolve it on the HOST. This is the distinction the
                            // module exists to keep.
                            Refusal {
                                number,
                                name: syscall_name(number),
                                reason: "takes a path but this engine has no translation for it; \
                                         the kernel resolves it on the HOST filesystem",
                            }
                        } else {
                            Refusal {
                                number,
                                name: syscall_name(number),
                                reason: "passed through, not emulated",
                            }
                        };
                        if strict && !is_execve {
                            report.strict_failure = Some(refusal.label());
                            report.refusals.push(refusal);
                            // The program was NOT run: no translation happened, so leave nothing
                            // that could be mistaken for a partial success.
                            report.translations.clear();
                            for s in &slots {
                                let _ = ptrace::kill(s.pid);
                            }
                            break;
                        }
                        report.refusals.push(refusal);
                    }
                }
                // `execve` succeeds without an exit stop (the image is replaced), so the parity
                // between entry and exit stops is re-established by the exec event below.
                slots[idx].at_entry = !slots[idx].at_entry;
                ptrace::syscall(pid, None)?;
            }
            WaitStatus::PtraceEvent(_, _, event)
                if event == nix::libc::PTRACE_EVENT_FORK
                    || event == nix::libc::PTRACE_EVENT_VFORK
                    || event == nix::libc::PTRACE_EVENT_CLONE =>
            {
                // A new tracee was created: read its pid and record it unstarted. Its birth stop
                // arrives as its own waitpid event and is consumed there.
                let new_pid = ptrace_event_msg(pid)?;
                if new_pid.as_raw() > 0 && !slots.iter().any(|s| s.pid == new_pid) {
                    slots.push(TraceeSlot {
                        pid: new_pid,
                        at_entry: false,
                        execve_handled: false,
                        started: false,
                    });
                }
                ptrace::syscall(pid, None)?;
            }
            WaitStatus::PtraceEvent(_, _, event) if event == nix::libc::PTRACE_EVENT_EXEC => {
                // Direct evidence of WHICH binary ran, rather than an inference from an exit code.
                // For a fork descendant this names the bind/rootfs binary the grandchild executed.
                report.executed = std::fs::read_link(format!("/proc/{}/exe", pid.as_raw())).ok();
                // FALSE, not true: the next `PTRACE_SYSCALL` stop is the execve's own syscall-EXIT
                // (the syscall has already succeeded and the image is replaced), so the stop AFTER
                // that is the first ENTRY of the new program. Measured 2026-11-08, p4 step 3b.
                slots[idx].at_entry = false;
                ptrace::syscall(pid, None)?;
            }
            WaitStatus::Exited(_, code) => {
                // The engine's own child's status is the run's exit code; a descendant's status is
                // reaped by whoever forked it and is not this report's verdict.
                if pid == child {
                    original_exit = Some(code);
                }
                slots.retain(|s| s.pid != pid);
            }
            WaitStatus::Signaled(_, sig, _) => {
                if pid == child {
                    original_exit = Some(128 + sig as i32);
                }
                slots.retain(|s| s.pid != pid);
            }
            // A signal-delivery-stop (including a SIGSTOP group-stop) is suppressed, exactly as the
            // module resumed its single tracee before this slice: the signal is not delivered.
            _ => {
                ptrace::syscall(pid, None)?;
            }
        }

        // The run is over only when EVERY tracee has exited.
        if slots.is_empty() {
            break;
        }
    }

    report.exit_code = original_exit.unwrap_or(-1);
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

/// `NT_PRSTATUS` is 1 in the Linux ptrace ABI (`include/uapi/linux/elf.h`); named here because
/// `libc` does not export it for every target, exactly as `tracee.rs` names it.
const NT_PRSTATUS: nix::libc::c_uint = 1;

/// Read the tracee's register file through `PTRACE_GETREGSET`/`NT_PRSTATUS`. The struct is
/// per-architecture (`tracee.rs` reads the same one); only the two supported targets are used.
/// Syscalls whose entire result IS the caller's identity (p7 gap 3, 2026-11-09). Only these four can
/// be mapped by rewriting the return register; the `getres*`/`getgroups` family writes through a guest
/// pointer and is deliberately left out rather than silently half-done.
fn id_reporting(number: i64) -> bool {
    number == nix::libc::SYS_getuid as i64
        || number == nix::libc::SYS_geteuid as i64
        || number == nix::libc::SYS_getgid as i64
        || number == nix::libc::SYS_getegid as i64
}

/// Make an id-reporting syscall answer 0, the fake root proot presents via `--change-id=0:0:0`.
/// `PDN_NATIVE_NO_ID_MAP=1` disables it; that is the negative control the tests use to prove the
/// rewrite - and not something else - is what changes the guest's answer.
fn override_id_result(pid: Pid) -> Result<()> {
    if std::env::var_os("PDN_NATIVE_NO_ID_MAP").is_some() {
        return Ok(());
    }
    let mut regs = get_regs(pid)?;
    set_id_result(&mut regs);
    set_regs(pid, regs)
}

/// The result register: x0 on aarch64, rax on x86_64 (the two hosts this crate is built for).
#[cfg(target_arch = "aarch64")]
fn set_id_result(regs: &mut nix::libc::user_regs_struct) {
    regs.regs[0] = 0;
}

#[cfg(target_arch = "x86_64")]
fn set_id_result(regs: &mut nix::libc::user_regs_struct) {
    regs.rax = 0;
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn set_id_result(_regs: &mut nix::libc::user_regs_struct) {}

fn get_regs(pid: Pid) -> Result<nix::libc::user_regs_struct> {
    let mut raw: nix::libc::user_regs_struct = unsafe { std::mem::zeroed() };
    let mut iov = nix::libc::iovec {
        iov_base: &mut raw as *mut _ as *mut nix::libc::c_void,
        iov_len: std::mem::size_of::<nix::libc::user_regs_struct>(),
    };
    // `as _`: the request parameter is `c_int` in Android's libc and `c_uint` in the libc the CI
    // runner links against (measured on run 42449343311 - `as c_int` broke the runner build).
    let ret = unsafe {
        nix::libc::ptrace(
            nix::libc::PTRACE_GETREGSET as _,
            pid.as_raw(),
            NT_PRSTATUS as *mut nix::libc::c_void,
            &mut iov as *mut nix::libc::iovec as *mut nix::libc::c_void,
        )
    };
    if ret < 0 {
        return Err(anyhow!(
            "PTRACE_GETREGSET (NT_PRSTATUS) failed for pid {}: {}",
            pid,
            std::io::Error::last_os_error()
        ));
    }
    Ok(raw)
}

/// Write a tracee's register file back through `PTRACE_SETREGSET`/`NT_PRSTATUS`.
fn set_regs(pid: Pid, mut raw: nix::libc::user_regs_struct) -> Result<()> {
    let mut iov = nix::libc::iovec {
        iov_base: &mut raw as *mut _ as *mut nix::libc::c_void,
        iov_len: std::mem::size_of::<nix::libc::user_regs_struct>(),
    };
    let ret = unsafe {
        nix::libc::ptrace(
            nix::libc::PTRACE_SETREGSET as _,
            pid.as_raw(),
            NT_PRSTATUS as *mut nix::libc::c_void,
            &mut iov as *mut nix::libc::iovec as *mut nix::libc::c_void,
        )
    };
    if ret < 0 {
        return Err(anyhow!(
            "PTRACE_SETREGSET (NT_PRSTATUS) failed for pid {}: {}",
            pid,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Read a NUL-terminated string out of a STOPPED tracee's memory, word by word through
/// `PTRACE_PEEKDATA`, bounded by `PATH_CAPACITY`.
///
/// `Ok(None)` means the FIRST word was not readable - which is what an `execve` syscall-EXIT stop
/// looks like (`arg0` is a return value, not a pointer), so it is the signal that this stop is not
/// a translatable `execve` entry. A word that fails mid-string is the same answer.
fn peek_cstr(pid: Pid, addr: u64) -> Result<Option<String>> {
    let mut out: Vec<u8> = Vec::new();
    let mut at = addr;
    while out.len() < PATH_CAPACITY {
        // errno must be cleared before the call: a valid word can read as -1, and ptrace signals
        // failure through errno, so the return value alone cannot be trusted (std reads errno, which
        // exists on every target, unlike `libc::__errno` - see the `child_main` note).
        nix::errno::Errno::clear();
        let word = unsafe {
            nix::libc::ptrace(
                nix::libc::PTRACE_PEEKDATA as _,
                pid.as_raw(),
                at as *mut nix::libc::c_void,
                std::ptr::null_mut::<nix::libc::c_void>(),
            )
        };
        if std::io::Error::last_os_error().raw_os_error().unwrap_or(0) != 0 {
            return Ok(None);
        }
        for b in word.to_ne_bytes() {
            if b == 0 {
                return Ok(Some(String::from_utf8_lossy(&out).into_owned()));
            }
            out.push(b);
            if out.len() >= PATH_CAPACITY {
                break;
            }
        }
        at = at.wrapping_add(8);
    }
    Err(anyhow!(
        "the tracee's path is not NUL-terminated within {} bytes",
        PATH_CAPACITY
    ))
}

/// Write `bytes` and a NUL terminator into a STOPPED tracee at `addr`, word by word through
/// `PTRACE_POKEDATA`. The final word is zero-padded so the terminator is part of what the kernel
/// will read.
fn poke_cstr(pid: Pid, addr: u64, bytes: &[u8]) -> Result<()> {
    let mut buf = bytes.to_vec();
    buf.push(0);
    let mut i = 0usize;
    while i < buf.len() {
        let mut chunk = [0u8; 8];
        let n = std::cmp::min(8, buf.len() - i);
        chunk[..n].copy_from_slice(&buf[i..i + n]);
        let word = i64::from_ne_bytes(chunk);
        let rc = unsafe {
            nix::libc::ptrace(
                nix::libc::PTRACE_POKEDATA as _,
                pid.as_raw(),
                addr.wrapping_add(i as u64) as *mut nix::libc::c_void,
                word as *mut nix::libc::c_void,
            )
        };
        if rc < 0 {
            return Err(anyhow!(
                "PTRACE_POKEDATA failed writing the translated path into pid {}: {}",
                pid,
                std::io::Error::last_os_error()
            ));
        }
        i += 8;
    }
    Ok(())
}

/// Write the translated path into the tracee and make argument `arg_index` of the stopped syscall
/// point at it.
///
/// The scratch lives `0x200` bytes below the tracee's stack pointer: the translated path is at most
/// `PATH_CAPACITY + 1` (= 512) bytes, so it fits in `[sp - 0x200, sp)`, a region the tracee is not
/// using while it is stopped in a syscall. The pointer is published through `PTRACE_SETREGSET` so
/// the kernel reads the host path instead of the guest one.
///
/// `arg_index` is the syscall argument number (0-based), NOT a register number: on aarch64 the
/// argument registers are `x0..x5` (`regs[0..6]`), and on x86_64 the kernel takes them from
/// `rdi, rsi, rdx, r10, r8, r9` - so the mapping is named here, once, rather than at each call site.
/// `execve` uses argument 0; `openat`-style syscalls carry the path in argument 1.
///
/// CEILING: the register file differs per architecture, so only the two supported targets are
/// named; any other target needs its own arm.
fn write_path_and_redirect_arg(pid: Pid, arg_index: usize, bytes: &[u8]) -> Result<()> {
    const SCRATCH_BELOW_SP: u64 = 0x200;
    let mut raw = get_regs(pid)?;
    #[cfg(target_arch = "aarch64")]
    {
        let scratch = raw.sp.wrapping_sub(SCRATCH_BELOW_SP) & !0xf;
        let Some(slot) = raw.regs.get_mut(arg_index) else {
            return Err(anyhow!(
                "syscall argument {} has no register on aarch64 (only 0..=5 do)",
                arg_index
            ));
        };
        poke_cstr(pid, scratch, bytes)?;
        *slot = scratch;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let scratch = raw.rsp.wrapping_sub(SCRATCH_BELOW_SP) & !0xf;
        let slot = match arg_index {
            0 => &mut raw.rdi,
            1 => &mut raw.rsi,
            2 => &mut raw.rdx,
            3 => &mut raw.r10,
            4 => &mut raw.r8,
            5 => &mut raw.r9,
            _ => {
                return Err(anyhow!(
                    "syscall argument {} has no register on x86_64 (only 0..=5 do)",
                    arg_index
                ))
            }
        };
        poke_cstr(pid, scratch, bytes)?;
        *slot = scratch;
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (arg_index, bytes);
        return Err(anyhow!(
            "syscall argument rewriting is not implemented for this target architecture"
        ));
    }
    set_regs(pid, raw)?;
    Ok(())
}

/// Translate ONE path argument of a syscall whose path the engine did not publish (every tracee,
/// including the engine's own child after its exec).
///
/// Reads the guest path at `args[arg_index]` with `peek_cstr`, resolves it through the SAME
/// `resolve` `execve` uses (binds first, then the rootfs), and points the argument at a copy of the
/// host path in the tracee's own memory. Returns `Ok(false)` when there is nothing to rewrite -
/// no rootfs plan, an unreadable/NULL pointer, an empty path (the `AT_EMPTY_PATH` case, where the
/// kernel acts on `dirfd`), or a relative path (which depends on `dirfd`/cwd, neither modelled).
///
/// The translated host path is written even when it does not exist: the kernel must answer `ENOENT`
/// for a guest path the rootfs does not carry, and leaving the guest path in place would let the
/// HOST file answer instead - the false positive this module exists to prevent.
fn translate_path_remote(
    pid: Pid,
    arg_index: usize,
    dirfd_arg: Option<usize>,
    name: &'static str,
    number: i64,
    args: &[u64; 6],
    plan: &RunPlan,
    report: &mut NativeRunReport,
) -> Result<bool> {
    let Some(rootfs) = plan.rootfs.as_ref() else {
        // No plan: the guest path is used as-is by the kernel. Not an error - it is the control arm
        // showing that the translation is what makes a rootfs-relative run work.
        return Ok(false);
    };
    let Some(arg) = args.get(arg_index) else {
        return Ok(false);
    };
    let Some(raw) = peek_cstr(pid, *arg)? else {
        return Ok(false);
    };
    if raw.is_empty() {
        return Ok(false);
    }
    // A RELATIVE guest path (2026-11-09, p7 relative). Once `RunPlan::cwd` is applied the guest's
    // starting directory is INSIDE the rootfs, so a relative name resolved there cannot reach the host
    // and the engine CAN translate it: join it onto the known guest-absolute cwd and go through the same
    // `resolve`. Until the cwd existed this case was a genuine host-fallback risk and refusing was the
    // right answer; afterwards the refusal failed runs that were correct (measured: a relative open
    // wrote into the rootfs and the run still exited 1).
    //
    // Two conditions keep the join honest. The syscall must resolve the name against the cwd - `AT_FDCWD`
    // (`arg0 == -100`) or a legacy no-dirfd syscall - because for any other fd the kernel resolves it
    // against THAT directory and joining onto the cwd would silently open a different file. And the name
    // must carry no `.`/`..` component: collapsing those is the lexical work the cwd guard already does,
    // and guessing it here would be a new guess rather than a translation.
    let guest = if raw.starts_with('/') {
        raw
    } else {
        let against_cwd = match dirfd_arg {
            Some(i) => args
                .get(i)
                .map(|v| *v as i32 == nix::libc::AT_FDCWD)
                .unwrap_or(false),
            None => true,
        };
        let needs_normalising = raw.split('/').any(|c| c == "." || c == "..");
        match plan.cwd.as_deref() {
            Some(cwd) if against_cwd && !needs_normalising => {
                format!("{}/{}", cwd.trim_end_matches('/'), raw)
            }
            _ => return Ok(false),
        }
    };
    let host = resolve(rootfs, &guest, &plan.binds);
    if host.as_path() == Path::new(&guest) {
        // The rewrite would not change anything: either the guest path is already the host path
        // (a bind mounted on itself, or the "already translated" case), so it is HANDLED and needs
        // no memory write. Recording it as a translation would put a line in the report for every
        // path the loader touched, which is noise rather than evidence.
        return Ok(true);
    }
    let bytes = host.as_os_str().as_encoded_bytes();
    if bytes.len() > PATH_CAPACITY {
        return Err(anyhow!(
            "translated path of {} bytes does not fit the {} byte translation area: {} \
             (syscall {}, number {})",
            bytes.len(),
            PATH_CAPACITY,
            host.display(),
            name,
            number
        ));
    }
    write_path_and_redirect_arg(pid, arg_index, bytes)?;
    report.translations.push(Translation {
        guest,
        host,
        syscall: name,
    });
    Ok(true)
}

/// Rewrite the `execve` path argument of a tracee whose path the engine did NOT publish (a fork
/// descendant). Reads the guest path from the tracee at `arg0`, resolves it through the SAME
/// `resolve` the shared-page translation uses (binds first, then the rootfs), and points `arg0` at
/// a copy of the host path written into the tracee's own memory.
///
/// Returns `Ok(false)` when `arg0` does not point at a readable string (a non-entry stop) or when
/// there is no plan, so the caller can tell "nothing to do here" from a real refusal.
fn translate_execve_remote(
    pid: Pid,
    arg0: u64,
    plan: &RunPlan,
    report: &mut NativeRunReport,
) -> Result<bool> {
    let Some(rootfs) = plan.rootfs.as_ref() else {
        // No plan: the guest path is used as-is by the kernel. Not an error - it is the control arm
        // showing that the translation is what makes a rootfs-relative run work.
        return Ok(false);
    };
    let Some(guest) = peek_cstr(pid, arg0)? else {
        return Ok(false);
    };
    if guest.is_empty() {
        return Ok(false);
    }
    let host = resolve(rootfs, &guest, &plan.binds);
    if !host.exists() {
        // The honest failure, identical to the shared-page translation: no fallback to the host
        // filesystem, because a silent fallback is exactly how a container would appear to work
        // while running a host binary.
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
    write_path_and_redirect_arg(pid, 0, bytes)?;
    report.translations.push(Translation {
        guest,
        host,
        syscall: "SYS_execve",
    });
    Ok(true)
}

/// Read the new pid the kernel reported at a `PTRACE_EVENT_FORK`/`VFORK`/`CLONE` stop, through
/// `PTRACE_GETEVENTMSG`. `nix` gates its own wrapper to `target_os = "linux"` where Android differs,
/// and libc declares the request as `c_int` on Android and `c_uint` on linux-gnu, so the request is
/// cast `as _` (the same asymmetry `tracee.rs` documents for `PTRACE_GETREGSET`).
fn ptrace_event_msg(pid: Pid) -> Result<Pid> {
    let mut msg: nix::libc::c_ulong = 0;
    let rc = unsafe {
        nix::libc::ptrace(
            nix::libc::PTRACE_GETEVENTMSG as _,
            pid.as_raw(),
            std::ptr::null_mut::<nix::libc::c_void>(),
            &mut msg as *mut nix::libc::c_ulong as *mut nix::libc::c_void,
        )
    };
    if rc < 0 {
        return Err(anyhow!(
            "PTRACE_GETEVENTMSG failed for pid {}: {}",
            pid,
            std::io::Error::last_os_error()
        ));
    }
    Ok(Pid::from_raw(msg as nix::libc::pid_t))
}

/// Translate a `RunPlan::cwd` guest path to the host path the child should `chdir` into, or refuse
/// it BY NAME (2026-11-09, p7 cwd).
///
/// Two refusals, both named rather than guessed:
///
///   * a cwd that is not absolute - the proot path always supplies the absolute default `/`, and a
///     relative guest path depends on a `dirfd`/cwd the engine does not model;
///   * a cwd containing a `..` component - `resolve` would join it under the rootfs and the KERNEL
///     would then collapse the `..` to a path OUTSIDE the rootfs and its binds, i.e. the guest
///     escaping its own container by naming its starting directory.
///
/// With a `rootfs` the path goes through the SAME `resolve` the execve rewrite uses, so a cwd inside
/// a bind lands on the bind's host path too. Without a `rootfs` the plan names a host program, so
/// the absolute cwd is taken as the host path it already is.
fn resolve_cwd(plan: &RunPlan, guest: &str) -> Result<PathBuf> {
    if !guest.starts_with('/') {
        return Err(anyhow!(
            "RunPlan::cwd must be an absolute guest path (got {:?}); the engine refuses a relative \
             cwd rather than guessing one against a `dirfd` it does not model",
            guest
        ));
    }
    if Path::new(guest)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(anyhow!(
            "RunPlan::cwd {:?} contains a `..` component and would resolve outside the rootfs; the \
             engine refuses it by name rather than let the kernel escape the container",
            guest
        ));
    }
    Ok(match plan.rootfs.as_ref() {
        Some(rootfs) => resolve(rootfs, guest, &plan.binds),
        None => PathBuf::from(guest),
    })
}

/// Guest path -> host path. A bind whose guest prefix matches wins over the rootfs; when several
/// binds match, the LONGEST prefix wins (so a more specific mount beats a broader one). A guest path
/// outside every bind resolves under the rootfs exactly as before, and a relative guest path is
/// joined under the rootfs too, so an exec cannot escape it.
///
/// Two rules that a raw string prefix test gets wrong, both measured 2026-11-08 (p7, first run of
/// `native_dynamic_guest`):
///
///   * A path that is ALREADY on the host side is returned unchanged. The kernel and the loader
///     hand host paths back in places - `/proc/self/exe` is the measured one, which the loader then
///     `stat`s - and translating those a second time produced `<rootfs><rootfs>/...`, a path that
///     exists nowhere. This is proot's "already translated" case.
///   * Prefixes are matched by PATH COMPONENT, not by string. The run bound `/sys` and the loader
///     opened `/system/lib64/libnetd_client.so`, which a string `starts_with("/sys")` accepts -
///     producing the silently wrong `/sys/tem/lib64/libnetd_client.so`. `Path::starts_with` compares
///     whole components, so `/system` no longer matches a bind on `/sys`.
pub fn resolve(rootfs: &Path, guest: &str, binds: &[(String, PathBuf)]) -> PathBuf {
    let guest_path = Path::new(guest);
    if guest_path.starts_with(rootfs) {
        return guest_path.to_path_buf();
    }
    let mut chosen: Option<&(String, PathBuf)> = None;
    for b in binds {
        if guest_path.starts_with(Path::new(&b.0)) && chosen.map_or(true, |c| b.0.len() > c.0.len())
        {
            chosen = Some(b);
        }
    }
    match chosen {
        Some((from, to)) => {
            let rest = guest.strip_prefix(from.as_str()).unwrap_or(guest);
            to.join(rest.trim_start_matches('/'))
        }
        None => rootfs.join(guest.trim_start_matches('/')),
    }
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
