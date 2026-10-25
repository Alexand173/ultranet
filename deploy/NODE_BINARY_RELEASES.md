# Node binary releases

The node is deployed as a single file at `/opt/ultranet/target/release/UltraNet` and replaced in place —
there is no node release versioning. (`/opt/ultranet/releases/<commit>` and `releases/previous` are
**website** deployments: `next.config`, `node_modules`, `src`, whitepaper HTML. They are not node
binaries.) Because of that, every node replacement must be recorded here with what changed, so the next
person can tell what is running and how to roll back.

Backups are kept beside the binary as `UltraNet.bak-<UTC timestamp>`.

## 2026-10-25T10:04Z — Merkle fix for payload-bound transactions

| | |
|---|---|
| Running before | sha256 `d8edb01e33bfe651540781e36668b702e56e9489f63be3b00df43ca45aa5c977`, 25,167,088 B |
| Backup | `/opt/ultranet/target/release/UltraNet.bak-20261025T0957Z` (same sha256) |
| Running after | sha256 `e31cc57983b36ac07e7e726d735c5b6d7ef3675ece319734160011ecb19272e3`, 25,169,088 B |
| Source base | commit `0cbe8cf` ("Finalize denomination correction and canonical state migration", 2026-08-30) |
| Change | **one function** — see below |

### Why

A block containing a sovereign supply correction could not be mined. `validate_block` binds
`SovereignSupplyCorrection` into the block Merkle root; `mine_block` bound only `MoveCall`. The miner
built one root, the validator recomputed another, and the node rejected its own block with
`❌ Merkle root mismatch!` — surfaced as `POST /api/mine` → `Error: Block rejected!`. The correction also
stayed pending, so mining was blocked for as long as it sat in the mempool.

### What changed

`src/lib.rs`, in `mine_block` only:

```rust
- if let TransactionPayload::MoveCall { .. } = &tx.payload {
+ if matches!(
+     &tx.payload,
+     TransactionPayload::MoveCall { .. }
+         | TransactionPayload::SovereignSupplyCorrection { .. }
+ ) {
      block_merkle_tree.insert(format!("{}:move", tx.sender).as_bytes(), &tx.get_hash());
  }
```

`+8 / −1` in one file. No other source, config, key, address, peer or genesis change.

The repository's own fix for this (`3795742`) is a broader refactor that shares one
`bind_block_transaction_merkle` between miner and validator. That refactor was deliberately **not**
deployed here: shipping it would have pulled 90 commits of drift onto the running node at the same time.
This build is the minimal equivalent.

### How the source base was confirmed

An unpatched release build of `0cbe8cf` reproduced the running binary **exactly** — same 25,167,088
bytes, same sha256 `d8edb01e…`. So the deployed node was that commit, byte for byte, and the only
difference in the new binary is the patch above. Validate any future claim the same way before deploying.

### Verification after deploy

| Check | Result |
|---|---|
| service | active, API answering within ~15 s |
| block 1 hash / state root | byte-identical to pre-deploy — history untouched |
| `chain_length` / blocks | 2 → 3 only after the intended mine; no other block |
| sovereign `3b8ef38a…` | 998,990.000000 → **1,000,000.000000** (v4 supply correction, block 2) |
| faucet `787e68b2…` | 1,000.001000, unchanged (its key is not held anywhere; see the run sheet) |
| approval journal | 1 record, unchanged |
| peers | same persistent peer `12D3KooWS3AHS46…` reconnected after restart |

### Rollback

```sh
systemctl stop ultranet
cp -p /opt/ultranet/target/release/UltraNet.bak-20261025T0957Z /opt/ultranet/target/release/UltraNet
chown ultranet:ultranet /opt/ultranet/target/release/UltraNet
systemctl start ultranet
```

Rolling back restores the Merkle bug: any block containing a payload-bound correction or a
`MoveCall`-plus-correction mix would again be rejected. Do not roll back while such a transaction is
pending.
