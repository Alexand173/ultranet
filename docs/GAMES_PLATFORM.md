# Add your game to UltraNet — developer quickstart

Written for game developers. Operators: see `runbooks/games-service.md` §5.1.

## What you get, and what you don't

Your game reports results to UltraNet over HTTPS. Each UTC day, the results of every registered game are
folded into **one 32-byte root**, and that root is written to the chain by a transaction signed by the
platform's owner. Later, you can prove that a given day of your game was in that root and has not been
changed or removed since.

- **Tamper-evidence, not truth.** UltraNet records what your game *says* happened. It does not check
  that a score was honest or a player was human. Players and auditors can verify that history was not
  rewritten; they cannot learn from it that your game's rules were followed.
- **No money moves.** The platform never pays players for any third-party game. Rewards, if you want
  them, are your own business and come from your own wallet.
- **"5 minutes" means sandbox.** You can be sending test results in five minutes. A sandbox game is
  private, never anchored, and limited to a small day (500 results by default). Going `registered` and
  `listed` is a decision by the platform operator.

## 1. Get a game id and key

You need a wallet key file (`{"public_key":"<hex>","secret_key":"<hex>"}`, as written by
`ultranet-faucet keygen --output owner.json`) whose address has been active on UltraNet, for instance by
claiming from the faucet. Then:

```bash
ultranet-game-dev address  --key owner.json                                   # your owner address
ultranet-game-dev register --key owner.json --id my-game --name "My Game"
```

The id is 3–32 characters of `a-z 0-9 -` (names like `grid`, `chess`, `admin` are reserved). The tool signs
a short message locally; your secret key never leaves your machine, and the server only learns your public
key and a signature. **You become the owner by signing: the owner is the address your public key hashes to.**
It prints your **API key once**; only its hash is stored. Keep it on your server, never in a mobile app or web
page: anyone holding it can submit results as your game.

Lost the key? `ultranet-game-dev rotate-key --key owner.json --id my-game` issues a new one and kills the old
one, with no operator involved. Each owner address may hold 3 games, and the platform takes 100 new games
a day, so a refusal with `owner_game_limit` or `registration_limit` is a limit, not a fault.
To sign from your own code instead, the exact layout is at the top of `src/games/ownership.rs`
(Dilithium5 over a SHA3-256 digest of action, id, name and time).

## 2. Send a result

```bash
curl -s -X POST https://api.ultranetwork.cc/api/games/g/my-game/results \
  -H "Authorization: Bearer $GAME_KEY" -H "Content-Type: application/json" \
  -d '{"player":"<64 lowercase hex address>","result_id":"match-0001","score":1200}'
```

| field | rule |
|---|---|
| `player` | 64 lowercase hex characters — the player's UltraNet address |
| `result_id` | 3–64 of letters, digits, `- _ .`; **your** unique id for this result |
| `score` | unsigned integer, optional (default 0) |
| `data_hash` | optional, 64 hex: a hash of anything you want to be able to show later (a replay, a log) |

Unknown fields are rejected.

**Retries are safe.** Sending the same `result_id` with identical contents returns `"duplicate": true` and
adds nothing, so retry freely on a timeout. Sending the same `result_id` with *different* contents is
refused with `409 result_conflict`: a recorded result is never overwritten.

Reply: `{"game_id","result_id","day","duplicate","status","anchored"}`. `anchored` says whether this result
will be in an anchored day: always `false` in the sandbox.

| status | meaning |
|---|---|
| 401 `invalid_key` | wrong or missing key |
| 403 `game_suspended` | your game has been suspended |
| 409 `result_conflict` | `result_id` already recorded with other contents |
| 429 | a daily limit (per game or per player) was reached |

## 3. Check your day against the chain

After the day closes (just after 00:00 UTC) and has been anchored:

```bash
curl -s https://api.ultranetwork.cc/api/games/g/my-game/proof/2026-11-16 > proof.json
python3 tools/verify_registry_proof.py proof.json
```

