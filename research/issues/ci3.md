## Summary

`.gitignore` lists `Cargo.lock`, so the lockfile is **not in the repository**. This workspace is
an application, and `podnext`, `podnext-daemon` and `pn-gateway` are binaries. In such a
workspace the lockfile must be in the repository for reproducible builds. This defect has two
consequences:

1. Every CI run resolved the dependencies again from the start. Thus a green run was not
   reproducible from its commit.
2. The addition of `--locked`, which is the fix that `research/CI_CD_AUDIT.md` D7 recommends,
   made **every** build fail on a clean checkout.

## Reproduction / evidence

```
$ git ls-files Cargo.lock
$                       # nothing — the file is not tracked

$ git check-ignore -v Cargo.lock
.gitignore:3:Cargo.lock	Cargo.lock

$ ls -la Cargo.lock
-rw-------. 1 u0_a396 u0_a396 137166 Nov  4 06:33 Cargo.lock    # exists locally only
```

CI run 41765095295 is in `Android Cross-Compile Matrix`, with all four matrix cells and the step
`Build workspace for <target>`:

```
Run cargo build --workspace --release --locked --target aarch64-linux-android
error: cannot create the lock file /home/runner/work/PDN/PDN/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
##[error]Process completed with exit code 101.
```

The `Aggregate Artifacts & Checksums` job then failed only because there were no artifacts. That
failure is a cascade and not an independent cause.

## Root cause

Line 3 of `.gitignore` contains `Cargo.lock`, which is the stock `cargo new --lib` template. PDN
is not a library-only workspace, so this entry is wrong here. The D7 *recommendation* of the
audit was correct, but its *evidence* was not. It recorded "`Cargo.lock` is committed (`ls -la
Cargo.lock` -> 137166 bytes)". That record proves only that the file exists on a developer
machine. It does not prove that git tracks the file. The cross-compile run surfaced the
difference.

## Impact

- CI was not reproducible. The dependency versions were whatever the registry resolved that day.
- `--locked` is the guard against exactly that, and no place was able to use it.
- Every fresh clone resolves the full graph on the first build.

## Fix

- Remove `Cargo.lock` from `.gitignore` and add a comment that explains why this removal is
  deliberate. Commit the lockfile.
- Keep `--locked` on every cargo invocation. It now has an effect.

The fix also repairs the checksum command of the aggregate step:
`find all-artifacts -type f -name "*.so" -o -name "podnext"`. The `-o` operator binds looser
than the implicit `-a` operator. Thus the command read
`(-type f -name "*.so") OR (-name podnext)`. It then was able to hand a *directory* named
`podnext` to `sha256sum`. The fix groups the predicates. It adds
`-print0 | xargs -0` and `mkdir -p all-artifacts`, so the redirect cannot fail. It also adds an
explicit warning and not a bare `cat` when nothing exists to checksum.

Labels: bug, ci
