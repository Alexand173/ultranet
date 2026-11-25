# Universal AppChain + Games — architecture decisions (proposal, 2026-11-25)

Status: **proposal for the owner. Nothing here is ratified, and no economics are set.** It answers the twelve
questions the release rule requires, separates what is technical from what only the owner can decide, and
fixes the build order. "Verified" means read from source or run; "proposed" means a recommendation.

## 1. What exists today (verified)

| Piece | State |
|---|---|
| AppChain as an L1 object | Register / anchor / transfer-owner transactions, unique L1 `chain_id`, ownership = the L1 owner address, owner-signed locally (Android) or by CLI. Names are labels. |
| Games Reward Service (`ultranet-games`) | **Two hard-coded games** (`Policy { grid, chess }`; `anchor_games` accepts only `grid`/`chess`). Verified Grid replay; unverified chess (record-only). Real payout worker: signs once, stores the bytes, resends identical bytes, confirms only on the node's word. |
| Records Service (`ultranet-records`) | Generic registry: any developer registers a game id, gets an API key, submits results. **Results are recorded, not validated. Nothing here pays.** All games share one daily root anchored to **one** chain (the Registry). |
| Player identity | An address in the request body. **No proof of wallet ownership.** |
| Real-node payout evidence | `tests/games_real_node_payout_test.rs`: real node code on a local port, real payout worker, a lost submit reply, identical-bytes resend, mined, confirmed, balances exact. Grid only. |

So "any user creates a chain and a game, others play, winners are paid" is **not** a gap of one feature. Three
things are missing and they are independent: *who may register a game on a chain*, *who proves a result is
true*, and *whose money is paid*.

## 2. The twelve questions

Owner decisions are marked **[OWNER]**. Everything else is a recommendation that follows from the fixed rule
that **the platform holds no key for any chain it does not own**.

1. **Who may create a game?** The L1 owner of the AppChain it is attached to. Registration carries a
   Dilithium proof signed by the owner address over (action, game id, chain id, time); the service checks the
   ledger that this address is that chain's owner *at that moment*. (The proof scheme exists in
   `games::ownership`; it does not yet include a chain id.)
2. **Game ownership vs chain ownership.** The game's controller is read from the ledger each time it matters
   (policy change, key rotation), not copied once. A chain handover therefore moves control of its games.
   **[OWNER]** confirm "control follows the chain" rather than "control is fixed at registration".
3. **Several games per chain?** Yes. The game id is the identity; the chain id is an attribute. Anchoring is
   per (game, day) and goes to that game's chain.
4. **Game ids.** A platform-unique lowercase slug chosen at registration, bound permanently to
   (`chain_id`, owner at registration). Never the display name; reserved names refused (already the rule).
5. **Validator registration and execution.** There are three honest tiers (section 3). Arbitrary third-party
   validation code **cannot** be run safely inside a payout service; see 3 for the options. **[OWNER]**
6. **Independent verification.** Tier 1 replays a deterministic game from public inputs (Grid does). Tier 2 is
   a referee the platform itself runs. Tier 3 is owner-reported and is never independently verified.
7. **Player authentication.** Challenge and response: the service issues a nonce bound to (game, address,
   expiry), the wallet signs it locally with its Dilithium key, the service checks that the public key hashes
   to the address. An address typed into a request proves nothing. This needs a signing screen in the app.
8. **Who approves a reward policy?** **[OWNER]** The rewards are money, so the approval path is an economic
   decision. Recommended mechanics, whatever the choice: policies are rows with a published change log, take
   effect after a delay, and can never exceed platform ceilings (the existing `MAX_*` limits).
9. **Who funds rewards?** **[OWNER]** Options in section 4. This is the central decision.
10. **Reconciliation after uncertain outcomes.** Solved and unchanged: stored signed bytes, identical resend,
    confirmation only from the node, operator resolution for the rest. It is game-agnostic already.
11. **A malicious game owner awarding arbitrary ULTRA.** Impossible *by construction* only under option A or B
    of section 4: a game can spend only its own pool, capped per address, per day and in total. It is *not*
    prevented if rewards come from a shared platform pool and a Tier 3 owner reports the wins.
12. **Adding a game without changing the core.** Move per-game reward policy from the compiled `Policy`
    struct into database rows keyed by game id; the payout engine already keys wins by game text. Tier 3 needs
    no code at all. Tier 1 needs a validator (3).

## 3. Result validation: the real constraint

| Tier | Who proves the result | May the central engine pay? | Exists |
|---|---|---|---|
| 1. Deterministic replay | The platform recomputes it from public inputs | Yes, within caps | Grid only |
| 2. Platform referee | A server-side opponent or referee the platform runs | Yes, within caps | No (chess has none; its results are unverified) |
| 3. Owner-reported | The game owner's server, trusted by its API key | **No**, unless funded by that owner (4) | Records Service records them |

A fourth tier would let owners upload their own validators. The two safe ways to do that are a **sandboxed,
deterministic, fuel-metered runtime** (e.g. WASM) or **on-chain verification**; neither exists, and calling an
owner-run HTTP callback a "validator" is Tier 3 under another name. **[OWNER]** Decide whether Tier 1 is
extended by a compiled adapter list (a platform release per new Tier 1 game) or by building a sandboxed
runtime. The first is small and cannot satisfy "no core change"; the second can and is a project of its own.

Rule to keep either way: **a game that cannot validate its results securely must not pay rewards.** Chess stays
record-only until it has a referee.

