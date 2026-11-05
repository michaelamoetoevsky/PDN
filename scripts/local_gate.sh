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

if [ "$fail" = "0" ]; then
  printf '\nGATE GREEN - safe to commit\n'
else
  printf '\nGATE RED - do not commit; the failing stage is above\n'
fi
exit "$fail"
