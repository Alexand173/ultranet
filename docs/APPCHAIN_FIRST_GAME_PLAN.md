# First AppChain: daily skill puzzle — plan

**Date:** 2026-11-14 · **Status:** plan, no code yet
**Agreed with the owner:** an original daily skill puzzle ("UltraNet Grid", working name) as AppChain #1. Test rewards of 0.00001 `$ULTRA` per win, withdrawable from 0.0001 `$ULTRA`, from a 1 `$ULTRA` pool funded from the faucet wallet.
**Horizon:** UltraNet is built for 10–100 years and thousands of AppChains. Everything below is designed as a **generic primitive that any AppChain can use**. Only the puzzle itself is specific to this game.

## 1. Starting point (from the code)

- An AppChain is a registry entry: `name`, an `owner` **text** field, and a treasury derived from the chain ID (`src/appchain/factory.rs`). Creating and anchoring a chain are admin-only routes.
- Anchoring commits the AppChain's own storage inside the node (`l3/<id>`) and charges 0.001 `$ULTRA` to the treasury. Roots supplied by clients are refused on purpose.
- **Nothing can write to an AppChain's storage.** There is no transaction route, and `produce_block` runs through the Move stub. Anchoring therefore commits empty state.
- The treasury can only be debited for anchor fees. It can't pay anyone.
- Mainnet has 0 AppChains, so changing the registry format needs no migration.

## 2. Node changes (generic, for every AppChain)

**Model (owner's rule, 2026-11-14): an AppChain has no validators of its own. It is an L1 object.** Its owner signs L1 transactions, and L1 validators only check the signature and fee and include them in a block, like any other transaction. The node never computes or judges an AppChain's state.

1. **`AppChainRegister { name }`**: a signed L1 transaction. The sender becomes the owner, the node assigns the next chain ID, and the fee is fixed (spam protection).
2. **`AppChainAnchor { chain_id, state_root }`**: a signed L1 transaction, valid only when `sender == owner`. It records the 32-byte root at the next anchor sequence number. The fee is fixed and paid by the owner, and it goes to L1 validators like any fee.
3. Both are added at the end of `TransactionPayload`, with their own signing format (envelope v5 with its own domain string), behind an activation height.
4. **They become consensus state:** registrations and anchors go into the state root (`appchain:<id>`, `appchain:<id>:anchor:<seq>`), so every validator holds the same registry. Today the registry is local to one node.
5. **Fees are debited explicitly** in both block re-execution and `update_state`. Today `update_state` debits only standard transfers, while the fee loop pays every transaction's fee to validators.
6. Routes: `POST /api/appchain/tx` (signed register or anchor), `GET /api/appchain/ledger`, `GET /api/appchain/ledger/{id}`, `GET /api/appchain/ledger/{id}/anchors`.

**Retire once active:**
- The admin-triggered anchor route that computes a root from the node's own `l3/<id>` storage. It is not owner-signed and not in a block.
- The node-side AppChain runtime (Move VM / `produce_block`). AppChain state lives with its owner, not inside L1 nodes.
- The keyless derived treasury address. `$ULTRA` sent there cannot leave. The owner pays the anchor fee directly.

**Later, before thousands of chains:** permissionless registration is already covered by (1). Name rules and pricing can be tuned by later governance.

## 3. Game server (operated by the owner)

- **Daily puzzle:** a grid of nodes; route a signal from source to target in as few moves as possible. The puzzle is generated deterministically from the day's seed.
- **The seed is committed before play** (an `AppChainAnchor` of `hash(seed)`) and revealed after the day closes. This proves the server didn't pick puzzles after seeing the answers.
- **Solutions are checked by replaying them** from the seed. A "win" means solving within the move limit. Clients never send a score.
- **Ledger:** winnings are credited per wallet, at most **10 wins per wallet per day**, and paid out only when a wallet reaches the withdrawal minimum.
- **Daily close:** publish the results file and anchor its hash (`AppChainAnchor`, signed by the game's owner key).
- **Payout wallet:** an ordinary L1 wallet whose key stays on the game server. Withdrawals are normal transfers, nonce-ordered from a single queue. This reuses the faucet's signer and queue design.

## 4. Test economics (phase 1)

| Setting | Value |
|---|---|
| Pool | **1 `$ULTRA`** (1,000,000 base units), funded from the faucet wallet |
| Reward | **0.00001 `$ULTRA`** (10 base units) per win |
| Wins the pool covers | about 100,000, minus withdrawal fees (1 base unit per 0.0001 withdrawal) |
| Limit per wallet | 10 wins per day, so at most 0.0001 `$ULTRA` per day |
| Withdrawal minimum | **0.0001 `$ULTRA`** (100 base units), reached after one day of 10 wins. Raise it later to cut down on transactions. |
| End | When the pool is empty or on an announced date, whichever comes first. The ending is shown in the app from day one. |

**How to fund the pool:** the payout wallet **claims once from the faucet like any user**. A claim is exactly 1 `$ULTRA`. This uses the existing, approved faucet path, needs no handling of the faucet key, and is recorded as a normal claim (it uses 1 of the 998 remaining). Document it in the faucet runbook so the reconciliation shows a known purpose.

**Phase 2 and beyond (not built now):** free play always stays available. Paid tournaments come next: players' entry fees form the prize pool, with a game fee for operating costs. **Rewards come only from fees and sponsors, never from minting and never from the faucet.** Get legal advice before paid entry.

## 5. App

- The Games tile becomes Supported, opening a directory with one entry, then the puzzle screen. The directory is built for many AppChains from the start.
- Screens: today's puzzle, result, your winnings, withdraw (to your own wallet address, no signing needed), and a "verify" view showing the commit hashes for the day.
- Signing in to the game: the wallet signs a challenge locally (the node already has `/api/auth/challenge`), so winnings are tied to an address the player actually controls.

## 6. Order of work

| Step | Work | Est. |
|---|---|---|
| 1 | Node: `AppChainRegister` + `AppChainAnchor` as consensus transactions, state-root binding, API, tests, testnet deploy | 1.5–2 weeks |
| 2 | Game server: seed and puzzle generator, solution checking, ledger, daily commits, payout queue | 1–2 weeks |
| 3 | App: Games directory, puzzle screen, winnings, withdraw, verify view | 1–2 weeks |
| 4 | Register AppChain #1 from the game's owner wallet, fund that wallet for anchor fees, fund the payout wallet with one faucet claim, launch phase 1 | 1 day |

## 7. Risks

- **Bots:** puzzles are easy for computers. The test pool is small and capped per wallet, so the loss is bounded at 1 `$ULTRA`. In phase 2, entry fees make farming unprofitable.
- **Consensus changes:** the new transaction type must reach every validator before its activation height.
- **Operator trust:** in phase 1, players trust the game server's ledger. The commits make tampering *detectable*, not impossible. Say so in the app.
