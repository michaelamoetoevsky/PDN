# Fork-PR benchmark reporting: reproduction, fix, and items still unverified

Department: CI Engineering, known as CIE. Scope: the file `.github/workflows/benchmark.yml` and the new file `.github/workflows/benchmark-report.yml`. Base revision `72ef9d6`.

**Verdict: raw API and log evidence reproduce the limitation. The fix is locally verifiable only for the YAML and the permission reasoning. The result "the comment actually appears on a fork PR" is UNVERIFIED. This environment cannot verify that result. Refer to section 5.**

---

## 1. Reproduce the limitation against the real API

All calls are read-only: `env -u LD_LIBRARY_PATH gh api …`. Repo `michaelamoetoevsky/PDN`.

### 1.1 The run and its job are real, and the run is a fork-PR run

```
$ env -u LD_LIBRARY_PATH gh api "repos/michaelamoetoevsky/PDN/actions/runs/41848512267" \
    --jq '{id,event,head_branch,head_sha,conclusion,workflow_name:.name,head_repository:.head_repository.full_name,pull_requests:.pull_requests}'
{"conclusion":"failure","event":"pull_request","head_branch":"master","head_repository":"gabm0932/PDN","head_sha":"30dabb147ac94b6df636dd6c041d7ef021bd3470","id":41848512267,"pull_requests":[],"workflow_name":"Benchmark"}
```

`head_repository: gabm0932/PDN` shows that the run belongs to a fork. That fork is the condition that makes the token read-only. Note `"pull_requests": []`. **The run's own PR association array is empty.** The run is still a `pull_request` event from PR #26. That fact is load-bearing for the fix in section 3.2.

### 1.2 The step list: the comment step is the only failure

```
$ env -u LD_LIBRARY_PATH gh api "repos/michaelamoetoevsky/PDN/actions/jobs/126638857856" \
    --jq '{id,name,conclusion,steps:[.steps[]|{number,name,conclusion}]}'
{"conclusion":"failure","id":126638857856,"name":"Run Benchmarks","steps":[
 {"number":1,"name":"Set up job","conclusion":"success"},
 {"number":2,"name":"Checkout repository","conclusion":"success"},
 {"number":3,"name":"Install Rust toolchain","conclusion":"success"},
 {"number":4,"name":"Cache cargo registry","conclusion":"success"},
 {"number":5,"name":"Install criterion","conclusion":"success"},
 {"number":6,"name":"Verify benchmark targets exist and compile","conclusion":"success"},
 {"number":7,"name":"Run benchmarks","conclusion":"success"},
 {"number":8,"name":"Compare with baseline","conclusion":"success"},
 {"number":9,"name":"Upload benchmark results","conclusion":"success"},
 {"number":10,"name":"Comment benchmark results on PR","conclusion":"failure"},
 {"number":19,"name":"Post Cache cargo registry","conclusion":"skipped"},
 {"number":20,"name":"Post Checkout repository","conclusion":"success"},
 {"number":21,"name":"Complete job","conclusion":"success"}]}
```

Every benchmark step is `success`. Only step 10 fails. This run predates commit `15e1d37`, which added `continue-on-error`. On that run the 403 still marked the job failed. Confirmed present on master: `15e1d37 fix(ci): make benchmark PR comment non-fatal and grant write perms`.

### 1.3 The 403, verbatim from the job log

```
$ env -u LD_LIBRARY_PATH gh api --allow-escape-sequences \
    "repos/michaelamoetoevsky/PDN/actions/jobs/126638857856/logs"
```

Relevant lines follow. Timestamps are trimmed.

```
POST /repos/michaelamoetoevsky/PDN/issues/26/comments - 403 with id A800:1E03B8:2134AE:6D1752:6AEB4D79 in 226ms
RequestError [HttpError]: Resource not accessible by integration - https://docs.github.com/rest/issues/comments#create-an-issue-comment
    at fetchWrapper (/home/runner/work/_actions/actions/github-script/v9/dist/index.js:61509:11) {
  status: 403,
  request: {
    method: 'POST',
    url: 'https://api.github.com/repos/michaelamoetoevsky/PDN/issues/26/comments',
    headers: {
      accept: 'application/vnd.github.v3+json',
      'user-agent': 'actions/github-script ... octokit-core.js/7.0.6 Node.js/24',
      authorization: 'bearer [REDACTED]',
      ...
```

