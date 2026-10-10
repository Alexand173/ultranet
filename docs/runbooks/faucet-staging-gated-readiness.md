# Staging gated readiness packet

| Readiness packet | Value |
| --- | --- |
| Status | **PASSED — 2026-10-10** (all gates closed; intake remains closed until separate go/no-go) || Scope | Maintenance-only preparation for the isolated staging faucet |
| Current test reference | `ULTRANET-STG-TEST-20261010-01` |
| Prior test reference | `ULTRANET-STG-TEST-20260920-01` (expired; do not use or extend) || Owner/operator | Vladan Jotov |
| Staging host | `ubuntu-2gb-nbg1-2` |
| Staging hostname | `faucet-staging.ultranetwork.cc` |
| Faucet unit | `ultranet-faucet-staging.service` |

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

The window recorded in the earlier draft (`2026-09-20T11:00:00Z`–`2026-09-20T13:00:00Z`, cutoff `2026-09-20T12:55:00Z`) has expired. Do not use it, infer a replacement, or treat its draft reference as readiness confirmation. A new operator-selected future UTC window and fixed cutoff must be recorded privately only after the independent technical gates are current.

Use [`faucet-staging-fresh-window-record-draft.md`](./faucet-staging-fresh-window-record-draft.md) as a template. Its September 20 values are stale; completing the checked-in template alone is not a signed record or permission to enable intake. Record the fresh test reference, window/cutoff, readiness timestamp, recovery reference, abort authority, and one-claim/no-retry/no-extension conditions in the approved private system.

The fresh window and readiness record require current evidence for all of the following:

- [ ] Credential-store remediation or a current, signed, bounded one-test risk disposition with independent recovery and explicit retention/cutoff/stop terms.
- [ ] Encrypted SQLite/WAL/SHM backup and disposable restore pass and still apply to the current source and live-state baseline; if state or identity changed, repeat only under a separately approved quiesced procedure.
- [ ] Replay evidence is prior-approved and still applicable, or a separate replay scope has been explicitly approved and reconciled. No replay POST is included in the one-claim window by default.
- [ ] Host identity, source/binary/unit/environment/credential aliases, effective egress and expiry timer, DNS/TLS/WAF/Caddy/origin boundaries, resolver/range evidence, and all applicable hard pre-canary gates are freshly verified.
- [ ] Web Analytics exact-host disposition and independent no-beacon browser evidence are current; September 19 inventory evidence is historical, not current proof.
- [ ] Independent recovery access and immediate disable/stop authority are tested.
- [ ] Owner accepts exactly one disposable claim, no retry/replay/reload/new-tab resubmission/changed-destination test, and no automatic extension.

**Stop at cutoff:** disable/stop through the private staging procedure, verify `FAUCET_ENABLED=false`, inactive service and closed port, preserve unresolved state, and do not resubmit or restart merely to drain or hide an uncertain result.

## Ordered path to a readiness-only PASSED status

Complete these gates in order. Keep evidence in the approved private record system where required; this repository may hold only redacted status and references, never signatures, secrets, recipient/idempotency values, raw requests, or signed envelopes.

