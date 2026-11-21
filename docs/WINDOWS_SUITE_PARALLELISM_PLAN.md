# Windows suite: the experiment to parallelise the gate

**Status: experiment designed, not run. The gate stays serial until the decision rule below says
otherwise.** Nothing in `.github/workflows/windows-tests.yml` changes as part of this document.

## 1. What is true today

The gate job `Windows x64 Rust tests` runs one step, `Run complete Windows test suite serially`:

```bash
cargo test --target x86_64-pc-windows-gnu --locked --all-targets -- --test-threads=1
```

| Measurement | Serial gate (`windows-tests.yml`) | Parallel release job (`release.yml`) |
|---|---|---|
| Suite step | 4429 s (`44877932589`), 5597 s (`44885315754`) | 3300 s (`44884199956`) |
| Whole job | 4730 s / 5897 s | 3540 s |
| Setup | 281 s / 286 s | 91 s |

Two serial runs of the same suite on the same runner class differ by **1168 s (26%)**. The parallel
release job has already run the same commit range green at 3300 s. So parallelism is not known to
work — it is known to have worked **once**, on a different workflow, and repeatability is the open
question.

**The scenario is narrower than "run tests in parallel".** `cargo test` runs test *binaries* one after
another; `--test-threads` sets threads **within** a binary. The flag therefore changes intra-binary
concurrency only, which is exactly the dimension the two historical failures live in.

## 2. Why this is worth an experiment rather than a judgement call

Serial was chosen under uncertainty, not evidence: two failures looked like concurrency and the
runner has two vCPUs. Both facts are consistent with "the tests are racy" *and* with "those two tests
were racy regardless of the flag". The cost of staying serial is concrete: roughly **39 minutes per
push to `main`** (5897 s vs 3540 s), paid on every gated change.

The cost of guessing wrong is worse than the cost of waiting: a flaky gate teaches people to re-run
until green, and a re-run culture eventually merges a real failure.

## 3. Hypothesis, stated so it can fail

> With the toolchain pinned to 1.97.0, running the gate suite with an explicit `--test-threads=2`
> completes green on 3 of 3 runs and reduces the suite step by at least 25% against the serial
> baseline, with no failure that is not one of the two known racy tests.

Nothing in that sentence is unfalsifiable: 3 runs, a threshold, and a named exception list.

## 4. Design

### Arms

| Arm | Flag | Why |
|---|---|---|
| A (control) | `--test-threads=1` | today's gate |
| B | `--test-threads=2` | matches the runner's two vCPUs, but **explicit** |

Explicit `2`, not "default": the default is `num_cpus`, so an unqualified flag silently changes
meaning when GitHub re-sizes the runner. An explicit number keeps the experiment's result
interpretable six months later.

### How the arms are produced from one branch

Add a `workflow_dispatch` input to `windows-tests.yml` and use it in the command. The default stays
`1`, so **merging this input alone does not change gate behaviour** — it only makes the experiment
runnable:

```yaml
on:
  workflow_dispatch:
    inputs:
      test_threads:
        description: "Threads per test binary (1 = the gate's serial setting)"
        required: false
        default: "1"
```

```yaml
run: |
  set -o pipefail
  cargo test --target x86_64-pc-windows-gnu --locked --all-targets \
    -- --test-threads=${{ inputs.test_threads || 1 }} 2>&1 | tee windows-test.log
```

The `|| 1` is mandatory, not decoration: `inputs` is empty on `push` and `pull_request`, and without
the fallback every ordinary gate run would expand to `--test-threads=` and fail on an argument parse.
It is the same trap as `REQUESTED_COMMIT_SHA: ${{ inputs.commit_sha || github.sha }}` in the deploy
workflow, which is why the input defaults to `1` rather than to "whatever the runner has".

If this is adopted later, the step name loses its "serially" and becomes `Run complete Windows test
suite`.

### Controls

- **One variable.** Both arms come from the same commit on the same branch; only the input differs.
- **Fixed toolchain.** Already pinned to 1.97.0 (`rust-toolchain.toml`), so a compiler release cannot
  masquerade as an arm effect.
- **Interleaved order: A, B, A, B, A, B.** Not AAA BBB. Runner performance drifts over a morning, and
  clustering one arm in time is how a cache-warmth difference becomes a fake 20% win.
- **Cache state recorded per run** (restore duration), because the 192 s cold restore is comparable to
  the effect being measured.
- **Full commit SHA** in every dispatch — the deploy workflow now enforces 40 characters; do the same
  here rather than trusting memory.
- **Every run is recorded, including re-runs.** A retried green run is a flake, not a pass.

### Metrics, per run

1. Suite step duration (the primary outcome).
2. Whole job duration.
3. Green or red, and for red: the test name, and whether it is in the known pair.
4. `test result:` lines from the log, so a run that "passed" with fewer tests is visible as a
   collection change rather than a win.

### Decision rule, written before the data arrives

| Observation | Decision |
|---|---|
| 3/3 green **and** median step ≥ 25% faster | adopt `--test-threads=2`, update §1 of `ci-windows.md` |
| 3/3 green **and** < 25% faster | keep serial: the minutes do not pay for the lost determinism |
| 1 red, and it is the sled `failed to allocate Arc` or the loopback start-up race | go to the hybrid arm (§5) |
| 1 red and it is anything else | keep serial; record the failure in `ci-logs.md` as a new finding |
| 2+ red | abandon; the suite is not parallelisable as it stands |

## 5. Hybrid arm, only if the decision rule sends us there

Keep the known-racy targets serial and everything else at 2 threads. This means giving up the single
`--all-targets` invocation, because thread settings are per cargo invocation:

```bash
cargo test --target x86_64-pc-windows-gnu --locked --lib -- --test-threads=2
cargo test --target x86_64-pc-windows-gnu --locked --test integration_test -- --test-threads=1
# ... one line per target whose tests are racy
```

That fragmentation is a real cost: `--all-targets` is what guarantees a new test file is picked up
without anyone remembering to add it. A hybrid gate that misses targets silently is worse than a slow
one, so any hybrid must enumerate targets from `cargo test --no-run --message-format=json` rather than
a hand-maintained list. Record this before attempting it.

## 6. Out of scope, and why

- **A matrix of two runners, half the suite each.** Potentially halves wall clock again, but doubles
  runner minutes, needs a shared cache strategy, and turns one green check into two. Revisit only if
  the parallel arm does not clear the 25% bar.
- **`cargo-nextest`.** A new tool in the gate, a new failure mode, and a per-target config to
  maintain. Not before the flag experiment runs.
- **Editing tests to remove the races.** Right thing eventually, unrelated to this measurement, and it
  would contaminate the comparison.

## 7. Cost

Six dispatches at roughly 60-100 minutes: **about 8 hours of wall clock**, and the corresponding
runner minutes. No production exposure: `windows-tests.yml` gates merges, it does not deploy, and the
experiment runs from a scratch branch that is never merged in its experimental shape.

## 8. Definition of done for the experiment

- [ ] `test_threads` input exists with default `1`, and a dispatch with no input behaves exactly like
      today's gate.
- [ ] Six recorded runs, interleaved, same commit, with durations and cache state in one table.
- [ ] A decision written down against the rule in §4, including the case where the answer is "keep
      serial".
- [ ] `docs/runbooks/ci-windows.md` §1 updated with whichever number becomes the baseline, and its
      "the suite runs serially" paragraph either kept or replaced with the reason it changed.
