# Homepage command centre: why the hero was pushed down and cut off

**Date:** 2026-11-05 20:10–20:25 UTC
**Symptom reported:** on opening the home page, the middle section looked split — an empty band above the
hero, the hero sitting low, and the three cards clipped at the bottom edge.

## What was actually wrong

Not the hero. The centre lane was stretched by its neighbours.

At `2xl` the command centre is a three-column grid inside a fixed-height section. Each side column stacks
terminals (`flex-1`). Measured at 1568×880:

| box | height |
|---|---|
| section (fixed `calc(100vh-5rem)`) | 800 |
| grid (section minus the two status tickers) | 735 |
| **grid row** | **906** |
| centre lane (stretched to the row) | 906 |
| hero content inside the lane | 687 |

The right column stacks three terminals; their *intrinsic* content (ledger rows, mempool, admin console)
totals ~906 px. `flex-1` sets `min-height: auto`, so the terminals refused to shrink, the row grew to 906,
and the grid — which carries `2xl:overflow-hidden` — clipped the overflow. Because the centre lane stretches
to the row and centres its content, a 687 px hero was centred inside a 906 px box whose top sat above the
section: the hero appeared pushed down, and the bottom cards fell outside the clip.

So the empty band, the low hero and the cut cards were one bug: **a container that clips instead of
letting its columns shrink**.

## Fix

1. `2xl:grid-rows-[minmax(0,1fr)]` — the row may not exceed the grid.
2. `min-h-0` on both side columns and on every terminal — they compress instead of demanding their
   intrinsic height (their inner lists already scroll).
3. `--command-center-lane: calc(100dvh - 5rem nav - 4.0625rem tickers)` in `globals.css`, applied as a
   `max-h` on the two side columns. Three stacked terminals can no longer define the row height.
4. The section uses `2xl:min-h-[calc(100vh-5rem)]` instead of a fixed height: it still owns the first screen
   when there is room, but a shorter window grows the section and scrolls rather than cutting the hero.

## Verification

Measured in eight viewports; "HERO FITS" means the hero's own box is fully inside the lane with no negative
gap at either end, and there is no horizontal overflow:

| viewport | columns | section | lane | hero gaps (top/bottom) | result |
|---|---|---:|---:|---|---|
| 1280×800 | stacked | 1906 | 687 | 64 / 64 | fits |
| 1440×900 | stacked | 1984 | 765 | 103 / 103 | fits |
| 1536×864 | 3 | 784 | 719 | 80 / 80 | fits |
| 1568×880 | 3 | 800 | 735 | 88 / 88 | fits |
| 1568×800 | 3 | 752 | 687 | 64 / 64 | fits (page scrolls 32 px) |
| 1536×720 | 3 | 752 | 687 | 64 / 64 | fits (page scrolls) |
| 1728×900 | 3 | 820 | 755 | 98 / 98 | fits |
| 1920×1080 | 3 | 1000 | 935 | 188 / 188 | fits |

A screenshot at 1568×880 confirms all three cards and both calls to action are visible with the top and
bottom tickers in place.

## One mistake worth recording

The first attempt put a `{/* … */}` JSX comment as a sibling of the returned root element, which is a syntax
error — the same mistake made in `layout.tsx` earlier in the day. `tsc` caught it within seconds and the dev
overlay showed it, but the pattern is worth naming: **comments belong above `return (` or inside an element,
never beside the root node.**
