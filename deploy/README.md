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

Set `ULTRANET_CORS_ORIGINS` to the exact HTTPS origin of the deployed Next.js site. Wildcards are rejected. Keep `ULTRANET_API_BIND=127.0.0.1:8081` for systemd deployments. Generate a separate administrator token with `openssl rand -hex 32` and set it as `ULTRANET_ADMIN_TOKEN`; never commit or expose that value to browser code.

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

Expose TCP/UDP `9000` to peers. Keep TCP `8081` closed in the VPS firewall; the reverse proxy should connect to `127.0.0.1:8081`.

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
