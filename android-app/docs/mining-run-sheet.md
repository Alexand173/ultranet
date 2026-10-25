# Mining run sheet — UltraVerse on UltraNet

**Baseline captured 2026-10-25T10:07Z**, after block 2 (the sovereign supply correction) was mined:

| Fact | Value |
|---|---|
| `chain_length` | **3** — blocks index 0 (genesis), 1 and 2 exist; the next block is index **3** |
| `mempool_size` | **3**, all of them **stale and invalid** (see the warning below) |
| `validator_count` | **6** records — 2 operators plus 4 bootstrap entries; the site now shows 2 |
| `total_transactions` / `total_blocks` | 1 / 3 |
| highest block | index 2, `hash 02ac7da4…`, one transaction (`3b8ef38a…` → `0x0`, amount 0), `gas_used 31932`, `epoch 2` |
| sovereign `3b8ef38a…` | **1,000,000.000000** after the correction; the faucet `787e…` keeps its stranded 1,000.001000 |
| pending validator proposals | **none** (`{"proposals": []}`) |
| approval journal | **1 entry** (`total_count 1`, `has_more false`) |

The single journal entry:

| Field | Value |
|---|---|
| validator address, `hex(SHA3-256(public_key))` | `094420342ad33eda3ed61841d09317a4d9a6a903a0a4c95b61d14b8ce2c55d78` |
| applicant, `proposal.proposer` | `2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed` |
| approved by, `approval_transaction.sender` | `3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c` (sovereign) |
| alias, `proposal.metadata` | `VladTepes` |
| `is_active` / `stake` / `weight` | `true` / `1000` / `1` |
| `joined_at` / `last_epoch` / `slash_count` | `1788338774` / `1` / `0` |

> **Read this first:** `mempool_size` reads **3** but those three entries are **stale and invalid** — they
> date from before the 2026-10-25 node fix and no longer pass validation. Mining skips them, so
> `POST /api/mine` answers `400 "No valid transactions available for mining"` and the app reports
> *No block produced*. **That is the expected result of this baseline, not a defect.** To see a block
> produced on the phone, submit one valid transaction first (see step 4a).
>
> The next block index is **3** — indices are zero-based, so `/api/block/3` currently answers
> `{"success":false,"message":"Block not found"}` and becomes the new block after mining.
>
> **Second:** the validator's own address (`0944…`) and the address that applied (`2859…`) are
> **different**. Both now open the app's gate: it derives the address from the key the way the node does
> (`hex(SHA3-256(public_key))`) and keeps the applicant as a fallback for journals written before the
> key was recorded — see step 2.

## 0. Preconditions

| Item | Value |
|---|---|
| APK | `android-app/app/build/outputs/apk/debug/app-debug.apk` · sha256 `9df975dca07e2e498cec370661948d0f649624b59555042a0a0c66bbe8921eed` (20 994 555 B, measured 2026-10-24T18:15Z) |
| Device | phone, `arm64-v8a`, API 23+, unlocked, animations off |
| Network | HTTPS to `api.ultranetwork.cc` (cleartext is disabled in the manifest) |
| Admin token | `ULTRANET_ADMIN_TOKEN` from `/etc/ultranet/ultranet.env`, at least 32 characters |
| Wallet address | read off the phone in step 3 |

The digest is per build, not per source: rebuilding identical code produces a different byte sequence, so
re-hash before writing it into the record block and confirm the dex carries the gate:

```sh
APK=android-app/app/build/outputs/apk/debug/app-debug.apk
sha256sum "$APK"
python3 - "$APK" <<'PY'   # prints True when the gate fix is in this build
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as apk:
    blob = b"".join(apk.read(name) for name in apk.namelist() if name.endswith(".dex"))
print("gate fix in dex:", b"validatorRecordMatches" in blob)
PY
```

`unzip` is not present on every host, and `unzip … | grep -c` prints `0` both when the patch is missing
and when the tool is missing — which reads as a false alarm. Use the `python3` form.

## 1. Node baseline (before touching the phone)

```sh
curl -sS https://api.ultranetwork.cc/api/stats | tee /tmp/stats-before.json | python3 -m json.tool
```

Record `chain_length`, `mempool_size` and `validator_count`. These are the numbers the app must match.

## 2. Governance gate for the address you will use

