# R2-REFERENCE — proot (Termux fork) and proot-distro v5.10.3, re-read from raw source

**Purpose.** Confirm, from the raw source trees only, which components and mechanisms PDN needs so that
v0.3.0 can genuinely compete with Termux's `proot-distro` on the four axes **MOBILITY (M), CAPABILITY (C),
STABILITY (S), OPTIMIZATION (O)**, answered against `docs/src/roadmap.md` section 10 (gaps 10.1 bringing
something in, 10.2 living inside, 10.3 moving out/between, 10.4 ecosystem, 10.5 recycling map).

**Sources, measured in this session (not trusted from the prior summary):**

- proot (Termux fork): `~/.hermes/cache/scratch/proot-src`, git `52d65a4`. `find -name '*.c' | wc -l` = **138**;
  `xargs wc -l | tail -1` = **32,638**. Largest files: `src/syscall/enter.c` **3069**, `src/tracee/tracee.c`
  **1053**, `src/execve/enter.c` **731**, `src/syscall/exit.c` **845**.
- proot-distro: `$PREFIX/lib/python3.14/site-packages/proot_distro`, **77 `.py` files / 29,015 lines**;
  version confirmed `5.10.3` from `proot_distro-5.10.3.dist-info/METADATA` (`Version: 5.10.3`), and the CLI
  computes it at run time (`constants.py:36 PROGRAM_VERSION = version(PROGRAM_NAME)`).
- PDN side: `crates/podnext/src/cli.rs`, PDN **v0.2.0** (`Cargo.toml:17`).

**Evidence rules held.** Every claim below cites `file:line` in one of those trees. Where the source is
available no document is cited. Sizes/counts are measured. Where a prior claim could not be reproduced, the
actual observation is quoted. Judgement calls ("PDN needs it") are labelled **inference**; everything else is
what the line says.

---

## 0. Re-test of the prior pass (`research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md`) — what held, what did not

| # | Prior claim | Re-tested result | Evidence |
|---|---|---|---|
| 1 | 138 `.c` files / 32,638 lines | **held** | measured above |
| 2 | 77 Python files / 29,015 lines | **held** | measured above |
| 3 | `cli/proot.c:142` binds `/dev/null`→`/etc/ld.so.preload` | line exists **but the framing is wrong** — see §1.3 | `src/cli/proot.c:142`, inside `handle_option_q` `:73`–`:145` |
| 4 | "there is no read-only bind; grep for `ro` matched only talloc_zero/bzero/strings.h" | **first clause held, second clause false** | `grep -rn 'MS_RDONLY\|readonly\|rdonly' src/` → **0 matches**; the `ro` word-grep does match a comment at `src/syscall/enter.c:434` and the `O_RDONLY` macro `src/compat.h:235` |
| 5 | `add_induced_bindings` is reconfiguration-only | **held** | `src/path/binding.c:641 if (tracee->reconf.tracee == NULL)`, comment `:640 /* Only for reconfiguration.  */`, defined `:634`, sole call `:756` |
| 6 | default bindings / option table | option table is in the **generated** `src/cli/proot.h` (`:92`), not `cli.c`; binding defaults set in `pre_initialize_bindings` `src/cli/proot.c:371` | `proot.h:92`, `proot.c:377`–`:388` |
| 7 | `derive_alias` examples | **held** | `helpers/docker/refs.py:362`–`:375` |
| 8 | Reference has no `--checksum` on URL install (roadmap 10.1 says "yes") | **roadmap cell is inaccurate for the reference** — no checksum option exists anywhere | `grep -rn 'checksum' commands/install.py commands/install_local.py helpers/download.py` → **0 matches**; URL path is `commands/install.py:314 download_file(url, tmp_archive, insecure=allow_insecure)` |

**Corrections to carry forward.** (a) The `/dev/null → /etc/ld.so.preload` bind is **not** a startup default
that every guest inherits; it fires only when `-q/--qemu` is given, because line 142 is inside
`handle_option_q` (`src/cli/proot.c:73`), which returns at `:145`. The prior doc's §1.3 presented it as a
default worth documenting for all guests — that reading does not survive the source. (b) The prior doc's
grep argument for "no read-only bind" used the wrong pattern; the *conclusion* is right (no
`MS_RDONLY`/`readonly` bind exists anywhere in `src/`) but the stated grep output was not what a re-run gives.

---

## 1. proot engine substrate (the layer PDN drives) — mostly S/C, cross-cutting to 10.2

### A1. Path translation is string-prefix rewriting, never identity
- **What:** `src/path/binding.c:239 int substitute_binding(...)`; `:250 if (!binding->need_substitution)`
  (symmetric bind → no rewrite); `:269 substitute_path_prefix(path, ref->length, reverse_ref->path,
  reverse_ref->length)`; the prefix splice is `src/path/path.c:731`–`:783`. Bindings are matched by
  longest-prefix against a sorted list: `binding.c:120 get_binding`, `:297 insort_binding`, `:381
  insort_binding2` (which sets `binding.c:383 binding->need_substitution = compare_paths(host,guest) != PATHS_ARE_EQUAL`).
- **Axis / gap:** S — every path the guest sees is a *name*, so the guest↔host boundary is a naming boundary
  (10.2, cross-cutting).
- **PDN needs it for v0.3.0?** Yes, as the substrate it already drives — no new PDN code. **Inference:** the
  competitive question does not apply; PDN cannot run without proot doing this. The consequence PDN must
  honour is the shim-integrity rule (a name crosses; an inode does not).
- **Acceptance test (can fail):** inside a guest, a path bound by name resolves to the host inode, and any
  PDN feature that must carry *identity* (not a name) across the boundary fails when the guest re-points the
  name between resolve and use.

### A2. No read-only bind exists
- **What:** `grep -rn 'MS_RDONLY\|readonly\|rdonly' src/` → **0 matches**. The only hits for `ro` as a word
  are a comment `src/syscall/enter.c:434` and `O_RDONLY` (`src/compat.h:235`, plus `O_RDONLY` uses at
  `enter.c:1872`). Binds are read-write; the only writability control is which side of the bind you are on.
- **Axis / gap:** S/C (10.2 `--bind`).
- **PDN needs it?** **No.** Competitive verdict (inference): proot-distro offers no read-only bind either
  (`login/bindings.py`, `proot_cmd.py:_add_custom_binds` build plain `--bind=` entries only), so a
  read-only-bind feature is not a place PDN can lose to it. PDN already documents the limitation.
- **Acceptance test:** `--bind src:dst:ro` is either refused with the reason or documented as read-write;
  and a guest write through any bind succeeds (proving no `ro` was silently applied).

