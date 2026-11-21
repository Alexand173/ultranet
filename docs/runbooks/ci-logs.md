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
| `43720284113` | 3 m 19 s | 17. Upload deployment inputs | `scp: /var/lib/<service>/staging/<sha>.tar.gz: Permission denied` | the deploy user cannot write the staging directory on the host — **server permissions, owner action** |

The second one never reached the deployment itself: steps 18 (guarded VPS deployment) and 19 (verify public
website) were skipped. So "the website deploy is broken" described two different problems, and only one of
them was in this repository.

## 4. Details worth knowing when reading these logs

- Secrets print as `***`. Their presence is not evidence they are valid; the workflow's own
  *Validate deployment inputs* step exists to fail loudly when one is missing, and it passed in both runs.
- A `cancelled` conclusion is not a failure: `concurrency: cancel-in-progress` supersedes a run when a newer
  commit lands on the same ref. Check the `event` and `headSha` before chasing it.
- Duration is a clue: a 44-second failure never built anything, so it is upstream of the build.
