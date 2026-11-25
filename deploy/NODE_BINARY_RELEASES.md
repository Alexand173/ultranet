# Node binary releases

The node is deployed as a single file at `/opt/ultranet/target/release/UltraNet` and replaced in place —
there is no node release versioning. (`/opt/ultranet/releases/<commit>` and `releases/previous` are
**website** deployments: `next.config`, `node_modules`, `src`, whitepaper HTML. They are not node
binaries.) Because of that, every node replacement must be recorded here with what changed, so the next
person can tell what is running and how to roll back.

Backups are kept beside the binary as `UltraNet.bak-<UTC timestamp>`.

## 2026-11-24T19:22Z — A2 artefact promoted to production (promoted)

| | |
|---|---|
| Status | **installed and running.** One restart, no other service touched |
| Promoted artefact | `099bb28d5c08d7108b53b1731f795a52d0e47cf026cbe0eff892a81f8479c6aa`, 25,762,784 B, built from `33b4eac` (entry below) |
| Replaced | `69aeddd94bccc863951b7126befec83c45f21a57db126bd2305d5d9cf6b7e441`, 25,750,552 B |
| Backup | `/opt/ultranet/target/release/UltraNet.bak-20261124T191836Z`, sha256 `69aeddd9…e441` — roll back by copying it back and running `systemctl restart ultranet` |
| Target | `ultranet.service`, `/etc/systemd/system/ultranet.service`, `ExecStart=/opt/ultranet/target/release/UltraNet` |
| Procedure | local sha re-verified → timestamped backup → `scp` to `/tmp/UltraNet.20261124T191836Z` → upload hash verified against the record → `install` to `.UltraNet.new` in the same directory → hash re-verified → `mv` (atomic, same filesystem) → `systemctl restart ultranet` **once** at 19:22:15Z |
| Running process | `MainPID 649818`, `/proc/649818/exe` sha256 `099bb28d…c6aa`, `ActiveState=active`, `NRestarts=0` (a manual restart does not count as an automatic one) |
| Listeners | `127.0.0.1:8081`, `0.0.0.0:9000` |
| Startup | Groth16 setup 19:22:15 → blockchain ready 19:22:25 → REST API and P2P up 19:22:26 |

### What changed in what it answers

| Check | Before | After |
|---|---|---|
| `/api/appchain/overview` `proof_scheme` | `SHA3-256 server state commitment v1` | `owner-signed state root (a signature, not a state proof)` |
| `anchor_availability` | — | `retired` |
| per-chain `proof_scheme` | pre-C3 string | `owner-signed state root (a signature, not a state proof)` on #1, #2, #3 |

### Unchanged, verified either side of the restart

| Check | Result |
|---|---|
| Ledger terms | version 5, register 10,000, anchor 1,000, transfer 1,000, gas 100,000/1, activation height 10 — **identical** |
| Chains | #1 UltraGrid `7d2cd1cc…` (1 anchor), #2 Registry `d4fca721…` (0), #3 Ultra Chess `7d2cd1cc…` (0) — id/name/owner/anchor set **identical** |
| Balances | sovereign, `7d2cd1cc…`, `d4fca721…`, faucet `fe0b9a92…` — **identical** |
| Chain length | 16 before, 16 after; mempool 0 |
| Other services | `ultranet-records` PID 641137, `ultranet-dashboard` 584525, `ultranet-faucet` 137719, `caddy` 117984 — **all unchanged, none restarted**; all `NRestarts=0` |
| L1 transactions | none submitted; no registration, anchor or ownership transfer |

## 2026-11-24T18:39Z — A2 release artefact built from the published commit (built, then promoted at 19:22Z)

