# Production deployment

**UltraNet creator and copyright holder:** Vladan Jotov  
**Deployment documentation license:** ISC License — see [`../LICENSE`](../LICENSE)

The production node keeps the Actix API private and exposes only the P2P port directly. Put the API behind a TLS reverse proxy such as Caddy or Nginx. [`Caddyfile.example`](./Caddyfile.example) contains a minimal API and Next.js proxy configuration.

## Shared environment

Create the service account before installing the environment file so the `ultranet` group can read it:

```bash
if ! id -u ultranet >/dev/null 2>&1; then
    sudo useradd --system --home-dir /var/lib/ultranet --shell /usr/sbin/nologin ultranet
fi
sudo install -d -o root -g ultranet -m 0750 /etc/ultranet
sudo install -o root -g ultranet -m 0640 deploy/ultranet.env.example /etc/ultranet/ultranet.env
sudoedit /etc/ultranet/ultranet.env
```

Set `ULTRANET_CORS_ORIGINS` to the exact HTTPS origin of the deployed Next.js site. Wildcards are rejected. Keep `ULTRANET_API_BIND=127.0.0.1:8081` for systemd deployments.

`ULTRANET_ADMIN_TOKEN` is a private administrator bearer token for state-changing node operations; it is not a wallet key, public node identifier, or browser login credential. Generate 32 random bytes locally with `openssl rand -hex 32`, and set the resulting 64-character hexadecimal value in `/etc/ultranet/ultranet.env`. This is the recommended format: current runtime validation requires at least 32 non-whitespace bytes, but does not yet enforce hexadecimal characters or exactly 64 characters. Restrict the file to the `root/ultranet` group. On Windows desktop packages, use the same command or the PowerShell generator in `release/windows/README-WINDOWS.txt` and store the value only in the private sibling `UltraNetNode.env`. Never commit, share, log, or expose the token to browser code. A missing or invalid token is a configuration error and prevents the API from starting before storage and cryptographic initialization.

The bearer token protects administrative/state-changing routes:

- `POST /api/mine`
- `POST /api/move/resource`
- `POST /api/state/prune`
- `POST /api/appchain/create`
- `POST /api/appchain/anchor`

Call protected routes with `Authorization: Bearer <ULTRANET_ADMIN_TOKEN>`. Missing, malformed, or incorrect tokens return `401 Unauthorized`. CORS preflight requests remain available, but the actual state-changing request must include the token. Wallet-signed transaction and governance proposal/approval endpoints retain their existing Dilithium verification contract and are not replaced by this node-admin token.

### UltraWallet browser authentication

The `/login` page uses a short-lived Dilithium challenge instead of accepting an access token in browser code. Configure `ULTRANET_AUTHORIZED_NODE_IDENTIFIERS` with the exact 64-character identifiers derived from approved wallet public keys. The flow is:

1. The browser requests `POST /api/auth/challenge` with the node identifier.
2. UltraWallet signs the canonical challenge locally; the private key never leaves the wallet.
3. The browser submits the public key and signature to `POST /api/auth/login`.
4. The node verifies the signature, derived identifier, allowlist, expiry, and one-time challenge, then creates a Sled-backed session.
5. The node sets a `Secure`, `HttpOnly`, `SameSite=Lax` session cookie plus a readable CSRF cookie. Unsafe session-authenticated requests must mirror the CSRF value in `X-UltraNet-CSRF`.

Set `ULTRANET_SESSION_COOKIE_SECURE=true` for HTTPS production. Set it to `false` only for local HTTP development. When the dashboard and API use sibling HTTPS subdomains, set `ULTRANET_AUTH_COOKIE_DOMAIN=ultranetwork.cc` so the HttpOnly session cookie and readable CSRF cookie are shared across those subdomains; leave it unset for single-origin/local deployments. Browser login is intentionally disabled when the allowlist is empty; bearer-token automation remains available. Never put `ULTRANET_ADMIN_TOKEN`, session tokens, or private keys in frontend source, local storage, URLs, or deployment artifacts.

