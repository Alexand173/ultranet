# O-1 audit — is AppChain authorisation owner-signed?

**Date:** 2026-11-24 · **Audited against:** `b9bfb293b99bf49a064b86e9bda60b070774aa4c`
**Scope:** the authority model only — who may register a chain, anchor it, transfer it. Not a review of
the games, the wallet UI or the deployment.

> **This document records evidence. It is not the decision.** Owner decision **O-1** — "owner-signed is
> the only model" — was **APPROVED on 2026-11-24** and is recorded separately in
> [O1-OWNER-DECISION-2026-11-24.md](./O1-OWNER-DECISION-2026-11-24.md)
> ([checklist row 1](../appchain-completion-checklist-rev1.md)). The approval is in that record, not
> here; this page is what the decision was based on and stays as written so the reasoning can be checked.

---

## 1. Verdicts

| # | Requirement | Verdict |
|---|---|---|
| R1 | Every AppChain has a unique L1 chain id; display names are labels, not identity | **PASS** |
| R2 | Registration, anchoring and ownership transfer are authorised by L1 transaction rules | **PASS** |
| R3 | The transaction sender is derived and verified from the signing public key | **PASS** |
| R4 | Anchor authorisation checks the actual owner of the target chain | **PASS** |
| R5 | The mobile wallet signs; the node holds no owner private key and never signs for an owner | **PASS**, by bounded search — see §6.1 |
| R6 | `POST /api/appchain/tx` is the supported owner-signed submission route | **PASS** |
| R7 | Every retired server-side anchor write route stays retired and cannot authorise a write | **PASS** |
| R8 | No website, backend, CLI, service or legacy route silently provides a server-authorised write path | **PASS**, one qualification in §6.2 |
| R9 | Chain ids, owners, registrations and token economics are unchanged | **PASS**, read-only live check |
| R10 | No remaining contradiction between implementation, tests and committed documentation | **PASS** after the corrections in `23685d9`; the five defects found are in §5 |

`PASS` means evidence in this repository supports the requirement today. It does not mean the model has
been ratified.

---

## 2. R1 — unique L1 chain ids; names are labels

- The id comes from consensus state, not from the caller: `src/appchain/ledger.rs:313` reads
  `view.next_chain_id()`, `:341-344` assigns it and increments with an overflow guard
  (`"AppChain ID space is exhausted"`).
- It is persisted as a counter that starts at 1: `src/storage.rs:1731-1740` (`next_id`, big-endian u32;
  a malformed record panics rather than silently reusing an id).
- Registration is idempotent per transaction: `ledger.rs:327-339` replays an existing registration by
  transaction hash instead of allocating a second id.
- Names are **not** unique: `ledger.rs:90-115` (`validate_name`) enforces length ≤ 64, no leading or
  trailing space, and the character set `A-Za-z0-9 -_`. There is no duplicate-name check anywhere.
- A name is still committed — `registration_digest` (`ledger.rs:290-299`) hashes id, name length, name,
  owner and the registering transaction — so it is part of what validators agree on. It is a label in
  the identity sense (two chains may share one), not an uncommitted string.
- Owner selection: `ledger.rs:345-354`, `owner: tx.sender.clone()` — the signer becomes the owner.

## 3. R2 — authorisation by L1 transaction rules

The obligation is layered: shape, then authorisation, then re-execution.

