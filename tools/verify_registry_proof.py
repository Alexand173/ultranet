#!/usr/bin/env python3
"""Check that your game's day is inside the root UltraNet anchored. Standard library only.

    curl -s https://api.ultranetwork.cc/api/games/g/<your-game>/proof/<YYYY-MM-DD> > proof.json
    python3 tools/verify_registry_proof.py proof.json
    python3 tools/verify_registry_proof.py proof.json --results results.json     # also recompute YOUR root

`results.json` is a list of the results you sent that day:
    [{"player": "<64 hex>", "result_id": "match-0001", "score": 1200, "data_hash": "<64 hex or omit>"}, ...]

What a pass means: the day's anchored root contains the root of exactly these results for your game id,
so none of them can have been changed or removed since. It does not mean the results were true.
The layouts are documented at the top of src/games/registry.rs; this file is an independent
implementation of them, checked against the Rust code's pinned vectors.
"""
import hashlib
import json
import struct
import sys


def sha3(*parts):
    h = hashlib.sha3_256()
    for part in parts:
        h.update(part)
    return h.digest()


def result_id_hash(result_id):
    return sha3(b"UltraGame/result-id/v1", result_id.encode())


def game_results_root(game_id, results):
    records = []
    for r in results:
        data_hash = bytes.fromhex(r["data_hash"]) if r.get("data_hash") else bytes(32)
        records.append((bytes.fromhex(r["player"]), result_id_hash(r["result_id"]), int(r.get("score", 0)), data_hash))
    records.sort(key=lambda rec: (rec[0], rec[1]))
    body = b"".join(p + i + struct.pack("<Q", s) + d for p, i, s, d in records)
    return sha3(b"UltraGame/results/v1", struct.pack("<I", len(game_id)), game_id.encode(),
                struct.pack("<I", len(records)), body)


def leaf_hash(game_id, game_root):
    return sha3(b"UltraGame/leaf/v1", struct.pack("<I", len(game_id)), game_id.encode(), game_root)


def climb(leaf, steps):
    current = leaf
    for step in steps:
        sibling = bytes.fromhex(step["sibling"])
        current = sha3(b"UltraGame/node/v1", sibling, current) if step["side"] == "left" \
            else sha3(b"UltraGame/node/v1", current, sibling)
    return current


def verify(proof, results=None):
    """Return a list of problems; empty means the proof holds."""
    problems = []
    game_id, game_root = proof["game_id"], bytes.fromhex(proof["game_root"])
    leaf = leaf_hash(game_id, game_root)
    if leaf.hex() != proof["leaf"]:
        problems.append("the leaf is not the hash of this game id and game root")
    if climb(leaf, proof["steps"]).hex() != proof["merkle_root"]:
        problems.append("the proof does not climb to the merkle root")
    day_root = sha3(b"UltraGame/day-root/v1", proof["date"].encode(), bytes.fromhex(proof["merkle_root"]))
    if day_root.hex() != proof["day_root"]:
        problems.append("the day root is not the hash of the date and the merkle root")
    if results is not None and game_results_root(game_id, results) != game_root:
        problems.append("the results you supplied do not produce the game root in the proof")
    return problems


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    proof = json.load(open(argv[1]))
    results = json.load(open(argv[argv.index("--results") + 1])) if "--results" in argv else None
    problems = verify(proof, results)
    if problems:
        for problem in problems:
            print("FAIL:", problem)
        return 1
    print(f"ok: {proof['game_id']} on {proof['date']} is inside day root {proof['day_root']}")
    if proof.get("anchor_tx"):
        print(f"anchored by transaction {proof['anchor_tx']} on chain #{proof.get('chain_id')}")
    else:
        print(f"not yet anchored (state: {proof.get('state')}); the root is fixed but not yet on the chain")
    if results is not None:
        print("and your own results reproduce the game root")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
