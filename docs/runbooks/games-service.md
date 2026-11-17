# The Games Reward Service — installing and running it

**Service:** `ultranet-games` (UltraGrid and Ultra Chess: checks answers, records wins, pays rewards) ·
**Port:** `127.0.0.1:8095`, published as `/api/games/*` (except the registry paths) on the API host
**Its sibling:** the Records Service (`ultranet-records`, [`records-service.md`](./records-service.md)) holds
the chain owner's key, serves third-party games and the generic records API, and **anchors this service's
days when it asks**. The two are separate processes with separate users, credentials, configuration and
databases. Nothing is shared but one secret that lets this service ask for an anchor; read §0.
**Rules and the reasoning behind every number:** [`../GAMES_RULES_AND_PAYOUTS.md`](../GAMES_RULES_AND_PAYOUTS.md)
**State of this runbook:** written and its commands exercised against a local build and the live public
API (read-only). **It has not been run on the production host, and no live payout has been made.**

## 0. The boundary between the two services

| | Games Reward Service (this) | Records Service |
|---|---|---|
| Holds | the **payout key**, the abuse key, the peer secret | the **chain owner's key** and passphrase, the peer secret |
| Never holds | any chain owner's key | any payout key |
| Database | `/var/lib/ultranet-games/games.db` | `/var/lib/ultranet-records/records.db` |
| Unix user | `ultranet-games` | `ultranet-records` |
| Serves | `/api/games/status`, `grid/*`, `chess/*`, `wallet/*`, `withdraw`, `payouts/*` | the registry (`/api/games/g…`, `/api/games/registry/…`), `/api/records…`, `/api/apps…`, and the signed `/internal/v1/anchor` |
| Anchoring | asks the Records Service (a signed request over loopback) | signs, with `appchain-cli` |

`check-config` and `serve` **refuse to start** if the owner key or its passphrase is in this unit's
credentials directory (`games-anchor-owner.key.enc`, `games-anchor.pass`, or their `records-` names). That
turns "this service does not hold the owner key" from a convention into a check. If the Records Service is
down, this service goes on recording wins and paying: the day's anchor fails, the error is recorded, and
the next pass anchors it once the Records Service is back. A day is never anchored twice (§5).

The shape is the faucet's: a loopback service, its own user, secrets as systemd credentials, a SQLite
file, a kill switch. It runs as a *separate* service from both the faucet and the node, and it must never
share a key with either.

---

## 1. What lives where

| Thing | Path | Notes |
|---|---|---|
| Binary | `/opt/ultranet-games/ultranet-games` | built from the **same source revision** as the Records Service |
| Unit | `/etc/systemd/system/ultranet-games.service` | from `deploy/ultranet-games.service` |
| Policy | `/etc/ultranet-games/games.env` | `root:ultranet-games`, `0640`; policy and endpoints only, **no secrets** |
| Secrets | `/etc/ultranet-games/secrets/*.cred` | systemd encrypted credentials |
| Database | `/var/lib/ultranet-games/games.db` | `0700` directory; WAL mode |
| Backups | `/var/backups/ultranet-games/` | encrypted, away from the signer credentials |

| Credential | What it is | Made with |
|---|---|---|
| `games-payout-signer.json` | the **payout wallet's** key: pays players. Its own key | `ultranet-faucet keygen --output …` |
| `games-abuse.key` | 32+ random bytes keying the privacy digests of client addresses | `openssl rand -base64 48` |
| `games-records-peer.key` | the secret shared with the Records Service (the same bytes as its `records-peer-games.key`) | `head -c 48 /dev/urandom \| base64 \| tr -d '\n'` |

**Three keys, three jobs, never mixed:** the payout wallet pays players (this service); the owner key anchors
chains (the Records Service); the faucet key funds newcomers. A compromise of one must not be a compromise of the others, so each is its
own file, its own address and its own balance.

---

## 2. Install

