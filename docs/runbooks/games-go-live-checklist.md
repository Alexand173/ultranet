# Games platform — go-live checklist (owner's steps)

Written for the owner running the host. It lists only what you do, in order, with the check that proves
each step. The reasons are in [`games-service.md`](./games-service.md) (section numbers below point there).
Nothing here has been run on the production host. Do not skip a check: each one catches a mistake that
is cheap now and expensive later.

Lines starting `HOST:` run on the server. Lines starting `DEV:` run on your build machine.

## A. Build (one revision for everything)

```bash
DEV: git checkout feat/ultragrid-engine && git rev-parse HEAD            # write this hash down
DEV: cargo test --lib games:: -- --test-threads=1                         # expect: ok, 0 failed
DEV: cargo build --release --bin ultranet-games --bin appchain-cli --bin ultranet-faucet --bin ultranet-game-dev
```
- [ ] The test run says `0 failed`, and the hash you wrote down is the one you build and install.

## B. Keys (§2.1–2.3)

```bash
HOST: sudo mkdir -m 0700 /root/games-staging && cd /root/games-staging
HOST: ultranet-faucet keygen --output games-payout-signer.json            # prints the PAYOUT address: note it
HOST: appchain-cli generate-key --out games-anchor-owner.key.enc          # prints the OWNER address: note it
HOST: printf '%s' '<long random passphrase>' > games-anchor.pass && chmod 600 games-anchor.pass
HOST: head -c 48 /dev/urandom | base64 > games-abuse.key
```
- [ ] **Offline encrypted copies of the owner key file and its passphrase, in two different places, before
  anything is shredded.** Once the chain is handed over, that file *is* the chain.
- [ ] The payout address and the owner address are different, and neither is the faucet's.
- [ ] Encrypt and shred as in §2.3; `ls /root/games-staging` shows no plaintext left.

## C. Chains on the ledger (owner key must hold funds first)

Fund the **owner address** (B) with enough for the fees: a registration is 0.01 `$ULTRA` and an anchor is
0.001 `$ULTRA`, so at least 0.05 `$ULTRA` covers the registry chain and a month of daily anchors (the
service anchors the registry once a day and Grid once a day: 0.002 `$ULTRA` a day).

```bash
HOST: appchain-cli address  --key-file games-anchor-owner.key.enc         # confirm: the OWNER address
HOST: appchain-cli register --key-file games-anchor-owner.key.enc --name "UltraNet Games Registry"
```
- [ ] The ledger lists the new chain with **your owner address as owner**:
  `curl -s https://api.ultranetwork.cc/api/appchain/ledger | head -c 600` (note its chain id).
- [ ] Chain #1 (UltraGrid) is handed to the owner key by `appchain-owner-key.md` §2, and the ledger shows
  the new owner. Until then Grid cannot be anchored by this service.
- [ ] Chess chain: register only if you will anchor chess; it does **not** turn prizes on.

## D. Configure and check (§2.4)

```bash
HOST: sudoedit /etc/ultranet-games/games.env
```
Set: `GAMES_PAYOUT_ADDRESS=<payout address>`, `GAMES_ANCHOR_CLI=/opt/ultranet-games/appchain-cli`,
`GAMES_ANCHOR_GAMES=grid`, `GAMES_REGISTRY_CHAIN_ID=<the registry chain id from C>`. Leave
`GAMES_CHESS_PAYS=false` and `GAMES_REQUIRE_CHAIN_PRESENCE=true`.
```bash
HOST: sudo systemctl daemon-reload && sudo systemctl start ultranet-games
HOST: journalctl -u ultranet-games -n 40 --no-pager
```
- [ ] The journal shows the rules in force and no error. `Ultra Chess: … RECORD ONLY` is correct.
- [ ] `timedatectl show -p NTPSynchronized` says `yes` (§2.5).
- [ ] `curl -s 127.0.0.1:8095/api/games/status` answers on the host.

## E. Money (only now)

- [ ] Fund the **payout address** (B): 0.1 `$ULTRA` from the faucet's one claim (V2 F-2).
- [ ] `ultranet-games status` shows the balance and `owed 0`.

## F. The edge (§3)

- [ ] Add the Caddy route from `deploy/Caddyfile.example`; run `caddy validate --config <file>` **before** reloading.
- [ ] Add every Cloudflare limit in the §3 table, including the new register, rotate-key and results rows.
- [ ] From outside: `curl https://api.ultranetwork.cc/api/games/status` answers, and
  `curl -s -o /dev/null -w '%{http_code}' -X POST https://api.ultranetwork.cc/api/games/g -d '{}'` is `400`,
  not `5xx`.

## G. Safety nets (§5)

```bash
HOST: ultranet-games backup --to /var/backups/ultranet-games/games-$(date -u +%Y%m%d).db
HOST: ultranet-games audit
```
- [ ] The backup opened and audited itself. **Copy it elsewhere and restore it once** on another machine;
  a backup you have not restored is not yet a backup.
- [ ] Schedule `audit` and `backup` daily (cron or a systemd timer) and alert on a non-zero exit.
- [ ] Monitoring covers the signals in `GAMES_RULES_AND_PAYOUTS.md` §7.

## H. Rehearse, then open

- [ ] Payout round trip on a **throwaway database** with `GAMES_GRID_REWARD_UNITS=100` (§4 step 10); never
  test against the production database. Withdraw to your own address and watch it go
  `queued → submitted → confirmed`; the amount that arrives must equal the amount owed.
- [ ] Registry round trip with your own key (below). It proves intake, the daily root and anchoring.
- [ ] `ultranet-games enable`.

```bash
DEV: ultranet-game-dev address  --key owner.json
DEV: ultranet-game-dev register --key owner.json --id my-first-game --name "My First Game"
```
Needs an owner address that is already active on the chain. Then, as the operator:
```bash
HOST: ultranet-games game-status --id my-first-game --status registered
```
Submit one result with the API key (`GAMES_PLATFORM.md` §2). The next UTC day, check that the registry day
closed and anchored (`journalctl -u ultranet-games`), that `GET /api/games/g/my-first-game/proof/<date>` returns
a proof, and that `python3 tools/verify_registry_proof.py proof.json` ends in `ok:`. Then
`ultranet-games game-status --id my-first-game --status suspended` to retire the rehearsal game.
- [ ] The proof verified and the anchor transaction is on the ledger.

## I. Developers

- [ ] Publish `ultranet-game-dev` (a release build for each platform you support) and
  `docs/GAMES_PLATFORM.md`, `tools/verify_registry_proof.py`, `sdk/typescript/`.
- [ ] Decide how a game moves from `sandbox` to `registered` and `listed` (who you contact, what you
  check) **before** the first request arrives; check names and ids against trademarks first.
- [ ] Only then ship the app build that shows the rewards screens (§4 step 11).

## If something looks wrong

Stop at the failing step. `ultranet-games disable` stops accepting results, and `ultranet-games status`,
`audit` and `payouts` need no key and no network (§5, §6). Do not edit the database by hand.
