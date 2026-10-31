# Accessibility audit records

Automated axe-core scans of the deployed website, kept so a later reader can tell what was measured,
on which revision, and what the scan could not cover. These are automated results only — none of them
substitutes a manual screen-reader, keyboard-only, zoom, or mobile assessment.

| Date (UTC) | Surface | Route | Verdict | Record |
| --- | --- | --- | --- | --- |
| 2026-10-21 14:41 | Whitepaper reader | `/docs/whitepaper` | 0 violations | [2026-10-21-whitepaper-reader-accessibility.md](./2026-10-21-whitepaper-reader-accessibility.md) |

## How a record is produced

1. Run the scan against the deployed or local route in a browser with axe-core.
2. Record the route, the engine version, the verdict, and the website revision
   (`git log -1 --format=%h -- website` at the time of the scan).
3. Keep the "Scope and limits" section. An empty limits section means the scan was over-claimed.
4. List any finding the scan produced but the change did not fix, so the record does not imply
   a clean bill of health.
