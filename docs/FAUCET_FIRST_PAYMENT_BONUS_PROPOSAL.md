# First-payment bonus — proposal and anti-farming design

**Status:** proposal, not implemented
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
| R6 | Payment occurs within `BONUS_WINDOW_SECONDS` of the claim (recommend 14 days) | claiming a bonus months later on a cold address |
| R7 | No cycle in the bonus window: the recipient has not paid the claimant, and the claimant has not paid that recipient before | ping-pong between two colluding wallets |
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

| Phase | Work | Value |
|---|---|---|
| 0 | Operator-run script: read claims + the node, list qualifying payments, pay them by hand | proves the mechanic before any code path |
| 1 | `bonuses` table, endpoint, worker payout, reason codes; R1–R6 and R9–R10 | the feature, automated |
| 2 | R7/R8 graph checks and the separate budget window | closes ping-pong and fan-out |

Phase 0 is worth doing first: if almost nobody makes a payment after claiming, the bonus is premature
regardless of how well it resists farming.

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
