## Session files accumulate for every run that ever exited

`~/.podnext/sessions/<pid>.json` is written by `register_session` and removed only by the lazy prune inside `active_sessions`, which runs when someone *queries* sessions. A run that exits normally never triggers it. Measured on this device:

```
$ ls -1 ~/.podnext/sessions/ | wc -l
46
$ for f in ~/.podnext/sessions/*.json; do p=$(basename $f .json); kill -0 $p && echo ALIVE || echo DEAD; done | sort | uniq -c
  46 DEAD
```

Forty-six files, zero live processes. Two of the three pruning assertions in `session.rs` were inside `#[cfg(test)]` only, so the suite was unable to catch this.

`register_session` now returns a `SessionGuard`, and that fixes it. The guard owns the locked file and unlinks it on `Drop`. It checks dev+ino first, so a reused pid cannot make us delete another process's record. The lazy prune stays as a backstop for SIGKILL, where no `Drop` runs.

Test: `session_file_removed_on_drop` asserts the observable count 1 -> 0.

## Service logs grow without bound

`StdioConfig.stdout`/`stderr` were opened `append`. `LogRotationConfig` was parsed but had **zero consumers**, a configuration surface that silently did nothing. This is the same class as the gateway fail-safe config in #27. The fix rotates at service start with a size threshold, and it keeps at most `max_files` copies. An honest ceiling is noted in code: rotation happens at open time. A single long-running service can therefore still grow its current file between restarts. A size-triggered rotation while the service runs needs a writer-side hook.

Test: `test_log_rotation_caps_file_count` runs 10 fill+rotate rounds and asserts the base path is gone and the copy count stays within `max_files`.

## Audited and found NOT to be leaks

Recorded so nobody re-investigates:

- `supervisor.rs` `services` HashMap: insert at load or create, remove at delete. Both operations are paired, and the map is bounded by the number of service TOML files.
- `supervisor.rs` `dependency_graph`: the code replaces it wholesale on rebuild, and it is not append-only.
- `supervisor.rs` `health_task`: the code takes it and aborts it on stop.
- `supervisor.rs` `event_tx`: a fixed-capacity 1024 broadcast ring.
- `watchdog` `states`: register and unregister are paired. `watchdog` `restart_timestamps`: the code retains it to the window on every `handle_failure` before the push.
- No `fs::read`/`read_to_end`/`Vec::with_capacity` sized from an untrusted length field in the audited files. The two `read_to_string` calls read our own <300 B record and a local operator-controlled TOML.
