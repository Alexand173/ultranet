# UltraGrid — the game, specified

**Date:** 2026-11-16 · **Chain:** AppChain #1 `UltraGrid`, owner `7d2cd1cc…f9d9388e` (live, anchored once)
**Authority for money:** [`ECONOMIC_ARCHITECTURE_V2.md`](./ECONOMIC_ARCHITECTURE_V2.md) §5 (R4, R4b). This
document applies it to one game and never overrides it. Where the two disagree, V2 wins.
**Supersedes for mechanics:** the puzzle sketch in [`APPCHAIN_FIRST_GAME_PLAN.md`](./APPCHAIN_FIRST_GAME_PLAN.md).

**Status:** the **engine is built and pinned** (this commit). The phone screen, the service, the daily
anchor and payouts are **not built**; §9 says exactly what remains.

---

## 1. The game

A grid of network tiles. A signal leaves the **source** on the left edge and must reach the **target** on
the right edge. The only move is to turn a tile a quarter turn clockwise. Fewer turns is better.

```text
level 1 of 2026-11-16        reference answer  9:1,5:2   (3 turns, par 3, limit 5)

│█┬│          S source (row 2, col 0)      T target (row 1, col 3)
█┘┼T          turn tile 9 once, tile 5 twice:
S┐┤┼          S → 9 → 5 → ┼ → T
┐┌││
```

| Tile | Open sides | Turns to repeat | Can be turned |
|---|---|---:|---|
| Straight `│ ─` | two opposite | 2 | yes |
| Bend `└ ┌ ┐ ┘` | two adjacent | 4 | yes |
| Tee `├ ┬ ┤ ┴` | three | 4 | yes |
| Cross `┼` | four | – | no (turning changes nothing) |
| Wall `█` | none | – | no |
| Source / target | four | – | no |

A side between two neighbours carries signal only when **both** tiles open onto it.

**A day is ten puzzles**, the same for everyone, growing from 4×4 (levels 0–1) to 8×8 (levels 8–9).

**An answer** is a list of `(tile, turns)`. Order does not matter; only the net turn per tile does. The
server replays it from the starting grid and floods it, so the result is *demonstrated*, not claimed.

**A win** is "connected within the move limit", where `limit = par + max(2, par/4)`. `par` is the cost of
the route the puzzle was built around: always achievable, **not claimed to be the minimum**. A shorter
route may exist, and a player who finds it simply beats par.

---

## 2. Where the puzzles come from

Everything derives from a 32-byte seed, so the phone, the server and an auditor see identical grids.

```text
seed(D)   = SHA3-256("UltraGrid/day-seed/v1" || u32le(chain_id) || "YYYY-MM-DD" || prev_root)
prev_root = state_root of the latest UltraGrid anchor whose block timestamp is before 00:00 UTC of D
            (32 zero bytes if there is none)
puzzle(L) = generate(seed(D), L)         L = 0..9
commitment(D) = SHA3-256("UltraGrid/set/v1" || u32le(10) || id(puzzle 0) || … || id(puzzle 9))
```

**Why the previous anchor.** A day's puzzles are fixed by something already on L1 before the day began,
so the operator cannot pick them after seeing who is playing. The anchor that closes day D−1 is mined
*after* midnight, so it feeds day D+1, not D: a one-day lag, and a missed anchor never blocks play (the
latest earlier anchor is used). Anyone recomputes `prev_root` from `GET /api/appchain/ledger/1/anchors`.

**Honest limit.** The owner chooses what goes into each anchored root, so a determined owner could grind
roots to steer a day's puzzles. The seed is about *binding*, not secrecy, and the owner gains nothing by
steering a puzzle they are not paid for.

**The engine** is `src/ultragrid/mod.rs`: pure, no clock, no network, no OS randomness. Its random stream
is SHA3-256 in counter mode with unbiased rejection sampling, written out in the module so any language
can reproduce it. **Pinned contract** (Rust tests + an independent Python port,
`tools/ultragrid_reference.py --check`): the first four stream words for seed `0x42×32`, the ten puzzle ids
and pars, and the set commitment `3a8539a3…a91027c`.

---

## 3. What the chain records

One anchor per day, signed by the owner key, 1,000 base units (0.001 `$ULTRA`).

```text
day_root = SHA3-256("UltraGrid/day-root/v1" || "YYYY-MM-DD" || commitment(D) || results_root)
results_root = SHA3-256 over the day's accepted receipts sorted by (address, level):
               address(32) || level(1) || moves(u16le) || puzzle_id(32)
```

