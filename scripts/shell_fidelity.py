#!/usr/bin/env python3
"""Terminal-fidelity probe for `podnext login` (ticket PDN#56).

Drives the REAL binary in a PTY (os.openpty + fork; never a screenshot) and
records, from inside a real guest:

  1a. `stty size` at a fixed PTY winsize         (static propagation)
  1b. `stty size` before/after a live resize of the parent PTY
  2.  `TERM` and `COLORTERM` as the guest sees them
  3.  the guest shell's `PS1`, and whether an interactive login emits a prompt
  4.  whether a multi-byte UTF-8 string survives a round trip (stdout, and
      through the guest's stdin via `cat`)

Negative control: the guest is re-run with TERM/COLORTERM forced to a marker
value, so the check is known to be able to observe a change (and to show which
of the two the runtime actually forwards).

CI: needs an installed container and a real PTY. With no container this prints
`SKIP: ...` and exits 0 -- a skip is NOT a pass, it is a skip (exit 0 only so
CI stays green). A measured property that fails exits 1.

Env overrides: PDNEXT_BIN (path to podnext), PDN_CONTAINER (container name).
Temp files, when used, go under $PREFIX/tmp (never /tmp on Android).
"""
import errno
import fcntl
import os
import pty
import re
import select
import shutil
import struct
import subprocess
import sys
import termios
import time

ROWS0, COLS0 = 24, 80
ROWS1, COLS1 = 40, 120
ENV = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}


def find_bin():
    b = os.environ.get("PDNEXT_BIN")
    if b and os.path.exists(b):
        return b
    return shutil.which("podnext")


def list_containers(binary):
    try:
        out = subprocess.run(
            [binary, "list"], env=ENV, capture_output=True, text=True, timeout=60
        ).stdout
    except Exception:
        return []
    names = []
    for line in out.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and cells[0] and cells[0] != "NAME" and set(cells[0]) != {"-"}:
            names.append(cells[0])
    return names


def find_container(binary):
    explicit = os.environ.get("PDN_CONTAINER")
    if explicit:
        return explicit
    names = list_containers(binary)
    if not names:
        return None
    return "alpine" if "alpine" in names else names[0]


def skip(reason):
    print(f"SKIP: {reason}")
    print("SKIP is not a pass.")
    sys.exit(0)


