# Reading a CI failure whose log appears empty

`gh run view <run-id> --log-failed` can **exit 0 and print nothing** for a run that failed. This happened on
both `deploy-website.yml` failures on `main` (2026-11-15), which is why that workflow stayed red for five
days with no diagnosis: the command looked like it had been answered, and the failure looked unexplained.

The logs were there the whole time. Read the structure first, then the log per job.

## 1. Structure — always available, even when text is not

```bash
gh api repos/Alexand173/ultranet/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | "JOB \(.name): \(.conclusion)",
        (.steps[]? | "  \(.number). \(.name) -> \(.status)/\(.conclusion // "-")")'
```

This names the failing step by number even if no log text can be fetched. It is where to start, not where to
finish: `step 8 -> failure` tells you which step, and every later step reading `skipped` tells you the run
stopped there rather than limping on.

## 2. Log per job, through the API

```bash
gh api repos/Alexand173/ultranet/actions/jobs/<job-id>/logs > /tmp/job.log
grep -nE "error|FAIL|AssertionError|denied|exit code" /tmp/job.log
```

`gh run view --log` and the per-job API route do not share a code path, which is why one can be empty while
the other is complete. If one yields nothing, try the other before concluding the logs are gone.

## 3. Worked example: the two 2026-11-15 failures

Both runs show one job, `Build and deploy website`, ending in `failure`. They are **not the same fault**:

| Run | Duration | Failing step | Message | Cause |
|---|---|---|---|---|
| `43734551644` | 44 s | 8. Test website | `AssertionError: use text-platinum-muted (5.48:1) instead of an alpha step that measures below 4.5:1` | three `text-platinum/45` spans in `components/appchains/`, below the WCAG AA floor — **code**, fixed |
| `43720284113` | 3 m 19 s | Upload deployment inputs | `scp: /var/lib/<service>/staging/<sha>.tar.gz: Permission denied` | the deploy user cannot write the staging directory on the host — **server permissions, owner action** |

The second one never reached the deployment itself: steps 18 (guarded VPS deployment) and 19 (verify public
website) were skipped. So "the website deploy is broken" described two different problems, and only one of
them was in this repository.

**The permission failure was closed on 2026-11-21.** `namei -l /var/lib/ultranet-deploy/staging` on the node
showed the parent directory correct and the leaf wrong:

```
drwxr-x--- ultranet-deploy ultranet-deploy  /var/lib/ultranet-deploy
drwxr-x--- root            root             /var/lib/ultranet-deploy/staging
```

The deploy account could traverse the staging directory but not create a file in it. `chown
ultranet-deploy:ultranet-deploy` plus `chmod 0750` on that one directory, verified by touching a file as
that user, and the next run (`44889603109`) passed *Upload deployment inputs* in 3 s and finished green
through *Verify public website* in 5 m 21 s. The lesson is in the shape of the output, not the fix: `namei`
names the exact component of a path that denies, which `ls -l` on the full path does not.

**Steps are cited by name here, not by number.** Adding a step at the top of this workflow renumbered every
later one — which happened on 2026-11-21 when *Reject malformed deployment inputs* was inserted as step 1 —
so a number in a runbook silently starts pointing at the wrong step.

## 4. Details worth knowing when reading these logs

- Secrets print as `***`. Their presence is not evidence they are valid; the workflow's own
  *Reject malformed deployment inputs* step fails loudly, naming any missing secret, and it passed in both
  runs. Read that step's messages literally: it reports names, never values.
- A `cancelled` conclusion is not a failure: `concurrency: cancel-in-progress` supersedes a run when a newer
  commit lands on the same ref. Check the `event` and `headSha` before chasing it.
- Duration is a clue: a 44-second failure never built anything, so it is upstream of the build.
- **`commit_sha` must be the full 40-character SHA.** That value is handed to `actions/checkout` as `ref`,
  and a short SHA reads as a branch name: the fetch becomes `+refs/heads/cdc59d5*`, fails three times, and
  the run dies at *Check out selected commit*. Run `44889235144` did exactly that on 2026-11-21 with
  `commit_sha=cdc59d5`, which is why the format check now runs **before** checkout: *Reject malformed
  deployment inputs* is step 1 and fails in seconds, naming the value it refused. Use `git rev-parse` and
  pass the result.

## 5. Measured durations, 2026-11-21

`deploy-website.yml`, run `44889603109` (commit `cdc59d5`), green end to end in **5 m 21 s**. The step
durations are worth having because they say where the time is when something looks slow:

Durations are ordered by when they ran, but steps are named: these numbers were measured before the 2026-11-21
renumber, and would otherwise be off by one from here on.

| Step | Time | Reading |
|---|---|---|
| Install website dependencies | 49 s | the single largest build-side cost |
| Build website | 28 s | |
| Accessibility gate | 60 s | second largest; Chromium install is a further 10 s |
| Upload deployment inputs | 3 s | fails in 3 s when the staging directory denies — the same order as a real upload |
| Run guarded VPS deployment | 131 s | `npm ci`, lint, type-check, build, whitepaper verify, restart |
| Verify public website | 2 s | |

*Lint website*, *Type-check website* and *Test website* together are 12 s, so a red run there is a real
finding rather than a timing artefact. *Run guarded VPS deployment* being 131 s means the server-side rebuild
dominates the deployment; if it grows, look at `npm ci` on the host before anything else.

After the 2026-11-21 split, both paths were exercised in the same hour:

| Run | Input | Outcome |
|---|---|---|
| `44895124879` | push to `main` | green end to end in **3 m 34 s** (214 s, warm cache — the earlier run was 321 s), through *Verify public website* |
| `44895129990` | dispatch with `commit_sha=cdc59d5` | failed at *Reject malformed deployment inputs* in **0 s**, checkout and everything after it `skipped` |

The failing run's whole message was one line:

```
::error::commit_sha must be a 40-character commit SHA, but got 'cdc59d5'. Resolve a short SHA with: git rev-parse <short sha>
```

That is the difference the ordering buys: one second and a line naming the value, instead of three failed
fetches of `+refs/heads/cdc59d5*` at checkout.
