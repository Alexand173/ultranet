# Games platform — go-live checklist (owner's steps)

Written for the owner running the host. It lists only what you do, in order, with the check that proves each
step. The reasons are in [`games-service.md`](./games-service.md) (the Games Reward Service) and
[`records-service.md`](./records-service.md) (the Records Service); section numbers below point there.
**There are two services and they never share a key.** Nothing here has been run on the production host. Do
not skip a check: each one catches a mistake that is cheap now and expensive later.

Lines starting `HOST:` run on the server. Lines starting `DEV:` run on your build machine.

## A. Build (one revision for everything)

```bash
DEV: git checkout feat/ultragrid-engine && git rev-parse HEAD            # write this hash down
DEV: cargo test --lib games:: -- --test-threads=1                         # expect: ok, 0 failed
DEV: cargo build --release --bin ultranet-games --bin ultranet-records --bin appchain-cli --bin ultranet-faucet --bin ultranet-game-dev
```
- [ ] The test run says `0 failed`, and the hash you wrote down is the one you build and install, for both services.

## B. Keys: three jobs, three places (games §2.1–2.3, records §2.1–2.3)

```bash
HOST: sudo mkdir -m 0700 /root/games-staging /root/records-staging
HOST: cd /root/games-staging   && ultranet-faucet keygen --output games-payout-signer.json    # the PAYOUT address: note it
HOST: head -c 48 /dev/urandom | base64 > games-abuse.key
HOST: cd /root/records-staging && appchain-cli generate-key --out records-anchor-owner.key.enc # the OWNER address: note it
HOST: printf '%s' '<long random passphrase>' > records-anchor.pass && chmod 600 records-anchor.pass
HOST: head -c 48 /dev/urandom | base64 | tr -d '\n' > records-peer-games.key && chmod 600 records-peer-games.key
HOST: cp records-peer-games.key /root/games-staging/games-records-peer.key
```
- [ ] **Offline encrypted copies** of the owner key file and its passphrase (two different places), and of the
  payout key, before anything is shredded. Once the chains are handed over, the owner key file *is* the chains.
- [ ] The payout address and the owner address are different, and neither is the faucet's.
- [ ] `/root/games-staging` holds the payout key, abuse key and peer secret and **no owner key**;
  `/root/records-staging` holds the owner key, passphrase and peer secret and **no payout key**.
- [ ] Encrypt each directory's files into **its own** service's secrets directory and shred the plaintext
  (games §2.3, records §2.3): `ls /root/games-staging /root/records-staging` shows nothing left.

## C. Chains on the ledger (the Records Service's owner key must hold funds first)

Fund the **owner address** (B) with enough for the fees: a registration is 0.01 `$ULTRA` and an anchor is
0.001 `$ULTRA`, so at least 0.05 `$ULTRA` covers the registry chain and a month of daily anchors (the Records
Service anchors the registry once a day and Grid once a day: 0.002 `$ULTRA` a day).

```bash
HOST: appchain-cli address  --key-file records-anchor-owner.key.enc         # confirm: the OWNER address
HOST: appchain-cli register --key-file records-anchor-owner.key.enc --name "UltraNet Games Registry"
```
- [ ] The ledger lists the new chain with **your owner address as owner**:
  `curl -s https://api.ultranetwork.cc/api/appchain/ledger | head -c 600` (note its chain id).
- [ ] Chain #1 (UltraGrid) is handed to the owner key by `appchain-owner-key.md` §2, and the ledger shows the
  new owner. Until then Grid's days are refused by the chain and retried.
- [ ] Chess chain: register only if you will anchor chess; it does **not** turn prizes on. If you do, it is
  in `RECORDS_PEER_ANCHOR_CHAINS`, which has **no default**: name a chain only once the Records owner key
  actually owns it, because the anchor is that key's transaction and L1 refuses it anywhere else.

