# CLI Reference

## Command stability

Two labels have one measurable rule. **`[stable]` means at least one automated test in this
repository exercises the command.** **`[experimental]` means no such test exists yet.** A
command that contains a refused path also gets `[experimental]`. `[experimental]` does not mean
the command is broken. `install`, `login` and `run` are the most-used paths in the project. They
sit here because the suite does not drive them yet. Re-derive this table with `podnext --help`.
The file `crates/podnext/tests/cli_reference_contract.rs` fails when the two disagree.

| Command | Label | Why |
|---|---|---|
| `podnext install` | `[experimental]` | no automated test covers it yet |
| `podnext list` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext login` | `[experimental]` | no automated test covers it yet |
| `podnext run` | `[experimental]` | no automated test covers it yet |
| `podnext ps` | `[experimental]` | no automated test covers it yet |
| `podnext kill` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext remove` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext rename` | `[experimental]` | no automated test covers it yet |
| `podnext copy` | `[experimental]` | no automated test covers it yet |
| `podnext reset` | `[experimental]` | no automated test covers it yet |
| `podnext sync` | `[experimental]` | no automated test covers it yet |
| `podnext search` | `[experimental]` | no automated test covers it yet |
| `podnext image` | `[experimental]` | `pull` and `push` refuse: there is no OCI image store before v0.3.0 |
| `podnext sessions` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext attach` | `[experimental]` | the reattach path is covered by `scripts/smoke_release.sh` (which asserts a marker across a reconnect) rather than by the crate's test suite; the no-session refusal is covered by `crates/podnext/tests/stub_refusals.rs` |
| `podnext info` | `[experimental]` | no automated test covers it yet |
| `podnext version` | `[experimental]` | no automated test covers it yet |
| `podnext doctor` | `[stable]` | exercised by `crates/podnext/tests/doctor_contract.rs` |
| `podnext service` | `[experimental]` | `logs` refuses: the supervisor keeps no per-service log |
| `podnext daemon` | `[experimental]` | no automated test covers it yet |
| `podnext watchdog` | `[experimental]` | `events` refuses: the watchdog keeps no event stream |
| `podnext quota` | `[experimental]` | no automated test covers it yet |
| `podnext log` | `[experimental]` | no automated test covers it yet |
| `podnext dbus` | `[experimental]` | `monitor` refuses: there is no signal stream |
| `podnext socket` | `[experimental]` | no automated test covers it yet |
| `podnext boot` | `[experimental]` | no automated test covers it yet |
| `podnext wakelock` | `[experimental]` | no automated test covers it yet |
| `podnext policy` | `[experimental]` | no automated test covers it yet |
| `podnext backup` | `[experimental]` | no automated test covers it yet |
| `podnext build` | `[experimental]` | contains unimplemented subcommands (submit/status/cancel/list/artifacts/logs) |
| `podnext config` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext gitops` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext dashboard` | `[experimental]` | no automated test covers it yet |
| `podnext network` | `[experimental]` | no automated test covers it yet |
| `podnext volume` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext docker` | `[experimental]` | requires the uml4termux kernel runtime and refuses under PRoot |
| `podnext compose` | `[experimental]` | no automated test covers it yet |
| `podnext completion` | `[experimental]` | no automated test covers it yet |

Clap adds `help` itself. `help` carries no label.

## Global Options

```bash
podnext [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] [-- <CMD_ARGS>...]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |
| `--engine <ENGINE>` | | Container engine: `native` (in-tree ptrace engine, the default) or `proot` (host binary, legacy) |
| `--rootfs <PATH>` | `-r` | Use PATH as guest root filesystem |
| `--bind <HOST:GUEST>` | `-b` | Bind mount (repeatable) |
| `--pwd <DIR>` | `-w` | Set working directory |
| `--qemu <BIN>` | `-q` | Execute through QEMU user-mode |
| `--link2symlink` | | Enable hardlink emulation via symlinks. No short form: the reference's `-l` is taken by `daemon logs --lines` |
| `-0, --root-id` | `-0` | Make the guest appear as root. Refused by name: every guest id is already mapped to root through `--change-id=0:0:0` |
| `--no-sysvipc` | | Disable System V IPC emulation. Refused by name on container commands and on the direct path: `proot-core` passes `--sysvipc` unconditionally |
| `--kernel <VER>` | | Override kernel release string |
| `--hostname <NAME>` | | Set hostname |
| `--env <VAR=VALUE>` | `-e` | Set environment variable (repeatable) |

