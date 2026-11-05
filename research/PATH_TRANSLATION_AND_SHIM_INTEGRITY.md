# PATH translation, shim integrity, and where the next releases can go

**Status**: research dossier, 6 Nov 2026. Nothing in here is implemented. Every claim is marked
**measured** (with the command that produced it), **read** (from a source file on this device), or
**unknown** - because the interesting part of a plan is which of its parts are guesses.

**Scope**: four questions, asked together because they constrain each other.

1. feature expansion - where `podnext` is behind the reference, and what is worth taking;
2. ecosystem and infrastructure - what has to exist for the project to be usable and releasable by someone
   other than its author;
3. user-facing simplification - what the current surface costs a person who just wants a shell;
4. **PATH translation and shim integrity** - the one with a demonstrable security edge, and the reason this
   document leads with evidence rather than with a roadmap.

The fourth is first because it has a proof, and because it silently constrains the first three: any
convenience that reaches from the guest back to the host tools is a convenience built on a name, and a name
can be taken.

---

## 1. The demonstration: PATH resolution in `podnext` is name-based, and it can be hijacked

**Measured.** `podnext` resolves its container runtime by scanning `PATH` for a file called `proot`:

```
$ sed -n '70,86p' crates/proot-core/src/lib.rs
/// Resolve the proot binary to an absolute path.
///
/// Required once the child environment is cleared: Rust's Unix `Command`
/// searches a bare program name in the *child's* PATH, so the plain name
/// "proot" would no longer be found once PATH is the guest's.
fn proot_binary() -> String {
    if let Ok(explicit) = std::env::var("PROOT_BIN") { if !explicit.is_empty() { return explicit; } }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let candidate = std::path::Path::new(dir).join("proot");
        if candidate.is_file() { return candidate.to_string_lossy().into_owned(); }
    }
    "proot".to_string()
}
```

A file is a name plus contents. The function checks the name and that it exists, so a shim wins:

```
$ mkdir -p $PREFIX/tmp/shimdemo/bin
$ printf '#!/bin/sh\necho "SHIM-WINS: aku bukan proot yang asli" >&2\nexit 7\n' > $PREFIX/tmp/shimdemo/bin/proot
$ chmod +x $PREFIX/tmp/shimdemo/bin/proot
$ PATH="$PREFIX/tmp/shimdemo/bin:$PATH" podnext run alpine -- true
SHIM-WINS: aku bukan proot yang asli
$ echo $?
7
```

**Measured, and this is the part that matters**: `rc=7` is the shim's exit status, delivered to the caller
as the container's result. A caller cannot tell this from a genuine container failure that happened to exit
7. No confirmation was asked for, no file outside the container was touched, and the container was never
entered - the hijack happens **before** `proot` confines anything, on the host side.

The severity on this device is bounded by the fact that the host user already owns `$PREFIX/bin`, so a host
attacker who can write there does not need this trick. What the trick *does* establish is the property:
**every name-based hand-off between the guest-facing machinery and the host is a place where content can
stand in for identity.**

### 1.1 The reference implementation already treats this as a design constraint

**Read**, `$PREFIX/lib/python3.14/site-packages/proot_distro/arch.py:221-244`, docstring of
`get_proot_bin()`:

> The answer is always absolute, and a lookup that finds nothing is a refusal rather than the bare name
> "proot". Both callers hand this string to an exec that resolves it against an environment the *image* has
> a say in -- `login` to `os.execvpe(child_env)`, the build's RUN step to `subprocess.Popen(env=child_env)`,
> which looks a name with no directory in it up in that environment's PATH. PATH is deliberately not blocked
> there, being the guest's to choose, so a bare name meant an image's `ENV PATH=/tmp/mine` could decide
> which binary this process became -- **outside any container, before proot had confined anything.** A
> relative name is the same hole one step removed, and worse once `login` chdirs into the rootfs: it would
> resolve against a file the guest wrote.

That is the same failure this document measured against `podnext`, and the reference closed it three ways
that `podnext` has not:

| reference (`proot-distro`) | `podnext` today |
|---|---|
| absolute path, always | PATH scan, absolute only if the file exists |
| a lookup that finds nothing is a **refusal** | falls back to the bare name `"proot"` |
| `PD_PROOT_BIN` override **validated** (exists, executable) or exit | `PROOT_BIN` trusted verbatim |

