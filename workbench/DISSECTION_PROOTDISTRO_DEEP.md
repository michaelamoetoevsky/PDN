# Deep dissection: the proot-distro 5.10.3 modules a first pass did not cover

**Status**: second research dossier, 6 Nov 2026. Companion to
`PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md` (hereafter **the first pass**), which read
`dirfd.py`, `paths.py`, `locking.py`, `guestfile.py`, `session.py`, `sysdata.py`,
`helpers/{download,tar_extract,layer_diff,dockerfile,rootfs}.py`,
`helpers/docker/{pull,push,transport,cache,refs}.py`,
`helpers/build_engine/{engine,run_step,copy_step,handlers}.py`,
`commands/{install,install_local,clear_cache,backup,restore}.py` and
`commands/login/{bindings,env,detach,proot_cmd,passwd,migrate}.py` at
docstring/signature level, with the functions PDN would copy read as code.

**This document does not re-derive any of that.** Where it touches a file the first pass
covered (`session.py`, `dirfd.py`, `helpers/docker/refs.py`, `helpers/docker/cache.py`,
`commands/login/__init__.py`), it cites the first pass for the parts already stated and
records only what the second pass newly read.

**Deliverable scope, from the task**: `parser.py`, `message.py`, `statedir.py`,
`compress.py`, `shm.py`, `atomic.py`, `constants.py`, `arch.py`, `names.py`,
`helpers/build_cache.py`, `progress.py`, `commands/{copy,sync,remove,kill,list,run,build}.py`,
`commands/help/{pages,render,__init__}.py`, `completions/*`, plus everything else in the
tree not named in the first pass.

**Honest limit**: every file below was read at **statement level** except where the row
says otherwise. The first-pass files are **not** re-read here; they are named in the
inventory with the marker `first pass`. Anything I could not verify is in section 5.

---

## 1. Method and inventory

### 1.1 Commands actually run

Every conclusion in this document traces to one of these. They are reproduced verbatim.

```sh
# (a) full file list + per-file line counts, cwd = the installed package
cd /data/data/com.termux/files/usr/lib/python3.14/site-packages/proot_distro
find . -type f | sort
find . -name '*.py' -exec wc -l {} + | sort -n
#   -> 77 .py files, 29015 lines total (matches the first pass's 77/29,015)

# (b) the first-pass dossier, to know exactly what not to re-derive
wc -l /data/data/com.termux/files/home/workspace/PDN/research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md
#   -> 391

# (c) the completions, with line counts
cd /data/data/com.termux/files/usr/lib/python3.14/site-packages/proot_distro/completions
wc -l *
#   -> 341 _proot-distro  343 proot-distro.bash  562 proot-distro.fish  = 1246

# (d) dirfd.py public surface (to anchor the descriptor helpers this pass cites)
cd /data/data/com.termux/files/usr/lib/python3.14/site-packages/proot_distro
grep -nE '^(class |def )' dirfd.py | head -80

# (e) the stdout/stderr discipline: who writes to stdout and where
grep -rn "print(" --include=*.py .
grep -rn "sys.stdout" --include=*.py .

# (f) line counts of the first-pass helper files (context for what is *not* re-read)
for f in helpers/dockerfile.py helpers/download.py helpers/tar_extract.py \
         helpers/layer_diff.py helpers/rootfs.py helpers/docker/pull.py \
         helpers/docker/push.py helpers/docker/transport.py \
         helpers/docker/cache.py helpers/docker/refs.py \
         helpers/build_engine/engine.py helpers/build_engine/run_step.py \
         helpers/build_engine/copy_step.py helpers/build_engine/handlers.py; do
  printf '%-42s %s\n' "$f" "$(wc -l < $f)"; done

# (g) how many modules repeat the same trust-boundary doctrine
grep -rln "guest-writable on Termux" --include=*.py . | wc -l   # -> 17 files
grep -rn "guest-writable on Termux" --include=*.py . | wc -l    # -> 25 occurrences

# (h) the tracked help contract this package's help output is compared against
wc -c /data/data/com.termux/files/home/workspace/PDN/research/cli-help-baseline.txt
#   -> 210617
```

Every other fact comes from a `read_file` of the source. No Python was executed against
the package; nothing in it was modified.

### 1.2 Inventory — every file, with line count and how it was read

Total: **77 `.py` files, 29,015 lines** (command (a)), plus **4 shell completion files,
1,246 lines** (command (c)). Blank-marked `read` = read at statement level in this pass.
`first pass` = already documented in the companion dossier; not re-derived here.