### Persistent validator dials

Set `ULTRANET_PERSISTENT_PEERS` when validators must maintain application-level connections instead of relying only on short-lived Kademlia discovery queries. Use a comma-separated list of complete libp2p multiaddresses, including the remote peer ID:

```dotenv
ULTRANET_PERSISTENT_PEERS=/ip4/203.0.113.10/tcp/9000/p2p/12D3KooW...
```

Configure reciprocal addresses on both validators when using a two-node topology. The value is a public routing setting, not a secret, but it must contain the peer ID from the target node's startup log. The node stores its libp2p Ed25519 identity at `ULTRANET_DB_PATH/p2p_identity.key` with owner-only permissions, so the peer ID remains stable across service restarts. If that file is intentionally deleted or rotated, regenerate both reciprocal addresses before restarting the peers. Each configured target is dialed at startup, kept alive with Ping, and retried with bounded backoff after the final connection closes. Kademlia discovery remains enabled for broader discovery and is not itself treated as a persistent validator relationship.

Inspect the policy with:

```bash
journalctl -u ultranet.service -f | grep -Ei "Persistent|connection established|connection closed|Ping|PeerManager"
```

### Offline CLI signing

For operators who do not use UltraWallet, build `ultranet-auth` on the offline signing machine:

```bash
cargo build --release --locked --bin ultranet-auth
./target/release/ultranet-auth sign-challenge \
  --api-base-url https://api.ultranetwork.cc \
  --keys /offline/sovereign_keys.json \
  --key-index 0 \
  --output /offline/auth-login-payload.json
```

The CLI keeps the Dilithium private key local, requests a fresh challenge, and writes the public `POST /api/auth/login` JSON payload. It accepts both the generated hex-encoded `sovereign_keys.json` format and the local owner backup format with byte-array `public_key` and `private_key` fields. Submit that payload with a private cookie jar or import it through the `CLI_SIGNED_PAYLOAD` mode on the browser `/login` page, as documented in [`../CLI_AUTH_SIGNING.md`](../CLI_AUTH_SIGNING.md). Do not copy the key file or the CLI signing binary to the VPS. The browser import accepts only the public login fields, keeps the pasted payload in memory, and clears it after a successful session.

## Validator-only web approval

The Join Swarm dashboard has a protected pending-approval surface, but the web flow is disabled unless the isolated signer boundary is explicitly provisioned. Web approval is not an online copy of `sovereign_keys.json`:

```text
validator wallet session
        │  review + exact hash confirmation
        ▼
UltraNet approval gateway (public API; no private key)
        │  private Unix socket per owner
        ├── owner-0 signer process / HSM
        ├── owner-1 signer process / HSM
        └── owner-2 signer process / HSM
        │  two distinct verified signatures
        ▼
existing node ValidatorApproval verifier + durable approval journal
```

The gateway and browser never receive a Sovereign secret key. The checked-in `ultranet-approval-signer` file adapter is a bootstrap/local-agent implementation that keeps one private owner key in a separate process and requires local `APPROVE` presence by default. For production, replace that adapter with an HSM or separately administered signer host; do not enable unattended file signing merely to remove a local presence check.

The checked-in systemd units use one socket and one Unix group per owner. `ultranet` receives only the three socket groups through a drop-in; it does not receive read permission on any signer key directory. The signer service adopts the descriptor created by its matching socket unit, so the signer cannot replace or broaden the gateway socket path:

```text
/run/ultranet-approval-signer/owner-0/approval.sock  owner-0 group
/run/ultranet-approval-signer/owner-1/approval.sock  owner-1 group
/run/ultranet-approval-signer/owner-2/approval.sock  owner-2 group
```

Enable the feature only after the signer, role mapping, backup, recovery, and socket-ACL procedure have passed a staging review. Install the service and group contracts first:

