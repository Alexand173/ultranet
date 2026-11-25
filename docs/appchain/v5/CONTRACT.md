# UltraNet AppChain protocol — version 5

**Status: prepared for publication, not published.** This document is the contract a third party can
hold UltraNet to. It describes what the node does today, at `d4165fc`; every claim cites a file and line,
or a live response. Publication itself is a separate gate (D-4 §8) and has not happened.

**Decision basis:** owner decision **O-1** — recorded 2026-11-24 in
[`../O1-OWNER-DECISION-2026-11-24.md`](../O1-OWNER-DECISION-2026-11-24.md), from the technical audit in
[`../O1-OWNER-AUTHORISATION-AUDIT.md`](../O1-OWNER-AUTHORISATION-AUDIT.md).

| | |
|---|---|
| Envelope version | **5** (`src/appchain/ledger.rs:29`) |
| Digest | SHA3-256 over the preimage in §4 |
| Machine-readable | [`contract.schema.json`](./contract.schema.json) (request, error envelope, read models) |
| Vectors | [`vectors.json`](./vectors.json) (three pinned digests, generated from `ledger.rs:820-822`) |
| Networks | L1 chain id **0**; AppChains are L1 objects, not separate networks |
| Fees | register 10,000 · anchor 1,000 · transfer 1,000 base units (`ledger.rs:39-45`) |

---

## 1. Identity: the chain id is the identity, the name is a label

- A chain is created by a signed `AppChainRegister` transaction. **The node assigns the chain id** from
  consensus state: `ledger.rs:313` reads the counter, `:341-344` increments it with an overflow guard.
  The counter is persisted (`src/storage.rs:1731-1740`), starts at 1, and is never reused.
- **The signer becomes the owner**: `ledger.rs:345-354` sets `owner: tx.sender.clone()`.
- **Display names are not unique and confer no authority.** `validate_name` (`ledger.rs:90-115`) enforces
  at most 64 characters from `A-Za-z0-9 -_`, no leading or trailing space, and nothing else. Two owners
  may register the same name; neither gains anything from it (test
  `two_owners_may_register_the_same_display_name_and_neither_gains_authority`). The name *is* committed
  into the registration digest (`ledger.rs:290-299`), so it is part of what validators agree on — it is a
  label in the identity sense, not an uncommitted string.

## 2. Routes

| Method | Path | Auth | Answers |
|---|---|---|---|
| POST | `/api/appchain/tx` | **none — the signature is the authorisation** | `200 {success:true, message, data:{hash, status:"pending"}}`; `400 {success:false, message}` |
| GET | `/api/appchain/ledger` | public | `200 {success, terms, chains}` |
| GET | `/api/appchain/ledger/{chain_id}` | public | `200 {success, chain}`; `404` `AppChain #{id} is not registered` |
| GET | `/api/appchain/ledger/{chain_id}/anchors` | public | `200 {success, chain_id, from, anchors}`; **unknown chain → `200` with `[]`** |
| GET | `/api/appchain/list` | public | `200 {success, chains:[AppChainView]}` |
| GET | `/api/appchain/overview` | public | `200 {success, chains, totals, anchor_availability, proof_scheme, updated_at}` |
| GET | `/api/appchain/anchors` | public | **legacy factory-registry history, not ledger anchors.** Not part of this contract |

### Retired write routes — `410 Gone`, permanently

| Method | Path | Status |
|---|---|---|
| POST | `/api/appchain/create` | `410`, body names `/api/appchain/tx` |
| POST | `/api/appchain/{chain_id}/anchor` | `410` |
| POST | `/api/appchain/anchor` | `410` |
| POST | `/api/appchain/{chain_id}/anchor/test` | `410` |

All four (`src/api.rs:1440-1620`, mounted `3599-3618`) are reached without parsing a body, so a malformed
request cannot turn one into a `400` that looks alive. Test
`retired_appchain_anchor_routes_answer_gone` asserts `401` without a credential, `410` with one, and
`410` with a malformed body on all four. **They must not be reintroduced** (O-1 clause 5).

## 3. Request envelope

Nine fields, all required, **unknown fields rejected at both levels** (`deny_unknown_fields` on the
request and on the operation enum — `src/api.rs:1626-1661`):

```json
{
  "sender": "<64 lowercase hex>",
  "sender_public_key": [<integers 0-255>],
  "nonce": 0,
  "timestamp": 0,
  "nullifier": [<32 integers 0-255>],
  "fee": 0,
  "signature": [<integers 0-255>],
  "version": 5,
  "operation": { … }
}
```

Byte fields are **integer arrays, not hex strings** — pinned by `tests/appchain_wire_format_test.rs`
(3 tests). Operations are externally tagged, one key each:

```json
{"register": {"name": "UltraNet Grid"}}
{"anchor": {"chain_id": 1, "state_root": "<64 hex>"}}
{"transfer_owner": {"chain_id": 1, "new_owner": "<64 hex>"}}
```