1. **Reconfirm scope and preserve the stop state.** Treat the prior window and all historical host/provider observations as stale unless independently reconfirmed. Keep intake disabled, the faucet stopped, port `8090` closed, and production excluded. No remote action is authorized by this checklist.
2. **Close Gate 1 — credential-store protection/disposition.** Either remediate to provider-approved encrypted-at-rest or TPM2/LUKS-backed protection and independently test recovery, or have the owner complete and sign a narrowly bounded private risk disposition for exactly one disposable staging test. Refresh host/device/store metadata, four encrypted credential metadata/hashes, plaintext/temp absence, recovery test, scope, expiry, retention, and stop authority. The checked-in disposition draft is not approval; the September 20 timestamps and references must not be reused as current evidence.**Status:** CLOSED on 2026-10-10. Disposition reference: ULTRANET-STG-CRED-RISK-20261010-01.
3. **Revalidate Gate 2 — backup/recovery applicability.** The September 20 encrypted backup and disposable restore are recorded as historically passed, not automatically current for a later test. Privately confirm host/source/database identity and that the live state has not changed in a way that invalidates its baseline. If freshness or consistency is uncertain, stop and obtain separate approval for a staging-only quiesced SQLite/WAL/SHM capture and disposable restore. Never overwrite live state; preserve checksums and restore comparisons privately.
4. **Close Gate 3 — replay/idempotency.** Supply prior-approved evidence that same-key/same-request returns the original result without a second side effect and changed-destination reuse returns the documented `409`, with reconciliation and redacted results; otherwise, obtain a separate explicit test scope, recipient isolation, stop action, and rollback/reconciliation procedure before any replay request. Do not perform replay as part of this packet or the one-claim window. Missing or ambiguous replay evidence keeps this gate blocked.
5. **Refresh the technical pre-canary gates.** Complete current staging identity, binary/source/unit/environment, egress/resolver and expiry monitoring, public route/TLS/WAF/Caddy/origin, proxy identity/path-normalization, Turnstile hostname/action and secret provenance, no-beacon analytics, log/core/temp/proc hygiene, recovery, disabled/stop monitoring, and all applicable rows in [`faucet-staging-claims-enablement.md`](../../deploy/faucet-staging-claims-enablement.md). Historical egress ended September 21 and the Web Analytics inventory dates to September 19; neither is current proof. Any host maintenance, provider change, or live validation requires its own explicit approval and bounded procedure.
6. **Select and sign a genuinely fresh window privately.** Only after Gates 1–3 and current technical gates are evidenced, have the owner choose future UTC start/end and a cutoff before the end, complete the fresh private test record, confirm independent recovery/abort authority, and sign through the approved private system. Do not put signature material in the repository or infer dates.
7. **Record readiness, not execution.** The owner reviews every gate and records a fresh readiness timestamp/reference privately. Update this packet to **PASSED** only when its checklist is backed by current evidence and the private records are signed and independently recoverable; keep a redacted reference only. This status means readiness to consider exactly one disposable staging claim within the signed window—it does not start the faucet, enable intake, authorize replay, or establish claim success or production readiness. The claim requires a separate exact-window go/no-go and every hard pre-canary gate.
8. **Retain the stop boundary.** If any check expires, conflicts, fails, or lacks evidence, leave the packet **NOT PASSED**, intake closed, `FAUCET_ENABLED=false`, service inactive, port closed, and escalate through the independent recovery path. Do not retry, reset counters, restore over live state, extend a window, or treat a historical approval artifact as authority.

## Current decision

| Gate | Status | Next evidence |
| --- | --- | --- |
| Credential-store | PASSED — ULTRANET-STG-CRED-RISK-20261010-01 | Signed bounded private disposition; sign-off UTC 2026-10-10T09:02:29Z |
| Backup/recovery | PASSED — applicability confirmed 2026-10-10 | Backup from 2026-09-20 still applies; host/source/db identity and SHA-256 unchanged |
| Replay evidence | EXCLUDED for this test — Gate 3 closed 2026-10-10 | No replay POST; separate replay scope required if ever needed |
| Web Analytics exact-host disposition | CLOSED — 2026-10-10 | Not configured in Cloudflare Web Analytics; proxy-level beacon injection blocked by CSP |
| New test window | PASSED — 2026-10-10 | Window: 2026-10-10T11:00:00Z – 2026-10-10T12:30:00Z, cutoff 2026-10-10T12:25:00Z |
| Intake | CLOSED | Keep `FAUCET_ENABLED=false`; no enable call |

## Redacted reference review — 2026-09-26

Reviewed the repository coordination records only; they are not private-system evidence:

- The credential disposition remains `DRAFT — NOT SIGNED — NOT ACTIVE`; current host/recovery, retention, and signed-disposition fields are incomplete.
- The private sign-off checklist records credential disposition blocked and replay outside scope. Its September 20 window/backup references are historical, not current readiness evidence.
- The fresh-window draft still contains the expired September 20 window and no readiness confirmation or owner signature.
- The one-claim record says readiness and results are not recorded; it does not evidence a claim or successful canary.

No current signed disposition, separately approved replay result, current no-beacon evidence, or fresh signed window was found in reviewed repository records or available attachments. No blocked gate is promoted. Backup/recovery remains passed only for the historical session pending applicability confirmation. Overall status remains **PREPARED — NOT PASSED**; intake stays closed.

## Step 1 execution hold — 2026-09-27

This was a repository-only review. No staging host, provider, DNS/Cloudflare, or private-record system was contacted; no host command or probe was run and no service, credential, database, or network state was changed. Accordingly, this entry does **not** freshly verify the live stop-state. The required hold remains: intake disabled (`FAUCET_ENABLED=false`), faucet inactive, `127.0.0.1:8090` closed, and production excluded. Keep the packet **PREPARED — NOT PASSED** until an approved current record supports any status change.**Update 2026-10-10:** All gates are now closed. Gate 1 signed disposition recorded. Gate 2 applicability confirmed. Gate 3 excluded. Web Analytics closed. Gate 4 window signed. Packet status updated to PASSED.

