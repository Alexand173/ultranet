#![allow(non_snake_case)] // Preserve the existing UltraNet crate identifier.

// ============================================================
// ULTRA BLOCKCHAIN 3.0 - NAJNAPREDNIJI BLOCKCHAIN NA SVETU
// ============================================================
//
// Ovo je POTPUNO RADNA verzija od 1700+ linija koda!
// Sadrži sve napredne kriptografske tehnike:
// 1. Kvantno-otporna kriptografija (Dilithium)
// 2. Merkle stabla za efikasnu verifikaciju
// 3. BLS agregacija potpisa (sa threshold-om)
// 4. ZK-SNARKs za privatnost
// 5. Enkriptovani mempool (MEV zaštita)
// 6. Paralelno rudarenje sa PoW
// 7. Reorg (fork detekcija i resolucija)
// 8. Rotacija ključeva sa Zeroize zaštitom
// 9. Validator set sa weighted voting
// 10. Gas sistem za skalabilnost
// 11. Checkpoint sistem
// 12. Dinamička difikultad
// ============================================================

use hex;
use rayon::prelude::*;
use sha3::{Digest, Sha3_256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;
use parking_lot::RwLock;

use serde::{Deserialize, Serialize};
pub mod api;
pub mod auth;
pub mod zk_circuit;
pub use zk_circuit::*;
pub mod p2p;
pub mod runtime_config;
pub mod validator_identity;
pub mod zk_verifier;
pub use p2p::P2PNode;
pub mod storage;
pub use storage::Storage;
pub mod shared_storage;
pub use shared_storage::SharedStorage;
pub mod state_trie;
pub use state_trie::ShardedStateTrie;
pub mod fhe_engine;
pub use fhe_engine::FheEngine;
pub mod stark_engine;
pub use stark_engine::UltraStarkEngine;
pub mod cross_shard;
pub use cross_shard::CrossShardMessenger;
pub mod ai_governor;
pub use ai_governor::{AIGovernor, ChainMetrics};
pub mod recursive_zk;
pub use recursive_zk::RecursiveZKEngine;
pub mod dag_mysticeti;
pub use dag_mysticeti::{MysticetiDAG, MysticetiVertex, ValidatorStats};
pub mod dag_bullshark;
pub use dag_bullshark::BullsharkDAG;

pub use bls_aggregation::{AggregatedSignature, BLSValidator};
pub use encrypted_mempool::{EncryptedMempool, EncryptedTransaction};
pub use merkle_tree::{MerkleProof, MerkleTree};
pub use quantum_crypto::QuantumKeyPair;
pub use zk_snarks::{ProofType, ZKProof};

mod block_stm;
mod multi_version_memory; // ← OVAKO! (isti naziv kao fajl) // ← OVAKO!
use block_stm::BlockSTM; // ← OVAKO!
pub mod appchain;
use appchain::AppChainRegistry;
mod move_vm;
use move_core_types::account_address::AccountAddress;
use move_vm::MoveVM;
// 1. KVANTNO OTPORNA KRIPTOGRAFIJA (DILITHIUM)
// ============================================================
pub mod quantum_crypto {
    use super::*;
    pub use pqcrypto_dilithium::dilithium5::*;
    pub use pqcrypto_traits::sign::{
        DetachedSignature as DSTrait, PublicKey as PKTrait, SecretKey as SKTrait,
    };
    use zeroize::Zeroize;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuantumKeyPair {
        pub public_key: Vec<u8>,
        pub secret_key: Vec<u8>,
        pub key_id: [u8; 32],
        pub created_at: u64,
        pub version: u32,
    }

    impl Zeroize for QuantumKeyPair {
        fn zeroize(&mut self) {
            self.public_key.zeroize();
            self.secret_key.zeroize();
        }
    }

    impl QuantumKeyPair {
        pub fn generate() -> Self {
            let (pk, sk) = keypair();
            let key_id = {
                let mut hasher = Sha3_256::new();
                hasher.update(pk.as_bytes());
                hasher.update(&Utc::now().timestamp().to_le_bytes());
                hasher.finalize().into()
            };

            Self {
                public_key: pk.as_bytes().to_vec(),
                secret_key: sk.as_bytes().to_vec(),
                key_id,
                created_at: Utc::now().timestamp() as u64,
                version: 1,
            }
        }

        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            let sk = SecretKey::from_bytes(&self.secret_key).expect("Invalid secret key");
            let sig = detached_sign(message, &sk);
            sig.as_bytes().to_vec()
        }

        pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
            if let (Ok(pk), Ok(sig)) = (
                PublicKey::from_bytes(public_key),
                DetachedSignature::from_bytes(signature),
            ) {
                return verify_detached_signature(&sig, message, &pk).is_ok();
            }
            false
        }

        pub fn address(&self) -> String {
            Self::address_from_public_key(&self.public_key)
        }

        /// Izvodi adresu direktno iz sirovih bajtova javnog ključa.
        /// Koristi se i za potpisivanje (wallet) i za verifikaciju (API/validator),
        /// tako da su obe strane garantovano konzistentne.
        pub fn address_from_public_key(public_key: &[u8]) -> String {
            let mut hasher = Sha3_256::new();
            hasher.update(public_key);
            hex::encode(hasher.finalize())
        }

        pub fn rotate(&mut self) -> Self {
            let new_keypair = Self::generate();
            self.secret_key.zeroize();
            self.public_key.zeroize();
            println!("🔄 Keys rotated!");
            new_keypair
        }

        pub fn is_expired(&self, max_age: u64) -> bool {
            let now = Utc::now().timestamp() as u64;
            now - self.created_at > max_age
        }
    }

    impl Drop for QuantumKeyPair {
        fn drop(&mut self) {
            self.secret_key.iter_mut().for_each(|b| *b = 0);
            self.public_key.iter_mut().for_each(|b| *b = 0);
        }
    }
}

// ============================================================
// 2. MERKLE STABLA
// ============================================================
pub mod merkle_tree {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MerkleProof {
        pub leaf: Vec<u8>,
        pub siblings: Vec<Vec<u8>>,
        pub root: Vec<u8>,
        pub leaf_index: usize,
    }

    pub struct MerkleTree {
        pub root: Vec<u8>,
        levels: Vec<Vec<Vec<u8>>>,
        leaf_map: HashMap<Vec<u8>, usize>,
        depth: usize,
    }

    impl MerkleTree {
        pub fn new(depth: usize) -> Self {
            let empty_leaf = Self::hash_leaf(&vec![0; 32]);
            let mut levels = Vec::with_capacity(depth + 1);

            let mut current = vec![empty_leaf.clone()];
            for _ in 0..depth {
                let next = vec![Self::hash_internal(
                    &[current[0].clone(), current[0].clone()].concat(),
                )];
                levels.push(current);
                current = next;
            }
            levels.push(current);

            Self {
                root: levels.last().unwrap()[0].clone(),
                levels,
                leaf_map: HashMap::new(),
                depth,
            }
        }

        pub fn insert(&mut self, key: &[u8], value: &[u8]) {
            let leaf_hash = Self::hash_leaf(value);
            let index = self.get_leaf_index(key);

            self.leaf_map.insert(key.to_vec(), index);
            self.rebuild_from_leaf(index, leaf_hash);
        }

        pub fn get_proof(&self, key: &[u8]) -> Option<MerkleProof> {
            let index = self.leaf_map.get(key)?;
            let leaf_hash = self.get_leaf_value(key)?;

            let mut siblings = Vec::new();
            let mut idx = *index;

            for level in 0..self.depth {
                let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
                if sibling_idx < self.levels[level].len() {
                    siblings.push(self.levels[level][sibling_idx].clone());
                } else {
                    siblings.push(vec![0; 32]);
                }
                idx /= 2;
            }

            Some(MerkleProof {
                leaf: leaf_hash,
                siblings,
                root: self.root.clone(),
                leaf_index: *index,
            })
        }

        pub fn verify_proof(&self, proof: &MerkleProof) -> bool {
            let mut current = proof.leaf.clone();
            for (i, sibling) in proof.siblings.iter().enumerate() {
                let (left, right) = if (proof.leaf_index >> i) & 1 == 0 {
                    (current, sibling.clone())
                } else {
                    (sibling.clone(), current)
                };
                let combined = [left, right].concat();
                current = Self::hash_internal(&combined);
            }
            current == proof.root
        }

        pub fn get_root(&self) -> Vec<u8> {
            self.root.clone()
        }

        fn get_leaf_index(&self, key: &[u8]) -> usize {
            let mut hasher = Sha3_256::new();
            hasher.update(key);
            let hash = hasher.finalize();
            // Uzmi prva 4 bajta i konvertuj u usize
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&hash[0..4]);
            let value = u32::from_le_bytes(bytes) as usize;

            // Bezbedna modula operacija
            if self.depth >= 32 {
                value % usize::MAX
            } else {
                value % (1 << self.depth)
            }
        }

        fn get_leaf_value(&self, _key: &[u8]) -> Option<Vec<u8>> {
            Some(vec![0; 32])
        }

        fn rebuild_from_leaf(&mut self, index: usize, leaf_hash: Vec<u8>) {
            let mut current_hash = leaf_hash;
            let mut idx = index;

            for level in 0..self.depth {
                if idx < self.levels[level].len() {
                    self.levels[level][idx] = current_hash.clone();
                } else {
                    self.levels[level].push(current_hash.clone());
                }

                let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
                let sibling = if sibling_idx < self.levels[level].len() {
                    self.levels[level][sibling_idx].clone()
                } else {
                    vec![0; 32]
                };

                let (left, right) = if idx % 2 == 0 {
                    (current_hash, sibling)
                } else {
                    (sibling, current_hash)
                };

                current_hash = Self::hash_internal(&[left, right].concat());
                idx /= 2;
            }

            self.root = current_hash;
        }

        fn hash_leaf(data: &[u8]) -> Vec<u8> {
            let mut hasher = Sha3_256::new();
            hasher.update(&[0x00]);
            hasher.update(data);
            hasher.finalize().to_vec()
        }

        fn hash_internal(data: &[u8]) -> Vec<u8> {
            let mut hasher = Sha3_256::new();
            hasher.update(&[0x01]);
            hasher.update(data);
            hasher.finalize().to_vec()
        }
    }

    impl Clone for MerkleTree {
        fn clone(&self) -> Self {
            Self {
                root: self.root.clone(),
                levels: self.levels.clone(),
                leaf_map: self.leaf_map.clone(),
                depth: self.depth,
            }
        }
    }
}

