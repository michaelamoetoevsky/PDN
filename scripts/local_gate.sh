#!/usr/bin/env bash
# Local pre-commit gate. Runs the cheap checks that must pass before anything is committed, and
# STOPS AT THE FIRST FAILURE WITH A NON-ZERO STATUS.
#
# Why this exists: a gate was run as part of a bare command list, its output was piped, the pipe
# returned the pager's status, and a commit went out with the gate red ("checks: 110 PASS, 1 FAIL").
# The ledger already exits non-zero on a failure - the mistake was not reading it. Piping a gate is
# how a gate stops being one, so this script is the thing to run instead, and it cannot be misread:
# it prints a verdict line per stage and exits non-zero the moment one fails.
#
#   scripts/local_gate.sh            # everything cheap
#   scripts/local_gate.sh --cargo    # also the cargo-gated ledger checks (slow; needs the memory guard)
#
# Deliberately NOT included: builds and test suites. Those belong in CI (and on this device they need
# the memory guard), so a local gate that ran them would be skipped rather than respected.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2
export PATH="$HOME/.hermes/bin:$PATH"

CARGO=0
[ "${1:-}" = "--cargo" ] && CARGO=1
LEDGER_ARGS="--offline"
[ "$CARGO" = "1" ] && LEDGER_ARGS="--cargo --no-write"

fail=0
skipped=0
stage() { printf '\n== %s\n' "$1"; }
verdict() {
  if [ "$1" = "0" ]; then printf '   ok\n'; else printf '   FAILED (exit %s)\n' "$1"; fail=1; fi
}

stage "shell/python syntax"
python3 -m py_compile scripts/*.py 2>/dev/null
verdict $?
rm -rf scripts/__pycache__

stage "temp-base rule (self-check)"
env -u LD_LIBRARY_PATH python3 scripts/temp_base.py >/dev/null
verdict $?

stage "warning ratchet (self-check, no cargo)"
env -u LD_LIBRARY_PATH python3 scripts/check_warnings.py self-check >/dev/null
verdict $?

stage "rust formatting (read-only)"
env -u LD_LIBRARY_PATH cargo fmt --all -- --check >/dev/null 2>&1
verdict $?

stage "issue ledger ($LEDGER_ARGS)"
ledger_out=$(env -u LD_LIBRARY_PATH python3 scripts/build_issue_ledger.py $LEDGER_ARGS 2>&1)
ledger_rc=$?
printf '%s\n' "$ledger_out" | grep -E '^checks: ' | sed 's/^/   /'
printf '%s\n' "$ledger_out" | grep -E 'FAIL' | grep -v '0 FAIL' | head -5 | sed 's/^/   /'
verdict $ledger_rc

# The help dump, done so it CANNOT report a false identity. Two ways it did exactly that during this
# project: `--out` was not a flag of dump_cli_help.py, so the file was never written while
# `sha256sum A B | sort -u | wc -l` printed 1 - which reads as "identical" and means "one file is
# missing"; and the dump's stderr was discarded, so the reason was invisible. Both are fixed here:
# both files must exist, the screen count is checked, and the comparison is a plain string equality.
stage "help dump (123 screens, byte-identical)"
BIN="${PDNEXT_BIN:-target/debug/podnext}"
BASE="${HELP_BASELINE:-${PREFIX:-/data/data/com.termux/files/usr}/tmp/help-baseline.txt}"
if [ ! -x "$BIN" ]; then
  printf '   SKIP: no binary at %s - a skip is not a pass\n' "$BIN"
  skipped=$((skipped + 1))
else
  dump=$(mktemp "${TMPDIR:-${PREFIX:-/data/data/com.termux/files/usr}/tmp}/help-gate.XXXXXX")
  env -u LD_LIBRARY_PATH python3 scripts/dump_cli_help.py "$BIN" >"$dump" 2>"$dump.count"
  dump_rc=$?
  count=$(tail -1 "$dump.count" 2>/dev/null)
  if [ "$dump_rc" != "0" ]; then
    printf '   FAILED: the dump itself failed (rc=%s): %s\n' "$dump_rc" "$(head -3 "$dump.count")"
    fail=1
  elif [ ! -f "$BASE" ]; then
    printf '   SKIP: no baseline at %s - a skip is not a pass\n' "$BASE"
    skipped=$((skipped + 1))
  elif ! printf '%s' "$count" | grep -q '123 help screens'; then
    printf '   FAILED: dump reported %s, expected 123 screens\n' "${count:-nothing}"
    fail=1
  elif cmp -s "$dump" "$BASE"; then
    printf '   ok  (%s, %s)\n' "$(sha256sum "$dump" | cut -c1-16)" "$count"
  else
    printf '   FAILED: dump differs from %s\n' "$BASE"
    diff "$dump" "$BASE" | head -10 | sed 's/^/   /'
    fail=1
  fi
  rm -f "$dump"
fi

if [ "$fail" = "0" ] && [ "$skipped" = "0" ]; then
  printf '\nGATE GREEN - safe to commit\n'
elif [ "$fail" = "0" ]; then
  # A skip is not a pass: the run is safe to commit as far as it went, and it did not go everywhere.
  printf '\nGATE GREEN with %s check(s) SKIPPED - the verdict is not full coverage\n' "$skipped"
else
  printf '\nGATE RED - do not commit; the failing stage is above\n'
fi
exit "$fail"