```bash
cargo build --release --locked --bin UltraNet --bin ultranet-approval-signer
sudo install -o root -g root -m 0755 target/release/UltraNet /opt/ultranet/target/release/UltraNet
sudo install -o root -g root -m 0755 target/release/ultranet-approval-signer /opt/ultranet/target/release/ultranet-approval-signer

for owner in 0 1 2; do
  sudo getent group "ultranet-approval-owner-$owner" >/dev/null || \
    sudo groupadd --system "ultranet-approval-owner-$owner"
  if ! id -u "ultranet-approver-$owner" >/dev/null 2>&1; then
    sudo useradd --system --gid "ultranet-approval-owner-$owner" \
      --home-dir "/var/lib/ultranet-approval-signer/owner-$owner" \
      --shell /usr/sbin/nologin "ultranet-approver-$owner"
  else
    sudo usermod --gid "ultranet-approval-owner-$owner" "ultranet-approver-$owner"
  fi
  sudo install -d -o "ultranet-approver-$owner" -g "ultranet-approval-owner-$owner" -m 0700 \
    "/var/lib/ultranet-approval-signer/owner-$owner"
done

sudo install -d -o root -g ultranet -m 0750 /etc/ultranet
sudo install -o root -g ultranet -m 0640 deploy/sovereign-owner-auth.example.json /etc/ultranet/sovereign-owner-auth.json
sudo install -o root -g root -m 0644 deploy/ultranet-approval-signer@.service /etc/systemd/system/ultranet-approval-signer@.service
sudo install -o root -g root -m 0644 deploy/ultranet-approval-signer@.socket /etc/systemd/system/ultranet-approval-signer@.socket
sudo install -o root -g root -m 0644 deploy/ultranet-approval-signer.tmpfiles /etc/tmpfiles.d/ultranet-approval-signer.conf
sudo install -d -o root -g root -m 0755 /etc/systemd/system/ultranet.service.d
sudo install -o root -g root -m 0644 deploy/ultranet.service.d/approval-sockets.conf /etc/systemd/system/ultranet.service.d/approval-sockets.conf
sudoedit /etc/ultranet/sovereign-owner-auth.json
sudo chmod 0640 /etc/ultranet/sovereign-owner-auth.json
sudo systemd-tmpfiles --create /etc/tmpfiles.d/ultranet-approval-signer.conf
sudo systemctl daemon-reload
for owner in 0 1 2; do
  sudo systemctl enable --now "ultranet-approval-signer@${owner}.socket"
done
sudo systemctl restart ultranet.service
```

For staging-only transport validation, install exactly one private key record into each owner directory with mode `0600`, owned by that owner’s signer account. Never use this file-backed step on production when an HSM or separately administered signer host is required. Do not copy `sovereign_keys.json` containing all owners to the node.

Each owner binding must use a unique absolute socket and signer ID. The file contains only non-secret session-to-signer mapping data:

```json
[
  {
    "owner_index": 0,
    "session_node_identifier": "<owner-session-wallet-address>",
    "signer_id": "owner-0",
    "signer_socket": "/run/ultranet-approval-signer/owner-0/approval.sock"
  },
  {
    "owner_index": 1,
    "session_node_identifier": "<owner-session-wallet-address>",
    "signer_id": "owner-1",
    "signer_socket": "/run/ultranet-approval-signer/owner-1/approval.sock"
  },
  {
    "owner_index": 2,
    "session_node_identifier": "<owner-session-wallet-address>",
    "signer_id": "owner-2",
    "signer_socket": "/run/ultranet-approval-signer/owner-2/approval.sock"
  }
]
```

Set both session allowlists. The identifiers must also be present in `ULTRANET_AUTHORIZED_NODE_IDENTIFIERS` so they can establish a wallet session:

```dotenv
ULTRANET_WEB_APPROVAL_ENABLED=true
ULTRANET_AUTHORIZED_NODE_IDENTIFIERS=<owner-0-session>,<owner-1-session>,<owner-2-session>
ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS=<owner-0-session>,<owner-1-session>,<owner-2-session>
ULTRANET_SOVEREIGN_OWNER_AUTH_FILE=/etc/ultranet/sovereign-owner-auth.json
ULTRANET_APPROVAL_SIGNER_TIMEOUT_SECONDS=20
ULTRANET_APPROVAL_INTENT_TTL_SECONDS=600
```

