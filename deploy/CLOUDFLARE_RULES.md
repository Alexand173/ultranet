# Cloudflare origin, WAF, and rate-limit policy

**Canonical faucet origin:** `https://faucet.ultranetwork.cc`
**Node API origin:** `https://api.ultranetwork.cc`
**Origin VPS:** `167.233.161.115`
**Faucet upstream:** `127.0.0.1:8090`
**Node upstream:** `127.0.0.1:8081`

This document is the dashboard-side companion to [`Caddyfile.example`](./Caddyfile.example) and [`cloudflare-origin-lockdown.sh`](./cloudflare-origin-lockdown.sh). It does not contain Cloudflare API tokens, Turnstile secrets, operator tokens, or private keys.

## Required order

1. Proxy both `api.ultranetwork.cc` and `faucet.ultranetwork.cc` A records through Cloudflare.
2. Confirm both names resolve only to Cloudflare anycast addresses.
3. Configure Cloudflare TLS as **Full (strict)**.
4. Confirm Caddy has valid certificates and public HTTPS works for the API, website, dashboard, and faucet status endpoint.
5. Run the firewall script in `--check` mode.
6. Apply Cloudflare-only TCP 80/443 rules while retaining the existing broad rules.
7. Repeat public HTTPS and direct-origin tests.
8. Remove broad TCP 80/443 rules only after the external checks pass and an SSH recovery path is available.

The script does not change Cloudflare DNS or dashboard rules. Use the Cloudflare dashboard or an approved API automation account to make those changes separately.

## DNS and TLS

Create or update these DNS records:

| Name | Type | Content | Proxy | TTL |
| --- | --- | --- | --- | --- |
| `api` | `A` | `167.233.161.115` | Proxied | Auto |
| `faucet` | `A` | `167.233.161.115` | Proxied | Auto |

Do not publish an `AAAA` record unless IPv6 routing to the VPS is intentionally configured. Do not leave `api.ultranetwork.cc` DNS-only while removing broad origin web access; that would break the existing API.

Cloudflare TLS settings:

- SSL/TLS mode: **Full (strict)**.
- HTTPS rewrites: enabled where appropriate.
- HSTS: enable only after HTTPS is confirmed stable for all existing hostnames.
- Universal SSL certificate must cover `ultranetwork.cc` and `*.ultranetwork.cc`.
- Do not install a self-signed certificate as a production workaround.

## Managed WAF

- Enable the Cloudflare managed WAF ruleset for the zone.
- Start in log/simulate mode when introducing new custom rules.
- Review false positives against the API, dashboard, website, and faucet status endpoint.
- Do not add a broad `Allow` or `Skip` rule for the faucet.
- Do not log request bodies, CAPTCHA tokens, idempotency keys, cookies, authorization headers, or signed envelopes.

The existing node API has multiple wallet, session, and operator routes. Do not replace its application authentication with a path-wide Cloudflare allow rule. `ULTRANET_ADMIN_TOKEN` remains an application credential and must never be placed in Cloudflare rules, browser code, or the faucet service.

## Faucet path and method block

Create a custom WAF rule scoped to the faucet hostname. The rule blocks every method/path combination except the public faucet contract:

```text
(http.host eq "faucet.ultranetwork.cc" and not (
  (http.request.method eq "GET" and http.request.uri.path eq "/api/faucet/status")
  or (http.request.method eq "GET" and starts_with(http.request.uri.path, "/api/faucet/claims/"))
  or (http.request.method eq "POST" and http.request.uri.path eq "/api/faucet/claims")
))
```

Action: **Block**.

This must block, at the edge:

```text
/internal/*
/api/account/*
/api/transaction/*
/api/faucet/unknown
POST /api/faucet/status
PUT/PATCH/DELETE on public faucet paths
```

The Caddy fallback remains a second boundary and returns `404` for the same unsupported traffic.

## Cache policy

Create a cache rule that bypasses cache for both dynamic API surfaces:

```text
(
  (http.host eq "api.ultranetwork.cc" and starts_with(http.request.uri.path, "/api/"))
  or (http.host eq "faucet.ultranetwork.cc" and starts_with(http.request.uri.path, "/api/faucet/"))
)
```

The faucet sends `Cache-Control: no-store`. Cloudflare must not cache faucet status, claim status, claim admission responses, node account data, or transaction responses.

## Faucet write rate limit

Create a rate-limiting rule with this expression:

```text
http.host eq "faucet.ultranetwork.cc"
and http.request.method eq "POST"
and http.request.uri.path eq "/api/faucet/claims"
```

Initial beta values:

| Setting | Value |
| --- | --- |
| Counting characteristic | Source IP |
| Requests per period | `3` |
| Period | `10 minutes` |
| Mitigation | Block |
| Mitigation duration | `30 minutes` |

Cloudflare rate limiting is an edge guard, not an exact accounting mechanism. The faucet application remains authoritative for address cooldown, subnet/IP controls, queue length, daily debit cap, reserve, idempotency, and durable claim state.

## Faucet read rate limit

Create a separate rule for polling:

```text
http.host eq "faucet.ultranetwork.cc"
and http.request.method eq "GET"
and (
  http.request.uri.path eq "/api/faucet/status"
  or starts_with(http.request.uri.path, "/api/faucet/claims/")
)
```

Initial values:

```text
60 requests per minute per source IP
10-minute mitigation
Block action
```

Tune only after observing legitimate polling behavior. Do not make claim polling expensive enough to hide a confirmed transaction from the user.

## Temporary canary restriction

Before the one-claim canary, create a temporary block rule:

```text
http.host eq "faucet.ultranetwork.cc"
and http.request.method eq "POST"
and http.request.uri.path eq "/api/faucet/claims"
and not ip.src in {OPERATOR_EGRESS_IP}
```

Replace `OPERATOR_EGRESS_IP` with the approved operator’s public egress address. Use **Block**, not `Skip` or `Allow`. Keep managed WAF, Turnstile, and the standard rate limit active.

Remove this rule only after the canary’s claim ID, transaction hash, nonce, fee, source debit, recipient balance, and database state have reconciled.

## Turnstile

Create the widget with:

```text
Allowed hostname: faucet.ultranetwork.cc
Action: faucet_claim
```

The browser may receive only the public site key. The server-side secret remains in the encrypted systemd credential `faucet-turnstile.secret`.

The Rust verifier accepts a Siteverify response only if all of these are true:

```text
success == true
hostname == "faucet.ultranetwork.cc"
action == "faucet_claim"
```

Missing or mismatched hostname/action is a rejected CAPTCHA. Provider transport failures and malformed responses remain temporary provider-unavailable errors. Turnstile tokens are short-lived and single-use; refresh the widget after expiry or rejection.

## Origin firewall

Cloudflare’s current ranges are fetched only from:

- `https://www.cloudflare.com/ips-v4`
- `https://www.cloudflare.com/ips-v6`

Run the repository procedure as an active SSH-connected root session:

```bash
sudo bash deploy/cloudflare-origin-lockdown.sh --check
sudo bash deploy/cloudflare-origin-lockdown.sh --apply
```

The first apply stages allow rules and retains broad access. After verifying all public services through Cloudflare and confirming a recovery path, remove broad web rules explicitly:

```bash
sudo env CLOUDFLARE_LOCKDOWN_CONFIRM=I_UNDERSTAND \
  bash deploy/cloudflare-origin-lockdown.sh --apply --remove-broad
```

The script validates that both DNS names resolve only to Cloudflare ranges, snapshots the downloaded lists and UFW state under `/var/backups/ultranet/cloudflare-origin-lockdown/`, preserves SSH/P2P rules, and refuses empty/malformed range lists. It never changes port 22 or port 9000.

After removal, verify from an external non-Cloudflare bypass path that direct requests to `167.233.161.115:80/443` fail while public Cloudflare HTTPS succeeds. If the existing API is not yet proxied, stop and restore the pre-change UFW state.

## Verification matrix

| Check | Expected result |
| --- | --- |
| `https://api.ultranetwork.cc/api/validate` | Successful node response through Cloudflare |
| `https://faucet.ultranetwork.cc/api/faucet/status` | `enabled=false`, six decimals, fixed claim amount |
| `https://faucet.ultranetwork.cc/internal/status` | `404` at Cloudflare/Caddy |
| `https://faucet.ultranetwork.cc/api/account/test` | `404` |
| `POST /api/faucet/status` | `404` |
| HTTP faucet request | `308` redirect to HTTPS |
| Direct origin after UFW removal | Connection blocked |
| Loopback `127.0.0.1:8090` | Service remains reachable locally |
| `ultranet-faucet.service` | Active, disabled intake, zero queue/budget |
| `ultranet.service` | Active |

No real claim is part of DNS, WAF, TLS, or UFW verification.