### A3. Startup binding defaults — and the `-q` correction
- **What:** `src/cli/proot.c:371 pre_initialize_bindings` sets `cwd="."` (`:377`–`:381`) and `rootfs="/"`
  (`:384`–`:388`) when unset. The `/dev/null`→`/etc/ld.so.preload` bind is at `src/cli/proot.c:142`, inside
  `handle_option_q` (`:73`), so it applies only with `-q/--qemu`; `grep -rn 'ld.so.preload' src/` returns
  exactly that one line.
- **Axis / gap:** S (10.2). 
- **PDN needs it?** **No new mechanism.** The correction matters only so PDN does not document a default it
  does not inherit.
- **Acceptance test:** a native (non-`-q`) `login` shows the guest's `/etc/ld.so.preload` to be the image's
  own file (empty/absent), **not** `/dev/null`; a `-q` session shows it bound.

### A4. Nine extensions, one default-on
- **What:** enable path `src/extension/extension.c:103 initialize_extension` → `:115 callback(INITIALIZATION,…)`.
  **Only `mountinfo` is default-on:** `src/cli/cli.c:481-482 if (NULL == getenv("PROOT_NO_MOUNTINFO"))
  initialize_extension(tracee, mountinfo_callback, NULL)`. Every other extension is opt-in from a CLI handler in
  `src/cli/proot.c`: `kompat` `:168` (`-k/--kernel-release`), `fake_id0` `:186` (`-i/-0/-S`), `link2symlink`
  `:289` (`--link2symlink/-l`), `ashmem_memfd` `:302` (`--ashmem-memfd`), `sysvipc` `:315` (`--sysvipc`),
  `fix_symlink_size` `:324` (`-L`), `hidden_files` `:330` (`-H`), `port_switch` `:336` (`-p`).