Run the staging preflight before enabling the feature. The full gate, canary, rollback, and production sign-off procedure is in [`../docs/APPROVAL_STAGING_PREFLIGHT.md`](../docs/APPROVAL_STAGING_PREFLIGHT.md). It has a static mode for CI/review and a host mode for a configured staging node:

```bash
bash scripts/check_approval_staging.sh --static
sudo bash scripts/check_approval_staging.sh --api-base-url https://api-staging.example.com
```

Host mode must pass as root and verifies the exact environment-file/mapping ownership, three unique owner accounts and groups, `0660` socket ownership, `ultranet` connectivity without key readability, socket activation, node health, and the unauthenticated review-route boundary. It intentionally does not approve a proposal. Perform one controlled staging canary with two distinct owners after the preflight; record the exact hash, signer audit events, node journal, activation response, and rollback result.

Run three independently permissioned signer sockets, one per owner index. Do not use one process/socket for all three private keys. The checked-in file adapter still requires the local signer operator to type `APPROVE`; because a systemd service has no interactive terminal, a production deployment must replace the file adapter with the audited HSM/local-presence adapter before live approvals. If the signer is unavailable, the dashboard must report `SIGNER_UNAVAILABLE` and no node approval is submitted.

The browser flow is deliberately limited to a review and exact-hash confirmation. After the first signer records a valid public signature, the second distinct owner repeats the confirmation. The gateway automatically combines exactly two public signatures and submits the existing version-3 payload. `POST /api/governance/approve` remains the final node verifier and the offline `ultranet-approve` procedure remains the break-glass path.

The approval gateway persists only short-lived intent state, public signatures, nonce reservations, public owner identity, stage, and audit outcomes. It must not persist or log private keys, wallet passwords, `ULTRANET_ADMIN_TOKEN`, signer credentials, or raw request dumps. Keep the signer socket off Caddy, Cloudflare, public DNS, and the dashboard's public network surface. The socket is local-only; only the node service account reaches it through the per-owner supplementary groups. Do not add a reverse-proxy route or TCP listener for any signer.

## AppChain registry migration

The first AppChain prototype stored four-field configs and anchors without a treasury, fee, anchor number, or proof metadata. Run the migrator once before starting the production binary against an existing database. It refuses to run while `ultranet.service` or an `UltraNet` process is active, writes raw `appchain_configs` and `appchain_anchors` records first, and is a dry run unless `--apply` is supplied.

Build the node and migration utility from the same source revision, install both binaries, stop the service, take a complete database archive, then apply the registry migration:

```bash
cargo build --release --locked --bin UltraNet --bin ultranet-appchain-migrate
sudo install -o root -g root -m 0755 target/release/UltraNet /opt/ultranet/target/release/UltraNet
sudo install -o root -g root -m 0755 target/release/ultranet-appchain-migrate /opt/ultranet/target/release/ultranet-appchain-migrate
sudo systemctl stop ultranet.service

BACKUP_ROOT="/var/backups/ultranet/appchain-$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -o root -g root -m 0700 "$BACKUP_ROOT"
sudo tar --xattrs --acls --numeric-owner -C /var/lib -czf "$BACKUP_ROOT/ultranet-db.tar.gz" ultranet
sudo /opt/ultranet/target/release/ultranet-appchain-migrate \
  --db-path /var/lib/ultranet \
  --backup-dir "$BACKUP_ROOT/registry" \
  --apply

sudo systemctl daemon-reload
sudo systemctl restart ultranet.service
sudo systemctl --no-pager --full status ultranet.service
curl --fail http://127.0.0.1:8081/api/stats
```

The raw registry backup contains `manifest.json` plus JSONL records with the exact original Sled key/value bytes. Legacy anchors are retained as `is_test=true` with `fee_charged=0`; the old schema had no treasury debit contract, so the migration never invents historical production spend. Keep both the raw registry backup and full database archive until post-restart verification is complete. The checked-in wrapper [`../scripts/migrate_appchain_registry.sh`](../scripts/migrate_appchain_registry.sh) invokes the installed release migrator and supports the same dry-run/`--apply` flow.

