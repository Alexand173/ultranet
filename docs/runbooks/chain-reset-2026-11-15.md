# Chain reset — 2026-11-15

A reset replaces the chain's state with a new genesis, keeping every identity that is not chain state.
This document is the procedure, the expected output of each step, and the check that proves the step
did what it claims. It has **not** been executed. Do not run it until the gates below are green.

- NODE_HOST: the production host. Steps marked **ENGINEER** require root and are yours to run; every
  other step is read-only and I can run or draft it.
- Maintenance window: 02:00–03:00 UTC.
- Node API: `http://127.0.0.1:8081` · Faucet API: `http://127.0.0.1:8090`

## What is being kept, and what is being destroyed

| Kept — must not change | Destroyed — recreated by genesis |
|---|---|
| Sovereign address `3b8ef38a…ee3c` and the three owner keys in `sovereign_keys.json` | Every block, every transaction, every nonce |
| Faucet address `fe0b9a9227…f1f8` and its signer credential | Every account balance, including the faucet float |
| `/etc/ultranet/*` and `/etc/ultranet-faucet/*` | The validator registry and the approval journal |
| `validator_dilithium5_key.json`, `p2p_identity.key` (see step 3 — they live **inside** the DB) | The AppChain ledger and its activation height |
| `/var/lib/ultranet-faucet/faucet.db` is preserved off-box, then replaced by an empty one | The AppChain `l3/` databases |

## Gates — none of these is optional

| Gate | State | Evidence required |
|---|---|---|
| **P1** restart changes no balance | **GREEN — certified 2026-11-15** | `cargo test --locked --lib -j 1 restart_replay -- --test-threads=1`: `restart_restores_every_balance` and `a_restart_reproduces_the_payouts_...` both pass. The replay now copies the account shards instead of re-deriving balances, so it no longer invents a 1,000-unit opening balance for every account it meets |
| **P2** a validator can be registered and becomes `reward_eligible` | **AMBER — tooling added, unexercised** | `src/bin/ultranet-propose.rs` now exists (added 2026-11-15) and compiles; it signs the version-2 envelope with the node's own identity, which no previous client could do. It has not yet been run against a live node, so step 9 is the first execution |
| **P3** faucet funded and auto-mining on | green (values known) | `FAUCET envelope ≥ 200.101000 $ULTRA`; `ULTRANET_AUTO_MINE_INTERVAL_SECONDS` set |

Constants this procedure relies on: block reward **500,000** base units (0.5), claim **100,000**
(0.1), faucet reserve **200,000,000** (200), faucet daily cap **10,000,000** (10), transfer fee
`max(1, amount ÷ 100)`, transaction freshness window **3600 s**, genesis allocation **1,000,000**.

---

## Step 0 — Freeze the identities (read-only)

Record what must not change, so it can be proven unchanged at the end.

```bash
sha256sum sovereign_keys.json > reset-identities.sha256
jq -r '.vault_address, .sovereign_threshold, (.owners[].address)' sovereign_keys.json
```

**Expected:** `3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c`, then `2`, then the
three owner addresses `f86ca985…`, `e6766eab…`, `cc30a94c…`.

**Check:** the file hash is recorded, the address is the sovereign address, threshold is 2. Also record
the faucet address printed by `sudo /opt/ultranet-faucet/ultranet-faucet check-config` — it must read
`fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8`.

## Step 1 — Baseline (read-only)

```bash
curl -fsS http://127.0.0.1:8081/api/stats | jq '.data | {chain_length, total_blocks, total_transactions}'
curl -fsS http://127.0.0.1:8081/api/account/3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c | jq '.data | {balance_ultra, nonce}'
curl -fsS http://127.0.0.1:8081/api/account/fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8 | jq '.data | {balance_ultra, nonce}'
sudo sqlite3 -readonly /var/lib/ultranet-faucet/faucet.db "SELECT status, COUNT(*) FROM claims GROUP BY status;"
```

**Expected:** `chain_length` ≈ 14, sovereign ≈ `998,990.000000`, faucet ≈ `1,209.88`, and a claim
count by status. Paste all four outputs into the attestation table (step 13).

**Check:** the numbers are recorded before anything is destroyed. Without them, "the reset worked"
has nothing to be compared against.

## Step 2 — Freeze the faucet (ENGINEER)

```bash
curl -fsS -X POST -H "Authorization: Bearer $(sudo cat /etc/ultranet-faucet/secrets/faucet-operator.token)" http://127.0.0.1:8090/internal/disable
sudo systemctl stop ultranet-faucet.service
curl -fsS http://127.0.0.1:8090/api/faucet/status
```

**Expected:** the `curl` to `/internal/disable` returns success; the final status call fails because
the service is stopped (that is the check). If it still answers, the stop did not take.

