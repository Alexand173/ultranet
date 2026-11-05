# Accessibility audit records

Automated axe-core scans of the deployed website, kept so a later reader can tell what was measured,
on which revision, and what the scan could not cover. These are automated results only — none of them
substitutes a manual screen-reader, keyboard-only, zoom, or mobile assessment.

| Date (UTC) | Surface | Route | Verdict | Record |
| --- | --- | --- | --- | --- |
| 2026-10-21 14:41 | Whitepaper reader | `/docs/whitepaper` | 0 violations | [2026-10-21-whitepaper-reader-accessibility.md](./2026-10-21-whitepaper-reader-accessibility.md) |
| 2026-10-31 18:20 | Muted text contrast floor, icons, heading order | 17 routes incl. `/`, `/docs`, `/login` | 0 violations (local and live) | [2026-10-31-contrast-floor-and-icons.md](./2026-10-31-contrast-floor-and-icons.md) |
| 2026-10-31 19:15 | Landmarks and keyboard reachability (all axe rules) | 17 routes incl. `/docs`, `/validator`, `/download` | 0 violations (local and live) | [2026-10-31-landmarks-and-keyboard-reachability.md](./2026-10-31-landmarks-and-keyboard-reachability.md) |
| 2026-11-05 15:55 | Responsive fit, repeatable scan, label-in-name fixes | 18 routes incl. the reader's embedded frame | 0 violations; no clipping 360–1920 px | [2026-11-05-responsive-fit-and-repeatable-scan.md](./2026-11-05-responsive-fit-and-repeatable-scan.md) |
| 2026-11-05 16:55 | Skip link, accessibility deploy gate, Windows suite root cause | 18 routes + 6 skip-link routes | 0 violations; skip link verified by keyboard | [2026-11-05-skip-link-and-deploy-gate.md](./2026-11-05-skip-link-and-deploy-gate.md) |
| 2026-11-05 19:20 | Hydration determinism, smooth scroll, new hydration check | 18 routes, dev build | 0 hydration messages; detector proven to fail; 0 violations | [2026-11-05-hydration-determinism.md](./2026-11-05-hydration-determinism.md) |

**Planned, not yet performed:** the manual screen-reader pass. See
[MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md). No manual NVDA or VoiceOver
result exists in this repository; everything above is automated plus programmatic keyboard/geometry
checks.

## Running the scan

```bash
cd website
npm run a11y                                        # http://localhost:3000
npm run a11y -- --base-url https://dashboard.ultranetwork.cc
npm run a11y -- --json a11y-report.json             # machine-readable
```

It runs axe-core's full default rule set over every route **and into embedded frames**, so
`/docs/whitepaper` is audited as two documents. It exits non-zero on any violation, which makes it
usable as a gate. Set `A11Y_CHROME_PATH` if no system Chrome is found.

## Running the hydration check

Axe cannot see a hydration mismatch: React reports it only in development, only once, and only to the
browser console. That is what `npm run hydration` is for, and it must be pointed at a dev server:

```bash
npm run dev              # one terminal — production builds report nothing
npm run hydration        # 18 routes, exits 1 on a mismatch
```

It shares its route list and browser resolver with the accessibility scan, so the two cannot cover
different surfaces. See [2026-11-05-hydration-determinism.md](./2026-11-05-hydration-determinism.md).

## Refreshing the screen-reader expectations

The manual audit checklist carries per-route expectations (title, heading outline, landmark chain, live
regions, first tab stops). They are generated from the running app, not remembered:

```bash
npm run dev
npm run sr:inventory -- --json /tmp/sr-inventory.json
```

It is a description, not a pass — it cannot judge speech. If its output disagrees with the checklist, the
checklist is stale. See [MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md).

## How a record is produced

1. Run the scan against the deployed or local route in a browser with axe-core.
2. Record the route, the engine version, the verdict, and the website revision
   (`git log -1 --format=%h -- website` at the time of the scan).
3. Keep the "Scope and limits" section. An empty limits section means the scan was over-claimed.
4. List any finding the scan produced but the change did not fix, so the record does not imply
   a clean bill of health.
