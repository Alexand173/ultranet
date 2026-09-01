#!/usr/bin/env python3
import json
import time
import secrets
import hashlib
import subprocess
from pqc.sign import dilithium5

# CONFIG
SOVEREIGN_ADDR = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"
FAUCET_ADDR = "787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358"
AMOUNT = 1_000_000_000       # 1.000 ULTRA
FEE = 10_000_000             # 10 ULTRA
GAS_LIMIT = 1_000_000
GAS_PRICE = 1
NONCE = 0                    # proveri pre pokretanja
CHAIN_ID = 0
VERSION = 1
API_URL = "https://api.ultranetwork.cc/api/transaction"

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

nullifier_bytes = secrets.token_bytes(32)
nullifier_list = list(nullifier_bytes)
timestamp = 1788108456   # add 10 seconds to be safe


sender_bytes = SOVEREIGN_ADDR.encode('utf-8')
recipient_bytes = FAUCET_ADDR.encode('utf-8')
amount_bytes = AMOUNT.to_bytes(8, 'little')
fee_bytes = FEE.to_bytes(8, 'little')
timestamp_bytes = timestamp.to_bytes(8, 'little')
nullifier_bytes = bytes(nullifier_list)
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