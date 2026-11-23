# The AppChain protocol — one procedure for every chain

**Status:** specification. Manual and server operation work today; delegated operation does not exist and
is §8. **Applies to:** every AppChain, ours or a third party's — UltraNet Grid, Ultra Chess, and every
game, exchange or service registered after them.
**Date:** 2026-11-15

> The point of this document: an AppChain has exactly **one owner**, and the only question a builder
> should have to answer is *where that key lives*. Everything else — registering, anchoring, changing
> your mind — follows from that one answer, the same way for every chain. This document makes that
> procedure explicit, so nobody invents a local variant.

---

## 1. What an AppChain is (one paragraph, no metaphor)

An AppChain is an **entry in L1 consensus state**: a name, an owner address, and an append-only list of
state roots that only the owner can add. It has no validators of its own, no runtime on the node, and no
way to hold or move `$ULTRA` by itself. The L1 never inspects what a root means. Everything a chain
actually does happens off-chain, in software its owner runs; the chain is the record that it happened and
the signature that proves who said so.

That model has one consequence that shapes this whole protocol: **the owner key is the chain.**

---

## 2. Actors

| Actor | Holds | Can do | Cannot do |
|---|---|---|---|
| **Owner** | The one key that owns the chain | Register, anchor, transfer ownership, (proposed) authorize an anchorer | Anything after transferring away |
| **Anchorer** (proposed, §8) | A delegation key | Submit anchors only | Transfer ownership, change the anchorer, register another chain as this one |
| **L1 validator** | No chain keys | Include or refuse a signed transaction; never judge a root | Forge, reorder ownership, or recover a lost key |
| **App / CLI** | Whatever key the operator gives it | Sign what a human or a server instructs | Nothing without a key |

---

## 3. The universal lifecycle

Every AppChain, ours or not, moves through exactly these states. The arrows are the only legal moves, and
each one is a signed L1 transaction.

```
                    ┌──────────────────────────────────────────┐
                    │  off-chain: build, run, earn, lose users │
                    └──────────────────┬───────────────────────┘
                                       │
   AppChainRegister { name }           ▼
   signer becomes owner        ┌─────────────────┐
   fee 10,000 base units       │   REGISTERED    │
   ───────────────────────────►│  owner = one    │
                               │  address        │
                               └────┬───────┬────┘
                                    │       │
        AppChainAnchor (owner)      │       │  AppChainTransferOwner (owner)
        fee 1,000, repeatable       │       │  fee 1,000, irreversible
                                    ▼       ▼
                          ┌──────────────┐  ┌────────────────┐
                          │   ANCHORING  │  │ OWNER CHANGED  │
                          │ append-only  │  │ old key is now │
                          │ sequence++   │  │ a stranger     │
                          └──────────────┘  └────────────────┘
```

There is **no unregister**. A chain that is never anchored is a name and a signature; it stays that way
forever. Deleting registry state is a state migration, not a transaction.

---

## 4. The three operating modes

Mode is not a consensus concept for the first two — it is a statement about **where the owner key lives**.
Only the third adds anything to the ledger.

| | **M1 · Manual** | **M2 · Server** | **M3 · Delegated** *(proposed)* |
|---|---|---|---|
| Owner key lives | Human's wallet (phone), encrypted | Server, encrypted key file (`appchain-cli`) | Human's wallet (phone) |
| Who anchors | The human, tapping | A timer on the server | A separate anchorer key on the server |
| Who can transfer ownership | The human | **The server** | The human — never the anchorer |
| Blast radius if the server is compromised | Nothing — no key on it | **The whole chain**, including its transfer | Anchors only; owner revokes one tx later |
| Blast radius if the phone is lost | Chain is frozen until recovery | Unaffected (server is owner) | Unaffected (owner key needs recovery; anchors continue) |
| Anchor cadence it can sustain | When a human remembers | Every N hours, forever | Every N hours, forever |
| Exists today? | **Yes** — `AppChainsScreen` | **Yes** — `appchain-cli anchor`, an owner-signed `AppChainAnchor` posted to `/api/appchain/tx` (no server-side anchor route exists) | **No** — §8 |

**The recommendation this document makes:** default a new chain to **M1**, and treat **M2 as a decision
you make when automation is genuinely needed**, because M2 is the only mode where a server compromise
costs you the chain. M3 is what most operators will actually want, which is why it is specified here.

**Registry chains carry no enforced owner.** A legacy factory registry chain records an owner *label*, and
nothing in the node checks it before a write. That is why the models above apply to ledger chains only, and
why the registry is described everywhere as legacy rather than as a second way to own a chain.

---

