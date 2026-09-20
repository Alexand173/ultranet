# Private staging sign-off checklist — redacted coordination copy

**Status:** PREPARATION ONLY — NOT SIGNED — NOT ACTIVE
**Scope:** one disposable staging test; no production authority and no automatic extension
**Owner/operator:** Vladan Jotov
**Staging host:** `ubuntu-2gb-nbg1-2`
**Staging hostname:** `faucet-staging.ultranetwork.cc`
**Faucet unit:** `ultranet-faucet-staging.service`

This checklist coordinates the private credential-disposition and fresh-window signing workflow. It is not an approval, readiness confirmation, claim authorization, provider mutation, service-start instruction, or replay authorization. Complete the authoritative records only in the approved private change-record system.

Do not place passwords, recovery phrases, Turnstile secrets, operator tokens, private keys, recipients, idempotency keys, signed envelopes, raw request bodies, signature bytes, or secret-bearing logs in this repository or chat.

## 1. Fixed non-secret references

```text
Credential disposition reference: ULTRANET-STG-CRED-RISK-20260920-01
Fresh test reference: ULTRANET-STG-TEST-20260920-01
Readiness record reference: ULTRANET-STG-READY-20260920-01
Approved source revision: bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e
Encrypted backup evidence reference: sha256:46d97cc420a06c5dc30a5135a66c13fde8c4233d32c58ca3a0c03f0c89ba4169
Recovery path: Hetzner console + verified SSH key
Abort authority: Vladan Jotov
```

## 2. Selected window

```text
Window start UTC: 2026-09-20T11:00:00Z
Window end UTC: 2026-09-20T13:00:00Z
Fixed cutoff UTC: 2026-09-20T12:55:00Z
Readiness confirmed UTC: NOT CONFIRMED
Intake: CLOSED
```

The cutoff is before the window end. Do not extend the window automatically. If the window is not signed and all gates are not current before the start, record it as unused/blocked and select a new future window through the same private process.

## 3. Current gate status

| Gate | Current state | Required before signing/enablement |
| --- | --- | --- |
| Credential-store | Blocked until current private disposition is completed | Independent host/recovery evidence, exact scope, retention, cutoff, stop authority and owner signature |
| Encrypted backup/recovery | Passed in the separate maintenance record | Retain private artifact/checksum/recovery/cleanup references; do not restore over live state |
| Replay/idempotency | Blocked / outside the one-request scope | Prior-approved evidence or separately authorized isolated replay scope |
| Web Analytics | Exact staging hostname recorded closed/not configured | Fresh independent no-beacon browser evidence remains required |
| Fresh window | Selected / not signed / not active | Owner review, private signature and readiness confirmation |
| Intake | Closed | Keep `FAUCET_ENABLED=false` until every gate passes |

A passed backup gate does not authorize a claim. A selected window does not authorize intake.

## 4. Private inputs to complete

### Credential disposition

- [ ] Host identity and storage metadata rechecked at the current private verification time.
- [ ] Independent recovery test reference and UTC timestamp recorded.
- [ ] Retention/disposal decision reference recorded.
- [ ] Owner role recorded.
- [ ] Disposition expiry confirmed as `2026-09-20T12:55:00Z`.
- [ ] Private signing system confirms the authorized signing key/fingerprint.
- [ ] Owner signs the credential disposition in the approved private system.
- [ ] Signed UTC and private integrity/reference recorded without signature bytes.

### Fresh window

- [ ] Credential disposition is owner-signed and independently recoverable.
- [ ] All technical gates are current, including replay evidence and no-beacon evidence.
- [ ] Readiness record is reviewed against the exact selected window and cutoff.
- [ ] Owner confirms exactly one disposable claim, no retry, no replay, no changed destination, and no extension.
- [ ] Owner signs the fresh-window record in the approved private system.
- [ ] Owner review UTC, signature UTC and private signature/reference are recorded without signature bytes.
- [ ] Readiness confirmed UTC is recorded only after every gate passes.

Do not use the governance approval JSON artifacts described by `OFFLINE_APPROVAL_SIGNING.md` as substitutes for these faucet records. They belong to a separate validator-approval workflow.

## 5. Signing sequence

1. Freeze the selected references and window. Do not edit the repository drafts while a private signature ceremony is in progress.
2. Confirm the private records contain only redacted metadata and the exact non-secret references above.
3. Verify host identity, independent recovery, source revision, unit/environment revision, credential aliases, production separation, egress expiry, service state, port closure and backup/recovery evidence.
4. Resolve every hard gate. In particular, do not sign readiness while replay/idempotency evidence or no-beacon evidence is still blocked.
5. In the approved private change-record system, sign the credential disposition first. Store the signature and integrity evidence privately.
6. Review the fresh-window record against the signed disposition, then sign the fresh-window record privately.
7. Record only private reference labels, UTC timestamps and permitted digests in the repository. Never copy signature bytes or private key material here.
8. Perform a final independent recovery/stop check. Keep intake closed until the private readiness record is confirmed.

Signing these records does not itself start the faucet, enable intake, submit a claim, call Siteverify, run replay requests, or mutate Cloudflare/network state.

## 6. Pre-window execution boundary

Only after the private readiness record is signed and every gate is current:

1. Reconfirm the host, source, window, cutoff, safety invariants and independent stop path.
2. Enable intake once through the protected staging operator procedure; do not edit the repository or environment as a shortcut.
3. Use one controlled helper session, one private disposable recipient and one fresh high-entropy idempotency key. Do not record those values here.
4. Submit exactly one claim. Do not retry, reload, open a new tab, replay the key, or change the destination.
5. Treat `202 Accepted` as admission only. Reconcile claim, node transaction, fee, nonce, balances, budget and SQLite state privately.
6. Disable intake and stop the faucet after reconciliation, on uncertainty, or at the fixed cutoff—whichever comes first.
7. Verify persisted `FAUCET_ENABLED=false`, inactive service, closed port `8090`, effective egress, preserved state and cleanup.

## 7. Stop conditions

Stop and preserve unresolved state if any of the following occurs:

- host, source, credential alias, widget, action or production separation mismatch;
- unavailable independent recovery or stop authority;
- stale/ineffective egress or inactive expiry monitoring;
- missing/invalid backup or failed state comparison;
- duplicate side effect, fee/hash/nonce/balance mismatch or provider failure;
- secret, recipient, token, key, request body or signed material exposure;
- replay evidence is required but not separately authorized;
- cutoff reached, service shutdown cannot be verified, or database state is uncertain.

Never retry, replay, reset counters, restore over live state, restart to hide an uncertain result, or extend the window automatically.

## 8. Closeout references

Complete privately after execution or abort:

```text
Actual start UTC: <private value or not executed>
Actual stop UTC: <private value or not executed>
Admission/reconciliation reference: <private redacted reference>
Disable/stop verification reference: <private reference>
Post-test backup/state comparison: <private reference>
Credential cleanup reference: <private reference>
Final outcome: <not executed / passed / failed / uncertain>
```

**Repository status:** This checklist and the linked drafts remain preparation records. They do not authorize a live claim.
