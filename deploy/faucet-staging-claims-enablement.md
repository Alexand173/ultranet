# Staging faucet public preflight and claims-enable gate

This is the handoff procedure for the disposable faucet on the separately provisioned staging VPS. It is intentionally split into a **read-only public preflight** and a later **one-claim enablement ceremony**.

Nothing in the public preflight enables intake or submits a claim. Do not run the enablement section until the public checks, real staging credentials, backup, and a separate written one-claim approval are complete.

## Current recorded state — 2026-09-03

- Host: `ubuntu-2gb-nbg1-2`, `2.28.61.71`; production origin remains `167.233.161.115`.
- Validator: `ultranet-staging.service`, loopback API `127.0.0.1:8081`, loopback P2P `/ip4/127.0.0.1/tcp/19000`.
- Faucet: `ultranet-faucet-staging.service`, loopback listener `127.0.0.1:8090`, active but boot-disabled and intake-disabled.
- Source/artifact: final deployed source revision `b0ea4bdb279ee9b158e018bb636316bd91fab1f6`; faucet artifact SHA-256 `bd7258d2d2d0d4b7d86c1025c6d15442d24bc66368a588f8bd2dad17cc739cd3`.
- Environment: `FAUCET_ENABLED=false`, staging-only state path and signer address, exact `faucet-staging.ultranetwork.cc` Turnstile hostname, no `ULTRANET_ADMIN_TOKEN`.
- State: zero queue depth, zero reserved/confirmed budget, zero claims, no observed OOM, and no production/node path access under the faucet service UID.
- Public route: **not provisioned**. `faucet-staging.ultranetwork.cc` remained unresolved during the follow-up DNS recheck at 17:22 UTC; no HTTPS or public-route request was sent. No `caddy-staging.service` or `/etc/caddy/Caddyfile.staging` exists on the VPS.
- Turnstile: the encrypted staging credential was rotated through the approved host-key alias and passed disabled startup projection checks. Provider Siteverify acceptance is not proven until a separately approved canary; no claim test has been run.

The current status is therefore **node/faucet local preflight and credential rotation passed; public route, provider acceptance, and claims enablement remain blocked**.

The follow-up VPS checks also confirmed NTP synchronization, four root-owned mode-`0600` encrypted credentials, loopback-only service listeners, inactive production services, and the same disabled public status. The reviewed [`ultranet-faucet-staging-egress.py`](./ultranet-faucet-staging-egress.py) helper, `IPAddressDeny=any` policy, generated Cloudflare/resolver allowlist, and [`ultranet-faucet-staging-egress-expiry.timer`](./ultranet-faucet-staging-egress-expiry.timer) are now applied on the VPS; the 2026-09-13 attestation passed effective-policy and unit checks. Egress freshness/effectiveness remains a hard pre-canary gate. The untracked `99-canary-override.conf` was captured with its exact hash/directives, removed under separate approval, and the effective unit was reverified without starting the faucet. The host credential-store warning and partial TPM capability are formally accepted for exactly one disposable staging canary only; TPM2/LUKS-backed protection remains required for production or long-lived staging.

The final local health check confirmed the validator active, the faucet inactive and disabled, `FAUCET_ENABLED=false`, port `8090` closed, loopback-only validator listeners on `127.0.0.1:8081` and `127.0.0.1:19000`, staging Caddy active, production Caddy inactive, Caddy validation passed, and `systemd-analyze verify` passed for all staging units. All four encrypted faucet credentials remain `root:root` mode `0600`; replacement-host encrypted storage and recovery evidence are still required before the credential-store gate can close.

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

Run this only after an operator has provisioned the separate public infrastructure. All checks in this phase are read-only.

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

Require strict certificate verification. Do not use `-k` or bypass hostname verification:

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

# Public read-only contract.
status_code "$BASE/api/faucet/status"                         # 200
status_code "$BASE/api/faucet/claims/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" # 404

