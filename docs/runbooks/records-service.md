# The Records Service — installing and running it

**Service:** `ultranet-records` (third-party games and every other application that anchors records: accepts
records, builds each day's root, answers proofs, and holds the chain owner's key) · **Port:** `127.0.0.1:8096`,
published through the reverse proxy as `/api/games/g…`, `/api/games/registry/…`, `/api/records…` and
`/api/apps…` · **Its sibling:** the Games Reward Service ([`games-service.md`](./games-service.md)), which pays
players and asks this service to anchor UltraGrid's and Ultra Chess's days.
**Architecture and the reasons:** [`../APPCHAIN_SERVICE_ARCHITECTURE.md`](../APPCHAIN_SERVICE_ARCHITECTURE.md)
**For developers:** [`../GAMES_PLATFORM.md`](../GAMES_PLATFORM.md)
**State of this runbook:** its commands were exercised against local builds, with disposable databases, a
stub ledger and a stand-in for `appchain-cli`. **It has not been run on the production host; nothing has been
registered or anchored by it.**

## 0. The boundary

| | Records Service (this) | Games Reward Service |
|---|---|---|
| Holds | the **chain owner's key** (`records-anchor-owner.key.enc`) and passphrase, the peer secret | the **payout key**, the abuse key, the peer secret |
| Never holds | any payout key, or the Games abuse key | any chain owner's key |
| Database | `/var/lib/ultranet-records/records.db` | `/var/lib/ultranet-games/games.db` |
| Unix user | `ultranet-records` | `ultranet-games` |
| Moves money | **never; it has no code path that does** | yes, payouts to players |

`check-config` and `serve` **refuse to start** if `games-payout-signer.json` or `games-abuse.key` is in this
unit's credentials directory. The systemd unit also makes the other service's directories inaccessible
(`InaccessiblePaths`), so even a misplaced file is not readable from here.

**What a compromised Games service can do to this one.** It holds the peer secret, so it can send a valid
signed request. It can ask for an anchor of any root on chains `RECORDS_PEER_ANCHOR_CHAINS` allows (default
`1,2`, UltraGrid and Ultra Chess). It can have at most `RECORDS_PEER_ANCHORS_PER_DAY` (default 8) distinct roots anchored per UTC day, so it
cannot drain the owner wallet by asking for many (each anchor costs 0.001 `$ULTRA`). That allowance is kept
in the database, so restarting or crashing this service does not reset it (§4.3). It cannot anchor the
registry's own chain or the L1 (refused with `403 forbidden`), cannot read or change a registry record (a different database, a different user), and cannot sign
anything. That is why the allow-list exists, and why it should stay as short as it is.

## 1. What lives where

| Thing | Path | Notes |
|---|---|---|
| Binaries | `/opt/ultranet-records/ultranet-records`, `/opt/ultranet-records/appchain-cli` | built from the **same source revision** as the Games service |
| Unit | `/etc/systemd/system/ultranet-records.service` | from `deploy/ultranet-records.service` |
| Policy | `/etc/ultranet-records/records.env` | `root:ultranet-records`, `0640`; limits and endpoints only, **no secrets** (`deploy/records.env.example`) |
| Secrets | `/etc/ultranet-records/secrets/*.cred` | systemd encrypted credentials |
| Database | `/var/lib/ultranet-records/records.db` | `0700` directory; WAL mode |
| Backups | `/var/backups/ultranet-records/` | encrypted, away from the credentials |

| Credential | What it is | Made with |
|---|---|---|
| `records-anchor-owner.key.enc` | the chain owner's key file | `appchain-cli generate-key --out …` |
| `records-anchor.pass` | that file's passphrase | you choose it; a long random string |
| `records-peer-games.key` | the secret shared with the Games service (the same bytes as its `games-records-peer.key`) | `head -c 48 /dev/urandom \| base64 \| tr -d '\n'` |

## 2. Install

