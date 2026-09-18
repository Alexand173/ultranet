# Staging faucet public preflight and claims-enable gate

This is the handoff procedure for the disposable faucet on the separately provisioned staging VPS. It is intentionally split into a **read-only public preflight** and a later **one-claim enablement ceremony**.

Nothing in the public preflight enables intake or submits a claim. Do not run the enablement section until the public checks, real staging credentials, backup, and operator readiness for the recorded one-claim development test are complete.

## Historical recorded state — 2026-09-03

The dated observations below are not fresh evidence for the September 18 development test. Later September 13 notes supersede the early service/public-route state; all technical gates still require revalidation. The [operator test record](../docs/runbooks/faucet-staging-one-claim-test-record.md) replaces the withdrawn, incorrect approval record. Owner/operator Vladan Jotov records readiness and technical evidence; no independent reviewer or signature is required for this development test. The fixed window is `2026-09-18T15:00:00Z`–`2026-09-18T17:00:00Z`; no automatic extension or inferred readiness is permitted.

For `ULTRANET-STG-TEST-20260918-01`, references below to an approved canary mean the operator-scoped development test after technical checks pass, not the withdrawn approval/signature. Historical risk-acceptance statements must be verified against valid operator evidence, never the withdrawn record. The owner reports a one-validator development network and zero current `$Ultra` value; this test does not establish multi-validator or production readiness.

- Host: `ubuntu-2gb-nbg1-2`, `2.28.61.71`; production origin remains `167.233.161.115`.
- Validator: `ultranet-staging.service`, loopback API `127.0.0.1:8081`, loopback P2P `/ip4/127.0.0.1/tcp/19000`.
- Faucet: `ultranet-faucet-staging.service`, loopback listener `127.0.0.1:8090`, active but boot-disabled and intake-disabled.
- Source/artifact: final deployed source revision `b0ea4bdb279ee9b158e018bb636316bd91fab1f6`; faucet artifact SHA-256 `bd7258d2d2d0d4b7d86c1025c6d15442d24bc66368a588f8bd2dad17cc739cd3`.
- Environment: `FAUCET_ENABLED=false`, staging-only state path and signer address, exact `faucet-staging.ultranetwork.cc` Turnstile hostname, no `ULTRANET_ADMIN_TOKEN`.
- State: zero queue depth, zero reserved/confirmed budget, zero claims, no observed OOM, and no production/node path access under the faucet service UID.
- Public route: **not provisioned**. `faucet-staging.ultranetwork.cc` remained unresolved during the follow-up DNS recheck at 17:22 UTC; no HTTPS or public-route request was sent. No `caddy-staging.service` or `/etc/caddy/Caddyfile.staging` exists on the VPS.
- Turnstile: the encrypted staging credential was rotated through the approved host-key alias and passed disabled startup projection checks. Provider Siteverify acceptance is not proven until a separately approved canary; no claim test has been run.

The historical preflight conclusion was **node/faucet local preflight and credential rotation passed; public route, provider acceptance, and claims enablement blocked**. Later notes describe public-route progress; they do not close all current execution gates.

The follow-up VPS checks also confirmed NTP synchronization, four root-owned mode-`0600` encrypted credentials, loopback-only service listeners, inactive production services, and the same disabled public status. The reviewed [`ultranet-faucet-staging-egress.py`](./ultranet-faucet-staging-egress.py) helper, `IPAddressDeny=any` policy, generated Cloudflare/resolver allowlist, and [`ultranet-faucet-staging-egress-expiry.timer`](./ultranet-faucet-staging-egress-expiry.timer) were recorded as applied on the VPS; the 2026-09-13 attestation passed effective-policy and unit checks. Egress freshness/effectiveness remains a hard pre-canary gate. The untracked `99-canary-override.conf` was captured with its exact hash/directives, removed under separate approval, and the effective unit was reverified without starting the faucet. The historical note asserted acceptance of the host credential-store warning and partial TPM capability for one canary. That assertion is not a current disposition: verify remediation or valid bounded private operator risk evidence without relying on the withdrawn approval. TPM2/LUKS-backed protection remains required for production or long-lived staging.

