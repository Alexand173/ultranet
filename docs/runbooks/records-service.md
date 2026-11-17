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
cannot drain the owner wallet by asking for many (each anchor costs 0.001 `$ULTRA`). It cannot anchor the
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
| The service will not start: "must not hold the games payout credentials" | a Games credential is in this unit's directory | remove it; it belongs to `ultranet-games` |

**Restarts are safe.** The service keeps nothing that matters in memory: records and days are in the
database, and an anchor of unclear outcome is settled from the ledger. Restarting between submitting an anchor
and noting it cannot produce a second anchor.

**Rotating the peer secret.** Generate a new one, install it in both services' credentials, restart both. In
between, days are not anchored (the Games service logs `peer_unauthenticated`) and nothing is lost: the next
pass anchors them.

## 6. Upgrading

The database records its schema version, and a build refuses to open a database whose version it does not
understand. `backup` first, install the new binary, restart. Build both services from the same revision.