Start with a read-only cross-check of the two routes the app's gate itself reads, so the phone has a
printed expectation before anything is tapped. Both are `GET`, unauthenticated and side-effect free: no
token, no body, no query parameter that writes. The only governance writes in this build are
`POST /api/governance/propose` and the approval-intent routes, and this run never calls them.

### 2a. Fetch both payloads

```sh
BASE=https://api.ultranetwork.cc
curl -sS -o /tmp/proposals.json -w 'proposals  HTTP %{http_code}  %{size_download} B\n' "$BASE/api/governance/proposals"
curl -sS -o /tmp/approvals.json -w 'approvals  HTTP %{http_code}  %{size_download} B\n' "$BASE/api/governance/approvals"
```

Both must answer `200`. A `404` means this node build does not mount the routes and a `500` means the
node failed to read its own journal; stop on either, because the app renders "no record" for both and a
mining tap after that tests nothing.

A `403` is neither of those. The edge in front of the API answers 403 to clients that send no usable
`User-Agent`, which is why these `curl` calls work while the stock `Python-urllib` agent does not —
measured 2026-10-24: default agent `403`, `curl/8.5.0` `200` on `/api/stats`. Any script you write for
these routes has to name an agent; the 2c script below does.

| Route | Shape (`src/api.rs:2084`, `src/api.rs:3026`) |
|---|---|
| `proposals` | `{"success": true, "proposals": [{hash, public_key, metadata, proposer, timestamp}]}` |
| `approvals` | `{"success": true, "approvals": [{proposal_hash, approval_transaction, proposal, activated_validator, recorded_at}], "pagination": {limit, total_count, has_more, next_cursor}}` |

`activated_validator` is a `ValidatorInfo` (`src/lib.rs:377`): `public_key` as a JSON array of 2592
integers, alongside `weight`, `is_active`, `joined_at`, `last_epoch`, `stake`, `rewards`, `slash_count`.
That array is the identity — the row never carries the derived hex address.

```sh
python3 -m json.tool /tmp/proposals.json
python3 -m json.tool /tmp/approvals.json
```

**Expected at the 16:33Z baseline:** `proposals` is `[]`; `approvals` holds exactly one row with
`total_count 1`, `has_more false`, `next_cursor null`.

### 2b. Read the page the app itself reads

`OperatorApi.hasValidatorRecord` requests `/api/governance/approvals` with no parameters, so it sees the
default page of **50** records; `limit` is capped at **100** (`src/api.rs:3022-3023`). Check that page
before blaming the app:

```sh
curl -sS "$BASE/api/governance/approvals" -o /tmp/approvals-page1.json
python3 -c 'import json;p=json.load(open("/tmp/approvals-page1.json"))["pagination"];print({k:p[k] for k in ("limit","total_count","has_more","next_cursor")})'
```

`has_more false` means the app read the whole journal. If it is `true`, records exist that the app never
fetches — walk the cursor (`?limit=100&cursor=<next_cursor>`) before concluding a key is unregistered,
and treat a gate that disagrees only with a later page as this known limitation, not a phone fault.

### 2c. Derive the address and print one verdict

The authority is the **derived address**, not a substring match: a validator's address is
`hex(SHA3-256(2592-byte Dilithium public key))` (`src/lib.rs:151-155`). Save as `verify-validator.py`
and run `python3 verify-validator.py <64-hex address>`. It re-reads the same two routes, so if its
numbers disagree with your saved files the journal moved between the calls.

```python
#!/usr/bin/env python3
import hashlib, json, sys, urllib.error, urllib.request

BASE = "https://api.ultranetwork.cc"
address = sys.argv[1].strip().lower().removeprefix("0x")

def get(path):
    # The edge answers 403 to the default `Python-urllib` agent, so send a real one.
    request = urllib.request.Request(BASE + path, headers={"User-Agent": "UltraNet-run-sheet/1.0"})
    try:
        with urllib.request.urlopen(request, timeout=20) as r:
            return json.load(r)
    except urllib.error.HTTPError as error:
        sys.exit("%s answered HTTP %s %s" % (path, error.code, error.reason))

def address_of(pubkey):
    # FIPS SHA3-256, matching RustCrypto Sha3_256 on the node. NOT keccak256.
    return hashlib.sha3_256(bytes(pubkey)).hexdigest()

stats = get("/api/stats")["data"]
print("chain_length=%s mempool_size=%s validator_count=%s" % (
    stats["chain_length"], stats["mempool_size"], stats["validator_count"]))

for p in get("/api/governance/proposals")["proposals"]:
    if (p.get("proposer") or "").lower() == address:
        print("PENDING application:", p["hash"], "|", p.get("metadata", ""))

found = None
for row in get("/api/governance/approvals")["approvals"]:
    activated = row.get("activated_validator") or {}
    proposal = row.get("proposal") or {}
    derived = address_of(activated["public_key"]) if activated.get("public_key") else None
    applicant = (proposal.get("proposer") or "").lower()
    approved_by = ((row.get("approval_transaction") or {}).get("sender") or "").lower()
    if address and address in (derived, applicant):
        found = (derived, activated, applicant, approved_by)

if found is None:
    print("NO activated record for", address)
    print("-> the app keeps mining disabled by design. Stop and resolve this first.")
    sys.exit(1)

derived, activated, applicant, approved_by = found
print("ACTIVATED:", derived)
print("  is_active:", activated.get("is_active"), " stake:", activated.get("stake"),
      " joined_at:", activated.get("joined_at"))
print("  applicant:", applicant, "| approved by:", approved_by)
```

