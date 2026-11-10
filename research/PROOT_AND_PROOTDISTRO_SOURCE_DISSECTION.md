# Source dissection: proot and proot-distro, read to be applied to PDN

**Status**: research dossier, 6 Nov 2026. This document is a companion to `PDN_vs_PROOT_PROOTDISTRO_DOCKER.md`,
which records what each tool has. It is also a companion to `PATH_TRANSLATION_AND_SHIM_INTEGRITY.md`,
which records the guest↔host boundary. This document shows the inside: what the code does, function by
function, and what PDN must take from it.

**Method, stated exactly, because "read everything" has to mean something:**

- **proot** - cloned from `github.com/termux/proot`. The clone is shallow. This fork builds the proot on
  this device. Per its own `AGENTS.md`, this fork has *no upstream sync*. Scale: **138 `.c` files /
  32,638 lines**, 71 headers, **113 test scripts**,
  `src/{cli,execve,extension,loader,path,ptrace,syscall,tracee}`. Every file was enumerated. A
  **symbol index** was generated mechanically into
  `~/.hermes/cache/scratch/proot_symbol_index.txt`. That index holds 69 files of `src/`, **498 function
  definitions, 636 lines**. The command that produces it is in section 0's reproduction block. The
  functions that carry the semantics PDN depends on were then **read in full**: `path/binding.c`,
  `path/path.c`, and the CLI/extension layout.
- **proot-distro** - read in place from the installed package at
  `$PREFIX/lib/python3.14/site-packages/proot_distro`, where 5.10.3 lives. Scale: **77 Python files /
  29,015 lines**, plus three shell completions for bash, zsh and fish. Every file was enumerated with
  every one of its **top-level symbols** - classes, functions, assignments - into
  `~/.hermes/cache/scratch/proot_distro_inventory.txt`, which holds **77 files, 942 symbols, 1,096
  lines**. For every module, the **module docstring and every function docstring** were extracted
  mechanically by AST, not by grep, and read. The functions whose behaviour PDN **copies** were read as
  code.

**Honest limit**: I did not read all 61,000 lines at statement level. What I did read at statement level is
marked **code**. What I read as a docstring or a signature is marked **surface**. Every claim carries a
`file:function` citation, so the next reader can check it. Anything that I cannot verify is marked **unknown**
in section 6.

**How to reproduce the two mechanical inventories**. The scratch files named above are pruned after a day.
This block regenerates them:

```sh
# proot: clone the fork that builds the proot on this device, then index every file's functions
D=$HOME/.hermes/cache/scratch/proot-src
git clone --depth 1 https://github.com/termux/proot "$D"
find "$D/src" -name '*.c' | sort | while read f; do
  printf '\n### %s (%s lines)\n' "${f#$D/}" "$(wc -l < "$f")"
  grep -nE '^[a-zA-Z_][a-zA-Z0-9_ *]*\(' "$f" | sed 's/^/    /'
done > $HOME/.hermes/cache/scratch/proot_symbol_index.txt
# expected: 69 files, 498 function definitions, 636 lines
grep -c '^### ' $HOME/.hermes/cache/scratch/proot_symbol_index.txt
grep -cE '^    [0-9]+:' $HOME/.hermes/cache/scratch/proot_symbol_index.txt

# proot-distro: the installed package, every file with each of its top-level symbols
S=$PREFIX/lib/python3.14/site-packages/proot_distro
find "$S" -name '*.py' | sort | while read f; do
  printf '\n### %s (%s lines)\n' "${f#$S/}" "$(wc -l < "$f")"
  grep -nE "^(class |def |async def |[A-Z_]+ *=)" "$f" | sed 's/^/    /'
done > $HOME/.hermes/cache/scratch/proot_distro_inventory.txt
# expected: 77 files, 942 symbols, 1096 lines
grep -c '^### ' $HOME/.hermes/cache/scratch/proot_distro_inventory.txt
grep -cE '^    [0-9]+:' $HOME/.hermes/cache/scratch/proot_distro_inventory.txt
```

---

## 1. proot: the layer PDN drives

### 1.1 Architecture, and what each directory is for