| Layer | Where | What it enforces |
|---|---|---|
| Envelope | `src/appchain/ledger.rs:118-169` | version 5 exactly (`:119-123`), L1 chain id 0 (`:124-126`), fixed recipient `appchain` (`:127-131`), amount 0 (`:132-134`), no ZK proof (`:135-137`), `ProofType::Ownership` (`:138-140`), fixed gas (`:141-143`), 64-hex sender (`:144-146`), fee ≥ the operation's floor (`:147-150`), name rules (`:151-153`), transfer target rules (`:154-168`) |
| Version gate | `src/lib.rs:2352-2362` | a version that is not 5 for an AppChain payload is refused before the AppChain rules run |
| Authorisation | `src/lib.rs:2465-2489` | registration needs activation; anchor and transfer need the chain to exist **and** `owner == sender` |
| Fee constants | `ledger.rs:39-50` | register 10,000, anchor 1,000, transfer 1,000 base units; gas 100,000 × 1 |
| Planning | `ledger.rs:306-380` | the same rules are re-applied when a block is planned; a block carrying an unauthorised anchor is invalid, not merely refused |
| Replay | `ledger.rs:369-376` | a replayed anchor is *reproduced*, not re-authorised — re-checking ownership could reject a block already committed after an owner change |
| Mining | `src/lib.rs:3007-3027` | `mine_block` re-runs `validate_transaction` per transaction (`:3013`) and drops AppChain transactions that cannot be mined against the parent ledger (`:3027`) |

The anchor fee is paid by the owner's L1 balance, not by a treasury: there is no treasury field on
`LedgerChain` (`ledger.rs:205-217`) at all.

## 4. R3 — sender derived from the public key

`src/lib.rs:155-159`:

```rust
pub fn address_from_public_key(public_key: &[u8]) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(public_key);
    hex::encode(hasher.finalize())
}
```

`src/lib.rs:2625-2632` refuses a transaction whose claimed sender is not that hash
(`"Sender address does not match the public key (identity spoofing)!"`) and then verifies the Dilithium
signature over the envelope digest. `sender_public_key` is **not** part of the preimage; it is bound by
this equality instead (`ledger.rs:174-203`).

## 5. R4 — the owner of the target chain is checked

- Admission: `src/lib.rs:2469-2478` loads the chain from the persisted ledger and compares
  `chain.owner != tx.sender` (`"Only the owner of AppChain #{chain_id} may anchor it"`).
- Block planning: `src/appchain/ledger.rs:363-376` repeats it against the parent state plus the overlay,
  so a registration and an anchor in the same block see the same owner.
- Transfer: `src/lib.rs:2479-2489` — owner only, and not to the address that already owns it; plus
  `ledger.rs:165-167` — an owner cannot transfer to itself.

## 6. R5, R6, R7, R8 — signing, submission, retirement, and the absence of a bypass

### 6.1 The wallet signs; the node does not

- Wallet preimage and signing: `android-app/.../wallet/AppChainSigner.kt:69` (`TX_VERSION = 5`),
  `:83-89` (`requireSupportedVersion`), `:101` (envelope domain), `:203-263` (`sign`, which derives the
  address from the seed and refuses a mismatch at `:236`).
- What crosses the wire: `data/OperatorApi.kt:90-101` sends `sender`, `sender_public_key`, `nonce`,
  `timestamp`, `nullifier`, `fee`, `signature`, `version`, `operation` — no seed, no private key — to
  `POST /api/appchain/tx` with `adminToken = null`.
- The node side: a bounded search for owner-key loading over `src/api.rs`, `src/lib.rs`,
  `src/appchain/**` and `src/runtime_config.rs` for `owner_key`, `owner_key_file`, `--key-file` and
  `APPCHAIN_OWNER` returns only the retirement *message text* at `api.rs:1443`. The node has no code
  path that loads or uses an AppChain owner key.
- **Method honesty:** this is a negative claim established by a bounded search of the node's own
  modules, not a proof about every possible build. It is consistent with the design: L1 records roots,
  it never produces them.

### 6.2 R6 — the supported route

`src/api.rs:3780` mounts `POST /api/appchain/tx` publicly; the handler is `api.rs:1722-1750`, the body
type `AppChainTxRequest` (`api.rs:1661`) is `deny_unknown_fields`, and the request is converted to a
`Transaction` by `appchain_request_to_transaction` (`api.rs:1673-1720`) before `add_transaction`
(`src/lib.rs:2106`) applies the rules in §3.

The CLI is a client of the same route, not a second path: `src/bin/appchain-cli.rs:245-264` decrypts the
key file locally (Argon2 + ChaCha20-Poly1305), `:315-317` signs locally, and `:319-332` posts to
`{node}/api/appchain/tx`.

