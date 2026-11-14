# First-payment bonus — proposal and anti-farming design

**Status:** proposal, not implemented. §9's five decisions are settled; §10 is the implementation
spec derived from them.
**Depends on:** the 0.1 $ULTRA claim policy (`FAUCET_CLAIM_AMOUNT_BASE_UNITS=100000`)
**Question it answers:** how to reward the behaviour we actually want — a first payment to another
person — without paying bots or wash-traders.

## 1. Why the naive version fails

"Pay 0.1 $ULTRA for a user's first outgoing transfer" is trivially farmed:

1. claim 0.1, send it to a second wallet you control, receive the bonus, repeat;
2. two colluding wallets ping-pong 0.05 back and forth, both collect;
3. a script claims N addresses, sends each 0.05 to one shared sink, collects N bonuses.

Every rule below exists to close one of those three loops. The goal is not to be unspoofable — at this
size nothing is — but to make farming cost more effort than the bonus is worth.

## 2. Definitions

- **Claimant** — an address with a *confirmed* faucet claim (`claims.status = 'confirmed'`).
- **Qualifying payment** — a confirmed transfer where the claimant is the sender, the amount is at
  least `BONUS_MIN_PAYMENT_BASE_UNITS`, the recipient is not the claimant, not the faucet address, and
  not the bonus sink itself.
- **Bonus** — a second transfer from the faucet signer to the claimant, once, after a qualifying payment.

## 3. Rules for eligibility (all must hold)

| # | Rule | Closes |
|---|---|---|
| R1 | A confirmed claim exists and has no bonus row yet (`claim_id` unique) | re-claiming the bonus |
| R2 | The qualifying payment's sender equals the claiming address | bonus for someone else's payment |
| R3 | Amount ≥ threshold (recommend `50,000` = 0.05 $ULTRA) | dust payment crafted only to unlock the bonus |
| R4 | Recipient differs from the sender **and** from the faucet address | self-send, faucet round-trip |
| R5 | The payment is `confirmed` on chain, not merely accepted | paying the bonus for a transfer that never settled |
| R6 | The payment is within **14 days** of the claim *and* the bonus is requested within **7 days** of the payment (D3) | claiming a bonus months later on a cold address, or banking a payment and requesting late |
| R7 | The recipient has not paid the claimant inside the same window — the direct two-party ring is **blocked** (D5); longer rings are **flagged** for review, not refused | ping-pong between two colluding wallets |
| R8 | Recipient is not an address previously funded by the same claimant under the same abuse digest | one person with N wallets fanning out and collecting |
| R9 | Bonus submission is bounded by the same per-IP/per-subnet abuse digests as claims | scripted mass submission |
| R10 | The separate bonus budget has not been exhausted | one incident draining the float |

R7 and R8 need lookups the faucet does not do today, but both are cheap: the node exposes
`GET /api/address/{address}/transactions?limit=…`, which the dashboard already uses. For R8 the faucet
stores *digests*, never raw addresses of unrelated users, and compares only against addresses it
already knows from earlier bonus evaluations.

## 4. Amount and budget

Recommend the bonus equal to one claim (`100,000` base units), because a smaller bonus is invisible
and a larger one changes what the product is.

**The budget consequence must be stated plainly:** bonuses spend the same float. If every claim earns a
bonus, 1,209 $ULTRA funds roughly half as many users (about 4,900 instead of 9,998). That is a funding
decision, not a bug. Two honest options:

- **A. Same float** — accept ~4,900 users, each receiving 0.2 in total.
- **B. Separate budget** — top the faucet up so bonuses do not consume the claim budget, and give the
  bonus its own daily cap (recommend `5.000000` $ULTRA a day, about 49 bonuses).

Recommend **B**, with the bonus daily cap deliberately smaller than the claim cap, so a farming
incident cannot outrun the claim intake.

## 5. State

