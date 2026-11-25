#!/usr/bin/env python3
"""Validate the v5 AppChain contract, its schema and its pinned vectors.

Three independent checks, all offline:

1. `vector-digests`  — rebuilds the signing preimage from CONTRACT.md §4 and recomputes SHA3-256 for
   every vector in `vectors.json`. This is a second implementation, in another language, of what the
   node does in Rust; if either drifts, the digests stop matching.
2. `schema`          — validates `vectors.json`, requests built from each vector, and sample responses
   captured from the running node against `contract.schema.json`.
3. `negative`        — proves the schema is strict where the node is strict: unknown fields, a wrong
   version, a short nullifier and a non-hex sender must all be rejected.

Run: python3 docs/appchain/v5/validate_contract.py
Requires: jsonschema (>=4), hashlib.sha3_256 from the standard library.
Exit code 0 means every check passed.
"""

from __future__ import annotations

import copy
import hashlib
import json
import pathlib
import struct
import sys

HERE = pathlib.Path(__file__).resolve().parent
SCHEMA_PATH = HERE / "contract.schema.json"
VECTORS_PATH = HERE / "vectors.json"

FAILURES: list[str] = []


def check(condition: bool, message: str) -> None:
    if condition:
        print(f"  PASS  {message}")
    else:
        print(f"  FAIL  {message}")
        FAILURES.append(message)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def sha3(data: bytes) -> str:
    return hashlib.sha3_256(data).hexdigest()


def preimage(vector: dict) -> bytes:
    """CONTRACT.md §4, field by field, in the documented order."""
    i = vector["inputs"]
    parts = [
        i["sender"].encode("ascii"),
        i["recipient"].encode("ascii"),
        u64(i["amount"]),
        u64(i["fee"]),
        u64(i["timestamp"]),
        bytes.fromhex(i["nullifier"]),
        u64(i["nonce"]),
        u64(i["gas_limit"]),
        u64(i["gas_price"]),
        "UltraNet/appchain-envelope/v5".encode("ascii"),
        u32(i["version"]),
        u32(i["chain_id"]),
        vector["tag"].encode("ascii"),
    ]
    payload = i["payload"]
    if vector["operation"] == "register":
        name = payload["name"].encode("ascii")
        parts += [u64(len(name)), name]
    elif vector["operation"] == "anchor":
        parts += [u32(payload["chain_id"]), bytes.fromhex(payload["state_root"])]
    elif vector["operation"] == "transfer_owner":
        new_owner = payload["new_owner"].encode("ascii")
        parts += [u32(payload["chain_id"]), u64(len(new_owner)), new_owner]
    else:  # pragma: no cover - the vectors file only carries these three
        raise SystemExit(f"unknown operation in vectors: {vector['operation']}")
    return b"".join(parts)


def hex_to_bytes(value: str) -> list[int]:
    return list(bytes.fromhex(value))


def request_from_vector(vector: dict) -> dict:
    """The wire form: byte fields as integer arrays, not hex strings."""
    i = vector["inputs"]
    operation = {vector["operation"]: i["payload"]}
    return {
        "sender": i["sender"],
        "sender_public_key": [0] * 2592,
        "nonce": i["nonce"],
        "timestamp": i["timestamp"],
        "nullifier": hex_to_bytes(i["nullifier"]),
        "fee": i["fee"],
        "signature": [0] * 4627,
        "version": i["version"],
        "operation": operation,
    }