- **Axis / gap:** C+S (10.2 escape hatches `--no-sysvipc`/`--no-link2symlink`; `--fix-low-ports`/`-p`).
- **PDN needs it for v0.3.0?** Yes — but only to *pass the flags through*, which it already does: PDN emits
  `--kill-on-exit`, `--link2symlink`, `--sysvipc`, `-L`, fake `--kernel-release` (`commands/login/proot_cmd.py:130`–`:158`
  shows the reference's identical order). Competitive verdict: without the escape-hatch parity PDN loses on the
  10.2 rows for hosts whose guests break under l2s/sysvipc.
- **Acceptance test:** `login --no-sysvipc` produces an argv with no `--sysvipc` and a per-container `/dev/shm`;
  `login --no-link2symlink` produces no `--link2symlink`; a privileged listener works under `-p`.

### A5. mountinfo default — why a guest's `/proc/mounts` carries no host binds
- **What:** `src/extension/mountinfo/mountinfo.c:191 mountinfo_callback`, enabled by default at
  `src/cli/cli.c:481`.
- **Axis / gap:** S (10.2).
- **PDN needs it?** Parity only; no new code. **Inference:** explanatory, not competitive.
- **Acceptance test:** inside a guest, `/proc/self/mountinfo` does not enumerate the host bind sources PDN passed.

### A6. Fake `/proc`+`/sys`, per-container `/dev/shm`, hardlink emulation (already ported to PDN)
- **What (reference):** `sysdata.py:527 setup_fake_sysdata`, `:604 fake_sysdata_bindings` (fake `/proc`+`/sys`
  under `containers/<name>/sysdata`); `shm.py:77 make_shm_dir` (mode `1777`), `:100 make_guest_tmp`;
  `l2s.py:56 resolve_l2s_target`, `:116 open_l2s_backing`, `:163 rewrite_l2s_targets`. PDN counterparts:
  `crates/proot-distro-core/src/sysdata.rs`, and l2s handling in `proot-distro-core`.
- **Axis / gap:** C (10.2 living inside), 10.5 "already ported — verify parity, do not rewrite".
- **PDN needs it for v0.3.0?** Yes, parity (verify, do not rewrite). Competitive verdict: a guest without a
  writable `/dev/shm` or with dpkg symlink-warnings loses to the reference; PDN already ships these.
- **Acceptance test:** `/dev/shm` is a per-container dir mode `1777`; `/proc` stubs present; an image carrying a
  hard link survives as an l2s symlink and `rename` rewrites its targets.

### A7. The Android bind set (default, no flag)
- **What:** `login/bindings.py:35 storage_bindings` (`/storage`, `/sdcard`, `/mnt/sdcard`) and `:58
  system_bindings` (`/apex`, `/system`, `/vendor`, linker config). Composition and gating in
  `commands/login/proot_cmd.py:161`–`:216`: Dalvik caches `:246 _add_dalvik_cache_binds`, Termux app private
  dirs `:264 _add_termux_app_binds` (normal-type only, `:180`), `/dev/urandom→/dev/random` etc. `:219`.
- **Axis / gap:** M (10.2 `--bind` / reaching the phone's storage without editing configs).
- **PDN needs it for v0.3.0?** **Yes** — the `--bind` *flag* at minimum. Competitive verdict (inference):
  PDN's `login` (`crates/podnext/src/cli.rs:353`–`:365`) takes no `--bind`, so a user cannot reach `/sdcard`
  at all; the reference does it by default. Without the flag PDN loses the 10.2 `--bind` row outright.
- **Acceptance test:** `login c --bind /sdcard:/mnt/sd` works and pass-through mounts are verified inside;
  a termux-type guest does not see the host's private dirs.

---

## 2. Bringing something in (10.1) — M/C

### B1. Docker's reference grammar, and alias derivation
- **What:** `helpers/docker/refs.py:179 parse_reference` (Docker `distribution/reference` grammar; rejects
  empty/`>1024`/name-over-255/uppercase/digest-not-sha256/port>65535), `:362 derive_alias` (`ubuntu:24.04`→`ubuntu`,
  `ghcr.io/foo/bar:tag`→`bar`), `:327 canonical_ref`, `:350 with_explicit_tag`, `:302 parse_image_ref` (total).
- **Axis / gap:** M/C, 10.1 "Install from a registry reference — yes, with alias derivation".
- **PDN needs it for v0.3.0?** **Yes.** PDN today requires an explicit name (`cli.rs:316-318 -n/--name`).
  Competitive verdict (inference): the 10.1 acceptance row is literally *"`podnext install alpine:3.20`
  creates a container named by the derived alias, no extra flags"* — without alias derivation PDN fails that
  row that the reference passes.
- **Acceptance test:** `podnext install alpine:3.20` (no `--name`) creates a container named `alpine`;
  `podnext install ghcr.io/foo/bar:tag` creates `bar`.

### B2. One command, three sources (registry / local / URL)
- **What:** `commands/install.py:74 _is_local_path` (leading `/`, `./`, `../`, `~`; `:82`), `:85 _is_url`
  (`http://`,`https://`), `:90 _derive_local_name`, dispatch `:164`–`:174`, then `:308
  install_from_local_file`, `:314 download_file(url, …, insecure=allow_insecure)`, `:320 pull_image`.
  Flags: `-a/--architecture` `:151`, `--allow-insecure` `:183`, `-q`.
- **Axis / gap:** M, 10.1 (registry ref, local rootfs tarball, URL).
- **PDN needs it for v0.3.0?** **Yes** for local + URL. Competitive verdict: PDN's `install` takes only an
  image ref (`cli.rs:313-322`); the reference installs a local tarball offline and from a URL. Losing scenario:
  a user with a rootfs tarball and no network can use the reference, not PDN.
- **Acceptance test:** `podnext install ./rootfs.tar.gz` works offline; `podnext install https://…/rootfs.tar.xz`
  works. (Note the checksum row in roadmap 10.1 is **not** met by the reference — see §0 item 8 — so a
  `--checksum` flag is a place PDN can *beat* it, not merely match.)

### B3. Local OCI image-layout archive, digests verified
- **What:** `commands/install_local.py:24` (format auto-detect), `:509-513` (`oci-layout` marker probe, ≤500
  members), `:137 _oci_blob_path`, `:145 validate_digest`, `:166 _oci_read_capped` (`:67 _MAX_JSON_BYTES =
  16*1024*1024`, `:77 _MAX_OCI_MEMBERS = 16384`, `:179` refuse over-cap), `:245 require_data_digest`, `:331
  layer digest mismatch → RuntimeError`. Plain-tar branch: `:101 detect_strip_count`, `:122 extract_plain_tar`.
- **Axis / gap:** M, 10.1 "Install from a local OCI image-layout archive (marker-detected, blobs verified
  against the declared digests)".
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict: this row is one the reference passes and PDN
  fails; a mismatch must be refused, not silently extracted.
- **Acceptance test:** `podnext install ./image.oci.tar` verifies every blob digest and **refuses** a tarball
  whose layer bytes do not hash to the declared digest.

### B4. Digest validation as a choke point
- **What:** `helpers/docker/cache.py:99 validate_digest` — docstring `:100` "choke point before any conversion
  of an untrusted digest into a filesystem path"; body `:107` rejects non-conformant, `:108 raise RuntimeError(
  f"Malformed digest: {digest!r}")`; grammar `:94 _DIGEST_RE`.
- **Axis / gap:** S, cross-cutting (10.1/10.3).
- **PDN needs it for v0.3.0?** **Yes** — PDN builds an image store where a digest becomes a filename.
  Competitive verdict: not a place to *lose* a feature race, but a place to lose the security comparison.
- **Acceptance test:** a crafted digest containing `..` or a path separator is refused and the offending value
  is quoted back.

### B5. Multi-arch selection + emulator
- **What:** `helpers/docker/pull.py:186 _pick_platform`, `:229 _child_digest` (manifest-list entry by
  arch+variant); `commands/install.py:151-153` arch override; `arch.py:195 ARCH_UNAME_M`, `:204
  _QEMU_BIN_NAMES` (`qemu-aarch64`,`qemu-arm`,`qemu-i386`,`qemu-riscv64`,`qemu-x86_64`), `:257
  get_emulator_args`, `:221 get_proot_bin` (honours `PD_PROOT_BIN`).
- **Axis / gap:** C, 10.1 "Multi-arch — `--architecture` plus emulator".
- **PDN needs it for v0.3.0?** **Yes.** PDN has only a legacy `--qemu` pre-pass (roadmap 10.1). Competitive
  verdict: PDN loses the multi-arch row until an x86_64 rootfs runs on this aarch64 device through the
  emulator with `--architecture`.
- **Acceptance test:** `podnext install --architecture x86_64 …` then `login` runs an x86_64 binary on aarch64
  via the emulator, documented.

### B6. Registry pull / push
- **What:** `helpers/docker/pull.py:438 pull_image`, `:658` cached-layer reuse; `helpers/docker/push.py:343
  push_image`, `:448 _blob_exists` (skip an already-present blob), `:150 _ProgressReader` (streamed upload).
- **Axis / gap:** M/C, 10.1 and 10.3 "Registry push/pull of built images".
- **PDN needs it for v0.3.0?** **Yes**, but it is gated on retaining layers — PDN today cannot (roadmap 10.3
  "impossible: no layers retained (#39)").
- **Acceptance test:** `build` → `push` → `pull` on a second machine yields a runnable container.

### B7. Credentials are origin-bound; no scheme downgrade
- **What:** `helpers/docker/transport.py:122 same_auth_origin` (`:151-155` — same-origin requires scheme+host+port
  match, or http→https on the default ports only), `:157 class AuthStrippingRedirectHandler`, `:177
  new_req.headers.pop("Authorization", None)`, `:582 _require_usable_realm`; `helpers/download.py:289` refuses
  HTTPS→plain-HTTP, `:283` only http/https-with-host followed, `:245 _FOLLOWED_SCHEMES=("http","https")`,
  `:403` handler list excludes ftp/file/data.
- **Axis / gap:** S, cross-cutting (roadmap #74 records the same gap from PDN's side).
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict (inference): a security-comparison loss, not a
  feature-race loss.
- **Acceptance test:** a same-host HTTPS→HTTP redirect drops the `Authorization` header and the transfer fails;
  an `ftp://`/`file://`/`data://` URL is not followed.

### B8. Retry is a policy, not a loop
- **What:** `helpers/download.py:595 retry_http`, `:553 is_retryable_http_error` (`:578` — 4xx not retried
  except 408/429, `:580` cert/plaintext-TLS not retried), `:512 retry_after` (`:528` delta-seconds, `:531`
  HTTP-date via `email.utils.parsedate_to_datetime`, `:539` past-date→0), `:495 MAX_RETRY_AFTER=60`, `:509
  _RETRY_LATER_CODES=(429,503)`, `:149 require_complete_body`, `:133 declared_length`.
- **Axis / gap:** S/O.
- **PDN needs it for v0.3.0?** **Yes** — PDN's preflight grew a hand-rolled retry (harness rule 79); the version
  to implement is the tool's.
- **Acceptance test:** a 404 is not retried; a body shorter than Content-Length fails with both lengths named.

### B9. `search` against Docker Hub
- **What:** `helpers/docker/search.py:68 SEARCH_URL = "https://index.docker.io/v1/search"`, `:142 _fetch_page`,
  `:145` params `{"q","n","page"}`, `:191 search_images`, result fields `:133-136` (`name`,`description`,
  `stars`,`pulls`,`official`,`automated`). CLI: `parser.py:217`; PDN already has `search` (`cli.rs:546`).
- **Axis / gap:** C/M, 10.4 "Curated distro names… `podnext search` lists installable names and `install <name>`".
- **PDN needs it for v0.3.0?** Partly — `search` exists; the **curated-name → install** link does not
  (installing "a name like `alpine` without a registry reference" is B1).
