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
