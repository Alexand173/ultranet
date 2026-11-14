# Faucet binary roll-forward to `fdea3a8` — 2026-11-14

Performed on `ubuntu-4gb-nbg1-2` (167.233.161.115) at ~11:43 UTC. Purpose: get the relaxed
per-source claim limits onto the running faucet, and the 0.1 $ULTRA policy defaults onto a binary whose
cap enforcement matches the repository.

## Pre-flight (read-only, before touching anything)

| Check | Result |
|---|---|
| Old binary | sha256 `c03c437fa6a184d9c4243035e2e9e0b2bb62e73c208f176b5f62fd9548ed4edf`, dated 2026-10-31 |
| State DB `PRAGMA user_version` | `1` — matches `SCHEMA_VERSION`, so the new binary will open it |
| Column parity | `claims`, `payouts`, `service_state`, `abuse_buckets`, `budget_windows` all match the current `CREATE TABLE` statements — **no column drift**, so no `no such column` failure at claim time |
| `service_state` identity | `enabled=1`, `kill_switch_reason=NULL`, `faucet_address` = `signer_key_id` = `fe0b9a92…` — the identity gate in `FaucetService::new` passes |
| Claims on record | 2, both `confirmed` |
| Disk | 32 GB free on `/` |
| Capability strings | old build **did** contain `FAUCET_MAX_CLAIMS` (cap enforced); did **not** contain `FAUCET_IP_CLAIMS_PER_DAY` |

The last row is why this roll-forward mattered: the env file already set `FAUCET_IP_CLAIMS_PER_DAY=25`,
but the old build hard-coded 3 per IP and could not read it.

## Change

| Item | Value |
| --- | --- |
| Source revision | `fdea3a8` (built on the host from `origin/main`) |
| New binary | sha256 `c95ff4945f0ccf8cee2c41ee8645c6bd2f0aedcc8ba00a6b1ae8a92d664aa353` |
| Previous binary kept | `/root/faucet-binary.pre-fdea3a8.20261114T114302Z` |
| State DB backup | `/var/backups/ultranet-faucet/faucet.db.pre-fdea3a8.20261114T114302Z` (+ `-wal`) |
| Env backup | `/root/faucet.env.pre-0.1-20261114T112650Z` |

`check-config` under the live environment printed the expected values (amount `100000`, intake
enabled, address `fe0b9a92…`) before the restart.

## Verification

- `systemctl is-active ultranet-faucet` → `active`; the restart did not hit `TimeoutStopSec`.
- `GET /api/faucet/status` → `enabled: true`, `availability: available`,
  `claim_amount_ultra: "0.100000"`, `cooldown_seconds: 86400`.
- `strings` on the new binary contains `FAUCET_IP_CLAIMS_PER_DAY`, so 25 per IP and 250 per subnet
  are now effective rather than dormant.
- Public check through the proxy returns the same 0.100000 figure.
- Faucet balance unchanged at `1,209.981001` across the restart.

## Claim page copy

The hosted page had drifted from the policy: its title and submit label said "1 $ULTRA", and one
confirmation message still did. `deploy/faucet-site/app.js` now reads `claim_amount_ultra` from the
status endpoint, and `index.html` is tracked in the repository for the first time (it existed only on
the host). Previous copy kept at `/root/faucet-app.js.bak-20261114T1152Z`.

## Residual

- `FAUCET_MAX_CLAIMS=10000` is now enforced by a build whose cap code matches the repository.
- The app's "Limited to the first 10,000 users." copy is a claim of policy, not a count read from the
  server; if the cap changes, the app needs a release or the copy must be dropped.