- **Acceptance test:** `podnext search` lists installable names and `podnext install <name>` uses them.

### B10. Archive extraction is path-confined and whiteout-aware
- **What:** `helpers/tar_extract.py:369` drops members with `..`/empty component, `:362` skips
  block/char/FIFO, `:558 safe_resolve_parts`, `:588 link_budget=40`, `:593` clamps `..`, `:642` writes O_EXCL
  off the parent descriptor; `:449 _apply_whiteout` (`:456 .wh..wh..opq`, `:465 .wh.`, `:481` refuses a target
  of `''`/`.`/`..`). PDN has `untar.rs` whiteouts (roadmap 10.5).
- **Axis / gap:** S.
- **PDN needs it for v0.3.0?** Parity (verify). Competitive verdict (inference): the `.wh...`→`rmtree(parent's
  parent)` hazard the reference documents (`:481` comment) is a class PDN must already refuse; verify, do not
  rewrite.
- **Acceptance test:** a crafted layer with a `.wh...` member does not delete anything above the extraction
  root.

---

## 3. Living inside it (10.2) — M

### C1. The full login/run option polygon (this is the gap 10.2 surface)
- **What:** `parser.py:116 _add_login_or_run_common` defines, for `login` **and** `run`: `-u/--user` (`:118`),
  `-P/--redirect-ports` and (login only) `--fix-low-ports` (`:120`–`:126`), `--isolated`/`--minimal` on Termux
  (`:127`–`:130`), `--shared-home`/`--termux-home` mutually exclusive (`:131`–`:137`), `--shared-tmp` (`:138`),
  `--shared-x11` (`:139`), `-b/--bind` repeatable (`:140`–`:142`), `--no-link2symlink`/`--no-sysvipc`/
  `--no-kill-on-exit` on Termux (`:143`–`:152`), `--emulator` (`:153`), `--kernel` (`:154`), `--hostname`
  (`:155`), `-w/--work-dir` (`:156`), `-e/--env` repeatable (`:157`), `-d/--detach` (`:158`). `login` alone adds
  `--get-proot-cmd` (`:267`). `login` itself takes `--detach` (`commands/login/__init__.py:363,565`).
- **Axis / gap:** M, exactly the 10.2 table.
- **PDN needs it for v0.3.0?** **Yes — this is the whole axis.** PDN's `login` (`cli.rs:353`–`:365`) declares only
  `user` (refused), `isolated`, and trailing `command`. Competitive verdict (inference): the 10.2 table is a
  one-for-one list of what the reference has and PDN does not; each missing flag is a row PDN loses.
- **Acceptance test:** each 10.2 row, individually — `--bind` verified inside; `--shared-tmp`/`--shared-home`
  testable; `PWD` matches `--work-dir`; `hostname` matches `--hostname`; a listener on 80 reachable under
  `--redirect-ports`; `--no-sysvipc`/`--no-link2symlink` honoured; `--detach` then reattach; `--env` visible;
  `--get-proot-cmd` prints a reproducing command.

### C2. proot argv assembly order
- **What:** `commands/login/proot_cmd.py:55 build_proot_args`; order documented `:22`–`:33` and implemented
  `:92`–`:127`: extensions `:94`/`:130`, `--change-id=uid:gid` for non-termux `:104`, `--rootfs=`/`--cwd=`,
  baseline `--bind=/dev /proc /sys` `:107`–`:109`, non-minimal binds `:111`–`:119`, custom binds `:121`, `-p`
  `:123`, inner `:126`. `--rootfs=.` is deliberate: the caller chdirs into a pinned descriptor so proot
  resolves the guest root against `getcwd()` (inode) not a name (`:75`–`:82`).
- **Axis / gap:** M/C (10.2).
- **PDN needs it for v0.3.0?** Yes, as the intended shape of PDN's own argv builder; the descriptor-pinned
  `rootfs=.` is the part worth copying (it is the same identity-vs-name rule as A1).
- **Acceptance test:** `login --get-proot-cmd` prints a copy-pasteable command; PDN's own session resolves its
  root by descriptor, so a concurrent session re-pointing the rootfs name cannot redirect it.

### C3. `--get-proot-cmd` and shell quoting
- **What:** flag `parser.py:267`; quoting helper `commands/login/quoting.py:35 dq` (POSIX double-quote).
- **Axis / gap:** M (10.2 debuggability).
- **PDN needs it?** Yes per 10.2. **Inference:** low cost, high debuggability value.
- **Acceptance test:** the printed command reproduces the session.

### C4. Image `Env`, identity variables, and the profile.d snippet
- **What:** `commands/login/env.py:104 read_manifest_env`, `:119 image_env_pairs` (blocklist `:83
  IMAGE_ENV_BLOCKED` incl. `LD_*`/`PROOT_*` via `execenv.is_host_exec_var`), `:148 identity_env_pairs`
  (`PD_CONTAINER`, `container`, `PD_IMAGE`, `PD_IMAGE_ID`), `:179 inject_termux_profile`, `:258
  _write_profile_snippet` (writes `/etc/profile.d/termux-profile.sh`).
- **Axis / gap:** C/M (10.2 `--env`, env semantics).
- **PDN needs it for v0.3.0?** **The image-`Env` half: yes.** PDN discards the image config today. **The
  profile.d re-injection: do not copy** (see §9 Dead ends) — it re-injects host-derived values and exists
  because the reference's `login` *loses* the environment to `/etc/profile`.
- **Acceptance test:** a var present in the image's `Env` is visible inside the guest; `--env FOO=bar` is
  visible; `LD_LIBRARY_PATH` from an image `Env` is **not** applied host-side.

### C5. hostname / work-dir / kernel
- **What:** `parser.py:155`–`:156`; `proot_cmd.py:151`–`:156` fakes `--kernel-release` from
  `DEFAULT_FAKE_KERNEL_VERSION` (`constants.py:120`) and `ARCH_UNAME_M` (`arch.py:195`).
- **Axis / gap:** M (10.2).
- **PDN needs it?** Yes per 10.2.
- **Acceptance test:** `hostname` inside matches `--hostname`; `PWD` matches `--work-dir`.

### C6. User lookup (`--user`)
- **What:** `commands/login/passwd.py:70 read_passwd_entry`, `:90 find_passwd_by_uid`; the flag is
  `parser.py:118`. The reference implements the lookup; PDN refuses it (`cli.rs:356-358`). Roadmap 10.2 asks
  for the semantics stated honestly.
- **Axis / gap:** C (10.2 `--user` semantics).
- **PDN needs it for v0.3.0?** **Not required** — the 10.2 row is "`--user` documented with what PRoot can and
  cannot enforce", not "implemented". Competitive verdict (inference): PDN's honest refusal is not a loss
  against the reference, whose `--user` is also a mapped-root illusion.
