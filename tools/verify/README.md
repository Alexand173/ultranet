# Verifying the Records Service

These scripts are the evidence behind the Records Service's safety claims. They are meant to be re-run by a reviewer.

| Script | What it does | Time | Needs |
|---|---|---|---|
| `records-rehearsal.sh` | Runs the **real node code** (`records-rehearsal-node`) on a scratch chain with the **real `appchain-cli`**, a throwaway owner key and the real `ultranet-records`, and checks every assumption the service depends on: pending transactions, the pending-aware nonce, the address history, the anchors index, a `kill -9` between acceptance and saving the hash, registry days, pending claims, a restored database. `REHEARSAL_HISTORY_LIMIT=1` also fills the owner's history to the node's 100-entry limit. | 3-8 min | ~5 GB free memory (the node holds FHE keys) |
| `records-mutations.sh` | Breaks one safety rule at a time in the source and requires the unit tests to fail. Restores the source after each. | ~20 min | a clean tree |
| `peer_post.py` | Signs a request to `/internal/v1/anchor` independently of the Rust code (the format in `src/games/peer.rs`). | | |
| `mkday.py` | Puts a result on a past day in a *disposable* Records database so that day can be closed now. | | |

The rehearsal touches nothing real. `records-rehearsal-node` refuses to start unless it is told `--disposable`, binds
only to `127.0.0.1`, will not use port 8081, requires a new database directory under the temp directory, and starts no
peer networking. It uses a throwaway passphrase and a throwaway key that never leave the scratch directory, which the
script leaves in place and prints; remove it yourself.

What it cannot show: consensus, peers, real value, or production. It shows how the node behaves for the one thing the
Records Service asks of it. The unit tests (`cargo test --lib games:: -- --test-threads=1`) cover everything the
rehearsal cannot reach (a failing ledger, a late index, a restored backup with a lagging index), using a fake ledger
that has been made to behave like the real one where the rehearsal found a difference.
