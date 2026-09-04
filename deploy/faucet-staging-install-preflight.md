# UltraNet staging faucet installation and preflight

This runbook is for a **separate staging VPS only**. It does not enable intake, submit a claim, alter the production VPS, change production Cloudflare rules, or copy production credentials.

The staging validator must already be provisioned as an isolated `ultranet-staging.service` with its own chain/genesis/network identity, database, P2P identity, service account, API credentials, and resource limits. It must bind its API to `127.0.0.1:8081` and must not share production storage or peers. Do not install `deploy/ultranet.service` unchanged as the staging validator unit.

The validator templates are:

- [`ultranet.env.staging-test.example`](./ultranet.env.staging-test.example)
- [`ultranet-staging.service`](./ultranet-staging.service)

The faucet templates used below are:

- [`faucet.env.staging.example`](./faucet.env.staging.example)
- [`ultranet-faucet-staging.service`](./ultranet-faucet-staging.service)
- [`caddy-staging.service`](./caddy-staging.service)
- [`Caddyfile.staging.example`](./Caddyfile.staging.example)
- [`ultranet-faucet-staging-egress.py`](./ultranet-faucet-staging-egress.py)
- [`ultranet-faucet-staging-egress-expiry.service`](./ultranet-faucet-staging-egress-expiry.service)
- [`ultranet-faucet-staging-egress-expiry.timer`](./ultranet-faucet-staging-egress-expiry.timer)
- [`faucet-staging-egress-review.md`](./faucet-staging-egress-review.md) — deferred review-only installation block and architecture decision

## Safety gates

Before running the installation block, confirm out-of-band:

- the SSH host is the separately provisioned staging VPS;
- its public IP is not the production origin `167.233.161.115`;
- `faucet-staging.ultranetwork.cc` is approved and will resolve only to the staging VPS;
- the staging validator is not using production database, genesis/network identity, P2P identity, peers, admin token, or storage;
- a separate staging Turnstile widget/secret exists for the exact hostname and action `faucet_claim`;
- no private key, Turnstile secret, abuse key, operator token, wallet password, or recovery phrase will be pasted into chat or ordinary command output.

The commands use placeholders through environment variables. Set them in the private maintenance shell; do not commit them or put secrets in shell history:

```bash
export STAGING_SOURCE_DIR=/opt/ultranet-staging-src
export STAGING_REVISION="<locked-40-character-commit-sha>"
export STAGING_VPS_IP="<staging-vps-public-ip>"
export STAGING_HOSTNAME="faucet-staging.ultranetwork.cc"
export STAGING_CADDY_SERVICE="caddy-staging.service"
```

The production IP guard below is deliberate. If the staging IP changes, update the documented production identity before continuing; never remove the guard merely to make the command run. The Caddy service name is a required staging-host value; never set it to the production `caddy.service` unit.

## 1. Verify host and source identity

Run on the staging VPS from the checked-out source revision:

```bash
set -euo pipefail
: "${STAGING_SOURCE_DIR:?Set STAGING_SOURCE_DIR in the private maintenance shell}"
: "${STAGING_REVISION:?Set STAGING_REVISION to the approved locked commit SHA}"
: "${STAGING_VPS_IP:?Set STAGING_VPS_IP to the approved staging VPS address}"
: "${STAGING_HOSTNAME:?Set STAGING_HOSTNAME to the approved staging hostname}"
: "${STAGING_CADDY_SERVICE:?Set STAGING_CADDY_SERVICE to the staging Caddy unit name}"

test "$STAGING_CADDY_SERVICE" = "caddy-staging.service" \
  || { printf '%s\n' 'Caddy service must be the checked-in staging unit name' >&2; exit 1; }
test "$STAGING_HOSTNAME" = "faucet-staging.ultranetwork.cc" \
  || { printf '%s\n' 'staging hostname does not match the checked-in staging templates' >&2; exit 1; }
test "$STAGING_HOSTNAME" != "faucet.ultranetwork.cc" \
  || { printf '%s\n' 'refusing production faucet hostname' >&2; exit 1; }
test "$STAGING_VPS_IP" != "167.233.161.115" \
  || { printf '%s\n' 'refusing documented production faucet origin' >&2; exit 1; }
test -d "$STAGING_SOURCE_DIR"
test "$(git -C "$STAGING_SOURCE_DIR" rev-parse HEAD)" = "$STAGING_REVISION" \
  || { printf '%s\n' 'source checkout is not the approved locked revision' >&2; exit 1; }
if test -n "$(git -C "$STAGING_SOURCE_DIR" status --porcelain --untracked-files=all)"; then
  printf '%s\n' 'source checkout has unexpected local or untracked changes' >&2
  exit 1
fi
```

