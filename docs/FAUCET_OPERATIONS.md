# UltraNet Faucet Operations Runbook

**Status:** mainnet capped-beta operations guide
**Service:** `ultranet-faucet.service`
**State:** `/var/lib/ultranet-faucet/faucet.db`
**Listener:** `127.0.0.1:8090`
**Node boundary:** `127.0.0.1:8081`

This runbook operates the isolated faucet described in [`FAUCET_TECHNICAL_SPEC.md`](./FAUCET_TECHNICAL_SPEC.md). It is intentionally separate from the validator service and from the legacy [`send_ultra.py`](../send_ultra.py) funding script.

## Safety boundary

The faucet is a low-balance online signer for one dedicated address:

```text
fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8
```

`787e68b2…` was the originally documented address. It is not this faucet: it holds `1,000.001000`
$ULTRA and no key for it is reachable, so it is a stranded balance — never fund it, and never point a
signer at it.

It may call only:

```text
GET  /api/account/<faucet-address>
GET  /api/transaction/estimate
POST /api/transaction
GET  /api/transaction/<hash>
```

The faucet must never receive or send `ULTRANET_ADMIN_TOKEN`. It must never read `sovereign_keys.json`, validator keys, recovery phrases, `/var/lib/ultranet`, `/etc/ultranet/ultranet.env`, the website build, or browser wallet secrets. It submits a normal version-1 L1 transfer; the node creates and validates the standard-transfer ZK proof.

The legacy `send_ultra.py` script is not an operational dependency. It loads sovereign key material, uses a static nonce, and writes `transfer.json`. Do not copy it, its key file, or its output to the faucet host.

## Policy and accounting

The initial beta policy is fixed and server-selected:

| Value | Base units | Human-readable |
| --- | ---: | ---: |
| Claim amount | `1,000,000` | `1.000000 $ULTRA` |
| Minimum fee | `10,000` | `0.010000 $ULTRA` |
| Source debit | `1,010,000` | `1.010000 $ULTRA` |
| Daily debit cap | `100,000,000` | `100.000000 $ULTRA` |
| Minimum source reserve | `200,000,000` | `200.000000 $ULTRA` |
| Address cooldown | `86,400` seconds | 24 rolling hours |

The deployed faucet address held `0` base units on 2026-10-26, so it cannot pay a claim until a float is transferred to it: it needs more than `FAUCET_MIN_BALANCE_RESERVE_BASE_UNITS` (`200.000000` $ULTRA) plus the claim amount and its fee. At the historical `1,000,000,000` base units (`1,000.000000` $ULTRA) snapshot the balance-only ceiling was 990 claims, but the service must stop at the configured reserve and daily cap. Budget reservations include both amount and fee.

## Funding the float

The float is a normal version-1 L1 transfer from the Sovereign account, which requires 2-of-3 owner
signatures. `send_ultra.py` performs it. It is a dry run unless `--send` is passed.

A 100 $ULTRA transfer does **not** clear the `200.000000` reserve above, so claims stay blocked after
it. Fund at least `201.000000` to make the first claim payable, and keep the reserve in mind when
choosing the amount.

### Prerequisites, once per machine

```bash
# 1. The Python binding to the repository's own PQClean Dilithium-5 (no downloads):
tools/pqclean_dilithium5/build.sh          # builds and installs the `pqc` package into ./venv

# 2. Sovereign key material, never committed and never copied to the faucet host:
ls -l sovereign_keys.json                   # at least two owner records, mode 0600
```

The node signs with the `pqcrypto-dilithium 0.5` crate and the binding compiles the same vendored C,
so signatures are byte-identical. Do not substitute an ML-DSA library: FIPS 204 is a different scheme
and its signatures do not verify on this chain.

### Step 1 — dry run and read every line

```bash
./venv/bin/python send_ultra.py --check-file /tmp/sig-artifact.json
```

The script fetches the live nonce and the node's fee quote, signs with two owners, and refuses to
continue if either signature fails to verify. Nothing is submitted and `transfer.json` is not written.
Confirm before going further:

- `Nonce` equals `nonce` from `GET /api/account/<sovereign>`; a stale nonce is rejected outright.
- `Network fee` equals `fee` from `GET /api/transaction/estimate` for this recipient and amount; the
  script aborts if the quote disagrees with its expectation.
- `Total debit` is `amount + fee` and sits below the Sovereign balance.
- `Timestamp` is within the hour; the node rejects older envelopes as *"Transaction is too old!"*.
- `signature : 9254 bytes (2 x 4627)`.

### Step 2 — verify the signature outside the binding

```bash
cargo run --locked --example verify_sig_artifact -- /tmp/sig-artifact.json
```

Expected: `node verifies : true`, and `derived address` equal to one of the Sovereign owner addresses
in `ULTRANET_AUTHORIZED_NODE_IDENTIFIERS`. This uses the same dilithium5 verifier the node runs, so a
`false` here means do not proceed — no transaction can succeed.

### Step 3 — sign and submit

```bash
./venv/bin/python send_ultra.py --send
```

The response is the node's answer verbatim. `success: true` and `status: "pending"` mean the transfer
is in the mempool, not confirmed — confirmation needs a block.

### Step 4 — reconcile on chain

```bash
BASE=https://api.ultranetwork.cc
curl -sS "$BASE/api/account/<faucet-address>" | python3 -m json.tool   # balance +100
curl -sS "$BASE/api/account/<sovereign>"     | python3 -m json.tool   # balance −101, nonce +1
curl -sS "$BASE/api/stats"                   | python3 -m json.tool   # mempool +1 until mined
```

The approval journal must not move (`GET /api/governance/approvals`): funding is a transfer, not a
governance action. Then mine the pending transfer, or leave it for the next block producer.

### If the response is ambiguous

Do **not** re-run the script. Every run produces a new timestamp, nullifier and nonce, so a second run
is a second transfer, not a retry. Read the Sovereign account and its history first:

- `nonce` increased and the destination balance rose → the first transfer landed.
- `nonce` unchanged and the mempool has no matching entry → nothing landed; a fresh dry run is safe.

Intake stays `FAUCET_ENABLED=false` until the reserve, Turnstile, proxy, backup and monitoring gates in
this runbook are all satisfied — funding the address does not enable claims.

The public intake default is disabled. Do not change `FAUCET_ENABLED=true` until all preflight gates below are complete.

## Credential provisioning

Use a controlled provisioning machine to create one dedicated Dilithium-5 key pair. The credential must contain only one record, for example:

```json
{
  "public_key": "<2592-byte Dilithium-5 public key as hex>",
  "secret_key": "<4896-byte Dilithium-5 secret key as hex>"
}
```

The faucet binary validates key sizes, signs/verifies a local probe, and derives the address from the public key. It refuses to start if the derived address differs from `FAUCET_ADDRESS`. Do not use a sovereign owner array or a validator identity file.

Generate independent random credentials for:

- `faucet-signer.json` — dedicated Dilithium signer record;
- `faucet-turnstile.secret` — Cloudflare Turnstile server secret;
- `faucet-abuse.key` — keyed digest secret for abuse-control identifiers;
- `faucet-operator.token` — private faucet control-plane bearer token.

Prefer `systemd-creds`/`LoadCredentialEncrypted=` or a host secret manager. If credentials must be staged as files before encryption, use a root-only directory and `chmod 600`; remove plaintext staging files after successful encryption. Never put these values in `/etc/ultranet-faucet/faucet.env`, shell history, the repository, logs, backups beside the database, or frontend configuration.

The operator token is distinct from `ULTRANET_ADMIN_TOKEN`. It authorizes only the faucet's loopback `/internal/*` routes.

For a new staging signer, generate a dedicated record on the controlled host or an offline provisioning machine. The command refuses to overwrite an existing path and prints only the derived public address, never the key bytes:

```bash
umask 077
/opt/ultranet-faucet/ultranet-faucet keygen --output /root/faucet-signer.json
```

