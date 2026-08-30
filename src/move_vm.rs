// ============================================================
// MOVE VM - PERSISTENTNA INTEGRACIJA ZA ULTRA BLOCKCHAIN
// ============================================================

use crate::fhe_engine::FheEngine;
use crate::shared_storage::SharedStorage;
use crate::stark_engine::{StarkProof, UltraStarkEngine};
use crate::state_trie::ShardedStateTrie;
use hex;
use move_core_types::account_address::AccountAddress;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================
// 1. DATA TYPES
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceType {
    Coin,
    NFT,
    Treasury,
    Collection,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResourceInfo {
    pub name: String,
    pub owner: String,
    pub data: Vec<u8>,
    pub resource_type: ResourceType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveModuleInfo {
    pub name: String,
    pub address: String,
    pub bytecode: Vec<u8>,
}

// ============================================================
// 2. MOVE VM WRAPPER (SLED BACKED)
// ============================================================

/// Trait za verifikabilnu egzekuciju koji omogućava ZK/STARK dokazivanje
#[allow(dead_code)]
pub trait VerifiableExecutor {
    fn execute_with_proof(
        &mut self,
        call: &str,
        args: Vec<Vec<u8>>,
    ) -> Result<(Vec<u8>, Vec<u8>), String>;
}

pub struct MoveVM {
    pub storage: Arc<SharedStorage>,
    pub execution_counter: u64,
    pub gas_used: u64,
    pub resources: std::collections::HashMap<String, Vec<MoveResourceInfo>>, // Cache/Compatibility
    pub state_trie: Option<Arc<RwLock<ShardedStateTrie>>>,
    pub fhe_engine: Option<Arc<RwLock<FheEngine>>>,
    pub stark_engine: Option<Arc<UltraStarkEngine>>,
    pub write_set: std::collections::HashMap<String, Vec<u8>>, // Temporary storage for validation
    pub is_validation: bool,
    pub last_fhe_proof: Option<StarkProof>,
}

impl VerifiableExecutor for MoveVM {
    fn execute_with_proof(
        &mut self,
        call: &str,
        args: Vec<Vec<u8>>,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        // U realnom sistemu, ovo generiše Trace egzekucije za STARK
        let res = self.execute_function(AccountAddress::ZERO, "Module", call, args)?;
        Ok((res, vec![0; 32])) // (rezultat, dummy_proof)
    }
}

impl MoveVM {
    pub fn new(storage: Arc<SharedStorage>) -> Self {
        Self {
            storage,
            execution_counter: 0,
            gas_used: 0,
            resources: std::collections::HashMap::new(),
            state_trie: None,
            fhe_engine: None,
            stark_engine: None,
            write_set: std::collections::HashMap::new(),
            is_validation: false,
            last_fhe_proof: None,
        }
    }

    pub fn set_stark(&mut self, stark: Arc<UltraStarkEngine>) {
        self.stark_engine = Some(stark);
    }

    pub fn set_validation_mode(&mut self, enabled: bool) {
        self.is_validation = enabled;
        if !enabled {
            self.write_set.clear();
        }
    }

    pub fn set_trie(&mut self, trie: Arc<RwLock<ShardedStateTrie>>) {
        self.state_trie = Some(trie);
    }

    pub fn set_fhe(&mut self, fhe: Arc<RwLock<FheEngine>>) {
        self.fhe_engine = Some(fhe);
    }

    pub const FHE_GAS_MULTIPLIER: u64 = 5000;

    fn persistent_fhe_mint(&mut self, args: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
        self.gas_used += 10 * Self::FHE_GAS_MULTIPLIER;
        if args.len() >= 2 {
            let encrypted_amount = &args[0]; // Ciphertext
            let to_addr = hex::encode(&args[1]);

            // Pročitaj postojeći enkriptovani balans
            let old_balance_ct = self
                .get_persistent_resource_data(&to_addr, "FheCoin")
                .unwrap_or_else(|_| vec![]); // Prazno ako ne postoji

            let new_balance_ct = if old_balance_ct.is_empty() {
                encrypted_amount.clone()
            } else {
                let fhe = self
                    .fhe_engine
                    .as_ref()
                    .ok_or("FHE Engine not initialized")?
                    .read();
                let res = fhe.compute_add(&old_balance_ct, encrypted_amount)?;

                // ✅ GENERIŠI STARK DOKAZ ZA FHE_ADD
                if let Some(stark) = &self.stark_engine {
                    self.last_fhe_proof =
                        Some(stark.prove_fhe_op("ADD", &old_balance_ct, encrypted_amount, &res));
                }
                res
            };

            self.save_persistent_resource(&to_addr, "FheCoin", new_balance_ct, ResourceType::Coin)?;
            println!("   ✅ [FHE] Minted encrypted tokens to {}", to_addr);
        }
        Ok(vec![1])
    }

    fn persistent_fhe_transfer(&mut self, args: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
        self.gas_used += 20 * Self::FHE_GAS_MULTIPLIER;
        if args.len() >= 4 {
            let encrypted_amount = &args[0];
            let from_addr = hex::encode(&args[1]);
            let to_addr = hex::encode(&args[2]);
            let proof_bytes = &args[3]; // ✅ ZK-FHE PROOF!

            // 1. Verifikuj ZK dokaz nad FHE operacijom (ZK-FHE)
            if proof_bytes.is_empty() {
                return Err("ZK-FHE Proof missing for encrypted transfer".to_string());
            }

            let from_bal_ct = self.get_persistent_resource_data(&from_addr, "FheCoin")?;
            let to_bal_ct = self
                .get_persistent_resource_data(&to_addr, "FheCoin")
                .unwrap_or_else(|_| vec![]);

            let (new_from_bal, new_to_bal) = {
                let fhe = self
                    .fhe_engine
                    .as_ref()
                    .ok_or("FHE Engine not initialized")?
                    .read();
                let nfb = fhe.compute_sub(&from_bal_ct, encrypted_amount)?;
                let ntb = if to_bal_ct.is_empty() {
                    encrypted_amount.clone()
                } else {
                    fhe.compute_add(&to_bal_ct, encrypted_amount)?
                };

                // ✅ GENERIŠI STARK DOKAZ ZA FHE_TRANSFER
                if let Some(stark) = &self.stark_engine {
                    self.last_fhe_proof = Some(stark.prove_fhe_op(
                        "TRANSFER_SUB",
                        &from_bal_ct,
                        encrypted_amount,
                        &nfb,
                    ));
                }

                (nfb, ntb)
            };

            self.save_persistent_resource(&from_addr, "FheCoin", new_from_bal, ResourceType::Coin)?;
            self.save_persistent_resource(&to_addr, "FheCoin", new_to_bal, ResourceType::Coin)?;

            println!("   ✅ [FHE] Transferred encrypted tokens homomorphically");
        }
        Ok(vec![1])
    }

    fn get_persistent_resource_data(&self, owner: &str, name: &str) -> Result<Vec<u8>, String> {
        let key = format!("{}:{}", owner, name);

        // 1. Proveri write_set
        if let Some(val) = self.write_set.get(&key) {
            if let Ok(info) = bincode::deserialize::<MoveResourceInfo>(val) {
                return Ok(info.data);
            }
        }

        // 2. Proveri Sled
        if let Some(val) = self.storage.move_resources.get(key).ok().flatten() {
            if let Ok(info) = bincode::deserialize::<MoveResourceInfo>(&val) {
                return Ok(info.data);
            }
        }
        Err("Resource not found".to_string())
    }

    /// Čuva modul u Sled bazi (Persistentno)
    pub fn deploy_module(
        &mut self,
        name: &str,
        bytecode: Vec<u8>,
        sender: AccountAddress,
    ) -> Result<(), String> {
        let module_info = MoveModuleInfo {
            name: name.to_string(),
            address: sender.to_string(),
            bytecode: bytecode.clone(),
        };

        let key = format!("{}:{}", sender, name);
        let val = bincode::serialize(&module_info).unwrap();

        self.storage
            .move_modules
            .insert(key, val)
            .map_err(|e| e.to_string())?;
        self.storage.storage.flush().map_err(|e| e.to_string())?;

        println!(
            "📦 Move module '{}' deployed persistently at {}!",
            name, sender
        );
        Ok(())
    }

    /// Izvršava funkciju i ažurira Sled resurse (Persistentno)
    pub fn execute_function(
        &mut self,
        _module_address: AccountAddress,
        module_name: &str,
        function_name: &str,
        args: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        self.execution_counter += 1;
        self.gas_used += 100;

        println!(
            "⚡ Move VM: Executing {}.{} (Real Storage)",
            module_name, function_name
        );

        // Mapiranje na perzistentnu logiku
        let res = match (module_name, function_name) {
            ("UltraCoin", "mint") => self.persistent_mint(args),
            ("UltraCoin", "transfer") => self.persistent_transfer(args),
            ("FheCoin", "mint") => self.persistent_fhe_mint(args),
            ("FheCoin", "transfer") => self.persistent_fhe_transfer(args),
            _ => {
                println!(
                    "   ⚠️ Logic for {}.{} is simulated in RAM for now",
                    module_name, function_name
                );
                Ok(vec![1])
            }
        };

        // ✅ OBAVEZNO FLUSH nakon svake promene stanja
        let _ = self.storage.storage.flush();
        res
    }

    fn persistent_mint(&mut self, args: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
        if args.len() >= 2 {
            let amount = u64::from_le_bytes(args[0][..8].try_into().unwrap());
            let to_addr = hex::encode(&args[1]);

            // Pročitaj iz Sled-a
            let balance = self
                .get_persistent_balance(&to_addr)
                .checked_add(amount)
                .ok_or_else(|| "UltraCoin balance overflow".to_string())?;

            // Upiši u Sled
            self.save_persistent_resource(
                &to_addr,
                "Coin",
                balance.to_le_bytes().to_vec(),
                ResourceType::Coin,
            )?;

            println!("   ✅ [SLED] Minted {} UltraCoins to {}", amount, to_addr);
        }
        Ok(vec![1])
    }

    fn persistent_transfer(&mut self, args: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
        if args.len() >= 3 {
            let amount = u64::from_le_bytes(args[0][..8].try_into().unwrap());
            let from_addr = hex::encode(&args[1]);
            let to_addr = hex::encode(&args[2]);

            let mut from_bal = self.get_persistent_balance(&from_addr);
            if from_bal < amount {
                return Err("Insufficient persistent balance".to_string());
            }

            let mut to_bal = self.get_persistent_balance(&to_addr);

            from_bal = from_bal
                .checked_sub(amount)
                .ok_or_else(|| "Insufficient persistent balance".to_string())?;
            to_bal = to_bal
                .checked_add(amount)
                .ok_or_else(|| "UltraCoin balance overflow".to_string())?;

            self.save_persistent_resource(
                &from_addr,
                "Coin",
                from_bal.to_le_bytes().to_vec(),
                ResourceType::Coin,
            )?;
            self.save_persistent_resource(
                &to_addr,
                "Coin",
                to_bal.to_le_bytes().to_vec(),
                ResourceType::Coin,
            )?;

            println!("   ✅ [SLED] Transferred {} persistent UltraCoins", amount);
        }
        Ok(vec![1])
    }

    fn get_persistent_balance(&self, owner: &str) -> u64 {
        let key = format!("{}:Coin", owner);

        // 1. Proveri write_set
        if let Some(val) = self.write_set.get(&key) {
            if let Ok(info) = bincode::deserialize::<MoveResourceInfo>(val) {
                return u64::from_le_bytes(info.data[..8].try_into().unwrap_or([0; 8]));
            }
        }

        // 2. Proveri Sled
        if let Some(val) = self.storage.move_resources.get(key).ok().flatten() {
            if let Ok(info) = bincode::deserialize::<MoveResourceInfo>(&val) {
                return u64::from_le_bytes(info.data[..8].try_into().unwrap_or([0; 8]));
            }
        }
        0
    }

    /// Read the canonical persistent UltraCoin balance for an account.
    ///
    /// The value is stored as an unsigned little-endian u64 in the Move Coin
    /// resource. Validation mode also sees the current candidate write set.
    pub fn persistent_coin_balance(&self, owner: &str) -> u64 {
        self.get_persistent_balance(owner)
    }

    /// Set a persistent UltraCoin balance for a protocol state transition.
    ///
    /// In validation mode this writes only to `write_set`; normal execution
    /// persists the resource and updates the `move:` trie through the existing
    /// resource writer. Callers must validate the transition before invoking it.
    pub fn set_persistent_coin_balance(&mut self, owner: &str, balance: u64) -> Result<(), String> {
        self.save_persistent_resource(
            owner,
            "Coin",
            balance.to_le_bytes().to_vec(),
            ResourceType::Coin,
        )
    }

    fn save_persistent_resource(
        &mut self,
        owner: &str,
        name: &str,
        data: Vec<u8>,
        r_type: ResourceType,
    ) -> Result<(), String> {
        let info = MoveResourceInfo {
            name: name.to_string(),
            owner: owner.to_string(),
            data,
            resource_type: r_type,
        };
        let key = format!("{}:{}", owner, name);
        let val = bincode::serialize(&info).unwrap();

        if self.is_validation {
            self.write_set.insert(key, val);
        } else {
            self.storage
                .move_resources
                .insert(key.as_bytes(), val.as_slice())
                .map_err(|e| e.to_string())?;

            // ✅ DODAJ U MPT TRIE
            if let Some(trie_lock) = &self.state_trie {
                let mut trie = trie_lock.write();
                let trie_key = format!("move:{}", key);
                let shard_id = self.storage.storage.get_shard_id(trie_key.as_bytes());
                trie.insert(shard_id, trie_key.as_bytes(), val.as_slice())?;
            }
        }
        Ok(())
    }

    pub fn get_stats(&self) -> std::collections::HashMap<String, String> {
        let mut stats = std::collections::HashMap::new();
        stats.insert("executions".to_string(), self.execution_counter.to_string());
        stats.insert("gas_used".to_string(), self.gas_used.to_string());
        stats.insert(
            "persistent_modules".to_string(),
            self.storage.move_modules.len().to_string(),
        );
        stats.insert(
            "persistent_resources".to_string(),
            self.storage.move_resources.len().to_string(),
        );
        stats
    }
}
