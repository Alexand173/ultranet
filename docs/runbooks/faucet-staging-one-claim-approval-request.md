# DRAFT — staging one-claim approval request

**Status:** DRAFT — NOT AUTHORIZATION
**Scope:** exactly one disposable claim on the isolated staging faucet only
**Host:** `ubuntu-2gb-nbg1-2`
**Public hostname:** `faucet-staging.ultranetwork.cc`
**Staging unit:** `ultranet-faucet-staging.service`

## Evidence available for review

- Server-side staging Turnstile rotation completed under the unchanged alias `faucet-staging-turnstile.secret`.
- Current encrypted credential SHA-256: `f819078d9f2e08f8c771d78f305832ba7f9878f7a9655daa9a3e9f313b35a922`.
- Root-only encrypted rollback copy preserved with SHA-256: `4034af98c4e4d16a518e34f1a206ed1fa6c810abf58a45cfbcc5722b980b9d53`.
- `99-canary-override.conf` is absent; the reviewed egress drop-in remains effective.
- Fresh disabled validation passed: `FAUCET_ENABLED=false`, `ultranet-faucet-staging.service` inactive, port `8090` closed.
- The canonical disabled status validation previously returned `enabled=false`, `availability=disabled`, fixed amount `1.000000`, and six decimals; no accepted claim was recorded.
- The formal residual-risk acceptance is recorded in `.kombai/debug/debug-changelog-20260912-0803.md`: the unencrypted host credential-store exception is accepted for this single staging canary only, not for production or long-lived staging use.

## Requested authorization

I request written authorization for **exactly one disposable staging claim** to validate the server-side Cloudflare Turnstile Siteverify path and the complete staging claim reconciliation. This request does not authorize production activity, long-lived staging enablement, or any second claim.

### Required conditions before execution

- Independent reviewer confirms the host, staging hostname, widget/action, binary/unit/environment revisions, egress policy, and rollback evidence.
- A private operator procedure is ready to enable intake for one claim without placing tokens, recipient addresses, idempotency keys, operator credentials, signer material, or signed envelopes in chat, source, ordinary logs, or this document.
- A fresh disposable recipient and fresh idempotency key are selected privately at execution time and are not recorded here.
- Staging faucet and validator state, balance, fee, nonce, database, and backup/rollback baselines are captured before enablement.
- No production signer, funds, DNS, service, firewall, credential, or state is used.

### Authorized execution boundary after approval

1. Confirm the faucet is still disabled and the pre-canary evidence is unchanged.
2. Enable intake through the protected staging operator procedure for one claim only.
3. Submit exactly one claim from the reviewed staging helper with a fresh Turnstile token and private disposable recipient.
4. Confirm the server-side Siteverify call succeeds and reconcile exactly one claim, transaction, fee, nonce, source debit, recipient credit, budget record, and SQLite record.
5. Stop or disable intake immediately after reconciliation; any mismatch is a permanent stop.
6. Rotate the staging Turnstile secret immediately after the canary using the approved protected procedure and preserve only redacted checksums/metadata.
7. Confirm `FAUCET_ENABLED=false`, the faucet is inactive, port `8090` is closed, egress remains effective, and no secret-bearing evidence exists.

### Automatic denial conditions

This approval is void if any of the following occurs: host or widget mismatch, stale egress policy, missing rollback, provider failure, duplicate payout, balance/nonce/hash mismatch, unexpected fee, leaked secret/token/header/body, production path or credential access, public internal-route exposure, OOM/restart drift, or inability to disable intake immediately.

## Approval record

- **Approving owner:** ______________________________
- **Independent reviewer:** _________________________
- **Approval reference:** ____________________________
- **Approval timestamp UTC:** ________________________
- **Canary window:** _________________________________
- **Post-canary rotation owner:** ____________________

**Approval statement:** I authorize exactly one disposable staging claim under the boundaries above. I understand that the residual host credential-store risk is accepted only for this single canary and that post-canary secret rotation and disablement evidence are mandatory.

**Signature:** ______________________________________
