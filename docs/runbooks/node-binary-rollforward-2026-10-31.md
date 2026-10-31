# Node roll-forward to `61cec4b`, and faucet intake enabled — 2026-10-31

Performed on `ubuntu-4gb-nbg1-2` (167.233.161.115) between 12:19 and 12:33 UTC. Purpose: get the
transaction-freshness fix onto the running node, and open the faucet intake now that the address holds
a float.

## Why the binary had to be rebuilt

`validate_transaction` compared timestamps with unsigned arithmetic (`now - tx.timestamp > 3600`), so a
timestamp inside the 60 s future allowance wrapped to ~1.8e19 and was reported as
*"Transaction is too old!"*. The node clock runs ~21 s behind the signing host, so the 202 $ULTRA
funding was refused before `61cec4b` replaced the comparison with `check_transaction_freshness`.

The host had no source tree (`/opt/ultranet` holds only `target/`, `ultranet_db` and releases), and its
glibc is **2.39** while the development host is 2.39+ — a locally built binary would not have loaded.
The node was therefore built on the server itself:

```bash
git clone --depth 1 --branch main https://github.com/Alexand173/ultranet.git /root/build-ultranet
cd /root/build-ultranet && /root/.cargo/bin/cargo build --release --locked   # 9m16s, 4 cores
```

`protoc` is not required; `cc`, `clang`, `pkg-config` and `make` are present. Both `UltraNet` and
`ultranet-faucet` come out of that single build, which is what the "same source revision" rule in
`deploy/README.md` asks for.

## What changed on the host

| Item | Value |
| --- | --- |
| Source revision | `61cec4b21adfe7347099cf18b513c11bcfcfd76a` |
| Installed binary | `/opt/ultranet/target/release/UltraNet`, sha256 `62b09fdb93c8fcdb…`, `ultranet:ultranet` 0755 |
| Previous binary kept | `/root/UltraNet.pre-61cec4b.bin`, sha256 `e31cc57983b36ac0…` |
| Faucet env backup | `/root/faucet.env.pre-enable-20261031T122957Z` |
| Faucet state backup | `/var/backups/ultranet-faucet/faucet.db*` (sha256 `932df3b0dadbd012…`) |

Roll-forward is safe on state: no commit since the Oct 25 build touches storage, genesis, migrations or
the state trie. The only node-runtime delta is the freshness check; the rest is website, Android, docs
and tooling.

## Verification

- `systemctl is-active ultranet` → `active`, `NRestarts` 0.
- Startup re-executed every block and logged `✅ Merkle root OK` and `✅ State Root (MPT) verified by
  re-execution` for blocks 0–3.
- `/api/validate` → `{"valid": true}`; `chain_length` stayed 4; sovereign `999,795.980000`, nonce 5;
  block 3 hash unchanged (`8a72b008…`).
- **Fix proven on the live node**: a 1-base-unit transfer signed with a timestamp **21 s in the future**
  was accepted (`7d3f6b02…`), which the previous binary rejects as "too old". It was mined into block 4.
- P2P reconnected to the persistent peer after restart.

## Faucet intake

`FAUCET_ENABLED=true` in `/etc/ultranet-faucet/faucet.env`, service restarted. `GET
https://faucet.ultranetwork.cc/api/faucet/status` reports `enabled: true`, `availability: available`.
A claim with an invalid Turnstile token is refused with HTTP 400, so the CAPTCHA gate is enforced.
The Caddy `faucet.ultranetwork.cc` block publishes only the create and read routes and answers 404 for
everything else, and `FAUCET_ADDRESS` already names the funded `fe0b9a92…` address.

## Open items found during this deploy

1. **The faucet binary is still the 2026-08-30 build.** Its embedded default address is the stranded
   `787e68b2…`; the env file overrides it, so behaviour is correct, but the "node and faucet from the
   same revision" rule is not yet met. `ultranet-faucet` from this build is ready in
   `/root/build-ultranet/target/release/`.
2. **Faucet balance reads `202.001001` while chain history explains `202.000000`.** The extra `1000`
   base units appear after the restart, and `get_unadjusted_balance` falls through to the Move
   `UltraCoin` resource when an account is absent from the rebuilt map, so the rebuilt map now
   materialises a `1000`-unit resource that the previous in-memory state did not. Same chain, two
   different balances: worth its own investigation before claims rely on it.
3. **Faucet shutdown hangs.** `systemctl restart` timed out at `stop-sigterm` and needed SIGKILL, the
   service having consumed 1h49m CPU. Shutdown should be fixed before it is restarted under pressure.
4. **`ultranet-memory-monitor.service` is inactive**, so the monitoring gate in
   `docs/FAUCET_OPERATIONS.md` is not satisfied even though intake is now enabled.
5. **The mempool still holds 3 stale invalid entries.** Mining skips them, so they neither block a
   block nor a claim, but they keep `mempool_size` from reaching 0.