## 5. Registration — the universal procedure

Identical for every chain. Only the values differ.

| Step | Action | Failure it prevents |
|---|---|---|
| 1 | Confirm the node reports `terms.activation_height` (not `null`), `register_fee_base_units`, `anchor_fee_base_units` from `GET /api/appchain/ledger` | Registering into a node that refuses everything, or paying a fee you did not read |
| 2 | Choose the mode (§4) **before** creating the key — it decides where the key is generated | Creating a key on a machine you do not intend to keep |
| 3 | Get an address that will own the chain, funded with at least `registration + anchor × cadence` | A registration that succeeds and then cannot anchor |
| 4 | Submit `AppChainRegister { name }`, signed by that address | — |
| 5 | Read back `GET /api/appchain/ledger/{id}` and confirm **`owner` equals your address** | A registration attributed to a key you did not expect |
| 6 | Submit one `AppChainAnchor` with a real root, and confirm the sequence number went to 1 | Discovering at the first daily commit that the path does not work |

Name rules are consensus-enforced: 1–64 characters, ASCII letters, digits, spaces, hyphens,
underscores, no leading or trailing space. Names are labels, not identities — two chains may share a
name, and the **chain ID is the identity**.

### 5.1 If the mode is M2, there are two legitimate ways to get there

| Path | Transactions | Cost | When to use it |
|---|---|---|---|
| **Server registers itself** — the key is generated on the server with `appchain-cli generate-key`, funded, then `appchain-cli register` | 1 | 10,000 | The server will own it from birth; nobody ever intends to tap for it |
| **Phone registers, then transfers** — the app registers with the wallet, then `TransferOwner` to the server address | 2 | 20,000 | You want proof that *you* created it first, and a window to change your mind |

Both are valid. The second is the one to use when a person must be able to say "I registered this before
any machine touched it" — at double the fee.

### 5.2 If the mode is M1 or M3, registration happens in the app

Unchanged from today: the wallet signs `AppChainRegister`, becomes the owner, and the chain appears in
the app with a "Tap to anchor" action. In M3 an additional transaction (or the same block) authorizes
the anchorer — §8.3.

---

## 6. Anchoring — the procedure per mode

The root is 32 bytes. The L1 does not know or care what it commits to; the owner's software does. What
the protocol fixes is **who may append, at what price, and in what order**.

| Mode | Who signs | Where the root comes from | Cadence this mode makes possible |
|---|---|---|---|
| M1 | Phone wallet, unlocked | The app's own state | Manual, whenever the human acts |
| M2 | `appchain-cli` on the server | The operator's software | Cron / systemd timer, indefinitely |
| M3 | Anchorer key on the server | The operator's software | Same as M2, with the owner's key off the server |

Non-negotiable rules for every mode:

1. **Anchors are append-only.** Sequence numbers are assigned by consensus, never by the client. A
   replayed transaction reproduces its original sequence and never consumes a new one (§`plan_block`,
   `ledger.rs`).
2. **The fee is `terms.anchor_fee_base_units`** — read it, never hardcode it. Paying more is allowed;
   paying less is refused.
3. **A chain that stops anchoring does not break.** It simply has an older root; nothing is invalidated.
   Freshness is a product claim, not a consensus one.
4. **Failure is always visible.** Every mode must report "last successful anchor" and the reason for a
   failed one. A silent gap is the only real failure mode of this protocol, and it is an observability
   bug, not a chain bug.

---

## 7. Changing your mind — the mode transition matrix

This is where most designs break, so every transition is explicit about what survives.

| From → To | How | Fee | What changes | What is saved by it |
|---|---|---|---|---|
| M1 → M2 | `TransferOwner` to the server address | 1,000 | The server becomes owner; the phone loses all rights immediately | Manual tapping, forever |
| M2 → M1 | `TransferOwner` back to a phone address | 1,000 | The human becomes owner again | Decommissioning a server |
| M1 → M3 | Authorize an anchorer (§8) | 1,000 (proposed) | The server may anchor; the phone stays owner | Automation without giving up ownership |
| M3 → M1 | Revoke the anchorer | 1,000 (proposed) | Automation stops; the human continues | Ending a contract with a server |
| M3 → M2 | Revoke, then `TransferOwner` | 2,000 | As M2 | Selling/simplifying a chain |
| Any → new owner | `TransferOwner` | 1,000 | Ownership moves; the old owner is a stranger to that chain | Selling the chain, or rescuing it from a leaked key |

Two rules that make this matrix safe:

- **Only the owner may start any transition.** An anchorer can never transfer ownership, and can never
  change which anchorer is authorized. That asymmetry is the entire safety argument for M3.