| dir | files | what it holds |
|---|---|---|
| `src/cli` | 6 | argument parsing, usage, **the option table and the default bindings** |
| `src/path` | 14 | guest↔host path translation and the binding lists |
| `src/tracee` | 13 | the ptrace state machine, memory access, seccomp |
| `src/syscall` | 25 | per-syscall translation (the largest: `enter.c` 3,069 lines, `exit.c` 845) |
| `src/execve` | 13 | ELF loading, the loader injection, ptrace-based exec |
| `src/extension` | 11 | nine behaviour-changing extensions (below) |
| `src/loader` | 8 | the small ELF blob injected into the guest to bootstrap |
| `src/ptrace` | 6 | the raw ptrace wrappers |

### 1.2 The path translation core, read in full, and it settles two earlier arguments

**Code**: `path/path.c:substitute_path_prefix()`, called from `path/binding.c:substitute_binding()`:

```c
int substitute_binding(const Tracee *tracee, Side side, char path[PATH_MAX])
{
	binding = get_binding(tracee, side, path);
	if (!binding) return -ENOENT;
	if (!binding->need_substitution) return 0;      /* "symmetric" binding */
	...
	substitute_path_prefix(path, ref->length, reverse_ref->path, reverse_ref->length);
	return 1;
}
```

Three facts, all **code**:

1. **The translation rewrites strings.** A guest path becomes a host path when a prefix is replaced.
   Identity is never consulted. Implication for PDN: any path that reaches the guest is a *name*. That
   fact is the premise of the shim dossier - now verified inside proot itself rather than inferred from
   behaviour.
2. **Bindings are matched, not mounted.** The function `get_binding()` walks a sorted list from
   `insort_binding` and `insort_binding2`, and the longest prefix wins. The flag `need_substitution`
   separates a symmetric binding from an asymmetric one. In a symmetric binding, the path is the same on
   both sides and no rewrite occurs.
3. **There is no read-only bind.** A grep for `ro` or read-only across `path/binding.c` and `cli/cli.c`
   matches only `talloc_zero`, `bzero` and `<strings.h>`. Binds are read-write. The only writability
   control is which side of the bind you are on. This upgrades PDN's documented limitation, where
   `--volume :ro` is refused, from a measured behaviour to a source-level fact.

**A correction to my own earlier writing**: I suggested that `add_induced_bindings()` explained the twin
directory tree that PDN's guests see. **Code**: the function is for reconfiguration only. It fires when a
second `-r` is layered over a first tracee, and `tracee->reconf.tracee == NULL` returns immediately. It
exists so a binding from the old config is expressed again in the new one. The lookalike tree in PDN
containers is **PDN's own doing**, consistent with #66's measurement, not proot's.

### 1.3 The default bindings and the classic preload trick

**Code**, `cli/proot.c:142`:

```c
new_binding(tracee, "/dev/null", "/etc/ld.so.preload", false);
```

`/dev/null` is bound over `/etc/ld.so.preload`, so a host that has an `ld.so.preload` cannot inject a
library into guests. This fact matters to PDN for two reasons. First, it is the kind of default worth
a *documentation* note, because someone will eventually ask why that file is empty in every guest.
Second, it is the same category of decision as section 1 of the shim dossier, where one file stands in for another file
by name.

### 1.4 The nine extensions, and which ones PDN already lives with

| extension | what it changes | PDN relevance |
|---|---|---|
| `fake_id0` | fakes uid/gid 0 semantics; this is what `--change-id=0:0:0` rides on | PDN uses it today (`doctor` prints the mapped-root note) |
| `link2symlink` | replaces hard links with symlinks (`link2symlink.c`, 2,003 lines) | PDN has already been bitten by in-place L2S rewriting on relocation |
| `sysvipc` + `ashmem_memfd` | System V IPC and Android shared memory (`--sysvipc`, `--ashmem-memfd`) | PDN sets `--no-sysvipc` semantics and isolates `/dev/shm` per container |
| `kompat` | Android compatibility shims (`kompat.c`, 1,049 lines) | PDN's `/proc` and `/sys` masking exists for the same reason |
| `mountinfo` | fakes `/proc/self/mountinfo` | PDN's bindings work measured that a guest's `/proc/mounts` carries no host binds - this is why |
| `hidden_files` | hides paths from the guest | the primitive PDN would need if it ever hides host storage rather than creating empty lookalikes |
| `port_switch` | remaps listening ports | PDN refuses `--fix-low-ports`, so this is the missing substrate |
| `fix_symlink_size` | reports sane symlink sizes | cosmetic |

