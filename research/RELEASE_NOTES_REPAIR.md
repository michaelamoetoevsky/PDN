# Release notes repair: v0.1.5 onward, and the withdrawal of v0.1.7

6 Nov 2026. Every number here was measured with `gh release view <tag> --json body` plus a byte-level
count of real newlines (`0x0a`) and literal backslash-n tokens (`0x5c 0x6e`) - not read off the rendered
release page, because the rendered page is what hid the defect in the first place.

## What was wrong, measured

| tag | bytes | real lines | literal `\n` | who published it | shape |
|---|---|---|---|---|---|
| v0.1.0 | 3,021 | **1** | 39 | `release.yml` | escaped commit list |
| v0.1.1 | 3,164 | **1** | 41 | `release.yml` | escaped commit list |
| v0.1.2 | 3,233 | **1** | 42 | `release.yml` | escaped commit list |
| v0.1.3 | 879 | 30 | 0 | `ci.yml` (boilerplate `body: \|`) | fine |
| v0.1.4 | 1,099 | 35 | 0 | `ci.yml` | fine |
| **v0.1.5** | 3,854 | **1** | 49 | `release.yml` fallback | escaped commit list |
| v0.1.6 | - | - | - | *(no release object; the tag exists)* | missing |
| **v0.1.7** | 285 | **1** | 0 | hand-written | one line, and it does not describe the tag |
| **v0.1.8** | 4,021 | **1** | 49 | `release.yml` | escaped commit list |
| v0.2.0 | 8,153 | 108 | 0 | `release.yml` | **repaired earlier today** (same defect) |

So the "mess from v0.1.5 onward" is exactly right, and the contrast with v0.1.3 and v0.1.4 is not an
accident of writing style: **two publishers existed.** `ci.yml` had a release job whose body was a YAML
literal block (`body: |`), so its newlines survived - and at `v0.1.3` its heading was
`## PDN (PRoot Distro Next) v0.1.3`, which is the boilerplate since deleted (#77). `release.yml` built its
body from an Actions output, which is line-oriented, and rewrote every newline to the two characters `\` `n`
before handing the text over.

Evidence for the mechanism, from the workflow as it stood at `v0.1.3`:

```
$ git show v0.1.3:.github/workflows/release.yml | grep -n "changelog="
187:  echo "changelog=$(cat CHANGELOG.md | sed 's/"/\\"/g' | sed ':a;N;s/\n/\\n/g;ta')" >> $GITHUB_OUTPUT
```

and from v0.1.5's body itself, whose first line was the first subject of the range
`v0.1.4..v0.1.5` - i.e. the no-changelog-section fallback fired:

```
- Skip tests/clippy/fmt for Android targets (cannot run on x86_64 runner) (9e77874) ...
```

An earlier version of this project's own research recorded the fix as "the release body used to be
`git log -n 50` (fixed this release)". That was half the defect: the *content* was wrong where the changelog
had no section, and the *shape* was wrong wherever the body travelled through the Actions output.

## What was done

| action | before | after, read back |
|---|---|---|
| `gh release edit v0.1.5 --notes-file` | 1 line, 3,854 B, 49 escapes | **23 lines, 1,358 B, 0 escapes** |
| `gh release edit v0.1.8 --notes-file` (the `[0.1.8]` CHANGELOG section, which exists - 31 lines) | 1 line, 4,021 B, 49 escapes | **32 lines, 1,435 B, 0 escapes** |
| `gh release delete v0.1.7 --yes` | 285 B claiming "Complete roadmap ... all quality gates pass" | **absent from `gh release list`** |

v0.1.7 was withdrawn because its body does not describe what it tags. Measured:
`git log -1 --format='%h %ad %s' --date=short a86aee2` is *Fix release workflow: revert to
nttld/setup-ndk@v1 for NDK setup* (2026-10-31) - a CI repair commit, with no roadmap completion in it. The
**tag remains** (verified: `git ls-remote --tags origin` still lists `refs/tags/v0.1.7`), so the pointer is
kept and only the misleading release object is gone. To remove the tag as well:

```sh
git push origin :refs/tags/v0.1.7 && git tag -d v0.1.7
```

The workflow fix that prevents recurrence landed earlier today and is not repeated here: the notes are
written to `release-notes.md` and read with `body_path:`, and `scripts/check_release_notes.sh` refuses a
flattened or escaped body - with `--self-test` proving that it refuses.

## What remains, deliberately untouched

- **v0.1.0, v0.1.1, v0.1.2** carry the same escape (1 line, 39-42 tokens). They are outside the range this
  round was asked for, and repairing them means the same one command each:
  `gh release edit <tag> --notes-file <file>`.
- **A stray draft release object for v0.1.3** (`gh release list` shows `v0.1.3 draft=true` beside the public
  one). It is invisible to visitors, and it is a duplicate of a tag that already has a published release, so
  it was left alone rather than deleted unasked. `gh release delete v0.1.3 --yes` removes the *published*
  one, not the draft, so anyone clearing it must target the draft by its id
  (`gh api repos/<owner>/<repo>/releases --jq '.[]|select(.draft)|.id'`).
- **v0.1.6 has a tag and no release object.** A release for it would have to say that its whole delta is
  the CI fixes in the range, which is true and dull; it is recorded here instead of published.
- The two releases whose tags share a commit (**v0.1.3 and v0.1.4 both point at `71df242`**) are left as
  they are: v0.1.4's body says so explicitly ("Features (same as v0.1.3)"), so it is honest.
