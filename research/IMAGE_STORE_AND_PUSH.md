# The image store and the push mechanism: what PDN has, what it is missing, and how the reference implementation does it

Status: analysis, no code changed. Related: #37 (lookup wall), and the issue filed from this document for
the layer wall. Measured on this device (Termux Android 16, aarch64) against `master` `effbfc1`.

---

## 1. Why this document exists

`podnext image push <ref>` fails for every image, and it fails for two independent reasons that look
like one:

1. **The lookup wall.** `cmd_image_push` constructs `proot_distro_core::ImageManager::new()`, which
   builds an empty `HashMap` and never loads anything from disk
   (`crates/proot-distro-core/src/image.rs:27`). The `images.get(...)` inside `ImageManager::push`
   therefore always misses, before any network call is made.
2. **The layer wall.** Even with the lookup fixed, the store holds no layer blobs at all. The crate
   says so itself in `image.rs`: `push` refuses early with *"PDN does not retain layer blobs after
   extraction"*, and a locally built image has no `layers` field anywhere — `metadata.json` for
   `alpine`, `deb-pdn` and `deb-renamed` was checked and has no layer key of any kind.

Wall 1 is a bug. Wall 2 is a design decision that was never made explicitly, and the two are
routinely confused because both surface as "Image 'x' not found locally". This document separates
them, prices both, and records how the implementation PDN is modelled on solves the same problem.

---

## 2. PDN's model today: the rootfs *is* the image