### 1.5 Its test suite, which is a behaviour contract PDN can mine

**Surface**: 187 entries in `tests/`, which are C probes named after the syscall surface, plus about 160
shell scripts named by hash. The probe names include `argv.c`, `argv0.c`, `execveat.c`, `open_nofollow.c`,
`open_tree.c`, `fchmodat2.c`, `fork_regs.c`, `getdents.c`, `link.c` and `ptrace-2.c`. The shell script
names start at `test-00000000.sh`. The granularity is syscall-level, not CLI-level. Two uses for PDN.
First, the C probes are candidate regression tests for its own guest behaviour, because a rootfs that
fails `open_nofollow` will fail PDN too. Second, the hash-named scripts are a model for a **contract
corpus** that outlives refactors. `AGENTS.md` also notes that the suite has known failures, so it is a
corpus to sample, not a gate to adopt wholesale.

---

## 2. proot-distro: the doctrines, with citations

The Python package is not "a CLI around proot". It is a small container platform, and its docstrings read
like a design review. Eight doctrines, each stated as a rule PDN can hold itself to.

### 2.1 Every file operation goes through a descriptor in `dirfd.py`, 1,361 lines

**Surface**: `opendir_at`, `open_file_at`, `open_regular_at`, `open_new_at`, `makedirs_at`,
`makedirs_under`, `opendir_under`, `descend_at`, `lstat_at`, `listdir_at`, `copy_file_at`,
`copy_symlink_at`, `copy_tree_at`, `rmtree_at`, `count_tree_at`, `close_frames`.

**Code/docstrings worth quoting**:

- `open_regular_at`: *"O_NOFOLLOW alone is not enough for an endpoint a container can prepare. It keeps a
  pla[ceholder]…"* - the check is O_NOFOLLOW **plus** an `fstat` type check, because a FIFO or device node
  under the name is just as hostile.
- `open_new_at`: *"O_EXCL, so no entry already carrying the name is ever written through."*
- `copy_metadata`: *"expressed against file descriptors, so no path - and therefore n[o TOCTOU]"*.
- `rmtree_at`: *"descending, never following symlinks. A symlink is unlinked, never traversed, so this
  cannot reach outside the tree it wa[s given]"*.
- `close_frames`: every walk keeps its stack of directory descriptors and closes them on interruption.
- Sparse-aware copying: `_looks_sparse`, `_data_extents` for SEEK_DATA and SEEK_HOLE, `_copy_extents`,
  `_copy_skipping_zeros` - a tarball of a sparse rootfs does not become gigabytes.

**Rule for PDN**: no operation that writes into a container's tree can take a composed path. Re-read
PDN's own `copy`, `sync` and rootfs writes against this list. The acceptance test is a container that
plants a symlink or FIFO where the host is about to write, and an operation that refuses and does not
follow it.

### 2.2 The runtime directory is a trust root, and it is guest-writable: `locking.py`, `session.py`, `clear_cache.py`

**Code/docstring**: `_locks_dir_fd` is *"RUNTIME_DIR is the trust root - the program's own state directory"*.
`_open_cache_root` is *"On Termux the cache lives *inside* RUNTIME_DIR, whose components are guest-writable -"*.
`_open_lock_file` is *"O_NOFOLLOW plus open_regular_at()'s type check, so neither a symlink nor a FIFO under
the [name is followed]"*. The functions `_is_planted`, `_is_denied` and `_drop_planted` handle a **hostile
lock path**. Someone can plant a directory or a symlink at a lock name to deny service. The code has a typed
response for each.

**Rule for PDN**: state that the runtime directory is adversarial, and treat every entry in it as
untrusted input - because on this platform the guest runs as the same uid and can write there.