- **Acceptance test:** `--user` is documented with its PRoot limits; a guest still runs as the mapped root.

---

## 4. Moving things out and between devices (10.3) — M

### D1. backup / restore with compression (PDN has this)
- **What:** `commands/backup.py:451 command_backup` (`:452` docstring "Archive an installed container to a tar
  file or stdout"), choices gzip/bzip2/xz/zstd/none (`:94`), ext map `:76`, tar mode `:615`; layout `:22`
  (`<name>/manifest.json` + `<name>/rootfs/`). `commands/restore.py:441 command_restore`, magic detect `:85`,
  streaming `:561`.
- **Axis / gap:** M/C, 10.3 "Backup/restore — already passing; keep it".
- **PDN needs it?** Already has it (`cli.rs:679 Backup(...)`).
- **Acceptance test:** backup → restore round-trips a container with its manifest and rootfs.

### D2. Portable export as an OCI layout archive
- **What:** there is **no `export` subcommand** — the reference's export surface is `build -o FILE`
  (`parser.py:367-370`, `helpers/oci_writer.py:155 write_oci_archive`, docker-load compatible), and import is
  `install <archive>` (`commands/install_local.py`). Commands list `commands/help/pages.py:1061 TOP_COMMANDS`
  has no `export`/`import`.
- **Axis / gap:** M, 10.3 "Portable export — OCI layout archive".
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict: PDN has no export today; the reference can produce
  an OCI-layout tar and re-install it. Losing scenario: moving a container between devices.
- **Acceptance test:** `podnext export <c> ./c.oci.tar` then `podnext install ./c.oci.tar` on another device
  yields the same container.

### D3. copy / sync between host and container
- **What:** `commands/copy.py:63 command_copy` (`:64` docstring), `[CONTAINER:]path` syntax `:24`;
  `commands/sync.py:882` docstring, checksum `parser.py:342`, delete `:343`, `-m`/`-r` on copy `:329-330`.
- **Axis / gap:** M, cross-cutting move.
- **PDN needs it?** Already has `copy`/`sync` (`cli.rs:490,527`).
- **Acceptance test:** existing behaviour retained; `[CONTAINER:]PATH` honoured both directions.

### D4. reset / rename / remove
- **What:** `commands/reset.py:45` (`:95` delegates to `command_install` with the stored ref/arch/name),
  `commands/rename.py:51` (`:94 os.rename` via containers-dir fds, rewrites l2s symlinks), `commands/remove.py:109`.
- **Axis / gap:** M/O.
- **PDN needs it?** Has all three (`cli.rs:454,472,512`).
- **Acceptance test:** existing.

---

## 5. Ecosystem (10.4) — O mostly

### E1. Short alias `pd`
- **What:** `__init__.py:23-24` — "exposed as the `proot-distro` and `pd` console scripts via pyproject.toml".
- **Axis / gap:** O, 10.4 "Short alias `pd`".
- **PDN needs it for v0.3.0?** **Yes** (trivial). Competitive verdict (inference): `pd` is a one-line packaging
  win the reference has; PDN does not (`Commands` name is `podnext`).
- **Acceptance test:** `pd` resolves to `podnext` in a Termux package.

### E2. Completions (bash + fish + zsh)
- **What:** shipped files `completions/proot-distro.bash`, `completions/proot-distro.fish`,
  `completions/_proot-distro` (`head -1` of the zsh file: `#compdef proot-distro pd`).
- **Axis / gap:** O, 10.4 "Completions — bash + fish".
- **PDN needs it for v0.3.0?** **Yes.** PDN has `completion {shell}` (`cli.rs:752-757`) — verify both bash and
  fish emit and load.
- **Acceptance test:** `podnext completion bash` and `podnext completion fish` both emit loadable scripts.

### E3. Per-command help pages, with a stated status and an example
- **What:** `commands/help/pages.py:43 HELP_PAGES` (18 pages: build `:44`, push `:166`, backup `:251`,
  clear-cache `:281`, copy `:350`, install `:389`, search `:509`, list `:565`, login `:594`, ps `:747`, kill
  `:761`, remove `:802`, rename `:829`, reset `:849`, restore `:864`, run `:894`, sync `:1010`); overview
  `commands/help/__init__.py:52 command_help`; dispatch `:49 HELP_COMMANDS`.
- **Axis / gap:** O, 10.4 "Discoverable help — a help page per command".
- **PDN needs it for v0.3.0?** Yes. PDN already tags each command `[stable]`/`[experimental]` and carries
  `long_about` with `EXAMPLES:` (`cli.rs:305` etc.) — verify every command states status and one example.
- **Acceptance test:** every command's `--help` states its stable/experimental status and one example.

### E4. Command aliases
- **What:** `parser.py:103 ALIAS_TO_CANONICAL` — `add/i/in/ins→install`, `rm→remove`, `sh→login`,
  `li/ls→list`, `bak/bkp→backup`, `clear/cl→clear-cache`, `cp→copy`, `s/se→search`, `h/he/hel→help`.
- **Axis / gap:** O.
- **PDN needs it?** Partly; PDN has `ls`, `rm` visible aliases (`cli.rs:326,448`). Competitive verdict
  (inference): the extra aliases are cheap wins, not axis-deciders.
- **Acceptance test:** each alias resolves to its canonical command.

---

## 6. State, session, locking, security (S/O, cross-cutting 10.2/10.5)

### F1. Descriptor VFS — every write into a container tree goes through a dir fd
- **What:** `dirfd.py:140 opendir_at`, `:157 open_file_at`, `:162 open_regular_at` (adds `O_NONBLOCK`,
  `:180 if not stat.S_ISREG` → refuses FIFO/device), `:188 open_new_at` (`:210 O_CREAT|O_EXCL`),
  `:218 makedirs_under`, `:252 makedirs_at`, `:316 descend_at`, `:378 listdir_at`, `:384 lstat_at`,
  `:660 copy_file_at`, `:728 copy_symlink_at`, `:982 copy_tree_at`, `:1176 rmtree_at` (docstring `:1178`
  "descending without following symlinks"), `:937 count_tree_at`, `:753 close_frames`, `:145 reopen`, sparse
  helpers `:535 _looks_sparse`, `:553 _data_extents`, `:591 _copy_extents`, `:606 _copy_skipping_zeros`.
- **Axis / gap:** S, 10.5 "descriptor VFS" (the reference area 10.5 lists as `dirfd`).
- **PDN needs it for v0.3.0?** **Yes** (roadmap 10.5 says build `dirfd`-equivalent; PDN already lists
  `proot-distro-core`). Competitive verdict (inference): on this platform the guest writes the same tree with
  the same uid, so a composed-path write is a TOCTOU hole the reference closes.
- **Acceptance test:** a container with a symlink or FIFO planted at a target name makes the operation
  **refuse**, not follow.

### F2. The runtime dir is a trust root, and it is guest-writable
- **What:** `locking.py:133 _locks_dir_fd`, `:398 _open_lock_file` (`O_NOFOLLOW` + type check),
  `:466 _is_planted`, `:478 _is_denied`, `:483 _drop_planted`; `statedir.py:62 STATE_ROOTS`, `:156
  open_state_dir` (O_NOFOLLOW walk), `:82 read_state_file` (capped). Docstrings: `_open_cache_root` names the
  runtime dir as guest-writable.
- **Axis / gap:** S.
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict: a security-comparison item.
- **Acceptance test:** a planted symlink or directory at a lock name produces a typed refusal, not a follow.

### F3. Locks carry a hint; "cannot measure" is a state
- **What:** `locking.py:247 _lock_info_at` (docstring `:250` "Reads the lock file's first line (PID + command
  name)"), `:346 busy_locks` (`:348` docstring), `:388 class LockStateUnknown` (docstring: "Nothing is held"
  and "this process is not allowed to look" are different answers).
