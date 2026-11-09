# Container Engines

## Description

PodNext runs a guest with one of two engines, and the difference is what actually executes the guest's
programs.

- `native` is the engine that ships inside PodNext (`proot_core::native_run`). It attaches to the guest
  with `ptrace`, translates the guest's file paths as they cross into the host, and never executes a
  `proot` binary. It has been the default since 2026-11-09.
- `proot` is the legacy path: PodNext assembles a command line and executes the host `proot` binary,
  which then does the same job in C.

What each engine does NOT do is worth stating plainly, because both are easy to over-read.

- `native` does not emulate every kernel call the way `proot` does. Where the host refuses an operation
  (opening `/sys`, for instance), the native engine passes that refusal through instead of inventing a
  successful answer. It does rewrite the four identity calls (`getuid`, `geteuid`, `getgid`, `getegid`)
  to zero, which is a deliberate and documented divergence that mirrors what `proot` does through
  `--change-id=0:0:0`.
- `proot` does not need to be installed for the default engine to work. A missing `proot` binary is only
  a problem when you ask for `--engine proot`, which is why `podnext doctor` reports it as a warning
  rather than a failure.

The engine choice is also not a promise of identical output. Read [Known Limitations](known-limitations.md)
for the list of measured differences that remain.

## How To Use

Run a container on the default engine (nothing to add):

```bash
podnext run mycontainer -- /bin/sh -c 'cat /etc/os-release'
```

Ask for the legacy engine explicitly:

```bash
podnext --engine proot run mycontainer -- /bin/sh
```

`--engine` is a global option, so it goes before the subcommand. `podnext --help`, `podnext run --help` and
`podnext login --help` all print the current default on the `--engine` line.

Prove for yourself that no `proot` binary is involved. Both of these make it impossible for the run to fall
back to a host `proot`:

```bash
PATH=/nonexistent-bin PROOT_BIN=/nonexistent/proot podnext run mycontainer -- /bin/sh
PROOT_BIN=/path/to/a/decoy/script podnext run mycontainer -- /bin/sh
```

The second form is the stronger one: point `PROOT_BIN` at a script that writes a file, and the file's
absence afterwards is evidence that the engine never executed it. The repository's acceptance harness does
exactly this (`scripts/own_engine_acceptance.py`).

To check your host before blaming a container:

```bash
podnext doctor
```

`doctor` reports what it measured and exits non-zero only when a check failed. A missing `proot` binary is
a `warn`, because the default engine does not use it.

## How To Configure

- Permanent choice: there is no config key for the engine. Pass `--engine proot` on the commands that need
  the legacy path, or leave it alone and get `native`.
- `PROOT_BIN` overrides the path to the `proot` binary, and is read only by the `proot` engine.
- `PATH` matters only to the `proot` engine, which resolves the binary through it.
- Environment variables for the guest come from the container's own configuration (`podnext config set
  <container> env.NAME=VALUE`), not from the host, and the engine forwards them unchanged. That is true for
  both engines: the difference is who executes the guest, not what the guest sees.

## Example

A measured session on a real Debian 13 rootfs, with the same container under both engines:

```text
$ podnext run deb-real -- /bin/sh -c 'ls /usr/bin | wc -l'
257
$ podnext --engine proot run deb-real -- /bin/sh -c 'ls /usr/bin | wc -l'
257
```

The guest's own report of a shell-spawned program, which is where the two engines used to differ:

```text
$ podnext run deb-real -- /bin/sh -c 'test -x /bin/ls && echo Y || echo N'
Y
$ podnext --engine proot run deb-real -- /bin/sh -c 'test -x /bin/ls && echo Y || echo N'
Y
```

One divergence you can see directly, and it is explained rather than hidden: the `argv[0]` of a program the
guest spawns is the guest interpreter under `native`, because a matching interpreter invocation carries the
program as its argument. The program's own `argv[0]` is preserved through the interpreter's `--argv0` option
where the interpreter has one; this host's Android `linker64` does not, so the fallback shows the program
path instead.

## Two programs, two stores

`podnext` and `proot-distro` are separate programs that do not share a container store.

| program | container store | notes |
|---|---|---|
| `podnext` | `$HOME/.podnext/containers/<name>/rootfs` | this project |
| `proot-distro` | its own layout under the Termux prefix | the Termux package this project replaces; separate CLI, separate store |

If `podnext list` shows your container but `proot-distro login <name>` says the container is not installed,
nothing is broken: the second command is a different program looking in a different place. Use `podnext
login <name>`.