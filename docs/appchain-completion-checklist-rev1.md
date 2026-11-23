# AppChain completion checklist (one list, closed item by item)

**Revision:** 1, 2026-11-22. Replaces the per-document status sections for tracking purposes; the decision
documents remain the record of *why*.
**Code revision:** `0a50928` on top of `c3a31ae`, `f5a532f` and `c05fb94`, all on top of `cda5132`. The
binary built from `cda5132` is what production runs (row 17b). None of it is pushed (row 24).
**Repository note:** this file is mirrored at `docs/appchain-completion-checklist-rev1.md`; the decision
documents it cites (D-1, D-4, D-5A–D) live in the planning workspace, not in the repository.
**How to read:** each row is closed only by its acceptance evidence, not by a document existing.

---

## 1. Closing items

| # | Item | Status | Evidence | Blocker |
|---|---|---|---|---|
| 1 | **Owner-signed authorization is the only model** (D-1 architecture) | **PENDING OWNER** | D-1 §1 decision text, invariants I1–I4; production inventory in D-1 §2 | owner vote |
| 2 | **Legacy node-side anchor path retired in code** | **DONE — committed `c28f174`, deployed** | `src/api.rs`: both routes answer **410** naming the owner-signed replacement; `run_appchain_anchor` (140 lines) **deleted**, `AppChainAnchorResponse` removed, `test_anchoring_enabled` removed, `anchor_availability` no longer reports `production`. Test `api::configuration_tests::retired_appchain_anchor_routes_answer_gone` **PASS** (also asserts 401 still applies first) | — |
| 3 | **Wrong-owner / cross-owner rejection proven** | **DONE** | `tests/appchain_authorization_test.rs::an_identity_that_owns_one_chain_cannot_anchor_another` **PASS** — three chains, two owners: the identity owning #3 anchors #3 and is refused on #1 and #2, with no anchor recorded | — |
| 4 | **Signature binding proven** (modified payload, substituted sender) | **DONE** | `a_payload_modified_after_signing_is_rejected`, `a_substituted_sender_is_rejected` — both **PASS** | — |
| 5 | **Transfer rules proven** | **DONE** | `a_foreign_owner_cannot_transfer_and_an_owner_cannot_transfer_to_themselves` — **PASS** | — |
| 6 | **Replay / nullifier / fee / version / strict parsing proven** | **DONE** | `a_nullifier_cannot_be_reused_but_an_identical_resubmission_is_accepted`, `a_registration_without_balance_is_rejected`, `an_envelope_that_is_not_version_five_is_rejected`, `an_unknown_field_is_rejected` — all **PASS** | — |
| 7 | **Public read surface proven** (unknown chain, sequence paging) | **DONE** | `the_read_surface_answers_unknown_chains_and_pages_by_sequence` — **PASS** | — |
| 8 | **No regression to the existing AppChain lifecycle** | **DONE** | `appchain_api_test` **PASS**; `api::` module 54 tests **PASS** | — |
| 9 | **Peer-anchor / cross-user signing dependency removed** | **BLOCKED — production configuration** | Inert today: no peer credential provisioned, and the signer (`d4fca721…`) is not the owner of the only allow-listed chain (#1, `7d2cd1cc…`). Removing `1` from `RECORDS_PEER_ANCHOR_CHAINS` is a production env change | requires production approval |
| 10 | **D-5B remaining rows** (R1 full error table, R5 activation boundary, R6 mempool drop, A1–A4 Android, E1–E5, mutation runs) | **NOT RUN** | — | Android tooling; D-5D for E4/E5 |
| 11 | **Mobile wallet: external prepare → review → sign → submit** (A-2) | **NOT STARTED** (design only) | A-2 design, unmodified | owner approval of A-2 |
| 12 | **Configurable node endpoint** (D-5D §2) | **SUPERSEDED by row 18** | implemented and committed in `186e213`; row 18 carries the evidence | — |
| 13 | **The two session defects** (D-5D §2.3 token transport, §3.2 locked resubmission) | **CLOSED 2026-11-23 — committed `0a50928`** | (a) the admin bearer token is refused for a non-loopback cleartext node **before the socket opens**: `NodeEndpoint.admitsAdminToken`, enforced inside `UltraNetOperatorApi.request`, thrown as `InsecureAdminEndpointException`; (b) a locked wallet can no longer deliver an already-signed transfer (`SubmissionGate`, one decision path for every submit), and an in-flight submission is generation-checked so a lock cannot be bypassed and cannot present itself as a live success. Evidence: `SubmissionGateTest` **4/4**, `NodeEndpointTest` **12/12** (3 new), `AdminTokenTransportTest` **3/3** on the JVM; `LockedWalletSubmissionTest` **PASS 2** on the emulator — it locks the real controller between signing and submitting and asserts no request reaches the node, and separately holds a submission open on the wire, locks, **unlocks**, and asserts a second copy of the same signed transfer is refused. That second case is what makes §3.3's "`lock()` must not reset the in-flight guard" true: the guard (`submissionInFlight`) now lives outside `WalletUiState`, which `lock()` resets | §3.2's second defect and two §3.3 rows are still open — see row 27 |
| 14 | **Naming model** (D-5C item 10.1) | **CLOSED — O-3 approved 2026-11-22 (option C)** | D-5C §2.0: names are labels, the chain id is the identity, no protocol change is required; proven by `two_owners_may_register_the_same_display_name_and_neither_gains_authority` (**PASS**) | — |
| 14a | **AppChain metadata** (D-5C item 10.2, v6 envelope) | **PENDING OWNER** | v6 necessity verified; the squatting question is moot after O-3, so only the consensus change is left | owner decision on 10.2 |
| 15 | **Public contract publication** (D-4) | **NOT STARTED** — basic level unblocked by row 8 | D-4 checklist T1–T14 | D-1 vote; D-5A |
| 16 | **Documentation reconciliation** (D-5A) | **CLOSED 2026-11-23 — committed `c05fb94` (with `f5a532f`, `c3a31ae`)** | All seven §1 defects corrected: guide REST table (already shipped), guide §18.5 ("Active AppChains: 0" replaced with the three live chains, dated), `README.md` anchoring paragraphs, `APPCHAIN_PROTOCOL.md` §8 ("not implemented" + digest pointer + registry has no enforced owner), `APPCHAIN_SERVICE_ARCHITECTURE.md` §7 (implemented vs planned), `android-app/README.md` (three operations through one route; session now describes the idle lock), `docs/runbooks/appchain-owner-key.md` (nobody else can anchor, by construction). Also corrected outside the original seven: `deploy/README.md`, both env examples, `docs/ECONOMIC_ARCHITECTURE_V2.md`, `docs/FAUCET_TECHNICAL_SPEC.md`, `docs/runbooks/ultragrid-phase1-attestation.md`, `docs/CHESS_APPCHAIN_PLAN.md`, `src/bin/fhe_demo.rs`, website `/appchains` metadata | D-1's formal vote is still row 1; check 5 is vacuous until D-4 T4 publishes the digest spec |
| 17 | **Production release — node, O-2 (superseded by 17b)** | **DONE 2026-11-22 18:55 UTC** | Binary `5344ed0c4ac6fb1d646092c8e6f1070125b0757daf123302a1b0d73f42d9fb73`, 25,752,032 B, built from `486dec3` plus the uncommitted `src/api.rs` retirement (`cargo test` gates green, then `cargo build --release --locked --bin UltraNet -j 4`). Rollback `UltraNet.bak-20261122T1853Z` = `ad442a46…` (the previous running binary). Installed `ultranet:ultranet` 0755 by atomic `mv`; **one** restart at 18:55:15 UTC; MainPID 535753, `NRestarts 0`; `/proc/535753/exe` → the installed path at the new sha; listener 127.0.0.1:8081. `chain_length` 16 before and after, mempool 0 | — |
| 17a | **Post-restart endpoint checks (node)** | **PASS** | `GET /api/appchain/ledger` 200, `/ledger/1` 200, `/overview` 200; `POST /api/appchain/tx` 400 on an empty body (route live); the three retired routes answer **401** unauthenticated — mounted and guarded. The 410 body itself is proven by `retired_appchain_anchor_routes_answer_gone`, because reaching it needs an operator credential that was deliberately not read | 410 was unproven over the wire at the time; proven later on the artifact itself, see 17b |
| 18 | **Configurable node endpoint** (D-5D §2) | **DONE — code + JVM tests** | `NodeEndpoint` (https unless loopback; no path, query or credentials), `NodeEndpointStore`, `ConfiguredAppChainGateway` re-reads the setting on every call, endpoint sheet on the AppChains screen; 9 `NodeEndpointTest` cases **PASS** | committed `186e213`; APK not yet released (row 25) |
| 19 | **A-2 external intake** (prepare → review → sign) | **DONE — code + JVM tests** | `PreparedAnchorIntake` (parse, then admission: chain resolved **by id**, owner from L1, fee from the node's terms), intake sheet, and a `SignatureReviewSheet` between every input sheet and the signer; 12 `PreparedAnchorIntakeTest` cases **PASS** | one emulator walk-through |
| 20 | **Idle lock and per-signature confirmation** (D-5D §3) | **DONE for the AppChain path; the two session defects are closed in row 13** | `IdleSession` (clock injected), `WalletController.lockIfIdle`, 30 s poll while unlocked, navigation counts as activity; register / anchor / transfer all confirm on the review screen; 6 `IdleSessionTest` cases **PASS** | — (closed in row 13) |
| 21 | **Android AppChain write-path tests** | **CLOSED 2026-11-22 — committed `186e213`** | `AppChainsFlowTest`, **5 tests, PASS 5**, 16 s (run 18:40 UTC, command `android-app/run-instrumented-tests.sh AppChainsFlowTest`): `registrationShowsTheWholeTransactionBeforeTheSignatureIsTaken`, `anIntakeAnchorDisplaysAndSignsExactlyTheSameRoot`, `aRequestForAChainAnotherWalletOwnsIsRefusedBeforeAnySignature`, `transferOwnershipShowsTheReviewedChainAndRecipientBeforeSigning`, `aWalletThatDoesNotOwnTheChainIsOfferedNoTransferAtAll`. With 27 JVM unit tests and `GroupedHomeScreenTest` 8/8, `AppChainSignerTest` 9/9 | — |
| 22 | **Website: retired anchor route removed** | **DONE and DEPLOYED 2026-11-22** | `anchorAppChain` / `anchorAppChainForTest` / `submitAnchor` deleted from `src/lib/appchains.ts`; `AnchorNowControl` replaced by `OwnerAnchorNotice`; console action state and result types removed; badge, cards and copy state the owner-signed model. Release commit `53177f77e7c5e2114de91a37516373d64deee80c`, archive sha256 `dacab143…`, deployed by `/usr/local/sbin/ultranet-dashboard-deploy` → **build id `e-vLnjxZUtv7aKDirg7n8`**, dashboard and validator services both `active`. Shipped chunk `app/appchains/page-a6cf9cd0ed4d2de6.js`: `ANCHOR_FROM_OWNER_WALLET` ×1, `ANCHOR NOW` ×0, `SERVER_VERIFIED_ANCHORING` ×0, and the only `/api/appchain/` reference is the read-only `overview` GET. Vitest 145/145, `tsc` clean, `eslint` clean, a11y gate 18 routes / 0 violations | the browser still has no AppChain signer, by design |
| 23 | **Developer documentation corrections** | **DONE — shipped** | `ULTRA_NET_TECHNICAL_GUIDE.md`: the AppChain REST table lists `/api/appchain/tx`, `/ledger`, `/ledger/{id}`, `/ledger/{id}/anchors`, `/overview` and marks `create`, `anchor`, `{id}/anchor` and `{id}/anchor/test` as **410**; the `verify_low_degree` claim about anchors corrected; the Mermaid flow and the `anchor_appchain` paragraph now describe the owner-signed path. `ultraverse.ts` AppChains facts corrected (chain id is the identity, one L1 owner, owner-signed, the wallet signs). Whitepaper HTML regenerated from the corrected guide during the deploy (9 occurrences of `/api/appchain/tx`) | — (banner corrected in `cda5132`; the restarted node's journal now prints `RETIRED (410)` for all three AppChain route lines) |

| 17b | **Production release — node, provenance-closed build (current)** | **DONE 2026-11-22 20:00 UTC** | Binary `69aeddd94bccc863951b7126befec83c45f21a57db126bd2305d5d9cf6b7e441`, 25,750,552 B, built from `cda5132` with `cargo build --release --locked --bin UltraNet -j 4` (exit 0, 30.05 s, rustc 1.97.0 pinned). Rollback made before promotion: `UltraNet.bak-20261122T1959Z` = `5344ed0c…`. Atomic promote by `install` + `mv`; **one** restart at 20:00:28Z; `active`, MainPID **540014**, `NRestarts 0`, `/proc/540014/exe` sha `69aeddd9…`, listener 127.0.0.1:8081; API answering after 14 s. `chain_length` 16, `epoch` 15, `auto_mined_blocks` 0 — unchanged. `ledger`/`ledger/1`/`overview` 200, `POST /api/appchain/tx` 400 live, retired routes 401 without a credential, journal banner prints `RETIRED (410)` on all three AppChain lines, 0 panics, Records and dashboard services not restarted | see remaining limitation below |
| 17c | **410 over the wire on the deployed artifact** | **PASS 2026-11-22** | The new artifact was run locally on a throwaway DB, P2P port 19000 and a throwaway bearer token: `/1/anchor` → 401 without credential, **410** with it, body "Anchoring AppChain #1 from the node is retired. Only the chain owner can anchor it: … post it to /api/appchain/tx"; `/1/anchor/test` → 410 likewise | not exercised on the production listener, which needs the operator token (not read) |
| 24 | **Source provenance for the deployed binary** | **CLOSED 2026-11-22** | Three local commits: `c28f174` (retirement + 10 authorization tests, 2 files, staged diff reviewed before commit), `186e213` (Android client, 14 files), `cda5132` (banner + regression test, 1 file). The predecessor `5344ed0c…` was byte-identical to the local artifact and its `src/api.rs` (mtime 17:16Z) predated the 18:51Z build, so the previously unattributable binary maps to `c28f174` as well | local only; not pushed |
| 25 | **Android release (APK)** | **NOT DONE — committed only** | `186e213` holds the client work. No release APK was built or distributed, and the endpoint / intake / lock sheets have not been walked on a physical phone | release decision |

| 26 | **D-5A consistency checks (scriptable)** | **PASS 2026-11-23** | 77 markdown files scanned. (1) retired route named without a status: **0** (after `docs/CHESS_APPCHAIN_PLAN.md:116`, the last one, was corrected). (2) capabilities claimed without implementation: 0 hits for unique-name or metadata claims. (3) all **36** routes named in the guide resolve to a literal in `src/api.rs` or are marked retired: **0 unresolved**. (4) the guide's chain count matches `GET /api/appchain/ledger`: **3** chains (#1, #2, #3), and the stale "Active AppChains: 0" prose is gone. (5) relative-link check: **0** broken project links — the single hit is `./.kombai/debug/…`, a local scratch file that is not part of the repository | check 5 of D-5A §5 (digest spec vs pinned vectors) is vacuous until D-4 T4 publishes one |
| 27 | **D-5D §3 requirements not yet implemented** | **OPEN — three separate items** | (a) **§3.2 second defect:** a failed AppChain submission is not marked unresolved and does not refresh the nonce, so a timeout after the node accepted it leaves a stale nonce and the next attempt fails as "Invalid nonce!". (b) **§3.3 failure semantics:** a transport failure must be distinguishable from a rejection and must refresh the account before any retry — `signAppChain`'s catch block has neither, only a message. (c) **§3.3 screen-off lock:** lock on screen-off, not only `ON_STOP`; today the triggers are the manual action, `ON_STOP`, and the idle poll. §3.3's *unresolved-state* clause is now fully implemented and tested (row 13) | needs its own approval; (a) and (b) touch `signAppChain`, (c) touches the lifecycle observer |

---

## 2. What the executed tests do and do not establish

**Established:** on this revision, the node refuses (i) a foreign owner anchoring or transferring, (ii) a
payload changed after signing, (iii) a substituted sender, (iv) a reused nullifier with different content,
(v) a non-v5 AppChain envelope, (vi) unknown fields, (vii) an unaffordable registration, and (viii) both
retired anchor routes — while still accepting an identical re-delivery as `pending` and serving the
documented read shapes.

**Not established:** anything about the Android client, the external-intake flow, independent hosting,
production behaviour of the new binary, or the mutation runs. No production claim follows from a local test.

---

## 3. Consolidated owner decision request

Three decisions remain; O-3 is closed. Each is answerable independently.

| ID | Decision | Why it is the owner's |
|---|---|---|
| **O-1** | Approve D-1 (owner-signed is the only model) — architecture only | it fixes the authority model for every future chain |
| **O-2** | Approve the relocation of the legacy anchor route to 410 **for deployment**, i.e. authorise a node build + restart window | irreversible-looking for any external caller, and it needs a production restart |
| **O-3** | ~~Choose the naming model for 10.1~~ **APPROVED 2026-11-22: option C** — unique L1 ids, display names not unique, no economics change | closed; recorded in D-5C §2.0 and proven by a passing test |
| **O-4** | Authorise A-2 and D-5D implementation (mobile intake, endpoint configuration, lock/confirmation) | both change the wallet's user-facing behaviour |

Not requested, because they are outside what has been authorised: changing fees, transferring ownership of
any chain, spending ULTRA, submitting any anchor or transaction, and touching production credentials.

---

## 4. Sequence from here

1. **O-1 and O-4** can be answered in any order. Only O-2 gates a production change. O-3 is closed.
2. O-4 work landed in rows 18–20 (endpoint, intake, review-before-signing, idle lock). What is left is
   evidence, not construction: one instrumentation class that drives the three sheets, then a device walk.
3. With O-1: D-5A documentation reconciliation, then D-4 basic-level publication (its evidence is row 8).
4. D-5C item 10.1 is closed and needs no protocol work. Only 10.2 (metadata, v6) remains, under a separate
   approval.
5. With O-2: rebuild the node, deploy under the existing A0a/A0b-style gates, re-run row 2 and row 8 on the
   deployed binary, then claim LIVE — and only then.

## 5. Verification of this checklist

- Repository: `486dec3` plus the O-3/O-4 work. Node: `tests/appchain_authorization_test.rs` (one test added,
  ten **PASS**). App: new `data/NodeEndpoint.kt`, `data/NodeEndpointStore.kt`, `data/AppChainGateway.kt`,
  `wallet/PreparedAnchor.kt`, `wallet/IdleSession.kt`, `app/src/test/` (27 cases **PASS**), and edits to
  `OperatorApi`, `WalletController`, `AppChainsScreen`, `UltraVerseApp`, `build.gradle.kts`. Nothing
  committed or pushed.
- Device: `AppChainsFlowTest` 3/3 PASS (18:25 UTC), plus `GroupedHomeScreenTest` 8/8 and
  `AppChainSignerTest` 9/9 earlier. Commands are recorded in rows 21.
- Website: `npx tsc --noEmit` clean, `npm run lint` clean, `npm test` 145/145 PASS (16 files); a11y gate 18
  routes, 0 violations; public smoke `/`, `/appchains`, `/docs/whitepaper`, `/docs/ultranet-whitepaper.html`,
  `/download` all 200, and the CI's own whitepaper assertions both satisfied.
- Node deployment: see rows 17 and 17a. One restart, no loop, no unrelated service change; the Records and
  dashboard services were not restarted (`ultranet-records` untouched throughout).
- Node deployment, current build: see rows 17b and 17c. One restart, provenance-closed source, 410 proven
  on the artifact, banner corrected in the journal.
- Commits: `c28f174`, `186e213`, `cda5132`, all local. Nothing pushed, no APK distributed, no L1
  transaction, no credential read, no fee or key change.
- Committed 2026-11-23 in four commits: `c05fb94` (13 documentation files), `f5a532f` (the `fhe_demo`
  example), `c3a31ae` (website metadata), `0a50928` (both Android session defects, 8 files). This checklist
  and `docs/PRODUCTION_READINESS_REPORT_2026-11-22.md` are committed alongside them. Not pushed.
- D-5A / D-5D change set (2026-11-23): `cargo build --bin fhe_demo` exit 0;
  `cargo test --lib configuration_tests` **27/27 PASS**; `cargo test --test appchain_authorization_test`
  **10/10 PASS**; `./gradlew testDebugUnitTest --rerun-tasks` **37/37 PASS** (27 existing + `SubmissionGateTest`
  4, `AdminTokenTransportTest` 3, `NodeEndpointTest` +3); `LockedWalletSubmissionTest` **PASS 2**,
  `AppChainsFlowTest` **PASS 5**, `UltraNetPayScreenTest` **PASS 2** on the emulator; website `tsc` clean and
  `vitest` **145/145**. No service was restarted, no ABI, fee or credential touched.
- API edge `api.ultranetwork.cc`: `ledger` 200, `overview` 200, `/api/appchain/tx` 400 empty-body, the three
  retired routes 401. The website origin (`ultranetwork.cc`) does not proxy the node API at all — those
  paths answer 404 there, so no retired route is reachable from the public site.
- No L1 transaction was created or submitted by this work: no registration, anchor, transfer or payout.
  No credential, key, fee, economic, firewall, Caddy or Cloudflare change was made.
- Documents: D-1, D-4, D-5B, D-5C, D-5D corrected in place; P0 and A-2 unmodified.
