# UltraNet production readiness report — 2026-11-22

> **Status note added 2026-11-23, on committing this report.** It is a snapshot of 2026-11-22 and is kept as
> the record of that day. Four things have changed since: the binary it describes (`5344ed0c…`) was replaced
> by `69aeddd9…` at 20:00Z on 2026-11-22; the source it describes as uncommitted is now committed
> (`c28f174`, `186e213`, `cda5132`, then `c05fb94`, `f5a532f`, `c3a31ae`, `0a50928`); the retired AppChain
> routes are documented as retired everywhere; and the two D-5D client session defects are closed. For the
> current state read [`appchain-completion-checklist-rev1.md`](./appchain-completion-checklist-rev1.md).

Verification and documentation only. No service was restarted, stopped, enabled, disabled or reloaded.
No transaction was signed or broadcast, no anchor was triggered, `appchain-cli` was not invoked for any
state-changing operation, `peer-reconcile` was not run, no hold was cleared, no environment file,
credential, key, password, proxy rule, firewall rule or economic parameter was modified. Nothing was
pushed. This file itself was left uncommitted for review at the time; it was committed on 2026-11-23 with
its body unchanged apart from the status note above and this sentence.

Every figure below is labelled. **HISTORICAL** means measured during the previous session on
2026-11-21 (roughly 19:21–19:36Z) and reproduced here from that session's output, not re-measured.
**FRESH** means measured during this verification, with the timestamp stated.

Working copies. Two checkouts of the same origin repository exist and they are not interchangeable:

| Copy | Location | State |
|---|---|---|
| Development copy, holds the six-file commit | `/home/valerian/UltraNet_Linux` | branch `main`, 1 commit ahead of `origin/main` |
| Production build copy | `/root/build-ultranet` | detached HEAD at `147ec2e` (the parent of `e502181`), 0 modified files, does not contain `e502181` |

The production build copy is a build tree; the node binary and the `ultranet-records` binary it produced
are installed under `/opt`. The documentation commit and this report belong to the development copy.

## 1. Executive summary

