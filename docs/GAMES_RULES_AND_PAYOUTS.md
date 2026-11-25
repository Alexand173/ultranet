# Games rules and payout rules — review

**Date:** 2026-11-16 · **Covers:** UltraGrid (AppChain #1) and Ultra Chess (AppChain #3, registered; #2 is the Records Registry)
**Code:** `src/games/` (rules, store, service, API), `src/bin/ultranet-games.rs`, the Android screens under
`ui/grid`, `ui/chess`, `ui/games`.
**Money authority:** [`ECONOMIC_ARCHITECTURE_V2.md`](./ECONOMIC_ARCHITECTURE_V2.md) §5. This document applies
it; where they disagree, V2 wins once ratified.
**Operations:** [`runbooks/games-service.md`](./runbooks/games-service.md)

**What this is.** A review of the rules as built, with the numbers the service actually enforces, the
problems the review found (three were fixed while writing it), and the decisions only the owner can make.
It is not a legal opinion, an audit by someone else, or a promise that any number is "safe": it says which
numbers bound which loss, and which risks no number bounds.

---

## 1. At a glance

| | UltraGrid | Ultra Chess |
|---|---|---|
| What the server can prove | **Everything.** It replays the answer on the puzzle it generated itself | **Nothing.** The opponent runs on the phone; the server is told the result |
| Reward per rewarded win | **10 units** (0.00001 `$ULTRA`), a *test* reward (V2 R4b) | **Nothing — record-only by default** (P1). 100 units (0.0001 `$ULTRA`) only once switched on |
| Rewarded wins per address per day | 3 | none, while it pays nothing; 3 once switched on |
| Recorded wins per address per day | unlimited (up to 10 levels) | unlimited |
| What a win must be | connected within `par + max(2, par/4)` turns | a player win, 10 to 600 plies, a game id never seen before |
| What the day anchors | the puzzle set **and** every accepted win | every accepted win |
| Anchored to | AppChain #1 (live) | AppChain #3 (registered; set `GAMES_CHESS_CHAIN_ID=3`, which is required) |
| Marked in every receipt | `verified = true` | `verified = false`, and the app says so |

Most one address can earn in a day, as shipped: `3 × 10 =` **30 units = 0.00003 `$ULTRA`**. With chess
switched on it becomes `30 + 300 =` **330 units = 0.00033 `$ULTRA`**, and never more than that.

---

## 2. The rules of play

### 2.1 UltraGrid

- A day is ten puzzles, the same for everyone, built from a seed that binds the chain, the date and the
  **last UltraGrid anchor mined before 00:00 UTC** (`ULTRAGRID_GAME_SPEC.md` §2). The operator cannot choose
  a day's puzzles after seeing who is playing.
- An answer is a list of `(tile, quarter-turns)`. The server replays it. Order does not matter, only the
  net turn per tile, so five taps on a bend are one turn.
- A **win** is *connected within the move limit*. `par` is what the puzzle was built around and is not a
  claimed minimum; a shorter route beats it.
- **One reward per `(address, date, level)`**, by a database constraint. Re-sending the same win returns
  the stored result and adds nothing.
- The first **three** levels an address solves in a UTC day are rewarded. The rest are recorded, anchored
  and unpaid, and the response says `recorded` and why. Which three is the player's choice: the rule rewards
  *solving*, not difficulty.
- The day is the **server's UTC date**. An answer for any other date is refused with the server's date, and
  the phone tells the player to fix their clock. Nothing is silently shifted.
- A practice set (the phone could not read the chain) is never sent and never rewarded.

### 2.2 Ultra Chess

- **It pays nothing by default, and that default lives in the code, not in a config file.**
  `Policy::standard()` sets `chess.pays = false`, and only `GAMES_CHESS_PAYS=true` turns it on, so an
  installation whose environment file is missing that line records and anchors every win and credits
  none. `/api/games/status` reports `pays: false` for the game, and `check-config` prints
  `RECORD ONLY`, so nothing has to guess.
- The player is White, against a local engine, in Classic, Random or Anti-Chess.
- The phone reports a **player win** with a game id (a fresh UUID per game), the mode and the ply count.
- **The server cannot verify any of it.** It can check the report is *plausible* (a known mode, 10 to 600
  plies, a well-formed unused game id) and nothing more. A scripted client can report wins; a person at a
  desk with a chess engine can beat this opponent every time; one person can play both sides.
- Because that is true, chess can never be protected by *proof* — only by caps, and those only matter
  once it is switched on: 3 rewarded wins per address per day, 200 a day for the game (the code default,
  not only the environment file's), the client limit, the on-chain presence rule, the pool check (§3) and
  one win per game id. V2 says the same thing in one line: the chain can verify none of it, so every
  protection is a cap.
- **If it is switched on, watch it rather than trust it.** The honest position is that chess should pay
  only while you are willing to lose the daily ceiling, and should get a real prize only once a
  server-side opponent exists — a server that replays its own game is the one thing that makes a chess
  result checkable. Until then `GAMES_CHESS_PAYS=false` costs nothing: the wins are still recorded and
  anchored, so the data is there the day the rule changes.

---

## 3. The rules of payment

Every parameter, its default, and the ceiling the service will not exceed. Defaults follow V2; a value
above a ceiling stops the service starting (`ultranet-games check-config` says which) rather than being
clamped. Tightening is always allowed.

| Rule | Default | Ceiling | Why this number |
|---|---:|---:|---|
| Grid reward | 10 | 1,000 | V2 R4b: a test value until the pool ends |
| Chess reward | 0 (record-only) | 1,000 | Not paid at all unless `GAMES_CHESS_PAYS=true` (P1). 100 is V2 R4's value for the day it is switched on |
| Rewarded wins per address per day, per game | 3 | 10 | V2 R4; bounds one address's day at 330 units |
| Rewarded wins per game per day | 2,000 Grid / 200 Chess | 10,000 | V2 R4; one game cannot monopolise the budget. Chess is 200 because that is the ceiling the review argues for (P1), and switching it on should not depend on remembering a second number |
| Rewarded wins per day, all games | 20,000 | 100,000 | V2 R4: the backstop whatever the number of games |
| Rewarded wins per client per day | 30 | 1,000 | A *flood* limit. Mobile carriers share addresses, so a tight per-IP cap punishes real players; the per-address cap does the farming work |
| Reward only addresses on the chain | yes | — | Without it an attacker mints addresses and drains the day's budget |
| Withdrawal minimum | 100 | 10,000,000 | The network fee is 1 % with a floor of 1, so below 100 a payout is mostly fee |
| Withdrawn amount | the whole balance | — | No partial payouts to track |
| Payout fee | paid by the payout wallet | — | V2: never the player |
| Queued for withdrawal per day | 200,000 | 5,000,000 | Defence in depth behind the wallet's own balance |
| Fee reserve inside the payout wallet | 1,000 | floor 100 | Fees come out of the wallet on top of every payout |
| Confirmation wait before a human is called | 900 s | 3,000 s | A signed transfer older than an hour can never be accepted |

### 3.1 The accounting model

Every balance is three numbers per address: **credited** (rewards), **paid** (confirmed on the chain) and
**reserved** (queued or in flight). Available is `credited − paid − reserved`. Credit is created only by a
rewarded win, in the same transaction that records it. `paid + reserved ≤ credited` is also a database
`CHECK`, independent of the code. `ultranet-games audit` re-derives every balance from the wins and payouts
that should explain it and exits non-zero on any disagreement; run it before trusting a restored backup.

### 3.2 A payout's life

```
withdraw ─► queued ─► signed ─► submitted ─► confirmed        (paid; credit becomes paid)
              │          │          │
              │          └──────────┴──► paused ─► an operator resolves it against the chain
              └─► failed (never signed: the balance is released)
```

1. **Queued.** The whole balance is reserved at once, so the same credit cannot be queued twice. Asking
   again returns the payout already open.
2. **Signed, once.** The wallet signs with the node's own nonce and the node's own fee estimate. The signed
   request is **stored before it is submitted**, and every later attempt sends those exact bytes.
3. **Confirmed only by the chain.** The payout is paid when the node reports it confirmed, not before.
4. **Anything unexpected pauses payments.** A mismatched hash, a rejected signed transfer, a payout that
   never confirms, an empty wallet, a missing signer: payments pause with a reason, the payout stays open,
   the balance stays reserved, and nothing is guessed. Results keep being accepted and credited while paused.
5. **An operator resolves it** with the chain in front of them (`payout-resolve --confirmed|--failed
   --note`). *Failed* releases the balance; *confirmed* books it as paid. There is no automatic "try
   again with a new signature", because a second signature could pay twice.

### 3.3 Anchoring

One anchor per game per day, signed by the chain's owner key through `appchain-cli` **in the Records Service** (this service asks it, over a signed loopback request, and holds no owner key), at 1,000 units (0.001
`$ULTRA`) from the **owner's** wallet, not from the pool. The day's root is a hash over a canonical, sorted
list of results (and, for Grid, the puzzle set), so anyone holding the same list recomputes and compares it.
Yesterday is anchored even when nobody won, so a listed chain stays "live" (V2 C-2). A day closes once and
its root is never recomputed. Before submitting, the service looks for the root on the ledger, so an anchor
whose outcome was unclear is settled without paying for a second one.

---

## 4. What the review found

### 4.1 Fixed while writing this

| # | Finding | Why it mattered | Fix | Test |
|---|---|---|---|---|
| F1 | **The pool could be over-promised.** The daily ceilings allow ~0.22 `$ULTRA` of rewards a day; the pool is 0.1 | Players would see balances the wallet cannot pay, and withdrawals would stall | A reward is granted only if *everything already owed + this reward + the reserve* fits inside the payout wallet's balance, read from the chain at that moment. Otherwise the win is recorded as `pool_low`. If the balance cannot be read the win is not recorded and the phone sends it again | `a_reward_is_never_promised_beyond_what_the_wallet_holds` and five more |
| F2 | **Anyone could pause everyone's payouts** by submitting a win for the payout wallet's own address: the signer refuses to pay itself, and a refusal pauses payments | One stranger, one request, payments stopped for all | That address is refused at every entry point; a payout addressed to it anyway is *released*, not a reason to pause | `the_rewards_wallets_own_address_cannot_win_look_up_or_withdraw`, `a_payout_somehow_addressed_to_the_wallet_is_released…` |
| F3 | **`GET /grid/day?date=` accepted any date** and wrote a row for each | Unbounded database growth from one client | Only today and yesterday; every other date is refused and leaves no row | the HTTP day-route test |

### 4.2 Strong, and why

- **Grid cannot be forged, only automated.** The server replays every answer on its own copy of the puzzle,
  and its copy is pinned byte-for-byte against the Rust engine, an independent Python port, the Kotlin app
  and the live chain (seed `7a87a896…`, commitment `1c9bc629…`).
- **Money moves in one direction through one door.** Credit exists only inside `record_win`; paying only
  inside the payout worker; each is a single transaction, and the audit proves the books balance.
- **No login to steal.** Money only ever goes *to the address that was credited*, so there is no session,
  no token and no admin HTTP route. Operator actions are commands on the host.
- **A bad payout cannot be made worse by the service.** Sign once, persist first, retry identically, pause
  on doubt, and a human decides.
- **The rules cannot be quietly loosened.** Hard ceilings in code, and `check-config` prints what is in force.

### 4.3 Real risks that remain, most serious first

| # | Risk | What bounds it | What does not |
|---|---|---|---|
| R1 | **Chess is unverifiable** (§2.2) | **Nothing is paid for it today.** If it is switched on: 200 rewarded wins a day for the game, 3 per address, the pool guard, and one reward per game id | Nothing proves a human played. Cheap for a script |
| R2 | **Anyone can automate Grid.** Routing a signal is an easy search | The same caps; one reward per level; chain presence | Verification stops *forged* wins, not *automated* ones |
| R3 | **Sybil addresses.** Chain presence only needs a balance or a transaction, so each new address costs one faucet claim | The faucet's own Turnstile and per-source limits; one faucet claim is worth ~3,300 days of a wallet's maximum Grid earnings, so farming rewards is not the better use of a claim | If the faucet is ever the weak gate, this rule is only as strong as it |
| R4 | **The owner key sits on the server (M2).** The chain's owner key and its passphrase are both credentials on the host | Encrypted at rest; a dedicated key per chain; `APPCHAIN_PROTOCOL.md` §5 | A compromised running host can anchor *and transfer* the chain. M3 (an anchor-only key) is not built |
| R5 | **One payout wallet** is one point of failure and one nonce sequence | Keep it small; the solvency guard; the daily payout cap; pause on any anomaly | It must never be used for anything else, or the nonce sequence breaks |
| R6 | **Fees erode the pool ~1 %** of everything paid, beyond what the reserve covers over time | The reserve; a top-up policy (§5) | Left alone, the wallet drifts below what is owed and rewards stop (safely) |
| R7 | **Clocks.** A day is a UTC day by the *server's* clock | The server refuses another date and says which; the phone says "fix your clock" | A wrong server clock moves the day boundary. Run time sync and monitor it |
| R8 | **Wallet pages are public.** `GET /wallet/{address}` shows any address's balance and recent wins | Addresses are already pseudonymous and public on-chain | It does reveal which addresses play. Say so in the privacy note |
| R9 | **Throughput.** Payouts are strictly one at a time, each waiting for a block | The daily cap keeps the queue short | At the cap a day's queue takes hours to drain |
| R10 | **Law.** Free play with tiny prizes is still a prize game in some places, and paid entry (V2 phase 2) is more so | None in code | Get advice before this leaves a test and before entry fees exist. Not a software question |

---

## 5. What this does not do, and what is still yours to do

Nothing here has been deployed and **no live payout has ever been made**. The only live traffic during
development was read-only GETs to the public API. Before opening:

1. **Decide** the open items in §6. P1 is now decided *in code* (chess record-only), so no step below
   can accidentally start paying for it; P2, P3 and P7 are still open.
2. **Create and fund the wallets.** The payout wallet (`ultranet-faucet keygen`) funded from the faucet's
   one-claim-per-chain allowance (V2 F-2: 0.1 `$ULTRA`, ≈ 9,900 Grid wins or 990 chess wins of capacity). The
   owner key (`appchain-cli generate-key`, held by the Records Service) funded for anchors: 0.001 a day per game.
3. **Hand over chain #1** to the server key (`runbooks/appchain-owner-key.md`), verify the owner on the ledger,
   and keep an encrypted offline copy of the key file and the passphrase **separately**. After the handover
   that file *is* the chain.
4. **Ultra Chess is AppChain #3** (already registered). Set `GAMES_CHESS_CHAIN_ID=3` (required, no default) and add `chess` to `GAMES_ANCHOR_GAMES`.
5. **Install** per the runbook: binary, unit, credentials, the reverse-proxy route, the Cloudflare limits,
   backups, and the monitoring in §7.
6. **Run `audit` and `check-config`**, then one real withdrawal of the smallest amount end to end with your
   own address before anyone else's.
7. **Release** the Android build that carries the rewards screens.

---

## 6. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| P1 | **Does chess pay?** | **Decided 2026-11-16: no — and the decision is enforced in the code, not in a config file.** `Policy::standard()` has `chess.pays = false`; only an explicit `GAMES_CHESS_PAYS=true` turns it on, so a missing line cannot start it and a malformed one is refused rather than read as "on". It stays off because the server can prove nothing about a chess result (R1): every unit it paid would be collectable by a program without a human playing. Wins are still recorded and anchored, so chain #3 is useful the day a server-side opponent exists |
| P2 | **Ratify V2 R4 / R4b** | Both are unsigned (☐) in the V2 sheet. The service follows them; sign them or change them there first |
| P3 | **Rewarded wins: per game or in total?** V2 says "per address per day" without saying which | Built *per game* (a day's worst case 330 units). *In total* would be 3 wins across both games: say if you want it. **While chess pays nothing this cannot bite** — the ceiling is 30 units a day either way — so it only needs answering the day chess is switched on |
| P4 | **Pool** | 0.1 `$ULTRA` (decided). It is the binding limit, not the ceilings; plan the top-up (R6) |
| P5 | **Key custody for the daily anchor** | M2 as chosen, with a dedicated key per chain and the offline backup. Revisit when M3 exists |
| P6 | **Heartbeat anchors** (anchor an empty day) | Keep: 0.001 a day per game to stay "live". `GAMES_ANCHOR_EMPTY_DAYS=false` turns it off |
| P7 | **Privacy wording** for the public wallet page (R8) | Add a line to the app and the privacy notice |

---

## 7. What to watch

| Signal | Where | Meaning |
|---|---|---|
| `ERROR … payouts paused` | journal | A human is needed. The log line says why; `ultranet-games status` and `payouts` show the rest |
| `audit` exits non-zero | cron, daily | The books disagree. Stop paying (`disable --paying`), investigate, do not restore blindly |
| Payout wallet balance vs `still owed` | `status` and the node | `owed + reserve` approaching the balance means rewards are about to stop: top up |
| Yesterday's `day` row not `confirmed` by 06:00 UTC | `ultranet-games close-day` output | The anchor did not land (check `journalctl -u ultranet-records` too); the service retries every 10 minutes |
| Owner key wallet balance | the node | Below ~30 days of anchors: top up |
| Server clock offset | `timedatectl` | R7 |

---

## 8. The invariants, and where each is tested

| Invariant | Test |
|---|---|
| A win pays once per `(address, day, level)` and once per chess game id | `the_same_win_twice_is_replayed_and_paid_once`, `a_chess_game_id_pays_once…` |
| The fourth win is recorded, never paid, and says why | `the_fourth_grid_win_of_a_day_is_recorded_and_unpaid`, `the_fourth_level_of_a_day_is_recorded_and_says_why` |
| Caps stop payment, never recording | `each_cap_stops_payment_but_never_recording`, `the_games_and_global_ceilings_stop_payment_not_recording` |
| Nothing is promised beyond the wallet | `a_reward_is_never_promised_beyond_what_the_wallet_holds`, `the_pool_is_checked_against_everything_owed_inside_the_same_transaction` |
| `paid + reserved ≤ credited` | `the_database_refuses_to_over_reserve`, `the_audit_catches_a_balance_that_money_does_not_explain` |
| A payout is signed once and retried with identical bytes | `a_payout_is_signed_exactly_once`, `an_unclear_submission_is_retried_with_identical_bytes` |
| Paid only on the chain's word | `a_full_withdrawal_signs_once_submits_and_settles_on_confirmation` |
| Any doubt pauses payments and keeps the payout open | the hash, rejection, timeout, signer and pool tests |
| A day closes once; the anchored root is the stored root | `a_day_closes_once_and_the_first_root_is_the_one_kept`, `a_finished_days_root_is_fixed…` |
| An anchor of unclear outcome is checked before it is repeated | `an_anchor_of_unclear_outcome_is_checked_on_the_ledger_before_it_is_repeated` |
| The roots match an independent computation | `the_roots_are_pinned` (Python-verified) |
| The rules cannot be loosened past a ceiling | `a_rule_cannot_be_loosened_past_its_ceiling` |
| A configuration without `GAMES_CHESS_PAYS` cannot start paying chess | `a_configuration_without_games_chess_pays_does_not_pay_chess`, `chess_is_record_only_by_default_and_still_records_and_anchors`, `chess_is_record_only_until_something_says_otherwise` |
| The server and the phone build the same puzzles | `GridEngineTest` and the live check against `/api/games/grid/day` |