```sql
CREATE TABLE bonuses (
    bonus_id          TEXT PRIMARY KEY NOT NULL,
    claim_id          TEXT NOT NULL UNIQUE REFERENCES claims(claim_id),
    payer_address     TEXT NOT NULL,
    payment_hash      TEXT NOT NULL UNIQUE,
    recipient         TEXT NOT NULL,
    amount_base_units INTEGER NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('eligible','rejected','queued','paid','failed')),
    rejected_reason   TEXT,
    created_at        INTEGER NOT NULL,
    paid_hash         TEXT,
    paid_at           INTEGER
);
CREATE INDEX bonuses_payer_idx ON bonuses(payer_address, created_at);
```

`payment_hash` unique is what makes double-claiming one payment impossible; `claim_id` unique is what
makes double-claiming one user impossible. Both are database constraints, not application checks,
because the application is where bugs live.

Bonus debits are recorded in their own daily window so they cannot silently consume the claim budget,
and the existing `service_state` kill switch stops both.

## 6. Flow

1. The app, after a confirmed transfer, calls `POST /api/faucet/bonuses` with `claim_id` and the
   transaction hash, plus an `Idempotency-Key` (same contract as claims).
2. The faucet verifies the hash belongs to the claimant and satisfies R2–R7 by reading the node.
3. On success it inserts an `eligible` row, then the existing worker pays it exactly like a claim
   envelope: sign, persist, submit, poll, mark `paid`.
4. Failure is reported with a reason code, never silently: `BELOW_MINIMUM`, `SELF_PAYMENT`,
   `CYCLE_DETECTED`, `WINDOW_EXPIRED`, `BUDGET_EXHAUSTED`, `ALREADY_CREDITED`.

## 7. What this does not solve

- **Exchange withdrawals**: paying to an exchange hot wallet counts as a payment and will earn a bonus.
  That is acceptable — it is still a real user spending real tokens.
- **Off-app payments**: a user who pays from a different wallet gets nothing. Document it rather than
  loosening R2, which is the load-bearing rule.
- **Determined farmers**: R7/R8 raise the cost; they do not make farming impossible at 0.1 per user.
  The real ceiling is the daily bonus cap, which is why it must be lower than the claim cap.

## 8. Phases

| Phase | Work | Exit criterion |
|---|---|---|
| 0 | **Measure first.** Operator script reads `claims` and the node and answers one question: of the addresses that claimed, how many made an outgoing payment of ≥ 0.05 within 14 days? | a number, recorded in this file. Below roughly one in five, stop — the bonus is premature regardless of how well it resists farming |
| 1 | `bonuses` table and migration, endpoint, worker payout, reason codes: R1–R6, R9, R10 and the **D5 block** | one real bonus paid end-to-end and one direct ring refused, both visible in the store |
| 2 | The **D5 flag**: edge table, review queue, operator listing (`/internal/bonuses?status=review`), and the daily/global bonus caps enforced in their own window | a flagged ring appears for review and no longer auto-rejects a reciprocal pair of genuine payers |
| 3 | App surface: after a confirmed transfer, the receipt says the bonus is available and offers one tap | the offer is only shown when the claim exists locally and the endpoint says `eligible` |

Phase 0 should run before any code is written. Phase 2 is where the graph work lives, and it can be
deferred: until the volume exists, the direct-ring block plus the daily cap is the whole defence.

## 10. Implementation spec, refined against §9

Everything below is written to drop into the existing faucet crate (`src/faucet/`), reusing the claim
path rather than inventing a parallel one.

### 10.1 Configuration

| Key | Value | Source |
|---|---|---|
| `FAUCET_BONUS_ENABLED` | `false` until phase 1 ships | new; mirrors `FAUCET_ENABLED` |
| `FAUCET_BONUS_AMOUNT_BASE_UNITS` | `100000` | D1 |
| `FAUCET_BONUS_MIN_PAYMENT_BASE_UNITS` | `50000` | D2 |
| `FAUCET_BONUS_CLAIM_WINDOW_SECONDS` | `1209600` (14 days) | D3 |
| `FAUCET_BONUS_REQUEST_WINDOW_SECONDS` | `604800` (7 days after the payment) | D3 |
| `FAUCET_BONUS_DAILY_CAP_BASE_UNITS` | `5000000` (5 ULTRA ≈ 49 bonuses) | D4 |
| `FAUCET_BONUS_GLOBAL_CAP` | `2000` bonuses | D4 |
| `FAUCET_BONUS_CYCLE_MODE` | `block_direct` | D5 |