## Isolated faucet service

The faucet is a separate low-privilege process. It must not use `send_ultra.py`, `sovereign_keys.json`, the validator Sled database, or `ULTRANET_ADMIN_TOKEN`. It submits only normal wallet-signed transfers through the public `POST /api/transaction` boundary. See [`../docs/FAUCET_TECHNICAL_SPEC.md`](../docs/FAUCET_TECHNICAL_SPEC.md) and [`../docs/FAUCET_OPERATIONS.md`](../docs/FAUCET_OPERATIONS.md).

Build the separate binary from the same source revision as the node:

```bash
cargo build --release --locked --bin ultranet-faucet
sudo useradd --system --home-dir /var/lib/ultranet-faucet --shell /usr/sbin/nologin ultranet-faucet
sudo install -d -o ultranet-faucet -g ultranet-faucet -m 0700 /var/lib/ultranet-faucet
sudo install -d -o root -g ultranet-faucet -m 0750 /etc/ultranet-faucet
sudo install -o root -g ultranet-faucet -m 0640 deploy/faucet.env.example /etc/ultranet-faucet/faucet.env
sudo install -d -o root -g root -m 0750 /opt/ultranet-faucet
sudo install -o root -g root -m 0755 target/release/ultranet-faucet /opt/ultranet-faucet/ultranet-faucet
sudo install -o root -g root -m 0644 deploy/ultranet-faucet.service /etc/systemd/system/ultranet-faucet.service
```

Install the encrypted credentials referenced by `deploy/ultranet-faucet.service` through the host's secret-management process. The faucet environment file contains no private key, CAPTCHA secret, operator token, or `ULTRANET_ADMIN_TOKEN`. Keep `FAUCET_ENABLED=false` until the isolated signer, separate SQLite backup, private proxy route, monitoring, and canary procedure have been verified. The full provisioning, preflight, kill-switch, backup, and rotation procedure is in [`../docs/FAUCET_OPERATIONS.md`](../docs/FAUCET_OPERATIONS.md).

### Faucet UI/API origin and CORS

Use `https://faucet.ultranetwork.cc` as the canonical faucet origin. The repository currently contains no faucet UI, so the installed Caddy route is API-only and the existing dashboard setting `NEXT_PUBLIC_API_BASE_URL=https://api.ultranetwork.cc` must not be changed for faucet work.

The preferred future design is same-origin: serve the faucet UI at `faucet.ultranetwork.cc`, route only `/api/faucet/status`, `/api/faucet/claims`, and `/api/faucet/claims/*` to `127.0.0.1:8090`, and route the UI shell to its own explicitly approved frontend target. With that arrangement, the browser needs no CORS headers, no wildcard origin, and no credentials. The faucet API must not be folded into the general node API or dashboard route.

If a future product decision requires a cross-origin UI, add one explicit `Access-Control-Allow-Origin` value and an explicit `OPTIONS` preflight route at the proxy. Allow only `GET`, `POST`, `Content-Type`, and `Idempotency-Key`; do not allow credentials or `*`. Do not add the faucet origin to the node's CORS allowlist unless the browser is intentionally calling the node directly.

The route must remain behind the upstream WAF/bot-control layer. The Caddy 2.6.2 route uses its per-upstream `trusted_proxies` CIDR list and rebuilds `X-Forwarded-For` from trusted proxy metadata; it strips client-supplied forwarding headers and does not copy raw `CF-Connecting-IP`. Keep origin HTTP/HTTPS access restricted to the WAF's published ranges or a tunnel before treating that identity as trusted.

The faucet Turnstile widget is restricted to the exact hostname `faucet.ultranetwork.cc` and action `faucet_claim`. The server-side Siteverify response must report `success=true` with both values before a request can pass CAPTCHA validation. Keep the secret only in the encrypted systemd credential; the public widget uses only the site key.

