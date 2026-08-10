# UltraNet v7.1 "Sovereign" Mainnet

[![Network Status](https://img.shields.io/badge/Mainnet-Online-emerald?style=flat-square)](http://localhost:8081)
[![Version](https://img.shields.io/badge/Protocol-v7.1--Sovereign-sky?style=flat-square)](./TECHNICAL_MANIFEST.md)

UltraNet is a high-performance L1 blockchain engineered for 100-year longevity. It combines **Post-Quantum Cryptography (Dilithium-5)**, **Parallel Execution (Block-STM)**, and **Recursive ZK-SNARKs** to provide institutional-grade security and sub-millisecond finality.

## Authorship and License

UltraNet's original code, protocol design, and whitepaper documentation were created by and are copyright © 2026 **Vladan Jotov**. Unless a file or dependency states otherwise, the original UltraNet materials are distributed under the **ISC License** in [`LICENSE`](./LICENSE). Third-party dependencies remain under their respective licenses.

## ⚡ Production Node Deployment
Production deployment files live in [`deploy/`](./deploy/). The API defaults to `127.0.0.1:8081` and must be placed behind a TLS reverse proxy. Only the P2P port should be exposed publicly.

```bash
# Clone and prepare configuration
git clone https://github.com/your-repo/UltraNet_Linux.git
cd UltraNet_Linux
if ! id -u ultranet >/dev/null 2>&1; then sudo useradd --system --home-dir /var/lib/ultranet --shell /usr/sbin/nologin ultranet; fi
sudo install -d -o root -g ultranet -m 0750 /etc/ultranet
sudo install -o root -g ultranet -m 0640 deploy/ultranet.env.example /etc/ultranet/ultranet.env
sudoedit /etc/ultranet/ultranet.env
```

Choose one runtime:

```bash
# systemd: install the release binary, then use deploy/ultranet.service
cargo build --release --locked
if ! id -u ultranet >/dev/null 2>&1; then sudo useradd --system --home-dir /var/lib/ultranet --shell /usr/sbin/nologin ultranet; fi
sudo install -d -o ultranet -g ultranet /opt/ultranet/target/release /opt/ultranet/public /var/lib/ultranet
sudo install -o ultranet -g ultranet -m 0755 target/release/UltraNet /opt/ultranet/target/release/UltraNet
sudo cp -a public/. /opt/ultranet/public/
sudo install -o root -g root -m 0644 deploy/ultranet.service /etc/systemd/system/ultranet.service
sudo systemctl daemon-reload && sudo systemctl enable --now ultranet

# or Docker Compose
docker compose -f deploy/docker-compose.production.yml up -d --build
```

**Production networking:**
- **P2P**: expose TCP/UDP port `9000` (Mainnet)
- **API**: keep `8081` private and proxy it through HTTPS
- **Browser CORS**: set `ULTRANET_CORS_ORIGINS` to the exact frontend origin
- **Admin API**: set `ULTRANET_ADMIN_TOKEN` from `openssl rand -hex 32`; never expose it to browser code

For the complete runbook, including protected state-changing routes, see [`deploy/README.md`](./deploy/README.md).

**Local dashboard:** `http://127.0.0.1:8081/dashboard`

## 🏗️ Technical Architecture
- **Consensus**: Bullshark / Mysticeti Directed Acyclic Graph (DAG).
- **Execution**: 16-way Parallel Merkle Patricia Trie (MPT) Shards.
- **Smart Contracts**: Move VM (Resource-oriented logic).
- **Security**: 2-of-3 Sovereign Multi-Sig Shield for Genesis funds.

## 👥 Joining the Network
Prospective validators should follow the [**Validator Onboarding Portal**](http://localhost:8081/dashboard) directly in the node dashboard.

**Documentation:**
- [**`AUTHORS.md`**](./AUTHORS.md): Authorship, provenance, and licensing scope.
- [**`ULTRA_NET_TECHNICAL_GUIDE.md`**](./ULTRA_NET_TECHNICAL_GUIDE.md): The UltraNet technical whitepaper.
- [**`VALIDATOR_GUIDE.md`**](./VALIDATOR_GUIDE.md): Setup, staking, and maintenance.
- [**`ULTRAWALLET_INTEGRATION.md`**](./ULTRAWALLET_INTEGRATION.md): Browser wallet provider and validator proposal contract.
- [**`CLI_AUTH_SIGNING.md`**](./CLI_AUTH_SIGNING.md): Offline CLI authentication signing, API login, and browser import workflow.
- [**`TECHNICAL_MANIFEST.md`**](./TECHNICAL_MANIFEST.md): In-depth protocol constraints.
- [**`GENESIS_REPORT.md`**](./GENESIS_REPORT.md): Initial supply and allocation audit.

## 🛡️ Security First
- **No Private Keys on Server**: All sovereign operations require 2-of-3 signatures signed offline.
- **Zero-Knowledge Enforcement**: Mandatory execution traces for all Move VM state transitions.
- **Automatic Slashing**: Protocol-level jailing for state root non-determinism.

---
*UltraNet: The permanent infrastructure for autonomous finance.*
