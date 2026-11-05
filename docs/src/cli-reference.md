# CLI Reference

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
| Option | Short | Description |
|--------|-------|-------------|
| `--name <NAME>` | `-n` | Custom container name |
| `--force` | `-f` | Force reinstall if container exists |

**Examples:**
```bash
podnext install ubuntu:22.04
podnext install alpine:latest --name my-alpine
podnext install debian:13 --force
```

### `podnext list`

List created containers.

```bash
podnext list [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--all` | `-a` | Show all containers (including stopped; default: true) |
| `--no-all` | | Hide stopped containers |
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

**Output:**
```
+---------+--------+---------+---------+---------+------------------+
| NAME    | DISTRO | VERSION | ARCH    | STATUS  | CREATED          |
+---------+--------+---------+---------+---------+------------------+
| ubuntu  | ubuntu | 22.04   | aarch64 | Stopped | 2024-01-15 10:30 |
| alpine  | alpine | 3.19    | aarch64 | Running | 2024-01-15 11:00 |
+---------+--------+---------+---------+---------+------------------+
```

### `podnext login`

Start interactive shell inside a container.

```bash
podnext login <CONTAINER> [OPTIONS] [-- <SHELL_ARGS>...]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--user <USER>` | `-u` | User to run as (default: root) |
| `--isolated` | | Run in isolated mode (do not mount host storage or Termux home directory) |

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
| Option | Short | Description |
|--------|-------|-------------|
| `--detach` | `-d` | Run in background *(experimental/stub: flag accepted but currently runs in foreground)* |
| `--rm` | | Automatically remove container when it exits |
| `--name <NAME>` | | Assign a name to the container |
| `--network <NETWORK>` | | Connect container to a network |
| `--volume <HOST:GUEST[:ro]>` | | Bind mount a volume (repeatable) |
| `--publish <HOST:CONTAINER>` | `-p` | Publish container port to host (repeatable) |
| `--entrypoint <CMD>` | | Overwrite default entrypoint of the image |
| `--tty` | `-t` | Allocate a pseudo-TTY |
| `--interactive` | `-i` | Keep STDIN open even if not attached |
| `--user <USER>` | `-u` | User to run as (`<name|uid>[:<group|gid>]`) |
| `--workdir <DIR>` | | Working directory inside the container |
| `--isolated` | | Run in isolated mode (do not mount host storage or Termux home directory) |

**Examples:**
```bash
podnext run ubuntu -- apt update
podnext run ubuntu -v /sdcard:/mnt/sdcard -- ls -la /mnt/sdcard
podnext run ubuntu --isolated -- echo "isolated run"
```

### `podnext ps`

List active sessions.

