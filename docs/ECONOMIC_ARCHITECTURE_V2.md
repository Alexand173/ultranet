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

> **R1 is ratified (see §2.2–2.3).** The code now pays 0.5 `$ULTRA` per block and the cap is
> 32,557,379.0968. §2.1 still derives the *previous* 3,061 figure on purpose: that derivation is the
> arithmetic proof that settled the review, and deleting it would invite the same challenge again.

`genesis.json` allocates **1,000,000 `$ULTRA`** (`1,000,000,000,000` base units) to the sovereign
address, and the only mechanism that creates more is the emission schedule. The cap used to be
undefined; as of 2026-11-15 it is declared in code as `MAX_SUPPLY_BASE_UNITS` and pinned by a test, so
it can no longer drift silently.

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

### 2.1 How the *previous* cap was derived, and why it could not be larger

*(Historical: this derives the 3,061 figure of the schedule that was in the code before the 2026-11-15
ratification. The method is what matters — the ratified numbers are in §2.2.)*

This was challenged in review with the figure `6,074.3449125`. The challenge counts the halving series
twice, and this paragraph is here so the question is settled with arithmetic rather than opinion.

The er0→6 per-block rewards are the **integer** sequence `50 >> era`:

```
era:            0     1     2     3    4    5    6+
reward/block:  50    25    12     6    3    1     0
```

Two independent checks, both of which must agree:

| Derivation | Result |
|---|---|
| Sum the actual integer sequence: `31,557,600 × (50+25+12+6+3+1)` = `31,557,600 × 97` | **3,061,087,200 base units = 3,061.0872 `$ULTRA`** |
| Upper bound from the exact geometric series: `50 × 2 × 31,557,600` | 3,155,760,000 base units = 3,155.76 `$ULTRA` |

Truncation can only ever *reduce* emission below the geometric bound, so any total above
**3,155.76** is impossible by construction. The review's 6,074.34 is 1.93× that ceiling: it takes **97**
— which is already the *sum* of the era rewards — and treats it as the era-0 reward, then applies
`1 + ½ + … + 1/64` on top. That is the double count. Era 0 pays 50 per block, not 97.

**On fractional base units:** there are none, and none are possible. `50 >> era` is integer division, so
12 (not 12.5) and 6 (not 6.25) are exactly what the protocol pays; 3,061.0872 is the total *after* that
truncation. No rounding rule is needed beyond the shift itself — which is why the constant can be pinned
by a test instead of described in prose.

The `× 0.8` density multiplier (`ai_governor.rs:111`) only reduces further, so 1,003,061.0872 is an
**upper bound on supply**, reached only if the multiplier never triggers.

### 2.2 The 100-year figure — for the schedule now in the code

**Ratified schedule (2026-11-15): 0.5 `$ULTRA` per block, halved every 31,557,600 blocks.** One
halving interval is exactly 10.0000 years at the 10 s target, so a century is ten intervals and the
integer sequence (`500,000 >> era`) needs nineteen of them to reach zero.

| Era | Years | Reward/block | Per day | Era emission | Cumulative |
|---|---|---:|---:|---:|---:|
| 0 | 0–10 | 500,000 | 4,320.00 | 15,778,800.00 | 15,778,800.00 |
| 1 | 10–20 | 250,000 | 2,160.00 | 7,889,400.00 | 23,668,200.00 |
| 2 | 20–30 | 125,000 | 1,080.00 | 3,944,700.00 | 27,612,900.00 |
| 3 | 30–40 | 62,500 | 540.00 | 1,972,350.00 | 29,585,250.00 |
| 4 | 40–50 | 31,250 | 270.00 | 986,175.00 | 30,571,425.00 |
| 5 | 50–60 | 15,625 | 135.00 | 493,087.50 | 31,064,512.50 |
| 6 | 60–70 | 7,812 | 67.50 | 246,527.97 | 31,311,040.47 |
| 7 | 70–80 | 3,906 | 33.75 | 123,263.99 | 31,434,304.46 |
| 8 | 80–90 | 1,953 | 16.87 | 61,631.99 | 31,495,936.45 |
| 9 | 90–100 | 976 | 8.43 | 30,800.22 | **31,526,736.67** |
| 10–18 | 100–190 | 488 → 1 | — | 30,642.43 | **31,557,379.10** |

