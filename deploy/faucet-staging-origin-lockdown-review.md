# Staging dual-stack origin-lockdown and Cloudflare audit review

This is a **review-only checklist** for protecting the isolated staging faucet origin. It does not apply UFW, change DNS or Cloudflare, reload Caddy, restart services, rotate credentials, enable intake, or submit a claim.

## Scope and fixed identities

| Item | Approved value |
| --- | --- |
| Staging hostname | `faucet-staging.ultranetwork.cc` |
| Staging IPv4 origin | `2.28.61.71` |
| Staging IPv6 origin | `2a01:4f8:1c16:797e::1` |
| Production origin exclusion | `167.233.161.115` |
| Staging public upstream | `127.0.0.1:8090` |
| Staging node API | `127.0.0.1:8081` — loopback only |
| Staging Caddy admin | `127.0.0.1:2019` — loopback only |
| Public status | `GET /api/faucet/status` |
| Intake state | `FAUCET_ENABLED=false` |

The existing [`cloudflare-origin-lockdown.sh`](./cloudflare-origin-lockdown.sh) is production-specific. It hardcodes `api.ultranetwork.cc`, `faucet.ultranetwork.cc`, and origin `167.233.161.115`; it must **not** be run unchanged on the staging VPS. It also requires UFW to already be active. The current staging preflight observed UFW inactive, so an independent staging firewall design and recovery decision are required before any apply step.

## What this review proves

The origin-lockdown change must ensure:

- public HTTPS continues to work through the Cloudflare proxy;
- direct TCP 80/443 access to both staging origin address families is blocked after broad-rule removal;
- only current Cloudflare IPv4 and IPv6 ranges can reach origin web ports;
- SSH, P2P, loopback, Caddy admin, validator API, faucet API, state, and credentials are not accidentally exposed or removed;
- the staging route remains disabled-only and no claim or operator enable path is exercised;
- a timestamped firewall snapshot and an independent recovery path exist before applying changes.

This is not proof of exact outbound Turnstile containment, a valid Turnstile secret, claim economics, or authorization to run a canary. Those remain separate gates.

## 1. Identity and recovery gates

Before any firewall change, record the following from a root-authorized SSH session and preserve only non-secret metadata:

```bash
set -euo pipefail

test "$(hostnamectl --static)" = "<approved-staging-hostname>"
ip -4 -o addr show scope global
ip -6 -o addr show scope global
systemctl is-active caddy-staging.service
systemctl is-active caddy.service || true
ufw status verbose
ufw show raw
ss -ltnp
```

Require all of these conditions:

- the SSH host key and provider server identity match the separately provisioned staging VPS;
- global IPv4 is `2.28.61.71` and production IPv4 `167.233.161.115` is absent;
- global IPv6 is `2a01:4f8:1c16:797e::1`, if IPv6 is intentionally retained;
- an independent provider-console or second administrative recovery path is tested and available;
- SSH and P2P rules are identified and excluded from the web-lockdown change; ports 22 and 9000 must remain reachable through the approved path;
- UFW is active and configured to process IPv6 before applying any IPv6 web rules;
- `caddy-staging.service` is the only intended public web owner; production `caddy.service` remains inactive/disabled;
- `FAUCET_ENABLED=false`, no `/internal/enable` call is made, and no claim request is sent.

If UFW is inactive, stop. Do not enable it or invent rules as part of this review.

## 2. DNS and Cloudflare range preconditions

From at least two independent external networks, query A, AAAA, and CNAME records for the exact staging hostname. Also inspect the approved read-only Cloudflare dashboard/API view.

Require:

- the exact A record content is `2.28.61.71` and Proxy status is **Proxied**;
- external A/AAAA answers are Cloudflare anycast addresses, not `2.28.61.71`, `2a01:4f8:1c16:797e::1`, or `167.233.161.115`;
- no unapproved DNS-only duplicate, CNAME, alternate hostname, or direct-origin AAAA route exists;
- the production faucet DNS record is unchanged;
- resolver answers agree sufficiently across independent resolvers.

Fetch current Cloudflare ranges only from the official endpoints over verified HTTPS:

```text
https://www.cloudflare.com/ips-v4
https://www.cloudflare.com/ips-v6
```

Record the UTC retrieval time, source URLs, line counts, SHA-256 checksums, and the exact source revision used. Validate each list as ASCII, non-empty, correctly typed, global CIDRs with no default route, malformed entry, or unexpected family. Do not accept a stale copied list or a range file from an unapproved mirror.