- **Axis / gap:** S/O.
- **PDN needs it for v0.3.0?** **Yes** (cheap, and the reference's discipline matches PDN's own exit-2
  "could not measure"). Competitive verdict (inference): the refusing message naming the holder is a usability
  win.
- **Acceptance test:** a second `install` of the same container prints the PID of the first; an unanswerable
  probe reports "unknown" rather than "busy".

### F4. A session is a file held open
- **What:** `session.py:115 register_session` (docstring `:120` "Must be called from the process that
  immediately exec's into proot"), `:174 os.set_inheritable(fd.fileno(), True)` (clears `O_CLOEXEC` so proot
  and descendants keep it), `:376 session_is_live` (flock probe), `:392 session_holders`, `:277
  _validate_record`.
- **Axis / gap:** S, 10.2 detach/attach, 10.5 session registry.
- **PDN needs it for v0.3.0?** Yes for the liveness test. Competitive verdict (inference): "live iff some
  process holds the fd" is stronger than a PID and has no PID-reuse hazard.
- **Acceptance test:** detach, then reattach to the same live shell; a stale/forged session record is not
  resurrected (`_validate_record`).

### F5. Output shapes: `ps`, `kill`, `list`
- **What:** `ps.py:46 _HEADERS = ("PID","CONTAINER","TYPE","USER","UPTIME","COMMAND")`, `:64` quiet prints
  pids, `:75` TYPE gets `*` for a detached session; `kill.py:88 command_kill`, `:94` default SIGTERM, `:120`
  a bare number is a PID else a container name (`parser.py:414-416` `--signal`/`--all`); `list.py:44
  _IMAGE_HEADERS = ("IMAGE","ARCH","ID","SIZE","CREATED")`, `:93` installed header, `:96` bullet line.
- **Axis / gap:** O.
- **PDN needs it for v0.3.0?** Parity, not bytes. Competitive verdict (inference): shape compatibility is a
  "must match" for migrating users, not an axis-decider.
- **Acceptance test:** PDN's `ps` exposes PID, container, type, user, uptime, command; `list` has an installed
  section and an `--image` section.

