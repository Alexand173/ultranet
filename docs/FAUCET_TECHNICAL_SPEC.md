# UltraNet Faucet Technical Specification and Security Plan

**Status:** Draft for a mainnet capped beta
**Scope:** Dedicated online faucet signer and public claim service
**Network:** UltraNet L1 (`chain_id: 0`)
**Last reviewed:** 2026-08-31

## 1. Executive summary

The UltraNet faucet is an onboarding service that sends a small, fixed amount of `$ULTRA` to a user-supplied public wallet address. It is intended to let a new user create or open an UltraWallet, receive enough funds for a first transaction, and try the network without first purchasing tokens.

The first release is deliberately conservative:

- mainnet capped beta;
- one fixed claim amount of `1.000000 $ULTRA`;
- one eligible claim per canonical destination address in a rolling 24-hour window;
- additional IP/device/network and global-budget controls;
- a separate faucet wallet and a separate online Dilithium-5 signer;
- no use of sovereign/genesis keys;
- no use or exposure of `ULTRANET_ADMIN_TOKEN`;
- durable claim and payout state with crash recovery;
- an immediate operator kill switch;
- a claim is complete only after the node reports transaction confirmation.

The faucet is not a minting service. It submits an ordinary wallet-signed L1 transfer through the existing public transaction boundary. It must not add a mint endpoint, alter genesis supply, or bypass the node's transaction validation.

## 2. Current repository and protocol constraints

The following facts are taken from the current repository. They are implementation constraints for the faucet rather than assumptions to be re-created in the faucet service.

### 2.1 Existing transaction boundary

The node exposes `POST /api/transaction` in [`src/api.rs`](../src/api.rs). The route accepts a public, wallet-signed standard transfer. It is not protected by the administrator bearer token. The node constructs the standard-transfer proof and validates the assembled transaction before admitting it to the mempool.

Read-only routes relevant to the faucet are:

```text
GET /api/account/<address>
GET /api/balance/<address>
GET /api/transaction/estimate?recipient=<address>&amount=<base-units>
GET /api/transaction/<hash>
GET /api/validate
```

`GET /api/account/<address>` returns the account balance in base units, the six-decimal denomination, and the next available nonce. `GET /api/transaction/<hash>` returns `pending` or `confirmed` for a known transaction. The faucet service may expose its own `failed` state for a payout that was rejected or permanently abandoned; the node does not currently persist a separate failed transaction record.

### 2.2 Address and denomination

UltraNet addresses are exactly 64 lowercase hexadecimal characters. The protocol uses six decimal places:

```text
1 $ULTRA = 1,000,000 base units
1.000000 $ULTRA = 1,000,000 base units
```

All service arithmetic must use checked integer base-unit operations. The browser may display `$ULTRA`, but it must never submit a floating-point amount or choose the payout amount.

### 2.3 Fee and budget accounting

The current minimum transfer fee is:

```text
max(1, amount_base_units / 100)
```

For the initial claim policy:

```text
claim amount:  1,000,000 base units = 1.000000 $ULTRA
minimum fee:      10,000 base units = 0.010000 $ULTRA
source debit:  1,010,000 base units = 1.010000 $ULTRA
```

The faucet budget counts `amount + fee`, not only the amount received by the user.

The operator supplied the following faucet account snapshot during planning:

```text
address:             787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358
balance_base_units:  1,000,000,000
balance_ultra:       1,000.000000
reported decimals:   6
```

At the fixed claim policy, the theoretical balance-only maximum is `floor(1,000,000,000 / 1,010,000) = 990` claims. This is not an operating target. The service must retain a reserve, honor a much smaller global daily cap, and disable intake before the hot wallet is depleted.

### 2.4 Signing envelope

Standard transfers use:

```text
chain_id = 0
version  = 1
```

The node's version-1 signing preimage, implemented by `UltraBlockchain::create_transaction_message` in [`src/lib.rs`](../src/lib.rs), is the SHA3-256 digest of these fields in this exact order:

```text
sender UTF-8 bytes
|| recipient UTF-8 bytes
|| amount as little-endian u64
|| fee as little-endian u64
|| timestamp as little-endian u64
|| nullifier[32]
|| nonce as little-endian u64
|| gas_limit as little-endian u64
|| gas_price as little-endian u64
```

Version 1 intentionally does not include `chain_id` or `version` in this digest. A separate signer implementation must use a test vector generated against the Rust implementation. It must not silently invent a new preimage.

The signed request contains public material and transaction fields:

```json
{
  "sender": "<faucet address>",
  "sender_public_key": [/* Dilithium-5 public key bytes */],
  "recipient": "<user address>",
  "amount": 1000000,
  "fee": 10000,
  "nonce": 0,
  "timestamp": 0,
  "nullifier": [/* exactly 32 fresh bytes */],
  "gas_limit": 500000,
  "gas_price": 1,
  "signature": [/* Dilithium-5 signature bytes */],
  "chain_id": 0,
  "version": 1
}
```

The faucet must generate the nullifier from an OS CSPRNG for every newly signed transaction. It must never accept a nullifier, nonce, fee, signature, public key, or amount from the browser.

### 2.5 Validation behavior the worker must respect

The node validates, among other things:

- sender address equals the address derived from the submitted public key;
- Dilithium signature verifies against the exact envelope;
- standard-transfer recipient is a valid 64-character lowercase hexadecimal address;
- amount does not exceed the protocol transfer limit;
- source balance covers amount plus fee;
- timestamp is not more than 60 seconds in the future or one hour old;
- fee meets the minimum fee formula;
- nonce matches the next available nonce, allowing the nonce already reserved by the same pending transaction;
- nullifier is not already reserved or confirmed;
- the transaction is persisted as pending before successful admission is returned.

The faucet must serialize all payouts from its source address. `GET /api/account/<faucet-address>` reports the next nonce, including pending reservations. Multiple independent workers must not race this account.

### 2.6 Legacy script warning

[`send_ultra.py`](../send_ultra.py) is not a faucet implementation. It is a legacy funding script that:

- reads `sovereign_keys.json` and loads sovereign secret keys;
- hard-codes sovereign and faucet addresses;
- uses a manually edited static nonce;
- constructs a legacy transfer directly;
- writes a reusable `transfer.json` artifact.

It must not be copied into the faucet service, deployed with the faucet, or granted access to the sovereign key material. Funding the dedicated faucet address and operating the faucet are separate responsibilities.

## 3. Goals and non-goals

### Goals

1. Give a new user enough `$ULTRA` for a first wallet-signed transaction.
2. Make a claim understandable, observable, idempotent, and recoverable after a crash.
3. Limit financial loss if the public service, signer host, or anti-abuse layer is compromised.
4. Preserve the existing UltraNet transaction and signature contract.
5. Provide operators with a clear balance, budget, nonce, queue, and kill-switch view.
6. Measure onboarding quality beyond raw claim count.

### Non-goals

- minting new supply;
- changing genesis allocation or sovereign balances;
- use of sovereign 2-of-3 keys;
- use of validator keys or recovery phrases;
- exposing `ULTRANET_ADMIN_TOKEN` to the faucet;
- putting any faucet private key in browser JavaScript;
- allowing users to choose arbitrary payout amounts;
- promising instant transaction finality;
- treating faucet claims alone as evidence of durable adoption;
- using the validator's Sled database as the faucet's application database.

## 4. Proposed architecture

```text
+-----------------------+
| Browser /faucet page  |
| public address only   |
+-----------+-----------+
            | HTTPS; no cookies; claim idempotency key
            v
+-----------------------+
| TLS reverse proxy     |
| exact host/origin     |
+-----------+-----------+
            | loopback
            v
+-----------------------+       loopback       +-----------------------+
| ultranet-faucet       | --------------------> | UltraNet node         |
| low-privilege user    |  GET account/fee     | 127.0.0.1:8081        |
| claim ledger          |  POST signed tx      | public tx boundary    |
| single payout worker  |  GET tx status       | systemd service       |
| isolated signer       |                      +-----------+-----------+
+-----------+-----------+                                  |
            | durable state                                  | P2P
            v                                                v
+-----------------------+                            UltraNet network
| /var/lib/ultranet-   |
| faucet (separate)    |
+-----------------------+
```

### 4.1 Service boundaries

Run the faucet as a separate process and systemd service:

```text
service:       ultranet-faucet.service
user/group:    ultranet-faucet / ultranet-faucet
listener:      127.0.0.1:8090
state path:    /var/lib/ultranet-faucet
config:        /etc/ultranet-faucet/faucet.env
```

The service must not be able to read or write:

- `/var/lib/ultranet`;
- `/etc/ultranet/ultranet.env`;
- `sovereign_keys.json` or any validator key backup;
- the website source or build directory;
- the node's Sled trees;
- unrelated application secrets.

The node keeps its API loopback-only as described in [`deploy/README.md`](../deploy/README.md). The faucet reaches the node locally and uses only the normal signed transfer route and read-only account/status routes.

### 4.2 Authority model

The faucet signer has exactly one intended authority: spend the balance of the dedicated faucet account through standard L1 transfers. It has no authority to:

- mine blocks;
- prune state;
- create or anchor AppChains;
- submit governance actions;
- perform supply correction;
- mint through Move;
- change validator state;
- read or rotate sovereign keys.