Reconcile the printed address with the `FAUCET_ADDRESS` value before encrypting the record. Never use this command with `sovereign_keys.json`, a validator key path, or an active production signer path. For staging, use the newly generated address in the staging environment file; do not point a disposable signer at a funded production address.

To encrypt a credential with the host key, use a root-only staging directory and remove the plaintext immediately after encryption:

```bash
sudo systemd-creds setup
sudo systemd-creds encrypt --with-key=host \
  --name=faucet-signer.json /root/faucet-signer.json \
  /etc/ultranet-faucet/secrets/faucet-signer.json.cred
sudo shred --remove /root/faucet-signer.json
```

Repeat the encryption step for the Turnstile secret, abuse key, and operator token. Verify only metadata (`stat`, ownership, and mode); never print credential contents.

## One-time host installation

Build the node and faucet from the same source revision, but install them as separate services and users. The faucet user must not be a member of the `ultranet` group.

```bash
sudo useradd --system --home-dir /var/lib/ultranet-faucet \
  --shell /usr/sbin/nologin ultranet-faucet
sudo install -d -o ultranet-faucet -g ultranet-faucet -m 0700 /var/lib/ultranet-faucet
sudo install -d -o root -g ultranet-faucet -m 0750 /etc/ultranet-faucet
sudo install -o root -g ultranet-faucet -m 0640 deploy/faucet.env.example /etc/ultranet-faucet/faucet.env
sudo install -d -o root -g root -m 0750 /opt/ultranet-faucet
sudo install -o root -g root -m 0755 target/release/ultranet-faucet /opt/ultranet-faucet/ultranet-faucet
sudo install -o root -g root -m 0644 deploy/ultranet-faucet.service /etc/systemd/system/ultranet-faucet.service
```

Install the encrypted systemd credentials at the paths referenced by the unit, then verify their ownership and mode. The service unit supplies only the credential names through `$CREDENTIALS_DIRECTORY`; it does not expose the files through the environment.

```bash
sudo systemctl daemon-reload
sudo systemctl enable ultranet-faucet.service
sudo systemctl start ultranet-faucet.service
sudo systemctl --no-pager --full status ultranet-faucet.service
```

The first start should remain disabled at the intake layer and should fail closed if the signer, Turnstile secret, abuse key, operator token, or database policy is missing. A missing credential is not a reason to weaken the unit.

## Dedicated staging environment

A real staging faucet is a separate deployment, not a second environment file on the production host. Provision a separate VPS and an isolated validator/network state before issuing any staging funds:

1. Build the validator and faucet from the same locked source revision. The staging validator must use a separate database, chain/genesis/network identity, P2P identity, service account, and API boundary; it must not share `/var/lib/ultranet`, production peers, production admin credentials, or production storage.
2. Create a dedicated `ultranet-faucet-staging` service user/group and a separate unit such as `ultranet-faucet-staging.service`. Its `EnvironmentFile`, `LoadCredentialEncrypted` source paths, `StateDirectory`, `WorkingDirectory`, and `ReadWritePaths` must all be staging-specific. The service must have no membership in the production `ultranet` group.
3. Generate a fresh staging-only Dilithium-5 signer. Set `FAUCET_ADDRESS` to the address derived by that key, and verify it differs from the production faucet address before encrypting the credential. Use a separate staging SQLite path and separate encrypted signer, Turnstile, abuse, and operator credentials.
4. Publish `faucet-staging.ultranetwork.cc` only after the staging origin is ready. Proxy its DNS record through Cloudflare to the staging VPS, use Full (strict) TLS, and keep the staging node API private. The staging Caddy route must forward only the three public faucet routes to its loopback listener and return `404` for `/internal/*`, unsupported methods, and unknown paths.
5. Create a separate Turnstile widget and secret for the exact hostname `faucet-staging.ultranetwork.cc` and action `faucet_claim`. Set `FAUCET_TURNSTILE_HOSTNAME` to that exact hostname; never reuse the production Turnstile secret or configure staging to accept the production hostname.
6. Add staging-scoped managed WAF, no-store/cache bypass, write/read rate limits, and a temporary operator-egress restriction for the canary. Do not add a broad allow rule and do not alter production Cloudflare rules while validating staging.
7. Configure separate backups, monitoring, alerting, kill-switch access, reserve, and daily debit cap. Keep `FAUCET_ENABLED=false` until the service-user, signer/address, node chain/decimals, database, proxy, WAF, and restore checks pass.