### 6.3 R7 — retirement holds

| Route | Handler | Answer |
|---|---|---|
| `POST /api/appchain/create` | `api.rs:1440-1445` | `410 Gone`, body names `/api/appchain/tx` |
| `POST /api/appchain/{id}/anchor` | `api.rs:1585-1593` | `410 Gone` |
| `POST /api/appchain/anchor` | `api.rs:1601-1606` | `410 Gone`; takes no body extractor |
| `POST /api/appchain/{id}/anchor/test` | `api.rs:1612-1620` | `410 Gone` |

All four are mounted behind `require_admin_token` (`api.rs:3599-3618`) and none of them calls a storage
mutator. The status is reached without parsing a body, so a malformed request cannot turn a retired
route into a `400` that looks alive — asserted on all four routes by
`retired_appchain_anchor_routes_answer_gone`.

### 6.4 R8 — no silent alternative write path

- **HTTP:** the only route that changes AppChain consensus state is `POST /api/appchain/tx`. Ledger
  writes otherwise happen when a block is applied (`apply_appchain_ledger` ← `update_state`), reached
  from `mine_block` (which re-validates, §3) or from peer block intake (`add_remote_block`, which runs
  `validate_block`).
- **`/api/mine`** is admin-gated but cannot authorise: it can only include transactions that already
  passed `validate_transaction` (`src/lib.rs:3013`).
- **Admin gate caveat:** `require_admin_token` (`api.rs:440-444`) accepts the bearer token *or* a valid
  wallet session cookie. For AppChain this is inert, because the routes it guards answer `410`.
- **Migration binary:** `src/bin/ultranet-appchain-migrate.rs` writes the legacy factory trees only,
  requires `--apply`, and refuses to run against a live node (`ensure_node_stopped`). It is offline.
- **Website:** read-only. `website/src/lib/appchains.ts:166-174` is a `GET` of
  `/api/appchain/overview`; `website/src/lib/appchains.test.ts:90-104` guards against an anchor export,
  a `POST`, or a template-built `/api/appchain/${…}` route.
- **Services:** the Records Service anchors by running `appchain-cli anchor` with an owner key file
  (`src/games/anchor.rs:133-149`, `src/games/records_config.rs:3,184`). That is an owner-signed
  transaction from a *client*, not a node-side bypass — and it only works on chains whose owner key it
  holds (§6.5).
- **Legacy/Move:** `src/move_contracts/AppChainFactory.move` has its own registry, is referenced by no
  Rust code, and cannot write the AppChain trees.

### 6.5 R9 — unchanged state and economics

Read-only, `GET` only, no credential, 2026-11-24:

| Check | Result |
|---|---|
| terms | `version 5`, register `10000`, anchor `1000`, transfer `1000`, gas `100000`/`1`, activation height `10` |
| chains | #1 UltraGrid `7d2cd1cc…` (block 11, 1 anchor), #2 Registry `d4fca721…` (block 14), #3 Ultra Chess `7d2cd1cc…` (block 15) |

The terms match the constants at `src/appchain/ledger.rs:39-50`, and the owners match
[checklist row 9](../appchain-completion-checklist-rev1.md). Because the Registry signer (`d4fca721…`)
owns only #2, an anchor it submits for #1 is refused by L1 and retried — the allow-list in
[records-service.md](../runbooks/records-service.md) is a ceiling, not a grant.

---

## 7. Tests executed for this audit

One target at a time, `--test-threads=1`, on `b9bfb29`:

| Command | Result |
|---|---|
| `cargo test --locked -j 1 --test appchain_authorization_test` | **11 passed, 0 failed** (257.20 s) |
| `cargo test --locked -j 1 --test appchain_wire_format_test` | **3 passed, 0 failed** |
| `cargo test --locked -j 1 --test api_json_errors_test` | **7 passed, 0 failed** |
| `cargo test --locked -j 1 --test appchain_api_test` | **1 passed, 0 failed** |
| `… --lib a_registry_chain_describes_its_retired_mechanism_truthfully` | **1 passed** |
| `… --lib retired_appchain_anchor_routes_answer_gone` | **1 passed** |
| `cargo check --locked --all-targets` | Finished, no errors |

