# Manual screen-reader audit plan (NVDA / VoiceOver)

Automated scans are done and green, but they cannot tell you whether a page *makes sense* when
spoken. This is the procedure for the manual pass that closes that gap. It has not been performed
yet — nothing in this repository records a manual screen-reader result, and until one exists the
audit story is "automated only".

## Scope

**Surfaces, in priority order**

| # | Route | Why it is first |
|---|---|---|
| 1 | `/validator` | The longest flow: two identities, key generation code blocks, a governance proposal. Also the page with the most interleaved headings and code. |
| 2 | `/send-ultra` | Wallet signing: an unrecoverable action. A mis-spoken confirmation is the worst failure mode on the site. |
| 3 | `/docs` | Index with search, filters and a card grid; tests live-region behaviour of the result count. |
| 4 | `/docs/whitepaper` | Embedded reader inside an iframe; separate document context, own controls. |
| 5 | `/download` | Code blocks, checksum warning, multiple release actions. |
| 6 | `/login`, `/operator`, `/appchains`, `/ultraverse`, `/learn` | Spot checks of the remaining shells. |

**Browsers and readers**

- NVDA 2025.x + Firefox on Windows (primary; NVDA drives the market on Windows).
- NVDA + Chrome on Windows (second pass — browse-mode differences are real).
- VoiceOver + Safari on macOS (primary Apple desktop).
- VoiceOver + Safari on iOS, iPhone (the mobile experience is untested by any tool we run).

## Preparation

1. Turn off the visible screen where possible; audit by ear only for the first pass.
2. Set speech rate to something you can follow (NVDA default is fast for newcomers).
3. Disable any "read all on page load" extensions so the run starts from a known state.
4. Have the route list above open; keep a notes file per route.

## Test sequence — per route

Run every step on **every** route in scope. Record `pass`, `pass with note`, or `fail`.

### 1. Landmark and heading navigation (structural)

- `D`/`R` (NVDA: `D`) — cycle landmarks. Expected: banner → main → contentinfo, in that order, no
  duplicate `main`.
- `H`/`1..6` — walk headings. Expected: a single `h1`, then `h2` per section, then `h3`; **no level
  is skipped**. Docs pages: `h1` (title) → `h2` (sections) → `h3` (sub-sections).
- `K` — links list. Expected: every link name describes its destination; no "here", no bare icon.

### 2. Reading order and content

- Read the page top to bottom with `Down`/`NVDA+Down`. Expected: no repeated text, no announcement of
  decoration (the corner coin, the noise layer, the "Visual_Decoration" labels must be silent).
- Tab to the main content: is there a skip mechanism, or does the reader land on the nav first? A nav
  with nine links before content is acceptable only if a skip link exists; currently **none exists** —
  record the cost of that in seconds-to-reach-content.

### 3. Tables and data

- `T` — tables. Expected: columns announced with headers; `/appchains` and `/operator` tables must
  announce header context on every cell.
- The command-center terminals are `<ul>` grids, not tables. Expected: they read as lists, and the
  live/status strings announce their state at sensible intervals (see 5).

### 4. Forms and actions

- On `/send-ultra` and `/validator`: tab through every control. Expected: label read with the field,
  errors announced when they appear (`aria-live`), the submit button says what it does.
- Corpus check: the recipient address field must announce that it is a 64-character hex address, and
  read the entered value back without truncating it silently.

### 5. Live regions and dynamic content

- Watch the mempool/ledger terminals for 30 seconds. Expected: updates are announced **once per
  meaningful change**, not on every 5-second poll. A feed that chatters every poll is a fail — record
  it, do not dismiss it as cosmetic.
- Trigger an error (submit an invalid address) and confirm the message is announced, then re-announced
  no more than once.

### 6. Modals, menus and focus management

- Open the mobile navigation and the command search (`/` or the search field): focus must move into the
  dialog, `Esc` must close it, focus must return to the trigger.
- Open the approval dialog on `/` and confirm the reader does not read the page behind it.

### 7. Code blocks and long values

- On `/download` and `/validator`: tab to a `<pre>`. Expected: it takes focus with a spoken name or a
  clear context; horizontal scrolling is announced as content continues (it is keyboard-reachable
  since 2026-10-31, but the *speech* experience is unverified).
- Multiline PowerShell snippets: confirm line-by-line reading does not merge or drop characters.

### 8. Wallet flow (highest risk)

- Complete a signing flow with the reader on: create/unlock, review the transfer, approve.
- Expected: the confirmation reads the **amount, recipient and fee** before the approve control.
  Any silent state change here is a release blocker, not a nice-to-have.

## Recording

Add one file per session, e.g. `docs/audits/YYYY-MM-DD-manual-sr-nvda-windows.md`:

```markdown
# Manual screen-reader pass — NVDA 2025.x on Windows / Firefox
**Date:** … **Reader:** … **Browser:** … **Routes:** …
**Build:** commit SHA deployed at the time

| Route | Step | Result | Note |
|---|---|---|---|
| /validator | 1 landmarks | pass | banner → main → contentinfo |
| /validator | 5 live regions | fail | mempool announces on every poll |
```

Then link it from `docs/audits/README.md`. Keep failures visible; do not aggregate them away.

## Pass criteria

A route passes when every step is `pass`, or `pass with note` where the note is a documented
limitation. Anything that changes wallet state silently, traps focus, or hides content from the
reader is a blocker regardless of the automated result.

## Known gaps this plan must cover

- **No skip link.** Nine nav links precede content on every page; a keyboard/reader user has no way to
  bypass them. This is the single most likely manual finding.
- **No manual pass has ever been recorded** — including for the reader's iframe, which no tool
  navigates with a reader attached.
- **Mobile Safari + VoiceOver is entirely unmeasured.**
- **Live-region politeness** across the command centre is unverified; automated tools cannot judge it.