SAMPLE_RESPONSES = {
    "LedgerResponse": {
        "success": True,
        "terms": {
            "activation_height": 10,
            "anchor_fee_base_units": 1000,
            "gas_limit": 100000,
            "gas_price": 1,
            "recipient": "appchain",
            "register_fee_base_units": 10000,
            "transfer_fee_base_units": 1000,
            "version": 5,
        },
        "chains": [
            {
                "anchor_count": 1,
                "id": 1,
                "latest_anchor_block": 12,
                "latest_state_root": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "name": "UltraGrid",
                "owner": "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e",
                "registered_block": 11,
                "registered_tx": "bbc892926056e1389d39caeb6095c3f83ff7ad43290baa78f44a2c6f51b43ca2",
            }
        ],
    },
    "AnchorsResponse": {
        "anchors": [
            {
                "block_index": 12,
                "block_timestamp": 1794761124,
                "chain_id": 1,
                "sequence": 1,
                "state_root": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "tx_hash": "11104b653c31f2ea62c05ce0c658788d07337db0328032e37c52f23854830095",
            }
        ],
        "chain_id": 1,
        "from": 1,
        "success": True,
    },
    "OverviewResponse": {
        "success": True,
        "chains": [
            {
                "id": 1,
                "name": "UltraGrid",
                "owner": "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e",
                "account_address": "",
                "genesis_root": "",
                "anchor_fee": "1000",
                "balance": "",
                "anchor_spend": "1000",
                "anchor_count": 1,
                "latest_anchor_at": 1794761124,
                "latest_state_root": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "anchor_availability": "not-implemented",
                "proof_scheme": "owner-signed state root (a signature, not a state proof)",
                "source": "ledger",
                "registered_tx": "bbc892926056e1389d39caeb6095c3f83ff7ad43290baa78f44a2c6f51b43ca2",
                "registered_block": 11,
            }
        ],
        "totals": {"anchor_count": 1, "anchor_spend": "1000"},
        "anchor_availability": "retired",
        "proof_scheme": "owner-signed state root (a signature, not a state proof)",
        "updated_at": 1795548492,
    },
    # Both captured from the running node on 2026-11-25: the ledger's 404 omits `data`, while a
    # deserialization rejection sends `data: null`. The schema must accept both.
    "ErrorResponse": {"message": "AppChain #999 is not registered", "success": False},
    "SubmitSuccess": {
        "success": True,
        "message": "AppChain transaction accepted",
        "data": {"hash": "a" * 64, "status": "pending"},
    },
}


def main() -> int:
    try:
        import jsonschema
    except ImportError:  # pragma: no cover
        print("jsonschema is required: pip install jsonschema")
        return 2

    schema = json.loads(SCHEMA_PATH.read_text())
    vectors = json.loads(VECTORS_PATH.read_text())
    validator = jsonschema.Draft202012Validator(schema)

    print("1. vector digests recomputed from the documented preimage")
    for vector in vectors["vectors"]:
        recomputed = sha3(preimage(vector))
        check(
            recomputed == vector["digest"],
            f"{vector['operation']:<15} {recomputed[:16]}… matches the pinned digest",
        )

    print("2. schema validation")
    check(not list(validator.iter_errors(vectors)), "vectors.json conforms to VectorsFile")
    for vector in vectors["vectors"]:
        request = request_from_vector(vector)
        errors = list(validator.iter_errors(request))
        check(not errors, f"{vector['operation']:<15} request built from the vector validates")
    def def_validator(name: str):
        target = next((s for s in schema["oneOf"] if s["$ref"].endswith(f"/{name}")), None)
        sub = {"$schema": schema["$schema"], "$defs": schema["$defs"], **copy.deepcopy(target)}
        return jsonschema.Draft202012Validator(sub)

    for name, sample in SAMPLE_RESPONSES.items():
        errors = list(def_validator(name).iter_errors(sample))
        check(not errors, f"{name:<15} sample response validates")

    # The node is inconsistent about `data` on errors: both shapes are real responses, so both must
    # validate. Captured 2026-11-25 from POST /api/appchain/tx and POST /api/appchain/create.
    for label, sample in [
        (
            "error with data:null",
            {
                "data": None,
                "message": "Malformed request: Json deserialize error: missing field `sender_public_key`",
                "success": False,
            },
        ),
        (
            "error with data:null (401)",
            {"success": False, "message": "Administrator bearer token required", "data": None},
        ),
    ]:
        errors = list(def_validator("ErrorResponse").iter_errors(sample))
        check(not errors, f"ErrorResponse   {label} validates")

    print("3. the schema rejects what the node rejects")
    base = request_from_vector(vectors["vectors"][0])
    negatives = {
        "an unknown top-level field": lambda r: r.update({"unexpected": 1}),
        "an unknown field inside the operation": lambda r: r["operation"]["register"].update({"extra": True}),
        "version 4 instead of 5": lambda r: r.update({"version": 4}),
        "a 31-byte nullifier": lambda r: r.update({"nullifier": [0] * 31}),
        "an uppercase-hex sender": lambda r: r.update({"sender": r["sender"].upper()}),
        "a byte field sent as a hex string": lambda r: r.update({"signature": "00" * 4627}),
        "a zero chain id on an anchor": lambda r: r.update(
            {"operation": {"anchor": {"chain_id": 0, "state_root": "ab" * 32}}}
        ),
    }
    for description, mutate in negatives.items():
        candidate = copy.deepcopy(base)
        mutate(candidate)
        check(
            bool(list(validator.iter_errors(candidate))),
            f"rejects {description}",
        )

    print()
    if FAILURES:
        print(f"FAILED: {len(FAILURES)} check(s)")
        return 1
    print("All contract checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