### 2.3 Locks carry a hint, and "cannot measure" is a state in `locking.py`

**Surface**: `ContainerLock`, `BuildLock`, `busy_locks`, `_lock_info_at`, `LockStateUnknown`.
**Code/docstring**: `_lock_info_at` is *"Reads the lock file's first line [PID + command name] and returns a
parenthesised note s[o]..."*. `busy_locks` is *"Both namespaces are scanned, so one call covers every command
that writes to the downlo[ad cache]"*. `LockStateUnknown` exists as a first-class outcome for a probe that
cannot answer.

**Rule for PDN**: when PDN refuses because a lock is held, print *who* holds it. Keep a distinct state
for `could not tell` rather than collapse it into `busy`. PDN already does this for measurements with
exit 2. It is the same discipline applied to locking.

### 2.4 Untrusted digests never become paths: `docker/cache.py` and `install_local.py`

**Code**, `docker/cache.py:validate_digest`:

> *"Used as a choke point before any conversion of an untrusted digest into a filesystem path [layer cache,
> OCI blob layout]. Accepts any OCI-conformant algorithm/hex pair. Rejects anything containing path
> separators or empty/dot components."*

and `install_local.py:_oci_blob_path` - *"Validates the digest first so a crafted index.json cannot route
the lookup through a member name wit[h a traversal]"*, plus `_oci_read_capped` - *"The size in the header is
the archive's to declare and the member can lie"*.

**Rule for PDN**: PDN's `install` takes digests from registries, and soon from local archives. A digest is
a **filename** in any blob store PDN builds. Use one validation function, called before any path is
composed, and a length cap on every JSON member read from a stranger's archive.

### 2.5 Credentials are bound to an origin in `docker/transport.py`

**Code**, `same_auth_origin` docstring, which records a fixed vulnerability in its own words:

> *"An Authorization header is addressed to an origin: the scheme, the host and the port. To carry it to
> another origin is to hand the secret to whoever answers there. The comparison used to be of the two
> netlocs. That comparison let a same-host redirect, or an upload Location, that changed only the scheme
> keep the header. https://reg.example -> http://reg.example put the Bearer token ... on the wire in clear."*

The function `download.py:check_redirect` refuses **a redirect from HTTPS to plain HTTP**. The function
`http_opener` deliberately does not install urllib's `ftp://`, `file://` and `data://` handlers.

**Rule for PDN**: this is the exact gap that #74 records from the other side. PDN's install path talks to
registries and, through `install.sh`, to GitHub. It has no origin binding, no downgrade refusal and no
checksum. The reference's design is the one to copy, including the refusal to follow a scheme downgrade.

### 2.6 Retry is a policy, not a loop in `helpers/download.py`

**Surface**: `retry_http`, `is_retryable_http_error`, and `retry_after`, which accepts delta-seconds
**and** HTTP-date. Also `_quota_spent`, `declared_length`, `require_complete_body`,
`is_cert_verification_error` and `is_plaintext_http_tls_error`.

**Docstrings worth quoting**: *"Deterministic failures are not retried, because they cannot succeed on a
repeat request: an HTTP client error, 4xx..."*. The function `require_complete_body` is *"The other half of
a truncated response, and the quiet one. A cut **chunked** body raises IncompleteRead on its own..."*.

**Rule for PDN**: PDN's own preflight script grew a hand-rolled three-attempt retry this release, after a
transient `gh run download` failure was misdiagnosed as a missing artifact. See rule 79 in the harness
notes. The reference's distinction is the version to implement. Retry a transient failure. Never retry a
deterministic failure. Honour `Retry-After`. Verify the declared body length. Put this logic in the tool,
not in a script around it.

### 2.7 Builds are content-addressed, cached, and leak nothing in `build_engine/`

