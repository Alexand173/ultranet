# Manual screen-reader audit — scripted checklist

Automated scans are green, but axe cannot tell you whether a page *makes sense* when spoken. This is the
procedure for the manual pass that closes that gap, and it is **scripted**: the per-route expectations
below are generated from the running app, not remembered, so the checklist cannot quietly drift away from
what the site actually announces.

No manual screen-reader result exists in this repository yet. Until one does, the audit story is
"automated only".

## Refresh the expectations before every session

```bash
cd website
npm run dev                                             # the expectations come from the app
npm run sr:inventory -- --json /tmp/sr-inventory.json    # all 18 routes
npm run sr:inventory -- --routes /validator,/send-ultra  # just the ones you are auditing
```

The script prints, per route: document title, first heading, heading outline, landmark chain, live regions,
embedded frames, control names and the first tab stops. It is a description, **not a pass** — it cannot
judge speech. If its output disagrees with the tables below, the tables are stale: update them from the
script's output and note the date.

Expectations recorded **2026-11-05**, build `cd7f39e`.

## Per-route expectations

Reading the columns: *title* is what the browser/tab and `NVDA+T` announce; *outline* is the heading levels in
document order; *landmarks* is what `D` cycles through; *live* is what will speak on its own; *first tabs*
are the first stops after a fresh load, in order.

| Route | Title announced | `h1` | Outline | Landmarks (`D`) | Live regions | First tab stops |
|---|---|---|---|---|---|---|
| `/` | UltraNet \| Blockchain Network | A network you can actually understand. | h1 h2×4 h3 h4×4 h2 h3×4 h2 h3 h2×3 h3 | nav → main → footer | polite: "UltraWallet not detected" | Skip → logo → LEARN → DOCS |
| `/docs` | UltraNet \| Blockchain Network | Documentation Hub | h1 h2 h2 h3×34 | nav → main | none | Skip → logo → LEARN → DOCS |
| `/docs/whitepaper` | UltraNet Whitepaper \| The Sovereign Technical Guide | UltraNet Technical Whitepaper | h1 h2 | nav → main (named "Whitepaper reader") | polite: `0%_READ` | skip → logo → LEARN → DOCS; iframe titled "UltraNet v7.1 Sovereign Technical Whitepaper" |
| `/docs/layer-0-foundation` | UltraNet \| Blockchain Network | Layer 0 Foundation | h1 h2×3 h3 h2 | nav → header → main | none | skip, logo, LEARN, DOCS |
| `/docs/unified-scalability` | UltraNet \| Blockchain Network | Unified Scalability | h1 h2×3 h3×2 h2 | nav → header → main | none | skip, logo, LEARN, DOCS |
| `/docs/trusted-execution` | UltraNet \| Blockchain Network | FHE Implementation Notes | h1 h2×3 h3 h2 | nav → header → main | none | skip, logo, LEARN, DOCS |
| `/docs/real-time-finality` | UltraNet \| Blockchain Network | Execution and Performance Evidence | h1 h2×4 | nav → header → main | none | skip, logo, LEARN, DOCS |
| `/docs/autonomous-intelligence` | UltraNet \| Blockchain Network | AI Governor Implementation | h1 h2×4 | nav → header → main | polite: "UNAVAILABLE" | skip, logo, LEARN, DOCS |
| `/login` | UltraNet \| Blockchain Network | Vault Access | h1 | main only | none | Skip → "ULTRAWALLET" tab → "CLI_SIGNED_PAYLOAD" tab → `input#node-identifier` |
| `/operator` | UltraNet Operator Console \| Authenticated Operations | AUTH_SERVICE_UNAVAILABLE | h1 | main only | none | Skip → RETURN_TO_LOGIN → RETRY_CHECK |
| `/appchains` | AppChains \| UltraNet Operator Console | AUTH_SERVICE_UNAVAILABLE | h1 | main only | none | Skip → RETURN_TO_LOGIN → RETRY_CHECK |
| `/validator` | Validator Onboarding \| UltraNet v7.1.7 | Become a validator | h1 h2 h3×3 h2 h3×3 h2 h3×3 h2×2 | nav → main | polite ×2 (one empty, one "UltraWallet not detected") | skip, logo, LEARN, DOCS |
| `/download` | Download UltraNet Node \| v7.1.7 | Sovereign Node v7.1 | h1 h2 h3×3 h2 h2 h3 h3 h2 | nav → main | none | skip, logo, LEARN, DOCS |
| `/legislation` | UltraNet \| Blockchain Network | Protocol Legislation | h1 h2×3 h3 | nav → main | none | skip, logo, LEARN, DOCS |
| `/transact` | Send $ULTRA \| UltraNet | Create a wallet | **h2 then h1** | nav → main | none | skip, logo, LEARN, DOCS |
| `/ultraverse` | UltraVerse Services \| UltraNet | Services on UltraNet | h1 h2 h3×3 h2 h3×4 | **main → header → nav** (named "Directory navigation") | none | Skip → UltraVerse logo → Services → Concepts |
| `/learn` | Learn UltraNet \| UltraNet | Understand the network before you use it. | h1 h2×3 h3×5 h2×2 | nav → main | polite: "Node data unavailable" | skip, logo, LEARN, DOCS |
| `/send-ultra` | Send $ULTRA \| UltraNet | Create a wallet | **h2 then h1** | nav → main | none | skip, logo, LEARN, DOCS |

