# UltraNet project status and next-step roadmap

**As of:** 2026-09-24
**Purpose:** evidence-based review of the development recorded in this repository, the currently observable network, tokenization and `$ULTRA` valuation readiness, and the work required to pursue a secure, useful, widely adopted blockchain.
**Code changes:** none. This is a report, not a protocol specification, audit, launch approval, investment recommendation, or promise of token value.

## Executive assessment

UltraNet has progressed from an initial Rust node repository to a substantial, multi-component project: it has a published v7.1.7 multi-platform node release, a Next.js website and wallet flows, validator proposal/approval tooling, a persistent node/API, a faucet implementation and a carefully isolated staging process. The repository also contains integration/unit tests, deployment templates, and detailed operator documentation. The latest CI changes are committed at `9381e1a`; the Windows test and release-version checks shown in the owner-provided GitHub Actions screenshot are green.

That is meaningful engineering progress, but **it is not yet sufficient evidence for describing UltraNet as a mature, decentralized, economically validated L1**. The public API currently reports a very small observed chain (2 blocks, 1 transaction), zero ZK proofs/verifications and no AppChain activity. It reports six validator records, but the owner clarifies that there is only one real regular validator; the other configured validator entries are demos, not real people. Per-validator identity, stake, and ping details are not exposed on the public page, and the approval service could not confirm. This roster clarification is operator-provided rather than independently verified. These observations do not prove independent validator operation or robust consensus.

The staging faucet's last checked-in readiness state is **PREPARED — NOT PASSED**: encrypted backup/restore passed, but credential disposition and replay evidence were blocked, fresh no-beacon evidence was still required, and the selected September 20 window was unsigned/inactive. That window has now passed. Its present-day runtime status has not been refreshed in this report; therefore, do not infer that a new test is authorized or that the service state is currently verified.

There is **no defensible current price or market capitalization for this specific `$ULTRA`** in the repository or the public market evidence reviewed for this report. There is a configured denomination and genesis allocation, but no verified price feed/listing, circulating-supply audit, liquidity, or reliable demand history. A ticker is not a market price. The best next step is not to announce a target valuation; it is to establish protocol correctness, independently verify supply and chain activity, resolve conflicting economic claims, and create real utility and transparent adoption metrics.

**Overall stage:** release-capable software and bootstrap network, with important correctness, security, decentralization, and token-economics evidence still to establish. A “best in the world” outcome cannot be promised; it can be approached by setting measurable standards and passing independent review.

## Scope and confidence

The Git history visible in this checkout contains 100 commits, beginning with `c88abea` (“Initial commit: UltraNet v7.1 Sovereign Mainnet”) on 2026-08-10 and ending at `9381e1a` on 2026-09-20. This describes the history preserved in this repository; it does **not** reconstruct design or development that occurred before the initial commit.

Repository claims below are checked against code and documentation where possible. Public endpoint values are snapshots fetched on 2026-09-24 and may change. They are not an independent consensus, ledger, security, legal, or market audit. “Not verified” means evidence was not found during this review, not proof that something cannot exist.

## Development history visible in this repository

| Period | Recorded milestone | What it establishes—and what it does not |
| --- | --- | --- |
| **Aug 10, 2026** | Initial repository commit describes UltraNet v7.1 “Sovereign Mainnet.” | The starting code/documentation snapshot; the label is not independent evidence of a production-grade or decentralized network. |
| **Aug 13–17** | Peer connection tracking and persistent validator dials/identity; public validator onboarding and node downloads; platform release packaging and early v7.1.x releases. | Build, deployment, and network plumbing matured. It does not establish a multi-operator live network or verified performance claims. |
| **Aug 24–26** | Phase-0 transfers, validator key export/onboarding, wallet-aware proposals, wallet recovery, and offline approval tooling. | A user and validator path is taking shape. Adoption, recovery success rates, and independent governance operation remain unmeasured. |
| **Aug 30–Sep 1** | Isolated faucet service and staging harness; v7.1.7 preparation; Windows tests and approval-flow coverage. | The repository has a real faucet implementation and CI gates; live public faucet readiness is a separate operational gate. |
| **Sep 3–8** | Staging node isolation, preflight, Caddy hardening, egress/expiry controls, origin-lockdown work, and Turnstile troubleshooting. | Security boundaries and operations were actively hardened; the canary still required fresh approvals/evidence. |
| **Sep 13–20** | Staging health/credential records, one-claim helper hardening, encrypted SQLite backup/restore, fresh-window and private-signoff drafts. | Gate 2 backup/recovery is recorded as passed. The overall packet remains not passed; do not convert a backup pass into claim authorization. |
| **Sep 20** | CI commit `9381e1a` increased Windows test timeout to 180 minutes and upgraded `actions/cache` to v5. | The provided Actions screenshot shows the Windows test workflow and release-version consistency check succeeded for this commit. The Windows test took about 1h 3m; this is CI evidence, not a full protocol security audit. |

