# Move VM: reject unknown calls at admission — proposal (2026-11-21)

Status: **proposal only. Nothing in `src/` is changed by this document.** Implementation needs the owner's explicit approval of a concrete patch.

## 1. Problem, stated precisely

`MoveVM::execute_function` has four real handlers (`UltraCoin.mint`, `UltraCoin.transfer`, `FheCoin.mint`, `FheCoin.transfer`). Any other `module.function` falls into the `_` arm (`src/move_vm.rs:276`), logs "simulated in RAM for now" and returns `Ok(vec![1])`. It changes no state.

Making that arm return `Err` would change **no consensus outcome**: all three block-application call sites discard the result (`let _ =` at `src/lib.rs:3736`, `src/lib.rs:4061`, `src/appchain/runtime.rs:252`). The defect is therefore not in the VM's return value but in what is allowed to *enter a block*: a call that can never do anything is still admitted, included and charged.

## 2. Read-only history check (live chain, 2026-11-21)

I fetched `GET /api/block/0` … `/api/block/12` from the public API (read-only; no transaction sent, no state touched) and counted payload kinds:

| Blocks | Payloads |
|---|---|
| 0 | none (genesis) |
| 1 / 2 | `ValidatorJoinProposal` / `ValidatorApproval` |
| 3–10 | `StandardTransfer` (11 in total; block 6 has 4) |
| 11 / 12 | `AppChainRegister` / `AppChainAnchor` |

**Zero `MoveCall` transactions in all 13 blocks, and the mempool was empty (`mempool_size 0`).** So a new admission rule would reject no historical call on the live chain. This does not cover other deployments' databases, so replay safety is still guaranteed by design (§4) and by a test (§5), not by this observation alone.

## 3. All entry paths for a new `MoveCall`

Every path into the mempool goes through `UltraBlockchain::add_transaction` (`src/lib.rs:2106`), which calls `validate_transaction` (`src/lib.rs:2337`) **before** it reserves the nullifier, touches the mempool or persists anything. A rejection there consumes nothing; the fee is only applied when a transaction is mined.

| Path | Reaches `add_transaction`? | Can carry a `MoveCall` that passes today? |
|---|---|---|
| `POST /api/move/execute` (`src/api.rs:1139`) | yes | **No — already rejected.** It builds the transaction with `zk_proof: vec![]` (`src/api.rs:1170` area) and `validate_transaction` requires a non-empty proof for any `MoveCall` (`src/lib.rs:2424-2429`). |
| `POST /api/transactions` (`src/api.rs:632`) | yes | No — it builds `StandardTransfer` only (`src/api.rs:741`). |
| AppChain operation endpoint (`src/api.rs:1795`) | yes | No — it builds AppChain payloads. |
| Approval flow (`src/api.rs:3105-3131`) | yes | No — approval payloads only. |
| Peer gossip `NetworkMessage::Transaction` (`src/p2p.rs:1147`) | yes (result ignored) | **Yes.** A peer can send a signed `MoveCall` with any non-empty `zk_proof`; the proof is only checked for being non-empty. |
| Startup restore of persisted pending transactions (`src/lib.rs:1865`) | no — direct `mempool.add_transaction` | Only if one was persisted earlier. It is not revalidated and panics on a mempool error, so **this path must not get the new rule**. |
| Block application and block validation (`src/lib.rs:2625`, `src/lib.rs:3439`) | no | Historical blocks; **must not get the new rule** (§4). |

Correction to my earlier message: I said `/api/move/execute` would return "call submitted!" and take the 500 fee. By this reading it never gets that far, because the empty proof is rejected first. The only path where an unknown call is admitted today is peer gossip. I have not run it; this is from reading the code.

## 4. Proposed rule

One check, in `validate_transaction` only, next to the existing `UltraCoin.mint` restriction (`src/lib.rs:2408`):

- Define a single allowlist of the supported pairs next to the dispatch in `src/move_vm.rs`, and have the check read it. The dispatch `match` itself is left as is, so there is no behaviour change for known calls. A test asserts every allowlisted pair is handled by a real handler, so the two cannot drift apart.
- A `MoveCall` whose `(module_name, function_name)` is not allowlisted returns `Err("unknown Move function <module>.<function>")`.
- `UltraCoin.mint` stays rejected by its existing, earlier rule. The effective admissible set is `UltraCoin.transfer`, `FheCoin.mint`, `FheCoin.transfer`.
- The pair is checked as written; `module_address` is not part of the rule (the VM ignores it today: `_module_address`).

**Not changed:** `validate_block_transaction_authorization`, `validate_block`, block application, startup restore, the VM dispatch, hashes, signatures, gas, fees. Old blocks therefore replay exactly as before; only *new submissions* of unknown calls are refused.

Consensus note: this is an admission (mempool) rule, not a block-validity rule, so nodes that do not have it still accept blocks produced by nodes that do. A node without the rule may still include an unknown call another node refused. That is acceptable and should be understood as policy, not protocol.

## 5. Tests required

1. An unknown `MoveCall` is rejected by `add_transaction`, with the new message.
2. A known `MoveCall` (e.g. `UltraCoin.transfer`) is not rejected by the new rule.
3. A **historical block** containing an unknown `MoveCall` still passes block validation and replays to the same state root as before the change.
4. After a rejection: the nullifier is not reserved, the mempool length is unchanged, nothing is saved as pending, `pending_nonces` is unchanged and no fee is applied — the same call can be retried after fixing it.
5. Every allowlisted pair reaches a real handler (guards the list against drift).
6. Startup restore still loads a persisted pending transaction that is now unknown, without panicking.

Mutation check (project practice): removing the check must fail tests 1 and 4; adding it to the block path must fail test 3.

## 6. Open points for the owner

- Whether any non-live database (staging, backup, a disposable chain) holds unknown `MoveCall`s. The rule is safe for them either way (§4), but test 3 is what proves it.
- Whether the Move surface should stay four hard-coded functions, or be removed from admission altogether until real Move execution exists. That is a product decision, not part of this patch.

## 7. Separate defects, recorded and **not** part of this patch

Found while reading `src/move_vm.rs`:

1. `persistent_mint` and `persistent_transfer` silently return `Ok(vec![1])` when given too few arguments (`if args.len() >= 2` / `>= 3`), so a malformed call looks successful.
2. `u64::from_le_bytes(args[0][..8].try_into().unwrap())` panics if `args[0]` is shorter than 8 bytes. It is reachable through the same peer-gossip path as in §3, so a malformed call can panic the node's block-application thread. This is the more serious of the two and should have its own fix and test.
3. `MoveDeploy` stores bytecode, but the VM never executes it: dispatch is on four hard-coded names only.
4. `FheCoin.mint` has no restriction comparable to the `UltraCoin.mint` guard.

Each needs its own decision; none is touched by the admission patch.
