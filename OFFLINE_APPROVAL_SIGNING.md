# Offline validator approval signing

This guide is for the three Sovereign owners who approve a pending validator. The applicant's validator key and personal wallet are **not** used for this step.

The approval threshold is **2 of 3**. Two different Sovereign owners sign the same version-3 approval draft with their Dilithium-5 private keys. The node verifies both signatures and activates the pending validator. The CLI does not assign approval authority; the node's configured Sovereign public keys remain the final authority.

The Join Swarm dashboard may orchestrate the same ceremony through a separately deployed signer/HSM boundary, but it does not replace this offline procedure. The browser can request a short-lived approval intent and display public progress only; it never receives a Sovereign private key, nonce/nullifier/digest internals, or signature arrays. A local signer/HSM presence confirmation remains mandatory even when the browser flow has only review and confirm buttons.

## Security boundary

Keep `sovereign_keys.json` and all `secret_key` / `private_key` fields on an offline signing machine. Do not copy them to:

- the validator node or VPS;
- the website or browser wallet;
- a repository, issue, chat, screenshot, or log;
- a machine used to serve the public API.

Only these public artifacts may leave the signing machine:

- the public owner identity manifest;
- the approval draft;
- an owner signature artifact containing a public key and signature;
- the final combined approval payload.

The node-admin `ULTRANET_ADMIN_TOKEN` is not an approval signature and is not required by `/api/governance/approve`.

## Key-file formats

The CLI accepts either of the repository's existing formats:

```json
[
  {
    "address": "64 lowercase hex characters",
    "public_key": "Dilithium-5 public key hex",
    "secret_key": "Dilithium-5 secret key hex"
  }
]
```

or:

```json
{
  "owners": [
    {
      "address": "64 lowercase hex characters",
      "public_key": [1, 2, 3],
      "private_key": [4, 5, 6]
    }
  ]
}
```

The byte arrays above are abbreviated examples and are not valid keys. Restrict the actual file before using it:

```bash
chmod 600 /offline/sovereign_keys.json
```

For a real 2-of-3 ceremony, do not give every owner a copy of a file containing all three private keys. Generate the public owner manifest once, then distribute one private owner record to each signer through a controlled offline process. Each signer should keep only their own private key file. If a signer has a one-record file, use `--owner-index 0`.

Build the signer before moving the signing environment offline:

```bash
cargo build --release --locked --bin ultranet-approve
```

The binary is `target/release/ultranet-approve`.

## 1. Inspect owner identities without printing private keys

This command reads and validates the complete key file but prints only owner indexes, derived addresses, and public keys:

```bash
target/release/ultranet-approve owners \
  --keys /offline/sovereign_keys.json \
  --pretty \
  --output /offline/owner-identities.json
```

The command refuses a key file that is readable by group or other users. It also rejects mismatched declared addresses and public/private keypairs.

## 2. Prepare one approval draft

First list pending proposals from a machine that can reach the node API:

```bash
API="https://api.ultranetwork.cc"
curl --fail-with-body -sS "$API/api/governance/proposals" | jq
```

Copy the exact `hash` for the validator you reviewed. It is a 64-character hexadecimal proposal hash.

Prepare a draft:

```bash
target/release/ultranet-approve prepare \
  --api-base-url "$API" \
  --proposal-hash "<64-hex-proposal-hash>" \
  --output /offline/approval-draft.json \
  --pretty
```

`prepare` performs the following checks and actions:

1. Confirms the proposal is still in the pending governance queue.
2. Fetches the current next nonce for the fixed Sovereign address.
3. Uses the current Unix timestamp.
4. Generates a fresh 32-byte nullifier with the operating system CSPRNG.
5. Writes only public approval fields to the draft.

The draft contains:

```json
{
  "proposal_hash": "64 lowercase hex characters",
  "timestamp": 1785183488,
  "nonce": 0,
  "nullifier": [32 integer byte values],
  "version": 3
}
```

The example values are placeholders. Never reuse the example timestamp, nonce, hash, or nullifier.

If the signing machines cannot reach the API, obtain the current nonce and proposal hash through a trusted coordinator and use explicit offline mode:

```bash
target/release/ultranet-approve prepare \
  --offline \
  --proposal-hash "<64-hex-proposal-hash>" \
  --nonce "<current-sovereign-next-nonce>" \
  --output /offline/approval-draft.json \
  --pretty
```

`--offline` makes no network requests. The operator must independently verify that the proposal is still pending, that the hash belongs to the intended validator, and that the nonce is current before signing. Without `--offline`, the CLI verifies the pending proposal and fetches the current nonce from the API.

## 3. Owner 1 signs the draft offline

On the first owner's offline signing machine:

```bash
target/release/ultranet-approve sign \
  --request /offline/approval-draft.json \
  --keys /offline/owner-0-key.json \
  --owner-index 0 \
  --output /offline/owner-0-approval.json \
  --pretty
```

The signer:

- reads only that owner's `secret_key` or `private_key` locally;
- derives and checks the owner's address from the public key;
- checks that the public and secret keys form one Dilithium-5 keypair;
- constructs the exact UltraNet version-3 approval digest;
- signs locally;
- self-verifies the signature before writing the artifact;
- never writes the secret key to the artifact.

The resulting artifact contains the draft, owner address, public key, and one hexadecimal 4,627-byte signature. The combiner identifies the owner by matching this public key to the public manifest; the artifact does not need to carry an owner index.

If each owner has a one-record private file, both signing commands use `--owner-index 0`; the public manifest remains the single manifest containing all three authorized public keys.

## 4. Owner 2 signs the same draft

Give Owner 2 the **same** `approval-draft.json`, preferably through a controlled public-artifact transfer. Owner 2 uses a different authorized key:

```bash
target/release/ultranet-approve sign \
  --request /offline/approval-draft.json \
  --keys /offline/owner-1-key.json \
  --owner-index 0 \
  --output /offline/owner-1-approval.json \
  --pretty
```

Both owners must sign the same:

- proposal hash;
- timestamp;
- nonce;
- nullifier;
- version `3`.

If any one of these differs, the signatures cannot be combined for this approval.

Owner 3 is not required when two distinct authorized owners have valid signatures. The artifact order does not matter; the combiner canonicalizes the public signature order by the owner indexes in `owner-identities.json`.

## 5. Verify and combine the two signatures

On a coordinator or offline signing machine with the public owner manifest (not `sovereign_keys.json`):

```bash
target/release/ultranet-approve combine \
  --request /offline/approval-draft.json \
  --authorized-owners /offline/owner-identities.json \
  --signature /offline/owner-0-approval.json \
  --signature /offline/owner-1-approval.json \
  --output /offline/approval.json \
  --pretty
```

`owner-identities.json` is the public output from the `owners` command. The combiner does not need `sovereign_keys.json` and should never receive it.

The combiner verifies:

- both artifacts use the exact draft fields;
- each signature is exactly 4,627 bytes;
- each signature verifies against the canonical version-3 digest;
- each artifact's public key matches the supplied authorized owner record;
- the two public keys map to distinct authorized owners;
- the two public keys are distinct;
- the final signature length is exactly 9,254 bytes.

The output is the public request body expected by the node:

```json
{
  "proposal_hash": "...",
  "timestamp": 1785183488,
  "nonce": 0,
  "nullifier": [/* 32 integers */],
  "version": 3,
  "signature": [/* 9,254 integers */]
}
```

The `signature` array is flat:

```text
signature[0..4626]       = first owner's Dilithium-5 signature
signature[4627..9253]    = second owner's Dilithium-5 signature
```

The secret keys are not part of this output.

## 6. Submit the public approval

Submit only the combined public payload:

```bash
target/release/ultranet-approve submit \
  --api-base-url "$API" \
  --request /offline/approval.json
```

This sends the payload to:

```text
POST https://api.ultranetwork.cc/api/governance/approve
```

A successful response is:

```text
Validator proposal approved!
```

No `Authorization: Bearer` admin token is needed for this endpoint. Authorization comes from the two valid Sovereign Dilithium signatures.

If the network response is lost, retry the **same** `/offline/approval.json` once. Do not change the timestamp, nonce, nullifier, or signatures for a transport retry. If the node explicitly reports a stale nonce, expired timestamp, or another validation failure, prepare a new draft and obtain fresh signatures.

## 7. Verify activation

```bash
curl --fail-with-body -sS "$API/api/governance/proposals" | jq
curl --fail-with-body -sS "$API/api/governance/approvals?limit=50" | jq
```

After a successful approval, the proposal leaves the pending queue and appears in the durable approval journal. The validator is added to the active validator set.

## Fixed approval envelope

The signer matches the node's `UltraBlockchain::create_transaction_message` implementation. For version 3, the signed digest is:

```text
SHA3-256(
  sovereign_address_bytes ||
  "0x0" ||
  amount=0 as little-endian u64 ||
  fee=0 as little-endian u64 ||
  timestamp as little-endian u64 ||
  nullifier[32] ||
  nonce as little-endian u64 ||
  gas_limit=1_000_000 as little-endian u64 ||
  gas_price=1 as little-endian u64 ||
  "UltraNet/approval-signing-envelope/v3" ||
  version=3 as little-endian u32 ||
  chain_id=0 as little-endian u32 ||
  "ValidatorApproval" ||
  proposal_hash[32]
)
```

The node accepts the approval only when at least two distinct configured Sovereign owner public keys verify signatures over this exact digest.