`podnext`'s `PROOT_BIN` is the escape hatch without the validation, and the bare-name fallback is the hole
the reference names explicitly.

### 1.2 The guest side is worse, and it is where PATH translation would introduce it

**Read**, `proot_distro/constants.py:109-114`: on Termux the **guest** default PATH is

```
/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/local/games:/usr/games:$TERMUX_PREFIX/bin:/system/bin:/system/xbin
```

so the guest is invited to call host tools by name. **Read**, `proot_distro/atomic.py:60-62`:

> On Termux the directory really is shared: RUNTIME_DIR and BASE_CACHE_DIR both sit under the
> `$TERMUX_PREFIX` **bound read-write into every non-isolated container.**

So in the reference's design the guest can write into the host prefix - and the reference's own authors
defend a TOCTOU/symlink-swap in their cache write against exactly that ("a process sharing the directory
reads it out of readdir(), unlinks it and puts a symlink in its place"). They defend the *cache* with
`O_EXCL` descriptors; the general property remains: a guest process can create a file that a later **host**
invocation will find by name and run.

**Measured, today's `podnext`**: it does **not** do the translation, and it does **not** bind the prefix.

```
$ podnext run alpine -- sh -c 'echo $PATH'
/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
$ podnext run alpine -- sh -c 'ls -d /data/data/com.termux/files/usr; touch /data/data/com.termux/files/usr/tmp/_w'
/data/data/com.termux/files/usr
touch: /data/data/com.termux/files/usr/tmp/_w: No such file or directory
```

The path exists inside the guest as the guest's **own empty directory** (the #66 finding, confirmed here:
the host's `$PREFIX/tmp` exists, so if this were the host prefix through a bind, `touch` would have
succeeded). This is a **useful accident of the current state**: `podnext` cannot yet be attacked from inside
a guest by writing a host tool, because it exposes no host path into the guest. Adding PATH translation -
the obvious convenience, and the thing the reference does - would create that surface.

### 1.3 What PRoot cannot do about it

**Measured**: proot 5.1.107.101 has no read-only bind form. `podnext` documents the consequence it already
hit (`--volume HOST:GUEST:ro` is refused rather than silently mounted writable, known-limitations). So:

- a bind cannot be made immutable from the host side;
- ids inside the container are mapped to the host uid, so file ownership cannot distinguish a file the guest
  wrote from one the host wrote;
- binds are resolved by path string, not by identity.

**Therefore**: under this PRoot, for any integration that puts host files where the guest can see them,
**prevention is impossible**. The honest goals are *validation* (refuse to run something that is not what we
mean) and *detection* (notice that it changed), plus *not creating the surface at all* where the convenience
does not justify it.

### 1.4 The prefix path already exists inside the guest as an empty tree the host cannot read

This is the measurement that decides what PATH translation can even mean here. `podnext` creates, inside
every container's rootfs, a lookalike of the Android path tree - `data/data/com.termux/files/usr`,
`system`, `vendor`, `sdcard`, `storage/emulated/0`, `apex`, `product` - and the host sees the prefix
lookalike as **mode 000**:

```
$ ls -ld .../containers/alpine/rootfs/data/data/com.termux/files/usr
d---------. 2 u0_a396 u0_a396 3452 Nov  4 07:30 .../rootfs/data/data/com.termux/files/usr
$ ls .../containers/alpine/rootfs/data/data/com.termux/files/usr/bin
ls: Permission denied
```

Inside the guest the same path is an empty directory it owns, and **the guest can build the rest of it**:

```
$ podnext run alpine -- sh -c 'mkdir -p /data/data/com.termux/files/usr/bin && echo MKDIR-BISA'
MKDIR-BISA
$ podnext run alpine -- sh -c 'printf "#!/bin/sh\necho GUEST-SHIM\n" > /data/data/com.termux/files/usr/bin/pkg && echo SHIM-BISA'
SHIM-BISA
```

Three consequences, all of them design-relevant:

1. **A naive PATH translation is inert, not dangerous.** `PATH=$PATH:$TERMUX_PREFIX/bin`, using the host
   string, resolves *inside the guest* to that empty guest-owned tree. It would look implemented and do
   nothing, because the host prefix is not bound there.
2. **Making it work requires a bind** (`-b $TERMUX_PREFIX:$TERMUX_PREFIX`, which is what the reference
   does), and a bind here is read-write with no read-only form (section 1.3) - so the guest can rewrite
   whatever the translation put there.
