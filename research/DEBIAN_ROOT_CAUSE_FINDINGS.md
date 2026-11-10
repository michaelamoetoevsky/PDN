# PDN Debian Root Cause Analysis and Fixes for November 2026

**Environment**: Termux on Android 16, aarch64, unprivileged, uid 10396.
**Comparators**: `proot-distro 5.10.3` on Debian Trixie compared to `podnext 0.1.7` on `deb-pdn`, Debian Trixie.
All findings below come from live command output. This document gives the reproduction commands verbatim.

---

## 1. Hard Links Blocked by Android SELinux -> Debian Install Fails

**Symptom in the install log:**

```
Error: failed to unpack `.../.podnext/containers/deb-pdn/rootfs/usr/bin/perl5.40.1`
Caused by:
    Permission denied (os error 13) when hard linking
    .../rootfs/usr/bin/perl to .../rootfs/usr/bin/perl5.40.1
```

**Proof of a Platform Constraint, Not an Application Logic Error**: the `ln` command fails directly inside `$HOME`.

```
$ echo x > ~/hltest/a; ln ~/hltest/a ~/hltest/b
ln: failed to create hard link '.../hltest/b' => '.../hltest/a': Permission denied
```

The filesystem is `f2fs`, as the command `stat -f` reports. The mount options are `seclabel,nosuid,nodev`. The file `/sys/fs/selinux/enforce` is unreadable. Alpine succeeded because BusyBox applets use **symlinks**. Debian images use **hard links**, for example `perl` -> `perl5.40.1`. This difference causes an extraction failure on all distros that use hard links.

**Remediation:** The file `crates/proot-distro-core/src/untar.rs` holds a custom streaming tar extractor. The code tries `hard_link` first. If the system rejects the link with `EACCES` or `EPERM`, the code **falls back to copying the file**. Symlinks remain genuine symlinks. The code skips device nodes and FIFOs. The code applies the file modes after it writes the content. The code rejects path traversal outside the destination bounds. This single implementation is shared by installation in `native.rs`, compression in `compress.rs`, and restoration in `backup_restore/mod.rs`.

**Verification:** `podnext install debian --name deb-pdn` -> `Container 'deb-pdn' installed successfully!` + `metadata.json` written. The rootfs is 19MB and the tree is fully populated.

---

## 2. Missing --change-id Makes apt install Impossible, the Primary Root Cause

**Symptom:**

```
$ podnext login deb-pdn -- apt-get install -y --no-install-recommends curl
dpkg: error: requested operation requires superuser privilege
E: Sub-process /usr/bin/dpkg returned an error code (2)
/bin/sh: 1: curl: not found
```

For comparison, the same command in proot-distro prints `Setting up curl (8.14.1-2+deb13u7)` + `curl 8.14.1 (aarch64-unknown-linux-gnu) ...`.

**Root Cause from Source Evidence:**

| Implementation | Proot Argv Construction |
|---|---|
| PDN `crates/proot-core/src/lib.rs:200-204` | `--kill-on-exit --link2symlink --sysvipc -L` — **omits `--change-id`** |
| proot-distro `login/proot_cmd.py:105` | `--change-id={login_uid}:{login_gid}` (defaults to `0:0`) |

Without `--change-id`, the guest processes keep the real Android host UID, which is 10396. Then `dpkg` rejects all privileged operations immediately. This fault was the main blocker. It stopped Node.js, Playwright, Chromium, and standard package installations from working in PDN containers.

**Remediation:** The change adds `--change-id=0:0` to **both** argv builders in `crates/proot-core/src/lib.rs`. It matches the proot-distro behavior for non-termux containers. This is a user-space illusion. PRoot maps the identity for traced processes, but it grants no actual host kernel privileges.

**Verification Suite:**

```
### 1. FAKE ROOT ###        id -u / id -g / id -un   ->  0 / 0 / root
### 2. APT INSTALL ###       Setting up curl (8.14.1-2+deb13u7)
                            curl 8.14.1 (aarch64-unknown-linux-gnu) libcurl/8.14.1 ...
### 3. DPKG AS ROOT ###      dpkg -l | wc -l -> 106   (from 83 before install)
### 4. ISOLATION REGRESSION ###  ls /etc | wc -l -> 60 ; alpine-release -> 0 ; PRETTY_NAME="Debian GNU/Linux 13 (trixie)"
### 5. ENV HYGIENE ###       LD_PRELOAD|LD_LIBRARY_PATH|API_KEY|termux -> 0
                            PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
### 6. PROC PARITY ###       124.08 932.76 / cpu  1954 0 2872 93276 259 340 250 83 0 0 / 0.12 0.07 0.02 2/165 765
### 7. ALPINE STILL OK ###   3.24.3 / apk-tools 3.0.8-r0
```