`sender_public_key` is **not** in the preimage. It is bound by equality instead: the node recomputes
`sender = hex(SHA3-256(public_key))` and refuses a mismatch (`src/lib.rs:2625-2630`,
`address_from_public_key` at `:155-159`).

## 4. Signing preimage

The node rebuilds this from the request and verifies the Dilithium-5 signature over the SHA3-256 digest
(`lib.rs:2808-2862` → `ledger.rs:174-203`):

```
sender(ascii) ‖ recipient="appchain"(ascii) ‖ amount u64LE (0)
  ‖ fee u64LE ‖ timestamp u64LE ‖ nullifier (32 raw bytes) ‖ nonce u64LE
  ‖ gas_limit u64LE (100000) ‖ gas_price u64LE (1)
  ‖ "UltraNet/appchain-envelope/v5" ‖ version u32LE (5) ‖ chain_id u32LE (0)
  ‖ tag ‖ payload
```

| Operation | tag | payload |
|---|---|---|
| register | `AppChainRegister` | `len(name) u64LE ‖ name(ascii)` |
| anchor | `AppChainAnchor` | `chain_id u32LE ‖ state_root (32 raw bytes)` |
| transfer | `AppChainTransferOwner` | `chain_id u32LE ‖ len(new_owner) u64LE ‖ new_owner(ascii)` |

Note the asymmetry, which is intentional and easy to get wrong: the **anchor root is raw bytes**, the
**new owner is ASCII hex text**.

**Pinned vectors** — reproduce all three exactly or clients will be rejected:

| Operation | Digest |
|---|---|
| register `"UltraNet Grid"` | `7dcaf29f0809d289fdf6e10223ae79c6e7e1b319d45c51de8b17550756130f90` |
| anchor chain 1, root `ab`×32 | `f1c64d0830b020aa34c0add54ae7815f61cbe0007e2b6795e844bbbfadd43a72` |
| transfer chain 1 to `2859eb67…` | `702b1dd8dcdf04061511aea85a9d049843b3e2e531cca690b9176d50b8bcf9ab` |

Full inputs and field encodings: [`vectors.json`](./vectors.json). The same three are asserted by the
node (`version_5_digests_are_pinned`), the CLI, and the Android device test.

## 5. Fees, gas and activation

| Term | Value | Source |
|---|---|---|
| register | 10,000 base units (0.01 $ULTRA) | `ledger.rs:39` |
| anchor | 1,000 base units (0.001) | `ledger.rs:41` |
| transfer | 1,000 base units (0.001) | `ledger.rs:45` |
| gas limit / price | 100,000 / 1 (fixed) | `ledger.rs:49-50` |
| recipient | `"appchain"` — no account, moves no amount | `ledger.rs:32` |
| activation height | operator-set, per network; **unset means disabled** | `ledger.rs:54-60` |

The fee is paid by the **owner's L1 balance**. There is no treasury on a ledger chain: `LedgerChain`
(`ledger.rs:205-217`) has no treasury field at all. A fee above the minimum is accepted (`>=`).

These are the values a node publishes at `GET /api/appchain/ledger` under `terms`; read them rather than
hard-coding, because a client that hard-codes a fee will be refused after a fee change.

## 6. Admission rules

| Rule | Behaviour |
|---|---|
| Version | must be exactly 5 for an AppChain payload; a non-5 version is refused with the **generic** `Unsupported transaction version!` before the AppChain rules run (`lib.rs:2352-2362`) |
| Shape | `validate_envelope` (`ledger.rs:118-169`): L1 chain 0, fixed recipient, amount 0, no ZK proof, `ProofType::Ownership`, fixed gas fields, sender is 64-hex, fee ≥ floor, name/transfer target rules |
| Signature | Dilithium-5 over §4; sender must equal the hash of the public key (`lib.rs:2579-2636`) |
| Registration | needs activation; any funded address may register |
| Anchor | the chain must already be registered, and **`sender` must equal the chain's owner** (`lib.rs:2469-2478`) |
| Transfer | owner only, and not to the current owner (`lib.rs:2479-2489`, `ledger.rs:165-167`) |
| Nonce | `max(confirmed, highest pending + 1)`; a transaction already reserved in the mempool is exempt |
| Nullifier | globally unique by compare-and-swap. **An identical resubmission of an already-confirmed transaction returns `200` with `status:"pending"`** — `pending` means *accepted*, never *confirmed* |
| Freshness | `timestamp` more than 60 s in the future, or more than 3600 s old, is refused |

**Re-execution:** the same owner rules are applied when a block is planned (`ledger.rs:306-380`), so a
block carrying an unauthorised anchor is invalid, not merely refused. A replayed anchor is *reproduced*,
not re-authorised (`ledger.rs:369-376`) — re-checking ownership would reject blocks already committed
after a handover.

## 7. Error model