The anchor proves **what was asked and what was accepted that day**, and nothing about whether a player
was human. `GET /api/appchain/ledger/1` is the public record; the result list is the server's.

---

## 4. Money — V2 applied to Grid

| Rule | Value | Source |
|---|---:|---|
| Reward per rewarded win | **10 base units** (0.00001) — a **test** value, shown as "test reward" on screen | V2 R4b |
| Rewarded wins per address per day | **3** (the first three levels solved that day) | V2 R4, owner's instruction |
| Wins **recorded** per address per day | unlimited (up to 10 levels), unpaid after the third | V2 R4 |
| Rewards per chain per day | 2,000 | V2 R4 |
| Rewards globally per day | 20,000 | V2 R4 |
| Withdrawal minimum | **100 base units** (0.0001); the whole balance is paid | V2 R4 |
| Payout fee | 1 base unit, **paid by the payout wallet**, never the player | V2 R4 |
| Payout wallet | **its own key**, separate from the faucet signer and from the owner key | V2 R4, invariant 7 |
| Source of the money | a funded pool; **never minted, never the faucet as a recurring source** | V2 invariant 2 |
| End | when the pool is empty or on an announced date; Grid then adopts the 100-unit standard or stops paying | V2 §5.3 |

**The arithmetic** (all in base units; 1 `$ULTRA` = 1,000,000):

| Quantity | Value |
|---|---:|
| Most one address can earn in a day | 3 × 10 = **30** (0.00003 `$ULTRA`) |
| Days of full play to reach the 100-unit minimum | 4 (12 wins = 120) |
| Pool cost of one rewarded win (10 + a tenth of the 1-unit fee per 100) | ≈ 10.1 |
| Rewarded wins a **1 `$ULTRA`** pool pays | ≈ 99,000 |
| Rewarded wins a **0.1 `$ULTRA`** pool pays (one faucet claim, V2 F-2) | ≈ 9,900 |
| The chain's own daily ceiling | 2,000 × 10 = 20,000 = 0.02 `$ULTRA`/day |
| Anchor, once a day | 1,000/day = **0.365 `$ULTRA`/year**, paid by the owner wallet |
| Owner wallet today | 0.089 `$ULTRA` → **89 daily anchors** before it needs topping up |

Two facts worth saying plainly:

- **The anchor costs more than the prizes** until there are about 100 rewarded wins a day (1,000 ÷ 10).
  Verifiability is bought by the owner. That is the model V2 §5.2 describes (entry fees, then sponsors, then
  the owner's own `$ULTRA`) and phase 2 is where it stops being the owner's cost.
- **Farming Grid is not worth it, and the faucet is the bigger lever.** One faucet claim (0.1 = 100,000
  units) equals 3,333 days of a wallet's maximum Grid earnings. A farm would harvest the faucet, which
  exists with or without this game, not Grid.

---

## 5. What stops cheating, and what does not

| | Stopped | Not stopped |
|---|---|---|
| A patched client claiming a win | **Yes.** Answers are replayed server-side on the engine's own grid | |
| Replaying one win for money twice | **Yes.** One reward per (address, date, level) | |
| Receipts rewritten after the fact | **Detectable.** The day root is anchored | not preventable |
| A **program** solving puzzles | **No.** Routing a signal is an easy search. Verifiability stops forged results, not automation | bounded only by the caps in §4 |

So the bound on a determined farm is: wallets × 30 units/day, wallets limited by the faucet's own gate,
total limited by the pool. The game is a **test of the loop**, not an income, and the screen must say so.
Phase-1 guards worth having, none of which is built: a minimum time between fetching a puzzle and
submitting (a few seconds per turn of par), and a one-time eligibility check per address.

---

## 6. The service (proposed shape, not built)

A small Rust binary like the faucet. Because money only ever flows **to the address that was credited**,
the API needs no login: submitting someone else's answer credits *them*.

```text
GET  /grid/day                 → { date, chain_id, prev_anchor:{sequence, root, block}, seed, commitment, closes_at }
POST /grid/answers             { address, date, level, answer:[{cell,turns}] }
                               → { outcome:{moves,connected,par,limit,win}, reward:"rewarded"|"recorded"|"none", balance_units }
GET  /grid/wallet/{address}    → { balance_units, withdrawable, history[] }
POST /grid/withdraw            { address } → { transaction_hash }     (balance ≥ 100; payout wallet signs)
```

The phone derives the day's puzzles itself from `seed` and checks them against `commitment`, so it never
has to trust the server's grids. Order inside the service matters: validate → write the receipt →
queue the payout → anchor on schedule. A receipt exists before money moves; a failed payout is `rejected`
with a reason, never a paid prize without a receipt. The same two-outcome honesty as the chess plan
(`rewarded` vs `recorded`) applies, and the screen must show which happened.

---

## 7. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| D1 | **Pool size and path.** Earlier you chose 1 `$ULTRA` from the faucet. V2 F-2 allows **one claim per chain**, and a claim is 0.1. | Keep the 1 `$ULTRA` pool but record it as a **named exception to F-2**, moved by the owner, or start with the 0.1 claim (≈ 9,900 wins is far more than a test needs) |
| D2 | **Eligibility gate.** "Address must have a faucet claim" blocks anyone who got `$ULTRA` another way, and cannot survive the faucet closing. | Use it in phase 1 only; replace with "address has any confirmed transaction" |
| D3 | **Win margin.** `par + max(2, par/4)` is a guess. | Keep for the test; tune from real solve data |
| D4 | **Where it lives in the app.** The Games tile opens Ultra Chess today. | Make it a two-entry directory (Chess, UltraGrid); name the game **UltraGrid**, matching the registered chain |
| D5 | **Top up the owner wallet.** 0.089 `$ULTRA` covers 89 daily anchors. | Fund ~1 `$ULTRA` (≈ 2.7 years) before the first daily anchor |

---

## 8. Tests

| Test | Asserts |
|---|---|
| `tile_tables_match_the_documented_arms` | the table in §1 is the code |
| `the_stream_is_pinned` | first four words for seed `0x42×32` (also reproduced in Python) |
| `golden_puzzles_are_pinned` | ten ids, ten pars, the commitment — the contract for the Kotlin port |
| `every_puzzle_is_solvable_by_its_reference_within_the_limit` | 40 seeds × 10 levels: not solved at the start, reference connects at exactly par |
| `generation_is_deterministic_and_seed_sensitive` | same seed same bytes; different seed different id |
| `terminals_sit_on_opposite_edges_and_are_never_walled` | source left, target right, never a wall |
| `canonical_bytes_round_trip_and_reject_damage` | truncation, wrong version, orientation outside a tile's period |
| `verify_refuses_moves_that_are_not_moves` | wall, terminal, out of range, zero or full-period turns, duplicate, too many entries |
| `a_connected_answer_over_the_limit_is_not_a_win` | extra turns on unrelated tiles keep it connected but lose the win |
| `the_order_of_entries_does_not_change_the_result` | only the net turn per tile matters |
| `daily_seed_binds_chain_date_and_previous_root` | any of the three changes the seed; bad dates refused |
| `a_day_has_ten_growing_puzzles_and_one_commitment` | sizes 4,4,5,5,6,6,7,7,8,8; ten distinct ids; order is committed |

---

## 9. Phases

| Phase | Deliverable | Exit criterion | State |
|---|---|---|---|
| **G0 engine** | `src/ultragrid`, CLI `ultragrid`, Python reference, pinned vectors | 15 tests green; Python `--check` agrees | **done** |
| **G1 phone** | Kotlin port (same vectors) and a playable screen, no prizes | the Kotlin test asserts the same ten ids and commitment; ten levels playable on the emulator | next |
| **G2 service** | answer verification, receipts, day close + anchor via `appchain-cli`, no money | a day's `day_root` is anchored and recomputable from the public API | |
| **G3 payouts** | payout wallet, ledger, withdrawals, caps | three wins pay, the fourth is `recorded`; a withdrawal returns a hash | |
| **G4 phase 2** | entry fees or sponsors replace the owner's pool | legal review first; the pool is no longer the owner's | |

G1 ships on its own and is worth doing first: it is the whole game, and none of it depends on money.

## 10. Try it

```bash
cargo run --bin ultragrid -- day --date 2026-11-16                     # the ten puzzles
cargo run --bin ultragrid -- reference --date 2026-11-16 --level 1     # a known-good answer: 9:1,5:2
cargo run --bin ultragrid -- verify --date 2026-11-16 --level 1 --answer "9:1,5:2"
python3 tools/ultragrid_reference.py --check
```
