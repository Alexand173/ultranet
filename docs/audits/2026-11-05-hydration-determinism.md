# Hydration: what reproduced, what did not, and the check that settles it

**Date:** 2026-11-05 19:00–19:30 UTC
**Build:** website worktree on `7a3b528`, dev server 15.5.21, React 19.2.8
**Verdict:** 18 routes, 0 hydration messages; 0 accessibility violations; the detector is proven to fail

## The claim I had to withdraw

The previous record attributed a dev-overlay hydration mismatch to `VisualNoise`. **It does not
reproduce.** On unmodified `4733615`, a fresh dev server and 18 route loads produced zero hydration
messages, with the scan confirming `dev=yes` on every route (so React was in development and would have
reported one).

The attribution was wrong for a reason worth writing down: `VisualNoise` is gated behind an `isMounted`
effect, so it renders `null` on the server **and** on the first client render. It is therefore not part of
the hydrated tree at all, and cannot produce a hydration mismatch. Nondeterminism is still a real defect
in that component — just not this symptom.

## A clean scan is only evidence if the scan can fail

So the detector was tested before it was trusted. A throwaway client component rendering
`typeof window === "undefined" ? "server" : "client"` was mounted in the layout and the check run against
`/`:

```
FAIL /  dev=yes  messages=1
       pageerror: Hydration failed because the server rendered text didn't match the client.
                As a result this tree will be regenerated on the client...
```

Exit 1. The probe was removed immediately afterwards. Without this step, "0 messages" would have been an
unfalsified assertion — the same mistake as trusting an exit code from `tail` instead of the scanner.

## What was fixed

| change | why it was a defect |
|---|---|
| seeded `NOISE_FIELD` | `VisualNoise` called `Math.random()` ~170 times **per render**. StrictMode, a `usePathname()` change or any parent update re-rolled the whole decoration, so the field could not be reproduced, pinned, or reviewed. |
| `data-scroll-behavior="smooth"` on `<html>` | Next 15 asks for it so it can suspend CSS smooth scrolling during a route swap. The build printed the warning; it does not now. |
| `PendingMempoolTerminal` clock starts unknown | `useState(() => Date.now())` runs on the server too, so the two sides seed different clocks. Masked only because the terminal starts with an empty snapshot — armed, not safe. |
| `readReturnPath()` instead of a render-time `window` read | The login page built its `?next=` destination during render: `null` on the server, a value on the client. Now read where it is used, in the effect and the post-login handler. |

## Verification

| check | result |
|---|---|
| hydration, 18 routes, dev build confirmed | **0 messages**, exit 0 |
| accessibility, 18 routes | **0 violations** (proves the shared-module extraction did not disturb the gate) |
| field across a reload | identical |
| field across a client-side round trip (`/` → `/docs` → `/`) | identical |
| node counts | 12 labels, 40 particles — unchanged |
| `/ultraverse` suppression | still suppressed |
| first particle | `16.7% / 6.92%`, matching the golden value in the unit test |
| `npm test` | 138 passing (129 existing + 9 new) |
| `npx tsc --noEmit`, `npm run lint` | clean |
| `npm run build` | exit 0, no warnings — the scroll-behavior warning is gone |

Two mistakes made and corrected during the work, recorded so they are not repeated: a JSX comment placed
outside the root element broke `layout.tsx` (caught by `tsc`, and by the dev server overlay), and the
source guard in the new test matched the word `Math.random(` inside a **comment**, so it now strips
comments before matching.

## Running it

```bash
cd website
npm run dev                 # in one terminal — the check is meaningless against a production build
npm run hydration           # 18 routes, exits 1 on a mismatch
npm run hydration -- --routes /,/login --json hydration.json
```

The check warns if it suspects a production build, because React stays silent there. It is deliberately
**not** in CI yet: that would need a dev server in the workflow, and a false green from scanning built
output would be worse than no gate.

## Scope and limits

Hydration messages only, on a dev build, on a Chromium headless render of 18 routes at 1440×900. It cannot
see interactive-only divergence (a mismatch that appears after a wallet session or a captcha), and it is
not a visual-diff test — the stability checks compare rendered attributes, not pixels.

The manual screen-reader pass is still outstanding; see
[MANUAL_SCREEN_READER_AUDIT_PLAN.md](./MANUAL_SCREEN_READER_AUDIT_PLAN.md).