and the response headers, which name the permission that the token lacked:

```
'x-accepted-github-permissions': 'issues=write; pull_requests=write',
...
    data: {
      message: 'Resource not accessible by integration',
      documentation_url: 'https://docs.github.com/rest/issues/comments#create-an-issue-comment',
      status: '403'
    }
```

The API response says the request needed `issues=write; pull_requests=write`. The job **requested** those scopes. At the time, `benchmark.yml` on master had `permissions: {contents: read, issues: write, pull-requests: write}`. The fork run did not **receive** them. That is the limitation, reproduced end to end. The fix must move the comment out of the fork's token context. The fix must not request more scopes.

### 1.4 The artifact the reporter must read

```
$ env -u LD_LIBRARY_PATH gh api "repos/michaelamoetoevsky/PDN/actions/runs/41848512267/artifacts" \
    --jq '.artifacts[]|{id,name,size_in_bytes,expired}'
{"expired":false,"id":13338356144,"name":"benchmark-results","size_in_bytes":27759}
```

Job log line, same artifact:

```
Artifact benchmark-results successfully finalized. Artifact ID 13338356144
Artifact benchmark-results has been successfully uploaded! Final size is 27759 bytes. Artifact ID is 13338356144
```

So the run that cannot comment does still produce `benchmark-results`, of 27,759 bytes. The data the reporter needs is present even when the post is refused.

### 1.5 PR resolution: `pull_requests` is empty, `commits/{sha}/pulls` is not

```
$ env -u LD_LIBRARY_PATH gh api "repos/michaelamoetoevsky/PDN/actions/runs/41848512267/pull_requests"
gh: Not Found (HTTP 404)          # this endpoint does not exist

$ env -u LD_LIBRARY_PATH gh api "repos/michaelamoetoevsky/PDN/commits/30dabb147ac94b6df636dd6c041d7ef021bd3470/pulls" \
    --jq '.[]|{number,state,head_repo:.head.repo.full_name,base:.base.ref}'
{"base":"master","head_repo":"gabm0932/PDN","number":26,"state":"closed"}
```

`commits/{sha}/pulls` resolves the fork run to PR **#26**, with `head_repo: gabm0932/PDN`. That is a second, independent confirmation of the fork. The fix uses this fallback because `workflow_run.pull_requests` carries the same empty array as section 1.1.

---

## 2. The fix

Two files:

- **NEW** `.github/workflows/benchmark-report.yml` is a `workflow_run` workflow on `Benchmark` `completed`, with `permissions: {actions: read, pull-requests: write, issues: write}`. It downloads the run's `benchmark-results` artifact and posts the summary comment on the originating PR.
- **CHANGED** `.github/workflows/benchmark.yml`. The in-job `Comment benchmark results on PR` step is **REMOVED**, and the job's write scopes go with it. `permissions` is now `{contents: read}` only.

### 2.1 Removal compared to reduction and the reason

**Chosen: removal, not reduction.** The alternative keeps the step gated to same-repo PRs. That method posts a comment on same-repo PRs from `benchmark.yml` *and* again from `benchmark-report.yml` for the same run. The result is two comments and two code paths for one job. Removal of the step makes the `workflow_run` workflow the single path for comments on every PR, fork and same-repo PRs alike. Removal also lets the benchmark job drop `issues: write` + `pull-requests: write`. That drop is least privilege, because a job that only builds and uploads cannot write anything.

### 2.2 Why a `workflow_run` workflow is not the fork's context

`workflow_run` is dispatched by the **base** repository, on its default branch, when the named workflow completes. The token the reporting workflow gets is therefore a normal base-repo token. The scopes come from its own `permissions:` block. That token is not the read-only token handed to a `pull_request` run from a fork. Refer to section 1.3 for that token. That is the entire mechanism. There is no PAT, no `pull_request_target` with a checkout, and nothing that grants a fork-write primitive.

### 2.3 Why it is safe against fork code

This is the load-bearing property, and it is written into the workflow header so nobody "simplifies" it away later:

