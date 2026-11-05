#!/bin/sh
# Pre-tag release preflight: smoke-test the artifact CI built for a commit, BEFORE the tag exists.
#
# Why before: `.github/workflows/release.yml` creates the release with `draft: false` and the action's
# default `make_latest`, so pushing the tag publishes a release marked latest immediately. A smoke test run
# after that cannot prevent anything - it can only describe the damage. This script downloads the
# release-mode aarch64 Android binary that the PDN CI/CD workflow already produced for the commit, checks
# what it is, and runs the real smoke test against it, with no tag in existence yet.
#
# Usage:  sh scripts/preflight_release.sh <sha|HEAD> <expected-version> [timeout-minutes]
# Exit:   0 the artifact passed the smoke test; 1 the smoke test failed; 2 cannot measure (bad arguments,
#         no such run, run not green, artifact missing, wrong architecture).
#
# POSIX sh only (Termux `sh` is dash). No /tmp: work happens under $PREFIX/tmp.

set -u

SHA_ARG=${1:-}
WANT=${2:-}
BUDGET=${3:-40}
TMPBASE=${PREFIX:-/data/data/com.termux/files/usr}/tmp
REPO=michaelamoetoevsky/PDN
ART=pn-android-aarch64
WORK="$TMPBASE/preflight-${SHA_ARG:-none}"

die() { printf 'CANNOT MEASURE: %s\n' "$1" >&2; exit 2; }

[ -n "$SHA_ARG" ] && [ -n "$WANT" ] || die "usage: sh scripts/preflight_release.sh <sha|HEAD> <expected-version> [timeout-minutes]"
command -v gh >/dev/null 2>&1 || die "gh is not installed"

SHA=$(git rev-parse --short=7 "$SHA_ARG" 2>/dev/null) || die "not a commit: $SHA_ARG"
printf 'preflight: commit %s, expecting version %s\n' "$SHA" "$WANT"

# 1. Find the workflow run for that commit. Total > 0 and pending == 0, printed every tick so a stuck
#    search says so instead of ending in silence.
printf 'waiting for a PDN CI/CD run on %s (up to %s minutes)\n' "$SHA" "$BUDGET"
RID=""
i=0
while [ "$i" -lt "$((BUDGET * 3))" ]; do
  RID=$(gh run list --limit 30 --json databaseId,headSha,name,status,conclusion \
        --jq "[.[] | select(.headSha|startswith(\"$SHA\")) | select(.name==\"PDN CI/CD\")][0].databaseId" 2>/dev/null)
  ST=$(gh run list --limit 30 --json headSha,name,status \
        --jq "[.[] | select(.headSha|startswith(\"$SHA\")) | select(.name==\"PDN CI/CD\")][0].status" 2>/dev/null)
  printf '  tick %s: run=%s status=%s\n' "$i" "${RID:-none}" "${ST:-none}"
  [ -n "$RID" ] && [ "$ST" = "completed" ] && break
  i=$((i + 1))
  sleep 20
done
[ -n "$RID" ] || die "no PDN CI/CD run found for $SHA"

CC=$(gh run view "$RID" --json conclusion --jq '.conclusion' 2>/dev/null)
printf 'run %s conclusion: %s\n' "$RID" "$CC"
[ "$CC" = "success" ] || die "the run for $SHA is '$CC', not success - fix the tree before tagging"

# 2. Fetch the device artifact. Two-stage download via `gh run download` (never a blob URL, which needs its
#    own Authorization header and fails without one).
rm -rf "$WORK"; mkdir -p "$WORK" || die "cannot create $WORK"
TRY=0; ERR=""
while [ "$TRY" -lt 3 ]; do
  TRY=$((TRY + 1))
  if ERR=$(gh run download "$RID" -n "$ART" -D "$WORK" 2>&1); then ERR=""; break; fi
  printf '  download attempt %s failed: %s\n' "$TRY" "$(printf '%s' "$ERR" | tail -1)"
  sleep 5
done
[ -z "$ERR" ] || die "could not download artifact '$ART' from run $RID after 3 attempts: $(printf '%s' "$ERR" | tail -1)"
BIN=$(find "$WORK" -type f -name podnext | head -1)
[ -n "$BIN" ] || die "no podnext binary inside artifact '$ART'"
chmod +x "$BIN"

# 3. Prove it is what it claims to be before trusting anything it prints.
SIZE=$(wc -c < "$BIN" | tr -d ' ')
ARCH=$(file "$BIN" 2>/dev/null | grep -c aarch64 || true)
printf 'artifact: %s bytes, aarch64 match: %s\n' "$SIZE" "$ARCH"
[ "${ARCH:-0}" -ge 1 ] || die "$BIN is not an aarch64 binary - wrong artifact?"
[ "$SIZE" -gt 1000000 ] || die "$BIN is suspiciously small ($SIZE bytes) - probably not a binary"

# 4. Run the real smoke test against the artifact. No tag exists at this point.
printf '\n=== smoke test against the CI artifact ===\n'
sh "$(dirname "$0")/smoke_release.sh" "$BIN" "$WANT"
RC=$?
printf '\npreflight exit: %s   artifact: %s\n' "$RC" "$BIN"
[ "$RC" = 0 ] && printf 'PREFLIGHT PASSED - safe to tag %s\n' "$WANT" || printf 'PREFLIGHT FAILED - do not tag\n'
exit "$RC"
