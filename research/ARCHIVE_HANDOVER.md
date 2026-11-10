# Archive handover

Date: 2026-11-10. Repository: `PDN`, command `podnext`. Branch: `master`. HEAD at archive: `9ab8ff2`. Tag for
this state: `unstable`.

The owner stopped the project on this date. This document records the last activity, the changes it made, and the
work that a successor must do. The project is archived. No further development is planned.

## 1. Status at archive

| Item | Value |
| --- | --- |
| Version in `Cargo.toml` | `0.2.0` |
| Last release tag | `v0.2.0` |
| Archive tag | `unstable` |
| Default engine | `native` |
| Engine parity battery | 30 cases: 28 EQUAL, 2 DIFFERENT, 0 BLOCKED |
| Last verified CI run | `d25573a`, 15 of 15 checks success |
| Open measured defects | 2 |

The installed binary on this device is `/data/data/com.termux/files/usr/bin/podnext`. It reports version `0.1.7`
and its date is 4 November. It runs the external `proot` binary. A `ps -A` command inside `podnext login ubuntu`
shows a `proot` process. The work in this repository therefore did not reach the command that the owner used.

The repository build `target/release/podnext` reports version `0.2.0`. It starts the in-tree engine. The guest
process is `ld-linux-aarch64`, and no `proot` process exists.

This gap is the most important fact in this document. The source advanced. The installed tool did not.

## 2. What the last activity was

The last activity was an engine parity campaign. The goal was one engine in the tree that runs a container the
way `proot` does. The method had three parts:

1. Build all binaries. Do not treat a green `cargo` run as proof of correct behaviour.
2. Run the real binary on a real Debian rootfs, with the same command in both engines.
3. Record the verdict of each case from the raw output. Mark a case EQUAL, DIFFERENT, BLOCKED or SUSPECT.

The campaign covers 56 commits, from `baef0a3` to `9ab8ff2`. It changed 25 files, with 4,082 insertions and 329
deletions.

## 3. Files that carry the engine

| File | Lines | Function |
| --- | --- | --- |
| `crates/proot-core/src/native_run.rs` | 3,092 | The engine: trace loop, path translation, execve substitution |
| `crates/proot-core/tests/native_exec_translation.rs` | 2,168 | Unit tests for translation and for the refusal rules |
| `crates/proot-core/tests/native_dynamic_guest.rs` | 427 | A dynamic guest with a missing `PT_INTERP` |
| `scripts/e2e_parity_battery.sh` | 173 | 30 cases, both engines, one verdict per case |
| `scripts/engine_differential.py` | 345 | One workload, two arms, compared field by field |
| `scripts/own_engine_acceptance.py` | 225 | Proves that no `proot` binary runs, with a decoy arm |
| `research/architecture/CHANGE_LOG.md` | 1,675 added | The evidence record for each decision and each reversal |

The last 12 commits changed two files only. `native_run.rs` received 28 insertions and 43 deletions. The change log
received 254 insertions.

## 4. Defects closed, each with its measurement

| Commit | Defect | Measurement that proved the fix |
| --- | --- | --- |
| `3f858c3` | The first syscall of a fork child was labelled as an exit | `dpkg-query -W` counted 0 packages, then 78. `proot` counts 78 |
| `3f858c3` | The bootstrap rule consumed the birth stop | The same command, with an entry stop and an exit stop in the trace |
| `246d027` | Path translation ran at exit stops, where the register holds a result | `unlinkat` and `utimensat` stopped being refused |
| `9a5787a` | A NULL or empty path was treated as a failed translation | `futimens` stopped failing |
| `fd1466d` | A descriptor target was matched by one spelling only | Every spelling of the rootfs path is compared |
| `56aa7f7` | A descriptor target was not resolved against the bind sources | `rm -rf` on a directory tree |
| `af56dfa` | The descriptor branch received a host path | `rm -rf` of a directory with content: the directory is gone |
| `a4da56c` | `PWD` in the guest showed the host rootfs path | `PWD=/`, `pwd` prints `/`, `cd /etc` prints `/etc` |
| `364922e` | Guest signals were swallowed | `kill -TERM $$` gives rc 143 in both engines |
| `65237ae` | The per-tracee cwd stored a host path | The guest sees its own directory form |
| `244da09` | A guest hardlink failed where the kernel refuses it | `--link2symlink` creates a symlink and reports success |
| `ee99ce4` | `st_uid` and `st_gid` were not mapped | The stat result carries the mapped identity |
| `292a4c1` | The two-path calls were not translated | `mv` works across the two arguments |
| `e4a617b` | `chdir` was not translated and the cwd was not tracked | `cd` inside the guest works |