| file | lines | this pass |
|---|---|---|
| `__init__.py` | 25 | read (module docstring only) |
| `arch.py` | 315 | read |
| `atomic.py` | 264 | read |
| `cli.py` | 345 | read |
| `commands/__init__.py` | 23 | read (comment only) |
| `commands/backup.py` | 652 | first pass |
| `commands/build.py` | 475 | read |
| `commands/clear_cache.py` | 504 | first pass |
| `commands/copy.py` | 389 | read |
| `commands/help/__init__.py` | 167 | read |
| `commands/help/pages.py` | 1080 | read |
| `commands/help/render.py` | 336 | read |
| `commands/install.py` | 420 | first pass |
| `commands/install_local.py` | 533 | first pass |
| `commands/kill.py` | 495 | read |
| `commands/list.py` | 272 | read |
| `commands/login/__init__.py` | 671 | read |
| `commands/login/bindings.py` | 84 | first pass |
| `commands/login/detach.py` | 179 | first pass |
| `commands/login/env.py` | 320 | first pass |
| `commands/login/migrate.py` | 107 | first pass |
| `commands/login/passwd.py` | 113 | first pass |
| `commands/login/proot_cmd.py` | 311 | first pass |
| `commands/login/quoting.py` | 45 | read |
| `commands/ps.py` | 137 | read |
| `commands/push.py` | 136 | read |
| `commands/remove.py` | 469 | read |
| `commands/rename.py` | 115 | read |
| `commands/reset.py` | 101 | read |
| `commands/restore.py` | 786 | first pass |
| `commands/run.py` | 152 | read |
| `commands/search.py` | 252 | read |
| `commands/sync.py` | 1101 | read |
| `commands/__pycache__/*` | — | not source (compiled) |
| `compress.py` | 176 | read |
| `constants.py` | 123 | read |
| `dirfd.py` | 1361 | first pass (surface re-greped, command (d)) |
| `execenv.py` | 123 | read |
| `guestfile.py` | 402 | first pass |
| `helpers/__init__.py` | 37 | read (comment only) |
| `helpers/build_cache.py` | 314 | read |
| `helpers/build_engine/__init__.py` | 48 | read |
| `helpers/build_engine/constants.py` | 60 | read |
| `helpers/build_engine/copy_step.py` | 1092 | first pass |
| `helpers/build_engine/dockerignore.py` | 84 | read |
| `helpers/build_engine/engine.py` | 726 | first pass |
| `helpers/build_engine/errors.py` | 23 | read |
| `helpers/build_engine/handlers.py` | 390 | first pass |
| `helpers/build_engine/parsing.py` | 155 | read |
| `helpers/build_engine/run_step.py` | 755 | first pass |
| `helpers/build_engine/stage.py` | 119 | read |
| `helpers/build_engine/users.py` | 103 | read |
| `helpers/docker/__init__.py` | 151 | read |
| `helpers/docker/cache.py` | 884 | first pass |
| `helpers/docker/layers.py` | 148 | read |
| `helpers/docker/media.py` | 51 | read |
| `helpers/docker/pull.py` | 708 | first pass |
| `helpers/docker/push.py` | 516 | first pass |
| `helpers/docker/refs.py` | 375 | first pass |
| `helpers/docker/search.py` | 254 | read |
| `helpers/docker/transport.py` | 865 | first pass |
| `helpers/dockerfile.py` | 510 | first pass |
| `helpers/download.py` | 725 | first pass |
| `helpers/layer_diff.py` | 771 | first pass |
| `helpers/oci_writer.py` | 283 | read |
| `helpers/rootfs.py` | 293 | first pass |
| `helpers/tar_extract.py` | 661 | first pass |
| `l2s.py` | 269 | read |
| `locking.py` | 764 | first pass |
| `message.py` | 290 | read |
| `names.py` | 62 | read |
| `parser.py` | 417 | read |
| `paths.py` | 874 | first pass |
| `progress.py` | 146 | read |
| `session.py` | 509 | read (first pass had it at docstring level) |
| `shm.py` | 118 | read |
| `statedir.py` | 265 | read |
| `sysdata.py` | 641 | first pass |
| `completions/_proot-distro` | 341 | read |
| `completions/proot-distro.bash` | 343 | read |
| `completions/proot-distro.fish` | 562 | read |

**Files newly read at statement level this pass: 33 Python modules + 3 completions.**
Everything in the task list was read; the only `.py` file not read this pass and not in the
first pass is `commands/restore.py` (first pass) — i.e. there is no uncovered source file
left except compiled `__pycache__` entries.

---

## 2. Module map, `file:function` anchors

### 2.1 Entry and plumbing

| anchor | what it is |
|---|---|
| `cli.py:246` `main()` | entry point; SIGQUIT→KeyboardInterrupt, nested-proot refusal, proot probe, parse, dispatch |
| `cli.py:64` `_COMMAND_HANDLERS` | canonical name → handler function |
| `cli.py:197` `_dispatch_help()` | renders per-command help *before* argparse runs |
| `cli.py:213` `_reject_unknown_command()` | refuses an unknown subcommand before argparse's own error |
| `cli.py:230` `_split_separator()` | splits login/run argv on a literal `--` |
| `cli.py:90` `_refuse_nested_proot()` | reads `/proc/<pid>/status` `TracerPid` → refuses a nested proot |
| `cli.py:122` `ensure_proot_installed()` | PATH probe + `PD_PROOT_BIN` validation + Termux offer |
| `parser.py:161` `build_parser()` | composes every subparser |
| `parser.py:39` `_PdArgumentParser.error()` | prints `crit_error()` then the command's help page, exit 1 |
| `parser.py:66` `REQUIRED_ARGS`, `parser.py:87` `required_args_for()` | required-positional messages, conditional on `--image` |
| `parser.py:103` `ALIAS_TO_CANONICAL` | alias table resolved before dispatch |
| `parser.py:116` `_add_login_or_run_common()` | the login/run option block |

### 2.2 Message and progress

| anchor | what it is |
|---|---|
| `message.py:47` `_RST`…`_WHITE` + `message.py:59` `_COLORS` | ANSI table; every entry begins with `_RST` |
| `message.py:107` `_init_colors()` | colours iff `stderr.isatty()` and `PD_FORCE_NO_COLORS` unset |
| `message.py:116` `tty_safe_for_writes()` | termios probe: ECHO **and** ICANON set ⇒ safe |
| `message.py:151` `terminal_width()` | stderr (fd 2) then stdout (fd 1) |
| `message.py:168` `_quiet` / `:171` `set_quiet()` / `:182` `is_quiet()` | process-global quiet flag |
| `message.py:187` `msg()` | erases the progress line (`\r\033[K`) then writes to **stderr** |
| `message.py:210` `log_info()` / `:217` `log_error()` / `:222` `warn()` / `:227` `crit_error()` | the four log helpers |
| `message.py:242` `quote_path()` / `:279` `quote_error()` | control-char/surrogate escaping |
| `progress.py:40` `REDRAW_THRESHOLD_BYTES` | 262144 |
| `progress.py:54` `ByteCounter` | read()/readinto() tallying wrapper |
| `progress.py:80` `progress_active()` / `:85` `draw_bytes_bar()` / `:118` `draw_count_bar()` / `:141` `clear_bar()` | the bar primitives |

### 2.3 Crash-safe writes and the state tree

| anchor | what it is |
|---|---|
| `atomic.py:92` `atomic_replace()` | yield fd on a sibling tmp; `os.replace` on success |
| `atomic.py:130` `atomic_write()` | `atomic_replace` wrapped in a file object |
| `atomic.py:145` `publish_file()` | rename an already-written file into the state tree |
| `atomic.py:187` `_replace_at()` | O_EXCL temp off the validated dir fd |
| `atomic.py:210` `_publish_at()` | lstat-vs-descriptor identity check before rename; `ESTALE` otherwise |
| `atomic.py:243` `_replace_by_name()` | plain branch for a user-named destination (`build -o`) |
| `statedir.py:62` `STATE_ROOTS` | sorted `{RUNTIME_DIR, BASE_CACHE_DIR}` by length, shortest first |
| `statedir.py:82` `read_state_file()` | capped read; `EFBIG` past 16 MiB |
| `statedir.py:114` `split_state_path()` | `normpath` then root split; `..` cannot survive |
| `statedir.py:156` `open_state_dir()` | per-component O_NOFOLLOW walk, `create=` makes levels with `mkdirat` |
| `statedir.py:218` `open_state_parent()` | `(dir_fd, leaf)` for the entry itself |
| `statedir.py:232` `remove_state_tree()` | `rmtree_at` reached through the walk |
| `compress.py:62` `ZSTD_AVAILABLE` / `:123` `require_read_support()` / `:158` `open_tar_writer()` | zstd detection and the single write path |
| `shm.py:72` `shm_dir()` / `:77` `make_shm_dir()` / `:100` `make_guest_tmp()` | the `/dev/shm` store as a *sibling* of the rootfs |
| `names.py:40` `_NAME_RE` / `:48` `is_valid_name()` / `:53` `require_valid_name()` | one container-name rule |