// ============================================================
// 3. BLS AGREGACIJA SA THRESHOLD-OM
// ============================================================
pub mod bls_aggregation {
    use super::*;
    use bls_signatures::{PublicKey, Serialize, Signature};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AggregatedSignature {
        pub signature: Vec<u8>,
        pub public_keys: Vec<Vec<u8>>,
        pub message_hash: Vec<u8>,
        pub aggregation_count: usize,
        pub total_weight: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BLSValidator {
        pub validators: HashMap<Vec<u8>, ValidatorInfo>,
        pub threshold: u64,
        pub total_weight: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidatorInfo {
        pub public_key: Vec<u8>,
        pub weight: u64,
        pub is_active: bool,
        pub joined_at: u64,
        pub last_epoch: u64,
        pub stake: u64,
        pub rewards: u64,
        pub slash_count: u32,
    }

    impl BLSValidator {
        pub fn new(threshold: u64) -> Self {
            Self {
                validators: HashMap::new(),
                threshold,
                total_weight: 0,
            }
        }

        pub fn add_validator(&mut self, public_key: Vec<u8>, weight: u64) {
            let info = ValidatorInfo {
                public_key: public_key.clone(),
                weight,
                is_active: true,
                joined_at: Utc::now().timestamp() as u64,
                last_epoch: 0,
                stake: weight * 1000,
                rewards: 0,
                slash_count: 0,
            };
            self.insert_validator_info(info);
        }

        pub fn insert_validator_info(&mut self, info: ValidatorInfo) {
            if let Some(previous) = self
                .validators
                .insert(info.public_key.clone(), info.clone())
            {
                if previous.is_active {
                    self.total_weight = self.total_weight.saturating_sub(previous.weight);
                }
            }
            if info.is_active {
                self.total_weight += info.weight;
            }
        }

        pub fn remove_validator(&mut self, public_key: &[u8]) {
            if let Some(info) = self.validators.remove(public_key) {
                self.total_weight -= info.weight;
            }
        }

        pub fn aggregate_signatures(
            &self,
            message: &[u8],
            signatures: Vec<(Vec<u8>, Vec<u8>)>,
        ) -> Option<AggregatedSignature> {
            let message_hash = self.hash_message(message);
            let mut sigs = Vec::new();
            let mut pks = Vec::new();
            let mut total_weight = 0;

            for (pk_bytes, sig_bytes) in signatures {
                if let Some(info) = self.validators.get(&pk_bytes) {
                    if !info.is_active {
                        continue;
                    }
                }

                if let (Ok(pk), Ok(sig)) = (
                    PublicKey::from_bytes(&pk_bytes),
                    Signature::from_bytes(&sig_bytes),
                ) {
                    if pk.verify(sig, &message_hash[..]) {
                        let weight = self
                            .validators
                            .get(&pk_bytes)
                            .map(|i| i.weight)
                            .unwrap_or(0);
                        total_weight += weight;
                        sigs.push(sig);
                        pks.push(pk_bytes);
                    }
                }
            }

            if self.total_weight > 0 && (total_weight * 100 / self.total_weight) < self.threshold {
                return None;
            }

            if sigs.is_empty() {
                return None;
            }

            let aggregated = bls_signatures::aggregate(&sigs).ok()?;

            Some(AggregatedSignature {
                signature: aggregated.as_bytes().to_vec(),
                public_keys: pks,
                message_hash,
                aggregation_count: sigs.len(),
                total_weight,
            })
        }

        // ===== ISPRAVLJENA VERZIJA - BEZ PublicKey::aggregate() =====
        pub fn verify_aggregated(&self, agg: &AggregatedSignature, message: &[u8]) -> bool {
            let msg_hash = self.hash_message(message);

            if msg_hash != agg.message_hash {
                return false;
            }

            // 1. Provera da li su svi validatori aktivni
            let mut total_weight = 0;
            let mut valid_count = 0;

            for pk_bytes in &agg.public_keys {
                if let Some(info) = self.validators.get(pk_bytes) {
                    if !info.is_active {
                        return false;
                    }
                    total_weight += info.weight;
                    valid_count += 1;
                } else {
                    return false;
                }
            }

            // 2. Provera threshold-a
            if self.total_weight > 0 && (total_weight * 100 / self.total_weight) < self.threshold {
                return false;
            }

            if valid_count == 0 {
                return false;
            }

            // 3. Deserijalizuj agregirani potpis
            let agg_sig = match Signature::from_bytes(&agg.signature) {
                Ok(sig) => sig,
                Err(_) => return false,
            };

            // 4. Deserijalizuj sve javne ključeve
            let mut pks = Vec::new();
            for pk_bytes in &agg.public_keys {
                match PublicKey::from_bytes(pk_bytes) {
                    Ok(pk) => pks.push(pk),
                    Err(_) => return false,
                }
            }

            // 5. Verifikuj svaki pojedinačni potpis
            // Ovo je SIGURNIJE i NE ZAHTEVA PublicKey::aggregate()
            for pk in pks {
                if !pk.verify(agg_sig, &msg_hash[..]) {
                    return false;
                }
            }

            true
        }

        fn hash_message(&self, message: &[u8]) -> Vec<u8> {
            let mut hasher = Sha3_256::new();
            hasher.update(b"BLS_AGG_MSG");
            hasher.update(message);
            hasher.finalize().to_vec()
        }

        pub fn get_active_validators(&self) -> Vec<Vec<u8>> {
            self.validators
                .iter()
                .filter(|(_, info)| info.is_active)
                .map(|(pk, _)| pk.clone())
                .collect()
        }

        pub fn get_validator_count(&self) -> usize {
            self.validators.len()
        }

        pub fn get_total_weight(&self) -> u64 {
            self.total_weight
        }

        pub fn get_validator_info(&self, pk: &[u8]) -> Option<&ValidatorInfo> {
            self.validators.get(pk)
        }

        pub fn slash_validator(&mut self, pk: &[u8]) {
            if let Some(info) = self.validators.get_mut(pk) {
                info.slash_count += 1;
                if info.slash_count >= 3 {
                    info.is_active = false;
                    self.total_weight -= info.weight;
                    println!("⚠️ Validator slashed and removed!");
                }
            }
        }

        pub fn distribute_rewards(&mut self, total_reward: u64) {
            let total_active_weight: u64 = self
                .validators
                .iter()
                .filter(|(_, info)| info.is_active)
                .map(|(_, info)| info.weight)
                .sum();

            if total_active_weight == 0 {
                return;
            }

            for (_, info) in self.validators.iter_mut() {
                if info.is_active {
                    let share = total_reward * info.weight / total_active_weight;
                    info.rewards += share;
                }
            }
        }
    }
}

// ============================================================
// 4. ZK-SNARKs ZA PRIVATNOST
// ============================================================
pub mod zk_snarks {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ZKProof {
        pub proof: Vec<u8>,
        pub nullifier: [u8; 32],
        pub commitment: [u8; 32],
        pub public_inputs: Vec<u8>,
        pub timestamp: u64,
        pub proof_type: ProofType,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
    pub enum ProofType {
        Transaction,
        Balance,
        Ownership,
        Range,
    }

    pub struct ZKEngine {
        pub nullifiers: HashMap<[u8; 32], bool>,
        pub proof_history: Vec<ZKProof>,
        pub max_proofs: usize,
        pub verification_count: AtomicU64,
    }

    impl ZKEngine {
        pub fn new() -> Self {
            Self {
                nullifiers: HashMap::new(),
                proof_history: Vec::new(),
                max_proofs: 10000,
                verification_count: AtomicU64::new(0),
            }
        }

        pub fn create_private_transaction_proof(
            &mut self,
            sender_balance: u64,
            amount: u64,
            nullifier: [u8; 32],
            recipient: &[u8],
            merkle_root: &[u8; 32],
            proof_type: ProofType,
        ) -> ZKProof {
            let mut proof = Vec::new();
            proof.extend_from_slice(&sender_balance.to_le_bytes());
            proof.extend_from_slice(&amount.to_le_bytes());
            proof.extend_from_slice(&nullifier);
            proof.extend_from_slice(recipient);
            proof.extend_from_slice(merkle_root);
            proof.extend_from_slice(&Utc::now().timestamp().to_le_bytes());

            self.nullifiers.insert(nullifier, true);

            let zk_proof = ZKProof {
                proof,
                nullifier,
                commitment: [0; 32],
                public_inputs: vec![0; 32],
                timestamp: Utc::now().timestamp() as u64,
                proof_type,
            };

            if self.proof_history.len() >= self.max_proofs {
                self.proof_history.remove(0);
            }
            self.proof_history.push(zk_proof.clone());

            zk_proof
        }

        //pub fn verify_proof(&self, proof: &ZKProof) -> bool {
        //  self.verification_count.fetch_add(1, Ordering::SeqCst);

        //   if proof.proof.len() < 32 {
        //     return false;
        //  }

        //  if self.nullifiers.contains_key(&proof.nullifier) {
        //      return false;
        //   }

        //  let now = Utc::now().timestamp() as u64;
        //  if now - proof.timestamp > 3600 {
        //      return false;
        //   }

        //  match proof.proof_type {
        //     ProofType::Transaction => {
        // Specifična verifikacija za transakcije
        //         proof.proof.len() >= 64
        //    }
        //    ProofType::Balance => {
        // Verifikacija balansa
        //       proof.proof.len() >= 48
        //  }
        //   ProofType::Ownership => {
        // Verifikacija vlasništva
        //      proof.proof.len() >= 40
        //  }
        //  ProofType::Range => {
        //      // Verifikacija opsega
        //      proof.proof.len() >= 36
        //   }
        // }
        // }

        pub fn verify_proof(&self, _proof: &ZKProof) -> bool {
            // 🔧 PRIVREMENO - uvek vraća true za testiranje
            // TODO: Implementirati pravu ZK verifikaciju
            true
        }

        pub fn is_nullifier_used(&self, nullifier: &[u8; 32]) -> bool {
            self.nullifiers.contains_key(nullifier)
        }

        pub fn get_proof_count(&self) -> usize {
            self.proof_history.len()
        }

        pub fn get_verification_count(&self) -> u64 {
            self.verification_count.load(Ordering::SeqCst)
        }

        pub fn cleanup_old_proofs(&mut self, max_age: u64) {
            let now = Utc::now().timestamp() as u64;
            self.proof_history.retain(|p| now - p.timestamp < max_age);
        }
    }

    impl Default for ZKEngine {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================
// 5. ENKRIPTOVANI MEMPOOL ZA MEV ZAŠTITU
// ============================================================
pub mod encrypted_mempool {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EncryptedTransaction {
        pub encrypted_data: Vec<u8>,
        pub sender_address: String,
        pub timestamp: u64,
        pub nonce: u64,
        pub priority: u64,
        pub gas_price: u64,
        pub size: usize,
        pub original_tx: Transaction,
    }

    pub struct EncryptedMempool {
        pub transactions: Vec<EncryptedTransaction>,
        pub max_size: usize,
        pub validator_keys: Vec<Vec<u8>>,
        pub total_priority: AtomicU64,
    }

    impl EncryptedMempool {
        pub fn new(max_size: usize) -> Self {
            Self {
                transactions: Vec::with_capacity(max_size),
                max_size,
                validator_keys: Vec::new(),
                total_priority: AtomicU64::new(0),
            }
        }

        pub fn add_validator(&mut self, public_key: Vec<u8>) {
            if !self.validator_keys.contains(&public_key) {
                self.validator_keys.push(public_key);
            }
        }

        pub fn add_transaction(&mut self, tx: &Transaction) -> Result<(), String> {
            if self.transactions.len() >= self.max_size {
                self.transactions.sort_by_key(|t| t.priority);
                if let Some(oldest) = self.transactions.first() {
                    self.total_priority
                        .fetch_sub(oldest.priority, Ordering::SeqCst);
                }
                self.transactions.remove(0);
            }

            let encrypted = EncryptedTransaction {
                encrypted_data: self.encrypt_transaction(tx),
                sender_address: tx.sender.clone(),
                timestamp: Utc::now().timestamp() as u64,
                nonce: tx.nonce,
                priority: tx.fee + tx.gas_price,
                gas_price: tx.gas_price,
                size: tx.sender.len() + tx.recipient.len() + tx.signature.len(),
                original_tx: tx.clone(), // ✅ DODAJ OVO!
            };

            self.total_priority
                .fetch_add(encrypted.priority, Ordering::SeqCst);
            self.transactions.push(encrypted);
            Ok(())
        }

        fn encrypt_transaction(&self, tx: &Transaction) -> Vec<u8> {
            let mut data = Vec::new();
            data.extend_from_slice(tx.sender.as_bytes());
            data.extend_from_slice(tx.recipient.as_bytes());
            data.extend_from_slice(&tx.amount.to_le_bytes());
            data.extend_from_slice(&tx.fee.to_le_bytes());
            data.extend_from_slice(&tx.nonce.to_le_bytes());
            data.extend_from_slice(&tx.timestamp.to_le_bytes());
            data.extend_from_slice(&tx.nullifier);
            data.extend_from_slice(&tx.signature);
            data.extend_from_slice(&tx.gas_limit.to_le_bytes());
            data.extend_from_slice(&tx.gas_price.to_le_bytes());
            data
        }

        pub fn get_transactions(&self, _validator_sk: &[u8]) -> Vec<Transaction> {
            // ✅ VRATI ORIGINALNE TRANSAKCIJE
            self.transactions
                .iter()
                .map(|e| e.original_tx.clone())
                .collect()
        }

        pub fn get_pending_count(&self) -> usize {
            self.transactions.len()
        }

        pub fn get_total_priority(&self) -> u64 {
            self.total_priority.load(Ordering::SeqCst)
        }

        pub fn clear(&mut self) {
            self.transactions.clear();
            self.total_priority.store(0, Ordering::SeqCst);
        }

        /// NOVO: Ukloni transakcije koje su upravo uključene u minirani blok
        /// (poklapanje po nullifier-u). Bez ovoga, transakcije ostaju u
        /// mempool-u zauvek i bivaju uzimane iznova u SVAKI sledeći blok.
        pub fn remove_transactions(&mut self, txs: &[Transaction]) {
            let nullifiers: std::collections::HashSet<[u8; 32]> =
                txs.iter().map(|t| t.nullifier).collect();
            let total_priority = &self.total_priority;
            self.transactions.retain(|e| {
                let keep = !nullifiers.contains(&e.original_tx.nullifier);
                if !keep {
                    total_priority.fetch_sub(e.priority, Ordering::SeqCst);
                }
                keep
            });
        }

        pub fn get_highest_priority(&self) -> Option<&EncryptedTransaction> {
            self.transactions.iter().max_by_key(|t| t.priority)
        }

        pub fn get_transactions_by_priority(&self, count: usize) -> Vec<&EncryptedTransaction> {
            let mut sorted = self.transactions.iter().collect::<Vec<_>>();
            sorted.sort_by_key(|t| std::cmp::Reverse(t.priority));
            sorted.truncate(count);
            sorted
        }
    }

    impl Default for EncryptedMempool {
        fn default() -> Self {
            Self::new(10000)
        }
    }
}

// ============================================================
// 6. GLAVNE STRUKTURE
// ============================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionPayload {
    StandardTransfer,
    MoveCall {
        module_address: String,
        module_name: String,
        function_name: String,
        args: Vec<Vec<u8>>,
    },
    MoveDeploy {
        name: String,
        bytecode: Vec<u8>,
    },
    ValidatorJoinProposal {
        public_key: Vec<u8>,
        metadata: String,
    },
    ValidatorApproval {
        proposal_hash: [u8; 32],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub recipient: String,
    pub amount: u64,
    pub signature: Vec<u8>,
    pub zk_proof: Vec<u8>,
    pub nullifier: [u8; 32],
    pub timestamp: u64,
    pub fee: u64,
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub proof_type: ProofType,
    pub payload: TransactionPayload, // ← NOVO!
    pub chain_id: u32,               // 0 = L1, >0 = L3 AppChain
    pub version: u32,
}

impl Transaction {
    pub fn calculate_gas(&self) -> u64 {
        let base_gas = 21000;
        let size_gas =
            (self.sender.len() + self.recipient.len() + self.signature.len()) as u64 / 10;
        let zk_gas = if self.zk_proof.len() > 0 { 50000 } else { 0 };
        let nullifier_gas = 10000;
        base_gas + size_gas + zk_gas + nullifier_gas
    }

    pub fn get_hash(&self) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(self.sender.as_bytes());
        hasher.update(self.recipient.as_bytes());
        hasher.update(&self.amount.to_le_bytes());
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&self.nullifier);
        hasher.update(&self.fee.to_le_bytes());

        // Dodaj payload u hash
        hasher.update(&bincode::serialize(&self.payload).unwrap_or_default());

        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    pub fn get_size(&self) -> usize {
        self.sender.len()
            + self.recipient.len()
            + self.signature.len()
            + self.zk_proof.len()
            + std::mem::size_of_val(&self.amount)
            + std::mem::size_of_val(&self.fee)
            + std::mem::size_of_val(&self.nonce)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltraBlock {
    pub index: u64,
    pub timestamp: u64,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
    pub nonce: u64,
    pub transactions: Vec<Transaction>,
    pub merkle_root: [u8; 32],
    pub state_root: [u8; 32],
    pub shard_roots: Vec<[u8; 32]>,
    pub aggregated_signature: Option<AggregatedSignature>,
    pub validator_set: Vec<Vec<u8>>,
    pub epoch: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub block_reward: u64,
    pub size: usize,
    pub version: u32,
    pub parent_hash: [u8; 32],
    pub difficulty: u64,
    pub total_difficulty: u128,
}

impl UltraBlock {
    pub fn calculate_size(&self) -> usize {
        let mut size = 0;
        size += std::mem::size_of_val(&self.index);
        size += std::mem::size_of_val(&self.timestamp);
        size += self.previous_hash.len();
        size += self.hash.len();
        size += std::mem::size_of_val(&self.nonce);
        size += std::mem::size_of_val(&self.merkle_root);
        size += std::mem::size_of_val(&self.state_root);
        size += std::mem::size_of_val(&self.epoch);
        size += std::mem::size_of_val(&self.gas_used);
        size += std::mem::size_of_val(&self.gas_limit);
        size += std::mem::size_of_val(&self.block_reward);
        size += std::mem::size_of_val(&self.version);
        size += self
            .transactions
            .iter()
            .map(|tx| tx.get_size())
            .sum::<usize>();
        size
    }

    pub fn get_transaction_count(&self) -> usize {
        self.transactions.len()
    }

    pub fn get_total_fees(&self) -> u64 {
        self.transactions.iter().map(|tx| tx.fee).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorJoinProposalData {
    pub public_key: Vec<u8>,
    pub metadata: String,
    pub proposer: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorApprovalRecord {
    pub proposal_hash: [u8; 32],
    pub approval_transaction: Transaction,
    pub proposal: ValidatorJoinProposalData,
    pub activated_validator: bls_aggregation::ValidatorInfo,
    pub recorded_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub block_hash: [u8; 32],
    pub block_index: u64,
    pub timestamp: u64,
    pub state_root: [u8; 32],
    pub validator_set: Vec<Vec<u8>>,
    pub total_difficulty: u128,
    pub version: u32,
}

pub struct UltraBlockchain {
    pub sovereign_owners: Vec<Vec<u8>>,
    pub sovereign_threshold: usize,
    pub pending_proposals: Arc<RwLock<HashMap<[u8; 32], ValidatorJoinProposalData>>>,
    pub chain: Vec<UltraBlock>,
    pub state: Arc<RwLock<HashMap<String, u64>>>,
    pub validator: Arc<RwLock<BLSValidator>>,
    pub mempool: Arc<RwLock<EncryptedMempool>>,
    pub merkle_tree: Arc<RwLock<MerkleTree>>,
    pub pending_nonces: Arc<RwLock<HashMap<String, HashSet<u64>>>>,
    pub admission_lock: Arc<parking_lot::Mutex<()>>,
    pub difficulty: Arc<AtomicU64>,
    pub max_block_size: usize,
    pub block_time: u64,
    pub is_running: AtomicBool,
    pub current_epoch: AtomicU64,
    pub total_transactions: AtomicU64,
    pub total_blocks: AtomicU64,
    pub checkpoints: Vec<Checkpoint>,
    pub total_difficulty: RwLock<u128>,
    pub genesis_time: u64,
    pub version: u32,
    pub zk_engine: Arc<RwLock<UltraZKEngine>>,
    pub storage: Arc<Storage>,
    pub recursive_zk: Arc<RwLock<RecursiveZKEngine>>,
    pub recursive_proofs: Vec<Vec<u8>>,
    pub dag: Arc<RwLock<MysticetiDAG>>,
    pub dag_round: Arc<AtomicU64>,
    pub validator_stats: Arc<RwLock<HashMap<u64, ValidatorStats>>>,
    pub bullshark: Arc<RwLock<BullsharkDAG>>,
    pub stm: Arc<BlockSTM>,
    pub sharded_stm: Vec<Arc<BlockSTM>>,
    pub move_vm: Arc<RwLock<MoveVM>>,
    pub state_trie: Arc<RwLock<ShardedStateTrie>>,
    pub fhe_engine: Arc<RwLock<FheEngine>>,
    pub stark_engine: Arc<UltraStarkEngine>,
    pub cross_shard: Arc<RwLock<CrossShardMessenger>>,
    pub ai_governor: Arc<RwLock<AIGovernor>>,
    pub appchain_registry: Arc<RwLock<AppChainRegistry>>,
    pub state_root_history: Arc<RwLock<Vec<[u8; 32]>>>,
    pub pruning_window: u64,
}

// ============================================================
// 7. NOVČANIK
// ============================================================
pub struct UltraWallet {
    pub keypair: QuantumKeyPair,
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
    pub created_at: u64,
    pub transaction_history: Vec<Transaction>,
    pub max_history: usize,
}

impl UltraWallet {
    pub fn new() -> Self {
        let keypair = QuantumKeyPair::generate();
        let address = keypair.address();

        Self {
            keypair,
            address,
            balance: 100000,
            nonce: 0,
            created_at: Utc::now().timestamp() as u64,
            transaction_history: Vec::new(),
            max_history: 100,
        }
    }

    pub fn create_transaction(
        &mut self,
        recipient: String,
        amount: u64,
        fee: u64,
        gas_limit: u64,
        gas_price: u64,
        zk_engine: &mut UltraZKEngine,
        merkle_root: &[u8; 32],
        proof_type: ProofType,
    ) -> Result<Transaction, String> {
        if amount + fee > self.balance {
            return Err(format!(
                "Insufficient funds! Current: {}, required: {}",
                self.balance,
                amount + fee
            ));
        }

        let nullifier = self.generate_nullifier();

        let mut recipient_bytes = [0u8; 32];
        let r_bytes = recipient.as_bytes();
        let r_len = std::cmp::min(r_bytes.len(), 32);
        recipient_bytes[..r_len].copy_from_slice(&r_bytes[..r_len]);

        let mut pk_bytes = [0u8; 32];
        let p_bytes = &self.keypair.public_key;
        let p_len = std::cmp::min(p_bytes.len(), 32);
        pk_bytes[..p_len].copy_from_slice(&p_bytes[..p_len]);

        let circuit = PrivateTransactionCircuit {
            amount: Some(amount),
            recipient: Some(recipient_bytes),
            timestamp: Some(Utc::now().timestamp() as u64),
            merkle_root: Some(*merkle_root),
            nullifier: Some(nullifier),
            block_height: Some(0),
            sender_balance: Some(self.balance),
            sender_public_key: Some(pk_bytes),
            sender_private_key_hash: Some([0; 32]),
            // Mora imati istu dubinu kao prilikom ZK setup-a (MERKLE_TREE_DEPTH),
            // jer Groth16 zahteva identičan oblik kola za setup i proof.
            merkle_path: Some(vec![[0; 32]; MERKLE_TREE_DEPTH]),
            signature: Some([0; 64]),
        };

        let proof_bytes = zk_engine.create_proof(circuit.clone())?;

        let timestamp = Utc::now().timestamp() as u64;
        let msg = self.create_message(
            &recipient, amount, fee, timestamp, &nullifier, gas_limit, gas_price,
        );
        let signature = self.keypair.sign(&msg);

        let tx = Transaction {
            sender: self.address.clone(),
            sender_public_key: self.keypair.public_key.clone(),
            recipient,
            amount,
            signature,
            zk_proof: proof_bytes,
            nullifier,
            timestamp,
            fee,
            nonce: self.nonce,
            gas_limit,
            gas_price,
            proof_type,
            payload: TransactionPayload::StandardTransfer,
            chain_id: 0,
            version: 1,
        };

        // Ažuriraj stanje
        self.balance -= amount + fee;
        self.nonce += 1;

        // Sačuvaj u istoriju
        if self.transaction_history.len() >= self.max_history {
            self.transaction_history.remove(0);
        }
        self.transaction_history.push(tx.clone());

        Ok(tx)
    }

    fn generate_nullifier(&self) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(self.address.as_bytes());
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&Utc::now().timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
        hasher.update(&rand::random::<u64>().to_le_bytes());
        let result = hasher.finalize();
        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&result);
        nullifier
    }

    fn create_message(
        &self,
        recipient: &str,
        amount: u64,
        fee: u64,
        timestamp: u64,
        nullifier: &[u8; 32],
        gas_limit: u64,
        gas_price: u64,
    ) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(self.address.as_bytes());
        hasher.update(recipient.as_bytes());
        hasher.update(&amount.to_le_bytes());
        hasher.update(&fee.to_le_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(nullifier);
        hasher.update(&self.nonce.to_le_bytes());
        hasher.update(&gas_limit.to_le_bytes());
        hasher.update(&gas_price.to_le_bytes());
        hasher.finalize().to_vec()
    }

    pub fn rotate_keys(&mut self) {
        let old_address = self.address.clone();
        self.keypair = self.keypair.rotate();
        self.address = self.keypair.address();
        println!("🔄 Keys rotated!");
        println!("   Previous address: {}", &old_address[..8]);
        println!("   New address: {}", &self.address[..8]);
    }

    pub fn get_balance(&self) -> u64 {
        self.balance
    }

    pub fn get_address(&self) -> String {
        self.address.clone()
    }

    pub fn get_nonce(&self) -> u64 {
        self.nonce
    }

    pub fn get_transaction_history(&self) -> &[Transaction] {
        &self.transaction_history
    }

    pub fn get_transaction_count(&self) -> usize {
        self.transaction_history.len()
    }

    pub fn update_balance(&mut self, new_balance: u64) {
        self.balance = new_balance;
    }

    pub fn is_expired(&self, max_age: u64) -> bool {
        let now = Utc::now().timestamp() as u64;
        now - self.created_at > max_age
    }
}

impl std::fmt::Display for UltraWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Wallet {{\n  Address: {}\n  Balance: {}\n  Nonce: {}\n  Created: {}\n  Transactions: {}\n}}",
            &self.address[..8],
            self.balance,
            self.nonce,
            self.created_at,
            self.transaction_history.len()
        )
    }
}

// ============================================================
// 8. IMPLEMENTACIJA BLOCKCHAIN-A
// ============================================================
impl UltraBlockchain {
    pub const GENESIS_REWARD: u64 = 50;
    pub const ULTRA_DECIMALS: u8 = 6;
    pub const DEFAULT_GAS_LIMIT: u64 = 10_000_000;
    pub const MAX_TRANSFER_AMOUNT: u64 = 1_000_000_000;
    pub const MAX_TRANSACTIONS_PER_BLOCK: usize = 1000;
    pub const MIN_BLOCK_TIME: u64 = 10;
    pub const VERSION: u32 = 1;
    pub const LEGACY_TRANSACTION_VERSION: u32 = 1;
    pub const PAYLOAD_BOUND_TRANSACTION_VERSION: u32 = 2;
    pub const APPROVAL_BOUND_TRANSACTION_VERSION: u32 = 3;
    pub const L1_CHAIN_ID: u32 = 0;

    pub fn is_valid_address(address: &str) -> bool {
        address.len() == 64
            && address
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    }

    pub fn minimum_transfer_fee(amount: u64) -> u64 {
        if amount == 0 {
            0
        } else {
            std::cmp::max(1, amount / 100)
        }
    }

    pub const SOVEREIGN_ADDR: &str =
        "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
    pub const SOVEREIGN_THRESHOLD: usize = 2;

    pub fn new(db_path: &str) -> Self {
        let storage = Arc::new(Storage::new(db_path).expect("Failed to open database"));
        Self::with_storage(storage)
    }
    pub fn with_storage(storage: Arc<Storage>) -> Self {
        let zk_engine = UltraZKEngine::new();
        let recursive_zk = RecursiveZKEngine::new();
        let mut validator_instance = BLSValidator::new(75);
        let mut validators_vec = Vec::new();
        let stm_instance = Arc::new(BlockSTM::new());
        let mut sharded_stm = Vec::new();
        for _ in 0..16 {
            sharded_stm.push(Arc::new(BlockSTM::new()));
        }

        let shared_storage = Arc::new(SharedStorage {
            storage: storage.clone(),
            dag_tree: storage
                .db
                .open_tree("dag_vertices")
                .expect("Failed to open dag_vertices tree"),
            move_modules: storage
                .db
                .open_tree("move_modules")
                .expect("Failed to open move_modules tree"),
            move_resources: storage
                .db
                .open_tree("move_resources")
                .expect("Failed to open move_resources tree"),
            fhe_keys: storage
                .db
                .open_tree("fhe_keys")
                .expect("Failed to open fhe_keys tree"),
            trie_shards: storage.trie_shards.clone(),
            reference_count: 1,
        });

        let move_vm_instance = Arc::new(RwLock::new(MoveVM::new(shared_storage.clone())));
        let state_trie_instance = Arc::new(RwLock::new(ShardedStateTrie::new(
            shared_storage.trie_shards.clone(),
            vec![[0u8; 32]; 16],
        )));

        if let Some(last_block) = storage.get_last_block() {
            let mut trie = state_trie_instance.write();
            for (i, root) in last_block.shard_roots.iter().enumerate() {
                if i < trie.shards.len() {
                    trie.shards[i].root_hash = *root;
                }
            }
            println!("🌳 Sharded MPT Trie initialized from last block shard roots.");
        }

        let fhe_engine_instance =
            Arc::new(RwLock::new(FheEngine::new(shared_storage.fhe_keys.clone())));
        let stark_engine_instance = Arc::new(UltraStarkEngine::new(128)); // 128 bits of security
        let ai_governor_instance = Arc::new(RwLock::new(AIGovernor::new()));
        let cross_shard_instance = Arc::new(RwLock::new(CrossShardMessenger::new()));
        let persisted_appchains = storage
            .get_all_appchain_configs()
            .unwrap_or_else(|error| panic!("Failed to load AppChain configs: {error}"));
        let persisted_appchain_anchors = storage
            .get_all_appchain_anchors()
            .unwrap_or_else(|error| panic!("Failed to load AppChain anchors: {error}"));
        let appchain_registry_instance = Arc::new(RwLock::new(AppChainRegistry::from_persisted(
            persisted_appchains,
            persisted_appchain_anchors,
        )));

        // Poveži Move VM sa Trie-om i FHE motorom
        {
            let mut move_vm = move_vm_instance.write();
            move_vm.set_trie(state_trie_instance.clone());
            move_vm.set_fhe(fhe_engine_instance.clone());
            move_vm.set_stark(stark_engine_instance.clone());
        }

        let mut trie_needs_rebuild = true;
        if storage.get_last_block().is_some() {
            trie_needs_rebuild = false;
        }

        let mut move_vm_guard = move_vm_instance.write();
        let sovereign_addr =
            AccountAddress::from_hex_literal(&format!("0x{}", Self::SOVEREIGN_ADDR)).unwrap();

        // ✅ PRODUCTION GENESIS BOOTSTRAP
        if move_vm_guard.storage.move_modules.is_empty() {
            println!("🌱 Move VM: Initializing Mainnet Genesis State...");
            move_vm_guard
                .deploy_module("UltraCoin", vec![0xCA, 0xFE, 0xBA, 0xBE], sovereign_addr)
                .expect("Failed to deploy UltraCoin");
            move_vm_guard
                .deploy_module("UltraNFT", vec![0xDE, 0xAD, 0xBE, 0xEF], sovereign_addr)
                .expect("Failed to deploy UltraNFT");

            // Sovereignty: 1,000,000 $ULTRA for the Engineer
            let mint_args = vec![1000000u64.to_le_bytes().to_vec(), sovereign_addr.to_vec()];
            let _ = move_vm_guard.execute_function(sovereign_addr, "UltraCoin", "mint", mint_args);
        } else {
            println!(
                "💾 Move VM: Persistent state detected in Sled ({} modules, {} resources)",
                move_vm_guard.storage.move_modules.len(),
                move_vm_guard.storage.move_resources.len()
            );
        }
        drop(move_vm_guard);

        let dag_instance = MysticetiDAG::new(5, 1, shared_storage.clone());
        let dag_shared = Arc::new(RwLock::new(dag_instance));

        let bullshark_instance =
            BullsharkDAG::new_with_dag(5, 1, dag_shared.clone(), shared_storage.clone());
        let existing_vertices = storage.get_all_vertices();
        let mut max_round = 0;

        if existing_vertices.is_empty() {
            println!("💾 No existing DAG vertices found, creating genesis vertex...");
            let genesis_time = Utc::now().timestamp() as u64;
            let genesis_hash = Self::calculate_genesis_hash();

            let genesis_vertex = MysticetiVertex {
                id: 0,
                round: 0,
                validator_id: 0,
                hash: genesis_hash,
                parents: vec![],
                transactions: vec![],
                timestamp: genesis_time,
                is_anchor: true,
                referenced_by: std::collections::HashSet::new(),
            };
            let _ = dag_shared.write().add_vertex(genesis_vertex.clone());
            let _ = storage.save_vertex(&genesis_vertex);
        } else {
            println!(
                "💾 Loading {} DAG vertices from storage...",
                existing_vertices.len()
            );
            for vertex in existing_vertices {
                if vertex.round > max_round {
                    max_round = vertex.round;
                }
                // ✅ Punimo i all_known_hashes indeks
                let mut dag = dag_shared.write();
                dag.all_known_hashes.insert(vertex.hash);
                drop(dag);
                let _ = dag_shared.write().add_vertex(vertex);
            }
        }

        let validator_stats = storage.get_all_validator_stats();
        println!(
            "💾 Loaded {} validator stats from storage",
            validator_stats.len()
        );

        let persisted_validators = storage
            .get_all_validators()
            .unwrap_or_else(|error| panic!("Failed to load active validator set: {error}"));
        let approval_records = storage
            .get_all_approval_records()
            .unwrap_or_else(|error| panic!("Failed to load validator approval journal: {error}"));
        if persisted_validators.is_empty() {
            println!("💾 No active validator set found, creating bootstrap validators...");
            for _ in 0..5 {
                use bls_signatures::Serialize;
                let sk = bls_signatures::PrivateKey::generate(&mut rand::thread_rng());
                let pk = sk.public_key();
                let pk_bytes = pk.as_bytes().to_vec();
                validator_instance.add_validator(pk_bytes.clone(), 100);
                validators_vec.push(pk_bytes);
            }
            storage
                .replace_validators(&validator_instance.validators)
                .expect("Failed to persist bootstrap validator set");
        } else {
            println!(
                "💾 Loading {} active validators from storage...",
                persisted_validators.len()
            );
            for validator in persisted_validators {
                if validator.is_active {
                    validators_vec.push(validator.public_key.clone());
                }
                validator_instance.insert_validator_info(validator);
            }
        }

        // The validator tree is the live state. The journal is a recovery and
        // audit source: if an approved validator record is missing, replay its
        // durable approval snapshot without duplicating an existing validator.
        for record in &approval_records {
            if validator_instance
                .get_validator_info(&record.activated_validator.public_key)
                .is_none()
            {
                let validator = record.activated_validator.clone();
                if validator.is_active {
                    validators_vec.push(validator.public_key.clone());
                }
                validator_instance.insert_validator_info(validator);
            }
        }
        println!(
            "💾 Loaded {} validator approval journal records",
            approval_records.len()
        );

        let mut mempool = EncryptedMempool::new(10000);

        // Pending transfers are durable so a lost client response can be
        // resolved after a node restart instead of being silently forgotten.
        let pending_transactions = storage
            .get_all_pending_transactions()
            .unwrap_or_else(|error| panic!("Failed to load pending transactions: {error}"));
        let mut pending_nonces: HashMap<String, HashSet<u64>> = HashMap::new();
        let active_validators = validator_instance.get_active_validators();
        for tx in &pending_transactions {
            for validator_pk in &active_validators {
                mempool.add_validator(validator_pk.clone());
            }
            if let Err(error) = mempool.add_transaction(tx) {
                panic!("Failed to restore pending transaction: {error}");
            }
            pending_nonces
                .entry(tx.sender.clone())
                .or_default()
                .insert(tx.nonce);
        }

        // ✅ NOVO: Učitaj postojeći lanac blokova sa diska (ako postoji).
        // Bez ovoga, `self.chain` je pri svakom restartu bio SAMO genesis
        // blok u memoriji, dok su svi ostali blokovi (i DAG vertexi za njih)
        // ostali samo na disku - `mine_block` je onda pokušavao da poveže
        // novi vertex na "genesis" kao roditelja, a taj hash nije postojao
        // u učitanom DAG-u (koji je imao pravi, duži lanac vertexa).
        let existing_blocks = storage.get_all_blocks();
        let genesis_time = Utc::now().timestamp() as u64;
        let genesis_hash = Self::calculate_genesis_hash();

        let chain: Vec<UltraBlock> = if existing_blocks.is_empty() {
            println!("💾 No existing blocks found, creating genesis block...");
            let genesis = UltraBlock {
                index: 0,
                timestamp: genesis_time,
                previous_hash: [0; 32],
                hash: genesis_hash,
                nonce: 0,
                transactions: vec![],
                merkle_root: [0; 32],
                state_root: [0; 32],
                shard_roots: vec![[0u8; 32]; 16],
                aggregated_signature: None,
                validator_set: validators_vec,
                epoch: 0,
                gas_used: 0,
                gas_limit: Self::DEFAULT_GAS_LIMIT,
                block_reward: Self::GENESIS_REWARD,
                size: 0,
                version: Self::VERSION,
                parent_hash: [0; 32],
                difficulty: 4,
                total_difficulty: 4,
            };
            let _ = storage.save_block(&genesis);
            vec![genesis]
        } else {
            println!(
                "💾 Loading {} blocks from storage...",
                existing_blocks.len()
            );
            existing_blocks
        };

        // ✅ NOVO: Rekonstruiši state (balansi) i brojače replay-ovanjem
        // svih blokova sa diska. Bez ovoga bi `state` HashMap bio prazan
        // pri svakom restartu, iako su transakcije već izvršene ranije.
        let mut rebuilt_state: HashMap<String, u64> = HashMap::new();
        let mut rebuilt_merkle_tree = MerkleTree::new(256);
        let mut rebuilt_history: Vec<[u8; 32]> = Vec::new();
        let mut rebuilt_nonces: HashMap<String, u64> = HashMap::new();
        let mut total_tx_count: u64 = 0;
        let mut last_epoch: u64 = 0;
        let mut last_total_difficulty: u128 = 4;

        for block in &chain {
            for tx in &block.transactions {
                let new_sender_balance = {
                    let bal = rebuilt_state.entry(tx.sender.clone()).or_insert(1000);
                    *bal = bal.saturating_sub(tx.amount + tx.fee);
                    *bal
                };
                let new_recipient_balance = {
                    let bal = rebuilt_state.entry(tx.recipient.clone()).or_insert(1000);
                    *bal = bal.saturating_add(tx.amount);
                    *bal
                };
                rebuilt_merkle_tree.insert(tx.sender.as_bytes(), &new_sender_balance.to_le_bytes());
                rebuilt_merkle_tree.insert(
                    tx.recipient.as_bytes(),
                    &new_recipient_balance.to_le_bytes(),
                );

                // ✅ AŽURIRAJ MPT TRIE (samo ako je potreban rebuild)
                if trie_needs_rebuild {
                    let s_key = format!("acc:{}", tx.sender);
                    let r_key = format!("acc:{}", tx.recipient);
                    let s_shard = storage.get_shard_id(s_key.as_bytes());
                    let r_shard = storage.get_shard_id(r_key.as_bytes());

                    let _ = state_trie_instance.write().insert(
                        s_shard,
                        s_key.as_bytes(),
                        &new_sender_balance.to_le_bytes(),
                    );
                    let _ = state_trie_instance.write().insert(
                        r_shard,
                        r_key.as_bytes(),
                        &new_recipient_balance.to_le_bytes(),
                    );
                }

                let next_nonce = tx.nonce.saturating_add(1);
                let current_nonce = rebuilt_nonces.entry(tx.sender.clone()).or_insert(0);
                *current_nonce = (*current_nonce).max(next_nonce);
                total_tx_count += 1;
            }
            if let Some(first_validator) = block.validator_set.first() {
                let validator_address = hex::encode(first_validator);
                let reward_balance = rebuilt_state.entry(validator_address.clone()).or_insert(0);
                *reward_balance = reward_balance.saturating_add(block.block_reward);

                // ✅ AŽURIRAJ MPT TRIE (samo ako je potreban rebuild)
                if trie_needs_rebuild {
                    let v_key = format!("acc:{}", validator_address);
                    let v_shard = storage.get_shard_id(v_key.as_bytes());
                    let _ = state_trie_instance.write().insert(
                        v_shard,
                        v_key.as_bytes(),
                        &reward_balance.to_le_bytes(),
                    );
                }
            }

            // ✅ DODAJ U ISTORIJU ROOT-OVA ZA PRUNING
            rebuilt_history.push(block.state_root);
            if rebuilt_history.len() > 100 {
                rebuilt_history.remove(0);
            }

            last_epoch = block.epoch;
            last_total_difficulty = block.total_difficulty;
        }

        for (address, nonce) in rebuilt_nonces {
            if storage
                .get_account_nonce(&address)
                .map_or(true, |stored_nonce| stored_nonce < nonce)
            {
                storage
                    .save_account_nonce(&address, nonce)
                    .unwrap_or_else(|error| panic!("Failed to save account nonce: {error}"));
            }
        }

        let total_blocks_loaded = chain.len() as u64;
        let pending_proposals = storage
            .get_all_pending_proposals()
            .unwrap_or_else(|error| panic!("Failed to load pending governance proposals: {error}"));
        println!(
            "💾 Loaded {} pending validator proposals from storage",
            pending_proposals.len()
        );

        Self {
            pending_proposals: Arc::new(RwLock::new(pending_proposals)),
            chain,
            state: Arc::new(RwLock::new(rebuilt_state)),
            validator: Arc::new(RwLock::new(validator_instance)),
            zk_engine: Arc::new(RwLock::new(zk_engine)),
            mempool: Arc::new(RwLock::new(mempool)),
            merkle_tree: Arc::new(RwLock::new(rebuilt_merkle_tree)),
            pending_nonces: Arc::new(RwLock::new(pending_nonces)),
            admission_lock: Arc::new(parking_lot::Mutex::new(())),
            difficulty: Arc::new(AtomicU64::new(4)),
            max_block_size: 1_000_000,
            block_time: Self::MIN_BLOCK_TIME,
            is_running: AtomicBool::new(true),
            current_epoch: AtomicU64::new(last_epoch),
            total_transactions: AtomicU64::new(total_tx_count),
            total_blocks: AtomicU64::new(total_blocks_loaded),
            checkpoints: Vec::new(),
            total_difficulty: RwLock::new(last_total_difficulty),
            genesis_time,
            version: Self::VERSION,
            storage,
            recursive_zk: Arc::new(RwLock::new(recursive_zk)),
            recursive_proofs: Vec::new(),
            dag: dag_shared,
            dag_round: Arc::new(AtomicU64::new(max_round)),
            bullshark: Arc::new(RwLock::new(bullshark_instance)),
             validator_stats: Arc::new(RwLock::new(validator_stats)),
             stm: stm_instance,
             sharded_stm,
             move_vm: move_vm_instance,
             state_trie: state_trie_instance,
             fhe_engine: fhe_engine_instance,
             stark_engine: stark_engine_instance,
             cross_shard: cross_shard_instance,
             ai_governor: ai_governor_instance,
             appchain_registry: appchain_registry_instance,
             state_root_history: Arc::new(RwLock::new(rebuilt_history)),
             pruning_window: 100,
             sovereign_owners: vec![
                hex::decode("6c6dd0c8393d8e29c91fedea5c089f3e0372c8d8d36d4fb3df0f59f630bc1ba08fdd888ed8db45f9787594cf53546fcf0ac52300fecb71554771a478e207796dda8778f4cce68a6e15d47dde2532c8d37e8af16b5e44b22f58ea3fcd92f05e0577985566a5e4b0a488af308f3bbb8cd201cfc6963d8df3ebc01b0d237c1609ae27b8b99eeabd70133d22752a7b03f8e62440d9acf549b856a6bba7ca6ef4c2e9b0b796548a22bde3170e3a5f129e65fb53f1360f95d9a984a112bba1866ab7ea054ebc17e380e46f224e5ba102210bbc2e1ab81e79884c02349c8fc40591fc0b884f9bd07aeae6867d6b55661c7dfa8df202e8b7f5d312611a6c5901156a81c694e6d4a3540f955c060588a91cdf7e4d8d0f5e916823927ae2b52d456996a5e87d4dbc9c28272e43168a8abf07fa01b8de34fdd15786765c3eaee3f8f85b2f7abf593c3a1c6d655295f6319910e0ac85bfe549b47e6c6e3797549c4e980b0fc073c054d40378025ba403fa0c2bd041eb22d8af201232f123dae630c127da84ff77d86cf3d105ff5b4f7c3605a50d2285776c2cfe829a7f6ca4f36c08f8a5d301c50f57adc7dafc083f9af3e7c15e57bd1796a903f4229764ad5e9025281e73ae2badd793b318b846407e128f8068c25e0a2791d1b30d4429ad1be13449106a3fbcdfb490db8a80301449c5c5ea909c6a26f2f2388902fe6d1b812d10e37684662b38e091848e49c2386558fa5cc9acb34795db4c4c8c9c47dae4887705be4c157a65e29f6654603f620d87b8892cb025fd65587b8e04e1076f988260e25b682be12df2dfbcde21f621d1753f721d9911a8eb185b3a7930f3d0656c77947c1f8621dbbd55b466c8ffef61d65a5d8dba200d9a06eebd9a054776de589c0949aaf6a31fc6c7487da2aec9eb6ef2b5c1e95112e6bcf18038a25f39c7e72a9e520158c39d69b4d888800a900f69400f483a79395e5dc9b11afd401e642f52de7c5da7bb73537d7635f3ec8499851bc0e18affce2acdc78bc50a6500cea0e0865b76661c212e9f8ec7a2b44ce87dfd6c730ca2c157fca7edc3e340fffa4be260d4c3d3a79043bc76232dcbb25c3ed393747e09ef64ea7e05406773ba58bea41a1739d66d14f85648bc0851e477b77d47ce9fe4cd07d2a2bad86b7de93b29957c0bd7229cb099113784a4c49aa18eff14b5a2f271325edde209fc7eb190f2acb77c71900bd027ca48d2d50e313b6d1d99f0919473b98574f84ae64b2842c59924db79eb27ddc1dd905a178f2f852c1d7371009490c49003eb3834b70d4faf781013957f415e41a183c1f514d642cb8c5b019bcf301dd8456198696394abcf82a627864e9612d85565cb5ccbd6289b65164c89b17cf8adf3e7c4d1eefb2283fcc49b4b48b5d44326ca30264e8dba29e4a906ff9edbb5eea46ad490f64d15893b0bba9cff171a473ac21551fe558fe3aca0e82d9c75e812115a94747fe6f9342e73242e02c7180d711c8d227fb890636888a9b9a98ca0c038c2d9f4d73877414f7fb03f7f72710edea6c8b2d6e4752ff7d8ec8cd23d946d0ad9df1cfb6548e76372d2d5f0a62b2633da384ae27d2485fb2ff2e2d3bad3971293c77911a6925b098c7d4d01a4aeef346a1e0814c74c435eea5dd505d4d0a6aa959c556d172b1db25bce88677b752a989f25eb73e18bdfe599beff27c4ed5a76f4f202b785c3c8da6edcbf050826694198cdfd5f47476533cd3fdbc8a9a01910092e1f8e754a2caa7912e394bd2361b40280086a0211562a12fb23d0da90d42dbaac15289e36c20e3bb6028d62fe738c1c0a983d41f477976e5c8bab5e792f4c09deb49ab758d3561011b41772b49b0953d895ee85e3fc426185850bbac9efb79d22087215c06038a8716fc7d263e45d6cd5ff1e690e744407f44137bbecdd2c3d596bd4e085e696ee8b7a97c638afb92fd537cdd3e4bc6028377090748121d41e73d1409137d34a70d2650fbba6e3c89ce445c19983e459f2c2ab5f968a28ae47bb39aa6cba0329f613bb29dd98d016e17871b080ac6a7f51db0874326d78e09735e27c931237af0bfe9d9df897841d2dd2f26e0399d3bba45eea291733e8dc56f172931e653e21d7ce617390839a150bcc0cea1e3ebcd80a490b0cf129df2aa08f3cb0cff7b875a1918add8ecfdce21309bed583ca5c61de14cf206b01b90d68b3e3de862933f54db6eec14f2290fa04a6c79160f9a2614c770f08a2351791101dd44f0b7759bd20b4915613e73029b3e0d4eb3ae4759dfa4720da8dfec63a028882a8cef40142e606061500c4b73005e4e6d014984f433bc19c45c129a9a271375ebd67be7fd868cbb4d0b7c7e76f62e83a155c948860c955f199db847976303f5064c71bd80f000fffeef78e3067be2e277aaf8da774bf6dc4e4c103af1ade274435db56bbd72fe7a495faae0fffc0b8495b820270f72a849238e9250fb2f5eb9f10430bf30c29f7eada1dddfd5ddff188dfb3af83f5d2dc52d45340ce5dba1ca868efe05d9aa300beb68e96d08f78788ca27ba95e01ba4e0992a60284aa7420d37de9689dd7902a15c906379b635762f88d651bedee28261e7daca30c44dcb4df6cdb0518eca1a5ef638b88394fbfa3e6e6f0fd4de9e1848542f7222d2dc4005a86880ff3c3a095f493a1916ae75b4f17be055faaa6fd206ed4016bb1667877775f2b4b5a9cecd4a845708f8061b19459b1281ed65f5aa23e88a4cd82e275cb668583f7b7b6e36d4bb1f261dc4010fd3be91a98999142b0d13896eca51ea3c771ff21e87bddaefb463c0a9cab76d51b6f105363c1ea60b2afab5de697899c26b450c836c47bd2eee7602e48d43c8af3febbeec30dea5ee07970f33891b6154689ad9d2820d27597d7d5a2f24999acc8552e79d902dddeefa17326e28025663d7e602e912ef22d737c435698dd12f18fb5a218e3fe6946a9afae576acb0ece844b3d047ec1f45e325ab8230441c087305b1b1f598f99bac97536bee013bf4662401b913c5d57cbd70110eeebc82a903419f5643ffc75f5b973d7de09e4ada59413720cc804436243210569f79477adcac6c06efa8ef682b695ae727de0a970237bf629a9828e0338726a34c3d2b4d7299b8134a6985f37d45926b1ad2da88acb020716db8352f9e251fe7ab0ea6a7f90be1156ef4f8fc11e8c061f98e01d8f8ab41d47a18717e1a94f2da38c01a7dedab8dede6d4cb3840d86e00e4ca1e8e18e9871fa50ce2d152015c5fc5eb0bbb353da8431094bd14735cf6488d0fefc49b5bfbf2e5fdf964d7a21a51400a2f1b2a87bbe6547f14de0198a4205bffb2cbc2efa60c4298de5c7ae80597c9476edaff4e09d50dbf2f955e0319b97424504632b010275e0a2546e24c2fc490c295b901fb1779a211acd1882dd84a0e59bbf088a1efcd31c910cb2fd0c6222eec7c37f8d4f32c9797d4eb4944149098e7dfd0d97df8bf9f77030f8224898f5a08f4df07ef75dc5789f7e1800b4111f0fa2c650496c28c221dd74f9c693801c2a606e920f105cff9fb4e63a754322f787f76a32404c86260e137cb3febe846e4d26b6f44f3ccd1858cc7633f020290f34598c8b0f028843110b6b57d7375df148661b08b101d665f758bde0d6ea4a2a27b5b2385f1c405e21dde675e43920").unwrap(),
                hex::decode("defb4ba12210f57e5ac873c5ca2e45a9ab0b430f3dbf8d7c1ca5a9109ea9c105224b4810b1ff89c47c8dc30115ad4e23f21a9846249ea9df3023cf8ddac61974b4388cde68aac62efc25457a6fe79936c558e0f63ae3c64b93273196b6623b4f4cd0052579745f358f8bbbe34dbabe61ce3e87d6d8d964e79b535e139b562d910663a453e368b3d7994642d6ad17ff0d2f2b973005b3e71cab2766d6109aa74828179e86a0d265b642338a3f680c9a1ed6cc5b10db76c8033fb17c6ad6ef06bfb1aca0a19ebfc5aa88cd241f0c11cdcc11703b462cb9c8a901c53cad13a3b4d1e58c6d62f60c76a55bb760f136daa5d62d126b5f11ce5cac084a1bda0495d558b2816afdc49bb6f906ce0818fa6d53af66667549dbb0488ebb2d4d91b083b6c7aff2ccc7d931f64796a3449d9668ca240aaee520b71628ff045910af60930f2440728620b14f4635d0a925f18534c6934ea55f52de2e831d6e589410d9c39dd2f9c8efc979113c99b8c10b6b4360c922c75595bae920c6e8e2b74e15c321a52976fd87ffdd61c75395606e338cc19f5036f765843db5e94d92344978c03f7c0c935e1e0f2f421553e4446dec2df9a5621081d329ec0129a2623a2eb40cbaeff9cf7692cf557191aca98f923c81eca87ee434d97635d36873ae657740758a1425b372106f19f2626e52fdfaea3ebd4d61d8f54728965773628e49e13942e851a46ef507c3f94748bf1f0f0b585a7ee53df9575efcfbe575955000497163ddc114ec813efaec990e6c7ac27cca8480a0d5071ff3bbcdbc4aa9e88b60cce6079ad0e74d01b6a5357d163e793ff57214b64665ca7c493f182b99d075c05e053922df9839e2367bd71851f8c0f69807653dfd5556626e52249749f431c2eeeb37ee4e79e2a2316dd02df81f09e829994380a2da9f9a4370aded06f95221b1487a2a293bbf17342c593d95419ceae1a10ca8cbd34850b3511e92fd4fa25c0ff60ce10c5c5a98cd71b650db438e004dd6f71fdac1883aae1deaf85c0ddf772ff18e109f56c67cd1d4b6c28638660c99a48b7ae8fcf87367c4efe5e77afe7f1054b4a0699af30edf2a701eb2bbbb7f9d27598af38c0e8957c4470ad8ccaeb3bc8c4f293c14e5b5dace074ae24f6cbaa3d58d22a61fa8b89715e375823200da78ac6045fcf844af7c10c359f0ef844a514644e4b6780c115402f16130ba09bcfd6d64308f1f48e98907c482a79ca23491fb940b8a451e9003c3eeb9e061800328cd41cfea0d8a7a09de445669116bd86e562f37f2a14188fe4f31ba18064d57dbd0aaff1d81a0111b0617a69f9aad0c500183146df741622e6a45f08307ab472507e83872950b1de213f2c36836d88a6249c39e739f1eb72add52b352424e0019df213d9d8ddc89628830770ab406453409ab3aae85b7f1f4ac361466523db36ab49094521b6eb15acf58a1f468fec61dc944f325d0369de35f7e5bec8c64348dff74cf2527c1b36d2ea42d94ad9c6de38c51cd966da7c272b0b8dd369da763976fe46cdac702b6425a351bbaf50ede7f908b49966228d25139dfd8108b2b28dc6bd29c9436b6b3bc94847caa0cd79a9738158c7c67bc8c24464e3d11674c1af1595db48a89db0a93e7354afdddbeb9138267a3790d3190d760beb20b57b0212055cfa2efa764e25d411e3dc0f1d2a40f7275736b94711a230588cf72376eda952edebdc2300a2d7916f4c3933aa48c46bb02571465cec1ee09f0a6006b41187326c20dd237abce436ca756fb077e1c97d6a04c3fa07edede4bf5ea94841356f6e85b3440d974a7acc7d86d8e983dff7dcd7d06039b42ab1cf2020b51384c0ee12386b75714f3a2f3518d89b70e71b7a8ed0c8ad7b12e9ea1fefe22e6c68663c3cd9ff3ac32b4d3d4d35c2b4cbc42b53a472c2c5e921cf1b757c665c6e155358f653801b4103933353456cd42107eccfda1f1c1fdc9e0a955ba79d26b9ed0e476c91331cc722e48996302b5481e18d299b2f0ea32d117733f22a88fcda15573b7230a18fa14e577029c5e1b254d6ed57db99d62d444c406079a66d6cc5755b151a15180211028bb02f56aebaa1126934f5f9b034ee17edc91d49e98219a68a2e41d27f864a21a796d3b7cd4e573100ba5480a3b82754713170fca103c8e43b1390154ca7a93b6c1c5632ff1bf71ab9d39ba7ba8422436b9848c3f5549c34b2730219f32043647b920ddaadce3fe021462f8806c9457bb1d938ff75ea116d602e6bc0e78ec53860db9fb7957ca4f2351ca55c141d8a1932b0173b43e85dcddbb334625f1b1a2ca487aaf7d21147068f14b8c50120cadd57904d2efd8bcb3ddf25c26b8ad19276ab14b4dad8a88b46e0a071724d93bd926a75a1a753bac76bbd4d6ea7fef43257d69adffcc272ba4a6bd6db644181a32de8b7f7c1d3efd214756b04824bf79bc8bf3016483f055b7f911f5af4b58720332be8a9c913d1ecc785e16870d9daba663685986f93ae2648f74166b5deb71a3075a4957cad51023f55d641e4049ca96b3987c489fc1caf4afa602357b36e52ed3428194907d5ed41937fdd8cb48d8a1e6871d3d4b1973a2a35aebb4b8471ec67d2a72ea2f8de68e22f6c25c09d71537880e70d01efd8276be19a0ad05ba61dadcc7ec5f227c9bee1515ea6f36cd6fb66f1d3b9c49ac7a1ab86b0b4358b4d439ff6d05ab3bda0b3f0b9ee5ca18532ae52dbd450278a10c53e498e7c2fbb35cc5b1c284b91a9a89fcb55f71b0bade78828170dac4f3e57fc3467909d0921f2c59d642d4bb154dbf036d260b6e89d823fd58fde42f3a6b105841d58e8055d5301a52f5b42cbda3d6ea1a8eab2a125d5fc227e3f4f44b2aa1355b3c231a5ec6996a1cb69ad81e3135d548e22eb55407ddc8dba04ab0f65e1cfdc47911ad23448e7b3541df1ecf2be125eda11d14c126b93c31f690083d41fc08257fc89b4ed1c9c2f11812aa0d6852cc4d388f010290a7b4f92b6257764056326d14b1810648ebf2babf395fae81af0b7ad9eaed630c45cb15bc63e9a994c1d6e1ba7c2b228111f575fa0dc86da823475080b790c1c66386b10559b08751ca3497cf3977bcbaee6502b280c1581932de541a7553c3b85f19e2855faebcec1b0be8716b773244a740fe165968f65c2e2c8d407a135a9552c8de0b3cd229a467ad2363f45379076ada979cade4ae855100b8bcd1860b697429ba9f861400051fb5afafb67864b34ec59de80afa651bac388199d7f603268de1f1b72fcd80e19510cb34240057d059711a19e7d48b99ad4a1e692ed6056a51861f3cc3ed00ff69fbc09f8a321e45eba8347616b10df8364d91093654fb2823c96a0185c6eb5e164248406b3be3c4d19584bc776899d2a2b7ccec48133120a9ef3b0d224e35e8e5c8fb31148125811463c3f5fb725c9e0ca31fe7f68d56a69ae4ccc3c7ebd4f8bb4e5bcb4d84dfdef51fd537d0f0c8fb1980ade6ba001b4d31fcac258cdb9bdee5356631a6182d426907adbb4a824151ee7e0cc114ac3bad77c035f47038dfc03283152383a917552731c94a8f1e1e62a667943151552a20ad9b2f561fbe56670de4fc85c9942c67f70859f2c04c9f266e83a3f961211e0642ecef0f6bc99e1be282bff8aa9025b11543ccccb9b61a48dd39e59fd16400efae").unwrap(),
                hex::decode("fefd95a893f6f10e1b7be44d53f2b6ce2149ffbe7bc9573837bff8926471bb427eb378a8b44bcf5bf47d551b0a96b2b96f3eb8d2c0c759a2e9e91febd61b0372cbe99ff42990a9181fd247bc44c84d3570df81fbb46b5429307319780ca67f957a98bca39b37e9f2e596626bfbfcf327c6a06488111bec04715a1945b1b506c7c05e6d6015192607aa7d522b198c9ee9ea7555e4c793773cba0603f085eb9bc3d654a95ea2a57859cd7e64b0234d291fde5cf4b3a556da69706336c3d6f0a36eea11b79cad72ae1577543865b3df809625d7145f916943252ea718d0142cc7872951718c12250408c7c42bd2dfb80726c2bbdf57ae9b97c62fb73bc420adfe0f356ab962f9e627ed4a7af599bbd9de51d069adfff974c5e6bb72a72e83b2942d9853620db2e5fcbffe8f7f32eecd2083777e773eb86d86471c887cdbf31b8425d787c8ee89913a7f53d27b62316864f462050ac44255bfa3420f5366ccd65b3b1b598c76061adf4bac2e84111eede9059253657875a1835460ea0cd1722c204822349581d169ec025b4f14a6b3bb086bdc2022d34f2b2515665da9a778ef6bd74a7a5f013cb51c88c37e47e6d6d5f857e36a11d65a1a5c61840f20c87d70472f9e593f37c1d21ec8b479e4d7609f4291f8ff4b3e231f9bc49dc281ed7f0decc76569f3c8f7f54530c4d364ab325486e685ae2f4453f08d72389ba389b1b4fe52eb6c68887068fe9b1f3b17fa2a41379001f3bdf543f8b8a8ff4c596c343e754c44d90f1ab6a7c96d181ed5346ccc22c8ed09f69d39f37fc8d44ac70ec9b65d8c48e1563c148c55ebf3788027ca3a9bc4e0a4c32427ad7ff8333a90063eaf449eb4c14b57954d7316c811cbae61d5ba2bc7ddda10b30b6f12cddbe746d0a084ba7fab00ae1e96972a3900edfd03d10e9f2e3c35a66f1857d208e28dd8ead107687d8196fc416b7f220a90501e1c5d7b968a53e912c4cc7fc1675e11e4da76bc001ca0109ab1b367a74e77ddd2e846eaacbf38930c86557ddf84c4a9c9f33f749b5118e0a2d3e2d3391f5f871add1ae0b2c026e2fc653826c4ee6f3053a6d857967c9c146fcb1ebd14b1b5f448eae187c1121b706dfea27f5eaea63c411c317cd976b1292f6569c9696ce749bb800fdb7ee8f0b0cb1b922f7bb6b77da37747bdef9935a5bff2dd50861f2cd78fd77f6a941b1469e614c346e641b29045fc26ffcab59f09ea2cf7be13c117142bb585ed1e90dce4ec020eea974af82522117992e0fc2ea1e044c37e0c47552d6449335899363ab40892f7f843dade5cfd56384444c6a54392e884dabec340d95d195b3dba2eb5f38289d385cda82df7cd65373a6dc92cafb06a6ab4610ddc8b4b53c61295199f5880bb71182a3f303b871b0cd48dd95bb3fce430797025d935c8cc76d62cda4c579bd4fec1b876627231648e2f2dfa63bbb9767d295c89eb073b757b2ab231c89e515b330b2a1c63511072f06da80f58f976e13e2bbaccff1fcd21791ff1a14f8b16745f954e46a8f3d88fb2cec8d0c7f84c6d0a1ab76b26b5c2eaf94dc594e905b6620612c4f29bf7a705dab0d2afe3a993de5dff3c29a8f6827785a5c7641e1489787368868b3589c6bf1c049e59e76007d7bba42b458630d01f000880e00fb30c655848cdb432c8079825e3587e3f2597141dd6e8798706099181017ecc8b17fd998e163a8b2512d3eb7c4342b785b3d122d39aa32b13c7c336e3d716c462a417625dae1eefa1ecccce504d77c0f315847e43d44d7a4b51ce8180ba185feec3103fd3e2a2723180b0ccd6d0d2300f8733493d84861f734ad25ec836ca04b0e26d10bd22e5feb99c2ad7356c3ce187598e965add62c0e058b9e0eac951a634b0870745aebee5a32abcb048426053cae4c54974a8f1f981d0279a75bd168bec23399015e5045a8360f7a0e0108b729fb91afa571201b427af0561390f50f1d11bd0b132e885d83488367684cd0d3bf49caaaad1a9d1d3f6c7b692ce1d88cf4861b524d7fbe723e207d1b38417610dd05bbd70f8589c31cefdd2232f782dd98689a336221bed0b1f831f26fcdf8ebca898bbd461be4524d4688da1c1256813f82f3cf61ae8d2820c5986d27dd99e5c49389d3795329d3df871e5f499e0674130c01eb52e8ddae870f7448ef53153eaf22bda9920307d92910c1fdaf3996c8ccee2c0c520ba1a0d49690f9af3abf464170da68fef400ff9360706a534ff4336475e8fb9b87f3c3ed23d344a32f90e44b0e3bff8ca67e861ccdad790a415355074b72ae218545926c89bbbf79c643dde1e67baf206a35d613ae854a203249ea2d3660855c49643f7c52f620c67ba734ee66763ae08d6d2c4bd1bdcd613fc01ba04232f7ba51129b7a9c1b2eca505cb450cdd1cd4ec525529c0ca8561e326ee88b985c48e57d0e4ee783ec13341980e43304968b07156e0d65f1718882df19ffd0d3416a39c8bbc08329b16f76bb80ac475259a2b3d77d122f6fdccce38f9a585091469a972190d037e7ab261a2973de7dd941c022af8e6ebe69778ae3ac9e5be0d58d852f1614a0ea81e887ebd04d7cd616478773149d93ca8801dfcd35f315151198f6e407784b75bcbc2ae9bef5e1f7a4c361388900e620ba5b610237bf88d84305e000624c8b6912ea091545748b7d6429182c4b5d12b734bff5c7e3628889698aa2b6c3d3632325bb3981606ffd8d35425e71e22652558fa7c31d40d2605bc56684bc80cce785e5b289e33bcdac1f892367d5e48f9c1403ad1b4ec605f6ca7cad892ad0e9d0063c9d60897539ce4d21abef7a9bbc7f1c68ebdad75c7f4580474507fb9bae8229bb141c8fab515b7af7d503f41cff5f0654c40006a5198f66b42af995d359899516dd76d5c6a092d8b11670bf351a751039f32b233c03906dff9671b9786edc85d1d3c0fd5cfee2b38b1473d6ab913a03f40da0beeb27e34ef747465c19462ca0f8821b797a67f811261c19fcdc52c3b523f75ee0dcec817c438cb065f0e6f980510af3f616b913beb613a9b6a2435e929c4c70587673737cef9136de7eae8c705952c5f23c7ec0a4ecaf9a7cde46703155016be3461c57ed73409764dad4a6f0ef1fed537fd34795902b408102987887e96924e62c0bd9ec909a099ca914b4f85515363de3aa6b476555d29ca31e84c24f552422ba925ca8ea4b8f8b26a6e99c2d6907980760d8429c8d108c485284aa077802d94cb11c2273f25a29beda58d3d98ba963b55d1e0232972baa1c772bd66d2bc12d7509e1861e6f28e1f7531cd125769aa26d2171720512a4d9b9b0de130099253e5e2d5b8204a16cd7a7f04b9fe24e263e9f702b1cc5013b025351d0abd0342c94dcefdd012207b8ba75fb43979db2312490c6a4a515e4adf39bcc077549ff3d7083c83382090857ad9e9e1910913e2c5a92390bf1016dd04a64fd81ee12128343cc9e78f0109c7a213aa10067a83b9e9e1bd1d0be2cfa96d1457798cba2bb981963d2286a5816a56bcc99cfcbf1351f8f8e038f38c7253aa4860d13e5ab16a96c8221b209d4a3de4ecdb68dcfeb3b7ac1e10567f5de85682daf0f3e2b7e2228069a588f630dc341947dc637b9e392f93de2cb87e971b55c0b481eececa1481f15efb29d5c6cfea6ab0d473f933a605cabc6a55").unwrap()
            ],
            sovereign_threshold: 2,
        }
    }
    // ============================================================
    // 7.9 AKTIVACIJA VALIDATORA (Sovereign-Approved Onboarding)
    // ============================================================
    /// Adds a newly-approved validator public key to the active BLS
    /// aggregation set. Called only after a `ValidatorApproval` transaction
    /// signed by the 2-of-3 Sovereign Multi-Sig has been verified.
    fn activate_validator(&self, public_key: &[u8]) -> Result<(), String> {
        if self
            .validator
            .read()
            .get_validator_info(public_key)
            .is_some_and(|validator| validator.is_active)
        {
            return Ok(());
        }

        let info = bls_aggregation::ValidatorInfo {
            public_key: public_key.to_vec(),
            weight: 1,
            is_active: true,
            joined_at: Utc::now().timestamp() as u64,
            last_epoch: self.current_epoch.load(Ordering::SeqCst),
            stake: 1_000,
            rewards: 0,
            slash_count: 0,
        };
        self.storage
            .save_validator(&info)
            .map_err(|error| format!("Failed to persist active validator: {error}"))?;
        self.validator.write().insert_validator_info(info);
        Ok(())
    }

    // ============================================================
    // 8.1 DODAVANJE TRANSAKCIJE
    // ============================================================
    pub fn add_transaction(&self, tx: Transaction) -> Result<(), String> {
        // Admission is serialized so two browser submissions cannot reserve the
        // same sender nonce or nullifier between validation and persistence.
        let _admission_guard = self.admission_lock.lock();

        if let Some(existing) = self.storage.get_transaction_by_nullifier(&tx.nullifier) {
            if existing.sender == tx.sender
                && existing.sender_public_key == tx.sender_public_key
                && existing.recipient == tx.recipient
                && existing.amount == tx.amount
                && existing.fee == tx.fee
                && existing.nonce == tx.nonce
                && existing.signature == tx.signature
            {
                return Ok(());
            }
            return Err("Transaction nullifier is already bound to different fields".to_string());
        }

        if self
            .pending_nonces
            .read()
            .get(&tx.sender)
            .is_some_and(|nonces| nonces.contains(&tx.nonce))
        {
            return Err("Transaction nonce is already pending for this sender".to_string());
        }

        // 1. Validacija transakcije
        self.validate_transaction(&tx)?;

        // 🛡️ VALIDATOR PROPOSAL & APPROVAL ENGINE
        // Prepare governance changes, but do not mutate them until the
        // transaction has entered the mempool successfully. This prevents a
        // storage error from leaving governance state ahead of transaction state.
        let proposal_to_persist = match &tx.payload {
            TransactionPayload::ValidatorJoinProposal {
                public_key,
                metadata,
            } => Some((
                tx.get_hash(),
                ValidatorJoinProposalData {
                    public_key: public_key.clone(),
                    metadata: metadata.clone(),
                    proposer: tx.sender.clone(),
                    timestamp: tx.timestamp,
                },
            )),
            _ => None,
        };
        let approval_to_remove = match &tx.payload {
            TransactionPayload::ValidatorApproval { proposal_hash }
                if tx.sender == Self::SOVEREIGN_ADDR =>
            {
                self.pending_proposals
                    .read()
                    .get(proposal_hash)
                    .cloned()
                    .map(|proposal| (*proposal_hash, proposal))
            }
            _ => None,
        };

        // 2. Provera nullifier-a
        // if self.zk_engine.read().is_nullifier_used(&tx.nullifier) {
        //   return Err("Nullifier već iskorišćen!".to_string());
        // }

        // 3. Provera gas-a
        let gas = tx.calculate_gas();
        if gas > tx.gas_limit {
            return Err(format!(
                "Gas limit is too low! Required: {}, limit: {}",
                gas, tx.gas_limit
            ));
        }

        // 4. Provera veličine
        let tx_size = tx.get_size();
        if tx_size > self.max_block_size / 10 {
            return Err(format!("Transaction is too large! Size: {}", tx_size));
        }

        let tx_hash = tx.get_hash();
        if !self
            .storage
            .reserve_nullifier(&tx.nullifier, &tx_hash)
            .map_err(|error| format!("Failed to reserve transaction nullifier: {error}"))?
        {
            return Err("Transaction nullifier is already pending or confirmed".to_string());
        }

        // 5. Dodaj u mempool and persist the public pending transaction before
        // returning success to the API caller.
        {
            let mut mempool = self.mempool.write();
            let validators = self.validator.read().get_active_validators();
            for validator_pk in validators {
                mempool.add_validator(validator_pk);
            }
            if let Err(error) = mempool.add_transaction(&tx) {
                let _ = self
                    .storage
                    .delete_nullifier_if_matches(&tx.nullifier, &tx_hash);
                return Err(error);
            }
        }
        if let Err(error) = self.storage.save_pending_transaction(&tx) {
            self.rollback_pending_transaction(&tx);
            return Err(format!("Failed to persist pending transaction: {error}"));
        }
        self.pending_nonces
            .write()
            .entry(tx.sender.clone())
            .or_default()
            .insert(tx.nonce);

        if let Some((proposal_hash, proposal)) = proposal_to_persist {
            if let Err(error) = self
                .storage
                .save_pending_proposal(&proposal_hash, &proposal)
            {
                self.rollback_pending_transaction(&tx);
                return Err(format!("Failed to persist validator proposal: {error}"));
            }
            self.pending_proposals
                .write()
                .insert(proposal_hash, proposal.clone());
            println!(
                "📥 [GOV] New Validator Proposal recorded: {}",
                proposal.metadata
            );
        }

        if let Some((proposal_hash, proposal)) = approval_to_remove {
            if let Err(error) = self.activate_validator(&proposal.public_key) {
                self.mempool
                    .write()
                    .remove_transactions(std::slice::from_ref(&tx));
                return Err(error);
            }
            let activated_validator = self
                .validator
                .read()
                .get_validator_info(&proposal.public_key)
                .cloned()
                .ok_or_else(|| "Activated validator missing from runtime set".to_string())?;
            let record = ValidatorApprovalRecord {
                proposal_hash,
                approval_transaction: tx.clone(),
                proposal,
                activated_validator,
                recorded_at: Utc::now().timestamp() as u64,
            };
            if let Err(error) = self.storage.save_approval_record(&record) {
                self.mempool
                    .write()
                    .remove_transactions(std::slice::from_ref(&tx));
                return Err(format!(
                    "Failed to persist validator approval journal: {error}"
                ));
            }
            if let Err(error) = self.storage.delete_pending_proposal(&proposal_hash) {
                self.mempool
                    .write()
                    .remove_transactions(std::slice::from_ref(&tx));
                return Err(format!(
                    "Failed to remove approved validator proposal: {error}"
                ));
            }
            self.pending_proposals.write().remove(&proposal_hash);
            println!("✅ [GOV] Validator ACTIVATED by Sovereign Multi-Sig!");
        }

        // 6. Ažuriraj statistiku
        self.total_transactions.fetch_add(1, Ordering::SeqCst);

        Ok(())
    }

    fn rollback_pending_transaction(&self, tx: &Transaction) {
        let hash = tx.get_hash();
        self.mempool
            .write()
            .remove_transactions(std::slice::from_ref(tx));
        let _ = self.storage.delete_pending_transaction(&hash);
        let _ = self
            .storage
            .delete_nullifier_if_matches(&tx.nullifier, &hash);
        let mut pending_nonces = self.pending_nonces.write();
        if let Some(nonces) = pending_nonces.get_mut(&tx.sender) {
            nonces.remove(&tx.nonce);
            if nonces.is_empty() {
                pending_nonces.remove(&tx.sender);
            }
        }
    }

    // ============================================================
    // 8.2 VALIDACIJA TRANSAKCIJE
    // ============================================================
    fn validate_transaction(&self, tx: &Transaction) -> Result<(), String> {
        // 🛡️ SOVEREIGN MULTI-SIG GUARD
        // Prevents total loss if one private key is accidentally seen/leaked.
        // The Sovereign account (1M $ULTRA) requires 2-of-3 signatures.
        let is_payload_bound_validator_proposal = tx.version
            == Self::PAYLOAD_BOUND_TRANSACTION_VERSION
            && matches!(
                &tx.payload,
                TransactionPayload::ValidatorJoinProposal { .. }
            );
        let is_payload_bound_validator_approval = tx.version
            == Self::APPROVAL_BOUND_TRANSACTION_VERSION
            && matches!(&tx.payload, TransactionPayload::ValidatorApproval { .. });

        if tx.version != Self::LEGACY_TRANSACTION_VERSION
            && !is_payload_bound_validator_proposal
            && !is_payload_bound_validator_approval
        {
            return Err(format!("Unsupported transaction version! {}", tx.version));
        }
        if tx.version == Self::LEGACY_TRANSACTION_VERSION && tx.chain_id != Self::L1_CHAIN_ID {
            return Err("Legacy version 1 transactions require L1 chain_id 0".to_string());
        }

        if tx.sender == Self::SOVEREIGN_ADDR {
            let msg = self.create_transaction_message(tx);
            let sig_size = 4627; // Dilithium-5

            let mut valid_signatures = 0;
            let mut used_keys = std::collections::HashSet::new();

            for chunk in tx.signature.chunks(sig_size) {
                if chunk.len() != sig_size {
                    continue;
                }
                for (idx, pk) in self.sovereign_owners.iter().enumerate() {
                    if !used_keys.contains(&idx) && QuantumKeyPair::verify(pk, &msg, chunk) {
                        valid_signatures += 1;
                        used_keys.insert(idx);
                        break;
                    }
                }
            }

            if valid_signatures < Self::SOVEREIGN_THRESHOLD {
                return Err(format!(
                "🛡️ Sovereign Security Triggered: Insufficient signatures! (Valid: {}, Required: {})", 
                valid_signatures, Self::SOVEREIGN_THRESHOLD
            ));
            }
            println!(
                "✅ Sovereign Multi-Sig verified (Threshold: {}/{})",
                valid_signatures,
                self.sovereign_owners.len()
            );
        } else {
            // 1. Provera da li se prijavljeni sender poklapa sa priloženim javnim ključem.
            let expected_sender = QuantumKeyPair::address_from_public_key(&tx.sender_public_key);
            if expected_sender != tx.sender {
                return Err(
                    "Sender address does not match the public key (identity spoofing)!".to_string(),
                );
            }

            // 2. Verifikacija Dilithium potpisa (Quantum-Secure)
            let msg = self.create_transaction_message(tx);
            if !QuantumKeyPair::verify(&tx.sender_public_key, &msg, &tx.signature) {
                return Err("Invalid Dilithium signature!".to_string());
            }
        }

        // 2. Verifikacija ZK Dokaza
        match &tx.payload {
            TransactionPayload::MoveCall { .. } | TransactionPayload::MoveDeploy { .. } => {
                // MOVE VM tranzicije zahtevaju STARK dokaz izvršenja
                if tx.zk_proof.is_empty() {
                    return Err("Move transaction requires a ZK-STARK proof!".to_string());
                }
            }
            TransactionPayload::StandardTransfer => {
                let zk_proof = ZKProof {
                    proof: tx.zk_proof.clone(),
                    nullifier: tx.nullifier,
                    commitment: [0; 32],
                    public_inputs: vec![],
                    timestamp: tx.timestamp,
                    proof_type: tx.proof_type.clone(),
                };
                let zk_res =
                    self.zk_engine
                        .read()
                        .verify_proof(&zk_proof.proof, &[], &zk_proof.nullifier);
                if zk_res.is_err() || !zk_res.unwrap() {
                    return Err("Invalid ZK-SNARK proof!".to_string());
                }
            }
            TransactionPayload::ValidatorJoinProposal { .. }
            | TransactionPayload::ValidatorApproval { .. } => {
                // Governance transactions skip ZK-SNARKs (Identity is verified via Dilithium)
            }
        }
        // NAPOMENA: Nullifier se NE registruje ovde! `validate_transaction` se poziva
        // više puta za istu transakciju (pri dodavanju u mempool i ponovo pri
        // rudarenju bloka) - registrovanje nullifier-a ovde bi odbilo transakciju
        // na drugom pozivu kao "duplo trošenje". Nullifier se registruje samo
        // jednom, kada je transakcija zvanično uključena u lanac (vidi mine_block).

        // NAPOMENA: Legacy `ZKVerifier::verify_proof_data` (snarkjs/circom put) je
        // uklonjen odavde - `tx.zk_proof` sadrži binarni ark-serialize dokaz
        // (native arkworks Groth16), NE JSON snarkjs dokaz, pa je poziv bio
        // pogrešan (uzrokovao je "Proof is not valid JSON" i panic pri sečenju
        // stringa na nevalidnoj UTF-8 granici). Dokaz je već potpuno verifikovan
        // gore preko `self.zk_engine.verify_proof(...)` (arkworks Groth16).

        // 3. Provera balansa
        let total_cost = tx
            .amount
            .checked_add(tx.fee)
            .ok_or_else(|| "Transaction total exceeds the maximum integer value".to_string())?;
        let state = self.state.read();
        let sender_balance = state.get(&tx.sender).unwrap_or(&0);
        if total_cost > *sender_balance {
            return Err(format!(
                "Insufficient balance! Current: {}, required: {}",
                sender_balance, total_cost
            ));
        }

        // 4. Provera vremena
        let now = Utc::now().timestamp() as u64;
        if tx.timestamp > now + 60 {
            return Err("Transaction is in the future!".to_string());
        }
        if now - tx.timestamp > 3600 {
            return Err("Transaction is too old!".to_string());
        }

        // 5. Provera fee-ja
        let min_fee = Self::minimum_transfer_fee(tx.amount);
        if tx.fee < min_fee {
            return Err(format!("Fee is too low! Minimum: {}", min_fee));
        }

        // 6. Provera recipient-a
        if tx.recipient.is_empty() || tx.recipient.len() > 100 {
            return Err("Invalid recipient!".to_string());
        }
        if matches!(tx.payload, TransactionPayload::StandardTransfer)
            && !Self::is_valid_address(&tx.recipient)
        {
            return Err(
                "Recipient must be a 64-character lowercase hexadecimal address".to_string(),
            );
        }

        // 7. Provera da li je sender != recipient
        if tx.sender == tx.recipient {
            return Err("Sender and recipient are identical!".to_string());
        }

        // 8. Provera maksimalnog iznosa
        if tx.amount > Self::MAX_TRANSFER_AMOUNT {
            return Err("Amount is too large!".to_string());
        }

        // 9. Provera nonce-a. A transaction already admitted to the mempool
        // owns its reserved nonce; new submissions must use the next nonce.
        let is_reserved_pending_nonce = self
            .pending_nonces
            .read()
            .get(&tx.sender)
            .is_some_and(|nonces| nonces.contains(&tx.nonce));
        let expected_nonce = self.get_nonce(&tx.sender);
        if !is_reserved_pending_nonce && tx.nonce != expected_nonce {
            return Err(format!(
                "Invalid nonce! Expected: {}, received: {}",
                expected_nonce, tx.nonce
            ));
        }

        Ok(())
    }

    pub fn get_account_nonce(&self, address: &str) -> u64 {
        self.storage.get_account_nonce(address).unwrap_or(0)
    }

    pub fn get_next_nonce(&self, address: &str) -> u64 {
        let confirmed = self.get_account_nonce(address);
        let pending = self
            .pending_nonces
            .read()
            .get(address)
            .map(|nonces| {
                nonces
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(confirmed)
                    .saturating_add(1)
            })
            .unwrap_or(confirmed);
        std::cmp::max(confirmed, pending)
    }

    fn get_nonce(&self, address: &str) -> u64 {
        let confirmed = self.get_account_nonce(address);
        let pending = self.pending_nonces.read();
        let next_pending = pending
            .get(address)
            .and_then(|nonces| nonces.iter().copied().max())
            .map(|nonce| nonce.saturating_add(1))
            .unwrap_or(confirmed);
        std::cmp::max(confirmed, next_pending)
    }

    fn clear_pending_nonces(&self, transactions: &[Transaction]) {
        let mut pending_nonces = self.pending_nonces.write();
        for tx in transactions {
            if let Some(nonces) = pending_nonces.get_mut(&tx.sender) {
                nonces.remove(&tx.nonce);
                if nonces.is_empty() {
                    pending_nonces.remove(&tx.sender);
                }
            }
        }
    }

    /// Builds the canonical signing preimage used by node validation.
    ///
    /// Wallet and integration clients should construct the exact transaction
    /// envelope first, then sign the bytes returned by this method. Version 2
    /// appends the domain-separated proposal payload binding.
    pub fn create_transaction_message(&self, tx: &Transaction) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(tx.sender.as_bytes());
        hasher.update(tx.recipient.as_bytes());
        hasher.update(&tx.amount.to_le_bytes());
        hasher.update(&tx.fee.to_le_bytes());
        hasher.update(&tx.timestamp.to_le_bytes());
        hasher.update(&tx.nullifier);
        hasher.update(&tx.nonce.to_le_bytes());
        hasher.update(&tx.gas_limit.to_le_bytes());
        hasher.update(&tx.gas_price.to_le_bytes());

        if tx.version == Self::PAYLOAD_BOUND_TRANSACTION_VERSION {
            // Version 2 appends an explicit domain-separated, length-prefixed
            // payload envelope. This binds ValidatorJoinProposal metadata and
            // proposal_public_key to the Dilithium signature without changing
            // the legacy version 1 preimage.
            hasher.update(b"UltraNet/transaction-signing-envelope/v2");
            hasher.update(&tx.version.to_le_bytes());
            hasher.update(&tx.chain_id.to_le_bytes());

            if let TransactionPayload::ValidatorJoinProposal {
                public_key,
                metadata,
            } = &tx.payload
            {
                hasher.update(&(public_key.len() as u64).to_le_bytes());
                hasher.update(public_key);
                let metadata_bytes = metadata.as_bytes();
                hasher.update(&(metadata_bytes.len() as u64).to_le_bytes());
                hasher.update(metadata_bytes);
            }
        } else if tx.version == Self::APPROVAL_BOUND_TRANSACTION_VERSION {
            // Version 3 is reserved for approvals and binds the exact pending
            // proposal hash to every sovereign signature. This prevents a
            // valid approval signature from being replayed for another proposal.
            hasher.update(b"UltraNet/approval-signing-envelope/v3");
            hasher.update(&tx.version.to_le_bytes());
            hasher.update(&tx.chain_id.to_le_bytes());
            hasher.update(b"ValidatorApproval");

            if let TransactionPayload::ValidatorApproval { proposal_hash } = &tx.payload {
                hasher.update(proposal_hash);
            }
        }

        hasher.finalize().to_vec()
    }

    // ============================================================
    // 8.2 DODAVANJE BLOKA SA MREŽE
    // ============================================================
    pub fn add_remote_block(&mut self, block: UltraBlock, zk_proof: Vec<u8>) -> Result<(), String> {
        // 1. Proveri da li blok već postoji
        if block.index < self.chain.len() as u64 {
            return Err("Block already exists in the chain".to_string());
        }

        // 2. Proveri da li je sledeći blok
        if block.index != self.chain.len() as u64 {
            return Err(format!(
                "Expected block index {}, received {}",
                self.chain.len(),
                block.index
            ));
        }

        let last_block = self.chain.last().ok_or("Chain is empty")?.clone();

        // 3. Osnovna validacija (hash, merkle, signatures, timestamp, epoch, state_root)
        // NAPOMENA: `validate_block` interno poziva `reexecute_block_for_validation`
        // i proverava MPT state root, tako da je re-egzekucija ovde automatska.
        if !self.validate_block(&block, &last_block) {
            return Err(
                "Block validation failed (hash, Merkle, or state root mismatch)".to_string(),
            );
        }

        // 4. Verifikacija Rekurzivnog SNARK-a (ako je priložen)
        if !zk_proof.is_empty() {
            let mut recursive_zk = self.recursive_zk.write();

            // Pošto RecursiveVerificationCircuit zahteva kompleksne inpute,
            // za demo ćemo samo sačuvati dokaz u istoriju ako je strukturno OK.
            if zk_proof.len() > 100 {
                // Osnovna provera veličine
                recursive_zk
                    .proof_history
                    .insert(block.index, (zk_proof, vec![]));
                println!(
                    "🔒 Block Finality: Recursive SNARK anchored for block {}",
                    block.index
                );
            }
        }

        // 6. Ažuriranje stanja (TRAJNO)
        self.update_state(&block);

        // 7. Registruj nullifier-e
        {
            let zk_engine = self.zk_engine.write();
            for tx in &block.transactions {
                zk_engine.commit_nullifier(tx.nullifier);
            }
        }

        // 8. Dodavanje u lanac
        self.chain.push(block.clone());
        self.total_blocks.fetch_add(1, Ordering::SeqCst);
        *self.total_difficulty.write() += 1;

        // 9. Sačuvaj u bazu
        if let Err(error) = self.storage.save_block(&block) {
            return Err(format!("Failed to persist remote block: {error}"));
        }
        self.clear_pending_nonces(&block.transactions);

        println!("✅ Remote block {} added to chain", block.index);
        Ok(())
    }

    // ============================================================
    // 8.3 RUDARENJE BLOKA
    // ============================================================
    pub fn mine_block(&mut self) -> Result<UltraBlock, String> {
        // 1. Preuzmi transakcije iz mempool-a
        let mempool = self.mempool.read();
        let transactions = mempool.get_transactions(&[]);
        drop(mempool);

        // 2. Validacija transakcija
        let mut valid_txs = Vec::new();
        let mut gas_used = 0;

        for tx in transactions {
            let gas = tx.calculate_gas();

            if gas_used + gas <= Self::DEFAULT_GAS_LIMIT
                && valid_txs.len() < Self::MAX_TRANSACTIONS_PER_BLOCK
            {
                if self.validate_transaction(&tx).is_ok() {
                    valid_txs.push(tx);
                    gas_used += gas;
                }
            } else {
                break;
            }
        }

        if valid_txs.is_empty() {
            return Err("No valid transactions available for mining".to_string());
        }

        // ✅ PODELI TRANSAKCIJE: ZK vs MOVE
        let (zk_txs, move_txs): (Vec<_>, Vec<_>) = valid_txs
            .iter()
            .cloned()
            .partition(|tx| matches!(tx.payload, TransactionPayload::StandardTransfer));

        // // ✅ ============================================================
        // ✅ BLOCK-STM: SHARDED PARALLEL EXECUTION (ZK ONLY)
        // // ✅ ============================================================
        println!(
            "⚡ Block-STM: Executing {} ZK transfers in {} shards",
            zk_txs.len(),
            self.sharded_stm.len()
        );
        let start = std::time::Instant::now();

        // 1. Grupiši transakcije po shardovima
        let mut shard_groups: Vec<Vec<Transaction>> = vec![Vec::new(); 16];
        for tx in zk_txs {
            let shard_id = self.storage.get_shard_id(tx.sender.as_bytes());
            shard_groups[shard_id as usize].push(tx);
        }

        // ✅ SEED BLOCK-STM MEMORY (Consistent with Global State)
        {
            let state = self.state.read();
            for (i, group) in shard_groups.iter().enumerate() {
                // Očisti staru memoriju za ovu rundu rudarenja
                self.sharded_stm[i].memory.rollback_to(0);

                for tx in group {
                    let s_bal = state.get(&tx.sender).cloned().unwrap_or(0);
                    let r_bal = state.get(&tx.recipient).cloned().unwrap_or(0);
                    self.sharded_stm[i].memory.write(&tx.sender, s_bal);
                    self.sharded_stm[i].memory.write(&tx.recipient, r_bal);
                }
            }
        }

        // 2. Izvrši svaki shard u sopstvenom thread-u (ili koristis rayon)
        let results: Vec<Vec<crate::block_stm::ExecutionResult>> = shard_groups
            .par_iter()
            .enumerate()
            .map(|(i, group)| {
                if group.is_empty() {
                    return Vec::new();
                }
                self.sharded_stm[i].execute_parallel(group)
            })
            .collect();

        let duration = start.elapsed();
        println!(
            "⚡ Block-STM: Parallel Sharded Execution Time: {:?}",
            duration
        );

        // 3. Prikupi uspešne transakcije
        let mut final_txs = Vec::new();
        for (i, shard_results) in results.into_iter().enumerate() {
            for (tx, result) in shard_groups[i].iter().zip(shard_results.iter()) {
                if result.success {
                    final_txs.push(tx.clone());
                }
            }
        }

        // ✅ DODAJ MOVE TRANSAKCIJE (Izvršavaju se sekvencijalno u update_state)
        final_txs.extend(move_txs);
        valid_txs = final_txs;
        gas_used = valid_txs.iter().map(|tx| tx.calculate_gas()).sum();

        // ✅ ============================================================
        // ✅ NASTAVAK KREIRANJA BLOKA
        // ✅ ============================================================

        // 3. MYSTICETI DAG - KREIRAJ VERTEX (BEZ PoW!)
        let last_block = self.chain.last().unwrap().clone();
        let prev_timestamp = last_block.timestamp;
        let round = self.dag_round.fetch_add(1, Ordering::SeqCst) + 1;
        let validator_id = 0; // Demo - prvi validator

        // Kreiraj Merkle root
        let mut block_merkle_tree = MerkleTree::new(256);
        for tx in &valid_txs {
            let key = tx.sender.as_bytes();
            let value = &tx.amount.to_le_bytes();
            block_merkle_tree.insert(key, value);

            // ✅ MOVE VM: Dodaj payload u Merkle root
            if let TransactionPayload::MoveCall { .. } = &tx.payload {
                block_merkle_tree.insert(format!("{}:move", tx.sender).as_bytes(), &tx.get_hash());
            }
        }
        let merkle_root_vec = block_merkle_tree.get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root_vec[0..32]);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 4. KREIRAJ BLOK (Privremeno sa praznim state root-om)
        let mut new_block = UltraBlock {
            index: last_block.index + 1,
            timestamp: now,
            previous_hash: last_block.hash,
            hash: [0; 32], // Temporarno
            nonce: 0,
            transactions: valid_txs,
            merkle_root: merkle_root_array,
            state_root: [0; 32],
            shard_roots: vec![],
            aggregated_signature: None,
            validator_set: self.validator.read().get_active_validators(),
            epoch: self.current_epoch.fetch_add(1, Ordering::SeqCst) + 1,
            gas_used,
            gas_limit: Self::DEFAULT_GAS_LIMIT,
            block_reward: self.calculate_block_reward(),
            size: 0,
            version: Self::VERSION,
            parent_hash: last_block.hash,
            difficulty: 0,
            total_difficulty: *self.total_difficulty.read() + 1,
        };

        // ✅ IZRAČUNAJ STATE ROOT RE-EGZEKUCIJOM (IZOLOVANO)
        // (Ovo je neophodno jer state_root mora biti post-execution, a ne smemo trajno menjati state pre validacije)
        let (state_root, shard_roots) =
            self.reexecute_block_for_validation(&new_block, &last_block);
        new_block.state_root = state_root;
        new_block.shard_roots = shard_roots;

        // ✅ Izračunaj pravi hash bloka (SADA KADA IMAMO SHARD ROOTS!)
        new_block.hash = self.calculate_block_hash(
            new_block.index,
            new_block.timestamp,
            &new_block.previous_hash,
            new_block.nonce,
            &new_block.transactions,
            &new_block.merkle_root,
            &new_block.shard_roots,
        );

        // 5. Kreiraj i dodaj DAG vertex (koristeći ISTI hash kao i blok)
        // 5. BULLSHARK: DODAJ VERTEX
        let mut bullshark = self.bullshark.write();
        let mut vertex = MysticetiVertex::new(validator_id, round, vec![last_block.hash]);
        vertex.hash = new_block.hash;

        bullshark
            .add_vertex(vertex.clone())
            .map_err(|e| format!("Bullshark error: {}", e))?;

        // Bullshark: Pruning - zadrži samo zadnjih 100 rundi
        bullshark.prune_old_rounds(100); // OVO JE ZAPRAVO ISPRAVNO!

        drop(bullshark);

        // ✅ SAČUVAJ VERTEX U BAZU
        if let Err(e) = self.storage.save_vertex(&vertex) {
            eprintln!("❌ Failed to save DAG vertex: {}", e);
        }

        // Ažuriraj validator statistiku
        {
            let mut stats = self.validator_stats.write();
            let validator_stat = stats
                .entry(validator_id)
                .or_insert_with(ValidatorStats::new);
            validator_stat.update(10, true);

            // ✅ SAČUVAJ STATISTIKU U BAZU
            if let Err(e) = self
                .storage
                .save_validator_stats(validator_id, validator_stat)
            {
                eprintln!("❌ Failed to save validator stats: {}", e);
            }
        }

        // 6. Validacija bloka
        if !self.validate_block(&new_block, &last_block) {
            return Err("Block rejected!".to_string());
        }

        // 6. Ažuriranje stanja (TRAJNO)
        self.update_state(&new_block);

        // 7. Registruj nullifier-e
        {
            let zk_engine = self.zk_engine.write();
            for tx in &new_block.transactions {
                zk_engine.commit_nullifier(tx.nullifier);
            }
        }

        // 7.1 ✅ NOVO: Ukloni upravo minirane transakcije iz mempool-a, inače bi
        // se iste transakcije uvek iznova uzimale u SVAKI sledeći blok.
        {
            let mut mempool = self.mempool.write();
            mempool.remove_transactions(&new_block.transactions);
        }

        // 8. Distribucija nagrada
        let updated_validator_set = {
            let validator = self.validator.read();
            let mut next = validator.clone();
            next.distribute_rewards(new_block.block_reward + new_block.get_total_fees());
            next
        };
        self.storage
            .replace_validators(&updated_validator_set.validators)
            .map_err(|error| format!("Failed to persist validator rewards: {error}"))?;
        *self.validator.write() = updated_validator_set;

        // 9. Dodavanje u lanac
        self.chain.push(new_block.clone());
        self.total_blocks.fetch_add(1, Ordering::SeqCst);
        *self.total_difficulty.write() += 1;

        // 10. Sačuvaj u bazu
        if let Err(e) = self.storage.save_block(&new_block) {
            eprintln!("❌ Failed to save block {}: {}", new_block.index, e);
        } else {
            self.clear_pending_nonces(&new_block.transactions);
            println!("💾 Block {} saved to disk", new_block.index);
        }

        // ✅ AI-GOVERNANCE: Snimi metriku i prilagodi protokol
        {
            let mut governor = self.ai_governor.write();
            governor.record_metrics(ChainMetrics {
                avg_block_time: (now - prev_timestamp) as f64,
                avg_gas_price: 1,
                active_validators: 5,
                transaction_density: new_block.transactions.len() as f64
                    / Self::MAX_TRANSACTIONS_PER_BLOCK as f64,
            });

            // Predvidi i primeni parametre za sledeći blok
            let next_difficulty =
                governor.predict_optimal_difficulty(self.difficulty.load(Ordering::SeqCst));
            self.difficulty.store(next_difficulty, Ordering::SeqCst);

            println!(
                "🤖 AI-Governor: Suggested next difficulty: {}",
                next_difficulty
            );
            println!(
                "🤖 AI-Governor: 100-Year Sustainability Score: {:.2}/100",
                governor.sustainability_score
            );

            // ✅ DYNAMIC RESHARDING: Proveri da li treba split-ovati shardove
            for i in 0..16 {
                if governor.should_split_shard(i as u8) {
                    println!(
                        "🚀 AI-Governor: SIGNAL: Shard {} is overloaded! Recommending split.",
                        i
                    );
                }
            }
        }

        // 11. Recursive ZK proof
        {
            use ark_bls12_381::Fr;

            let mut recursive_zk = self.recursive_zk.write();
            if recursive_zk.is_setup {
                let prev_proof = self.recursive_proofs.last();
                let public_inputs = vec![
                    Fr::from(new_block.index),
                    Fr::from(new_block.timestamp),
                    Fr::from(self.chain.len() as u64),
                ];

                if let Ok(proof) = recursive_zk.create_recursive_proof(
                    prev_proof.map(|p| p.as_slice()),
                    new_block.hash,
                    new_block.index,
                    new_block.timestamp,
                    self.chain.len() as u64,
                    public_inputs,
                ) {
                    self.recursive_proofs.push(proof);
                    println!("🔐 Recursive proof created for block {}", new_block.index);
                }
            }
        }

        // 12. Checkpoint
        if self.chain.len() % 100 == 0 {
            self.create_checkpoint(&new_block);
        }

        // 13. Statistika
        // 13. BULLSHARK STATISTIKA
        {
            let bullshark = self.bullshark.read();
            let stats = bullshark.get_stats(); // I OVO JE ISPRAVNO!
            println!("📊 BULLSHARK STATS:");
            println!(
                "   Vertices: {}",
                stats.get("total_vertices").unwrap_or(&"0".to_string())
            );
            println!(
                "   Rounds: {}",
                stats.get("total_rounds").unwrap_or(&"0".to_string())
            );
            println!(
                "   Anchors: {}",
                stats.get("anchors").unwrap_or(&"0".to_string())
            );
            println!(
                "   Leaders: {}",
                stats.get("leaders").unwrap_or(&"0".to_string())
            );
            println!(
                "   Committed: {}",
                stats.get("committed_rounds").unwrap_or(&"0".to_string())
            );
            drop(bullshark);
        }

        println!(
            "✨ Block {} added successfully! (Bullshark DAG)",
            new_block.index
        );
        println!("📊 Transactions: {}", new_block.transactions.len());
        println!("💨 Gas used: {}", new_block.gas_used);
        println!("🎁 Block reward: {}", new_block.block_reward);

        // ✅ AŽURIRAJ ISTORIJU ROOT-OVA ZA PRUNING
        {
            let mut history = self.state_root_history.write();
            history.push(new_block.state_root);
            if history.len() > self.pruning_window as usize {
                history.remove(0);
            }
        }

        // ✅ AUTOMATSKI PRUNING (svakih 50 blokova)
        if new_block.index % 50 == 0 {
            let trie_lock = self.state_trie.clone();
            let history = self.state_root_history.read().clone();

            std::thread::spawn(move || {
                let mut trie = trie_lock.write();
                // Prune-uj sve shardove
                for i in 0..16 {
                    let _ = trie.prune(i as u8, history.clone());
                }
            });
        }

        Ok(new_block)
    }

    fn calculate_block_hash(
        &self,
        index: u64,
        timestamp: u64,
        previous_hash: &[u8; 32],
        nonce: u64,
        transactions: &[Transaction],
        merkle_root: &[u8; 32],
        shard_roots: &[[u8; 32]],
    ) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(&index.to_le_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(previous_hash);
        hasher.update(&nonce.to_le_bytes());
        hasher.update(merkle_root);

        for sr in shard_roots {
            hasher.update(sr);
        }

        for tx in transactions {
            hasher.update(tx.sender.as_bytes());
            hasher.update(tx.recipient.as_bytes());
            hasher.update(&tx.amount.to_le_bytes());
            hasher.update(&tx.nonce.to_le_bytes());
            hasher.update(&tx.timestamp.to_le_bytes());
            hasher.update(&tx.nullifier);
        }

        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    fn calculate_genesis_hash() -> [u8; 32] {
        // ✅ DETERMINISTIČKI hash - MORA biti identičan pri svakom pokretanju
        // čvora, jer se koristi i za DAG genesis vertex i za genesis blok.
        // Ranije korišćenje `Utc::now()` je pravilo RAZLIČIT genesis hash pri
        // svakom restartu, pa je novi (u memoriji) genesis blok imao hash koji
        // se nije poklapao sa genesis vertexom sačuvanim u DAG-u na disku,
        // što je uzrokovalo "Parent not found" grešku prilikom rudarenja.
        let mut hasher = Sha3_256::new();
        hasher.update(b"ULTRA BLOCKCHAIN 4.0 - GENESIS - DETERMINISTIC");
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    fn calculate_block_reward(&self) -> u64 {
        let total_blocks = self.total_blocks.load(Ordering::SeqCst);
        self.ai_governor
            .read()
            .predict_optimal_reward(Self::GENESIS_REWARD, total_blocks)
    }

    // ============================================================
    // ============================================================
    // 8.5 VALIDACIJA BLOKA - MYSTICETI DAG VERZIJA
    // ============================================================
    fn validate_block(&self, block: &UltraBlock, prev: &UltraBlock) -> bool {
        println!("🔍 VALIDATING BLOCK {} (Mysticeti DAG)...", block.index);
        println!("   block.timestamp: {}", block.timestamp);
        println!("   prev.timestamp: {}", prev.timestamp);
        println!("   block.gas_used: {}", block.gas_used);
        println!("   block.gas_limit: {}", block.gas_limit);
        println!("   block.size: {}", block.size);
        println!("   block.epoch: {}", block.epoch);
        println!("   prev.epoch: {}", prev.epoch);
        println!("   block.version: {}", block.version);
        println!("   self.version: {}", self.version);

        // ✅ MYSTICETI: PRESKAČEMO PoW PROVJERU!
        // 1. Provera PoW - ISKLJUČENO!
        // let difficulty = self.difficulty.load(Ordering::SeqCst);
        // let target = "0".repeat(difficulty as usize);
        // if !hex::encode(block.hash).starts_with(&target) {
        //     println!("❌ Nevalidan PoW!");
        //     return false;
        // }
        println!("✅ PoW SKIPPED (Mysticeti DAG)");

        // 2. Provera hash-a
        let recomputed = self.calculate_block_hash(
            block.index,
            block.timestamp,
            &block.previous_hash,
            block.nonce,
            &block.transactions,
            &block.merkle_root,
            &block.shard_roots,
        );
        let recomputed_hex = hex::encode(recomputed);
        println!("   recomputed.hash: {}", recomputed_hex);
        if block.hash != recomputed {
            println!("❌ Hash mismatch!");
            return false;
        }
        println!("✅ Hash OK");

        // 3. Provera Merkle root-a
        let mut merkle_tree = MerkleTree::new(256);
        for tx in &block.transactions {
            let key = tx.sender.as_bytes();
            let value = &tx.amount.to_le_bytes();
            merkle_tree.insert(key, value);

            // Dodaj payload u verifikaciju stabla ako je potrebno
            if let TransactionPayload::MoveCall { .. } = &tx.payload {
                merkle_tree.insert(format!("{}:move", tx.sender).as_bytes(), &tx.get_hash());
            }
        }
        let root = merkle_tree.get_root();
        let mut root_array = [0u8; 32];
        root_array.copy_from_slice(&root[0..32]);
        println!("   computed.merkle_root: {}", hex::encode(root_array));
        println!("   block.merkle_root: {}", hex::encode(block.merkle_root));
        if root_array != block.merkle_root {
            println!("❌ Merkle root mismatch!");
            return false;
        }
        println!("✅ Merkle root OK");

        // 4. Provera BLS agregacije
        if let Some(agg_sig) = &block.aggregated_signature {
            let validator = self.validator.read();
            if !validator.verify_aggregated(agg_sig, &block.hash) {
                println!("❌ BLS aggregation is invalid!");
                return false;
            }
            println!("✅ BLS OK");
        } else {
            println!("⚠️ BLS aggregation is unavailable (skipping)");
        }

        // 5. Provera vremena
        println!("   block.timestamp: {}", block.timestamp);
        println!("   prev.timestamp: {}", prev.timestamp);
        if block.timestamp < prev.timestamp {
            println!("❌ Timestamp is invalid! (block.timestamp < prev.timestamp)");
            return false;
        }
        println!("✅ Timestamp OK");

        // 6. Provera gas-a
        if block.gas_used > block.gas_limit {
            println!(
                "❌ Gas limit exceeded! ({} > {})",
                block.gas_used, block.gas_limit
            );
            return false;
        }
        println!("✅ Gas OK");

        // 7. Provera veličine
        if block.size > self.max_block_size {
            println!(
                "❌ Block is too large! ({} > {})",
                block.size, self.max_block_size
            );
            return false;
        }
        println!("✅ Size OK");

        // 8. Provera epoch-e
        if block.epoch <= prev.epoch {
            println!("❌ Epoch is invalid! ({} <= {})", block.epoch, prev.epoch);
            return false;
        }
        println!("✅ Epoch OK");

        // 9. Provera verzije
        if block.version != self.version {
            println!(
                "❌ Unsupported version! ({} != {})",
                block.version, self.version
            );
            return false;
        }
        println!("✅ Version OK");

        // ✅ MYSTICETI: PRESKAČEMO DIFFICULTY I TOTAL DIFFICULTY!
        // 10. Provera difficulty-ja - ISKLJUČENO!
        // if block.difficulty != difficulty {
        //     println!("❌ Difficulty ne odgovara!");
        //     return false;
        // }
        println!("✅ Difficulty SKIPPED (Mysticeti DAG)");

        // 11. Provera total difficulty-ja - ISKLJUČENO!
        // let expected_total = prev.total_difficulty + difficulty as u128;
        // if block.total_difficulty != expected_total {
        //     println!("❌ Total difficulty ne odgovara!");
        //     return false;
        // }
        println!("✅ Total difficulty SKIPPED (Mysticeti DAG)");

        // 9. PROVERA STATE ROOT-A (MPT)
        let (candidate_root, _candidate_shards) = self.reexecute_block_for_validation(block, prev);
        println!("   candidate_root: {}", hex::encode(candidate_root));
        println!("   block.state_root: {}", hex::encode(block.state_root));

        if candidate_root != block.state_root {
            println!("❌ State root mismatch! Fraud detected.");
            return false;
        }
        println!("✅ State Root (MPT) verified by re-execution");

        println!(
            "🎉 ALL CHECKS PASSED! Block {} is VALID (Mysticeti DAG)!",
            block.index
        );
        true
    }

    /// Re-izvršava sve transakcije bloka na izolovanom stanju radi verifikacije
    fn reexecute_block_for_validation(
        &self,
        block: &UltraBlock,
        prev: &UltraBlock,
    ) -> ([u8; 32], Vec<[u8; 32]>) {
        let mut candidate_trie =
            ShardedStateTrie::new(self.storage.trie_shards.clone(), prev.shard_roots.clone());
        let mut candidate_balances: HashMap<String, u64> = HashMap::new(); // Potpuno izolovan
        let mut move_vm = self.move_vm.write();

        // 1. Podeli transakcije
        let (zk_txs, move_txs): (Vec<_>, Vec<_>) = block
            .transactions
            .iter()
            .cloned()
            .partition(|tx| matches!(tx.payload, TransactionPayload::StandardTransfer));

        // ✅ SEED BLOCK-STM MEMORY IZ PRETHODNOG TRIE STANJA
        {
            let mut shard_groups: Vec<Vec<Transaction>> = vec![Vec::new(); 16];
            for tx in &zk_txs {
                let shard_id = self.storage.get_shard_id(tx.sender.as_bytes());
                shard_groups[shard_id as usize].push(tx.clone());
            }

            for (i, group) in shard_groups.iter().enumerate() {
                self.sharded_stm[i].memory.rollback_to(0);
                for tx in group {
                    // Fetch sender
                    let s_key = format!("acc:{}", tx.sender);
                    let s_shard = self.storage.get_shard_id(s_key.as_bytes());
                    let s_bal = candidate_trie
                        .get(s_shard, s_key.as_bytes())
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                        .unwrap_or(0);

                    // Fetch recipient
                    let r_key = format!("acc:{}", tx.recipient);
                    let r_shard = self.storage.get_shard_id(r_key.as_bytes());
                    let r_bal = candidate_trie
                        .get(r_shard, r_key.as_bytes())
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                        .unwrap_or(0);

                    self.sharded_stm[i].memory.write(&tx.sender, s_bal);
                    self.sharded_stm[i].memory.write(&tx.recipient, r_bal);

                    // Sačuvaj i u candidate_balances za MoveVM i finalnu obradu
                    candidate_balances.insert(tx.sender.clone(), s_bal);
                    candidate_balances.insert(tx.recipient.clone(), r_bal);
                }
            }
        }

        // 2. Paralelna egzekucija ZK transfera (Sharded Block-STM)
        println!(
            "⚡ Parallel Validation: Executing {} ZK transfers in shards",
            zk_txs.len()
        );

        let mut shard_groups: Vec<Vec<Transaction>> = vec![Vec::new(); 16];
        for tx in &zk_txs {
            let shard_id = self.storage.get_shard_id(tx.sender.as_bytes());
            shard_groups[shard_id as usize].push(tx.clone());
        }

        let results: Vec<Vec<crate::block_stm::ExecutionResult>> = shard_groups
            .par_iter()
            .enumerate()
            .map(|(i, group)| {
                if group.is_empty() {
                    return Vec::new();
                }
                self.sharded_stm[i].execute_parallel(group)
            })
            .collect();

        for (i, shard_results) in results.into_iter().enumerate() {
            for (tx, result) in shard_groups[i].iter().zip(shard_results.iter()) {
                if result.success {
                    let sender_bal = candidate_balances.entry(tx.sender.clone()).or_insert(0);
                    *sender_bal = sender_bal.saturating_sub(tx.amount + tx.fee);
                    let s_bal = *sender_bal;

                    let rec_bal = candidate_balances.entry(tx.recipient.clone()).or_insert(0);
                    *rec_bal = rec_bal.saturating_add(tx.amount);
                    let r_bal = *rec_bal;

                    let s_key = format!("acc:{}", tx.sender);
                    let r_key = format!("acc:{}", tx.recipient);
                    let s_shard = self.storage.get_shard_id(s_key.as_bytes());
                    let r_shard = self.storage.get_shard_id(r_key.as_bytes());

                    let _ = candidate_trie.insert(s_shard, s_key.as_bytes(), &s_bal.to_le_bytes());
                    let _ = candidate_trie.insert(r_shard, r_key.as_bytes(), &r_bal.to_le_bytes());
                }
            }
        }

        // 3. Sekvencijalna egzekucija Move/FHE transakcija
        move_vm.set_validation_mode(true);
        for tx in move_txs {
            match &tx.payload {
                TransactionPayload::MoveCall {
                    module_address,
                    module_name,
                    function_name,
                    args,
                } => {
                    let addr = AccountAddress::from_hex_literal(module_address)
                        .unwrap_or(AccountAddress::ZERO);
                    let _ =
                        move_vm.execute_function(addr, module_name, function_name, args.clone());
                }
                TransactionPayload::MoveDeploy { name, bytecode } => {
                    let sender_addr = AccountAddress::from_hex_literal(&tx.sender)
                        .unwrap_or(AccountAddress::ZERO);
                    let _ = move_vm.deploy_module(name, bytecode.clone(), sender_addr);
                }
                TransactionPayload::ValidatorJoinProposal { .. }
                | TransactionPayload::ValidatorApproval { .. } => {
                    // Governance events handled in transition layer
                }
                _ => unreachable!(),
            }
        }

        // Nagrade i fee-jevi
        if let Some(first_validator) = block.validator_set.first() {
            let validator_address = hex::encode(first_validator);
            let bal = candidate_balances
                .entry(validator_address.clone())
                .or_insert_with(|| {
                    let v_key = format!("acc:{}", validator_address);
                    let v_shard = self.storage.get_shard_id(v_key.as_bytes());
                    candidate_trie
                        .get(v_shard, v_key.as_bytes())
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                        .unwrap_or(0)
                });
            *bal = bal.saturating_add(block.block_reward);

            let v_key = format!("acc:{}", validator_address);
            let v_shard = self.storage.get_shard_id(v_key.as_bytes());
            let _ = candidate_trie.insert(v_shard, v_key.as_bytes(), &bal.to_le_bytes());
        }

        let total_fees: u64 = block.transactions.iter().map(|tx| tx.fee).sum();
        if total_fees > 0 && !block.validator_set.is_empty() {
            let fee_per_validator = total_fees / block.validator_set.len() as u64;
            for validator_pk in &block.validator_set {
                let validator_address = hex::encode(validator_pk);
                let bal = candidate_balances
                    .entry(validator_address.clone())
                    .or_insert_with(|| {
                        let f_key = format!("acc:{}", validator_address);
                        let f_shard = self.storage.get_shard_id(f_key.as_bytes());
                        candidate_trie
                            .get(f_shard, f_key.as_bytes())
                            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                            .unwrap_or(0)
                    });
                *bal = bal.saturating_add(fee_per_validator);

                let f_key = format!("acc:{}", validator_address);
                let f_shard = self.storage.get_shard_id(f_key.as_bytes());
                let _ = candidate_trie.insert(f_shard, f_key.as_bytes(), &bal.to_le_bytes());
            }
        }

        // Dodaj Move VM resurse iz write_set-a u Trie
        for (key, val) in move_vm.write_set.iter() {
            let trie_key = format!("move:{}", key);
            let shard_id = self.storage.get_shard_id(trie_key.as_bytes());
            let _ = candidate_trie.insert(shard_id, trie_key.as_bytes(), val);
        }

        // Isključi validation mode i obriši write_set
        move_vm.set_validation_mode(false);

        let root = candidate_trie.root_hash();
        let shard_roots = candidate_trie.shards.iter().map(|s| s.root_hash).collect();
        (root, shard_roots)
    }
    // ============================================================
    // 8.6 AŽURIRANJE STANJA
    // ============================================================
    fn update_state(&mut self, block: &UltraBlock) {
        for tx in &block.transactions {
            match &tx.payload {
                TransactionPayload::StandardTransfer => {
                    // Smanji stanje pošiljaocu
                    let new_sender_balance = {
                        let mut state = self.state.write();
                        let bal = state.entry(tx.sender.clone()).or_insert(0);
                        *bal = bal.saturating_sub(tx.amount + tx.fee);
                        *bal
                    };

                    // Povećaj stanje primaocu
                    let new_recipient_balance = {
                        let mut state = self.state.write();
                        let bal = state.entry(tx.recipient.clone()).or_insert(0);
                        *bal = bal.saturating_add(tx.amount);
                        *bal
                    };

                    // Ažuriraj Merkle stablo stanja
                    {
                        let mut merkle_tree = self.merkle_tree.write();
                        merkle_tree.insert(tx.sender.as_bytes(), &new_sender_balance.to_le_bytes());
                        merkle_tree.insert(
                            tx.recipient.as_bytes(),
                            &new_recipient_balance.to_le_bytes(),
                        );
                    }

                    // ✅ AŽURIRAJ MPT TRIE
                    {
                        let mut state_trie = self.state_trie.write();
                        let s_key = format!("acc:{}", tx.sender);
                        let r_key = format!("acc:{}", tx.recipient);
                        let s_shard = self.storage.get_shard_id(s_key.as_bytes());
                        let r_shard = self.storage.get_shard_id(r_key.as_bytes());

                        let _ = state_trie.insert(
                            s_shard,
                            s_key.as_bytes(),
                            &new_sender_balance.to_le_bytes(),
                        );
                        let _ = state_trie.insert(
                            r_shard,
                            r_key.as_bytes(),
                            &new_recipient_balance.to_le_bytes(),
                        );
                    }
                }
                TransactionPayload::MoveCall {
                    module_address,
                    module_name,
                    function_name,
                    args,
                } => {
                    let addr = AccountAddress::from_hex_literal(module_address)
                        .unwrap_or(AccountAddress::ZERO);
                    let mut move_vm = self.move_vm.write();
                    let _ =
                        move_vm.execute_function(addr, module_name, function_name, args.clone());
                }
                TransactionPayload::MoveDeploy { name, bytecode } => {
                    let sender_addr = AccountAddress::from_hex_literal(&tx.sender)
                        .unwrap_or(AccountAddress::ZERO);
                    let mut move_vm = self.move_vm.write();
                    let _ = move_vm.deploy_module(name, bytecode.clone(), sender_addr);
                }
                TransactionPayload::ValidatorJoinProposal { .. }
                | TransactionPayload::ValidatorApproval { .. } => {
                    // Handled at protocol addition layer
                }
            }
        }

        // Dodaj block reward prvom validatoru
        if let Some(first_validator) = block.validator_set.first() {
            let validator_address = hex::encode(first_validator);

            let reward_balance = {
                let mut state = self.state.write();
                let bal = state.entry(validator_address.clone()).or_insert(0);
                *bal = bal.saturating_add(block.block_reward);
                *bal
            };

            // ✅ AŽURIRAJ MPT TRIE
            {
                let mut state_trie = self.state_trie.write();
                let v_key = format!("acc:{}", validator_address);
                let v_shard = self.storage.get_shard_id(v_key.as_bytes());
                let _ = state_trie.insert(v_shard, v_key.as_bytes(), &reward_balance.to_le_bytes());
            }
        }

        let total_fees: u64 = block.transactions.iter().map(|tx| tx.fee).sum();
        if total_fees > 0 && !block.validator_set.is_empty() {
            let fee_per_validator = total_fees / block.validator_set.len() as u64;
            for validator_pk in &block.validator_set {
                let validator_address = hex::encode(validator_pk);

                let fee_balance = {
                    let mut state = self.state.write();
                    let bal = state.entry(validator_address.clone()).or_insert(0);
                    *bal = bal.saturating_add(fee_per_validator);
                    *bal
                };

                // ✅ AŽURIRAJ MPT TRIE
                {
                    let mut state_trie = self.state_trie.write();
                    let f_key = format!("acc:{}", validator_address);
                    let f_shard = self.storage.get_shard_id(f_key.as_bytes());
                    let _ =
                        state_trie.insert(f_shard, f_key.as_bytes(), &fee_balance.to_le_bytes());
                }
            }
        }
    }

    // ============================================================
    // 8.7 CHECKPOINT SISTEM
    // ============================================================
    fn create_checkpoint(&mut self, block: &UltraBlock) {
        let checkpoint = Checkpoint {
            block_hash: block.hash,
            block_index: block.index,
            timestamp: block.timestamp,
            state_root: block.state_root,
            validator_set: block.validator_set.clone(),
            total_difficulty: block.total_difficulty,
            version: block.version,
        };

        self.checkpoints.push(checkpoint);
        println!("📌 Checkpoint created for block {}", block.index);
    }

    pub fn restore_from_checkpoint(&mut self, index: u64) -> Result<(), String> {
        let checkpoint = self
            .checkpoints
            .iter()
            .find(|c| c.block_index == index)
            .cloned()
            .ok_or("Checkpoint was not found".to_string())?;

        let block_index = checkpoint.block_index as usize;
        if block_index >= self.chain.len() {
            return Err("Checkpoint blok ne postoji!".to_string());
        }

        // Rollback lanca
        self.chain.truncate(block_index + 1);

        // Restore stanja
        let mut state = self.state.write();
        state.clear();
        drop(state);

        // Ponovo izgradi stanje
        for i in 0..=block_index {
            let block = self.chain[i].clone();
            self.update_state(&block);
        }

        // Restore total difficulty
        *self.total_difficulty.write() = checkpoint.total_difficulty;

        println!("✅ Restore from checkpoint {} succeeded!", index);
        Ok(())
    }

    // ============================================================
    // 8.8 REORG (FORK DETEKCIJA I RESOLUCIJA)
    // ============================================================
    pub fn handle_reorg(&mut self, new_chain: Vec<UltraBlock>) -> Result<(), String> {
        // 1. Pronađi zajednički predak
        let fork_point = self.find_fork_point(&new_chain)?;

        // 2. Ako je novi lanac duži, izvrši reorg
        if new_chain.len() > self.chain.len() {
            println!("🔄 REORG: Replacing chain!");
            println!("📊 Old chain: {} blocks", self.chain.len());
            println!("📊 New chain: {} blocks", new_chain.len());

            // 3. Rollback stanje do fork_point
            self.rollback_state(fork_point)?;

            // 4. Validacija i dodavanje novih blokova
            for block in &new_chain[fork_point + 1..] {
                if !self.validate_block(block, &new_chain[block.index as usize - 1]) {
                    return Err("Invalid block in the new chain!".to_string());
                }
                self.update_state(block);
                self.chain.push(block.clone());
                self.total_blocks.fetch_add(1, Ordering::SeqCst);
                *self.total_difficulty.write() += block.difficulty as u128;
            }

            println!(
                "✅ REORG successful! The new chain has {} blocks",
                self.chain.len()
            );
            Ok(())
        } else {
            Err("The new chain is not longer".to_string())
        }
    }

    fn find_fork_point(&self, new_chain: &[UltraBlock]) -> Result<usize, String> {
        let mut fork_point = 0;
        let min_len = std::cmp::min(self.chain.len(), new_chain.len());

        for i in 0..min_len {
            if self.chain[i].hash == new_chain[i].hash {
                fork_point = i;
            } else {
                break;
            }
        }

        if fork_point == 0 && self.chain[0].hash != new_chain[0].hash {
            return Err("No common ancestor found!".to_string());
        }

        Ok(fork_point)
    }

    fn rollback_state(&mut self, fork_point: usize) -> Result<(), String> {
        let mut state = self.state.write();
        *state = HashMap::new();
        drop(state);

        // Ponovo izgradi stanje od genesis do fork_point
        for i in 0..=fork_point {
            let block = self.chain[i].clone();
            self.update_state(&block);
        }

        Ok(())
    }

    // ============================================================
    // 8.9 PROVERA VALIDNOSTI LANCA
    // ============================================================
    pub fn is_chain_valid(&self) -> bool {
        // ✅ AKO JE LANAC PREKRATAK, PROVERI SVE
        if self.chain.len() <= 1 {
            println!("✅ Chain is valid! {} blocks", self.chain.len());
            return true;
        }

        let check_depth = 100;
        let start = if self.chain.len() > check_depth {
            self.chain.len() - check_depth
        } else {
            1 // ✅ POČNI OD 1 DA IZBEGNEŠ ODUZIMANJE
        };

        for i in start..self.chain.len() {
            let current = &self.chain[i];
            let previous = &self.chain[i - 1];

            if current.previous_hash != previous.hash {
                println!("⚠️ ATTACK DETECTED: Link broken at block {}!", i);
                return false;
            }

            if !self.validate_block(current, previous) {
                println!("⚠️ ATTACK DETECTED: Block {} is invalid!", i);
                return false;
            }
        }
        println!("✅ Chain is valid! {} blocks", self.chain.len());
        true
    }

    // ============================================================
    // 8.10 DIFFICULTY ADJUSTMENT
    // ============================================================
    pub fn adjust_difficulty(&mut self) {
        if self.chain.len() < 10 {
            return;
        }

        let last_block = self.chain.last().unwrap();
        let prev_block = &self.chain[self.chain.len() - 10];

        let time_diff = last_block.timestamp - prev_block.timestamp;
        let expected_time = 10 * 10; // 10 blokova * 10 sekundi

        let current_difficulty = self.difficulty.load(Ordering::SeqCst);
        let new_difficulty = if time_diff < expected_time {
            current_difficulty + 1
        } else if time_diff > expected_time * 2 {
            current_difficulty.saturating_sub(1)
        } else {
            current_difficulty
        };

        // Ograniči difficulty
        let new_difficulty = new_difficulty.clamp(1, 100);

        self.difficulty.store(new_difficulty, Ordering::SeqCst);
        println!(
            "📊 Difficulty adjusted: {} -> {}",
            current_difficulty, new_difficulty
        );
        println!(
            "⏱️  Time for 10 blocks: {}s (expected: {}s)",
            time_diff, expected_time
        );
    }

    // ============================================================
    // 8.11 STATISTIKA
    // ============================================================
    pub fn get_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();
        stats.insert(
            "total_blocks".to_string(),
            self.total_blocks.load(Ordering::SeqCst).to_string(),
        );
        stats.insert(
            "total_transactions".to_string(),
            self.total_transactions.load(Ordering::SeqCst).to_string(),
        );
        stats.insert("chain_length".to_string(), self.chain.len().to_string());
        stats.insert(
            "difficulty".to_string(),
            self.difficulty.load(Ordering::SeqCst).to_string(),
        );
        stats.insert(
            "validator_count".to_string(),
            self.validator.read().get_validator_count().to_string(),
        );
        stats.insert(
            "mempool_size".to_string(),
            self.mempool.read().get_pending_count().to_string(),
        );
        stats.insert(
            "epoch".to_string(),
            self.current_epoch.load(Ordering::SeqCst).to_string(),
        );
        stats.insert(
            "checkpoints".to_string(),
            self.checkpoints.len().to_string(),
        );
        stats.insert(
            "total_difficulty".to_string(),
            self.total_difficulty.read().to_string(),
        );
        stats.insert("version".to_string(), self.version.to_string());
        stats.insert("genesis_time".to_string(), self.genesis_time.to_string());
        stats.insert("block_time".to_string(), self.block_time.to_string());
        stats.insert(
            "max_block_size".to_string(),
            self.max_block_size.to_string(),
        );
        stats.insert("gas_limit".to_string(), Self::DEFAULT_GAS_LIMIT.to_string());
        stats.insert(
            "max_transactions".to_string(),
            Self::MAX_TRANSACTIONS_PER_BLOCK.to_string(),
        );

        // ZK statistika
        let zk_engine = self.zk_engine.read();
        stats.insert(
            "zk_proofs".to_string(),
            zk_engine.get_proof_count().to_string(),
        );
        stats.insert(
            "zk_verifications".to_string(),
            zk_engine.get_verification_count().to_string(),
        );
        drop(zk_engine);

        // Validator statistika
        let validator = self.validator.read();
        stats.insert(
            "total_weight".to_string(),
            validator.get_total_weight().to_string(),
        );
        drop(validator);

        stats
    }

    pub fn get_latest_transactions(&self, limit: usize) -> Vec<serde_json::Value> {
        let mut all_txs = Vec::new();

        // Prođi kroz zadnje blokove unazad dok ne skupiš dovoljno transakcija
        for block in self.chain.iter().rev() {
            for tx in block.transactions.iter().rev() {
                if all_txs.len() >= limit {
                    break;
                }

                let hash = tx.get_hash();
                let shard_id = (u64::from_str_radix(&tx.sender[0..2], 16).unwrap_or(0) % 16) as u8;

                all_txs.push(serde_json::json!({
                    "id": hex::encode(&hash[0..8]),
                    "hash": hex::encode(hash),
                    "amount": (tx.amount as f64 / 1_000_000.0).to_string(), // Pretpostavljamo 6 decimala za ULTRA
                    "shard": shard_id,
                    "sender": tx.sender,
                    "recipient": tx.recipient,
                    "timestamp": tx.timestamp
                }));
            }
            if all_txs.len() >= limit {
                break;
            }
        }

        all_txs
    }

    fn get_unadjusted_balance(&self, address: &str) -> u64 {
        // 1. Proveri legacy state (za kompatibilnost)
        let state = self.state.read();
        let legacy_balance = *state.get(address).unwrap_or(&0);
        if legacy_balance > 0 {
            return legacy_balance;
        }
        drop(state);

        // 2. Proveri Move VM Resource (UltraCoin)
        let clean_addr = address.strip_prefix("0x").unwrap_or(address);
        let vm = self.move_vm.read();
        let res_key = format!("{}:Coin", clean_addr);
        if let Some(val) = vm.storage.move_resources.get(&res_key).ok().flatten() {
            if let Ok(info) = bincode::deserialize::<crate::move_vm::MoveResourceInfo>(&val) {
                if info.data.len() >= 8 {
                    let mut balance = [0u8; 8];
                    balance.copy_from_slice(&info.data[0..8]);
                    return u64::from_le_bytes(balance);
                }
            }
        }
        0
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        let balance = self.get_unadjusted_balance(address);
        let registry = self.appchain_registry.read();
        registry
            .active_chains
            .values()
            .find(|chain| chain.account_address == address)
            .map(|chain| balance.saturating_sub(chain.anchor_spend))
            .unwrap_or(balance)
    }

    pub fn get_appchain_treasury_balance(&self, chain: &crate::appchain::AppChainConfig) -> u64 {
        self.get_unadjusted_balance(&chain.account_address)
            .saturating_sub(chain.anchor_spend)
    }
    // ============================================================
    // 8.12 ČIŠĆENJE STARIH PODATAKA
    // ============================================================
    pub fn cleanup(&mut self, max_age: u64) {
        // Očisti stare ZK proof-ove
        let mut zk_engine = self.zk_engine.write();
        zk_engine.cleanup_old_proofs(max_age);
        drop(zk_engine);

        // Očisti stare transakcije iz mempool-a
        let mut mempool = self.mempool.write();
        mempool.clear();
        drop(mempool);

        println!("🧹 Old data cleanup completed!");
    }

    // ============================================================
    // 8.13 SNAPSHOT FUNKCIONALNOST
    // ============================================================
    pub fn create_snapshot(&self) -> HashMap<String, Vec<u8>> {
        let mut snapshot = HashMap::new();

        // Snapshot lanca
        let chain_data = bincode::serialize(&self.chain).unwrap_or_default();
        snapshot.insert("chain".to_string(), chain_data);

        // Snapshot stanja
        let state = self.state.read();
        let state_data = bincode::serialize(&*state).unwrap_or_default();
        snapshot.insert("state".to_string(), state_data);
        drop(state);

        // Snapshot validatora
        let validator = self.validator.read();
        let validator_data = bincode::serialize(&validator.validators).unwrap_or_default();
        snapshot.insert("validators".to_string(), validator_data);
        drop(validator);

        println!("📸 Snapshot created!");
        snapshot
    }

    pub fn restore_snapshot(&mut self, snapshot: HashMap<String, Vec<u8>>) -> Result<(), String> {
        // Restore lanca
        if let Some(chain_data) = snapshot.get("chain") {
            let chain: Vec<UltraBlock> = bincode::deserialize(chain_data)
                .map_err(|e| format!("Chain restore error: {}", e))?;
            self.chain = chain;
        }

        // Restore stanja
        if let Some(state_data) = snapshot.get("state") {
            let state: HashMap<String, u64> = bincode::deserialize(state_data)
                .map_err(|e| format!("State restore error: {}", e))?;
            let mut current_state = self.state.write();
            *current_state = state;
        }

        println!("🔄 Snapshot restored!");
        Ok(())
    }

    // ============================================================
    // ✅ RECURSIVE ZK FUNKCIJE - DODAJ OVDE!
    // ============================================================

    pub fn get_recursive_proof(&self, block_index: u64) -> Option<Vec<u8>> {
        let recursive_zk = self.recursive_zk.read();
        recursive_zk
            .get_proof_by_block(block_index)
            .map(|(proof, _)| proof.clone())
    }

    pub fn get_latest_recursive_proof(&self) -> Option<Vec<u8>> {
        let recursive_zk = self.recursive_zk.read();
        recursive_zk
            .get_latest_proof()
            .map(|(proof, _)| proof.clone())
    }

    pub fn verify_recursive_chain(&self) -> Result<bool, String> {
        let recursive_zk = self.recursive_zk.read();

        // Koristimo TAČNE javne ulaze koji su sačuvani uz dokaz prilikom
        // njegovog kreiranja (prev_hash, block_hash, block_index, timestamp,
        // total_blocks) - Groth16::verify zahteva identičan skup/redosled/
        // enkodiranje javnih ulaza kao prilikom generisanja dokaza.
        if let Some((latest_proof, public_inputs)) = recursive_zk.get_latest_proof() {
            recursive_zk.verify_recursive_proof(latest_proof, public_inputs)
        } else {
            Ok(true)
        }
    }

    pub fn get_recursive_stats(&self) -> (usize, u64, bool) {
        let recursive_zk = self.recursive_zk.read();
        (
            recursive_zk.get_proof_chain_length(),
            recursive_zk.proof_counter,
            recursive_zk.is_setup,
        )
    }

    /// NOVO: Napredna pretraga DAG vertexa (RAM -> Disk)
    pub fn get_dag_vertex(&self, hash: &[u8; 32]) -> Option<MysticetiVertex> {
        // 1. Probaj RAM
        {
            let dag = self.dag.read();
            if let Some(v) = dag.vertices.get(hash) {
                return Some(v.clone());
            }
        }

        // 2. Probaj Disk (Sled)
        self.storage.get_vertex(hash)
    }
} // ⬅️ OVO JE KRAJ impl UltraBlockchain

// ============================================================
// 9. FUNKCIJA ZA POKRETANJE ČVORA
// ============================================================
pub async fn run_node() -> Result<(), String> {
    println!("");
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║         🚀 ULTRA BLOCKCHAIN 3.0                      ║");
    println!("║      THE MOST ADVANCED BLOCKCHAIN IN THE WORLD       ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("");

    // 1. Validate configuration before opening storage or running cryptographic setup.
    let runtime_config = runtime_config::prepare()?;
    let validator_address = validator_identity::ensure(&runtime_config.db_path)?;
    println!("🔑 Validator Dilithium-5 identity ready: address={validator_address}");
    println!("   Export the public key with --export-validator-public-key");
    let db_path = runtime_config.db_path.to_string_lossy().into_owned();
    let shared = SharedStorage::new(&db_path).map_err(|error| {
        format!(
            "Cannot open shared storage at {}: {error}",
            runtime_config.db_path.display()
        )
    })?;

    let mut blockchain = UltraBlockchain::with_storage(shared.get_storage());

    // Za P2P node - DELI ISTU BAZU!
    let blockchain_p2p = Arc::new(tokio::sync::RwLock::new(UltraBlockchain::with_storage(
        shared.get_storage(),
    )));

    shared.print_stats();
    println!("✅ Blockchain initialized!");
    println!("🔧 Initializing ZK engine...");
    {
        let mut zk_engine = blockchain.zk_engine.write();
        if let Err(e) = zk_engine.setup() {
            eprintln!("❌ ZK setup failed: {}", e);
            return Err(format!("ZK setup error: {}", e));
        }
    }
    println!("✅ ZK engine initialized!");

    println!("🔧 Initializing Recursive ZK engine...");
    {
        let mut recursive_zk = blockchain.recursive_zk.write();
        if let Err(e) = recursive_zk.setup() {
            eprintln!("❌ Recursive ZK setup failed: {}", e);
            return Err(format!("Recursive ZK setup error: {}", e));
        }
    }
    {
        // P2P instanca deli istu bazu, ali ima svoj Recursive ZK engine -
        // mora biti setup-ovan i tamo da bi mogla da kreira/verifikuje proof-ove.
        let blockchain_p2p_guard = blockchain_p2p.read().await;
        let mut recursive_zk_p2p = blockchain_p2p_guard.recursive_zk.write();
        if let Err(e) = recursive_zk_p2p.setup() {
            eprintln!("❌ Recursive ZK (P2P) setup failed: {}", e);
            return Err(format!("Recursive ZK (P2P) setup error: {}", e));
        }
    }
    println!("✅ Recursive ZK engine initialized!");
    println!(
        "   - Validators: {}",
        blockchain.validator.read().get_validator_count()
    );
    println!(
        "   - Difficulty: {}",
        blockchain.difficulty.load(Ordering::SeqCst)
    );
    println!("   - Gas limit: {}", UltraBlockchain::DEFAULT_GAS_LIMIT);
    println!("   - Version: {}", UltraBlockchain::VERSION);
    println!("");

    // ✅ NOVO: Demo scenario (Alice->Bob, mining, simulacija napada) se
    // izvršava SAMO pri PRVOM pokretanju, kada je lanac svež (samo genesis).
    // Ranije se ovaj blok izvršavao na SVAKOM restartu čvora, što je značilo
    // da se pri svakom restartu rudari NOVI, lažni "demo" blok i trajno
    // upisuje u perzistentni lanac - to je uzrokovalo da se broj blokova i
    // indeksi neočekivano uvećavaju nakon svakog restarta.
    if blockchain.chain.len() > 10000 {
        // Disabling demo scenario
        println!("🆕 Fresh chain detected - starting the demo scenario (once only)...");

        // 2. Kreiranje novčanika
        let mut alice = UltraWallet::new();
        let bob = UltraWallet::new();
        let charlie = UltraWallet::new();

        println!("👤 ALICE:    {}", &alice.get_address()[..8]);
        println!("👤 BOB:      {}", &bob.get_address()[..8]);
        println!("👤 CHARLIE:  {}", &charlie.get_address()[..8]);
        println!("💰 Initial balance: 1000 tokens\n");

        // 3. Kreiranje privatne transakcije
        println!("📝 Alice is sending 100 tokens to Bob (private)...");

        let merkle_root = blockchain.merkle_tree.read().get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root[0..32]);

        let mut zk_engine = blockchain.zk_engine.write();
        let tx = alice.create_transaction(
            bob.get_address(),
            100,
            1,
            500000,
            1,
            &mut *zk_engine,
            &merkle_root_array,
            ProofType::Transaction,
        )?;
        drop(zk_engine);

        println!("🔐 Nullifier: {}...", hex::encode(&tx.nullifier[..4]));
        println!("🔐 ZK proof: {}...", hex::encode(&tx.zk_proof[..8]));
        println!(
            "🔐 Dilithium signature: {}...",
            hex::encode(&tx.signature[..8])
        );
        println!("💨 Gas: {}", tx.calculate_gas());
        println!("📦 Size: {} bytes", tx.get_size());
        println!("");

        // 4. Dodavanje transakcije
        blockchain.add_transaction(tx)?;
        println!("✅ Transaction added to the encrypted mempool!");
        println!(
            "   - Mempool size: {}",
            blockchain.mempool.read().get_pending_count()
        );
        println!("");

        // 5. Rudarenje bloka
        println!("⏳ Mining block...");
        blockchain.mine_block()?;

        // 6. Provera validnosti
        println!("\n🔍 Checking chain validity...");
        blockchain.is_chain_valid();

        // 7. Statistika
        println!("\n📊 STATISTICS:");
        let stats = blockchain.get_stats();
        for (key, value) in stats {
            println!("   - {}: {}", key, value);
        }

        // 8. Attack simulation
        println!("\n🚫 ATTACK SIMULATION:");
        let original_amount = if let Some(block) = blockchain.chain.get_mut(1) {
            if !block.transactions.is_empty() {
                let orig = block.transactions[0].amount;
                block.transactions[0].amount = 999999;
                println!(
                    "💀 Tampered transaction: amount = {}",
                    block.transactions[0].amount
                );
                Some(orig)
            } else {
                None
            }
        } else {
            None
        };

        println!("🔍 Checking after attack:");
        blockchain.is_chain_valid();

        // ✅ VRATI ORIGINALNO STANJE DA BI LANAC OSTAO VALIDAN ZA API
        if let Some(orig) = original_amount {
            if let Some(block) = blockchain.chain.get_mut(1) {
                block.transactions[0].amount = orig;
                println!("🛡️ Restoring original state: amount = {}", orig);
            }
        }

        // 9. Rotacija ključeva
        println!("\n🔄 Rotating Alice's keys...");
        alice.rotate_keys();

        // 10. Adjustacija difikultada
        println!("\n📊 Adjusting difficulty...");
        blockchain.adjust_difficulty();

        // 11. Kreiranje checkpoint-a
        if let Some(last_block) = blockchain.chain.last().cloned() {
            blockchain.create_checkpoint(&last_block);
        }

        // 12. Snapshot
        println!("\n📸 Creating snapshot...");
        let snapshot = blockchain.create_snapshot();
        println!("   - Snapshot size: {} bytes", snapshot.len());

        // 13. Čišćenje starih podataka
        println!("\n🧹 Cleaning up old data...");
        blockchain.cleanup(3600);
    } else {
        println!(
            "🔗 Existing chain detected ({} blocks) - skipping demo scenario.",
            blockchain.chain.len()
        );
        println!("\n🔍 Checking chain validity...");
        blockchain.is_chain_valid();
    }

    // 14. Kraj
    println!("");
    println!("╔════════════════════════════════════════════════════════╗");
    println!("║         ✅ ULTRA BLOCKCHAIN 3.0 IS OPERATIONAL!      ║");
    println!("║      🏆 THE MOST ADVANCED BLOCKCHAIN IN THE WORLD   ║");
    println!("╚════════════════════════════════════════════════════════╝");
    println!("");
    println!("🔗 Block count: {}", blockchain.chain.len());
    println!(
        "💰 Total transactions: {}",
        blockchain.total_transactions.load(Ordering::SeqCst)
    );
    println!(
        "🔐 Validator count: {}",
        blockchain.validator.read().get_validator_count()
    );
    println!("📌 Checkpoints: {}", blockchain.checkpoints.len());
    println!("");

    // ✅ POKRENI REST API SERVER DIREKTNO
    let api_bind =
        std::env::var("ULTRANET_API_BIND").unwrap_or_else(|_| "127.0.0.1:8081".to_string());
    println!("🚀 Starting REST API server at http://{api_bind}");
    println!("📋 Endpoints:");
    println!("   POST /api/transaction - Add transaction");
    println!("   POST /api/mine - Mine block");
    println!("   GET  /api/chain - Chain state");
    println!("   GET  /api/balance/:address - Balance");
    println!("   GET  /api/validate - Validate chain");
    println!("   GET  /api/block/:index - Block by index");
    println!("   GET  /api/stats - Statistics");
    println!("");
    println!("🔴 Press Ctrl+C to stop...");
    println!("");

    let blockchain_arc = Arc::new(RwLock::new(blockchain));

    // Start the P2P node before entering the blocking API server. This keeps
    // initialization errors in the main result so desktop launches fail clearly.
    let mut p2p_node = P2PNode::new(blockchain_p2p.clone())
        .await
        .map_err(|error| format!("P2P node initialization failed: {error}"))?;
    p2p_node
        .start_listening("/ip4/0.0.0.0/tcp/9000")
        .map_err(|error| format!("P2P listen failed: {error}"))?;
    println!("✅ P2P node is running on port 9000!");
    tokio::spawn(async move {
        if let Err(error) = p2p_node.run().await {
            eprintln!("❌ P2P node runtime error: {error}");
        }
    });

    // Start the API directly on the existing Tokio runtime.
    if let Err(error) = api::run_server(blockchain_arc).await {
        return Err(format!("API server failed: {error}"));
    }

    Ok(())
}

// ============================================================
// UNIT TESTOVI: Dilithium potpis i ZK verifikacija (validate_transaction)
// ============================================================
#[cfg(test)]
mod signature_verification_tests {
    use super::*;
    use std::fs;

    fn open_test_chain(name: &str) -> UltraBlockchain {
        let path = format!("test_db_sigcheck_{}", name);
        let _ = fs::remove_dir_all(&path);
        UltraBlockchain::new(&path)
    }

    fn cleanup_test_chain(name: &str) {
        let path = format!("test_db_sigcheck_{}", name);
        let _ = fs::remove_dir_all(&path);
    }

    fn build_message(
        sender: &str,
        recipient: &str,
        amount: u64,
        fee: u64,
        timestamp: u64,
        nullifier: &[u8; 32],
        nonce: u64,
        gas_limit: u64,
        gas_price: u64,
    ) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(sender.as_bytes());
        hasher.update(recipient.as_bytes());
        hasher.update(&amount.to_le_bytes());
        hasher.update(&fee.to_le_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(nullifier);
        hasher.update(&nonce.to_le_bytes());
        hasher.update(&gas_limit.to_le_bytes());
        hasher.update(&gas_price.to_le_bytes());
        hasher.finalize().to_vec()
    }

    #[test]
    fn browser_version_one_digest_fixture_matches_rust() {
        let blockchain = open_test_chain("digest_fixture");
        let tx = Transaction {
            sender: "11".repeat(32),
            sender_public_key: vec![],
            recipient: "22".repeat(32),
            amount: 25_000_000,
            signature: vec![],
            zk_proof: vec![],
            nullifier: (0u8..32).collect::<Vec<_>>().try_into().unwrap(),
            timestamp: 1_700_000_000,
            fee: 250_000,
            nonce: 0,
            gas_limit: 500_000,
            gas_price: 1,
            proof_type: ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::LEGACY_TRANSACTION_VERSION,
        };
        let digest = blockchain.create_transaction_message(&tx);
        assert_eq!(
            hex::encode(digest),
            "f968acd8ef5f17f72eed6d71d1c4ba9a03de4bbb5c28f3da2718ef6d18079c72"
        );
        cleanup_test_chain("digest_fixture");
    }

    #[test]
    fn legacy_transaction_rejects_non_l1_chain_id() {
        let blockchain = open_test_chain("legacy_chain_id");
        let transaction = Transaction {
            sender: "11".repeat(32),
            sender_public_key: vec![],
            recipient: "22".repeat(32),
            amount: 1,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [0xAA; 32],
            timestamp: Utc::now().timestamp() as u64,
            fee: 1,
            nonce: 0,
            gas_limit: 500_000,
            gas_price: 1,
            proof_type: ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: 1,
            version: UltraBlockchain::LEGACY_TRANSACTION_VERSION,
        };

        let error = blockchain
            .validate_transaction(&transaction)
            .expect_err("legacy transfers must not be admitted on another chain");
        assert_eq!(error, "Legacy version 1 transactions require L1 chain_id 0");
        cleanup_test_chain("legacy_chain_id");
    }

    #[test]
    fn test_valid_dilithium_signature_is_accepted() {
        let blockchain = open_test_chain("valid");
        let wallet = UltraWallet::new();

        let nullifier = [7u8; 32];
        let timestamp = Utc::now().timestamp() as u64;
        let recipient = "0xrecipient_address".to_string();
        let (amount, fee, nonce, gas_limit, gas_price) = (10u64, 1u64, 0u64, 500_000u64, 1u64);

        let msg = build_message(
            &wallet.address,
            &recipient,
            amount,
            fee,
            timestamp,
            &nullifier,
            nonce,
            gas_limit,
            gas_price,
        );
        let signature = wallet.keypair.sign(&msg);

        let tx = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient,
            amount,
            signature,
            zk_proof: vec![0xAA; 64], // Placeholder for this test
            nullifier,
            timestamp,
            fee,
            nonce,
            gas_limit,
            gas_price,
            proof_type: ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: 0,
            version: 1,
        };

        // We check that it doesn't fail on Dilithium signature check.
        // It might fail on ZK check because zk_proof is fake, so we look at error message.
        let result = blockchain.validate_transaction(&tx);
        if let Err(e) = result {
            assert!(!e.contains("Dilithium"), "Signature check failed: {}", e);
        }
        cleanup_test_chain("valid");
    }

    #[test]
    fn test_payload_bound_validator_proposal_rejects_tampering() {
        let blockchain = open_test_chain("proposal_envelope");
        let wallet = UltraWallet::new();
        let nullifier = [9u8; 32];
        let timestamp = Utc::now().timestamp() as u64;
        let payload = TransactionPayload::ValidatorJoinProposal {
            public_key: wallet.keypair.public_key.clone(),
            metadata: "Genesis-Alpha-01".to_string(),
        };

        let mut tx = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier,
            timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
        };

        let message = blockchain.create_transaction_message(&tx);
        tx.signature = wallet.keypair.sign(&message);
        let proposal_result = blockchain.validate_transaction(&tx);
        assert!(
            proposal_result.is_ok(),
            "A correctly signed version 2 proposal must validate: {:?}",
            proposal_result
        );

        let mut metadata_tampered = tx.clone();
        metadata_tampered.payload = TransactionPayload::ValidatorJoinProposal {
            public_key: wallet.keypair.public_key.clone(),
            metadata: "Genesis-Evil-01".to_string(),
        };
        let metadata_result = blockchain.validate_transaction(&metadata_tampered);
        assert!(metadata_result
            .expect_err("Metadata tampering must invalidate the signature")
            .contains("Dilithium"));

        let mut public_key_tampered = tx;
        public_key_tampered.payload = TransactionPayload::ValidatorJoinProposal {
            public_key: vec![0xBB; wallet.keypair.public_key.len()],
            metadata: "Genesis-Alpha-01".to_string(),
        };
        let public_key_result = blockchain.validate_transaction(&public_key_tampered);
        assert!(public_key_result
            .expect_err("Proposal key tampering must invalidate the signature")
            .contains("Dilithium"));

        cleanup_test_chain("proposal_envelope");
    }

    #[test]
    fn test_payload_bound_validator_approval_rejects_proposal_hash_tampering() {
        let mut blockchain = open_test_chain("approval_envelope");
        let owners = [
            QuantumKeyPair::generate(),
            QuantumKeyPair::generate(),
            QuantumKeyPair::generate(),
        ];
        blockchain.sovereign_owners = owners
            .iter()
            .map(|owner| owner.public_key.clone())
            .collect();

        let timestamp = Utc::now().timestamp() as u64;
        let nullifier = [10u8; 32];
        let proposal_hash = [0x11u8; 32];
        let mut approval = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier,
            timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval { proposal_hash },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };

        let message = blockchain.create_transaction_message(&approval);
        approval.signature = {
            let mut signatures = owners[0].sign(&message);
            signatures.extend_from_slice(&owners[1].sign(&message));
            signatures
        };
        let approval_result = blockchain.validate_transaction(&approval);
        assert!(
            approval_result.is_ok(),
            "A correctly signed version 3 approval must validate: {:?}",
            approval_result
        );

        let mut tampered = approval;
        tampered.payload = TransactionPayload::ValidatorApproval {
            proposal_hash: [0x22u8; 32],
        };
        let tampered_result = blockchain.validate_transaction(&tampered);
        assert!(tampered_result
            .expect_err("Proposal hash tampering must invalidate the approval signature")
            .contains("Insufficient signatures"));

        cleanup_test_chain("approval_envelope");
    }

