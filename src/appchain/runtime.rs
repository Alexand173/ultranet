// ============================================================
// L3 APPCHAIN RUNTIME - ISOLATED INSTANCE
// ============================================================

use crate::move_vm::MoveVM;
use crate::recursive_zk::RecursiveZKEngine;
use crate::shared_storage::SharedStorage;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::sync::Arc;

pub const APPCHAIN_ANCHOR_PROOF_VERSION: u32 = 1;
const APPCHAIN_STATE_ROOT_DOMAIN: &[u8] = b"UltraNet/AppChain/state-root/v1";
const APPCHAIN_ANCHOR_PROOF_DOMAIN: &[u8] = b"UltraNet/AppChain/anchor-proof/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppChainStateSnapshot {
    pub chain_id: u32,
    pub state_root: [u8; 32],
    pub state_height: u64,
    pub module_count: u64,
    pub resource_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppChainAnchorProof {
    pub version: u32,
    pub chain_id: u32,
    pub anchor_number: u64,
    pub state_root: [u8; 32],
    pub state_height: u64,
    pub module_count: u64,
    pub resource_count: u64,
    pub treasury_address: String,
    pub fee_charged: u64,
    pub timestamp: u64,
    pub trace_commitment: [u8; 32],
}

#[allow(dead_code)]
pub struct AppChainRuntime {
    pub id: u32,
    pub vm: MoveVM,
    pub storage: Arc<SharedStorage>,
    pub recursive_zk: Arc<RwLock<RecursiveZKEngine>>,
    runtime_meta: sled::Tree,
}

#[allow(dead_code)]
impl AppChainRuntime {
    pub fn new(id: u32, base_path: &str) -> Self {
        Self::try_new(id, base_path).expect("Failed to create AppChain storage")
    }

    pub fn try_new(id: u32, base_path: &str) -> Result<Self, String> {
        let path = format!("{}/l3/{}", base_path, id);
        let storage = Arc::new(
            SharedStorage::new(&path)
                .map_err(|error| format!("failed to open AppChain storage: {error}"))?,
        );
        let runtime_meta = storage
            .storage
            .db
            .open_tree("runtime_meta")
            .map_err(|error| format!("failed to open AppChain runtime metadata: {error}"))?;

        Ok(Self {
            id,
            vm: MoveVM::new(storage.clone()),
            storage,
            recursive_zk: Arc::new(RwLock::new(RecursiveZKEngine::new())),
            runtime_meta,
        })
    }

