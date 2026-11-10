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
//! A GUEST WHOSE PT_INTERP DOES NOT EXIST ON THE HOST (2026-11-09, p7 interp). MEASURED on a real
//! Debian rootfs (the `deb-pdn` container, whose `/usr/bin/dash` names `PT_INTERP
//! /lib/ld-linux-aarch64.so.1`): `podnext --engine native run <ctr> -- /bin/sh -c 'echo hi'` exited
//! 127 with NO guest output, while the same command under proot printed `hi` and exited 0. The cause
//! is structural and was proven without any engine: the KERNEL opens `PT_INTERP` on the HOST before
//! the tracee executes one instruction, and this Android host has no `/lib` at all
//! (`ls /lib/ld-linux-aarch64.so.1` -> `No such file or directory`; running the guest binary directly
//! on the host gives the same `No such file or directory`, exit 127). Entry-time path translation
//! cannot reach it, because it happens before the first tracee syscall.
//!
//! The remedy is to exec the guest's OWN interpreter with the guest program as its argument - the
//! `ld.so PROGRAM [ARGS...]` form. `guest_interpreter` reads `PT_INTERP` out of the guest ELF
//! (`elf_interp`) and substitutes ONLY when the path is really absent on the host AND present inside
//! the rootfs; the engine then publishes the interpreter's guest path as the exec path and lists the
//! execve TARGET's guest path as the loader's PROGRAM argument, so the loader opens it through the
//! SAME traced `openat` the engine already translates. The interpreter being exec'd IS recorded
//! (`NativeRunReport::interpreter`), so a reader can tell a substituted run from a normal one.
//!
//! THE LOADER'S PROGRAM IS THE EXECVE TARGET, NOT THE CALLER'S argv[0] (2026-11-09, p7
//! interp-program). The first version built the loader's argv as `[<interp>, argv[0], args...]`,
//! which is correct only when argv[0] happens to be the absolute guest path - true for the engine's
//! own child, whose argv[0] IS the path `run_native` was given, but FALSE for a shell-spawned
//! external: `dash` execs `/usr/bin/ls` with argv[0] = `ls`, so the loader was told to load a program
//! named `ls` and reported `ls: cannot open shared object file` (measured; proot ran it, exit 0).
//! The loader now gets the target guest path, and the caller's argv[0] is preserved with `--argv0
//! STRING` where the interpreter supports it: MEASURED, Debian 13's glibc `ld.so --help` prints
//! `--argv0 STRING` and its image carries the token, and Alpine 3.21's musl 1.2.4 image carries
//! `argv0` in its option table too; Android's bionic `/system/bin/linker64` has no such option (no
//! `argv0` token at all), so on that loader the plain form is used. The engine's own child needs no
//! `--argv0` - its argv[0] IS the target path - while a descendant's argv[0] is whatever the caller
//! passed, the case that broke. WHERE THE OPTION IS SUPPORTED the guest sees the argv[0] it was
//! given; WHERE IT IS NOT, the program's argv[0] becomes its guest program path. That divergence is
//! measured, not assumed, by `a_spawned_external_is_given_its_target_path_as_the_loader_program`.
//!
//! A SECOND execve FROM THE SAME PROCESS IS TRANSLATED TOO (2026-11-09, p7 exec-again). A shell's
//! `exec` builtin replaces the process with no fork, so the tracee that is already past its first
//! exec issues another `execve`; that used to be refused by name and left to the kernel, which then
//! resolved the guest path on the HOST. MEASURED on a real Debian rootfs: `-- /bin/sh -c 'exec
//! /bin/ls /etc'` ran the host's `/bin/ls` - Android's toybox, through the `/bin -> /system/bin`
//! symlink - and died inside bionic's loader with `CANNOT LINK EXECUTABLE "/bin/ls": library
//! "libcrypto.so" not found`, while proot exited 0. Every execve is now translated at its entry stop,
//! by the same remote path a descendant uses once the shared page is gone.
//!
//! THE ENGINE ANSWERS `faccessat2` WHEN THE HOST REFUSES IT (2026-11-09, p7 seccomp). MEASURED on
//! this Android device: the app's seccomp filter traps `faccessat2` (SIGSYS, x8 = 439) and the
//! kernel returns WITHOUT writing the result register, so the tracee reads garbage. With the
//! substitution in place `-- /bin/sh -c 'ls /etc'` STILL failed - `ls: Permission denied`, and the
//! trace showed NO `clone` and NO `execve`: dash's PATH lookup uses `eaccess()`, Debian's glibc does
//! not fall back to the older `faccessat`, so the lookup died before any exec and no loader fix
//! could help. `command -v ls` fails the same way while `ls /etc | head` works, because a pipeline
//! skips the check. The syscall NUMBER cannot be rewritten on arm64 (the kernel takes it from
//! `regs->syscallno`, not from `x8`; measured - lowering it changed nothing), so the engine
//! RESOLVES the path through the same `resolve` it already translated, decides the access question
//! itself (`guest_access`), and writes that answer into `x0` at the exit stop ONLY when the host
//! never answered (the register still holding the first argument, or `-ENOSYS`). A host that allows
//! `faccessat2` is left untouched; `set_robust_list` and `rseq` are also trapped but glibc tolerates
//! their failure, so they are not emulated.
//!
//! NOT DONE HERE, named so the gap is explicit rather than implied:
//! the other five argv features; the guest's environment (an empty `envp` is passed on purpose -
//! see `child_main`); and `dirfd`-relative path translation (a RELATIVE guest path is still refused
//! by name rather than guessed - only the STARTING cwd is modelled, not a per-syscall `dirfd`).
//! A fork DESCENDANT's `execve` gets the SAME substitution, written onto the descendant's own stack
//! by `translate_execve_remote`/`write_substitution_remote` (2026-11-09, p7 interp-desc): a real
//! container runs a shell that spawns `/bin/dash`, `ls`, an init script - every one of those is a
//! descendant exec, and without the substitution the kernel opens the rootfs copy's absent
//! `PT_INTERP` on the host and the exec fails (measured: `native` exit 127, `/bin/sh: 1: /bin/dash:
//! not found`, against proot printing `CHILD-RAN` and exiting 0).

use anyhow::{anyhow, Result};
use nix::sys::ptrace::{self, Options};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::io::{Read, Seek, SeekFrom};
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
/// How much room a descendant's interpreter substitution may use below the tracee's stack pointer
/// (2026-11-09, p7 interp-desc). The child path reserves its strings in the page mapped before the
/// fork; a descendant has only its own stack, so this is the window the engine writes into - the
/// same `[sp - N, sp)` region `write_path_and_redirect_arg` uses, widened because a substitution
/// writes TWO strings plus a fresh pointer array. Exceeding it is a NAMED refusal, never a
/// truncated argv.
const DESCENDANT_SCRATCH_BYTES: u64 = 0x1000;
/// Upper bound on a descendant's original argv the engine will carry through a substitution. A
/// longer array is refused BY NAME: the engine rewrites the WHOLE array and will not write a prefix.
const DESCENDANT_ARGV_MAX: usize = 256;
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
    /// The GUEST'S ENVIRONMENT, as `(key, value)` pairs (2026-11-09, p7 envp). Empty means the guest
    /// starts with no environment at all. The engine builds `envp` from this list and an explicit
    /// allow-list of its own (`guest_environment`) instead of forwarding its own `environ`, for two
    /// measured reasons: `nix::libc::environ` is not declared for the Android target, and handing the
    /// guest the engine's whole environment would be a claim about isolation this engine has not
    /// measured. The allow-list mirrors what the proot path sets (`proot-core/src/lib.rs:255-270`):
    /// `PATH`, `HOME=/root`, a forwarded `TERM`, and `LANG`/`LC_ALL`/`COLORTERM` when the caller has
    /// them. Container-configured variables arrive here through this field, exactly as they reach the
    /// proot argv through `config.env`.
    pub env: Vec<(String, String)>,
    /// Mirror the reference's `--link2symlink`: when the kernel refuses a guest hardlink (Android's
    /// SELinux policy does - measured `ln: failed to create hard link 'b' => 'a': Permission denied`),
    /// the engine creates a symlink and reports success, which is what proot does for the same call.
    pub link2symlink: bool,
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
    /// The guest's `PT_INTERP` path when the engine had to EXEC THE INTERPRETER instead of the guest
    /// binary, because that path does not exist on the host (2026-11-09, p7 interp). `None` means the
    /// kernel started the guest directly, i.e. no substitution was made - so a reader can tell a
    /// substituted run from a normal one instead of inferring it from the exit code.
    pub interpreter: Option<String>,
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

/// The two-path calls: `renameat`/`renameat2` and `linkat`. Declared here because the one-path table
/// below cannot express them and the kernel keeps them as separate numbers.
const SYS_RENAMEAT: i64 = nix::libc::SYS_renameat as i64;
const SYS_RENAMEAT2: i64 = nix::libc::SYS_renameat2 as i64;
const SYS_LINKAT: i64 = nix::libc::SYS_linkat as i64;

