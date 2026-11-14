# Ultra Chess on an AppChain — plan

**Architecture:** the game is a normal Android app feature. The AppChain stores **one receipt per
finished game** — never a move. The receipt is what a payout can be audited against, and the anchor
is what makes it tamper-evident afterwards.

**P0 delivered (2026-11-14):** rules, opponent, screen and the catalogue tile are in the app and
green on the emulator (62 tests). Nothing below the app line has been built yet.

**Non-goals for v1:** move-by-move storage, game logic on the node, PvP, matchmaking, a chess clock,
resuming a game across launches.

## 1. What goes where

```
  ┌───────────────────── Android app ─────────────────────┐
  │  ChessRules (legal moves, 3 modes)                    │
  │  ChessAi    (negamax, legal moves only)               │
  │  ChessScreen(mode tabs · board · status · prize)      │
  │                                                       │
  │  game ends → {game_id, mode, result, plies}  ────────┐│
  └─────────────────────────────────────────────────────│┘
                                                        │  HTTPS
                                    ┌───────────────────▼─────────────┐
                                    │ prize service (new binary)      │
                                    │  caps · idempotency per game_id │
                                    └──┬───────────────────────┬──────┘
                       admin token     │                       │ signs a normal transfer
                                       ▼                       ▼
                    ┌──────────────────────────┐    ┌────────────────────────┐
                    │ node: appchain state     │    │ node: 0.0001 $ULTRA    │
                    │ chess/game/{id} = {...}  │    │ to the player's address│
                    └────────────┬─────────────┘    └────────────────────────┘
                                 │ batched anchor (0.001 per anchor, from treasury)
                                 ▼
                    ┌──────────────────────────┐
                    │ appchain anchor: root    │
                    │ over the receipt set     │
                    └──────────────────────────┘
```

Two consequences worth stating up front, because they shape everything else:

1. **The app cannot write the receipt itself.** `POST /api/appchain/{id}/state` is operator-authed,
   and the admin token must never ship in an APK. The prize service holds the token, writes the
   receipt, and pays — the phone only talks to the service.
2. **A win is computed on the phone, so it is self-reported.** Nothing on the node can tell a real
   win from a patched client claiming one. The caps in §5 are the whole defence, and they must be
   written down as the design, not discovered later as a bug.

## 2. Game layer (app only, no chain)

One screen, `ChessScreen`, with the three modes the engineering decision settled:

| Mode | Rules | Win |
|---|---|---|
| Classic | Standard chess | Checkmate |
| Random | Standard rules, shuffled back ranks (bishops on opposite colours, no castling rights) | Checkmate |
| Anti-Chess | No check/checkmate; **a capture must be played whenever one exists**; the king is an ordinary piece | You win by **losing all your pieces**; a side with no legal move also wins |

- `ChessRules` — position, pseudo-legal moves, legality filter, castling, en passant, promotion,
  per-mode legality. Pure functions over an immutable position so the search can copy freely.
- `ChessAi` — negamax with alpha-beta, captures first, depth 3 with a ~700 ms budget and a node cap;
  Anti-Chess inverts the evaluation (its legal set already enforces compulsory capture). It only ever
  plays moves from `legalMoves`, which is the property the tests pin.
- `ChessScreen` — mode selector, board (pieces from the system font, so no asset pipeline), selected
  square + legal targets, last move, status line, **New game**, and the prize row.

The `Games` concept tile in the catalogue becomes one supported tile, **Ultra Chess**, in
Applications.

## 3. Node: minimal state path

Two routes, the smallest thing that makes an anchor mean something.

| Method | Route | Auth | Body / query | Response |
|---|---|---|---|---|
| POST | `/api/appchain/{chain_id}/state` | operator token | `{ "key": "chess/game/{id}", "value": "{...}" }` | `201 { key, state_version, stored_at }` |
| GET | `/api/appchain/{chain_id}/state` | public | `?prefix=chess/&limit=50&after={key}` | `{ state_version, entries: [{key, value, updated_at}], next }` |

Limits, enforced in the handler and not merely documented:

- key ≤ 120 chars, `[A-Za-z0-9._:/-]` only; value ≤ 4 KiB
- per chain: ≤ 100,000 keys, ≤ 32 MiB of values
- the write bumps `state_version` — this is what makes the next anchor commit to real state instead
  of the empty root it commits to today
- no deletes in v1; a receipt is a fact, not a mutable record

Receipt value (written by the service, not the app):