**Check:** no claim can be accepted while the chain is being replaced. A claim signed against the old
chain can never confirm and would disable intake on its own.

## Step 3 — Back up, including the identities inside the database (ENGINEER)

```bash
BACKUP_ROOT="/var/backups/ultranet/reset-$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -o root -g root -m 0700 "$BACKUP_ROOT"
sudo systemctl stop ultranet.service
sudo tar --xattrs --acls --numeric-owner -C /var/lib -czf "$BACKUP_ROOT/ultranet-db.tar.gz" ultranet
sudo install -o root -g root -m 0600 /var/lib/ultranet/validator_dilithium5_key.json "$BACKUP_ROOT/"
sudo install -o root -g root -m 0600 /var/lib/ultranet/p2p_identity.key "$BACKUP_ROOT/"
sudo tar -C /var/lib -czf "$BACKUP_ROOT/faucet-db.tar.gz" ultranet-faucet
sudo tar -C /etc -czf "$BACKUP_ROOT/etc.tar.gz" ultranet ultranet-faucet
sudo sha256sum "$BACKUP_ROOT"/*.tar.gz "$BACKUP_ROOT"/*.json | sudo tee "$BACKUP_ROOT/SHA256SUMS"
```

**Expected:** five artefacts plus `SHA256SUMS`. `tar -tzf "$BACKUP_ROOT/ultranet-db.tar.gz" | head`
lists `ultranet/db`, `ultranet/conf`, `ultranet/blobs/`.

**Check:** the two identity files exist **outside** the tarball as loose files, readable only by root.
They are the reason this step exists: both live inside `ULTRANET_DB_PATH` and a wipe destroys them —
which changes the validator address and the peer ID.

## Step 4 — Confirm the node is down (ENGINEER)

```bash
ss -ltnp | grep -c 8081 || echo "port 8081 closed"
systemctl is-active ultranet.service
```

**Expected:** `port 8081 closed`, and `inactive` (or `failed`).

**Check:** nothing can write to sled while it is being deleted.

## Step 5 — Wipe the chain state (ENGINEER, destructive)

Delete the **contents** of the database directory. Not the directory itself, and nothing outside it.

```bash
sudo find /var/lib/ultranet -mindepth 1 -maxdepth 1 -exec rm -rf {} +
sudo find /var/lib/ultranet -mindepth 1 | wc -l   # expected: 0
sudo du -sh /var/lib/ultranet                      # expected: 0
```

**Expected:** both reads return 0.

**Check:** `/etc/ultranet/ultranet.env`, `/opt/ultranet` and the faucet's directories are untouched —
`ls /etc/ultranet` still shows `ultranet.env`. Deleting only the `blocks` tree is **not** sufficient:
a non-empty `move_modules` skips the genesis mint, and the state shards keep the old balances.

## Step 6 — Restore the two identities (ENGINEER)

```bash
sudo install -o ultranet -g ultranet -m 0600 "$BACKUP_ROOT/validator_dilithium5_key.json" /var/lib/ultranet/
sudo install -o ultranet -g ultranet -m 0600 "$BACKUP_ROOT/p2p_identity.key" /var/lib/ultranet/
sudo -u ultranet stat -c '%a %U:%G %n' /var/lib/ultranet/validator_dilithium5_key.json /var/lib/ultranet/p2p_identity.key
```

**Expected:** `600 ultranet:ultranet` for both. The node refuses a key file that is world-readable.

**Check:** the validator address printed in the journal at step 7 equals the one recorded at step 0, and
the peer ID is unchanged — so `ULTRANET_PERSISTENT_PEERS` on every other node still resolves.

## Step 7 — Start the node and verify genesis (ENGINEER to start, then read-only)

```bash
sudo systemctl start ultranet.service
sleep 20
sudo journalctl -u ultranet.service --since '1 minute ago' --no-pager | tail -40
curl -fsS http://127.0.0.1:8081/api/stats | jq '.data.chain_length'
curl -fsS http://127.0.0.1:8081/api/block/0 | jq -r '.data.hash'
python3 -c 'import hashlib;print(hashlib.sha3_256(b"ULTRA BLOCKCHAIN 4.0 - GENESIS - DETERMINISTIC").hexdigest())'
curl -fsS http://127.0.0.1:8081/api/validate | jq '.data'
```

**Expected:** `chain_length` is `"1"`; the block-0 hash equals the printed SHA3-256; `/api/validate`
returns `{"valid":true}`; the journal shows `🔑 Validator Dilithium-5 identity ready`, the Move VM
genesis banner, and `✅ Blockchain initialized!`.