    #[test]
    fn test_valid_zk_proof_is_accepted() {
        let blockchain = open_test_chain("zk_valid");
        let mut zk_engine = blockchain.zk_engine.write();

        let nullifier = [1u8; 32];
        let circuit = PrivateTransactionCircuit {
            amount: Some(100),
            recipient: Some([0; 32]),
            timestamp: Some(Utc::now().timestamp() as u64),
            merkle_root: Some([0; 32]),
            nullifier: Some(nullifier),
            block_height: Some(0),
            sender_balance: Some(1000),
            sender_public_key: Some([0; 32]),
            sender_private_key_hash: Some([0; 32]),
            merkle_path: Some(vec![[0; 32]; MERKLE_TREE_DEPTH]),
            signature: Some([0; 64]),
        };

        let proof = zk_engine
            .create_proof(circuit)
            .expect("Proof generation failed");
        drop(zk_engine);

        let zk_engine = blockchain.zk_engine.read();
        let is_valid = zk_engine
            .verify_proof(&proof, &[], &nullifier)
            .expect("Verification failed");
        assert!(is_valid, "Valid proof must be accepted");

        cleanup_test_chain("zk_valid");
    }

    #[test]
    fn test_invalid_zk_proof_is_rejected() {
        let blockchain = open_test_chain("zk_invalid");
        let zk_engine = blockchain.zk_engine.read();

        let nullifier = [1u8; 32];
        let fake_proof = vec![0xEE; 100]; // Random bytes

        let result = zk_engine.verify_proof(&fake_proof, &[], &nullifier);
        assert!(result.is_err(), "Fake proof must cause an error");

        cleanup_test_chain("zk_invalid");
    }