- [ ] **After registering chains or any other use of the owner key by hand**, run `ultranet-records peer-reconcile --acknowledge-owner-transactions`:
  the service counts the owner's transactions against its own recorded attempts and refuses to anchor until you say that
  the ones it did not make are yours (`records-service.md` §5.1).

## D. Configure and check, Records Service first (records §2.4, games §2.4)

```bash
HOST: sudoedit /etc/ultranet-records/records.env
HOST: sudoedit /etc/ultranet-games/games.env
```
Records: `RECORDS_ANCHOR_CLI=/opt/ultranet-records/appchain-cli`, `RECORDS_REGISTRY_CHAIN_ID=<the id from C>`,
`RECORDS_PEER_CREDENTIAL=records-peer-games.key`, `RECORDS_OWNER_ADDRESS=<the OWNER address from B>` (required for any anchoring). Games: `GAMES_PAYOUT_ADDRESS=<payout address>`,
`GAMES_RECORDS_URL=http://127.0.0.1:8096`, `GAMES_ANCHOR_GAMES=grid`. Leave `GAMES_CHESS_PAYS=false` and
`GAMES_REQUIRE_CHAIN_PRESENCE=true`.
```bash
HOST: sudo systemctl daemon-reload && sudo systemctl start ultranet-records
HOST: sudo systemctl start ultranet-games
HOST: journalctl -u ultranet-records -u ultranet-games -n 60 --no-pager
```
- [ ] Each journal shows the rules or limits in force and no error. `Ultra Chess: … RECORD ONLY` is correct.
- [ ] `ultranet-records check-config` says "Owner address: … (matches the owner key)", "Payout key: not held here" and `ultranet-games check-config` says
  "Owner key: not held here".
- [ ] `timedatectl show -p NTPSynchronized` says `yes` (peer requests are refused beyond 60 s of skew).
- [ ] `curl -s 127.0.0.1:8095/api/games/status` and `curl -s 127.0.0.1:8096/api/records/status` answer on the host.

## E. Money (only now)

- [ ] Fund the **payout address** (B): 0.1 `$ULTRA` from the faucet's one claim (V2 F-2).
- [ ] `ultranet-games status` shows the balance and `owed 0`.

## F. The edge (games §3, records §3)

- [ ] Add the Caddy routes from `deploy/Caddyfile.example` (Records paths to 8096, `/api/games/*` to 8095,
  `/internal/*` not routed); run `caddy validate --config <file>` **before** reloading.
- [ ] Add every Cloudflare limit in both runbooks' §3 tables, including register, rotate-key and results.
- [ ] The proxy forwards the `Authorization` header to the Records Service (the example does; a copy that
  strips it makes every result submission a 401). Prove it through the proxy, not on loopback: a rehearsal
  game's key must get `200` from `POST https://api.ultranetwork.cc/api/records`.
- [ ] `curl -s -o /dev/null -w '%{http_code}' -X POST https://api.ultranetwork.cc/internal/v1/anchor` is **404** from outside.
- [ ] From outside: `curl https://api.ultranetwork.cc/api/games/status` answers, `.../api/games/g` answers, and
  `curl -s -o /dev/null -w '%{http_code}' -X POST https://api.ultranetwork.cc/api/games/g -d '{}'` is `400`, not `5xx`.

## G. Safety nets

```bash
HOST: ultranet-games   backup --to /var/backups/ultranet-games/games-$(date -u +%Y%m%d).db
HOST: ultranet-games   audit
HOST: ultranet-records backup --to /var/backups/ultranet-records/records-$(date -u +%Y%m%d).db
```
- [ ] Both backups opened themselves. **Copy each elsewhere and restore it once** on another machine; a backup
  you have not restored is not yet a backup. The Records database is the registry: it cannot be rebuilt.
  Rehearse the **restore procedure** (`records-service.md` §5.1: `restore-mark`, `peer-reconcile`, `--apply --clear-hold`) on the
  copy before you need it.