| Quantity | Value |
|---|---:|
| Emission, first 100 years | **31,526,736.6672** |
| Emission, all time (eras 0–18) | **31,557,379.0968** |
| **Maximum supply** | **32,557,379.0968** |
| Emission ÷ genesis allocation | **3,155.8 %** (31.6×) |
| Sovereign share of supply | **3.07 %** |
| Pinned in code | `MAX_SUPPLY_BASE_UNITS = 32_557_379_096_800` |

**Block-time invariance, unchanged from before:** emission is fixed in *blocks*, so block time changes
*when* coins arrive, never how many. At any block time of **16.67 s or less**, the whole schedule fits
inside 100 years. At 20 s a century captures five eras (30,571,425.00); at 30 s, three plus part of a
fourth (28,269,900.00) — the same coins, later.

**The units rule that this cost us once:** with 6 decimals, 0.5 `$ULTRA` is **500,000 base units**. A
suggestion of "5,000,000 base units (0.5 `$ULTRA`)" was 5 `$ULTRA`/block, a **315,557,379** emission —
ten times the intent. `the_declared_cap_matches_the_schedule` now fails on any such drift.

### 2.3 R1 reopened, then ratified — the coded schedule did not meet the objective

**Outcome (2026-11-15):** schedule **A** was chosen and is implemented — `GENESIS_REWARD = 500_000`,
halving every `HALVING_INTERVAL_BLOCKS`, deployed by **reset at block 0** (§2.4). The four candidates
and the evidence that produced the decision are kept below, because the reasoning is worth more than
the conclusion.

The owner's intent is a Bitcoin-shaped security budget: the network is paid for by emission for a
century, and the genesis allocation is a *small* share of supply. The coded schedule does the opposite,
and every earlier number in this document describes the coded schedule honestly rather than the intended
one. Stated plainly:

| | Coded (`GENESIS_REWARD = 50`) | Intended |
|---|---:|---:|
| Emission, all time | **3,061.09** | ≥ 30,000,000 |
| Emission ÷ genesis allocation | **0.31 %** | ~30× |
| Supply | 1,003,061 | ~31,000,000+ |
| Sovereign share of supply | **99.70 %** | ~3 % |
| Validator revenue, 6 validators, year 1 | 26.28/yr each | ~250,000/yr each |

Bitcoin's shape is the inverse: 20.99M of a 21M cap is emission, and the genesis is negligible. A
network whose emission is 0.3 % of its allocation has no security budget — it has a pre-mine with a
rounding error attached.

Four candidate schedules, all computed with the integer rules the code actually uses (`R >> era` for
halving, or `× 0.8` per decade for the smooth variant):

| | **A · Bitcoin-shaped** | **B · exact 30M** | **C · 20-yr epochs** | **D · smooth −20 %/decade** |
|---|---:|---:|---:|---:|
| Reward/block | **0.500000** | 0.476000 | 0.238000 | 0.190000 |
| Halving cadence | 10 yr | 10 yr | 20 yr | −20 % per decade |
| Emission, first 100 yr | **31,526,736.67** | 30,013,423.52 | 29,103,996.60 | 26,760,592.34 |
| Emission, all time | ~31.5M | ~30.0M | ~30.0M | ~30.0M |
| **Supply cap** | **~32,530,000** | ~31,010,000 | ~30,100,000 | ~30,000,000 |
| Era-0 emission | 4,320.00/day | 4,112.64/day | 2,056.32/day | 1,641.60/day |
| Validator/year (6 validators, year 1) | **262,800** | 250,186 | 125,093 | 99,864 |
| Subsidy still paid in year 90–100 | 3,078/yr total | 2,932/yr | 46,911/yr | 80,475/yr |
| Sovereign share at year 10 | 6.0 % | 6.2 % | 11.8 % | 14.3 % |
| Sovereign share at year 100 | **3.07 %** | 3.22 % | 3.32 % | 3.60 % |

**Recommendation: A.** Round, auditable numbers (0.5 `$ULTRA` per block, halve every ten years), the
era-0 share is exactly half the schedule the way Bitcoin's is, and the sovereign allocation lands at
3.07 % of supply — a pre-mine ratio that needs no defence. C and D are the answer only if the priority
is *reducing early dilution* rather than paying for security immediately: they roughly halve the decade-1
subsidy and keep a larger tail in year 100, at the cost of the sovereign holding 4× more of supply in
year 10.

### 2.4 How such a change may be deployed (this is the part that can break the chain)

