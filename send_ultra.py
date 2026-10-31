#!/usr/bin/env python3
"""Fund the UltraNet faucet from the Sovereign account.

The Sovereign account requires 2-of-3 signatures, so this script loads `sovereign_keys.json`, signs
the canonical version-1 transaction digest with the first two owners, and submits the envelope.

Three values are deliberately not constants:

  * `nonce` is read from the node before signing. A stale nonce is rejected outright, and the
    Sovereign account moves it on every transfer.
  * `timestamp` is taken at signing time. The node rejects a transaction older than one hour with
    "Transaction is too old!", which is what a hardcoded timestamp eventually guarantees.
  * `fee` is quoted by `/api/transaction/estimate` for this exact recipient and amount. It is not
    flat: the node charges `max(1, amount / 100)`, one percent, so the quote is checked against
    that rule rather than a constant, and a change in policy stops the run instead of being signed.

Pass `--amount` in ULTRA (default 100). The fee is charged on top of it, so moving 201 needs
203.010000 of balance.
"""
import argparse
import hashlib
import json
import os
import secrets
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from decimal import Decimal, InvalidOperation

from pqc.sign import dilithium5

parser = argparse.ArgumentParser(
    description="Fund the UltraNet faucet from the Sovereign account (dry run unless --send).",
)
parser.add_argument(
    "--send",
    action="store_true",
    help="sign, write transfer.json and submit the envelope. Without this flag the script only dry-runs.",
)
parser.add_argument(
    "--check-file",
    default="",
    help="write the digest, signature and public key to this path so an independent verifier "
         "(the Rust node) can confirm the binding is byte-compatible.",
)
parser.add_argument(
    "--amount",
    default="100",
    help="float to transfer, in ULTRA with up to six decimals (default 100). The node charges a "
         "1%% minimum fee on top, so 201 needs 203.010000 of balance.",
)
args = parser.parse_args()

# CONFIG
NODE_URL = "https://api.ultranetwork.cc"
SOVEREIGN_ADDR = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"
FAUCET_ADDR = "fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8"

BASE_UNITS = 1_000_000      # six decimals, as the node counts
CHAIN_ID = 0
VERSION = 1
API_URL = NODE_URL + "/api/transaction"


def parse_ultra(text: str) -> int:
    """ULTRA on the command line → base units, refusing precision the chain cannot carry."""
    try:
        amount = Decimal(text)
    except InvalidOperation:
        raise SystemExit(f"--amount {text!r} is not a number of ULTRA.")
    units = amount * BASE_UNITS
    if units != units.to_integral_value():
        raise SystemExit(f"--amount {text} has more than six decimals; the node counts base units.")
    if units <= 0:
        raise SystemExit("--amount must be greater than zero.")
    return int(units)


