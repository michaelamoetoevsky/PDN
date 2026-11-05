# CLI Reference

## Command stability

Two labels, one measurable rule: **`[stable]` means at least one automated test in this
repository exercises the command; `[experimental]` means no such test exists yet, or the
command contains a path that refuses.** `[experimental]` is not a claim that the command is
broken — `install`, `login` and `run` are the most-used paths in the project and sit here
because the suite does not drive them yet. Re-derive this table with `podnext --help`;
`crates/podnext/tests/cli_reference_contract.rs` fails when the two disagree.

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
| `podnext image` | `[experimental]` | `pull` and `push` refuse: there is no OCI image store before v0.2.0 |
| `podnext sessions` | `[stable]` | exercised by `crates/podnext/tests/automation_contract.rs` |
| `podnext attach` | `[experimental]` | no automated test covers it yet |
| `podnext info` | `[experimental]` | no automated test covers it yet |
| `podnext version` | `[experimental]` | no automated test covers it yet |
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

`help`, which clap adds itself, carries no label.

## Global Options

```bash
podnext [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] [-- <CMD_ARGS>...]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |
| `--rootfs <PATH>` | `-r` | Use PATH as guest root filesystem |
| `--bind <HOST:GUEST>` | `-b` | Bind mount (repeatable) |
| `--pwd <DIR>` | `-w` | Set working directory |
| `--qemu <BIN>` | `-q` | Execute through QEMU user-mode |
| `--link2symlink` | `-0` | Enable hardlink emulation via symlinks |
| `--no-sysvipc` | | Disable System V IPC emulation |
| `--kernel <VER>` | | Override kernel release string |
| `--hostname <NAME>` | | Set hostname |
| `--env <VAR=VALUE>` | `-e` | Set environment variable (repeatable) |

## Container Commands

### `podnext install`

Install a distribution from OCI image.

```bash
podnext install <IMAGE_REF> [OPTIONS]
```

**Arguments:**
- `IMAGE_REF` - OCI image reference (e.g., `ubuntu:22.04`, `docker.io/library/alpine:latest`)

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --name <NAME>` | Custom container name |
| `-f, --force` | Force reinstall if container exists |

**Examples:**
```bash
podnext install ubuntu:22.04
podnext install alpine:latest --name my-alpine
podnext install docker.io/library/debian:bookworm --force
```

### `podnext list`

List created containers (`ls` is accepted as an alias).

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

**Output:**
```
NAME       IMAGE              STATUS      CREATED
ubuntu     ubuntu:22.04       stopped     2024-01-15 10:30:00
alpine     alpine:latest      running     2024-01-15 11:00:00
```

### `podnext login`

Start interactive shell inside a container.

```bash
podnext login <CONTAINER> [OPTIONS] [-- <SHELL_ARGS>...]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-u, --user <USER>` | User to run as |
| `--isolated` | Run in isolated mode (do not mount host storage or Termux home directory) |

**Examples:**
```bash
podnext login ubuntu
podnext login ubuntu -- /bin/bash
podnext login ubuntu --user 1000
podnext login debian --isolated
```

### `podnext run`

Run command in container.

```bash
podnext run <CONTAINER> [OPTIONS] -- <CMD> [ARGS...]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--name <NAME>` | Assign a name to the container |
| `--isolated` | Run in isolated mode (do not mount host storage or Termux home directory) |
| `-d, --detach` | Run in background — not implemented yet: the command refuses rather than pretending |
| `--rm` | Remove container after exit |
| `--volume <HOST:GUEST>` | Mount a host directory into the container (repeatable). A `:ro` suffix is refused — proot has no read-only bind form, and the suffix would become part of the guest path name |
| `-e, --env <VAR=VALUE>` | Set environment variable (repeatable) |
| `-w, --workdir <DIR>` | Working directory inside container |
| `--network <NETWORK>` | Connect container to a network |

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

List active sessions (better formatted).

```bash
podnext sessions [OPTIONS]
```

### `podnext attach`

Attach to a container session.

```bash
podnext attach <SESSION_ID> [OPTIONS]
```

### `podnext kill`