```bash
# Build from one revision.
cargo build --release --bin ultranet-games --bin ultranet-faucet

sudo useradd --system --home-dir /var/lib/ultranet-games --shell /usr/sbin/nologin ultranet-games
sudo install -d -o ultranet-games -g ultranet-games -m 0700 /var/lib/ultranet-games
sudo install -d -o root -g ultranet-games -m 0750 /etc/ultranet-games /etc/ultranet-games/secrets
sudo install -d -o root -g root -m 0750 /opt/ultranet-games
sudo install -o root -g root -m 0755 target/release/ultranet-games /opt/ultranet-games/
sudo install -o root -g ultranet-games -m 0640 deploy/games.env.example /etc/ultranet-games/games.env
sudo install -o root -g root -m 0644 deploy/ultranet-games.service /etc/systemd/system/ultranet-games.service
```

### 2.1 The payout wallet

```bash
sudo mkdir -m 0700 /root/games-staging && cd /root/games-staging
# run from the build tree, using the binary built in §2 (the faucet's key generator makes this key too)
/path/to/ultranet/target/release/ultranet-faucet keygen --output games-payout-signer.json   # prints the address
```

Put that address in `GAMES_PAYOUT_ADDRESS`. Never run `keygen` over `sovereign_keys.json`, a validator
key or the faucet's signer. **Do not use this wallet for anything else:** every payout uses its nonce
sequence, and a transfer from elsewhere would break it.

### 2.2 The peer secret (only if days are to be anchored)

This service holds **no** owner key. To have a day anchored it asks the Records Service, and proves who
it is with a shared secret:

```bash
head -c 48 /dev/urandom | base64 | tr -d '\n' > games-records-peer.key && chmod 600 games-records-peer.key
```

Give the *same bytes* to the Records Service as its `records-peer-games.key` (see `records-service.md`
§2.2), then forget them here. The owner key, its generation and the hand-over of chain #1 are the Records
Service's: [`appchain-owner-key.md`](./appchain-owner-key.md).

### 2.3 Encrypt every credential, then destroy the plaintext

```bash
head -c 48 /dev/urandom | base64 > games-abuse.key
sudo systemd-creds setup
for name in games-payout-signer.json games-abuse.key games-records-peer.key; do
  sudo systemd-creds encrypt --with-key=host --name="$name" "$name" "/etc/ultranet-games/secrets/$name.cred"
done
sudo shred --remove games-payout-signer.json games-abuse.key games-records-peer.key
```

**Make an offline, encrypted copy of `games-payout-signer.json` before you shred anything**: it is the key to
the payout wallet and its balance. The owner key is not here; its offline copies are the Records Service's
duty (`records-service.md` §2.3).

### 2.4 Configure and verify, before anything is started

```bash
sudoedit /etc/ultranet-games/games.env      # set GAMES_PAYOUT_ADDRESS; GAMES_RECORDS_URL and GAMES_ANCHOR_GAMES to have days anchored
sudo systemctl daemon-reload
sudo systemctl start ultranet-games
journalctl -u ultranet-games -n 40 --no-pager
```

The unit runs `ultranet-games check-config` before every start (`ExecStartPre`), under the same user and
the same credentials as the service itself. A bad value, a loosened rule, a payout key that does not match
`GAMES_PAYOUT_ADDRESS`, or a missing credential stops the start with a clear message in the journal
instead of starting it half-configured. When it passes, the journal shows the rules in force: read them.
(`check-config` can also be run on a development machine with `CREDENTIALS_DIRECTORY` pointing at a folder
of *throwaway* plaintext credentials; never do that with real keys.)

**Read the two game lines and check they say what you meant.** `Ultra Chess: … RECORD ONLY` is the
correct line today: chess pays nothing unless the environment says `GAMES_CHESS_PAYS=true`, because the
server cannot verify a chess result (`../GAMES_RULES_AND_PAYOUTS.md` §2.2, §6 P1). Wins are still
recorded and anchored, so leaving it alone loses nothing but the prizes. If that line says `paying` and
you did not mean it, stop and remove the line.

### 2.5 Time

Days are UTC days by this host's clock. Run time synchronisation and check it: a wrong clock moves the
day boundary and nothing else in the system will tell you.