### Coordinated Cloudflare origin lockdown

Proxy both `api.ultranetwork.cc` and `faucet.ultranetwork.cc` in Cloudflare before removing broad VPS web ingress. The API hostname must not remain DNS-only when the origin firewall is tightened. The checked-in [`CLOUDFLARE_RULES.md`](./CLOUDFLARE_RULES.md) contains the managed-WAF, faucet path/method, cache, Turnstile, rate-limit, canary, and rollback policy.

The guarded [`cloudflare-origin-lockdown.sh`](./cloudflare-origin-lockdown.sh) procedure downloads and validates the official Cloudflare IPv4/IPv6 ranges, verifies both names resolve only to those ranges, snapshots UFW state, and adds Cloudflare-only TCP 80/443 rules. It never modifies SSH/P2P rules or Cloudflare dashboard settings:

```bash
sudo bash deploy/cloudflare-origin-lockdown.sh --check
sudo bash deploy/cloudflare-origin-lockdown.sh --apply
```

The first apply retains existing broad rules. After public HTTPS checks pass and a separate SSH recovery path is confirmed, remove the broad rules explicitly:

```bash
sudo env CLOUDFLARE_LOCKDOWN_CONFIRM=I_UNDERSTAND \\
  bash deploy/cloudflare-origin-lockdown.sh --apply --remove-broad
```

Do not run the removal step while `api.ultranetwork.cc` is DNS-only. Keep the faucet disabled until the origin firewall, WAF, Turnstile, monitoring, and canary gates are complete.

## systemd

Install the release binary under `/opt/ultranet/target/release/UltraNet`, copy `public/` to `/opt/ultranet/public`, and run it as a dedicated `ultranet` user.

```bash
sudo install -d -o ultranet -g ultranet /opt/ultranet/target/release /opt/ultranet/public /var/lib/ultranet
sudo install -m 0755 target/release/UltraNet /opt/ultranet/target/release/UltraNet
sudo cp -a public/. /opt/ultranet/public/
sudo install -o root -g root -m 0644 deploy/ultranet.service /etc/systemd/system/ultranet.service
sudo systemctl daemon-reload
sudo systemctl enable --now ultranet
curl --fail http://127.0.0.1:8081/api/stats
```

### VPS memory safety

The production validator service includes a containment profile for the current 8 GiB VPS: `MemoryHigh=6912M`, `MemoryMax=7168M`, and `MemorySwapMax=4096M`. The 7 GiB hard limit keeps a runaway node cgroup from consuming the whole host; 32 GB RAM remains the recommended capacity for sustained validator workloads. Swap is emergency headroom, not a replacement for adequate RAM.

Install the 4 GiB persistent swap file and the systemd memory monitor once as root:

```bash
if ! sudo swapon --show=NAME --noheadings | grep -qx '/swapfile'; then
  if [[ ! -e /swapfile ]]; then
    sudo fallocate -l 4G /swapfile
    sudo chmod 0600 /swapfile
    sudo mkswap /swapfile
  else
    sudo chmod 0600 /swapfile
    sudo swaplabel /swapfile >/dev/null
  fi
  sudo swapon /swapfile
fi
if ! grep -qE '^[[:space:]]*/swapfile[[:space:]]' /etc/fstab; then
  printf '%s\n' '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
fi
sudo install -o root -g root -m 0755 deploy/ultranet-memory-monitor.sh /usr/local/sbin/ultranet-memory-monitor
sudo install -o root -g root -m 0644 deploy/ultranet-memory-monitor.service /etc/systemd/system/ultranet-memory-monitor.service
sudo install -o root -g root -m 0644 deploy/ultranet-memory-monitor.timer /etc/systemd/system/ultranet-memory-monitor.timer
sudo install -o root -g root -m 0644 deploy/ultranet-memory-sysctl.conf /etc/sysctl.d/99-ultranet-memory.conf
sudo sysctl -p /etc/sysctl.d/99-ultranet-memory.conf
sudo systemctl daemon-reload
sudo systemctl enable --now ultranet-memory-monitor.timer
sudo systemctl restart ultranet.service
```

