#!/bin/sh
# Check a release-notes file: real line breaks, and no literal backslash-n token.
#
# Why this exists: the published v0.2.0 body was 8 KB of notes on a single line, with `\n` visible where
# the line breaks should have been. Every gate passed - the changelog section itself was correct, and the
# flattening happened on the way to the release. This checks the file the release actually reads.
#
# Usage:  sh scripts/check_release_notes.sh <notes-file> [--self-test]
# Exit:   0 the notes are well-formed; 1 they are not, with the reason; 2 cannot measure.
#
# POSIX sh only (Termux `sh` is dash). No /tmp: the self-test's copies go under $PREFIX/tmp.

set -u

FILE=${1:-}
MODE=${2:-}

[ -n "$FILE" ] || { printf 'usage: %s <notes-file> [--self-test]\n' "$0" >&2; exit 2; }
[ -f "$FILE" ] || { printf 'no such file: %s\n' "$FILE" >&2; exit 2; }

check() {
	_f=$1
	_lines=$(wc -l < "$_f")
	if [ "$_lines" -lt 5 ]; then
		printf 'FAIL: %s has %s lines - the notes were flattened\n' "$_f" "$_lines" >&2
		return 1
	fi
	if grep -qF '\n' "$_f"; then
		printf 'FAIL: %s contains a literal backslash-n token\n' "$_f" >&2
		grep -nF '\n' "$_f" | head -3 >&2
		return 1
	fi
	printf 'OK: %s (%s lines, no escaped newlines)\n' "$_f" "$_lines"
	return 0
}

if [ "$MODE" = "--self-test" ]; then
	TMPBASE=${PREFIX:-/data/data/com.termux/files/usr}/tmp
	DIR="$TMPBASE/notes-check-$$"
	mkdir -p "$DIR" || exit 2
	# A clean copy has to pass, and a copy carrying the escape that shipped has to fail. When both answer
	# the same way the check does not discriminate, and this says so instead of claiming a verdict.
	cp "$FILE" "$DIR/clean.md"
	sed ':a;N;s/\n/\\n/g;ta' "$FILE" > "$DIR/escaped.md"
	"$0" "$DIR/clean.md" >/dev/null 2>&1; _clean=$?
	"$0" "$DIR/escaped.md" >/dev/null 2>&1; _esc=$?
	rm -rf "$DIR"
	if [ "$_clean" = 0 ] && [ "$_esc" = 1 ]; then
		printf 'SELF-TEST OK: clean notes pass, escaped notes fail\n'
		exit 0
	fi
	printf 'SELF-TEST INCONCLUSIVE: clean rc=%s, escaped rc=%s\n' "$_clean" "$_esc" >&2
	exit 2
fi

check "$FILE"