Use [`../deploy/faucet.env.staging.example`](../deploy/faucet.env.staging.example) as the non-secret starting point. Install [`../deploy/ultranet-faucet-staging.service`](../deploy/ultranet-faucet-staging.service) and [`../deploy/Caddyfile.staging.example`](../deploy/Caddyfile.staging.example) only on the separate staging VPS. The environment file requires a real staging address before `check-config` can pass and intentionally does not contain any credential material.

The staging unit expects the `ultranet-faucet-staging` user/group, `/opt/ultranet-faucet-staging/ultranet-faucet`, `/etc/ultranet-faucet-staging/faucet.env`, and four encrypted credentials under `/etc/ultranet-faucet-staging/secrets/`. Its Caddy template publishes only the three public faucet routes for `faucet-staging.ultranetwork.cc`; configure the separate Cloudflare WAF, cache, rate-limit, Turnstile, and origin-firewall policies before enabling intake. Use [`../deploy/faucet-staging-install-preflight.md`](../deploy/faucet-staging-install-preflight.md) for the guarded installation and non-mutating host preflight sequence.

## Local preview (no production access)

Before installing systemd or provisioning real credentials, run the built-in preview harness on a loopback port:

```bash
FAUCET_PREVIEW_BIND=127.0.0.1:18090 \
  cargo run --quiet --locked --bin ultranet-faucet -- preview
```

The preview generates a fresh temporary Dilithium credential with mode `0600`, stores SQLite state under `/tmp/ultranet-faucet-preview-<pid>`, uses an in-process mock node with a disposable balance, and accepts a deterministic test CAPTCHA. It never reads `FAUCET_*` production credentials, calls Cloudflare, calls the validator API, binds a non-loopback address, or submits real funds. The temporary directory is removed on clean shutdown.

Exercise `/api/faucet/status`, `POST /api/faucet/claims`, claim polling, duplicate idempotency, changed-fingerprint rejection, authenticated `/internal/status`, and `/internal/disable`. Use the preview only for service/API QA; it is not a substitute for controlled-node integration or mainnet preflight.

## Preflight before enabling intake

Run all checks locally or through a private administrative shell. Do not perform a public claim during preflight.

1. Validate the unit and environment without printing credential contents:

   ```bash
   sudo systemd-analyze verify /etc/systemd/system/ultranet-faucet.service
   sudo -u ultranet-faucet bash -c 'set -a; . /etc/ultranet-faucet/faucet.env; set +a; /opt/ultranet-faucet/ultranet-faucet check-config'
   ```

2. Confirm that the node is the intended chain and reports the expected denomination:

   ```bash
   curl --fail --silent http://127.0.0.1:8081/api/validate
   curl --fail --silent \
     http://127.0.0.1:8081/api/account/fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8
   curl --fail --silent \
     'http://127.0.0.1:8081/api/transaction/estimate?recipient=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&amount=1000000'
   ```

   Verify `decimals=6`, the canonical faucet address, the expected fixed amount, and a fee of at least `10,000` base units. Do not assume the fee from display values.

3. Confirm the service state is disabled, the kill-switch path works, and the database is writable only by the faucet user:

   ```bash
   curl --fail --silent http://127.0.0.1:8090/api/faucet/status
   curl --fail --silent -H "Authorization: Bearer <operator-token>" \
     http://127.0.0.1:8090/internal/status
   sudo -u ultranet-faucet test -r /var/lib/ultranet-faucet/faucet.db
   sudo -u ultranet-faucet test ! -r /var/lib/ultranet/blocks
   ```

4. Verify isolation with an explicit access review. The faucet process must not be able to read `/etc/ultranet/ultranet.env`, the node Sled path, sovereign key backups, or the website deployment.