```json
{ "game_id":"…", "address":"<64 hex>", "mode":"classic|random|anti",
  "result":"player_win", "plies":37, "finished_at":1763128800, "app":"0.1.0" }
```

Anchoring stays exactly as it is (`POST /api/appchain/{id}/anchor`, 0.001 from the chain treasury),
but **batched: one anchor per 500 receipts, or per 6 hours, whichever comes first.** That bound is not
cosmetic. The anchor fee is 0.001 and the prize is 0.0001, so ten prizes are worth one anchor: any
schedule finer than a few hundred receipts spends more on committing the results than on paying them.

Receipts rotate into day buckets (`chess/{yyyy-mm-dd}/{game_id}`) so the key set stays bounded: a
bucket is pruned once its anchor is written, and the anchor root remains the durable record of it.
Without that, 100 chains × thousands of games/day would hit the 100,000-key cap in weeks and every
game after it would be refused.

## 4. Prize service (new, modelled on the faucet)

A separate small Rust binary and systemd unit, the same shape as `ultranet-faucet` — it holds one
funded key and the operator token, and the phone never sees either.

```
POST /api/chess/results   { address, game_id, mode, result, plies, app }
GET  /api/chess/results/{prize_id}
```

`POST` → `202 { prize_id, amount_base_units: "100", status: paid|recorded }`; `GET` → `{ status:
queued|paid|recorded|rejected, transaction_hash?, reason? }`. Same contract as the faucet:
`Idempotency-Key`, acceptance is not payment, the app polls.

**Every win is recorded, only the first three per address per day are paid.** That is a two-outcome
acceptance, and the response must say which one happened:

- `paid` — receipt written, 100 base units queued
- `recorded` — receipt written, no money (daily limit reached); the win still counts and still anchors

The screen must show the difference. A player who wins a fourth game and sees "Prize paid" would be
right to call the app a liar, and a player who sees nothing at all would think the win was lost.

Order inside the service, and it matters:

1. validate and apply caps (§5)
2. write the chain receipt (`chess/game/{game_id}`) — the receipt exists before any money moves
3. queue the payout; the worker signs and submits 0.0001 $ULTRA (fee 1 base unit: the 1 % rule floors
   at 1, so a paid prize costs 101 base units all in)
4. anchor on the batching rule in §3
5. `GET` reports `paid` with the hash

If step 3 fails, the receipt still stands and the row is `rejected` with a reason — never a paid
prize without a receipt, never a silent loss of one.

## 5. Caps — the only real defence

| Limit | Value | Why |
|---|---|---|
| Prize | 100 base units (0.0001 $ULTRA) | one ten-thousandth of a coin: a token of a win, not income |
| One prize per `game_id` | database constraint | replaying one win is impossible |
| Paid wins per address per day | 3 | an address can earn at most 0.0003 in a day |
| Wins **recorded** per address per day | unlimited | the chain is a record of play, not a payment ledger |
| Paid wins per IP per day | 3 | matches the per-address cap; a NAT is not a farm |
| Paid wins per appchain per day | 2,000 (0.2 $ULTRA) | one chain cannot monopolise the global budget |
| Paid wins globally per day | 20,000 (2 $ULTRA + fees) | the backstop, whatever the number of chains |
| Minimum plies | 10 | a "win" in three moves is not a game |
| Address must hold a confirmed faucet claim | checked once per address | the prize cannot mint a wallet that never onboarded |

### 5.1 Does it actually scale? The arithmetic

| Scenario | Paid wins/day | Prize spend/day | With fees (101/prize) |
|---|---|---|---|
| 1 chain, 1,000 daily players | 3,000 | 0.3 $ULTRA | 0.303 |
| 10 chains, 10,000 daily players | 30,000 → capped | 2.0 $ULTRA | 2.02 |
| 100 chains, 100,000 daily players | capped | 2.0 $ULTRA | 2.02 |
| Anchor fees at 100 chains, 4 anchors/day each | — | — | 0.4 |

At the 20,000/day global cap the prize budget is fixed at ~2 $ULTRA/day no matter how many chains
exist — on a 1,209 $ULTRA float that is roughly 600 days, and the cap, not the player count, is what
decides it. Two follow-ons worth knowing:

- **The prize is not the thing that drains the float.** The faucet hands out 0.1 per claim, up to 25
  per IP per day — 2.5 $ULTRA per address per day, four orders of magnitude more than 0.0003. A farm
  harvests the faucet, not the chess prize. So the prize wallet must be **separate from the faucet
  signer**: a prize bug must never be able to strand claim payouts, and the two budgets are capped
  independently.