The checkout cleanliness check is a review gate, not a substitute for verifying the complete staging checkout. Build the node and faucet from this same revision. The egress helper is a separate root-owned deployment artifact and must be reviewed and checked at the same source revision:

```bash
cd "$STAGING_SOURCE_DIR"
cargo build --release --locked --bin UltraNet --bin ultranet-faucet

sudo install -d -o root -g root -m 0755 \
  /opt/ultranet-staging/target/release \
  /opt/ultranet-faucet-staging
sudo install -o root -g root -m 0755 \
  target/release/UltraNet \
  /opt/ultranet-staging/target/release/UltraNet
sudo install -o root -g root -m 0755 \
  target/release/ultranet-faucet \
  /opt/ultranet-faucet-staging/ultranet-faucet
```

The separately provisioned `ultranet-staging.service` must reference the installed staging node binary and its staging-only environment/database paths. Verify that unit before starting the faucet.

## 2. Create the staging faucet account and paths

Do not reuse the production `ultranet-faucet` account or grant the staging account membership in `ultranet`:

```bash
set -euo pipefail
if ! getent group ultranet-faucet-staging >/dev/null; then
  sudo groupadd --system ultranet-faucet-staging
fi

if id -u ultranet-faucet-staging >/dev/null 2>&1; then
  test "$(id -gn ultranet-faucet-staging)" = "ultranet-faucet-staging" \
    || { printf '%s\n' 'existing staging faucet account has the wrong primary group' >&2; exit 1; }
  if id -nG ultranet-faucet-staging | tr ' ' '\n' | grep -qx ultranet; then
    printf '%s\n' 'staging faucet account is a member of the production ultranet group' >&2
    exit 1
  fi
else
  sudo useradd --system \
    --gid ultranet-faucet-staging \
    --home-dir /var/lib/ultranet-faucet-staging \
    --shell /usr/sbin/nologin \
    ultranet-faucet-staging
fi

sudo install -d -o ultranet-faucet-staging -g ultranet-faucet-staging -m 0700 \
  /var/lib/ultranet-faucet-staging
sudo install -d -o root -g ultranet-faucet-staging -m 0750 \
  /etc/ultranet-faucet-staging
sudo install -d -o root -g root -m 0700 \
  /etc/ultranet-faucet-staging/secrets
```

## 3. Install the non-secret environment and generate staging credentials

Install the template, then replace the address placeholder with the address printed by the staging-only key generator. The printed address is public identity data; the generated JSON file is private credential material:

```bash
sudo install -o root -g ultranet-faucet-staging -m 0640 \
  "$STAGING_SOURCE_DIR/deploy/faucet.env.staging.example" \
  /etc/ultranet-faucet-staging/faucet.env

sudo install -d -o root -g root -m 0700 \
  /root/ultranet-faucet-staging-plaintext
sudo /opt/ultranet-faucet-staging/ultranet-faucet keygen \
  --output /root/ultranet-faucet-staging-plaintext/faucet-staging-signer.json
```

Record the printed staging address through the approved change record, then edit only the non-secret environment file:

```bash
sudoedit /etc/ultranet-faucet-staging/faucet.env
sudo grep -Eq '^FAUCET_ENABLED=false$' \
  /etc/ultranet-faucet-staging/faucet.env
if ! sudo grep -Eq '^FAUCET_ADDRESS=[0-9a-f]{64}$' \
  /etc/ultranet-faucet-staging/faucet.env; then
  printf '%s\n' 'staging faucet address must be exactly 64 lowercase hexadecimal characters' >&2
  exit 1
fi
if sudo grep -q 'replace-with-derived-staging-faucet-address' \
  /etc/ultranet-faucet-staging/faucet.env; then
  printf '%s\n' 'staging faucet address placeholder was not replaced' >&2
  exit 1
fi
if ! sudo grep -Eq '^FAUCET_TURNSTILE_HOSTNAME=faucet-staging\.ultranetwork\.cc$' \
  /etc/ultranet-faucet-staging/faucet.env; then
  printf '%s\n' 'staging Turnstile hostname is not the approved exact host' >&2
  exit 1
fi
```

