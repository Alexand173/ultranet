# O-1 — owner decision record

**Decision:** O-1, AppChain authorisation model
**Status:** **APPROVED** by the owner on 2026-11-24
**Approved wording:** reproduced verbatim below; the only change from the draft shown to the owner is the
replacement of the date placeholder
**Evidence the decision rests on:** [O1-OWNER-AUTHORISATION-AUDIT.md](./O1-OWNER-AUTHORISATION-AUDIT.md)
(technical audit, recorded 2026-11-24, commit `29d7d49`)
**Checklist reference:** [appchain-completion-checklist-rev1.md](../appchain-completion-checklist-rev1.md), row 1

---

> **O-1 — AppChain authorisation model.** Approved on 2026-11-24 by the owner.
>
> 1. Every AppChain is identified by its **unique L1 Chain ID**, assigned by the node at registration from consensus state. Display names are non-unique labels; two chains may carry the same name, and a name confers no authority.
> 2. Registration, anchoring and ownership transfer are authorised **only** by valid owner-authorised L1 transactions, under the rules already enforced by `AppChainRegister`, `AppChainAnchor` and `AppChainTransferOwner`.
> 3. The owner signs with the **mobile wallet**; the key stays on the device. The node does not hold, load or use an owner's private key, and never signs or computes a state root on an owner's behalf.
> 4. `POST /api/appchain/tx` is the supported owner-signed submission route.
> 5. The retired server-side anchor routes (`/api/appchain/create`, `/api/appchain/{id}/anchor`, `/api/appchain/{id}/anchor/test`, `/api/appchain/anchor`) answer `410 Gone` and **must not be reintroduced** in any form, including behind a token, flag or environment variable.
> 6. Existing Chain IDs, owners, registrations and token economics remain unchanged by this decision: chains #1 UltraGrid, #2 Registry and #3 Ultra Chess; fees of 10,000 / 1,000 / 1,000 base units.
> 7. This decision fixes the authority model only. It does not authorise a deployment, a publication, a chain registration, an ownership transfer, or any change to fees or economics; each of those keeps its own gate.

---

## What this decision does not authorise

Clause 7 is not a formality. Recording O-1 closes the authority question and nothing else. In
particular it does **not** authorise, and must not be cited as approving:

- a node deployment, promotion or restart, or promoting the built-but-unpromoted artefact
  `14aff6d0…` (see [NODE_BINARY_RELEASES.md](../../deploy/NODE_BINARY_RELEASES.md));
- publication of the D-4 contract;
- registering a new AppChain, or transferring the ownership of any chain — Chain #1 included;
- any change to fees, token economics, or the activation height;
- any L1 transaction, wallet operation or spend.

Each of those keeps its own gate, and this record does not pre-approve any of them.

## Notes recorded with the approval

- The model described in clauses 1–5 is already what the code does; O-1 ratifies it as policy rather
  than changing it. The audit found no implementation gap.
- Clause 6 records the state at approval: three registered chains, owners `7d2cd1cc…` for #1 and #3 and
  `d4fca721…` for #2, with register / anchor / transfer fees of 10,000 / 1,000 / 1,000 base units.
- Clause 3 is about the *node*. A separate service may hold an owner key file and sign as a client
  (`src/games/anchor.rs` runs `appchain-cli`); that is an owner-signed transaction, not a node-side
  exception, and it is bounded by L1's ownership check.
