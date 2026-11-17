# UltraNet AppChain Service — architecture and migration plan

**Status: proposal for review. Nothing in this document is built, and nothing in production changes until
the owner approves it.** It describes how the existing `ultranet-games` service grows into a general
record-anchoring service without breaking anything that already exists.

**Decisions it asks for are in §11.** Everything else is the reasoning behind them.

---

## 1. Where we are (facts, as of 2026-11-17)

| Thing | State |
|---|---|
| L1 AppChain objects | Live. `AppChainRegister`, `AppChainAnchor`, `AppChainTransferOwner`. Fees: register 10,000 / anchor 1,000 / transfer 1,000 base units |
| Chain #1 UltraGrid | On the ledger, owner `7d2cd1cc…388e`, one anchor |
| Ultra Chess chain, Registry chain | **Not registered** |
| `ultranet-games` service | Built and tested, **not deployed**. Loopback `127.0.0.1:8095`, published as `/api/games/*` |
| What it does | Grid answer checking and rewards, chess result recording, payouts, daily anchoring through `appchain-cli`, **and** a registry for third-party games (results, per-game roots, one folded daily root, inclusion proofs, self-service registration by signature) |
| Developer tooling | `ultranet-game-dev`, `tools/verify_registry_proof.py`, `sdk/typescript/ultranet-games.mjs` |
| Anchored by the registry so far | **Nothing.** No registry day has ever been anchored |

The last row matters most for §6.

## 2. Principles (these do not change)

1. **An AppChain is an L1 object.** It has no validators of its own. The owner signs; the L1 includes. The
   node never computes an application's state.
2. **Anchoring proves a record existed and has not changed. It does not prove the record is true.** Every
   document, screen and API reply must say so.
3. **An application may redistribute `$ULTRA`; it may never create it** (V2 R4). The records service moves no
   money at all.
4. **Money and records live in separate trust domains.** The key that pays players is never loaded by the
   process that accepts records from strangers.
5. **Backward compatibility is a requirement, not a courtesy.** Anything already anchored must stay
   verifiable for as long as the chain exists (§9).
6. **Only hashes and identifiers are stored.** The platform never stores an application's private payload.

## 3. The layers

```
 Applications     UltraGrid · Ultra Chess · third-party games · UltraQuiz · UltraCert · UltraDAO · …
        │  HTTP, one API key per application (or an owner signature)
        ▼
 Records service  (the "AppChain Service", infrastructure)
        │   intake · per-application daily root · one folded daily root · proofs · anchoring · usage reports
        │   holds: the anchor-owner key only
        ▼
 UltraNet L1      registration · ownership · fees · anchor confirmation
```

**What is infrastructure and what is an application.** The records service is infrastructure: generic, with
no knowledge of what a record means. Everything that *interprets* a record is an application that sits on top:

| Application | What it adds | Where it lives |
|---|---|---|
| UltraGrid | replays answers, decides wins, pays rewards | the games service (stays) |
| Ultra Chess | records phone-reported results; pays nothing | the games service (stays) |
| Third-party games, quizzes, certificate issuers, DAOs | their own rules, their own server | **their** servers; they call the records service |

**The services split in two processes.** Today one process holds both the payout key and the generic intake.
Opening generic intake to certificates and votes in that same process would put player money one bug away
from strangers' input. So:

| Process | Keys it loads | Accepts input from |
|---|---|---|
| **Records service** (new name for the registry half) | anchor-owner key only | applications and developers |
| **Games service** (stays as `ultranet-games`) | payout key; calls the records service | the Android app |

Until the split is built, the combined service keeps running as it is; the split changes where code runs,
not what it does.

**Modules are not built into the service.** The earlier sketch lists a Quiz module, a DAO module and a
Certificates module inside the service. This proposal does not build them there. A quiz needs answer rules, a
DAO needs voting rules, and a certificate issuer carries legal duties; each belongs with the party that owns
those rules. The service gives them intake, roots, proofs and anchors. A module is built into the platform only
if the owner decides to operate that application itself, and then as a client of the records service, like
UltraGrid.

## 4. The record model

The registry's record already has the right shape. It is generalised in naming only; **the bytes that are
hashed do not change** (§6 changes one thing about them, deliberately).

| Generic field | Today's field | Rule |
|---|---|---|
| `app_id` | game id | 3–32 of `a-z 0-9 -`; reserved names refused |
| `record_id` | `result_id` | 3–64 of letters, digits, `- _ .`; unique per `app_id`; the idempotency key |
| `subject` | `player` | **32 bytes, shown as 64 hex.** An UltraNet address, *or* the SHA3-256 of any identifier the application chooses (an email, a student number, a serial). The platform cannot tell which, and does not need to |
| `value` | `score` | optional unsigned integer, default 0. Meaning belongs to the application |
| `data_hash` | `data_hash` | optional 32 bytes: a hash of whatever the application wants to be able to show later |

Two consequences:

- **Certificates, votes and supply-chain events fit without a schema change**, as long as the subject is
  hashed. A diploma's subject need not be an address. This is also the privacy rule: a name or a health value
  is never sent, only its hash.
