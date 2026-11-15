# Chain reset — execution attestation, 2026-11-15

Executed the reset described in `docs/runbooks/chain-reset-2026-11-15.md` on the production
host (`production-faucet`, 167.233.161.115). Steps 1–12 ran; step 13 is this record.

## Gate that had to be cleared first

The deployed node binary predated the payout fixes. Before the wipe, `/opt/ultranet/target/release/UltraNet`
was the **2026-10-31** build (`sha256 f5abcdb3…`), and the build tree on the host was at `fdea3a8`.
A reset onto that binary would have reproduced the phantom-address rewards, the unpayable-reward
defect and the replay fiction. `1d9eeeb` was therefore built on the host and installed.

| Item | Value |
|---|---|
| Source revision | `1d9eeeb` |
| Node binary | `sha256 7a8bc5a539d57e266127736852201fb0ae079328892fd53abec84f9a82fa5066` |
| Faucet binary | `sha256 befd96478716f96fb8b601f0453388cfe4cc8e8b0e7ad9c2dc1e100d91dcf8ea` |
| Previous binaries | kept as `*.pre-reset-20261115T124645Z` |

## Backup (step 3)

`/root/backups/reset-20261115T124321Z` — 4.8 GB, byte-identical to the source tree
(`du -sb` 5041013961 both sides), with `pre-reset-stats.json`, `ultranet.env`, the two unit
files, and the faucet state. The faucet's claim ledger was additionally kept under `faucet/`.

Validator identity was backed out **before** the wipe and restored **before** the first start:

| File | sha256 (identical before and after) |
|---|---|
| `validator_dilithium5_key.json` | `52558195e7debde28c4fa8fa716bbb1ec650bde32178f74efea9e0995d957933` |
| `p2p_identity.key` | `0ef194f2c69e8976f48010d5dd20194be46464815de9d424175f185cea1a0139` |

Operator identity is preserved: validator address `eb2e35cc…09fc`, peer id `12D3KooWS3AH…`.

## Post-reset chain

| Check | Result |
|---|---|
| Genesis | `chain_length 1`, `genesis_time 1794746830`, 5 bootstrap validators |
| Sovereign | `1,000,000.000000` exactly |
| Old chain re-imported? | No — the persistent peer requested blocks 2–13 and they were rejected; the fork held |

## Funding (step 10)

`MAX_TRANSFER_AMOUNT` is `1,000,000,000` base units (1,000 $ULTRA), so the requested 1,200
cannot move in one transaction. It was sent as two, both signed 2-of-3 by the sovereign:

| Amount | Fee | Tx |
|---|---|---|
| 1,000.000000 | 10.000000 | `576e72c7d1360aa506aaf28e93f51a209582cc8614e342d4f549d2b49128def3` |
| 200.000000 | 2.000000 | `bb9e5a0a011715e51fc63bfeceaedb762207cab94c4010622d7f57ec5f9e71c3` |

Sovereign `998,788.000000`; faucet `1,200.000000`.

## Validator registration (step 9)

Proposal `447b584cbb48f0bcf52345dea3ac2c6e42c206b60263541a6c34239d864b2533`, signed by the node
identity, payout address `eb2e35cc…09fc` (= `SHA3-256` of the validator key, not a placeholder).
Approved offline by sovereign owners 0 and 1, submitted, activated. `validator_count` 5 → 6.
Rewards began crediting the operator address immediately (13.5 $ULTRA by 13:00 UTC) — the
phantom-address defect is gone on a live chain.

## Faucet (step 11–12)

State wiped and service re-enabled with the new binary. `FAUCET_ENABLED=true`,
`claim_amount_ultra 0.100000`, daily debit cap 10 $ULTRA, reserve 200, cooldown 86400.

Canary claim `6e30cdb7b93b45faeb710244115e9722` → confirmed on chain: the canary address
`357e4aab…1385` received **0.100000** $ULTRA, faucet `1,199.899000`, `chain_length 6`, pool empty.

## Residual findings (not fixed here)

1. **Faucet page copy is wrong.** `deploy/faucet-site/index.html` hardcodes `1.000000` in the
   heading and the receipt (and in the meta description), while `app.js#loadPolicy()` updates only
   the button label. After the 0.1 policy change the page therefore advertises 1.0 and the
   receipt repeats it, even though 0.1 is paid. Line 28 of `app.js` even warns about this case.
2. `MAX_TRANSFER_AMOUNT` capped the funding transfer at 1,000 $ULTRA per transaction.
3. `ULTRANET_PERSISTENT_PEERS` still names the old-chain peer `12D3KooWS3AH…` at 109.245.57.92;
   it cannot serve this chain and reconnects continuously.
