# Faucet claim on a physical phone — end-to-end test

**What this proves:** one real user, on a real handset, taps the claim card, passes the anti-bot
check on the hosted page, and receives 0.1 $ULTRA that a block confirms. Nothing else. The node is
not touched, and no configuration changes are needed to run it.

**Why it has to be a phone and a human:** the Turnstile token is bound to the hostname
`faucet.ultranetwork.cc` and the `faucet_claim` action, and automation profiles cannot pass the
widget. Every step up to the check is verified by the instrumented suite; the check itself is the
one step no emulator can complete.

## 1. Preconditions

| Check | Command | Expected |
|---|---|---|
| Faucet is open | `curl -s https://faucet.ultranetwork.cc/api/faucet/status` | `"enabled":true`, `"availability":"available"`, `"claim_amount_ultra":"0.100000"` |
| Float is sufficient | `curl -s https://api.ultranetwork.cc/api/account/fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8` | balance ≥ 0.1 + fee, well above the 0.2 reserve |
| Auto-miner is running | `sudo journalctl -u ultranet.service --since '10 minutes ago' --no-pager \| grep -Ei 'Auto-mine'` | at least one line in the last interval |
| Faucet service is healthy | `sudo systemctl --no-pager --full status ultranet-faucet.service` | `active (running)`, no restart counter increase |
| Address has not claimed today | see §4, query 1 | no row for the test address in the last 24 h |

Record before starting: float balance, `chain_length` from `curl -s https://api.ultranetwork.cc/api/stats`,
current UTC time.

## 2. Install the build

The APK to test (built 2026-11-14, includes the Profile tab with the receive code):

```
android-app/app/build/outputs/apk/debug/app-debug.apk
sha256 ef52355df2ba9fbcffc17a44b5493c561d1e484083de4f75bbeeb4d5250f48f3
size   36,690,539 bytes
```

Over USB with `adb`:

```
adb devices                       # the handset must be listed as "device", not "unauthorized"
adb install -r android-app/app/build/outputs/apk/debug/app-debug.apk
adb shell monkey -p cc.ultranet.ultraverse -c android.intent.category.LAUNCHER 1
```

Without `adb`: copy the file to the phone, open it, and allow "install unknown apps" for the file
manager. Verify the hash after copying if the file travelled over a chat app — those sometimes
re-encode attachments.

## 3. Walk the flow

The home screen offers the claim in one of three forms. Which one appears tells you what state the
app thinks the wallet is in.

| State | What the card says | What the tap does |
|---|---|---|
| No wallet | "Create a wallet to get free 0.1 $ULTRA" / "Limited to the first 10,000 users." | Opens Create wallet. Finish it, then come back — the card now reads differently. |
| Wallet locked | "Unlock your wallet to get free 0.1 $ULTRA" | Opens the unlock screen. |
| Unlocked, balance 0 | "Get free 0.1 $ULTRA to start" / limit line | Opens the claim sheet (§3.1). |

The card disappears once the wallet holds a confirmed balance. If it is still there after a
confirmed claim, pull to refresh the hero — the balance comes from the node, and a claim that has
not been included in a block yet is not a balance.

### 3.1 The claim sheet

1. The sheet opens with the header "Get free 0.1 $ULTRA" and loads
   `https://faucet.ultranetwork.cc/?address=<your address>`. The address is prefilled — the field
   should already hold 64 hex characters, with the 64 indicator cells under it lit.
2. Turnstile runs inside the page. Expect a brief spinner, then the check. On a phone this is
   usually one tap; if it asks for a puzzle, complete it.
3. Status line reads "Verification complete. Claim when ready." and the button becomes enabled as
   "Claim 0.1 $ULTRA".
4. Tap it. Expect "Submitting the claim…" for a moment, then the receipt panel:
   claim id, "Waiting for a block to confirm it.", and the status line
   "Claim accepted. Payout follows once a block includes it."

Accepted is not paid. The faucet signs and submits; the balance moves only when a block includes
the transfer. With the auto-miner running this is normally well under a minute.

5. When the poll returns `confirmed`, the receipt reads
   "Confirmed on chain. The 0.1 $ULTRA is in your wallet." and the sheet closes with a snackbar:
   "You received 0.1 $ULTRA."