- **Mutable state does not fit.** A loan position or a balance is not an append-only record. It can be anchored
  as a periodic snapshot, but the root then proves only that *the operator claimed this state*, which the L1
  cannot verify. DeFi is therefore outside the first release (§8).

## 5. Two models, both supported

**Model A — shared registry (built).** Applications submit to the Registry chain. One folded root covers
every application, once a day. Cost to the platform: **0.001 `$ULTRA` a day in total, however many
applications**. Each application's records are separated by `app_id` and each gets its own per-application
root and inclusion proof.

**Model B — dedicated AppChain.** An application registers its own chain on the L1 (owner, rules, cadence and
fees of its own) and anchors its own roots. The L1 already supports this. **The records service does not yet:**
it knows one registry chain id. Model B needs, per application:

| Setting | Why |
|---|---|
| `chain_id` | which chain its roots are anchored to |
| anchor cadence | daily, or on demand |
| anchor owner | which key signs (a different owner needs a different key; see §10 R2) |

Model B is deliberately a later phase (§10 P4).

**Shared means a shared notary.** In Model A the owner of the Registry chain is the platform. An application
that must not depend on the platform's honesty uses Model B and its own chain. The documentation says this
plainly (§8).

## 6. Decision D1 — the domain tags, and why it cannot wait

Every hash in the registry begins with a fixed label ("domain tag"), so that a hash made for one purpose can
never be mistaken for another. The registry's labels contain the word `UltraGame`. **They are part of the
format of anything anchored.** After the first registry day is anchored, changing a label would make every
earlier proof fail to verify, so the label must be treated as permanent from that day.

The registry has anchored nothing. This is therefore the only time a neutral name is free.

| Tag today | Anchored? | Proposal |
|---|---|---|
| `UltraGame/result-id/v1` | yes (inside the per-application root) | rename to `UltraRecord/record-id/v1` |
| `UltraGame/results/v1` | yes | `UltraRecord/records/v1` |
| `UltraGame/leaf/v1` | yes | `UltraRecord/leaf/v1` |
| `UltraGame/node/v1` | yes | `UltraRecord/node/v1` |
| `UltraGame/day-root/v1` | yes | `UltraRecord/day-root/v1` |
| `UltraGame/empty/v1` | yes (an empty day) | `UltraRecord/empty/v1` |
| `UltraGame/owner-proof/v1` | no (a signing domain) | `UltraRecord/owner-proof/v1`, for consistency; not required |
| `UltraGame/api-key/v1` | no (how a stored key hash is made) | **leave**: internal, and changing it only invalidates keys that don't exist yet |
| `UltraGrid/*`, `UltraChess/*` | Grid has one anchor | **never change** |

**What it costs:** the constants and tests in `src/games/registry.rs` and `ownership.rs`,
`tools/verify_registry_proof.py` and the Node SDK (with their tests), and the pinned vectors in each. These are
the only places the tags appear. Every pinned value is recomputed independently in Python, as it was the first
time, so the change is checked and not just made.

**What it buys:** a format whose name will not read "game" for the next hundred years.

**Recommendation:** do it before the first registry anchor, and treat the new tags as frozen from then on.
Version the layouts as `v1`, and change anything later only by adding `v2` beside it.

## 7. The API

**`/api/games/*` stays, byte-for-byte compatible.** The Android app, the documented developer routes and the
SDK use it. It becomes a thin alias for the generic routes below, and its documented behaviour does not change.

| New route | Same as today's | Notes |
|---|---|---|
| `POST /api/records` | `POST /api/games/g/{id}/results` | `app_id` in the body; Bearer API key as today |
| `POST /api/apps` | `POST /api/games/g` | owner signature; same limits |
| `POST /api/apps/{id}/rotate-key` | `…/g/{id}/rotate-key` | same replay protection |
| `GET /api/apps/{id}` | `GET /api/games/g/{id}` | public page only for registered apps |
| `GET /api/apps/{id}/proof/{date}` | `…/g/{id}/proof/{date}` | same proof body |
| `GET /api/records/days/{date}` | `GET /api/games/registry/days/{date}` | same |

Rules: unknown fields stay refused; the same `record_id` with identical contents stays a no-op and with
different contents stays a `409`; responses stay `no-store`. The response bodies of the old routes are not
changed. New routes may add fields, never remove them.

**On-demand anchoring** (needed by certificates and votes) would be `POST /api/apps/{id}/anchor`, owner-signed,
refused unless the application is on a dedicated chain or has paid for it (§9). Not part of the first release.

## 8. What the service can honestly be used for

| Use | Fits? | The limit that must be stated |
|---|---|---|
| Game results, leaderboards | Yes | Results are the game's claim, not the platform's finding |
| Quizzes: answers, scores | Yes | The quiz server is trusted for grading |
| Certificates | Yes, with hashed subjects | The issuer is trusted. Anchoring proves *this record existed by this day and is unaltered*, not that the diploma is real |
| Supply-chain events | Yes | A sensor or a person can still report a false temperature |
| NFT metadata hashes | Yes | Anchoring metadata is not ownership; ownership needs L1 settlement |
| Votes / DAO | **Only with care** | The operator could refuse or omit a vote before the day closes and nobody could tell. Votes that matter need a dedicated chain and a cadence, and the limit must be shown to voters |
| DeFi snapshots | **Not in the first release** | The L1 cannot verify DeFi logic; an anchored position is a claim |
| Healthcare | **Not in the first release** | Hashes of personal data can still be personal data; legal review first |