Every JSON route answers with the same shape — `success: false` plus `message` — but **`data` is
inconsistent**: some producers send `"data": null`, others omit the key. Both were observed on the
running node on 2026-11-25:

| Request | Response |
|---|---|
| `POST /api/appchain/tx` with an incomplete body | `400 {"data":null,"message":"Malformed request: Json deserialize error: missing field \`sender_public_key\`…"}` |
| `POST /api/appchain/create` without a credential | `401 {"success":false,"message":"Administrator bearer token required","data":null}` |
| `GET /api/appchain/ledger/999` | `404 {"message":"AppChain #999 is not registered","success":false}` — no `data` key |

A client must therefore treat `data` as optional on errors and read `success`/`message`. Do not write a
decoder that requires `data`, and do not write one that assumes its absence.

| Status | When |
|---|---|
| `400` | malformed JSON, unknown field (top-level or nested), wrong type, missing content type, rejected transaction, unknown chain on `/ledger/{id}` |
| `413` | request body over the limit |
| `404` | `AppChain #N is not registered` |
| `410` | a retired write route, naming `/api/appchain/tx` |
| `500` | the node could not read its own ledger |

Pinned by `tests/api_json_errors_test.rs` (7 tests, including the `413` case).

## 8. Read semantics

- `from` on `/ledger/{id}/anchors` is an **anchor sequence**, not an offset or a timestamp.
- An unknown chain on `/anchors` returns `200` with an empty array; on `/ledger/{id}` it returns `404`.
  That asymmetry is deliberate: paging a chain you have not registered yet is not an error.
- `proof_scheme` is informational: ledger chains report
  `owner-signed state root (a signature, not a state proof)`, and it is not a claim that the node proves
  state.
- `anchor_availability` is `"retired"` for the node: the node cannot anchor for anyone.

## 9. Versioning policy

1. The envelope version is part of the domain string; supporting another version means shipping a new
   preimage, never parameterising this one.
2. A client **reads `terms.version`** and refuses to sign a version it does not implement. It must not
   assume 5 forever.
3. Payment and transfer semantics (`Payment` transactions) are untouched by AppChain versions.
4. Nothing in this contract changes silently: a change to the preimage, the fee, or the owner rules is a
   new version, announced with a new node `terms`.
5. The node keeps accepting version 5 only; an unknown version fails closed.

## 10. Invariants and non-goals

**Invariants.** Exactly one owner per chain per block. Ownership moves only by an owner-signed
transaction. Anchors are append-only, sequenced by consensus. Registrations and anchors are consensus
state. Nothing is deletable.

**Not part of this contract** (and not implemented): delegated anchoring, per-chain metadata (v6),
unique names, node-side anchoring of any kind, any per-chain state storage, and any server-held owner
key. `POST /api/appchain/{id}/state` does not exist and never has.

## 11. What a client must do

1. `GET /api/appchain/ledger` → read `terms` (version, fees, activation height).
2. Build the transaction, sign the §4 preimage with the owner's Dilithium-5 key **on the client**.
3. `POST /api/appchain/tx` → `200` + hash means *accepted*, not confirmed.
4. Confirm by reading `/api/appchain/ledger/{id}/anchors` (or `anchor_count`) after a block lands.
5. Keep the key. The node has no way to recover it, and no way to act for you.

A worked end-to-end example — register, discover by chain id, anchor, hand over — is
`tests/appchain_e2e_lifecycle_test.rs`.

## 12. Machine-readable form

| Artefact | What it is |
|---|---|
| [`contract.schema.json`](./contract.schema.json) | JSON Schema (2020-12) for the request, the error envelope, and every read model. Strict where the node is strict (`additionalProperties: false` on the request and inside each operation), permissive on responses so a node may add fields without breaking a conforming client |
| [`vectors.json`](./vectors.json) | The three pinned digests and their inputs |
| [`validate_contract.py`](./validate_contract.py) | Offline validator: recomputes all three digests from §4 in Python (a second implementation of what the node does in Rust), validates the vectors, requests and captured responses against the schema, and asserts the schema rejects what the node rejects |

```
python3 docs/appchain/v5/validate_contract.py
```

Current result: 18/18 checks pass — three digests reproduced, five response shapes accepted, seven
malformed requests rejected.

**Version support.** This contract describes envelope **version 5**, and it is the only AppChain envelope
version the node accepts: `validate_envelope` refuses anything else (`ledger.rs:118-124`) and a non-5
version fails earlier still with `Unsupported transaction version!` (`lib.rs:2352-2362`). Versions 1–3 and
6 are **not implemented in this tree**; version 4 exists only for sovereign supply-correction transactions,
which are not AppChain traffic. A client must read `terms.version` rather than assume, and a future
envelope is a new document, not an additive change to this one.

**Publication.** Prepared, not published. Publishing means placing this directory where a third party can
fetch it (repository path or a documentation page) and announcing the version; that is a separate owner
decision (D-4 §8) and has not been taken.