The monitor records cgroup current/peak memory, swap usage, soft-limit events, and cgroup OOM counters once per minute in the journal:

```bash
journalctl -t ultranet-memory-monitor -f
systemctl show ultranet.service -p MemoryCurrent -p MemoryPeak -p MemoryHigh -p MemoryMax -p MemorySwapMax -p NRestarts
swapon --show
```

The validator uses a 15-second restart delay and a maximum of three starts in five minutes. This preserves automatic recovery for transient failures while stopping a persistent crash/configuration loop from repeatedly consuming CPU and memory.

Expose TCP/UDP `9000` to peers. Keep TCP `8081` closed in the VPS firewall; the reverse proxy should connect to `127.0.0.1:8081`. If `ULTRANET_DB_PATH` is omitted outside systemd/Docker, the node selects the per-user data directory documented in [`../README.md`](../README.md); an explicit path remains authoritative.

## Windows desktop package

The Windows x64 maintenance package is launcher-first. Extract the complete archive, copy `UltraNetNode.env.example` to `UltraNetNode.env`, create the private administrator token, and double-click `Start-UltraNetNode.bat`. The launcher sets the sibling env-file path, runs `--check-config` and `--check-fhe`, and pauses only for an interactive desktop failure. It uses `%LOCALAPPDATA%\\UltraNet\\data` by default. See [`../release/windows/README-WINDOWS.txt`](../release/windows/README-WINDOWS.txt) for checksum verification, PowerShell token generation, firewall guidance, and log collection. Do not use the desktop env file as a systemd `EnvironmentFile` or put its token into Docker/Next.js configuration.

## Docker Compose

Copy the environment file first, then start the production compose file:

```bash
sudo install -d -m 0750 /etc/ultranet
sudo install -m 0640 deploy/ultranet.env.example /etc/ultranet/ultranet.env
$EDITOR /etc/ultranet/ultranet.env
docker compose -f deploy/docker-compose.production.yml up -d --build
```

The Compose configuration publishes the API only on host loopback and persists the database in the `ultranet_mainnet_data` volume. The container still binds its internal API listener to `0.0.0.0:8081` so the loopback port mapping can reach it.

## Frontend

Build and run the Next.js site separately. Set `NEXT_PUBLIC_API_BASE_URL` to the public TLS API origin used by the browser, not to `localhost`.

```bash
cd website
npm ci
NEXT_PUBLIC_API_BASE_URL=https://api.example.com NEXT_PUBLIC_EXPLORER_URL=https://api.example.com/dashboard npm run build
npm run start -- --hostname 127.0.0.1 --port 3000
```

The `prebuild` and `predev` hooks generate `public/docs/ultranet-whitepaper.html` from the canonical `ULTRA_NET_TECHNICAL_GUIDE.md` at the repository root. Keep that source file at `/opt/ultranet/ULTRA_NET_TECHNICAL_GUIDE.md` beside the deployed `website/` directory; the existing `public/docs/ultranet-whitepaper.pdf` remains the downloadable export.

For a persistent systemd dashboard, copy [`ultranet-dashboard.service`](./ultranet-dashboard.service) to `/etc/systemd/system/` and create `/etc/ultranet/website.env` from [`website.env.example`](./website.env.example). Build with the public API variables present because Next.js embeds `NEXT_PUBLIC_*` values during `npm run build`. The build also needs frontend devDependencies such as TypeScript, so use `npm ci --include=dev`; keep `NODE_ENV=production` for the service runtime:

```bash
sudo install -o root -g root -m 0644 deploy/ultranet-dashboard.service /etc/systemd/system/ultranet-dashboard.service
sudo install -o root -g ultranet -m 0640 deploy/website.env.example /etc/ultranet/website.env
cd website
set -a
. /etc/ultranet/website.env
set +a
npm ci --include=dev
npm run build
sudo systemctl daemon-reload
sudo systemctl enable --now ultranet-dashboard.service
```