The existing production script validates a minimum of 10 IPv4 and 5 IPv6 ranges. A staging-specific implementation should retain at least that safety floor, reject unexpectedly large/malformed lists, and fail closed if the fetched policy cannot be reviewed.

## 3. Staging-specific rule design review

The staging change must be a separate, parameterized, reviewed firewall procedure. Do not reuse production hostnames, production origin guards, or production rule IDs.

### Required allows

- Cloudflare IPv4 CIDRs → staging origin TCP 80 and 443;
- Cloudflare IPv6 CIDRs → staging origin TCP 80 and 443;
- existing approved SSH/P2P/maintenance rules;
- loopback and host-local traffic required by the OS.

### Required exclusions

The web policy must not expose or alter:

- `127.0.0.1:8081` validator API;
- `127.0.0.1:8090` faucet API directly;
- `127.0.0.1:2019` Caddy admin;
- `/root`, state directories, signer files, encrypted credentials, or service sockets;
- production services, production Caddy config, production firewall rules, or production origin `167.233.161.115`.

### Required rejects

After the broad-rule removal phase, the effective firewall must not permit:

- `0.0.0.0/0` or `::/0` to TCP 80/443;
- broad `Anywhere` IPv4 or IPv6 web rules;
- stale, private, default, duplicate, or unapproved Cloudflare ranges;
- an unintended provider-health-check or maintenance range;
- an IPv4-only policy when IPv6 web ingress remains enabled.

Record every rule with its address family, CIDR, destination port, action, rule ID, and source checksum. Confirm the generated IPv4 and IPv6 rule sets exactly match the reviewed range material.

## 4. Staged application sequence

This sequence is a change-review order, not an authorization to execute it.

### Stage A — dry run and snapshot

1. Confirm all identity, DNS, Cloudflare-range, UFW, recovery, and disabled-intake gates.
2. Run a staging-specific `--check`/dry-run that performs no firewall mutation.
3. Create a root-only timestamped snapshot containing UFW verbose/raw state, current IPv4/IPv6 addresses, active rules, listener metadata, and fetched range checksums.
4. Confirm an active SSH session plus a separate recovery path.

### Stage B — additive Cloudflare rules

1. Add reviewed Cloudflare IPv4 TCP 80/443 rules.
2. Add reviewed Cloudflare IPv6 TCP 80/443 rules.
3. Keep broad rules temporarily so public services do not disappear while the allowlist is tested.
4. Do not reload Caddy, restart the faucet, or enable intake as part of the firewall operation.
5. Verify public Cloudflare HTTPS and disabled status from independent IPv4 and IPv6 networks.

### Stage C — broad-rule removal

Only after Stage B passes and the recovery path is confirmed:

1. Remove broad IPv4 TCP 80/443 rules.
2. Remove broad IPv6 TCP 80/443 rules.
3. Verify no broad or default-route web rule remains in the effective firewall.
4. Verify public proxied HTTPS still succeeds over IPv4 and IPv6.
5. Verify direct origin access fails for both families:
   - `2.28.61.71:80` and `2.28.61.71:443`;
   - `[2a01:4f8:1c16:797e::1]:80` and `[2a01:4f8:1c16:797e::1]:443`.
6. Verify SSH/P2P access and loopback service behavior remain unchanged.

A direct-origin failure must be observed from an external non-Cloudflare path. A request to the public hostname is not a direct-origin test because it resolves to Cloudflare.

## 5. Dual-stack verification matrix

| Check | IPv4 requirement | IPv6 requirement |
| --- | --- | --- |
| Host identity | `2.28.61.71` present | `2a01:4f8:1c16:797e::1` intentionally present and protected |
| Public DNS | A answers are Cloudflare only | AAAA answers are Cloudflare only or intentionally absent; no origin bypass |
| TLS/SNI | exact staging hostname, verified chain | exact staging hostname, verified chain |
| HTTP redirect | Cloudflare 308/approved redirect | Cloudflare 308/approved redirect |
| Public status | `200`, disabled, fixed amount, six decimals | same |
| Internal/node paths | denied at edge/Caddy | denied at edge/Caddy |
| Public Caddy | reachable only through Cloudflare ranges | reachable only through Cloudflare ranges |
| Direct origin before broad removal | reachable only as a known temporary state | reachable only as a known temporary state |
| Direct origin after broad removal | connection refused/timeout from bypass path | connection refused/timeout from bypass path |
| Firewall rules | current Cloudflare v4 CIDRs only | current Cloudflare v6 CIDRs only |
| SSH/P2P | unchanged and reachable | unchanged where intentionally configured |

