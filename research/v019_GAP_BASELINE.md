# v0.1.9 section 9 Gap Baseline: Measured, Not Diagnosed

**Date**: 5 Nov 2026 WIB
**Binary**: `target/debug/podnext`, `podnext 0.1.8`, the workspace build, built 5 Nov 13:58.
`$PREFIX/bin/podnext` reports `0.1.7` and is **stale**. It is not the subject of any measurement here.
**Raw captures**: `$PREFIX/tmp/pdn-evidence/` holds `summary.json` and one `.txt` file for each command's `--help`.
**Head at measurement**: `5b479d1`, after the withdrawal of the v0.1.9 tag.

Every statement below is the output of a command that was run. Where a claim was *not* verified, it
says so. Nothing here is inferred from reading code alone.

---

## 1. section 9.1 Stability and honesty

### 1.1 Command labelling: unmet

```
$ ./target/debug/podnext --help      # 38 top-level commands
$ grep -cE '\[(stable|experimental)\]' crates/podnext/src/main.rs   -> 0
```

The help listing uses `[Core]`, `[Container]`, `[System]`, `[Advanced]` tags instead. Roadmap section 9.1
requires every command labelled `stable` / `experimental` in `--help` **and** in the docs:
**0 of 38 commands satisfy it.**

### 1.2 cli-reference contract: command existence holds, option tables do not

```
$ # extract every '### `podnext …`' heading from docs/src/cli-reference.md and run '<cmd> --help'
documented commands: 33   |   non-zero exit: none
```

So the *command* surface in the reference is real. The option tables are not:

```
$ ./target/debug/podnext version --help | head -10
EXAMPLES:
  podnext version
  podnext version --format json            <- real flag is -f/--format

$ ./target/debug/podnext version --short
error: unexpected argument '--short' found
$ ./target/debug/podnext version --json
error: unexpected argument '--json' found
```

`docs/src/cli-reference.md:543-544` documents `--short` and `--json` as the options of `podnext
version`. Both are rejected by the CLI.

### 1.3 Exit-code contract: the docs still document codes that do not exist

`docs/src/cli-reference.md` "## Exit Codes" currently lists **0 to 12**, including
`3 Container not found`, `4 Image not found`, … `12 Internal error`. Observed behaviour:

| case | command | rc |
|---|---|---|
| unknown subcommand | `podnext definitely-not-a-command` | 2 |
| missing required argument | `podnext login` | 2 |
| container not found | `podnext login no-such-container-xyz -- true` | 1 (`Error: Container '…' not found`) |
| image not found | `podnext image inspect no-such-image-xyz` | 1 |
| invalid volume name | `podnext volume rm ../escape` | 1 |
| success | `podnext version` | 0 |

`crates/podnext/tests/automation_contract.rs` asserts exactly 0 / 1 / 2. Codes 3 to 12 are neither
produced nor tested. The document claims them. This is the class the earlier round described as
"fictitious exit codes 3 to 12 removed". The table is still present at `5b479d1`.

### 1.4 Environment variables: 8 documented, 0 read

```
$ for v in PN_CONFIG_DIR PN_DATA_DIR PN_CACHE_DIR PN_LOG_LEVEL PN_GATEWAY_ADDR \
           PN_RUNTIME PN_SECURITY_PROFILE PN_NO_COLOR; do
      grep -rn "$v" crates/*/src --include=*.rs | wc -l ; done
0 0 0 0 0 0 0 0
$ grep -rn "env::vars()\|getenv" crates/*/src --include=*.rs   -> no matches
```

None of the eight appears anywhere in `crates/`, and nothing enumerates the environment dynamically
either. Variables actually read: `PN_DEBUG`, `PN_NEXT_DEBUG`, `PN_NEXT_SERVICES_DIR`, `PN_SHM_DIR`,
`PDN_BACKUP_DIR`, `PDN_CONTAINERS_DIR`, `PDN_GITOPS_DIR`, `PN_TEST_ROOT`, `PROOT_BIN`, `RUNTIME_DIR`,
`TMPDIR`, `PREFIX`, `HOME`, `PATH`, `USERPROFILE`, `UML4TERMUX_RUNTIME`. The reference does
not document any of these variables.

---

## 2. section 9.2 Simplicity and performance

### 2.1 File size target: unmet

```
crates/podnext/src/main.rs            4472 lines
crates/podnext/src/commands/mod.rs    3699 lines
crates/podnext/src/native.rs          1015 lines
```

Roadmap target: no CLI file over ~800 lines.

### 2.2 Metadata latency: 3.4 times the target

```
median of 5 runs, workspace binary:
  list -a     min 29.9 ms   median 34.3 ms   max 43.7 ms      target: p50 < 10 ms
  image ls    min 282.9 ms  median 311.8 ms  max 2232.3 ms    (roadmap baseline: 28.6 ms)
```

### 2.3 `image ls` SIZE: usually real, silently meaningless when `du` fails

```
$ ./target/debug/podnext image ls
| debian | latest | deb-pdn   | 2026-11-03 06:36 | 3.4 KB  |
| alpine | latest | pdn-bench | 2026-11-02 19:37 | 8.3 MB  |
| ubuntu | 22.04  | ubuntu    | 2026-11-03 22:08 | 70.0 MB |

recursive size of each rootfs (os.walk over every file):
  deb-pdn     402.1 MB      pdn-bench   8.3 MB      ubuntu   70.0 MB
```