/// `SYS_newfstatat` is declared by the libc crate for linux-gnu (79 on arm64, 262 on x86_64) and
/// for android/x86_64, but NOT for android/aarch64 - measured 2026-11-08 on libc 0.2.191, whose
/// `android/b64/aarch64/mod.rs` has no such constant. The arm64 table (`asm-generic/unistd.h`)
/// fixes it at 79, the same number the gnu arm64 crate declares, so that one number is named here
/// rather than leaving `stat` untranslatable on the device.
#[cfg(any(not(target_os = "android"), target_arch = "x86_64"))]
const SYS_NEWFSTATAT: i64 = nix::libc::SYS_newfstatat as i64;
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const SYS_NEWFSTATAT: i64 = 79;

/// `fstat` follows the same story as `newfstatat`: the libc crate does not declare it for
/// android/aarch64, and the arm64 table fixes it at 80.
#[cfg(any(not(target_os = "android"), target_arch = "x86_64"))]
const SYS_FSTAT: i64 = nix::libc::SYS_fstat as i64;
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const SYS_FSTAT: i64 = 80;

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
    // WRITE-SIDE PATHS (2026-11-09). The table covered the read side (open/stat/readlink/faccess), so a
    // guest could READ the rootfs but not change it: `mkdir -p /root/x` in the guest was refused with
    // "SYS_mkdirat takes a path but this engine has no translation for it", while the proot arm created
    // the directory. These are the single-path calls the reference translates; each takes its path at
    // arg1 with the dirfd at arg0, the same shape as openat.
    if number == nix::libc::SYS_mkdirat as i64 {
        return Some(PathSyscall {
            name: "SYS_mkdirat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_unlinkat as i64 {
        return Some(PathSyscall {
            name: "SYS_unlinkat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_fchmodat as i64 {
        return Some(PathSyscall {
            name: "SYS_fchmodat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    if number == nix::libc::SYS_utimensat as i64 {
        return Some(PathSyscall {
            name: "SYS_utimensat",
            arg_index: 1,
            dirfd_arg: Some(0),
        });
    }
    // `symlinkat(target, newdirfd, linkpath)`: the LINK path is arg2; the target (arg0) is a string the
    // guest stores, not a path resolved now, so it is left exactly as the guest wrote it.
    if number == nix::libc::SYS_symlinkat as i64 {
        return Some(PathSyscall {
            name: "SYS_symlinkat",
            arg_index: 2,
            dirfd_arg: Some(1),
        });
    }
    // 2026-11-09: THE GUEST COULD NOT `cd`. Measured with a real Debian rootfs:
    //   podnext --engine native run <ctr> -- /bin/sh -c 'cd /tmp; ...'
    // exited 1 with "the native engine did not translate 2 path syscall(s): SYS_chdir (49) [takes a path but
    // has no translator]", while the same command on the proot arm ran - so any guest script that changes
    // directory failed, which is most of them. `chdir` takes its path at arg0 and resolves it against the
    // process's cwd, which is exactly what `dirfd_arg: None` already means here; `fchdir` takes a descriptor
    // and is deliberately absent.
    if number == nix::libc::SYS_chdir as i64 {
        return Some(PathSyscall {
            name: "SYS_chdir",
            arg_index: 0,
            dirfd_arg: None,
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

    // The guest's INTERPRETER, when the kernel cannot start the guest binary directly (2026-11-09,
    // p7 interp). `PT_INTERP` is read from the guest ELF and substituted only when that path is
    // really absent on the host AND present inside the rootfs; `None` means the ordinary `execve`
    // happens and the kernel opens the interpreter itself. Decided HERE, before the fork, so a
    // refusal is an `Err` from `run_native` rather than a child that dies for a reason the caller
    // cannot read.
    let interpreter = guest_interpreter(plan, &guest_path);
    if let Some(interp) = interpreter.as_deref() {
        if interp.len() > PATH_CAPACITY {
            return Err(anyhow!(
                "the guest's PT_INTERP of {} bytes does not fit the {} byte translation area: {}",
                interp.len(),
                PATH_CAPACITY,
                interp
            ));
        }
    }

    // The guest's argv (2026-11-09, p7 argv). Until this increment the engine published argv[0] ONLY
    // and terminated the pointer array after it, so a guest invoked with arguments saw `argc == 1`
    // and fell back to its own defaults - silently, and undocumented (measured: a probe passed
    // `argv[1]` printed its compiled-in default name). Both limits are checked HERE, before the
    // fork, so exceeding them is an `Err` from `run_native` rather than a child that exits 126 for
    // a reason the caller cannot read. A substitution adds ONE slot: the kernel's argv is then
    // `[<interpreter>, argv[0..]]`, the form `ld.so PROGRAM [ARGS...]` runs.
    let argv_slots_needed = argv.len() + 1 + usize::from(interpreter.is_some()); // entries + null (+interp)
    if argv_slots_needed > ARGV_SLOTS {
        return Err(anyhow!(
            "argv of {} entries needs {} slots with the terminating null, and this engine publishes \
             {}: the extra arguments would be dropped rather than run",
            argv.len(),
            argv_slots_needed,
            ARGV_SLOTS
        ));
    }
    // The kernel-level argv[0] under a substitution is the interpreter's GUEST path, published as its
    // own string in this region so the engine's rewrite of the exec PATH cannot drag the host path
    // into it (2026-11-09, p7 argv0). It is CHARGED here, not exempted, so its length is refused by
    // name like every other argument string rather than silently overflowing the region.
    let argv_extra_bytes: usize = argv[1..].iter().map(|a| a.len() + 1).sum::<usize>()
        + interpreter.as_deref().map_or(0, |i| i.len() + 1);
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
    // The guest's environment is decided in the PARENT, before the fork (2026-11-09, p7 envp), so the
    // child does no allocation beyond turning the strings into `CString`s for `execve`.
    let env_strings = guest_environment(&plan.env);

    let (pipe_r, pipe_w) = nix::unistd::pipe()?;

    match unsafe { fork() }? {
        ForkResult::Child => {
            // Close the write end we inherited: only the engine releases the child.
            let _ = nix::unistd::close(pipe_w);
            child_main(
                page as *mut u8,
                &guest_path,
                interpreter.as_deref(),
                argv,
                pipe_r,
                cwd_host.as_deref(),
                &env_strings,
            )
        }
        ForkResult::Parent { child } => {
            // The engine only writes the release byte; the child owns the read end.
            let _ = nix::unistd::close(pipe_r);
            let report = parent_loop(
                child,
                page as *mut u8,
                plan,
                strict,
                pipe_w,
                interpreter.as_deref(),
            );
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
    interpreter: Option<&str>,
    argv: &[String],
    pipe_r: nix::libc::c_int,
    cwd: Option<&Path>,
    env: &[String],
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

        // The exec path published in the shared page. Under an interpreter substitution this is the
        // guest's OWN interpreter (its PT_INTERP), not the guest binary: the kernel cannot open that
        // path on the host, so the engine execs the interpreter and hands the guest as `argv[1]`
        // (2026-11-09, p7 interp). `guest_interpreter` only returns a value when the interpreter is
        // really absent on the host and present in the rootfs, so an ordinary guest is unchanged.
        // The engine REWRITES this region to the translated host path at the execve stop, so nothing
        // else may point at it - in particular argv[0] (below) does not.
        let exec_path = interpreter.unwrap_or(guest_path);
        write_cstr(page.add(PATH_OFFSET), exec_path);
        // argv as the KERNEL receives it. Without a substitution it is the guest's own argv, with
        // argv[0] the program's (guest) name: only the syscall's path argument is translated, not
        // what the program is told it was called. With a substitution the kernel's argv is
        // `[<interpreter>, argv[0..]]` - the interpreter's own argv[0] sits first and the guest
        // program is the interpreter's argv[1] - which is the `ld.so PROGRAM [ARGS...]` form; the
        // loader then makes the program's argv from the arguments after its own name, so the GUEST
        // still sees its own argv[0].
        write_cstr(page.add(ARGV0_OFFSET), &argv[0]);
        let argv_slots = page.add(ARGV_OFFSET) as *mut *const nix::libc::c_char;
        // Every string after argv[0] is copied into the region after the header, the interpreter's
        // own argv[0] string first under a substitution. The sizes were validated in `run_native`
        // before this fork (including the interpreter string), so these writes cannot run off the
        // page; the terminating null goes into the slot after the last entry.
        let mut arg_cursor = page.add(ARGV_EXTRA_OFFSET);
        // The slot the guest's argv[0] goes in: 1 under a substitution (after the interpreter's own
        // argv[0]), 0 otherwise.
        let prog_slot = match interpreter {
            Some(interp) => {
                // The kernel's argv[0] is the INTERPRETER, and the guest reads it through
                // /proc/<pid>/cmdline. It is published as its OWN string holding the interpreter's
                // GUEST path (e.g. /lib/ld-linux-aarch64.so.1), NOT as a pointer into PATH_OFFSET:
                // the engine rewrites PATH_OFFSET to the translated HOST path at the execve stop, so
                // a pointer there would leak the host path into argv[0] - the divergence this cell
                // exists to prevent (2026-11-09, p7 argv0, measured against proot).
                write_cstr(arg_cursor, interp);
                *argv_slots = arg_cursor as *const nix::libc::c_char;
                arg_cursor = arg_cursor.add(interp.len() + 1);
                1
            }
            None => 0,
        };
        *argv_slots.add(prog_slot) = page.add(ARGV0_OFFSET) as *const nix::libc::c_char;
        for (slot, arg) in argv.iter().skip(1).enumerate() {
            write_cstr(arg_cursor, arg);
            *argv_slots.add(prog_slot + 1 + slot) = arg_cursor as *const nix::libc::c_char;
            arg_cursor = arg_cursor.add(arg.len() + 1);
        }
        *argv_slots.add(prog_slot + argv.len()) = std::ptr::null();
        // The self-report the engine believes instead of its own address arithmetic. The length is
        // the one actually written at PATH_OFFSET (the interpreter under a substitution).
        let header = page.add(HEADER_OFFSET) as *mut u64;
        *header = HEADER_MAGIC;
        *header.add(1) = page.add(PATH_OFFSET) as u64;
        *header.add(2) = exec_path.len() as u64;

        // The child's own marker before its `execve`, issued as a RAW syscall on purpose: Android's
        // bionic CACHES `getpid()`, so `libc::getpid` is often not a syscall at all and would leave
        // the engine no stop to anchor the trace on (measured 2026-11-08: the `getpid` marker never
        // appeared as a stop, while a raw `syscall(SYS_getppid)` did). The raw `getpid` is a real
        // syscall whose entry stop is the one immediately before the translated `execve`.
        let _sync_marker = nix::libc::syscall(nix::libc::SYS_getpid);
        std::hint::black_box(_sync_marker);

        // The guest's environment (2026-11-09, p7 envp). It comes from an EXPLICIT list
        // (`guest_environment`) rather than the engine's own `environ`: `nix::libc::environ` is not
        // declared for the Android target anyway (measured 2026-11-08: `cannot find value environ in
        // crate nix::libc`), and forwarding the engine's whole environment would be a claim about
        // isolation this engine has not measured. The strings live on the child's own heap, which is
        // valid until the `execve` below replaces the image, so no page region is needed for them.
        let env_cstrings: Vec<std::ffi::CString> = env
            .iter()
            .map(|s| std::ffi::CString::new(s.as_str()).unwrap_or_default())
            .collect();
        let mut env_ptrs: Vec<*const nix::libc::c_char> = env_cstrings
            .iter()
            .map(|c| c.as_ptr() as *const nix::libc::c_char)
            .collect();
        env_ptrs.push(std::ptr::null());

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
            env_ptrs.as_ptr(),
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
    interpreter: Option<&str>,
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
        interpreter: interpreter.map(str::to_string),
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
        /// The last `linkat` this tracee made: (source host path, destination host path, guest link
        /// text). Filled at the ENTRY stop and consumed at the EXIT stop, because at exit the argument
        /// registers already hold the result (2026-11-09, link2symlink).
        pending_link: Option<(std::path::PathBuf, std::path::PathBuf, String)>,
        /// The guest's CURRENT directory for this tracee as a guest-absolute path, or `None` when the engine
        /// does not know it (2026-11-09).
        /// `plan.cwd` is only the STARTING directory: a guest that runs `cd` moves, and every later
        /// relative path must resolve against where it actually is - proot tracks this per process too.
        /// Measured: with only `plan.cwd`, `cd /etc && head -1 os-release` translated the relative open
        /// against `/` and answered `cannot open 'os-release'`.
        cwd: Option<String>,
        started: bool,
        /// The answer the engine computed for a `faccessat2` whose path it resolved, applied at the
        /// exit stop only if the host refused the call with `-ENOSYS` (see `parent_loop`).
        faccessat_result: Option<i32>,
    }
    let mut slots: Vec<TraceeSlot> = vec![TraceeSlot {
        pid: child,
        at_entry: true,
        execve_handled: false,
        pending_link: None,
        // NO default: `None` means the engine does not know where the guest is, and a relative path must then
        // be refused rather than joined onto a guess. Measured by the p7-relative control arm: defaulting this
        // to "/" made the engine rewrite `relative-report` to `<rootfs>/relative-report` while the kernel
        // resolved it against the HOST cwd, i.e. a translation to the wrong place.
        cwd: plan.cwd.clone(),
        started: true,
        faccessat_result: None,
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
                        pending_link: None,
                        cwd: plan.cwd.clone(),
                        started: false,
                        faccessat_result: None,
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
            // The child has not executed a syscall yet, so the NEXT stop is its first ENTRY. Without
            // this the fork child's first syscall was labelled an EXIT and every entry-only step was
            // skipped for it. MEASURED on a real Debian rootfs (2026-11-10), full stop trace of the
            // pipeline's left child:
            //   EVENT code=128          <- birth stop, consumed by this guard
            //   SYSCALL entry=false     <- its FIRST syscall, labelled exit
            //   ... EXITED 127          <- `/bin/sh: dpkg-query: not found`
            // The guard consumed the birth stop but left `at_entry` false, and that inversion (not a
            // missing stop) is why the bare-name `execve` was never translated before the kernel
            // answered ENOENT.
            slots[idx].at_entry = true;
            if !matches!(status, WaitStatus::PtraceSyscall(_)) {
                ptrace::syscall(pid, None)?;
                continue;
            }
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

                // ANDROID'S SECCOMP FILTER BLOCKS `faccessat2` (2026-11-09, p7 seccomp). MEASURED on
                // this device: the tracee takes a SIGSYS with x8 = 439, so the call returns
                // `-ENOSYS`. Debian's glibc does not fall back to the older `faccessat` for that, so
                // `eaccess()` fails and a shell's PATH lookup dies BEFORE it ever execs: with the
                // substitution in place, `-- /bin/sh -c 'ls /etc'` still printed `ls: Permission
                // denied`, and the trace showed NO `clone` and NO `execve` - the interpreter form was
                // never reached, so no loader fix could help. `command -v ls` (which uses
                // `eaccess`) fails the same way while `ls /etc | head` works, because a pipeline
                // skips the check - that is what isolates `eaccess` as the broken link.
                //
                // The kernel takes the syscall NUMBER from `regs->syscallno`, which ptrace cannot
                // rewrite on arm64 (measured: lowering x8 to `faccessat` changed nothing), so the
                // engine ANSWERS the call instead: the access question is decided here, from the same
                // `resolve` it already translated the path with, and written into the return register
                // at the exit stop ONLY when the host never answered (see there). On a host that
                // allows `faccessat2` this changes nothing.
                if number == nix::libc::SYS_faccessat2 as i64 {
                    if slots[idx].at_entry {
                        slots[idx].faccessat_result = guest_access(pid, &args, plan)?;
                    } else if let Some(answer) = slots[idx].faccessat_result.take() {
                        let rv = return_raw(&get_regs(pid)?);
                        // MEASURED: a seccomp-trapped syscall returns WITHOUT writing x0 - the
                        // register still holds the first argument (AT_FDCWD, -100). So a value that
                        // is still the input, or `-ENOSYS`, means the host never answered; the
                        // engine's own answer is the only one the tracee can use.
                        let unanswered =
                            rv == -(nix::libc::ENOSYS as i64) || (rv != 0 && rv == args[0] as i64);
                        if unanswered {
                            set_return_value(pid, answer)?;
                        }
                    }
                }

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

                // The stat RESULT carries the identity too (2026-11-09). Only when the same mapping that
                // answers `getuid` with 0 is in force, and only on success, so a failed call's buffer is
                // never touched; `PDN_NATIVE_NO_ID_MAP=1` turns this off with the rest of the mapping.
                if !slots[idx].at_entry
                    && std::env::var_os("PDN_NATIVE_NO_ID_MAP").is_none()
                    && matches!(syscall_result(pid), Ok(0))
                {
                    if let Some((arg_index, offset)) = stat_result_identity(number) {
                        if let Some(ptr) = args.get(arg_index).copied() {
                            let _ = poke_bytes(pid, ptr + offset, &[0u8; 8]);
                        }
                    }
                }

                if !slots[idx].at_entry
                    && number == SYS_LINKAT
                    && plan.link2symlink
                    && slots[idx].pending_link.is_some()
                {
                    // LINK2SYMLINK (2026-11-09). Android's SELinux policy refuses a hardlink inside the
                    // app's data area, and proot answers that by creating a symlink instead - which is
                    // why the proot arm succeeds and the native arm did not. The engine does the same
                    // from the host side, on the translated destination, with the guest's own link text,
                    // and reports the success the guest asked for. proot-core passes `--link2symlink` to
                    // proot unconditionally, so this mirrors the shipped behaviour rather than adding an
                    // option nobody asked for.
                    if let Some((_source, destination, link_text)) = slots[idx].pending_link.take()
                    {
                        let failed = matches!(syscall_result(pid), Ok(r) if r < 0);
                        if failed && std::os::unix::fs::symlink(&link_text, &destination).is_ok() {
                            override_id_result(pid)?;
                        }
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

                if is_execve {
                    // EVERY execve is translated - not just the FIRST one per tracee (2026-11-09,
                    // p7 exec-again). A shell's `exec` builtin REPLACES the same process, and a
                    // script's interpreter is started the same way; those used to be refused by name
                    // and left to the kernel, which then resolved the guest path on the HOST.
                    // MEASURED on a real Debian rootfs: `-- /bin/sh -c 'exec /bin/ls /etc'` ran the
                    // host's `/bin/ls` (Android's toybox via the /bin -> /system/bin symlink) and
                    // died inside bionic's loader with `library "libcrypto.so" not found` - not a
                    // missing guest file, an UNTRANSLATED second exec.
                    // The address test ALONE is not an identity test (2026-11-10): a forked child
                    // inherits the shared page, so an ordinary pointer of its own can coincide with the
                    // published address. Measured on a real Debian rootfs with
                    // `sh -c 'dpkg-query -W | wc -l'`: the pipeline's left child exec'd the BARE name
                    // `dpkg-query`, its arg0 matched the page address, this branch was taken, and
                    // `at_entry == false` meant NOTHING happened - the execve reached the kernel with the
                    // host PATH and returned ENOENT (`/bin/sh: dpkg-query: not found`, 0 packages) while
                    // proot printed its 78. The same collision explains the bare-name `ls` case that a
                    // previous change worked around by rewriting the caller's argv[0] instead.
                    // Only an entry stop carries the published pointer, which the comment below always
                    // said; so the pointer test is now paired with it and everything else is treated as
                    // a REMOTE path, read out of the tracee and rewritten there.
                    if path_pointer_is_ours(&report, arg0) && slots[idx].at_entry {
                        // The engine's OWN child: the path lives in the shared page, so the rewrite
                        // is a plain memory write through `translate_execve`.
                        {
                            slots[idx].execve_handled = true;
                            if let Err(e) = translate_execve(page, plan, &mut report) {
                                let _ = ptrace::kill(pid);
                                return Err(e);
                            }
                        }
                    } else {
                        // A path the engine did NOT publish: the engine's own child after its first
                        // exec (the page is gone), a fork descendant, or a later execve of either.
                        // The string lives in the tracee's memory, so it is read and rewritten there
                        // (PTRACE_PEEKDATA / POKEDATA + SETREGSET). This does NOT gate on the
                        // engine's own at_entry bookkeeping: `translate_execve_remote` reads the
                        // pointer out of the tracee and answers `false` when there is no readable
                        // path (e.g. arg0 is a return value at an exit stop), which is the honest
                        // answer either way.
                        match translate_execve_remote(pid, arg0, args[1], plan, &mut report) {
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
                    // 2026-11-09: `chdir` joined the table so a GUEST `cd` translates, and that made
                    // the engine's OWN pre-exec chdir (setup on a host path `resolve_cwd` already
                    // translated) go through translation too - measured: every `--engine native` run
                    // then exited 1 with "SYS_chdir (49) [its path could not be translated]". The
                    // exemption branch below already exists for exactly that call, so the table lookup
                    // is filtered for it and the old branch keeps handling it.
                    // The engine's own bootstrap chdir is handed the ALREADY-TRANSLATED host path, and a
                    // guest `chdir` is handed a guest path, so the two are told apart by where the path
                    // points rather than by `execve_handled` (measured 2026-11-09: that flag is true once
                    // the main child's execve has been handled, which is BEFORE the engine chdirs it into
                    // the guest, so it cannot distinguish the two).
                    // A `chdir` whose path cannot be READ at that stop is the engine's own bootstrap: the
                    // child is chdir'd into the translated host cwd before it execs, and at that point the
                    // argument is not a readable guest string (measured 2026-11-09: `seen=Ok(None)` for the
                    // first chdir of a run, `seen=Ok(Some("/etc"))` for the guest's own `cd`). A readable
                    // path is the guest's and goes through the table. Deciding it by readability rather than
                    // by pid or by comparing host paths keeps one rule: the engine never refuses its own
                    // setup call, and never leaves a guest call to the kernel.
                    // DETERMINISTIC, and measured on both platforms (2026-11-10): the bootstrap chdir of a
                    // run reports `pid_child=true execve_handled=false`, and the guest's own `cd` reports
                    // `execve_handled=true`. An earlier version of this rule tried readability and then
                    // "already a host path", because I had concluded - WITHOUT measuring - that
                    // `execve_handled` was already true at the bootstrap. It is false, the flag is set later
                    // at the execve entry stop, and those heuristics turned a working exemption into a
                    // double translation that failed CI (`cwd=Some("/probe") exit=126`). Bookkeeping the
                    // engine already keeps beats any inference from the syscall's arguments.
                    let engine_bootstrap_chdir = number == nix::libc::SYS_chdir as i64
                        && pid == child
                        && !slots[idx].execve_handled;
                    // TWO-PATH CALLS (2026-11-09). `renameat`/`renameat2` and `linkat` each take two guest
                    // paths and the one-path table cannot express them, so they were refused by name while
                    // the proot arm ran `mv` and `ln`. Both go through the SAME helper the one-path calls
                    // use - source at arg1/dirfd arg0, destination at arg3/dirfd arg2 - so there is one
                    // translation policy, not two.
                    let two_path =
                        number == SYS_RENAMEAT || number == SYS_RENAMEAT2 || number == SYS_LINKAT;
                    if two_path && !engine_bootstrap_chdir {
                        let name = if number == SYS_LINKAT {
                            "SYS_linkat"
                        } else {
                            "SYS_renameat"
                        };
                        let tracee_cwd = slots[idx].cwd.clone();
                        for (arg_index, dirfd_arg) in [(1usize, Some(0usize)), (3, Some(2))] {
                            match translate_path_remote(
                                pid,
                                arg_index,
                                dirfd_arg,
                                name,
                                number,
                                &args,
                                plan,
                                tracee_cwd.as_deref(),
                                &mut report,
                            ) {
                                Ok(true) => {}
                                Ok(false) => {
                                    // Half a rename is worse than a refusal: rewriting one side and
                                    // leaving the other on the host path is exactly what this engine
                                    // refuses, so the whole call is recorded as untranslated.
                                    let refusal = Refusal {
                                        number,
                                        name: Some(name),
                                        reason: "one of its two paths could not be translated",
                                    };
                                    if strict {
                                        report.strict_failure = Some(refusal.label());
                                        report.refusals.push(refusal);
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
                        }
                        if number == SYS_LINKAT {
                            // Both paths are known at this ENTRY stop; the exit stop carries only the
                            // result, so the source, the destination and the guest's own link text are
                            // captured here for the handler below.
                            if let (Ok(Some(src)), Ok(Some(dst))) =
                                (peek_cstr(pid, args[1]), peek_cstr(pid, args[3]))
                            {
                                let rootfs = plan.rootfs.as_deref().unwrap_or(Path::new("/"));
                                slots[idx].pending_link = Some((
                                    resolve(rootfs, &src, &plan.binds),
                                    resolve(rootfs, &dst, &plan.binds),
                                    dst,
                                ));
                            }
                        }
                    } else if let Some(PathSyscall {
                        name,
                        arg_index,
                        dirfd_arg,
                    }) = path_syscall(number).filter({
                        // ENTRY STOPS ONLY (2026-11-10). At an exit stop the argument registers hold
                        // RETURN VALUES, so a path syscall seen there has no path to read: the read
                        // fails, the engine records a refusal, and a successful run is failed for a
                        // call that had already completed. Measured on a real Debian rootfs:
                        //   touch /tmp/f1 && rm -f /tmp/f1      -> SYS_utimensat (88) refused, rc=1
                        //   mkdir /tmp/zz2; touch .../a; rm -rf -> SYS_unlinkat (35) refused, rc=1
                        // while proot ran both and exited 0. The refusals were all exit stops.
                        let at_entry = slots[idx].at_entry;
                        move |_| !engine_bootstrap_chdir && at_entry
                    }) {
                        let tracee_cwd = slots[idx].cwd.clone();
                        match translate_path_remote(
                            pid,
                            arg_index,
                            dirfd_arg,
                            name,
                            number,
                            &args,
                            plan,
                            tracee_cwd.as_deref(),
                            &mut report,
                        ) {
                            Ok(true) => {
                                // A successful `chdir` MOVES the guest: remember where it went so the
                                // next relative path resolves there (guest-absolute, lexically joined).
                                if name == "SYS_chdir" {
                                    if let Ok(Some(raw)) = peek_cstr(pid, args[arg_index]) {
                                        slots[idx].cwd = Some(if raw.starts_with('/') {
                                            raw
                                        } else {
                                            match tracee_cwd.as_deref() {
                                                Some(base) => {
                                                    format!(
                                                        "{}/{}",
                                                        base.trim_end_matches('/'),
                                                        raw
                                                    )
                                                }
                                                None => raw,
                                            }
                                        });
                                    }
                                }
                            }
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
                        // engine observed but does not handle. (execve never reaches this arm: every
                        // execve is handled above, at its entry stop.)
                        let refusal = if unhandled_path_syscall(number).is_some() {
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
                        if strict {
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
                    // A child inherits the PARENT's current directory, not the plan's starting one
                    // (2026-11-09). Measured: `cd /etc && head -1 os-release` still failed after the
                    // per-tracee cwd landed, because `head` is a child and its slot started at the plan
                    // cwd - so its relative open resolved against `/` instead of `/etc`.
                    let inherited_cwd = slots
                        .iter()
                        .find(|s| s.pid == pid)
                        .map(|s| s.cwd.clone())
                        .unwrap_or_else(|| plan.cwd.clone());
                    slots.push(TraceeSlot {
                        pid: new_pid,
                        at_entry: false,
                        execve_handled: false,
                        pending_link: None,
                        cwd: inherited_cwd,
                        started: false,
                        faccessat_result: None,
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
/// The guest's environment (2026-11-09, p7 envp). An EXPLICIT allow-list plus whatever the caller
/// configured, never a copy of the engine's `environ`: the guest is told what it is given, and the
/// caller's environment cannot leak in by accident. Mirrors the proot path's `env_clear()` +
/// `cmd.env(...)` sequence so both engines present the same environment.
pub fn guest_environment(configured: &[(String, String)]) -> Vec<String> {
    let mut env = vec![
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        "HOME=/root".to_string(),
        format!(
            "TERM={}",
            crate::guest_term(std::env::var("TERM").ok().as_deref())
        ),
    ];
    for passthrough in ["LANG", "LC_ALL", "COLORTERM"] {
        if let Ok(value) = std::env::var(passthrough) {
            env.push(format!("{passthrough}={value}"));
        }
    }
    for (key, value) in configured {
        env.push(format!("{key}={value}"));
    }
    env
}

/// Syscalls whose entire result IS the caller's identity (p7 gap 3, 2026-11-09). Only these four can
/// be mapped by rewriting the return register; the `getres*`/`getgroups` family writes through a guest
/// pointer and is deliberately left out rather than silently half-done.
fn id_reporting(number: i64) -> bool {
    number == nix::libc::SYS_getuid as i64
        || number == nix::libc::SYS_geteuid as i64
        || number == nix::libc::SYS_getgid as i64
        || number == nix::libc::SYS_getegid as i64
}

/// Where a stat-family call leaves its result, and where uid/gid sit inside it.
///
/// MEASURED 2026-11-09: with the identity mapping in force the guest answers `id -u` with 0, but `stat -c %u`
/// and `ls -l` still printed the HOST's 10396 on the native arm while the proot arm printed 0 - because
/// `--change-id=0:0:0` makes proot rewrite the stat result and this engine only rewrote the syscalls that
/// RETURN an id. The two fields are adjacent in both structures, so one 8-byte write covers them:
///   aarch64 `struct stat`:  st_mode@16 (u32), st_nlink@20 (u32), st_uid@24, st_gid@28
///   aarch64 `struct statx`: stx_uid@20, stx_gid@24
/// x86_64 layouts differ and are NOT covered here - a named gap, not a silent one.
fn stat_result_identity(number: i64) -> Option<(usize, u64)> {
    #[cfg(target_arch = "aarch64")]
    {
        // ARG 2, not 1: `newfstatat(dirfd, path, buf, flags)` puts its RESULT in the third argument, and
        // writing at the second one overwrote the guest's PATH string - measured 2026-11-09 by the
        // acceptance harness, whose guest stopped running (exit 1, no output) the moment this hook
        // landed, while the differential and write-parity harnesses stayed green and could not see it.
        // `fstat(fd, buf)` keeps its buffer at arg1.
        if number == SYS_NEWFSTATAT {
            return Some((2, 24));
        }
        if number == SYS_FSTAT {
            return Some((1, 24));
        }
        if number == nix::libc::SYS_statx as i64 {
            return Some((4, 20));
        }
    }
    let _ = number;
    None
}

/// The syscall's return value at an exit stop, on either surviving architecture.
fn syscall_result(pid: Pid) -> Result<i64> {
    let raw = get_regs(pid)?;
    #[cfg(target_arch = "aarch64")]
    {
        // The struct exposes the general registers as `regs[0..=30]` on aarch64 - the compiler said so
        // when this first said `raw.x0` - and x0 IS the syscall result there.
        Ok(raw.regs[0] as i64)
    }
    #[cfg(target_arch = "x86_64")]
    {
        Ok(raw.rax as i64)
    }
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

/// The answer `faccessat2` would give, computed by the ENGINE for a guest path, or `None` when the
/// engine cannot answer (no rootfs plan, unreadable/relative path, or an unreadable mode).
///
/// The call is decided against the SAME resolved host path the engine translates the syscall's path
/// argument to, using the permission rule the guest is told it runs under: the engine reports uid 0
/// (`--change-id=0:0:0`), and for a privileged caller `R_OK`/`W_OK` are granted and `X_OK` requires
/// any execute bit. `F_OK` (mode 0) is existence only. A path the rootfs does not carry answers
/// `ENOENT`, exactly as the untranslated kernel would.
fn guest_access(pid: Pid, args: &[u64; 6], plan: &RunPlan) -> Result<Option<i32>> {
    let Some(rootfs) = plan.rootfs.as_ref() else {
        return Ok(None);
    };
    let Some(guest) = peek_cstr(pid, args[1])? else {
        return Ok(None);
    };
    if guest.is_empty() || !guest.starts_with('/') {
        return Ok(None);
    }
    let host = resolve(rootfs, &guest, &plan.binds);
    if !host.exists() {
        return Ok(Some(-nix::libc::ENOENT));
    }
    let mode = args[2] as i32 & 7;
    if mode == 0 {
        return Ok(Some(0));
    }
    let Ok(metadata) = std::fs::metadata(&host) else {
        return Ok(Some(-nix::libc::EACCES));
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = metadata.permissions().mode();
        // Privileged caller (uid 0): R_OK/W_OK granted, X_OK needs any execute bit.
        let allowed = if mode & 1 != 0 {
            bits & 0o111 != 0
        } else {
            true
        };
        return Ok(Some(if allowed { 0 } else { -nix::libc::EACCES }));
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Ok(None)
    }
}

/// Write an arbitrary value into the tracee's syscall RETURN register (`x0` / `rax`).
fn set_return_value(pid: Pid, value: i32) -> Result<()> {
    let mut raw = get_regs(pid)?;
    set_return_raw(&mut raw, value as i64);
    set_regs(pid, raw)
}

/// The return register of a register file: `x0` on aarch64, `rax` on x86_64.
#[cfg(target_arch = "aarch64")]
fn set_return_raw(raw: &mut nix::libc::user_regs_struct, value: i64) {
    raw.regs[0] = value as u64;
}

#[cfg(target_arch = "x86_64")]
fn set_return_raw(raw: &mut nix::libc::user_regs_struct, value: i64) {
    raw.rax = value as u64;
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn set_return_raw(_raw: &mut nix::libc::user_regs_struct, _value: i64) {}

/// Read the syscall RETURN register (`x0` / `rax`) out of a register file.
#[cfg(target_arch = "aarch64")]
fn return_raw(raw: &nix::libc::user_regs_struct) -> i64 {
    raw.regs[0] as i64
}

#[cfg(target_arch = "x86_64")]
fn return_raw(raw: &nix::libc::user_regs_struct) -> i64 {
    raw.rax as i64
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn return_raw(_raw: &nix::libc::user_regs_struct) -> i64 {
    0
}

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
    poke_bytes(pid, addr, &buf)
}

/// Write `bytes` into a STOPPED tracee at `addr`, word by word through `PTRACE_POKEDATA`, the final
/// partial word zero-padded. No terminator is added - the caller chooses the exact bytes - so this
/// is what `poke_cstr` builds on and what the substitution uses for its argv pointer array.
fn poke_bytes(pid: Pid, addr: u64, bytes: &[u8]) -> Result<()> {
    let mut i = 0usize;
    while i < bytes.len() {
        let mut chunk = [0u8; 8];
        let n = std::cmp::min(8, bytes.len() - i);
        chunk[..n].copy_from_slice(&bytes[i..i + n]);
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
                "PTRACE_POKEDATA failed writing at {:#x} into pid {}: {}",
                addr.wrapping_add(i as u64),
                pid,
                std::io::Error::last_os_error()
            ));
        }
        i += 8;
    }
    Ok(())
}

/// Read a NUL-terminated pointer array (a tracee's `argv`) out of a STOPPED tracee, word by word
/// through `PTRACE_PEEKDATA`, bounded by `DESCENDANT_ARGV_MAX`. Returns the pointers WITHOUT the
/// terminating null. A NULL `addr` is an empty array. An array longer than the bound is refused BY
/// NAME: the substitution rewrites the WHOLE array and must not carry a truncated one (2026-11-09,
/// p7 interp-desc).
fn read_ptr_array(pid: Pid, addr: u64) -> Result<Vec<u64>> {
    let mut out: Vec<u64> = Vec::new();
    if addr == 0 {
        return Ok(out);
    }
    let mut at = addr;
    loop {
        if out.len() >= DESCENDANT_ARGV_MAX {
            return Err(anyhow!(
                "a fork descendant's argv has more than {} entries; a substitution rewrites the \
                 whole array, and this engine refuses to carry a truncated one",
                DESCENDANT_ARGV_MAX
            ));
        }
        // errno cleared before the call for the same reason `peek_cstr` clears it: a valid word can
        // read as -1 and ptrace signals failure through errno.
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
            return Err(anyhow!(
                "could not read a fork descendant's argv array at {:#x} (pid {})",
                at,
                pid
            ));
        }
        let ptr = word as u64;
        if ptr == 0 {
            return Ok(out);
        }
        out.push(ptr);
        at = at.wrapping_add(8);
    }
}

/// Write a fork descendant's interpreter substitution into the tracee's OWN stack (2026-11-09, p7
/// interp-desc): the interpreter's HOST path (the new execve path), the interpreter's GUEST path
/// (the new argv[0]), and a fresh argv pointer array `[<interp guest>, <original argv[0..]>, NULL]`.
/// Argument 0 (path) and argument 1 (argv) are published through ONE `PTRACE_SETREGSET`; envp
/// (argument 2) is left untouched, exactly as the child path leaves it.
///
/// The strings and array are laid out in `[sp - DESCENDANT_SCRATCH_BYTES, sp)`, the same window
/// `write_path_and_redirect_arg` writes into, each region 8-byte aligned (so the zero padding of a
/// string's final word cannot reach into the next region). The total is computed BEFORE anything is
/// written and refused BY NAME when it does not fit - the engine's discipline is that nothing is
/// silently dropped, so an argv is never truncated.
///
/// The original argv pointers are reused as-is: the caller's argv[0] string already exists in the
/// tracee, so only the interpreter's guest path, the PROGRAM path and the pointer array are new.
/// envp is untouched.
///
/// PROGRAM IS THE EXECVE TARGET, NOT THE CALLER'S argv[0] (2026-11-09, p7 interp-program). A shell
/// execs `/usr/bin/ls` with argv[0] = `ls`, so building the loader's argv as `[<interp>, argv[0],
/// args...]` told the loader to load a program literally named `ls` and it reported `ls: cannot open
/// shared object file` (measured on a real Debian rootfs). The loader's PROGRAM is now `guest_program`
/// - the target guest path the engine already translated - and, when the interpreter understands
/// `--argv0 STRING`, the caller's argv[0] is passed through it so the guest still sees its own name.
/// Without the option the plain form is used and the program's argv[0] becomes `guest_program`; that
/// divergence is measured and documented in this module's header.
fn write_substitution_remote(
    pid: Pid,
    host_interp: &Path,
    guest_interp: &str,
    guest_program: &str,
    original_argv: &[u64],
    use_argv0: bool,
) -> Result<()> {
    const ARGV0_OPTION: &str = "--argv0";
    let host_bytes = host_interp.as_os_str().as_encoded_bytes();
    let guest_bytes = guest_interp.as_bytes();
    let prog_bytes = guest_program.as_bytes();
    let align8 = |n: usize| (n + 7) & !7usize;
    let host_len = align8(host_bytes.len() + 1);
    let guest_len = align8(guest_bytes.len() + 1);
    let prog_len = align8(prog_bytes.len() + 1);
    let opt_len = if use_argv0 {
        align8(ARGV0_OPTION.len() + 1)
    } else {
        0
    };
    // Under `--argv0`: [<interp>, "--argv0", <orig argv[0]>, <program>, <orig argv[1..]>]. Without
    // it: [<interp>, <program>, <orig argv[1..]>]. Plus the terminating null.
    let tail = original_argv.get(1..).unwrap_or(&[]);
    let head = if use_argv0 { 4 } else { 2 };
    let slots = head + tail.len() + 1;
    let array_len = slots * std::mem::size_of::<u64>();
    let total = host_len + guest_len + prog_len + opt_len + array_len;
    if total as u64 > DESCENDANT_SCRATCH_BYTES {
        return Err(anyhow!(
            "a fork descendant's interpreter substitution needs {} bytes of stack scratch ({} argv \
             entries) and this engine reserves {} below the stack pointer; refusing rather than \
             truncating the argv",
            total,
            original_argv.len(),
            DESCENDANT_SCRATCH_BYTES
        ));
    }

    let mut raw = get_regs(pid)?;
    let sp = stack_pointer(&raw);
    let scratch = sp.wrapping_sub(DESCENDANT_SCRATCH_BYTES) & !0xf;
    let host_at = scratch;
    let guest_at = scratch + host_len as u64;
    let prog_at = guest_at + guest_len as u64;
    let opt_at = prog_at + prog_len as u64;
    let array_at = opt_at + opt_len as u64;

    poke_cstr(pid, host_at, host_bytes)?;
    poke_cstr(pid, guest_at, guest_bytes)?;
    poke_cstr(pid, prog_at, prog_bytes)?;
    if use_argv0 {
        poke_cstr(pid, opt_at, ARGV0_OPTION.as_bytes())?;
    }
    let mut array: Vec<u8> = Vec::with_capacity(array_len);
    array.extend_from_slice(&guest_at.to_ne_bytes());
    if use_argv0 {
        array.extend_from_slice(&opt_at.to_ne_bytes());
        // The caller's argv[0] string already lives in the tracee; reuse its pointer. A caller that
        // passed no argv[0] gets the program path, so the loader is never handed a null string.
        array.extend_from_slice(
            &original_argv
                .first()
                .copied()
                .unwrap_or(prog_at)
                .to_ne_bytes(),
        );
    }
    array.extend_from_slice(&prog_at.to_ne_bytes());
    for p in tail {
        array.extend_from_slice(&p.to_ne_bytes());
    }
    array.extend_from_slice(&0u64.to_ne_bytes());
    poke_bytes(pid, array_at, &array)?;

    #[cfg(target_arch = "aarch64")]
    {
        // x0 = path, x1 = argv; the argument registers, matching `write_path_and_redirect_arg`.
        raw.regs[0] = host_at;
        raw.regs[1] = array_at;
    }
    #[cfg(target_arch = "x86_64")]
    {
        raw.rdi = host_at;
        raw.rsi = array_at;
    }
    set_regs(pid, raw)?;
    Ok(())
}

/// The tracee's stack pointer: `sp` on aarch64, `rsp` on x86_64. Kept in one place so the two
/// register-file layouts are named once, the same rule `write_path_and_redirect_arg` follows.
#[cfg(target_arch = "aarch64")]
fn stack_pointer(raw: &nix::libc::user_regs_struct) -> u64 {
    raw.sp
}

#[cfg(target_arch = "x86_64")]
fn stack_pointer(raw: &nix::libc::user_regs_struct) -> u64 {
    raw.rsp
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn stack_pointer(_raw: &nix::libc::user_regs_struct) -> u64 {
    0
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
    // ONE SLOT PER ARGUMENT (2026-11-09). A two-path call translates two arguments, and with a single
    // scratch address the second write landed on the first: measured on `mv a b`, the source and the
    // destination both pointed at the LAST translated path, so the kernel answered
    // `mv: cannot move 'a' to 'b': No such file or directory` while the proot arm renamed it.
    const SCRATCH_SLOT: u64 = 512;
    let slot_offset = SCRATCH_SLOT * arg_index.min(3) as u64;
    let mut raw = get_regs(pid)?;
    #[cfg(target_arch = "aarch64")]
    {
        let scratch = raw.sp.wrapping_sub(SCRATCH_BELOW_SP + slot_offset) & !0xf;
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
        let scratch = raw.rsp.wrapping_sub(SCRATCH_BELOW_SP + slot_offset) & !0xf;
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
/// The guest path of the directory a tracee's descriptor refers to, if it is inside the rootfs.
///
/// `/proc/<pid>/fd/<fd>` answers the HOST path the descriptor was opened on; stripping the rootfs
/// prefix maps it back into the guest, which is what a `dirfd`-relative name must be joined onto.
/// A descriptor outside the rootfs answers `None` and the caller refuses, as before.
fn host_dir_for_fd_as_guest(
    pid: Pid,
    fd: i32,
    rootfs: &Path,
    binds: &[(String, PathBuf)],
) -> Option<String> {
    let target = std::fs::read_link(format!("/proc/{}/fd/{}", pid.as_raw(), fd)).ok()?;
    // Compare every spelling of the two paths before declaring the descriptor to be OUTSIDE the rootfs
    // (2026-11-10). `rm -rf` unlinks a directory's children through a descriptor it opened, and the
    // target `/proc/<pid>/fd/N` reports is the same directory as the rootfs path in the store - but not
    // necessarily the same STRING: the store can hold a symlinked or non-canonical rootfs while the
    // descriptor reports the resolved one. A textual `strip_prefix` then failed, the call was refused,
    // and `rm -rf` exited 1 where proot exits 0 (`SYS_unlinkat (35) ... could not be translated`), while
    // the absolute form with `AT_FDCWD` translated fine.
    let root_forms = [rootfs.to_path_buf(), std::fs::canonicalize(rootfs).ok()?];
    let target_forms = [
        target.clone(),
        std::fs::canonicalize(&target).unwrap_or_else(|_| target.clone()),
    ];
    for root in root_forms.iter().flatten() {
        for t in target_forms.iter() {
            if let Ok(rel) = t.strip_prefix(root) {
                return Some(format!("/{}", rel.display()));
            }
        }
    }
    // THE BINDS TOO, and this is the case that mattered (2026-11-10). The guest's `/tmp` is a BIND whose
    // SOURCE is `<store>/containers/<name>/tmp`, i.e. outside the rootfs - so a descriptor held by a
    // recursive removal, opened on that directory, has a target no rootfs prefix matches. The
    // untranslated host path was then handed to `resolve` and mapped into the rootfs a SECOND time:
    //   guest = "<store>/containers/deb-renamed/tmp/za/a"
    //   host  = "<store>/containers/deb-renamed/rootfs/<store>/containers/deb-renamed/tmp/za/a"
    // The call then acted on a path that does not exist, silently did nothing, and the guest reported
    // `Directory not empty` while proot removed the directory. Matching the bind SOURCE recovers the
    // guest form of that directory, which is what belongs in the tracee.
    for (guest, host) in binds {
        let host_forms = [
            host.clone(),
            std::fs::canonicalize(host).unwrap_or_else(|_| host.clone()),
        ];
        for h in host_forms.iter() {
            for t in target_forms.iter() {
                if let Ok(rel) = t.strip_prefix(h) {
                    let joined = format!("{}/{}", guest.trim_end_matches('/'), rel.display());
                    return Some(joined.trim_end_matches('/').to_string());
                }
            }
        }
    }
    None
}

/// Resolve `guest` through the same `resolve` the rest of the engine uses, write the host path into
/// the tracee and redirect the argument. Split out so the cwd-relative path and the `dirfd`-relative
/// path share one implementation instead of two that can disagree.
#[allow(clippy::too_many_arguments)]
fn finish_translation(
    pid: Pid,
    arg_index: usize,
    rootfs: &Path,
    plan: &RunPlan,
    name: &'static str,
    number: i64,
    guest: String,
    report: &mut NativeRunReport,
) -> Result<bool> {
    let host = resolve(rootfs, &guest, &plan.binds);
    if host.as_path() == Path::new(&guest) {
        return Ok(true);
    }
    let bytes = host.as_os_str().as_encoded_bytes();
    if bytes.len() > PATH_CAPACITY {
        return Err(anyhow!(
            "translated path of {} bytes does not fit the {} byte translation area: {} (syscall {}, number {})",
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

fn translate_path_remote(
    pid: Pid,
    arg_index: usize,
    dirfd_arg: Option<usize>,
    name: &'static str,
    number: i64,
    args: &[u64; 6],
    plan: &RunPlan,
    tracee_cwd: Option<&str>,
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
    // A NULL path is NOT an untranslatable one (2026-11-10). `utimensat(fd, NULL, times, 0)` is the
    // `futimens` form: it operates on the DESCRIPTOR in arg0 and simply has no path, so there is nothing
    // to translate and nothing to complain about. Measured on a real Debian rootfs with
    // `touch /tmp/f1 && rm -f /tmp/f1`:
    //   PEEKDATA SYS_utimensat number=88 arg_index=1 arg=0x0               peek=Ok(None)
    //   PEEKDATA SYS_unlinkat  number=35 arg_index=1 arg=0x558b6e19e0      peek=Ok(Some("/tmp/f1"))
    // Treating that NULL as a failed translation made the engine refuse and the run exit 1 where proot
    // exits 0, even though the guest's own work had already succeeded (`UNLINK-OK` printed).
    if *arg == 0 {
        return Ok(true);
    }
    let Some(raw) = peek_cstr(pid, *arg)? else {
        return Ok(false);
    };
    if raw.is_empty() {
        // Same reasoning: an empty path is something the kernel rejects with ENOENT by itself. Failing
        // the whole RUN for it would report the engine's bookkeeping, not the guest's behaviour.
        return Ok(true);
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
    // must carry no `..` component: that is the ONLY component that can walk a lexically joined name out
    // of the rootfs (through a symlink), so it is refused rather than joined. A `.` component is the
    // identity - the kernel resolves `cwd/.` to `cwd` for the guest exactly as the join does - so it is
    // translated like any other relative name. This test used to be `== "." || == ".."`, which also
    // refused `.`; that refused the loader's `newfstatat(AT_FDCWD, ".")` for a fork descendant (measured
    // 2026-11-09 on a real Debian rootfs: `-- /bin/sh -c '/bin/dash -c "echo CHILD-RAN"'` printed
    // CHILD-RAN but exited 1 on the un-translated `.`), a path that cannot escape the rootfs.
    let guest = if raw.starts_with('/') {
        // The name may already be a HOST path: see `guest_form_of_host_path`. Only when it is inside the
        // rootfs or a bind source does this change anything, so an ordinary guest-absolute name is
        // untouched.
        guest_form_of_host_path(&raw, rootfs, &plan.binds).unwrap_or(raw)
    } else {
        // A DIRFD-RELATIVE name with a REAL descriptor (2026-11-09). `rm -rf /root/x` refused with
        // "SYS_unlinkat (35) [its path could not be translated]" because `rm` opens the directory and
        // unlinks children through that fd, and this code only joined onto the cwd for `AT_FDCWD`.
        // The descriptor's own path is readable through /proc, so the name is resolved against the
        // directory it actually belongs to: mapping that host path back into the guest keeps the
        // translation honest (and a descriptor outside the rootfs is still refused).
        let mut fd_guest_dir: Option<String> = None;
        let against_cwd = match dirfd_arg {
            Some(i) => match args.get(i).map(|v| *v as i32) {
                Some(fd) if fd == nix::libc::AT_FDCWD => true,
                Some(fd) if fd >= 0 => {
                    fd_guest_dir = host_dir_for_fd_as_guest(pid, fd, rootfs, &plan.binds);
                    false
                }
                _ => false,
            },
            None => true,
        };
        let escapes = raw.split('/').any(|c| c == "..");
        if let (Some(dir), false) = (fd_guest_dir.as_deref(), against_cwd) {
            // The directory MUST be a guest-absolute path (2026-11-10). A host path arriving here is resolved
            // a second time inside the rootfs, the call acts on a filename that does not exist, and the guest
            // reports the failure as its own (`Directory not empty` while proot removes the directory). The
            // value is therefore mapped back with the same rule any host-shaped name gets, and anything that is
            // still not a guest path refuses the call instead of being written into the tracee.
            let dir_owned = guest_form_of_host_path(dir, rootfs, &plan.binds)
                .unwrap_or_else(|| dir.to_string());
            if !dir_owned.starts_with('/') || std::path::Path::new(&dir_owned).starts_with(rootfs) {
                return Ok(false);
            }
            let dir = dir_owned.as_str();
            if escapes {
                return Ok(false);
            }
            return finish_translation(
                pid,
                arg_index,
                rootfs,
                plan,
                name,
                number,
                format!("{}/{}", dir.trim_end_matches('/'), raw),
                report,
            );
        }
        match tracee_cwd {
            Some(cwd) if against_cwd && !escapes => {
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
/// descendant), substituting the guest's interpreter when its `PT_INTERP` is absent on the host -
/// the SAME substitution the child path makes through the shared page (2026-11-09, p7 interp-desc).
///
/// Reads the guest path from the tracee at `arg0`, resolves it through the SAME `resolve` the
/// shared-page translation uses (binds first, then the rootfs), and points `arg0` at a copy of the
/// host path written into the tracee's own memory. When `guest_interpreter` reports a substitution
/// (its three conditions: absolute, absent on the host, present in the rootfs - REUSED, not
/// re-implemented), the exec is redirected instead: `arg0` becomes the interpreter's HOST path and
/// `arg1` a NEW argv array on the tracee's own stack (`write_substitution_remote`), so the loader
/// runs the guest program exactly as the child path does.
///
/// Returns `Ok(false)` when `arg0` does not point at a readable string (a non-entry stop) or when
/// there is no plan, so the caller can tell "nothing to do here" from a real refusal.
/// Resolve a NON-absolute execve path the way the kernel would for THIS tracee (2026-11-10).
///
/// A shell execs a bare name (`dpkg-query`) and lets the kernel search `PATH`; a relative path
/// (`./prog`, `sub/prog`) is resolved against the cwd. Translation needs an ABSOLUTE guest path,
/// because that path is what gets mapped into the rootfs - and without this step the bare name was
/// resolved against `/` (`<rootfs>/dpkg-query`), the file was not found, the execve was left alone and
/// the KERNEL answered `ENOENT` after searching the host's PATH. That is what a user sees as
///
/// ```text
/// /bin/sh: 1: dpkg-query: not found
/// ```
///
/// while the same guest under proot prints its 78 packages. MEASURED, 5 runs each way: native 0,
/// proot 78, deterministic. The full path worked all along (`/usr/bin/dpkg-query --version`), which
/// is why a battery that only ever used absolute paths could not see this.
///
/// `None` means "the engine cannot tell where the kernel would look" - not absolute, cwd unknown, or
/// nothing in PATH exists in the rootfs. The caller then leaves the execve alone so the guest gets the
/// kernel's own error, which is the honest answer.
fn resolve_exec_path(pid: Pid, guest: &str, plan: &RunPlan) -> Option<String> {
    if guest.starts_with('/') {
        return Some(guest.to_string());
    }
    let rootfs = plan.rootfs.as_ref()?;
    if guest.contains('/') {
        let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
        let host_root = std::fs::canonicalize(rootfs).ok()?;
        let rel = cwd.strip_prefix(&host_root).ok()?;
        let joined = format!("{}/{}", rel.display(), guest);
        let joined = joined.trim_start_matches('/');
        return Some(format!("/{joined}"));
    }
    let environ = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let path_var = environ
        .split(|b| *b == 0)
        .filter_map(|e| std::str::from_utf8(e).ok())
        .find_map(|e| e.strip_prefix("PATH="))
        .unwrap_or("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin");
    for dir in path_var.split(':').filter(|d| d.starts_with('/')) {
        let candidate = format!("{}/{}", dir.trim_end_matches('/'), guest);
        if resolve(rootfs, &candidate, &plan.binds).exists() {
            return Some(candidate);
        }
    }
    None
}

fn translate_execve_remote(
    pid: Pid,
    arg0: u64,
    argv_ptr: u64,
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
    // A bare name or a relative one is what the KERNEL would search for; the engine needs the absolute
    // guest path it would have found, or it must leave the call alone (see `resolve_exec_path`).
    let Some(guest) = resolve_exec_path(pid, &guest, plan) else {
        return Ok(false);
    };

    // The descendant's interpreter substitution: the SAME three-condition policy the child path
    // uses, reused through `guest_interpreter`. A substitution only happens when the interpreter is
    // absent on the host and present in the rootfs, so an ordinary guest executable (its loader on
    // the host) takes the plain path below, unchanged.
    if let Some(interp) = guest_interpreter(plan, &guest) {
        let host_interp = resolve(rootfs, &interp, &plan.binds);
        let original = read_ptr_array(pid, argv_ptr)?;
        // The loader's PROGRAM is the EXECVE TARGET (`guest`), not the caller's argv[0]. Where the
        // interpreter understands `--argv0` the caller's argv[0] is preserved through it; otherwise
        // the plain form is used and the program's argv[0] becomes `guest` (documented divergence).
        let use_argv0 = interpreter_accepts_argv0(&host_interp);
        write_substitution_remote(pid, &host_interp, &interp, &guest, &original, use_argv0)?;
        // Record the interpreter the way the child path does: the report names it, and the execve
        // translation names the interpreter's GUEST path -> its HOST path inside the rootfs.
        if report.interpreter.is_none() {
            report.interpreter = Some(interp.clone());
        }
        report.translations.push(Translation {
            guest: interp,
            host: host_interp,
            syscall: "SYS_execve",
        });
        return Ok(true);
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
/// Turn a path that is ALREADY a host path back into its guest form (2026-11-10).
///
/// A guest can compose a name from material the engine itself translated: a recursive removal reads the
/// directory it opened through a descriptor and builds `<that directory>/<entry>`, so the NAME it passes is a
/// host path. Mapping that name into the rootfs again put the file somewhere else entirely and the call
/// silently did nothing (the guest reported `Directory not empty` and exited 0 where proot removed the
/// directory). Both the rootfs and the bind SOURCES are recognised - the guest's `/tmp` is a bind whose source
/// lives outside the rootfs - and the LONGEST match wins, so a bind nested inside another cannot shadow it.
fn guest_form_of_host_path(
    raw: &str,
    rootfs: &Path,
    binds: &[(String, PathBuf)],
) -> Option<String> {
    let path = Path::new(raw);
    let canonical = std::fs::canonicalize(path).ok();
    let mut best: Option<(usize, String)> = None;

    // Every prefix that can appear in a name the guest composed: the bind sources first (the guest's
    // `/tmp` is a bind whose source is outside the rootfs), then the rootfs itself with an empty guest
    // directory.
    let mut prefixes: Vec<(PathBuf, String)> = Vec::new();
    for (guest, host) in binds {
        prefixes.push((host.clone(), guest.clone()));
        if let Ok(c) = std::fs::canonicalize(host) {
            prefixes.push((c, guest.clone()));
        }
    }
    prefixes.push((rootfs.to_path_buf(), String::new()));
    if let Ok(c) = std::fs::canonicalize(rootfs) {
        prefixes.push((c, String::new()));
    }

    for (prefix, guest_dir) in prefixes {
        let len = prefix.as_os_str().len();
        if best.as_ref().is_some_and(|(l, _)| len <= *l) {
            continue;
        }
        for candidate in [Some(path), canonical.as_deref()].into_iter().flatten() {
            let Ok(rel) = candidate.strip_prefix(&prefix) else {
                continue;
            };
            let joined = format!("{}/{}", guest_dir.trim_end_matches('/'), rel.display());
            let joined = joined.trim_end_matches('/').to_string();
            best = Some((
                len,
                if joined.is_empty() {
                    "/".to_string()
                } else {
                    joined
                },
            ));
            break;
        }
    }
    best.map(|(_, joined)| joined)
}

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

/// Whether the guest's own interpreter understands `--argv0 STRING`, measured from the interpreter
/// IMAGE rather than assumed from its name (2026-11-09, p7 interp-program).
///
/// Public only so the regression tests can compute which argv[0] form to EXPECT on the host they run
/// on (bionic has no `--argv0`; glibc and musl 1.2.4 do), instead of pinning one host's answer.
///
/// MEASURED on this host:
///   * Debian 13's glibc `ld-linux-aarch64.so.1`: `--help` prints
///     `  --argv0 STRING        set argv[0] to STRING before running`, exit 0, and the image carries
///     the literal `--argv0\0`;
///   * Alpine 3.21's musl 1.2.4 `ld-musl-aarch64.so.1`: its option table is
///     `library-path\0preload\0argv0\0`, so it carries `argv0\0` too and accepts `--argv0`;
///   * Android's bionic `/system/bin/linker64` carries no `argv0` token at all (`grep -c argv0` -> 0)
///     and has no such option - it is the interpreter the CI/device fixtures use.
/// So the option token in the loader image is the capability probe. CEILING: a loader that accepts
/// the option but keeps its name out of the image is reported as unsupported and takes the plain
/// form, whose only consequence is the documented argv[0] divergence.
pub fn interpreter_accepts_argv0(host_interp: &Path) -> bool {
    const TOKEN: &[u8] = b"argv0\0";
    match std::fs::read(host_interp) {
        Ok(image) => image.windows(TOKEN.len()).any(|w| w == TOKEN),
        Err(_) => false,
    }
}

/// The guest's interpreter to EXEC in place of the guest binary, or `None` when no substitution is
/// needed or the engine cannot make one (2026-11-09, p7 interp).
///
/// A substitution is made only when BOTH halves are measured true: the guest's `PT_INTERP` does NOT
/// exist on the host (so the kernel would fail before the tracee runs a single instruction - the
/// measured Debian-rootfs failure), and the SAME path DOES exist inside the rootfs (so there is a
/// real interpreter to exec). With either half false the engine leaves the exec alone: a guest whose
/// interpreter the host can open is started normally, and a guest whose interpreter exists NOWHERE is
/// left for the kernel to refuse honestly rather than exec a guessed path.
fn guest_interpreter(plan: &RunPlan, guest: &str) -> Option<String> {
    let rootfs = plan.rootfs.as_ref()?;
    let host = resolve(rootfs, guest, &plan.binds);
    let interp = elf_interp(&host)?;
    // The kernel resolves a RELATIVE `PT_INTERP` against the tracee's cwd, which the engine does not
    // model at exec time; refuse it rather than guess. `PT_INTERP` is absolute on every supported
    // target.
    if !interp.starts_with('/') {
        return None;
    }
    if Path::new(&interp).exists() {
        return None; // the kernel can open it itself: nothing to substitute
    }
    if !resolve(rootfs, &interp, &plan.binds).exists() {
        return None; // the rootfs carries no such interpreter: exec will fail, honestly
    }
    Some(interp)
}

/// The `PT_INTERP` string of the ELF at `path`, or `None` when the file cannot be read, is not a
/// 64-bit little-endian ELF, or carries no `PT_INTERP` (2026-11-09, p7 interp).
///
/// Only the three fields the engine needs are read - the ELF header, the program-header table and
/// the interpreter string - so a large guest binary is not pulled into memory to answer one question.
/// Both supported hosts (aarch64, x86_64) are 64-bit little-endian; any other class or byte order
/// answers `None` rather than a misread path.
pub fn elf_interp(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut ehdr = [0u8; 64];
    file.read_exact(&mut ehdr).ok()?;
    if &ehdr[0..4] != b"\x7fELF" || ehdr[4] != 2 || ehdr[5] != 1 {
        return None; // not a 64-bit little-endian ELF
    }
    let e_phoff = u64::from_le_bytes(ehdr[32..40].try_into().ok()?);
    let e_phentsize = u16::from_le_bytes(ehdr[54..56].try_into().ok()?) as usize;
    let e_phnum = u16::from_le_bytes(ehdr[56..58].try_into().ok()?) as usize;
    const PT_INTERP: u32 = 3;
    for i in 0..e_phnum {
        let mut ph = vec![0u8; e_phentsize];
        file.seek(SeekFrom::Start(
            e_phoff.checked_add(i as u64 * e_phentsize as u64)?,
        ))
        .ok()?;
        file.read_exact(&mut ph).ok()?;
        if u32::from_le_bytes(ph[0..4].try_into().ok()?) != PT_INTERP {
            continue;
        }
        // ELF64 program header: p_offset at byte 8, p_filesz at byte 32.
        let p_offset = u64::from_le_bytes(ph[8..16].try_into().ok()?);
        let p_filesz = u64::from_le_bytes(ph[32..40].try_into().ok()?) as usize;
        // The interpreter path is a NUL-terminated string of at most `p_filesz` bytes.
        let mut raw = vec![0u8; p_filesz];
        file.seek(SeekFrom::Start(p_offset)).ok()?;
        file.read_exact(&mut raw).ok()?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let s = String::from_utf8_lossy(&raw[..end]).into_owned();
        return if s.is_empty() { None } else { Some(s) };
    }
    None
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
