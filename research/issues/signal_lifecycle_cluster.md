## What is wrong

PDN does not manage its own process lifecycle signals, so a stop of a PDN process leaves the guest active. Three separate defects are present, and all three are confirmed when the code runs.

**1. `podnext run` installs no signal handler at all.** The only handlers in the crate were the D-Bus broker's Ctrl-C and the daemon's. The run path had none. Reproduction:

```
$ podnext run alpine -- sleep 300     # then: kill -TERM <podnext pid>
-- proot survivors --                 # BEFORE
 8040     1  8040 /data/data/com.termux/files/usr/bin/proot   <- reparented to PID 1
-- session file --
-rw------- 8030.json                 <- still there, never cleaned
exit=143
```

**2. `container stop()` signalled one pid and mistook a zombie for a live process.**

```rust
unsafe { libc::kill(pid as i32, libc::SIGTERM) };
while start.elapsed().as_secs() < timeout {
    if unsafe { libc::kill(pid as i32, 0) } != 0 { break; }   // kill(pid,0) == 0 for a ZOMBIE
    tokio::time::sleep(Duration::from_millis(100)).await;
}
if unsafe { libc::kill(pid as i32, 0) } == 0 {
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };         // SIGKILLs the corpse
}
container.status = ContainerStatus::Stopped;                  // asserted, not verified
```

`kill(pid, 0)` returns success for a zombie, so the loop can never exit early for an exited-but-unreaped process: it burns the whole timeout, then signals a corpse. Measured with a 1 s timeout: old predicate took 1.0023 s, the new one 14.2 ms.

**3. The status was asserted, not observed.** The code wrote `Stopped` even if the process outlived SIGKILL. A caller was told a container had stopped when it had not.

**4. `podnext-daemon` handled Ctrl-C only. That is SIGINT.** A supervisor sends SIGTERM. Examples are `sv stop`, systemd, `kill <pid>`, and a container stop. The daemon let SIGTERM kill it by default disposition:

```
== OLD / TERM ==   exit_status=143   socket_after=REMAINS
== OLD / INT  ==   exit_status=0     socket_after=removed
```

It also called `std::process::exit(0)` from a spawned task, and that skips every `Drop`.

**5. Spawned proot had no process group.** The code called `Command::new(&proot_bin)` with no `process_group(0)`, so a group signal was not even possible.

## Fix

- In both spawn sites of `proot-core/src/lib.rs`, add `process_group(0)`. Then pgid == child pid, so the pid the code already returned is the group id.
- `container.rs`: a shared `terminate_group(pgid, timeout)` that SIGTERMs the **group** and escalates to SIGKILL **on the group**. It returns `Err` if the group survives. Liveness uses `waitpid` with WNOHANG plus `/proc/<pid>/stat`, and it skips state `Z`. A zombie no longer reads as alive. Both `stop()` and `kill()` use it, and both set `ContainerStatus::Error`. Both propagate the error instead of a claim of `Stopped`.
- In `native.rs`, where the run path actually lives, `podnext run` installs SIGTERM/SIGINT handlers before it awaits the guest. It forwards the signal to the guest's process group, and it awaits the execution future so the child is reaped once. It removes its session record on every exit path, and it returns `128+N`. That is 143 for SIGTERM and 130 for SIGINT.
- `daemon.rs`: eager SIGTERM/SIGINT/SIGHUP handlers, and it ignores SIGHUP because no reload path exists. The run function owns shutdown, not a spawned task. The accept loop `select!`s on it, and in-flight connections drain under a 10 s grace. It removes the socket and returns `Ok(())`, so `Drop` runs and the exit status means something.

## Evidence

```
AFTER: -- proot survivors -- (none: clean)
       -- session file --  ls: cannot access '14332.json': No such file or directory
       exit=143
SIGINT variant: survivors none, exit=130
normal: podnext run alpine -- true         -> 0
        podnext run alpine -- sh -c 'exit 7' -> 7
group kill: BEFORE 3 procs in group (leader + 2 sleeps) -> AFTER survivors in group: 0
FIXED daemon: TERM -> exit 0, socket removed;  INT -> exit 0, socket removed
```

Tests added, all asserting observable state rather than durations:
`kill_leaves_no_surviving_descendant`, `zombie_is_not_reported_alive`, `forward_to_guest_terminates_child_group`, `sigterm_and_sigint_shutdown_sighup_is_ignored`, `graceful_shutdown_removes_socket_and_returns_ok`.

Refs: commits that fix the run path, the container stop path, and the daemon.