3. **The shim can live where the host cannot look.** Once the guest creates
   `.../usr/bin/pkg`, a guest-authored file stands at exactly the path string a translation would trust -
   and the host's own audit tools cannot enumerate it, because the parent is mode 000 on the host side. The
   sentence in the reference's own source, that "on Termux the directory really is shared", is an
   understatement in this configuration: it is shared *and* the host half is dark.

That third point is why option B is not paranoia. It is the only mechanism in the list that answers "is the
file I am about to execute the file I recorded", and no cheaper signal exists:

```
$ ls -Z .../rootfs/etc/_guest_marker      # written from inside the guest
u:object_r:app_data_file:s0:c140,c257,c512,c768   uid:gid 10396:10396
$ ls -Z .../rootfs/etc/_host_marker       # written by the host, same directory
u:object_r:app_data_file:s0:c140,c257,c512,c768   uid:gid 10396:10396
```

**Measured**: SELinux context and ownership are identical, so neither distinguishes a file the guest wrote
from one the host wrote. The `s0:` category set does not carry the distinction either.

---

## 2. Design options, each with the check that would settle it

| # | Option | Closes | Cost | Acceptance test |
|---|---|---|---|---|
| A | Resolve to an absolute path; validate `PROOT_BIN` (exists, executable, absolute); **refuse** instead of falling back to a bare name | T1 partially | hours | a shim earlier in PATH is not executed; `PROOT_BIN=/nonexistent` exits with a named reason; `PROOT_BIN` relative exits 2 |
| B | **Pin by identity**: resolve once, record `(dev, ino, size, mtime, sha256)` in the runtime dir, verify before every use, refuse and say which field changed | T1 fully; makes T2 detectable | days | replacing the recorded proot with a byte-identical file passes; replacing it with any other file fails with `sha256 mismatch` and names both digests |
| C | **Do not expose host tools by name at all.** Put the interface on the IPC surface `podnext` already has (`podnext dbus serve`, socket activation), and have the guest-side entry point be a generated shim that speaks the protocol | T2, T3 by construction | weeks | the guest can list what the host offers through the socket; a guest-written file named like a host tool is never executed by the host; the socket rejects a peer without the per-session token |
| D | If PATH translation is still wanted: **generate** the shim directory from a committed manifest with digests, bind it read-write (unavoidable), and re-verify the digests on every `podnext` invocation | detection only | days | deleting or editing one shim makes the next `podnext` invocation fail with the digest that changed, and the message says "detection, not prevention" |

**Recommended order**: **A now** (it is small, it closes the demonstrated hole, and it needs no new surface),
**B next** (identity, so the answer is a number rather than a promise), **C as the direction for v0.3.0**
(the mobility polygon will want host integration anyway, and an authenticated protocol is the only variant
that cannot be defeated by a file), **D only if someone insists on PATH translation**, labelled in its own
documentation as detection.

Two design rules fall out of the measurements, and they are cheap to hold:

- **Nothing the guest can write may be found by name by the host.** That is option C stated as a rule.
- **`--isolated` must never receive host PATH translation or host binds.** The reference has the same flag
  for the same reason; if translation lands, the flag becomes the security boundary and deserves a test that
  asserts the guest sees `$TERMUX_PREFIX/bin` as empty.

---

## 3. Feature expansion - where the reference is ahead, and what is worth taking

Already itemised with acceptance tests in roadmap section 10 (now `v0.3.0`), so this section adds only what
this round's measurements say about the *shape* of the work:

- **Install surface is narrow. Measured**: `podnext install --help` takes a registry reference only
  (`install <IMAGE>`), with `--name` and `--force`; the reference also installs a local rootfs tarball, an
  OCI image-layout archive with digest verification, and a URL with a checksum. The first of those three is
  the cheapest and the most useful on a phone with a slow connection: it turns "install" into a file
  operation.
- **Multi-arch is one flag away from useless. Measured**: `--qemu <PATH>` exists, and the reference's
  emulator path additionally binds `/apex`, `/linkerconfig/ld.config.txt`, `$TERMUX_PREFIX`, `/system`,
  `/vendor`, `/plat_property_contexts`, `/property_contexts` (**read**, `arch.py:308-315`) - i.e. making
  QEMU work under Android needs a bind set `podnext` does not have. Taking the flag without the binds would
  ship a documented feature that cannot work.
