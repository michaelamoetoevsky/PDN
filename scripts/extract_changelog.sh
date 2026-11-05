#!/bin/sh
# Print the CHANGELOG.md section for one version, for use as a release body.
#
# Why: the release workflow used to build the release body with `git log -n 50 --pretty='- %s (%h)'`, which
# ships the last fifty commit subjects - chore and docs noise included, and cut at a count that has nothing
# to do with the release. The curated section is already in the repository; this reads it.
#
# Usage:  sh scripts/extract_changelog.sh [CHANGELOG.md] <version>
#   <version> may carry a leading `v` (a tag name is the same string as the heading).
# Output: the section body on stdout. Exit 1 with a message and NO stdout when there is no section for that
#         version - this script never invents text, and a caller must decide whether that is fatal.
#
# POSIX sh only (Termux `sh` is dash).

set -u

FILE=${1:-CHANGELOG.md}
VER=${2:-}

[ -n "$VER" ] || { printf 'usage: %s [CHANGELOG.md] <version>\n' "$0" >&2; exit 2; }
[ -f "$FILE" ] || { printf 'no such file: %s\n' "$FILE" >&2; exit 2; }

case "$VER" in v*) VER=${VER#v};; esac

# Everything strictly between `## [<ver>]` and the next `## [` heading, with runs of blank lines collapsed.
BODY=$(awk -v want="$VER" '
  /^## \[/ {
    if (found) exit
    if ($0 ~ "^## \\[" want "\\]") { found = 1; next }
  }
  found { print }
' "$FILE" | sed -e 's/[[:space:]]*$//' | awk 'BEGIN{b=0} {if ($0=="") {b++; if (b>1) next} else b=0; print}')

if [ -z "$(printf '%s' "$BODY" | tr -d '[:space:]')" ]; then
  printf 'no changelog section for version %s in %s\n' "$VER" "$FILE" >&2
  exit 1
fi

printf '%s\n' "$BODY"
exit 0
