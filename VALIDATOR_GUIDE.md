# UltraNet v7.1 Validator Onboarding Guide: The Sovereign Network

**Creator and copyright holder:** Vladan Jotov  
**Document license:** ISC License — see [`LICENSE`](./LICENSE)

## 1. Overview
Welcome to the frontier of institutional blockchain infrastructure. **UltraNet v7.1 "Sovereign"** is a high-performance, quantum-secure, sharded L1 network designed for century-long data integrity. As a validator, you provide the compute power for the Bullshark/Mysticeti DAG and the Arkworks-driven ZK-SNARK verifier.

## 2. Infrastructure Requirements
The UltraNet kernel is optimized for parallel hardware. To maintain the **sub-millisecond finality (27.79µs/vertex)** target, your infrastructure must meet or exceed these specifications:

### 2.1 Hardware
- **Processor**: 8+ Physical Cores @ 3.5GHz+ (AVX-512 support recommended for ZK acceleration).
- **Memory**: 32 GB DDR4/DDR5 (Required to manage 16 concurrent State MPT Shards).
- **Storage**: 1 TB NVMe SSD (Minimum 100k IOPS). 
- **Connectivity**: 1 Gbps Low-latency Fiber (Static IP required).

### 2.2 Operating Environment
- **OS**: Linux (Ubuntu 22.04 LTS / Debian 12 / RHEL 9).
- **Kernel**: 5.15+ (Required for optimal `tokio` asynchronous I/O performance).

### 2.3 Published v7.1.6 Node Packages

