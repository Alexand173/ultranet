# UltraVerse Android — plan for activating the inactive services

**Date:** 2026-11-14
**Goal stated by the owner:** activate the 11 inactive service tiles, grow the user base to large scale, and create real demand for `$ULTRA`.
**Status of this document:** a plan. It changes no code.

## 1. What the app has today

`ServiceCatalog.kt` lists 16 services. Five are live and native: Wallet, UltraNet Pay, AppChains, Validator applications, Validate transactions (operator only).

These 11 are inactive (`concept(...)`, red ×):

| Group | Inactive service |
| --- | --- |
| Payments & addresses | Name Service |
| Move & acquire assets | DEX, Bridge, Fiat on-ramp, Exchange / listing access, BTC / other-crypto purchases |
| Build & participate | Staking, Holder-led DAO, AppChain ownership trading |
| Applications | Games, NFT marketplace |
| Bottom navigation | Swap, Profile |

The two bottom-bar tabs fit into the waves below; they don't need separate backends:

- **Profile:** activate it with Wave 1 as a screen covering your `.ultra` name (5.1), wallet address and receive QR, contacts, appearance, language, lock and backup reminders. Everything stays local except the name lookup. It needs no account, which matches the app's no-account rule.
- **Swap:** keep it inactive until the DEX (Wave 3) has a real second asset and liquidity. Then make it a shortcut into the DEX swap screen. Until then, a swap tab with nothing to swap against would mislead users.

## 2. Backend facts that decide what can be built

What I checked in the node (`src/`):

1. **The node accepts only native transaction payloads:** `StandardTransfer`, `MoveCall`, `MoveDeploy`, `ValidatorJoinProposal`, `ValidatorApproval`, `SovereignSupplyCorrection` (`src/lib.rs:893`). None of them covers names, stakes, votes, NFTs, orders or ownership transfers.
2. **The Move VM does not execute Move.** `MoveVM::execute_function` (`src/move_vm.rs:255`) recognizes only `UltraCoin.mint/transfer` and `FheCoin.mint/transfer`. Every other call logs "simulated in RAM" and returns success (`Ok(vec![1])`) without changing state. A DEX, NFT or DAO written in Move would therefore report success and do nothing. `UltraNFT.move`, `BridgePortal.move` and `AppChainFactory.move` are source files, not running contracts.
3. **Move transactions also require a ZK-STARK/SNARK proof** (`src/lib.rs:2342`). The phone has no way to produce one today.
4. **There is one asset.** `$ULTRA` is the only token, and there is no token standard. A DEX needs at least two assets.
5. **The network is still bootstrapping** (`docs/ULTRANET_STATUS_AND_ROADMAP_2026-09-24.md`). At the last snapshot there were 2 blocks, 1 user transaction and one real validator. Blocks carry no aggregate signature. The 21M supply cap that the website claims is not implemented. Supply has not been audited, and there is no market price or listing.

**Consequence:** each new service needs (a) a new native payload with validation and state in the node, (b) an API route, and (c) a local signer, screen and tests in the app. This is the same pattern the Validator application already follows (`ValidatorProposalSigner.kt` + `ValidatorProposalDigestTest`). Wiring tiles to Move would produce screens that look like they work and don't.

## 3. Sequencing principle

"Lots of users" and "token value" depend on the chain being able to carry them. Sending large traffic to a network with one validator, unaudited supply and a stubbed contract VM risks a public failure, and a public failure costs more users than it brings in. The order is:

1. **Gate 0:** fix the network enough to carry users.
2. **Wave 1:** services that create real reasons to hold and spend `$ULTRA` and that need only modest node work.
3. **Wave 2:** services that need decentralization or a token standard first.
4. **Wave 3:** services that depend on outside parties, legal review or audited bridges.

## 4. Gate 0 — must be done before any user-acquisition push

These come from the repo's own roadmap (P0/P1). They are listed here because every wave depends on them.