### 2.4 Session, kill, ps

| anchor | what it is |
|---|---|
| `session.py:115` `register_session()` | writes `<pid>.json`, clears O_CLOEXEC, holds `LOCK_EX` |
| `session.py:210` `active_sessions()` | validates + prunes dead entries |
| `session.py:257` `_record_pid()` / `:277` `_validate_record()` | name-is-the-tie; shape checks |
| `session.py:376` `session_is_live()` / `:392` `session_holders()` / `:456` `_session_alive_at()` | the shared non-blocking flock probe and the `/proc` inode scan |
| `commands/ps.py:50` `command_ps()` | six-column table, `--quiet` = PIDs on stdout |
| `commands/kill.py:88` `command_kill()` | target resolution + teardown |
| `commands/kill.py:73` `_NON_TERMINATING_SIGNALS` / `:224` `_teardown()` / `:252` `_escalate()` | TERM → QUIT → KILL escalation |
| `commands/kill.py:359` `_read_pid_ppid()` / `:386` `_collect_tree()` / `:411` `_is_alive()` | process-tree walk and zombie test |

### 2.5 copy, sync, remove, reset, rename

| anchor | what it is |
|---|---|
| `commands/copy.py:63` `command_copy()` / `:82` `_copy_tree_pinned()` / `:183` `_move_pinned()` / `:242` `_do_copy()` | the copy/move machinery |
| `commands/sync.py:881` `command_sync()` / `:895` `_do_sync()` | entry and pre-flight |
| `commands/sync.py:89` `_Ctx` | per-transfer state (src_rels, skipped_rels, root_unreadable) |
| `commands/sync.py:190` `_needs_update()` / `:330` `_refresh_file_metadata()` / `:380` `_sync_file()` | comparison and per-file write |
| `commands/sync.py:470` `_record_level()` / `:506` `_collect_rels()` | pass 1 |
| `commands/sync.py:554` `_mirror_entries()` / `:663` `_mirror_at()` | pass 2 |
| `commands/sync.py:746` `_collect_extras_at()` / `:820` `_remove_extras_at()` / `:1084` `_prune()` | pass 3 (`--delete`) |
| `commands/remove.py:108` `command_remove()` / `:126` `_remove_container()` / `:174` `_remove_image()` | two deletion targets |
| `commands/remove.py:318` `_unlink_targets()` | manifest entries + refcounted orphan layers |
| `commands/rename.py:50` `command_rename()` / `:85` `_do_rename()` | rename + l2s rewrite |
| `commands/reset.py:44` `command_reset()` | wipe rootfs + `shm/`, reinstall from the stored manifest |

### 2.6 Build and its outputs

| anchor | what it is |
|---|---|
| `commands/build.py:93` `command_build()` | orchestration |
| `commands/build.py:75` `_read_dockerfile()` | `-` = stdin, read as bytes, `MAX+1` bytes |
| `commands/build.py:363` `_make_build_tmp()` / `:408` `_fallback_build_tmp()` | scratch root off the state walk |
| `commands/build.py:422` `_parse_build_args()` / `:434` `_derive_tag_from_path()` / `:464` `_install_as_container()` | helpers |
| `helpers/build_cache.py:288` `compute_recipe_hash()` / `:241` `record()` / `:157` `lookup()` / `:165` `recorded_layer_digests()` / `:204` `discard_index()` | the recipe-hash index |
| `helpers/build_engine/stage.py:50` `Stage` | per-FROM state, owns two descriptors |
| `helpers/build_engine/constants.py:50` `needs_proot()` | is any RUN (incl. `ONBUILD RUN`) present |
| `helpers/build_engine/parsing.py:43` `split_operands()` / `:67` `parse_chmod()` / `:91` `parse_kv_list()` / `:137` `is_tar_header()` | stateless parsers |
| `helpers/build_engine/users.py:35` `resolve_id()` / `:72` `resolve_chown()` / `:88` `resolve_user_for_proot()` | rootfs-relative uid/gid |
| `helpers/build_engine/dockerignore.py:32` `load_dockerignore()` / `:48` `is_ignored()` | pattern matcher |
| `helpers/oci_writer.py:57` `build_manifest_and_config()` / `:112` `store_in_cache()` / `:155` `write_oci_archive()` | the two output variants |
| `helpers/docker/media.py:30` `canonical_json()` / `:41`–`:50` media types | the one JSON canon + media strings |
| `helpers/docker/layers.py:44` `download_blob()` / `:126` `apply_layer()` | blob fetch + apply |

### 2.7 Help and completions

| anchor | what it is |
|---|---|
| `commands/help/pages.py:43` `HELP_PAGES` | all page content as plain dicts |
| `commands/help/pages.py:1061` `TOP_COMMANDS` | the no-args command table (3-tuples with a warning) |
| `commands/help/render.py:290` `render_page()` | renders one page dict |
| `commands/help/render.py:50` `term_width()` / `:42` `NARROW_BREAKPOINT` | width clamp 32..92, breakpoint 60 |
| `commands/help/render.py:142` `options_block()` / `:175` `commands_block()` / `:240` `bullets_block()` / `:223` `shell_block()` | layout primitives |
| `commands/help/__init__.py:41` `_make_help_fn()` / `:49` `HELP_COMMANDS` / `:52` `command_help()` | dispatcher |
| `completions/proot-distro.bash:37` `_proot_distro()` | bash completion; `:21` `_proot_distro_get_images` shells out to `list --image --quiet` |
| `completions/_proot-distro:59` `_proot_distro()` | zsh; `:37` `_proot_distro_images` same shell-out |
| `completions/proot-distro.fish:40` `__proot_distro_images` | fish; same shell-out |

---

## 3. The mechanisms worth taking, each with the code that proves it

### 3.1 Prose on stderr, machine-readable identifiers on stdout

The whole program writes its human output through `msg()` → stderr. `stdout` is reserved
for the four things a script pipes: `list --quiet`, `list --image --quiet`,
`ps --quiet`, `search --quiet`, `login --get-proot-cmd`, and a `backup` archive stream.

