# Whitepaper reader — accessibility scan

**Date:** 2026-10-21 14:41 UTC
**Route:** `/docs/whitepaper` (`http://localhost:3000/docs/whitepaper`)
**Website revision at scan time:** `877fca9` — *Add fresh staging Gate 1 checklist and whitepaper fixes*
**Engine:** axe-core 4.11.1, automated only
**Verdict:** 0 violations

The revision is the `website/` tip as of the scan time (`git log --before=2026-10-21T16:41Z -1 -- website`),
which is what CI had deployed that day. It is recorded so a later reader can tell which build was measured.

## Result

- **Violations:** 0
- **Reader route:** loaded successfully; no console errors or failed network requests on the reader page.
- **Document frame:** titled iframe; generated HTML uses `lang="en"`, a named main/article landmark, and a descriptive document title.
- **Reading controls:** progress is exposed as a named progressbar with current value and live text; code blocks are keyboard focusable; each task-list checkbox has a specific accessible name; each of the 9 Mermaid diagrams has a distinct caption/ARIA reference and descriptive role label.
- **PDF link:** GET `/docs/ultranet-whitepaper.pdf` returned HTTP 200 as `application/pdf` (3,232,992 bytes). The exported PDF catalog declares 43 pages.

## Scope and limits

This was an automated axe scan of the rendered reader route and its initial embedded-document state, plus route/PDF smoke checks. It is not a manual screen-reader, keyboard-only, browser-zoom, or mobile accessibility assessment.

The initial standalone HTML inspection (before adding explicit icon metadata) emitted one non-critical root `/favicon.ico` 404. The generator now links the existing `/images/ultra-coin.png` brand asset; after reload, the standalone document requested that PNG successfully (HTTP 200) and issued no new `/favicon.ico` request. The reader route audit itself reported no failed requests or accessibility violations.

## Known follow-ups

- **Resolved 2026-10-31.** The site had no `app/icon.*` or `public/favicon.*` route, so pages other
  than the reader requested `/favicon.ico` and received a 404. The brand mark now ships as
  `app/icon.svg` with a `/favicon.ico` rewrite; see
  [2026-10-31-contrast-floor-and-icons.md](./2026-10-31-contrast-floor-and-icons.md).
- **Resolved 2026-10-31.** The shared docs footer carried a serious `color-contrast` failure
  (`text-platinum/30`, 2.45:1) on every `DocPage` route. It was raised to `/50` (4.91:1) first, then
  the whole sub-AA ladder was replaced by the opaque `platinum-muted` token; see
  [2026-10-31-contrast-floor-and-icons.md](./2026-10-31-contrast-floor-and-icons.md).
