# Staging token helper deployment review

This document covers the static, token-only helper at the repository root:

- `staging-token.html`
- `staging-token.css`
- `staging-token.js`

It does not authorize a claim, intake enablement, credential rotation, or a live Caddy restart. It is a deployment procedure for a separately reviewed staging maintenance window.

## Safety boundary

The helper:

- must be served only by `https://faucet-staging.ultranetwork.cc`;
- uses only the public staging Turnstile site key and action `faucet_claim`;
- keeps the Turnstile token and idempotency key in memory only;
- never displays, logs, stores, or sends an operator token;
- never calls `/internal/enable`, `/internal/disable`, `/internal/status`, or `/internal/reconcile`;
- submits only the public `POST /api/faucet/claims` contract;
- does not replace the private operator enable/disable, backup, reconciliation, or rollback procedure.

Do not serve this page from production, a DNS-only origin, an arbitrary local hostname, or a proxy that changes the staging hostname/SNI. The staging Turnstile widget must explicitly authorize the exact staging hostname.

## Reviewed Caddy route

The route is present only in [`Caddyfile.staging.example`](./Caddyfile.staging.example):

```caddyfile
@staging_token_helper path /staging-token.html /staging-token.css /staging-token.js
handle @staging_token_helper {
    root * /var/www/faucet-staging
    file_server
}
```

The matcher is exact-path scoped. It does not publish the node API, faucet listener, `/internal/*`, Caddy admin, signer, credential directory, or arbitrary filesystem paths. The existing API handlers and final `404` remain unchanged.

## Staging Caddy control plane

The staging template enables Caddy's admin API only on `127.0.0.1:2019`. The staging unit's `ExecReload` now targets this address explicitly (`--address 127.0.0.1:2019`): an earlier unqualified `caddy reload` resolved `localhost` to `[::1]:2019` and failed with `dial tcp [::1]:2019: connect: connection refused` against the IPv4-only listener. This deployment still uses a controlled `caddy-staging.service` restart for the first activation, since a first activation does not depend on an already-running admin endpoint. The endpoint must never bind to `0.0.0.0`, `[::]`, a public address, or the production Caddy service.

Do not change `/etc/caddy/Caddyfile`, `caddy.service`, or any production listener.

## Private maintenance deployment

Run only on the identified staging host after verifying the host fingerprint, source revision, DNS/Cloudflare approval, and a current rollback copy of `/etc/caddy/Caddyfile.staging`. Keep intake disabled throughout this step.

```bash
set -euo pipefail

HOST_EXPECTED='ubuntu-2gb-nbg1-2'
SOURCE_DIR='/opt/ultranet-staging-src'
STATIC_ROOT='/var/www/faucet-staging'

 test "$(hostnamectl --static)" = "$HOST_EXPECTED"
 test -d "$SOURCE_DIR"
 test -z "$(git -C "$SOURCE_DIR" status --porcelain --untracked-files=all)"
 grep -Eq '^FAUCET_ENABLED=false$' /etc/ultranet-faucet-staging/faucet.env

# Verify the source revision and artifact hashes from the private change record.
# Do not paste private credentials or Turnstile values into this shell.
git -C "$SOURCE_DIR" rev-parse HEAD
sha256sum \
  "$SOURCE_DIR/staging-token.html" \
  "$SOURCE_DIR/staging-token.css" \
  "$SOURCE_DIR/staging-token.js" \
  "$SOURCE_DIR/deploy/Caddyfile.staging.example"

# Keep the static tree readable by caddy but not writable by it.
sudo install -d -o root -g caddy -m 0750 "$STATIC_ROOT"
sudo install -o root -g caddy -m 0640 \
  "$SOURCE_DIR/staging-token.html" \
  "$SOURCE_DIR/staging-token.css" \
  "$SOURCE_DIR/staging-token.js" \
  "$STATIC_ROOT/"

# Review the live Caddyfile change before replacing it. Preserve a root-only
# rollback copy and never touch /etc/caddy/Caddyfile or caddy.service.
BACKUP_DIR="/var/backups/ultranet/faucet-staging-caddy/$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -o root -g root -m 0700 "$BACKUP_DIR"
sudo install -o root -g root -m 0600 \
  /etc/caddy/Caddyfile.staging \
  "$BACKUP_DIR/Caddyfile.staging.before"
# caddy-staging.service runs as User=caddy, Group=caddy, with no
# supplementary groups. Keep the config root-owned but readable by caddy.
sudo install -o root -g caddy -m 0640 \
  "$SOURCE_DIR/deploy/Caddyfile.staging.example" \
  /etc/caddy/Caddyfile.staging
sudo -u caddy test -r /etc/caddy/Caddyfile.staging

sudo caddy validate --config /etc/caddy/Caddyfile.staging --adapter caddyfile
sudo systemctl restart caddy-staging.service
sudo systemctl is-active --quiet caddy-staging.service

# The Caddy admin endpoint must be loopback-only (exact match, IPv4).
ADMIN_LISTENERS="$(ss -ltnH | awk '$4 ~ /:2019$/ {print $4}')"
test "$ADMIN_LISTENERS" = '127.0.0.1:2019'
```