`ULTRANET_ADMIN_TOKEN` is a broad node-operator credential. It must not be placed in the faucet environment, code, logs, HTTP headers, or backups. The current public transaction endpoint does not require it. If the node later introduces service authentication, use a narrowly scoped capability, mTLS identity, or dedicated service credential rather than granting the faucet the full administrator token.

## 5. Mainnet beta policy

### 5.1 Claim policy

Initial policy values should be configuration, not constants in browser code:

| Policy | Initial value | Notes |
| --- | ---: | --- |
| Claim amount | `1,000,000` base units | `1.000000 $ULTRA` |
| Fee floor | `10,000` base units | Reconfirm from node estimate |
| Expected source debit | `1,010,000` base units | Amount plus fee |
| Address cooldown | 24 rolling hours | Pending claims also consume eligibility |
| Global daily debit cap | 100 `$ULTRA` equivalent | Includes fees; initially permits at most 99 whole claims |
| Minimum wallet reserve | 200 `$ULTRA` equivalent | Disable intake below this threshold |
| Intake default | disabled until canary | Explicit operator enable required |
| Confirmation timeout | bounded, configurable | No infinite worker retry loop |

The global cap is measured in base units over a UTC window. A daily cap of `100,000,000` base units and a per-claim debit of `1,010,000` permits 99 claims (`99,990,000` base units) before the next claim would exceed the cap. The implementation must reserve the exact debit atomically rather than rely on rounded display values.

The amount must remain fixed for the first beta. If variable claims are introduced later, the server must select the amount from an allowlisted policy and budget the exact amount plus fee. A browser-provided `amount` is never authoritative.

### 5.2 Eligibility semantics

A destination address is eligible only if all of the following are true:

- it is canonical and valid;
- it is not the faucet address or a protected operator address;
- it has no pending or confirmed faucet claim inside the cooldown window;
- the request passes anti-bot verification;
- the address/IP/network/device controls allow the request;
- the daily budget can reserve the full source debit;
- the faucet is enabled and the signer/node health gates are green.

A claim that has been durably queued or submitted consumes the address cooldown even if the browser loses its response. A payout that is explicitly rejected before the node accepts it may be retried according to policy, but the retry must use a new claim record and never reuse a consumed idempotency key for a different request.

### 5.3 Adoption metrics

Track the funnel rather than optimizing for free-token volume:

```text
claim accepted
  -> payout confirmed
  -> first outgoing user transaction
  -> returning user after 24 hours
  -> AppChain/developer activity
```

The public product should communicate the next action after a confirmed claim: open Send Ultra, send a small transaction, inspect the explorer, or follow an AppChain quickstart.

## 6. Public API specification

The public faucet API is a separate service API. It is not added to the validator node until the service contract and threat model are approved.

### 6.1 `POST /api/faucet/claims`

Request:

```http
POST /api/faucet/claims HTTP/1.1
Host: faucet.ultranetwork.cc
Content-Type: application/json
Idempotency-Key: 2f0a1bb3-3d8b-4cfb-90f2-4c4c2b5e6d0e
```

```json
{
  "address": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "captcha_token": "<short-lived anti-bot provider token>"
}
```

The request schema must reject unknown fields. The server owns all transaction fields. In particular, the request must not contain `amount`, `fee`, `nonce`, `timestamp`, `nullifier`, `gas_limit`, `gas_price`, `sender_public_key`, `signature`, `private_key`, `seed`, `recovery_phrase`, or `ULTRANET_ADMIN_TOKEN`.

The idempotency key must be an opaque, high-entropy value with a bounded maximum length. Store a keyed digest and a request fingerprint; do not log or persist the raw key unnecessarily.

Successful response:

```http
HTTP/1.1 202 Accepted
Retry-After: 5
Content-Type: application/json
```

```json
{
  "success": true,
  "message": "Faucet claim queued",
  "data": {
    "claim_id": "01JFAUCET9M4H5F4XG7N0Q1C2A",
    "status": "queued",
    "address": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "amount_base_units": 1000000,
    "amount_ultra": "1.000000",
    "decimals": 6,
    "retry_after_seconds": 86400
  }
}
```

The service may return the canonical address because the user supplied it, but it must not return private service diagnostics, internal queue identifiers, exact budget remaining, source nonce, IP risk scores, or signed transaction bytes.

Response status policy:

| Status | Meaning |
| ---: | --- |
| `202` | Claim durably accepted/queued |
| `400` | Invalid JSON, unknown fields, or invalid address |
| `409` | Existing active claim, or idempotency key bound to another request |
| `422` | Anti-bot proof or policy validation failed |
| `429` | Address, IP/network, or global rate limit |
| `503` | Faucet disabled, signer unavailable, node unavailable, or budget exhausted |

