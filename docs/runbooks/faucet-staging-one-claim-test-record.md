# Staging one-claim development test record

**Status:** TEST DETAILS RECORDED — READINESS AND RESULTS NOT YET RECORDED
**Scope:** exactly one disposable claim on the isolated staging faucet only

This record replaces the withdrawn, incorrect approval record at the owner's request. The former approval statement, signature, and reviewer attribution are not valid evidence and must not be used. This is an operator-run development test, not an independent-review or production approval record. No independent reviewer is assigned or required for this development test; Vladan Jotov records the technical checks as operator.

## Development context

As stated by the owner, UltraNet is not yet fully established, has only one validator, and the current value of `$Ultra` is zero. These are owner-supplied development-context statements, not independently verified network or market findings. A successful test validates only the tested staging claim path; it does not establish multi-validator consensus, decentralization, market value, or production readiness.

Zero token value does not remove credential, infrastructure, privacy, or transaction-integrity risks. Technical checks and staging isolation still apply.

## Test details

- **Owner:** Vladan Jotov
- **Operator:** Vladan Jotov
- **Test reference:** ULTRANET-STG-TEST-20260918-01
- **Timestamp:** 2026-09-18T14:13:26Z
- **Window:** 2026-09-18T15:00:00Z – 2026-09-18T17:00:00Z

The timestamp and window are supplied test metadata, not evidence of readiness, actual start, or completion. Actual execution timestamps and results must be recorded separately. This documentation update does not execute a claim or change live services.

## Intended staging target — verify before testing

- Host: `ubuntu-2gb-nbg1-2`
- Public hostname: `faucet-staging.ultranetwork.cc`
- Faucet unit: `ultranet-faucet-staging.service`
- Validator unit: `ultranet-staging.service`

These are the runbook's intended staging identifiers, not fresh observations. Never substitute a production host, service, signer, credential, endpoint, or state path.

The remaining private gates are prepared in [`faucet-staging-gated-readiness.md`](./faucet-staging-gated-readiness.md). That packet is not a readiness approval and does not select or extend a test window.

Use the [`credential-store disposition draft`](./faucet-staging-credential-store-disposition-draft.md) and [`fresh-window record draft`](./faucet-staging-fresh-window-record-draft.md) for private completion. Neither draft is an approval until Vladan Jotov completes the fields and signs through the approved private record system.

## Operator checks before testing

- [ ] Verify host, staging isolation, hostname, widget, helper/server action `faucet_claim`, and binary/unit/environment identities.
- [ ] Record fresh evidence for all technical gates in [the claims handoff](../../deploy/faucet-staging-claims-enablement.md#phase-c--backup-and-operator-test-readiness), including egress/expiry, proxy/WAF boundaries, clock, replay-test evidence, and credential protection.
- [ ] Confirm the private operator procedure, immediate disable/stop, and recovery access work. Record readiness and its actual UTC timestamp.
- [x] Capture consistent staging SQLite/WAL backup and baseline queue, budget, balance, fee, nonce, and node state; no claim was executed by this capture.
- [ ] Complete and privately sign the [`credential-store disposition draft`](./faucet-staging-credential-store-disposition-draft.md); do not infer risk acceptance from the withdrawn approval record or from zero token value. Missing protection or a valid bounded operator risk disposition remains a stop condition.
- [ ] Select a fresh disposable recipient and high-entropy idempotency key privately. Never record them or tokens, credentials, signer material, or signed envelopes here.
- [ ] Confirm intake remains disabled until all checks pass and the recorded test window is open.

Missing or failed technical evidence blocks the test. Historical runbook observations are not current readiness evidence. If the window expires unused, record a new operator-selected window before testing; no automatic extension is permitted.

## One-claim test procedure

1. Verify the disabled faucet and current baseline, then enable intake once through the protected staging operator procedure. A stopped upstream cannot prove disabled application status: complete the handoff's application checks only within separately scoped bounded disabled validation, with verified shutdown.
2. Submit exactly one claim from one controlled session of the reviewed staging helper using a fresh Turnstile token and private disposable recipient. No retry, reload/new-tab resubmission, replay POST, changed-destination test, or second claim is included. The helper's in-memory attempt lock is not a durable or server-side claim limit.
3. Require `202 Accepted` as admission only, not payout confirmation. Check server-side Siteverify acceptance, monitor until `confirmed` within the operator's bounded procedure, and reconcile the claim, transaction hash, fee, nonce, source debit, recipient credit, budget, and SQLite record. Do not disable on the normal path solely because admission succeeded: disablement prevents queued payouts from starting. Failure, uncertainty, timeout or cutoff still requires immediate disable/stop.
4. Disable intake and stop the faucet immediately after reconciliation, on any failure, or at window end—whichever comes first. Preserve unresolved state at cutoff; never resubmit or reset state to hide an uncertain result.
5. Follow the [post-canary rotation and closeout procedure](./faucet-staging-turnstile-rotation.md#13-post-canary-rotation-and-closeout), including provider rotation/overlap/invalidation tracking, protected replacement, and cleanup. A disabled validation needs explicit operator scope; no extra claim or Siteverify probe is included.
6. Verify persisted `FAUCET_ENABLED=false`, inactive faucet, closed port `8090`, effective egress, and preserved reconciliation evidence. A completed claim and confirmed budget must not be reset to zero.

Stop for host/widget mismatch, stale egress, missing backup, provider failure, duplicate payout, balance/nonce/hash/fee mismatch, secret exposure, production access, public internal-route exposure, OOM/restart drift, or inability to disable immediately. Mandatory disablement and secret-containment closeout continue after the window if needed; this does not extend claim authority.

Disablement is not cancellation or rollback of a transaction already in flight. A running disabled service can still poll pending transactions; `/internal/reconcile` executes worker logic and is not a read-only status probe. Do not restart or re-enable to drain unresolved work after cutoff. Preserve evidence for separately scoped recovery. Offline helper regression tests do not establish live Siteverify acceptance, server idempotency or payout readiness.

## Test results — pending

- **Operator readiness confirmation / actual UTC timestamp:** Not recorded.
- **Fresh technical evidence reference:** Not recorded.
- **Actual test start / stop UTC:** Not recorded.
- **Siteverify and one-claim reconciliation outcome:** Not recorded.
- **Disablement, rotation, and cleanup evidence:** Not recorded.
- **Final result:** Not recorded — no pass or execution is asserted by this document.

Keep evidence redacted and private where required. Do not create or modify `owner-1-approval.json` for this test record; it is not a substitute for technical evidence.
