# Staging Cloudflare policy and verification

This document defines the Cloudflare-side policy for `faucet-staging.ultranetwork.cc`. It is staging-only and does not authorize live dashboard/API changes, firewall changes, faucet enablement, or claims.

## Fixed identities

| Item | Value |
| --- | --- |
| Staging hostname | `faucet-staging.ultranetwork.cc` |
| Staging origin IPv4 | `2.28.61.71` |
| Staging origin IPv6 | `2a01:4f8:1c16:797e::1` |
| Production origin exclusion | `167.233.161.115` |
| Faucet upstream | `127.0.0.1:8090` |
| Validator API | `127.0.0.1:8081`, loopback-only |
| Caddy admin | `127.0.0.1:2019`, loopback-only |
| Turnstile hostname | `faucet-staging.ultranetwork.cc` |
| Turnstile action | `faucet_claim` |
| Intake state | `FAUCET_ENABLED=false` |

Never combine the staging hostname with the production hostname in a permissive expression. Never expose the staging origin directly as a DNS-only route.

### Web Analytics / Insights — explicit staging disablement

Cloudflare Web Analytics is not controlled by Caddy or the repository. For the proxied staging hostname, the operator must open **Web Analytics → Manage site → Advanced options** and set the automatic setup to **Disable**. Do not install a manual Web Analytics snippet in the helper or any staging page.

Required evidence before a browser gate or canary:

- the exact staging hostname's Web Analytics setting is visibly **Disabled** in an approved read-only dashboard view, or the exact staging hostname is absent from the authenticated Web Analytics sites inventory and is recorded as not configured;
- the helper source and served HTML contain no `static.cloudflareinsights.com` or `beacon.min.js` reference;
- the browser network panel shows no request to `static.cloudflareinsights.com` during page load, Turnstile rendering, input, or Reset;
- the origin response retains `Cache-Control: no-store, no-transform` as defense in depth against intermediary payload transformation.

The origin header and CSP are not proof that the Cloudflare control-plane setting is disabled. If the dashboard state cannot be verified, keep this gate blocked and do not add `static.cloudflareinsights.com` to the CSP. A blocked beacon request is evidence of an attempted injection, not a pass.

If an authenticated Web Analytics sites inventory lists the parent zone or other hostnames but does not list the exact `faucet-staging.ultranetwork.cc` hostname, record the exact staging site as **not configured / closed** rather than inferring its state from the parent hostname. Do not click **Add site** or **Update** as part of this read-only check. The no-beacon browser check remains required, and the parent hostname's analytics setting is a separate scope. Cloudflare Web Analytics automatic setup is **Disabled** only when the exact staging-host control-plane evidence visibly shows that state; an absent exact-host entry is recorded as **not configured / closed**, not as a false Disabled selection.

## Required Cloudflare configuration

### DNS

- Create exactly the approved staging A record pointing to `2.28.61.71`.
- Keep the record **Proxied** with Auto TTL.
- If IPv6 origin routing is intentionally retained, verify the staging AAAA policy separately; public AAAA answers must be Cloudflare anycast, never `2a01:4f8:1c16:797e::1`.
- Do not create a staging DNS-only duplicate, alternate CNAME, or bypass hostname.
- Confirm the production faucet record is unchanged.

### TLS and transport

Set Cloudflare SSL/TLS mode to **Full (strict)**. Do not use Flexible or ordinary Full mode.

Required evidence:

- Cloudflare dashboard/API confirms Full (strict) for the zone/host.
- Caddy has a valid origin certificate for `faucet-staging.ultranetwork.cc`.
- The certificate SAN covers the exact staging hostname.
- External IPv4 and IPv6 clients complete normal certificate validation without `-k`.
- HTTP redirects to HTTPS with the approved `308` behavior.
- HTTP/3 and 0-RTT have an explicit decision before a canary. If enabled, test retry/replay behavior; otherwise disable or defer them.
- HSTS remains deferred until dual-stack HTTPS is stable.

Example read-only checks:

```bash
HOST=faucet-staging.ultranetwork.cc

curl -4 --fail --silent --show-error \
  --connect-timeout 5 --max-time 15 \
  "https://$HOST/api/faucet/status"

curl -6 --fail --silent --show-error \
  --connect-timeout 5 --max-time 15 \
  "https://$HOST/api/faucet/status"

openssl s_client \
  -connect "$HOST:443" \
  -servername "$HOST" \
  -verify_hostname "$HOST" \
  -verify_return_error \
  </dev/null
```

Do not use these checks as a substitute for the Cloudflare Full (strict) dashboard/API evidence.

## Operational rule order

The following is the required review order. Cloudflare products execute in separate phases, so record the actual dashboard rule IDs and phase/order rather than assuming a single global list.

### 1. Staging TRACE custom response

Prepared rule, not applied by this repository:

```text
(http.host eq "faucet-staging.ultranetwork.cc"
 and http.request.method eq "TRACE")
```

Action:

```text
Block
Custom response status: 404
Content-Type: text/plain
Body: Not found
```

This must be a WAF Custom Rule scoped only to the staging hostname. It must not be a managed-rule override or a production rule. Cloudflare custom responses are plan-dependent; confirm the zone plan supports them before applying.

Required verification:

- exact host match only;
- rule is before any general staging method/path block in the custom-rule order;
- no `Allow` header;
- no Caddy/faucet log entry;
- no upstream side effect;
- no body, token, cookie, authorization header, or internal response in logs;
- managed WAF remains active for other requests.

If unsupported by the plan, document and approve the fallback `405-at-edge / 404-at-Caddy` behavior instead. Do not call the exact 404 gate passed in that case.

### 2. General staging path/method boundary

Use a separate staging custom WAF rule with this expression:

```text
(http.host eq "faucet-staging.ultranetwork.cc" and not (
  (http.request.method eq "GET" and http.request.uri.path eq "/api/faucet/status")
  or (http.request.method eq "GET" and starts_with(http.request.uri.path, "/api/faucet/claims/"))
  or (http.request.method eq "POST" and http.request.uri.path eq "/api/faucet/claims")
))
```

Action: `Block`.

This edge boundary must deny:

- `/internal/*`;
- `/api/account/*`;
- `/api/transaction/*`;
- unknown faucet paths;
- collection `/api/faucet/claims` without an ID for GET;
- POST status;
- PUT/PATCH/DELETE on faucet routes;
- OPTIONS and TRACE if not handled by the explicit TRACE rule;
- repeated-slash, dot-segment, encoded-separator, encoded-`internal`, trailing-slash, and alternate-case bypass variants.

Caddy remains a second boundary. Its final handler must return `404` and it must proxy only the approved faucet upstream.

### 3. Managed WAF

Keep the Cloudflare managed WAF ruleset active. Do not add a broad `Allow` or `Skip` for the staging host. A terminating custom-rule `Block` for TRACE or unsupported paths may prevent later managed rules from running for that request; verify this interaction in the audit and ensure it does not bypass inspection for allowed status/claim traffic.

Start new staging-specific rules in log/simulate mode where supported, review false positives, then enforce only after route tests pass. Do not copy production expressions that include the wrong hostname or origin assumptions.

### 4. Cache policy

Create a staging-specific cache bypass for dynamic faucet API traffic:

```text
(http.host eq "faucet-staging.ultranetwork.cc"
 and starts_with(http.request.uri.path, "/api/faucet/"))
```

The policy must not cache:

- faucet status;
- claim status;
- claim admission responses.

The origin must continue to send `Cache-Control: no-store, no-transform`. Verify `CF-Cache-Status` and redacted headers; no dynamic response may be served from cache. `no-transform` is defense in depth against payload rewriting and does not replace the Web Analytics **Disable** control-plane setting.

### 5. Rate limiting

Keep limits staging-specific and active for the later canary:

```text
http.host eq "faucet-staging.ultranetwork.cc"
and http.request.method eq "POST"
and http.request.uri.path eq "/api/faucet/claims"
```

Use the approved staging values from the private change record. Keep a separate read-polling rule for:

```text
GET /api/faucet/status
GET /api/faucet/claims/<id>
```

Cloudflare limits are an edge guard. The faucet remains authoritative for address cooldown, subnet/IP limits, queue length, reserve, idempotency, daily cap, and durable state.

