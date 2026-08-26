# UltraWallet Integration Contract

**Contract version:** `1`  
**Validator proposal signing-envelope version:** `2`  
**Validator approval signing-envelope version:** `3`  
**Protocol:** UltraNet v7.1 Sovereign
**Audience:** Browser wallet implementers and UltraNet dApp integrators
**Creator and copyright holder:** Vladan Jotov  
**Document license:** ISC License — see [`LICENSE`](./LICENSE)

This document defines the browser-provider boundary used by the validator onboarding portal. UltraWallet owns all private-key operations; the website only requests a local signature and forwards the resulting public transaction fields to an UltraNet node.

## 1. Provider discovery

A wallet injects a provider into the page before the dApp submits a validator proposal:

```ts
window.ultraWallet?: UltraWalletProvider;
```

The canonical TypeScript definitions live in [`website/src/lib/ultra-wallet.ts`](./website/src/lib/ultra-wallet.ts). The global `Window` declaration is in [`website/src/types/ultra-wallet.d.ts`](./website/src/types/ultra-wallet.d.ts).

The provider must be available in the same browsing context as the dApp. The dApp must treat a missing provider as a normal not-connected state and must never construct an unsigned proposal as a fallback.

## 2. Provider interface

```ts
interface UltraWalletProvider {
  request(request: {
    method: "ultranet_signValidatorProposal";
    params: {
      metadata: string;
      proposalPublicKey: string;
      version: 2;
    };
  }): Promise<unknown>;
}
```

`request` returns `unknown` intentionally. The dApp validates the returned object at runtime before it can reach the node. A wallet may reject the request with an error shaped like:

```ts
interface UltraWalletError {
  code?: string | number;
  message: string;
  data?: unknown;
}
```

Wallets should use stable error codes where possible, but the dApp must always provide a useful fallback from `message`.

## 3. Signing request

The dApp sends one request for each validator application:

```ts
const signedProposal = await window.ultraWallet.request({
  method: "ultranet_signValidatorProposal",
  params: {
    metadata: "Genesis-Alpha-01",
    proposalPublicKey: "0x6c6dd0c8...",
    version: 2,
  },
});
```

### Request semantics

| Field | Type | Requirement |
|---|---|---|
| `method` | literal string | Must equal `ultranet_signValidatorProposal`. |
| `params.metadata` | string | Human-readable validator alias. The dApp trims it before submission. |
| `params.proposalPublicKey` | string | Applicant's Dilithium public key representation supplied by the validator. |
| `params.version` | literal `2` | Requests the payload-bound signing envelope. Other versions are rejected for validator proposals. |

The wallet should display the alias and proposal public key to the user before approval. The wallet must sign locally and must not expose a private key, seed phrase, or secret key through the provider response, browser storage, logs, or network requests.

## 4. Signed response

A successful request resolves to this JSON-compatible object:

```ts
interface SignedValidatorProposal {
  sender: string;
  sender_public_key: number[];
  proposal_public_key: number[];
  nonce: number;
  timestamp: number;
  nullifier: number[];
  signature: number[];
  version: 2;
}
```

### Field rules

| Field | Wire format | Requirement |
|---|---|---|
| `sender` | non-empty string | Address derived from `sender_public_key`. The node rejects identity mismatches. |
| `sender_public_key` | JSON byte array | Every value is an integer from `0` through `255`; must contain the wallet's Dilithium public key bytes. |
| `proposal_public_key` | JSON byte array | Every value is an integer from `0` through `255`; must contain the applicant's Dilithium public key bytes. |
| `nonce` | JSON number | Non-negative safe integer. The node checks the account nonce. |
| `timestamp` | JSON number | Non-negative Unix timestamp in seconds and a JavaScript safe integer. |
| `nullifier` | JSON byte array | Exactly 32 bytes. The node rejects any other length. |
| `signature` | JSON byte array | Every value is an integer from `0` through `255`; must contain a valid Dilithium signature. |
| `version` | literal `2` | Must match the payload-bound signing envelope requested by the dApp. |

Use ordinary JSON arrays for byte fields. Do not return `Uint8Array`, base64 strings, hex strings, or objects with numeric keys in the provider response. The current Actix endpoint deserializes these fields directly into Rust `Vec<u8>` values.

The dApp performs runtime validation with `isSignedValidatorProposal` before making the network request. Wallets should still validate their own output before resolving the provider promise.

## 5. Node submission contract

After validating the wallet response, the dApp adds the request metadata and submits the following body:

```json
{
  "sender": "<address>",
  "sender_public_key": [1, 2, 3],
  "proposal_public_key": [4, 5, 6],
  "metadata": "Genesis-Alpha-01",
  "nonce": 0,
  "timestamp": 1785183488,
  "nullifier": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  "signature": [7, 8, 9],
  "version": 2
}
```

```text
POST <API_BASE_URL>/api/governance/propose
Content-Type: application/json
```

`API_BASE_URL` defaults to `http://localhost:8081` and can be configured with `NEXT_PUBLIC_API_BASE_URL`. In production, point it at the HTTPS reverse-proxy origin (for example, `https://api.example.com`) rather than the node's private loopback address.