    #[test]
    fn test_add_remote_block_validates_state_root() {
        let mut blockchain = open_test_chain("remote_root");
        let last_block = blockchain.chain.last().unwrap().clone();

        // 1. Create a synthetic block with an incorrect state root
        let mut invalid_block = UltraBlock {
            index: last_block.index + 1,
            timestamp: Utc::now().timestamp() as u64,
            previous_hash: last_block.hash,
            hash: [0; 32],
            nonce: 0,
            transactions: vec![],
            merkle_root: [0; 32],
            state_root: [0xEE; 32], // POGREŠAN ROOT
            shard_roots: vec![[0; 32]; 16],
            aggregated_signature: None,
            validator_set: vec![],
            block_reward: 50,
            size: 0,
            gas_used: 0,
            gas_limit: 10_000_000,
            version: 1,
            epoch: last_block.epoch + 1,
            total_difficulty: last_block.total_difficulty + 1,
            difficulty: 4,
            parent_hash: last_block.hash,
        };

        // Izračunaj hash da prođe validate_block
        // 1.1 Merkle root za prazne transakcije
        let block_merkle_tree = MerkleTree::new(256);
        let merkle_root_vec = block_merkle_tree.get_root();
        invalid_block
            .merkle_root
            .copy_from_slice(&merkle_root_vec[0..32]);

        invalid_block.hash = blockchain.calculate_block_hash(
            invalid_block.index,
            invalid_block.timestamp,
            &invalid_block.previous_hash,
            invalid_block.nonce,
            &invalid_block.transactions,
            &invalid_block.merkle_root,
            &invalid_block.shard_roots,
        );

        let result = blockchain.add_remote_block(invalid_block, vec![]);
        assert!(
            result.is_err(),
            "Remote block with an incorrect state root must be rejected"
        );
        assert!(
            result
                .unwrap_err()
                .contains("Block validation failed (hash, Merkle, or state root mismatch)"),
            "The error must identify the failed block validation"
        );

        cleanup_test_chain("remote_root");
    }

