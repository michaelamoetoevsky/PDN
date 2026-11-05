#!/bin/sh
# Smoke-test a RELEASED podnext artifact on this device, step by step, with every exit code recorded.
#
# Why this exists as a committed script: the v0.1.9 tag was pushed, forced red, and withdrawn without the
# artifact ever being run. A release is not "done" because CI built bytes - it is done when those exact
# bytes install, run, log in, and clean up on the device they claim to support. This script is that step,
# and it is re-runnable by construction: it takes a binary, uses a container name it checked is free, and
# refuses to touch anything that already exists.
#
# Usage:  sh scripts/smoke_release.sh <path-to-podnext> [expected-version] [container-name]
# Exit:   0 every step passed; 1 a step failed; 2 cannot measure (bad arguments, missing binary, or the
#         target container already exists - in which case nothing was changed).
#
# POSIX sh only (Termux `sh` is dash). No /tmp: the log goes to $PREFIX/tmp.

set -u

BIN=${1:-}
EXPECT=${2:-}
NAME=${3:-pdn-smoke-$$}
TMPBASE=${PREFIX:-/data/data/com.termux/files/usr}/tmp
LOG="$TMPBASE/smoke_release-$(date +%Y%m%dT%H%M%S).log"
FAILED=0
STEPS=""

die() { printf 'CANNOT MEASURE: %s\n' "$1" >&2; exit 2; }

[ -n "$BIN" ] || die "usage: sh scripts/smoke_release.sh <path-to-podnext> [expected-version] [container-name]"
[ -f "$BIN" ] || die "binary not found: $BIN"
[ -x "$BIN" ] || die "binary not executable: $BIN"
mkdir -p "$TMPBASE" || die "temporary base not writable: $TMPBASE"

# Never destroy evidence or user state: refuse to run against an existing container.
# `list -a` is a pipe table: the name is the first column of a data row, and the row itself does NOT end
# with the name (it ends with ` |`). Matching on end-of-line would silently fail to find a collision.
if "$BIN" list -a 2>/dev/null | grep -qE "^\| *$NAME *\|"; then
  die "container '$NAME' already exists - pick another name; nothing was changed"
fi

step() {
  # step <label> <expected-rc> <command...>
  label=$1; want=$2; shift 2
  printf '\n== %s ==\n$ %s\n' "$label" "$*" >> "$LOG"
  "$@" >> "$LOG" 2>&1
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  %-34s rc=%-3s ok\n' "$label" "$rc"
    STEPS="$STEPS$label|$rc|ok
"
  else
    printf '  %-34s rc=%-3s FAILED (wanted %s)\n' "$label" "$rc" "$want"
    STEPS="$STEPS$label|$rc|FAILED(wanted $want)
"
    FAILED=1
  fi
  return 0
}

printf 'smoke test: %s\ncontainer:  %s\nlog:        %s\nstarted:    %s\n' \
  "$BIN" "$NAME" "$LOG" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" | tee "$LOG"

# 1. The artifact identifies itself, and identifies itself as the version we think we shipped.
OUT=$("$BIN" version 2>&1); rc=$?
printf '\n== version ==\n%s\n' "$OUT" >> "$LOG"
if [ "$rc" = 0 ]; then
  printf '  %-34s rc=0   ok (%s)\n' "version" "$OUT"
  STEPS="$STEPS""version|0|ok
"
  if [ -n "$EXPECT" ]; then
    if printf '%s' "$OUT" | grep -q -- "$EXPECT"; then
      printf '  %-34s ok (%s)\n' "version matches release" "$EXPECT"
      STEPS="$STEPS""version matches release|0|ok
"
    else
      printf '  %-34s FAILED (expected %s)\n' "version matches release" "$EXPECT"
      STEPS="$STEPS""version matches release|-|FAILED
"
      FAILED=1
    fi
  fi
else
  printf '  %-34s rc=%s FAILED\n' "version" "$rc"
  STEPS="$STEPS""version|$rc|FAILED
"
  FAILED=1
fi

# 2. Host diagnostics - the checks that do not need a container.
step "doctor (host)" 0 "$BIN" doctor

# 3. Install from the registry. alpine is ~3 MB, so this stays cheap on a phone.
step "install alpine" 0 "$BIN" install alpine:latest --name "$NAME"

# 4. It is listed, and the listing knows its name - asserted, not merely "the command exited 0".
if "$BIN" list -a 2>/dev/null | grep -qE "^\| *$NAME *\|"; then
  printf '  %-34s ok\n' "list shows the container"
  STEPS="$STEPS""list shows the container|0|ok
"
else
  printf '  %-34s FAILED (absent from list -a)\n' "list shows the container"
  STEPS="$STEPS""list shows the container|-|FAILED
"
  FAILED=1
fi

# 5. A command runs inside it and its stdout reaches the caller.
OUT5=$("$BIN" run "$NAME" -- echo SMOKE-MARKER-OK 2>&1); rc5=$?
printf '\n== run ==\n%s\n' "$OUT5" >> "$LOG"
if [ "$rc5" = 0 ] && printf '%s' "$OUT5" | grep -q SMOKE-MARKER-OK; then
  printf '  %-34s rc=0   ok (marker seen)\n' "run -- echo"
  STEPS="$STEPS""run -- echo (marker seen)|0|ok
"
else
  printf '  %-34s rc=%s FAILED (marker seen: %s)\n' "run -- echo" "$rc5" "$(printf '%s' "$OUT5" | grep -c SMOKE-MARKER-OK)"
  STEPS="$STEPS""run -- echo|$rc5|FAILED
"
  FAILED=1
fi

# 6. The interactive entry point accepts a trailing command (this is what `--user` and friends live behind).
step "login -- true" 0 "$BIN" login "$NAME" -- true

# 7. The guest side of doctor, which is the part that reads the proot argv.
step "doctor --container" 0 "$BIN" doctor --container "$NAME"

# 8. Sessions listing stays honest with nothing running.
step "sessions" 0 "$BIN" sessions

# 9. Clean removal. `remove` refuses without a tty unless forced, and that refusal is correct behaviour -
#    it names the escape hatch and exits 1 - so a non-interactive smoke test must pass it explicitly.
step "remove --force" 0 "$BIN" remove "$NAME" --force

# 10. And it is gone - a removal that leaves the container behind is not a removal.
if "$BIN" list -a 2>/dev/null | grep -qE "^\| *$NAME *\|"; then
  printf '  %-34s FAILED (still listed)\n' "container is gone"
  STEPS="$STEPS""container is gone|-|FAILED
"
  FAILED=1
else
  printf '  %-34s ok\n' "container is gone"
  STEPS="$STEPS""container is gone|0|ok
"
fi

{
  printf '\n== summary ==\n'
  printf '%s' "$STEPS" | while IFS='|' read -r l rc v; do
    [ -n "${l:-}" ] && printf '  %-34s %s\n' "$l" "$v"
  done
  if [ "$FAILED" = 0 ]; then
    printf 'SMOKE TEST PASSED - the artifact installs, runs, logs in and cleans up on this device\n'
  else
    printf 'SMOKE TEST FAILED - do not mark the release latest; log: %s\n' "$LOG"
  fi
} | tee -a "$LOG"

if [ "$FAILED" != 0 ]; then
  printf 'cleanup: %s remove %s --force   # this run left the container installed on purpose\n' "$BIN" "$NAME"
  exit 1
fi
exit 0