The live Caddyfile should be updated through the operator's reviewed configuration workflow rather than copied blindly if it contains approved staging-only changes not present in the template. Compare the resulting file before restart. Restart only `caddy-staging.service`; never use `caddy.service` for this procedure.

## Read-only verification

From a controlled external network, with no claim submission:

```bash
HOST='faucet-staging.ultranetwork.cc'

curl --fail --silent --show-error \
  --connect-timeout 10 --max-time 20 \
  "https://$HOST/staging-token.html" \
  | grep -Fq 'UltraNet staging claim helper'

curl --fail --silent --show-error --head \
  "https://$HOST/staging-token.css"

curl --fail --silent --show-error --head \
  "https://$HOST/staging-token.js"

# The asset route must not expose arbitrary files or internal controls.
test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  "https://$HOST/internal/status")" = 404
test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  "https://$HOST/staging-token.html/extra")" = 404

# The public status contract must stay disabled during this verification.
curl --fail --silent --show-error --connect-timeout 10 --max-time 20 \
  "https://$HOST/api/faucet/status" \
  | grep -Fq '"enabled":false'
```

These checks were run from an independent network path against the live deployment: HTML/CSS/JS returned `200`, `/internal/status` and `/staging-token.html/extra` returned `404`, `/api/faucet/status` reported `enabled:false`, and the Caddy access log showed zero `POST /api/faucet/claims` requests. This confirms the route, isolation, and disabled-intake invariants without exercising Turnstile.

Do not use a local `127.0.0.1` preview as proof of Turnstile validity; the helper intentionally refuses token rendering outside the approved staging hostname.

## Remaining Turnstile verification

The automated checks above cannot exercise Cloudflare Turnstile. During this deployment, the automation network could not reach `https://challenges.cloudflare.com`, so the widget failed closed with "Turnstile could not load. Stop and check the staging host/network." That is the helper behaving correctly under a network that cannot reach the challenge endpoint, not a route or origin failure. The following interactive steps remain and must be completed from an independent browser/network before this helper is considered fully verified. Do not submit a claim during this procedure.

1. From a normal browser on a network that is not restricted from reaching Cloudflare, open only `https://faucet-staging.ultranetwork.cc/staging-token.html`.
2. Confirm the address bar shows the exact approved hostname over HTTPS, with no redirect to a different host.
3. Confirm the Turnstile widget renders (not the "could not load"/"did not load" error text) and that the status line does not report a host guard error.
4. Enter a non-sensitive syntactically valid test value only, such as 64 lowercase `0` characters, into the recipient field. Do not enter a real address; the page does not submit on input alone.
5. Complete the Turnstile challenge. Confirm the status line reports verification success and the submit button becomes enabled only once both the address and Turnstile checks pass.
6. Using the browser's network panel, confirm that page load, widget render, input typing, Turnstile completion, and pressing Reset produce no `POST` to `/api/faucet/claims`. Do not press "Submit one claim".
7. Confirm the Turnstile token never appears in the DOM text, the URL, `localStorage`/`sessionStorage`, or any console/log output.
8. Press Reset (or let the widget expire) and confirm the status line returns to the pre-verification state and the submit button becomes disabled again.
9. Record the outcome as `turnstile_interactive_check=passed` only if every step above succeeded; otherwise record `turnstile_interactive_check=blocked_by_network` or the specific failure, and do not proceed to enable claim intake.