Create the independent abuse and operator secrets without printing them. Supply the Turnstile secret from the separately provisioned staging widget:

```bash
sudo sh -c 'umask 077; \
  openssl rand -hex 32 > /root/ultranet-faucet-staging-plaintext/faucet-staging-abuse.key; \
  openssl rand -hex 32 > /root/ultranet-faucet-staging-plaintext/faucet-staging-operator.token'
sudoedit /root/ultranet-faucet-staging-plaintext/faucet-staging-turnstile.secret
sudo chown root:root /root/ultranet-faucet-staging-plaintext/faucet-staging-turnstile.secret
sudo chmod 0600 /root/ultranet-faucet-staging-plaintext/faucet-staging-turnstile.secret
```

Encrypt each credential with the staging host credential key. The aliases must remain exactly aligned with the environment template and the staging systemd unit:

```bash
sudo systemd-creds setup

for credential in \
  faucet-staging-signer.json \
  faucet-staging-turnstile.secret \
  faucet-staging-abuse.key \
  faucet-staging-operator.token; do
  sudo systemd-creds encrypt --with-key=host \
    --name="$credential" \
    "/root/ultranet-faucet-staging-plaintext/$credential" \
    "/etc/ultranet-faucet-staging/secrets/$credential.cred"
done

sudo chown root:root /etc/ultranet-faucet-staging/secrets/*.cred
sudo chmod 0600 /etc/ultranet-faucet-staging/secrets/*.cred
sudo find /etc/ultranet-faucet-staging/secrets -maxdepth 1 -type f \
  -printf '%f %u:%g %m\n' | sort
sudo shred --remove /root/ultranet-faucet-staging-plaintext/*
sudo rmdir /root/ultranet-faucet-staging-plaintext
```

The metadata listing must show only the four expected `.cred` files, owned by `root:root`, with mode `600`. Never use `cat`, `journalctl`, shell tracing, or an environment dump on these files.

## 4. Install and validate the staging units and Caddy route

Install the disposable validator's non-secret environment and encrypted admin
credential separately from the faucet credentials. The node unit uses
`ULTRANET_ADMIN_TOKEN_FILE`; do not create `ULTRANET_ADMIN_TOKEN` in this
environment and do not place the token value in the environment file:

```bash
if ! getent group ultranet-staging >/dev/null; then
  sudo groupadd --system ultranet-staging
fi
if ! id -u ultranet-staging >/dev/null 2>&1; then
  sudo useradd --system \
    --gid ultranet-staging \
    --home-dir /var/lib/ultranet-staging \
    --shell /usr/sbin/nologin \
    ultranet-staging
else
  test "$(id -gn ultranet-staging)" = "ultranet-staging" \
    || { printf '%s\n' 'existing staging node account has the wrong primary group' >&2; exit 1; }
fi

sudo install -d -o root -g ultranet-staging -m 0750 /etc/ultranet-staging
sudo install -d -o root -g root -m 0700 /etc/ultranet-staging/secrets
sudo install -d -o ultranet-staging -g ultranet-staging -m 0700 /var/lib/ultranet-staging
sudo install -o root -g ultranet-staging -m 0640 \
  "$STAGING_SOURCE_DIR/deploy/ultranet.env.staging-test.example" \
  /etc/ultranet-staging/ultranet.env
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-staging.service" \
  /etc/systemd/system/ultranet-staging.service

# Stage the value only in the root-only maintenance directory. Never print it.
sudo install -d -o root -g root -m 0700 /root/ultranet-staging-plaintext
sudoedit /root/ultranet-staging-plaintext/ultranet-staging-admin.token
sudo chown root:root /root/ultranet-staging-plaintext/ultranet-staging-admin.token
sudo chmod 0600 /root/ultranet-staging-plaintext/ultranet-staging-admin.token
sudo systemd-creds encrypt --with-key=host \
  --name=ultranet-staging-admin.token \
  /root/ultranet-staging-plaintext/ultranet-staging-admin.token \
  /etc/ultranet-staging/secrets/ultranet-staging-admin.token.cred
sudo chown root:root /etc/ultranet-staging/secrets/ultranet-staging-admin.token.cred
sudo chmod 0600 /etc/ultranet-staging/secrets/ultranet-staging-admin.token.cred
sudo shred --remove /root/ultranet-staging-plaintext/ultranet-staging-admin.token
sudo rmdir /root/ultranet-staging-plaintext
```