`429` responses should include a bounded `Retry-After` value. Do not reveal whether an address is on a hidden abuse blocklist.

### 6.2 `GET /api/faucet/claims/{claim_id}`

Claim IDs must be unguessable. A random 128-bit or stronger identifier is required. The endpoint returns public lifecycle data only:

```json
{
  "success": true,
  "data": {
    "claim_id": "01JFAUCET9M4H5F4XG7N0Q1C2A",
    "status": "queued",
    "amount_base_units": 1000000,
    "amount_ultra": "1.000000",
    "decimals": 6,
    "transaction_hash": null,
    "submitted_at": null,
    "confirmed_at": null,
    "failure_code": null
  }
}
```

Allowed status values:

```text
queued -> submitting -> pending -> confirmed
                         \-> failed
```

A `failed` response must contain a stable, non-secret failure code such as `NODE_REJECTED`, `NODE_UNAVAILABLE`, `BUDGET_DISABLED`, or `SIGNER_UNAVAILABLE`. It must not echo raw node errors, headers, secrets, key paths, or signed payloads.

### 6.3 `GET /api/faucet/status`

Public status may expose:

```json
{
  "success": true,
  "data": {
    "enabled": true,
    "availability": "available",
    "claim_amount_base_units": 1000000,
    "claim_amount_ultra": "1.000000",
    "decimals": 6,
    "cooldown_seconds": 86400
  }
}
```

Do not expose the exact hot-wallet balance, source nonce, exact daily budget remaining, queue internals, or anti-abuse scores publicly. An operator-only status surface may expose those values behind local access or separate operator authentication.

### 6.4 Operator-only controls

The following functions belong to a private control plane, not the public API:

```text
GET  /internal/health
GET  /internal/metrics
GET  /internal/status
POST /internal/enable
POST /internal/disable
POST /internal/reconcile
```

The kill switch must stop new claim intake immediately while preserving claim-status reads and safe reconciliation. Control-plane access must not be implemented by placing the node's administrator bearer token in browser code.

## 7. Claim and payout state machine

### 7.1 State transitions

```text
                         +------------------+
                         |                  |
                         v                  |
                     +--------+            |
        accepted --> | queued | -----------+
                     +---+----+             |
                         |                  |
                         v                  |
                   +-----------+           |
                   | submitting|-----------+
                   +-----+-----+           |
                         |                  |
             accepted    |                  | explicit permanent rejection
             by node     v                  v
                   +-----------+       +---------+
                   |  pending  |       | failed  |
                   +-----+-----+       +---------+
                         |
                         v
                   +-----------+
                   | confirmed |
                   +-----------+
```

Required invariants:

1. No claim reaches `queued` until address, anti-abuse, eligibility, and budget reservation succeed.
2. A claim has at most one reserved source debit.
3. A claim has at most one signed transaction envelope.
4. Every retry after transport uncertainty reuses the exact signed envelope.
5. A second claim for the same address cannot be admitted during the cooldown window, even after a worker restart.
6. A claim cannot be marked `confirmed` without a node transaction hash and a confirmed node response.
7. A `failed` claim never silently releases a debit that the node may have accepted; reconciliation must resolve uncertain submissions before releasing funds.

### 7.2 Request flow

1. Parse JSON with unknown-field rejection and a strict body-size limit.
2. Canonicalize the address by trimming permitted surrounding input and requiring the final value to be exactly lowercase hexadecimal. Do not accept aliases or Unicode lookalikes.
3. Validate the anti-bot proof server-side. Never send the provider secret to the browser.
4. Derive an abuse-control key from the address and short-lived network/device signals. Use a keyed digest rather than storing raw identifiers where possible.
5. Check the idempotency key. If it already maps to the same request fingerprint, return the original claim. If it maps to a different fingerprint, return `409`.
6. Atomically check the address cooldown and reserve the exact amount-plus-fee debit in the current UTC budget window.
7. Insert the claim and payout record in the same durable transaction as the reservation.
8. Enqueue the claim for the single payout worker and return `202`.

### 7.3 Worker flow

