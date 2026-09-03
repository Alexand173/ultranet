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
- Public route: **not provisioned**. `faucet-staging.ultranetwork.cc` was unresolved during the read-only DNS probe; no `caddy-staging.service` or `/etc/caddy/Caddyfile.staging` exists on the VPS.
- Turnstile: the current encrypted value is a disposable local-preflight placeholder. It proves credential projection and startup only; it is not a provider-issued secret and must be replaced before any claim test.

The current status is therefore **node/faucet local preflight passed; public route and claims enablement blocked**.

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

Before any enablement approval, replace the disposable Turnstile placeholder with a real secret from a separately provisioned staging widget:

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

Use a root-only plaintext staging directory only during the encryption ceremony. After encryption, verify metadata only (`root:root`, mode `0600`), remove plaintext files securely, and verify the service projection is readable by the service process without exposing contents.

Restart with `FAUCET_ENABLED=false` and rerun:

```bash
sudo systemctl daemon-reload
sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service
sudo systemctl restart ultranet-faucet-staging.service
sudo -u ultranet-faucet-staging /opt/ultranet-faucet-staging/ultranet-faucet check-config
```

Recheck all local gates from [faucet-staging-install-preflight.md](./faucet-staging-install-preflight.md): signer/address, node chain and six decimals, fee estimate, zero queue/budget, disabled public status, operator authentication, isolation, loopback listeners, `MemorySwapMax=0`, and zero OOM events.

## Phase C — backup and explicit claim approval

Do not enable intake until all of the following are present in the private change record:

- successful Phase A public checks;
- real staging Turnstile hostname/secret verification;
- staging signer/address and node account/fee verification;
- separate Caddy/WAF/rate-limit/origin policy evidence;
- encrypted staging SQLite/WAL backup and a tested disable/stop rollback path;
- exact binary checksum, source revision, unit revision, environment revision, and credential alias record;
- an explicit written approval for **exactly one disposable claim**.

Public status approval, node-only approval, or preview approval does not authorize a claim.

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
- [ ] Local and public status report disabled intake, fixed amount, and six decimals; unsupported/internal routes are blocked at the public boundary.
- [ ] The real staging Turnstile secret replaces the disposable placeholder and is bound to the exact staging hostname/action.
- [ ] A staging-only backup and rollback path is recorded; binary, unit, environment, and credential metadata are reproducible.
- [ ] A separate written approval authorizes exactly one disposable claim before any enable call.
- [ ] The single canary reconciles claim, node, balance, nonce, fee, source debit, recipient balance, and SQLite state exactly.
- [ ] Same-key replay produces no second payout; changed-destination replay returns `409`.
- [ ] Intake is disabled immediately after the canary, with redacted evidence preserved.
- [ ] No production endpoint, credential, state, proxy, DNS, firewall, service, or funds are touched.