```bash
timedatectl show -p NTPSynchronized      # NTPSynchronized=yes
```

---

## 3. The reverse proxy and the rate limits

`deploy/Caddyfile.example` routes `/api/games/*` to `127.0.0.1:8095` on the same API host, except the registry paths
(`/api/games/g…`, `/api/games/registry/…`) and `/api/records*` / `/api/apps*`, which go to the Records Service at
`127.0.0.1:8096`, and `/internal/*`, which is not routed at all. The app needs
no new name or certificate, and strips the headers a client could forge. The service has **caps, not a
login**, so put these limits at the edge (the Records Service's own limits are in `records-service.md` §3) (Cloudflare, per `deploy/CLOUDFLARE_RULES.md`) before opening it:

| Path | Method | Suggested limit per client |
|---|---|---|
| `/api/games/grid/answers`, `/api/games/chess/results` | POST | 30 / minute |
| `/api/games/withdraw` | POST | 6 / minute |
| `/api/games/wallet/*`, `/api/games/payouts/*`, `/api/games/status`, `/api/games/grid/day` | GET | 120 / minute |

The Caddyfile edit in this repository was **not machine-validated** (Caddy is not installed where it was
written): run `caddy validate` on the host before reloading.

---

## 4. Going live — in this order

1. [ ] Credentials created, encrypted, offline copies made, plaintext shredded (§2).
2. [ ] `check-config` is clean; the rules printed are the ones you ratified.
3. [ ] Owner key funded; chain #1 handed over and **verified on the ledger** (anchoring only).
4. [ ] Chess chain registered (0.01 `$ULTRA`); `GAMES_ANCHOR_GAMES=grid,chess` (anchoring only —
   registering the chain does **not** turn chess's prizes on; `GAMES_CHESS_PAYS` does, and P1 says leave
   it false).
5. [ ] Payout wallet funded: 0.1 `$ULTRA` from the faucet's one claim per chain (V2 F-2).
6. [ ] Service started (§2.4). It listens on loopback only, so nothing outside can reach it yet.
7. [ ] Reverse proxy route and Cloudflare limits in place; `curl https://<api>/api/games/status` answers.
8. [ ] `ultranet-games backup --to …` works, and a copy was restored somewhere and passed `audit`.
9. [ ] Monitoring is wired for the signals in `GAMES_RULES_AND_PAYOUTS.md` §7.
10. [ ] `enable`, then **one real round trip with your own address**. Watch the arithmetic first: the
    withdrawal minimum is 100 units, Grid pays 10 units for each of 3 rewarded wins a day, and chess
    pays nothing while it is record-only — so at the shipped numbers one address needs **four days of
    play** to reach a first withdrawal. To test the payout path today instead of in four days, run a
    throwaway database (`GAMES_DB_PATH` at a scratch file) with `GAMES_GRID_REWARD_UNITS=100` — inside
    the ceiling — and discard it afterwards; never point a test at the production database. Then
    withdraw your own address, watch it go `queued → submitted → confirmed`, and compare the amount that
    arrived with the amount owed.
11. [ ] Only then ship the app build that shows the rewards screens.

---

## 5. Everyday operation

```bash
ultranet-games status                         # the switches, the totals, the open payouts
ultranet-games audit                          # re-derives every balance; exits 1 on any disagreement
ultranet-games payouts                        # unfinished payouts, with attempts and the last error
ultranet-games backup --to /var/backups/ultranet-games/games-$(date -u +%Y%m%d).db
```

`status`, `audit`, `payouts`, `backup`, `disable`, `enable` and `payout-resolve` open the database and
nothing else: they need no key and make no network call, so they are safe to run on a misbehaving
service. Run `audit` and a backup daily. A backup is only a backup once it has been opened and audited;
`backup` does that itself and says so.

**Top-ups.** `status` shows what is still owed. Fees take about 1 % of everything paid, so the wallet
drifts below the balance it is promised against. When `owed + the 1,000-unit reserve` nears the wallet's
balance, rewards stop (safely: wins are recorded as `pool_low`), so top the wallet up before then.

**Chain days.** The service closes yesterday and asks the Records Service to anchor it, every ten minutes
until it is on the ledger, so normally you watch rather than run anything: `journalctl -u ultranet-games -f` and
`GET /api/appchain/ledger/1/anchors`. `close-day --game grid --date <yesterday>` does it by hand, but it
loads the full configuration and the credentials the unit holds, so it is not a plain-shell command —
`status`, `audit`, `payouts` and `backup` are the four that only open the database. `close-day` is safe to
repeat: a day closes once, and the ledger is checked before anything is submitted, by this service and again by
the Records Service, so an anchor is never paid for twice. If the Records Service is down the command says so
in `last error` and nothing is lost: run it again when it is back.

### 5.1 Third-party games

Not here. The registry, self-service registration, `game-register` and its siblings, and the generic records
API belong to the Records Service: [`records-service.md`](./records-service.md). This binary has no registry
commands.

---

## 6. When something goes wrong

| What you see | What it means | What to do |
|---|---|---|
| `ERROR … payouts paused` in the journal, `status` shows `paying out: false` | A human is needed. The reason is in the log and in `status` | Read it, fix the cause, then `ultranet-games enable --paying` |
| …reason: the payout wallet balance is below what the next payout needs | The wallet is short | Top it up. The payout is still queued and the player's balance is intact; enable and it goes out |
| …reason: a payout was not confirmed in time / not found on the node | A payout was sent and the chain has not confirmed it | `ultranet-games payouts` for the hash. **Look it up on the chain.** On it: `payout-resolve --id … --confirmed --note "…"`. Not on it: `--failed` (the balance returns to the player). Never guess |
| …reason: the node returned a different hash / answered about a different transaction | Something between the service and the node is wrong | Do not resolve anything yet. Check the node, the proxy and the payout hash by hand |
| …reason: the node rejected a signed payout | The node refused a request this service signed | Check the node's logs. Not on the chain: `--failed`. Then investigate before enabling |
| `audit` prints `PROBLEM:` | The books disagree | `disable --paying`. Keep the database. Do not restore over it: copy it away, then compare with the last audited backup |
| A day stays `closing` or `anchored` for hours | The anchor is not landing | `close-day` for the day shows `last error`. Common: the Records Service is down or its peer secret differs from this one's (`peer_unauthenticated`); owner key not yet the chain's owner; owner wallet empty; node unreachable (check `journalctl -u ultranet-records`) |
| Players report `pool_low` | Owed + reserve exceeds the wallet | Top up |
| Players report a date error | The phone's clock or the server's differs | Check `timedatectl` on the host first |
| **The payout key is leaked** | Whoever holds it can drain the wallet | `disable --paying` at once; move the remaining balance out with the key while you still control it; generate a new key; point `GAMES_PAYOUT_ADDRESS` at it |
| **The owner key or its passphrase is leaked** | Whoever holds both can anchor *and hand the chain to themselves* | Transfer the chain to a newly generated key **from the leaked key**, immediately (`appchain-owner-key.md` §5), then destroy the old one |
| **The owner key file is lost** | Nobody can ever anchor that chain again | Restore the offline copy. If there is none, this is final: the chain stays readable and its anchors stay verifiable |
| The database is lost | Wins and balances are gone | Restore the latest backup. Players' balances after that backup are lost to this service; the day anchors on the ledger remain the record of what was accepted |

**The kill switch.** `ultranet-games disable --accepting --paying --reason "…"` stops new results and all
payouts; every command that disables needs a reason, so the next person reading `status` knows why.
Players keep their balances.

---

## 7. Upgrading

The database records its schema version, and a build refuses to open a database whose version it does not
understand, so an old binary cannot damage a newer database or the reverse.

1. `backup` and `audit` first.
2. Stop the service (`TimeoutStopSec=30s` lets an in-flight payout step finish between signing and saving).
3. Install the new binary and start it. `ExecStartPre` re-checks the configuration.
4. `status` and `audit` again.

Rolling back is installing the old binary **only if the schema version is unchanged**; otherwise restore
the pre-upgrade backup.