The dashboard service listens only on `127.0.0.1:3000`; Caddy exposes it at `https://dashboard.ultranetwork.cc`. Never copy `sovereign_keys.json` into the VPS, container build context, image, or frontend deployment. The repository `.dockerignore` excludes it as a second line of defense; keep the offline sovereign key ceremony outside the node host.

## Automated frontend deployment

`.github/workflows/deploy-website.yml` builds and deploys the Next.js dashboard when `main` changes under `website/`, `deploy/README.md`, `deploy/website.env.example`, or `ULTRA_NET_TECHNICAL_GUIDE.md`. It also supports a manual dispatch with an explicit 40-character commit SHA. Rust-only changes do not restart the dashboard.

The workflow uses the GitHub `production` environment and these secrets:

- `ULTRANET_DEPLOY_HOST` — production host name or address.
- `ULTRANET_DEPLOY_USER` — must be `ultranet-deploy`.
- `ULTRANET_DEPLOY_SSH_KEY` — private Ed25519 key whose public half is installed for `ultranet-deploy`.
- `ULTRANET_DEPLOY_KNOWN_HOSTS` — pinned `known_hosts` entry for the production SSH host.

Do not put these values in the repository, workflow YAML, frontend environment files, or public logs. The workflow requires strict host-key checking and never falls back to `StrictHostKeyChecking=no`. If validation fails, the workflow reports only the missing secret names and never prints secret values.

### One-time VPS bootstrap

Run as `root` through the existing administrative SSH path. Use a dedicated key for GitHub Actions; do not reuse a personal root key:

```bash
useradd --create-home --home-dir /home/ultranet-deploy --shell /bin/bash --comment "UltraNet frontend deploy" ultranet-deploy
install -d -o ultranet-deploy -g ultranet-deploy -m 0700 /home/ultranet-deploy/.ssh
install -d -o ultranet-deploy -g ultranet-deploy -m 0750 /var/lib/ultranet-deploy/staging
install -d -o ultranet -g ultranet -m 0750 /opt/ultranet/releases
install -o root -g root -m 0755 deploy/ultranet-dashboard-deploy.sh /usr/local/sbin/ultranet-dashboard-deploy
install -o root -g root -m 0440 deploy/ultranet-deploy-sudoers.example /etc/sudoers.d/ultranet-deploy-dashboard
visudo -cf /etc/sudoers.d/ultranet-deploy-dashboard
```

Install the GitHub Actions public key in `/home/ultranet-deploy/.ssh/authorized_keys` with these options before the `ssh-ed25519` key: `no-agent-forwarding,no-port-forwarding,no-X11-forwarding,no-pty`. The account owns only the upload staging directory and can run only `/usr/local/sbin/ultranet-dashboard-deploy` through the installed sudo rule. The helper rejects unsafe archive members, runs the server-side `npm ci --include=dev`, lint, type-check, and build, verifies the generated whitepaper, keeps one rollback release, and restarts only `ultranet-dashboard.service`.

The deployment archive contains only the website source and `ULTRA_NET_TECHNICAL_GUIDE.md`; it excludes `.git`, `node_modules`, `.next`, environment files, keys, databases, and Rust build outputs. The canonical Markdown source is installed at `/opt/ultranet/ULTRA_NET_TECHNICAL_GUIDE.md` so future server builds can regenerate the HTML reader.

### Manual redeploy and rollback

To redeploy a specific commit after the production secrets are configured, use the workflow's **Run workflow** action and enter its full commit SHA. The server helper builds and verifies the candidate before swapping `/opt/ultranet/website`. If the dashboard restart, local whitepaper routes, PDF route, validator state, or generated asset checks fail, it restores the previous website and restarts the prior dashboard release. `ultranet.service` is never restarted by this workflow.

Keep the previous release directory until the public HTTPS verification passes. Remove old `/opt/ultranet/releases/previous-*` and `/opt/ultranet/releases/failed-*` directories only during an explicit maintenance cleanup after reviewing their contents.
