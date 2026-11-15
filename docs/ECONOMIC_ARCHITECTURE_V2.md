# UltraNet Economic Architecture v2

**Status:** proposed constitution — every number marked *ratify* needs the owner's signature before any code
changes. Nothing in this document is implemented yet except where §2 says "current".
**Date:** 2026-11-15 · **Supersedes:** the economics sections of `docs/APPCHAIN_FIRST_GAME_PLAN.md` §4 and
`docs/CHESS_APPCHAIN_PLAN.md` §5. This document is the authority; those are applications of it.
**Horizon:** 100 years, thousands of AppChains, one token.

> **The rule that makes this document real.** Any code change that alters a number in §5 must amend §5
> first, in the same commit, with the reason. A parameter that drifts in code while the document says
> otherwise is the failure mode this document exists to prevent.

---

## 1. What this document decides

UltraNet has six places where value moves, and they were designed at different times by different
questions. v2 fixes the relationship between them once:

| # | Relationship | Question it answers |
|---|---|---|
| R1 | L1 token emission | How much new `$ULTRA` may ever exist, and who decides? |
| R2 | Validator revenue | What does a validator earn, from what, and in what proportion? |
| R3 | AppChain costs | What does it cost to own a chain, forever? |
| R4 | AppChain reward pools | Where does prize money come from, and what may it never be? |
| R5 | Faucet distribution | How much onboarding subsidy exists, and when does it end? |
| R6 | Long-term sustainability | What keeps `$ULTRA` sound across generations of chains? |

---

## 2. The supply truth

`genesis.json` allocates **1,000,000 `$ULTRA`** (`1,000,000,000,000` base units) to the sovereign
address, and the only mechanism that creates more is the emission schedule. There is **no declared cap
in the code** — the cap is arithmetic, and it needs writing down.

`1 $ULTRA = 1,000,000 base units` (`microULTRA`, 6 decimals). Every price below is an integer count of
those units.

| Quantity | Value | Source |
|---|---|---|
| Genesis allocation | 1,000,000.000000 `$ULTRA` | `genesis.json` → `allocation` |
| Emission per block, era 0 | 50 base units (`0.000050`) | `GENESIS_REWARD`, `lib.rs:1397` |
| Halving interval | 31,557,600 blocks ≈ 10.008 years | `ai_governor.rs:95` |
| Era rewards per block | 50, 25, 12, 6, 3, 1, then 0 | `50 >> era` |
| **Total emission, all eras** | **3,061.0872 `$ULTRA`** | 97 × 31,557,600 base units |
| **Maximum supply, forever** | **1,003,061.0872 `$ULTRA`** | allocation + all emission |
| Era-0 emission per year | 157.68 `$ULTRA` | 3,153,600 blocks × 50 |
| Era-0 inflation against allocation | **0.0158 % / year** | 157.68 ÷ 1,000,000 |
| Supply today (live, block 13) | 1,000,000.000650 `$ULTRA` | 13 × 50 base units emitted |

**Ratify R1:** keep the schedule as written, and declare the cap **1,003,061.0872 `$ULTRA`** as a
protocol constant with a test that pins it. Emission is the *only* mint in the system, forever. The
`× 0.8` transaction-density multiplier in `ai_governor.rs:111` may only ever *reduce* emission, never
raise it above 50 per block.

Why the cap matters more than the inflation rate: 0.0158 %/year is economically invisible, but an
undefined cap is a governance hole. A number that cannot be changed without a document change is worth
more than three hundredths of a percent.

---

## 3. R2 — Validator revenue

### 3.1 The rule

A validator earns from exactly two sources, and both must be stated as fractions of a defined pool:

1. **Seigniorage** — the block reward, distributed **by weight** among active validators
   (`distribute_rewards`, `lib.rs:582`).
2. **Transaction fees** — 100 % of every fee in a block, split **per `validator_set` entry with integer
   division** (`lib.rs:3779`, `lib.rs:4035`).