1. Recover `queued`, `submitting`, and `pending` records on startup.
2. Verify the service is enabled and all health/budget gates remain green.
3. Query `GET /api/account/<faucet-address>` and validate the returned address and `decimals`.
4. Query `GET /api/transaction/estimate` for the fixed recipient and amount, or apply the validated protocol fee floor. Reject a response that conflicts with the configured denomination or fixed amount.
5. Read the returned next nonce. Because all payouts are serialized, this nonce belongs to the next worker transaction.
6. Generate a fresh 32-byte nullifier using an OS CSPRNG.
7. Construct the complete version-1 envelope with server time, fixed amount, validated fee, returned nonce, `gas_limit=500_000`, `gas_price=1`, `chain_id=0`, and `version=1`.
8. Sign the exact canonical preimage with the dedicated faucet key. Verify locally that the public key derives the configured faucet address and that the signature verifies before sending.
9. Persist the public signed envelope, transaction hash, nonce, and nullifier before submitting. Treat this record as replay-sensitive even though it contains no private key.
10. Submit the exact envelope to `POST /api/transaction`.
11. On a success response, store the node hash and move to `pending` or `confirmed` according to the returned status.
12. On a timeout or connection loss, do not generate a new nullifier or nonce. Retry the same envelope under a bounded policy and poll the stored hash.
13. Poll `GET /api/transaction/<hash>` with exponential backoff and a maximum age. A node response of `confirmed` completes the claim.
14. If the node explicitly rejects the transaction, classify the stable error, reconcile the source account/nonce and transaction hash, and then either fail the claim or create a controlled retry according to policy.
15. After every transition, persist the state and emit a redacted audit event.

### 7.4 Crash recovery

The service must be safe to stop at each point in the worker flow:

| Crash point | Recovery behavior |
| --- | --- |
| Before budget reservation | No claim exists; no funds reserved |
| After reservation, before claim insert | Database transaction rolls back atomically |
| After claim insert, before queue acknowledgement | Startup scans `queued` records |
| Before signing | Claim remains `queued`/`submitting`; no transaction exists |
| After signing, before submission | Reuse the persisted exact envelope |
| During submission | Query by stored hash and retry exact envelope only |
| After node acceptance, before response | Reconcile by hash/nullifier; never sign a second transfer |
| After `pending` persistence | Poll until confirmation or bounded manual review |
| After confirmation, before response | Status endpoint returns `confirmed` |

## 8. Durable data model

Use a dedicated database owned by the faucet service. SQLite is appropriate for a single-VPS beta; Postgres is appropriate if the service will be active/active or moved independently. Do not share the validator's Sled database.

The schema must provide durable unique constraints and transactional updates.

### 8.1 `claims`

```text
claim_id                  primary key
address                   canonical destination address
address_digest            keyed digest for lookup/abuse policy
created_at                UTC timestamp
cooldown_until            UTC timestamp
status                    queued | submitting | pending | confirmed | failed
amount_base_units         unsigned integer
fee_base_units            unsigned integer
source_debit_base_units   unsigned integer
idempotency_fingerprint   keyed request fingerprint
failure_code              nullable stable code
submitted_at              nullable timestamp
confirmed_at              nullable timestamp
updated_at                timestamp
```

Required indexes/constraints:

```text
unique active cooldown claim for address
non-negative amount, fee, and source debit
source_debit = amount + fee with checked arithmetic
```

The address should be retained only as long as needed for claim status and the documented operational retention period. Abuse identifiers should be keyed digests with shorter retention.

### 8.2 `idempotency_keys`

```text
key_digest                primary key
request_fingerprint       keyed hash of canonical request
claim_id                  foreign key
created_at                timestamp
expires_at                timestamp
```

A key bound to a different request fingerprint must never be reassigned.

### 8.3 `payouts`

```text
claim_id                  primary key / foreign key
transaction_hash          nullable until signed
nullifier                 nullable until signed
nonce                     nullable until signed
signed_envelope           encrypted/restricted public transaction record
attempt_count             integer
last_error_code           nullable stable code
last_attempt_at           nullable timestamp
submitted_at              nullable timestamp
confirmed_at              nullable timestamp
```

Although the envelope contains public data, it is retained as confidential operational material because it enables an exact replay attempt and can disclose payout timing and internal transaction details. Store it with restrictive permissions and do not include it in ordinary logs.

### 8.4 `budget_windows`

```text
window_start_utc          primary key
window_end_utc            timestamp
reserved_base_units       unsigned integer
confirmed_base_units      unsigned integer
claim_count               unsigned integer
policy_version            string
```

A claim reservation must atomically verify that `reserved + source_debit <= global_cap`. Do not release a reservation merely because an HTTP response was lost.

### 8.5 `service_state`

```text
enabled                   boolean
kill_switch_reason        nullable string
signer_key_id             public identifier only
faucet_address            canonical public address
last_observed_nonce       nullable unsigned integer
last_node_health_at       nullable timestamp
schema_version            integer
```

Never store a private key in this table. The signer key is loaded through a dedicated secret mechanism and is not exportable through the API.

## 9. Security plan

### 9.1 Key custody