AMOUNT = parse_ultra(args.amount)
# `max(1, amount / 100)` is the node's minimum transfer fee (src/lib.rs:1443), so the quote is
# checked against that rule instead of a hardcoded number.
EXPECTED_FEE = max(1, AMOUNT // 100)

# The edge in front of the API answers 403 to clients that send no usable User-Agent.
USER_AGENT = "UltraNet-send-ultra/2.0"


def node_get(path: str):
    request = urllib.request.Request(NODE_URL + path, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit(f"{path} answered HTTP {error.code} {error.reason}")
    except urllib.error.URLError as error:
        raise SystemExit(f"{path} is unreachable: {error.reason}")
    if not payload.get("success") or payload.get("data") is None:
        raise SystemExit(f"{path} returned no data: {payload.get('message')}")
    return payload["data"]


# Učitaj sovereign ključeve
with open("sovereign_keys.json") as f:
    data = json.load(f)
    if isinstance(data, list):
        owners = data
    elif isinstance(data, dict) and "owners" in data:
        owners = data["owners"]
    else:
        owners = [data]

owner0 = owners[0]
owner1 = owners[1]

sender_pubkey_hex = owner0["public_key"]
sender_pubkey_bytes = bytes.fromhex(sender_pubkey_hex)
sender_pubkey_list = list(sender_pubkey_bytes)

sk0_hex = owner0["secret_key"]
sk1_hex = owner1["secret_key"]
sk0 = bytes.fromhex(sk0_hex)
sk1 = bytes.fromhex(sk1_hex)

# Live state: the nonce the node expects next, and the fee it quotes for this exact transfer.
account = node_get(f"/api/account/{SOVEREIGN_ADDR}")
NONCE = account["nonce"]
balance = account["balance_base_units"]

query = urllib.parse.urlencode({"recipient": FAUCET_ADDR, "amount": AMOUNT})
estimate = node_get("/api/transaction/estimate?" + query)
if estimate["recipient"] != FAUCET_ADDR or estimate["amount"] != AMOUNT:
    raise SystemExit("The node quoted a different transfer than the one configured here.")
if estimate["fee"] != EXPECTED_FEE:
    raise SystemExit(
        f"The node quotes a fee of {estimate['fee']} base units, not the {EXPECTED_FEE} that "
        "max(1, amount / 100) predicts. Review the fee policy before signing: the fee is part of "
        "the signed digest."
    )

FEE = estimate["fee"]
GAS_LIMIT = estimate["gas_limit"]
GAS_PRICE = estimate["gas_price"]
timestamp = int(time.time())
total = AMOUNT + FEE

print(f"Sovereign : {SOVEREIGN_ADDR}")
print(f"Faucet    : {FAUCET_ADDR}")
print(f"Amount    : {AMOUNT / BASE_UNITS:.6f} ULTRA")
print(f"Network fee: {FEE / BASE_UNITS:.6f} ULTRA  (gas_limit {GAS_LIMIT}, gas_price {GAS_PRICE})")
print(f"Total debit: {total / BASE_UNITS:.6f} ULTRA of {balance / BASE_UNITS:.6f} ULTRA")
print(f"Nonce     : {NONCE}")
print(f"Timestamp : {timestamp}")

if total > balance:
    raise SystemExit("The Sovereign balance does not cover this transfer plus its fee.")

nullifier_bytes = secrets.token_bytes(32)
nullifier_list = list(nullifier_bytes)

sender_bytes = SOVEREIGN_ADDR.encode('utf-8')
recipient_bytes = FAUCET_ADDR.encode('utf-8')
amount_bytes = AMOUNT.to_bytes(8, 'little')
fee_bytes = FEE.to_bytes(8, 'little')
timestamp_bytes = timestamp.to_bytes(8, 'little')
nonce_bytes = NONCE.to_bytes(8, 'little')
gas_limit_bytes = GAS_LIMIT.to_bytes(8, 'little')
gas_price_bytes = GAS_PRICE.to_bytes(8, 'little')

message = (sender_bytes + recipient_bytes + amount_bytes + fee_bytes +
           timestamp_bytes + nullifier_bytes + nonce_bytes +
           gas_limit_bytes + gas_price_bytes)
digest = hashlib.sha3_256(message).digest()
print("Digest:", digest.hex())

print("Signing with owner0...")
sig0 = dilithium5.sign(digest, sk0)
print("Signing with owner1...")
sig1 = dilithium5.sign(digest, sk1)

# Verify both signatures against their own owner key before anything can be submitted: a binding
# that signs with the wrong scheme fails here instead of on chain.
if not dilithium5.verify(sig0, digest, sender_pubkey_bytes):
    raise SystemExit("owner0's signature does not verify under owner0's public key; refusing to continue.")
if not dilithium5.verify(sig1, digest, bytes.fromhex(owner1["public_key"])):
    raise SystemExit("owner1's signature does not verify under owner1's public key; refusing to continue.")

combined_sig = list(sig0 + sig1)
if len(combined_sig) != 9254:
    raise ValueError(f"Combined signature length is {len(combined_sig)}, expected 9254")

tx = {
    "sender": SOVEREIGN_ADDR,
    "sender_public_key": sender_pubkey_list,
    "recipient": FAUCET_ADDR,
    "amount": AMOUNT,
    "fee": FEE,
    "nonce": NONCE,
    "timestamp": timestamp,
    "nullifier": nullifier_list,
    "gas_limit": GAS_LIMIT,
    "gas_price": GAS_PRICE,
    "signature": combined_sig,
    "chain_id": CHAIN_ID,
    "version": VERSION
}

if args.check_file:
    os.makedirs(os.path.dirname(os.path.abspath(args.check_file)), exist_ok=True)
    with open(args.check_file, "w") as handle:
        json.dump(
            {
                "digest": digest.hex(),
                "signature": sig0.hex(),
                "public_key": sender_pubkey_hex,
                # The node checks both signatures and requires two distinct owners, so both are
                # recorded here rather than only the one whose key signs the account.
                "second_signature": sig1.hex(),
                "second_public_key": owner1["public_key"],
                "combined_signature_bytes": len(combined_sig),
                # The signed fields let the verifier rebuild the preimage with the node's own
                # `create_transaction_message_for`, so a layout mismatch fails here rather than
                # as "invalid signature" after submission.
                "signed_fields": {
                    "sender": SOVEREIGN_ADDR,
                    "recipient": FAUCET_ADDR,
                    "amount": AMOUNT,
                    "fee": FEE,
                    "nonce": NONCE,
                    "timestamp": timestamp,
                    "nullifier": nullifier_bytes.hex(),
                    "gas_limit": GAS_LIMIT,
                    "gas_price": GAS_PRICE,
                    "chain_id": CHAIN_ID,
                    "version": VERSION,
                },
            },
            handle,
        )
    print(f"Wrote the signing artifact to {args.check_file} for independent verification.")

if not args.send:
    redacted = {
        key: (value if key not in {"sender_public_key", "signature", "nullifier"} else f"<{len(value)} entries>")
        for key, value in tx.items()
    }
    print()
    print("DRY RUN — nothing was submitted and transfer.json was not written.")
    print(f"  digest    : {digest.hex()}")
    print(f"  signature : {len(combined_sig)} bytes (2 x {dilithium5.SIGNATURE_BYTES})")
    print(json.dumps(redacted, indent=2))
    print("Re-run with --send to sign and submit this transfer.")
    raise SystemExit(0)

with open("transfer.json", "w") as f:
    json.dump(tx, f, indent=2)

print("transfer.json created. Submitting via curl...")

cmd = [
    "curl", "-s", "-X", "POST", API_URL,
    "-H", "Content-Type: application/json",
    "--data-binary", "@transfer.json"
]
result = subprocess.run(cmd, capture_output=True, text=True)
print("Response:")
print(result.stdout)
if result.stderr:
    print("Errors:", result.stderr)