**Surface**: `do_run` is *"Cache lookup happens first: a recipe-hash hit applies the cached layer and skips
proot entirely"*. `_run_extra_inputs` is *"The identity variables are part of that state: a step can read
$PD_IMAGE the way it reads any ENV"*. Also `_identity_env`, and `_chdir_to`, which does fchdir into the
rootfs descriptor for `--rootfs=.`. The function **`_become_subreaper`** is *"Ask the kernel to reparent
orphaned descendants onto this process. This is what makes a step's leftovers findable at all. A
backgrounded command outlives the shell"*, plus `_children_of`. The function `handlers.do_workdir` is
*"Emits a thin layer covering any newly-created ancestor directories so the path still exists when the
imag[e is run]"*. The function `copy_step._copy_from_context` is *"A source that resolves outside the
context does not exist as far as this is concerned"*.

**Rule for PDN**: two mechanisms are worth taking even if PDN does not adopt the whole build engine.
First the **subreaper**. PDN's soak harness counts leftover processes after a session. A subreaper
launcher reaps them, and makes what remains findable. Second the **thin layer for created ancestors**.
PDN's `build local` must not produce an image whose WORKDIR does not exist when run.

### 2.8 The session is a file held open, and the environment survives login in `session.py` and `login/env.py`

**Surface**: `register_session` is *"Must be called from the process that immediately exec's into proot, right
before [exec]"*. It **clears O_CLOEXEC** on the registry fd, so proot and every descendant keep it. Also
`session_is_live`, a shared non-blocking flock probe, and `session_holders`, which reports the PIDs that
still hold it. Also `_validate_record`, which is *"the recorded one has to be it"*. The PID in the file name
must match the record, and that rule stops a stale or forged file from resurrecting a dead session.

**And a bug class PDN must check itself for**: `login/env.py:inject_termux_profile` is *"Write a profile.d
snippet that re-applies the login-time environment. Login shells source /etc/profile, which reinitialises
the environment and discar[ds it]"*. The reference found that a `login` into a shell loses the variables the
launcher exported. It fixed the problem when it wrote `/etc/profile.d/termux-profile.sh` into the guest.

**Rule for PDN**: the detach/attach round trip PDN shipped, item #57, tracks a supervisor by PID and a lock.
The reference's test, "live iff some process holds the fd", is stronger and has no PID-reuse hazard. PDN
must also test whether its own `login` loses exported variables to `/etc/profile`. The fix is eight lines
and a file in the guest. If the bug is present today, it stays invisible until a user's `PATH` "resets
itself" after login.

### 2.9 Install is one command with three sources, and the alias is derived in `commands/install.py` and `docker/refs.py`

**Surface**: `_is_local_path` is *"A reference is local only when it has an explicit path syntax: a leading
'/', './', '../'"*. Also `_is_url`, and `_derive_local_name`, which is *"Derive a container alias from an
archive filename"*. Also `_resolve_install_name`, and `command_install`, which is *"Install a container
from a Docker image, URL, or local archive."* Also `login/bindings.py:storage_bindings` and
`login/bindings.py:system_bindings`, the Android bind set that PDN does not add and that #66 measured as
absent. Also `login/proot_cmd.py:_add_termux_app_binds`, whose docstring distinguishes container types:
*"Normal-type containers only: a termux-type guest must not see the host's /data/data/com.termu[x]..."*.

**Code**, `derive_alias`, the §10.1 acceptance item, now implemented:

```
'ubuntu:24.04'           -> 'ubuntu'      'ghcr.io/foo/bar:tag'   -> 'bar'
'myuser/img:tag'         -> 'img'         'localhost:5000/foo:tag'-> 'foo'
'debian@sha256:<hex>'    -> 'debian'      # routes through parse_image_ref, so a port
                                          # colon is not mistaken for a tag
```

**Rule for PDN**: §10.1's install items are not research questions any more. They are three branches and a
name derivation, with an existing reference implementation to compare against. Note the licence boundary.
PDN is GPL-2.0-or-later, and the reference is GPL-3.0. Match the architecture and the behaviour. Do not
transcribe the code.

### 2.10 Cache is collected by reference count in `commands/clear_cache.py`

**Surface**: `command_clear_cache` with `--orphan` or `--build-cache`, and `_referenced_blob_names`, which
is *"Return the layer-cache file names that are still referenced. The manifest cache is always a reference
source. The build index is a reference source unless the caller[ says otherwise]"*. Also `_tree_size`,
measured through descriptors, and `_relax_cache_root`, which empties a sealed tree.