# Edge/Caddy deny boundary. These must be 404, not 401/200.
status_code "$BASE/internal/status"                          # 404
status_code "$BASE/api/account/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" # 404
status_code "$BASE/api/transaction/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" # 404
status_code -X POST "$BASE/api/faucet/status"                # 404
status_code -X PUT "$BASE/api/faucet/claims"                 # 404
status_code "$BASE/api/faucet/unknown"                       # 404
```

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

1. Create or verify a separate Turnstile widget for exactly `faucet-staging.ultranetwork.cc` and action `faucet_claim`. Never reuse the production secret.
2. Encrypt the new secret with the staging host credential key and the unchanged alias `faucet-staging-turnstile.secret` into a temporary same-filesystem `.cred` path. Do not pass the secret as a command argument or place it in `faucet.env`, an environment variable, shell history, chat, logs, screenshots, or a repository file.
3. Confirm only the temporary encrypted file's name, owner/group, mode, and checksum. Keep the old encrypted credential in a root-only rollback location until disabled restart checks and the approved canary decision are complete.
4. Atomically rename the new encrypted file over `/etc/ultranet-faucet-staging/secrets/faucet-staging-turnstile.secret.cred`; preserve `root:root` and mode `0600`. Do not change the unit alias or environment alias.
5. The running process retains the old systemd projection. Restart only `ultranet-faucet-staging.service` while intake is disabled. `daemon-reload` is unnecessary unless the unit or mapping changed. If startup fails, atomically restore the old encrypted artifact and restart disabled.
6. Run the checks below, including `check-config`, disabled status, unauthenticated internal status, credential metadata, isolation, loopback listeners, OOM/core-dump state, and redacted logs. `check-config` proves configuration and alias wiring, not that Cloudflare accepts the new secret.
7. Only a separately approved one-claim canary with a fresh single-use Turnstile token can validate provider acceptance. If it fails, disable/stop intake and restore the prior encrypted artifact if required; never substitute production credentials.
8. Destroy the old encrypted rollback artifact and temporary operator materials only after the private rollback window ends, and record metadata without retaining plaintext.

Restart with `FAUCET_ENABLED=false` and rerun:

```bash
sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service
sudo systemctl restart ultranet-faucet-staging.service
sudo -u ultranet-faucet-staging /opt/ultranet-faucet-staging/ultranet-faucet check-config
```

Recheck all local gates from [faucet-staging-install-preflight.md](./faucet-staging-install-preflight.md): signer/address, node chain and six decimals, fee estimate, zero queue/budget, disabled public status, operator authentication, isolation, loopback listeners, `MemorySwapMax=0`, clock synchronization, core-dump/log hygiene, and zero OOM events.

## Phase C — backup and explicit claim approval

Do not enable intake until all of the following are present in the private change record:

- successful Phase A public checks;
- real staging Turnstile hostname/secret verification;
- staging signer/address and node account/fee verification;
- separate Caddy/WAF/rate-limit/origin policy evidence;
- effective systemd egress allowlist, resolver integrity, Cloudflare range freshness, expiry-monitor, and denied-destination negative-test evidence;
- encrypted staging SQLite/WAL backup and a tested disable/stop rollback path;
- exact binary checksum, source revision, unit revision, environment revision, and credential alias record;
- an explicit written approval for **exactly one disposable claim**.

Public status approval, node-only approval, or preview approval does not authorize a claim.

### Hard security gates required before the canary

The following edges were identified during the follow-up review. They are hard pre-canary gates. Their checked-in implementation is not authorization to apply policy or enable intake:

- **Outbound egress:** the staging service now has `IPAddressDeny=any` plus loopback allowances, with the reviewed `ultranet-faucet-staging-egress.py` helper generating a root-owned resolver/Cloudflare destination drop-in and expiring manifest. Require the helper's `--check --effective` result, fresh range/resolver approval, and the expiry monitor before a canary. Do not treat `RestrictAddressFamilies` as a destination allowlist; systemd still cannot enforce exact hostname/port egress, so this residual risk must be explicitly accepted or replaced with a dedicated local proxy. The deferred review-only installation block and architecture comparison are in [`faucet-staging-egress-review.md`](./faucet-staging-egress-review.md).
- **Proxy identity:** inspect the effective Caddy version and forwarded-header chain. Require strict trusted-proxy/XFF behavior and prove that forged `X-Forwarded-For`, `X-Real-IP`, `CF-Connecting-IP`, and related headers cannot change abuse accounting or the Turnstile `remoteip` value. Client-supplied forwarding headers must remain stripped upstream.
- **Path normalization:** test repeated slashes, dot segments, encoded separators, encoded `internal`, trailing-slash variants, and alternate-case paths at both Caddy and Actix. No normalized variant may reach a node or operator route. Keep `GET /api/faucet/claims` (without an ID) denied.
- **Host, origin, and transport:** verify exact Host/SNI routing, certificate coverage, no alternate-host route, no direct IPv4/IPv6 origin, no public Caddy admin endpoint, and separate staging Caddy certificate/state storage. If HTTP/3 or 0-RTT is enabled, test or disable it for the canary so retries cannot bypass the operator-egress restriction.
- **WAF and cache policy:** use staging-specific expressions, not production-host rules copied broadly. Verify edge method/path blocking, no-store cache bypass, IPv4/IPv6 coverage, read/write limits, and the temporary operator-egress block without logging request bodies or secrets.
- **Clock and replay:** require synchronized system time before Turnstile or idempotency testing. Use a fresh high-entropy idempotency key, keep it out of logs, and verify same-key replay returns the original claim while a changed destination returns `409`.
- **Crash and log hygiene:** verify effective faucet core dumps are disabled or protected from secret-bearing collection. Confirm request bodies, CAPTCHA tokens, cookies, authorization headers, idempotency keys, signed envelopes, and secrets are absent from journals, edge logs, crash reports, `/proc`-visible environments, and temporary files.
- **Backup consistency:** create a staging-only encrypted backup using a consistency-safe SQLite/WAL procedure, preserve hashes and metadata separately, and test the disabled/stop rollback before enabling intake.
- **Disablement monitoring:** verify the private operator kill switch, service stop path, queue/budget alerts, node/hash/nonce reconciliation, and immediate disablement. Any mismatch is a permanent stop for this run.
- **Live unit drift and credential at rest:** the untracked `99-canary-override.conf` was removed under separate approval and the effective egress guard was reverified without starting the faucet. The host credential-store media/TPM risk is formally accepted for exactly one disposable staging canary only; TPM2/LUKS-backed protection remains required for production or long-lived staging. The successful disabled restart and override removal do not authorize a canary by themselves.

### Comprehensive gate evidence matrix

Every row requires a private evidence record, an identified reviewer, and a tested stop action before the canary:

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
| Approval/operations | Separate approvals, verified operator egress, abort authority, canary window, cleanup/rollback owner | Missing approval or inability to disable without production access |

## Phase D — one-claim canary

Run only after the claim approval is recorded and the temporary Cloudflare operator-egress restriction is active. Keep the configured daily cap at the staging value of `3,030,000` base units and keep the minimum reserve unchanged.

1. Confirm the faucet is still `enabled=false`, queue/budget counters are zero, and the node is healthy.
2. Enable intake once through the protected staging operator procedure. Never paste the operator token into a command, log, or chat transcript.
3. Submit exactly one request to a fresh, approved, non-faucet destination using a fresh high-entropy `Idempotency-Key`. The browser/request supplies only the destination and CAPTCHA token; the server chooses amount, fee, nonce, timestamp, nullifier, gas fields, and signature.
4. Require `202 Accepted`, one claim ID, fixed amount `1,000,000`, display amount `1.000000`, and no private or signed material in the response.
5. Poll the claim until `confirmed`, or stop and disable intake if it becomes inconsistent or times out.
6. Reconcile exactly across:
   - public claim status and transaction hash;
   - node transaction status, fee, nonce, and source debit;
   - source account balance and recipient balance;
   - faucet SQLite claim/payout/budget records;
   - no duplicate debit or second payout.
7. Replay the same destination/CAPTCHA/idempotency key and require the original claim ID with no second debit.
8. Replay the same idempotency key with a different valid destination and require `409 Conflict`.
9. Disable intake immediately through the protected staging operator procedure. Confirm public status and operator status report disabled, zero active queue, and the expected confirmed budget.
10. Remove the temporary operator-egress exception only after reconciliation evidence is preserved.

Stop immediately and keep intake disabled for any duplicate payout, hash mismatch, nonce drift, unexpected fee, address mismatch, provider failure, leaked secret, public internal route, origin bypass, OOM event, or database inconsistency.

## Phase E — rollback and cleanup

If any gate fails:

1. Disable intake through the protected staging operator path.
2. Stop `ultranet-faucet-staging.service` and preserve the staging SQLite/WAL state.
3. Preserve only redacted logs, public response fields, checksums, status codes, and reconciliation evidence.
4. Restore `FAUCET_ENABLED=false` and verify the persisted service state is disabled before any restart.
5. Remove only the disposable staging state/credentials named in the approved rollback record.
6. Never restore over `/var/lib/ultranet`, switch to production credentials, or point the faucet at a remote node.

A successful canary still does not authorize production enablement. Production requires its own signer, backup, WAF, monitoring, firewall, and canary approval.

## Definition of Done

- [ ] Staging DNS resolves only through the approved public route and never to the production origin.
- [ ] Strict HTTPS/TLS, Cloudflare Full (strict), Caddy separation, WAF path/method rules, no-store policy, rate limits, and origin controls are externally verified without a claim.
- [ ] Local and public status report disabled intake, fixed amount, and six decimals; unsupported/internal routes, normalized variants, `OPTIONS`, and `TRACE` are blocked at the public boundary.
- [ ] Exact Host/SNI, IPv4/IPv6, HTTP/3/0-RTT behavior, Caddy admin isolation, and staging certificate/state separation are verified.
- [ ] Destination-level faucet egress is active and fresh, resolver/Cloudflare range provenance is recorded, the expiry monitor is installed, and strict proxy client-IP trust, NTP synchronization, core-dump protection, and secret-free logs/temp/proc surfaces are evidenced.
- [x] The real staging Turnstile secret replaced the disposable placeholder through an atomic encrypted-credential rotation; exact provider hostname/action acceptance remains a separate gate.
- [ ] A staging-only consistency-safe SQLite/WAL backup and rollback path is recorded; binary, unit, environment, and credential metadata are reproducible.
- [ ] A separate written approval authorizes exactly one disposable claim before any enable call.
- [ ] The single canary reconciles claim, node, balance, nonce, fee, source debit, recipient balance, and SQLite state exactly.
- [ ] Same-key replay produces no second payout; changed-destination replay returns `409`.
- [ ] All thirteen hard-gate categories have evidence, an identified reviewer, and a tested stop action.
- [ ] Intake is disabled immediately after the canary, with redacted evidence preserved.
- [ ] No production endpoint, credential, state, proxy, DNS, firewall, service, or funds are touched.