- [ ] One economic spec: decide on the supply cap, emission in blocks, fees and the treasury, then enforce it in code and fix the website claims.
- [ ] At least 3–5 independent validator operators on a public testnet, with an upgrade drill.
- [ ] Either implement block-level aggregate signatures or stop claiming them.
- [ ] Make the Move path refuse unknown functions instead of reporting success: change the `_ => Ok(vec![1])` arm in `src/move_vm.rs` to an error. This is a one-line safety fix and can ship now.
- [ ] An explorer and public metrics that come from measured data rather than hard-coded values.
- [ ] A Play Store release pipeline: release signing key (not the debug key), crash reporting without PII, staged rollout.
- [ ] Bring the faucet's staging gate to PASSED. The faucet is the cheapest way to get a new user to a first transaction.

## 5. Wave 1 — highest impact, smallest backend

### 5.1 Name Service (`alice.ultra` → address)  ★ top priority

> **Superseded (2026-11-14):** the owner decided that Name Service means **local contacts only**: a phone address book so users don't type hex. It needs no node work and no fees. See [name-service-coding-plan.md](name-service-coding-plan.md). The on-chain design below is kept for reference only.

- **Why first:** it makes Pay usable by ordinary people, gives the token a sink through registration and renewal fees, and is easy to share ("send to alice.ultra").
- **Node:** payloads `NameRegister { name, years }`, `NameRenew`, `NameTransfer`, `NameSetTarget`. Validation covers the character set and length rules, uniqueness and fee ≥ price(length). Burn the fee or send it to the treasury, per the Gate 0 spec. Routes: `GET /api/name/{name}`, `GET /api/name/reverse/{address}`.
- **App:** a new `ui/names/` screen covering search, availability, price, register and my names. Pay's recipient field resolves `*.ultra` and **always shows the resolved address before signing**. Portfolio shows names in place of hex addresses.
- **Tests:** digest vector shared between Rust and Kotlin, a resolution test, and a Pay test proving a name resolves to the exact address that gets signed.
- **Monetization:** short names cost more (3 characters > 5 characters > 8+ characters), plus yearly renewal. This is real, repeated demand for `$ULTRA`.

### 5.2 Games → "Apps on UltraNet" directory, built on AppChains

- AppChains already exist (registry, treasury, anchoring). Activate the Games tile as a curated list of AppChains tagged `game`. Each entry shows its name, treasury, last anchor and an Open action. Opening goes to an in-app web view of the AppChain's own front end, with explicit consent.
- **Node:** an optional `category` and `metadata_uri` field on AppChain create.
- **Monetization:** AppChain creation and anchoring fees in `$ULTRA`.
- **Caution:** games that pay token prizes for chance-based outcomes are gambling in most jurisdictions. Allow only skill-based or non-cash-out designs until legal review.

### 5.3 Pay growth features (no new tile, highest user impact)

- Payment request QR codes (`ultranet:<addr>?amount=…&memo=…`) and Android share links.
- Incoming-payment notifications: a background poll via WorkManager with no server-side identity.
- Contacts list (local only) and recent recipients.
- Localization, starting with Serbian and English.
- A merchant mode that shows a fixed amount QR and confirms when the payment arrives.

## 6. Wave 2 — after Gate 0 (multiple validators, token spec)

### 6.1 Staking

- **Prerequisite:** independent validators to stake on. Staking to a single validator is centralization with a yield label.
- **Node:** `Stake { validator, amount }`, `Unstake` with an unbonding period, and rewards paid from fees and emission per the economic spec. Slashing rules must be written before launch.
- **App:** a validator list (uptime, commission, stake share) and stake/unstake/claim actions. Show a projected rate only if it is computed from on-chain data, never a promised APY.
- **Legal:** advertised staking yield is a regulated activity in several jurisdictions (US/EU MiCA). Get advice before marketing it.

### 6.2 Holder-led DAO

- Extends the existing `governance/` proposals and approval intents.
- **Node:** `DaoPropose`, `DaoVote { proposal, choice }` weighted by a balance snapshot at the proposal height, plus quorum and timelock. Use it first for treasury grants, then for the documented transition away from 2-of-3 sovereign approval. That transition is also a roadmap goal.
- **App:** proposal list, detail view, vote, and results.

### 6.3 Token standard → NFT marketplace

