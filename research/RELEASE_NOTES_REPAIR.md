# Release notes repair: v0.1.5 onward, and the withdrawal of v0.1.7

6 Nov 2026. Every number here comes from `gh release view <tag> --json body` plus a byte-level
count. The count covers real newlines, `0x0a`, and literal backslash-n tokens, `0x5c 0x6e`. No number
comes from the rendered release page. The rendered page is what hid the defect in the first place.

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

The phrase "mess from v0.1.5 onward" is exactly right. The contrast with v0.1.3 and v0.1.4 is not an
accident of style. **Two publishers existed.** The file `ci.yml` had a release job whose body was a YAML
literal block, `body: |`, so its newlines survived. At `v0.1.3` its heading was
`## PDN (PRoot Distro Next) v0.1.3`, which is the boilerplate since deleted in #77. The file `release.yml` built its
body from an Actions output. That output is line-oriented. The job rewrote every newline to the two characters `\` `n`
before it handed the text over.

Evidence for the mechanism, from the workflow as it stood at `v0.1.3`:

```
$ git show v0.1.3:.github/workflows/release.yml | grep -n "changelog="
187:  echo "changelog=$(cat CHANGELOG.md | sed 's/"/\\"/g' | sed ':a;N;s/\n/\\n/g;ta')" >> $GITHUB_OUTPUT
```

Further evidence comes from the body of v0.1.5 itself. Its first line was the first subject of the range
`v0.1.4..v0.1.5`. The fallback for a missing changelog section fired:

```
- Skip tests/clippy/fmt for Android targets (cannot run on x86_64 runner) (9e77874) ...
```

An earlier version of this project's own research recorded the fix as "the release body used to be
`git log -n 50`" and "fixed this release". That was half the defect. The *content* was wrong where the changelog
had no section. The *shape* was wrong wherever the body travelled through the Actions output.

## What was done

| action | before | after, read back |
|---|---|---|
| `gh release edit v0.1.5 --notes-file` | 1 line, 3,854 B, 49 escapes | **23 lines, 1,358 B, 0 escapes** |
| `gh release edit v0.1.8 --notes-file` (the `[0.1.8]` CHANGELOG section, which exists - 31 lines) | 1 line, 4,021 B, 49 escapes | **32 lines, 1,435 B, 0 escapes** |
| `gh release delete v0.1.7 --yes` | 285 B claiming "Complete roadmap ... all quality gates pass" | **absent from `gh release list`** |

v0.1.7 was withdrawn because its body does not describe what it tags. The measurement:
`git log -1 --format='%h %ad %s' --date=short a86aee2` is *Fix release workflow: revert to
nttld/setup-ndk@v1 for NDK setup*, dated 2026-10-31. That is a CI repair commit with no roadmap completion in it. The
**tag remains**. A check with `git ls-remote --tags origin` still lists `refs/tags/v0.1.7`. The pointer is therefore
kept, and only the misleading release object is gone. To remove the tag as well:

```sh
git push origin :refs/tags/v0.1.7 && git tag -d v0.1.7
```

The workflow fix that prevents a repeat landed earlier today and is not repeated here. The notes go
to `release-notes.md` and are read with `body_path:`. The script `scripts/check_release_notes.sh` refuses a
flattened or escaped body. Its `--self-test` mode proves that it refuses.

## What remains, deliberately untouched

- **v0.1.0, v0.1.1, v0.1.2** carry the same escape: 1 line, 39-42 tokens. They are outside the range of this
  round. Each repair needs the same single command:
  `gh release edit <tag> --notes-file <file>`.
- **A stray draft release object for v0.1.3**. The command `gh release list` shows `v0.1.3 draft=true` beside the public
  one. Visitors cannot see it. It is a duplicate of a tag that already has a published release. Nobody asked for its
  removal, so it stays. The command `gh release delete v0.1.3 --yes` removes the *published*
  release, not the draft. To clear the draft, target it by id with
  `gh api repos/<owner>/<repo>/releases --jq '.[]|select(.draft)|.id'`.
- **v0.1.6 has a tag and no release object.** A release for it must say that its whole delta is
  the CI fixes in the range. That is true and dull. The fact is recorded here instead of published.
- Two releases share a commit: **v0.1.3 and v0.1.4 both point at `71df242`**. They stay as
  they are. The body of v0.1.4 says so explicitly, with the words "Features" and "same as v0.1.3". That statement is honest.