Stop container.

```bash
podnext kill <CONTAINER>
```

There are no `--signal` or `--timeout` options: the teardown sequence (SIGQUIT to the traced root,
then a scan of `/proc/*/fd` for inherited session inodes) is fixed. Selecting the signal is listed in
`known-limitations.md`.

### `podnext remove`

Delete a container (`rm` is accepted as an alias).

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

Copy files or directories between the host and containers. Paths take the form `[CONTAINER:]PATH`. A path without a container prefix is treated as a host path. Path traversals (e.g., `..`) are strictly verified and rejected.

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

Mirror files between host and containers using content hashing (SHA256). Only differing files are transferred, so subsequent runs are zero-overhead no-ops.

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

Reset an installed container to a clean install of the original OCI image it came from. Deletes current rootfs modifications and reinstalls cleanly.

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

List local images (`list` is accepted as an alias).

```bash
podnext image ls [OPTIONS]
podnext image list [OPTIONS]
```

Lists what `~/.podnext/images` holds. An image produced by `podnext build local` lives in the
container store instead (see [`podnext build local`](#podnext-build-local)) and does not appear here;
`podnext image inspect <tag>` does read it.

### `podnext build local`

Build an image from a Dockerfile into the local store, without a registry.

```bash
podnext build local [TAG] [OPTIONS]
podnext build local -t myapp:latest ./context
podnext build local --file ./context/Dockerfile.myapp ./context
```

The build context must exist and must contain a Dockerfile; a missing context is an error that
writes nothing, and success is reported only after the image is on disk.

**Where the image goes.** A built image *is* a container directory,
`~/.podnext/containers/<store-key>/rootfs`. The store key is derived from the tag by dropping
everything from the first colon, so `myapp:latest` is stored as `myapp`:

```bash
podnext build local -t myapp:latest ./context
#   Stored as container 'myapp' (a tag is not a path component)
#   Run it with: podnext run myapp
```

The tag itself is recorded in the container's `metadata.json`. A tag is not used verbatim as a
directory name because PRoot splits `--bind` arguments on `:`, so a colon in a host path silently
drops the binding - a container stored as `myapp:latest` would lose `/dev/shm` and the `/proc`
stubs, and applications needing shared memory (Chromium, PostgreSQL) would fail with unrelated
errors. `podnext run` therefore takes the store key, not the tag. A colon anywhere in a bind
source is now a hard error rather than a silently dropped mount, which also covers bind mounts you
configure yourself.

Rebuilding the same tag replaces its image. A *different* tag that derives to the same key
(`myapp:v1` then `myapp:v2`) is refused rather than silently destroying the first image; remove the
container or choose another tag.

### `podnext config validate`

Check a gateway configuration file before deploying it.

```bash
podnext config validate [PATH]
PN_GATEWAY_CONFIG=./gateway.toml podnext config validate
```

Reads the file with the same loader the gateway uses. A valid file prints the resolved values and
exits 0; an invalid one names the offending key with its line and column and exits 2. Without a
`PATH` argument the file is taken from `PN_GATEWAY_CONFIG`; if neither is given the command fails,
because there is no implicit search path.

### `podnext image inspect`

```bash
podnext image inspect <IMAGE>... [--format table|json|yaml|text]
```

Reports a pulled image from `~/.podnext/images`, or a locally built one from the container store —
accepting either the tag (`myapp:latest`) or the store key (`myapp`). A built image is reported with
its base image, creation time, working dir, command, and its `EXPOSE`/`USER` values labelled as
recorded rather than enforced.

A reference that neither source knows is an error naming both locations, rather than an empty result
with a successful exit status.

### `podnext image rm`

Remove local image (`remove` is accepted as an alias).

```bash
podnext image rm <IMAGE> [OPTIONS]
podnext image remove <IMAGE> [OPTIONS]
```

### `podnext image pull`

Pull image from registry.

```bash
podnext image pull <IMAGE> [OPTIONS]
```

### `podnext image push`

Push image to registry.

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

List networks (`list` is accepted as an alias).

```bash
podnext network ls [OPTIONS]
podnext network list [OPTIONS]
```

### `podnext network rm`

Remove network (`remove` is accepted as an alias).

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

Create volume.

```bash
podnext volume create <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--driver <DRIVER>` | Volume driver (default: `local`) |
| `--opts <KEY=VALUE>` | Driver options |

Volume size and storage-pool selection are not implemented; a volume is a directory under
`$HOME/.podnext/volumes`.

### `podnext volume ls`

List volumes (`list` is accepted as an alias).

```bash
podnext volume ls [OPTIONS]
podnext volume list [OPTIONS]
```

### `podnext volume rm`

Remove volume (`remove` is accepted as an alias).

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

**Output:**
```
PDN Version: 1.0.0
Host OS: Linux 6.1.0 (aarch64)
Termux: 0.118.0
PRoot: 5.4.0
proot-distro: N/A (native)

Containers: 3
Images: 5
Networks: 2
Volumes: 4

Resources:
  CPU: 8 cores
  Memory: 8.0 GB
  Disk: 128 GB (45 GB free)

Features:
  GPU: ✓ (Adreno 730)
  CRIU: ✓
  Seccomp: ✓
  AppArmor: ✗
  SELinux: ✗
  Cgroups v2: ✓
```

### `podnext dbus`

Manage D-Bus IPC broker and service forwarder.

```bash
# Start standalone D-Bus broker on a specific UNIX socket
podnext dbus serve --socket /tmp/pn-dbus.sock

# List registered D-Bus services
podnext dbus list --socket /tmp/pn-dbus.sock

# Call a method over D-Bus
podnext dbus call org.pnnext.Service /org/pnnext/Service org.pnnext.Service.GetStatus --socket /tmp/pn-dbus.sock
```

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

### `podnext version`

Show version information.

```bash
podnext version [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-f, --format <FORMAT>` | Output format: `table` (default), `json`, `yaml`, `text` |

There is no `--short` and no `--json`; `podnext version --format json` is the JSON form.

## Direct Execution (proot-compatible)

Run commands directly with PRoot (bypassing container management):

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

Three classes are asserted by `crates/podnext/tests/automation_contract.rs`. A command executed inside
a container (`podnext run … -- cmd`, `podnext login … -- cmd`) propagates the guest's status verbatim,
including `128 + signal` for a signal death — a wrapper script can test `$?` against the real code.
Codes 3–12 were documented here but are not produced by any code path; the table above is the
contract.

## Environment Variables

These are the variables the code actually reads. Each row names the site that reads it, so the table
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
| `PROOT_BIN` | Path to the `proot` binary (default: resolved from `PATH`) | `crates/podnext/src/native.rs:129`, `crates/proot-core/src/lib.rs:72` |
| `RUNTIME_DIR` | Directory holding session runtime state | `crates/proot-distro-core/src/session.rs:25` |
| `UML4TERMUX_RUNTIME` | Select the uml4termux runtime | `crates/proot-distro-core/src/runtime.rs:205` |
| `TMPDIR` | Temp base; falls back to `$PREFIX/tmp`, never `/tmp` | `crates/proot-distro-core/src/paths.rs:24` |
| `PREFIX` | Termux prefix used for the `tmp` fallback | `crates/proot-distro-core/src/paths.rs:24` |
| `HOME` | Container store root: `$HOME/.podnext` | `crates/proot-distro-core/src/paths.rs` |
| `PN_TEST_ROOT` | Test-only root override | `crates/proot-distro-core/src/paths.rs:29`, `shm.rs:23` |

`PN_CONFIG_DIR`, `PN_DATA_DIR`, `PN_CACHE_DIR`, `PN_LOG_LEVEL`, `PN_GATEWAY_ADDR`, `PN_RUNTIME`,
`PN_SECURITY_PROFILE` and `PN_NO_COLOR` were documented here and appear nowhere in `crates/`: nothing
reads them. They have been removed rather than implemented — a variable that is documented but inert
is worse than an absent one.

## Configuration Files

### Container Config (`~/.config/pn-next/containers/<name>.toml`)

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
seccomp_profile = "default"
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