**Rule for PDN**: PDN has a cache directory and no eviction. If it adds one, the reference's shape is the
one to copy. **Know what is referenced before the tool deletes anything**. Offer an orphan sweep separately
from a full clear.

---

## 3. What to apply to PDN, ordered, with the check that settles each

| # | take | from | acceptance test | when |
|---|---|---|---|---|
| 1 | digest validation as a choke point before any path is composed from it | `docker/cache.py:validate_digest` | a crafted digest with `..` is refused with the offending value quoted | now |
| 2 | origin-bound credentials: no auth across scheme/host/port, refuse HTTPS→HTTP redirects | `docker/transport.py`, `download.py:check_redirect` | a same-host HTTPS→HTTP redirect drops the Authorization header and the transfer fails | now |
| 3 | a retry policy that distinguishes transient from deterministic, honours `Retry-After`, and verifies the declared body length | `helpers/download.py` | a 404 is not retried; a truncated body fails with both lengths | now |
| 4 | lock hints: the refusing message names the holder | `locking.py:_lock_info_at` | a second `install` of the same container prints the PID of the first | now |
| 5 | check whether `login` loses exported variables to `/etc/profile`, and if so persist them into the guest the way the reference does | `login/env.py:inject_termux_profile` | **measured, and the bug class does not apply**: `PDN_PROBE` exported before `login` is absent from the guest in *both* a login shell and a non-login shell, and the guest environment is exactly `COLORTERM HOME LANG PATH TERM` - PDN forwards no host variables at all, which is deliberate ("isolated environment") and safer than the reference's profile re-injection | resolved |
| 5b | retain and apply the image's own config: `Env` into the session environment, and `WorkingDir`, `Entrypoint`/`Cmd` for `run` | `paths.py:container_image_config`, `login/env.py:image_env_pairs`, `handlers.py:do_entrypoint`/`do_cmd` | **measured gap**: no container's `metadata.json` carries an `Env` key (alpine: only `created_at/image/name/rootfs/status/tag`), and `grep -rn 'image_config\|\.Env\b\|config\.config' crates/podnext/src` returns nothing - PDN installs a rootfs and discards the image config entirely | now |
| 6 | descriptor-based file operations for everything that writes into a container tree | `dirfd.py` (whole module) | a container with a symlink/FIFO planted at a target name makes the operation refuse, not follow | v0.3.0 |
| 7 | derive the container alias from a reference, and accept a local tarball / OCI layout / URL as install sources | `install.py`, `install_local.py`, `derive_alias` | §10.1's four acceptance rows | v0.3.0 |
| 8 | the Android bind set on `login` (`storage_bindings`, `system_bindings`, Dalvik caches), with the termux-type exclusion | `login/bindings.py`, `login/proot_cmd.py` | `/sdcard` inside the guest is the host's storage, and a termux-type guest does not see the app's private dirs | v0.3.0 |
| 9 | reap instead of count: make the launcher a subreaper so a session's leftovers are reaped and the rest are findable | `build_engine/run_step.py:_become_subreaper` | the soak harness reports zero leftovers *and* an explicit list when it does not | v0.3.0 |
| 10 | a build that emits the thin layer its WORKDIR needs, and a recipe-hash cache for RUN steps | `handlers.py:do_workdir`, `run_step.py:do_run` | a built image's WORKDIR exists at run time; a second build of the same recipe hits the cache | v0.3.0 |
| 11 | cache eviction by reference count (`--orphan` sweep separate from a full clear) | `clear_cache.py` | `clear-cache --orphan` removes exactly the unreferenced blobs, counted before and after | v0.3.0 |
| 12 | a syscall-level probe corpus, mined from proot's own C tests | `tests/*.c` | PDN's guest-behaviour suite includes probes for `open_nofollow`, `execveat`, `fchmodat2`, `getdents` | v0.3.0 |
| 13 | document the default bind `/dev/null` → `/etc/ld.so.preload` and any other default PDN inherits from proot | `cli/proot.c:142` | `known-limitations.md` explains why that file is empty in a guest | now |