```bash
cargo build --release --bin ultranet-records --bin appchain-cli
sudo useradd --system --home-dir /var/lib/ultranet-records --shell /usr/sbin/nologin ultranet-records
sudo install -d -o ultranet-records -g ultranet-records -m 0700 /var/lib/ultranet-records
sudo install -d -o root -g ultranet-records -m 0750 /etc/ultranet-records /etc/ultranet-records/secrets
sudo install -d -o root -g root -m 0750 /opt/ultranet-records
sudo install -o root -g root -m 0755 target/release/ultranet-records target/release/appchain-cli /opt/ultranet-records/
sudo install -o root -g ultranet-records -m 0640 deploy/records.env.example /etc/ultranet-records/records.env
sudo install -o root -g root -m 0644 deploy/ultranet-records.service /etc/systemd/system/ultranet-records.service
```

### 2.1 The owner key (only if this service will anchor)

```bash
sudo mkdir -m 0700 /root/records-staging && cd /root/records-staging
/opt/ultranet-records/appchain-cli generate-key --out records-anchor-owner.key.enc   # prints its address
printf '%s' 'a long random passphrase' > records-anchor.pass && chmod 600 records-anchor.pass
```

This key owns nothing yet. **Hand chain #1 to it** with the procedure in
[`appchain-owner-key.md`](./appchain-owner-key.md) §2 and verify on the ledger that the owner is now this
address, and register the registry's own chain with it (`appchain-cli register --name "UltraNet Games
Registry"`, 0.01 `$ULTRA`). Until chain #1 is handed over, the Games service's days are refused by the
chain and retried; until the registry chain exists, registry days are closed but not anchored.

### 2.2 The peer secret

Create it once and put the *same bytes* in both services' staging directories:

```bash
head -c 48 /dev/urandom | base64 | tr -d '\n' > records-peer-games.key && chmod 600 records-peer-games.key
cp records-peer-games.key /root/games-staging/games-records-peer.key
```

### 2.3 Encrypt every credential, then destroy the plaintext

```bash
sudo systemd-creds setup
for name in records-anchor-owner.key.enc records-anchor.pass records-peer-games.key; do
  sudo systemd-creds encrypt --with-key=host --name="$name" "$name" "/etc/ultranet-records/secrets/$name.cred"
done
sudo shred --remove records-anchor-owner.key.enc records-anchor.pass records-peer-games.key
```

**Make an offline, encrypted copy of `records-anchor-owner.key.enc` and of its passphrase, in two different
places, before you shred anything.** Once the chains are handed over, that file *is* the chains: lose it and
nobody can ever anchor them again. The passphrase is stored beside the key on the host, so credential
encryption protects a stolen disk or backup, **not** a compromised running service. That is the cost of
anchoring unattended (mode M2), and it is why this key is separate from the payout key.

### 2.4 Configure and verify, before anything is started

```bash
sudoedit /etc/ultranet-records/records.env   # RECORDS_ANCHOR_CLI, RECORDS_REGISTRY_CHAIN_ID, RECORDS_PEER_CREDENTIAL
sudo systemctl daemon-reload
sudo systemctl start ultranet-records
journalctl -u ultranet-records -n 40 --no-pager
```

The unit runs `ultranet-records check-config` before every start: a bad value, a limit loosened past its
ceiling, a missing credential, or the payout key in this unit's credentials stops the start with a message in
the journal. When it passes, the journal shows the limits in force: read them. The startup warnings tell you
what is switched off (`RECORDS_ANCHOR_CLI`, `RECORDS_REGISTRY_CHAIN_ID`, `RECORDS_PEER_CREDENTIAL`).

**Start order:** this service first, then the Games service. The Games service starts fine without it (it
cannot anchor, and says so), but starting this one first means the first daily pass succeeds.

### 2.5 Time

Days are UTC days by this host's clock, and the peer authentication rejects a request more than 60 seconds
from this clock. Run time synchronisation on **both** hosts' services (they share a machine, so one
`NTPSynchronized=yes` check covers both).

## 3. The reverse proxy and the rate limits

`deploy/Caddyfile.example` routes the registry and records paths here and everything else under
`/api/games/*` to the Games service. **`/internal/*` is never routed**: it is the signed route the Games
service uses, and it is reachable only on loopback. The example does **not** strip `Authorization` on these
routes, because it carries a game's API key; stripping it makes every result submission a 401. Prove it
through the proxy, not on loopback, before opening (go-live checklist).

Edge limits (Cloudflare, per `deploy/CLOUDFLARE_RULES.md`):