The historical final local health check confirmed the validator active, the faucet inactive and disabled, `FAUCET_ENABLED=false`, port `8090` closed, loopback-only validator listeners on `127.0.0.1:8081` and `127.0.0.1:19000`, staging Caddy active, production Caddy inactive, Caddy validation passed, and `systemd-analyze verify` passed for all staging units. All four encrypted faucet credentials were `root:root` mode `0600`. Fresh credential-protection/recovery evidence or a valid bounded private operator risk disposition remains required; this record neither requires a replacement host for the disposable test nor asserts acceptance of the existing host.

## Latest browser evidence — 2026-09-12

- The Turnstile Success screenshot proves the callback reached the helper and enabled the submit control, but it is not an approval to submit a claim.
- A prior browser Network screenshot shows one attempted `POST /api/faucet/claims` returning `403 Forbidden` from the Cloudflare edge. The visible recipient and idempotency value are intentionally not recorded. The rejection is fail-closed, with no evidence of `202 Accepted`, a claim ID, transaction hash, or payout; that session's no-claim gate remains a historical failure.
- A fresh controlled Reset verification reported zero `POST /api/faucet/claims` requests during Reset, Turnstile reset to `Verify you are human`, the submit button disabled, local storage empty, the faucet inactive, port `8090` closed, no recent restarts, and no recent faucet backend log entries.
- Record `turnstile_interactive_check=passed (controlled Reset procedure)` for the fresh run only. This does not authorize credential rotation, intake enablement, or the canary; `FAUCET_ENABLED=false` remains required.

## Safety invariants

Keep these invariants true in every section:

- Never use `faucet.ultranetwork.cc`, `api.ultranetwork.cc`, `167.233.161.115`, production credentials, production state, `sovereign_keys.json`, or `send_ultra.py` for staging.
- Never give `ultranet-faucet-staging` membership in the production `ultranet` group.
- Never place a signer key, Turnstile secret, abuse key, operator token, wallet password, recovery phrase, admin token, authorization header, or signed envelope in chat, source, ordinary logs, screenshots, backups beside the database, or shell history.
- Never call `/internal/enable` or `POST /api/faucet/claims` during the public preflight.
- Never reload production `caddy.service`, overwrite `/etc/caddy/Caddyfile`, alter production DNS/Cloudflare rules, or change production firewall state.
- If any guard fails, leave intake disabled and stop.

## Phase A — public non-claim preflight

Run this only after an operator has provisioned the separate public infrastructure. All probes in this phase are read-only; this phase never starts a service. Distinguish these states:

| State | Permitted evidence | Not established |
| --- | --- | --- |
| Faucet stopped, port 8090 closed | Static helper/TLS and denied-route checks, fresh host/listener attestation | Disabled application contract or Siteverify acceptance |
| Explicitly scoped bounded validation, faucet running with intake disabled | Local/public disabled JSON contract and upstream-dependent lookups | Claim authority or provider acceptance |
| Separately scoped one-claim window | Phase D admission, monitoring and reconciliation | Production readiness or another request |

A public upstream `502` is compatible with a stopped faucet only when fresh host/listener evidence corroborates it; it is not a passed disabled-status gate. Unexpected responses require investigation. Deferred application checks remain pending, not waived. Never start the faucet merely to replace a `502` with `200`.

### A1. DNS identity

From a controlled external network:

```bash
set -euo pipefail
STAGING_HOSTNAME=faucet-staging.ultranetwork.cc
PRODUCTION_IP=167.233.161.115

addresses="$(getent ahostsv4 "$STAGING_HOSTNAME" 2>/dev/null | awk '{print $1}' | sort -u)"
test -n "$addresses"
! grep -Fxq "$PRODUCTION_IP" <<<"$addresses"
printf '%s\n' "$addresses"
```