- **Node:** a native `AssetCreate / AssetMint / AssetTransfer` payload, i.e. a minimal fungible and non-fungible standard. Don't wait for a real Move VM.
- **NFT marketplace:** `ListForSale { asset, price }`, `Buy` (atomic asset ⇄ `$ULTRA` within the same transaction), `CancelListing`. A protocol fee of X% goes to the treasury.
- **App:** gallery, mint, list, buy. Media is pinned off-chain (IPFS/HTTPS) and stored by content hash.

### 6.4 AppChain ownership trading

- **Node:** `AppChainTransferOwnership`, reusing the listing and buy primitive from 6.3.
- **App:** a "For sale" filter in the AppChains registry.

## 7. Wave 3 — external parties, legal work or audited code

| Service | What it really takes | Recommendation |
| --- | --- | --- |
| **DEX** | A second asset (6.3) and a native AMM pool payload (`PoolCreate/AddLiquidity/Swap`) with invariant tests and an external audit. | Build after the token standard has issued assets. Never seed fake liquidity or wash-trade. |
| **Bridge** | `BridgePortal.move` is a stub. Bridges are the most-exploited component in crypto. | Only with an audited design, a multi-party committee, rate limits and caps. No earlier than 6 months after Wave 2. |
| **Fiat on-ramp** | Licensed providers (Transak, MoonPay, Ramp…) list only chains with audited supply, an explorer and liquidity. | Business development, not code. In-app it is a provider widget once someone accepts the integration. Never custody fiat yourselves. |
| **Exchange / listing access** | Needs a listing, which needs audited supply, legal opinion, liquidity and market makers. | The tile becomes a "Where `$ULTRA` trades" list showing venue, pair and asset ID, only after real listings exist. |
| **BTC / other-crypto purchases** | Needs a bridge, atomic swaps (HTLC with BTC) or a licensed provider. | HTLC atomic swaps are the trust-minimized option, built after the DEX. Until then, keep the tile inactive. |

## 8. Making `$ULTRA` valuable

Token value is demand divided by supply. Demand that lasts comes from people **needing** the token to do something:

| Source of demand | Service |
| --- | --- |
| Transfer fees | Pay, Wallet (live) |
| Name registration and renewal | Name Service |
| AppChain creation, anchoring, ownership trades | AppChains, ownership trading |
| Mint and marketplace fees | Token standard, NFT marketplace |
| Locking for security and governance | Staking, DAO |
| Swap fees | DEX |

Pair this with the Gate 0 economic spec: a fixed or explicit supply policy and fee burn or treasury rules. Avoid anything that inflates numbers without users, such as airdrop farming, self-funded liquidity, wash volume or promised returns. These are what get tokens delisted and projects sued. Do not publish a price target. The roadmap's public-claims policy already forbids it.

## 9. User-growth metrics to track from day one

Track these privately and never collect seeds or keys: wallets created → first funded → first send → active after 7 and 30 days; names registered; Pay payments per week; distinct counterparties; failed transactions by reason. If the 7-day number is poor, a growth campaign only increases churn.

## 10. Engineering template for every new service

1. Node: payload variant, canonical digest, validation, state, API route, and Rust unit tests including a fixed digest vector.
2. App `wallet/`: a `XxxSigner.kt` that reuses the Dilithium JNI path, plus an `androidTest` that pins the same digest vector.
3. App `data/`: API client calls on `Dispatchers.IO`.
4. App `ui/xxx/`: the screen, read from `UltraVerseColors`, with the balance chip from `LocalWalletBalance`.
5. `ServiceCatalog`: switch the entry from `concept(...)` to `Availability.Supported`, add the route in `UltraVerseApp`, and update `GroupedHomeScreenTest` counts.
6. README "Current scope" entry, written in the same evidence-first style.

## 11. Suggested timeline

| Weeks | Work |
| --- | --- |
| 1 | Move "simulated success" fix; economic spec draft; Play release signing |
| 1–4 | Name Service, node and app; Pay request QR, notifications, Serbian localization |
| 3–6 | Games/Apps directory on AppChains; faucet gate passed; validator recruitment on public testnet |
| 6–12 | Independent validators live → Staking, DAO |
| 8–14 | Token standard → NFT marketplace, AppChain ownership trading |
| 14+ | DEX (audited); on-ramp and listing business development once supply is audited; Bridge and BTC swaps last |