| Path | Method | Suggested limit per client |
|---|---|---|
| `/api/games/g` and `/api/apps` (register), `/api/games/g/*/rotate-key` and `/api/apps/*/rotate-key` | POST | 5 / hour (each is a post-quantum signature check) |
| `/api/games/g/*/results` and `/api/records` | POST | 600 / minute (a game's server sends many; the key is its login, the per-game daily cap is the real bound) |
| `/api/games/g`, `/api/games/g/*`, `/api/games/registry/*`, `/api/apps/*`, `/api/records/days/*`, `/api/records/status` | GET | 120 / minute |

Caddy is not installed where the example was written: run `caddy validate` on the host before reloading.

## 4. Everyday operation

```bash
ultranet-records status                       # whether it is accepting, and the registered games
ultranet-records game-list
ultranet-records disable --reason "why"       # stop accepting records and peer anchors
ultranet-records enable
ultranet-records backup --to /var/backups/ultranet-records/records-$(date -u +%Y%m%d).db
ultranet-records close-day --date <yesterday> # by hand; normally automatic
ultranet-records peer-usage [--date <day>]    # the Games service's anchor allowance for a day (database only)
ultranet-records peer-resolve …               # an operator's decision about a peer's anchor in doubt (§5.2; checks the node)
ultranet-records registry-resolve …           # the same for a registry day (§5.2; checks the node)
ultranet-records restore-mark --note "…"      # after restoring a backup: hold anchoring (§5.1, database only)
ultranet-records peer-reconcile [--apply] [--acknowledge-owner-transactions] [--clear-hold]   # §5.1, reads the node
```

`status`, `game-list`, `disable`, `enable`, `backup` and the `game-*` commands open the database and nothing
else: no key, no network. `close-day` loads the full configuration and credentials, like `serve`. A backup is
only a backup once it has been opened: `backup` opens the copy itself and says how many games it lists.
`disable` also stops this service answering the Games service's anchor requests (`service_disabled`).

### 4.1 Third-party games

```bash
ultranet-records game-register --id tetris --name "Tetris" --owner <64 hex>   # prints the API key ONCE
ultranet-records game-status --id tetris --status registered   # sandbox | registered | listed | suspended
ultranet-records game-rotate-key --id tetris                   # prints the new key ONCE; the old one stops
```

**The ladder.** A game starts in `sandbox`: private, unanchored, held to a small day. `registered` puts it in
the daily root and gives it a public page and proofs. `listed` adds it to the public directory. `suspended`
refuses its key and hides it; its past results stay in closed days, which are never rewritten. Promote only a
game whose owner you can reach; nothing here pays or moves money for any game.

**Self-service.** Developers register their own game and rotate a lost key without you (`ultranet-game-dev`,
or `POST /api/apps`, `POST /api/games/g`, and the rotate routes). The request carries the developer's public key
and a signature over the action, id, name and time (layout at the top of `src/games/ownership.rs`); the owner
is the address that key hashes to. Every self-registered game starts in `sandbox`. It is bounded because it is
free: `RECORDS_GAMES_PER_OWNER` (3) per owner, `RECORDS_SELF_REGISTRATIONS_PER_DAY` (100) per UTC day, and the
owner address must already be active on the chain (`RECORDS_REQUIRE_CHAIN_PRESENCE`). A rotation request is
refused if its timestamp is not later than the game's last change, so a captured request cannot be replayed;
rotating twice within one second is refused once.

**Ids and keys.** An id is 3 to 32 of `a-z 0-9 -`; `grid`, `chess`, `registry`, `admin` and similar names are
reserved. The key is shown once and only its hash is stored; if a developer loses it they rotate (or you do).
Check names against trademarks before promoting.

### 4.2 Anchoring

**The registry's day.** All registered games share one root per UTC day, anchored once, on
`RECORDS_REGISTRY_CHAIN_ID`. Before that variable is set (and `RECORDS_ANCHOR_CLI` configured) days still
close and their roots are fixed, and the anchor is made later: every unanchored day inside the 7-day catch-up
window is anchored when the chain exists. A day with no registered game is not anchored.

**A first-party game's day, for the Games service.** `POST /internal/v1/anchor {chain_id, root}`, signed. The
service looks at the ledger first: a root already there is answered with its transaction and signed for
nobody; a root submitted a moment ago returns the same transaction; if the ledger cannot be read, nothing is
submitted. The ledger is therefore checked by both services, and an anchor cannot be paid for twice by a
retry, a restart or a duplicate request.

### 4.3 The peer allowance and the rule that time is not evidence

The Games service may have at most `RECORDS_PEER_ANCHORS_PER_DAY` (default 8) **distinct roots** anchored per
UTC day, across all chains. The count lives in the Records database (`peer_anchor_days` and
`peer_anchor_roots`), changed together in one atomic transaction on a WAL database with `synchronous=FULL`.

* **A slot is taken when a root is *reserved*, before anything is submitted.** A crash can waste a slot and can
  never bypass the limit. The same root asked again, on that day **or any later day**, takes no slot: it is the
  same anchor.
* **An attempt is recorded before the signer is called**, so a crash leaves `submitting` with the attempt counted.
* **The ledger does not refuse a repeated root.** Each anchor takes the next sequence and pays its own fee, so a
  second submission of a root whose first outcome is unknown can cost a second fee. Therefore:
  * **`submitted`** (the node accepted it, the hash is recorded): answered with that transaction, **never**
    submitted again automatically, however long ago. Only the ledger showing it, or an operator, settles it.
  * **`submitting`** (the outcome is unknown): **in doubt**, and refused (`502 anchor_failed`, "in doubt"). It
    returns to `reserved` only if reconciliation **proves** the attempt never reached the node (below).
    Expiry of a time window is never that proof.
* **What counts as unknown.** Only "the signer could not be run at all" is known not to have submitted anything.
  Every other failure (the CLI ran and reported an error, a timeout, output that could not be read) may be an
  accepted transaction, because the CLI can fail after the node has already taken it.
* **The reconciliation that can prove it.** `appchain-cli` signs with the time it runs, so a transaction it made
  carries a timestamp within `[attempt − 120 s, attempt + CLI timeout + 120 s]`. After that window and a further
  margin have passed, the owner's address history (`/api/address/<owner>/transactions`: pending **and**
  confirmed) is read. If it reaches back past the window, is complete, and holds **no** AppChain transaction in
  the window, the first attempt did not reach the node and the root may be tried once more. If the history holds
  a transaction in the window (even a long-confirmed one: it could be this anchor), cannot be shown completely, or
  the node cannot be read, the root stays in doubt. This needs `RECORDS_OWNER_ADDRESS`, which `check-config` checks
  against the owner key.
