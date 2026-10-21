# Staging gated readiness packet

| Readiness packet | Value |
| --- | --- |
| Status | **PASSED — 2026-10-10 (HISTORICAL READINESS ONLY; INCIDENT OPEN; LIVE STATE NOT INDEPENDENTLY VERIFIED)**; no new canary authorized |
| Scope | Maintenance-only preparation for the isolated staging faucet |
| Current test reference | `ULTRANET-STG-TEST-20261010-01` (window expired; reported run interrupted; claim outcome unresolved) |
| Prior test reference | `ULTRANET-STG-TEST-20260920-01` (expired; do not use or extend) |
| Owner/operator | Vladan Jotov |
| Staging host | `ubuntu-2gb-nbg1-2` |
| Staging hostname | `faucet-staging.ultranetwork.cc` |
| Faucet unit | `ultranet-faucet-staging.service` |

This packet prepares the remaining private gates. It does not enable intake, start the faucet, submit a claim, call Siteverify, run replay POSTs, mutate credentials, create a backup, change Cloudflare state, or extend the expired test window.

## Safety boundary

The previous window, `2026-09-18T15:00:00Z`–`2026-09-18T17:00:00Z`, is expired. It must not be extended automatically. A new operator-selected UTC window and a fresh readiness confirmation are required before any bounded disabled validation or one-claim procedure.

These are required safe-state invariants, not a continuing guarantee. The latest time-bounded read-only observation is recorded under **Live stop-state verification — 2026-10-21T16:36:56Z** below; it does not replace fresh verification before any separately authorized procedure.

```text
FAUCET_ENABLED=false
ultranet-faucet-staging.service: inactive
127.0.0.1:8090: closed
production origin 167.233.161.115: excluded
production services, credentials, state, DNS, Caddy and firewall: untouched
```
## Gate 1 — staging risk disposition — 2026-10-17

- **Reference:** ULTRANET-STG-CRED-RISK-20261017-01
- **Test reference:** ULTRANET-STG-TEST-20261017-01
- **Status:** OWNER-SIGNED — NOT INDEPENDENTLY REVIEWED — ONE TEST ONLY
- **Created UTC:** 2026-10-17T09:15:34Z
- **Expires UTC:** 2026-10-18T09:15:34Z
- **Recovery:** Hetzner VNC, tested PASS, 2026-10-10T09:02:29Z
- **Independent reviewer:** NOT ASSIGNED (technical test)
- **Note:** Gate 1 closure does not authorize a test. A separate exact-window go/no-go is required.

Document hash: sha256:45401bbfb5669387155603ef1907ba86b6ca289506a8ecc16b2080e05deadd2f
GPG signature: Good signature from Vladan Jotov
Do not record wallet passwords, recovery phrases, Turnstile secrets, operator tokens, idempotency keys, claim recipients, signed envelopes, private keys, or raw request bodies here.

## Gate 1 disposition expiry review — 2026-10-21

The recorded one-test disposition above expired at `2026-10-18T09:15:34Z`. It is historical evidence only and does not provide current risk acceptance or authority for any test. This repository-only note does not revalidate the private record, its signature, or current host/recovery evidence. No test, service action, or host-state claim is authorized or inferred.

Any future test requires fresh Gate 1 evidence and a new unique, exact-scope private disposition completed through the approved private record process, with expiry no later than the separately approved test cutoff. Keep the incident hold and all other gates in force.

## Gate 1 — credential-store protection and disposition

**Current historical finding, not current acceptance:** the host credential store was recorded as `/var/lib/systemd/credential.secret`, `root:root`, mode `0400`, on unencrypted `/dev/sda1` ext4. `systemd-creds has-tpm2` reported partial capability with no usable TPM-backed protection. The old risk-acceptance wording and withdrawn approval record are not evidence for a new window.

### Fresh unsigned staging review checklist — 2026-10-21

**Status: PREPARED — UNSIGNED — NO CURRENT GATE 1 AUTHORITY.** This is a repository checklist only. It does not authorize host/provider commands, SSH access, service or credential changes, a claim, a replay, Siteverify, or a test. Complete the evidence gathering only in a separately approved staging maintenance session. Keep detailed evidence and all private references in the approved private record system; place only redacted status/references in this repository. Never record credential contents, signatures, private keys, recovery phrases, operator tokens, recipients, idempotency keys, or raw requests here.

#### A. Scope and stop-state preconditions