- **A transition that has not landed in a block has not happened.** After announcing a mode change,
  the old mode is still live until the transaction is confirmed. Every operator instruction in this repo
  says "confirm, then destroy the old key" in that order.

---

## 8. M3 — delegated anchoring (the one new consensus feature)

Everything above works today except M3. This section is the design, not the code.

**Not implemented.** No `AppChainAuthorizeAnchorer` or `AppChainRevokeAnchorer` envelope exists in the node.
The version-5 operation enum in `src/appchain/ledger.rs` carries `register`, `anchor` and `transfer_owner`
only, and a transaction offering either payload is refused as an unknown operation. A chain therefore has
exactly one signer today — its owner — and delegation is a v6 change, listed as roadmap row **P2** below.

**The envelope itself is specified in code today.** Each operation is committed under the domain
`UltraNet/appchain-envelope/v5` (`src/appchain/ledger.rs`), and that digest is pinned on three independent
sides: `appchain-cli` (`cli_digests_match_the_node_vectors`), the Android `AppChainSignerTest`, and
`tests/appchain_authorization_test.rs`. The published byte-level specification is **D-4 item T4** and does
not exist yet, so until it does, the code and those three pins are the contract.

### 8.1 Two new payloads

```rust
// variant tag, cited in the v6 signing domain
AppChainAuthorizeAnchorer {
    chain_id: u32,
    anchorer: String,      // 64-hex address; the zero address 000…0 means "revoke"
    expires_at_block: u64, // 0 = no expiry; otherwise a hard stop, enforced by consensus
}

AppChainRevokeAnchorer { chain_id: u32 }   // sugar for anchorer = zero address
```

### 8.2 Rules, enforced at block validation

| Rule | Reason |
|---|---|
| Sender must be the chain's **owner** | An anchorer must never be able to renew or replace itself |
| `anchorer` is 64-hex, or the zero address to revoke | Same address rule as everything else |
| `anchorer != owner` | Authorizing yourself is a no-op that hides a mistake |
| `expires_at_block == 0 \|\| expires_at_block <= current + MAX_DELEGATION_BLOCKS` | A delegation that never expires is an ownership transfer with extra steps. Proposed cap: **777,600 blocks ≈ 90 days**, renewable |
| Fee: `terms.authorize_fee_base_units`, proposed 1,000 | Same as a transfer: one ledger write |
| Effect is committed to the state root (`appchain:{id}` digest gains the anchorer and expiry) | It is consensus state, not a hint |
| Replay reproduces the stored record, never re-applies | The same rule as registrations, anchors and transfers |

### 8.3 Anchor admission becomes

```
accept an AppChainAnchor when:
    tx.sender == chain.owner
 || (tx.sender == chain.anchorer
      && (chain.anchorer_expires_at_block == 0 || block_index < chain.anchorer_expires_at_block)
      && chain.anchorer != zero)
```

Note what is **not** in that expression: nothing about transfer, nothing about the registration name,
nothing about other chains. An anchorer's authority is exactly one verb, for a bounded time.

### 8.4 Why expiry is not optional

An anchorer key on a server is a key on a server. The only thing that makes a compromise survivable is
that the attacker's window is **bounded and visible**: it cannot transfer the chain, and it cannot renew
itself. A 90-day expiry means the worst case is bounded anchors for up to 90 days, plus a missed renewal
that the monitoring must report. Without expiry, M3 collapses into M2 with more moving parts.

### 8.5 What M3 deliberately does not give you

- No per-anchorer rate limits or quotas — the fee does that work.
- No multiple anchorers. One at a time, replaced rather than added: a list is a governance problem
  dressed as a feature.
- No anchorer-signed registration. A chain is born owned by a person or a server key, not by a delegate.

---

## 9. Key custody — the rules that apply to all modes

| Key | Where it lives | Encrypted by | May it be in the node's environment? |
|---|---|---|---|
| Owner key, M1/M3 | Phone secure storage | Wallet password (Android Keystore) | **Never** |
| Owner key, M2 | `/opt/ultranet/owner-key.enc`, mode `0600` | Passphrase (Argon2id → ChaCha20-Poly1305) via `appchain-cli` | **Never** |
| Anchorer key, M3 | Server, same encoding as M2 | Same | **Never** |
| Faucet / payout keys | Separate services, never the node | Their own credential stores | **Never** |

Two rules that are not negotiable, because they are what the transfer operation exists to rescue:

1. **The old key is destroyed only after the new one has anchored successfully.** A handover that has
   not landed in a block has not happened.