- Generate the faucet key in a controlled offline or hardened provisioning workflow.
- Verify the derived public address exactly equals `787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358` before enabling claims.
- Store only this dedicated faucet key. Do not copy sovereign keys, validator keys, recovery phrases, or `ULTRANET_ADMIN_TOKEN` to the faucet host.
- Prefer systemd credentials, a local secret manager, or an HSM-backed signer over a plain environment variable.
- If a file is unavoidable, use a root-owned path readable only by `root` and `ultranet-faucet`, `umask 077`, restrictive permissions, and encrypted backups.
- Keep private key bytes in memory only for the signing operation and zeroize buffers on completion and shutdown.
- Do not expose signing as a generic RPC. The signer accepts only a validated internal payout envelope and returns a signature; it cannot choose arbitrary sender, amount, or recipient.
- Maintain a rotation runbook. Rotation creates a new faucet address, funds it explicitly, verifies it, drains or disables the old account, and updates the policy. Never overwrite the active key in place without a reconciliation plan.

### 9.2 Process and filesystem isolation

Use the existing systemd hardening pattern from [`deploy/ultranet.service`](../deploy/ultranet.service), strengthened for the faucet:

```ini
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
CapabilityBoundingSet=
ReadWritePaths=/var/lib/ultranet-faucet
```

Also apply bounded memory, CPU, open files, request body size, connection count, and queue length. Restrict network families to the required loopback/HTTPS families. The service must not run as root and must not have a shell or interactive login.

### 9.3 Network and reverse proxy

- Bind the faucet service to `127.0.0.1`.
- Terminate TLS at the existing Caddy/Nginx layer.
- Use `https://faucet.ultranetwork.cc` as the canonical faucet origin; the current repository has no faucet UI, so the deployed route is API-only until a same-origin UI is approved.
- Allow only `GET /api/faucet/status`, `GET /api/faucet/claims/<claim-id>`, and `POST /api/faucet/claims`; return `404` for `/internal/*`, unsupported methods, and unknown paths before proxying.
- Keep TCP `8081` private; only the reverse proxy and local faucet process need node API access.
- Prefer same-origin UI/API so no CORS headers or browser credentials are required. If a cross-origin UI is approved, allow exactly one origin and explicit preflight methods/headers; never use wildcard CORS or credentials.
- Strip client-supplied authorization, cookie, CSRF, Cloudflare, and forwarding headers. Forward only the reverse proxy's observed peer address until origin access is restricted to the upstream WAF and a trusted-proxy configuration is reviewed.
- Apply proxy body-size, request-rate, connection, and timeout limits before traffic reaches the service.

### 9.4 Abuse resistance

Address cooldown is only one control. The initial beta should combine:

1. Cloudflare Turnstile or equivalent short-lived anti-bot proof.
2. One active/successful claim per canonical address per rolling 24 hours.
3. Per-IP and per-subnet/ASN quotas with conservative defaults and an exception process for shared networks.
4. A global daily debit cap, source-balance reserve, and queue cap.
5. A per-device/session signal with privacy-limited retention; never treat it as a sole identity proof.
6. A circuit breaker for abnormal claim velocity, duplicate idempotency conflicts, nonce drift, node errors, or unexpected outgoing transfers.
7. Manual operator review for repeated abnormal patterns or any request to raise limits.

Do not depend on address uniqueness alone. An attacker can generate unlimited addresses. Do not depend on IP alone. Corporate networks, mobile carriers, VPNs, and privacy relays make IP-only controls both bypassable and unfair.

### 9.5 Replay and idempotency

- The browser receives only a claim ID and status information.
- The browser cannot supply transaction fields.
- The service creates exactly one signed envelope per payout attempt.
- The nullifier is fresh for a new envelope and never client-controlled.
- A lost response causes an exact retry/poll, not a second signature.
- A node `409` or nullifier conflict must be reconciled against the original claim and transaction hash. It must not be silently treated as a new payout.
- Idempotency keys are bound to canonical request fingerprints and expire only after a safe retention period.
- State transitions and budget reservations are durable and transactional; in-memory locks are supplementary only.

### 9.6 Node/admin boundary

The faucet must never send:

```http
Authorization: Bearer <ULTRANET_ADMIN_TOKEN>
```

It must not call administrative routes such as `/api/mine`, `/api/state/prune`, `/api/appchain/create`, `/api/appchain/{id}/anchor`, or governance endpoints. A compromised faucet must be limited to its separate wallet balance and normal standard transfers.

### 9.7 Input and output safety

- Reject unknown JSON fields.
- Limit request and response sizes.
- Validate content type and JSON depth.
- Enforce exact lowercase hexadecimal address syntax.
- Use checked arithmetic for amount, fee, budget, and nonce values.
- Use server time, not a client timestamp.
- Escape all address/status values rendered into HTML.
- Do not reflect CAPTCHA tokens, cookies, authorization headers, or raw node errors.
- Return stable public error messages and keep detailed diagnostics in redacted private logs.