This interactive check is a prerequisite for, but is not itself, approval to enable `FAUCET_ENABLED` or run the canary claim procedure.

## Rollback

If Caddy validation, restart, static asset fetch, hostname/SNI, CSP, or route isolation fails:

```bash
set -euo pipefail
: "${BACKUP_DIR:?Set BACKUP_DIR to the recorded root-only Caddy backup directory}"
sudo rm -f \
  /var/www/faucet-staging/staging-token.html \
  /var/www/faucet-staging/staging-token.css \
  /var/www/faucet-staging/staging-token.js
sudo install -o root -g caddy -m 0640 \
  "$BACKUP_DIR/Caddyfile.staging.before" \
  /etc/caddy/Caddyfile.staging
sudo -u caddy test -r /etc/caddy/Caddyfile.staging
sudo caddy validate --config /etc/caddy/Caddyfile.staging --adapter caddyfile
sudo systemctl reset-failed caddy-staging.service
sudo systemctl restart caddy-staging.service
sudo systemctl is-active --quiet caddy-staging.service
```

Keep `FAUCET_ENABLED=false`. Do not call an internal enable/disable endpoint as part of static-route validation, and do not switch to production Caddy, DNS, credentials, state, or funds.

## Known non-blocking Caddy warnings

The following log lines have been observed during staging deployments and are not deployment failures. Do not respond to them by enabling claims, changing production Caddy, or weakening origin controls:

- `Caddyfile input is not formatted; run the 'caddy fmt' command to fix inconsistencies` — the tracked template and live staging copy were normalized with Caddy 2.6.2 `caddy fmt`; this historical warning is retained for log interpretation and is cosmetic if it reappears on an older copy.
- `failed to sufficiently increase receive buffer size ... wanted: 2048 kiB, got: 416 kiB` — HTTP/3 UDP buffer sizing; requires a host-level `net.core.rmem_max`/`wmem_max` change to fully resolve and does not affect HTTP/1.1 or HTTP/2 traffic.
- `no OCSP stapling for [...]: no OCSP server specified in certificate` — expected for the current certificate issuance path.

Treat any other error-level log line, and any of the three lines above appearing together with a failed `systemctl is-active`, as a real failure requiring rollback.

## Minor self-review observations

- The formatter warning was a source/configuration hygiene issue only. The canonical template is now Caddy-formatted; future live updates must install the reviewed template, run `caddy validate`, and reload only `caddy-staging.service`.
- The Turnstile interactive check remains pending from an independent browser/network because the automated environment cannot reach `challenges.cloudflare.com`. This is the expected fail-closed behavior; claim intake must remain disabled until the documented no-claim check passes.
- The HTTP/3 receive-buffer warning is optional host tuning, not a route or availability defect for HTTP/1.1/HTTP/2. Do not make kernel changes as part of this deployment review.
- The OCSP message is certificate metadata behavior for the current issuance path. It is not evidence of a Caddy control-plane or origin-isolation failure.
- A separate origin-lockdown review records Cloudflare returning `405` for `TRACE` while the Caddy fallback returns `404`. Treat that as an edge-policy verification item, not a formatting issue, and do not mark the exact-status gate passed until the documented edge decision is completed.
- The historical disabled-only preflight records that no staging Caddy service existed at that earlier checkpoint. Treat that statement as historical evidence; use this runbook and the current VPS/service attestation as the source of truth for the deployed Caddy state.

## Self-review checklist

- [ ] The former shell heredoc and literal `EOF` are absent from the served page.
- [ ] The page contains no Turnstile secret, operator token, wallet key, recovery phrase, or signed envelope.
- [ ] The JS contains no console logging, browser storage, internal endpoint, or authorization header.
- [ ] The exact staging hostname guard and `faucet_claim` action remain present.
- [ ] The route exposes only three exact static asset paths.
- [ ] The static directory is root-owned and caddy-readable but not caddy-writable.
- [ ] Caddy remains the staging unit, binds admin only to loopback, and is validated before restart.
- [ ] `caddy-staging.service` `ExecReload` targets `127.0.0.1:2019` explicitly.
- [ ] The faucet service remains disabled and the egress policy remains effective.
- [ ] No canary claim is submitted by this deployment step.
- [ ] The interactive Turnstile verification is recorded as passed, or claim intake stays disabled pending it.