For x86_64 hosts, the [published UltraNet v7.1.6 GitHub release](https://github.com/Alexand173/ultranet/releases/tag/v7.1.6) provides precompiled node binaries. Select the archive for your platform:

| Platform | Download | Contains |
| :--- | :--- | :--- |
| Windows x64 | [`UltraNetNode-windows-x64.zip`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-windows-x64.zip) | `UltraNetNode.exe` plus launcher and configuration template |
| Linux x64 | [`UltraNetNode-linux-x64.tar.gz`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-linux-x64.tar.gz) | `UltraNetNode` |
| macOS x64 (Intel) | [`UltraNetNode-macos-x64.tar.gz`](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/UltraNetNode-macos-x64.tar.gz) | `UltraNetNode` |

Download the [release checksum manifest](https://github.com/Alexand173/ultranet/releases/download/v7.1.6/SHA256SUMS.txt) at the same time. Verify the archive before extracting or executing it:

```bash
# Linux: download one archive plus SHA256SUMS.txt
sha256sum --ignore-missing --check SHA256SUMS.txt

# macOS: compare this output with the matching manifest line
shasum -a 256 UltraNetNode-macos-x64.tar.gz
grep 'UltraNetNode-macos-x64.tar.gz$' SHA256SUMS.txt
```

On Windows PowerShell, compare the calculated hash with the manifest entry:

```powershell
$expected = (Get-Content .\SHA256SUMS.txt | Where-Object { $_ -match 'UltraNetNode-windows-x64\.zip$' }).Split()[0]
$actual = (Get-FileHash .\UltraNetNode-windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
"Checksum OK"
```

After verification, extract the platform archive and start the node with the environment from [`deploy/ultranet.env.example`](./deploy/ultranet.env.example):

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

The published `v7.1.6` Windows package contains `UltraNetNode.exe`,
`Start-UltraNetNode.bat`, `UltraNetNode.env.example`, and
`README-WINDOWS.txt`. Copy the example to `UltraNetNode.env`, create the
private admin token below, and launch the batch file first. It runs
`--check-config` and `--check-fhe` before storage/cryptographic setup and keeps
interactive configuration failures visible. The `v7.1.6` release pipeline
completed successfully for Linux, macOS, Windows, and published asset-contract
verification.

If your host is not x86_64, or you require a source build, continue with the compilation path below.

### 2.4 Administrator token for ordinary operators

`ULTRANET_ADMIN_TOKEN` is a private administrator bearer token required by the node API for state-changing operations such as mining, pruning, and AppChain management. It is not a wallet key, public node identifier, or `DILITHIUM_PUB_KEY`. Do not share it or place it in browser code.

Create a fresh 32-byte token locally:

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

Set the resulting 64-character hexadecimal value in `UltraNetNode.env` for the Windows package or in `/etc/ultranet/ultranet.env` for systemd. This is the recommended format: current runtime validation requires at least 32 non-whitespace bytes, but does not yet enforce hexadecimal characters or exactly 64 characters. Keep the file private. If the node reports that `ULTRANET_ADMIN_TOKEN` is required, create a token with one of the commands above and restart the node; do not use a wallet key or public identity in its place.

When `ULTRANET_DB_PATH` is omitted, local desktop launches use `%LOCALAPPDATA%\\UltraNet\\data` on Windows, `~/Library/Application Support/UltraNet/data` on macOS, and `$XDG_DATA_HOME/ultranet` or `~/.local/share/ultranet` on Linux. An explicit `ULTRANET_DB_PATH` remains authoritative for systemd, Docker, and existing state.

### 2.5 Export the validator public key

The first-run console prints a public libp2p `Peer ID`, but that is not the Dilithium key required by the proposal form. After configuring the node and starting it successfully, run the export command from the folder containing the node binary:

```powershell
# Windows PowerShell
.\\UltraNetNode.exe --export-validator-public-key .\\DILITHIUM_PUB_KEY.hex
```

```bash
# Linux or macOS
./UltraNetNode --export-validator-public-key ./DILITHIUM_PUB_KEY.hex
```

The command creates one stable Dilithium-5 identity in the private node data directory and writes only the public half to `DILITHIUM_PUB_KEY.hex`. The output is a complete lowercase hexadecimal public key (2,592 bytes / 5,184 hex characters). Copy the complete line into the `DILITHIUM_PUB_KEY.hex` proposal field. Keep `validator_dilithium5_key.json` private; it contains the corresponding secret key. The command refuses to overwrite an existing output file. It does not submit a proposal or replace the local UltraWallet signature.

Never copy the `Peer ID`, `ULTRANET_ADMIN_TOKEN`, wallet address, or a random online-generated value into the public-key field. Older release archives that do not recognize `--export-validator-public-key` must be replaced with an updated package built from this repository.

## 3. Deployment Sequence

### 3.1 Environment Hardening
Ensure your build environment has the necessary cryptographic primitives:
```bash
sudo apt update && sudo apt install -y build-essential clang cmake libssl-dev pkg-config
```

### 3.2 Protocol Compilation
We recommend compiling from source to enable hardware-specific optimizations:
```bash
git clone https://github.com/Alexand173/ultranet.git
cd ultranet
# Enable 'lto' for maximum production performance and use the committed lockfile.
cargo build --release --locked
```

### 3.3 Firewall & Networking
Create `/etc/ultranet/ultranet.env` from [`deploy/ultranet.env.example`](./deploy/ultranet.env.example) before starting the node:

```bash
ULTRANET_API_BIND=127.0.0.1:8081
ULTRANET_CORS_ORIGINS=https://dashboard.example.com
ULTRANET_DB_PATH=/var/lib/ultranet
```

The API bind and CORS policy are environment-configurable. CORS accepts only explicit `http://` or `https://` origins and rejects wildcards. For a public VPS, keep the API on loopback and terminate TLS in a reverse proxy.

Expose the following protocol ports:
| Port | Protocol | Usage |
| :--- | :--- | :--- |
| **9000** | TCP/UDP | **Mainnet P2P Swarm** (Gossip, Block Sync, DAG) |
| **8081** | TCP | **Local-only API; do not expose directly** |

## 4. Operation & Management

### 4.1 Launching the Node
For a local smoke test, run the release binary with the configured environment:
```bash
cargo build --release --locked --bin UltraNet
ULTRANET_API_BIND=127.0.0.1:8081 \
ULTRANET_CORS_ORIGINS=https://dashboard.example.com \
ULTRANET_DB_PATH=/var/lib/ultranet \
./target/release/UltraNet
```
For a VPS, use the hardened `systemd` service described below instead of a shell background process.

### 4.2 Ensuring 24/7 Uptime (systemd)
Use the production service file, which runs under a dedicated `ultranet` user, reads `/etc/ultranet/ultranet.env`, and keeps the API on loopback:

```bash
sudo install -o root -g root -m 0644 deploy/ultranet.service /etc/systemd/system/ultranet.service
sudo systemctl daemon-reload
sudo systemctl enable --now ultranet
sudo systemctl status ultranet
journalctl -u ultranet -f
```

The root-level `ultranet.service.template` is kept in sync for existing workflows. See [`deploy/README.md`](./deploy/README.md) for binary installation, persistent storage, firewall, and reverse-proxy instructions.

For the current 8 GiB VPS, install the 4 GiB persistent swap file and the systemd memory monitor described in [`deploy/README.md`](./deploy/README.md). The validator unit contains `MemoryHigh=6912M`, `MemoryMax=7168M`, and `MemorySwapMax=4096M` to contain a runaway process while leaving room for the OS, Caddy, and the dashboard. It also uses a 15-second restart delay and a three-starts-per-five-minutes limit. These are emergency containment settings; the 32 GB memory recommendation remains the target for sustained validator operation.

### 4.3 Validator Registration
During the **Bootstrap Phase (v7.1)**, the validator set is managed by the 2-of-3 Sovereign Multi-Sig.
1. **Identity Extraction**: Run `--export-validator-public-key` and keep the generated `DILITHIUM_PUB_KEY.hex` public file.
2. **On-boarding Request**: Submit the complete public key to the governance portal; do not submit the private `validator_dilithium5_key.json` file.
3. **Weight Assignment**: Once approved, your weight will be added to the BLS aggregation set in `src/lib.rs`.

## 5. Performance Monitoring
Access the **UltraNet Pro Dashboard** at `http://<your-node-ip>:8081/dashboard`.

**Critical Indicators:**
- **DAG Vertex Latency**: Should remain < 100ms.
- **ZK Verification Time**: Target < 600ms per proof.
- **State Roots**: Must perfectly match the sharded MPT roots published in the dashboard.

## 6. Rewards & Slashing
- **Rewards**: Distributed automatically every epoch based on successful BLS signature aggregation and vertex anchoring.
- **Slashing**: Validators failing to verify correct ZK proofs or exhibiting state-root non-determinism will be automatically jailed after 3 consecutive failures.

---
*UltraNet: Securing the next century of autonomous finance.*