### 3.2 What is true today

| Fact | Consequence |
|---|---|
| `total_fees / validator_set.len()`, remainder dropped | An anchor's 1,000 across 6 validators pays 166 × 6 = 996; **4 base units are destroyed**. A 1-unit transfer fee pays **nothing at all** |
| Split is per set entry, not per weight | A validator with weight 1 and one with weight 8 receive the same fee share |
| `/api/stats` reports `validator_count: 6` | With five placeholder records, the one real validator collects **one sixth** of all fees and two thirds of a reward distribution it did not earn |
| `stake` is `weight × 1000`, set at registration | It is bookkeeping. **No `$ULTRA` is ever bonded**, so nobody loses money for misbehaving; `slash_validator` removes a key after three strikes and slashes nothing |
| Emission dominates fee revenue | Era-0 emission is 0.432 `$ULTRA`/day; fees only match it at ~108 chains anchoring 4×/day (§7) |

### 3.3 v2 proposal

**V-1 (ratify).** Fees are split **by weight**, and the integer remainder goes to the **block producer**.
No unit is destroyed. A 1-unit fee pays that unit to someone.

**V-2 (ratify).** Only validators that are **active and weight > 0** participate. Placeholder and demo
records are `is_active = false` and cannot receive fees or rewards. This is a one-line economic fix with
a large effect: the real validator's fee income multiplies by six the day it lands.

**V-3 (ratify, later).** A **bond** of 1,000 `$ULTRA` (100,000,000 base units) is required to validate,
held from the validator's own balance, and forfeited to the fee pool on the third slash. This is what
turns `stake` from a label into money at risk. It is deliberately **phase 4**, because it needs its own
consensus change and a migration for the current roster.

### 3.4 The honest number

At 6 active validators and era-0 emission, a validator earns `157.68 ÷ 6 ≈ 26.28 $ULTRA` a year from
seigniorage plus all fees. That is the entire revenue of running a consensus validator on this network
today. Anyone deciding to validate should read that number first.

---

## 4. R3 — AppChain costs

### 4.1 The price list

| Action | Constant | Base units | `$ULTRA` | Payer |
|---|---|---:|---:|---|
| Register a chain | `APPCHAIN_REGISTER_FEE` | 10,000 | 0.01 | owner, once, permanent |
| Anchor a state root | `APPCHAIN_ANCHOR_FEE` | 1,000 | 0.001 | owner, per commit |
| Transfer ownership | `APPCHAIN_TRANSFER_FEE` | 1,000 | 0.001 | owner, per handover |
| Plain transfer | `max(1, amount / 100)` | 1 % (floor 1) | 1 % | sender |

### 4.2 The rule that matters

**Anchoring frequency is the only real cost lever.** Everything else is noise:

| Cadence | Anchors/day | `$ULTRA`/year |
|---|---:|---:|
| Daily | 1 | **0.365** |
| Twice daily | 2 | 0.730 |
| Every 6 hours | 4 | 1.460 |
| Hourly | 24 | 8.760 |

A 32-byte root is a 32-byte root. The L1 cannot price by state size, because the state root is a fixed
size by construction — so **frequency is the only honest meter**, and v2 says so instead of pretending
otherwise.

**C-1 (ratify).** Fees stay flat: 10,000 to register, 1,000 per anchor, 1,000 per handover.

**C-2 (ratify).** A chain that wants to be **listed as live** in the app must anchor at least **once per
day**. This is a product rule enforced by the app, not a consensus rule — the L1 never judges it. It
exists so that "anchored today" means something.

**C-3 (proposal, phase 3).** If the registry grows past 10,000 chains, the anchor fee halves per order of
magnitude of chain count (1,000 → 500 → 250), so the registry stays affordable at a million chains while
L1 revenue still grows with the number of chains. Deliberately not now: a fee schedule that changes
underneath owners needs its own activation-height process.

---

## 5. R4 — AppChain reward pools

### 5.1 The rule, in one sentence