The authorization suite covers, by name: `an_identity_that_owns_one_chain_cannot_anchor_another`,
`a_foreign_owner_cannot_transfer_and_an_owner_cannot_transfer_to_themselves`,
`a_substituted_sender_is_rejected`, `a_payload_modified_after_signing_is_rejected`,
`two_owners_may_register_the_same_display_name_and_neither_gains_authority`,
`a_nullifier_cannot_be_reused_but_an_identical_resubmission_is_accepted`,
`an_envelope_that_is_not_version_five_is_rejected`, `an_unknown_field_is_rejected`,
`an_unknown_field_inside_each_operation_is_rejected`, `a_registration_without_balance_is_rejected`,
`the_read_surface_answers_unknown_chains_and_pages_by_sequence`.

**Not re-executed in this pass:** the Android JVM and device suites. R5 rests on reading the signer and
the request builder, plus the pinned digest vectors mirrored in `ledger.rs` and the CLI; the last
recorded Android run is in [checklist row 13](../appchain-completion-checklist-rev1.md). Treat R5's
*wallet behaviour* as proven by inspection here and by those recorded runs, not by a run made today.

---

## 8. Documentation discrepancies found, and their status

All five were corrected in `23685d9` (documentation only).

| # | Document | Was | Now |
|---|---|---|---|
| D1 | `README.md:123` | admin bearer token covers "AppChain management" | names mining and pruning, and says AppChain writes are signature-authorised |
| D2 | `docs/APPCHAIN_FIRST_GAME_PLAN.md:15-17,33` | present-tense admin-only registration/anchoring, client roots refused, treasury debit; "Retire once active" still open | §1 is dated as a snapshot with a current-state note; the list states what each item's status actually is |
| D3 | `docs/CHESS_APPCHAIN_PLAN.md:54,64,98` | treasury-funded batched anchor; `POST /api/appchain/{id}/state` operator-authed | diagram captioned as the superseded sketch, cost charged to the owner wallet; the §3 table is struck through and marked never built |
| D4 | `docs/runbooks/records-service.md:29-30`, `docs/runbooks/games-service.md:209-211`, `docs/APPCHAIN_SERVICE_ARCHITECTURE.md:85,106` | implied the Records service could anchor any allow-listed chain, chain #1 included | states the allow-list is a ceiling; the signer owns #2 only, so #1 is refused and retried until the handover |
| D5 | `docs/runbooks/appchain-owner-key.md:177-181` | "See D-1 for the decision that made this the only model" | separates implemented behaviour from ratified policy; O-1 is PENDING OWNER |

Discrepancies that remain, deliberately:

- **Production still reports the pre-C3 `proof_scheme` string** (`SHA3-256 server state commitment v1`)
  because the corrected artefact `14aff6d0…` is built and recorded but not promoted — see
  [NODE_BINARY_RELEASES.md](../../deploy/NODE_BINARY_RELEASES.md).
- **O-1 itself**, below.

---

## 9. Blockers

1. **O-1 is unrecorded.** The implementation is owner-signed and this audit supports the decision, but
   the owner vote is the remaining gate. D-4 publication stays blocked until it is recorded.
2. **Promotion of the C3 artefact** (`14aff6d0…`) is a separate decision with its own build-and-restart
   gate. Nothing was deployed as part of this audit.
3. **Checklist row 9** (peer-anchor on chain #1) still needs a production environment change — either the
   ownership handover, or removing `1` from `RECORDS_PEER_ANCHOR_CHAINS`.

## 10. What this audit does not establish

- It does not approve O-1, D-1, D-4 or anything else, and is not a signature.
- It does not claim the node is production-ready, live, or publication-ready.
- It does not cover the games economics, the wallet UI, or the website's presentation.
- It does not re-prove the Android behaviour today (§7).