### 9.8 Privacy

The faucet does not need a user name, email, private key, recovery phrase, or wallet session. It should collect only:

- destination address;
- claim and payout lifecycle data;
- short-lived anti-abuse signals;
- operational timestamps and stable failure codes.

Use keyed/truncated digests for IP/device signals where possible. Document retention and deletion. Do not sell or publicly expose the claim-address dataset.

### 9.9 Logging rules

Allowed structured fields:

```text
claim_id
redacted address digest
status transition
transaction hash
stable failure code
coarse latency
queue age
budget window
```

Never log:

```text
private keys
recovery phrases
ULTRANET_ADMIN_TOKEN
CAPTCHA provider secrets or tokens
session cookies
Authorization headers
raw signed envelopes
full IP/device signals
secret file contents
```

Use access-controlled logs, bounded retention, and alerting on log pipeline failure. A leaked transaction hash is not a private key, but it still should not be used as a substitute for claim authorization.

## 10. Monitoring and incident response

### 10.1 Operator metrics

Required private metrics:

- faucet balance in base units;
- source debit reserve and confirmed debit;
- current UTC budget reservation and remaining allowance;
- claims by status and stable rejection code;
- queue depth and age of oldest claim;
- payout attempt count and confirmation latency;
- node reachability and API latency;
- chain validation result;
- observed faucet nonce and nonce rejection count;
- signer availability and key identifier;
- CAPTCHA and rate-limit rejection rates;
- unexpected outgoing transactions from the faucet account.

### 10.2 Alerts

Alert immediately on:

- balance below the configured reserve;
- any outgoing transaction not created by a known claim;
- nonce changes outside the worker's serialized sequence;
- repeated signature or address-derivation failures;
- node chain validation failure;
- queue age or size threshold exceeded;
- repeated node rejection/timeouts;
- daily budget reservation approaching the cap;
- signer process restart or secret-load failure;
- unusual claim velocity or address/IP collision patterns.

### 10.3 Kill switch

The kill switch must:

1. atomically set intake to disabled;
2. reject new claims with `503`;
3. leave claim-status and operator reconciliation available;
4. stop signing new envelopes;
5. allow safe inspection of already submitted hashes;
6. preserve durable audit state;
7. be usable without editing production source or copying secrets into a browser.

### 10.4 Compromise response

If the signer host or key is suspected to be compromised:

1. Disable public intake immediately.
2. Stop the payout worker after preserving its current state.
3. Inspect all outgoing transactions from the faucet address and compare them with claim records.
4. Reconcile pending hashes and nonce state against the node.
5. Revoke/quarantine the compromised key and rotate to a new faucet address.
6. Preserve logs and database records for investigation without exposing them publicly.
7. Restore from a known-good service image and secret source.
8. Re-run key/address, balance, nonce, budget, and kill-switch checks.
9. Re-enable only under a new canary cap.

The faucet hot-wallet balance is an explicit loss limit. It must never be funded with sovereign, validator, or treasury reserves needed for protocol operation.

## 11. Deployment specification

### 11.1 Configuration

Proposed `/etc/ultranet-faucet/faucet.env` values:

```dotenv
FAUCET_BIND=127.0.0.1:8090
FAUCET_NODE_API_BASE_URL=http://127.0.0.1:8081
FAUCET_ADDRESS=787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358
FAUCET_CLAIM_AMOUNT_BASE_UNITS=1000000
FAUCET_DAILY_DEBIT_CAP_BASE_UNITS=100000000
FAUCET_MIN_BALANCE_RESERVE_BASE_UNITS=200000000
FAUCET_ADDRESS_COOLDOWN_SECONDS=86400
FAUCET_MAX_QUEUE_LENGTH=100
FAUCET_ENABLED=false
FAUCET_CAPTCHA_PROVIDER=turnstile
FAUCET_STATE_PATH=/var/lib/ultranet-faucet
```

The private signer key must not be placed in this general configuration file. Prefer a systemd credential such as `LoadCredentialEncrypted=` or an equivalent secret-manager reference. If a key path is required, the path itself may be configured but the key file must be separately permissioned and excluded from ordinary backups/logs.

The service must validate at startup:

- faucet address format;
- derived signer address equals `FAUCET_ADDRESS`;
- amount and caps are valid base-unit integers;
- daily cap is greater than one claim debit;
- reserve is non-negative and less than the configured funding plan;
- node API URL is an allowed loopback/explicit HTTPS origin;
- CAPTCHA configuration is complete;
- state directory ownership and permissions are correct.

