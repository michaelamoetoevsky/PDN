## Verdict: not a PDN defect

Concurrent unsynchronised writers to one file are last-writer-wins by definition. Each `>` open
truncates with `O_TRUNC`. Thus the content of one writer survives and the rest is lost. No ordering
guarantee exists. That behaviour is POSIX, and this issue measured that behaviour.

## The control that settles it

The same worker shape with **zero PDN involved** reproduces the identical signature in **5 of 5
runs**. The run used plain host processes. It used no `podnext`, no proot and no container
anywhere in the path:

```
--- control run 1 (final='token_4', lines=1) ---
Worker 1: expected token_1, read 'token_4', clobbered=True
Worker 2: expected token_2, read 'token_4', clobbered=True
Worker 3: expected token_3, read 'token_4', clobbered=True
Worker 4: expected token_4, read 'token_4', clobbered=False
Worker 5: expected token_5, read 'token_4', clobbered=True
Total clobbered: 4/5
... runs 2-5 identical: 4/5 clobbered, file ends with exactly one line
```

The same shape through `podnext run` on one container, 5 runs, is **also 4/5 clobbered every
time**. The result is no worse and no different.

## No PDN-level amplification

The check covered a staging file, a lock, a copy-up and an `O_TRUNC` on the behalf of the
guest:

1. **The guest file IS the host file.** After the run, `/shared_race.txt` is a plain file in the
   container rootfs directory, `~/.podnext/containers/<name>/rootfs/shared_race.txt`. That
   directory is ordinary and not an overlay.
2. **The run path takes no lock.** `run_container_native` -> `ProotBackend::execute()` takes no
   `ContainerLock`. The only `flock` in the path is in `register_session`. That lock covers a
   per-pid session JSON and is unrelated to guest files.
3. **`ContainerLock` is dead code.** Only its own unit tests reference it, and no production
   caller does. Thus the premise of the issue, that container-level locking exists, is false.
   But its absence changes nothing here.
4. **No staging or copy-up occurs on write.** The engine execs the system proot. The guest
   `write()` and `open(O_TRUNC)` reach the host kernel unchanged through the translated fd.

## Consequence for the docs

`docs/src/troubleshooting.md` now states the contract. Concurrent `podnext run` and `login`
commands against one container share a single mutable filesystem. That filesystem has no ordering
guarantee and no atomicity guarantee. PDN does no write serialisation, no staging and
no copy-up. The document cites the control experiment. Thus no user can file this as a container
bug again.

The project explicitly does NOT add a per-container execution lock. A lock on the container is
not a lock on the file that the workload uses. Serialising unrelated workloads to "fix" a data
race a lock cannot fix is worse than documenting the semantics.

The project closes the issue as not planned. The cleanup below remains.

## Minor cleanup left open

`crates/proot-distro-core/src/locking.rs` `ContainerLock` has no production caller. Delete it,
or add a comment that it guards nothing in the run path. Its mere existence allowed the
assumption "locking exists so we are safe".
