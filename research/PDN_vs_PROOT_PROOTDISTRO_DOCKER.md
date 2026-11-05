# PDN against proot, proot-distro and Docker

**Status**: research dossier, 6 Nov 2026, written against `podnext` v0.2.0 (tag `v0.2.0`, `0881275`).
**Purpose**: the three comparators are more mature than PDN in features, ecosystem and infrastructure. This
document records what they have, what PDN has, and what to take from whom - so the next release follows them
instead of rediscovering their answers.

**Evidence discipline**: every row is marked **M** (measured on this device, with the command), **R** (read
from a source or package file on this device), or **K** (documented knowledge about the project, not verified
here). The distinction matters: `docker`'s CLI is installed here and measurable; its daemon is not running and
was not started, so anything about daemon internals is **K**.

Cross-reference: `research/PATH_TRANSLATION_AND_SHIM_INTEGRITY.md` covers the fourth question (PATH
translation versus shims that can be forged) and is not repeated here.

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

The dependency column is the first infrastructure lesson and it is easy to miss: proot-distro is **installable
and upgradable through apt**, with a versioned dependency on proot. PDN ships a static binary through
`curl | bash` (section 3.1).

## 1. Features - what each comparator has that PDN does not

### 1.1 The execution model, and why the comparison is asymmetric

`proot` is the primitive: 19 options **M** (`-r -b -q -w --kill-on-exit -v -V -h -k -i --link --sysvipc
--ashmem-memfd -H -p -L -R -S`). PDN does not reimplement proot's option surface; it *drives* it. So
proot's contribution to the comparison is not a feature list to copy but three specifics PDN touches
directly:

- `--ashmem-memfd` **M**: Android shared-memory emulation, the flag that makes `/dev/shm` workable. PDN
  already isolates `/dev/shm` per container (measured in #66) and this is the underlying knob.
- `--link` (L2S) **M**: the hardlink-to-symlink conversion, whose in-place-rewrite behaviour PDN has already
  had to handle during relocation.
- `-q` **M**: QEMU user-mode emulation, which is the only path to multi-arch here - and PDN exposes it as
  `--qemu <PATH>` without the emulator bind set the reference adds (`arch.py:308-315`, **R**).

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

The `plugin` row is the most instructive **M**: the very fact that `compose` and `buildx` are *missing* from
this CLI while existing in the Docker world is the extension mechanism working as designed. PDN's 39 fixed
commands have no equivalent, and its `docker`/`compose` subcommands are refusals pointing at the roadmap.

### 1.3 proot-distro's features PDN lacks, measured

18 commands **M**:
`help search install list login run ps kill remove rename reset backup restore clear-cache copy sync build
push`. The parity gaps, unchanged from the other dossier and repeated here for completeness of this one:

- `install` from a **rootfs archive or local OCI layout** (**R**, its own help text says "OCI image or rootfs
  archive") against PDN's registry-reference-only install;
- `restore` (PDN has `backup`, no counterpart);
- `clear-cache` (PDN has a cache directory with no eviction command);
- `--architecture` plus the emulator bind set.

**And one feature of the reference that is not a command at all**: `commands/help/{__init__,pages,render}.py`
**R** - a paginated, rendered help subsystem inside the CLI. PDN's `--help` is flat text, 23 of 39 commands
carry no example, and the mdBook is unpublished (section 3.2). The reference treats help as a feature; PDN
treats it as clap's default.

### 1.4 What PDN has that none of them do

Fourteen commands the reference lacks, listed in the other dossier, of which three are worth naming here
because they are PDN's actual differentiator and should be defended rather than trimmed: `doctor` (host and
guest diagnostics with the value behind every verdict - *none* of the three comparators has an equivalent),
`service`/`watchdog`/`quota` (supervision, which Docker delegates to the daemon and proot-distro does not
have at all), and `sync` with content hashing.

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

**No distribution channel is the real gap.** proot and proot-distro arrive through apt, which means signed
packages, dependency resolution and upgrades; `termux-gui-pm` can then treat a `proot-distro` distro as a
manageable thing **M**. `podnext` arrives as a script that downloads a binary and installs it with no
verification (#74), and nothing in the repository can upgrade it. Everything else in this table is a feature;
this one is whether the project can be adopted at all by someone who is not its author.

## 3. Infrastructure - packaging, supply chain, CI, docs

### 3.1 Packaging and supply chain

| | proot / proot-distro | Docker | `podnext` |
|---|---|---|---|
| Signed channel | apt repository signatures **K** | signed packages and images with content trust **K** | none |
| Per-artifact verification | apt checks digests **K** | `docker trust`, digest-pinned pulls, SBOM/provenance **K** | **none** - `install.sh` greps clean for `sha256\|checksum\|gpg` **M** |
| Checksums published for users | via the package index **K** | via digests in every reference **K** | computed by the "Aggregate Artifacts & Checksums" job and stored as a **workflow artifact** **M**, invisible to a user |
| Working installer path | n/a | n/a | `install.sh` prefers a `.deb` that **no workflow produces** **M** |

This is the sharpest infrastructure finding in the document, and it is not a feature request: PDN computes
what a user needs, and publishes it where only a logged-in maintainer can see it. Docker's answer - pin by
digest, verify against a signature - is the design to copy, and it is testable: a tampered download must fail
with both digests printed.

### 3.2 Documentation

| | comparators | `podnext` |
|---|---|---|
| Site | proot-me.github.io **M**, docs.docker.com **K**, the reference documents itself in-CLI with `help/{pages,render}` **R** | `docs/book.toml` + 16 pages including getting-started, architecture, roadmap, known limitations and CLI reference **M** - and nothing in `.github/workflows/` mentions `pages` or `deploy` **M**, so the book is built in CI and discarded |

PDN's documentation is *better written* than either of the two small comparators - it records measured
exceptions, declared simplifications and per-command stability - and it is unreachable. Publishing is one
workflow file.

### 3.3 Continuous integration and release machinery

**Measured**: PDN's `.github/workflows/` holds eight workflows - `ci.yml` (host build, four cross-compile
targets, integration test, security audit, benchmarks, documentation build, artifact aggregation),
`cross-compile.yml`, `benchmark.yml`, `benchmark-report.yml`, `dependency-updates.yml`,
`devcontainer-test.yml`, `issue-ledger.yml`, `release.yml`. Fuzzing is wired in. That is not the weak part of
this project; the weak parts are that `ci.yml` filters on paths, so a documentation-only push runs nothing at
all (corrected here: an earlier version of this paragraph claimed `ci.yml` had no `workflow_dispatch`; it has
had one since its first commit - `git show v0.1.8:.github/workflows/ci.yml`, line 26), that the runner's
warning ceiling cannot be re-recorded without its raw log (#73), that the release body was first
`git log -n 50` and then, once that was fixed, arrived as 8 KB of notes on **one line** because the workflow
escaped the notes into a line-oriented Actions output (`#75`, fixed, and the published body repaired), and
that a second tag-triggered release job in `ci.yml` could never succeed and would have overwritten the
curated body with stale boilerplate had it ever run (`#77`, removed).

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

**Ordering argument**: 1-4 are adoption and truthfulness (nothing works if the tool cannot be installed or
verified), 5-8 are the mobility polygon that `v0.3.0` already owns, 9-10 are architecture, and 11 is listed so
that the rejection is on the record rather than forgotten.

## 5. What this document does not know

- **Docker's daemon behaviour is unmeasured here.** The CLI is installed (**M**) and the daemon was not
  started, deliberately: starting a container runtime on this phone is outside the scope of a research task
  and outside the release window. Every daemon-internal claim above is **K**.
- **Whether the apt channel's verification is actually enforced end-to-end on Termux** (repository signing
  keys present and checked) is **K**, not verified; if point 2 of the table is pursued, that is the first
  thing to measure.
- **The reference's `help` subsystem output was not read** - the file layout is **R**, the rendered pages
  were not exercised. Reading them is worth ten minutes before copying the idea.
- **Whether `docker compose`/`buildx` can be installed as plugins in this Termux environment** is **K**; the
  absence measured here may be a packaging choice rather than a platform limit.