- [ ] Confirm the separately approved maintenance authorization and evidence destination before connecting. This checklist itself grants no access or operational authority.
- [ ] Verify the trusted SSH host key; confirm the destination is exactly staging `2.28.61.71` / `ubuntu-2gb-nbg1-2`, not production. Stop on an unknown key, mismatch, or uncertain environment.
- [ ] Record current UTC time, hostname, source revision, assigned-address summary, observer, and evidence reference. Redact UUIDs, PIDs, nonessential addresses, and unrelated host details from any shared summary.
- [ ] Preserve the incident hold. Do not enable intake, start/restart a service, submit a claim, send a replay, call Siteverify, mutate credentials, change provider/network state, or contact production.

#### B. Fresh read-only host and credential-store snapshot

- [ ] Record `lsblk -f`, root filesystem, credential-store device/filesystem, owner/group/mode, and `systemd-creds has-tpm2` result; do not infer encrypted-at-rest or TPM protection from file permissions alone.
- [ ] Record metadata only for the four staging `*.cred` files (filename/alias, owner, group, mode, size, and SHA-256). Never display, decrypt, copy, or include credential contents.
- [ ] Confirm protected plaintext and temporary credential artifacts are absent; record only the redacted result and evidence reference.
- [ ] Record the exact staging unit, environment-file identity, credential aliases, and current service state without exposing values.
- [ ] Record listener bind addresses, ports, and owning service/process role. Interpret `127.0.0.1:8090` as the faucet listener: it must be closed when the faucet is required inactive. `127.0.0.1:8081` is the staging node API and may be listening if the node runs; identify its owner and do not conflate it with the faucet stop gate.
- [ ] Confirm no production credential/path overlap. Stop on any overlap, secret exposure, stale evidence, or unexplained state.

#### C. Independent recovery check

- [ ] Confirm Hetzner console access independently of the current SSH session.
- [ ] From a trusted client, verify authorized staging SSH access with the host key checked and only a harmless identity command. Do not reveal, transmit, or display a private key.
- [ ] Record pass/fail, UTC time, operator, and private evidence reference for both paths. If either is unavailable or not independent, do not sign a disposition.

#### D. Private disposition preparation (leave unsigned here)

- [ ] In the approved private record system, confirm the proposed reference and destination are unique before creating anything; do not overwrite prior records or reuse expired October references.
- [ ] Record fresh host/storage findings; actual owner and role; exact host, unit, hostname, revision, credential aliases; recovery owner/path; retention/disposal owner; abort authority; and the exact stop action.
- [ ] If the owner elects a risk exception, state acceptance of residual credential-store risk for exactly one named future disposable staging test only. Exclude production, long-lived staging, any second test/claim, retries, replay, changed-destination tests, provider changes, and automatic extension.
- [ ] Do not invent a future test window, reference, signer, reviewer, evidence result, or expiry. With no separately approved future test cutoff, leave the disposition pending/unsigned. Any eventual expiry must be no later than the approved test cutoff/closeout.
- [ ] Freeze the exact final private record before owner signature. Verify the signing key fingerprint against a trusted pre-existing private record, then verify the signature and digest over those exact bytes. Keep the key, signature, and private artifact outside this repository and chat.
- [ ] If an independent reviewer is assigned, they separately verify independence, signer authority, signature/digest, exact scope, recovery, cutoff, and stop/retention terms. Otherwise record `Independent reviewer: NOT ASSIGNED` and `OWNER-SIGNED — NOT INDEPENDENTLY REVIEWED — ONE TEST ONLY`; do not imply independent review.

#### E. Stop and repository update boundaries

- [ ] Stop without signing on host-key/environment mismatch, production overlap, secret exposure, uncertain or stale evidence, failed independent recovery, or inability to verify the exact stop action.
- [ ] Do not change the current Gate 1 status from `EXPIRED — HISTORICAL OWNER SIGNATURE; NOT CURRENT AUTHORITY` based on this checklist. A future redacted append-only packet update requires a separately authorized, completed private record and must not include full hashes, signature bytes, private documents, storage UUIDs, or secrets.
- [ ] Keep Gate 2 applicability, Gate 3 replay policy, technical pre-canary gates, incident reconciliation, and the fresh signed-window/go-no-go as independent blockers. Closing Gate 1 alone never authorizes a test.

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
| Credential-store | EXPIRED — HISTORICAL OWNER SIGNATURE; NOT CURRENT AUTHORITY | Historical reference: ULTRANET-STG-CRED-RISK-20261017-01; recorded expiry 2026-10-18T09:15:34Z. Fresh Gate 1 evidence and a new exact-scope private disposition are required before any separately approved future test. |
| Backup/recovery | PASSED — applicability confirmed 2026-10-10 | Backup from 2026-09-20 still applies; host/source/db identity and SHA-256 unchanged |
| Replay evidence | EXCLUDED for this test — Gate 3 closed 2026-10-10 | No replay POST; separate replay scope required if ever needed |
| Web Analytics exact-host disposition | CLOSED — 2026-10-10 | Not configured in Cloudflare Web Analytics; proxy-level beacon injection blocked by CSP |
| New test window | EXPIRED / EXECUTION INTERRUPTED | Window: 2026-10-10T11:00:00Z – 2026-10-10T12:30:00Z, cutoff 2026-10-10T12:25:00Z; do not reuse or extend |
| Claim outcome | OWNER-REPORTED — Definite failure, no side effect (not independently verified) | Reference: RECON-20261010-01; owner review 2026-10-10T16:47:14Z; document hash sha256:04ec74e4...bc81 |
| Intake / live state | INCIDENT HOLD — no new claim authorized; stop-state verified at 2026-10-21T16:36:56Z (time-bounded) | Strict-key read-only SSH confirmed faucet inactive/disabled, `FAUCET_ENABLED=false`, port `8090` closed, and node API on loopback `8081`. This does not close reconciliation or authorize a claim. |