- **No checkout.** `benchmark-report.yml` has no `actions/checkout` step, and the header states that no one can add one. Nothing from the fork's tree is fetched.
- **No execution of fork-derived content.** The workflow reads exactly two items. Item one is the triggering run's metadata through the API, that is `id`, `event`, and `head_sha`. Item two is the `benchmark-results` artifact uploaded by the base repo's own `Benchmark` workflow. The artifact's *contents* are untrusted, because they quote fork-controlled benchmark output. The workflow only interpolates those contents **inside a fenced code block in a comment body**. Nothing is sourced, `eval`'d, or run.
- **Fixed artifact paths.** `benchmark.yml` uploads a fixed path list, that is `benchmark-output.txt` and `benchmark-baseline.txt`. The fork does not choose file names, and extraction is confined to `$GITHUB_WORKSPACE`.

### 2.4 Find the PR number and not `workflow_run.pull_requests` alone

```
1. run.event !== 'pull_request'            -> log "not a pull_request … no-op" and stop   (section 2.5)
2. run.pull_requests[0]                    -> use it if present
3. else repos.listPullRequestsAssociatedWithCommit(commit_sha = run.head_sha)
   -> prefer state == 'open', else data[0]
4. no PR found                             -> core.warning(…) and stop
```

Step 2 is the requirement's "triggering run's associated pull requests". Step 3 is mandatory because that array was **empty** on the fork run, as section 1.1 shows. At the same time `commits/{sha}/pulls` resolved it to #26, as section 1.5 shows. A fix that trusts step 2 alone posts nothing on the exact runs this change exists to fix.

### 2.5 The non-PR no-op

`run.event !== 'pull_request'` is checked **inside the script**, not as a job-level `if:`, so the log line always appears:

```
core.info(`Event is '${run.event}', not 'pull_request' - nothing to comment; no-op.`);
```

A job-level `if:` shows the job as `skipped` and prints nothing. Only the artifact-download step carries a step-level `if: github.event.workflow_run.event == 'pull_request'`. A push run therefore costs one `skipped` step and one log line. The master path is otherwise unchanged.

---

## 3. Permission and condition table

### 3.1 `permissions` blocks

| File | Scope | Value | Why it is needed | Verified how |
|---|---|---|---|---|
| `benchmark.yml` (job `benchmark`) | `contents` | `read` | clone + fetch base ref for the baseline comparison | local YAML read |
| `benchmark.yml` (job `benchmark`) | *(removed)* `issues: write`, `pull-requests: write` | — | were only for the now-deleted comment step; the fork token never had them (section 1.3) | local diff |
| `benchmark-report.yml` (workflow) | `actions: read` | | download the run's artifact (`download-artifact@v8` needs `actions:read` on the target repo, per its README) | action README + section 1.4 |
| `benchmark-report.yml` (workflow) | `pull-requests: write` | | read PR association / comment on the PR | required by the task and by the API (`x-accepted-github-permissions`) |
| `benchmark-report.yml` (workflow) | `issues: write` | | `POST /issues/{n}/comments` is the issues API | API response in section 1.3 |

No `contents:` is declared in `benchmark-report.yml`: the workflow reads no repository content, so it has no `contents` permission at all.

### 3.2 Every `if:` condition relied on

| File | Where | Condition | Effect |
|---|---|---|---|
| `benchmark.yml` | step `Compare with baseline` | `github.event_name == 'pull_request' && github.event.pull_request.base.repo.full_name == github.repository` | unchanged by this work; same-repo PRs only |
| `benchmark-report.yml` | step `Download benchmark results artifact` | `github.event.workflow_run.event == 'pull_request'` | skip the download on push/manual runs |
| `benchmark-report.yml` | script | `run.event !== 'pull_request'` (JS) | log the no-op line and return |
| `benchmark-report.yml` | script | `if (!pr)` (JS) | `core.warning` and return when no PR |
| `benchmark-report.yml` | script | `if (!fs.existsSync('benchmark-output.txt'))` (JS) | `core.warning` and return if the artifact was missing |

### 3.3 Trigger parameters

| Key | Value | Note |
|---|---|---|
| `on.workflow_run.workflows` | `["Benchmark"]` | must equal benchmark.yml's `name: Benchmark` (it does) |
| `on.workflow_run.types` | `[completed]` | fires once per Benchmark run, success or failure |

---

## 4. Run/PR IDs the logic reads, traced