Verify in the Cloudflare dashboard or approved read-only API view that:

- the exact staging record is independently approved and proxied;
- it resolves to Cloudflare anycast addresses externally, not the VPS origin;
- no unapproved `AAAA` route or DNS-only bypass exists;
- the production faucet record is unchanged.

Do not create or edit DNS records in this procedure.

### A2. TLS and public status

Require strict certificate verification. Do not use `-k` or bypass hostname verification. With the faucet stopped, verify TLS using the static helper route; do not run the disabled JSON assertion below.

**The following application-contract check requires explicitly scoped bounded disabled-running validation.** Use [rotation Section 8](../docs/runbooks/faucet-staging-turnstile-rotation.md#8-disabled-validation-and-projection-check) for its guarded start/local-check/EXIT-stop lifecycle. Include this external check and A3's upstream lookups within that same bounded session, before its cleanup exits, with a private completion/failure signal and a fixed cutoff. Do not leave a service running while waiting indefinitely for an external operator. Verify stop independently on success, failure or cutoff. Missing validation scope or coordination leaves these gates pending; it does not permit an unguarded start.

During that validation only:

```bash
curl --fail --silent --show-error --connect-timeout 5 --max-time 15 \
  --output /tmp/staging-faucet-status.json \
  --dump-header /tmp/staging-faucet-status.headers \
  "https://$STAGING_HOSTNAME/api/faucet/status"

python3 -c '
import json, pathlib
value = json.loads(pathlib.Path("/tmp/staging-faucet-status.json").read_text())
data = value["data"]
assert value["success"] is True
assert data["enabled"] is False
assert data["availability"] == "disabled"
assert data["claim_amount_base_units"] == 1_000_000
assert data["claim_amount_ultra"] == "1.000000"
assert data["decimals"] == 6
print("public_status=disabled amount=1.000000 decimals=6")
'
grep -i '^cache-control:.*no-store' /tmp/staging-faucet-status.headers
rm -f /tmp/staging-faucet-status.json /tmp/staging-faucet-status.headers
```

Also verify the certificate covers `faucet-staging.ultranetwork.cc`, Cloudflare is using **Full (strict)**, and HTTP is redirected to HTTPS. Record only status codes, certificate identity, and redacted headers.

### A3. Public method/path boundary

These requests must not create a claim and must not reach the node or faucet operator routes:

```bash
set -euo pipefail
BASE="https://faucet-staging.ultranetwork.cc"

status_code() {
  curl --silent --show-error --connect-timeout 5 --max-time 15 \
    --output /dev/null --write-out '%{http_code}' "$@"
  printf '\n'
}

# Edge/Caddy deny boundary, including while the faucet is stopped.
# These must be 404, not 401/200/502.
status_code "$BASE/internal/status"                          # 404
status_code "$BASE/api/account/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" # 404
status_code "$BASE/api/transaction/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" # 404
status_code -X POST "$BASE/api/faucet/status"                # 404
status_code -X PUT "$BASE/api/faucet/claims"                 # 404
status_code "$BASE/api/faucet/unknown"                       # 404
```

Only during A2's explicitly scoped bounded disabled-running session, also require `GET /api/faucet/status` to return `200` with its disabled contract and `GET /api/faucet/claims/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` to return `404` for that nonexistent claim. These upstream-dependent checks are deferred while the faucet is stopped. A `502` is not a passed missing-claim check. Record and compare actual codes; comments in a command block are not assertions.

Do **not** send `POST /api/faucet/claims` as a disabled-state test. The public boundary is proven by the allowlist and unsupported-route checks; claim admission is reserved for the separately approved canary.

Also test the collection path without an ID, `OPTIONS` and `TRACE` on the public paths, and harmless normalization variants such as repeated slashes, dot segments, encoded separators, encoded `internal`, trailing slashes, and alternate-case paths. Denied paths must remain denied after normalization; no variant may reach a node or operator route. Record only status codes and redacted headers.

Confirm from Cloudflare/Caddy logs that no request body, CAPTCHA token, cookie, authorization header, idempotency key, signed envelope, or internal response was logged.

### A4. Separate Caddy and origin checks

On the staging VPS, inspect metadata and validate without reloading:

```bash
set -euo pipefail
STAGING_CADDY_SERVICE=caddy-staging.service

test "$STAGING_CADDY_SERVICE" = caddy-staging.service
test -f /etc/caddy/Caddyfile.staging
test "$(systemctl show "$STAGING_CADDY_SERVICE" -p FragmentPath --value)" != /etc/systemd/system/caddy.service
systemctl show "$STAGING_CADDY_SERVICE" -p FragmentPath -p ExecStart
caddy validate --config /etc/caddy/Caddyfile.staging --adapter caddyfile
```

Verify the effective staging unit:

- is `caddy-staging.service`, never `caddy.service`;
- loads only `/etc/caddy/Caddyfile.staging` or an explicitly reviewed equivalent;
- proxies only to `127.0.0.1:8090`;
- has the `16KB` body limit, `no-store`, and security headers;
- strips `Authorization`, cookies, CSRF, and client-supplied forwarding headers;
- has a final `404` fallback for `/internal/*`, node paths, unknown paths, and unsupported methods;
- does not publish `127.0.0.1:8081`, `/root`, state, signer, or credential paths.

Do not run `systemctl reload`, `systemctl restart`, or any production Caddy command in this phase. Confirm direct-origin access is blocked by the separately approved WAF/VPN/tunnel policy without changing that policy.

## Phase B — replace preflight credentials

Before any enablement approval, retain the rotated staging credential and complete provider-side verification for the separately provisioned staging widget:

- allowed hostname: `faucet-staging.ultranetwork.cc`;
- action: `faucet_claim`;
- secret: staging-only, never the production Turnstile secret.

Keep the four staging credentials separate and encrypted with systemd credentials:

```text
faucet-staging-signer.json
faucet-staging-turnstile.secret
faucet-staging-abuse.key
faucet-staging-operator.token
```

Use a root-only plaintext staging directory only during the encryption ceremony. Prefer a protected tmpfs or approved secret manager. After encryption, verify metadata only (`root:root`, mode `0600`), remove plaintext files using the host-approved secure cleanup procedure, and verify the service projection is readable by the service process without exposing contents. Secure deletion on SSD-backed storage is best-effort; do not treat `shred` as proof that prior plaintext blocks are unrecoverable.

For a rotation after the staging hostname is active, keep intake disabled and follow this fail-closed sequence:

1. Verify the existing staging-only Turnstile widget's allowed hostname `faucet-staging.ultranetwork.cc`. Verify `faucet_claim` separately in the helper render and server-side validation configuration, not as a presumed dashboard setting. Provider-side secret rotation is required; widget creation or a sitekey change needs separate approval. Never reuse the production secret.
2. Encrypt the new secret with the staging host credential key and the unchanged alias `faucet-staging-turnstile.secret` into a temporary same-filesystem `.cred` path. Do not pass the secret as a command argument or place it in `faucet.env`, an environment variable, shell history, chat, logs, screenshots, or a repository file.
3. Confirm only the temporary encrypted file's name, owner/group, mode, and checksum. Keep the old encrypted credential in a root-only rollback location until disabled restart checks and the approved canary decision are complete.
4. Atomically rename the new encrypted file over `/etc/ultranet-faucet-staging/secrets/faucet-staging-turnstile.secret.cred`; preserve `root:root` and mode `0600`. Do not change the unit alias or environment alias.
5. The running process retains the old systemd projection. Stop the faucet, then use only the explicitly authorized, bounded [disabled-validation procedure](../docs/runbooks/faucet-staging-turnstile-rotation.md#8-disabled-validation-and-projection-check), including failure cleanup and verified shutdown. `daemon-reload` is unnecessary unless a separately approved unit/mapping change occurred. Do not automatically restart after a failure.
6. Check `check-config`, disabled status, credential metadata, isolation, loopback listeners, OOM/core-dump state, and privately reviewed/redacted logs. `check-config` proves configuration and alias wiring, not that Cloudflare accepts the new secret.
7. Only a separately approved one-claim canary with a fresh single-use Turnstile token can validate provider acceptance. On failure, disable/stop intake and preserve state. An encrypted rollback file does not restore provider validity after revocation; never reactivate a compromised or revoked secret, retry the claim, or substitute production credentials.
8. Apply the approved retention/disposal decision to old encrypted artifacts only after the private rollback window ends. Clean protected plaintext and temporary operator materials on failure as well as success; retain only permitted metadata.

For post-canary work, follow [rotation and closeout](../docs/runbooks/faucet-staging-turnstile-rotation.md#13-post-canary-rotation-and-closeout), including actual provider-side rotation and old-secret invalidation tracking. Re-encryption alone does not revoke a Cloudflare secret.

Recheck all local gates from [faucet-staging-install-preflight.md](./faucet-staging-install-preflight.md): signer/address, node chain and six decimals, fee estimate, zero active queue/reservations, disabled status, operator authentication, isolation, loopback listeners, `MemorySwapMax=0`, clock synchronization, core-dump/log hygiene, and zero OOM events. Zero claims/confirmed budget is an unused-preflight baseline only; after a canary preserve its reconciled claim and expected confirmed budget with no new transactions or nonce changes. Never reset state to manufacture zero counters.

## Phase C — backup and operator test readiness

Do not enable intake until all of the following are present in the private change record:

- successful Phase A public checks;
- real staging Turnstile hostname/secret verification;
- staging signer/address and node account/fee verification;
- separate Caddy/WAF/rate-limit/origin policy evidence;
- effective systemd egress allowlist, resolver integrity, Cloudflare range freshness, expiry-monitor, and denied-destination negative-test evidence;
- encrypted staging SQLite/WAL backup and a tested disable/stop rollback path;
- exact binary checksum, source revision, unit revision, environment revision, and credential alias record;
- the operator's test reference, window, and readiness confirmation for **exactly one disposable claim**.

Public status checks, node-only checks, or preview checks do not establish claim readiness. Use the [staging one-claim test record](../docs/runbooks/faucet-staging-one-claim-test-record.md); Vladan Jotov records the technical checks and readiness before enablement. No independent-review attestation or signed approval record is required for this development test. A test record alone is not evidence that the gates passed.

### Hard security gates required before the canary

The following edges were identified during the follow-up review. They are hard pre-canary gates. Their checked-in implementation is not authorization to apply policy or enable intake:

- **Outbound egress:** the staging service now has `IPAddressDeny=any` plus loopback allowances, with the reviewed `ultranet-faucet-staging-egress.py` helper generating a root-owned resolver/Cloudflare destination drop-in and expiring manifest. Require the helper's `--check --effective` result, fresh range/resolver approval, and the expiry monitor before a canary. Do not treat `RestrictAddressFamilies` as a destination allowlist; systemd still cannot enforce exact hostname/port egress, so this residual risk must be explicitly accepted or replaced with a dedicated local proxy. The deferred review-only installation block and architecture comparison are in [`faucet-staging-egress-review.md`](./faucet-staging-egress-review.md).
- **Proxy identity:** inspect the effective Caddy version and forwarded-header chain. Require strict trusted-proxy/XFF behavior and prove that forged `X-Forwarded-For`, `X-Real-IP`, `CF-Connecting-IP`, and related headers cannot change abuse accounting or the Turnstile `remoteip` value. Client-supplied forwarding headers must remain stripped upstream.
- **Path normalization:** test repeated slashes, dot segments, encoded separators, encoded `internal`, trailing-slash variants, and alternate-case paths at both Caddy and Actix. No normalized variant may reach a node or operator route. Keep `GET /api/faucet/claims` (without an ID) denied.
- **Host, origin, and transport:** verify exact Host/SNI routing, certificate coverage, no alternate-host route, no direct IPv4/IPv6 origin, no public Caddy admin endpoint, and separate staging Caddy certificate/state storage. If HTTP/3 or 0-RTT is enabled, test or disable it for the canary so retries cannot bypass the operator-egress restriction.
- **WAF and cache policy:** use staging-specific expressions, not production-host rules copied broadly. Verify edge method/path blocking, no-store cache bypass, IPv4/IPv6 coverage, read/write limits, and the temporary operator-egress block without logging request bodies or secrets.
- **Clock and replay:** require synchronized system time, a fresh high-entropy idempotency key, and privately recorded evidence that same-key replay returns the original claim while a changed destination returns `409`. This evidence must come from prior approved tests or separately authorized testing; the September 18 single-request scope does not include extra replay POSTs. Missing evidence still blocks the canary.
- **Crash and log hygiene:** verify effective faucet core dumps are disabled or protected from secret-bearing collection. Confirm request bodies, CAPTCHA tokens, cookies, authorization headers, idempotency keys, signed envelopes, and secrets are absent from journals, edge logs, crash reports, `/proc`-visible environments, and temporary files.
- **Backup consistency:** create a staging-only encrypted backup using a consistency-safe SQLite/WAL procedure, preserve hashes and metadata separately, and test the disabled/stop rollback before enabling intake.
- **Disablement monitoring:** verify the private operator kill switch, service stop path, queue/budget alerts, node/hash/nonce reconciliation, and immediate disablement. Any mismatch is a permanent stop for this run.
- **Live unit drift and credential at rest:** the historical check recorded removal of `99-canary-override.conf` and reverified effective egress without starting the faucet. Recheck drift and require credential-store remediation or a valid bounded private operator risk disposition for this test; historical acceptance wording and the withdrawn approval are not evidence. TPM2/LUKS-backed protection remains required for production or long-lived staging. Disabled restart and override removal alone do not authorize a canary.

### Comprehensive gate evidence matrix

Every row requires a private evidence record checked by the operator, Vladan Jotov, and a tested stop action before the canary. No separate independent reviewer is assigned for this development test:

| Gate | Required evidence | Stop condition |
| --- | --- | --- |
| Supply chain/identity | Locked source, binary, unit, helper, drop-in, host, signer, and address identities | Checksum, host, or staging/production identity mismatch |
| DNS/resolver | Independent resolver agreement, no origin/AAAA/CNAME bypass, approved `/etc/resolv.conf` path | Split-horizon, poisoning indicator, direct origin, or unapproved resolver |
| TLS/transport | Exact SNI/SAN, Full (strict), HTTP redirect, IPv4/IPv6 parity, HTTP/3/0-RTT decision | Wrong certificate, downgrade, origin disclosure, or replay bypass |
| Edge proxy/WAF | Staging-only paths/methods, no-store, body limit, rate limits, trusted-proxy chain, Caddy admin isolation | Any normalized or alternate route reaches an upstream |
| Application contract | Content type, body size, malformed input, address, CAPTCHA, method, and route rejection checks | Unexpected status, leaked internal error, or protected route reachability |
| Client identity/abuse | Forged forwarding-header tests and edge/application limit reconciliation | Client can spoof IP/subnet identity or bypass abuse controls |
| Turnstile/credentials | Exact hostname/action, atomic encrypted rotation, provider lifecycle, no plaintext/proc/log exposure | Provider ambiguity, leaked secret, stale projection, or fail-open error |
| Process isolation/egress | Effective systemd properties, fresh manifest, current resolver/range checks, denied-destination test | Missing/stale/overbroad allowlist or production path access |
| Storage/recovery | Consistent encrypted SQLite/WAL/SHM backup, copy integrity, tested disable/restore | Inconsistent backup, wrong owner/mode, or production overlap |
| Economic/transaction | Amount, fee, reserve, cap, nonce, hash, source debit, recipient, budget, and SQLite reconciliation | Any mismatch, duplicate debit, or retry ambiguity |
| Availability/resources | Queue, attempts, timeouts, body/concurrency limits, cgroup ceilings, OOM/core-dump review | Unexpected backlog, OOM, restart loop, or node-failure drift |
| Privacy/observability | Redacted edge/journal/crash/proc/temp review and alerts for stale/mismatched state | Any request body, token, key, header, envelope, or secret exposure |
| Test operations | Operator test reference/readiness, verified operator egress, abort authority, test window, cleanup/rollback responsibility | Missing readiness/evidence, expired window, or inability to disable without production access |

## Phase D — one-claim canary

Run only after the operator's test readiness and all technical evidence are recorded, the test window is open, and the temporary Cloudflare operator-egress restriction is active. Keep the configured daily cap at the staging value of `3,030,000` base units and keep the minimum reserve unchanged.

Use one controlled helper session. Its attempt lock, the daily cap and the operator-egress restriction are not durable/server-side exactly-one admission enforcement. No reload/new-tab resubmission or retry after an HTTP rejection, malformed response, timeout or network error is included. Offline helper tests do not prove server replay behavior or live readiness.

`202 Accepted` is admission, not confirmation. Disabling prevents queued selection and new payout start (`src/faucet/store.rs`, `src/faucet/service.rs`), so do not disable on the normal path solely because admission succeeded. It cannot guarantee cancellation of an in-flight transaction; pending polling may continue while running disabled. `/internal/reconcile` invokes worker logic and can submit while enabled: it is not a read-only status check. The backend's existing bounded same-envelope submission behavior is distinct from an extra public claim POST and does not authorize extra manual worker runs. On failure, ambiguity, timeout or window end, disable/stop immediately and preserve unresolved state; never re-enable/restart just to drain the queue.

1. Confirm the faucet is still `enabled=false`, queue/budget counters are zero, and the node is healthy.
2. Enable intake once through the protected staging operator procedure. Never paste the operator token into a command, log, or chat transcript.
3. Submit exactly one request to a fresh, approved, non-faucet destination using a fresh high-entropy `Idempotency-Key`. The browser/request supplies only the destination and CAPTCHA token; the server chooses amount, fee, nonce, timestamp, nullifier, gas fields, and signature.
4. Require `202 Accepted`, one claim ID, fixed amount `1,000,000`, display amount `1.000000`, and no private or signed material in the response.
5. Poll the claim until `confirmed`, or disable intake and stop on inconsistency, timeout, or the approved window end—whichever comes first. At cutoff preserve unresolved state for reconciliation without another claim or unapproved restart.
6. Reconcile exactly across:
   - public claim status and transaction hash;
   - node transaction status, fee, nonce, and source debit;
   - source account balance and recipient balance;
   - faucet SQLite claim/payout/budget records;
   - no duplicate debit or second payout.
7. Do not issue additional replay POSTs or changed-destination tests under `ULTRANET-STG-TEST-20260918-01`. Same-key/no-second-debit and changed-destination/`409` evidence must come from previously approved tests or a separately authorized test scope. Missing mandatory replay evidence remains a blocker, not a waived or passed gate.
8. Disable intake immediately through the protected staging operator procedure. Confirm disabled runtime status, zero active queue/reservations, and the expected confirmed budget; assert persisted `FAUCET_ENABLED=false`, stop the faucet, and verify inactive state and closed port `8090`.
9. Immediately follow the [post-canary rotation and closeout procedure](../docs/runbooks/faucet-staging-turnstile-rotation.md#13-post-canary-rotation-and-closeout). No additional claim or Siteverify probe is authorized to validate the replacement secret.
10. Remove the temporary operator-egress exception only under its approved cleanup procedure after disablement and reconciliation evidence are preserved; do not broaden unrelated policy.

Stop immediately and keep intake disabled for any duplicate payout, hash mismatch, nonce drift, unexpected fee, address mismatch, provider failure, leaked secret, public internal route, origin bypass, OOM event, or database inconsistency.

## Phase E — rollback and cleanup

If any gate fails:

1. Disable intake through the protected staging operator path.
2. Stop `ultranet-faucet-staging.service` and preserve the staging SQLite/WAL state.
3. Preserve only redacted logs, public response fields, checksums, status codes, and reconciliation evidence.
4. Restore `FAUCET_ENABLED=false` and verify the persisted service state is disabled before any restart.
5. Preserve completed or uncertain claim state. Do not reset/restore SQLite, balances, or nonce to erase a claim, and never retry on an ambiguous result. Disposal applies only to artifacts explicitly named in a separately approved retention/cleanup record after reconciliation; never delete evidence to satisfy zero counters.
6. Never restore over `/var/lib/ultranet`, switch to production credentials, or point the faucet at a remote node. Credential rollback cannot undo provider revocation: unknown, revoked, or compromised secrets stay out of use. Secret-exposure containment follows the private incident procedure immediately while stopped, without waiting for financial reconciliation.

A successful canary still does not authorize production enablement. Production requires its own signer, backup, WAF, monitoring, firewall, and canary approval.

## Definition of Done

- [ ] Staging DNS resolves only through the approved public route and never to the production origin.
- [ ] Strict HTTPS/TLS, Cloudflare Full (strict), Caddy separation, WAF path/method rules, no-store policy, rate limits, and origin controls are externally verified without a claim.
- [ ] Local/public disabled-contract and missing-claim checks pass within explicitly scoped bounded disabled validation, followed by verified shutdown; stopped-upstream `502` is not a substitute. Unsupported/internal routes, normalized variants, `OPTIONS`, and `TRACE` are blocked at the public boundary.
- [ ] Exact Host/SNI, IPv4/IPv6, HTTP/3/0-RTT behavior, Caddy admin isolation, and staging certificate/state separation are verified.
- [ ] Destination-level faucet egress is active and fresh, resolver/Cloudflare range provenance is recorded, the expiry monitor is installed, and strict proxy client-IP trust, NTP synchronization, core-dump protection, and secret-free logs/temp/proc surfaces are evidenced.
- [x] The real staging Turnstile secret replaced the disposable placeholder through an atomic encrypted-credential rotation; exact provider hostname/action acceptance remains a separate gate.
- [ ] A staging-only consistency-safe SQLite/WAL backup and rollback path is recorded; binary, unit, environment, and credential metadata are reproducible.
- [ ] Operator test reference, window, and readiness for exactly one disposable claim are recorded before any enable call.
- [ ] The single canary reconciles claim, node, balance, nonce, fee, source debit, recipient balance, and SQLite state exactly.
- [ ] Prior approved or separately authorized replay evidence proves same-key replay has no second payout and changed-destination replay returns `409`; no replay POST is included in the September 18 single-request scope.
- [ ] Post-canary provider rotation/invalidation, protected local replacement, baseline-preserving disabled validation, and final stopped-state evidence are recorded under the rotation runbook.
- [ ] All thirteen hard-gate categories have operator-checked evidence and a tested stop action.
- [ ] Intake is disabled immediately after the canary, with redacted evidence preserved.
- [ ] No production endpoint, credential, state, proxy, DNS, firewall, service, or funds are touched.