2. **Every chain keeps one key off the machine that signs daily.** For M2 that means the phone wallet
   stays as a backup until the server has anchored at least once — and for M3 it is automatic.

### 9.1 Recovery matrix

| Situation | M1 | M2 | M3 |
|---|---|---|---|
| Server compromised | Chain unaffected | **Chain lost** unless rescued fast: `TransferOwner` from the leaked key while you still control it | Revoke the anchorer; the chain never moves |
| Server key lost | Unaffected | Generate a new server key, `TransferOwner` from the phone backup | Re-authorize a new anchorer from the phone |
| Phone lost | Frozen until the seed phrase is restored | Unaffected | Ownership frozen; anchoring continues until expiry |
| Both lost | Chain is dead — readable forever, never anchorable again | Same | Same |
| Delegation expired silently | — | — | Anchors are refused; monitor the expiry and warn at 14/7/1 days |

---

## 10. Invariants

1. **Exactly one owner at any block.** Two owners is a chain split; the ledger forbids it.
2. **Ownership moves only by an owner-signed transaction.** No admin route, no environment variable, no
   operator override.
3. **Delegation never confers ownership**, cannot change delegation, and is bounded in time.
4. **Anchors are append-only**, sequence numbers come from consensus, and a replay reproduces them.
5. **The registry is consensus state** — every validator holds the same registry, and registrations and
   anchors are in the state root.
6. **Nothing is deletable.** Not a chain, not an anchor. Retirement is a policy statement, not a state
   change.
7. **Fees always come from `terms`.** A client that hardcodes a fee is broken by design, not by a fork.
8. **A key is never in the node's environment, a URL, a log line or a repo.**

---

## 11. What a new AppChain must implement (the checklist)

Any team — ours or a stranger's — that wants a chain on this network:

- [ ] Generate a key for the intended mode (§4), never on a machine you will throw away
- [ ] Fund it for registration **plus** the anchor cadence you promise (1,000 per anchor)
- [ ] Read `terms` from `GET /api/appchain/ledger`; refuse to proceed if `activation_height` is null
- [ ] `AppChainRegister`, then verify `owner` on `GET /api/appchain/ledger/{id}`
- [ ] Anchor once and verify sequence 1 before announcing anything publicly
- [ ] Choose a cadence you can actually keep, and publish it — a chain that anchors hourly and stops is
      worse than one that anchors daily and says so
- [ ] Keep a second key **off** the signing machine (§9)
- [ ] If automating with M3: publish the anchorer address and its expiry, and monitor it
- [ ] Report "last successful anchor" in your own UI; never let a gap be silent

---

## 12. UX contract

Mode must be explainable in one line per option, or people will pick the wrong one and never know:

| Option shown | One-line consequence |
|---|---|
| **"I'll anchor myself"** | You sign each commit from this wallet. Nothing to run, nothing to leak — but nothing happens while you do not tap. |
| **"Run it on my server"** | The server holds the chain, so it can also hand it away. Keep a copy of this wallet as the backup. |
| **"Let my server anchor, I keep the chain"** *(M3)* | The server can add commits and nothing else, and stops working when the authorization expires. |

The chain detail screen must always show: **mode**, **who officially owns it**, **last anchor and its
block**, and for M3 the **anchorer address and expiry**. "Who can do what" is the only thing a user of
this protocol actually needs to know.

---

## 13. Phases

| Phase | Deliverable | Gate |
|---|---|---|
| **P0** | This document, agreed | — |
| **P1** | M1 + M2 polished: app shows mode, last anchor, and the transfer warning; CLI gains `anchor` docs and a timer unit | An operator can register and anchor in both modes without reading code |
| **P2** | M3: `AppChainAuthorizeAnchorer` / `RevokeAnchorer`, ledger field, envelope rule, replay, state-root binding, activation height | A delegation expires on schedule, and a revoked anchorer is refused in the same block it is revoked |
| **P3** | App: mode picker at registration, anchorer + expiry display, revocation sheet | A user can move M1 → M3 → M1 without support |

---

## 14. Open decisions

| # | Decision | Why it needs you |
|---|---|---|
| A1 | Is M3 (delegated anchoring) in scope at all, or is "manual or server" strictly both-or-nothing? | M3 is the only part of this document that costs a consensus change |
| A2 | Delegation cap — 777,600 blocks (~90 days) proposed | A longer window weakens the compromise argument; a shorter one burdens renewals |
| A3 | Fee for authorize/revoke — 1,000 proposed | The same as a transfer: one ledger write |
| A4 | Does an anchorer need its own registration deposit, or is authorization alone enough? | A deposit makes a compromised anchorer key cost the attacker too |