### Current product components

- **Node and APIs:** Rust/Actix service with persistence, wallet-signed transaction admission, validator proposal/approval paths, account and chain queries, and operations routes. See [Cargo.toml](../Cargo.toml), [src/lib.rs](../src/lib.rs), and [src/api.rs](../src/api.rs).
- **Network and execution:** P2P/DAG-related modules, sharded state, Block-STM-related components, Move integration, and ZK/FHE modules exist in the repository. Presence of a module or dependency does not by itself prove that every documented property is implemented, secure, enabled in the deployed build, or performant in production.
- **Wallet and website:** the website contains wallet, send, download, validator onboarding, and learning flows. See [ULTRAWALLET_INTEGRATION.md](../ULTRAWALLET_INTEGRATION.md) and [website](../website).
- **Validator operations:** Dilithium identity export and a 2-of-3 sovereign proposal approval process are documented in [VALIDATOR_GUIDE.md](../VALIDATOR_GUIDE.md) and [OFFLINE_APPROVAL_SIGNING.md](../OFFLINE_APPROVAL_SIGNING.md).
- **Faucet:** an isolated, budgeted service exists, but the staging canary remains a separately gated operation. See [FAUCET_TECHNICAL_SPEC.md](FAUCET_TECHNICAL_SPEC.md) and the [staging readiness packet](runbooks/faucet-staging-gated-readiness.md).

## Current state: repository and public observations

### Repository facts

- The node package advertises v7.1.x releases, while [`Cargo.toml`](../Cargo.toml) declares package version `3.0.0`, and the public API manifest returns `7.1 Sovereign`. These may represent different version namespaces, but the mapping is not made explicit. Publish one clear distinction between crate/build version, protocol version, and network version.
- [`genesis.json`](../genesis.json) declares a named mainnet chain, five initial validators, a 1,000,000 `$ULTRA` sovereign allocation, six decimals, a reward of 50 base units, and a halving interval of 31,557,600 blocks. Runtime transaction code uses numeric `L1_CHAIN_ID = 0`; document and test how this relates to the named chain ID in the genesis file.
- The runtime sets `GENESIS_REWARD = 50` **base units**, or `0.000050 $ULTRA` at six decimals. The reward function applies integer right shifts by era (`50 >> era`), so the reward reaches zero at era 6 due to integer rounding, despite the guard at era 64. Confirm that this behavior is intended and publish a precise, tested emission schedule.
- The website’s legislation table claims a 21,000,000 `$ULTRA` supply cap and describes the halving interval in seconds. No 21-million cap was found in the node’s `src` implementation. The code, genesis configuration, and marketing text are not presently a single consistent economic specification. The source interval is a number of **blocks**, not seconds; at the configured 10-second target, 31,557,600 blocks corresponds to roughly ten years, not 31,557,600 seconds.
- The one-time sovereign supply-correction path is implemented as a special signed transaction. This does not establish a global maximum supply, an audited circulating supply, or that the correction has been executed on the public network.
- The Move source includes basic `UltraCoin` and `UltraNFT` modules. The native-coin module has a treasury/mint concept and the NFT module stores basic name/URI items; they do not by themselves provide a complete tokenization standard, asset provenance, regulated issuance, redemption, or market infrastructure. The node’s Move adapter also contains explicit persistence handlers and a simulated fallback for unhandled functions; verify actual execution semantics before promising general Move compatibility.
- The technical guide explicitly says the Groth16 setup is deterministic for development and recommends a public multi-party ceremony before broad mainnet trust. It also describes a single-operator bootstrap risk. These are stated caveats and should be treated as release gates, not footnotes.

### Public endpoint snapshot (2026-09-24 UTC)

