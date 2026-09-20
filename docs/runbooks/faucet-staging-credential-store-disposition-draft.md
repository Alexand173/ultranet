# Private staging credential-store disposition — draft

**Status:** DRAFT — NOT SIGNED — NOT ACTIVE
**Scope:** one disposable staging test only; no production or long-lived staging authority
**Owner/operator:** Vladan Jotov
**Staging host:** `ubuntu-2gb-nbg1-2`
**Staging service:** `ultranet-faucet-staging.service`
**Staging hostname:** `faucet-staging.ultranetwork.cc`

This is a private change-record draft. It is not an approval, signature, readiness confirmation, test window, or authorization to start the faucet, enable intake, submit a claim, send a replay request, call Siteverify, or change provider/network state. Complete and sign it only in the approved private operator system. Do not put the signature, private key, password, recovery phrase, Turnstile secret, operator token, recipient, idempotency key, raw request, or signed envelope in this repository or chat.

## 1. Decision requested

The owner is asked to approve a narrowly bounded residual-risk exception for exactly one disposable staging test because the current host credential-store protection is not equivalent to encrypted-at-rest or TPM2/LUKS-backed protection.

This exception must not be used to justify long-lived staging, production, repeated canaries, or any additional claim. TPM2/LUKS/provider-approved encrypted storage remains the required follow-up remediation.

```text
Disposition reference: ULTRANET-STG-CRED-RISK-20260920-01
Test reference: ULTRANET-STG-TEST-20260920-01
Decision: ACCEPT FOR EXACTLY ONE DISPOSABLE STAGING TEST
Owner/operator: Vladan Jotov
Host identity confirmed UTC: <actual UTC timestamp after independent host verification>
Source revision confirmed: bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e
Disposition created UTC: 2026-09-20T09:14:17Z
Disposition expires UTC: 2026-09-20T12:55:00Z
Retention/disposal decision reference: <private retention/disposal decision reference>
```

## 2. Current finding

Record only redacted metadata, checked against fresh host evidence:

```text
Credential store: /var/lib/systemd/credential.secret
Owner/group/mode: root:root 0400
Backing device/filesystem: /dev/sda1 ext4, unencrypted at rest
systemd-creds has-tpm2 result: partial capability; no usable TPM-backed protection
Production credential overlap: no
```

The finding describes host protection only. It does not mean that any staging credential was exposed, decrypted, copied, or included in this record.

## 3. Exact permitted scope

If approved, the disposition covers only:

- the staging host named above;
- the staging-only encrypted credentials mapped by the checked-in unit;
- one future operator-selected test window;
- one disposable claim, only after every independent technical gate is current;
- private disabled/start/stop validation needed to verify the credential projection;
- immediate post-test disablement, stop, reconciliation, and cleanup.

It does not cover:

- production services, credentials, state, DNS, Caddy, firewall, funds, or Cloudflare settings;
- long-lived storage of staging credentials on this host;
- a second claim, retry, reload/new-tab resubmission, replay POST, or changed-destination test;
- provider secret rotation, widget changes, or Siteverify probing outside the separately approved test;
- extending an expired window or creating an automatic future window.

## 4. Required conditions before use

The operator must check these in the private record immediately before the window:

- [ ] Fresh host identity and SSH/provider recovery path are verified independently of the active session.
- [ ] Staging source, binary, unit, environment, credential aliases, and database path match the approved revision.
- [ ] Production origin `167.233.161.115`, production services, credentials, state, DNS, Caddy, firewall, and funds are excluded.
- [ ] `FAUCET_ENABLED=false` is present and persisted service state is disabled.
- [ ] `ultranet-faucet-staging.service` is inactive outside explicitly bounded validation and port `8090` is closed.
- [ ] Effective egress policy is fresh, effective, and monitored by the expiry timer.
- [ ] Four staging encrypted credential files are present with metadata only: expected aliases, `root:root`, mode `0600`, and approved hashes.
- [ ] Protected plaintext and temporary encrypted artifacts are absent.
- [ ] The encrypted SQLite/WAL/SHM backup and independent disposable restore passed; live state is unchanged.
- [ ] Web Analytics exact-host disposition and the independent no-beacon browser check are current, or the test remains blocked.
- [ ] Replay evidence is prior-approved or separately scoped; this disposition does not create replay authority.
- [ ] The fresh window record is completed, signed, and independently recoverable.

## 5. Recovery and stop authority

Fill these fields privately; do not replace them with a chat statement:

```text
Recovery owner: Vladan Jotov
Independent recovery path: Hetzner console + verified SSH key
Recovery test reference/timestamp: <private recovery test reference and UTC timestamp>
Abort authority: Vladan Jotov
Backup/recovery evidence reference: sha256:46d97cc420a06c5dc30a5135a66c13fde8c4233d32c58ca3a0c03f0c89ba4169
Credential retention owner: Vladan Jotov
Plaintext/temp cleanup owner: Vladan Jotov
```

The exact stop action is:

1. Use the protected staging disable path if the service is running and the operator procedure remains reachable.
2. Assert persisted `FAUCET_ENABLED=false`.
3. Stop `ultranet-faucet-staging.service`.
4. Independently verify `ActiveState=inactive` and no listener on `127.0.0.1:8090`.
5. Preserve uncertain SQLite/ledger/claim state; never reset counters, restore over live state, retry, or restart merely to hide an ambiguous result.
6. Escalate through the independent recovery path if stop verification fails.

## 6. Stop conditions

Reject or immediately close the exception if any of the following occurs:

- host, source, unit, environment, credential alias, or production separation is uncertain;
- the independent recovery path is unavailable;
- an unexpected plaintext or temporary credential artifact exists;
- a secret appears in logs, shell history, process arguments, `/proc`, screenshots, or evidence;
- egress is stale/ineffective or the expiry monitor is not active;
- backup, restore, integrity, state comparison, or live-state invariance fails;
- a claim, replay, Siteverify call, provider mutation, or production contact occurs outside the approved scope;
- shutdown, disabled state, or port closure cannot be independently verified;
- the fixed cutoff is reached.

## 7. Owner attestation — complete privately

By signing the private record, the owner confirms that the exact scope, residual risk, recovery path, retention/cutoff, and stop action above are understood and accepted for one disposable staging test only. The owner does not approve production use, long-lived credential storage, a second claim, replay testing, or automatic extension.

```text
Owner name: Vladan Jotov
Owner role: <role>
Decision: accepted
Signature method/reference: GPG key D0A4140820960E9137F1ECFCDDDB8223C9645B04; private record label PRIVATE-OWNER-SIGNATURE-20260920
Signed UTC: <actual private signing UTC timestamp>
Private record integrity/reference: <private record integrity reference>
```

**Repository status:** This draft remains non-authoritative until the private fields are completed and the owner signs through the approved private change-record system.