- **Export/import and registry `push`/`pull` are blocked by the same missing thing: the store keeps no layer
  blobs** (#37, #39). Both features are cheap once that exists and impossible before it, so the image store
  is the spine - as section 4.4 already says.

## 4. Ecosystem and infrastructure - the gaps that actually bit this round

Measured during the release work, not theorised:

1. **The release body was `git log -n 50`.** Fixed this round: the curated `[version]` section is used, with
   an explicit fallback that says it is a fallback. Infrastructure that describes a release badly is
   infrastructure nobody trusts.
2. **`ci.yml` cannot be run deliberately.** It triggers on push paths (`crates/**`, `tests/**`,
   `Cargo.toml`, `Cargo.lock`, `deny.toml`, `.github/workflows/**`) and has no `workflow_dispatch`
   (**measured**, `sed -n '1,25p' .github/workflows/ci.yml`). Consequence, hit in this release: the final
   commit touched only documentation, so no Android artifact existed for it, and the pre-tag smoke test had
   to use the artifact of the last commit that touched code - with `git diff --quiet <that sha>..HEAD --
   crates/ Cargo.toml Cargo.lock` as the proof that the code was the same. That is a workaround for a
   missing button. Adding `workflow_dispatch` costs one line and removes the argument.
3. **The runner's warning ceiling is loose and cannot be re-recorded** because the ratchet step discards
   the raw cargo output it measures (the device's entry was re-recorded this round at 266, cold). **#73**
   carries it. The general lesson for infrastructure: if a gate measures something, keep the measurement.
4. **`pd` does not exist**, the short alias the reference has. Trivial, and it is the difference between a
   tool people use and a tool they forget.

## 5. User-facing simplification - what the current surface costs

Measured this round, in the order a new user meets them:

| friction | evidence | candidate |
|---|---|---|
| `remove` refuses without a tty unless `-f`/`-y` is given | `Error: stdin is not a tty: use -f/--force or -y/--yes ...` (the refusal is correct; the recipe is what a script has to discover) | keep the refusal, add `--yes` to the docs' examples for scripted use |
| `attach` silently ends when stdin closes | measured: closed pipe `rc=0`, 0 bytes; open pipe 33 bytes with the marker | already declared in known-limitations; consider printing a one-line hint on a non-tty stdin |
| `login --user` is refused | section 10.2; `--user` belongs to the login polygon | state the *reason* at the point of use (already in `--help`) |
| 39 subcommands, three tiers | `--help` listing | a `podnext help getting-started` path, and examples in every `--help` (the status labels already landed) |
| one command to memorise per task | - | `pd shell <c>` and `pd run <c> <cmd>` aliases; `install` alias derivation (section 10.1) |

Everything above is cheap and none of it needs new architecture. The expensive one is PATH translation,
which is why it is the only item in this document with a threat model.

## 6. Unknowns worth naming

- **Whether a guest can be given a read-only view of any host directory under this PRoot.** Measured: no
  `:ro` bind form exists. Unknown: whether a nested PRoot inside the guest, or a bind onto a guest-side
  tmpfs, could fake one. Worth 30 minutes before option C is designed in detail.
- ~~Whether Android SELinux contexts can distinguish guest-written files from host-written ones.~~
  **Measured, section 1.4: they cannot.** Identical context (`app_data_file:s0:c140,c257,c512,c768`) and
  identical ownership for both. Option B's digest is not a choice among signals - it is the only one.
- **The host cannot audit the guest's version of the prefix tree.** The lookalike is mode 000 on the host
  side, so `find`/`ls` from the host report `Permission denied` rather than a listing (measured). That is
  fine while nothing depends on the tree, and it becomes a problem the moment something does: a
  verification tool that cannot read what it verifies has to run inside the guest, where it can be
  replaced. Worth settling before option D is designed, not after.
- **The reference's guest PATH was not measured** because no `proot-distro` container is installed on this
  device; the claim about its binds and PATH comes from its source and its comments, which is *read* and not
  *measured*. Installing a reference container (~100 MB) would settle it, and is not worth the download
  today.
- **Whether `--change-id` can make a guest-created file distinguishable by ownership.** Read: ids are mapped
  to the single host uid, so probably not. Unmeasured.
