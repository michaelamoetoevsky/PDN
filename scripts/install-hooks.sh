#!/bin/sh
# Install the repository's git hooks (.githooks/) for this clone and every worktree.
#
#   sh scripts/install-hooks.sh          # install
#   sh scripts/install-hooks.sh --check  # report only, change nothing
#
# Hooks are version-controlled in .githooks/ and activated by core.hooksPath. Because
# core.hooksPath lives in the shared .git/config, `git worktree add` clones inherit it
# too, which is the point: worktrees used for parallel work must not be able to commit
# unformatted Rust that then fails CI.
set -e

REPO=$(git rev-parse --show-toplevel)
cd "$REPO"

current=$(git config --get core.hooksPath || true)

if [ "${1:-}" = "--check" ]; then
    if [ "$current" = ".githooks" ]; then
        echo "hooks: installed (core.hooksPath=$current)"
        exit 0
    fi
    echo "hooks: NOT installed (core.hooksPath=${current:-<unset>} -> git uses .git/hooks)" >&2
    exit 1
fi

[ -x .githooks/pre-commit ] || chmod +x .githooks/pre-commit

git config core.hooksPath .githooks
echo "hooks: core.hooksPath=$(git config --get core.hooksPath)"
echo "hooks: executable check -> $( [ -x .githooks/pre-commit ] && echo yes || echo NO )"
echo "hooks: staged Rust files are now fmt-checked before every commit"
echo "hooks: bypass once with PDN_SKIP_HOOKS=1 git commit ..."
