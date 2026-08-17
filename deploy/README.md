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

`ULTRANET_ADMIN_TOKEN` is a private administrator bearer token for state-changing node operations; it is not a wallet key, public node identifier, or browser login credential. Generate 32 random bytes locally with `openssl rand -hex 32`, set the resulting 64-character hexadecimal value in `/etc/ultranet/ultranet.env`, and restrict the file to the `root/ultranet` group. On Windows desktop packages, use the same command or the PowerShell generator in `release/windows/README-WINDOWS.txt` and store the value only in the private sibling `UltraNetNode.env`. Never commit, share, log, or expose the token to browser code. A missing or invalid token is a configuration error and prevents the API from starting before storage and cryptographic initialization.

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

Do not put these values in the repository, workflow YAML, frontend environment files, or public logs. The workflow requires strict host-key checking and never falls back to `StrictHostKeyChecking=no`.

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