    fn state_version(&self) -> Result<u64, String> {
        let value = self
            .runtime_meta
            .get(self.id.to_be_bytes())
            .map_err(|error| format!("cannot read AppChain state version: {error}"))?;
        match value {
            None => Ok(0),
            Some(value) if value.len() == 8 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&value);
                Ok(u64::from_le_bytes(bytes))
            }
            Some(_) => Err("AppChain state version record is malformed".to_string()),
        }
    }

    fn bump_state_version(&self) -> Result<u64, String> {
        let next = self
            .state_version()?
            .checked_add(1)
            .ok_or_else(|| format!("AppChain #{} state version overflowed", self.id))?;
        self.runtime_meta
            .insert(self.id.to_be_bytes(), next.to_le_bytes().as_slice())
            .map_err(|error| format!("cannot persist AppChain state version: {error}"))?;
        self.storage
            .storage
            .flush()
            .map_err(|error| format!("cannot flush AppChain state version: {error}"))?;
        Ok(next)
    }

    /// Build a deterministic commitment over the persisted AppChain state.
    ///
    /// Sled iterates keys in byte order, so the same durable state produces the
    /// same root on every node. This is a server-side state commitment; a
    /// future recursive ZK circuit can replace the proof envelope without
    /// changing the treasury/accounting contract.
    pub fn snapshot_state(&self) -> Result<AppChainStateSnapshot, String> {
        let state_height = self.state_version()?;
        let mut hasher = Sha3_256::new();
        hasher.update(APPCHAIN_STATE_ROOT_DOMAIN);
        hasher.update(self.id.to_le_bytes());
        hasher.update(state_height.to_le_bytes());

        let mut module_count = 0u64;
        for item in self.storage.move_modules.iter() {
            let (key, value) =
                item.map_err(|error| format!("cannot read AppChain module state: {error}"))?;
            hasher.update(b"module");
            hasher.update((key.len() as u64).to_le_bytes());
            hasher.update(&key);
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(&value);
            module_count = module_count.saturating_add(1);
        }

        let mut resource_count = 0u64;
        for item in self.storage.move_resources.iter() {
            let (key, value) =
                item.map_err(|error| format!("cannot read AppChain resource state: {error}"))?;
            hasher.update(b"resource");
            hasher.update((key.len() as u64).to_le_bytes());
            hasher.update(&key);
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(&value);
            resource_count = resource_count.saturating_add(1);
        }

        for (shard_id, shard) in self.storage.storage.state_shards.iter().enumerate() {
            for item in shard.iter() {
                let (key, value) =
                    item.map_err(|error| format!("cannot read AppChain account state: {error}"))?;
                hasher.update(b"account");
                hasher.update((shard_id as u64).to_le_bytes());
                hasher.update((key.len() as u64).to_le_bytes());
                hasher.update(&key);
                hasher.update((value.len() as u64).to_le_bytes());
                hasher.update(&value);
            }
        }

        Ok(AppChainStateSnapshot {
            chain_id: self.id,
            state_root: hasher.finalize().into(),
            state_height,
            module_count,
            resource_count,
        })
    }

    pub fn create_anchor_proof(
        &self,
        snapshot: &AppChainStateSnapshot,
        anchor_number: u64,
        treasury_address: &str,
        fee_charged: u64,
        timestamp: u64,
    ) -> AppChainAnchorProof {
        let trace_commitment = anchor_trace_commitment(
            snapshot,
            anchor_number,
            treasury_address,
            fee_charged,
            timestamp,
        );
        AppChainAnchorProof {
            version: APPCHAIN_ANCHOR_PROOF_VERSION,
            chain_id: snapshot.chain_id,
            anchor_number,
            state_root: snapshot.state_root,
            state_height: snapshot.state_height,
            module_count: snapshot.module_count,
            resource_count: snapshot.resource_count,
            treasury_address: treasury_address.to_string(),
            fee_charged,
            timestamp,
            trace_commitment,
        }
    }

    pub fn verify_anchor_proof(
        &self,
        snapshot: &AppChainStateSnapshot,
        proof: &AppChainAnchorProof,
    ) -> Result<(), String> {
        if proof.version != APPCHAIN_ANCHOR_PROOF_VERSION {
            return Err(format!(
                "unsupported AppChain anchor proof version {}",
                proof.version
            ));
        }
        if proof.chain_id != self.id || proof.chain_id != snapshot.chain_id {
            return Err("AppChain anchor proof chain ID does not match the runtime".to_string());
        }
        if proof.state_root != snapshot.state_root
            || proof.state_height != snapshot.state_height
            || proof.module_count != snapshot.module_count
            || proof.resource_count != snapshot.resource_count
        {
            return Err(
                "AppChain anchor proof does not match the server state snapshot".to_string(),
            );
        }
        let expected = anchor_trace_commitment(
            snapshot,
            proof.anchor_number,
            &proof.treasury_address,
            proof.fee_charged,
            proof.timestamp,
        );
        if expected != proof.trace_commitment {
            return Err("AppChain anchor proof commitment is invalid".to_string());
        }
        Ok(())
    }

    pub fn produce_block(&mut self, transactions: Vec<crate::Transaction>) -> crate::UltraBlock {
        println!(
            "🏗️ AppChain #{}: Producing block with {} transactions...",
            self.id,
            transactions.len()
        );

        // 1. Izvrši transakcije
        for tx in &transactions {
            if let crate::TransactionPayload::MoveCall {
                module_name,
                function_name,
                args,
                ..
            } = &tx.payload
            {
                let addr = move_core_types::account_address::AccountAddress::ZERO;
                let _ = self
                    .vm
                    .execute_function(addr, module_name, function_name, args.clone());
            }
        }
        if !transactions.is_empty() {
            let _ = self.bump_state_version();
        }

        // 2. Pokupi FHE dokaze (Phase 3)
        let fhe_proof = self.vm.last_fhe_proof.clone();
        if fhe_proof.is_some() {
            println!(
                "🛡️ AppChain #{}: ZK-FHE proof generated for this block!",
                self.id
            );
        }

        crate::UltraBlock {
            index: 1, // AppChain block production remains a separate follow-up.
            timestamp: 0,
            previous_hash: [0; 32],
            hash: [0; 32],
            nonce: 0,
            transactions,
            merkle_root: [0; 32],
            state_root: [0; 32],
            shard_roots: vec![],
            aggregated_signature: None,
            validator_set: vec![],
            epoch: 0,
            gas_used: 0,
            gas_limit: 1000000,
            block_reward: 10,
            size: 0,
            version: 1,
            parent_hash: [0; 32],
            difficulty: 0,
            total_difficulty: 0,
        }
    }
}

fn anchor_trace_commitment(
    snapshot: &AppChainStateSnapshot,
    anchor_number: u64,
    treasury_address: &str,
    fee_charged: u64,
    timestamp: u64,
) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(APPCHAIN_ANCHOR_PROOF_DOMAIN);
    hasher.update(snapshot.chain_id.to_le_bytes());
    hasher.update(anchor_number.to_le_bytes());
    hasher.update(snapshot.state_root);
    hasher.update(snapshot.state_height.to_le_bytes());
    hasher.update(snapshot.module_count.to_le_bytes());
    hasher.update(snapshot.resource_count.to_le_bytes());
    hasher.update((treasury_address.len() as u64).to_le_bytes());
    hasher.update(treasury_address.as_bytes());
    hasher.update(fee_charged.to_le_bytes());
    hasher.update(timestamp.to_le_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn state_snapshot_and_anchor_proof_round_trip() {
        let path = format!("test_db_appchain_runtime_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let runtime = AppChainRuntime::new(7, &path);
        let snapshot = runtime.snapshot_state().unwrap();
        let treasury = crate::appchain::derive_appchain_treasury_address(7);
        let proof = runtime.create_anchor_proof(&snapshot, 1, &treasury, 1_000, 42);
        runtime.verify_anchor_proof(&snapshot, &proof).unwrap();
        assert_eq!(proof.state_root, snapshot.state_root);
        assert_eq!(proof.chain_id, 7);
        assert_ne!(snapshot.state_root, [0u8; 32]);
        drop(runtime);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn anchor_proof_rejects_state_root_tampering() {
        let path = format!("test_db_appchain_runtime_tamper_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let runtime = AppChainRuntime::new(8, &path);
        let snapshot = runtime.snapshot_state().unwrap();
        let treasury = crate::appchain::derive_appchain_treasury_address(8);
        let mut proof = runtime.create_anchor_proof(&snapshot, 1, &treasury, 1_000, 42);
        proof.state_root[0] ^= 1;
        assert!(runtime.verify_anchor_proof(&snapshot, &proof).is_err());
        drop(runtime);
        let _ = fs::remove_dir_all(path);
    }
}