- **Anchor fees become the real cost at scale**, not prizes: 0.4 $ULTRA/day at 100 chains is a fifth
  of the entire prize budget. That is why §3 batches by 500 receipts rather than by game.

**The honest statement to put in the README and on the screen:** these caps bound the damage; they do
not make a self-reported result true. A modified client can claim wins up to the caps. The only sound
fix is playing against a **server-side** AI, where the node knows the position — that is the eventual
answer if prizes ever get big enough to be worth farming, and it is deliberately out of scope here.

## 6. Files

| Area | File |
|---|---|
| App rules | `android-app/…/ui/chess/ChessRules.kt` (new) |
| App opponent | `android-app/…/ui/chess/ChessAi.kt` (new) |
| App screen | `android-app/…/ui/chess/ChessScreen.kt` (new) |
| Catalogue | `data/ServiceCatalog.kt` — `Games` concept → supported `Ultra Chess` |
| Shell | `ui/UltraVerseApp.kt` — `HomeSection.Chess`, tile route, Back |
| Node | `src/appchain/runtime.rs` (state read/write), `src/api.rs` (two routes + validation) |
| Service | `src/bin/prizes.rs` or a `src/prizes/` crate, `deploy/ultranet-prizes.service` |
| Docs | this file, `README.md`, `android-app/README.md` |

## 7. Phases and exit criteria

| Phase | Deliverable | Exit criterion |
|---|---|---|
| **P0 — game** | Rules, AI, screen, catalogue tile, three modes | All three modes playable on the emulator; perft and variant tests green; no chain code touched |
| **P1 — state path** | Two node routes + limits | Write two receipts, anchor, `GET` returns both, and **the anchor root differs before and after** — that difference is the proof the path works |
| **P2 — prize service** | Service, payout, caps, batched anchor | Three wins in a day pay 0.0001 each with a hash and the fourth is recorded as `recorded` with no money; every one of the four receipts is present at `GET /state` |
| **P3 — receipt UI** | Prize row: pending → confirmed hash; remaining today | The screen states the prize's status truthfully, including "rejected" |

P0 ships on its own and is worth doing first: it is the whole product, and none of it depends on the
chain.

## 8. Tests

| Test | Asserts |
|---|---|
| `perftCounts` | 20 moves at the start, 400 after two plies, 8,902 after three — catches move-generation bugs |
| `castlingNeedsSpaceAndSafety` | no castling out of, through or into check; no castling after the rook moves |
| `enPassantAndPromotion` | the captured pawn is removed; promotion offers four pieces |
| `antiChessForcesTheCapture` | with a capture available, `legalMoves` contains captures only |
| `antiChessWinsByLosingEverything` | a side with no pieces is declared the winner |
| `randomStartHasOneKingPerSide` | 16 pieces per side, one king each, bishops on opposite colours |
| `aiOnlyPlaysLegalMoves` | 200 random positions × 3 modes: the returned move is in `legalMoves` |
| `aiTakesAFreeQueen` | a hanging queen is captured |
| `chessTileOpensTheBoard` | the tile is supported and opens `ChessScreen` on all three tabs |
| node: `state_requires_operator_token` | 401 without it, 201 with it |
| node: `anchor_commits_written_state` | root before ≠ root after a write |
| prize: `prize_pays_once_per_game` | second request for the same `game_id` is refused |
| prize: `fourthWinIsRecordedNotPaid` | 3 `paid`, 4th `recorded` — and its receipt still written |
| prize: `globalCapStopsPaymentNotRecording` | at the daily cap, wins still write receipts and still anchor |

## 9. Definition of done

- [ ] Three modes playable, with the AI never making an illegal move
- [ ] `Games` tile is supported and opens Ultra Chess; Back returns to the catalogue
- [ ] Receipt written to the chain state, visible via `GET`, and included in an anchor
- [ ] A paid win pays 0.0001 $ULTRA once, with a hash the screen can show
- [ ] The 4th win of the day is recorded, shows "recorded, not paid", and still appears at `GET /state`
- [ ] Caps enforced server-side and stated on screen
- [ ] Full instrumented suite green; README updated; commits split app / node / service

## 10. Open decision

P2 needs a funded key in a service — a second thing that can move money, with its own credentials,
unit and Caddy route. The alternative for a first release is **P0 + P1 only**: the game and the
receipt, with the 0.0001 paid manually by the operator from the records. That proves the loop on a
handful of games without standing up a money-moving service, and it is what I would do first.