```bash
podnext ps [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

### `podnext sessions`

List active sessions (detailed format).

```bash
podnext sessions [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

### `podnext attach`

Attach to a container session (creates new session).

```bash
podnext attach <CONTAINER> [OPTIONS] [-- <COMMAND>...]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--user <USER>` | `-u` | User to run as |

### `podnext kill`

Stop container sessions.

```bash
podnext kill <CONTAINER>
```

Sends `SIGQUIT` followed by process teardown to running container processes. Use `all` or `--all` as `<CONTAINER>` to stop all active sessions across all containers.

### `podnext remove`

Delete a container.

```bash
podnext remove <CONTAINER> [OPTIONS]
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

Search images on Docker Hub/OCI registries. When query is omitted, popular verified distributions are listed.

```bash
podnext search [QUERY] [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--limit <N>` | `-l` | Maximum results (default: 10) |
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

**Examples:**
```bash
podnext search
podnext search debian
podnext search ubuntu --limit 5
```

### `podnext image ls`

List local images (`list` is accepted as an alias).

```bash
podnext image ls [OPTIONS]
podnext image list [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--all` | `-a` | Show all images (including intermediate) |
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

Lists what `~/.podnext/images` holds. An image produced by `podnext build local` lives in the
container store instead (see [`podnext build local`](#podnext-build-local)) and does not appear here;
`podnext image inspect <tag>` does read it.

### `podnext build local`

Build an image from a Dockerfile into the local store, without a registry.

```bash
podnext build local [OPTIONS] [PATH]
podnext build local -t myapp:latest ./context
podnext build local -f ./context/Dockerfile.myapp ./context
```

The build context must exist and must contain a Dockerfile; a missing context is an error that
writes nothing, and success is reported only after the image is on disk.

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--tag <NAME:TAG>` | `-t` | Name and optional tag in `name:tag` format |
| `--file <PATH>` | `-f` | Path to Dockerfile (defaults to `<PATH>/Dockerfile`) |

*Note: Multi-arch remote build subcommands (`podnext build submit`, `status`, `cancel`, `list`, `artifacts`, `logs`) are experimental/stubs.*

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

Remove local image(s).

```bash
podnext image rm <IMAGE>... [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--force` | `-f` | Force removal without confirmation |

### `podnext image pull`

Pull image from registry. *(experimental/stub: image pull stub; does not persist image to store)*

```bash
podnext image pull <IMAGE> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--platform <PLATFORM>` | Platform (e.g., `linux/arm64`) |
| `--all-tags` | Download all tagged images |

### `podnext image push`

Push image to registry.

```bash
podnext image push <IMAGE>
```

## Network Commands

### `podnext network create`

Create a network.

```bash
podnext network create <NAME> [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--driver <DRIVER>` | `-d` | Network driver (default: `bridge`) |
| `--subnet <CIDR>` | `-s` | Subnet for bridge network |
| `--gateway <IP>` | `-g` | Gateway IP |

### `podnext network ls`

List networks.

```bash
podnext network ls [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

### `podnext network rm`

Remove network(s).

```bash
podnext network rm <NAME>...
```

### `podnext network inspect`

Inspect network details.

```bash
podnext network inspect <NAME>... [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `json`, `table`, `yaml`, `text` (default: `json`) |

### `podnext network connect`

Connect a container to a network.

```bash
podnext network connect <NETWORK> <CONTAINER> [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--ip <IP>` | `-i` | Container IP address |

### `podnext network disconnect`

Disconnect a container from a network.

```bash
podnext network disconnect <NETWORK> <CONTAINER>
```

## Volume Commands

### `podnext volume create`

Create a persistent volume.

```bash
podnext volume create <NAME> [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--driver <DRIVER>` | `-d` | Volume driver (default: `local`) |
| `--opts <KEY=VALUE>` | `-o` | Driver options (repeatable) |

### `podnext volume ls`

List volumes.

```bash
podnext volume ls [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

### `podnext volume rm`

Remove one or more volumes.

```bash
podnext volume rm <NAME>... [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--force` | `-f` | Force removal without confirmation |

### `podnext volume inspect`

Inspect volume details.

```bash
podnext volume inspect <NAME>... [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `json`, `table`, `yaml`, `text` (default: `json`) |

### `podnext volume prune`

Remove all unused volumes.

```bash
podnext volume prune [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--force` | `-f` | Force prune without confirmation |

## System Commands

### `podnext info`

Show system information.

```bash
podnext info [OPTIONS]
```

**Options:**
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `table`, `json`, `yaml`, `text` (default: `table`) |

**Output:**
```
+---------+----------------+--------------+-------------------+
| VERSION | PLATFORM       | RUST VERSION | BACKEND           |
+---------+----------------+--------------+-------------------+
| 0.1.8   | Termux/Android | 1.80+        | proot-distro-core |
+---------+----------------+--------------+-------------------+
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

# Force sync
podnext gitops sync --force

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
| Option | Short | Description |
|--------|-------|-------------|
| `--format <FORMAT>` | `-f` | Output format: `text`, `table`, `json`, `yaml` (default: `text`) |

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
| 1 | General error / execution failure |
| 2 | Invalid command-line arguments / syntax |

*Note: For `podnext run` and `podnext login`, child process exit codes (0–127, or 128+SIG for signals) are forwarded directly.*

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PN_DEBUG` | Enable low-level trace logging across crates (`PN_DEBUG=1`) | Unset (disabled) |
| `PN_NEXT_DEBUG` | Enable debug logging in `podnext` CLI frontend | Unset (disabled) |
| `PN_GATEWAY_CONFIG` | Path to gateway configuration file for `podnext config validate` | Unset |
| `PN_NEXT_SERVICES_DIR` | Directory containing service definition TOML files | `~/.config/pn-next/services` |
| `PN_NEXT_RUNTIME_DIR` | Runtime PID and socket directory | `~/.local/run/pn-next` |
| `PN_NEXT_LOG_DIR` | Directory for service execution logs | `~/.local/log/pn-next` |
| `PN_SHM_DIR` | Custom directory override for POSIX shared memory emulation | Container shm / host shm |
| `PN_TEST_ROOT` | Root directory override used in tests and test harnesses | Unset |

## Configuration Files

Container instances and metadata are maintained under `~/.podnext/containers/<name>/` (`config.json`, `metadata.json`, `rootfs/`). Managed service configurations are loaded from `~/.config/pn-next/services/<name>.toml`.

### Container Config Format (`~/.podnext/containers/<name>/config.json`)

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