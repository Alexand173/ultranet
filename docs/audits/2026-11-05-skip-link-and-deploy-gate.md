# Skip link, accessibility deploy gate, and why the Windows suite was red

**Date:** 2026-11-05 16:30–17:00 UTC
**Build:** `61d65f0`, scanned locally and by the deploy workflow itself
**Verdict:** 18 routes, 0 violations; skip link verified by real key presses on six routes

## Skip link

Nine navigation links preceded the content on every route, so a keyboard or screen-reader user had to
tab through all of them before reaching the page body. This was the finding
[MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md) predicted as the most likely
manual blocker, so it was fixed before the manual pass instead of after it.

- `components/ui/SkipToContent.tsx` renders a `Skip to content` link as the **first focusable element in
  the document**, before the noise layer, the command search and the nav. It is `sr-only` until focused,
  at which point it becomes a fixed, bordered, cyan control at the top-left — no visual change otherwise.
- The target is `#main-content`, added to **all 25 `<main>` occurrences across 15 files** (several files
  branch into different mains: `TransactionPage` 7, `OperatorConsole` 3, `AppChainsConsole` 3).
  `tabIndex={-1}` makes them programmatically focusable without adding a tab stop.
- `globals.css` gives `#main-content` `scroll-margin-top: 5rem` so the jump clears the fixed 80 px nav
  instead of parking the first heading underneath it, and suppresses the focus ring on the target itself
  (the scroll is the affordance; a ring around a whole page is noise). The link keeps its own ring.

**Evidence (real `Tab` then `Enter`, 1366×860):**

| route | first tab stop | after `Enter` | heading top |
|---|---|---|---|
| `/` | Skip to content (visible 16,16 · 179 px) | `activeElement#main-content`, `#main-content` | 154 |
| `/docs` | same | same | 136 |
| `/validator` | same | same | 136 |
| `/send-ultra` | same | same | 165 |
| `/ultraverse` | same | same | 118 |
| `/docs/whitepaper` | same | same | 184 |

Exactly one `#main-content` exists per route in every case, and every heading sits well below the 80 px
nav bar.

## Deploy gate

`npm run a11y` now runs inside `deploy-website.yml` at step 13 of 18 — **after the build, before
packaging**, so a violation blocks the deploy rather than being reported after it. The step starts the
built app on `:3000`, polls until it answers, then scans it, which means the gate covers the exact bytes
about to ship. Chromium is fetched with a one-shot `npx playwright@1.64.1 install chromium` and cached by
lockfile hash; `playwright-core` still ships no browser, so the server-side `npm ci` is unchanged.

This is not theoretical: the coin-label contrast regression that went live on 2026-11-05 would have been
stopped by this step.

**The gate was made to fail on purpose,** because a gate that has only ever been seen passing is not known
to work. A throwaway page with an unlabelled image, an empty link and low-contrast text produced:

```
FAIL /bad.html  frames=1 incomplete=0 violations=3
       color-contrast ×1 — p
       image-alt ×1 — img
       link-name ×1 — a
SCANNER EXIT CODE: 1
```

Exit 1 on the scanner is what fails the workflow step, and the step sits before packaging, so the deploy
stops instead of shipping the regression. (The first measurement of this read `EXIT CODE: 0` — that was the
exit status of the `tail` in the pipeline, not the scanner. Measure the scanner directly, or with
`pipefail`.)

## Aside: why the Windows suite was red

Unrelated to the website work, but it was the first thing asked about, so the answer belongs on record.
The failing step uploaded its log, whose tail was conclusive:

```
error[E0063]: missing field `max_claims` in initializer of `FaucetConfig`
  --> tests\faucet_api_test.rs:54:18
error: could not compile `UltraNet` (test "faucet_api_test")
```

`9c6370f` added `FaucetConfig::max_claims` and updated `tests/faucet_service_test.rs` and
`src/faucet/preview.rs`, but missed the third initializer. **No test ever ran** — the target did not
compile. `cargo check --locked --all-targets` failed identically on Linux, so the failure was never
Windows-specific; the 20-minute runtime was Windows compiling every dependency before reaching the error.

The durable fix is not the one line: **no Linux job compiled test targets**, so drift in `tests/**` was
invisible until a 20-minute Windows run. `windows-tests.yml` now has a `Linux check (all targets)` job
running `cargo check --locked --all-targets`, which catches the same drift in a couple of minutes on the
platform the code is developed on.

Both jobs on `04e7e03`: **`Windows x64 Rust tests` success, `Linux check (all targets)` success** — the
first green Windows run since the drift landed.

The suite was then run locally one target at a time (`-j 1 --test-threads=1`) because compiling every test
target at once crashes this workstation:

| target | result | time |
|---|---|---|
| `--lib` | 126 passed, 1 ignored | 314.9 s |
| `--test faucet_api_test` | 2 passed | 0.02 s |
| `--test faucet_service_test` | 3 passed | 0.03 s |
| `--test integration_test` | 10 passed | 332.5 s |
| `--test dag_stress_test` | 1 passed | 0.54 s |

142 passed, 0 failed. That is higher than the 114 recorded on 2026-10-31; tests have been added in the
meantime and the difference is not itemised here, so the two numbers are not comparable as a like-for-like
regression signal — only the 0 failures is.

## Scope and limits

Automated axe (full rule set), plus programmatic focus and geometry checks driven by real key presses.
**No manual screen-reader pass exists** — that is still the largest gap, and
[MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md) remains the plan for it. Mobile
Safari with VoiceOver is entirely unmeasured.

Two pre-existing issues surfaced while verifying and were deliberately not fixed here:

- Next.js warns that `html { scroll-behavior: smooth }` should be paired with
  `data-scroll-behavior="smooth"` on `<html>` for future route transitions.
- The dev overlay reports a hydration mismatch from the decorative `VisualNoise` component, which calls
  `Math.random()` during render. It is server/client drift in decoration, visible only in development.