def set_winsize(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def spawn_pty(argv, rows, cols, env):
    """openpty, size it, then fork+exec argv with the slave as controlling tty."""
    master, slave = pty.openpty()
    set_winsize(master, rows, cols)
    pid = os.fork()
    if pid == 0:
        try:
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
            os.dup2(slave, 0)
            os.dup2(slave, 1)
            os.dup2(slave, 2)
            if slave > 2:
                os.close(slave)
            os.close(master)
            os.execvpe(argv[0], argv, env)
        except Exception:
            os._exit(127)
    os.close(slave)
    return pid, master


def drain(master, timeout, write_at=None):
    """Read the pty for `timeout` seconds. write_at = list of (delay, bytes)."""
    buf = b""
    pending = list(write_at or [])
    end = time.time() + timeout
    t0 = time.time()
    while time.time() < end:
        while pending and time.time() - t0 >= pending[0][0]:
            os.write(master, pending.pop(0)[1])
        r, _, _ = select.select([master], [], [], 0.2)
        if r:
            try:
                chunk = os.read(master, 4096)
            except OSError as e:
                if e.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            buf += chunk
    return buf


def reap(master, pid):
    try:
        os.close(master)
    except OSError:
        pass
    try:
        os.kill(pid, 9)
    except OSError:
        pass
    try:
        os.waitpid(pid, 0)
    except OSError:
        pass


def show(label, raw):
    print(f"{label}:")
    for line in raw.replace(b"\r\n", b"\n").split(b"\n"):
        if line.strip():
            print("    " + repr(line))


def run_capture(binary, container, guest_args, rows, cols, env):
    argv = [binary, "login", container, "--"] + guest_args
    pid, master = spawn_pty(argv, rows, cols, env)
    out = drain(master, 25.0)
    reap(master, pid)
    return out


def main():
    binary = find_bin()
    if not binary:
        skip("podnext binary not found (set PDNEXT_BIN or install it on PATH)")
    container = find_container(binary)
    if not container:
        skip("no container installed (`podnext list` returned nothing)")
    print(f"binary    : {binary}")
    print(f"container : {container}")
    print(f"python    : {sys.version.split()[0]}")
    print()

    failures = []

    # --- 1a. static winsize propagation -----------------------------------
    print(f"[1a] stty size at fixed PTY winsize {ROWS0}x{COLS0}:")
    raw = run_capture(binary, container, ["sh", "-c", "stty size"], ROWS0, COLS0, ENV)
    show("raw", raw)
    m = re.findall(rb"(\d+)\s+(\d+)", raw)
    got = (int(m[0][0]), int(m[0][1])) if m else None
    print(f"    parsed={got}  expected=({ROWS0}, {COLS0})")
    if got != (ROWS0, COLS0):
        failures.append(f"1a static winsize: guest saw {got}, PTY was {(ROWS0, COLS0)}")
    print()

    # --- 1b. live resize in one guest run ---------------------------------
    print(f"[1b] live resize {ROWS0}x{COLS0} -> {ROWS1}x{COLS1} in one guest run:")
    loop = (
        "i=0; while [ $i -lt 8 ]; do stty size; i=$((i+1)); sleep 0.6; done"
    )
    pid, master = spawn_pty([binary, "login", container, "--", "sh", "-c", loop], ROWS0, COLS0, ENV)
    front = drain(master, 2.6)
    set_winsize(master, ROWS1, COLS1)  # parent PTY resized mid-run
    back = drain(master, 8.0)
    reap(master, pid)
    allraw = front + back
    show("raw (all 'stty size' readings, in order)", allraw)
    sizes = re.findall(rb"(\d+)\s+(\d+)", allraw)
    seq = [(int(a), int(b)) for a, b in sizes]
    before = seq[0] if seq else None
    after = seq[-1] if seq else None
    print(f"    readings={seq}")
    print(f"    before={before} expected=({ROWS0}, {COLS0})")
    print(f"    after ={after}  expected=({ROWS1}, {COLS1})")
    if before != (ROWS0, COLS0) or after != (ROWS1, COLS1):
        failures.append(f"1b live resize: before={before}, after={after}, seq={seq}")
    print()

    # --- 2. TERM / COLORTERM ---------------------------------------------
    print("[2] TERM / COLORTERM inside the guest:")
    raw = run_capture(
        binary, container,
        ["sh", "-c", 'printf "TERM=[%s]\\nCOLORTERM=[%s]\\n" "$TERM" "$COLORTERM"'],
        ROWS0, COLS0, ENV,
    )
    show("raw (pty)", raw)
    term = re.search(rb"TERM=\[([^\]]*)\]", raw)
    cterm = re.search(rb"COLORTERM=\[([^\]]*)\]", raw)
    term_v = term.group(1).decode() if term else None
    cterm_v = cterm.group(1).decode() if cterm else None
    print(f"    TERM={term_v!r}  COLORTERM={cterm_v!r}  (caller had TERM={ENV.get('TERM')!r})")
    if not term or not cterm:
        failures.append("2 TERM/COLORTERM not observable in guest output")
    print()

    # --- 3. PS1 ----------------------------------------------------------
    print("[3] PS1 (guest shell default) and interactive prompt:")
    raw = run_capture(binary, container, ["sh", "-c", 'printf "PS1=[%s]" "$PS1"'], ROWS0, COLS0, ENV)
    show("sh -c PS1 (raw)", raw)
    ps1 = re.search(rb"PS1=\[([^\]]*)\]", raw)
    print(f"    PS1={ps1.group(1).decode() if ps1 else None!r}")
    # interactive: does a plain `podnext login <c>` emit any prompt at all?
    pid, master = spawn_pty([binary, "login", container], ROWS0, COLS0, ENV)
    banner = drain(master, 7.0)
    reap(master, pid)
    show("plain `podnext login` output (raw, 7s)", banner)
    prompt_seen = bool(banner.replace(b"\r", b"").strip())
    print(f"    interactive prompt bytes_seen={prompt_seen}")
    if ps1 is None:
        failures.append("3 PS1 not readable from guest shell")
    if not prompt_seen:
        failures.append("3 interactive `podnext login <c>` emitted no prompt (guest stdin unusable, see 4b)")
    print()

    # --- 4a. UTF-8 round trip via stdout ----------------------------------
    payload = "héllo→世界"
    print(f"[4a] UTF-8 round trip via stdout: {payload!r}")
    raw = run_capture(binary, container, ["sh", "-c", f"printf '%s\\n' '{payload}'"], ROWS0, COLS0, ENV)
    show("raw", raw)
    want = payload.encode("utf-8")
    print(f"    want={want}  survived={want in raw}")
    if want not in raw:
        failures.append(f"4a stdout utf8 round trip lost bytes for {payload!r}")
    print()

    # --- 4b. UTF-8 round trip through the guest's stdin (cat) -------------
    print(f"[4b] UTF-8 round trip through guest stdin (`cat`, feed {payload!r}):")
    pid, master = spawn_pty(
        [binary, "login", container, "--", "cat"], ROWS0, COLS0, ENV
    )
    feed = payload.encode("utf-8") + b"\n"
    out = drain(master, 8.0, write_at=[(1.5, feed)])
    reap(master, pid)
    show("raw", out)
    # correct behaviour: tty echo + cat's echo => payload appears twice.
    hits = out.count(want)
    print(f"    occurrences of payload in output={hits} (expect 2: tty echo + cat echo)")
    if hits < 2:
        failures.append(f"4b guest stdin not readable: `cat` never returned the fed payload (hits={hits})")
    print()

    # --- negative control for 4b: the harness itself round-trips on the host
    print(f"[neg2] control: same PTY harness, host `cat` (no podnext), feed {payload!r}:")
    host_cat = shutil.which("cat") or "/bin/cat"
    pid, master = spawn_pty([host_cat], ROWS0, COLS0, ENV)
    out = drain(master, 4.0, write_at=[(1.0, feed)])
    reap(master, pid)
    show("raw (control)", out)
    hh = out.count(want)
    print(f"    occurrences of payload in output={hh} (expect 2)")
    if hh < 2:
        failures.append("neg2 control: the harness itself failed to round-trip stdin; 4b is unproven")
    print()

    # --- negative control: marker TERM/COLORTERM --------------------------
    print("[neg] control: TERM='footest' COLORTERM='xterm-kitty' forced on the caller:")
    ctl_env = dict(ENV)
    ctl_env["TERM"] = "footest"
    ctl_env["COLORTERM"] = "xterm-kitty"
    raw = run_capture(
        binary, container,
        ["sh", "-c", 'printf "TERM=[%s]\\nCOLORTERM=[%s]\\n" "$TERM" "$COLORTERM"'],
        ROWS0, COLS0, ctl_env,
    )
    show("raw (control)", raw)
    ct = re.search(rb"TERM=\[([^\]]*)\]", raw)
    cc = re.search(rb"COLORTERM=\[([^\]]*)\]", raw)
    ct_v = ct.group(1).decode() if ct else None
    cc_v = cc.group(1).decode() if cc else None
    print(f"    TERM={ct_v!r}  COLORTERM={cc_v!r}")
    # This control proves the check CAN see a caller-supplied value: COLORTERM
    # is forwarded (changes), TERM is not (runtime hardcodes xterm-256color).
    if cc_v != "xterm-kitty":
        failures.append(
            "neg control: COLORTERM='xterm-kitty' was not forwarded to the guest; "
            "the env check cannot be shown to observe a change"
        )
    if ct_v == "footest":
        print("    (noted) TERM is forwarded from the caller")
    else:
        print(f"    (finding) TERM is NOT forwarded: caller 'footest' -> guest {ct_v!r} "
              f"(runtime hardcodes it)")
    print()

    if failures:
        print("FAIL:")
        for f in failures:
            print(f"    - {f}")
        sys.exit(1)
    print("PASS: every measured property matched expectation.")


if __name__ == "__main__":
    main()