## 4. Funding (the decision that sets the security model)

| Option | Money | Custody | Result |
|---|---|---|---|
| A. Per-game pool inside the platform payout wallet | Game owner deposits | Platform | A game can spend only its balance. Needs a per-game ledger and a deposit rule; the platform holds funds. |
| B. Owner runs their own payout service | Game owner | None | Non-custodial, nothing for the platform to pay; owner operates a signer and a database. |
| C. Shared platform pool for every game | Platform (faucet-style) | Platform | A Tier 3 owner could drain it. Acceptable only for Tier 1 and 2 games the platform verifies itself. |

**[OWNER]** Recommended: **C for Tier 1 and 2 (Grid, a future refereed chess), A or B for Tier 3.** No fee,
token or deposit rule is introduced here; the deferred rows R4, R4b, F-1 and F-2 of the economic sheet are
untouched.

## 5. Build order

Each stage is separately reviewable and none needs production.

| # | Stage | Depends on | State |
|---|---|---|---|
| 0 | Real-node payout rehearsal (Grid) | none | **done**, `tests/games_real_node_payout_test.rs` |
| 1 | Owner-inspectable closed day (`GET /api/games/days/{game}/{date}`) | none | **done**, uncommitted |
| 2 | Game registration bound to a chain: `chain_id` on the registry record, ownership proof includes it and the ledger is checked | decision 2 | not started |
| 3 | Player wallet authentication (challenge and response) and an Android signing step | none | not started |
| 4 | Per-game reward policy as database rows, read by the payout engine | decisions 8, 9 | not started |
| 5 | A second Tier 1 example game through the same path, and the same rehearsal for it | decision on the validator route | not started |
| 6 | Android: create a chain, register a game, discover and join, see reward and payout state | 2, 3, 4 | partly (chain creation exists) |

## 6. What a rehearsal proves, and what it does not

The Grid rehearsal proves the payout path against real node code. It does not prove: a second game, a game on
a chain the player does not own, a third-party owner's registration, wallet-authenticated play, or any funding
rule. Those rows stay NOT IMPLEMENTED until stages 2 to 5 exist.

## 7. Adapter modules: validation, versioning, upgrade (Stage 3, runtime built, not connected)

`src/games/adapter.rs` runs a game adapter as a deterministic WebAssembly module. It is a runtime and a
contract, nothing more: it is not connected to the Games service, pays nobody and touches no ledger or database.

- **Validation (done, tested).** A module is loaded only if it is at most 256 KiB, validates under the integer
  core (no floating point, SIMD, threads, bulk memory, reference types, tail calls, multiple or 64-bit
  memory), imports nothing, exports the six functions of ABI 1 with the exact types, and declares a memory
  maximum no larger than the host's 16 pages. Each run is a fresh instance with a fuel budget, a bounded call
  depth and bounded input and output.
- **Determinism (done, tested).** No imports means no clock, network, file or randomness. The only result the
  host will stand behind comes from two fresh runs that agree on every output byte and on the fuel used.
- **A module cannot declare itself verified.** The output has no such field; "verified" is the host's decision.
- **Identity and versioning (rule, enforced by the hash).** A module is the SHA3-256 of its exact bytes; ABI 1
  is `adapter_abi_version() == 1`. A new ABI is a new number and a new host path, never a reinterpretation.
- **Authorisation (proposed, not built).** A module runs for a game only if its hash is on a platform-approved
  list; the catalog's `adapter_ref` and `adapter_version` would name it. Registering a hash is a platform
  decision, not the game owner's, because an approved adapter is what lets a game's results be paid.
- **Upgrade (proposed, not built).** An upgrade is a new hash. A finished match keeps the hash it was judged
  by, so changing an adapter never re-judges history. A game moves to a new hash only by an owner-signed
  catalog update (the stage 2 route) naming an approved hash, and each move is in the audit history.
- **Runtime choices that matter.** `wasmi` with `portable-dispatch`: the default tail-call dispatch recursed
  the host stack on a long-running guest loop, which is unacceptable for untrusted modules.

## 8. Reward pools and policies (Stage 4, built at service level, not connected to payouts)

`src/games/pools.rs` implements the funding decision: every game owner funds their own game, per-game accounting
pools, no shared platform pool, no automatic funding from UltraNet. Deposits are bound to a game by an owner-signed
claim over a confirmed L1 transfer (one transaction, one game, once). Rewards need a verified-adapter result, an
approved policy and unreserved funds. Policies are owner-proposed and platform-approved, immutable and hashed.

What it is not: custody is still the platform's single payout key, with isolation in the ledger; the payout worker,
HTTP routes and an operator CLI are not built; and the existing Grid and Chess payouts are untouched.

## 9. Pool payouts (Stage 5, built; proven against a real node, not connected to any route or game)

`src/games/pool_payouts.rs`: the payout worker for catalog games, separate from the Grid and Chess path. Credit and
queued payout are one transaction with the network fee held from the same pool; the exact signed bytes are stored
before sending and are the only thing ever resent; settlement waits for the ledger; pool problems freeze the pool
path alone; withdrawals go to the chain's owner at signing time. `tests/games_pool_payout_real_node_test.rs` runs it
against the repository's real node code: two real AppChains and games, real deposit transfers, a lost reply, a
restart, a real chain handover and a real withdrawal. Still open: the HTTP routes, the operator CLI, a Grid and
Chess migration, an operator path for a payout in doubt, and who pays the network fee as a policy (here: the game).