## 5. Defects open, with the measured mechanism

### 5.1 `mkdir -p` loses a nested component

The command `mkdir -p /tmp/v1/a` gives different results in the two engines:

```text
native: /tmp/a exists, /tmp/v1/a does not exist
proot : /tmp/v1/a exists, /tmp/a does not exist
```

The engine joins the relative name `a` onto `/tmp`. The guest means `/tmp/v1`.

The mechanism is measured. One line per `chdir` stop shows the state that the branch decision reads:

```text
CH4 pid=22923 child=true  entry=true  execve_handled=false bootstrap=true    <- engine's own child
CH4 pid=22927 child=false entry=true  execve_handled=true  bootstrap=false   <- mkdir child
```

The engine's own child reports `execve_handled=false` at both of its `chdir` stops. The bootstrap exemption
therefore consumes both stops. The stored cwd never moves. The processes that the child starts inherit the wrong
directory.

The flag is written in three places. They are the execve entry stop, the remote execve translation, and the
`PTRACE_EVENT_EXEC` arm. None of them runs for that process. The next probe is therefore one line: does any
`PTRACE_EVENT_EXEC` reach the loop, and for which pid.

The change in `6b11aa1` sets the flag at the exec event. That change did not fix the case. The battery stayed at 28
EQUAL and 2 DIFFERENT, so the change is harmless and insufficient.

### 5.2 `wait` does not return a background job

The command `sh -c 'echo start; (echo bg) & wait; echo waited'` gives:

```text
native: start | bg-done |            the word waited is missing
proot : start | bg-done | waited
```

`b85cda2` records the status of each reaped descendant and replays the `wait4` syscall. The case still fails. The
replay path needs a measurement of its own.

## 6. Work that remains

The order below follows the priority recorded in the workbench.

1. Install the build. The owner's command uses version `0.1.7`. Copy the release build over it, or the campaign
   stays invisible. Keep a backup of the old binary.
2. Close the exec-event question. Measure which pid receives `PTRACE_EVENT_EXEC`. Then close `mkdir -p`.
3. Close `wait4`. Add the background-job case to the battery. Prove that both engines agree.
4. Reach feature parity on the proot argv surface. Five flags are refused by name today: `--kill-on-exit`,
   `--sysvipc`, `-L`, `--change-id`, and the `--rootfs` form with `-b`.
5. Model the identity semantics. `getresuid`, `getresgid` and `getgroups` are not mapped. The `setuid` effect in the
   guest is not modelled.
6. Extend the process model. Attach to a tracee that is not the result of a fork is not implemented. Clone with
   namespace flags is not implemented.
7. Choose a policy for the guest environment. The engine publishes a selected environment today.
8. Find the mechanism of the `/` listing divergence. `proot` adds host directories to `ls /`. Three candidate
   causes failed to verify.
9. Trace the session and PTY paths. A detached session fails on a fixture store. The failure also occurs on the
   proot path, so the engine is not the cause.
10. Work the remaining assessment items. The workbench holds 24 INFO items and one HIGH item about test fixtures
    that can touch the operator store.
11. Put the documentation gate into the local gate and into CI. `scripts/check_ste.py` exists and measures the
    documentation style.

## 7. How to reproduce the measurements

```sh
export HOME=/data/data/com.termux/files/home
cd $HOME/workspace/PDN
unset CARGO_TARGET_DIR TMPDIR PN_TEST_ROOT PROOT_BIN LD_LIBRARY_PATH
env -u LD_LIBRARY_PATH cargo build --release -p podnext --bin podnext
sh scripts/e2e_parity_battery.sh --store "$(cat $HOME/.hermes/cache/scratch/e2e-fixture.txt)" \
   --container deb-renamed --bin target/release/podnext
```

The battery prints one block for each case, then a summary. The raw output of every case stays in the work
directory that the summary names.

## 8. What this project is not

The engine is not a complete replacement for `proot`. Two ordinary guest workloads still differ. The list in
section 6 names the rest. A reader must not read the sentence "the default engine is native" as "parity is
complete".

The documentation in this repository was rewritten to ASD-STE100 Simplified Technical English at archive time. The
gate `scripts/check_ste.py` measures the mechanical part of that standard: sentence length, verb form, punctuation
and modal verbs. The vocabulary part of the standard is not machine-checked. This document does not claim it.