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

**Companion document**: `research/PDN_vs_PROOT_PROOTDISTRO_DOCKER.md` - the same three questions measured
against proot, proot-distro and Docker, whose features, ecosystem and infrastructure are more mature, with the
ordered list of what to take from whom.

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

## 3. Feature expansion - measured against the reference, not against a wish list

The reference ships **18 commands** (**measured**, `proot-distro --help`):

    help search install list login run ps kill remove rename reset backup restore clear-cache copy sync build push

`podnext` ships **39** (**measured**, `podnext --help`). The parity question is therefore not "what is
missing" but "which of the missing ones change what a user can do", and the answer is a short list:

| reference capability | `podnext` today | evidence | why it matters |
|---|---|---|---|
| `install` from a **rootfs archive** or a local OCI layout | registry reference only (`install <IMAGE>`, `--name`, `--force`) | `podnext install --help`; the reference's own text says "OCI image **or rootfs archive**" | the cheapest way to install on a phone with a slow link is a file you already have |
| `restore` (container from a TAR) | `backup` exists, no counterpart | command list above | a backup that cannot be restored is half a feature |
| `clear-cache` | none (a cache directory exists under `~/.podnext/cache`) | `ls ~/.podnext` shows `cache`, `backups`, `containers` | storage on a phone is finite and the cache has no eviction path |
| `--architecture` + emulator | `--qemu <PATH>` only | `podnext run --help` | and it is not a one-flag gap: the reference's emulator path additionally binds `/apex`, `/linkerconfig/ld.config.txt`, `$TERMUX_PREFIX`, `/system`, `/vendor`, `/plat_property_contexts`, `/property_contexts` (`arch.py:308-315`, **read**) |
| `push` to a registry | refused, honestly | `image push` refuses and says why | blocked by the store, not by the CLI |

**And the other direction, which the roadmap does not record.** `podnext` has fourteen commands the
reference has no equivalent for - `doctor`, `service`, `daemon`, `watchdog`, `quota`, `log`, `dbus`, `socket`,
`boot`, `wakelock`, `policy`, `config`, `gitops`, `dashboard` - plus `network`, `volume` and a Dockerfile
`build`. That is where its own feature expansion should be judged: not by closing a parity table, but by
whether a supervision surface (`service`/`watchdog`/`quota`) is worth 39 commands of discovery cost. Section
5 measures that cost.

**A structural observation that decides sequencing.** `install` is a single large function over the registry
path, and the three missing install sources are three different *shapes* of input (a tarball, an OCI layout,
a URL with a checksum) that all end in the same place: "unpack into a clean rootfs". The cheapest expansion
is to make the parse step explicit and add the shapes one at a time, each with the test that refuses a bad
digest - which is what section 10.1 already asks for, with the difference that the OCI-layout case should
come **before** the URL case: a local file can be verified byte-for-byte, a remote one only by the checksum
the user supplies, and the local case is therefore both easier and stronger evidence.

## 4. Ecosystem and infrastructure - what this round measured, including two live defects

The project's infrastructure is not thin; **measured**, `.github/workflows/` holds eight workflows: `ci.yml`
(PDN CI/CD), `cross-compile.yml` (Android Cross-Compile Matrix), `benchmark.yml`, `benchmark-report.yml`,
`dependency-updates.yml`, `devcontainer-test.yml`, `issue-ledger.yml`, `release.yml`. Fuzzing, security
audit, cross-compilation for four targets, benchmarks and a documentation build all run. The gaps found
while making a release are specific:

### 4.1 `install.sh` verifies nothing, and its primary path cannot exist

**Measured**: `grep -n "sha256\|sha512\|checksum\|gpg" install.sh` returns nothing, and the installer
downloads a binary from `releases/download/<tag>/podnext-<target>` and installs it. HTTPS is the whole of the
integrity story.

**Measured**: `install.sh` tries, first, to download `pdn_<version>_<target>.deb`, and
`grep -rn "\.deb" .github/workflows/*.yml` matches **nothing** - no workflow produces a `.deb`. The release's
real assets for `v0.1.8` are `install.sh`, `install-short.sh`, `podnext-aarch64-linux-android`,
`podnext-x86_64-linux-android`. So the preferred path is dead code that falls through to the binary, and the
preflight loop (`for cmd in curl tar dpkg`) still requires `dpkg` for a path that can never succeed.

**Measured, and this is the sharpest version of the problem**: checksums *are* produced - `cross-compile.yml`
has a job named "Aggregate Artifacts & Checksums" that downloads every artifact and writes combined digests -
and they are stored as a **workflow artifact**, not a release asset. The project computes the numbers a user
would need to verify a download, and publishes them where only a maintainer with a GitHub login can find
them.

**Fix, in that order**: publish `SHA256SUMS` as a release asset; make `install.sh` fetch it and verify before
installing, refusing on mismatch; delete the `.deb` path or make the workflow produce the `.deb`; drop `dpkg`
from the preflight if it goes. Acceptance: a tampered download fails with the digest it expected and the
digest it got.

### 4.2 Documentation is built and never published

**Measured**: `docs/book.toml` and `docs/src/SUMMARY.md` exist (so the mdBook scaffold is real, 16 pages
including getting-started, architecture, roadmap, known limitations, CLI reference), and `ci.yml` has a "Build
Documentation" job - but nothing in `.github/workflows/` mentions `pages` or `deploy`. The documentation is
proved to compile and is then thrown away with the runner.

**Fix**: a Pages deployment from the tag or from `master`, one workflow file. This is worth more than most of
section 10: a project whose whole argument is honesty about its own limits should have those limits reachable
at a URL.

### 4.3 No distribution channel

