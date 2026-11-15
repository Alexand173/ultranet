# AppChain ledger activation — 2026-11-15

Phase 1 of the UltraGrid plan (`plan-ultragrid-appchain-20261115_140124.md`). This records the
activation-height change only: no AppChain exists yet, and none was created by this work.

## What changed

| | Before | After |
|---|---|---|
| `ULTRANET_APPCHAIN_ACTIVATION_HEIGHT` | absent (feature inert, `u64::MAX`) | `10` |
| `/api/appchain/ledger` → `activation_height` | `null` | `10` |
| Chain length | `6` | `10` |
| Chains registered | 0 | 0 |
| Node binary | `sha256 7a8bc5a5…5066` | unchanged |
| Env file | — | `640 root:ultranet`, line 15 |

Applied per `docs/runbooks/appchain-activation.md`; node restarted **2026-11-15 14:26:22 UTC**.
Backup of the previous env file: `/root/backups/ultranet.env.pre-appchain-<timestamp>`.

## The part that is not obvious: the height had to be reached, not just set

`check_appchain_activation` compares against `self.chain.len()` — the **next** block index
(`src/lib.rs:2551-2557`), not the last one. Setting the variable alone changes nothing while the chain
sits below it, and the chain does not advance on its own: the auto-miner returns early when the pool
is empty (`src/auto_miner.rs:131-134`). An activation was therefore only real once four more blocks
existed.

Two operational notes came out of doing it:

1. **Four transfers did not produce four blocks.** They were all sealed into one block, because the miner
   takes whatever is pending when the tick fires. The chain went 6 → 7, not 6 → 10. Space submissions
   (~15 s apart, above the 10 s interval) if you need distinct blocks, and wait on `chain_length`,
   not on the account nonce — the nonce advances at mempool admission, blocks do not.
2. Both facts make "not active" indistinguishable from "activation failed" if you only watch the
   config. Watch `chain_length` and the ledger's `activation_height` together.

## Proof that admission actually opened

A registration attempt from a **throwaway, unfunded** key was submitted and refused:

```
error: AppChain transaction rejected: Insufficient balance! Current: 0, required: 10000
```

That is the post-activation failure. Before the change the same request is refused with
*"AppChain transactions are not active on this network yet"*. `chains` remained `0`, so the probe
created nothing. The probe key (`0a52193a…ff62`) owns nothing and was discarded.

## State to carry into Phase 2

- Admission is open at `next_height >= 10`.
- Register fee 0.01, anchor 0.001, transfer 0.001; envelope version 5, recipient `"appchain"`.
- No owner key exists yet and no funds are on any AppChain owner address.
- Rollback is removal of the variable plus a restart: it stops *admission*, it does not remove a
  chain. Nothing has been registered, so rollback is currently free.
