# Muted text contrast floor, icons, and heading outline — accessibility scan

**Date:** 2026-10-31 18:20–18:35 UTC
**Routes scanned:** `/`, `/docs`, `/docs/layer-0-foundation`, `/docs/unified-scalability`, `/docs/trusted-execution`, `/docs/real-time-finality`, `/docs/autonomous-intelligence`, `/login`, `/operator`, `/appchains`, `/validator`, `/download`, `/legislation`, `/transact`, `/ultraverse`, `/learn`, `/send-ultra`
**Website revision at scan time:** local working tree of `e9540a4` (contrast) + `ecf9ec1` (icons, heading order); live re-scan on the deploy of `8b53129`
**Engine:** axe-core 4.11.1 (rules `color-contrast`, `heading-order`), automated only
**Verdict:** 0 violations, local and live

## Why axe alone was not enough

`DocPage` and the command-center terminals paint fixed radial-gradient and noise overlays. Where axe
cannot resolve the effective background it reports **`incomplete`, not `violation`** — 163 nodes on
`/`, 190 on `/validator`, 88 on `/download`. Trusting the violation count alone would have declared a
clean bill of health over text measuring 1.2:1.

The ladder below was therefore computed from WCAG relative luminance against the real composited
colours, and axe was used to confirm the result rather than to discover it.

## The sub-AA ladder

Platinum is `#F2F3F4` on ink-black `#010B13`; an alpha step composites with the row behind it.

| class | composite on ink | ratio on ink-black | ratio on a `bg-platinum/5` row | AA (4.5:1) |
|---|---|---:|---:|---|
| `text-platinum/10` | `#19222A` | 1.23:1 | 1.17:1 | fail |
| `text-platinum/20` | `#313940` | 1.69:1 | 1.62:1 | fail |
| `text-platinum/25` | `#3D454B` | 2.03:1 | 1.94:1 | fail |
| `text-platinum/30` | `#495156` | 2.45:1 | 2.34:1 | fail |
| `text-platinum/35` | `#555C62` | 2.92:1 | 2.78:1 | fail |
| `text-platinum/40` | `#61686D` | 3.51:1 | 3.34:1 | fail |
| `text-platinum/45` | `#6D7378` | 4.13:1 | 3.93:1 | fail |
| `text-platinum/50` | `#7A7F84` | 4.91:1 | **4.49:1** | pass on ink only |
| **`text-platinum-muted`** | `#82878C` (opaque) | **5.48:1** | **5.00:1** | pass |

The floor sits between `/45` and `/50`, so the reported `/30` was one of four failing steps — and
`/50`, which the docs footer had already adopted, only passes on plain ink. An opaque token is used
instead of a higher alpha step so the colour cannot re-composite with the row behind it; it measures
5.15:1 over the cyan hover wash as well.

## Result

- **`text-platinum-muted`** (`#82878C`) added to the `@theme` block in `src/styles/globals.css`.
- **198 labels across 52 files** moved from `/10`, `/20`, `/25`, `/30`, `/35`, `/40` and `/45` to the
  token. Borders, backgrounds and every text step at `/50` or above were left untouched — verified by
  diffing the `bg-platinum/*` and `border-platinum/*` token counts before and after: identical.
- **`/favicon.ico`**: returned 404 on every route except the standalone whitepaper reader, and no page
  declared an icon. The mark now ships as `src/app/icon.svg` with a `next.config.mjs` rewrite so the
  conventional crawler path resolves. Live: `200 image/svg+xml`, and `<link rel="icon" …>` is emitted
  in the document head.
- **Heading order**: four subheads sat at `h4` directly under an `h2` (three pillar pages), and the
  100-year milestone cards skipped `h3` under *The 100-Year Vision*. All are `h3` now; no visual weight
  changed.
- **Docs "coming soon" cards**: `opacity-60` on the container multiplied every child colour and left the
  cards' own category and `[ Encrypted ]` labels at **2.61:1**. The surface is dimmed instead of the text.
- **Verification:** 0 `color-contrast` and 0 `heading-order` violations on 17 local routes and on the
  six production routes re-scanned after deploy (`/`, `/docs`, three pillar pages, `/login`).
- **Regression guard:** `src/lib/contrast-floor.test.ts` scans `src/` and fails with file:line on any
  reintroduced alpha step below the floor, and asserts the token value. Proven by mutation: adding
  `text-platinum/30` to a scratch component failed the suite naming that path. `npm test` now runs in the
  deploy workflow, which previously ran lint, type-check and build only.

## Scope and limits

Automated axe scans plus computed luminance. This is not a manual screen-reader, keyboard-only,
browser-zoom, or mobile assessment, and it does not cover contrast of images, gradients, or video
content. `incomplete` results were counted but not resolved by axe; the ladder above is the substitute,
and it covers text colours only.

One earlier finding on `/docs/trusted-execution` (`.p-6 > .leading-relaxed`) did not reproduce on a
second scan with a longer settle. It was a mid-animation frame from the page's framer-motion fade-in,
not a static failure — recorded here because it was observed, not because it persists.

## Follow-up: measured verification above the floor (2026-10-31 19:05 UTC)

This record originally warned that `/50` on a raised container measures 4.49:1 and was "worth a targeted
pass". That number was arithmetic on an assumed background, and measurement disproved it. It is corrected
here rather than left standing.

**Method.** Every text node on 16 routes was measured in the browser against its real composited
background: the ancestor background layers were stacked with alpha compositing, inherited container
`opacity` was multiplied in, pages were scrolled with `prefers-reduced-motion: reduce` so no element sat
mid-animation, and `aria-hidden` subtrees were excluded. Both the AA threshold (4.5:1) and the large-text
threshold (3:1) were applied per node.

The measurement was itself validated before its verdict was trusted, by injecting two controls onto a
`#0C1620` surface:

| injected control | measured | classified |
|---|---:|---|
| `rgba(242,243,244,0.30)` | 2.54:1 | correctly flagged |
| `#82878C` (the token) | 5.03:1 | correctly passed |

**Result:** 0 sub-AA text nodes across `/`, `/docs`, the four pillar pages, `/login`, `/operator`,
`/appchains`, `/validator`, `/download`, `/ultraverse`, `/learn`, `/transact`, `/send-ultra` and
`/legislation`. The `/50` labels sit on near-ink surfaces rather than raised rows, so they already clear
AA. `text-platinum/50` is therefore unchanged, and the guard test continues to forbid anything below it.

## Known follow-ups
- Disabled controls are exempt from the AA contrast requirement. The two disabled buttons in
  `appchains/AnchorNowControl` were raised to the token anyway because they carry readable status text.
- `/favicon.ico` is served as SVG through a rewrite. Clients that require a real ICO container (some
  legacy Windows shell surfaces) would need a rasterised icon, which cannot be generated on this host.