`calculate_block_reward()` derives the reward from `total_blocks`, so **changing the base reward or the
interval changes the reward of every block at every height**, including blocks already mined. A node
running the new formula would reject a replay of the existing chain.

Two legal routes, and no third:

| Route | What it costs | When it is the right answer |
|---|---|---|
| **Height gate**: `if block_index < H { old reward } else { new reward }` | One condition in the reward function, an agreed `H` on every validator, plus a pinned test at the boundary | The chain has real history worth preserving |
| **Reset**: start a new genesis at block 0 with the new schedule | Discards every existing account, balance and nonce — today that is 1,000,000.000650 `$ULTRA` at block 13, so almost nothing | The chain is 13 blocks old and nothing depends on it yet |

The live network is at **block 13** with 12 transactions. That is the cheapest this decision will ever
be, and doing it by height gate instead keeps a 13-block history nobody needs while permanently
complicating the reward function. My recommendation is the **reset** — provided the owner confirms
nothing external depends on the current 13 blocks.

**Ratify R1:** ~~keep the schedule as written, and declare the cap 1,003,061.0872~~ → **superseded**.
R1 is reopened; the schedule and cap are chosen from §2.3, and whichever is chosen must state the
deployment route from §2.4. Emission remains the *only* mint in the system, forever, and the `× 0.8`
density multiplier may only ever reduce it.

Why the cap matters more than the inflation rate: 0.0158 %/year is economically invisible, but an
undefined cap is a governance hole. A number that cannot be changed without a document change is worth
more than three hundredths of a percent.

---

## 3. R2 — Validator revenue

### 3.1 The rule

A validator earns from exactly two sources, and both must be stated as fractions of a defined pool:

1. **Seigniorage** — the block reward, distributed **by weight** among reward-eligible validators by
   `payout_credits`, called by `update_state`, `reexecute_block_for_validation` and the restart replay.
   `distribute_rewards` still runs, but only on the mining node and only on the `rewards` field: it is
   bookkeeping, not payment, and it disagrees with what is paid (§3.3.2, finding G).
2. **Transaction fees** — 100 % of every fee in the block, split on the same by-weight basis through
   the same helper.

### 3.2 What was true before 2026-11-15 (kept as the record)

> **Resolved.** Every row below was fixed on 2026-11-15 (§3.3). They are kept because a defect that
> cannot be read about again will be reintroduced.

| Fact | Consequence |
|---|---|
| `total_fees / validator_set.len()`, remainder dropped | An anchor's 1,000 across 6 validators pays 166 × 6 = 996; **4 base units are destroyed**. A 1-unit transfer fee pays **nothing at all** |
| Split is per set entry, not per weight | A validator with weight 1 and one with weight 8 receive the same fee share |
| `/api/stats` reports `validator_count: 6` | With five placeholder records, the one real validator collects **one sixth** of all fees and two thirds of a reward distribution it did not earn |
| `stake` is `weight × 1000`, set at registration | It is bookkeeping. **No `$ULTRA` is ever bonded**, so nobody loses money for misbehaving; `slash_validator` removes a key after three strikes and slashes nothing |
| Emission dominates fee revenue | Era-0 emission is 0.432 `$ULTRA`/day; fees only match it at ~108 chains anchoring 4×/day (§7) |

### 3.3 Ratified and implemented (2026-11-15)

**V-0 (implemented).** A validator is paid at `hex(SHA3-256(public_key))` — the same rule wallets use.
The old code credited `hex::encode(public_key)`, the raw key, thousands of characters long: **every
block reward and every fee went to an account no key could spend from.** `validator_payout_address`
is now the single definition, and `a_validator_is_paid_at_the_hash_of_its_key` pins it.

> **Correction, same day.** The first attempt changed two of the places that credit a block. The third,
> the restart replay, kept the old rule: it credited `hex(public_key)` with the whole reward on every
> startup, dropped the fees, and filled the trie with a phantom account. It also paid the genesis block,
> which no live path ever did. The fix was not to teach replay the payout rules but to stop replay from
> paying at all — see V-4. `a_restart_restores_payouts` and `restart_restores_every_balance` fail if
> either the phantom account or the fictional balances return.

**V-1 (implemented).** Block reward *and* fees are distributed **by weight** through two shared
helpers, `payout_shares_for` and `payout_credits`, used by the live path (`update_state`), the
re-execution path (`reexecute_block_for_validation`) and the restart replay, so all three reproduce the
same credits. The integer
remainder goes to the largest weight (ties → first in order), so **no unit is destroyed** —
previously an anchor's 1,000 became 996 across six validators and a 1-unit fee paid nobody. The block
reward is no longer paid entirely to `validator_set.first()`.