5. Confirm the reverse proxy limits the request body and publishes only the intended hostname. Put public traffic behind an upstream WAF/bot-control and edge rate limiter; Caddy's body limit is not a WAF. Keep the node API private and do not add wildcard CORS or browser credentials.

## Reverse proxy and origin policy

The canonical faucet origin is `https://faucet.ultranetwork.cc`. The current repository has no faucet UI, so the configured route is API-only. It accepts only:

```text
GET  /api/faucet/status
GET  /api/faucet/claims/<claim-id>
POST /api/faucet/claims
```

Every other method and path, including `/internal/*`, must return `404` at Caddy and must never reach `ultranet-faucet`. The API listener remains `127.0.0.1:8090`; the node API remains `127.0.0.1:8081` and is not part of this public route.

The preferred future UI deployment is same-origin on `faucet.ultranetwork.cc`. No CORS headers, browser credentials, or wildcard origins are required in that model. Keep `website/.env.example` and the existing dashboard `NEXT_PUBLIC_API_BASE_URL` pointed at `https://api.ultranetwork.cc`. If a cross-origin faucet UI is approved later, add one exact UI origin plus an explicit preflight policy at the proxy; never add `*` or credentials.

The proxy must strip `Authorization`, cookies, CSRF headers, `CF-Connecting-IP`, and client-supplied forwarding headers. The Caddy 2.6.2 route uses its per-upstream `trusted_proxies` CIDR list and rebuilds `X-Forwarded-For` from trusted proxy metadata rather than copying raw `CF-Connecting-IP`. Until the VPS firewall or a tunnel restricts origin access to the upstream WAF, direct-origin requests must not be treated as trusted client identity.

Turnstile is bound to the exact application context `faucet.ultranetwork.cc` with action `faucet_claim`. The server accepts a Siteverify response only when `success=true`, `hostname` matches `faucet.ultranetwork.cc`, and `action` matches `faucet_claim`; missing or mismatched context is treated as a rejected CAPTCHA. The browser supplies only the short-lived token and never selects the hostname or action.

Validate and reload the route without exposing credentials:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
sudo systemctl reload caddy
curl --fail --silent https://faucet.ultranetwork.cc/api/faucet/status
curl --silent --show-error --output /dev/null --write-out '%{http_code}\n' \\
  https://faucet.ultranetwork.cc/internal/status  # must be 404