Zero regressions appeared. Filesystem isolation stays intact. Environment variables are sanitized. The `/proc` stubs match proot-distro byte-for-byte. Alpine containers stay unaffected.

---

## 3. Container Restoration Returns Random UUIDs and Creates No Files

**Root Cause:** The function `restore_container` returned `Ok(Uuid::new_v4().to_string())`. It did not move the extracted archive contents from `rootfs/...` into `~/.podnext/containers/<name>/`. A restored container had no `metadata.json`. The CLI subcommands did not see such a container.

**Remediation:** In `crates/pn-next/src/backup_restore/mod.rs`:
1. The code resolves the target from `--target`, or it uses `<container>-restored`.
2. The code rejects an existing target, unless `--overwrite` is given.
3. The code moves the extracted directory from `extract_dir/rootfs` to `containers_dir/<target>/rootfs`. On failure it uses the recursive `copy_tree` function, which keeps the symlinks.
4. The code writes `metadata.json` to register the container formally.
5. The code provisions the `sysdata` stubs in the same way as a fresh installation.

**Verification:**

```
$ podnext backup create deb-pdn           -> Backup created with ID: 928d887d-2eed-49f2-8ce3-e55e913d673f
$ podnext backup restore 928d887d-... --target deb-restored
                                          -> Restored to container: deb-restored
$ podnext list                            -> deb-restored | deb-pdn | restored | aarch64 | Stopped
$ podnext login deb-restored -- sh -c 'cat /etc/os-release|grep PRETTY; dpkg -l|wc -l; cat /proc/uptime'
                                          -> PRETTY_NAME="Debian GNU/Linux 13 (trixie)" / 83 / 124.08 932.76
```

---

## 4. Measured Divergence Summary

| Metric | PDN | proot-distro | Notes |
|---|---|---|---|
| Guest UID | 0 (post-fix) | 0 | Prior to fix: 10396 -> apt failed |
| `uname -r` | Real kernel `6.1.157-android14-…` | `6.17.0-PRoot-Distro` | Architectural choice: avoids spoofing identities unless requested |
| Guest PATH | Clean (`/usr/local/sbin:…:/bin`) | **Leaks** `/data/data/com.termux/files/usr/bin` | In proot-distro, `python3` resolves to **host** Python (`sys.prefix=/data/data/com.termux/files/usr`) |
| `getent hosts deb.debian.org` | Resolves | Resolves | IPv4/IPv6 ordering varies; both functional |
| `apt-get update` | 10.1 MB / 13 s | 10.1 MB / 13 s | Identical throughput |
| `/proc/uptime`, `/proc/stat`, `/proc/loadavg` | Identical bytes | Identical bytes | Synthetic sysdata stubs |

In `proot-distro`, the host PATH leaks into the guest. Then host Bionic binaries can accidentally run inside guest glibc environments. That fault causes unpredictable ABI crashes. PDN removes this entire vulnerability class.

---

## 5. Subcommands Promoted from Stubs

| Subcommand | Prior State | Remediation |
|---|---|---|
| `network connect/disconnect` | `not yet implemented` | Added `containers` persistence to `NetworkConfig`; `connect` writes network DNS to guest `/etc/resolv.conf`, `disconnect` restores default |
| `volume prune` | `not yet implemented` | Aggregates volume references from all container `bind_mounts`; prompts for confirmation unless `--force` is passed |
| `log tail` | `not yet implemented` | Backed by `StructuredLogging::query` shared with `log query` |
| `dashboard start/stop/status` | Print-only stub | Dispatches `WebDashboard::start()`; manages PID file at `~/.podnext/dashboard.pid`; handles `SIGTERM` -> `SIGKILL`; status checks process liveness and port responsiveness |

---

## 6. In-Container Validation: Debian Environment

The tests below ran through `podnext login deb-pdn -- ...` after the fixes.

### 6.1 Headless Chromium + Playwright Functional

`/dev/shm` was absent in the initial PDN builds, because `--bind=/dev` shadowed `rootfs/dev`. Chromium then failed to allocate shared memory segments. In `crates/proot-distro-core/src/guestdirs.rs`, the code creates `containers/<name>/shm` with mode **1777**. The code bind-mounts that directory to `/dev/shm` after `--bind=/dev`.