**An AppChain may redistribute `$ULTRA`; it may never create it.** Rewards are transfers from a funded
wallet, and the L1 cannot verify any of it — so every protection has to be a cap, stated publicly.

### 5.2 Funding sources, in priority order

1. **Entry fees** paid by participants (phase 2 of any game; needs legal review before charging).
2. **Sponsors** — external money, named on screen.
3. **The owner's own `$ULTRA`.**

Never: minting, never the faucet as a recurring source, never another chain's pool.

### 5.3 v2 standard parameters

| Parameter | Value | Rationale |
|---|---:|---|
| Standard reward per paid win | **100 base units** (`0.0001`) | chess plan's value; a token of a win, not income |
| Rewarded wins per address per day | **3** | 0.0003/day, 0.1095/year ceiling per address |
| Wins *recorded* per address per day | unlimited | the chain is a record of play, not a payment ledger |
| Rewards per chain per day | **2,000** (0.202 `$ULTRA`) | one chain cannot monopolise the budget |
| Rewards globally per day | **20,000** (2.020 `$ULTRA`) | the backstop, whatever the number of chains |
| Withdrawal minimum | **100 base units** | 10 wins at the standard reward |
| Payout fee | 1 base unit (1 % floor) | paid by the payout wallet, never the player |
| Payout wallet | **separate key from the faucet signer** | a prize bug must never strand onboarding payouts |

**Grid's 10-unit reward is a phase-1 exception**, not the standard: it is a test pool of 1 `$ULTRA` with
an announced end date, and the app must say "test reward" on the screen. When that pool ends, Grid adopts
the 100-unit standard or stops paying.

### 5.4 Why the off-chain ledger is part of the economics, not an implementation detail

| Pay per win | Credit and pay out at 100 |
|---|---|
| 10-unit transfer, fee floor 1 → **10 % overhead**, one L1 transaction per win | 100-unit transfer, fee 1 → **1 % overhead**, one transaction per 10 wins |

The fee floor of 1 unit is what makes micro-payouts structurally expensive. Batching is the difference
between 10 % and 1 %, which is why R4 requires a ledger plus a withdrawal minimum instead of instant
payouts.

---

## 6. R5 — Faucet distribution

### 6.1 What it is

An **onboarding subsidy**: one claim proves a wallet exists and is worth a tenth of a coin. It is **not**
income, not a prize, and not a marketing budget that can be refilled from emission.

### 6.2 Current parameters (live, verified)

| Parameter | Value | Source |
|---|---:|---|
| Claim | 0.1 `$ULTRA` (100,000 base units) | `DEFAULT_CLAIM_AMOUNT_BASE_UNITS` |
| Daily debit cap | 10 `$ULTRA` (10,000,000) | `DEFAULT_DAILY_DEBIT_CAP_BASE_UNITS` |
| Minimum reserve | 200 `$ULTRA` (200,000,000) | `DEFAULT_MIN_BALANCE_RESERVE_BASE_UNITS` |
| Cooldown per address | 86,400 s | `DEFAULT_ADDRESS_COOLDOWN_SECONDS` |
| Claims per IP per day | 25 | `DEFAULT_IP_CLAIMS_PER_DAY` |
| Claims per subnet per day | 250 | `DEFAULT_SUBNET_CLAIMS_PER_DAY` |
| Lifetime claim ceiling | 10,000 claims | `DEFAULT_MAX_CLAIMS` |
| Live balance | 1,209.880001 `$ULTRA` | `/api/account/fe0b9a92…` |

### 6.3 The arithmetic nobody should have to rediscover

| At | Claims | Days to exhaust 1,209.88 |
|---|---:|---:|
| 10 `$ULTRA`/day cap (full) | 100/day | **121** |
| 1 claim/day | 1/day | 12,098 |
| Lifetime ceiling 10,000 claims | 10,000 total | 1,000 `$ULTRA` — the cap is the binding constraint, not the calendar |

