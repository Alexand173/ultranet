# Staging gated readiness packet

**Status:** PREPARED — NOT PASSED
**Scope:** maintenance-only preparation for the isolated staging faucet
**Test reference:** `ULTRANET-STG-TEST-20260920-01` (selected draft window; not signed or active)
**Owner/operator:** Vladan Jotov
**Staging host:** `ubuntu-2gb-nbg1-2`
**Staging hostname:** `faucet-staging.ultranetwork.cc`
**Faucet unit:** `ultranet-faucet-staging.service`

This packet prepares the remaining private gates. It does not enable intake, start the faucet, submit a claim, call Siteverify, run replay POSTs, mutate credentials, create a backup, change Cloudflare state, or extend the expired test window.

## Safety boundary

The previous window, `2026-09-18T15:00:00Z`–`2026-09-18T17:00:00Z`, is expired. It must not be extended automatically. A new operator-selected UTC window and a fresh readiness confirmation are required before any bounded disabled validation or one-claim procedure.

Keep these invariants true while this packet is pending:

```text
FAUCET_ENABLED=false
ultranet-faucet-staging.service: inactive
127.0.0.1:8090: closed
production origin 167.233.161.115: excluded
production services, credentials, state, DNS, Caddy and firewall: untouched
```

Do not record wallet passwords, recovery phrases, Turnstile secrets, operator tokens, idempotency keys, claim recipients, signed envelopes, private keys, or raw request bodies here.

## Gate 1 — credential-store protection and disposition

**Current historical finding, not current acceptance:** the host credential store was recorded as `/var/lib/systemd/credential.secret`, `root:root`, mode `0400`, on unencrypted `/dev/sda1` ext4. `systemd-creds has-tpm2` reported partial capability with no usable TPM-backed protection. The old risk-acceptance wording and withdrawn approval record are not evidence for a new window.

Before a canary, record one of the following in a private change record:

- [ ] Provider-approved encrypted-at-rest storage or TPM2/LUKS-backed credential protection is installed and independently recovered/tested; or
- [ ] A separately approved, bounded private operator risk disposition covers exactly this disposable staging test, names the recovery owner/path, defines the cutoff and retention, and expires after closeout.

Required redacted evidence:

- host identity and storage/device metadata;
- credential-store owner/mode and systemd credential capability result;
- recovery path tested independently of the active SSH session;
- four staging encrypted credential filenames, owner/group/mode and hashes only;
- proof that protected plaintext and temporary encrypted artifacts are absent;
- explicit disposition owner, scope, expiry and stop action.

**Stop if:** the host identity, recovery path, storage protection, credential alias, metadata, or disposition is uncertain; a production credential is involved; or any secret appears in logs, shell history, process arguments, `/proc`, screenshots, or evidence.

Use [`faucet-staging-credential-store-disposition-draft.md`](./faucet-staging-credential-store-disposition-draft.md) for the private owner-signed bounded-risk record. The draft is not approval until its private fields are completed and signed.

## Gate 2 — consistent encrypted backup and recovery

No backup or restore is performed by this preparation packet. The operator must complete a staging-only, consistency-safe procedure in a separately approved maintenance session while intake is disabled and the faucet is stopped or otherwise quiesced according to the database procedure.

Required private evidence:

- [x] Exact staging database path and source revision recorded without production overlap.
- [x] SQLite database, WAL and SHM presence/state recorded before capture.
- [x] Consistency-safe SQLite `.backup` method recorded; no ordinary copy of a live database with an active WAL was used.
- [x] Encrypted backup artifact created outside the live database directory.
- [x] Backup checksum, size, owner/group/mode, encryption metadata and retention owner recorded; no plaintext database archive is retained beside the live database.
- [x] Backup is restored into a disposable non-live path, never over `/var/lib/ultranet-faucet-staging`.
- [x] Restored copy passes SQLite integrity checking and application-level state comparison for queue, reservations, claims, budget, balance, fee and nonce baselines.
- [x] Disable/stop rollback is tested with an independent recovery session and a fixed cleanup/retention decision.
- [x] Live state is unchanged after the test; no counters are reset to manufacture a zero baseline.

**Stop if:** WAL/SHM state is missing or inconsistent, encryption or integrity cannot be proven, the restore would touch live state, the service cannot be stopped and verified, or any unexpected claim/budget/nonce/state change appears.

## Gate 3 — replay/idempotency evidence

The September 18 single-request scope does not include extra replay POSTs. Do not generate replay evidence by sending a new claim request in this packet.

Before a canary, provide either prior approved evidence or a separately scoped replay test with its own approval, recipient isolation and rollback/reconciliation procedure:

- [ ] Reusing the same idempotency key and same request fields returns the original claim result without a second payout, debit, nonce or claim record.
- [ ] Reusing the same idempotency key with a changed destination is rejected with the documented `409` conflict contract.
- [ ] Responses are recorded as redacted status/contract evidence only; tokens, recipients, keys, request bodies and signed material are not retained.
- [ ] Node, faucet and SQLite state are reconciled after the replay test.
- [ ] The test has an explicit stop action and does not authorize a second claim.

**Stop if:** replay behavior is undocumented or ambiguous, the same key produces a second side effect, changed-destination reuse is accepted, or the evidence requires a live claim outside a separately approved scope.

## Gate 4 — fresh operator-selected test window

The former window is closed. A new window has been selected in the private draft, but it remains unsigned and inactive until all independent gates pass:

```text
Test reference: ULTRANET-STG-TEST-20260920-01
Owner/operator: Vladan Jotov
Window start UTC: 2026-09-20T11:00:00Z
Window end UTC: 2026-09-20T13:00:00Z
Fixed cutoff UTC: 2026-09-20T12:55:00Z
Readiness confirmed UTC: NOT CONFIRMED — pending completion of all independent gates
Readiness record/reference: ULTRANET-STG-READY-20260920-01
Abort authority and recovery path: Vladan Jotov; Hetzner console + verified SSH key
```

Use [`faucet-staging-fresh-window-record-draft.md`](./faucet-staging-fresh-window-record-draft.md) to complete the new private window record. Do not infer or extend the expired window; this draft does not authorize intake.

The new window is not valid until all of the following are checked in the private record:

- [ ] Credential-store gate passed or the bounded one-test risk disposition is explicit and current.
- [ ] Consistent encrypted SQLite/WAL/SHM backup and disposable restore passed.
- [ ] Replay evidence is prior-approved or separately scoped; no extra replay POST is smuggled into the one-claim window.
- [ ] Current egress/expiry/timer, host identity, binary/source/unit/environment and public route evidence is fresh.
- [ ] Cloudflare Web Analytics automatic setup is **Disabled** for the staging hostname, or the exact staging hostname is absent from the authenticated Web Analytics site inventory and recorded as not configured; no `static.cloudflareinsights.com` request is observed.
- [ ] Independent recovery access and immediate disable/stop authority are tested.
- [ ] Operator confirms exactly one disposable claim, no retry, no reload/new-tab resubmission, no changed-destination test, and no automatic extension.

**Stop at cutoff:** disable/stop through the private staging procedure, verify `FAUCET_ENABLED=false`, inactive service and closed port, preserve unresolved state, and do not resubmit or restart merely to drain or hide an uncertain result.

## Current decision

| Gate | Status | Next evidence |
| --- | --- | --- |
| Credential-store | BLOCKED / disposition not current | Remediation or bounded private risk record |
| Backup/recovery | PASSED / encrypted restore and live-state invariance verified | Retain private artifact, checksum, recovery result and cleanup record |
| Replay evidence | BLOCKED / outside current scope | Prior-approved evidence or separate replay scope |
| Web Analytics exact-host disposition | CLOSED / staging hostname not configured | Authenticated inventory evidence plus fresh no-beacon browser evidence |
| New test window | SELECTED / NOT SIGNED / NOT ACTIVE | Owner signature, readiness timestamp and remaining gate evidence |
| Intake | CLOSED | Keep `FAUCET_ENABLED=false`; no enable call |

## Definition of done

- [ ] Credential-store protection or bounded one-test disposition is current, private and independently recoverable.
- [x] Encrypted SQLite/WAL/SHM backup restores into a disposable path and matches the baseline without touching live state.
- [ ] Replay evidence is valid, separately authorized where necessary, and proves no duplicate side effect.
- [ ] Cloudflare Web Analytics automatic setup is disabled for the exact staging hostname, or the exact staging hostname is absent from the authenticated inventory and recorded as not configured; no beacon request is observed; the origin emits `no-store, no-transform`.
- [ ] A new operator-selected window, fixed cutoff, readiness timestamp and abort path are recorded.
- [ ] All existing safety invariants remain true until a separately approved procedure changes them.

## Execution attempt — 2026-09-19T06:40Z

This read-only/bounded preparation attempt did not close the canary gates:

- The earlier dashboard attempt reached the unauthenticated sign-in page; a later authenticated Web Analytics inventory screenshot dated 2026-09-19 listed only `ultranetwork.cc` and `musicotp.net`. The exact staging hostname `faucet-staging.ultranetwork.cc` was absent, so its Web Analytics site-level automatic setup is recorded as **CLOSED / NOT CONFIGURED**. The parent hostname's enabled setting is separate and was not changed. Existing helper/source checks found no analytics reference; fresh no-beacon browser evidence remains a separate verification item.
- Staging identity and safety checks passed: host `ubuntu-2gb-nbg1-2`, source `bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e` clean, `FAUCET_ENABLED=false`, faucet inactive/disabled, port `8090` closed, egress check passed, expiry timer active/enabled, Caddy staging active, override drop-in absent, and four encrypted credentials remained `root:root 0600`.
- The host credential store remains `/var/lib/systemd/credential.secret`, `root:root 0400`, on unencrypted ext4 with partial TPM capability. The bounded one-test path was selected, but no private risk-disposition record with recovery owner, cutoff, retention, and stop authority has been supplied; Gate 1 remains blocked.
- A read-only SQLite baseline passed `integrity_check` and `foreign_key_check`; `service_state.enabled=0`, schema version is `1`, and claims, payouts, idempotency, cooldown, abuse, and budget counts were zero. Opening the WAL database materialized an empty WAL and SHM sidecar; only those operation-created sidecars were removed after verifying the faucet was stopped and the database checksum was unchanged.
- Gate 2 was completed in a separately approved private maintenance session on 2026-09-20. The staging host was `ubuntu-2gb-nbg1-2` at `2.28.61.71`, source `bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e` was clean, the faucet remained inactive with `FAUCET_ENABLED=false`, port `8090` was closed, and the effective egress policy passed through `2026-09-21T10:04:01Z`.
- The exact database `/var/lib/ultranet-faucet-staging/faucet.db` was `ultranet-faucet-staging:ultranet-faucet-staging` mode `0600`, 77824 bytes, with no WAL/SHM sidecars. SQLite `.backup` produced `/var/backups/ultranet/faucet-staging-db/20260920T080847Z/faucet.db.asc`; the encrypted artifact is GPG-protected to the external recovery recipient, the staging directory is root-only `0700`, the artifact is root-only `0600`, and no plaintext database is present. The recovery copy is retained privately at `~/backups/faucet.db.asc` with mode `0600`; its decryption was independently verified.
- The restored disposable database passed integrity/foreign-key checks and matched the redacted baseline: schema/user version `1`, `service_state.enabled=0`, zero claims, payouts, idempotency keys, cooldowns, budget rows/reservations/confirmed units/claim counts, and abuse buckets. The live database SHA-256 remained `13727f7d895a5c539cdf61c980aa6caeee8350880576e10837a274bf1f127a9a` before and after. Gate 2 is **PASSED**; this is not replay evidence and does not authorize a claim.
- No credential mutation or database-content mutation occurred during verification. No service start, claim POST, Siteverify request, Cloudflare mutation, production contact, replay POST, or window selection occurred.

## Final configuration and documentation self-review — 2026-09-20

The final pre-sign-off review covered the checked-in staging configuration, the deployed staging host, and the linked readiness records:

- [x] `deploy/faucet.env.staging.example` remains staging-only, keeps `FAUCET_ENABLED=false`, uses the exact staging hostname/action contract, and contains no credential values.
- [x] `deploy/ultranet-faucet-staging.service` maps all four expected encrypted credential aliases, denies all destinations before the reviewed allowlist, restricts writes to the staging state path, and excludes production paths.
- [x] `deploy/ultranet-staging.service`, the faucet unit, egress expiry service/timer, and the installed staging units pass systemd verification; the deployed faucet remains inactive.
- [x] `deploy/Caddyfile.staging.example` and the installed `/etc/caddy/Caddyfile.staging` pass Caddy validation; staging Caddy remains separate and active.
- [x] The helper/source/configuration regression suite passes `76/76`; no Web Analytics source injection is present and the origin defense-in-depth header is `no-store, no-transform`.
- [x] Gate 2 backup/recovery evidence is recorded with external encryption, disposable restore, unchanged live state, corrected root-only retention permissions, and no plaintext database beside the live database.
- [x] The private credential-store disposition draft and fresh-window record draft are linked from the readiness, test, and claims-handoff documentation. They contain no signatures, inferred dates, secrets, recipients, idempotency keys, or claim authority.
- [x] Historical signed JSON approval artifacts remain untouched and are explicitly non-authoritative for the new test.
- [ ] Credential-store disposition is owner-signed and current.
- [ ] Replay/idempotency evidence is separately authorized and recorded.
- [ ] Independent no-beacon browser evidence is current.
- [ ] Vladan Jotov signs the selected future UTC window with its fixed cutoff and private recovery reference.

**Final self-review conclusion:** configuration and documentation are prepared for private sign-off, but the readiness packet remains **PREPARED — NOT PASSED**. Intake stays closed and no claim procedure is authorized.
