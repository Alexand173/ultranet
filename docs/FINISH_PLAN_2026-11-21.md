# UltraNet — finishing plan (2026-11-21)

Status: audit snapshot of `main` (`aa96201`). Read-only audit; nothing here has been deployed or ratified.
Items marked **OWNER** need the owner's key, money or decision and cannot be done from the repository.

## 1. Where things stand

| Area | State |
|---|---|
| Node / L1 | Rust node, 3.0.0. Lib suite green: 497 passed, 0 failed, 1 ignored (§6). One real validator. |
| Live chain | 13 blocks. Blocks are sealed only by `POST /api/mine` unless `ULTRANET_AUTO_MINE_INTERVAL_SECONDS` is set. Last auto-mined block was ~138 h ago; mempool empty. |
| Website | Lint, vitest (141/141), `tsc` and build all green. No developer, games or records pages. |
| Android (UltraVerse) | 26/26 instrumented classes passed on 2026-11-18. v0.1.0 / versionCode 1. Release is signed with the debug key; no store pipeline. Many services inactive. |
| Faucet | Live, 0.1 `$ULTRA` per claim, funded and paying since 2026-10-31. |
| Records + Games services | Code, tests, runbooks and a real-node rehearsal done. **Not deployed** (live endpoints return 404). |
| Move VM | Unknown-function arm still reports success (Gate 0 item, not fixed). |
| Economics | R1–V-2 ratified. R4, R4b, F-1, F-2 and others still open. |

## 2. Finishing order

### Phase A — chain safety (blocks everything else)
1. **Unattended block production.** Set `ULTRANET_AUTO_MINE_INTERVAL_SECONDS` on the live node, or run a scheduled sealer; add an alert when the head is older than N minutes. (OWNER: node config change.)
2. **Move VM unknown-function fix.** Make an unknown function fail instead of reporting success. Small change, but it touches execution semantics, so it needs explicit approval and a test per rule.
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

## 5. Not yet verified
Windows CI status, Android build on `main` today, live explorer/dashboard data.

## 6. Rust suite on `main`
`cargo test --lib -- --test-threads=1` on `main`: 497 passed, 0 failed, 1 ignored (562 s), exit 0.
