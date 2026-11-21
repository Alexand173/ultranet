# UltraNet — finishing plan (2026-11-21)

Status: audit snapshot of `main` (`aa96201`). Read-only audit; nothing here has been deployed or ratified.
Items marked **OWNER** need the owner's key, money or decision and cannot be done from the repository.

## 1. Where things stand

| Area | State |
|---|---|
| Node / L1 | Rust node, 3.0.0. Lib suite green: 497 passed, 0 failed, 1 ignored (§6). One real validator. |
| Live chain | 13 blocks. Unattended block production is configured: `ULTRANET_AUTO_MINE_INTERVAL_SECONDS=10`. The chain is currently idle because the mempool is empty, so the head is old; that is not a miner failure (§2 Phase A item 1). |
| Website | Lint, vitest (141/141), `tsc` and build all green. No developer, games or records pages. |
| Android (UltraVerse) | 26/26 instrumented classes passed on 2026-11-18. 0.1.0 / versionCode 1. Release is signed with the debug key; no store pipeline. Many services inactive. |
| Faucet | Live, 0.1 `$ULTRA` per claim, funded and paying since 2026-10-31. |
| Records + Games services | Code, tests, runbooks and a real-node rehearsal done. **Not deployed** (live endpoints return 404). |
| Move VM | Admission patch **done** in local commit `a7deb34` (not pushed): unknown `MoveCall` pairs are refused at admission. The VM's unknown-function arm itself is unchanged. Other VM defects remain (§2 Phase A item 2). |
| Economics | R1–V-2 ratified. R4, R4b, F-1, F-2 and others still open. |

## 2. Finishing order

### Phase A — chain safety (blocks everything else)
1. **Idle chain and block-production alerting.** Unattended block production is already configured; an old chain head means an idle chain, not a stalled miner. Verified on 2026-11-21: the live env carries `ULTRANET_AUTO_MINE_INTERVAL_SECONDS=10` and `ULTRANET_AUTO_MINE_MAX_BLOCKS_PER_MINUTE=6`, and `auto_mine_tick` returns `Ok(None)` before touching the chain when the mempool is empty (`src/auto_miner.rs:131-134`, "the common, quiet case"). `/api/stats` agrees: `mempool_size 0`, `auto_mined_blocks 1`, `auto_mine_last_at` about 51 minutes after genesis.
   What is actually missing is **detection**, and the naive form of it would be wrong: an alert on head age alone fires permanently on an idle chain and gets muted within a week. The rule is `mempool_size > 0 AND head_age > threshold` — a queued transaction that cannot confirm is the failure worth waking someone for. This step only records the rule; the alerting itself is not implemented or changed here. `/api/stats` already exposes `auto_mine_last_at` and `auto_mined_blocks` for it.
   Separately, a product decision: if the chain must show progress for an explorer's benefit, periodic empty blocks are a consensus policy change with their own consequences, not a configuration fix. (OWNER decision, not a config change.)
2. **Move VM unknown-call admission — done (local commit `a7deb34`, not pushed).** `validate_transaction` now refuses a `MoveCall` whose `module.function` the VM does not handle (`unknown Move function <module>.<function>`). Design and history check: `docs/MOVE_VM_ADMISSION_PROPOSAL_2026-11-21.md`.
   - **Scope:** admission only. Block validation, replay, VM dispatch, gas and fees are unchanged, so blocks that already contain such calls keep replaying. A pending unknown call already in a mempool is no longer mined (`mine_block` re-runs admission); the live chain had none (13 blocks, no `MoveCall`, empty mempool).
   - **Tests:** the 6 new admission tests passed (6/6).
   - **Not re-run:** the complete Rust suite was **not** re-run after the commit. It was run once on the patch before the commit (503 passed, 0 failed, 1 ignored); the commit itself was checked only with the 6 new tests.
   - **Still open, separate defects** (proposal §7): silent `Ok` on too few arguments in `persistent_mint`/`persistent_transfer`; a panic on `args[0][..8]` when an argument is shorter than 8 bytes, reachable through peer gossip; `MoveDeploy` bytecode is never executed; `FheCoin.mint` has no restriction like `UltraCoin.mint`.
