# AppChain activation height — operator runbook

The node build that carries the AppChain ledger (`src/appchain/ledger.rs`) accepts AppChain
transactions only above a block height that operators set by hand. This runbook is how that height
gets set once, agreed by every validator, and verified before the first registration.

Without it the build is inert: registration and anchoring are refused, and the public ledger endpoint
truthfully reports that the feature is not active.

## 1. What the flag does, and what it does not do

`ULTRANET_APPCHAIN_ACTIVATION_HEIGHT` is read once per process and compared against the **next block
index** (`chain.len()`) on every mempool admission:

| Property | Behaviour |
|---|---|
| Variable unset | `activation_height()` returns `u64::MAX` — every AppChain transaction is refused with `AppChain transactions are not active on this network yet` |
| Set to H | Transactions are admitted from the moment the node's next block index reaches H |
| Enforcement point | **Admission only** (`validate_transaction`), not block validation |
| Effect of a mismatch between nodes | Not a chain split. A node with a lower H admits the transaction into its mempool and may mine it into a block; a node with a higher H refuses to admit it locally but still applies it when that block arrives, because the envelope and signature are valid |

That last row is the important one to state plainly: this flag coordinates *when the feature becomes
usable*, it is **not** a consensus rule. A node set lower than the others cannot be prevented from
accepting an early registration — it will simply produce a block the rest of the network validates and
applies. Choose one value, publish it, and set it everywhere before anyone submits.

## 2. Pre-flight (read-only, before touching anything)

Run on each node and record the output beside the agreed height.

```bash
# 1. Current height on this node - the flag must be above it.
curl -s https://api.ultranetwork.cc/api/stats | grep -o '"chain_length":"[0-9]*"'

# 2. Is the ledger build even present? 404 means this binary predates the feature.
curl -s -o /dev/null -w '%{http_code}\n' https://api.ultranetwork.cc/api/appchain/ledger

# 3. What the node currently reports - activation_height is null while disabled.
curl -s https://api.ultranetwork.cc/api/appchain/ledger

# 4. The binary on disk, so the value is recorded against a known build.
sha256sum /opt/ultranet/target/release/UltraNet
systemctl show ultranet -p ExecMainStartTimestamp -p ActiveState
```

Step 2 returning `200` and step 3 reporting `"activation_height": null` is the state this runbook
takes over from. A `404` means the install in §4 has not happened yet.

## 3. Choose the height

The value is a **block index**, not a timestamp, and blocks here are produced by the auto-miner or by
`POST /api/mine`, so elapsed time is not a reliable conversion. Pick:

```
agreed_height = current_chain_length + 100
```

One hundred blocks is the smallest margin that still absorbs the time between the first operator
setting the variable and the last one restarting. Record the agreed value and every node's pre-flight
output in the change record:

| Node | Chain length before | Agreed height | New binary SHA-256 | Restarted (UTC) |
|---|---|---|---|---|
| | | | | |

## 4. Apply

The variable belongs in the environment file, not the unit: `deploy/ultranet.service` reads
`EnvironmentFile=-/etc/ultranet/ultranet.env`, and a `Environment=` line in the unit would silently
shadow it.

```bash
# 1. Append the agreed height (idempotent: replace an existing line instead of adding a second).
sudo sed -i '/^ULTRANET_APPCHAIN_ACTIVATION_HEIGHT=/d' /etc/ultranet/ultranet.env
echo 'ULTRANET_APPCHAIN_ACTIVATION_HEIGHT=<agreed_height>' | sudo tee -a /etc/ultranet/ultranet.env
sudo chown root:ultranet /etc/ultranet/ultranet.env
sudo chmod 0640 /etc/ultranet/ultranet.env

# 2. Install the ledger binary, then restart. Both, in this order: the running
#    build ignores the variable, so restarting before the install does nothing.
sudo install -m 0755 target/release/UltraNet /opt/ultranet/target/release/UltraNet
sudo systemctl restart ultranet
```

For a single-node network this is one restart. For several validators, every operator applies the same
height; the feature becomes usable when the highest-H node is restarted and the chain passes it.

## 5. Verify

```bash
# 1. Service is up and not restart-looping.
systemctl is-active ultranet
systemctl show ultranet -p NRestarts

# 2. The height is echoed back - this must equal the agreed value on every node.
curl -s https://api.ultranetwork.cc/api/appchain/ledger | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["terms"]["activation_height"])'

# 3. The registry starts empty and reports the fixed fees.
curl -s https://api.ultranetwork.cc/api/appchain/ledger | python3 -c \
  'import json,sys; t=json.load(sys.stdin)["terms"]; print(t["register_fee_base_units"], t["anchor_fee_base_units"], t["version"])'

# 4. Normal block production is unaffected.
curl -s https://api.ultranetwork.cc/api/stats
```

Expected: `activation_height` is the agreed integer (not `null`), fees read `100000` and `1000`,
version reads `5`, and the chain keeps producing blocks. A registration submitted **below** the agreed
height is refused with `AppChain transactions are not active on this network yet` — that refusal, seen
before the height, is the flag working.

## 6. Rollback

Raise the height, or remove the line entirely, then restart:

```bash
sudo sed -i '/^ULTRANET_APPCHAIN_ACTIVATION_HEIGHT=/d' /etc/ultranet/ultranet.env
sudo systemctl restart ultranet
```

This stops **new** admissions. It does not remove anything already mined: a registration or anchor
that reached a block is in the ledger trie and in the state root, and reverting it is a state migration,
not a configuration change. Nothing in this runbook deletes ledger state.

## 7. Failure modes

| Symptom | Cause | Action |
|---|---|---|
| `activation_height: null` after restart | Variable missing from `/etc/ultranet/ultranet.env`, or set in the unit and shadowed | Re-apply §4, confirm with §5 step 2 |
| Registration refused although the height has passed | Submitted against a node whose `chain.len()` is still below the value | Wait for the height, or resend to another node |
| Registration accepted by the API but never appears in the ledger | Accepted into the mempool, not yet mined — anchors and registrations need a block | Confirm the auto-miner interval, or `POST /api/mine` |
| Nodes disagree on `terms.activation_height` | Height applied to only some nodes | Treat as a configuration incident: raise the odd node to the agreed value and restart before the first registration |
| `404` on `/api/appchain/ledger` | Binary predates the ledger build | Install per §4, then verify |

## 8. After the first registration

Once chain #1 exists, anchoring is owner-signed and the fee comes from the owner's L1 balance —
`0.001` per anchor, `0.1` for a registration. Keep at least a year of anchors funded: at two anchors a
day that is `0.73` on top of the registration. The ledger endpoint reports the running totals per
chain, and `GET /api/appchain/ledger/{id}/anchors` is the owner's audit trail.