**Pass condition:** prints `ACTIVATED:` with an address equal to the one unlocked on the phone. If it
prints `NO activated record`, stop — the app's action stays disabled and mining cannot be tested.

`genesis.json` is not a source here: it declares `initial_validator_count: 5` with
`validator_set_source: "runtime-generated during bootstrap"` and names no validator addresses.

### The app now derives the validator address

`OperatorApi.hasValidatorRecord` delegates to `validatorRecordMatches` in
`data/ValidatorRecord.kt`: it compares the wallet address against
`hex(SHA3-256(activated_validator.public_key))` and still accepts the applicant recorded in
`proposal.proposer`, then matches pending proposals the same way.

| Wallet address | App gate |
|---|---|
| `09442034…` (the validator key's own derived address) | **opens** |
| `2859eb67…` (applicant / proposer) | **opens** (kept for journals written before the key was recorded) |

Both work, and the derived address is the authoritative one. Before this change the gate searched the
serialised row for the address as text, which can never find a 2592-byte key array — so the approved
validator key was refused while only its applicant was accepted.

`ValidatorRecordMatchTest` pins the behaviour against the journal captured on 2026-10-24T16:33Z
(fixture `app/src/androidTest/assets/validator-journal.json`, trimmed to the fields the gate reads;
the derived digest over the live key is `09442034…`).

## 3. Wallet on the phone

1. `adb install -r app/build/outputs/apk/debug/app-debug.apk`
2. Confirm both native slices shipped — a missing arm64 slice installs fine and dies at the first
   Dilithium call:

   ```sh
   python3 -c "import zipfile;print([n for n in zipfile.ZipFile('app/build/outputs/apk/debug/app-debug.apk').namelist() if n.endswith('libultranetcrypto.so')])"
   ```

   Expected: `lib/arm64-v8a/libultranetcrypto.so` **and** `lib/x86_64/libultranetcrypto.so`.
3. Create or restore the wallet, then unlock it.
4. **Portfolio → Account address → copy.** Re-run step 2 against exactly this string.

## 4. Validate transactions — read before acting

**Build & participate → Validate transactions.**

| Check | Expected |
|---|---|
| *Pending transactions* | equals `mempool_size` from step 1 (currently **3**); a failed read must read **Unavailable**, never `0` |
| *Chain length* | equals step 1 (`2`) |
| Validator gate | green *"Validator record found for 0x…"* — anything else means stop |
| *Import admin token* → Save | *"Admin token saved on this device."*; under 32 characters is rejected locally |
| Balance chip in the top bar | shows balance and spendable; tapping opens Portfolio, Back returns here |

### 4a. This baseline cannot produce a block, and that is correct

`mempool_size` reads 3, but those records are stale — mined blocks from before the 2026-10-25 node fix
left them, and they no longer pass validation. A tap on **Validate transactions** therefore answers
`400 No valid transactions available for mining` and the screen shows *No block produced*. That is the
screen correctly reporting a refused mine; it is not an authorization failure and not a regression.

Two ways to also see a successful mine:

- **Send one valid transaction first.** Any funded wallet can; the phone wallet itself holds 0.000000
  ULTRA, so it needs funding before it can send. Once a transfer is `pending`, `mempool_size` becomes 4
  and the next tap produces the block.
- **Accept the refusal as the result.** The gate, the token, the mempool read and the honest reporting
  are all still exercised — which is what this run is really about.

## 5. Mine, then verify outside the app

Tap **Validate transactions**, then:

```sh
curl -sS https://api.ultranetwork.cc/api/stats | tee /tmp/stats-after.json | python3 -m json.tool
curl -sS https://api.ultranetwork.cc/api/block/3 | python3 -m json.tool   # index = chain_length before mining
```

Block indices are zero-based: with `chain_length 3` the existing blocks are 0, 1 and 2, so the block
produced by this run is index **3**, and `chain_length` becomes 4. Fetching `/api/block/4` would return
`Block not found` even after a successful mine.

The approval journal must not move during a mining run — mining consumes mempool transactions, it does
not approve anyone. Compare it against the file saved in step 2a (same shell, so `$BASE` is still set):

```sh
curl -sS "$BASE/api/governance/approvals" -o /tmp/approvals-after.json
diff <(python3 -m json.tool /tmp/approvals.json) <(python3 -m json.tool /tmp/approvals-after.json) \
  && echo "approval journal unchanged by this run"
```

A diff here means a governance write landed from somewhere else while you were mining; record it and do
not read the run as a pass.

The operator call itself, if run from the node instead of the phone:

```sh
TOKEN=$(sudo awk -F= '/^ULTRANET_ADMIN_TOKEN=/{print $2}' /etc/ultranet/ultranet.env)
curl -sS -o /tmp/mine.json -w 'HTTP %{http_code}\n' \
  -X POST https://api.ultranetwork.cc/api/mine -H "Authorization: Bearer $TOKEN"
python3 -m json.tool /tmp/mine.json; unset TOKEN
```

| App shows | Node shows | Verdict |
|---|---|---|
| Block produced (height, tx count, hash) | `chain_length` +1, `mempool_size` → 0 | pass |
| No block produced | `400 Error: No valid transactions…` | not an auth error; the mempool was empty |
| Authorization rejected | `401` / `403` | token wrong or rotated |
| Block result unknown | no change | check the height before retrying; never double-tap |

## 6. Session behaviour (same run)

- Open AppChains → Back → catalog: **still unlocked**, no password prompt.
- Home to launcher and back: the wallet **locks** (activity `ON_STOP`).

## 7. Failure matrix

| Symptom | Cause | Action |
|---|---|---|
| Action disabled, gate says no record | the address has no activated or pending record | re-run step 2 with the copied address; `NO activated record` means the node does not know that key |
| Gate says no record, but step 2c printed `ACTIVATED` | the phone's address differs from the one you checked, or the record sits past the page of 50 the app reads (2b `has_more true`) | copy the address again from Portfolio and re-run 2a–2c; if it only matches a later cursor page, that is the app's single-page read, not a mismatch in the key |
| Action disabled although step 2c printed `ACTIVATED` | the installed build predates the derived-address gate | install the current debug APK and confirm `validatorRecordMatches` is in its dex (§0) |
| Gate green, action disabled | token not imported | import the token, 32+ characters |
| 401 with a token that works in `curl` | trailing whitespace, or the node restarted with a new token | *Manage saved token* → replace |
| Node unreachable in the app, website fine | phone network blocks the API host | no cleartext fallback by design |

## 8. Cleanup and record

**Manage saved token → Remove admin token** clears the ciphertext in SharedPreferences and deletes the
Keystore entry; the wallet is untouched. Uninstalling removes both. Lock the wallet when finished.

```
Window (UTC):         __________ → __________
App sha256:           9df975dc…bbe8921eed   (re-hash the installed file; §0)
Wallet address:       ______________________________________
Height before/after:  3 → ______          (next block index: 3)
Mempool before/after: 3 → ______         (stale entries; a valid tx must be sent first)
Validator count:      6 → ______
Gate cross-check:     ACTIVATED / none   (derived: ____________________)
Approval journal:     1 row → ______      (has_more false · unchanged by run: y/n)
Block hash:           ______________________________________
Verdict:              pass / fail — operator: ____  reviewer: ____
```

Baseline already captured read-only on 2026-10-24T16:33Z; only the "after" columns need filling.

## What this run does not prove

No validator key signs the block. `POST /api/mine` accepts no request body and the node signs its own
block, so this validates the app's read, gate, token and reporting path — not validator-key block
signing. On a 0 ULTRA wallet the send form also refuses with *"Amount and fee exceed your available
balance after pending sends"*; that is the guard working, not a defect.