## 4. What PDN already does as well or better

Not copying blindly matters as much as copying. Compared against the source:

- **`doctor`** has no counterpart anywhere in either codebase. The reference's closest thing is scattered
  checks inside the commands it needs them for.
- **The release/verification machinery** is more developed than anything the two comparators ship. That
  machinery includes `local_gate.sh`, `preflight_release.sh`, `smoke_release.sh`, the tracked help
  fixture, the warning ratchet and the 129-predicate ledger. The reference has no equivalent of a release
  smoke test on the artifact.
- **Documentation of limits**: PDN's `known-limitations.md` records declared simplifications and measured
  exceptions. Neither comparator does this at all, and their docstrings - excellent as they are - carry
  rationale that never reaches a user.
- **OCI whiteout handling** in PDN's `untar.rs`, commit `aa3d2b0`, already matches
  `tar_extract.py:_apply_whiteout` in intent. The difference is descriptor discipline, not behaviour.

## 5. The one design decision this dissection changes

The shim dossier concluded that any integration that hands the guest a host *file* by name is spoofable
under this proot, and it recommended an IPC surface. The source adds the missing half: the reference's own
state directory is **guest-writable**, as `clear_cache.py:_open_cache_root` shows. That is why it defends
every name in it with O_NOFOLLOW, type checks and O_EXCL. So the choice for PDN is not "path or socket" but
**which names the guest can write**:

- names inside `RUNTIME_DIR` are guest-writable today, as **code** shows, because the uid is the same and
  no read-only bind exists. PDN must treat them as hostile input, not as PDN's own state.
- a name that PDN *executes* must never live in that set.
- a value that must cross the boundary must cross as a protocol message over a socket that PDN creates.
  Apply the file-descriptor discipline above to the socket's own path.

## 6. Unknowns

- **`src/syscall/*` and `src/tracee/*` were read at surface level only**. Those are 25 and 13 files, the
  largest bodies in proot. The per-syscall translation is the source of PDN's future guest-behaviour
  surprises. A second pass that targets `syscall/enter.c`, 3,069 lines, is the obvious next read.
- **The extension enable/disable logic was not traced**: which extensions proot turns on by default, and
  under which conditions, is `extension/extension.c` plus each extension's constructor - unread here. PDN's
  argv assumptions depend on it.
- **Whether the reference's `help` pages render as well as their source suggests** - `commands/help/pages.py`
  is 1,080 lines of content and was not exercised.
- **The `login` environment question from item 5 is answered. PDN does not have the reference's bug, and
  it does not forward host variables at all.** The measurement was on `alpine`. `PDN_PROBE`, exported
  before `login`, is absent inside the guest in both `sh -c` and `sh -lc`. The guest environment is exactly
  `COLORTERM HOME LANG PATH TERM`, with `PATH` from the image's own `/etc/profile`. PDN's isolation is a
  deliberate design, called the "isolated environment", rather than an accident. It is the safer side of
  this trade, because the reference's `inject_termux_profile` deliberately re-applies host-derived
  variables inside the guest. What PDN lacks is the *other* half, the image's own `Env`, which it never
  reads. That gap is item 5b.
- **`login --user`**: the reference implements the lookup in `login/passwd.py:read_passwd_entry` and
  `find_passwd_by_uid`. PDN refuses the flag outright. If PDN ever implements it, the source to study is
  that module plus `handlers.py:do_user`. The refusal PDN ships today is honest, so this is a feature
  decision, not a defect.
- **Release verification of `v0.2.0`, done during the writing of this dissection**: the published
  aarch64 asset reproduces the tracked help contract **byte for byte** - `IDENTICAL: 210617 bytes, sha256
  8236a573be5952cdd983c5c8f6cd3833f8e6aa0b1782e1dc1cd08c638fd2b8c5` - and `scripts/smoke_release.sh`
  passes 14/14 against those same published bytes. One defect was found in the same pass and fixed. The
  release body was 8 KB of notes on a single line. The workflow rewrote every newline to a literal
  `\n` before it handed the text to an Actions output. See #75. It is the reason item 3 of section 3 exists.