### 6. Temporary canary restriction

Before a separately approved one-claim canary, use a staging-only block expression that permits only the approved operator egress address. Keep managed WAF, Turnstile, cache bypass, and ordinary rate limits active. Remove the temporary exception only after full reconciliation and immediate disablement.

No canary restriction is part of the current preflight or this document-only change.

## Full (strict) audit checklist

Use an approved read-only dashboard session or read-only API token. Preserve only rule IDs, timestamps, plan status, redacted settings, status codes, and certificate metadata.

- [ ] Staging A record content is `2.28.61.71` and Proxy status is Proxied.
- [ ] No direct staging origin A/AAAA/CNAME or DNS-only bypass exists.
- [ ] Production faucet DNS is unchanged.
- [ ] SSL/TLS mode is Full (strict).
- [ ] Certificate SAN covers `faucet-staging.ultranetwork.cc`.
- [ ] HTTP redirects to HTTPS.
- [ ] IPv4 and IPv6 TLS/status checks pass from independent networks.
- [ ] HTTP/3 and 0-RTT have an explicit decision.
- [ ] TRACE custom response is supported, correctly ordered, or fallback is explicitly accepted.
- [ ] General staging path/method block is exact-host scoped.
- [ ] Managed WAF is active without broad Allow/Skip bypass.
- [x] Web Analytics exact-host disposition is **CLOSED / NOT CONFIGURED** because the authenticated sites inventory contains no `faucet-staging.ultranetwork.cc` entry.
- [ ] Independent browser verification observes no request to `static.cloudflareinsights.com` or `beacon.min.js` during page load, Turnstile rendering, input, or Reset.
- [ ] Cache bypass and origin `no-store, no-transform` are verified.
- [ ] Staging POST/read rate limits are active and have no normalization bypass.
- [ ] Turnstile hostname/action are exact and the secret remains staging-only.
- [ ] Cloudflare Security Events and Caddy logs contain no request bodies or secrets.

## Origin firewall dependency

Cloudflare configuration is not sufficient by itself. The staging origin must also run the separate:

```bash
sudo bash deploy/faucet-staging-origin-lockdown.sh --check
```

The apply path must be performed only after:

- UFW is active with IPv6 enabled;
- SSH/P2P recovery is verified;
- current Cloudflare IPv4/IPv6 ranges are reviewed;
- additive rules pass public tests;
- broad-rule removal has private approval;
- direct origin tests fail over both address families;
- public Cloudflare HTTPS remains reachable.

## Stop conditions

Keep `FAUCET_ENABLED=false` and stop if:

- Full (strict) is not confirmed;
- any public DNS answer exposes an origin;
- any IPv4/IPv6 firewall family is overbroad or unverified;
- direct origin remains reachable after removal;
- a TRACE/custom-rule response reaches Caddy unexpectedly;
- a normalized or alternate path reaches the faucet/node/operator upstream;
- request bodies, tokens, cookies, authorization headers, idempotency keys, signed envelopes, or secrets appear in logs;
- any action would touch production DNS, Cloudflare rules, services, credentials, state, or funds.

A successful public route audit does not authorize a claim. The canary requires separate egress, backup, credential, economic reconciliation, and written approval gates.

## References

- [Cloudflare custom rules in the dashboard](https://developers.cloudflare.com/waf/custom-rules/create-dashboard/)
- [Cloudflare custom rules API](https://developers.cloudflare.com/waf/custom-rules/create-api/)
- [`http.host` field](https://developers.cloudflare.com/ruleset-engine/rules-language/fields/reference/http.host/)
- [`http.request.method` field](https://developers.cloudflare.com/ruleset-engine/rules-language/fields/reference/http.request.method/)
- [Cloudflare managed WAF](https://developers.cloudflare.com/waf/managed-rules/)
- [Cloudflare feature interoperability](https://developers.cloudflare.com/waf/feature-interoperability/)
- [`faucet-staging-origin-lockdown-review.md`](./faucet-staging-origin-lockdown-review.md)
- [`faucet-staging-install-preflight.md`](./faucet-staging-install-preflight.md)