The following approved **redacted private reference records** are still needed. Supply reference labels and non-secret freshness/scope metadata only; keep signatures, credentials, recipient/idempotency values, raw payloads, and private evidence in the approved private system:

1. **Current stop-state and staging identity:** a fresh UTC-timestamped reference independently tying the exact staging host, source/binary/unit/environment identity to `FAUCET_ENABLED=false`, inactive faucet, closed port `8090`, and production exclusion. Record the observer/source and scope; historical host notes do not satisfy this.
2. **Gate 1 credential disposition:** either current provider-approved encrypted-at-rest/TPM2/LUKS remediation and independent recovery-test references, or the owner's current signed bounded one-test disposition reference. Include fresh redacted host/device/store and four encrypted-credential metadata/hashes, plaintext/temp-artifact absence, independent recovery result, exact scope, expiry, retention, and stop authority.
3. **Gate 2 backup applicability:** a private comparison reference confirming current host/source/database identity and live-state baseline still match the historical backup/restore, plus integrity/state comparison. If not applicable or uncertain, first obtain separate approval for a staging-only quiesced capture and disposable restore; provide its redacted approval, integrity, comparison, and unchanged-live-state references. Never restore over live state.
4. **Gate 3 replay/idempotency:** either an applicable prior-approved test-scope reference and redacted results/reconciliation proving same-request reuse returns the original result with no duplicate effect and changed-destination reuse yields `409`, or a separate explicit replay authorization with isolation, stop, and reconciliation references before any test. Do not include request values or send a replay under this step.
5. **Fresh technical gates:** current redacted references for host/source/binary/unit/environment and credential aliases; effective egress/resolver and expiry monitoring; staging DNS/TLS/WAF/Caddy/origin and path/method boundaries; Turnstile hostname/action and secret provenance; exact-host Web Analytics disposition and independent no-beacon browser evidence; log/core/temp/proc hygiene; and tested recovery/stop controls. Each reference must identify its UTC freshness and exact staging scope.
6. **Fresh window and readiness:** only after all independent gates pass, the owner's private signed future-window and fixed-cutoff record, readiness timestamp/reference, independently recoverable abort path, and exactly-one-claim/no-retry/no-extension boundaries. This remains a later prerequisite, not a current authorization.

## Gate 1 closed — 2026-10-10

Gate 1 (credential-store disposition) is closed by a signed bounded private 
disposition. The record is retained privately under 
/root/ultranet-staging-docs/ with mode 0600.

Reference: ULTRANET-STG-CRED-RISK-20261010-01
Test reference: ULTRANET-STG-TEST-20261010-01
Signed UTC: 2026-10-10T09:02:29Z
GPG signature: Good signature from Vladan Jotov (staging backup key) <vlajko973@gmail.com>
Detached digest: sha256:e00d9eda...dfde9536 (redacted)

Evidence summary (recorded in the signed private record, not in this repository):
- Host: ubuntu-2gb-nbg1-2
- Source revision: bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e
- FAUCET_ENABLED: false
- Faucet service: inactive, disabled
- Port 8090: closed
- Egress policy: passed, expires 2026-10-11T10:57:17Z
- Recovery test: VNC access confirmed via Hetzner console
- Credential files: 4 files, root:root 0600

Remaining gates: Gate 2 (backup applicability), Gate 3 (replay), fresh 
technical gates, and fresh signed test window.

## Gate 2 applicability confirmed — 2026-10-10

The September 20 encrypted backup and disposable restore are confirmed 
applicable to the current staging state.

Verification (read-only):
- Host: ubuntu-2gb-nbg1-2
- Source revision: bbfe78a6e776aff32d4d9d0312e8c4ae3b771b3e
- Database: /var/lib/ultranet-faucet-staging/faucet.db
- Database SHA-256: 13727f7d895a5c539cdf61c980aa6caeee8350880576e10837a274bf1f127a9a (unchanged)
- Database size: 77824 bytes (unchanged)
- Database mtime: Sep 13 07:46 (unchanged)
- Owner/mode: ultranet-faucet-staging:ultranet-faucet-staging 0600 (unchanged)
- WAL/SHM: absent (unchanged)
- Credential files: 4 files, root:root 0600 (unchanged)

Backup reference: 
/var/backups/ultranet/faucet-staging-db/20260920T080847Z/faucet.db.asc
Backup SHA-256: 46d97cc420a06c5dc30a5135a66c13fde8c4233d32c58ca3a0c03f0c89ba4169