**V-2 (implemented).** `ValidatorInfo` gains `reward_eligible` (default **false**). The genesis
bootstrap sets it false, the registration path sets it true, and the payout shares include only active,
weight-bearing, eligible validators.

> **Caveat, and it is not academic.** The default is a `serde` attribute, and the validator tree is
> stored with **bincode**, which is positional: a record written before this field existed does not
> decode with `false`, it decodes with the old `is_active` byte in that slot, which usually fails the
> record and aborts startup, and occasionally succeeds with every following field shifted. The field
> protects new writes, not old ones. This is why the reset runbook forbids restoring an old database
> and why a migrated roster needs a versioned record rather than a default (§3.3.2, finding F).

**V-4 (implemented 2026-11-15).** A restart no longer re-derives balances from the blocks. Every
transition is written to the account records by `apply_account_balance` — the same write that
feeds the state trie — so those records are the authoritative post-chain state and the replay copies
them; it still derives nonces, the epoch, the difficulty and the state-root history from the blocks. The
old replay seeded only the sovereign and invented a 1,000-unit opening balance for every other account it
met, so a restart replaced real balances with a fiction. The trade-off is stated rather than hidden: a
crash between applying a block and saving it now leaves the records one block ahead of the chain, and the
restart will not repair that. The alternative — rebuilding every balance from the genesis roots — needs
the trie to be enumerable by account key, which it is not.

**V-3 (phase 4, open).** A **bond** of 1,000 `$ULTRA` is required to validate, held from the validator's
own balance and forfeited to the fee pool on the third slash. This is what turns `stake` from a label
into money at risk. Deliberately later: it needs its own consensus change and a migration.

### 3.3.1 The consequence an operator must know

If **no** reward-eligible validator is in the set, the block's reward and fees are **not created** —
nothing is credited to a fallback address, and a warning is logged. Emission is a ceiling, not a
promise. On a freshly reset chain whose only validators are the genesis placeholders, **nothing is
minted until a real validator is registered**: the reset runbook must register the operator's validator
key as part of the reset, not afterwards.

### 3.3.2 Open defects found while certifying the reset (2026-11-15)

The payout rewrite was reviewed adversarially before the reset was allowed to run. These were found;
none is fixed by §3.3, and each is a consensus change with its own blast radius.

| # | Severity | Defect | Effect |
|---|---|---|---|
| ~~**J**~~ | ~~Blocker~~ **fixed 2026-11-15** | Restart replay seeded only `SOVEREIGN_ADDR` and started every other account at `or_insert(1000)` | A restart replaced real balances with 1,000 minus fees. Replay now copies the account shards, which `update_state` writes on every transition; `restart_restores_every_balance` guards it and the reset runbook's P1 gate is certified |
| B | Critical | `block_reward` is absent from `calculate_block_hash` and unvalidated in `validate_block` | A producer may claim any reward; 0.5 per block is not consensus |
| C | Critical | `total_fees` sums every fee, but only `StandardTransfer` and AppChain payloads debit the sender | A fee on `MoveCall`/`MoveDeploy` is minted to validators from air |
| D | Critical | `validator_set` is absent from the block hash and unvalidated | A producer may list itself alone or one key five times |
| E | Major | Activation happens at mempool admission, not in block application | A verifier that never saw the approval credits a different address and rejects an honest block |
| F | Major | `reward_eligible` relies on a `serde` default on a bincode-stored struct | Old records fail to decode (startup abort) or decode shifted, paying placeholders |
| G | Major | `distribute_rewards` ignores `reward_eligible` and runs only on the mining node | The reported `rewards` field contradicts what is paid and diverges across nodes |
| H | Minor | `mine_block` applies state and clears the mempool before a fallible `replace_validators` | A persistence error leaves state applied to a block that is not in the chain |

### 3.4 The original proposal (historical)

**V-1 (ratify).** Fees are split **by weight**, and the integer remainder goes to the **block producer**.
No unit is destroyed. A 1-unit fee pays that unit to someone.

**V-2 (ratify).** Only validators that are **active and weight > 0** participate. Placeholder and demo
records are `is_active = false` and cannot receive fees or rewards. This is a one-line economic fix with
a large effect: the real validator's fee income multiplies by six the day it lands.