Production should fail closed if the signer key is missing, the derived address mismatches, or the faucet is accidentally enabled without a reserve and global cap.

### 11.2 Reverse proxy

Add only a dedicated proxy route after the faucet has passed its local integration checks. The public route must forward to `127.0.0.1:8090`, apply TLS, request limits, and security headers. The node remains behind its existing private API boundary.

The canonical faucet origin is `https://faucet.ultranetwork.cc`; the Turnstile widget and Siteverify response must use the exact hostname `faucet.ultranetwork.cc` and action `faucet_claim`. Prefer a same-origin faucet UI so no CORS headers or browser credentials are needed. If a cross-origin UI is approved, allow exactly one explicit origin and explicit preflight methods/headers; never use wildcard CORS or credentials.

For the currently deployed Caddy 2.6.2, configure Cloudflare `trusted_proxies` inside each `reverse_proxy` handler with the reviewed IPv4/IPv6 CIDR list. Do not use the newer global `servers` syntax or the newer `static` module token on this version. Strip client-supplied forwarding and operator/session headers before forwarding. Trust `CF-Connecting-IP` only after the VPS firewall or tunnel restricts web ingress to Cloudflare.

### 11.3 Backups

Back up the faucet database and policy state separately from the validator database. Encrypt backups, restrict access, and verify restore procedures. Do not place the faucet private key beside ordinary database archives. A restored database must preserve idempotency keys, cooldowns, budget reservations, payout envelopes, and kill-switch state before intake is enabled.

## 12. Verification and rollout gates

### 12.1 Unit tests

Test at minimum:

- address canonicalization and rejection of uppercase/Unicode/incorrect-length values;
- six-decimal conversion and fixed-format rendering;
- checked amount-plus-fee arithmetic;
- fee calculation and node-estimate mismatch handling;
- 24-hour cooldown boundaries;
- daily budget reservation and cap boundaries;
- duplicate idempotency key with same fingerprint;
- duplicate idempotency key with different fingerprint;
- duplicate active claim for an address;
- unknown JSON field rejection;
- generated nullifier length and non-reuse;
- signer address derivation;
- exact version-1 signing vectors;
- state transition invariants;
- redaction of secrets and raw node errors.

### 12.2 Node integration tests

Against a controlled node fixture, test:

- valid standard transfer admission;
- invalid faucet signature;
- sender/public-key mismatch;
- invalid recipient address;
- stale and future timestamps;
- insufficient faucet balance;
- incorrect fee;
- stale and competing nonce;
- duplicate nullifier;
- timeout after node acceptance;
- exact signed-envelope retry returning the same transaction identity;
- pending-to-confirmed polling;
- node restart with pending payout recovery;
- chain validation failure disabling intake.

### 12.3 Security tests

Prove that the faucet process cannot:

- read `/etc/ultranet/ultranet.env`;
- read sovereign key files;
- read validator Sled state;
- call administrative node routes with a credential it should not possess;
- bind a public listener other than the reverse-proxy target;
- write outside its dedicated state directory;
- return secrets through status, error, or metrics endpoints.

Run dependency and container/systemd hardening checks before mainnet exposure.

### 12.4 Staged rollout

1. Implement and test the service with intake disabled.
2. Run against a local or isolated node with a test faucet key.
3. Verify cross-language signing vectors and crash recovery.
4. Deploy the production service with no public route and perform read-only balance/nonce/health checks.
5. Verify the configured signer derives the dedicated faucet address and the node reports six decimals.
6. Enable one canary claim under operator observation.
7. Enable a small daily cap and inspect every payout/reconciliation result.
8. Increase the cap only after no duplicate debit, nonce drift, unexpected transfer, or unresolved pending claim is observed.
9. Review first-transaction and return-user metrics before changing the claim amount.

The default production state is disabled until the dedicated key, durable state, backup, monitoring, reverse proxy, kill switch, and incident runbook are all verified.

## 13. Definition of Done

- [ ] The public API, request validation, response shapes, status machine, durable data model, accounting, and node transaction envelope are specified.
- [ ] Key custody, service isolation, admin-token separation, anti-abuse policy, replay/idempotency behavior, privacy, logging, monitoring, kill switch, and incident response are specified.
- [ ] The plan records the supplied faucet balance as `1,000,000,000` base units (`1,000.000000 $ULTRA`) and correctly budgets amount plus fee.
- [ ] Current repository facts are linked to `src/api.rs`, `src/lib.rs`, `src/auth.rs`, `send_ultra.py`, and `deploy/` conventions.
- [ ] No faucet endpoint, signer, production configuration, or funds are changed as part of drafting this document.