## Container Commands

### `podnext install`

Install a container from one of three sources. The first source is an OCI/Docker image
reference. The second source is a local rootfs archive: a plain tarball or an OCI image-layout
tar. The third source is an http or https URL to that archive.

```bash
podnext install <SOURCE> [OPTIONS]
```

**Arguments:**
- `SOURCE` is one of three forms. Form one is an OCI image reference, for example `ubuntu:22.04` or `docker.io/library/alpine:latest`. Form two is a local archive with explicit path syntax, for example `./rootfs.tar.gz`, `/path/image.oci.tar` or `~/x.tar.xz`. Form three is an `http://` or `https://` URL. A local OCI image-layout tar has every blob verified against the digest that its own manifest declares.

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | Custom container name (defaults to the derived alias) |
| `-f, --force` | Force reinstall if container exists |
| `--checksum <SHA256>` | Verify a local or downloaded archive against `sha256:<hex>` (or bare hex). Refused for a registry reference, which is already content-addressed |

**Examples:**
```bash
podnext install ubuntu:22.04
podnext install alpine:latest --name my-alpine
podnext install docker.io/library/debian:bookworm --force
podnext install ./rootfs.tar.gz
podnext install ./image.oci.tar --name deb
podnext install https://example.org/rootfs.tar.xz --checksum sha256:<hex>
```

### `podnext list`

List created containers. The command accepts `ls` as an alias.

```bash
podnext list [OPTIONS]
podnext ls [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-a, --all` | Show all containers (including stopped, default: true) |
| `--no-all` | Hide stopped containers |
| `--format <FORMAT>` | Output format: table, json, yaml, text |

**Output** captured from `podnext list` on this device. The command prints one such block per
container, separated by the `+---+` rules shown:

```text
+------------------+--------+---------+---------+---------+------------------+
| NAME             | DISTRO | VERSION | ARCH    | STATUS  | CREATED          |
+------------------+--------+---------+---------+---------+------------------+
| alpine           | alpine | 3.24.3  | aarch64 | Stopped | 2026-11-02 19:13 |
+------------------+--------+---------+---------+---------+------------------+
| deb-pdn          | debian | 13      | aarch64 | Stopped | 2026-11-03 06:36 |
+------------------+--------+---------+---------+---------+------------------+
```

The columns are `NAME DISTRO VERSION ARCH STATUS CREATED`. An earlier version of this page showed
`NAME IMAGE STATUS CREATED`, which no code path printed. `--format json|yaml|text` emits the same
fields.

### `podnext login`

Start an interactive shell inside a container.

```bash
podnext login <CONTAINER> [OPTIONS] [-- <SHELL_ARGS>...]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-u, --user <USER>` | Refused: the guest runs as the mapped root; `--user` belongs to the login polygon (v0.3.0) |
| `--isolated` | Run in isolated mode (do not mount host storage or Termux home directory) |

**Examples:**
```bash
podnext login ubuntu
podnext login ubuntu -- /bin/bash
podnext login debian --isolated
```

### `podnext run`

Run a command in a container.