| Fact | Evidence |
|---|---|
| An image is a container directory | `build_local_image` writes `~/.podnext/containers/<store-key>/rootfs` and returns it; `crates/podnext/src/commands/mod.rs` documents the store as "a container directory under `~/.podnext/containers/<tag>/rootfs`" |
| The store key is derived, colon-free | `proot_distro_core::names::store_key` (#35) |
| Metadata is a flat JSON record, not a manifest | keys observed: `cmd`, `created_at`, `env`, `exposed_ports`, `image`, `labels`, `name`, `rootfs`, `status`, `tag`, `user`, `working_dir` |
| `image ls` already reads this store | `cmd_image_ls` lists `ContainerManager`, not `ImageManager` |
| Layer blobs are discarded after extraction | `image.rs` refusal message; no blob cache exists under `~/.podnext/` |

Consequence: everything that treats an image as *OCI artifacts* (manifest, config, layers) has
nothing to work with, and `push` — which is nothing but "ship those artifacts" — cannot be made to
work by fixing the lookup alone.

Measured costs that follow from this model (this device):

```
image ls            median 28.6 ms   (10 containers, 7 with metadata.json)
metadata.json parse 3.79 ms          (7 files, 0.54 ms each)
rootfs walk (size)  105 ms / 575 entries   229 ms / 6,188   214 ms / 11,641
tar (no compression) 46 ms for 9.1 MB  ⇒ ≈0.35 s for a 307 MB rootfs (linear extrapolation)
tar+gzip             62 ms for 9.1 MB  ⇒ ≈2.1 s for a 307 MB rootfs
```

---

## 3. The reference: Termux `proot-distro`'s image infrastructure

The version installed here
(`$PREFIX/lib/python3.14/site-packages/proot_distro/`, 77 modules, 2.7 MB) is a Python implementation
with a complete registry-facing image layer. Structure, by module:

### 3.1 Storage layout

```
RUNTIME_DIR            = $PREFIX/var/lib/proot-distro      (Termux)   | $XDG_DATA_HOME/proot-distro
BASE_CACHE_DIR         = RUNTIME_DIR/cache                 (Termux)   | $XDG_CACHE_HOME/proot-distro
CONTAINERS_DIR         = RUNTIME_DIR/containers
SESSIONS_DIR           = RUNTIME_DIR/sessions
LEGACY_ROOTFS_DIR      = RUNTIME_DIR/installed-rootfs      (legacy name, no longer used)
LAYER_CACHE_DIR        = BASE_CACHE_DIR/oci_layers
MANIFEST_CACHE_DIR     = BASE_CACHE_DIR/oci_manifests
```

`helpers/docker/cache.py` documents the blob layout: **one file per blob, named for the digest of its
content** (`oci_layers/<digest with ':' replaced by '_'>`), and a digest-to-path mapper that validates
the digest grammar first so a crafted digest cannot escape the cache directory. That last detail is a
security boundary of exactly the kind PDN hit in `#16`/`#37`: an untrusted string becoming a path.

The decisive inversion, and the answer to "why would you push an image you already have":

> the container rootfs is **derived state**. The image is the manifest + config + layer blobs, held
> digest-addressed in the cache. `apply_layer` (`helpers/docker/layers.py`) turns layers into the
> rootfs; nothing is ever "pushed" from the rootfs.

### 3.2 Build: one layer per instruction, by filesystem diff

`helpers/layer_diff.py` (771 lines) provides `snapshot` (fingerprint every entry under a rootfs),
`diff_snapshots` (added / modified / deleted), `_whiteout_paths` (deleted paths → OCI whiteout
entries), and `write_layer_tar` (gzipped OCI layer). `helpers/build_engine/engine.py` calls the diff
after each dispatched instruction and records a history entry whose `empty_layer` flag is set from
whether the handler actually grew `stage.layers` — the OCI image-config spec requires the count of
non-empty-layer history entries to equal the number of layers, and the comment in the engine notes
that getting this wrong "makes built layers invisible".

Multi-stage is supported: `helpers/build_engine/stage.py` gives each stage its own directory and its
own scopes for `ENV`/`ARG`/`WORKDIR`/`USER`/`SHELL`, with `COPY --from=<stage>` resolved through the
stage's descriptors rather than through host paths.

`helpers/oci_writer.py` assembles the blobs: `build_manifest_and_config` (manifest + config, config
`history` taken from the engine), `canonical_json` (sorted keys, no whitespace — so the digest is
reproducible), and `store_in_cache`, whose docstring states the goal plainly: a **built** image is
written through the same `save_manifest_cache` path as a **pulled** one, "same cache key, same
payload shape", so a later `install` is indistinguishable and works offline.

### 3.3 Push: Docker Registry v2, blob-by-blob

`helpers/docker/push.py` (516 lines) implements the transfer. `push_image` is documented as "push a
built image (resolved from the manifest cache) to its registry", and its step order is:

```
parse_tag_reference → load_manifest_cache → manifest_layers
   → verify each layer is present locally (blob_present / open_required_layer)
   → sha256 the manifest → get_auth_token (registry credential resolution)
   → for each layer: _blob_exists (HEAD, skip what the registry already has)
                     open_required_layer → _upload_blob_fd (streamed PUT)
   → _put_manifest (PUT /v2/<repo>/manifests/<tag|digest>)
```

Supporting machinery, all present and named for the failure it handles:

| Concern | Mechanism |
|---|---|
| Upload session | `_resolve_upload_url` reads the `Location` header from `POST /v2/<repo>/blobs/uploads/` |
| Skipping work | `_blob_exists` HEADs each digest first, so a re-push uploads only what is missing |
| Small blobs | `_upload_blob_bytes` (monolithic POST+PUT) |
| Large blobs | `_upload_blob_fd`, streamed from a descriptor, with `_ProgressReader` drawing progress and `_upload_timeout` scaled to the byte count (`_COMMIT_RATE = 1 MiB`) |
| Credential hygiene | `_headers_for` sends the credential only to the registry's own origin; `AuthStrippingRedirectHandler` in `helpers/docker/transport.py` strips `Authorization` when a redirect leaves that origin; `_require_usable_realm` refuses a Bearer realm it must not send credentials to |
| Auth/rate limits | `get_auth_token`, `_parse_bearer_challenge`, `credentials_rejected_msg`, `auth_denied_msg`, `push_denied_msg`, `rate_limited_msg` (with `Retry-After` rendered for a human), `insecure_for` + `_http_registry_reachable` for opt-in plaintext registries |
| Private local bookkeeping | `_strip_private_keys` removes keys prefixed `_` before anything is uploaded, so internal fields cannot leak into a published manifest |
| Media types | `helpers/docker/media.py`: OCI manifest/config/layer/index plus the Docker legacy manifest list/v2 |

### 3.4 The mirror-image path

`helpers/docker/pull.py` (708 lines) is the same protocol in reverse (blobs into `LAYER_CACHE_DIR`,
then applied). `commands/install_local.py` additionally accepts a **local archive**, auto-detecting
two formats — a raw rootfs tarball and an **OCI image layout** (`oci-layout` marker present) — and
hashes blobs against the digest the archive names them by. Its comments are explicit that an archive
is a stranger's input: bounded reads and a ceiling on declared members, because neither the archive
size nor its own claims are trusted.

---

## 4. What this means for PDN

Three coherent futures, with the cost this device measured:

| Option | Cost | What it buys | What it costs |
|---|---|---|---|
| **A — keep the rootfs model, refuse `push`** | 0 | Honest CLI; nothing to maintain | No distribution off-device, ever |
| **B — keep the rootfs, tar the rootfs at push time** | ≈0.35 s raw / ≈2.1 s gzip per 307 MB, paid once per push, on the image that exists | `push` works with a single-layer image | No build history; one giant layer; a registry sees a 307 MB blob it must store whole; `pull` on the other side gets one layer |
| **C — store OCI artifacts (manifest + config + layers), derive the rootfs** | +0.1–0.25 s per build for the diff/snapshot work (measured rootfs walk band), plus compressed layer storage | Everything in section 3: real push, real pull, offline install from a layout archive, `image ls`/`inspect` with digests, reproducible manifests | The largest change: `build local`, `install`, the container layout and `#29`'s "the container directory is the image" all move |

Option C is what the reference implementation does, and it is the only one under which the words
"push" and "pull" mean what they mean everywhere else. Option B is a defensible intermediate if the
only goal is "get this rootfs to another machine once". Option A is what the code does today, minus
the false promise: `image push` currently exists, prints "Pushing image: …", and cannot succeed.

Note what is *not* on the table: fixing only the lookup (wall 1) and leaving the store without
layers. That is the state that produced `#17`, where `push` uploaded a manifest describing blobs that
were never sent — a failure that only appears later, on someone else's `docker pull`.

---

## 5. Licence and independence

Termux `proot-distro` is **GPL-3.0** (`__init__.py` header: "either version 3 of the License").
PDN declares **GPL-2.0-or-later** in `Cargo.toml` and in `README.md`, and is a Rust reimplementation.

Two consequences recorded here because they bound any follow-up work:

1. **This document describes architecture, not code.** Module names, function names and protocol
   behaviour are referenced so the designs can be compared; no implementation may be transcribed
   across the licence boundary, and GPL-3.0-only terms are not compatible with GPL-2.0-only. Any PDN
   implementation of section 3 must be written independently from the OCI/Docker specifications.
2. **The repository's own licence metadata disagrees with itself**, found while checking this:
   `Cargo.toml:19` says `license = "GPL-2.0-or-later"`, `README.md` says GPL-2.0-or-later, and the
   `LICENSE` file (34 lines) contains the **Apache-2.0** text ("Copyright 2026 PDN Contributors",
   "Apache License"). One of the three is wrong, and this is exactly the kind of thing that has to be
   settled before anyone reasons about what may be taken from where.

---

## 6. Reproduce

```sh
# wall 1: the lookup never reaches disk
grep -n "ImageManager::new()" crates/podnext/src/main.rs            # cmd_image_push
grep -n "fn new()" -A12 crates/proot-distro-core/src/image.rs        # empty HashMap, no load

# wall 2: no layers exist to send
podnext build local -t demo:latest ./ctx && ls ~/.podnext/containers/demo/
python3 -c "import json;print(sorted(json.load(open('$HOME/.podnext/containers/demo/metadata.json'))))"

# both, from the user's side
podnext image push demo:latest      # "not found locally" - wall 1, before any network call
podnext image push alpine:3.24.3    # same message for an image that IS present - the tell

# the reference implementation on this device
ls $PREFIX/lib/python3.14/site-packages/proot_distro/helpers/docker/
grep -n "LAYER_CACHE_DIR\|MANIFEST_CACHE_DIR" \
  $PREFIX/lib/python3.14/site-packages/proot_distro/constants.py
sed -n '1,40p' $PREFIX/lib/python3.14/site-packages/proot_distro/helpers/docker/cache.py
```