**The statement every page, reply and doc carries:** *UltraNet records what an application reports and makes
it tamper-evident from the moment the day is anchored. It does not verify that the report is true, and until
the day is anchored the operator could still omit it.*

**Retention.** Anchored roots are permanent, and cannot be edited. Stored records are hashes and identifiers
only. If a record must be erased, the anchored root stays valid and that record can no longer be re-proven;
this limitation should be told to an application before it registers, not after.

## 9. Fees, accounting and who pays

**Today:** the Registry chain's owner pays 0.001 `$ULTRA` a day, from the owner wallet, for all applications
together. Applications pay nothing. There is no source of income behind it.

**Proposal:** the service **reports** and does not **collect**.

- It counts, per application and per day, the records accepted and the anchors that covered them.
- Payment is off the service: an application's owner pays the platform's owner address directly (an ordinary
  L1 transfer, visible on the ledger), and the operator decides what a paid tier unlocks (listing, a higher
  daily limit, on-demand anchoring).
- The service never holds an application's balance. This keeps it out of custody, and keeps principle 3 intact.

Which price, which tier and which currency are the owner's decisions (D3).

## 10. Migration plan

Each phase is one reviewable change, keeps everything before it working, and ends with the whole suite passing
and the previous API unchanged.

| Phase | What | Needs approval of |
|---|---|---|
| **P0** | Decide D1. If yes: rename the registry tags, recompute and re-pin every vector (Rust, Python, Node), update the docs. **Before** any registry anchor | D1 |
| **P1** | Add `/api/records` and `/api/apps` as aliases over the existing registry. Add `subject`/`value` naming and the "hash any identifier" rule to the docs. Old routes untouched | §7 |
| **P2** | Split the process: a records service with the anchor key only. The games service calls it. Database: one registry file per service, migrated in place by the existing upgrade path | D2 |
| **P3** | Usage reporting per application per day (§9). Read-only | D3 |
| **P4** | Model B: per-application chain id, cadence and owner; on-demand anchoring; a paid-tier switch | D3, D4 |
| **P5** | Rename the binary, the unit and the directory to `ultranet-appchain-service`, leaving `ultranet-games` as the games service and keeping the old unit name working for one release | last |
| **P6** | Application modules, only when a real second operator asks for one | demand |

**Rollback.** P0–P1 are code-only and revert with git. P2 keeps the combined binary until the split has run a
full day on the checklist; the database upgrade is transactional and `backup` is taken first. P5 is a rename
with the old names kept as aliases, so reverting it changes nothing on disk.

### The compatibility checklist (a release is blocked if any line is false)

- [ ] Chain #1, its id, its owner and its existing anchor are untouched.
- [ ] `UltraGrid/*` and `UltraChess/*` layouts, and their pinned vectors, are unchanged.
- [ ] Every `/api/games/*` route returns the same fields as before; the Android tests still pass.
- [ ] The database upgrades in place from the previous version, and `audit` agrees before and after.
- [ ] A proof produced before the change still verifies after it (a recorded fixture, not a promise).
- [ ] Environment variable names in `games.env` are unchanged (new ones may be added).
- [ ] No credential is loaded by a process that does not need it.

## 11. Decisions for the owner

| # | Decision | Recommendation |
|---|---|---|
| D1 | Rename the registry's domain tags to `UltraRecord/*` before the first registry anchor | **Yes.** Free now, impossible later |
| D2 | Split the records service from the games (payout) service | **Yes, at P2.** Not before the combined service is live and quiet |
| D3 | Who pays the anchors, and how: owner, developers (annual or per-use), sponsors | Report usage first (P3); decide the price from real numbers |
| D4 | On-demand anchoring: offered, at what price, to whom | Only for dedicated chains and paid tiers |
| D5 | Legal review before certificates, healthcare, voting and any prize game are advertised | Required before launch of those uses |
| D6 | Retention: what the platform tells an application about erasure (§8) | Say it at registration |
| D7 | Wording of the central-notary statement (§8) | Approve the text before any new use is announced |

## 12. What this document does not decide

The price of anything. Whether the owner operates any application beyond Grid and Chess. A DeFi design. A web
console or a Kotlin SDK (both deferred by the owner). Anything about the L1's consensus or fee schedule: the
service uses the L1 as it is.

## 13. What would make this plan wrong

- If a second real application never appears, P4–P6 are wasted work. P0–P2 are still worth doing: they are
  small, and they remove a risk.
- If the owner wants votes or certificates with *independent* assurance, Model A cannot give it, and the
  honest answer is a dedicated chain with its own owner, not a better shared registry.
- If the L1 changes how AppChain objects work, §5 must be revisited.