| Source | Field | Value on the reproduced run | Used for |
|---|---|---|---|
| `context.payload.workflow_run` | `id` | `41848512267` | `run-id` for `download-artifact@v8`; comment footer link |
| `context.payload.workflow_run` | `event` | `pull_request` | the no-op gate (section 2.5) |
| `context.payload.workflow_run` | `head_sha` | `30dabb147ac94b6df636dd6c041d7ef021bd3470` | fallback PR lookup via `listPullRequestsAssociatedWithCommit` |
| `context.payload.workflow_run` | `pull_requests` | `[]` (empty) | primary PR lookup — **misses**, hence the fallback |
| `repos/{o}/{r}/commits/{head_sha}/pulls` | `[0].number` | `26` | the PR actually commented on |
| `context.repo` | `owner`/`repo` | `michaelamoetoevsky` / `PDN` | all API calls and the comment |

No other run, job, step, artifact or PR identifier is hardcoded anywhere in either workflow. The ids above appear only in this report as evidence.

---

## 5. Verified vs. UNVERIFIED

### Verified locally and provable here

- `benchmark.yml` and `benchmark-report.yml` both parse: `env -u LD_LIBRARY_PATH python3 -c "import yaml; yaml.safe_load(open(...))"` → OK.
- No trailing whitespace in either file.
- The embedded `github-script` body is syntactically valid JavaScript. The check is extraction from the YAML and a test with `node --check` inside an `async function` wrapper. The reason is that github-script@v9 evaluates the body in an async context, so top-level `await` is legal there. A bare `node --check` on the unwrapped body reports `await is only valid in async functions`.
- The workflow structure reads back correctly from the parsed YAML. The `benchmark` job has `permissions: {contents: read}` and 8 steps, with no comment step. The `report` job has `permissions: {actions: read, pull-requests: write, issues: write}` and 2 steps. The download step is gated on `github.event.workflow_run.event == 'pull_request'`.
- The limitation itself is reproduced from raw API + log evidence, as section 1 shows.

### UNVERIFIED facts that this environment cannot establish

- **That the comment actually appears on a fork PR.** A fork PR requires a fork and a push. This task forbids `git push`, and there is no fork available. A green local YAML parse says nothing about runtime behaviour. Treat "fork PRs now get the table" as **unproven** until a real fork run exists.
- **That `actions/download-artifact@v8` succeeds cross-run in this repo.** The `run-id` + `github-token` inputs are confirmed against the action's README. The `actions: read` scope is declared, but the cross-run download has not been executed.
- **That `listPullRequestsAssociatedWithCommit` returns the PR at run time.** It returned PR #26 for the reproduced head SHA, as section 1.5 shows. On a live open PR the `state` filter behaves differently from a merged one. The filter picks the `open` entry in that case. Unverified on an open PR.
- **That the master path is truly unchanged at runtime.** The logic reads as a no-op for `event == 'push'`. The job still starts a runner and logs the no-op line. Per the requirement that is the intent, and no one has observed it.
- **Action versions.** `download-artifact@v8` matches this repo's existing usage in `cross-compile.yml:186` and `release.yml:312`. The resolution of that tag is not verifiable offline.

---

## 6. How a maintainer with a fork can test it

1. Ensure `benchmark-report.yml` is on the default branch, `master`. `workflow_run` only fires for workflows that exist on the default branch.
2. Open or push to a PR that touches `crates/**`, `tests/**`, `Cargo.toml`, `Cargo.lock` or `.github/workflows/**`. This list is the `Benchmark` path filter. Make the push **from a fork**, and let the `Benchmark` workflow complete.
3. Expect a new run named **Benchmark Report**. Expect a job named **Report benchmark results on PR**. Expect a step named **Post benchmark summary comment**. The log lines are `Benchmark run <id>: event=pull_request head_sha=<sha>` then `Posted benchmark summary to PR #<n>.` On the fallback path, expect `workflow_run.pull_requests was empty; commit <sha> -> 1 PR(s)`. That line is expected for fork runs.
4. Expect a `## Benchmark Results` comment on the PR with the tail of `benchmark-output.txt` in a code block.
5. Sanity-check the no-op: push to `master` and confirm the same job logs `Event is 'push', not 'pull_request' - nothing to comment; no-op.` and posts nothing.
6. If step 3 fails with 403, the fix has not addressed the token context. The `x-accepted-github-permissions` header from the new run's log will name the scope that the token still lacks.
