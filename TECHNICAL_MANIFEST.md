# UltraNet v7.1 Technical Manifest: The Sovereign Protocol

**Creator and copyright holder:** Vladan Jotov  
**Document license:** ISC License — see [`LICENSE`](./LICENSE)  
**Authorship record:** [`AUTHORS.md`](./AUTHORS.md)

## 1. Protocol Identity & Genesis
- **Release Version**: 7.1 "Sovereign"
- **Native Token**: **$ULTRA**
- **Genesis Block Height**: 1
- **Genesis Allocation**: 1,000,000.000000 $ULTRA = 1,000,000,000,000 `microULTRA` base units (Move Module `UltraCoin`; `decimals = 6`)
- **Longevity Philosophy**: 100-Year "Century Curve" Protocol Stability.

> **Source of Truth:** `genesis.json` is the definitive source for UltraNet sovereign genesis configuration, including the sovereign address, network parameters, and initial token allocations.

## 2. Cryptographic Security Architecture
UltraNet v7.1 implements a defense-in-depth strategy utilizing lattice-based and pairing-based cryptography.

### 2.1 Post-Quantum (PQ) Signatures
- **Scheme**: **Dilithium-5** (Lattice-based).
- **Public Key**: ~2.5 KB.
- **Signature Size**: 4,627 bytes (Detached).
- **Verification**: SHA3-256 pre-hashed message digest.
- **Anti-Spoofing**: Strict Public Key to Address mapping enforcement in `validate_transaction`.

### 2.2 Sovereign Multi-Signature Shield
- **Address**: `3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c`
- **Threshold**: **2-of-3 signatures** required for all sovereign operations.
- **Payload Structure**: Signature field expects N x 4,627 bytes concatenated signatures.
- **Key Storage**: Decentralized across 3 unique Owners (Offline/Cold storage mandatory).

### 2.3 Zero-Knowledge Infrastructure (ZK-Ultra)
- **Primary Backend**: **Arkworks Groth16** (Rust-native).
- **Circuit Architecture**: R1CS Private Transaction Circuit.
- **Proof Types**: `Transaction`, `Balance`, `Ownership`, `Range`.
- **Double-Spend Prevention**: Public nullifier commitments without sender trace (SHA3-256).

## 3. High-Performance Consensus & Execution
UltraNet achieves industry-leading throughput by decoupling transaction dissemination from ordering and execution.

### 3.1 Bullshark/Mysticeti DAG
- **Architecture**: Directed Acyclic Graph (DAG) Consensus.
- **Mechanism**: Shoal-anchoring with Bullshark sub-DAG commits.
- **Performance Benchmarks**: 
    - Verified sub-millisecond per-vertex processing (27.79µs).
    - Asynchronous consensus allowing zero-latency dissemination.

### 3.2 Block-STM Parallel Engine
- **Strategy**: Optimistic Parallel Execution with multi-version memory management.
- **Sharding**: 16 Parallel MPT (Merkle Patricia Trie) Shards.
- **Consistency**: Fraud-proof state root re-execution for all inbound remote blocks.

## 4. Economic & Governance Engine
The AI-Governor manages the network's health autonomously to ensure century-long viability.

### 4.1 AI Monetary Policy
- **Halving Cycle**: Every **31,557,600 blocks** (~10 years).
- **Base Reward**: 50 protocol base units (`0.000050 $ULTRA` at six decimals); the existing runtime value is unchanged.
- **Inflation Control**: Dynamic reward adjustments based on transaction density and sustainability scoring.

### 4.2 Move VM Integration
- **Smart Contract Language**: Move (Resource-oriented architecture).
- **Execution Security**: Mandatory ZK-STARK execution traces for all Move calls.
- **AppChain Anchoring**: Native L3 factory for sovereign enterprise networks.

## 5. Network Parameters
- **Target Block Time**: 10 Seconds.
- **Max Block size**: 1/10th of memory-allocated pool per transaction.
- **P2P Layer**: Port `9000` (Mainnet Swarm).
- **API/Dashboard**: Port `8081` (REST interface).

---
*This manifest is generated from the immutable core of UltraNet v7.1. All specifications are active and enforced by the protocol as of the Genesis Block.*