```bash
podnext run <CONTAINER> [OPTIONS] -- <CMD> [ARGS...]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--name <NAME>` | Refused: `run` uses the named container it was given, so a second name would be ignored |
| `--isolated` | Run in isolated mode (do not mount host storage or Termux home directory) |
| `-d, --detach` | Run in the background under a supervised PTY; it prints the supervisor pid and returns. Reconnect with `podnext attach <container>` (Ctrl-P Ctrl-Q detaches again) |
| `--rm` | Refused: removing the container after the run needs ephemeral containers, which PDN does not have yet |
| `--volume <HOST:GUEST>` | Mount a host directory into the container (repeatable). A `:ro` suffix is refused — proot has no read-only bind form, and the suffix would become part of the guest path name |
| `-e, --env <VAR=VALUE>` | Set environment variable (repeatable) |
| `-w, --workdir <DIR>` | Refused: the work dir belongs to the login polygon (v0.3.0) |
| `--network <NETWORK>` | Refused: the run path does not attach networks |
| `-p, --publish <HOST:CONTAINER[/PROTO]>` | Refused: port publishing is not wired into the run path |
| `--entrypoint <ENTRYPOINT>` | Refused: the run path uses the command recorded for the container |
| `-u, --user <USER>` | Refused: the guest runs as the mapped root; `--user` belongs to the login polygon (v0.3.0) |
| `-t, --tty` | Accepted for compatibility: stdio is inherited, so a TTY is used when the caller has one |
| `-i, --interactive` | Accepted for compatibility: stdin is inherited unless the caller redirects it |
| `--seccomp` | Refused: this PRoot build applies no seccomp filter, and the flag used to be accepted and dropped. The proot argv is byte-identical with and without it (0 seccomp tokens; 133 `--bind` entries either way) and `proot --help` lists no seccomp option at all (issue #62). A `security.seccomp_profile` in container metadata is refused for the same reason (issue #60) |
| `--no-kill-on-exit` | Refused: `proot-core` passes `--kill-on-exit` to proot unconditionally, so the proot argv is byte-identical with and without this flag (issue #63). `--kill-on-exit` is the default and cannot be turned off |

A **refused** flag exits 1. The command writes the reason to stderr and does not run the command.
`run` used to accept a flag and then ignore it. A caller cannot tell that result from success.

**Examples:**
```bash
podnext run ubuntu -- apt update
podnext run ubuntu -e "DEBIAN_FRONTEND=noninteractive" -- apt install -y curl
podnext run ubuntu --volume /sdcard:/mnt/sdcard -- ls /mnt/sdcard
podnext run ubuntu --isolated -- echo "isolated run"
```

### `podnext ps`

List active sessions.

```bash
podnext ps [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--format <FORMAT>` | Output format: table, json, yaml, text |

### `podnext sessions`

List active sessions with better formatting.

```bash
podnext sessions [OPTIONS]
```

### `podnext attach`

Reconnect to a running detached session. Start that session with
`podnext run --detach <container>`. `attach` never starts a session. When no session runs for
the container, `attach` exits 1 and names the command that starts one.

```bash
podnext attach <CONTAINER> [OPTIONS]
```

The argument is the **container name**, not a session id. Inside the session, **Ctrl-P Ctrl-Q**
detaches again and leaves the guest running. The same guest shell answers the next `attach`.

```sh
$ podnext run --detach alpine -- sh -c 'echo hello-detached; sleep 6'
4157
$ cat ~/.podnext/sessions/4157.log
hello-detached
$ podnext attach no-such-container
Error: No running session for container 'no-such-container'. Start one with `podnext run --detach no-such-container -- <command>`.
```

`run --detach` prints the supervisor pid and returns. The guest output is appended to
`~/.podnext/sessions/<pid>.log`. `attach <container>` reconnects to that session. **Ctrl-P
Ctrl-Q** detaches again and leaves the guest running. Measurement in #57, not re-run here: two
`attach` connections are answered by the same guest shell pid. A marker written while attached
survives a detach. No `podnext` or `proot` process is left after the guest exits.

The current supervisor has two **declared simplifications**. The supervisor does not report them
as defects. First, the supervisor does not re-forward a resize of the attaching terminal while
attached. Second, the supervisor appends output produced while detached to the session log
`~/.podnext/sessions/<pid>.log`. The supervisor does not replay that output on reconnect.

### `podnext kill`

Stop a container.

```bash
podnext kill <CONTAINER>
```

There are no `--signal` or `--timeout` options. The teardown sequence is fixed. The sequence sends
SIGQUIT to the traced root. Then it scans `/proc/*/fd` for inherited session inodes.
`known-limitations.md` lists signal selection.

### `podnext remove`

Delete a container. The command accepts `rm` as an alias.

```bash
podnext remove <CONTAINER> [OPTIONS]
podnext rm <CONTAINER> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--force` (`-f`) | Force remove container without confirmation |

### `podnext rename`

Rename an installed container in its metadata and filesystem.

```bash
podnext rename <OLD_NAME> <NEW_NAME>
```

**Examples:**
```bash
podnext rename debian deb-production
```

### `podnext copy`

Copy files or directories between the host and containers. Paths take the form `[CONTAINER:]PATH`. The command treats a path without a container prefix as a host path. The command strictly verifies path traversals such as `..` and rejects them.

```bash
podnext copy <SOURCE> <DESTINATION> [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--recursive` | `-R` | Copy directories recursively |
| `--move` | `-m` | Remove the source after successful transfer |

**Examples:**
```bash
# Copy a file from container to host
podnext copy debian:/etc/resolv.conf ./resolv.conf

# Copy a directory recursively from host to container
podnext copy ./my-app debian:/opt/my-app -R

# Move a file from container to host
podnext copy debian:/var/log/app.log ./app.log -m
```

### `podnext sync`

Mirror files between the host and containers with content hashing. The hash is SHA256. The command transfers only differing files. A later run is a no-op with zero overhead.

```bash
podnext sync <SOURCE> <DESTINATION> [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--delete` | `-d` | Remove destination entries that do not exist in source |

**Examples:**
```bash
# Mirror directory to container and delete deleted files
podnext sync ./src debian:/root/src -d
```

### `podnext reset`

Reset an installed container to a clean install of the original OCI image. The command deletes current rootfs modifications and reinstalls cleanly.

```bash
podnext reset <CONTAINER>
```

**Examples:**
```bash
podnext reset debian
```

## Image Commands

### `podnext search`

Search images on Docker Hub/OCI registries.

```bash
podnext search <QUERY> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-l, --limit <N>` | Maximum results (default: 10) |
| `-f, --format <FORMAT>` | Output format: table, json, yaml, text |

**Examples:**
```bash
podnext search ubuntu
podnext search alpine --limit 5
podnext search debian --format json
```

### `podnext image ls`

List local images. The command accepts `list` as an alias.

```bash
podnext image ls [OPTIONS]
podnext image list [OPTIONS]
```

The command lists what `~/.podnext/images` holds. An image produced by `podnext build local` lives
in the container store instead. See [`podnext build local`](#podnext-build-local). That image does
not appear here. `podnext image inspect <tag>` does read it.

### `podnext build local`

Build an image from a Dockerfile into the local store, without a registry.

```bash
podnext build local [TAG] [OPTIONS]
podnext build local -t myapp:latest ./context
podnext build local --file ./context/Dockerfile.myapp ./context
```
The build context must exist. The build context must contain a Dockerfile. A missing context is an
error that writes nothing. The command reports success only after the image is on disk.

**Where the image goes.** A built image *is* a container directory at
`~/.podnext/containers/<store-key>/rootfs`. The command derives the store key from the tag. It
drops everything from the first colon. Thus it stores `myapp:latest` as `myapp`:

```bash
podnext build local -t myapp:latest ./context
#   Stored as container 'myapp' (a tag is not a path component)
#   Run it with: podnext run myapp
```

The container records the tag itself in `metadata.json`. The command does not use a tag verbatim
as a directory name. PRoot splits `--bind` arguments on `:`. Thus a colon in a host path silently
drops the binding. A container stored as `myapp:latest` can lose `/dev/shm` and the `/proc` stubs.
Applications that need shared memory, such as Chromium and PostgreSQL, fail with unrelated
errors. `podnext run` therefore takes the store key, not the tag. A colon anywhere in a bind
source is now a hard error, not a silently dropped mount. That rule also covers bind mounts you
configure yourself.

A rebuild of the same tag replaces its image. A *different* tag that derives to the same key is
refused. For example, `myapp:v1` then `myapp:v2` is refused. The refusal stops the silent
destruction of the first image. Remove the container or choose another tag.

### `podnext config validate`

Check a gateway configuration file before deployment.

```bash
podnext config validate [PATH]
PN_GATEWAY_CONFIG=./gateway.toml podnext config validate
```

The command reads the file with the same loader that the gateway uses. A valid file prints the
resolved values and exits 0. An invalid file names the offending key with its line and column and
exits 2. Without a `PATH` argument the command takes the file from `PN_GATEWAY_CONFIG`. If the
operator gives neither, the command fails, because there is no implicit search path.

### `podnext image inspect`

```bash
podnext image inspect <IMAGE>... [--format table|json|yaml|text]
```

The command reports a pulled image from `~/.podnext/images`. It also reports a locally built
image from the container store. It accepts either the tag `myapp:latest` or the store key
`myapp`. The report gives the base image, creation time, working dir and command of a built
image. It labels the `EXPOSE` and `USER` values as recorded rather than enforced.

A reference that neither source knows is an error. The error names both locations. The command
does not return an empty result with a successful exit status.

### `podnext image rm`

Remove a local image. The command accepts `remove` as an alias.

```bash
podnext image rm <IMAGE> [OPTIONS]
podnext image remove <IMAGE> [OPTIONS]
```

### `podnext image pull`

Pull an image from a registry.

```bash
podnext image pull <IMAGE> [OPTIONS]
```

### `podnext image push`

Push an image to a registry.

```bash
podnext image push <IMAGE> [OPTIONS]
```

## Network Commands

### `podnext network create`

Create a network.

```bash
podnext network create <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--driver <DRIVER>` | Network driver: bridge, host, none, macvlan |
| `--subnet <CIDR>` | Subnet for bridge network |
| `--gateway <IP>` | Gateway IP |

IP-range carving inside a subnet is not implemented.

### `podnext network ls`

List networks. The command accepts `list` as an alias.

```bash
podnext network ls [OPTIONS]
podnext network list [OPTIONS]
```

### `podnext network rm`

Remove a network. The command accepts `remove` as an alias.

```bash
podnext network rm <NAME> [OPTIONS]
podnext network remove <NAME> [OPTIONS]
```

### `podnext network inspect`

Inspect network details.

```bash
podnext network inspect <NAME> [OPTIONS]
```

## Volume Commands

### `podnext volume create`

Create a volume.

```bash
podnext volume create <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--driver <DRIVER>` | Volume driver (default: `local`) |
| `--opts <KEY=VALUE>` | Driver options |

Volume size and storage-pool selection are not implemented. A volume is a directory under
`$HOME/.podnext/volumes`.

### `podnext volume ls`

List volumes. The command accepts `list` as an alias.

```bash
podnext volume ls [OPTIONS]
podnext volume list [OPTIONS]
```

### `podnext volume rm`

Remove a volume. The command accepts `remove` as an alias.

```bash
podnext volume rm <NAME> [OPTIONS]
podnext volume remove <NAME> [OPTIONS]
```

### `podnext volume inspect`

Inspect volume details.

```bash
podnext volume inspect <NAME> [OPTIONS]
```

## System Commands

### `podnext info`

Show system information.

```bash
podnext info [OPTIONS]
```

**Output** captured here. The command prints a single-row table, not a feature list:

```text
+---------+----------------+--------------+-------------------+------------------------------------------+
| VERSION | PLATFORM       | RUST VERSION | BACKEND           | ISOLATION                                |
+---------+----------------+--------------+-------------------+------------------------------------------+
| 0.1.8   | Termux/Android | 1.80+        | proot-distro-core | proot (unmapped host tree, masked /proc) |
+---------+----------------+--------------+-------------------+------------------------------------------+
```

The five fields are `VERSION PLATFORM RUST VERSION BACKEND ISOLATION`. The flag
`--format json|yaml|text` emits the same. An earlier version of this page showed a multi-section
feature report. That report had `PDN Version: 1.0.0`, `GPU: ✓`, `CRIU: ✓` and cgroups. No code
path printed that report.

### `podnext dbus`

Manage D-Bus IPC broker and service forwarder.

```bash
# Start standalone D-Bus broker on a specific UNIX socket
podnext dbus serve --socket $PREFIX/tmp/pn-dbus.sock

# List registered D-Bus services
podnext dbus list

# Call a method over D-Bus
podnext dbus call org.pnnext.Service /org/pnnext/Service org.pnnext.Service GetStatus --socket $PREFIX/tmp/pn-dbus.sock
```

The socket value is an explicit **host** path. Thus it uses the host temp base.
`podnext dbus serve` is a host process. The PDN temp base is `$PREFIX/tmp`, never `/tmp`. See the
`TMPDIR` row. On Android `/tmp` is not writable on the host. `/tmp` is only meaningful *inside* a
guest. There PDN binds a per-container directory. An earlier version of this page used
`/tmp/pn-dbus.sock`. That path is the guest temp base, not the host temp base. Without
`--socket`, `dbus serve` uses its own compiled default of `/run/pn-next/dbus/session` at
`crates/pn-next/src/dbus/mod.rs`.

### `podnext gitops`

Declarative application lifecycle management and reconciliation.

```bash
# Sync applications and volumes from declarative YAML manifests
podnext gitops sync

# Show GitOps synchronization status
podnext gitops status

# Inspect configuration drift against desired state
podnext gitops drift

# Reconcile drifts and enforce state
podnext gitops reconcile
```

### `podnext doctor`

Report host and guest diagnostics.

```bash
podnext doctor [--container <NAME>] [--format <FORMAT>]
```

Host checks cover the following items. The first item is the proot binary and its version. The
run path resolves that path the same way. The second item is the temporary directory: `TMPDIR`
and whether it is writable. The third item is shared memory: `/dev/shm`, or the directory PDN
binds in its place. The fourth item is DNS: the nameserver list from `$PREFIX/etc/resolv.conf`,
`/etc/resolv.conf` or `/system/etc/resolv.conf`. The fifth item is the kernel: `uname -a`. The
sixth item is cgroups: `/proc/self/cgroup`. With `--container <NAME>` the command probes the
guest from the inside. The probe reads `os-release`, architecture, `/dev/shm`, the mount count
and `resolv.conf`. It also captures the **proot command line that ran**. For a typical container
that command line has 12,371 characters with 125 `--bind=` arguments. The table elides it, and
`--format json` gives it whole.

Every row carries the value or command it came from. A check that cannot run reports `unknown`
instead of a pass. Only a failed check changes the exit status:

```sh
$ podnext doctor
ok         proot binary      /data/data/com.termux/files/usr/bin/proot: proot 5.1.107.101
ok         temporary directory /data/data/com.termux/files/usr/tmp is writable (TMPDIR=Some("..."))
ok         shared memory     /dev/shm is not present ..., which is expected on Android: PDN binds a
                             per-container directory in its place, so this is informational ...
ok         dns               /data/data/com.termux/files/usr/etc/resolv.conf -> 8.8.8.8, 8.8.4.4
ok         host kernel       Linux localhost 6.1.157-android14-11-... aarch64 Android
ok         cgroups           5 entry/entries; first: 4:memory:/
unknown    guest             no container named; pass one (podnext doctor --container <name>) ...

7 checks: 0 failed, 0 warnings, 1 unknown
```

`--format json` and `--format yaml` emit the same list as structured records. Exit status is `0`
when no check failed. Exit status is `1` when at least one check failed. Exit status is `2` for
invalid arguments.

### Path defaults

The daemon resolves its socket, service, runtime and log directories at start. It reads
`TERMUX__HOME`/`HOME`, `XDG_RUNTIME_DIR`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME`. It does not
compile those paths in. Thus a relocated home or a second profile takes them along.
`podnext-daemon --help` prints the values it resolved on this machine. The test
`crates/podnext/tests/relocated_paths.rs` drives the binary with a relocated home to check it.

### `podnext version`

Show version information.

```bash
podnext version [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-f, --format <FORMAT>` | Output format: `table` (default), `json`, `yaml`, `text` |

There is no `--short` and no `--json`. The JSON form is `podnext version --format json`.

## Direct Execution with PRoot

Run commands directly with PRoot. This path bypasses container management:

```bash
# Basic execution
podnext -r /path/to/rootfs -- /bin/bash

# With bind mounts
podnext -r /path/to/rootfs -b /host/path:/guest/path -- ls /guest/path

# With environment
podnext -r /path/to/rootfs -e "VAR=value" -- /bin/bash -c "echo $VAR"

# With QEMU emulation
podnext -r /path/to/rootfs -q qemu-aarch64 -- /bin/bash
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (missing container/image, failed operation, refused stub) |
| 2 | Invalid arguments (unknown subcommand, missing or conflicting flags) |
| *n* | The command's own exit status, passed through unchanged |

Three classes are asserted by `crates/podnext/tests/automation_contract.rs`. A command executed
inside a container propagates the guest status verbatim. For example, `podnext run ... -- cmd`
and `podnext login ... -- cmd`. A signal death gives `128 + signal`. A wrapper script can test
`$?` against the real code. Codes 3-12 were documented here but no code path produces them. The
table above is the contract.

## Environment Variables

These are the variables that the code reads. Each row names the site that reads it. The reader can re-verify the table with `grep`.
can be re-verified with `grep`.

| Variable | Effect | Read at |
|----------|--------|---------|
| `PN_DEBUG` | Any value turns on debug tracing | `pn-next` `pn_trace!` (supervisor, backup_restore) |
| `PN_NEXT_DEBUG` | Same, for the CLI process | `crates/podnext/src/main.rs:1503` |
| `PN_NEXT_SERVICES_DIR` | Directory holding service definitions | `crates/podnext/src/commands/mod.rs:614` |
| `PN_SHM_DIR` | Directory bound to the guest's `/dev/shm` | `crates/proot-distro-core/src/shm.rs:16` |
| `PDN_BACKUP_DIR` | Backup destination | `crates/pn-next/src/backup_restore/mod.rs:98` |
| `PDN_CONTAINERS_DIR` | Container store root | `crates/pn-next/src/backup_restore/mod.rs:87` |
| `PDN_GITOPS_DIR` | GitOps manifest directory | `crates/pn-next/src/gitops/mod.rs:483` |
| `PROOT_BIN` | Path to the `proot` binary. Optional: read only by the `--engine proot` path, which is no longer the default | `crates/podnext/src/native.rs:129`, `crates/proot-core/src/lib.rs:72` |
| `RUNTIME_DIR` | Directory holding session runtime state | `crates/proot-distro-core/src/session.rs:25` |
| `UML4TERMUX_RUNTIME` | Select the uml4termux runtime | `crates/proot-distro-core/src/runtime.rs:205` |
| `TMPDIR` | Temp base; falls back to `$PREFIX/tmp`, never `/tmp` | `crates/proot-distro-core/src/paths.rs:24` |
| `PREFIX` | Termux prefix used for the `tmp` fallback | `crates/proot-distro-core/src/paths.rs:24` |
| `HOME` | Container store root: `$HOME/.podnext` | `crates/proot-distro-core/src/paths.rs` |
| `PN_TEST_ROOT` | Test-only root override | `crates/proot-distro-core/src/paths.rs:29`, `shm.rs:23` |
| `TERMUX__HOME` | Termux home for the daemon defaults and the store root | `crates/proot-distro-core/src/paths.rs` (`paths::host::home`) |
| `TERMUX__PREFIX` | Termux prefix, ahead of `PREFIX` | `crates/proot-distro-core/src/paths.rs` (`paths::host::prefix`) |
| `TERMUX_APP__PACKAGE_NAME` | Host app package name (default `com.termux`) | `crates/proot-distro-core/src/paths.rs` (`paths::host::package_name`) |
| `XDG_RUNTIME_DIR` | Runtime base for the daemon socket and state | `crates/proot-distro-core/src/paths.rs` (`paths::host::runtime_dir`), `crates/podnext/src/daemon.rs` |
| `XDG_CONFIG_HOME` | Configuration base (`pn-next/services`, …) | `crates/proot-distro-core/src/paths.rs` (`paths::host::config_dir`), `crates/podnext/src/commands/mod.rs` |
| `XDG_STATE_HOME` | Log base when set; otherwise `$HOME/.local/log` | `crates/podnext/src/daemon.rs` (`default_log_dir`) |

The page once documented `PN_CONFIG_DIR`, `PN_DATA_DIR`, `PN_CACHE_DIR`, `PN_LOG_LEVEL`,
`PN_GATEWAY_ADDR`, `PN_RUNTIME`, `PN_SECURITY_PROFILE` and `PN_NO_COLOR`. They appear nowhere in
`crates/`, and nothing reads them. The project removed them rather than implemented them. A
variable that is documented but inert is worse than an absent one.

## Configuration Files

### Container Config at `~/.config/pn-next/containers/<name>.toml`

```toml
[container]
name = "ubuntu"
distro = "ubuntu"
version = "22.04"

[container.config]
# planned: user
hostname = "ubuntu"
working_dir = "/root"
user = "root"

[container.config.env]
TERM = "xterm-256color"
LANG = "en_US.UTF-8"

[container.config.mounts]
"/sdcard" = { target = "/mnt/sdcard", readonly = false }
"/data/data/com.termux/files/home" = { target = "/home/termux", readonly = false }

# planned: io_weight, io_max_read, io_max_write
[container.resources]
memory_limit = "2G"
cpu_limit = "2.0"
pids_limit = 100
io_weight = 100
io_max_read = "100MB/s"
io_max_write = "100MB/s"

[container.security]
seccomp_profile = "default" # refused: this PRoot build installs no BPF filter (issue #60)
no_new_privileges = true
read_only_rootfs = false
user_namespace = false

# planned: networks
[container.network]
mode = "bridge"
networks = ["default"]

# planned: api
[container.gpu]
enabled = false
devices = []
api = "vulkan"
```

## Shell Completion

```bash
# Bash
podnext completion bash > /etc/bash_completion.d/podnext

# Zsh
podnext completion zsh > /usr/share/zsh/site-functions/_podnext

# Fish
podnext completion fish > ~/.config/fish/completions/podnext.fish
```

## See Also

- [Getting Started](getting-started.md)
- [Configuration](configuration.md)
- [Security](security.md)
- [Architecture](architecture.md)
| `--kill-on-exit` | | Kill processes when the command exits. Refused by name on container commands and on the direct path: it is the default and is passed to proot unconditionally |