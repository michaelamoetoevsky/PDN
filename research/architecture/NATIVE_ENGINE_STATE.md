# Native engine (proot-core): measured starting state for the p4 block

All figures and claims here were measured on 2026-11-08 against `master` at `ca7e27e` by reading the
source and by running the crate's own tests. Nothing in this file is inferred from prose.

## What the crate is

`crates/proot-core` is 4,101 lines across 15 modules. The largest are `syscall.rs` (1,395),
`lib.rs` (679), `tracee.rs` (327), `selective.rs` (246), `seccomp.rs` (211), `arch.rs` (205) and
`ptrace.rs` (196).

## What exists AND is reachable (verified)

- **A working ptrace tracer, used for a selective seccomp path.** `selective::run_traced` and
  `selective::run_selected_cmd` arm a seccomp filter, then serve the stops it produces. They are called
  from `lib.rs:534` and covered by `crates/proot-core/tests/selective_seccomp.rs` (lines 74 and 143).
  This is the crate's only *exercised* ptrace mechanism.
- **`PtraceController` with real options.** `ptrace.rs:68-74` requests `PTRACE_O_TRACESYSGOOD`,
  `TRACECLONE`, `TRACEFORK`, `TRACEVFORK`, `TRACEEXEC`, `TRACESECCOMP` and `EXITKILL`.
- **`run_loop` is implemented, not a stub.** `ptrace.rs:94` waits on `waitpid(None, None)` and dispatches
  `PtraceEvent` / `PtraceSyscall` / `Signaled` / `Exited` / `Stopped` through `handle_event`. It does its
  job; the problem is below, not here.

## What is NOT reachable

- **`ProotEngine::run()` has no caller in any binary.** `lib.rs:173` runs the `run_loop` above, but nothing
  constructs `ProotEngine` outside tests, so this tracer never runs in production. The measured
  consequence stands: no `PTRACE_CONT`, `PTRACE_SYSCALL` or `PTRACE_GETREGSET` call exists anywhere in the
  tree - those three strings have zero hits.
- **`tracee.rs:110 read_syscall()` is the stub.** It returns the placeholder
  `RawSyscall { number: 0, args: [0; 6] }`, so any dispatch built on it would decide on `0` for every
  syscall. `syscall.rs` holds 155 handler *patterns* with nothing reading a real number to select among
  them.

## Consequence for the p4 plan

The plan's steps 1-2 ("resume the tracee", "read real registers") are **already half solved** by a
different mechanism than the plan assumed: the seccomp-armed stop in `selective.rs` is a real, tested way
to stop on a chosen syscall, and it does not need `run_loop` to be reachable first. So the smallest
verifiable increment is not "build a tracer" but:

1. **Make `read_syscall()` read a real register** (`PTRACE_GETREGSET` with `NT_PRSTATUS`, the number in the
   arch-appropriate register - `arch.rs` already exists for that mapping), returning `number: 0` only when
   the read genuinely fails, and failing by name rather than silently.
2. **Prove it with the harness that already exists**: extend `tests/selective_seccomp.rs` so one case
   asserts the *number* it observed is the syscall the workload really made (for example `openat`), and a
   negative control shows the assertion fails when the read is stubbed. A number that is merely non-zero
   proves nothing.
3. Only then fill the dispatch table, because until step 1 the table can only ever be selected by `0`.

This order is what keeps the block measurable: each step has an observable that can fail, and step 2's
negative control is the difference between "we read a register" and "we read the right register".

---

## Measured later the same day (2026-11-08): steps 1 and 2 landed, step 3 re-scoped

**Step 1 - done and CI-proven (`771b817`).** `Tracee::read_registers()` reads `PTRACE_GETREGSET` with
`NT_PRSTATUS`; aarch64 takes the number from x8 and the result from x0, x86_64 from `orig_rax`/`rax`. Three
platform facts cost a compile cycle each and are recorded in the code: nix 0.27 has no aarch64
`ptrace::getregs`; libc does not export `NT_PRSTATUS` for this target; and Android's `libc::ptrace` takes
`request: c_int` and is variadic while the Linux runner's takes `c_uint`, so the cast is `as _`.

**Step 2 - done and CI-proven (`6977c6b`, `40c8cf6`).** The number reaches the handler table
(`Tracee::handle_syscall_entry` now uses its `handler` argument instead of ignoring it), `SyscallHandler`
gained `pub fn register`, and `tests/native_dispatch.rs` asserts both directions: a handler registered for
`SYS_getpid` runs, and one registered for `SYS_getppid` does not while `syscall_info` still records
`SYS_getpid` - which separates a broken dispatch from a broken read. `SyscallHandlerFn` requires
`Send + Sync`, so the flag is an `Arc<AtomicBool>`.

**Step 3 - re-scoped by measurement, not by plan.** `ProotEngine::run()` (lib.rs:173-184) installs the
seccomp filter and calls `ptrace_ctrl.run_loop(...)` with **no spawn and no `TRACEME` child**, so it can
never see a tracee. The code that builds a command line is `ProotEngine::start()` (:186 onward), and it
spawns the **external** `proot` binary with `--kill-on-exit --link2symlink --sysvipc -L
--change-id=0:0:0 --kernel-release ... --rootfs=.`. So production runs external proot and the internal
tracer has no child of its own. "Run containers without the proot binary" is therefore a subsystem with
three parts: a spawn path (`fork` -> `PTRACE_TRACEME` -> `execve`), emulation of those six argv features,
and `execve` interception with path translation - the last being the hardest.

**Next increment (`p4-3a`), smallest honest unit.** Add a spawn+trace entry point that forks, `TRACEME`s and
execs a command **as given**, drives the loop over it, and returns the exit status together with the
syscall numbers it saw but could not translate - refusing **by name** instead of pretending to have run the
workload. Prove it with `/bin/true` (exit 0, at least one syscall observed) and a control (`/nonexistent`,
which must report the exec failure rather than a zero exit). Do **not** swap the production callers
(`native/image.rs:130-131`, `proot-core/src/lib.rs:194-195` and `:336-337`) before parity is measured on the
same workload: swapping early turns `podnext run` into a regression.