```

Cloudflare must provide TLS/WAF/bot protection, disable caching for `/api/faucet/*`, apply a dedicated write rate limit, and avoid logging request bodies or secret headers. Caddy's body limit is a request guard, not a WAF. The exact dashboard expressions and the guarded UFW procedure are in [`../deploy/CLOUDFLARE_RULES.md`](../deploy/CLOUDFLARE_RULES.md) and [`../deploy/cloudflare-origin-lockdown.sh`](../deploy/cloudflare-origin-lockdown.sh).

The node API hostname must also be Cloudflare-proxied before broad TCP 80/443 rules are removed. The current production API and faucet share the VPS origin, so tightening the firewall for only the faucet would either leave a bypass or break the API. Run the lockdown script first in `--check` mode, then stage allow rules, verify all public HTTPS routes, and remove broad rules only with an active SSH recovery path. Never run the removal step while `api.ultranetwork.cc` is DNS-only.

## Public API

The public routes are:

```text
POST /api/faucet/claims
GET  /api/faucet/claims/<claim-id>
GET  /api/faucet/status
```

A claim request contains only:

```json
{
  "address": "<64 lowercase hexadecimal address>",
  "captcha_token": "<short-lived Turnstile token>"
}
```

It also requires an opaque `Idempotency-Key` header. The response is `202 Accepted` only after the claim, address cooldown, idempotency binding, and exact source-debit budget reservation are durable in SQLite. A repeated request with the same key and fingerprint returns the original claim; a changed request returns `409`.

The browser never sends or sees the signer key, node admin token, nonce, fee, nullifier, signature, ZK proof, or internal queue state. A claim is complete only when `GET /api/faucet/claims/<claim-id>` reports `confirmed`.

## Monitoring

Inspect the service and redacted worker events with:

```bash
sudo systemctl --no-pager --full status ultranet-faucet.service
sudo journalctl -u ultranet-faucet.service --since '15 minutes ago' --no-pager
curl --fail --silent -H "Authorization: Bearer <operator-token>" \
  http://127.0.0.1:8090/internal/status
curl --fail --silent -H "Authorization: Bearer <operator-token>" \
  http://127.0.0.1:8090/internal/metrics
```

Alert on:

- balance below `FAUCET_MIN_BALANCE_RESERVE_BASE_UNITS`;
- any outgoing faucet transaction without a matching claim and stored envelope;
- nonce movement outside the single worker sequence;
- signer/address-probe or credential-load failures;
- node validation failure, repeated timeout, or rejection;
- queue growth/age, retry spikes, or unresolved pending records;
- budget reservations approaching the daily cap;
- Turnstile/rate-limit collision spikes;
- unexpected service restarts.

The service intentionally exposes only coarse public availability. Exact balance, budget, queue, nonce, and signer key ID belong to the operator endpoint and protected logs.

## Kill switch and incident response

Disable intake immediately without editing source or placing a secret in browser code:

```bash
curl --fail --silent -X POST \
  -H "Authorization: Bearer <operator-token>" \
  http://127.0.0.1:8090/internal/disable
```

Then preserve the database and logs before investigation:

```bash
sudo systemctl stop ultranet-faucet.service
sudo tar --xattrs --acls --numeric-owner -C /var/lib \
  -czf /var/backups/ultranet/faucet-$(date -u +%Y%m%dT%H%M%SZ).tar.gz \
  ultranet-faucet
```

If signer compromise is suspected:

1. disable intake and stop the worker;
2. inspect every outgoing transaction from the faucet address;
3. compare hashes, nullifiers, nonces, and envelopes with the claim database;
4. quarantine/revoke the key credential;
5. create and fund a new dedicated faucet address through a separately approved ceremony;
6. restore the service from a known-good image and database backup;
7. update `FAUCET_ADDRESS` only after the new public-key derivation and balance checks pass;
8. keep the old address disabled and monitor it for late/unauthorized activity;
9. start a new canary with a new budget window.

Do not overwrite the old credential in place while unresolved transactions exist. A key rotation is an address migration and reconciliation exercise.

## Backup and restore

Back up the faucet SQLite database and policy/environment metadata separately from the validator database. Include the SQLite WAL safely by using the SQLite backup mechanism or stopping the service before archiving. Encrypt backups and keep the signer credential in a separate secret-management backup domain.

A restore is not complete until it preserves:

- idempotency keys and request fingerprints;
- active address cooldowns;
- reserved and confirmed daily debit;
- payout envelopes, hashes, nullifiers, and nonces;
- pending/confirmed/failed claim state;
- service enabled/kill-switch state;
- schema version and policy version.

Restore with intake disabled, verify the node chain/address/decimals/nonce, inspect all pending hashes, and run the canary gates before enabling again. Never restore the faucet database over `/var/lib/ultranet`.

## Controlled rollout

1. Deploy the binary and unit with `FAUCET_ENABLED=false`.
2. Run unit, mocked-node, signing-vector, migration, and filesystem-isolation checks.
3. Verify the production signer derives the dedicated address and never loads sovereign/admin credentials.
4. Keep the proxy route private while exercising `/internal/status` and reconciliation.
5. Enable one canary claim under operator observation.
6. Reconcile the exact node transaction hash, fee, nonce, source debit, and claim status.
7. Enable only the small configured daily cap; inspect queue, retries, confirmation latency, and abuse metrics.
8. Increase limits only after no duplicate debit, nonce drift, unexpected transfer, unresolved pending claim, or isolation failure is observed.

The default production state remains disabled if any signer, node, database, backup, reverse-proxy, monitoring, or incident-response gate is incomplete.