* **A pending owner transaction blocks a new submission. The node does not protect against a duplicate.** The rehearsal
  against the real node code (`tools/verify/records-rehearsal.sh`) showed that `appchain-cli` signs with a
  **pending-aware** account nonce, so a second signer call while the first transaction is still pending is **accepted**,
  with the next nonce, and pays a second fee. Nothing in the node refuses it. The protection is this service's own: the
  state recorded around every attempt (above), and this check. It counts pending transactions two ways: the node's
  address history, and, when that list is full and hides them, the account nonce minus the newest confirmed nonce the
  owner sent, which is exactly the number pending. If neither can be established (no confirmed owner transaction to
  measure against, the node's answers disagree, or it cannot be read), nothing is submitted.
* **One root may be submitted at most `RECORDS_PEER_SUBMISSIONS_PER_ROOT` (3) times a day**, so a root that keeps
  failing cannot be retried into a pile of fees. The next UTC day is a new allowance and a new count.
* **Concurrent requests**, from threads or from two processes on one database, are serialised by SQLite's write
  lock: exactly the allowance is granted.
* **What the node does not show.** The node lists pending transactions only when the confirmed history leaves room in
  its 100-entry list; for an owner with 100 or more transactions they are left out of it (the rehearsal reproduces this
  with real node code). The nonce measure above covers that. It cannot cover the case where *nothing* the owner sent is
  listed as confirmed, which refuses. Independently, the owner-history check treats a confirmed history that does not
  reach back to its horizon as unreadable and refuses (§5.1).

`ultranet-records peer-usage` shows the day: slots used, each root, its state, attempts and transaction.

### 4.4 Usage and cost: the figures behind who pays

```bash
ultranet-records usage-report                     # the last 7 UTC days, today included
ultranet-records usage-report --date 2026-11-17   # one day
ultranet-records usage-report --from 2026-11-01 --to 2026-11-30
ultranet-records usage-report --days 30 --json    # same figures, for diffing month to month
ultranet-records usage-report --cost-basis confirmed   # only what this node can evidence
```

Each flag may be given at most once, and a window may be described one way only: a repeated flag, or `--date`
together with `--from`, `--to` or `--days`, is refused rather than letting the last one win.

**The cost basis.** A fee is charged when a transaction is *included in a block*; `submitted` only means a signer
returned a hash. So the report can count two ways, and says which one it used:

| `--cost-basis` | Counts | Use |
|---|---|---|
| `submitted` (default) | registry anchors accepted at submission **+ peer anchors** | what the owner has committed: the money that must be available |
| `confirmed` | registry anchors this node has confirmed only | what can be evidenced against the chain |

Every report prints four figures whichever basis is chosen, under `counted at this basis`, `not counted at this
basis` and `for reference`: registry `submitted`, `confirmed` and `awaiting`, peer `peer_without_evidence`, plus the
unsent count. `total at submission` is `submitted + peer_without_evidence`; `total at confirmed` is `confirmed`. A
confirmed-basis report therefore never shows a smaller headline without the larger one beside it.

**Peer anchors can never be confirmed here.** Their confirmation lives in the Games database, not this one. They are
reported as `peer_without_evidence` under both bases, and under `confirmed` they are not apportioned to applications
at all — this database holds no evidence for them and none of the records to apportion them against.

**What the day table says.** One row per UTC day, with the cost columns labelled separately — `cost`, `confirmed`,
`awaiting`, `peer cost`, `attributed` — so no two headings can run together. At the submitted basis a row's `cost` is
that day's `confirmed + awaiting + peer cost`; at the confirmed basis it is `confirmed` alone, and the other two columns
are printed but not counted into it. Every figure in that table is **base units**, and the report prints the conversion
directly under it (`1,000,000 base units = 1 ULTRA`); the summary then repeats every cost class, and each total, in base
units with its ULTRA equivalent beside it. Counts are written the way they read: `1 day`, `1 anchor`, `1 app`, never
`1 days`.

`--json` echoes the basis as `cost_basis`, and inside `totals.cost` and every `days[].cost` it prints the same four
classes plus `total_at_basis`, so a consumer never has to re-derive which subset the headline counted.

It answers four questions and nothing else: how many records each application had accepted per UTC day, which anchor
transactions were charged, which records each charge covered, and how much of the spend carried no records. It opens
the database **read-only** and needs no key, no network and no `appchain-cli`:

* `RECORDS_DB_PATH` and `RECORDS_REGISTRY_CHAIN_ID` are read straight from the environment. The full service
  configuration is deliberately not built, because that is how a report would end up holding a credential.
* The database is never created, migrated or modified. If `RECORDS_DB_PATH` is wrong the command says `no database`
  rather than creating an empty one that then reports zeroes for the wrong reason.
* Reading a WAL database can make SQLite create the `-wal`/`-shm` companions; the database file itself is never
  modified. Where those companions cannot be created — a directory the service user may not write — the outcome
  depends on the SQLite build: it either reads the database without them, or refuses with an explanation. What it
  never does is open the database writable to get its numbers.

**How a charge is apportioned.** An allocation set is a `(chain_id, attribution_day)` pair — one set per anchor, so
two chains anchoring on the same day are two sets and no charge is ever counted twice. Inside a set, each application
takes its integer share of the charge in proportion to its records; the units left over go to the largest fractional
remainders, ties broken by ascending application id. The shares of a set therefore add up to that set's charge exactly:
nothing is dropped, nothing is assigned twice, and nothing is carried to another day. `--json` prints every set under
`allocation_sets`, so the arithmetic can be recomputed by hand.

**What it cannot say, and the report says so itself.**

| | Why |
|---|---|
| A per-app figure is a *share*, not a measured charge | one anchor covers every application, so per-app cost is attribution |
| A peer anchor is reported on its **request** day, not the day it covers | the covered day lives in the Games database's `days` table, which this service does not own |
| `reserved` and `submitting` are reported as *unsent* and cost nothing | the outcome is unknown; see §5.2 |
| `submitted` is not called `confirmed` | only the registry's own day state, synced from the ledger, earns that word |
| A peer anchor cannot be confirmed here | its confirmation lives in the Games database; it is reported as `peer_without_evidence` under both bases |
| `anchors with no records` follows the chosen basis | under `confirmed` only confirmed anchors are counted, so the figure is smaller and means less |
| `confirmed` means "this node's ledger sync saw it" | a lagging sync under-counts, so the error is toward understating cost, never overstating it |

Read it as: at the default basis, **cost per UTC day** is what the owner has committed — every accepted submission,
including peer anchors whose confirmation this service cannot observe. Under `--cost-basis confirmed` it is only what
the node has evidenced, which is smaller for a reason the report states beside the figure. **anchors with no records**
is the part of the chosen total that buys nothing but liveness. Neither figure is a bank statement: this service knows
what it submitted and what its own ledger sync has seen, not what a wallet will be debited.

## 5. When something goes wrong

| Symptom | Likely cause | What to do |
|---|---|---|
| Games service logs `peer_unauthenticated` | the two peer secrets differ, or the clocks differ by over 60 s | compare the two credential files' contents (never print them in a ticket); check `timedatectl` |
| Games service logs `forbidden` | the chain is not in `RECORDS_PEER_ANCHOR_CHAINS`, or the day's `RECORDS_PEER_ANCHORS_PER_DAY` is used up (normal use is one anchor per game per day, so a used-up cap is a signal, not a nuisance) | add the game's chain (never the registry's) and restart; investigate a used-up cap before raising it |
| `anchoring_disabled` (503) | `RECORDS_ANCHOR_CLI` unset | set it, restart |
| `anchor_failed` (502) | `appchain-cli` failed: owner wallet empty, key not the chain's owner, node unreachable | `journalctl -u ultranet-records`; check the owner balance |
| A registry day stays `closed` / `anchored` for hours | the anchor is not landing | `close-day --date <day>` shows `last error` |
| `chain_unavailable` | the node's ledger API is unreachable | nothing was submitted; it retries |
| Every result submission is a 401 through the proxy | the proxy strips `Authorization` | fix the Caddy route (§3) |
| Games service sees `internal_error` on every anchor; the journal says "peer anchor accounting is unusable" | the stored allowance is inconsistent or corrupt (see §5.1) | follow §5.1; **do not** edit the tables to "make it work" |
| `reconciliation_required` (503) | the database is held after a restore, the fence shows it was replaced, the ledger shows an anchor it has no record of, or the owner has signed more transactions than it recorded attempts for | §5.1: `peer-reconcile`, then `--apply --clear-hold` (and `--acknowledge-owner-transactions` if you know why) |
| `anchor_failed` … "in doubt" | a submission's outcome is unknown and cannot yet be proved | §5.2 |
| `anchor_failed` … "still pending" | an owner transaction is pending in the node | wait for it to be mined; it retries by itself |
| The service will not start: "must not hold the games payout credentials" | a Games credential is in this unit's directory | remove it; it belongs to `ultranet-games` |

### 5.1 Restoring a backup: what is detected, and the fail-closed procedure

A restored database may remember fewer reservations and attempts than the live service made: it may have anchored
roots after the backup was taken. A service that simply started on it would hand out an allowance it had forgotten it
spent, and could re-submit a root or a registry day whose first attempt it has forgotten. A restore is therefore not
complete until the database has been **reconciled with the ledger and the owner's history**.

**What the service detects by itself** (any one of these holds anchoring, durably, and answers `503
reconciliation_required`; nothing is signed, for peers and for the registry alike):

| Situation | Detected by | Horizon / limit |
|---|---|---|
| The database file was replaced by an older copy, the fence file left in place (`cp backup records.db`) | the **fence**: a counter in the database and a copy beside it (`records.db.fence`) | always, at the next anchor decision. A missing, damaged or foreign fence file counts too |
| The ledger shows an anchor on a peer chain, or on the registry chain, that no record here accounts for | the **ledger check** | anchors since 00:00 UTC yesterday, as far as the **anchors index** has caught up |
| The owner has signed more AppChain transactions than this database recorded attempts for (an older backup restored with its fence file, a lost attempt record, an anchor already mined while the index lags, or the owner key used elsewhere) | the **owner-history check**: the owner's confirmed history comes from the chain itself, not from the index | transactions since 00:00 UTC yesterday, or since your last acknowledged reconciliation if later |
| The owner's confirmed history cannot be read back that far | the owner-history check, which then refuses | same horizon |

**What it cannot detect.** State this plainly to anyone relying on it:

* **Restoring the whole state directory** (the database *and* its fence file, consistent with each other) is invisible
  to the fence. It is caught only by the owner-history check, and only for transactions inside its horizon. A
  discrepancy older than yesterday 00:00 UTC is not seen.
* **A pending transaction the node hides, lost with a restore.** The node leaves pending transactions out of the owner's
  history once the confirmed history fills its list. The service counts them by the account nonce when it is about to
  submit (§4.3), so a restored database does not submit on top of one; but a pending anchor that this database has *no
  record of* is not counted against any root, so it is not recognised as the same anchor. It is mined within a block or
  two and then visible to the ledger and the owner-history checks.
* **Owner transactions made elsewhere** (registering a chain, a manual `appchain-cli` call) look exactly like a
  forgotten attempt. The service refuses until you acknowledge them. This is a false alarm by design, not a miss.
* Someone with root on the host can edit or replace the database and the fence file together, and could as easily
  replace the binary. None of this defends against that.

So **the explicit procedure below is a hard operational prerequisite of every restore**; the automatic checks are the net,
not the plan.

1. Stop the service. Keep the damaged database aside (`cp records.db records.damaged.db`); never delete it.
2. Put the backup in place, **and delete the old `records.db-wal` and `records.db-shm` files**. Leave the fence file alone.
3. `ultranet-records restore-mark --note "restored from <which backup>"`. (Database only, no keys.) This sets the hold
   explicitly; without it, the fence sets it at the first anchor decision.
4. Start the service. It answers `reconciliation_required` and signs nothing.
5. `ultranet-records peer-reconcile` (it reads the node). It reports the fence state; every anchor the ledger shows on the
   peer chains or the registry chain since yesterday that this database does not know (`UNKNOWN …`); any pending owner
   transaction; and how many AppChain transactions the owner has signed against how many attempts were recorded.
6. `ultranet-records peer-reconcile --apply` adopts each unknown peer anchor as a used slot, and each unknown registry
   anchor against the closed day whose root it carries. One that matches no closed day cannot be adopted: investigate it.
7. If it reports `UNACCOUNTED` owner transactions and you know why (you registered a chain, you ran the CLI by hand),
   add `--acknowledge-owner-transactions`: everything up to now is then treated as accounted for. **Do not acknowledge
   what you cannot explain.**
8. Wait for any pending owner transaction to be mined. Then `ultranet-records peer-reconcile --apply --clear-hold`. It
   **refuses** to release the hold while unknown anchors remain, while owner transactions are unaccounted for, while the
   owner has pending transactions, or (unless you pass `--i-have-checked-for-pending-transactions` after checking the
   node yourself) while the node cannot show its pending list. On success it also makes the fence describe the database
   as it now is.
9. Check `ultranet-records peer-usage`. A restored database has also lost every registry record written since the backup;
   that is the larger cost of a restore, and no command brings it back.

If the stored allowance is *inconsistent* rather than old (the counter disagrees with the rows, or a row does not
decode), the service refuses to anchor for that day, signs nothing and changes nothing. `peer-usage` prints the
inconsistency. Establish what the ledger holds for that day, take a backup, then either restore as above or, if the
rows are right and only the counter is wrong, stop the service and set the counter to the number of rows
(`sqlite3 records.db "UPDATE peer_anchor_days SET used = (SELECT COUNT(*) FROM peer_anchor_roots WHERE day='<day>') WHERE day='<day>';"`).

**After any manual use of the owner key** (registering a chain, a hand-run `appchain-cli anchor` or `transfer`), run
`ultranet-records peer-reconcile --acknowledge-owner-transactions`, or the service will refuse to anchor until you do.

### 5.2 An anchor in doubt: the operator's procedure

Symptom: the Games service logs `anchor_failed` with "in doubt", or `ultranet-records peer-usage` shows a root as
`submitting`. The service will not submit it again by itself while it cannot tell whether the first attempt landed.

1. `ultranet-records peer-usage --date <day>` gives the root, its chain, the attempts and the time.
2. **Find out what the node holds.** Is the root on the ledger? `curl -s https://api.ultranetwork.cc/api/appchain/ledger/<chain>/anchors`.
   Is a transaction from the owner pending or recent? `curl -s "https://api.ultranetwork.cc/api/address/<owner>/transactions?limit=100"`
   (look for `recipient: appchain` near the time of the attempt). Check the owner's nonce.
3. **If a transaction exists:** record it so it is not repeated. A confirmed one that the index shows under this root is recorded as landed; a pending one is recorded as a claim only (add `--pending-unverified-ok`) and the service settles it when it is mined (see the registry rules above):
   `ultranet-records peer-resolve --date <day> --chain <id> --root <root> --landed <tx hash> --note "found pending at <time>"`.
   The service then answers with that transaction, and the ledger confirms it when it lands.
4. **If you are certain nothing was submitted** (the node never saw it, the owner's history has no transaction in the
   window, the CLI never ran): `ultranet-records peer-resolve … --not-submitted --note "<what you checked>"`. The root
   may then be submitted again; the attempt stays counted, and the per-root cap still applies.
5. **If you cannot tell, do not resolve it.** An anchor that is late is a nuisance; a duplicated anchor is a fee. The
   day's anchor can wait: the allowance resets at UTC midnight and the same root is still the same anchor tomorrow.

The service does the proof by itself when it can (§4.3), so in practice this procedure is for a history the node
cannot show completely, or a transaction in the window that you can identify as not being this anchor.

**The registry's own daily anchor follows the same rules.** Each closed registry day has an anchor record
(`registry_anchor_attempts`) in the same states: an attempt is recorded before the signer is called; a `submitted`
day is never submitted again automatically; a `submitting` day is in doubt until reconciliation proves the attempt never
reached the node (the owner's complete history shows no AppChain transaction in its window), a pending owner transaction
blocks a new submission, and the hold, fence, ledger and owner-history checks all apply. Its symptom is `last error`
containing "in doubt" in `close-day`'s output or on the day. To settle one by hand:

* `ultranet-records registry-resolve --date <day> --landed <tx hash> --note "..."` records a transaction the operator
  found. **The claim is checked against the node first**: the transaction must exist, be signed by the owner address,
  and be an AppChain transaction. A **mined** one is recorded as landed only if the anchors index also shows exactly this
  day's root, on the registry chain, under exactly this transaction; a different chain, root, owner or transaction is
  refused, and so is one the index does not show yet (wait for it).
* **A pending transaction is only a claim.** Its chain and root cannot be read until it is mined, so `--landed` refuses it
  unless you add `--pending-unverified-ok`, and even then it is recorded as a **claim**, never as landed: the day stays
  in doubt (not anchored, not resubmitted) and every pass says why. The service verifies the claim itself once the
  transaction is mined and indexed (owner, AppChain, and the root on the registry chain under that hash); if it is
  pending, unknown to the node (dropped), mined but not shown by the index under that root, or somebody else's, it stays
  in doubt with that reason. **A claim cannot silence an anchor for good:** when you have seen enough, clear it with
  `--not-submitted`, which also forgets the named hash. One transaction hash cannot be claimed for two anchors. The flag
  changes nothing for a mined transaction, which is always verified in full.
* `ultranet-records registry-resolve --date <day> --not-submitted --note "..."` returns the day to `reserved` when you
  are certain nothing was submitted.
* `peer-resolve` takes the same validated claim, with the same rules, for a peer's root.

**Restarts are safe.** The service keeps nothing that matters in memory except the peer replay cache (a
request replayed within 60 seconds of a restart is answered from the record and never anchored twice): records,
days and the peer allowance are in the database. Restarting between submitting an anchor and noting it leaves the
root in doubt (§4.3), which is refused, not repeated.

**Rotating the peer secret.** Generate a new one, install it in both services' credentials, restart both. In
between, days are not anchored (the Games service logs `peer_unauthenticated`) and nothing is lost: the next
pass anchors them.

## 6. Upgrading

The database records its schema version, and a build refuses to open a database whose version it does not
understand. `backup` first, install the new binary, restart. Build both services from the same revision.