**Check:** every node that joins this chain must produce the **same** block-0 hash. The genesis
timestamp is `now` and the five placeholder keys are random per node, so two independent nodes started
from an empty database will **not** agree — they must be fed the same database copy, not each started
fresh. Confirm `ULTRANET_APPCHAIN_ACTIVATION_HEIGHT` in `/etc/ultranet/ultranet.env` now; unset or
non-numeric silently means *disabled*, and block indices have restarted at 0.

## Step 8 — Verify the new allocations (read-only)

```bash
curl -fsS http://127.0.0.1:8081/api/account/3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c | jq '.data | {balance_ultra, nonce}'
curl -fsS http://127.0.0.1:8081/api/account/fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8 | jq '.data | {balance_ultra, nonce}'
curl -fsS http://127.0.0.1:8081/api/stats | jq '.data.validator_count'
```

**Expected:** sovereign `1,000,000.000000`, nonce `0`; faucet `0.000000`, nonce `0`;
`validator_count` `"5"` — the placeholders.

**Check:** five validators exist and **none may be paid**. Confirm via the snapshot:
`ULTRANET_ENABLE_LOCAL_VALIDATOR_SNAPSHOT=true` (loopback + admin bearer required), then
`curl -fsS -H "Authorization: Bearer $ULTRANET_ADMIN_TOKEN" http://127.0.0.1:8081/api/local/validator-snapshot | jq '.data.persisted_validators | {total, active_weight}'`.
Also confirm no account exists at a key longer than 64 characters — that address is the old defect:
`curl -fsS http://127.0.0.1:8081/api/account/$(printf '5a%.0s' {1..48}) | jq '.data.balance_ultra'` → `0.000000`.

## Step 9 — Register the operator's validator (gate P2)

`POST /api/governance/propose` needs a version-2 envelope signed by the applicant's Dilithium key.
The Android app and the browser wallet can build one but cannot sign with a key held on a server, so
`ultranet-propose` does it on the node, and `ultranet-approve` completes the 2-of-3 approval **offline,
on the machine that holds `sovereign_keys.json`** — never on the node.

```bash
# 1. apply, on the node, signed by the validator identity restored in step 6
sudo -u ultranet /opt/ultranet/target/release/ultranet-propose \
  --db /var/lib/ultranet --metadata "operator validator" --send

# 2. proposal hash, then the offline approval (offline machine, never the node)
curl -fsS http://127.0.0.1:8081/api/governance/proposals | jq '.data'
ultranet-approve owners --keys /offline/sovereign_keys.json --pretty --output owners.json
ultranet-approve prepare --api-base-url http://127.0.0.1:8081 \
  --proposal-hash <hash> --pretty --output draft.json
ultranet-approve sign --keys /offline/sovereign_keys.json --request draft.json \
  --owner-index 0 --output sig-0.json
ultranet-approve sign --keys /offline/sovereign_keys.json --request draft.json \
  --owner-index 1 --output sig-1.json
ultranet-approve combine --request draft.json --signature sig-0.json --signature sig-1.json \
  --authorized-owners owners.json --output approval.json
ultranet-approve submit --request approval.json --api-base-url http://127.0.0.1:8081
```

**Expected:** `ultranet-propose` prints the applicant address, the **payout address** (the SHA3 hash of
the key — this is where rewards will land), the nonce, the timestamp and the envelope digest, then the
node's acceptance. `prepare` fetches the sovereign nonce and builds a draft; two owner signatures
combine into one approval; `submit` returns success and the approval is mined.

**Check:** `curl -fsS "http://127.0.0.1:8081/api/governance/approvals?limit=1" | jq '.data'` shows the
journal entry with `activated_validator.reward_eligible: true`, and the snapshot's `active_weight`
includes the new key. `sha256sum sovereign_keys.json` is unchanged — approval signs with the keys, it
never rewrites the file. Until this completes, **every block is produced and paid to nobody**: the
reward is not created and a warning is logged.

If the timestamp is rejected as too old, the node's clock is behind this host; `ultranet-propose` reads
the node's clock, so re-run it rather than editing the envelope.

**Expected:** the proposal appears, the approval is mined, and the journal entry's
`activated_validator.reward_eligible` is `true`.

**Check:** the registry entry is the key whose hash is the payout address. Until this step completes,
**every block is produced and paid to nobody** — the reward is not created and a warning is logged.

## Step 10 — Fund the faucet (ENGINEER)

Faucet address and key do not change; only its balance does.

```bash
./venv/bin/python send_ultra.py --amount 201 --check-file /tmp/faucet-funding.json
cargo run --locked --example verify_sig_artifact -- /tmp/faucet-funding.json
./venv/bin/python send_ultra.py --amount 201 --send
curl -fsS http://127.0.0.1:8081/api/account/fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8 | jq '.data | {balance_ultra, nonce}'
```

