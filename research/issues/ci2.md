## Summary

`dependency-updates.yml` referenced an action that **does not exist**:
`peter-evans/update-github-actions@v1`. The command `gh api repos/peter-evans/update-github-actions`
returns `404 Not Found`. The repository list of the owner contains no such project. Thus the job
`github-actions-dependencies` was never able to succeed. The broken pin went unnoticed because
the scheduled trigger skips that job. The value `github.event.inputs.dependency-type` is empty on
a `schedule` event. Thus the job never executed.

## Reproduction / evidence

```
$ env -u LD_LIBRARY_PATH gh api repos/peter-evans/update-github-actions
{"message":"Not Found","documentation_url":"https://docs.github.com/rest/repos/repos#get-a-repository","status":404}

$ env -u LD_LIBRARY_PATH gh api "users/peter-evans/repos?per_page=100" --jq '.[].name' \
    | grep -i 'action\|update'
create-or-update-comment
create-or-update-project-card
approve-pull-request-action
create-or-update-comment
gradle-auto-dependency-updates
...
# no `update-github-actions`
```

The audit recorded this action in `research/CI_CD_AUDIT.md` as an unresolvable third-party
reference. A separate check confirmed it. The check queried the REST API for every pin in every
workflow and covered 14 distinct pins.

## Root cause

The pin arrived with the first version of the workflow, commit `950a927`. The job never
exercised the pin. The only automatic trigger is the weekly `schedule`. That trigger leaves the
input that gates the job unset. Thus the job is `skipped` on every scheduled run. See run
`41424692889`. In that run, `Update GitHub Actions Dependencies` shows `skipped` and
`Update Rust Dependencies` failed.

## Impact

- The job is dead code. It cannot run, so GitHub Actions versions never receive updates.
- Worse, it is a *silent* failure mode. An unresolvable action in any job appears only when
  that job runs, and such a job can stay dormant indefinitely.

## Fix

The fix replaces the third-party updater with a self-contained audit step. That step depends on
`gh` only, which is already present on the runner:

- It enumerates every `uses:` pin across `.github/workflows/*.yml`.
- It resolves each pin with `gh api repos/<owner>/<repo>/releases/latest`. For actions that
  publish no releases, it uses the tag list as a fallback. Such pins are branch-ref pins.
- It writes a summary table to `$GITHUB_STEP_SUMMARY`.
- **The step fails the job when it cannot resolve a pin.** Thus it catches a nonexistent action
  here. The action does not silently break the job that references it.

The exit status must control the resolution, and not the output. `gh api` prints its error body
on stdout. Thus an emptiness check treats a 404 as a resolved version. The first version of this
step had exactly that bug. A run against the previous revision caught the bug.

The author verified both directions before the commit:

```
A) PREVIOUS revision:
  peter-evans/update-github-actions  pinned=v1  latest=-   UNRESOLVABLE
  -> unresolved=1
  -> VERDICT: FAIL (exit 1)          # the gate catches it

B) CURRENT revision:
  -> unresolved=0
  -> VERDICT: pass (exit 0)
```

The REST API confirmed the existence of all 14 remaining pins. The pin bump to modern
Node-24-compatible majors occurred only where a release exists.

Labels: bug, ci, github-actions