| Observation | Snapshot | Proper interpretation |
| --- | --- | --- |
| [`/api/stats`](https://api.ultranetwork.cc/api/stats) | `chain_length=2`, `total_blocks=2`, `total_transactions=1`, `validator_count=6`, `total_weight=501`, `mempool_size=2`, `zk_proofs=0`, `zk_verifications=0`, `stm_total_executions=0`. | `validator_count=6` is not six real operators. The owner clarifies that the five validator entries in the genesis configuration are demos, not real people; the network has one regular validator whom the owner, acting as genesis sovereign, approved. The owner role is not counted here as a second regular validator. This is operator-provided context, not independently verified by the endpoint. The other metrics still indicate very low observed activity and do not prove transaction diversity or cryptographic correctness. |
| [`/dashboard`](https://api.ultranetwork.cc/dashboard) | Height 2, TPS 0.00, zero active AppChains, and no recent AppChain finalizations; per-validator identity/stake/ping is not exposed, and approval-service confirmation failed on the fetched page. | Improve data freshness, availability, and transparency before using this as adoption or decentralization evidence. |
| [`/api/validate`](https://api.ultranetwork.cc/api/validate) | Returns `valid: true`. | A positive application response is useful but is not an external audit or a proof of distributed finality. |
| [`/api/manifest`](https://api.ultranetwork.cc/api/manifest) | Returns static protocol fields including six decimals, genesis allocation, reward, and consensus label. | A manifest is not itself a live supply, price, performance, or validator attestation. The latency field is hard-coded in the API handler and needs benchmark provenance. |
| [`/api/account/<sovereign address>`](https://api.ultranetwork.cc/api/account/3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c) | Returned `998,990.000000 $ULTRA`, nonce `2`, and `updated_at=2026-08-30T16:48:43Z` (from the endpoint timestamp). | The account record is stale relative to this report date and covers only one balance, not total or circulating supply. Reconcile it against fresh state, complete balances, issuance, burn/lock rules, and public block/transaction evidence. |
| [`/api/ai/history`](https://api.ultranetwork.cc/api/ai/history) vs dashboard | API returned an empty history with sustainability score 100; dashboard showed score 0. | Dashboard/API metric disagreement should be reconciled and explicitly labelled before investor, validator, or user reliance. |

**Network status conclusion:** public pages and genesis files label the network “mainnet,” and public endpoints respond. The fetched data does not demonstrate sustained transaction activity, independent validator participation, robust decentralized consensus, or a mature application ecosystem. Describe the current state accurately as a **live, low-activity bootstrap network pending independent verification**, unless fresh evidence establishes more.

### Staging faucet gate

The last repository readiness decision, dated Sep 20, records:

- **Credential-store:** blocked pending a current owner-signed bounded-risk disposition or improved protection.
- **Encrypted backup/recovery:** passed in a separate maintenance session; restore was disposable and live state was recorded unchanged.
- **Replay/idempotency:** blocked/out of scope pending prior-approved evidence or a separately authorized test.
- **Web Analytics:** exact-host disposition recorded as not configured/closed, but fresh independent no-beacon browser evidence remains required.
- **Selected test window:** Sep 20, 2026, `11:00Z–13:00Z`, cutoff `12:55Z`; it was explicitly unsigned, inactive, and readiness unconfirmed. It is expired as of this report.
- **Intake:** the packet says closed and requires `FAUCET_ENABLED=false`; this was last documented on Sep 20 and was not reverified on Sep 24.

See the detailed [gated readiness packet](runbooks/faucet-staging-gated-readiness.md), [private sign-off checklist](runbooks/faucet-staging-private-signoff-checklist.md), [credential disposition draft](runbooks/faucet-staging-credential-store-disposition-draft.md), and [fresh-window draft](runbooks/faucet-staging-fresh-window-record-draft.md). Do not reuse the expired window or infer readiness from these drafts.

## Tokenization and `$ULTRA` valuation

### What can be stated today

- `$ULTRA` is specified as the native token with six decimal places: 1 `$ULTRA` = 1,000,000 `microULTRA`.
- The repository specifies a 1,000,000 `$ULTRA` genesis allocation to the sovereign address. The current account endpoint is not a supply audit.
- The base block reward is 50 base units (`0.000050 $ULTRA`) before the era reduction described above.
- The project has early coin/NFT code and a wallet transaction flow, but not a demonstrated end-to-end tokenized-asset marketplace or a complete real-world-asset issuance/redeem framework.

### What cannot be responsibly stated today

No current price, market capitalization, or fully diluted valuation can be calculated from the inspected repository and public endpoints. The reviewed public search did not find a verifiable market listing that clearly matches this particular UltraNet chain/token identity; this is a lack of verified evidence, not proof of absolute non-listing. `$ULTRA` shares a ticker with unrelated assets, so ticker-only listings are not adequate identification.

A defensible market capitalization requires:

```text
market capitalization = independently verified price × audited circulating supply
fully diluted valuation = independently verified price × audited fully diluted supply
```

Neither factor is currently established well enough here. A 1,000,000-token genesis allocation is not the same as circulating supply; a claimed 21,000,000 cap is not valid unless it is specified, implemented, and independently tested. Do not publish a dollar target, investment return, or market-cap claim as if it were a protocol fact.

### Evidence needed before publishing valuation data

1. Freeze and publish the canonical chain ID, genesis hash, native-token identity, precision, and recognized token interfaces.
2. Reconcile every account balance and all issuance, transfers, fees, rewards, burns, locks, treasury holdings, and supply corrections from genesis to a verifiable block height. Publish both a reproducible method and an independent review.
3. Adopt one explicit economic policy: maximum supply (or explicitly uncapped supply), reward schedule, rounding rules, treasury vesting, validator rewards, fees, burns, and governance authority. Enforce it in code and protocol tests; remove or correct contradictory public statements.
4. Publish circulating, locked, treasury, and fully diluted quantities with definitions and update provenance.
5. Only if genuine trading exists, identify the exact asset on each market by chain and asset identifier; disclose venue, pair, timestamp, volume, spread, depth, and liquidity quality. Avoid treating a thin or self-funded pool as a reliable price.
6. Separate factual market data from illustrative valuation scenarios. Any scenario should state assumptions and risks and must not be represented as a promised price.

### Tokenization work still needed

For native tokens and NFTs, define a canonical token standard covering issuer authority, unique identity, metadata schema and persistence, supply invariants, decimals, transfer authorization, upgrade policy, events/indexing, and wallet/explorer support. For real-world assets specifically, decide the legal and technical model for ownership records, custody, transfer restrictions, redemption, corporate actions, off-chain attestations/oracles, and jurisdictional compliance. Do not describe a Move module or an NFT demo as production RWA tokenization.

## Prioritized roadmap

The sequence below is deliberate: make claims true and testable before expanding the audience or attaching valuation narratives.

### P0 — Establish one truthful, auditable protocol baseline

- **Canonical specification:** reconcile crate, release, protocol, and network versions; named genesis chain ID vs numeric transaction chain ID; deterministic genesis hash and validator-set initialization; and exact block-height semantics.
- **Economic invariants:** decide whether a 21-million cap exists, implement or remove the claim, document emissions in base units and blocks, test all reward eras and rounding, reconcile genesis balance with public balances, and define the supply-correction authority and status.
- **Consensus and cryptography review:** commission independent protocol/security review. The current DAG vertex implementation contains no signature field and comments describe adding vertices without waiting for signatures; verify how authenticated voting, quorum, safety, liveness, replays, equivocating validators, partitions, restarts, and upgrades are actually secured end to end. Do not assume the names “Bullshark/Mysticeti” prove conformance to reviewed upstream protocols.
- **Execution/proof review:** test deterministic replay across independent nodes; clarify where Move bytecode is genuinely executed vs handled by native adapters; validate proof verification for each supported path; and complete a public multi-party Groth16 ceremony for production parameters if those circuits remain in scope.
- **Release integrity:** pin and review Git dependencies, add dependency/license and vulnerability tracking, reproducible builds, SBOM/provenance, signed release artifacts, and a supported upgrade/rollback procedure.
- **Operational truth:** replace hard-coded or stale public stats with measured, timestamped values; show endpoint failures as unavailable, not fabricated “healthy” state; publish how each counter is obtained.

**Exit criteria:** independently reviewed specification, reproducible genesis, consensus and state-transition test suite passing under multi-node/adversarial scenarios, verified token-supply invariant, and public status metrics with provenance. No “production-grade,” “quantum-secure,” “100-year,” or TPS/finality performance guarantee without scoped evidence.

### P1 — Prove a multi-operator network and safe public onboarding

- Operate a public testnet with independent validator organizations, documented hardware profiles, test tokens, block explorer, faucet, upgrade drills, and a public incident channel.
- Publish validator set membership, unique operator/entity count, geography/network diversity where safe, voting power distribution, uptime, missed rounds, and externally verifiable signing evidence. Distinguish registered keys from connected, active, independent validators.
- Document onboarding authority, staking/bonding (if any), rewards, penalties, appeals, key rotation, recovery, and exit. The current 2-of-3 sovereign approval is a bootstrap trust model; specify measurable conditions and an auditable transition path away from it.
- Rework onboarding around real external-operator success: signed packages, one-command verification, minimum and recommended hardware, telemetry opt-in, support, and a safe testnet migration guide.
- Test the wallet and recovery experience with real users. Measure wallet creation/recovery completion, first successful test transfer, confirmation latency, failure reasons, and support burden without collecting seed phrases or private keys.

**Exit criteria:** multiple independent operators can join, run, upgrade, recover, and leave without a single founder controlling routine operations; adversarial testnet drills meet published safety and liveness criteria.

### P2 — Finish bounded tokenization and developer utility

- Freeze a minimal, audited native coin/NFT standard before adding more asset kinds.
- Build a full token lifecycle: issue, transfer, query, burn/retire if allowed, freeze/recover only under explicit governance, and index events with deterministic test vectors.
- For RWA offerings, add legal review, issuer due diligence, custody and redemption design, investor eligibility, data/oracle sources, and disclosure before deploying contracts.
- Publish SDKs, API schemas, test vectors, example applications, local test tooling, and a verifiable explorer. Make developer support and backwards compatibility explicit.
- Use grants and partnerships only against measurable, user-serving milestones—not token price, wash activity, or vanity validator counts.

**Exit criteria:** independent developers can build and test a tokenized asset application on public testnet using stable interfaces and verify state from public sources.

### P3 — Controlled production growth and responsible valuation publication

- Keep staging and production separated. Before any public faucet claim, select a new future window and recheck every private gate; the Sep 20 window expired. Keep intake disabled until security disposition, replay evidence, no-beacon check, recovery, egress, route, backup, and stop controls are current.
- Start a capped, disclosed beta only after independent review and operational drills; reconcile every payout and measure whether recipients make a first transfer and return later. Faucet claim totals alone are not adoption.
- Publish monthly metrics: active unique wallets (with a privacy-safe definition), returning wallets, successful/failed transactions, fee revenue, useful dApps, AppChain activity, unique validator operators, voting-power concentration, uptime, and security incidents.
- Publish a market/valuation data policy only after audited supply and genuine, sufficiently transparent liquidity exist. Use clearly attributed third-party market sources and show timestamp, venue, pair, depth, and confidence. Never present a projection as guaranteed.

**Exit criteria:** independently measured sustained usage, resilient operation, transparent governance, audited token accounting, and a market-data methodology that users can reproduce.

## Recommended immediate work order

1. Mark the September 20 faucet window expired and maintain a conservative closed posture until fresh host evidence says otherwise.
2. Reconcile the public chain snapshot against actual block/transaction records, validator identities, sovereign balance, supply-correction records, and a fresh independent node.
3. Publish an errata/canonical specification for versioning, chain identity, total supply policy, decimals, emission/halving units, and genesis/reward semantics.
4. Commission an independent consensus, cryptography, economic-invariant, and deployment review; create a tracked remediation register with severity, evidence, owner, and closure criteria.
5. Launch a public testnet and validator diversity program before claiming broad decentralization or driving mainnet user acquisition.
6. Finish the safe wallet/explorer/developer path and measure genuine retained use.
7. Define tokenization scope and legal controls, then deploy a reviewed standard with testnet examples.
8. Revisit valuation only when both audited circulating supply and credible, verifiable market liquidity exist.

## Public claims policy

Until the relevant exit criteria are met:

- say “the repository implements” or “the public endpoint reported” rather than treating design intent as independent verification;
- label the chain as a low-activity bootstrap network unless fresh independent evidence supports a stronger status;
- do not promise quantum-proof security, sub-millisecond finality, 21-million capped supply, a 100-year lifespan, institutional readiness, or a token price without independent evidence and implemented protocol rules;
- state clearly that validator admission is currently sovereign-approved and that decentralization is a goal, not an established result;
- distinguish native token balances from circulating supply, market cap, and investor ownership rights.

## Addendum — fresh P0 evidence and aggregate-signature trace (2026-09-24 14:19 UTC)

This read-only follow-up supplements the public snapshot above; it does not alter the original report date or imply independent node verification.

### Fresh public observations

- At 14:19 UTC, [`/api/stats`](https://api.ultranetwork.cc/api/stats) still reported `chain_length=2`, `total_blocks=2`, and `total_transactions=1`. Its `genesis_time=1789194163` converts to `2026-09-12T06:22:43Z`.
- [`/api/block/1`](https://api.ultranetwork.cc/api/block/1) returned block index `1` at `2026-08-30T16:48:43Z` with `aggregated_signature: null`. The block contains one `StandardTransfer`: 1,000,000,000 base units (1,000 `$ULTRA` at six decimals), with a 10,000,000-base-unit fee. It is a sovereign-to-faucet funding transfer, not evidence of broader user activity. The faucet account endpoint, fetched at 14:04 UTC, reported 1,000.001000 `$ULTRA`; that balance is 0.001 `$ULTRA` above the block's transfer amount and is not reconciled by the observations reviewed here. Do not describe it as a matching transfer balance or assume a fee credit.
- The block's timestamp is nearly 25 days older than this observation. Compared with the earlier fetched block-0 timestamp (`2026-08-30T14:50:35Z`), the API `genesis_time` is also inconsistent with the block timestamp chronology. Treat these API values as stale/inconsistent telemetry, not a fresh chain-health attestation.

### What `aggregated_signature: null` means in the current implementation

The observed `null` is expected from the current code path, but it is not evidence that aggregate signing succeeded or that a block-level BLS certificate exists:

1. [`UltraBlock`](../src/lib.rs#L1013-L1024) makes the field optional. Genesis blocks and normally produced L1 blocks initialize it to `None` ([genesis](../src/lib.rs#L1704-L1715), [block production](../src/lib.rs#L2852-L2864)); the other in-repository construction sites do likewise.
2. The BLS aggregation function exists ([`aggregate_signatures`](../src/lib.rs#L431-L481)), but there is no call site that assigns its result to a block. The producer calculates the block hash and proceeds into DAG insertion and validation without populating the field.
3. Block validation only invokes `verify_aggregated` when the option is `Some`; when it is `None`, it logs that aggregation is unavailable and skips that check ([validation](../src/lib.rs#L3239-L3249)). Thus, absence does not fail the BLS-validation branch; the remaining validation continues.
4. The API serializes the block directly with `serde_json::to_value` ([`get_block`](../src/api.rs#L973-L982)). Since the optional field is not configured to be omitted when absent, `None` appears as JSON `null`.

**Conclusion:** `null` is the expected serialization of the current unpopulated optional field, not an API transport artifact. The repository does not establish that skipping this BLS check is an intentional consensus-policy decision: the aggregation path is unwired, and absence is accepted by this validation branch. This does not, by itself, prove that no other authentication/finality mechanism exists, but it means this field provides no block-level aggregate-signature evidence for the observed block. Keep consensus/finality claims gated pending an end-to-end review of signer collection, quorum rules, certificate assignment, verification, persistence, and multi-node/adversarial tests.

## Sources reviewed

### Repository

- [README.md](../README.md)
- [genesis.json](../genesis.json)
- [Cargo.toml](../Cargo.toml)
- [src/lib.rs](../src/lib.rs), [src/ai_governor.rs](../src/ai_governor.rs), [src/api.rs](../src/api.rs), [src/dag_mysticeti.rs](../src/dag_mysticeti.rs), [src/dag_bullshark.rs](../src/dag_bullshark.rs), [src/move_vm.rs](../src/move_vm.rs), [src/supply_correction.rs](../src/supply_correction.rs)
- [UltraNet technical guide](../ULTRA_NET_TECHNICAL_GUIDE.md), [technical manifest](../TECHNICAL_MANIFEST.md), [genesis report](../GENESIS_REPORT.md), [validator guide](../VALIDATOR_GUIDE.md)
- [faucet technical specification](FAUCET_TECHNICAL_SPEC.md), [faucet operations](FAUCET_OPERATIONS.md), [staging readiness packet](runbooks/faucet-staging-gated-readiness.md), and [private sign-off checklist](runbooks/faucet-staging-private-signoff-checklist.md)
- Git history from commit `c88abea` on 2026-08-10 through `9381e1a` on 2026-09-20.

### Public, read-only observations captured 2026-09-24

- [UltraNet homepage](https://ultranetwork.cc/)
- [Public dashboard](https://api.ultranetwork.cc/dashboard)
- [Public stats API](https://api.ultranetwork.cc/api/stats)
- [Public block 1 API](https://api.ultranetwork.cc/api/block/1)
- [Public chain validation API](https://api.ultranetwork.cc/api/validate)
- [Public protocol manifest API](https://api.ultranetwork.cc/api/manifest)
- [Public sovereign account API](https://api.ultranetwork.cc/api/account/3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c)
- [Public faucet account API](https://api.ultranetwork.cc/api/account/787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358)
- [Public AI history API](https://api.ultranetwork.cc/api/ai/history)

This report makes no changes to source code, runtime state, token balances, credentials, or network configuration.