Record status codes, remote address family, certificate identity/dates/SANs, redacted headers, firewall rule metadata, and timestamps only. Never retain request bodies, CAPTCHA tokens, cookies, authorization headers, idempotency keys, signed envelopes, or credentials.

## 6. Rollback and stop conditions

Stop immediately and leave `FAUCET_ENABLED=false` if any of these occurs:

- host, provider identity, origin address, production exclusion, or recovery path mismatch;
- DNS answer is direct, DNS-only, split-horizon, unexpected AAAA, or not Cloudflare-only;
- current Cloudflare range retrieval or validation fails;
- any SSH/P2P/recovery path becomes uncertain;
- public Cloudflare HTTPS, status, or existing API fails after additive rules;
- direct origin remains reachable after broad-rule removal;
- any IPv4/IPv6 family is left overbroad or unintentionally blocked;
- a service reload/restart, credential exposure, claim, or production change is proposed as a workaround.

Use the protected recovery path to restore the timestamped UFW snapshot. Preserve the staging firewall snapshot, range checksums, service/unit revision, redacted probes, and exact failure. Do not switch to production credentials, state, DNS, Caddy, or origin as a recovery shortcut.

## TRACE: 405 at Cloudflare versus 404 at Caddy

The observed public request:

```text
TRACE /api/faucet/status → 405 Not Allowed
```

returned a Cloudflare-generated response (`CF-RAY: -`) and did not reach the staging Caddy/faucet route. The checked-in Caddy fallback would return `404` for unsupported methods that reach Caddy. These statuses mean different things:

- **404** hides route existence more effectively and matches the current staging runbook's exact expectation for unsupported public traffic.
- **405** clearly says the method is not allowed for a recognized HTTP resource and may expose an `Allow` header or other method metadata.
- A Cloudflare-generated `405` is still a safe rejection if it is proven not to reach Caddy/upstream, has no `Allow` or internal metadata leak, creates no side effect, and is covered by the staging WAF policy.

### Recommendation

Treat the current `405-at-edge` as **blocked but not an exact 404 pass**. Do not enable claims while the exact-status gate is unresolved. For staging, prefer an edge custom response/rule that returns `404` for `TRACE` and other unsupported methods when Cloudflare can do so without bypassing managed WAF, cache, rate limits, or logging controls.

If Cloudflare cannot safely produce the desired `404`, record an explicit policy decision of `405-at-edge / 404-at-Caddy`, verify that the edge response has no `Allow` header, confirm no Caddy/faucet log entry or upstream side effect, and update the runbook's expected result. Do not alter production rules as part of this staging review.

### Prepared staging-only custom response rule — not applied

If the zone plan supports WAF custom responses, prepare this rule in the Cloudflare dashboard or approved API for **only** `faucet-staging.ultranetwork.cc`:

```text
Expression:
(http.host eq "faucet-staging.ultranetwork.cc"
 and http.request.method eq "TRACE")

Action: Block
Custom response:
  status_code: 404
  content_type: text/plain
  content: Not found
```

This is a WAF Custom Rule, not a managed-rule override. Cloudflare documents the `Block` action with `action_parameters.response` fields for `status_code`, `content`, and `content_type`; custom response status codes must be in the 400–499 range, so `404` is valid. The custom response capability is plan-dependent (Pro and above per the current documentation). A terminating custom-rule block is evaluated before later managed WAF phases, so verify that the rule does not introduce a managed-WAF bypass for any other request.

References:

- [Cloudflare custom rules in the dashboard](https://developers.cloudflare.com/waf/custom-rules/create-dashboard/)
- [Cloudflare custom rules API](https://developers.cloudflare.com/waf/custom-rules/create-api/)
- [`http.host` field](https://developers.cloudflare.com/ruleset-engine/rules-language/fields/reference/http.host/)
- [`http.request.method` field](https://developers.cloudflare.com/ruleset-engine/rules-language/fields/reference/http.request.method/)

Before any live apply, verify the exact hostname, rule ID/order, plan entitlement, managed-WAF interaction, no `Allow` header, no origin/Caddy/faucet log entry, no upstream side effect, and no request-body/token logging. If the feature is unavailable, retain the documented `405-at-edge / 404-at-Caddy` fallback and do not mark the exact 404 gate passed. This repository change is a specification only; no Cloudflare rule was created or changed.

## 7. Cloudflare read-only configuration audit

Use only an approved read-only dashboard session or read-only API token. Record rule IDs, versions, timestamps, and redacted settings. Never copy API tokens, Turnstile secrets, raw request bodies, or private credentials into the evidence record.

### DNS

- Inspect `faucet-staging.ultranetwork.cc`.
- Confirm A content `2.28.61.71`, Proxy status **Proxied**, Auto TTL, and exact hostname.
- Confirm no unauthorized AAAA/CNAME, DNS-only duplicate, alternate hostname, or direct-origin record.
- Compare the production faucet record before/after; it must be unchanged.

### SSL/TLS and transport

- Confirm zone/host SSL mode **Full (strict)**.
- Confirm the active certificate covers `faucet-staging.ultranetwork.cc`.
- Confirm minimum TLS version and HTTP-to-HTTPS redirect/Automatic HTTPS Rewrites.
- Record the HTTP/3 and 0-RTT decision. If enabled for a later canary, test retry/replay behavior; otherwise disable or explicitly defer it.
- Defer HSTS until dual-stack HTTPS has been stable and independently verified.
- From separate IPv4 and IPv6 networks, verify normal certificate validation without `-k`, exact SNI/SAN, public status, and redirect behavior.

### Managed WAF and route/method policy

- Confirm the Cloudflare managed WAF ruleset is active for the staging host.
- Confirm no broad `Allow` or `Skip` rule bypasses managed WAF or the staging method/path block.
- Verify an exact-host expression that allows only:
  - `GET /api/faucet/status`;
  - `GET /api/faucet/claims/<id>`;
  - `POST /api/faucet/claims` for the later canary only.
- Verify edge blocking for `/internal/*`, node account/transaction paths, unknown paths, unsupported methods, collection-without-ID, normalization variants, and alternate-case paths.
- Record the explicit `TRACE` decision: preferred custom `404`, or approved `405-at-edge / 404-at-Caddy` behavior.
- Confirm no production hostname is included in a permissive staging expression.

### Cache and headers

- Confirm cache bypass/no-store for status, claim status, and claim admission paths.
- Confirm the origin `Cache-Control: no-store` header remains present.
- Confirm no dynamic faucet response is served from cache.
- Confirm security headers and the 16 KB request-body limit remain effective at Caddy.

### Rate limiting

- Confirm staging-specific POST claim and GET read limits, action, duration, counters, and bypass behavior.
- Test the policy conceptually against alternate methods and normalization variants; do not submit a claim during this audit.
- Keep staging limits separate from production and preserve the application's authoritative cooldown, subnet/IP, queue, reserve, idempotency, and daily-cap controls.

### Turnstile

- Confirm the staging widget is bound exactly to `faucet-staging.ultranetwork.cc` and action `faucet_claim`.
- Confirm the secret remains staging-only and is not placed in Cloudflare rules, logs, shell history, or the evidence record.
- Replacing the disposable placeholder is a separate encrypted systemd-credential rotation gate, not part of this audit.

### Origin protection and logs

- Confirm the origin policy allows web ingress only from current Cloudflare IPv4 and IPv6 ranges.
- Confirm direct origin tests fail from independent non-Cloudflare networks after broad-rule removal.
- Confirm Caddy admin, validator API, faucet API, state, signer, and credential paths remain non-public.
- Review redacted Cloudflare Security Events and origin/Caddy logs for the probes. Confirm no request body, CAPTCHA token, cookie, authorization header, idempotency key, signed envelope, or internal response was logged.
- Preserve only redacted screenshots/exports, rule IDs, range checksums, certificate identity, status codes, resolver answers, and timestamps.

## 8. Decision tree after review

- **Any identity, DNS, Cloudflare, IPv4, IPv6, recovery, WAF, route, or direct-origin check fails:** stop; keep intake disabled; do not remove broad rules or run a canary.
- **All public and dual-stack origin checks pass:** complete the real Turnstile credential rotation, effective faucet egress policy/expiry evidence, trusted-proxy identity tests, encrypted SQLite/WAL backup, and tested stop/disable rollback.
- **Only after all hard gates pass:** obtain separate written approval for exactly one disposable canary; enable once, submit exactly one approved claim, reconcile node/balance/nonce/fee/SQLite state, test same-key replay and changed-destination `409`, then disable intake immediately.

## Definition of Done

- [ ] A staging-specific dual-stack origin-lockdown review exists and rejects use of the production-hardcoded lockdown script on the staging VPS.
- [ ] IPv4/IPv6 DNS, range provenance, UFW rule coverage, staged removal, direct-origin tests, and rollback requirements are documented.
- [ ] The `405` edge versus `404` Caddy behavior has an explicit recommendation and acceptance condition.
- [ ] The Cloudflare audit covers DNS, Full (strict), WAF, path/method, cache, rate limits, Turnstile, origin controls, and privacy evidence.
- [ ] No firewall, DNS, Cloudflare, service, credential, production, or claim change was performed by this review.
