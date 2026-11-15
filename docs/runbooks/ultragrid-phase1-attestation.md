# UltraGrid — phase 1 attestation

Records the activation of the AppChain ledger and the first AppChain on UltraNet L1: the
registration of **UltraGrid (#1)** and the owner-signed state root that anchors it. Written after
verifying each claim against the live node on 2026-11-15. Nothing here is inferred from logs alone;
every value below was read back from the public API.

## 1. Ledger activation

| | |
|---|---|
| `ULTRANET_APPCHAIN_ACTIVATION_HEIGHT` | `10` (in `/etc/ultranet/ultranet.env`, `640 root:ultranet`) |
| Node restart that applied it | 2026-11-15 **14:26:22 UTC** |
| `activation_height` before / after | `null` → `10` |
| Chain length before / after | 6 → 10 |

The height is compared against the **next** block index, and the chain does not advance while the
mempool is empty, so reaching it took deliberate blocks; details in
[`appchain-activation-2026-11-15.md`](./appchain-activation-2026-11-15.md). Admission was proven
with an unfunded probe registration that failed on balance rather than on activation.

## 2. Registration

| | |
|---|---|
| Chain | **#1 `UltraGrid`** |
| Owner | `7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e` |
| Transaction | `bbc892926056e1389d39caeb6095c3f83ff7ad43290baa78f44a2c6f51b43ca2` |
| Block | `11` |
| Status | `confirmed` |
| Envelope | sender = owner, recipient `appchain`, amount `0`, fee `10000` (0.01), nonce `0` |
| Signed at | `1794754874` = 2026-11-15T15:01:14Z |

The chain id was assigned by the node, not the client. The owner address is the address whose
version-5 envelopes the repository's test vectors use (`VECTOR_SENDER` in
`src/appchain/ledger.rs`, `SENDER` in `src/bin/appchain-cli.rs`), so this chain is anchored by the
identity the published vectors describe.

## 3. Anchor — sequence 1

| | |
|---|---|
| Chain | #1 |
| Sequence | **1** |
| `state_root` | `0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef` |
| Transaction | `11104b653c31f2ea62c05ce0c658788d07337db0328032e37c52f23854830095` |
| Block | `12` (the only transaction in it) |
| Status | `confirmed` |
| Envelope | sender = owner, recipient `appchain`, amount `0`, fee `1000` (0.001), nonce `1` |
| Block timestamp | `1794761124` = 2026-11-15T16:45:24Z |

`/api/appchain/ledger/1` reports `anchor_count: 1`, `latest_anchor_block: 12` and
`latest_state_root` equal to the hash above, character for character. The anchor was signed from
the owner's mobile wallet, which is what proves the client-side envelope path — the node has no
owner key and the node-side anchor endpoint refuses ledger chains with `409`.

### Balance accounting

The owner started with `0.100000` from a faucet claim and now holds `0.089000`, nonce `2`:

```
0.100000 − 0.010000 (registration) − 0.001000 (anchor) = 0.089000
```

No other address moved, so both protocol debits came from the owner and nothing was funded from a
treasury — a ledger chain has none.

## 4. How to reproduce the verification

```bash
curl -s https://api.ultranetwork.cc/api/appchain/ledger/1 | python3 -m json.tool
curl -s https://api.ultranetwork.cc/api/appchain/ledger/1/anchors | python3 -m json.tool
curl -s https://api.ultranetwork.cc/api/transaction/11104b653c31f2ea62c05ce0c658788d07337db0328032e37c52f23854830095
curl -s https://api.ultranetwork.cc/api/block/12
curl -s https://api.ultranetwork.cc/api/account/7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e
```

The chain is visible at `https://ultranetwork.cc/appchains` and
`https://dashboard.ultranetwork.cc/appchains` (same build, both reverse-proxied to the dashboard
service), rendered as `OWNER_SIGNED` with no treasury. `https://api.ultranetwork.cc` is the JSON
API only: `/api/appchain/overview` returns the chain, and any non-`/api` path there is a 404.

Software in service at the time of writing:

| Component | Identifier |
|---|---|
| Node binary | `sha256 ad442a468def06eca5ae5e1b54ad5e4a9985f0041a02c7327e26c90c5d0319fd`, installed 2026-11-15T15:54:18Z |
| Website build | `Qiblh1MkYUBCQKc7suf91`, deployed 2026-11-15T16:19Z from commit `741fc95` |
| Ledger/site change | `8f495e3` (API reads both stores), `741fc95` (UI) |

## 5. Caveats — read before treating this as a real commitment

1. **Sequence 1 is a dummy root, not a statement about UltraGrid state.** It was chosen to prove
   the path. `latest_state_root` therefore carries no meaning about the grid, and the next anchor
   must be **sequence 2** with `prev_root = 0123…cdef`. UltraGrid's own `anchors.jsonl` should say
   so plainly rather than let a reader infer otherwise.
2. **The state-trie commitment is not observable over HTTP.** Applying block 12 writes
   `appchain:1:anchor:1` into the state trie (`src/appchain/ledger.rs`, `trie_entries()`), but the
   node exposes no route that reads a trie key — only `/api/state/size`. The verifiable chain is
   *confirmed transaction → block 12 applied → ledger record with the matching root*; the trie
   write itself rests on the code path, not on a query. Adding a trie-read or proof route would
   make it independently checkable.
3. **No delete exists.** Both records are permanent. A wrong owner can only be fixed by a
   `TransferOwner` (fee 0.001), never by re-registering.

## 6. Outstanding

- **Handover drill** — transfer #1 to a second owner, confirm the old owner's next anchor is
  refused and the new owner's succeeds. Not yet performed; it is the last unfinished item from the
  plan's definition of done.
- **First real anchor** (sequence 2) computed from a persisted UltraGrid state, per the chained
  commitment in the plan.