The node credential metadata must show only the expected root-owned encrypted
file. Never use `cat`, shell tracing, `journalctl`, or an environment dump on
it. Confirm that the environment file contains no `ULTRANET_ADMIN_TOKEN=`
assignment before continuing.

```bash
if sudo grep -Eq '^ULTRANET_ADMIN_TOKEN=' /etc/ultranet-staging/ultranet.env; then
  printf '%s\n' 'ULTRANET_ADMIN_TOKEN must not be present in the staging node environment' >&2
  exit 1
fi
sudo stat -c '%n %U:%G %a' \
  /etc/ultranet-staging/ultranet.env \
  /etc/ultranet-staging/secrets/ultranet-staging-admin.token.cred \
  /var/lib/ultranet-staging
```

```bash
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging.service" \
  /etc/systemd/system/ultranet-faucet-staging.service
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/caddy-staging.service" \
  /etc/systemd/system/caddy-staging.service

sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/Caddyfile.staging.example" \
  /etc/caddy/Caddyfile.staging

# Install the non-secret egress validator and the fail-closed expiry monitor.
# The refresh command is intentionally operator-run; it is not a timer and it
# never runs while the faucet service is active.
sudo install -d -o root -g root -m 0755 /usr/local/libexec
sudo install -o root -g root -m 0755 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress.py" \
  /usr/local/libexec/ultranet-faucet-staging-egress.py
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.service" \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.service
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.timer" \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.timer
sudo install -d -o root -g root -m 0755 \
  /etc/ultranet-faucet-staging/egress

# The refresh command must run from a private root maintenance shell while
# the faucet is stopped and intake is disabled. It is not a service timer.
# Initial policy creation requires explicit private approval for the published
# Cloudflare ranges and the resolver destinations observed on this host.
# This writes only root-owned, non-secret range/manifest/drop-in metadata.
sudo systemctl stop ultranet-faucet-staging.service
sudo grep -Eq '^FAUCET_ENABLED=false$' /etc/ultranet-faucet-staging/faucet.env
sudo /usr/local/libexec/ultranet-faucet-staging-egress.py \
  --refresh --approve-range-change --approve-resolver-change

sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.timer
sudo caddy validate \
  --config /etc/caddy/Caddyfile.staging \
  --adapter caddyfile
sudo systemctl daemon-reload
sudo /usr/local/libexec/ultranet-faucet-staging-egress.py \
  --check --effective
sudo systemctl enable --now ultranet-faucet-staging-egress-expiry.timer
```

The refresh helper is intentionally not attached to a timer: it fetches external
policy data and requires a private operator review when resolver or Cloudflare
ranges change. To refresh later, stop the faucet, verify `FAUCET_ENABLED=false`,
run the helper with `--approve-range-change` and/or `--approve-resolver-change`
only when the change record authorizes it, then run `daemon-reload`,
`--check --effective`, and restart the faucet while it remains disabled. If the
manifest expires, the expiry timer invokes `--enforce` and stops the faucet.

Keep the staging Caddy route separate from production. The staging Caddy service must be a separately provisioned unit whose `ExecStart` loads `/etc/caddy/Caddyfile.staging` (or an explicitly reviewed effective config that imports it). Do not overwrite `/etc/caddy/Caddyfile` on the production origin. Before any public reload, configure the separate Cloudflare proxied DNS record, Full (strict) TLS, managed WAF, no-store cache bypass, read/write limits, Turnstile hostname/action, temporary canary restriction, and origin firewall policy.

After the staging Caddy service and effective config are confirmed, validate and reload only the named staging unit:

```bash
sudo systemctl show "$STAGING_CADDY_SERVICE" -p FragmentPath -p ExecStart
sudo caddy validate --config /etc/caddy/Caddyfile.staging --adapter caddyfile
sudo systemctl reload "$STAGING_CADDY_SERVICE"
```

The `STAGING_CADDY_SERVICE` guard rejects the production `caddy.service` name. Do not run this block with an unreviewed service alias or against the production Caddy service.