The Records Registry AppChain (#2) is live and the Records service has run continuously since
2026-11-21T19:21:50Z without a restart, an error line or an anchoring attempt. Registry chain #2 reports
zero anchors and zero spend, which is correct rather than stalled: every closed day in the seven-day
anchoring window carries an empty game list, so no day has been eligible to anchor. UltraGrid (#1) is
unchanged at one anchor and 1,000 base units of spend. The main node answers on 127.0.0.1:8081 with an
empty mempool at chain length 15; the Records service answers on 127.0.0.1:8096 only, with nothing
publicly proxied to that port. The Games service and the peer route remain out of scope and inactive.
Disk stands at 64% used after 13.9 GiB were reclaimed by `cargo clean`.

Outstanding limitation, unchanged: the first real automated anchor has never been executed. It remains
unproven in production and is not described here as tested.

## 2. Registry AppChain #2 registration

| Field | Value | Source |
|---|---|---|
| Chain id | 2 | FRESH, `/api/appchain/overview` |
| Name | `Registry` | FRESH |
| Owner address | `d4fca72103ca6c0a81ceb97fbefe30093f1799775c29be2a29ec4e22510d9d44` | FRESH |
| Registration transaction | `0100cf08a8c0454bf9a59e903a77a88c91786491401745c11d59ddb0296e596e` | FRESH |
| Anchor count | 0 | FRESH |
| Anchor spend | 0 | FRESH |
| `latest_anchor_at` | `null` (never anchored) | FRESH |
| Registry health `date` | `2026-11-22` | FRESH |

The owner registration was performed in the previous session; the registration transaction hash is the
same value reported then, so registration has not been repeated and the owner record has not moved.

## 3. Records service status and API health

**HISTORICAL (2026-11-21).** Unit installed, started and enabled at 19:21:50Z after three missing
namespace directories (`/etc/ultranet-games`, `/var/lib/ultranet-games`, `/opt/ultranet-games`) were
created; health reported `date: 2026-11-21` with `accepting`, `anchoring`, `success` all true and
`held` false.

**FRESH (2026-11-22T08:26:44Z, host clock).**

| Check | Result |
|---|---|
| `systemctl is-active ultranet-records.service` | `active` |
| `systemctl is-enabled ultranet-records.service` | `enabled` |
| `ActiveEnterTimestamp` | `Sat 2026-11-21 19:21:50 UTC` — unchanged, so the unit has not been restarted since it was started |
| `GET 127.0.0.1:8096/api/records/status` | **HTTP 200**, `{"accepting":true,"anchoring":true,"date":"2026-11-22","held":false,"registry_chain_id":2,"success":true}` |
| `GET 127.0.0.1:8081/api/stats` | **HTTP 200** |
| `GET 127.0.0.1:8081/api/mempool` | **HTTP 200**, `count: 0`, `limit: 100`, `returned: 0`, `transactions: []` |
| `GET 127.0.0.1:8081/api/appchain/overview` | **HTTP 200**, readable, 2 chains |
| Journal, `panic`/`fatal`/`denied`/`permission`/`error` since 2026-11-21T19:30Z | **0 matching lines** |
| Journal, last line | `2026-11-21T19:21:50+00:00 … starting service: "actix-web-service-127.0.0.1:8096", workers: 4` |

The last journal line being the start-up line means the process has logged nothing since it came up:
no request errors, no denial, no panic, and no daily anchoring activity to report either.

Node counters (FRESH, from `/api/stats` under `data`): `chain_length 15`, `mempool_size 0`,
`auto_mined_blocks 3`, `auto_mine_last_at 1795287555` = **2026-11-21T18:59:15Z**, `validator_count 6`.
The chain head has not advanced since the previous snapshot at chain length 15, and the mempool is empty.
That is the idle-chain condition already documented: `auto_mine_tick` returns early when nothing is
queued, so an old head is not evidence of a stalled miner.

## 4. Owner-history reconciliation and safety controls

**HISTORICAL (2026-11-21), the only reconciliation reading that exists.** `peer-reconcile`, read-only
invocation:

```
fence: intact
owner AppChain transactions since the baseline: 0; attempts this database recorded: 0
today's allowance: 0 slot(s) used
```

**NOT VERIFIED TODAY.** `peer-reconcile` was prohibited by this task's boundaries and was not run, so
the fence state and the owner-history baseline are unverified as of 2026-11-22. They are not reported as
current. The controls themselves are unchanged and can be checked against the following facts, which
were verified today:

| Control | Evidence |
|---|---|
| Daily allowance not consumed | chain #2 `anchor_count 0`, `anchor_spend 0` — no slot spent |
| No anchor attempt logged | 0 journal lines matching `anchor`/`registry_daily`/`reconcile`/`hold` beyond the start-up banner |
| Owner key not used since registration | nothing to use it for: no eligible day, and the owner nonce is 1 |
| Service holds no payout key | credential directory contains the owner key and passphrase only; `check-config` reported "Payout key: not held here (correct)" (HISTORICAL) |
| Restore hold not active | `held: false` in the fresh health payload |

Safety rule that follows: any future manual use of the owner key invalidates the acknowledged baseline
and requires a fresh acknowledgement, which is itself a state change needing explicit approval.

## 5. UltraGrid chain #1 — unchanged

| Field | HISTORICAL 2026-11-21 | FRESH 2026-11-22 | Match |
|---|---|---|---|
| Owner | `7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e` | `7d2cd1cc30c1088e…` | yes |
| Anchor count | 1 | 1 | yes |
| Anchor spend | 1000 | 1000 | yes |
| `latest_anchor_at` | 1794761124 | 1794761124 = **2026-11-15T16:45:24Z** | yes |
| Registration transaction | `bbc892926056…` | `bbc892926056…` | yes |
| Totals (both chains) | `anchor_count 1`, `anchor_spend 1000` | `anchor_count 1`, `anchor_spend 1000` | yes |

Chain #1 has not been touched.

## 6. Games and payout scope exclusions

| Item | FRESH result |
|---|---|
| `ultranet-games` unit files present | **0** — no unit installed |
| `RECORDS_PEER_CREDENTIAL` in the Records environment | present but **commented out** (`# RECORDS_PEER_CREDENTIAL=<redacted>`), so the peer route has no verifier |
| `GET https://api.ultranetwork.cc/internal/v1/anchor` | **404** |
| `GET https://api.ultranetwork.cc/api/records/status` | **404** — the Records API is not published |
| `127.0.0.1:8096` listen scope | `127.0.0.1:8096` only, pid 450072 (`ultranet-record`) |
| Caddy upstreams in `/etc/caddy/Caddyfile` | `127.0.0.1:8081`, `127.0.0.1:3000`, `127.0.0.1:8090` — **no 8096 entry** |
| `caddy validate` | `Valid configuration` (parse only, nothing reloaded) |
| `caddy` service | `active` |

Games reward accounting and payouts are therefore outside the live surface: no games service, no peer
ingest path, no payout credential, and no public Records route. `/api/games/registry/days` with no date
returns 404 (there is no list route); individual day records are readable.

## 7. Disk cleanup

**HISTORICAL (2026-11-21).** `cargo clean` in `/root/build-ultranet` removed **19,054 files, 13.9 GiB**,
which was the debug `target/` directory. Before: 59 G used, 14 G available, **82%**. After:
46 G used, 27 G available, **64%**. The active production binaries are independent of that directory.

**FRESH (2026-11-22T08:26:44Z).**

```
/dev/sda1        75G   46G   27G  64% /
```

No reclamation has occurred since, and none was needed.

## 8. Commit `e502181`, its six files, and validation

| Item | Result |
|---|---|
| Full hash | `e502181293d44602527aa8ae526657dae351a976` |
| Subject | `fix(portability): consume path explicitly off-Unix, add --to to the transfer script, record CI gate history` |
| Tree object | `ba283def6a994e656c8cfd462402d124d153476a` |
| Parent | `147ec2e8e236d2a606b509076ad837cc0b359026`, which is also `origin/main` |
| Path count | **exactly 6**, all modifications |
| Position relative to `origin/main` | `0 1` from `rev-list --left-right --count` — 1 ahead, 0 behind |
| Contained in any remote branch | **no** (`git branch -r --contains` returned nothing) |
| Altered since it was created | **no** — the object still hashes to `e502181…`, which is itself proof the content and its message are unchanged |

The six paths:

| Path | Change |
|---|---|
| `docs/FINISH_PLAN_2026-11-21.md` | gate history, idle-chain wording, duplicated section 6 removed |
| `send_ultra.py` | `--to` recipient, validated before any key is read; faucet remains the default |
| `src/approval_signer.rs` | two `cfg(not(unix))` arms |
| `src/bin/ultranet-approve.rs` | one `cfg(not(unix))` arm |
| `src/bin/ultranet-supply-correct.rs` | one `cfg(not(unix))` arm |
| `src/faucet/signer.rs` | one `cfg(not(unix))` arm |

Validation recorded when it was committed (**HISTORICAL**, not re-run today because re-running the suite
is not part of a read-only check): host `cargo check --locked --all-targets -j 1` clean in 9.06 s;
`py_compile` on `send_ultra.py` passed; the Windows cross-target check reported **0** `unused variable:
path` warnings against 5 before; secret scan over the added lines found no private key, passphrase, token
or newly added 64-hex literal; the diff was five hunks across four Rust files plus the script and the
documentation.

## 9. Fresh production verification results

Captured **2026-11-22T08:26:44Z** (host clock) / **2026-11-22T08:26:49Z** (development workstation).
Read-only GET requests and read-only system commands only.

| # | Check | Result |
|---|---|---|
| 1 | Records unit active | pass — `active` |
| 2 | Records unit enabled | pass — `enabled` |
| 3 | Records health endpoint | pass — HTTP 200, `success: true`, `held: false` |
| 4 | Main node API 127.0.0.1:8081 | pass — `/api/stats` HTTP 200 |
| 5 | Mempool readable | pass — HTTP 200, `count 0` |
| 6 | AppChain overview readable | pass — HTTP 200, chains 1 and 2 |
| 7 | Chain #2 owner / anchors / spend | `d4fca721…10d9d44` / `0` / `0` |
| 8 | Chain #1 unchanged | pass — see §5, field by field |
| 9 | Owner balance and nonce | `balance_base_units 40000`, `balance_ultra 0.040000`, `nonce 1` |
| 10 | Records journal clean | pass — 0 error/fatal/panic/permission lines since 2026-11-21T19:30Z |
| 11 | Caddy normal, no 8096 proxy | pass — `active`, config valid, upstreams 8081/3000/8090 only |
| 12 | Games absent, peer route disabled | pass — 0 units, credential line commented out |
| 13 | Disk after cleanup | `46G used / 27G available / 64%` |

Method note: the account endpoint returns `{"success","message","data"}`, so balances and the nonce live
under `data`. A first pass in this session read the top level and printed nothing; the values above come
from a second read that walked the payload. The numbers agree with the previous session's reading of the
same endpoint, so this was a parsing mistake in the check, not a change in state.

## 10. Outstanding limitation

**The first real automated anchor has not been exercised.** No day has ever been anchored on chain #2 in
production. The anchoring path — daily pass, eligibility test, owner-signed registration call, fee
payment, block inclusion and post-anchor reconciliation — is unproven in production and is not described
in this report as tested or proven. It has been exercised only in the single-node rehearsal documented
with the Records work.

Today's zero anchor count is explained by measurement, not assumption: within the seven-day window every
closed day reports `games: []` and `anchor_tx: null`.

| Day | `anchor_tx` | Games |
|---|---|---|
| 2026-11-15 … 2026-11-20 | `null` on every day | none eligible |
| 2026-11-21 | `null`, `day_root 2c24c0ea…6799a`, `merkle_root fb474aef…a3fd8` | `[]` |
| 2026-11-22 | HTTP 404 `that day has not been closed yet` | not closeable yet |

## 11. Supervised first-anchor observation procedure

Procedure only. None of it was executed today. All commands are read-only; the anchor itself fires from
the service's own daily pass and must not be forced. The pass runs every 600 s and covers *yesterday
through seven days back*; a day anchors only if it holds results attached to a `registered` or `listed`
game. With no games, this procedure cannot reach step 5 until real game results exist.

1. **Confirm the day is eligible.** `curl -s http://127.0.0.1:8096/api/games/registry/days/<date>` — a
   non-empty `games` array with eligible status is the precondition. An empty array means no anchor is
   due and none should be expected.
2. **Baseline before the pass.**
   - `curl -s http://127.0.0.1:8081/api/appchain/overview`
   - `curl -s http://127.0.0.1:8081/api/account/d4fca72103ca6c0a81ceb97fbefe30093f1799775c29be2a29ec4e22510d9d44`
   - `curl -s http://127.0.0.1:8081/api/mempool`
3. **Watch the log.** `journalctl -u ultranet-records.service -f -o short-iso`, filtered on
   `anchor|daily|registry|reconcil|hold|fence`.
4. **Confirm the transaction reached the mempool and cleared.** `/api/mempool` should show the queued
   owner transaction and then return to `count 0` once a block includes it — the auto-miner seals within
   about 10 s.
5. **Find the transaction.** Resolve the new head at `/api/block/<n>`. Expected shape: sender
   `d4fca721…`, fee `1000` base units, AppChain anchor call to chain 2.
6. **Expected state changes, all at once and only once.**

   | Field | Before | After |
   |---|---|---|
   | chain #2 `anchor_count` | 0 | 1 |
   | chain #2 `anchor_spend` | 0 | 1000 |
   | overview totals `anchor_count` | 1 | 2 |
   | overview totals `anchor_spend` | 1000 | 2000 |
   | owner `balance_base_units` | 40000 | 39000 |
   | owner `nonce` | 1 | 2 |
   | chain #1 | unchanged | unchanged |
   | mempool | 0 | 0 (after inclusion) |

   The owner funds **40 anchors** at 1,000 base units each from its current 40,000.

7. **Failure signatures — report, do not repair.** A retry within the 600 s window surfaces as
   `last_error`; `reconciliation_required` (HTTP 503) means the owner-history baseline must be
   re-established; `held: true` means a restore hold is in force; `fence: REGRESSED` means the fence
   check failed. Any of these means stop and escalate.
8. **Do not force it.** There is no supported way to trigger an anchor from outside, and
   `appchain-cli` signing or broadcasting by hand is an owner-key operation that invalidates the
   reconciliation baseline.

## 12. Statement of non-actions

During this task:

- **No push** occurred. `origin/main` is still at `147ec2e`; `e502181` is contained in no remote branch.
- **No commit** was created, and **`e502181` was not amended, rewritten or otherwise altered**; its
  object hash is unchanged. This report is left as an uncommitted file.
- **No anchor transaction** was submitted or triggered, and **no `appchain-cli` signing, broadcasting or
  mutating operation** was run. Chain #2 remains at 0 anchors and 0 spend; totals remain 1 and 1000.
- **No service** was restarted, stopped, enabled, disabled or reloaded. The Records unit's
  `ActiveEnterTimestamp` is still 2026-11-21T19:21:50Z.
- **`peer-reconcile` was not run** and no hold was cleared.
- **No production configuration changed**: no environment file, credential, key, password, Caddy rule,
  firewall rule or economic parameter was modified. `caddy validate` parses the config in memory and
  does not apply it.
- **No secret was printed** in this report or in the terminal output it is based on. The only redacted
  line is the commented-out peer credential, shown as `# RECORDS_PEER_CREDENTIAL=<redacted>`.
