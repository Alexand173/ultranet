# Mining run sheet — UltraVerse on UltraNet

**Baseline captured 2026-10-24T15:53Z:** `chain_length 2` · `mempool_size 3` · `validator_count 6` · pending
proposals **none** · 1 activated validator · sovereign/approver `3b8ef38a…94e87ee3c`

> **Read this first:** the mempool is **not** empty. The first successful `/api/mine` will consume those
> three transactions and produce block 3. Decide before you start whether that is acceptable.

## 0. Preconditions

| Item | Value |
|---|---|
| APK | `android-app/app/build/outputs/apk/debug/app-debug.apk` · sha256 `27aa576da1f7a27e95e226f54c1ca685b85f5e441660d79c2b6de98d3c8ab7a7` |
| Device | phone, `arm64-v8a`, API 23+, unlocked, animations off |
| Network | HTTPS to `api.ultranetwork.cc` (cleartext is disabled in the manifest) |
| Admin token | `ULTRANET_ADMIN_TOKEN` from `/etc/ultranet/ultranet.env`, at least 32 characters |
| Wallet address | read off the phone in step 3 |

## 1. Node baseline (before touching the phone)

```sh
curl -sS https://api.ultranetwork.cc/api/stats | tee /tmp/stats-before.json | python3 -m json.tool
```

Record `chain_length`, `mempool_size` and `validator_count`. These are the numbers the app must match.

## 2. Governance gate for the address you will use

Save as `verify-validator.py` and run `python3 verify-validator.py <64-hex address>`. The authority is
the **derived address**, not a substring match: a validator's address is `hex(SHA3-256(2592-byte
Dilithium public key))` (`src/lib.rs:151-155`).

```python
#!/usr/bin/env python3
import hashlib, json, sys, urllib.request

BASE = "https://api.ultranetwork.cc"
address = sys.argv[1].strip().lower().removeprefix("0x")

def get(path):
    with urllib.request.urlopen(BASE + path, timeout=20) as r:
        return json.load(r)

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

## 3. Wallet on the phone

1. `adb install -r app/build/outputs/apk/debug/app-debug.apk`
2. Confirm both native slices shipped — `adb` unzip check for `lib/arm64-v8a/libultranetcrypto.so`
   **and** `lib/x86_64/libultranetcrypto.so`. A missing arm64 slice installs fine and dies at the
   first Dilithium call.
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

## 5. Mine, then verify outside the app

Tap **Validate transactions**, then:

```sh
curl -sS https://api.ultranetwork.cc/api/stats | tee /tmp/stats-after.json | python3 -m json.tool
curl -sS https://api.ultranetwork.cc/api/block/3 | python3 -m json.tool   # index = the new height
```

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
| Action disabled, gate says no record | address matches no activated record | re-run step 2 with the copied address |
| Gate green, action disabled | token not imported | import the token, 32+ characters |
| 401 with a token that works in `curl` | trailing whitespace, or the node restarted with a new token | *Manage saved token* → replace |
| Node unreachable in the app, website fine | phone network blocks the API host | no cleartext fallback by design |

## 8. Cleanup and record

**Manage saved token → Remove admin token** clears the ciphertext in SharedPreferences and deletes the
Keystore entry; the wallet is untouched. Uninstalling removes both. Lock the wallet when finished.

```
Window (UTC):         __________ → __________
App sha256:           27aa576d…c8ab7a7
Wallet address:       ______________________________________
Height before/after:  ______ → ______
Mempool before/after: ______ → ______
Block hash:           ______________________________________
Verdict:              pass / fail — operator: ____  reviewer: ____
```

## What this run does not prove

No validator key signs the block. `POST /api/mine` accepts no request body and the node signs its own
block, so this validates the app's read, gate, token and reporting path — not validator-key block
signing. On a 0 ULTRA wallet the send form also refuses with *"Amount and fee exceed your available
balance after pending sends"*; that is the guard working, not a defect.