**Code**, `message.py:187` — the progress line is erased before every message, but only
on a TTY so a redirected log gets no control characters:

```python
def msg(*args):
    if not tty_safe_for_writes():
        return
    try:
        is_tty = sys.stderr.isatty()
    except (AttributeError, OSError, ValueError):
        is_tty = False
    if is_tty:
        sys.stderr.write("\r\033[K")
        sys.stderr.flush()
    print(*args, file=sys.stderr)
```

**Evidence** (command (e)): the only bare `print()` calls in the whole package are
`message.py:207` (`file=sys.stderr`), `commands/list.py:83,121`, `commands/ps.py:57`,
`commands/search.py:99`; the only `sys.stdout` uses are `commands/backup.py:613,618`
(the archive stream) and `commands/login/__init__.py:624-626` (`--get-proot-cmd`).

**Rule for PDN**: PDN already separates logs from data by convention; the reference makes
it a rule with exactly six exceptions, each a deliberate pipeable result.

### 3.2 `tty_safe_for_writes()` — do not draw on a TTY another program owns

The check is termios state, not a program name, so it covers pinentry, `sudo`,
`ssh-askpass`, `less` and a curses UI alike.

**Code**, `message.py:143`:

```python
    try:
        attrs = termios.tcgetattr(fd)
    except (OSError, termios.error):
        return True
    lflag = attrs[3]
    return bool(lflag & termios.ECHO) and bool(lflag & termios.ICANON)
```

Non-TTY stderr returns True (plain redirection unaffected) — `message.py:139`.

**Rule for PDN**: the exact situation the docstring names — `pd backup | gpg -c` — is one
PDN's progress bars would corrupt today for the same reason.

### 3.3 `quote_path()` — untrusted names are the guest's to choose

Every name read off a rootfs, archive, manifest or registry reply passes through
`quote_path` before printing. Control characters below `0x20`, DEL, backslash and lone
surrogates are escaped; the escape is applied to the *name only*, never the whole message,
because the colour codes are control characters by definition (`message.py:261`).

**Code**, `message.py:266`:

```python
    out = []
    for ch in text:
        escaped = _QUOTE_MAP.get(ch)
        if escaped is not None:
            out.append(escaped)
        elif ch < " " or ch == "\x7f":
            out.append(f"\\x{ord(ch):02x}")
        elif "\ud800" <= ch <= "\udfff":
            out.append(f"\\u{ord(ch):04x}")
        else:
            out.append(ch)
    return "".join(out)
```

**Callers** (grep-visible in the modules read): `sync.py:149` `_Ctx.shown`,
`copy.py:134` `shown`, `list.py:121,142-145` (image ref/arch/id),
`remove.py:449,457` (image_ref, arch), `ps.py:76,134` (user, command), `login/__init__.py`
(`_print_command`). The lone-surrogate branch is specifically for a byte that is not
UTF-8 arriving out of argv/env/JSON where strict stdout would `UnicodeEncodeError`.

### 3.4 `atomic.py` — the descriptor, not the name

The whole module exists because a name can be swapped between the write and the rename.

**Code**, `atomic.py:92` docstring + body:

```python
@contextlib.contextmanager
def atomic_replace(path: str, *, suffix: str = ".tmp"):
    root, parts = _state_location(path)
    if root is None:
        yield from _replace_by_name(path, suffix)
        return
    dir_fd = statedir.open_state_dir(
        os.path.join(root, *parts[:-1]), create=True,
    )
    try:
        yield from _replace_at(dir_fd, parts[-1], suffix)
    finally:
        os.close(dir_fd)
```

and the check that closes the last window, `atomic.py:222`:

```python
    if (entry is None
            or (entry.st_dev, entry.st_ino)
            != (written.st_dev, written.st_ino)):
        raise OSError(
            errno.ESTALE,
            "the temporary file was replaced while it was being written",
            tmp_name,
        )
    os.replace(tmp_name, name, src_dir_fd=dir_fd, dst_dir_fd=dir_fd)
```

**Why it matters**: on Termux `RUNTIME_DIR` and `BASE_CACHE_DIR` both sit under the
`$TERMUX_PREFIX` bound read-write into every non-isolated container (`atomic.py:60-62`), so
a guest *can* read the unpredictable tmp name out of `readdir()`, unlink it and plant a
symlink. The descriptor settles the inode; the identity check shortens the rename's
exposure from "the whole download" to "the instant between lstat and rename".

### 3.5 `statedir.py` — one O_NOFOLLOW walk for every state directory

The trust roots are named once; everything **below** them is walked component by
component. `split_state_path` normalises first so a `..` leaves the result outside the
tree and returns `(None, None)` rather than a walk opening `..` off a validated fd
(`statedir.py:123-127`).

**Code**, `statedir.py:185`:

```python
        for depth, part in enumerate(parts, 1):
            try:
                nxt = dirfd.opendir_at(fd, part)
            except FileNotFoundError:
                if not create:
                    raise
                try:
                    os.mkdir(part, 0o777, dir_fd=fd)
                except FileExistsError:
                    pass            # lost a race with another writer
                try:
                    nxt = dirfd.opendir_at(fd, part)
                except OSError as exc:
                    if dirfd.is_refusal(exc):
                        raise _refuse(
                            os.path.join(root, *parts[:depth])
                        ) from None
                    raise
```

**Honest scope note the reference itself states** (`statedir.py:44-50`): this closes the
*persistent* case (the link is already there when the command starts) but not a name
re-pointed *after* a validated descriptor was handed out — a caller that goes back to
composing paths (an extractor, proot resolving a bind source) gets only the persistent
guarantee. That is exactly the caveat PDN's shim dossier reaches from the other side.

### 3.6 `read_state_file()` — the cap is on bytes drawn, not a declared size

**Code**, `statedir.py:98`:

```python
    chunks = []
    remaining = limit + 1
    while remaining > 0:
        chunk = os.read(fd, min(remaining, _READ_CHUNK))
        if not chunk:
            break
        chunks.append(chunk)
        remaining -= len(chunk)
    data = b"".join(chunks)
    if len(data) > limit:
        raise OSError(
            errno.EFBIG,
            f"state file is larger than {limit} bytes; refusing to read it",
        )
    return data
```

`MAX_STATE_JSON_BYTES = 16 * 1024 * 1024` (`statedir.py:77`). The docstring's reason:
*"the size a file reports is not a promise about how much can be read out of it"* — a file
being appended to while read would otherwise pass an `fstat` check and then exceed it.

### 3.7 `session.py` — liveness is an inherited flock, not a PID