## Live stop-state verification — 2026-10-21T16:36:56Z

This time-bounded snapshot was collected through SSH with strict host-key checking and a hostname guard for the staging host. Commands were read-only; no service, file, credential, database, ledger, or network configuration was changed. No claim, replay, Siteverify request, or provider API call was made. This is an operational snapshot, not an independent reviewer attestation or a readiness/authorization record.

- **Host identity:** `ubuntu-2gb-nbg1-2`; expected staging IPv4 `2.28.61.71` present.
- **Faucet:** `ultranet-faucet-staging.service` inactive (`dead`) and disabled; persisted `FAUCET_ENABLED=false`.
- **Listeners:** no TCP listener on `8090`; `127.0.0.1:8081` is listening for the staging node API only.
- **Node-reported state:** `chain_length=1`, `mempool_size=0`, `total_transactions=0`. Chain length is recorded as returned; no separate block inspection is asserted here.
- **Account lookup:** redacted address `4781a663…558210`; balance `0` base units (`0.000000 ULTRA`), nonce `0`.

**Incident status remains OPEN / ON HOLD.** The October 10 queued-claim reconciliation remains owner-reported and not independently reviewed. Gate 1 remains expired; the fresh Gate 1 checklist remains unsigned. This snapshot neither closes the incident hold nor authorizes a claim or future test.

## Redacted reference review — 2026-09-26

Reviewed the repository coordination records only; they are not private-system evidence:

- The credential disposition remains `DRAFT — NOT SIGNED — NOT ACTIVE`; current host/recovery, retention, and signed-disposition fields are incomplete.
- The private sign-off checklist records credential disposition blocked and replay outside scope. Its September 20 window/backup references are historical, not current readiness evidence.
- The fresh-window draft still contains the expired September 20 window and no readiness confirmation or owner signature.
- The one-claim record says readiness and results are not recorded; it does not evidence a claim or successful canary.

**Historical repository review — 2026-09-26:** No current signed disposition, separately approved replay result, current no-beacon evidence, or fresh signed window was found at that time. The packet was then **PREPARED — NOT PASSED**; that status was superseded by the readiness-only decision recorded on 2026-10-10. Neither historical record establishes current live service state or closes the present incident hold.

## Step 1 execution hold — 2026-09-27

This was a repository-only review on 2026-09-27. No staging host, provider, DNS/Cloudflare, or private-record system was contacted; no host command or probe was run and no service, credential, database, or network state was changed. It did not verify live stop-state. The listed disabled/inactive/closed conditions were required hold conditions, not observations. The **PREPARED — NOT PASSED** statement was superseded by the readiness-only gate closure recorded on 2026-10-10. Later owner-supplied service observations in the incident summary report services active and ports listening; current live state remains unverified.

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

## Incident hold and queued-claim reconciliation — 2026-10-10

The packet's **PASSED — 2026-10-10** status records a historical readiness-only decision; it is not evidence that a claim completed or that the execution window remains valid. The signed window `2026-10-10T11:00:00Z`–`2026-10-10T12:30:00Z` (cutoff `2026-10-10T12:25:00Z`) is expired and must not be reused or extended.

Until reconciliation is complete:

- Keep intake closed. Do not submit another claim, retry, replay, change destination, reload/resubmit, restart, redeploy, reset counters, delete records, or restore over live state.
- The owner/operator must preserve the private claim reference, exact UTC timeline/cutoff, redacted helper/API response, queue/reservation/budget snapshot, node transaction/hash and mempool/chain state, fee/nonce/balance observations, relevant redacted logs, and database/ledger references in the approved private evidence system. Do not copy recipients, idempotency values, credentials, signatures, raw requests/responses, or signed material here.
- In the approved private process, correlate the API/helper event with the persisted claim and reservation, faucet/SQLite state, node mempool/chain transaction, hash, fee, nonce, balances, and redacted logs. Record whether there was admission, processing, a confirmed side effect, a definite failure, or unresolved uncertainty. The queue status alone cannot prove any of these.
- If any side effect or ledger state is uncertain, preserve the state and escalate through the independent recovery path. Do not retry to obtain a cleaner status.
- The owner must execute the existing private disable/abort procedure under its authorization and independently record `FAUCET_ENABLED=false`, `ultranet-faucet-staging.service` inactive, and `127.0.0.1:8090` closed. This packet and the readiness PASS do not themselves authorize a new service operation. The October 10 record made no such verification claim; the separate October 21 read-only snapshot above is time-bounded and grants no operational authority.
- Record only a redacted reconciliation reference and owner-reviewed outcome in the private system. Any later attempt requires a new future UTC window, fresh evidence, and separate exact-window go/no-go after the incident is resolved.

No remote host command, database inspection/mutation, claim/replay request, restart, or deployment was performed by this documentation update.

## Definition of done

- [x] Credential-store protection or bounded one-test disposition is current, private and independently recoverable. — ULTRANET-STG-CRED-RISK-20261010-01, signed 2026-10-10T09:02:29Z
- [x] Historical encrypted SQLite/WAL/SHM backup and disposable-restore procedure passed without touching live state.
- [x] The historical backup is confirmed applicable to the fresh test identity and state. — Confirmed 2026-10-10.
- [x] Replay evidence is EXCLUDED for this test. — Decision recorded 2026-10-10. No replay POST authorized. Separate replay scope required if ever needed.
- [x] Cloudflare Web Analytics is not configured for the exact staging hostname (absent from inventory), AND the proxy-level beacon injection is blocked by the Caddy CSP. No analytics data is sent. — Confirmed 2026-10-10.
- [x] Historical hard pre-canary technical preflight passed at 2026-10-10T10:37:32Z: staging host, `FAUCET_ENABLED=false`, faucet service inactive/disabled, port 8090 closed, egress passed, and four credential files `root:root 0600`. This predates the incident and later service restarts; it is not current live-state verification.
- [x] Historical operator-selected window, fixed cutoff, readiness timestamp, and abort path were recorded and owner-signed. — Reference: `ULTRANET-STG-TEST-20261010-01`, signed 2026-10-10T10:18:57Z; the window is expired and must not be reused.
- [x] Historical readiness-only status was recorded as PASSED on 2026-10-10. This did not authorize claim execution; intake remains on incident hold, and later owner-supplied observations report services active and ports listening.
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

**Historical final self-review conclusion (superseded):** This earlier note records configuration and documentation as prepared for private sign-off and the packet as **PREPARED — NOT PASSED**. The current packet summary above supersedes that readiness state with a historical readiness-only **PASSED** status; neither status closes the current incident hold or authorizes a claim.

## Owner-reported reconciliation summary — 2026-10-10 (independent review pending)

**Reference:** `RECON-20261010-01`
**Test reference:** `ULTRANET-STG-TEST-20261010-01`
**Evidence collection UTC:** `2026-10-10T13:37:00Z` (owner-supplied)
**Owner review UTC:** `2026-10-10T16:47:14Z` (owner-supplied)
**Private document:** `reconciliation-outcome-20261010.md` (not available for this repository review)
**Document hash:** sha256:04ec74e4af5ff3f41e074a98908f8d4ceea7f3a51d0fba43753b5bac1a14bc81
**GPG signature:** Good signature from Vladan Jotov (staging backup key) — verified locally
**Independent reviewer:** NOT ASSIGNED (technical test)

**Evidence summary:**
- Claim status: queued
- Worker attempt_count: 0
- No transaction hash, no node transaction, no on-chain effect, no funds moved
- Faucet balance: 0 $ULTRA
- FAUCET_ENABLED=false (verified 2026-10-10T16:47:34Z)

**Stop-state timeline:**
- 2026-10-10T11:52:05Z: faucet service stopped (systemd journal)
- 2026-10-10T14:45:10Z: services restarted by owner for diagnostics
- 2026-10-10T16:47:34Z: both services active, ports 8090/8081 listening, FAUCET_ENABLED=false, faucet balance 0 $ULTRA

**Conclusion:**
- No financial side effect observed in available evidence
- Classification: OWNER-REPORTED — NOT INDEPENDENTLY VERIFIED
- Incident remains OPEN until independent review

**Remaining blockers:**
- Independent review not assigned
- Node fix d9476e4 not deployed (binary contains fix; service not confirmed)
- Fresh signed window required
- Faucet funding required (0 $ULTRA)