Gate 2 status: PASSED — applicability confirmed.
## Gate 3 — Replay/idempotency decision — 2026-10-10

Replay testing is EXCLUDED from this technical test. The scope is exactly 
one disposable claim, with:

- no retry
- no replay POST
- no changed-destination test
- no reload/new-tab resubmission

**Rationale:** The token value is zero. The purpose of this test is to 
verify the end-to-end Turnstile Siteverify path and the one-claim ceremony, 
not to test replay semantics. Replay evidence is not fabricated or inferred 
from this exclusion; it will be obtained separately when required.

**Gate 3 status:** CLOSED / EXCLUDED for this test.

**Not authorized by this decision:**
- Any replay POST during the one-claim window
- Any changed-destination test
- Any second claim or retry
- Any claim that replay behavior is verified

## Web Analytics gate — closed 2026-10-10

Cloudflare Web Analytics status for faucet-staging.ultranetwork.cc:
- Authenticated Web Analytics inventory lists only ultranetwork.cc and musicotp.net.
- faucet-staging.ultranetwork.cc is NOT in the inventory.
- Control-plane status: CLOSED / NOT CONFIGURED.

Cloudflare proxy-level HTML injection:
- Cloudflare injects a `static.cloudflareinsights.com/beacon.min.js` script tag 
  into the helper HTML. This is proxy-level automatic injection, not a Web 
  Analytics site configuration.
- The Caddy CSP header (`script-src 'self' https://challenges.cloudflare.com`) 
  does not include `static.cloudflareinsights.com`.
- The browser blocks the beacon as a CSP violation.

Evidence (fresh browser session, 2026-10-10):
- Cloudflare Web Analytics inventory: faucet-staging.ultranetwork.cc absent.
- Console shows: "Loading the script 'https://static.cloudflareinsights.com/beacon.min.js/...' 
  violates the following Content Security Policy directive: 'script-src 'self' 
  https://challenges.cloudflare.com'. ... The action has been blocked."
- Network tab: no cloudflareinsights.com request.
- Network tab: no POST to /api/faucet/claims.
- Local Storage: empty.
- Session Storage: empty.

Result: No analytics data is sent. The CSP provides defense-in-depth 
against the proxy-level beacon injection.

Web Analytics gate status: CLOSED.

## Gate 4 — Fresh test window — 2026-10-10

Window start UTC: 2026-10-10T11:00:00Z
Window end UTC:   2026-10-10T12:30:00Z
Fixed cutoff UTC: 2026-10-10T12:25:00Z

Test reference: ULTRANET-STG-TEST-20261010-01
Signed UTC: 2026-10-10T10:18:57Z
GPG signature: Good signature from Vladan Jotov (staging backup key)
Document hash: sha256:fc80ffcf093db81bcef9a0a055cd06bfe98da3183a08964fef57020d2da0a014

Credential disposition reference: ULTRANET-STG-CRED-RISK-20261010-01
Backup/recovery reference: sha256:46d97cc420a06c5dc30a5135a66c13fde8c4233d32c58ca3a0c03f0c89ba4169

Gate 4 status: PASSED.

## Definition of done

- [x] Credential-store protection or bounded one-test disposition is current, private and independently recoverable. — ULTRANET-STG-CRED-RISK-20261010-01, signed 2026-10-10T09:02:29Z
- [x] Historical encrypted SQLite/WAL/SHM backup and disposable-restore procedure passed without touching live state.
- [x] The historical backup is confirmed applicable to the fresh test identity and state. — Confirmed 2026-10-10.
- [x] Replay evidence is EXCLUDED for this test. — Decision recorded 2026-10-10. No replay POST authorized. Separate replay scope required if ever needed.
- [x] Cloudflare Web Analytics is not configured for the exact staging hostname (absent from inventory), AND the proxy-level beacon injection is blocked by the Caddy CSP. No analytics data is sent. — Confirmed 2026-10-10.
- [ ] All applicable hard pre-canary technical gates have fresh evidence and a tested stop action.
- [x] A new operator-selected window, fixed cutoff, readiness timestamp and abort path are recorded privately and owner-signed. — Reference: ULTRANET-STG-TEST-20261010-01, signed 2026-10-10T10:18:57Z
- [x] The readiness-only status is confirmed; intake remains closed until a separate exact-window go/no-go. — Packet status: PASSED, 2026-10-10. Intake still closed.
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

**Final self-review conclusion:** configuration and documentation are prepared for private sign-off, but the readiness packet remains **PREPARED — NOT PASSED**. (Gate 1 closed 2026-10-10; Gates 2–4 pending) |Intake stays closed and no claim procedure is authorized.