| | |
|---|---|
| Status | built and recorded at 18:39Z; **promoted at 19:22Z** — see the entry above. Nothing was installed, uploaded or restarted in this step |
| Source | `33b4eacd93f6ac2419ea23cb17717c6ab82df4e4` (the 14-commit range `7368b23..33b4eac` published to `origin/main`), clean tree (0 entries in `git status --porcelain`) |
| Artefact | `target/release/UltraNet`, 25,762,784 B |
| sha256 | `099bb28d5c08d7108b53b1731f795a52d0e47cf026cbe0eff892a81f8479c6aa` |
| Built with | `rustc 1.97.0 (2d8144b78 2026-07-07)`, `cargo 1.97.0 (c980f4866 2026-06-30)`, active toolchain `1.97.0-x86_64-unknown-linux-gnu` selected by `rust-toolchain.toml` |
| Build | `cargo build --release --locked --bin UltraNet -j 4` → exit 0, `Finished \`release\` profile [optimized] target(s) in 30.64s`, wall 0:30.69, 18:39:09Z → 18:39:39Z, 263 % CPU, peak RSS 1,501,012 kB |
| CI at source | run `45434122263` (Windows suite) green: 557 passed, 0 failed, 1 ignored, 31 binaries; Linux `cargo check --all-targets` green; run `45434122132` version consistency green |
| Supersedes | `14aff6d0…` (25,766,344 B), built from `b9bfb29`. **Do not promote that one as the artefact for this HEAD** — it predates five commits and cannot be mapped to the published tree |

### Why

The published range adds no code to *this* binary. Between `b9bfb29` and `33b4eac` the only source change is
`f58973f` — `src/games/records.rs`, `records_config.rs`, `tests.rs` — and that code is **not linked into
`UltraNet`**: `strings target/release/UltraNet` finds zero occurrences of `RECORDS_OWNER_ADDRESS`,
`RECORDS_PEER_ANCHOR_CHAINS` or `GAMES_REWARD`. The Records service is a separate target
(`src/bin/ultranet-records.rs`, installed at `/opt/ultranet-records/ultranet-records` via
`deploy/ultranet-records.service`), and it is **not** covered by this entry.

So the behaviour of this artefact equals `14aff6d0…`; what changed is provenance — it is now provably built
from the commit that CI validated and that `origin/main` points at, rather than from a five-commit-old tree.

### Verification of the build

| Check | Result |
|---|---|
| Hash, three independent tools | `sha256sum`, `openssl dgst -sha256` and Python `hashlib` all return `099bb28d…c6aa` |
| Size | 25,762,784 B, file written 2026-11-24T18:39:39Z (the build window above) |
| Cargo says the tree is fully built | re-running the same command prints `Finished` in 0.28s with **no** `Compiling` line — the artefact is current for `33b4eac`, not stale |
| Recompiled, not copied | the build log contains exactly one `Compiling` unit for the crate at 18:39, and no dependency rebuilds; the five-commit-old binary is not reused |
| Dependencies | `Cargo.lock`, `Cargo.toml` and `rust-toolchain.toml` are byte-identical between `b9bfb29` and `33b4eac` (`git diff --stat` empty), so cached dependency objects are the same locked set |
| Production untouched | no `systemctl` call, no upload, no file replaced; this entry exists only in the repository |

## 2026-11-24T13:36Z — AppChain proof scheme made truthful (built, not deployed)

| | |
|---|---|
| Status | **built and recorded only.** Nothing was installed, promoted, uploaded, restarted or served. Production still runs the 2026-11-22 binary, `69aeddd9…` |
| Source | `b9bfb293b99bf49a064b86e9bda60b070774aa4c`, clean tree (0 entries in `git status --porcelain`) |
| Artefact | `target/release/UltraNet`, 25,766,344 B |
| sha256 | `14aff6d0f065a4d3c4a9b1f91f17faa9670bccf09a7b041f673ac08024d74870` |
| Built with | `rustc 1.97.0 (2d8144b78 2026-07-07)`, `cargo 1.97.0 (c980f4866 2026-06-30)`, active toolchain `1.97.0-x86_64-unknown-linux-gnu` selected by `rust-toolchain.toml` |
| Build | `cargo build --release --locked --bin UltraNet -j 4` → exit 0, `Finished \`release\` profile [optimized] target(s) in 28.96s`, wall 0:29.01, 13:35:51Z → 13:36:20Z |
| Supersedes | the local artefact `a73068ce…`, 25,766,512 B, built 2026-11-23 21:22 from `78be820`. That one was never recorded here and was never deployed |

### Why

