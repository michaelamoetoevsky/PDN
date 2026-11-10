# The ARM64 syscall-gate trap: carry policy in x9, not x7

Read this before you write a ptrace or seccomp dispatch gate for arm64. This book records the trap here. The trap is invisible until a register value comes back wrong. PDN does not have the gate yet. The first implementer who adds one will repeat the trap.

## The trap

On arm64, the kernel delivers a syscall stop to a tracer. The stop is a `SECCOMP_RET_TRACE` stop or a `ptrace` stop at `PTRACE_SYSCALL`. Through the stop, the tracer reads and writes the registers of the tracee. The tracer can store its own state in a register. The state is a dispatch policy, a per-category cookie, or the retained descriptor kind. If the tracer stores the state in `x7` and reads it again at the next stop, the value is gone. **The kernel restores x7** as part of the ARM64 syscall-stop path. The restore overwrites what the tracer put there.

The evidence is the register note from shroot. This tree dissects the note at `research/DISSECTION_SHROOT.md:45-47`. The note repeats at `research/R2-EXECUTION.md:375-378`. Policy and the retained typed-descriptor kind travel in **x9**, not x7. The kernel restores x7 after an ARM64 syscall stop. Shroot records the cause in `interception.md:27-28`. Shroot cites the kernel at `arch/arm64/kernel/ptrace.c#L1694-L1726`. The same note records that x10 can carry a leased input-buffer address. Shroot records that fact in `interception.md:36`. Thus x10 is not a general-purpose slot either.

## Why x7 looks right, and is not

- `x0` through `x7` are the argument registers. `x8` holds the syscall number. `x7` is the last argument register. The common syscall takes seven arguments or fewer. The call itself does not use `x7`. Thus `x7` is a tempting place to keep our value across the stop.
- It is not free. The kernel's ptrace path rewrites `x7` across an ARM64 syscall stop. The citation above records that fact. Thus a value the tracer writes into `x7` is not the value it reads back. `x10` is spoken for in the same design. The syscall mechanism does not repurpose some registers. The policy belongs in those registers.
- **Remedy:** carry policy and any cookie in **`x9`**. `x9` is not a syscall-argument register. The kernel preserves `x9` across the stop. Shroot does exactly this. Use `x10` only for the input-buffer address that it already carries. Never use `x10` for policy.

## What it means for PDN

- PDN has no ptrace or seccomp-stop handling to test the trap. I measured this tree. `grep -rn 'x7\|x9' crates/proot-core/src/` returns nothing. The tree has no register policy, no dispatch gate, and no cookie.
- `crates/proot-core/src/seccomp.rs` today **installs a real BPF filter**. It does not refuse. `SeccompFilter::install` sets `PR_SET_NO_NEW_PRIVS`. It loads a deny-list program through `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...)`. The deny-list program contains `open_by_handle_at`, `kexec_load`, and `SECCOMP_RET_ALLOW` for everything else. The code is at `crates/proot-core/src/seccomp.rs:59-89`, with the builder at `:111-133`. `crates/proot-core/src/lib.rs:173,187,322` calls it. `ProotConfig.seccomp` defaults to `true` at `crates/proot-core/src/cli.rs:86`. A deny-list `SECCOMP_RET_ALLOW` filter needs no tracer and touches no register. It is a filter, not a gate.
- Thus the trap cannot bite PDN today. There is no `SECCOMP_RET_TRACE` path. No tracer stores state in a register. The trap becomes live when a proot-free backend is built. Any `SECCOMP_RET_TRACE` dispatch gate also activates the trap. That implementer must put policy in x9, not x7. The user-facing `--seccomp` flag still refuses before dispatch. That issue is #62 in `crates/podnext/src/direct.rs`. The refusal is about a *proot/distro-tailored* filter. This proot build does not offer that filter. The refusal is independent of the process-level deny-list that `SeccompFilter` installs.