**V-3 (ratify, later).** A **bond** of 1,000 `$ULTRA` (100,000,000 base units) is required to validate,
held from the validator's own balance, and forfeited to the fee pool on the third slash. This is what
turns `stake` from a label into money at risk. It is deliberately **phase 4**, because it needs its own
consensus change and a migration for the current roster.

### 3.5 The honest number

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

**C-1 (ratified 2026-11-21).** Fees stay flat: 10,000 to register, 1,000 per anchor, 1,000 per handover.

**C-2 (ratify).** A chain that wants to be **listed as live** in the app must anchor at least **once per
day**. This is a product rule enforced by the app, not a consensus rule — the L1 never judges it. It
exists so that "anchored today" means something.

**C-3 (proposal, phase 3).** If the registry grows past 10,000 chains, the anchor fee halves per order of
magnitude of chain count (1,000 → 500 → 250), so the registry stays affordable at a million chains while
L1 revenue still grows with the number of chains. Deliberately not now: a fee schedule that changes
underneath owners needs its own activation-height process.

### 4.3 Per-AppChain anchor pricing — what is real, and what it would cost to build

**The proposal:** instead of one anchor price, each AppChain carries its own, so a symbolic puzzle pays
less than an exchange. It is a reasonable instinct and it is *not* what the code does. Both halves of
that need stating precisely, because a review claimed the mechanism already exists and is therefore free
to switch on:

| Path | Per-chain price? | Evidence |
|---|---|---|
| Legacy prototype registry (`/api/appchain/create`, `/api/appchain/anchor` — both **retired, `410 Gone`**) | A field **exists and is never varied** | `factory.rs:30` `default_anchor_fee()` returns the global constant; `api.rs:1499` writes that constant at creation; no route accepts a price |
| Legacy charging basis | The chain's **derived treasury address** | `factory.rs:152`, `api.rs:1672` read `previous_config.anchor_fee` |
| **Consensus ledger** (`AppChainAnchor`) | **No such field** | `grep -c anchor_fee src/appchain/ledger.rs` → `0`; `required_fee()` returns the global `APPCHAIN_ANCHOR_FEE` |

The legacy field cannot be the foundation. Its fee is debited from a **keyless derived treasury**
(`derive_appchain_treasury_address`), an address nobody can spend from — coins sent there are stranded,
which is precisely why the ledger replaced it with a debit on the owner's own L1 account. It is also
admin-token gated and never enters a block. Per-chain pricing in that path would price something that
cannot pay.

**What it would take in the ledger** (a real consensus change, comparable in size to the ownership
transfer already shipped):

1. A `minimum_anchor_fee` field on `LedgerChain`, set from the registration payload.
2. An envelope rule: `tx.fee >= max(PROTOCOL_ANCHOR_FLOOR, chain.minimum_anchor_fee)` — a floor the
   protocol owns, or an owner sets their own price to 1 base unit and pays nothing.
3. A way to change it later, which is either a new signed operation or a re-registration, and its own
   replay/idempotency rules.
4. An activation height agreed by every validator, like every other consensus change.

**The economics, stated honestly: flexible pricing does not raise L1 revenue by itself.** The revenue is
`Σ anchors_i × price_i` and every owner chooses the smallest legal price. A chain with 10,000 users has
no reason to declare 0.1 when the floor is 0.0001 — so the expected outcome is a **race to the floor** and
the same revenue as today, with more code:

| Scenario | Anchors/day | Daily L1 revenue |
|---|---:|---:|
| 10 chains at the floor (0.0001) | 40 | 0.004 |
| 10 chains, one voluntarily at 0.01 | 40 | 0.014 (only if it chooses to) |
| 10 chains at today's flat 0.001 | 40 | 0.04 |

Note the third row: **today's flat fee already collects more than a floor-plus-choice model.** The
flexibility argument and the revenue argument pull in opposite directions.

**R3b (recommend: defer).** Price on what the L1 can *verify*, not on what an owner *declares*. The
verifiable quantity is **anchor frequency** — an owner cannot fake anchoring more often, because each
anchor is a paid transaction. A scheduled tier (first 4 anchors/day at 1,000, additional anchors at 2,000)
charges a busy chain more while a small chain pays little, with no self-declared tier and no floor race.
Per-chain pricing stays available as a product feature — a chain may *choose* to prepay for a higher-tier
listing — but it is not an L1 revenue mechanism.