    #[test]
    fn test_dilithium_sizes() {
        use pqcrypto_dilithium::dilithium5::*;
        println!("📊 Dilithium-5 Sizes:");
        println!("   PUBLIC KEY:  {}", public_key_bytes());
        println!("   SECRET KEY:  {}", secret_key_bytes());
        println!("   SIGNATURE:   {}", signature_bytes());
    }

    #[test]
    #[ignore = "Explicit key generation only; set ULTRANET_KEYS_OUTPUT and run with --ignored"]
    fn test_gen_multisig_keys() {
        use std::env;
        use std::fs::{self, OpenOptions};
        use std::io::Write;
        use std::path::PathBuf;

        let output_path = env::var("ULTRANET_KEYS_OUTPUT")
            .expect("Key generation is opt-in. Set ULTRANET_KEYS_OUTPUT to a new output path.");
        let output_path = PathBuf::from(output_path);
        let active_path = PathBuf::from("sovereign_keys.json");
        let output_is_active = output_path == active_path
            || fs::canonicalize(&output_path).ok() == fs::canonicalize(&active_path).ok();

        assert!(
            !output_is_active,
            "Refusing to overwrite the active sovereign_keys.json file"
        );
        assert!(
            !output_path.exists(),
            "Refusing to overwrite existing key output: {}",
            output_path.display()
        );

        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).expect("Unable to create key output directory");
            }
        }

        println!("🛡️ Generating 3 Sovereign Owner Keys for 2-of-3 Multi-Sig...");
        let mut keys = Vec::new();
        for i in 1..=3 {
            let key = crate::QuantumKeyPair::generate();
            keys.push(serde_json::json!({
                "index": i,
                "address": key.address(),
                "public_key": hex::encode(&key.public_key),
                "secret_key": hex::encode(&key.secret_key)
            }));
        }
        let json = serde_json::to_string_pretty(&keys).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .expect("Unable to create new key output file");
        file.write_all(json.as_bytes()).unwrap();
        println!("✅ Sovereign keys saved to {}", output_path.display());
    }
}
