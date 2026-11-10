# PDN against proot, proot-distro and Docker

**Status**: research dossier, 6 Nov 2026, against `podnext` v0.2.0. The tag is `v0.2.0`. The commit is `0881275`.
**Purpose**: the three comparators are more mature than PDN in features, ecosystem and infrastructure. This
document records what they have, what PDN has, and what to take from whom. The next release thus follows them.
It does not rediscover their answers.

**Evidence discipline**: every row carries one mark. The mark **M** means measured on this device, with the
command. The mark **R** means read from a source or package file on this device. The mark **K** means
documented knowledge about the project, not verified here. The distinction matters. The `docker` CLI is
installed here and is measurable. Its daemon does not run here, and no test started it. So anything about
daemon internals is **K**.

Cross-reference: the file `research/PATH_TRANSLATION_AND_SHIM_INTEGRITY.md` covers the fourth question, which
is PATH translation versus shims that can be forged. This document does not repeat that question.

---

## 0. The three comparators, as measured here

| | proot | proot-distro | Docker (CLI) | `podnext` v0.2.0 |
|---|---|---|---|---|
| Version | 5.1.107.101 **M** | 5.10.3 **M** | v24.0.6-ce **M** | 0.2.0 **M** |
| Package / size | `proot`, 352 KB **M** | `proot-distro`, 1724 KB **M** | `docker-cli` **M** | none **M** |
| Homepage | proot-me.github.io **M** | github.com/termux/proot-distro **M** | docs.docker.com **K** | github.com/.../PDN **M** |
| Depends | libandroid-shmem, libtalloc **M** | proot >= 5.1.107-71, python, python-pip **M** | - **M** | static binary **M** |
| Command surface | 19 options **M** | 18 commands **M** | 17 management commands **M** | 39 commands **M** |
| Short alias | - | **`pd`**, shipped in the package **M** (`dpkg -L` shows `bin/pd`) | - | none **M** |

The dependency column is the first infrastructure lesson and it is easy to miss. proot-distro is
**installable and upgradable through apt**, with a versioned dependency on proot. PDN ships a static binary
through `curl | bash`. Refer to section 3.1.

## 1. Features - what each comparator has that PDN does not

### 1.1 The execution model, and why the comparison is asymmetric

`proot` is the primitive. It has 19 options **M**:
`-r -b -q -w --kill-on-exit -v -V -h -k -i --link --sysvipc --ashmem-memfd -H -p -L -R -S`
PDN does not reimplement the proot option surface. PDN *drives* proot. So the proot contribution to the
comparison is not a feature list to copy. It is three specifics that PDN touches directly:

- `--ashmem-memfd` **M**: Android shared-memory emulation, the flag that makes `/dev/shm` workable. PDN already isolates `/dev/shm` per container, as measured in #66. This is the underlying knob.
- `--link` **M**, also known as L2S: this flag converts a hardlink to a symlink. PDN has already had to handle its in-place rewrite behaviour during relocation.
- `-q` **M**: this flag gives QEMU user-mode emulation. That is the only path to multi-arch here. PDN exposes it as `--qemu <PATH>`, without the emulator bind set that the reference adds. The reference is `arch.py:308-315` **R**.

### 1.2 Docker's management surface, measured, and what it says

```
$ docker --help | sed -n '/Management Commands/,/^$/p'
  builder  checkpoint  container  context  image  manifest  network  plugin  system  trust  volume
```

Five of those eleven do not exist in PDN in any form, and each is a lesson rather than a command:

| Docker command | what it is | PDN today | take it? |
|---|---|---|---|
| `trust` **M** | signature and digest verification for what you pull | verifies nothing; the installer verifies nothing either (#74) | **yes** - this is the supply-chain gap, and it has a well-tested design to copy |
| `manifest` **M** | multi-arch manifest lists | `install <image>` resolves one arch; `--qemu` is a flag, not a resolver | yes, with `--architecture` |
| `plugin` **M** | the extension mechanism, which is *why* `compose` and `buildx` are absent from this CLI yet present in every real install (**M**: `docker compose` -> "'compose' is not a docker command") | none | yes, but with section 1 of the other dossier in hand |
| `context` **M** | named endpoints, so one client talks to several daemons | one local store, no notion of a remote | later |
| `checkpoint` **M** | live checkpoint/restore of a running container | none | no - needs daemon-level support PDN cannot have under proot |
| `builder` **M** | BuildKit-driven builds with cache semantics | `build local` from a Dockerfile, no cache, no multi-stage report | yes, incrementally |

The `plugin` row is the most instructive **M**. The fact that `compose` and `buildx` are *missing* from this
CLI shows the extension mechanism at work, because both exist in the Docker world. PDN has 39 fixed commands
with no equivalent. Its `docker`/`compose` subcommands are refusals that point at the roadmap.

### 1.3 proot-distro's features PDN lacks, measured

18 commands **M**:
`help search install list login run ps kill remove rename reset backup restore clear-cache copy sync build
push`. The parity gaps, unchanged from the other dossier and repeated here for completeness of this one:

- `install` from a **rootfs archive or local OCI layout** **R**. Its own help text says "OCI image or rootfs
  archive". PDN installs only from a registry reference.
- `restore`. PDN has `backup`, but no counterpart.
- `clear-cache`. PDN has a cache directory, but no eviction command.
- `--architecture` plus the emulator bind set.

**And one feature of the reference that is not a command at all**: `commands/help/{__init__,pages,render}.py`
**R**: a paginated help subsystem inside the CLI, with rendered output. PDN's `--help` is flat text. 23 of 39
commands carry no example, and the mdBook is unpublished. Refer to section 3.2. The reference treats help as a
feature. PDN treats it as the clap default.

### 1.4 What PDN has that none of them do

Fourteen commands the reference lacks are listed in the other dossier. Three of them are worth naming here,
because they are PDN's actual differentiator. PDN must defend these commands rather than trim them. The
command `doctor` gives host and guest diagnostics, with the value behind every verdict. None of the three
comparators has an equivalent. The commands `service`/`watchdog`/`quota` give supervision. Docker delegates
supervision to the daemon, and proot-distro does not have it at all. The command `sync` hashes content.

## 2. Ecosystem - how each project is reached, extended and extended by others

| dimension | proot | proot-distro | Docker | `podnext` |
|---|---|---|---|---|
| Distribution channel | apt (`proot`) **M** | apt (`proot-distro`) **M** | `docker-cli` in this repo, plus apt/yum/apk/Docker Desktop **M**/**K** | `curl \| bash` from GitHub Releases **M** |
| Upgrade path | apt upgrade **M** | apt upgrade **M** | package manager or Desktop auto-update **K** | re-run the script; nothing checks |
| Short alias | - | `pd` shipped **M** | - | none **M** |
| Extension mechanism | - | none in 5.10.3 (**M**: 0 `.sh` plugins shipped, no `PLUGINS` symbol) | plugins, which is how `compose`/`buildx` attach **M** | none |
| Third-party ecosystem | a project site **M** | derivative tooling exists in this repo: `termux-gui-pm` is described as "a graphical package manager for various package formats for Termux **and proot-distro distros**" **M** | the largest in the container world **K** | none |
| Registry reach | - | Docker Hub via `search` **M** | Docker Hub plus any OCI registry **K** | Docker Hub via `search`/`install` **M** |

Two rows deserve a sentence each, because they are the ecosystem lessons PDN is furthest from:

**The alias is not cosmetic.** `pd` is shipped by the package **M**. It is the cheapest adoption feature in
this table and PDN has nothing like it.

**No distribution channel is the real gap.** proot and proot-distro arrive through apt. That method gives
signed packages, dependency resolution and upgrades. Then `termux-gui-pm` can treat a `proot-distro` distro as
a manageable thing **M**. `podnext` arrives as a script that downloads a binary and installs it with no
verification, as issue #74 records. Nothing in the repository can upgrade it. Everything else in this table is
a feature. This one is whether a person who is not its author can adopt the project at all.

## 3. Infrastructure - packaging, supply chain, CI, docs

### 3.1 Packaging and supply chain

| | proot / proot-distro | Docker | `podnext` |
|---|---|---|---|
| Signed channel | apt repository signatures **K** | signed packages and images with content trust **K** | none |
| Per-artifact verification | apt checks digests **K** | `docker trust`, digest-pinned pulls, SBOM/provenance **K** | **none** - `install.sh` greps clean for `sha256\|checksum\|gpg` **M** |
| Checksums published for users | via the package index **K** | via digests in every reference **K** | computed by the "Aggregate Artifacts & Checksums" job and stored as a **workflow artifact** **M**, invisible to a user |
| Working installer path | n/a | n/a | `install.sh` prefers a `.deb` that **no workflow produces** **M** |

This is the sharpest infrastructure finding in the document, and it is not a feature request. PDN computes
what a user needs. It publishes that value where only a logged-in maintainer can see it. Docker's answer is
to pin by digest and to verify against a signature. This design is the one to copy, and it is testable. A
tampered download must fail, with both digests printed.

### 3.2 Documentation

| | comparators | `podnext` |
|---|---|---|
| Site | proot-me.github.io **M**, docs.docker.com **K**, the reference documents itself in-CLI with `help/{pages,render}` **R** | `docs/book.toml` + 16 pages including getting-started, architecture, roadmap, known limitations and CLI reference **M** - and nothing in `.github/workflows/` mentions `pages` or `deploy` **M**, so the book is built in CI and discarded |

PDN's documentation is *better written* than either of the two small comparators. It records measured
exceptions, declared simplifications and per-command stability. Yet it is unreachable. Publishing is one
workflow file.

### 3.3 Continuous integration and release machinery

**Measured**: PDN's `.github/workflows/` holds eight workflows. The names are `ci.yml`, `cross-compile.yml`,
`benchmark.yml`, `benchmark-report.yml`, `dependency-updates.yml`, `devcontainer-test.yml`, `issue-ledger.yml`
and `release.yml`. The workflow `ci.yml` does the host build, four cross-compile targets, the integration test,
the security audit, the benchmarks, the documentation build and artifact aggregation. Fuzzing is wired in. That
is not the weak part of this project. The weak parts are below. The workflow `ci.yml` filters on paths, so a
documentation-only push runs nothing at all. An earlier version of this paragraph claimed that `ci.yml` had no
`workflow_dispatch`. That claim is corrected here. It has had one since its first commit, as
`git show v0.1.8:.github/workflows/ci.yml` at line 26 shows. The runner warning ceiling cannot be re-recorded
without its raw log, as issue #73 records. The release body was first `git log -n 50`. Then, once that was
fixed, the body arrived as 8 KB of notes on **one line**. The workflow escaped the notes into a line-oriented
Actions output, as issue #75 records. Issue #75 is fixed, and the published body is repaired. A second
tag-triggered release job in `ci.yml` never can succeed. That job never ran, and it held the power to replace
the curated body with stale boilerplate. Issue #77 removed it.

Docker's infrastructure lesson is scale rather than shape: separate repositories with their own CI for CLI,
daemon, compose and buildx **K**, which is also why its `plugin` command exists.

## 4. What to take from whom, in the order the measurements argue for

| # | take | from | why now | acceptance test |
|---|---|---|---|---|
| 1 | publish `SHA256SUMS` as a release asset; verify it in `install.sh`, refuse on mismatch | **Docker** (digest-and-verify) | the installer verifies nothing and the checksums already exist (**#74**) | a tampered binary fails with expected and actual digest |
| 2 | an apt-installable distribution path, or at least make `install.sh`'s working path the documented one | **proot-distro** | no channel means no adoption by anyone else | a user installs and upgrades without reading the repository |
| 3 | `pd` alias | **proot-distro** | cheapest adoption feature measured in this document | `pd --help` is `podnext --help` |
| 4 | publish the mdBook | **all three** | 16 pages already exist and are discarded | a URL serves getting-started |
| 5 | `--architecture` + emulator bind set, not just `--qemu` | **proot** + **proot-distro** | multi-arch is currently a flag with no resolver and no binds | an x86_64 rootfs runs on this device, documented |
| 6 | `install` from a local rootfs archive, then local OCI layout | **proot-distro** | the OCI-layout case is verifiable byte-for-byte, so it is stronger evidence than a URL | both installs succeed and a wrong digest is refused |
| 7 | `restore`, `clear-cache` | **proot-distro** | a backup without a restore reads as a bug; the cache grows forever | round trip a backup; clear-cache frees the measured size |
| 8 | an example in the 23 commands that lack one, and the status label in each command's own help | **proot-distro** (`help/`) | mechanical, and the CLI reference contract test already parses those blocks | 39 of 39 commands carry an example |
| 9 | multi-arch manifest resolution and digest-pinned installs | **Docker** (`manifest`, `trust`) | `install alpine:3.20` currently takes whatever the registry serves for this arch | an install prints the digest it resolved and refuses a mismatch |
| 10 | an extension mechanism | **Docker** (`plugin`) | after section 1 of the other dossier: any extension that hands the guest a host file by name is spoofable | a plugin manifest declares what a plugin may do, and the loader refuses anything else |
| 11 | checkpoint/restore | **Docker** (`checkpoint`) | **not recommended** - needs daemon-level support PDN cannot have under proot | - |

**Ordering argument**: 1-4 are adoption and truthfulness. Nothing works if a user cannot install or verify the
tool. Items 5-8 are the mobility polygon that `v0.3.0` already owns. Items 9-10 are architecture. Item 11 is
listed so that the rejection is on the record rather than forgotten.

## 5. What this document does not know

- **Docker's daemon behaviour is unmeasured here.** The CLI is installed, with the mark **M**. No test started the daemon. That choice is deliberate. A container runtime on this phone is outside the scope of a research task, and outside the release window. Every daemon-internal claim above is **K**.
- **Whether the apt channel's verification is actually enforced end-to-end on Termux** is **K**, not verified. That verification covers the repository signing keys. It asks if those keys are present and checked. If the project pursues point 2 of the table, that check is the first thing to measure.
- **The reference's `help` subsystem output was not read.** The file layout is **R**. No test exercised the rendered pages. Read them before you copy the idea. That work is worth ten minutes.
- **Whether `docker compose`/`buildx` can be installed as plugins in this Termux environment** is **K**. The absence measured here can be a packaging choice rather than a platform limit.