Two commits, each confined to `src/api.rs` and `tests/appchain_api_test.rs`:

1. `c4d294b` — the `/create` retirement message now names `POST /api/appchain/tx`, and the retirement
   test covers all four retired write routes instead of three, including the malformed-body case on
   each. This completes the C1 gate, whose `/create` coverage had not been committed.
2. `b9bfb29` — the two `proof_scheme` constants. A registry chain reports
   `server-computed state hash (retired, not a state proof)`; a ledger chain reports
   `owner-signed state root (a signature, not a state proof)`. `/overview` reports the ledger value,
   and the merge path leaves a chain's own value alone.

Both views had been reporting `SHA3-256 server state commitment v1`, a name for a mechanism this node
cannot perform: it holds no owner key and computes no state commitment for anyone.

### Verification of the build

| Check | Result |
|---|---|
| `cargo check --locked --all-targets` | Finished, no errors |
| `cargo test --locked -j 1 --test appchain_api_test` | 1 passed, 0 failed |
| `cargo test --locked -j 1 --lib a_registry_chain_describes_its_retired_mechanism_truthfully` | 1 passed, 0 failed |
| `cargo test --locked -j 1 --lib retired_appchain_anchor_routes_answer_gone` | 1 passed, 0 failed |
| sha256 recomputed from the file after the value above was recorded | matches this table |

Only the value of an informational field changed: no route, status code, fee, key, storage, envelope
or genesis change.

### What production still answers

Read-only checks on 2026-11-24, no credential, no write:

| Check | Result |
|---|---|
| `GET /api/appchain/ledger` terms | version 5, register 10000, anchor 1000, transfer 1000, gas 100000/1, activation 10 — unchanged |
| chains | #1 UltraGrid `7d2cd1cc…`, #2 Registry `d4fca721…`, #3 Ultra Chess `7d2cd1cc…` — unchanged |
| `GET /api/appchain/overview` `proof_scheme` | `SHA3-256 server state commitment v1` — the pre-C3 string, because this artefact is not deployed |

### Promotion

Not performed, and not implied by this entry. When this artefact is promoted, use the same preflight as
the 2026-11-22 entry: record the running sha, back it up beside the binary, install by atomic `mv`,
restart once, verify `/proc/<pid>/exe` and the listener, then add a new section here with the result.

## 2026-11-22T20:00Z — AppChain: provenance committed, retired routes settled, banner corrected

| | |
|---|---|
| Running before | sha256 `5344ed0c4ac6fb1d646092c8e6f1070125b0757daf123302a1b0d73f42d9fb73`, 25,752,032 B (the 18:55Z O-2 promotion, which was recorded only in the AppChain checklist) |
| Backup | `/opt/ultranet/target/release/UltraNet.bak-20261122T1959Z` (same sha256, same size) |
| Running after | sha256 `69aeddd94bccc863951b7126befec83c45f21a57db126bd2305d5d9cf6b7e441`, 25,750,552 B |
| Source | `cda51321f647f5e34dec1486b09d25b9b698c060`, clean tree |
| Built with | `rustc 1.97.0 (2d8144b78 2026-07-07)`, pinned by `rust-toolchain.toml` |
| Build | `cargo build --release --locked --bin UltraNet -j 4` → exit 0, 30.05 s |
| Restart | one planned `systemctl restart ultranet` at **20:00:28Z** |

### Why

Three things were still wrong after O-2, and only one of them was cosmetic:

1. The binary that O-2 installed was built from a **working tree**, not a commit, so nobody could
   tell from the repository what was running. That is now closed: the retirement, the authorization
   tests and the banner correction are commits `c28f174`, `186e213` and `cda5132`.
2. The 410 body had **never been observed over the wire**. It is now proven, and not by unit test
   alone — see below.
3. The startup banner still advertised both retired anchor routes as "Server-verified AppChain
   anchor" and "Development-only fixture anchor".

### What changed since the 18:55Z binary

Only the banner text, and the function that holds it:

```rust
fn appchain_route_banner() -> [&'static str; 3] {
    [
        "   POST /api/appchain/create - RETIRED (410): register by signing an AppChainRegister transaction",
        "   POST /api/appchain/{chain_id}/anchor - RETIRED (410): anchor by signing an AppChainAnchor transaction with the owner wallet and posting it to /api/appchain/tx",
        "   POST /api/appchain/{chain_id}/anchor/test - RETIRED (410): submit a signed AppChainAnchor through /api/appchain/tx",
    ]
}
```

No route, handler, middleware, fee, key, peer or genesis change. The 1,480-byte size difference is
the replaced string table and the banner loop. `configuration_tests::startup_banner_does_not_
advertise_retired_anchor_routes` fails if any line claims node-side verification or offers the
fixture route.

### How the provenance was confirmed

The 18:55Z binary was checked before it was replaced: the local `target/release/UltraNet` was
byte-identical to the deployed file (`5344ed0c…`), and `src/api.rs` had not been modified after the
build (mtime 17:16Z, build 18:51Z). The new commit `c28f174` captures exactly that file content, so
the previously unattributable binary now maps to a commit even though the new binary supersedes it.

### Verification

Before promotion, the **new artifact** was run locally on a throwaway database, its own P2P port and
a throwaway bearer token (nothing from production was read):

| Check | Result |
|---|---|
| `POST /api/appchain/1/anchor` — no credential | **401** |
| `POST /api/appchain/1/anchor` + credential | **410**, body: `Anchoring AppChain #1 from the node is retired. Only the chain owner can anchor it: sign an AppChainAnchor transaction with the owner wallet and post it to /api/appchain/tx` |
| `POST /api/appchain/1/anchor/test` + credential | **410**, body names `/api/appchain/tx` |
| `POST /api/appchain/tx`, empty body | 400 — route live |
| `GET /api/appchain/overview`, `/ledger` | 200 |
| banner, as printed | all three AppChain lines read `RETIRED (410)` |

After the restart, on production:

| Check | Result |
|---|---|
| service | `active`, MainPID **540014**, `NRestarts 0`, `ExecMainStartTimestamp` 20:00:28Z |
| running binary | `/proc/540014/exe` → installed path, sha256 `69aeddd9…` |
| listener | 127.0.0.1:8081 |
| `chain_length` / `epoch` / `auto_mined_blocks` | 16 / 15 / 0 — unchanged by the restart |
| `GET /api/appchain/ledger`, `/ledger/1`, `/overview` | 200, 200, 200 |
| `POST /api/appchain/tx`, empty body | 400 — live |
| retired routes, no credential | 401 (`/1/anchor`, `/1/anchor/test`, `/anchor`) |
| journal banner | `POST /api/appchain/{chain_id}/anchor - RETIRED (410): anchor by signing an AppChainAnchor transaction with the owner wallet and posting it to /api/appchain/tx` |
| panics / startup failures since restart | 0 |
| other services | `ultranet-records` active, `ultranet-dashboard` active — not restarted |

### Rollback

```sh
sudo systemctl stop ultranet
sudo cp -p /opt/ultranet/target/release/UltraNet.bak-20261122T1959Z /opt/ultranet/target/release/UltraNet
sudo chown ultranet:ultranet /opt/ultranet/target/release/UltraNet
sudo systemctl start ultranet
```

Two older rollbacks are still retained: `UltraNet.bak-20261122T1853Z` (26,113,336 B, `ad442a46…`,
the pre-O-2 binary) and `UltraNet.bak-20261025T0957Z` (25,167,088 B, `d8edb01e…`).

### Limitations

- The 410 was proven on a **local instance of the same artifact**, not on the production listener:
  production still answers 401 without a credential, and reaching the 410 there needs the operator
  bearer token, which was deliberately not read.
- The node prints two banners (a short one from the startup path and the long `📋 Endpoints:` list
  from the API server). Only the long one lists AppChain routes.
- `❌ libp2p connection closed … reason_kind=io` repeats about once a minute for the persistent peer
  `12D3KooWS3AHS46…`; it predates this release and is not accompanied by a reconnect storm or a
  restart.

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
| account `787e68b2…` — **not the faucet**, a stranded balance | `1,000.001000`, unchanged (no reachable key; see the run sheet) |
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