## 5. Non-mutating staging preflight

Keep intake disabled. The following checks do not submit a claim or call `/internal/enable`:

```bash
set -euo pipefail
sudo systemctl --no-pager --full status ultranet-staging.service
sudo systemctl --no-pager --full status ultranet-faucet-staging.service || true

sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.timer
sudo /usr/local/libexec/ultranet-faucet-staging-egress.py \
  --check --effective

sudo -u ultranet-faucet-staging bash -c \
  'set -a; . /etc/ultranet-faucet-staging/faucet.env; set +a; \
   /opt/ultranet-faucet-staging/ultranet-faucet check-config'

sudo stat -c '%n %U:%G %a' \
  /etc/ultranet-faucet-staging/faucet.env \
  /etc/ultranet-faucet-staging/secrets/*.cred \
  /var/lib/ultranet-faucet-staging

sudo -u ultranet-faucet-staging test ! -r /etc/ultranet/ultranet.env
sudo -u ultranet-faucet-staging test ! -r /etc/ultranet-faucet/secrets/faucet-signer.json.cred
sudo -u ultranet-faucet-staging test ! -r /var/lib/ultranet/blocks
sudo -u ultranet-faucet-staging test ! -r /opt/ultranet/target/release/UltraNet
```

Start the staging validator and faucet only after the unit, address, credential metadata, and isolation checks pass. The environment template keeps intake disabled:

```bash
sudo systemctl start ultranet-staging.service
sudo systemctl start ultranet-faucet-staging.service
sudo systemctl --no-pager --full status ultranet-faucet-staging.service
```

Set the public staging address in the private maintenance shell from the recorded keygen result. Do not place the signer JSON in this shell variable:

```bash
export STAGING_FAUCET_ADDRESS="<64-character-staging-faucet-address>"
if ! printf '%s' "$STAGING_FAUCET_ADDRESS" | grep -Eq '^[0-9a-f]{64}$'; then
  printf '%s\n' 'staging faucet address must be exactly 64 lowercase hexadecimal characters' >&2
  exit 1
fi
if test "$(sudo sed -n 's/^FAUCET_ADDRESS=//p' /etc/ultranet-faucet-staging/faucet.env)" != "$STAGING_FAUCET_ADDRESS"; then
  printf '%s\n' 'staging address variable does not match faucet environment' >&2
  exit 1
fi
```

Check the staging node contract and faucet identity without funding or submitting a claim:

```bash
curl --fail --silent --show-error http://127.0.0.1:8081/api/validate
curl --fail --silent --show-error \
  "http://127.0.0.1:8081/api/account/$STAGING_FAUCET_ADDRESS"
curl --fail --silent --show-error \
  'http://127.0.0.1:8081/api/transaction/estimate?recipient=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&amount=1000000'

curl --fail --silent --show-error http://127.0.0.1:8090/api/faucet/status
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  http://127.0.0.1:8090/internal/status
```

The operator must verify from the responses that the staging node has the intended chain/network identity and `decimals=6`, the signer-derived address equals `FAUCET_ADDRESS`, the fixed claim amount is `1,000,000` base units, the estimated fee is at least `10,000`, and local intake reports `enabled=false`. Unauthenticated local `/internal/status` should remain `401`.

Review only redacted service output and confirm the signer/address probe succeeded:

```bash
sudo journalctl -u ultranet-faucet-staging.service \
  --since '10 minutes ago' --no-pager
sudo systemctl show ultranet-faucet-staging.service \
  -p User -p Group -p SupplementaryGroups -p ExecStart \
  -p ProtectSystem -p InaccessiblePaths -p ReadOnlyPaths -p ReadWritePaths
sudo grep -n '^LoadCredentialEncrypted=' \
  /etc/systemd/system/ultranet-faucet-staging.service
```

## 6. Public route and origin-boundary checks

Run these only after the staging DNS, TLS, WAF, and effective staging Caddy configuration are active. Run them from an external controlled network, not only from the VPS:

```bash
curl --fail --silent --show-error \
  "https://$STAGING_HOSTNAME/api/faucet/status"
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  "https://$STAGING_HOSTNAME/internal/status"
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  -X POST "https://$STAGING_HOSTNAME/api/faucet/status"
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  "https://$STAGING_HOSTNAME/api/faucet/unknown"
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \
  -X PUT "https://$STAGING_HOSTNAME/api/faucet/claims"
```