**R3c (ratify).** If per-chain pricing is ever adopted, the protocol floor is
`APPCHAIN_ANCHOR_FEE` (1,000 base units) and an owner may declare only **above** it. The floor is the
spam barrier; it is not negotiable per chain.

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

### 7.4 A century in one table (coded schedule — superseded by §2.3)

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

1. Emission is the only mint. The cap is whichever schedule §2.3 ratifies — candidates land between
   ~30.0M and ~32.5M — and it is fixed forever once chosen.
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
| R1 | Maximum supply, forever | **RATIFIED 2026-11-15:** 32,557,379.0968 (`MAX_SUPPLY_BASE_UNITS = 32_557_379_096_800`) | ☑ |
| R1b | Emission schedule | **RATIFIED 2026-11-15:** 0.5 `$ULTRA`/block, halve every 31,557,600 blocks (`GENESIS_REWARD = 500_000`) | ☑ |
| R1c | Deployment route for R1 (§2.4) | **RATIFIED 2026-11-15:** reset at block 0 | ☑ |
| V-0 | Validator payout address | **RATIFIED + IMPLEMENTED 2026-11-15:** `hex(SHA3-256(public_key))` | ☑ |
| V-1 | Fee split | **RATIFIED + IMPLEMENTED 2026-11-15:** by weight, remainder to the largest weight, no unit destroyed | ☑ |
| V-2 | Placeholder validators | **RATIFIED + IMPLEMENTED 2026-11-15:** `reward_eligible = false` for genesis entries | ☑ |
| V-3 | Validator bond (phase 4) | 1,000 `$ULTRA`, forfeited on third slash | ☐ |
| C-1 | AppChain prices | **RATIFIED 2026-11-21 (owner):** 0.01 register (10,000 base units), 0.001 anchor (1,000), 0.001 handover (1,000); already the coded `APPCHAIN_REGISTER_FEE`, `APPCHAIN_ANCHOR_FEE`, `APPCHAIN_TRANSFER_FEE` | ☑ |
| C-2 | "Live" listing rule | ≥ 1 anchor/day, enforced by the app | ☐ |
| C-3 | Volume-tiered anchor fee (phase 3) | halve per 10× chains above 10,000 | ☐ |
| R3b | Per-AppChain anchor price | **rejected as an L1 revenue lever**; price by verifiable anchor frequency instead | ☐ |
| R3c | Protocol anchor floor | 1,000 base units, owners may declare only above it | ☐ |
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

---

## 12. Revision log

| Rev | Date | Change | Why |
|---|---|---|---|
| v2.0 | 2026-11-15 | First issue: R1–R6, 14 ratification rows | Establish one economic rule before code |
| v2.3 | 2026-11-15 | P1 implemented: V-0 payout address, V-1 weighted split without loss, V-2 placeholder exclusion. §3.3 rewritten as ratified | Fees were credited to `hex(public_key)` — an unspendable phantom account — so validators had never actually been paid. Also fixes the remainder destroyed on every block |
| v2.2 | 2026-11-15 | R1 ratified: 0.5 `$ULTRA`/block, cap 32,557,379.0968, deployed by reset. §2.2 rewritten for the chosen schedule | The coded 3,061 emission was 0.3% of the allocation — the inverse of the intended Bitcoin-shaped distribution. Implemented in `GENESIS_REWARD`, `MAX_SUPPLY_BASE_UNITS` and `genesis.json` |
| v2.1 | 2026-11-15 | Added §2.1 (cap derivation and its upper bound) and §4.3 (per-AppChain anchor pricing), rows R3b and R3c | Review questioned the emission total and claimed per-chain anchor pricing already worked. §2.1 shows 97 is the *sum* of era rewards, not era 0's reward, so 3,061.0872 stands and 6,074.34 exceeds the geometric ceiling. §4.3 shows the per-chain field lives only in the retired prototype and that floor-plus-choice pricing collects *less* than the flat fee. |
| v2.4 | 2026-11-21 | C-1 ratified by the owner with the existing fees: 10,000 / 1,000 / 1,000 base units (register / anchor / handover). §9 row ticked, §4.2 label updated | The same values were already coded and already charged on the live chain (blocks 11 and 12), so the sheet recorded no decision for them. No code or parameter changes |

Both challenges are recorded rather than quietly absorbed: a document that cannot show why it survived a
review will be challenged again by the next reader.
