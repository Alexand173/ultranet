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
git clone https://github.com/Alexand173/ultranet.git
cd ultranet
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

# On the current small VPS, apply the swap and memory-monitor setup from deploy/README.md.
# The production unit contains a 7 GiB cgroup ceiling and 4 GiB emergency swap allowance.

# or Docker Compose
docker compose -f deploy/docker-compose.production.yml up -d --build
```

## 📦 Published Node Release

UltraNet `v7.1.6` is published with precompiled x86_64 node binaries. Use the [GitHub release page](https://github.com/Alexand173/ultranet/releases/tag/v7.1.6) as the source of truth for the release notes and assets. Configure the node environment before starting it.

| Platform | Direct download | Archive contents |
| :--- | :--- | :--- |
| Windows x64 | [`UltraNetNode-windows-x64.zip`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-windows-x64.zip) | `UltraNetNode.exe` plus launcher and configuration template |
| Linux x64 | [`UltraNetNode-linux-x64.tar.gz`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-linux-x64.tar.gz) | `UltraNetNode` |
| macOS x64 (Intel) | [`UltraNetNode-macos-x64.tar.gz`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-macos-x64.tar.gz) | `UltraNetNode` |

Download the [published `SHA256SUMS.txt` manifest](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/SHA256SUMS.txt) beside the archive and verify it before extracting or executing the binary. The manifest lists all three archives, so the Linux command below ignores entries for files you did not download:

```bash
# Linux (GNU coreutils)
sha256sum --ignore-missing --check SHA256SUMS.txt
```

On macOS, calculate the archive hash and compare it with the matching line in the manifest:

```bash
shasum -a 256 UltraNetNode-macos-x64.tar.gz
grep 'UltraNetNode-macos-x64.tar.gz$' SHA256SUMS.txt
```

On Windows PowerShell:

```powershell
$expected = (Get-Content .\SHA256SUMS.txt | Where-Object { $_ -match 'UltraNetNode-windows-x64\.zip$' }).Split()[0]
$actual = (Get-FileHash .\UltraNetNode-windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
"Checksum OK"
```

After verification, extract the archive and start the node with your configured environment:

```bash
# Linux
tar -xzf UltraNetNode-linux-x64.tar.gz
chmod +x UltraNetNode
./UltraNetNode
```

```bash
# macOS (Intel)
tar -xzf UltraNetNode-macos-x64.tar.gz
chmod +x UltraNetNode
./UltraNetNode
```

```powershell
# Windows PowerShell desktop package
Expand-Archive .\UltraNetNode-windows-x64.zip -DestinationPath .
Copy-Item .\UltraNetNode.env.example .\UltraNetNode.env
notepad .\UltraNetNode.env
.\Start-UltraNetNode.bat
```

The published `v7.1.6` Windows archive contains `UltraNetNode.exe`,
`Start-UltraNetNode.bat`, `UltraNetNode.env.example`, and
`README-WINDOWS.txt`. Copy the example to `UltraNetNode.env`, create the
private `ULTRANET_ADMIN_TOKEN` described below, and launch the batch file first.
It runs `--check-config` and `--check-fhe` before starting the node, uses the
writable per-user `%LOCALAPPDATA%\\UltraNet\\data` default, and keeps an
interactive failure visible. The `v7.1.6` release pipeline completed
successfully for Linux, macOS, Windows, and published asset-contract
verification.

Do not extract or run an archive if its checksum does not match. For non-x86_64 systems, or when you need a source build, follow the compilation instructions in [`VALIDATOR_GUIDE.md`](./VALIDATOR_GUIDE.md).

### AppChain management and test anchoring

AppChain creation and anchoring are authenticated operator operations. The `/api/appchain/overview` endpoint exposes registry metadata, the dedicated L1 treasury address, live treasury balance, and cumulative protocol debits. `POST /api/appchain/create` creates a durable registry record and derives a unique treasury address; the treasury starts at zero and must be funded through a normal L1 transfer.

`POST /api/appchain/{chain_id}/anchor` snapshots the server-side L3 state, derives a deterministic SHA3-256 state root, creates and verifies a versioned proof envelope, then records the anchor fee debit against that AppChain treasury. The current proof scheme is a server-verified state commitment; a recursive ZK proof circuit can replace the envelope in a later protocol version without changing the treasury contract.

For local fixture testing only, set `ULTRANET_ENABLE_TEST_ANCHORING=true` on a debug build. This enables `POST /api/appchain/{chain_id}/anchor/test`, which runs the same server-side snapshot/proof/debit path but labels the result as a test anchor. Leave this setting unset or false in production.

### What is `ULTRANET_ADMIN_TOKEN`?

Every node API requires `ULTRANET_ADMIN_TOKEN` to protect state-changing administrator operations such as mining, pruning, and AppChain management. It is a private bearer token for the node operator; it is **not** your wallet key, public node identifier, `DILITHIUM_PUB_KEY`, or a value that ordinary website users should share.

Create it locally, then place it only in your service environment or private `UltraNetNode.env` file:

```bash
openssl rand -hex 32
```

On Windows PowerShell:

```powershell
$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$token = [BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
$rng.Dispose()
$token
```

Copy the resulting 64-character hexadecimal value after `ULTRANET_ADMIN_TOKEN=`. This is the recommended format: the current runtime validation requires at least 32 non-whitespace bytes, but it does not yet enforce hexadecimal characters or exactly 64 characters. Use the recommended format for compatibility with the intended security contract. Never commit, email, paste, or place this token in browser code. A missing or invalid token stops the node before it opens storage and prints an English configuration error. On Windows desktop packages, keep the value only in the private sibling `UltraNetNode.env`; on systemd use `/etc/ultranet/ultranet.env`; in Docker, provide the required variable before `docker compose up -d`.

When `ULTRANET_DB_PATH` is not set, the node uses a writable per-user directory: `%LOCALAPPDATA%\\UltraNet\\data` on Windows, `~/Library/Application Support/UltraNet/data` on macOS, and `$XDG_DATA_HOME/ultranet` or `~/.local/share/ultranet` on Linux. An explicit `ULTRANET_DB_PATH` always wins.

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
Prospective validators should start with the public [**Validator Onboarding page**](https://ultranetwork.cc/validator), run the v7.1.6 node, and submit a signed proposal after the Genesis connection is visible.

Users can send `$ULTRA` through the public [**Send $ULTRA wallet flow**](https://ultranetwork.cc/transact). The wallet keeps key material local, shows an explicit review, and never asks for `ULTRANET_ADMIN_TOKEN`.

**Documentation:**
- [**`AUTHORS.md`**](./AUTHORS.md): Authorship, provenance, and licensing scope.
- [**`ULTRA_NET_TECHNICAL_GUIDE.md`**](./ULTRA_NET_TECHNICAL_GUIDE.md): The UltraNet technical whitepaper.
- [**`VALIDATOR_GUIDE.md`**](./VALIDATOR_GUIDE.md): Setup, staking, and maintenance.
- [**`ULTRAWALLET_INTEGRATION.md`**](./ULTRAWALLET_INTEGRATION.md): Browser wallet provider and validator proposal contract.
- [**`CLI_AUTH_SIGNING.md`**](./CLI_AUTH_SIGNING.md): Offline CLI authentication signing, API login, and browser import workflow.
- [**`OFFLINE_APPROVAL_SIGNING.md`**](./OFFLINE_APPROVAL_SIGNING.md): Offline 2-of-3 validator approval signing and submission.
- [**`TECHNICAL_MANIFEST.md`**](./TECHNICAL_MANIFEST.md): In-depth protocol constraints.
- [**`GENESIS_REPORT.md`**](./GENESIS_REPORT.md): Initial supply and allocation audit.

## 🛡️ Security First
- **No Private Keys on Server**: All sovereign operations require 2-of-3 signatures signed offline.
- **Zero-Knowledge Enforcement**: Mandatory execution traces for all Move VM state transitions.
- **Automatic Slashing**: Protocol-level jailing for state root non-determinism.

---
*UltraNet: The permanent infrastructure for autonomous finance.*