So two rows match the real size and one does not. The mechanism is proven, not guessed. The row
comes from `du_size_bytes()` at `main.rs:3019`, invoked at `main.rs:3080`. That call runs `du -sb` and
uses the fallback `path.metadata().len()` unless the process exits 0:

```
$ du -sb ~/.podnext/containers/deb-pdn/rootfs ; echo rc=$?
421675061  /data/.../containers/deb-pdn/rootfs
du: cannot read directory '.../rootfs/apex': Permission denied
du: cannot read directory '.../rootfs/odm': Permission denied
rc=1
$ stat -c '%s bytes' ~/.podnext/containers/deb-pdn/rootfs
3452 bytes                      <- printed as "3.4 KB"
```

The container carries Android system directories inside its rootfs that an app uid cannot read, `du`
exits 1, and the column silently becomes the directory inode size. Nothing marks it as unknown.
`image ls` also labels the container name as **IMAGE ID**. The value `image_id: c.name.clone()` is at
`main.rs:3084`.

### 2.4 Hard-coded Termux paths: 11 occurrences

```
crates/podnext/src/commands/mod.rs:76   const DAEMON_SOCKET = "/data/data/com.termux/files/home/.local/run/pn-next/daemon.sock"
crates/podnext/src/commands/mod.rs:615  "/data/data/com.termux/files/home/.config/pn-next/services"
crates/podnext/src/daemon.rs:86,93,100,107   clap default_value for socket / services / runtime / log dirs
crates/proot-distro-core/src/constants.rs:76,77,78   PREFIX / HOME / TMP constants
crates/proot-distro-core/src/runtime.rs:212,403      uml4termux daemon socket and console log
```

Counter-evidence to the roadmap's framing: the relocation of `HOME` **does** move the container store.
`HOME=$PREFIX/tmp/pdn-fake-env/home PREFIX=$PREFIX/tmp/pdn-fake-env/prefix podnext list` returned an
empty table and mentioned the real home nowhere. `TERMUX__HOME`, `TERMUX__PREFIX`,
`TERMUX_APP__PACKAGE_NAME` and `XDG_*` have **0 occurrences** in `crates/*/src`, so the roadmap's
acceptance criterion is untested and unimplemented. It requires the suite to pass with `HOME` and
`PREFIX` relocated through those variables. The paths above are not a single uniform defect. The daemon
defaults are overridable clap values, and `constants.rs` holds fallbacks.

---

## 3. section 9.3 The interactive shell

### 3.1 `--bind` / `--volume` are accepted and silently do nothing

```
$ echo MARKER-OK > $PREFIX/tmp/bindtest/marker.txt
$ podnext run test-alpine --bind $PREFIX/tmp/bindtest:/probe -- sh -c 'ls -l /probe; cat /probe/marker.txt'
guest sees:
ls: /probe: No such file or directory
cat: can't open '/probe/marker.txt': No such file or directory        rc=0

$ podnext run test-alpine --volume $PREFIX/tmp/bindtest:/probe -- sh -c '...'   -> identical output
$ podnext run test-alpine -- sh -c '...'                                        -> identical output
```

The host path exists and is readable. `ls -l` confirms this result and shows 10 bytes. Three identical
results including the no-flag control mean the flag has no effect at run time and reports success.
`run --help` does list both `--volume <HOST_PATH:CONTAINER_PATH[:ro]>` and `-b, --bind <HOST_PATH:GUEST_PATH>`, so
this is a runtime plumbing gap, not a parsing one.

### 3.2 The documented short alias is a different flag

```
$ podnext run ubuntu -v /sdcard:/mnt/sdcard -- ls /mnt/sdcard      # docs/src/cli-reference.md:118
error: unexpected argument '/sdcard:/mnt/sdcard' found        rc=2
$ podnext run --help | grep -E '^\s+-v|--verbose'
      -v, --verbose...
```

`-v` is `--verbose`. The documented volume example cannot run.

### 3.3 `podnext doctor`: absent

```
$ grep -rn "doctor" crates/ --include=*.rs     -> no matches
```

### 3.4 Not measured yet

These items are not measured yet: the `--detach` round trip, the `SIGWINCH`/`PS1`/`TERM` fidelity inside the
guest, the `run … -- <cmd>` quoting table, and a soak run for zombies. The `-d, --detach` option exists on `run`.
`podnext sessions` currently returns an empty table. These are open measurements, not findings.

---

## 4. Release-checklist artefacts

```
docs/src/known-limitations.md   absent
CHANGELOG.md                    absent (the release workflow generates one from `git log` at publish time)
README.md "What's New" / version claim   no match
manual smoke test (install alpine -> run -> login -> remove)   not run
```

---

## 5. Consequences for the plan

1. section 9.1 is not "partially done": labelling, the `version` option table, the exit-code table and the
   environment-variable table are all measurable defects at `5b479d1`.
2. section 9.2's SIZE item is narrower and sharper than "the number is fake": the fallback must be an
   explicit `unknown` value, or a size that tolerates unreadable subdirectories, for example `du` without the
   hard `success()` requirement, plus a marker.
3. section 9.3's mount flags are the highest-value shell fix found so far, and they are verifiable end to
   end with the test above. A bind that works turns the three identical outputs into a visible
   marker file.
4. The `commands/*.rs`-style split from section 9.2 and the `TERMUX__*`/`XDG_*` work are large and
   mechanical. They must start only after the measurable honesty items above, so that the same
   measurement script can prove progress.