`check-config` must reject a bonus daily cap that exceeds the claim daily cap, and a bonus amount
that is a multiple other than 1× the claim amount without an explicit override comment. The point of
D4 is that a farming incident cannot outrun claim intake; a config that breaks that is a refusal at
startup, not a warning in a log.

### 10.2 Schema (migration `user_version` 1 → 2)

```sql
CREATE TABLE bonuses (
    bonus_id          TEXT PRIMARY KEY NOT NULL,
    claim_id          TEXT NOT NULL UNIQUE REFERENCES claims(claim_id),
    payer_address     TEXT NOT NULL,
    payer_digest      BLOB NOT NULL,
    payment_hash      TEXT NOT NULL UNIQUE,
    recipient         TEXT NOT NULL,
    amount_base_units INTEGER NOT NULL,
    status            TEXT NOT NULL CHECK (status IN
                        ('eligible','review','queued','paid','rejected','failed')),
    reason_code       TEXT,
    created_at        INTEGER NOT NULL,
    queued_at         INTEGER,
    paid_hash         TEXT,
    paid_at           INTEGER
);
CREATE INDEX bonuses_payer_idx ON bonuses(payer_digest, created_at);
CREATE INDEX bonuses_status_idx ON bonuses(status, created_at);

-- D5: one row per observed payment edge between an evaluated claimant and a counterparty.
CREATE TABLE bonus_edges (
    payer_digest      BLOB NOT NULL,
    counterparty      TEXT NOT NULL,
    direction         TEXT NOT NULL CHECK (direction IN ('out','in')),
    first_seen_at     INTEGER NOT NULL,
    PRIMARY KEY (payer_digest, counterparty, direction)
);

CREATE TABLE bonus_windows (
    window_start_utc      INTEGER PRIMARY KEY NOT NULL,
    window_end_utc        INTEGER NOT NULL,
    reserved_base_units   INTEGER NOT NULL CHECK (reserved_base_units >= 0),
    confirmed_base_units  INTEGER NOT NULL CHECK (confirmed_base_units >= 0),
    bonus_count           INTEGER NOT NULL CHECK (bonus_count >= 0),
    global_bonus_count    INTEGER NOT NULL CHECK (global_bonus_count >= 0)
);
```

`payer_digest` is the same abuse-keyed digest the claim path already stores (`address_digest`), so
this table never holds a second copy of an address that the claims table could leak.

Two states exist for reasons that matter: `review` is a **paid-nothing** state that waits for an
operator, and `rejected` is a terminal refusal the app can explain. Collapsing them into one state
would make D5 unrepresentable.

### 10.3 Endpoint

```
POST /api/faucet/bonuses        { "claim_id": "...", "payment_hash": "..." }
GET  /api/faucet/bonuses/{id}   -> { status: eligible|review|queued|paid|rejected|failed }
```

Same contract as claims: `Content-Type: application/json`, an `Idempotency-Key`, `202` with the
bonus id on acceptance, and polling for the outcome. `202` means "accepted for evaluation" and is
never a payout.

Evaluation order matters, because the reason code should name the first thing that failed:

| Order | Check | Reason code | Outcome |
|---|---|---|---|
| 1 | `claim_id` exists and is `confirmed` | `NO_CLAIM` | 422 |
| 2 | No bonus row for this `claim_id` | `ALREADY_CREDITED` | 409 |
| 3 | Payment hash known and no row for it | `PAYMENT_NOT_FOUND` | 422 |
| 4 | Sender is the claimant | `NOT_SENDER` | 422 |
| 5 | Recipient ≠ sender, ≠ faucet address | `SELF_PAYMENT` | 422 |
| 6 | Amount ≥ 50,000 | `BELOW_MINIMUM` | 422 |
| 7 | Payment confirmed, and within 14 days of the claim | `WINDOW_EXPIRED` | 422 |
| 8 | Requested within 7 days of the payment | `REQUEST_EXPIRED` | 422 |
| 9 | D5 block: recipient has not paid the claimant in the window | `CYCLE_BLOCKED` | 422 |
| 10 | D5 flag: claimant has paid this recipient before, or the recipient is in a longer ring | — | `review`, 202 |
| 11 | Bonus window has room, global cap not reached | `BUDGET_EXHAUSTED` / `GLOBAL_CAP_REACHED` | 429 |
| 12 | Per-IP / per-subnet digest has room (R9) | `RATE_LIMITED` | 429 |