- [ ] Schedule both daily and alert on a non-zero exit.
- [ ] Monitoring covers the signals in `GAMES_RULES_AND_PAYOUTS.md` §7, and `peer_unauthenticated`,
  `anchor_failed` and `chain_unavailable` in the Records Service's journal.

## H. Rehearse, then open

- [ ] **Games payout round trip** on a **throwaway database** with `GAMES_GRID_REWARD_UNITS=100` (games §4 step
  10); never test against the production database. Withdraw to your own address and watch it go
  `queued → submitted → confirmed`; the amount that arrives must equal the amount owed.
- [ ] **Anchoring round trip:** the Games service's first daily pass has asked the Records Service to anchor
  yesterday; `curl -s https://api.ultranetwork.cc/api/appchain/ledger/1/anchors` shows it. Then stop the Records
  Service for one pass, see `last error` in `ultranet-games close-day`, start it, and see the same day anchored
  once.
- [ ] **Registry round trip** with your own key (below).
- [ ] `ultranet-games enable` and `ultranet-records enable`.

```bash
DEV: ultranet-game-dev address  --key owner.json
DEV: ultranet-game-dev register --key owner.json --id my-first-game --name "My First Game"
```
Needs an owner address that is already active on the chain. Then, as the operator:
```bash
HOST: ultranet-records game-status --id my-first-game --status registered
```
Submit one result with the API key (`GAMES_PLATFORM.md` §2). The next UTC day, check that the registry day
closed and anchored (`journalctl -u ultranet-records`), that `GET /api/games/g/my-first-game/proof/<date>`
returns a proof, and that `python3 tools/verify_registry_proof.py proof.json` ends in `ok:`. Then
`ultranet-records game-status --id my-first-game --status suspended` to retire the rehearsal game.
- [ ] The proof verified and the anchor transaction is on the ledger.

## I. Developers

- [ ] Publish `ultranet-game-dev` (a release build for each platform you support) and
  `docs/GAMES_PLATFORM.md`, `tools/verify_registry_proof.py`, `sdk/typescript/`.
- [ ] Decide how a game moves from `sandbox` to `registered` and `listed` (who you contact, what you check)
  **before** the first request arrives; check names and ids against trademarks first.
- [ ] Only then ship the app build that shows the rewards screens (games §4 step 11).

## J. Restoring a backup: a mandatory procedure, never a shortcut

Run this **every time** a Records database is restored. The service has automatic checks (a fence that notices a replaced
database file, a ledger check, an owner-history check since 00:00 UTC yesterday), but they are a net, not the plan: restoring the
whole state directory, fence file included, is invisible to the fence, and older discrepancies are outside the history check's
horizon. Full detail and exactly what is and is not detected: `records-service.md` §5.1.

```bash
HOST: sudo systemctl stop ultranet-records
HOST: cp records.db records.damaged.db                         # keep the damaged one; never delete it
HOST: rm -f records.db-wal records.db-shm && cp <backup> records.db   # leave records.db.fence where it is
HOST: ultranet-records restore-mark --note "restored from <backup>"
HOST: sudo systemctl start ultranet-records                    # it holds: reconciliation_required, nothing signed
HOST: ultranet-records peer-reconcile                          # read what the ledger and the owner's history say
HOST: ultranet-records peer-reconcile --apply                  # adopt the anchors the database forgot
HOST: ultranet-records peer-reconcile --apply --clear-hold     # refuses until nothing is unaccounted for
```
- [ ] The hold was released by `--clear-hold` itself, not by editing the database; `ultranet-records peer-usage` shows no `HELD`.
- [ ] If `UNACCOUNTED` owner transactions were reported, you know why before you add `--acknowledge-owner-transactions`.
- [ ] You have accepted that registry records written since the backup are lost.

## If something looks wrong

Stop at the failing step. `ultranet-games disable` stops the Games service accepting results and
`ultranet-records disable` stops the Records Service; each is independent. `ultranet-games status`, `audit`,
`payouts` and `ultranet-records status`, `game-list` need no key and no network. Do not edit either database by hand.