**Expected:** dry run prints the digest, the fee `2.010000` and a total debit of `203.010000`; the
artifact verifies; the faucet reads `201.000000` at nonce `1`; the sovereign reads `796.990000`.

**Check:** `201 > 200.101` (the reserve plus one claim plus its fee), so the first claim does not trip
`BudgetExhausted`. Do not reuse a `transfer.json` or a `--check-file` artefact from an earlier run: the
nonce and timestamp are read fresh each run, and a stale timestamp falls outside the 3600 s window.
`sha256sum sovereign_keys.json` must still match step 0 — signing a transfer does not modify the file.

## Step 11 — Start the faucet on an empty database (ENGINEER)

```bash
sudo systemctl stop ultranet-faucet.service
sudo install -d -o ultranet-faucet -g ultranet-faucet -m 0700 /var/lib/ultranet-faucet
sudo find /var/lib/ultranet-faucet -type f -name 'faucet.db*' -delete
sudo systemctl start ultranet-faucet.service
curl -fsS http://127.0.0.1:8090/api/faucet/status
curl -fsS -H "Authorization: Bearer $(sudo cat /etc/ultranet-faucet/secrets/faucet-operator.token)" http://127.0.0.1:8090/internal/health
curl -fsS -H "Authorization: Bearer $(sudo cat /etc/ultranet-faucet/secrets/faucet-operator.token)" http://127.0.0.1:8090/internal/status | jq '.data | {enabled, kill_switch_reason, faucet_address, last_observed_nonce}'
```

**Expected:** status `enabled:false` (the stored kill switch survives a restart, by design),
`signer_loaded:true`, `faucet_address` = `fe0b9a92…`, and no `kill_switch_reason` from the old chain.

**Check:** the identity gate passed — the signer derives the same address as at step 0. Then enable
intake and confirm:

```bash
curl -fsS -X POST -H "Authorization: Bearer <operator token>" http://127.0.0.1:8090/internal/enable
curl -fsS http://127.0.0.1:8090/api/faucet/status   # availability: "available"
```

## Step 12 — One canary claim, watched

Claim one address from a real browser (the Turnstile widget cannot be satisfied headlessly), then:

```bash
curl -fsS http://127.0.0.1:8090/api/faucet/claims/<claim_id> | jq '.data | {status, amount_ultra, transaction_hash}'
curl -fsS http://127.0.0.1:8081/api/account/<claim address> | jq '.data.balance_ultra'
sudo journalctl -u ultranet-faucet.service --since '10 minutes ago' --no-pager | grep -E "payout|claim"
```

**Expected:** `status` moves `queued → pending → confirmed` within about one auto-mine interval; the
claim address reads `0.100000`; the faucet debited `0.1 + fee`.

**Check:** `confirmed` **and** the block exists. A claim that stays `pending` past 900 s disables intake
— that is a gate failure, not a slow block, and it means auto-mining is off.

## Step 13 — Attestation

Append to this file and commit: the step-0 hashes (unchanged), the step-1 baseline, and the final table.

```
sovereign  3b8ef38a…  1,000,000.000000  nonce 0
faucet     fe0b9a92…        201.000000  nonce 1   (unchanged address)
height                        1 → 2 → …            chain_length grows
reward per block            500,000 base units     at the validator's SHA3 address
validator                  reward_eligible: true   registered, not seeded
phantom accounts           none
/api/validate              {"valid":true}
identity hashes            unchanged from step 0
```

---

## Rollback

Restore `$BACKUP_ROOT/ultranet-db.tar.gz` and the identities, restart, and the old chain returns —
**including the phantom reward accounts and the wrong balances it accumulated**, because that build
wrote them. Rollback is only coherent **before step 10**: after the funding transfer, the 201 $ULTRA
exists on the new chain only.

## Known residuals after a successful reset

1. Findings **B, C, D, E, F, G, H** are unchanged. The new chain still does not enforce its own block
   reward, still mints fees for `MoveCall`, and still trusts the producer's `validator_set`.
2. The five placeholder validators are recreated with new random keys on every fresh database. They
   are unpayable, but they exist and inflate `validator_count` to 6 after registration.
3. `genesis.json` remains documentation only; the node does not read it. Editing it changes nothing.
4. Balances are now trusted from the account shards rather than re-derived from blocks, so a crash
   **between** `update_state` and `save_block` leaves the shards one block ahead of the chain and a
   restart no longer repairs it. Re-mine the affected block or restore the backup. This is the price of
   the fix below, and it is smaller than the defect it replaces.

### Closed on 2026-11-15

Finding J — a restart replaced every non-sovereign balance with 1,000 minus fees — is fixed. Replay
copies the account shards, which `update_state` writes on every transition, instead of re-deriving
balances from transactions. The drill and the gate both pass, so **P1 is certified** and this runbook is
executable once P2 is exercised.
