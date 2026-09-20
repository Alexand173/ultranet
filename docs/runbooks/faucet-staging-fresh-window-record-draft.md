# Fresh staging test-window record — draft

**Status:** DRAFT — SELECTED — NOT SIGNED — NOT ACTIVE
**Scope:** exactly one disposable staging claim, subject to all independent gates
**Owner/operator:** Vladan Jotov
**Staging host:** `ubuntu-2gb-nbg1-2`
**Staging hostname:** `faucet-staging.ultranetwork.cc`
**Faucet unit:** `ultranet-faucet-staging.service`

This record replaces no prior approval and does not extend the expired `2026-09-18T15:00:00Z`–`2026-09-18T17:00:00Z` window. Do not fill timestamps by inference. Do not treat this draft, the repository, or a historical JSON approval artifact as authorization.

## 1. Window identity — complete privately

```text
Test reference: ULTRANET-STG-TEST-20260920-01
Owner/operator: Vladan Jotov
Window start UTC: 2026-09-20T11:00:00Z
Window end UTC: 2026-09-20T13:00:00Z
Fixed cutoff UTC: 2026-09-20T12:55:00Z
Readiness confirmed UTC: NOT CONFIRMED — pending completion of all independent gates
Readiness record/reference: ULTRANET-STG-READY-20260920-01
Credential disposition reference: ULTRANET-STG-CRED-RISK-20260920-01
Backup/recovery evidence reference: sha256:46d97cc420a06c5dc30a5135a66c13fde8c4233d32c58ca3a0c03f0c89ba4169
Abort authority and recovery path: Vladan Jotov; Hetzner console + verified SSH key
```

The window is invalid if it is in the past, the cutoff is after the end, the owner-signed disposition is missing, or the private readiness record cannot be recovered independently.

## 2. Pre-window gate checklist

Complete in the private readiness record. A checked template item is not evidence by itself.

- [ ] Owner-signed bounded credential-store disposition is current, exact-host scoped, independently recoverable, and expires after closeout.
- [ ] Host identity, source revision, binary checksum, unit/environment revision, credential aliases, and production separation are fresh.
- [ ] `FAUCET_ENABLED=false` is present and persisted; the faucet is inactive and port `8090` is closed before the procedure.
- [ ] Validator identity, chain/network profile, signer/address, decimals, fee policy, node state, and loopback boundaries are fresh.
- [ ] Effective egress allowlist and resolver policy are current; expiry timer is active; no canary override is present.
- [ ] Encrypted SQLite/WAL/SHM backup and disposable restore passed; live database checksum/state is unchanged.
- [ ] Independent recovery access and immediate disable/stop authority are tested.
- [ ] Public route, TLS/SNI, WAF/path boundaries, cache policy, rate limits, and Caddy separation are fresh where required.
- [ ] Exact staging Turnstile hostname/action/sitekey and server-side secret provenance are verified privately.
- [ ] Web Analytics exact-host disposition is recorded as `CLOSED / NOT CONFIGURED` or visibly `Disabled`; independent no-beacon browser evidence passed.
- [ ] Replay/idempotency evidence is prior-approved or separately scoped. No replay POST is included in this window unless its separate authorization explicitly says so.
- [ ] The owner explicitly accepts exactly one disposable claim, no retry, no reload/new-tab resubmission, no changed-destination test, and no automatic extension.

## 3. Authorized test boundary

If and only if every pre-window gate is current and the private readiness record is signed:

1. Reconfirm the host, window, cutoff, safety invariants, and independent stop path.
2. Enable intake once through the protected staging operator procedure; do not edit the repository or environment as a shortcut.
3. Use one controlled helper session, one fresh private disposable recipient, and one fresh high-entropy idempotency key. Never record those values here.
4. Submit exactly one claim. Do not retry, reload, open a new tab, submit a second request, replay the same key, or change the destination.
5. Treat `202 Accepted` as admission only. Monitor and reconcile the redacted claim, node transaction, fee, nonce, source debit, recipient credit, budget, and SQLite state through the private procedure.
6. Disable intake and stop the faucet after successful reconciliation, on any failure/uncertainty, or at the fixed cutoff—whichever comes first.
7. Verify persisted disabled state, inactive service, closed port, effective egress, preserved state, and post-test credential/secret cleanup.

This record does not authorize a provider Siteverify probe outside the one claim, an additional replay test, a second payout, production access, or a window extension.

## 4. Immediate stop and cutoff procedure

Stop immediately for host/widget mismatch, stale egress, missing backup, provider failure, duplicate side effect, fee/hash/nonce/balance mismatch, secret exposure, production overlap, public internal-route exposure, OOM/restart drift, database inconsistency, or loss of recovery access.

At cutoff:

- stop new admission through the protected staging procedure;
- stop the faucet and independently verify `FAUCET_ENABLED=false`, inactive service, and closed port `8090`;
- preserve uncertain claim/ledger/database state;
- do not retry, replay, reload, restart to drain, reset counters, or extend the window;
- record the unresolved state and escalate through the private recovery path.

## 5. Closeout record — complete after execution

```text
Actual start UTC: <not recorded until execution>
Actual stop UTC: <not recorded until execution>
Admission status: <not recorded until execution>
Claim/reconciliation reference: <private reference; no recipient or claim ID here>
Provider response/result: <redacted private result>
Disable/stop verification reference: <private reference>
Post-test backup/state comparison: <private reference>
Turnstile rotation/cleanup reference: <private reference>
Final outcome: <not executed / passed / failed / uncertain>
```

No claim result, payout result, Siteverify result, recipient, idempotency key, signed envelope, secret, or raw request body belongs in this repository.

## 6. Owner confirmation — complete privately

```text
Owner name: Vladan Jotov
Selected reference: ULTRANET-STG-TEST-20260920-01
Selected window/cutoff: 2026-09-20T11:00:00Z–2026-09-20T13:00:00Z; cutoff 2026-09-20T12:55:00Z
Exactly-one-claim boundary accepted: yes
No-retry/no-replay/no-extension boundary accepted: yes
Readiness record reviewed UTC: <actual owner review UTC after all gates pass>
Signature method/reference: GPG key D0A4140820960E9137F1ECFCDDDB8223C9645B04; private record label PRIVATE-OWNER-SIGNATURE-20260920
Owner signature UTC: <actual private signing UTC timestamp>
```

**Repository status:** This draft is not a readiness approval and must remain inactive until the private fields are completed, technically evidenced, and owner-signed.