6. Back on Home, the hero shows the new balance and the claim card is gone.

If the sheet is closed between steps 4 and 5, the claim is still valid: the faucet worker finishes
it. Check the address balance before claiming again — a second claim inside the 24 h cooldown is
refused with 409.

## 4. Verify in the faucet

All read-only. `FAUCET_STATE_PATH` is `/var/lib/ultranet-faucet/faucet.db`.

**Query 1 — the claim row** (replace `<address>`):

```
sudo sqlite3 -readonly /var/lib/ultranet-faucet/faucet.db \
  "SELECT claim_id, status, amount_base_units, fee_base_units, datetime(created_at,'unixepoch'),
          datetime(confirmed_at,'unixepoch'), failure_code
     FROM claims WHERE address='<address>' ORDER BY created_at DESC LIMIT 3;"
```

Expected: one row, `status` moving `queued` → `submitting` → `pending` → `confirmed`,
`amount_base_units` 100000, `failure_code` NULL.

**Query 2 — the payout and its transaction hash:**

```
sudo sqlite3 -readonly /var/lib/ultranet-faucet/faucet.db \
  "SELECT p.claim_id, p.transaction_hash, p.nonce, p.attempt_count, p.last_error_code
     FROM payouts p JOIN claims c ON c.claim_id = p.claim_id
    WHERE c.address='<address>' ORDER BY p.claim_id DESC LIMIT 3;"
```

Expected: exactly one `transaction_hash`. `attempt_count` above 1 means the first submission was
retried — look at `last_error_code` and the worker log before concluding anything.

**Query 3 — the budget window booked the spend:**

```
sudo sqlite3 -readonly /var/lib/ultranet-faucet/faucet.db \
  "SELECT datetime(window_start_utc,'unixepoch'), reserved_base_units, confirmed_base_units, claim_count
     FROM budget_windows ORDER BY window_start_utc DESC LIMIT 2;"
```

**Service log** — the worker logs failures and nothing else, so an empty result is the good case:

```
sudo journalctl -u ultranet-faucet.service --since '30 minutes ago' --no-pager \
  | grep -Ei 'faucet worker cycle failed|payout attempt failed'
```

**On chain** — the hash from query 2 should be in a block:

```
curl -s https://api.ultranetwork.cc/api/stats | grep chain_length
```

`chain_length` must have advanced past the value recorded in §1.

## 5. When it fails

| Symptom | Meaning | Action |
|---|---|---|
| Sheet shows "The claim page could not be loaded." | WebView could not reach the host | Check the phone's connection; reopen the card. Do not retry blindly in a loop. |
| "The verification widget could not load." | Turnstile blocked or offline | Reload the page inside the sheet; if it persists, test the same URL in Chrome on the phone to separate the network from the app. |
| 422 "Verification failed." | Token expired or was already used | The page resets the widget; complete the check again. |
| 409 "This address already claimed…" | Inside the 24 h cooldown | Expected. Stop; the address has already been served. |
| 429 "Too many claims from this network." | Per-IP (25/day) or per-subnet (250/day) consequence of a shared carrier address | Wait for `Retry-After`. This is a policy hit, not a bug. |
| 503 "The faucet cannot pay out right now…" | Paused, out of funds, or below reserve | Check `/api/faucet/status` and the float; do not disable and re-enable as a first response. |
| "No answer from the faucet." | The POST timed out | The claim may still have landed. Run query 1 before any retry. |
| Receipt stays "Waiting for a block…" for more than 15 minutes | Confirmation timeout (900 s) | Confirm the auto-miner is running (preconditions), then check the payout row; the queue will retry up to 5 attempts. |

**Never** work around a failure with a second address to "see if it works": that spends float and
pollutes the abuse counters that this test is meant to observe.

**Stop switch:** if the faucet must be taken out of service during the test,
`POST /internal/disable` (operator token required) on the host, and record why.

## 6. Record sheet

| Field | Value |
|---|---|
| Tested at (UTC) | |
| Phone model / Android version | |
| App build sha256 | `ef52355d…` |
| Claimant address | |
| Claim state seen (create / unlock / claim) | |
| Claim id | |
| Transaction hash | |
| Block height at start → end | |
| Float before → after | |
| Wall time from tap to confirmed | |
| Result / observations | |
