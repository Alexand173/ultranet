# Landmarks and keyboard reachability — accessibility scan

**Date:** 2026-10-31 19:10–19:25 UTC
**Routes scanned:** 17 (`/`, `/docs`, four `/docs/*` pillars, `/login`, `/operator`, `/appchains`, `/validator`, `/download`, `/ultraverse`, `/learn`, `/transact`, `/send-ultra`, `/legislation`)
**Website revision at scan time:** `d2dbd87`
**Engine:** axe-core 4.11.1, default rule set (all rules), automated only; keyboard behaviour checked with real key presses
**Verdict:** 0 violations, local and live

## Findings and fixes

| rule | where | fix |
|---|---|---|
| `region` ×9 | `/docs` | only the chapter grid was a `<main>`; the `h1`, search input, whitepaper card, wallet callout and the fixed status bar were content outside any landmark. One page-level `<main>` now wraps them and the former inner `<main>` became `<section aria-label="Protocol chapters">`, so each document still has exactly one `main`. |
| `region` ×1 | `/download` (and every page rendering the coin) | the fixed corner coin was unlandmarked content. It is decoration — its image alt reads "Decorative UltraNet coin illustration" and its label reads `Protocol_Illustration` — so its wrapper is now `aria-hidden="true"` instead of being given a landmark. |
| `landmark-complementary-is-top-level` ×1 | `/validator` | `ProposalWalletPrerequisite` used `<aside>` inside the page's `main`. It is contextual guidance, not a complement to the page, so it is a `<section>` with the same `aria-labelledby`. Valid on both surfaces the component serves (dark `/validator`, light `/learn`). |
| `scrollable-region-focusable` ×2 | `/validator` | horizontally scrollable `<pre>` blocks were unreachable by keyboard, which also stranded their clipped content. |

Six scrollable code blocks were made focusable — three on `/validator` and three on `/download`. The
`/download` blocks did not trip the rule at desktop width because axe measures the current viewport and
their content fits there; they clip at narrower widths, so they were fixed too rather than left to fail
for someone on a small screen. Each gained `tabIndex={0}` and a `focus-visible` ring, so the new tab stop
is visible when it is reached.

`VisualNoise` needed no change: its decorative `Visual_Decoration: … NOT_LIVE` labels are at 6% opacity
(1.11:1, correct for decoration and exempt under WCAG 1.4.3) and the component's root already carried
`aria-hidden="true"`, so assistive technology never received them.

## Evidence

- **Landmarks:** `document.querySelectorAll('main').length === 1` on all 17 routes.
- **axe, all rules:** 0 violations on all 17 routes locally, and 0 on six production routes after deploy
  (`/`, `/docs`, `/validator`, `/download`, `/login`, `/operator`).
- **Keyboard:** real `Tab` presses on `/validator` reached `pre` elements as consecutive focus stops
  (three confirmed before the loop stopped), proving the scrollable blocks are operable, not merely
  marked focusable.
- **Live DOM spot checks:** `pre[tabindex="0"]` count is 3 on both `/validator` and `/download`; the coin
  is inside an `aria-hidden` wrapper wherever it renders.

## Scope and limits

Automated axe scans plus keyboard focus checks. This is not a manual screen-reader pass (NVDA, JAWS,
VoiceOver), not a browser-zoom or reflow assessment, and not a mobile or touch assessment. Focus order was
sampled on `/validator` only; it was not walked end to end on every route.

The whitepaper reader is an embedded document and is audited separately in
[2026-10-21-whitepaper-reader-accessibility.md](./2026-10-21-whitepaper-reader-accessibility.md); axe on the
parent page does not reach inside its frame.

## Known follow-ups

- A manual screen-reader pass is still outstanding across the site. Everything recorded here is automated
  plus keyboard mechanics.
- Focus-visible styling on the six code blocks is newly introduced; it was verified to appear on keyboard
  focus, but not reviewed against the visual design system at every breakpoint.
