# The ARM64 syscall-gate trap: carry policy in x9, not x7

Read this before writing a ptrace or seccomp dispatch gate for arm64. It is written down here, in the
book, because the trap is invisible until a register value you wrote comes back wrong, and PDN does not
have the gate yet — the first implementer to add one is the one who would repeat it.

## The trap

On arm64, when a syscall stop is delivered to a tracer (a `SECCOMP_RET_TRACE` stop, or a `ptrace` stop at
`PTRACE_SYSCALL`), the tracer reads and writes the tracee's registers through the stop. If the tracer
stashes its own state — a dispatch policy, a per-category cookie, the retained descriptor kind — in
`x7` and reads it back on the next stop, the value is gone: **the kernel restores x7** as part of the
ARM64 syscall-stop path, overwriting what the tracer put there.

The evidence is shroot's own register note, dissected in this tree
(`research/DISSECTION_SHROOT.md:45-47`) and repeated in `research/R2-EXECUTION.md:375-378`: policy and
the retained typed-descriptor kind travel in **x9**, not x7, because the kernel restores x7 after an
ARM64 syscall stop (shroot's `interception.md:27-28`, citing the kernel at
`arch/arm64/kernel/ptrace.c#L1694-L1726`). The same note records that x10 can carry a leased
input-buffer address (`interception.md:36`), so x10 is not a general-purpose slot either.

## Why x7 looks right, and is not

- `x0`–`x7` are the argument registers and `x8` holds the syscall number. `x7` is the last argument
  register, so for the common syscall that takes seven arguments or fewer it is unused by the call
  itself — a tempting place to keep "our" value across the stop.
- It is not free. The kernel's ptrace path rewrites `x7` across an ARM64 syscall stop (citation above),
  so a value the tracer writes into it is not the value it reads back. `x10` is spoken for in the same
  design. The registers the syscall mechanism does not repurpose are where the policy belongs.
- **Remedy:** carry policy and any cookie in **`x9`**, which is not a syscall-argument register and is
  preserved across the stop. That is exactly what shroot does. Use `x10` only for the input-buffer
  address it already carries, never for policy.

## What it means for PDN

- PDN has no ptrace/seccomp-stop handling to put the trap to the test. Measured on this tree:
  `grep -rn 'x7\|x9' crates/proot-core/src/` returns nothing — no register policy, no dispatch gate,
  no cookie.
- `crates/proot-core/src/seccomp.rs` today **installs a real BPF filter**; it does not refuse.
  `SeccompFilter::install` sets `PR_SET_NO_NEW_PRIVS` and loads a deny-list program
  (`open_by_handle_at`, `kexec_load`, everything else `SECCOMP_RET_ALLOW`) through
  `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...)` (`crates/proot-core/src/seccomp.rs:59-89`, builder
  `:111-133`). It is called from `crates/proot-core/src/lib.rs:173,187,322`, and `ProotConfig.seccomp`
  defaults to `true` (`crates/proot-core/src/cli.rs:86`). A deny-list `SECCOMP_RET_ALLOW` filter needs
  no tracer and touches no register: it is a filter, not a gate.
- So the trap cannot bite PDN today: there is no `SECCOMP_RET_TRACE` path and no tracer stashing state
  in a register. It becomes live the moment a proot-free backend, or any `SECCOMP_RET_TRACE` dispatch
  gate, is built — and that implementer must put policy in x9, not x7. (The user-facing `--seccomp`
  flag still refuses before dispatch, issue #62, `crates/podnext/src/direct.rs`; that refusal is about
  a *proot/distro-tailored* filter, which this proot build does not offer, and is independent of the
  process-level deny-list `SeccompFilter` installs.)