### F6. Cache eviction by reference count
- **What:** `clear_cache.py:225 command_clear_cache`, `:300 _referenced_blob_names` (docstring `:301` "the
  layer-cache file names that are still referenced"), `:168 _tree_size`, `:117 _relax_cache_root`;
  `helpers/docker/cache.py:515 open_verified_layer` (re-hash before use, evict on mismatch), `:561
  open_required_layer`.
- **Axis / gap:** O, 10.5.
- **PDN needs it for v0.3.0?** **Yes** (PDN has a cache and no eviction). Competitive verdict (inference):
  without a referenced-set sweep, PDN cannot offer `--orphan` safely.
- **Acceptance test:** `clear-cache --orphan` removes exactly the unreferenced blobs, counted before and after.

### F7. State paths honour `TERMUX__HOME`/`TERMUX__PREFIX`/XDG
- **What:** `constants.py:48 TERMUX_HOME = os.environ.get("TERMUX__HOME", …)`, `:51 TERMUX_PREFIX =
  os.environ.get("TERMUX__PREFIX", …)`, `:75` Termux → `RUNTIME_DIR = TERMUX_PREFIX/var/lib/proot-distro`
  (`:76`), else XDG (`:79`–`:86`); `CONTAINERS_DIR :89`, `SESSIONS_DIR :92`, `LAYER_CACHE_DIR :98`,
  `MANIFEST_CACHE_DIR :99`.
- **Axis / gap:** O/S, 10.5 "state dir + paths — honours `TERMUX__HOME`, `TERMUX__PREFIX`, XDG; replace 12
  hard-coded Termux paths".
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict (inference): on a non-standard Termux install the
  reference relocates; PDN with hard-coded paths does not.
- **Acceptance test:** exporting `TERMUX__HOME`/`TERMUX__PREFIX`/`XDG_DATA_HOME` moves PDN's state.

### F8. Nested-proot refusal (and other CLI guards)
- **What:** `cli.py:90 _refuse_nested_proot`, message `:92`–`:99`; unknown-command guard `:213`; option guard
  `:310`–`:318`; signal routing `:258`.
- **Axis / gap:** S.
- **PDN needs it?** Parity/optional. **Inference:** a good error beats a hang.
- **Acceptance test:** running inside another proot refuses with a clear message and exit 1.

### F9. Message/verbosity shapes
- **What:** `message.py:210 log_info("[*] …")`, `:217 log_error("[!] …")`, `:222 warn("Warning: …")`, `:227
  crit_error("Error: …")`, `:187 msg`, `:242 quote_path` (escapes control chars in untrusted names), `:116
  tty_safe_for_writes`, quiet `:171 set_quiet`.
- **Axis / gap:** O.
- **PDN needs it?** Match **semantics** (errors always shown, info suppressed by quiet, untrusted names
  escaped), not bytes.
- **Acceptance test:** under `--quiet`, info lines vanish but errors remain; a container name containing ESC
  is escaped in every message that prints it.

### F10. Container-name rule is centralised
- **What:** `names.py:40 _NAME_RE`, `:42 NAME_RULE_HINT`, `:53 require_valid_name` (calls `crit_error`).
- **Axis / gap:** S.
- **PDN needs it?** Parity.
- **Acceptance test:** an invalid name is refused with the rule quoted.

---

## 7. Build engine and image store (10.5 spine) — C/O

### G1. RUN steps are content-addressed and cached
- **What:** `helpers/build_engine/run_step.py:82 do_run` (docstring `:85` "Cache lookup happens first: a
  recipe-hash hit applies the cached … OCI layer"), `:106 compute_recipe_hash`, `:110 cache_lookup`, `:176
  cache_record`; extra inputs `:192 _run_extra_inputs`, `:208 _identity_env`.
- **Axis / gap:** O/C, 10.5 "build engine + layer differ".
- **PDN needs it for v0.3.0?** Yes (PDN's `build` today is a single-rootfs builder).
- **Acceptance test:** a second build of the same recipe hits the cache (no re-run).

### G2. The launcher is a subreaper
- **What:** `helpers/build_engine/run_step.py:435 _become_subreaper`, `:461 _children_of`, used at `:323`;
  docstring "Ask the kernel to reparent orphaned descendants onto this process."
- **Axis / gap:** S/O.
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict (inference): PDN's soak harness *counts* leftovers
  after a session; making the launcher a subreaper turns "count" into "reap" and makes the rest findable.
- **Acceptance test:** the soak harness reports zero leftovers **and**, when it does not, an explicit list.

### G3. WORKDIR emits the thin layer it needs
- **What:** `helpers/build_engine/handlers.py:176 do_workdir` (docstring `:177` "set the cwd and create the
  directory on disk. Emits a thin layer covering any newly-created ancestor directories"); handler table
  `:389`; `:76 do_env`, `:133 do_user`, `:265 do_cmd`, `:271 do_entrypoint`.
- **Axis / gap:** C/S.
- **PDN needs it for v0.3.0?** **Yes.** Competitive verdict (inference): a built image whose `WORKDIR` does not
  exist at run time is a correctness bug the reference avoids.
- **Acceptance test:** a built image's `WORKDIR` exists at run time.

### G4. Multi-stage, one layer per instruction, whiteouts
- **What:** `helpers/build_engine/engine.py` owns stage state (`_make_stage_dirs` `:275`, `_stage_rootfs_fd`
  `:201`, `Stage` `handlers`/`stage.py`); `helpers/layer_diff.py:480 write_layer_tar` (gzip layers `:449`),
  `helpers/oci_writer.py:57 build_manifest_and_config`, `:155 write_oci_archive`.
- **Axis / gap:** C, 10.5 "build engine + layer differ … replaces the current single-rootfs builder".
- **PDN needs it for v0.3.0?** Yes — it is the spine (roadmap 10.6: the image store is the spine).
- **Acceptance test:** a multi-stage Dockerfile builds; each instruction produces a layer; `docker load` of the
  `-o` archive runs the image.

---

## 8. The exact proot-distro UX surface PDN must match or beat

**Version:** `5.10.3` (`proot_distro-5.10.3.dist-info/METADATA: Version: 5.10.3`).

**Commands** (`commands/help/pages.py:1061 TOP_COMMANDS`, 18 total) with aliases (`parser.py:103`):

```
help (h/he/hel)      search (s/se)      install (add/i/in/ins)   list (li/ls)
login (sh)           run                ps                       kill
remove (rm)          rename             reset                    backup (bak/bkp)
restore              clear-cache (clear/cl)                     copy (cp)
sync                 build              push
```

**Flags per command** (`parser.py:161`–`:417`):

- `install`: `IMAGE`, `-n/--name`|`--override-alias`, `-a/--architecture`, `--allow-insecure`, `-q/--quiet`, `-h`.
- `search`: `QUERY?`, `-l/--limit`, `-q`, `-h`.
- `remove`: `CONTAINER|IMAGE?`, `-i/--image`, `-a/--architecture`, `-v`|`-q`, `-h`.
- `rename`: `ORIG NEW`, `-q`, `-h`.  `reset`: `NAME`, `-q`, `-h`.
- `login`: `NAME`, common set below, `--get-proot-cmd`, inner `…`, `-h`.
- `run`: `NAME`, common set below, `--get-proot-cmd`, `-h`.
- common (`:116`): `-u/--user` (default `root`), `-P/--redirect-ports`, `--fix-low-ports`(login), `--isolated`|
  `--minimal`(Termux), `--shared-home`|`--termux-home`, `--shared-tmp`, `--shared-x11`, `-b/--bind PATH[:PATH]`,
  `--no-link2symlink`/`--no-sysvipc`/`--no-kill-on-exit`(Termux), `--emulator PATH`, `--kernel STRING`,
  `--hostname STRING`, `-w/--work-dir PATH`, `-e/--env VAR=VALUE`, `-d/--detach`.
- `list`: `-i/--image`, `-h`, `-q`.
- `backup`: `NAME`, `-o/--output FILE`, `-c/--compress {gzip,bzip2,xz,zstd,none}`, `-v`|`-q`, `-h`.
- `restore`: `ARCHIVE?`, `-v`|`-q`, `-h`.
- `clear-cache`: `--orphan`, `--build-cache`, `-v`|`-q`, `-h`.
- `copy`: `SRC DEST`, `-v`|`-q`, `-m/--move`, `-r/--recursive`, `-h`.  `sync`: `SRC DEST`, `-v`|`-q`, `-c/--checksum`, `-d/--delete`, `-h`.
- `build`: `PATH=.`, `-f/--file`, `-t/--tag`, `--build-arg`, `-a/--architecture`, `--target`, `--emulator`, `-o/--output`, `--install-as`, `--no-cache`, `-v`|`-q`, `-h`.
- `push`: `IMAGE`, `-a/--architecture`, `--allow-insecure`, `-q`, `-h`.
- `ps`: `-q`, `-h`.  `kill`: `PID|CONTAINER?`, `-s/--signal`, `--all`, `-h`.

**Output shapes** (`message.py`): info `[*] text` (`:214`), error `[!] text` (`:219`), `Warning: text` (`:224`),
`Error: text` (`:229`); `list` installed bullet `  * name` (`list.py:96`) and image table `IMAGE ARCH ID SIZE
CREATED` (`list.py:44`); `ps` table `PID CONTAINER TYPE USER UPTIME COMMAND` (`ps.py:46`).

**Error text** (`cli.py:224,314`, `names.py:61`, `message.py:229`): `unknown command 'X'.`,
`unrecognized option: 'X'.`, `unexpected argument: 'X'.`, `container name 'X' is not valid. It must begin with
a letter or digit and contain only letters, digits, underscores, dots, or hyphens.`

**Environment variables:** `PD_FORCE_NO_COLORS` (`message.py:108`), `PD_DOCKER_AUTH` (`transport.py:657`;
help text `commands/help/__init__.py:153`), `PD_PROOT_BIN` (`cli.py:133`, `arch.py:221`), passthrough of
`PROOT_NO_SECCOMP` and the user's own env (`env.py:99`, `:133`).

**Data location:** `RUNTIME_DIR = $TERMUX__PREFIX/var/lib/proot-distro` (`constants.py:76`), printed by
`command_help` `commands/help/__init__.py:134`.

**What PDN offers today (v0.2.0, `crates/podnext/src/cli.rs`):** the reference's 18 commands are all present in
some form **except `export`/`import`**, plus PDN-only commands (`doctor` `:625`, `info` `:586`, `version`
`:605`, `attach` `:574`, `sessions` `:563`, and system groups). PDN's `login` is the narrow one (`:353`). PDN
uses `--format {table,json}` (`OutputFormat`) throughout, which the reference lacks.

---

## 9. Dead ends — do NOT copy

| Dead end | Where | Why |
|---|---|---|
| Transcribing the reference's GPL-3.0 code | package header `__init__.py:7`–`:18` vs PDN `LICENSE` | PDN is GPL-2.0-or-later (roadmap 10.5); match the **architecture**, never the text. |
| The loose reference splitter | `helpers/docker/refs.py:270 _split_loosely` | Pre-grammar string cutting kept only as a fallback; a new implementation must use `parse_reference` (`:179`) as the primary path, not this. |
| The host-env profile re-injection | `commands/login/env.py:179 inject_termux_profile`, `:258 _write_profile_snippet` | It exists because the reference's `login` *loses* the environment to `/etc/profile`; it deliberately writes host-derived values into the guest. PDN's deliberate isolation is the safer side (prior pass item 5, re-confirmed by source: the snippet exports image `Env`, identity vars and user `--env` only because /etc/profile discards what proot inherited). Take the image-`Env` half (C4), not the host re-injection. |
| Treating `/dev/null`→`/etc/ld.so.preload` as a universal default | `src/cli/proot.c:142` (inside `handle_option_q` `:73`) | It fires only with `-q/--qemu`; documenting it as a default for every guest is wrong (§0, §A3). |
| Silent plaintext HTTP | `transport.py:325 insecure_for`, `:341`, `helpers/download.py:289` | The reference only steps down to HTTP behind an explicit `--allow-insecure` and only for a named custom registry; PDN must keep the same explicit gate. |
| Executables under the guest-writable runtime dir | `clear_cache.py:_open_cache_root` docstring (runtime dir is guest-writable), `statedir.py:62 STATE_ROOTS` | On Termux the guest runs as the same uid and can write there. A name PDN *executes* must never live in that set (prior §5). |
| A "checksum" flag the reference claims but lacks | roadmap 10.1 says the reference has "URL install with checksum"; `grep` finds none | Do not match a feature the reference does not have — this is a row where PDN can **beat** it, not match it. |

---

## 10. Axis / gap summary (NEEDS = competitive judgement, inference)

| ID | Mechanism | Axis | Gap | PDN needs for v0.3.0? |
|---|---|---|---|---|
| A1 | string-prefix path translation | S | 10.2 | substrate, no new code |
| A2 | no read-only bind | S/C | 10.2 | **no** |
| A3 | startup bind defaults (`-q` correction) | S | 10.2 | no |
| A4 | 9 extensions, mountinfo default-on | C/S | 10.2 | yes (flags already passed) |
| A5 | mountinfo default | S | 10.2 | parity |
| A6 | fake proc/sys, shm, l2s | C | 10.2/10.5 | yes (verify parity) |
| A7 | Android bind set + `--bind` | M | 10.2 | **yes** |
| B1 | reference grammar + `derive_alias` | M/C | 10.1 | **yes** |
| B2 | registry / local / URL sources | M | 10.1 | **yes** |
| B3 | local OCI layout, digests verified | M | 10.1 | **yes** |
| B4 | `validate_digest` choke point | S | 10.1 | **yes** |
| B5 | multi-arch + emulator | C | 10.1 | **yes** |
| B6 | registry pull/push | M/C | 10.1/10.3 | **yes** |
| B7 | origin-bound creds, no downgrade | S | 10.3 | **yes** |
| B8 | retry policy | S/O | 10.1 | **yes** |
| B9 | `search` (Hub) + curated names | C/M | 10.4 | partly |
| B10 | confined, whiteout-aware extract | S | 10.1 | parity |
| C1 | login/run polygon | M | 10.2 | **yes (the axis)** |
| C2 | argv assembly, `rootfs=.` | M/C | 10.2 | yes |
| C3 | `--get-proot-cmd` | M | 10.2 | yes |
| C4 | image `Env` (+ identity); profile.d = dead end | C/M | 10.2 | yes (image `Env` only) |
| C5 | hostname / work-dir | M | 10.2 | yes |
| C6 | `--user` lookup | C | 10.2 | not required (document) |
| D1 | backup / restore | M/C | 10.3 | has it |
| D2 | OCI-layout export (`build -o`) | M | 10.3 | **yes** |
| D3 | copy / sync | M | 10.3 | has it |
| D4 | reset / rename / remove | M/O | 10.3 | has it |
| E1 | `pd` alias | O | 10.4 | **yes** |
| E2 | bash+fish completions | O | 10.4 | **yes** |
| E3 | per-command help + status | O | 10.4 | yes |
| E4 | command aliases | O | 10.4 | partly |
| F1 | descriptor VFS (`dirfd`) | S | 10.5 | **yes** |
| F2 | runtime dir is untrusted | S | 10.5 | **yes** |
| F3 | lock hint + `LockStateUnknown` | S/O | 10.5 | **yes** |
| F4 | session = fd held open | S | 10.2/10.5 | yes |
| F5 | `ps`/`kill`/`list` shapes | O | 10.4 | parity |
| F6 | ref-counted cache eviction | O | 10.5 | **yes** |
| F7 | env-honouring state paths | O/S | 10.5 | **yes** |
| F8 | nested-proot refusal | S | — | parity |
| F9 | message / quiet semantics | O | — | match semantics |
| F10 | centralised name rule | S | — | parity |
| G1 | content-addressed RUN cache | O/C | 10.5 | yes |
| G2 | subreaper | S/O | 10.5 | **yes** |
| G3 | WORKDIR thin layer | C/S | 10.5 | **yes** |
| G4 | multi-stage + layer differ | C | 10.5 | yes (the spine) |

---

## 11. Unknowns left by this pass (stated plainly)

- `src/syscall/*` and `src/tracee/*` were read at surface level (largest bodies; `enter.c` 3069 lines). The
  per-syscall translation is where guest-behaviour surprises will keep coming from.
- **`src/cli/proot.h` is generated** (`:1 "automatically generated from the documentation"`); the canonical
  option *list* re-verified here is the table at `proot.h:92`, but its `doc/` source was not diffed against it.
- Help-page *rendering* (`commands/help/pages.py`, 1080 lines of content; `render.py`) was not exercised — only
  its structure and command set were read.
- The reference's `--format json`? The reference has **no** JSON output; PDN does. Not a gap, noted so it is not
  mistaken for a missing parity item.