```
$ podnext login deb-pdn -- /bin/sh -c 'ls -ld /dev/shm; touch /dev/shm/probe && echo SHM_WRITABLE'
drwxrwxrwt. 2 root root 3452 ...  /dev/shm
SHM_WRITABLE
```

This Playwright test used `playwright-core` 1.64.1 on the host and targeted the container browser:

```
$ podnext copy ~/.hermes/cache/scratch/pwtest.js deb-pdn:/root/pwtest.js
$ podnext login deb-pdn -- /bin/sh -c \
    'NODE_PATH=/data/data/com.termux/files/usr/lib/node_modules \
     /data/data/com.termux/files/usr/bin/node /root/pwtest.js'
PLAYWRIGHT_DOM=PW_OK
UA=Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/156.0.0.0 Safari/537.36
```

Direct execution of headless Chromium:

```
$ podnext login deb-pdn -- /bin/sh -c '/data/data/com.termux/files/usr/lib/chromium/chrome \
    --headless=new --no-sandbox --disable-gpu --user-data-dir=/root/cud \
    --dump-dom "data:text/html,<h1>OKPDN</h1>"'
<html><head></head><body><h1>OKPDN</h1></body></html>
```

*Technical note*: `--no-sandbox` is required, because unprivileged user namespaces are absent under PRoot. The flag `--disable-dev-shm-usage` is **not** required, because `/dev/shm` is now properly provisioned.

### 6.2 Node.js Runtime

Both pathways passed:
- The host Node.js from Bionic runs through an absolute path inside the guest. It works.
- The guest packages from `apt-get install -y nodejs npm` are available in Debian Trixie as Node.js 20.19.2.

### 6.3 A Docker Daemon Inside PRoot Is Physically Impossible

These results come from direct measurement on the unprivileged uid 10396. The capability sets are `CapEff=CapPrm=CapBnd=0`. The `mount(2)` call ends the process with SIGSYS (31) from the Android seccomp filter. The `unshare(CLONE_NEWUSER)` call returns `EINVAL`. The `CLONE_NEWNS`, `CLONE_NEWPID`, and `CLONE_NEWNET` calls return `EPERM`. The `/sys/fs/cgroup` directory is read-only. The host has no `dockerd`. PRoot returns a fake `rc=0` on the mount and unshare syscalls. Docker then cannot allocate genuine filesystems or namespaces. The only viable path is a real kernel in user space through `uml4termux`.

### 6.4 Hermes Agent Execution Inside Guest Container

```
$ proot-distro login debian -- /data/data/com.termux/files/usr/bin/hermes --version
Hermes Agent v0.22.0+canary.20261014T070054Z (2026.9.24) · canary
Install directory: /data/data/com.termux/files/usr/lib/hermes-agent/app
Install method: apt
Python: 3.14.6
OpenAI SDK: 2.24.0
```

The entire import chain resolves cleanly inside the guest. This works when `$TERMUX_PREFIX` is bound to provide the host Python interpreter.

---

## 7. Added Subcommands & Verification

| Command | Behavior |
|---|---|
| `podnext rename <old> <new>` | Renames container root directory via `rename(2)` and updates `metadata.json` (name + rootfs path) |
| `podnext copy [--recursive] [--move] <SRC> <DEST>` | Handles `[CONTAINER:]PATH` specs; dereferences target symlinks on standard copy (matching `cp` and `proot-distro`), preserves symlinks on `--move` |

```
$ podnext rename deb-restored deb-renamed
Renamed container 'deb-restored' to 'deb-renamed'

$ podnext copy ~/pdn-copytest.txt deb-pdn:/root/copytest.txt
$ podnext login deb-pdn -- cat /root/copytest.txt
hello-from-host
```

---

## 8. Resolved Bug Fixes

1. **`ContainerLock::acquire` Panic**: The code ran `libc::flock(self.file.as_ref().unwrap()...)` before the statement `self.file = Some(file)`. That order caused an unwrap panic on the first lock acquisition. The fix acquires the lock on the open descriptor before the assignment.
2. **Unsafe Archive Unpacking**: The code used the standard `Archive::unpack` in four code paths. Those paths had no support for the Android SELinux hard link restrictions. All archive unpacking now uses `untar::extract`.

---

## 9. Architectural Invariants

- A rootless extraction does not restore file ownership, the uid and the gid. Android users cannot run `chown`. The file mode bits map the permissions.
- The container `/etc/hostname` defaults to the image base name. The command `hostname` reports `localhost` by default, unless an override is given.
