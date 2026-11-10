# Native engine, proot-core: measured starting state for the p4 block

All figures and claims here come from measurement on 2026-11-08 against `master` at `ca7e27e`. The
measurement read the source and ran the crate's own tests. Nothing in this file is inferred from prose.

## What the crate is

`crates/proot-core` is 4,101 lines across 15 modules. The largest are `syscall.rs` at 1,395 lines,
`lib.rs` at 679 lines, `tracee.rs` at 327 lines, `selective.rs` at 246 lines, `seccomp.rs` at 211 lines,
`arch.rs` at 205 lines and `ptrace.rs` at 196 lines.

## What exists AND is reachable, verified

- **A working ptrace tracer, used for a selective seccomp path.** `selective::run_traced` and
  `selective::run_selected_cmd` arm a seccomp filter, then serve the stops it produces. They are called
  from `lib.rs:534` and `crates/proot-core/tests/selective_seccomp.rs` covers them at lines 74 and 143.
  This is the crate's only *exercised* ptrace mechanism.
- **`PtraceController` with real options.** `ptrace.rs:68-74` requests `PTRACE_O_TRACESYSGOOD`,
  `TRACECLONE`, `TRACEFORK`, `TRACEVFORK`, `TRACEEXEC`, `TRACESECCOMP` and `EXITKILL`.
- **`run_loop` is implemented, not a stub.** `ptrace.rs:94` waits on `waitpid(None, None)` and dispatches
  `PtraceEvent` / `PtraceSyscall` / `Signaled` / `Exited` / `Stopped` through `handle_event`. It does its
  job. The problem is below, not here.

## What is NOT reachable

- **`ProotEngine::run()` has no caller in any binary.** `lib.rs:173` runs the `run_loop` above, but nothing
  constructs `ProotEngine` outside tests, so this tracer never runs in production. The measured
  consequence stands: no `PTRACE_CONT`, `PTRACE_SYSCALL` or `PTRACE_GETREGSET` call exists anywhere in the
  tree. Those three strings have zero hits.
- **`tracee.rs:110 read_syscall()` is the stub.** It returns the placeholder
  `RawSyscall { number: 0, args: [0; 6] }`, so a dispatch built on it decides on `0` for every
  syscall. `syscall.rs` holds 155 handler *patterns*, and no code reads a real number to select among
  them.

## Consequence for the p4 plan

The plan's steps 1-2, "resume the tracee" and "read real registers", are **already half solved** by a
different mechanism than the plan assumed. The seccomp-armed stop in `selective.rs` is a real, tested way
to stop on a chosen syscall, and it does not need `run_loop` to be reachable first. So the smallest
verifiable increment is not "build a tracer" but:

1. **Make `read_syscall()` read a real register.** Use `PTRACE_GETREGSET` with `NT_PRSTATUS`, and read the
   number from the arch-appropriate register. `arch.rs` already exists for that mapping. Return `number: 0`
   only when the read genuinely fails, and fail by name, not silently.
2. **Prove it with the harness that already exists**: extend `tests/selective_seccomp.rs` so one case
   asserts the *number* it observed is the syscall the workload really made, for example `openat`. Add a
   negative control that shows the assertion fails when the read is stubbed. A number that is merely
   non-zero proves nothing.
3. Only then fill the dispatch table, because until step 1 the table can only ever be selected by `0`.

This order is what keeps the block measurable: each step has an observable that can fail, and step 2's
negative control is the difference between "we read a register" and "we read the right register".

---

## Measured later the same day, 2026-11-08: steps 1 and 2 landed, step 3 re-scoped

**Step 1 - done and CI-proven at `771b817`.** `Tracee::read_registers()` reads `PTRACE_GETREGSET` with
`NT_PRSTATUS`. On aarch64 the number comes from x8 and the result from x0. On x86_64 they come from
`orig_rax` and `rax`. Three platform facts each cost a compile cycle, and the code records them. Nix 0.27
has no aarch64 `ptrace::getregs`. libc does not export `NT_PRSTATUS` for this target. Android's `libc::ptrace`
takes `request: c_int` and is variadic, while the Linux runner's takes `c_uint`, so the cast is `as _`.

**Step 2 - done and CI-proven at `6977c6b` and `40c8cf6`.** The number reaches the handler table,
because `Tracee::handle_syscall_entry` now uses its `handler` argument and no longer ignores it.
`SyscallHandler` gained `pub fn register`, and `tests/native_dispatch.rs` asserts both directions. A handler
registered for `SYS_getpid` runs. A handler registered for `SYS_getppid` does not run, while `syscall_info`
still records `SYS_getpid`. That separates a broken dispatch from a broken read. `SyscallHandlerFn` requires
`Send + Sync`, so the flag is an `Arc<AtomicBool>`.

**Step 3 - re-scoped by measurement, not by plan.** `ProotEngine::run()` at lib.rs:173-184 installs the
seccomp filter and calls `ptrace_ctrl.run_loop(...)` with **no spawn and no `TRACEME` child**, so it can
never see a tracee. The code that builds a command line is `ProotEngine::start()` at :186 onward, and it
spawns the **external** `proot` binary with `--kill-on-exit --link2symlink --sysvipc -L
--change-id=0:0:0 --kernel-release ... --rootfs=.`. So production runs external proot, and the internal
tracer has no child of its own. "Run containers without the proot binary" is therefore a subsystem with
three parts. Part one is a spawn path: `fork` -> `PTRACE_TRACEME` -> `execve`. Part two is emulation of
those six argv features, and part three is `execve` interception with path translation, which is the hardest.

**Next increment `p4-3a`, smallest honest unit.** Add a spawn+trace entry point that forks, calls
`TRACEME` and execs a command **as given**. It drives the loop over that command, and it returns the exit
status together with the syscall numbers it saw and did not translate. It refuses **by name**, and it
does not pretend to have run the workload. Prove it with `/bin/true`, which exits 0 and shows at least
one syscall. Add a control, `/nonexistent`, which must report the exec failure and not a zero exit. Do
**not** swap the production callers before parity is measured on the same workload, namely
`native/image.rs:130-131`, `proot-core/src/lib.rs:194-195` and `:336-337`. An early swap turns `podnext run` into a regression.