Two routes legitimately differ from the common shape and must not be "fixed" during an audit: `/login`,
`/operator` and `/appchains` render without the site nav or footer (their only landmark is `main`, which is
why the skip link still works), and `/ultraverse` puts `main` before `header` and `nav`.

## Defects the inventory found — decide before you start

These are real, reproducible from the data above, and none of them is caught by axe. Either fix them first
so the session tests the fixed site, or record them as known failures.

1. **Duplicate page titles (WCAG 2.4.2, Level A).** Nine routes announce the identical title
   *"UltraNet | Blockchain Network"*: `/`, `/docs`, `/docs/layer-0-foundation`, `/docs/unified-scalability`,
   `/docs/trusted-execution`, `/docs/real-time-finality`, `/docs/autonomous-intelligence`, `/login`,
   `/legislation`. `/transact` and `/send-ultra` also share *"Send $ULTRA | UltraNet"*. A reader user moving
   between tabs gets no way to tell the pages apart. Fix: a per-route `metadata.title`.
2. **Heading order breaks on two routes.** `/transact` and `/send-ultra` put an `h2` *before* the `h1`, so
   `1` and `H` walk disagree with visual order (WCAG 1.3.1). The `h2` is "The node key identifies the
   computer…"; the `h1` is "Create a wallet".
3. **Landmarks are unnamed.** Only `/docs/whitepaper`'s main and `/ultraverse`'s nav declare a name. That is
   *acceptable* today because each type is unique per route — do not add labels to fix a problem that only
   appears when a second nav exists. Record it as "expected: announced as 'navigation' / 'main'".
4. **`/operator` tab order contains the dev-only `nextjs-portal`.** Present only in development; a production
   build has no such stop. Audit the deployed build, not `npm run dev`, if a tab stop looks unexplained.
5. **Live regions speak on load**, not only on change: `/` announces "UltraWallet not detected",
   `/learn` "Node data unavailable", `/docs/autonomous-intelligence` "UNAVAILABLE", `/validator` has an
   empty polite region plus the wallet notice. Judging whether that is *one announcement too many* is exactly
   what step 5 is for.

## The checklist

Run every step on every route in scope; record `pass`, `pass with note`, or `fail`. The route-specific
values are in the table above.

| # | Step | What to do | Expected |
|---|---|---|---|
| 1 | Landmarks | `D` repeatedly | Matches the route's landmark chain; exactly one `main`; no route announces two of the same type |
| 2 | Headings | `H`, then `1..6` | One `h1` matching the table, no skipped levels (`h2` before `h1` on `/transact`, `/send-ultra` is defect 2) |
| 3 | Skip link | `Tab` once, `Enter` | Announces "Skip to content"; focus lands in `main`, first heading is not under the fixed nav |
| 4 | Reading order | `Down` / `NVDA+Down` from the top | No repeated text; the noise layer, corner coin and "Visual_Decoration" labels are silent |
| 5 | Live regions | Sit for 30 s on `/`, `/learn`, `/validator`, `/docs/whitepaper` | Speaks once per meaningful change, **not** on every 5 s poll; the reader's `0%_READ` progress does not chatter |
| 6 | Tables and lists | `T` on `/operator`, `/appchains`; arrow through terminals | Table cells carry header context; command-centre terminals read as lists, not fake tables |
| 7 | Forms | `Tab` through `/send-ultra`, `/validator`, `/login` | Label spoken with each field; the 64-hex address field says what it wants and reads back without silent truncation |
| 8 | Overlays | Open command search (`/`), mobile nav, approval dialog on `/` | Focus enters the dialog, `Esc` closes it, focus returns to the trigger, page behind is not read |
| 9 | Code blocks | `Tab` to a `<pre>` on `/download`, `/validator` | Takes focus with a spoken name or clear context; multiline PowerShell is not merged or dropped |
| 10 | Wallet flow | Unlock → review → approve on `/send-ultra` | Amount, recipient and fee spoken **before** the approve control; no silent state change |

Step 10 is the release-blocking one: a confirmation that reads wrongly is unrecoverable.

## Recording

One file per session, e.g. `docs/audits/YYYY-MM-DD-manual-sr-nvda-windows.md`:

```markdown
# Manual screen-reader pass — NVDA 2025.x on Windows / Firefox
**Date:** …  **Reader:** …  **Browser:** …  **Build:** commit SHA deployed
**Inventory:** npm run sr:inventory -- --json … (paste the route count)

| Route | Step | Result | Note |
|---|---|---|---|
| /validator | 1 landmarks | pass | nav → main, one main |
| / | 5 live regions | fail | "UltraWallet not detected" re-announces on every session check |
```

Then link it from `docs/audits/README.md`. Keep failures visible; do not aggregate them away.

## Pass criteria

A route passes when every step is `pass`, or `pass with note` where the note is a documented limitation.
Anything that changes wallet state silently, traps focus, or hides content from the reader is a blocker
regardless of the automated result.

## Known gaps this plan must cover

- **No manual pass has ever been recorded**, including for the reader's iframe, which no tool navigates with
  a reader attached.
- **Mobile Safari + VoiceOver is entirely unmeasured.**
- **Live-region politeness** across the command centre is unverified; automated tools cannot judge it, and
  step 5 is the only place it gets tested.
- **The five defects above are open** as of `cd7f39e`: duplicate titles, two broken heading orders, unnamed
  (but unique) landmarks, dev-only tab stop, eager live regions.