Add `--results results.json`, a list of the results you sent that day (same fields as above), and the
script also recomputes *your* game's root from your own records and confirms it matches:

```bash
python3 tools/verify_registry_proof.py proof.json --results results.json
```

A pass means the anchored root contains exactly those results for your game id. If `anchor_tx` is empty,
the day is fixed but not yet on the chain (`state` tells you where it is). Other endpoints:
`GET /api/games/g` (directory of listed games), `GET /api/games/g/my-game`, and
`GET /api/games/registry/days/<date>` (the day's games and roots).

The script uses only the Python standard library and is an independent implementation of the layouts at
the top of `src/games/registry.rs`, checked against the Rust code's fixed test vectors. To verify in
another language, port those layouts and compare against the same vectors.

## The generic names: `/api/records` and `/api/apps`

Applications that are not games can use the same registry under neutral names. The two sets of routes are
the same registry: a record written through one is the same record through the other, and a proof is the
same proof. The `/api/games/…` routes are unchanged and stay.

| Generic route | Same as | Body or reply |
|---|---|---|
| `POST /api/records` | `POST /api/games/g/{id}/results` | `{"app_id","record_id","subject","value"?,"data_hash"?}`, `Authorization: Bearer <key>` |
| `POST /api/apps` | `POST /api/games/g` | `{"app_id","name","public_key","timestamp","signature"}` |
| `POST /api/apps/{id}/rotate-key` | `POST /api/games/g/{id}/rotate-key` | unchanged body |
| `GET /api/apps/{id}` | `GET /api/games/g/{id}` | page of a registered app |
| `GET /api/apps/{id}/proof/{date}` | `GET /api/games/g/{id}/proof/{date}` | the same proof |
| `GET /api/records/days/{date}` | `GET /api/games/registry/days/{date}` | the same day |

| Generic field | Internal field | Rule |
|---|---|---|
| `app_id` | game id | 3–32 of `a-z 0-9 -` |
| `record_id` | `result_id` | 3–64 of letters, digits, `- _ .`; your idempotency key |
| `subject` | `player` | 64 hex characters: an address, **or the SHA3-256 of any identifier you choose** (a student number, a serial). Capitals are lowercased, as on the old route |
| `value` | `score` | optional integer, default 0; its meaning is yours |
| `data_hash` | `data_hash` | unchanged |

Generic replies carry the old names too, plus `app_id` and `record_id`, so a client can read either. Each
route refuses the other vocabulary's request fields as unknown (`400 invalid_request`). The owner's
signature is over the same digest on both routes (action, id, name, time), so a signature made for one is
valid on the other. Authentication, limits, idempotency (`duplicate: true`) and conflicts (`409
result_conflict`) are the registry's and are identical.

The hash layouts and domain tags (`UltraRecord/*`) are unchanged by the renaming; it is a renaming of
request and reply fields only.

## Node.js SDK

`sdk/typescript/ultranet-games.mjs` is one file with no dependencies (Node 18+) for your game **server**:

```js
import { GamesClient } from "./ultranet-games.mjs";
const games = new GamesClient({ gameId: "my-game", apiKey: process.env.ULTRANET_GAME_KEY });
await games.submitResult({ player, resultId: "match-0001", score: 1200 });   // safe to retry
const day = await games.verifyDay("2026-11-16", myRecordsForThatDay);        // { ok, problems, anchored }
```

It also exports `gameResultsRoot`, `verifyProof` and `ownerProofDigest`. Its tests (`node --test sdk/typescript/`)
check the same fixed vectors as the Rust code and the Python script. There is deliberately no Kotlin or
mobile SDK: results must be sent by your server, because an API key inside an app can be extracted and
used to report results for your game.

## What is not built yet

A web console (the website's wallet can only sign transfers today; signing a game proof there needs a new wallet method and a decision), and any developer-funded rewards. Until then you use
`ultranet-game-dev` and call the HTTP API directly, as above. Registration is self-service, but a game
only leaves the sandbox when the operator promotes it.