Expected results:

- `GET /api/faucet/status`: `200`, disabled intake, fixed policy, six decimals;
- `/internal/status`, `POST /api/faucet/status`, unknown paths, and unsupported methods: `404` at the public boundary;
- no public route to the staging node API, dashboard, signer, credential directory, or any `/internal/*` endpoint;
- direct origin web access blocked by the staging firewall while Cloudflare HTTPS remains reachable;
- Cloudflare logs contain no request bodies, CAPTCHA tokens, cookies, authorization headers, idempotency keys, or signed envelopes.

Do not use a public claim to prove routing. A staging canary is a separate approved procedure requiring a temporary operator-egress restriction, `202 Accepted`, confirmation polling, exact node/database reconciliation, duplicate-idempotency replay, changed-destination conflict, and post-canary disablement. Do not proceed to that canary while any preflight result is inconsistent.

## Stop and rollback

If any local/public status, signer/address, node identity, proxy boundary, credential, database permission, budget, or log check fails:

```bash
# Use the protected operator path with the staging-only token through the
# approved secret-management/operator procedure; never paste the token here.
# POST http://127.0.0.1:8090/internal/disable

sudo systemctl stop ultranet-faucet-staging.service
sudo journalctl -u ultranet-faucet-staging.service \
  --since '15 minutes ago' --no-pager
```

Preserve the staging SQLite database, WAL/SHM state, unit/config revision, and redacted logs for review. Never switch the staging service to production credentials or address as a recovery shortcut.

## Recorded disabled-only preflight — 2026-09-03

A disabled-only staging faucet preflight was initially run on the separately provisioned VPS `2.28.61.71` after the staging validator's node-only gate passed. The initial faucet unit used source revision `fb1274278190b4ddbeb5c9a4f0625d18d4df0fb6`; the deployed `ultranet-faucet` artifact matched SHA-256 `bd7258d2d2d0d4b7d86c1025c6d15442d24bc66368a588f8bd2dad17cc739cd3`.

The preflight installed the dedicated `ultranet-faucet-staging` user/group, staging binary, staging environment, four encrypted credential files, and the staging systemd unit. A fresh staging-only Dilithium signer/address was generated and did not reuse the production faucet address. The plaintext signer, abuse, operator, and Turnstile staging files were removed after encryption; only root-owned mode-`0600` `.cred` files remain. The staging service has no production group membership and cannot read production or validator paths under its service UID.

The staging faucet service is active but remains disabled at both configuration and persisted service-state layers. `check-config` passed with `127.0.0.1:8090`, node API `http://127.0.0.1:8081`, the exact hostname `faucet-staging.ultranetwork.cc`, the fixed claim amount `1,000,000` base units, and `FAUCET_ENABLED=false`. Local status reported `enabled=false`, `availability=disabled`, six decimals, zero queue depth, zero reserved/confirmed budget, and zero claims. Unauthenticated `/internal/status` and `/internal/health` returned `401`; an unknown claim lookup returned `404`.

The staging node returned valid chain state, the staging signer address account probe returned `decimals=6`, and the transaction estimate returned the fixed amount with a fee of at least `10,000` base units. API and P2P listeners remained loopback-only. The faucet cgroup used `MemoryHigh=512M`, `MemoryMax=1G`, and `MemorySwapMax=0`; `oom=0` and `oom_kill=0` were recorded. No production/faucet/Caddy service was active and no production or public faucet connection was observed.

For this no-claim preflight, the Turnstile credential was a random disposable local placeholder because a provider-issued staging widget secret was not available. It was loaded only to prove credential projection and startup; no request was sent to Cloudflare Siteverify. Therefore Turnstile hostname/secret validity, public DNS/TLS/WAF behavior, Caddy validation, origin firewall policy, and external route checks remain unverified. The staging Caddy template is reviewed and committed but is not installed because no separate staging Caddy service exists on this host.

No `/internal/enable` call was made, no claim was submitted, no faucet funds were moved, and no production DNS, Cloudflare, firewall, proxy, service, credential, state, or funds were modified. Replace the placeholder with a separately provisioned staging Turnstile secret and obtain a separate claim approval before any canary procedure.