**F-1 (ratify).** The faucet budget is **1,200 `$ULTRA` lifetime** (0.12 % of the allocation), already
almost exactly what is funded. It is spent at ≤10/day, and when it is gone onboarding moves to earned
rewards. **No refill from emission, and no second funding round without a v3 amendment.**

**F-2 (ratify).** The faucet may fund an AppChain payout wallet **once**, as a documented, named claim
(the Grid plan's 1 `$ULTRA` pool). Every such claim is recorded in the faucet runbook with its purpose so
reconciliation shows intent, not theft.

**F-3 (ratify).** The faucet is the largest single leak in the system — 0.1 per claim versus 0.0001 for a
prize, a **1,000×** difference — so any discussion of "reward economics" starts here, not at the prize.

---

## 7. R6 — Long-term sustainability

### 7.1 The three flows, at four scales

| Chains | Anchors/day (4 each) | Fee flow/day | Fee flow/year | vs era-0 emission (157.68/yr) |
|---:|---:|---:|---:|---|
| 10 | 40 | 40,000 u (0.04) | 14.6 | 9 % |
| 100 | 400 | 0.40 | 146 | 93 % |
| 1,000 | 4,000 | 4.00 | 1,460 | 9.3 × |
| 10,000 | 40,000 | 40.00 | 14,600 | 93 × |

Fee revenue equals emission — the point at which the network pays for itself without minting — at
**108 chains anchoring four times a day**. Above that, `$ULTRA` can stop inflating entirely and still pay
validators. This is the single most important threshold in the model.

### 7.2 Should part of the fees be burned?

Emission is the only inflation, and it decays to zero after six halvings (~60 years). A burn makes the
network deflationary earlier. The trade-off is stark:

| Policy | Validators get | Burn break-even (net-zero vs 432,000 u/day emission) |
|---|---|---|
| 0 % burned (today) | All 1,000 per anchor | Never — emission is always pure inflation |
| **50 % burned** | 500 per anchor | **864 anchors/day = 216 chains at 4/day** |
| 100 % burned | 0 (emission only) | 432 anchors/day = 108 chains at 4/day |

**S-1 (proposal, phase 3).** Burn **50 % of AppChain fees** (anchor, registration, handover) and pay the
other 50 % to validators. Rationale: validators keep an incentive to include the transactions, and the
network becomes net-deflationary at a scale it will plausibly reach (216 chains) rather than only in
theory. Plain 1 % transfer fees are **not** burned — they pay for block space, which is a service.

**S-2 (ratify).** Nothing outside the emission schedule may ever mint, including governance. "We will
mint to fund X" is not a v2 option, whatever X is.

### 7.3 What actually gives `$ULTRA` value

Not fees. Even at 1,000 chains, the yearly fee flow is 1,460 `$ULTRA` — **0.15 % of the supply**, far too
small to be the demand story. `$ULTRA` is demanded because:

1. **AppChains must hold it** to register and to keep anchoring (§4).
2. **Prize pools must be denominated in it** (§5) — every chain that pays players needs a funded wallet.
3. **Faucet onboarding is finite** (§6), so participation eventually requires acquiring coins.

That is a *usage* thesis, not a *fee* thesis, and the document should not pretend otherwise: the token's
soundness comes from a hard cap and real demand for settlement, not from burning a rounding error of
fees.

### 7.4 A century in one table

Each era is 31,557,600 blocks, so its total emission is that block count times its reward: era 0 emits
1,577.88, era 1 emits 788.94, and so on down to era 6's zero. The cumulative column is the sum of those,
not of the yearly figures.

| Era | Years | Reward/block | Emitted in era | Emission/yr | Cumulative supply | What must be true |
|---|---|---:|---:|---:|---:|---|
| 0 | 1–10 | 50 | 1,577.88 | 157.68 | 1,001,577.88 | faucet spends its 1,200; 100+ chains anchor |
| 1 | 11–20 | 25 | 788.94 | 78.84 | 1,002,366.82 | fee revenue > emission; burn active |
| 2 | 21–30 | 12 | 378.6912 | 37.84 | 1,002,745.51 | registry self-funding; bonds enforced |
| 3 | 31–40 | 6 | 189.3456 | 18.92 | 1,002,934.86 | fees dominate; emission negligible |
| 4 | 41–50 | 3 | 94.6728 | 9.46 | 1,003,029.53 | supply effectively fixed |
| 5 | 51–60 | 1 | 31.5576 | 3.15 | 1,003,061.09 | the last coin is minted here |
| 6+ | 61+ | 0 | 0 | 0 | 1,003,061.09 forever | validators live entirely on fees |

---

## 8. Invariants — never change without a new architecture version

1. Emission is the only mint. Cap 1,003,061.0872 `$ULTRA`.
2. An AppChain redistributes; it never creates.
3. The L1 does not verify AppChain state, and therefore never prices or taxes it by usage.
4. Every distribution has a named source and a cap (faucet lifetime, per-address, per-chain, global).
5. A fee is always charged to whoever causes the state change, on top of the amount.
6. Any parameter change ships behind a published activation height, like `ULTRANET_APPCHAIN_ACTIVATION_HEIGHT`.
7. No private key, faucet key or prize key ever sits in the node's environment.

---

## 9. Ratification sheet

Sign each line, or change it here first. This is the only approval mechanism.

| ID | Parameter | Proposed value | Ratify |
|---|---|---|---|
| R1 | Maximum supply, forever | 1,003,061.0872 `$ULTRA` | ☐ |
| R1b | Emission schedule | 50/block, halve every 31,557,600 blocks, reduce-only multiplier | ☐ |
| V-1 | Fee split | by weight, remainder to producer | ☐ |
| V-2 | Placeholder validators | cannot earn fees or rewards | ☐ |
| V-3 | Validator bond (phase 4) | 1,000 `$ULTRA`, forfeited on third slash | ☐ |
| C-1 | AppChain prices | 0.01 register, 0.001 anchor, 0.001 handover | ☐ |
| C-2 | "Live" listing rule | ≥ 1 anchor/day, enforced by the app | ☐ |
| C-3 | Volume-tiered anchor fee (phase 3) | halve per 10× chains above 10,000 | ☐ |
| R4 | Standard reward | 100 base units, 3/day/address, 2,000/chain/day, 20,000/day global | ☐ |
| R4b | Grid exception | 10 base units until its 1 `$ULTRA` pool ends | ☐ |
| F-1 | Faucet budget | 1,200 `$ULTRA` lifetime, no refill from emission | ☐ |
| F-2 | Faucet → AppChain pool | one documented claim, ever, per chain | ☐ |
| S-1 | Burn share (phase 3) | 50 % of AppChain fees burned, 50 % to validators | ☐ |
| S-2 | No minting by governance | absolute | ☐ |

---

## 10. Phases

| Phase | Work | Code touched | Gate |
|---|---|---|---|
| **P0** | Ratify this document; declare the cap constant + pinning test; amend the two plan docs | a constant and a test | this sheet signed |
| **P1** | V-1 fee split by weight with remainder to producer; V-2 placeholders excluded | consensus accounting | state-root review + full suite |
| **P2** | One reward policy in the app and services; Grid marked as a test | app copy + prize service | first paid prize |
| **P3** | S-1 burn 50 % of AppChain fees; C-3 tiering if needed | consensus + accounting | activation height, all validators |
| **P4** | V-3 validator bond | consensus + migration | roster cleaned, bond escrow designed |

Nothing in P1–P4 happens before P0 is signed, and no phase may ship without the superseded rows in §9
updated in the same commit.

---

## 11. What this document does not decide

- **AppChain utility fees** (what a chain charges its own users) — deliberately each owner's business.
- **Prize legality** — entry-fee tournaments need legal review before they exist.
- **Third-party chains** — v2 prices them exactly like ours, which is the point of a registry.
