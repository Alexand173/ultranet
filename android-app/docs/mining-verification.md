# Validate transactions — real-token test on a physical phone

The Android app can be verified end to end on an emulator for everything except the final call:
`POST /api/mine` is gated by the node's operator admin token, and the node signs the block itself.
This checklist covers that last step.

## What this proves, and what it cannot

Proves: the app reads the live mempool, checks for a validator record, unlocks the action with a
real token, and reports exactly what the node returned.

Cannot prove: that a **validator key** signed the block. No UltraNet endpoint accepts a
client-supplied block signature — `/api/mine` takes no body. The app is explicit about this on the
screen rather than implying otherwise.

## Before you start

| Requirement | Detail |
|---|---|
| Phone | `arm64-v8a`, Android 23+, `targetSdk 36` |
| App | debug APK from `./gradlew assembleDebug` |
| Network | HTTPS reach to `https://api.ultranetwork.cc` (cleartext is disabled) |
| Node token | the `ULTRANET_ADMIN_TOKEN` value from the node's environment file, at least 32 characters |
| Node state | the node must be running with that token set; otherwise `/api/mine` answers 401/403 |
| Validator record | the wallet address must appear in governance (proposal proposer or activated validator), otherwise the action stays disabled by design |

Install and confirm both ABIs shipped before trusting a Dilithium call:

```sh
adb install -r app/build/outputs/apk/debug/app-debug.apk
python3 -c "import zipfile;print([n for n in zipfile.ZipFile('app/build/outputs/apk/debug/app-debug.apk').namelist() if n.endswith('.so')])"
```

Expected: `lib/arm64-v8a/libultranetcrypto.so` **and** `lib/x86_64/libultranetcrypto.so`.

## Steps

1. **Open the app** → the catalog shows four groups. Keep it in the foreground from here on.
2. **Create or restore the local wallet** (Portfolio → Create local wallet / Restore wallet), then
   unlock it. `WalletStoreTest` covers storage; this run is about the mining path.
3. **Switch to a service and back** — open AppChains, press Back, open the catalog again. The wallet
   must stay unlocked. If it asks for the password here, the session-persistence requirement has
   regressed.
4. **Build & participate → Validate transactions.**
   - *Pending transactions* should show the node's current `mempool_size`, with *Chain length*
     beside it. Pull `GET /api/stats` on a laptop to compare — the numbers must match.
   - If the node is unreachable the card reads **Unavailable**. It must never show `0` for a failed
     read.
5. **Read the validator gate.**
   - With a wallet that has no governance record:
     **"You need validator keys to mine. Register as a validator first."**
     The action stays disabled and the footer repeats *"Register as a validator before validating
     transactions."*
   - With a wallet that does have a record: **"Validator record found for 0x…"** in green.
6. **Import the token** — *Import admin token*, paste the value, **Save token**. The screen should
   read *"Admin token saved on this device."* A token under 32 characters is rejected locally.
7. **Confirm the balance is visible** — the top bar of this screen shows the wallet balance and
   the spendable amount; tapping it opens Portfolio, and Back returns to this screen.
8. **Tap Validate transactions**, then compare against the node.

## Expected outcomes

| Node response | What the screen must show |
|---|---|
| Block produced | **Block produced** with height, transaction count and hash. *Copy block hash* copies it. Mempool refreshes to the new value. |
| 400 / "no valid transactions" | **No block produced** — "No pending transactions. No block was produced." This is not an authorization error. |
| 401 / 403 | **Authorization rejected** with *Replace admin token*. The token is not silently retried. |
| Timeout / transport error | **Block result unknown** — "This screen will not retry automatically." Then check the chain before retrying. |

Verify the block landed outside the app:

```sh
curl -s https://api.ultranetwork.cc/api/stats | python3 -m json.tool
# chain_length should have increased by one and mempool_size dropped
```

The produced hash can be looked up in the explorer.

## Failure triage

- **Action disabled with a validator record present** — the token is missing. Check *Operator
  authorization* on the screen.
- **401 with a token that works in `curl`** — the stored token has trailing whitespace, or the node
  was restarted with a different `ULTRANET_ADMIN_TOKEN`. Replace it via *Manage saved token*.
- **Screen says the node is unreachable but the website works** — the phone may be on a network that
  blocks the API host; the app has no cleartext fallback by design.

## Cleanup

**Manage saved token → Remove admin token.** This clears the ciphertext in SharedPreferences and
deletes the Keystore entry; the wallet itself is untouched. Uninstalling the app also removes both.

## Session behaviour to confirm on the same run

- Displays: switching service screens keeps the wallet unlocked; the balance chip stays in every
  tool's top bar.
- Ends: leaving the app to the launcher (activity `ON_STOP`) locks the wallet, and an explicit
  **Lock wallet** locks it immediately. A locked wallet removes the balance chip.