`register_session()` clears O_CLOEXEC (`session.py:174`) so proot **and every guest
process it spawns** inherit the descriptor across `os.execvpe`; the kernel drops the lock
when the last holder exits, even on `kill -9`. Liveness is then a *shared, non-blocking*
flock probe.

**Code**, `session.py:456`:

```python
    try:
        fd, _st = dirfd.open_regular_at(dir_fd, name, os.O_RDONLY)
    except OSError:
        return False
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
        except OSError:
            # EACCES/EAGAIN means the session's exclusive lock is held;
            return True
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
```

and the stronger identifier — scanning `/proc/*/fd` for the registry file's inode
(`session.py:440-452`), which finds guests after the root proot was `SIGKILL`ed and they
were reparented to init.

**The forged-record defence** (the module's own docstring, `session.py:64-72`): the file
name is the only thing tying a record to a process, so `_record_pid` accepts only the
canonical decimal `<pid>.json` (`session.py:269-273`) and `_validate_record` requires the
recorded `pid` to equal the name's (`session.py:299`). A record whose `pid` disagrees is
not read *and not tracked* — the stale file is pruned if unheld (`session.py:244-246`).

### 3.8 `kill.py` — escalation built on proot's own signal semantics

Two facts from proot's sources are stated as the design premise (`kill.py:25-40`): proot's
event loop ignores every signal but QUIT/ILL/ABRT/FPE/SEGV, USR1/USR2 and the job-control
set; and it sets no `PTRACE_O_EXITKILL`, so `kill -9 <proot>` leaves the guest running.

**Code**, `kill.py:236`:

```python
    if _wait_for_drain(tracked):
        return delivered
    # Still up. proot ignores SIGTERM/SIGINT/SIGHUP outright, so a
    # graceful request never reached the container root; SIGQUIT is the
    # signal its event loop acts on, and its handler SIGKILLs every
    # tracee before the loop leaves through ECHILD.
    delivered += _escalate(_live_roots(tracked))
    if _wait_for_drain(tracked):
        return delivered
    return delivered + _signal_pass(_live_roots(tracked), signal.SIGKILL)
```

and the zombie-aware liveness test, `kill.py:418`:

```python
    try:
        with open(f"/proc/{pid}/stat") as fh:
            data = fh.read()
    except OSError:
        return False
    try:
        return data.rsplit(")", 1)[1].split()[0] != "Z"
    except IndexError:
        return False
```

`_NON_TERMINATING_SIGNALS` (`kill.py:73`) is delivered exactly as asked and *never*
escalated, so `kill -s STOP` is not turned into a teardown.

### 3.9 `sync.py` — three fd-walked passes, and `--delete` that declines

The load-bearing details, each a defence against a specific mistake:

- **`--delete` is disabled when the source root could not be listed** (`sync.py:1093`):
  `ctx.root_unreadable` exists precisely because `src_rels` would then be empty and *every*
  destination entry would look like an orphan. Stated as *"rsync disables --delete on an I/O
  error for the same reason"*.
- **`src_rels` is filled by both passes** (`sync.py:565-568`): an entry created between the
  counting pass and the mirror pass would otherwise be written and then pruned as an orphan.
- **A hardlinked destination is rewritten, not `fchmod`ed** (`sync.py:367`):
  ```python
      if stat.S_IMODE(dst_st.st_mode) == want and not stale:
          return _META_OK
      if dst_st.st_nlink != 1:
          return _META_REWRITE
  ```
  The docstring: *"nothing distinguishes one a guest made to a host file from an ordinary
  rootfs entry (same uid, same filesystem, no race needed)"*.
- **Each file is written to a sibling temp and renamed** (`sync.py:438-447`), O_EXCL, temp
  removed on `BaseException` including Ctrl-C (`sync.py:454-460` — *"this only ever caught
  OSError"*).
- **Directory metadata is applied on the way back up** (`sync.py:685`), so a read-only
  source directory stays writable while its contents are written, and its mtime is set
  after the contents finish bumping it.
- **A source entry the mirror skipped is added to `skipped_rels`** (`sync.py:607`), which
  keeps `--delete` off the destination standing in its place.

The walks are explicit fd stacks (`sync.py:520` frame layout `[fd, None, rel, pending, owned]`)
plus `dirfd.Levels`, which bounds the number of open descriptors regardless of depth
(`sync.py:68`).

### 3.10 `copy.py` — `cp -a` merge and `mv` rename semantics, on pinned fds

**Code**, `copy.py:314` — the destination question is asked with *container* semantics:

```python
    dest_target = resolve_container_path(dest) if move_mode else dest_path
    if os.path.isdir(dest_target):
        dest_path = resolve_container_child(dest, dest_target,
                                            os.path.basename(src_path),
                                            deref_leaf=not move_mode)
```

`_move_pinned` (`copy.py:198`) tries `os.rename(..., src_dir_fd=, dst_dir_fd=)` first and
falls back to copy+remove only on `EXDEV`; on that path nothing is removed when the copy
half skipped or failed an entry (*"a move whose copy half skipped an entry would otherwise
delete the one copy of it"*, `copy.py:190-196`). `merge=True` is passed for a directory
copy (`copy.py:357`) so a repeated `copy` updates in place, while `--move`'s fallback
leaves it off so a populated destination is refused as `rename(2)` would.

### 3.11 `remove.py` — image deletion is reference-counted

**Code**, `remove.py:340`:

```python
    # Layers are shared between images, so a blob may only go once no
    # surviving manifest lists it. The inventory is re-read here, after
    # the deletions above, so it reflects exactly what is left.
    keep = set()
    for record in iter_cached_images():
        keep.update(layer["digest"] for layer in record["layers"])
```

`_resolve_images` (`remove.py:203`) resolves a reference through the cache key first and
only falls back to an ID prefix (min 4 chars, `remove.py:72`), refusing an ambiguous prefix
outright (`remove.py:235`). Manifest and layer caches are opened as descriptors and every
entry is unlinked as `(dir_fd, name)` (`remove.py:327,360`).

### 3.12 `build.py` — the proot gate is deferred until the Dockerfile is parsed

`build` and `push` are exempt from the CLI's startup proot probe (`cli.py:192`); `build`
runs its own check *after* parsing, only when a RUN is actually present.

**Code**, `build.py:170`:

```python
    if needs_proot(instructions):
        from proot_distro.cli import ensure_proot_installed
        ensure_proot_installed()
```

`needs_proot` also looks inside `ONBUILD RUN`
(`helpers/build_engine/constants.py:56-59`). A COPY/metadata-only Dockerfile therefore
builds with no proot on the host at all.

**The scratch root is created off the state walk** (`build.py:391-400`): `statedir.open_state_dir`
then `os.mkdir(name, 0o700, dir_fd=dir_fd)` then `dirfd.opendir_at(dir_fd, name)`, with
`_fallback_build_tmp()` (`build.py:408`) to `tempfile.mkdtemp` only when the state tree
refuses. One exclusive `BuildLock` per tag, sorted by lock path (`build.py:234`), for the
duration of the build.

### 3.13 Help is data; argparse never renders it

`parser.py` disables argparse's own help everywhere (`add_help=False`, stated at
`parser.py:23-25`) and intercepts before argparse:

**Code**, `cli.py:204`:

```python
    if len(raw_args) < 2 or raw_args[1] not in ("-h", "--help", "--usage"):
        return False
    cmd = ALIAS_TO_CANONICAL.get(raw_args[0], raw_args[0])
    if cmd in HELP_COMMANDS:
        HELP_COMMANDS[cmd]()
        return True
    return False
```

and the parser's own error path renders the right page (`parser.py:49`):

```python
    def error(self, message: str) -> None:
        from proot_distro.commands.help import HELP_COMMANDS
        msg()
        crit_error(message)
        if self._pd_command and self._pd_command in HELP_COMMANDS:
            HELP_COMMANDS[self._pd_command]()
        msg()
        sys.exit(1)
```

`HELP_COMMANDS` is built mechanically from `HELP_PAGES` (`commands/help/__init__.py:41-49`),
so there is exactly one place a command's help content lives and one place it is rendered.
Termux-only options are gated in the page data with `*([...] if IS_TERMUX else [])`
(`pages.py:23`, e.g. `:623`, `:650-658`) so help cannot drift from what argparse accepts.

### 3.14 Completions read the CLI's own output rather than re-deriving it

Both the bash and zsh completions answer "what images exist" by *calling the program*:

**Code**, `completions/proot-distro.bash:21`:

```sh
# Cached OCI images, as `remove --image` accepts them. The manifest cache
# is keyed by a hash, so the references are read back through the program
# itself rather than derived from file names.
_proot_distro_get_images() {
    proot-distro list --image --quiet 2>/dev/null
}
```

Container names, by contrast, are derived from the directory layout directly
(`completions/proot-distro.bash:10-19` — `${TERMUX__PREFIX:-…}/var/lib/proot-distro/containers/*/rootfs`).
Termux detection is duplicated into shell with the same "two of three indicators" rule as
`constants.py:55` (`completions/proot-distro.bash:53-61`, mirrored in fish `:48-66` and zsh
`:47-54`).

### 3.15 `compress.py` — one zstd write path so `-o` and `>` agree

**Code**, `compress.py:169`:

```python
    with ExitStack() as stack:
        if output_path is not None:
            fileobj = stack.enter_context(open(output_path, "wb"))
        # Closing order is the reverse of entry: the tar's end-of-archive
        # blocks are written first, then the zstd frame is finished, then
        # the file is closed. ZstdFile does not close what it wraps.
        zfh = stack.enter_context(_zstd.ZstdFile(fileobj, "w", level=level))
        yield stack.enter_context(tarfile.open(fileobj=zfh, mode="w|"))
```

The reason is stated at `compress.py:38-45`: `tarfile.open(mode='w|zst')` rejects a level
outright, so `backup -o out.tar.zst` and `backup > out.tar.zst` would otherwise compress at
different levels. `ZSTD_LEVEL = 10` with a measured justification in the comment
(`compress.py:67-74`: 22 MiB in 3.8 s vs gzip-9's 27 MiB in 23 s on 101 MiB).

### 3.16 `shm.py` — `/dev/shm` is a sibling of the rootfs, not a name inside it

**Code**, `shm.py:72`:

```python
def shm_dir(rootfs: str) -> str:
    """Path of the shm store belonging to *rootfs*."""
    return os.path.join(os.path.dirname(rootfs), SHM_DIR_NAME)
```

The docstring's argument (`shm.py:26-47`): `/dev/shm` used to be the rootfs's own `/tmp`,
and *a bind source is a name, resolved by proot after this program has finished checking
it*; a guest that flips `/tmp` from a directory to a symlink between the check and the exec
would have the next session mount a host directory read-write inside the container — which
under `--isolated` is the whole of that mode's promise undone. Making it a sibling means
swapping it requires writing to its *parent*, which no session confined to the rootfs can
reach. The guest's own `/tmp` is still created (`make_guest_tmp`, `shm.py:100`), just no
longer the same directory.

### 3.17 `execenv.py` — an image may not set the host-side exec's variables

**Code**, `execenv.py:60` + `:84`:

```python
_HOST_EXEC_PREFIXES = ("LD_", "PROOT_")

def is_host_exec_var(key: str) -> bool:
    return key.startswith(_HOST_EXEC_PREFIXES)
```

The rule is about *provenance*, not the name (`execenv.py:45-58`): a value the invoking
user set (`PROOT_NO_SECCOMP=1 pd login debian`, a `--env` flag) keeps working; a value that
came out of a file describing an *image* is refused, because it is read before proot has
confined anything and the rootfs sits at a path the image chose. `is_exportable`
(`execenv.py:96`) additionally rejects a NUL, a non-filesystem-encodable code point, and a
string at or past `MAX_ENV_STRING_BYTES = 32 * 4096` (E2BIG, which would fail the *whole*
exec, `execenv.py:75-81`).

### 3.18 `l2s.py` — resolving a stand-in and reading it are two steps

**Code**, `l2s.py:109`:

```python
    real_target = os.path.realpath(abs_target)
    rootfs_real = os.path.realpath(rootfs)
    if real_target != rootfs_real and not real_target.startswith(
        rootfs_real + os.sep
    ):
        return None
    return real_target
```

The docstring supplies the reaching example: `innocent.txt -> .proot.l2s.evil0001` and
`.proot.l2s.evil0001 -> /absolute/host/path` was enough for `backup` to pack host bytes
into an archive and the build engine to pack them into a layer `push` uploads
(`l2s.py:80-91`). The whole chain is `realpath`ed so containment is decided on the file
that holds the *content*, not the first hop. The read that follows re-walks with O_NOFOLLOW
from a rootfs descriptor (`open_l2s_backing`, `l2s.py:116-152`). The rewrite walk intercepts
SIGINT/SIGQUIT for its duration (`l2s.py:191-196`): aborting mid-walk leaves dangling
symlinks.

### 3.19 `build_cache.py` — the index is a flocked JSON file; a read that fails is not "empty"

**Code**, `build_cache.py:165` + `:185`:

```python
def recorded_layer_digests():
    try:
        data = json.loads(_read_index())
    except FileNotFoundError:
        return set(), True
    except (OSError, ValueError):
        return set(), False
```

The `readable` flag exists because the caller (the layer sweep) must not read "could not
parse" as "the index pins nothing" — that would collect every blob the real index holds
(`build_cache.py:172-179`). `compute_recipe_hash` (`build_cache.py:288`) hashes
parent-digest, instruction name, canonical flags, value and heredoc bodies, all through
`_hash_bytes` which encodes with `surrogatepass` so a non-UTF-8 `--build-arg` does not raise
out of a cache lookup (`build_cache.py:274-285`).

### 3.20 `oci_writer.py` — a Docker-legacy `manifest.json` so `docker load` works

**Code**, `oci_writer.py:248`:

```python
    entry = {
        "Config": f"blobs/sha256/{config_digest_hex}",
        "RepoTags": [image_ref] if image_ref else [],
        "Layers": layer_paths,
    }
    if layer_sources:
        entry["LayerSources"] = layer_sources
    return [entry]
```

The docstring (`oci_writer.py:162-165`): *"docker load, which falls back to a buggy
'per-directory legacy import' loop when only the OCI layout is present."* The layer bytes
are read through `open_required_layer` and packed from that descriptor so the bytes packed
are the bytes that were hashed (`oci_writer.py:218-227`).

### 3.21 `cli.py` — three argv refinements worth copying verbatim

- **Unknown subcommand is refused before argparse**, `cli.py:277-281`: *"An unknown
  subcommand name causes _SubParsersAction to raise ArgumentError, which routes through
  self.error() — printing argparse's own message before our custom error handler runs."*
- **`--` is re-parsed for login/run**, `cli.py:306-314`:
  ```python
      check_unknown = unknown
      if canonical in ("login", "run") and "--" in raw_args:
          sep_idx = raw_args.index("--")
          _, check_unknown = parser.parse_known_args(raw_args[:sep_idx])
  ```
  so an inner command's flags are not reported as unknown.
- **`set_quiet` is withheld from `list` and `ps`**, `cli.py:337`: those two have different
  `--quiet` semantics (bare identifiers on *stdout*) and do not use `log_info()`;
  `search` keeps the flag because it means both.

### 3.22 `commands/list.py`, `commands/ps.py`, `commands/search.py` — column layout with a stacked fallback

Three commands share the same shape: content-sized fixed columns, one flexible column
taking the terminal remainder, and a stacked form when the terminal is too narrow.
`list.py:159` switches on `sum(widths) + _GAP * (len(widths) - 1) <= terminal_width()`;
`ps.py:90` gives COMMAND `terminal_width() - used` and truncates with `...`;
`search.py:165-170` gives DESCRIPTION the remainder and stacks when it would be under
`_MIN_DESC = 20`. All cell text that comes off disk passes through `quote_path` first
(`list.py:141-145`, `ps.py:76`, `search.py` leaves it to the helper by design —
`commands/search.py:34-35`).

---

## 4. Claim table

Every row: claim → the file:line, grep result or command output that supports it.

| # | claim | evidence |
|---|---|---|
| 1 | package is 77 `.py` files / 29,015 lines + 4 completions / 1,246 lines | command (a): `wc -l` total `29015`; command (c): `341+343+562=1246` |
| 2 | first-pass dossier is 391 lines and its scope is as listed | command (b): `391`; its section header "Method, stated exactly" |
| 3 | every human-facing message goes to stderr | `message.py:207` `print(*args, file=sys.stderr)`; command (e) shows no other `print(` outside 4 pipeable-result sites |
| 4 | only stdout users are the 4 quiet/identifier paths, the archive stream, and `--get-proot-cmd` | command (e): `list.py:83,121`, `ps.py:57`, `search.py:99`, `backup.py:613,618`, `login/__init__.py:625` |
| 5 | `msg()` erases the progress line only on a TTY | `message.py:201-206` |
| 6 | progress is suppressed on a TTY another program owns | `message.py:143-148`; `progress.py:102,126` call `tty_safe_for_writes()` |
| 7 | colours off unless stderr is a TTY and `PD_FORCE_NO_COLORS` unset | `message.py:108` |
| 8 | untrusted names are escaped incl. lone surrogates | `message.py:266-276`; callers `sync.py:149`, `copy.py:134`, `ps.py:134`, `list.py:121`, `remove.py:449` |
| 9 | `atomic_replace` reaches a state-tree destination by descriptor | `atomic.py:121-127` |
| 10 | the rename is refused if the temp's inode changed | `atomic.py:232-239` (`ESTALE`) |
| 11 | the temp is O_EXCL and readable | `atomic.py:196` `dirfd.open_new_at(dir_fd, tmp_name, 0o600, readable=True)` |
| 12 | a non-state destination keeps plain `mkstemp`+`replace` | `atomic.py:243-256` |
| 13 | state roots are sorted shortest-first | `statedir.py:62` |
| 14 | a `..` cannot survive path split | `statedir.py:123-127` |
| 15 | state dirs are walked O_NOFOLLOW, created with `mkdirat` | `statedir.py:185-201` |
| 16 | the walk closes over the persistent case only | `statedir.py:44-50` (the module's own statement) |
| 17 | state-read cap is on bytes drawn | `statedir.py:98-110`; limit `statedir.py:77` |
| 18 | session liveness is an inherited flock | `session.py:174` (`set_inheritable`), `session.py:182` (`LOCK_EX`), `session.py:472` (`LOCK_SH|LOCK_NB` probe) |
| 19 | session holders are found by `/proc/*/fd` inode scan | `session.py:425-452` |
| 20 | only canonical `<pid>.json` is a record number | `session.py:269-273` |
| 21 | a record whose `pid` disagrees with its name is not read | `session.py:299-300` |
| 22 | `kill` escalates TERM→QUIT→KILL | `kill.py:236-249` |
| 23 | job-control/USR signals are never escalated | `kill.py:73-78`, `kill.py:233-234` |
| 24 | liveness test treats a zombie as dead | `kill.py:418-424` |
| 25 | `--delete` is disabled when the source root is unreadable | `sync.py:1093-1096`; flag set `sync.py:482` |
| 26 | `src_rels` is filled by both passes | `sync.py:565-568`, `sync.py:130-131` |
| 27 | a multi-linked destination is rewritten, not `fchmod`ed | `sync.py:367-368` |
| 28 | `--move` removes nothing when the copy was incomplete | `copy.py:229-233`; `_sync_file` skip path `sync.py:421-424` |
| 29 | image deletion keeps layers another manifest lists | `remove.py:340-342` |
| 30 | an image ID prefix under 4 chars is not matched | `remove.py:72`, `remove.py:230-231` |
| 31 | `build` requires proot only when a RUN is present | `build.py:170-172`; `helpers/build_engine/constants.py:50-59` |
| 32 | the build scratch root is made off the state walk | `build.py:391-400` |
| 33 | the tag becomes a `BuildLock`, sorted by lock path | `build.py:234-237` |
| 34 | help is rendered before argparse, from `HELP_COMMANDS` | `cli.py:204-209`; `commands/help/__init__.py:49` |
| 35 | an argparse error prints the command's own help page | `parser.py:55-60` |
| 36 | `add_help=False` everywhere, help handled manually | `parser.py:22-25`, `parser.py:166` |
| 37 | Termux-only options are gated in the page data | `pages.py:623,627,650-658`; matching gate in argparse `parser.py:127,143` |
| 38 | help width is clamped to 32..92, stacked below 60 | `commands/help/render.py:39-43` |
| 39 | completions obtain image refs by running the program | `completions/proot-distro.bash:21-23`, `_proot-distro:37-41`, `proot-distro.fish:40-42` |
| 40 | termux detection is duplicated into all three shells | `completions/proot-distro.bash:53-61`, `_proot-distro:47-54`, `proot-distro.fish:48-66`; source of truth `constants.py:55-68` |
| 41 | zstd write goes through one `w|` path at one level | `compress.py:169-176`; reason `compress.py:38-45`; level `compress.py:75` |
| 42 | `/dev/shm` bind source is a sibling of the rootfs | `shm.py:72-74`; argument `shm.py:26-47` |
| 43 | an image's `LD_*`/`PROOT_*` cannot reach the host exec | `execenv.py:60,84,93` |
| 44 | l2s containment is decided on the fully resolved chain | `l2s.py:107-113` |
| 45 | `build_cache` distinguishes "index unreadable" from "index pins nothing" | `helpers/build_cache.py:182-186` |
| 46 | recipe hash is surrogate-safe | `helpers/build_cache.py:274-285` |
| 47 | the OCI archive carries a Docker-legacy `manifest.json` | `helpers/oci_writer.py:198-200, 231-255` |
| 48 | the argv `--` separator is re-parsed for login/run | `cli.py:306-309`; split `cli.py:230-243` |
| 49 | `set_quiet` is withheld from `list`/`ps` | `cli.py:337` |
| 50 | the same "guest-writable on Termux" doctrine is stated in 17 files / 25 places | command (g) |
| 51 | PDN has a tracked help contract of 210,617 bytes | command (h) |
| 52 | every module in the task list was read | section 1.2 (only `commands/restore.py` is first-pass-only) |

---

## 5. Open questions and what could NOT be verified

1. **`commands/restore.py` (786) was not re-read.** The first pass covers it; this pass only
   confirms its line count. If `restore` interacts with `statedir.remove_state_tree` or the
   session registry in a way the first pass did not record, that is unverified here.

2. **No behaviour was executed.** Nothing in the package was run and no container touched
   (task constraint). Every claim above is source-level. In particular:
   - that `kill`'s escalation actually stops a guest that ignored `SIGTERM` is **asserted by
     the code, not measured** here;
   - that the `statedir` O_NOFOLLOW walk raises `ENOTDIR` on a planted symlink is code-read
     only (`statedir.py:197-207`); the error code mapping lives in `dirfd.is_refusal`
     (`dirfd.py:102`), first-pass territory;
   - the `msg()`-on-a-pTX-erased-progress-line behaviour is source-read, not observed.

3. **The help output was not rendered.** `commands/help/pages.py` is 1,080 lines of content
   and `render.py` 336 lines of layout; I read both, but I did not run `pd <cmd> --help`
   (prohibited) and so cannot confirm the 210,617-byte fixture reproduces. That is exactly
   the check the first pass listed as unknown, and it remains unknown here — the fixture is
   cited as a *reference* the PDN repo holds, not as something verified in this pass.

4. **The completions' runtime behaviour was not exercised.** Whether bash's
   `_proot_distro_get_containers` loop and zsh's `${d:t}` agree on a container named with a
   space or a newline is source-read only; the container-name regex (`names.py:40`) forbids
   neither space nor newline at the *command* boundary, so this is a real, untested branch.

5. **`dirfd.py` (1,361) was re-greped, not re-read** (command (d)). The 51 top-level symbols
   are inventoried; `Levels`, `temp_name`, `makedirs_at` and `open_regular_at` are cited by
   their `file:line` from that grep, and their behaviour is the first pass's account. No
   second-pass statement here contradicts or extends the first pass's `dirfd` section.

6. **`commands/login/__init__.py` (671) was read in full this pass**, which the first pass
   had not. Two things it adds are worth flagging as *unverified-by-measurement*: the
   `.l2s` descriptor pin (`login/__init__.py:520-529`) and the `--get-proot-cmd` warning
   that the printed line is *not* a copy of what the program runs
   (`login/__init__.py:543-547`). Both are source-read; neither was run.

7. **Two "would PDN be bitten by this" questions are raised but not answered**, because
   answering them needs PDN's own code executed:
   - Does PDN's `backup`/progress path draw on a TTY a sibling `pinentry` owns, the way
     `message.tty_safe_for_writes()` exists to prevent? (mechanism 3.2)
   - Does PDN's copy/sync write any destination by *name* rather than by descriptor, which
     on Termux is guest-writable? (mechanisms 3.4–3.5; the first pass's items 6 and the shim
     dossier's conclusion are the same question from other angles.)

8. **`helpers/docker/{pull,push,transport,cache,refs}.py`, `helpers/download.py`,
   `helpers/tar_extract.py`, `helpers/layer_diff.py`, `helpers/rootfs.py`,
   `helpers/dockerfile.py`, `helpers/build_engine/{engine,run_step,copy_step,handlers}.py`,
   `dirfd.py`, `paths.py`, `locking.py`, `guestfile.py`, `sysdata.py`,
   `commands/{install,install_local,clear_cache,backup,restore}.py` and
   `commands/login/{bindings,env,detach,proot_cmd,passwd,migrate}.py` are covered by the
   first pass at the depth it states and were deliberately not re-derived.** Their line
   counts are reproduced in section 1.2 so a later reader can see what is *not* in this
   pass and go to the companion document.
