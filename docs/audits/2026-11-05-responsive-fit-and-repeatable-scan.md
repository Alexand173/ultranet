# Responsive fit, repeatable scan, and the violations it found

**Date:** 2026-11-05 15:40–16:00 UTC
**Engine:** axe-core 4.14.0, full default rule set, plus a purpose-built composite-contrast measurement
**Build:** local dev of the working tree that became this commit; live re-scan after deploy
**Verdict:** 18 routes, 0 violations — including the whitepaper reader's embedded document

## What was actually wrong with "content only visible when zoomed out"

Three separate defects, none of which was the page zooming itself.

**1. The nav cluster needed 1331 px but appeared from 1024 px.** The row offers 1232 px inside
`max-w-7xl` (1280 − 48 padding), the logo takes 136 px and the nine labels plus the vault button took
1195 px at `gap-8` and `tracking-[0.2em]`. Because `body` sets `overflow-x: hidden`, the excess was
clipped rather than scrollable: from 1024–1444 px the `VAULT_LOGIN` button was simply cut off and
unreachable, and `SEND ULTRA` wrapped onto two lines, hiding even more of the overflow.

Fixed by compressing rather than hiding: `gap-6` / `tracking-[0.1em]` / `px-5` bring the cluster to
**1054 px**, and the desktop cluster now starts at `xl` (1280) where it fits with margin
(cluster right edge 1241 vs 1265 available). Below 1280 the hamburger menu takes over. `whitespace-nowrap`
was added so a label can never wrap and silently shrink the row.

**2. Grid children could not shrink below their `<pre>` min-content width.** At 390 px the `/download`
cards pushed 110 px past the viewport and the `/validator` runbook cards 564 px, because grid items
default to `min-width: auto`. `min-w-0` on the cards (and on the two-column code grid) lets the `<pre>`
scroll internally instead of stretching its parent.

**3. A terminal header was clipped by its own frame.** `Ledger // Recent_Committed_Transactions` overhung
the frame by 22 px at 1920. The title now truncates with an ellipsis and exposes the full string as a
`title`. This affects every `TerminalFrame`, so it is fixed once for all of them.

**Verified fit:** `/`, `/docs`, `/validator`, `/download`, `/learn`, `/legislation` at 360, 390, 430, 768,
900, 1024, 1180, 1280, 1366, 1440 and 1920 px — no horizontal scrolling, no clipped content, nav correct
at every width (desktop ≥1280 with 24 px of margin, menu below).

### Measurement caution worth recording

The first pass reported no overflow anywhere, and was wrong. The automation browser ran at
**80% zoom** (`devicePixelRatio: 0.8`), so asking for a 1440 px viewport produced a 1800 px CSS viewport
— every "small screen" test silently tested a larger one. A second browser instance at 100% then
over-tested in the opposite direction. Any viewport measurement must assert
`devicePixelRatio === 1` and `documentElement.clientWidth ≈ requested width` before its result is
believed.

## The repeatable scan, and what it caught

`npm run a11y` (`website/scripts/a11y-scan.mjs`) runs axe with its full default rule set over 18 routes
and **descends into embedded frames**, so `/docs/whitepaper` is now scanned as two documents
(`frames=2`). It exits non-zero on any violation, can emit JSON, and resolves its Chromium binary from
`A11Y_CHROME_PATH`, then system locations, then Playwright's cache. Dependencies are `axe-core` and
`playwright-core` only — no browser download at install time, so server-side `npm ci` is unaffected.

On its first run it found six violation groups that the earlier per-rule scans had missed:

| rule | route | cause | fix |
|---|---|---|---|
| `color-contrast` ×1 | `/download`, `/transact`, `/send-ultra` | the coin label `Protocol_Illustration` at 8 px and `text-cyan-glow/40` measured **3.24:1** (`#076D71`) | `text-cyan-glow/60` → ~5.9:1 |
| `label-content-name-mismatch` ×1 | `/validator` | button labelled "Copy the full Genesis multiaddr" while its visible text reads "Copy multiaddr" | name now interpolates the visible label |
| `label-content-name-mismatch` ×3 | `/download` | link labelled `Download <file>` while its visible text reads "Download archive" | `Download archive: <file>` |
| `label-content-name-mismatch` ×1 | `/ultraverse` | brand link labelled "UltraVerse services" while it visibly reads "UltraVerse / on UltraNet" | redundant `aria-label` removed; the visible text names it |

`label-content-name-mismatch` is WCAG 2.5.3 (Label in Name) — a speech-input and voice-control failure,
not merely a lint result. All four are fixed; the re-scan reports **0 violations on 18 routes**.

## Focus-visible review on the code blocks

The six focusable `<pre>` blocks introduced on 2026-10-31 were checked at 390, 768, 1024 and 1440:

- `:focus-visible` is true after keyboard focus and the ring is a real 2 px spread
  (`oklab(0.924 -0.012 0.119 / 0.6)`), not a transparent placeholder.
- The ring takes the block's own accent — amber on the admin-token block, cyan on the runbook — so it
  reads as a state change rather than decoration.
- No ancestor clips the ring in practice: the page wrapper is `overflow-x: clip`, but every block is
  inset inside padded cards, so the ring never reaches that edge.
- Blocks are genuinely scrollable when narrow (`scrollsHorizontally: true` at 1024 on the runbook),
  which is the behaviour the ring is advertising.

## Scope and limits

Automated axe (full rule set) plus programmatic focus and geometry checks. No manual screen-reader pass
is recorded anywhere in this repository — see
[MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md), which is the plan for closing
that gap and lists the expected findings (notably the missing skip link).

`incomplete` results remain high on gradient-heavy pages (188 on `/validator`) because the effective
background cannot be resolved automatically; those are covered by the composite measurement method
recorded in [2026-10-31-contrast-floor-and-icons.md](./2026-10-31-contrast-floor-and-icons.md).