The node-admin bearer token is intentionally not part of this wallet contract. Validator proposals and approvals continue to rely on their existing Dilithium signing envelopes; the administrator token protects operational routes such as mining, pruning, and AppChain administration and must never be placed in browser code.

The corresponding Rust request type is `ValidatorProposalRequest` in [`src/api.rs`](./src/api.rs). The node creates a `ValidatorJoinProposal` transaction with:

- `recipient = "0x0"` (governance address)
- `amount = 0`
- `fee = 0`
- `gas_limit = 1_000_000`
- `gas_price = 1`
- `proof_type = Ownership`
- `chain_id = 0` (L1)
- `version = 2` (payload-bound signing envelope)

### Success response

```json
{
  "success": true,
  "message": "Validator proposal submitted!",
  "data": null
}
```

The proposal is placed in the node's pending governance queue. It becomes an active validator only after the required 2-of-3 Sovereign approval flow.

### 5.1 Durable pending-queue behavior

The node persists the accepted proposal in the existing Sled database under the `pending_proposals` tree. When the node restarts with the same database path, the pending queue is hydrated before the governance API serves requests. Wallets and operator clients can call:

```text
GET <API_BASE_URL>/api/governance/proposals
```

The response keeps the existing `hash`, `public_key`, `metadata`, `proposer`, and `timestamp` fields. A successful version-3 approval persists the approved validator in the `validators` tree before removing the proposal from the durable and in-memory queues. The active `ValidatorInfo` record, including weight, activity, rewards, epoch, and slash count, is restored when the node restarts. If governance records cannot be decoded during startup, the node fails closed rather than silently accepting incomplete state.

This contract guarantees durability for pending applications and validator activation. The operator-facing approval journal is available separately at `GET <API_BASE_URL>/api/governance/approvals?limit=50`; subsequent pages use the returned `pagination.next_cursor`. `limit` is bounded to 100, offset pagination is rejected, and the response includes total-count and continuation metadata. It is not part of the browser wallet provider contract and does not change the proposal request/response schema.

### Failure response

Validation failures return a non-2xx response with the shared API shape:

```json
{
  "success": false,
  "message": "<reason>",
  "data": null
}
```

Common causes include an invalid nullifier length, an address/public-key mismatch, an invalid Dilithium signature, an incorrect nonce, an unsupported transaction version, or a node-side transaction rejection.

## 6. Security and lifecycle requirements

1. **Local signing only.** Private keys never leave UltraWallet.
2. **No unsigned fallback.** If `window.ultraWallet` is missing, the dApp must stop before `POST /api/governance/propose`.
3. **User confirmation.** The wallet should show the metadata, proposal public key, sender address, and node origin before signing.
4. **Fresh replay fields.** The wallet must generate a fresh 32-byte nullifier and use a current timestamp for each request. It must not silently reuse a prior signature response.
5. **Canonical byte encoding.** Every byte array must be encoded as a JSON array of unsigned byte values.
6. **Origin awareness.** Wallet implementations should restrict signing requests to an allowed UltraNet dApp origin in production.
7. **Provider errors.** Rejections must not be retried automatically with a different key or altered proposal fields.

## 7. Versioned signing envelopes
Validator proposals require signing-envelope version `2`. Validator approvals require signing-envelope version `3`.

### 7.1 Version 2: validator proposals
Version 2 preserves the legacy transaction fields and appends a domain-separated payload section to the Dilithium preimage:

```text
SHA3-256(
  legacy transaction fields ||
  "UltraNet/transaction-signing-envelope/v2" ||
  version.to_le_bytes() ||
  chain_id.to_le_bytes() ||
  proposal_public_key.length.to_le_bytes() ||
  proposal_public_key ||
  metadata.length.to_le_bytes() ||
  metadata.utf8_bytes
)
```

The length prefixes are unsigned little-endian `u64` values. The envelope binds both `proposal_public_key` and `metadata` to the signature, preventing either field from being changed after wallet approval.

### 7.2 Version 3: validator approvals
Version 3 is reserved for `ValidatorApproval` transactions and binds the exact pending proposal hash to every sovereign signature:

```text
SHA3-256(
  legacy transaction fields ||
  "UltraNet/approval-signing-envelope/v3" ||
  version.to_le_bytes() ||
  chain_id.to_le_bytes() ||
  "ValidatorApproval" ||
  proposal_hash
)
```

`proposal_hash` is the 32-byte hash returned by `GET /api/governance/proposals`. An approval signature cannot be replayed against a different proposal hash without failing sovereign signature verification. This is a sovereign operator transaction, not a browser `UltraWalletProvider` method: the injected browser wallet contract covers applicant proposal signing, while the 2-of-3 approval requires the protected sovereign owner signing clients. The repository includes the offline `ultranet-approve` workflow in [`OFFLINE_APPROVAL_SIGNING.md`](./OFFLINE_APPROVAL_SIGNING.md). The approval request is submitted to:

```text
POST <API_BASE_URL>/api/governance/approve
```

with `proposal_hash`, `timestamp`, `nonce`, `nullifier`, concatenated Dilithium-5 signatures, and `version: 3`.

Legacy transaction version `1` remains supported for existing transfers, Move transactions, and previously persisted blocks. Version `2` is accepted only for `ValidatorJoinProposal` transactions, and version `3` only for `ValidatorApproval` transactions. The node rejects a validator proposal or approval that supplies a different version.

The backend implementation is in `UltraBlockchain::create_transaction_message`, `propose_validator`, and `approve_validator`. Regression coverage proves that changing proposal metadata, proposal public keys, or approval proposal hashes invalidates the signature.

## 8. Generic wallet transactions

UltraWallet may also sign a standard L1 `$ULTRA` transfer for the public `POST /api/transaction` endpoint:

```ts
const signedTransaction = await window.ultraWallet.request({
  method: "ultranet_signTransaction",
  params: {
    recipient: "<64 lowercase hexadecimal address>",
    amount: 25_000_000,
    fee: 250_000,
    nonce: 0,
    timestamp: Math.floor(Date.now() / 1000),
    nullifier: [/* exactly 32 fresh bytes */],
    gasLimit: 500_000,
    gasPrice: 1,
    chainId: 0,
    version: 1,
  },
});
```

`amount` and `fee` are integer base units. The website presents six decimal places, so `25_000_000` is `25.000000 $ULTRA`. The node's current minimum transfer fee is one percent of the amount, with a minimum of one base unit. The wallet must show the sender, recipient, amount, fee, total, node origin, and irreversible-transfer warning before signing.

The version-1 signing digest is exactly:

```text
SHA3-256(
  sender UTF-8 bytes ||
  recipient UTF-8 bytes ||
  amount.to_le_bytes() ||
  fee.to_le_bytes() ||
  timestamp.to_le_bytes() ||
  nullifier[32] ||
  nonce.to_le_bytes() ||
  gas_limit.to_le_bytes() ||
  gas_price.to_le_bytes()
)
```

`chain_id`, `version`, and the JSON object are not included in this legacy digest because the Rust validator's version-1 `create_transaction_message` does not hash them. A future signing-envelope version must be introduced and vector-tested before changing this order.

The response contains only public fields and the raw 2,592-byte Dilithium-5 public key, 4,627-byte signature, and transaction values as JSON arrays/numbers. Private keys, seeds, recovery phrases, passwords, and admin tokens must never appear in the response or request:

```json
{
  "sender": "<derived address>",
  "sender_public_key": [/* 2,592 bytes */],
  "recipient": "<64 lowercase hexadecimal address>",
  "amount": 25000000,
  "fee": 250000,
  "nonce": 0,
  "timestamp": 1785183488,
  "nullifier": [/* 32 bytes */],
  "gas_limit": 500000,
  "gas_price": 1,
  "signature": [/* 4,627 bytes */],
  "chain_id": 0,
  "version": 1
}
```

Submit it to `POST <API_BASE_URL>/api/transaction`. This is a wallet-authorized public transfer endpoint; it does not use `ULTRANET_ADMIN_TOKEN`, an admin bearer header, or the operator session. A successful response returns a transaction projection with a hash and `status: "pending"`. The same signed request may be submitted again after an uncertain network response: the node binds the nullifier to the original fields and returns the existing hash instead of adding a second mempool entry. A different transaction using the same nullifier is rejected.

Read-only account and history routes are:

```text
GET /api/account/<address>
GET /api/transaction/estimate?recipient=<address>&amount=<base-units>
GET /api/address/<address>/transactions?limit=20
GET /api/transaction/<hash>
```

Status values are `pending`, `confirmed`, or `failed`. If the submission response is lost, do not automatically sign or submit another transfer; keep the public signed request in memory only and use the explicit status/idempotency action. The node's `ULTRANET_ADMIN_TOKEN` remains a private operator credential for routes such as mining, pruning, and AppChain administration.

## 9. Minimal provider example

The following example shows the provider boundary only. The signing implementation must remain inside the wallet and is intentionally omitted:

```ts
window.ultraWallet = {
  async request(request) {
    if (
      request.method !== "ultranet_signValidatorProposal" ||
      request.params.version !== 2
    ) {
      throw { code: "UNSUPPORTED_METHOD", message: "Unsupported UltraNet method or signing version" };
    }

    // 1. Display request.params to the user.
    // 2. Build the canonical UltraNet governance transaction locally.
    // 3. Sign with the wallet's Dilithium private key.
    // 4. Return only the public fields below.
    return {
      sender: "<derived-address>",
      sender_public_key: [/* Dilithium public-key bytes */],
      proposal_public_key: [/* applicant public-key bytes */],
      nonce: 0,
      timestamp: Math.floor(Date.now() / 1000),
      nullifier: [/* exactly 32 bytes */],
      signature: [/* Dilithium signature bytes */],
      version: 2,
    };
  },
};
```

This example is a contract illustration, not a signing implementation. Never place private key material in the returned object.