3. **Ratify the open economic parameters** (R4, R4b, F-1, F-2, rest of the §9 sheet). (OWNER decision.) Nothing downstream should hard-code them before this.

### Phase B — Records and Games services go-live
Follow `docs/runbooks/games-go-live-checklist.md` in order. All of it is OWNER-run:
1. Register the registry AppChain; hand chain #1 to the owner key.
2. Fund the payout and owner wallets.
3. Install `ultranet-records` and `ultranet-games` units, the env files, and the proxy (`/internal` never routed).
4. Cloudflare rate limits; run `records-rehearsal.sh` once more against the installed binaries.
5. Complete the restore procedure drill (checklist §J) before the first real anchor.

### Phase C — Android release hardening
1. Real release signing key (OWNER: generate and keep offline), versioning policy, crash reporting.
2. Store pipeline and staged rollout.
3. Add the Records/Games compatibility checklist item noted in the last review.
4. Inactive services activated in waves per `android-app/docs/inactive-services-activation-plan.md`, only after Gate 0.

### Phase D — Website
1. Games, records and developer pages (docs, API reference, proof verification).
2. Deferred items stay deferred: web console and Kotlin SDK (see memory note).

### Phase E — Network and observability
1. More than one validator (the largest decentralisation gap).
2. Explorer and metrics consistency check against the live node.
3. Faucet staging sign-off: the runbook is historical-only (PASSED 2026-10-10, live state not independently verified); schedule a fresh independent check.

## 3. Known limits to carry forward (Records safety work)
- Whole-state-directory restore is invisible to the fence; the restore procedure is mandatory.
- Owner-history check has a time horizon; attribution is by time, not by root.
- A pending claim needs an operator to clear it if the node drops the transaction.
- The rehearsal is single-node and cannot age transactions.

## 4. Standing rules
No push, merge or deploy, no chain registration or anchoring, no production key generation, payouts or balance changes, and no consensus/protocol/hash-layout/domain-tag changes without explicit approval. Stop for review after each phase.

## 5. Verification status, checked 2026-11-21 14:31Z

All three items that were open at the time of writing are now checked:

| Item | Result |
|---|---|
| Windows CI | **Green.** Every commit that carries content has a Windows verdict — 518 passed, 0 failed, 1 ignored on both `62afe40` and `b38f6af`. See the gate history below the table |
| Android build on `main` today | **Verified.** `:app:assembleDebug` succeeded at 11:13Z with `JAVA_HOME`/`ANDROID_HOME` from `android-app/.toolchains`; `app-debug.apk` rebuilt to 36,784,284 B. The earlier build failed only because `ANDROID_HOME` was unset, not because of the code |
| Live explorer/dashboard data | **Verified.** `chain_length` 13 and `/api/block/12` returns the head block, so indices agree; `dashboard.ultranetwork.cc` answers 200 with the expected title. The master branch API and the dashboard read the same node |

### Windows gate history

| Commit | Run | Result |
|---|---|---|
| `aa96201` | `44895977034` | **cancelled** — superseded by a later push (`cancel-in-progress`), not a failed test |
| `62afe40` | `44898222530` | **success** — 518 passed, 0 failed, 1 ignored |
| `b38f6af` | `44906758391` | **success** — 518 passed, 0 failed, 1 ignored (job 5323 s, suite step 4910 s, 27 binaries) |

`Verify release version consistency` on `b38f6af`: **success**, run `44906758360`.

`a71e2c2`, `21921ce`, `4a074ab` and `e4acdd0` also show `cancelled`: each was replaced by a newer push
to `main` before its Windows job finished. A cancellation is not evidence that a test failed, and none
of those four carried source changes.

Two observations from the live check, neither of which blocks anything:

- Block 12 carries `"aggregated_signature": null`. With one real validator there is no aggregate to
  produce, but this is the same field that surfaced as a P0 finding earlier in the project, so it is
  worth confirming the intended semantics rather than assuming it resolves itself when a second
  validator joins.
- `release` builds still use the debug signing config (`android-app/app/build.gradle.kts:39`,
  `signingConfigs.getByName("debug")`), which confirms the Phase C item: there is no release identity
  to publish with.

## 6. Rust suite on `main`
`cargo test --lib -- --test-threads=1` on `main`: 497 passed, 0 failed, 1 ignored (562 s), exit 0.