Steps 9 and 10 need the node's transaction history
(`GET /api/address/{address}/transactions?limit=…`). Cache the answer per address for the life of
one evaluation; do not walk a graph inside a request.

### 10.4 Payout

`eligible` → the existing worker picks it up exactly like a claim: sign, persist to `payouts`,
submit, poll, mark `paid`. Two rules carry over unchanged and must be re-asserted for bonuses:

- the faucet's own nonce is read from `service_state.last_observed_nonce`, never assumed;
- `service_state.enabled = 0` stops bonus payout as well as claims.

The bonus debits `bonus_windows`, not `budget_windows`. A bonus that cannot debit its own window is
not queued — it is refused with `BUDGET_EXHAUSTED`, because the alternative is silently halving the
claim runway that D4 exists to protect.

### 10.5 Tests

| Test | Asserts |
|---|---|
| `bonus_requires_confirmed_claim` | a `pending` claim cannot earn a bonus |
| `bonus_pays_once_per_claim` | a second request with a fresh payment hash returns `ALREADY_CREDITED` |
| `bonus_rejects_dust` | 49,999 base units → `BELOW_MINIMUM`; 50,000 → eligible |
| `bonus_rejects_self_and_faucet` | recipient = sender, and recipient = `service_state.faucet_address` |
| `bonus_windows_are_independent` | 50 bonuses drain `bonus_windows` and leave `budget_windows` untouched |
| `direct_ring_is_blocked` | A→B then B→A returns `CYCLE_BLOCKED` for the second side |
| `longer_ring_is_flagged_not_refused` | A→B→C→A yields one `review` row and no rejection |
| `global_cap_stops_payout` | at 2,000 bonuses the endpoint returns `GLOBAL_CAP_REACHED` |
| `kill_switch_stops_bonus_payout` | `enabled = 0` leaves queued bonuses unsubmitted |

### 10.6 Deliberately out of scope

- **Exchange and off-app payments** — unchanged from §7: paying a hot wallet earns the bonus, paying
  from a wallet the app never saw does not.
- **App-side push** — phase 3. The receipt's offer must be driven by the endpoint's answer, never by a
  client-side guess, or the app will promise a bonus the faucet then refuses.
- **Retroactive bonuses** — the table starts empty. Phase 0 is an operator script, not a migration.

## 9. Decisions

These are the recommended values, chosen so the bonus rewards a genuine first payment without
competing with the claim budget.

| # | Decision | Value | Why |
|---|---|---|---|
| D1 | Bonus amount | **0.1 $ULTRA** (`100,000`), equal to one claim | the same number the user already saw; a half-bonus reads as an error, not a reward |
| D2 | Minimum qualifying payment | **0.05 $ULTRA** (`50,000`) | it is half the grant, so the user actually spends something they would notice; a 0.05 payment costs 0.0005 in fees |
| D3 | Window | **14 days** from the claim, and the bonus must be requested within **7 days** of the payment | long enough for a user to find someone to pay, short enough that the signal stays tied to onboarding |
| D4 | Budget | **Separate**: `5.000000` $ULTRA a day (≈49 bonuses) plus a 2,000-bonus global cap, recorded in its own window | paying bonuses from the claim budget would halve reach from 9,998 to ~4,900; a smaller bonus cap also means a farming incident cannot outrun claim intake |
| D5 | Cycle detection (R7) | **Block** the direct two-party case (A pays B and B pays A, both bonus-eligible); **flag** longer rings for operator review in phase 1 | blocking the trivial ring is cheap and certain; automatic rejection of long rings would produce false negatives on legitimate reciprocal payments between people who actually know each other |

With D4 in place, the claim promotion keeps its full 9,998-call runway and the bonus is bounded
independently. Phase 0 (manual payment of qualifying bonuses) should still come first: if almost nobody
pays after claiming, none of this is worth automating.