**Measured**: `pkg show podnext` finds no such package; there is no Termux package, no Homebrew formula, no
container image of `podnext` itself. Installation is a `curl | bash` of a script that verifies nothing
(4.1). For a tool whose pitch is "the safe way to run a distro on Termux", that is the weakest link in the
chain, and it is the same link `install.sh` sits on.

### 4.4 `ci.yml` filters on paths, so a documentation-only push runs nothing

**Measured, and corrected.** It triggers on push paths (`crates/**`, `tests/**`, `Cargo.toml`, `Cargo.lock`,
`deny.toml`, `.github/workflows/**`). An earlier version of this section added "with no `workflow_dispatch`",
which is **false and was false when it was written**: `ci.yml` has carried `workflow_dispatch` since its
first commit (`d35ec30`), and still did at `v0.1.8` (`git show v0.1.8:.github/workflows/ci.yml`, line 26).
What actually happened is the path filter alone: the release commit touched documentation only, so no run
existed for it, and the pre-tag smoke test used the last commit that touched code - with
`git diff --quiet <sha>..HEAD -- crates/ Cargo.toml Cargo.lock` as proof the code was identical. The
correction also carries its own remedy: since `workflow_dispatch` is present, a release commit that touches
no code can be dispatched by hand and produce its own artifact, which is better than the diff proof. A push
that touches `.github/workflows/**` triggers the workflow, which is how this was caught.

### 4.5 Extension surface: none, and that may be right

`podnext` has no plugin mechanism, no distro definition format and no template system; `install` takes a
registry reference, `build` takes a Dockerfile, and everything else is a fixed subcommand. The reference's
`$PREFIX/etc/proot-distro/` directory - historically the place distro plugins live - is **empty on this
device** (**measured**), and no `PLUGINS` symbol appears in its Python package (**measured**, grep over
`*.py`), so the "extensible distro plugin" story I expected to find as the reference's ecosystem advantage is
not visible here. The honest conclusion is that neither tool has an extension surface today, and that adding
one is a design question (what would a plugin be allowed to do, and who verifies it?) rather than a parity
gap. Given section 1, a plugin mechanism that runs third-party code as the host user is a decision to make
with the shim findings in hand.

## 5. User-facing simplification - the cost measured, per command

Two numbers decide this section, both **measured** over all 39 commands:

- **16 of 39** commands have an `EXAMPLES` block in their own `--help`;
- **21 of 39** repeat their `stable` / `experimental` label in their own `--help` (the label is always in the
  parent listing, so this measures consistency, not absence).

And one comparison: the reference exposes **18** commands, `podnext` exposes **39**. The extra surface is not
padding - it is supervision and observability the reference does not have - but it is 2.2x the discovery cost
for a person who wants a shell, and nothing in the top-level `--help` says "start here".

What is already good, **measured**: two steps to a shell on both tools (`install <name>` then `login <name>`);
`podnext search alpine` returns installable names with description, stars, pulls and an official flag, so the
"curated distro names" item in section 10.4 is largely answered by search; every stub refuses with a named
reason and a non-zero exit; and the `doctor` command exists, which the reference does not have at all.

Friction found by using it, in the order a new user meets it:

| friction | evidence | what to do |
|---|---|---|
| no entry point for a beginner | `podnext --help` has no getting-started pointer (**measured**) | one line in the top-level help, and a `getting-started` page that already exists and is unpublished (4.2) |
| 23 commands have no example | the count above | an example in every `--help`; it is mechanical, and the CLI reference contract test already parses those blocks |
| `remove` refuses a non-tty | `stdin is not a tty: use -f/--force or -y/--yes ...` | keep the refusal, put `--yes` in the documented scripted example |
| `attach` ends silently on closed stdin | measured: closed pipe rc=0, 0 bytes; open pipe 33 bytes with the marker | already declared; a one-line hint on a non-tty stdin would turn a mystery into a sentence |
| 18 commands do not repeat their own status label | 21 of 39 do (the count above) | print the label from the parent listing in each command's own help, generated rather than retyped |
| `pd` does not exist | `pkg show`/`command -v pd` (**measured**) | a two-letter alias is the cheapest adoption feature in this document |

**The one simplification that is not cheap is the one this document leads with.** Every attempt to shorten
the path between the guest and the host tools - a translated PATH, a shim directory, a shared prefix - lands
in section 1: a name the guest can take. Any UX feature that reaches across that boundary should be designed
as IPC with a token (option C), not as a file on a path.

## 7. What to file first, in the order the measurements argue for

1. **The installer verifies nothing, and its preferred path cannot exist** (section 4.1). Two defects in one
   file: no checksum, and a `.deb` download that no workflow produces while the preflight still requires
   `dpkg`. Filed as an issue from this document.
2. **Publish the checksums the project already computes.** `cross-compile.yml` aggregates digests and leaves
   them as a workflow artifact. One step to attach `SHA256SUMS` to the release, and it is the piece that makes
   (1) fixable rather than merely regrettable.
3. **Deploy the documentation.** `docs/book.toml` and 16 pages exist; nothing publishes them (section 4.2).
4. **~~`workflow_dispatch` on `ci.yml`~~** (section 4.4). Withdrawn: it is already there and always has been.
   The real remedy for a release commit that touches no code is to dispatch the workflow by hand on that
   commit, so its own artifact exists and no diff proof is needed.
5. **A `pd` alias and an example in the 23 commands that lack one** (section 5). The cheapest adoption work
   in the document, and the CLI reference contract test already parses those blocks.
6. **`restore`, `clear-cache`, and install from a local archive** (section 3), in that order: a backup without
   a restore is the one that looks like a bug to a user.

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
