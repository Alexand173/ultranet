use sha3::{Digest, Sha3_256};
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::convert::TryInto;
use rayon::prelude::*;
use hex;
use chrono::Utc;
use rand::rngs::OsRng;
use rand::RngCore;

// ============ 1. KVANTNO OTPORNA KRIPTOGRAFIJA (DILITHIUM) ============
mod quantum_crypto {
    use super::*;
    use pqcrypto_dilithium::dilithium5::*;
    use pqcrypto_traits::sign::{PublicKey, SecretKey, Signature, SigningKey, Signer};

    #[derive(Debug, Clone)]
    pub struct QuantumKeyPair {
        pub public_key: Vec<u8>,
        pub secret_key: Vec<u8>,
    }

    impl QuantumKeyPair {
        pub fn generate() -> Self {
            let (pk, sk) = keypair();
            Self {
                public_key: pk.as_bytes().to_vec(),
                secret_key: sk.as_bytes().to_vec(),
            }
        }

        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            // Pravilno korišćenje SecretKey iz pqcrypto_traits
            let sk = SecretKey::from_bytes(&self.secret_key)
                .expect("Invalid secret key");
            let signature = sign(message, &sk);
            signature.as_bytes().to_vec()
        }

        pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
            // Pravilno korišćenje PublicKey i Signature iz pqcrypto_traits
            if let Ok(pk) = BlsPublicKey::from_bytes
(public_key) {
                if let Ok(sig) = Signature::from_bytes(signature) {
                    return verify(&sig, message, &pk).is_ok();
                }
            }
            false
        }

        pub fn address(&self) -> String {
            let mut hasher = Sha3_256::new();
            hasher.update(&self.public_key);
            hex::encode(hasher.finalize())
        }
    }

    // Zeroizacija pri drop-u (zaštita od side-channel napada)
    impl Drop for QuantumKeyPair {
        fn drop(&mut self) {
            self.secret_key.iter_mut().for_each(|b| *b = 0);
            self.public_key.iter_mut().for_each(|b| *b = 0);
        }
    }
}

// ============ 2. MERKLE STABLA ============
mod merkle_tree {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct MerkleProof {
        pub leaf: Vec<u8>,
        pub siblings: Vec<Vec<u8>>,
        pub root: Vec<u8>,
        pub leaf_index: usize,
    }

    pub struct MerkleTree {
        pub root: Vec<u8>,
        levels: Vec<Vec<Vec<u8>>>,
    }

    impl MerkleTree {
        pub fn new(leaves: &[Vec<u8>]) -> Self {
            if leaves.is_empty() {
                return Self {
                    root: vec![0; 32],
                    levels: vec![],
                };
            }

            let mut current: Vec<Vec<u8>> = leaves.iter().map(|l| Self::hash_leaf(l)).collect();
            let mut levels = vec![current.clone()];

            while current.len() > 1 {
                if current.len() % 2 != 0 {
                    current.push(current.last().unwrap().clone());
                }
                
                let mut next = Vec::new();
                for chunk in current.chunks(2) {
                    let combined = [chunk[0].clone(), chunk[1].clone()].concat();
                    next.push(Self::hash_internal(&combined));
                }
                levels.push(next.clone());
                current = next;
            }

            Self {
                root: current.first().cloned().unwrap_or_else(|| vec![0; 32]),
                levels,
            }
        }

        pub fn generate_proof(&self, leaf: &[u8]) -> Option<MerkleProof> {
            let leaf_hash = Self::hash_leaf(leaf);
            let mut leaf_index = self.levels[0].iter().position(|h| h == &leaf_hash)?;
            let mut siblings = Vec::new();
            let mut idx = leaf_index;

            for level in 0..self.levels.len() - 1 {
                let sibling = if idx % 2 == 0 {
                    if idx + 1 < self.levels[level].len() {
                        self.levels[level][idx + 1].clone()
                    } else {
                        self.levels[level][idx].clone()
                    }
                } else {
                    self.levels[level][idx - 1].clone()
                };
                siblings.push(sibling);
                idx /= 2;
            }

            Some(MerkleProof {
                leaf: leaf_hash,
                siblings,
                root: self.root.clone(),
                leaf_index,
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
}

// ============ 3. BLS AGREGACIJA POTPISA ============
mod bls_aggregation {
    use super::*;
    use bls_signatures::{PrivateKey, PublicKey, Signature, Serialize, Deserialize};

   #[derive(Debug, Clone)] // Dodaj Debug ovde
    pub struct AggregatedSignature {
        pub signature: Vec<u8>,
        pub public_keys: Vec<Vec<u8>>,
        pub message_hash: [u8; 32],
    }

    pub struct BLSValidator {
        pub validators: Vec<Vec<u8>>,
    }

    impl BLSValidator {
        pub fn new() -> Self {
            let mut validators = Vec::new();
            // Generišemo 3 početna validatora sa random generatorom
            let mut rng = rand::thread_rng();
            for _ in 0..3 {
                let sk = PrivateKey::generate(&mut rng);
                let pk = sk.public_key();
                validators.push(pk.as_bytes().to_vec());
            }
            Self { validators }
        }

        pub fn aggregate_signatures(
            &self,
            message: &[u8],
            signatures: Vec<(Vec<u8>, Vec<u8>)>, // (public_key, signature)
        ) -> Option<AggregatedSignature> {
            let message_hash = self.hash_message(message);
            let mut sigs = Vec::new();
            let mut pks = Vec::new();

            for (pk_bytes, sig_bytes) in signatures {
                if let (Ok(pk), Ok(sig)) = (
                    BlsPublicKey::from_bytes
(&pk_bytes),
                    Signature::from_bytes(&sig_bytes),
                ) {
                    if pk.verify(&message_hash, &sig) {
                        sigs.push(sig);
                        pks.push(pk_bytes);
                    }
                }
            }

            if sigs.is_empty() {
                return None;
            }

            let aggregated = Signature::aggregate(&sigs).ok()?;
            Some(AggregatedSignature {
                signature: aggregated.as_bytes().to_vec(),
                public_keys: pks,
                message_hash,
            })
        }

        pub fn verify_aggregated(&self, agg: &AggregatedSignature, message: &[u8]) -> bool {
            let msg_hash = self.hash_message(message);
            if msg_hash != agg.message_hash {
                return false;
            }

            let mut pks = Vec::new();
            for pk_bytes in &agg.public_keys {
                if let Ok(pk) = BlsPublicKey::from_bytes
(pk_bytes) {
                    pks.push(pk);
                } else {
                    return false;
                }
            }

            if pks.is_empty() {
                return false;
            }

            if let (Ok(sig), Ok(agg_pk)) = (
                Signature::from_bytes(&agg.signature),
                bls_signatures::aggregate(&pks),
            ) {
                return agg_pk.verify(&msg_hash, &sig);
            }
            false
        }

        fn hash_message(&self, message: &[u8]) -> [u8; 32] {
            let mut hasher = Sha3_256::new();
            hasher.update(b"BLS_AGG_MSG");
            hasher.update(message);
            hasher.finalize().into()
        }
    }
}

// ============ 4. GLAVNE STRUKTURE ============
use quantum_crypto::QuantumKeyPair;
use merkle_tree::{MerkleTree, MerkleProof};
use bls_aggregation::{BLSValidator, AggregatedSignature};

#[derive(Debug, Clone)]
struct Transaction {
    sender: String,              // Adresa (hash javnog ključa)
    sender_public_key: Vec<u8>,  // Pun javni ključ za verifikaciju
    recipient: String,
    amount: u64,
    signature: Vec<u8>,          // Dilithium potpis
    zk_proof: Option<Vec<u8>>,   // ZK-SNARK dokaz (pojednostavljeno)
    timestamp: u64,
    merkle_proof: Option<MerkleProof>,
}

#[derive(Debug, Clone)]
struct UltraBlock {
    index: u64,
    timestamp: u64,
    previous_hash: String,
    hash: String,
    nonce: u64,
    transactions: Vec<Transaction>,
    merkle_root: String,         // Merkle root svih transakcija
    aggregated_signature: Option<AggregatedSignature>,
    state_root: String,          // Stanje svih balansa
}

struct Blockchain {
    chain: Vec<UltraBlock>,
    bls_validator: BLSValidator,
}

struct Validator {
    balances: HashMap<String, u64>,
    pending_transactions: Vec<Transaction>,
}

// ============ 5. IMPLEMENTACIJA ============
impl Validator {
    fn new() -> Self {
        Self {
            balances: HashMap::new(),
            pending_transactions: Vec::new(),
        }
    }

    // Validacija transakcije sa svim naprednim tehnikama
    fn validate_transaction(&mut self, tx: &Transaction) -> bool {
        // 1. Verifikacija Dilithium potpisa (kvantna otpornost)
        let msg = self.create_transaction_message(tx);
        if !QuantumKeyPair::verify(&tx.sender_public_key, &msg, &tx.signature) {
            println!("❌ Nevalidan potpis!");
            return false;
        }

        // 2. Provera balansa (sprečavanje duple potrošnje)
        let sender_balance = self.balances.get(&tx.sender).unwrap_or(&1000);
        if tx.amount > *sender_balance {
            println!("❌ Nedovoljno sredstava!");
            return false;
        }

        // 3. Verifikacija Merkle dokaza (ako postoji)
        if let Some(proof) = &tx.merkle_proof {
            let tree = MerkleTree::new(&[tx.sender_public_key.clone()]);
            if !tree.verify_proof(proof) {
                println!("❌ Nevalidan Merkle dokaz!");
                return false;
            }
        }

        // 4. ZK-SNARK validacija (pojednostavljeno)
        if let Some(proof) = &tx.zk_proof {
            // U pravoj implementaciji: verify_proof(proof, public_inputs)
            // Ovo je simulacija
            if proof.len() < 32 {
                println!("❌ Nevalidan ZK dokaz!");
                return false;
            }
        }

        true
    }

    fn create_transaction_message(&self, tx: &Transaction) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(tx.sender.as_bytes());
        hasher.update(tx.recipient.as_bytes());
        hasher.update(&tx.amount.to_le_bytes());
        hasher.update(&tx.timestamp.to_le_bytes());
        hasher.finalize().to_vec()
    }

    fn validate_block(&self, block: &UltraBlock, prev: &UltraBlock) -> bool {
        // 1. Provera PoW
        if !block.hash.starts_with("0000") {
            println!("❌ Nevalidan PoW!");
            return false;
        }

        // 2. Provera hash-a
        let recomputed = calculate_hash(
            block.index,
            block.timestamp,
            &block.previous_hash,
            block.nonce,
            &block.transactions,
            &block.merkle_root,
        );
        if block.hash != recomputed {
            println!("❌ Hash ne odgovara!");
            return false;
        }

        // 3. Provera Merkle root-a
        let tx_hashes: Vec<Vec<u8>> = block.transactions
            .iter()
            .map(|tx| {
                let mut hasher = Sha3_256::new();
                hasher.update(tx.sender.as_bytes());
                hasher.update(tx.recipient.as_bytes());
                hasher.update(&tx.amount.to_le_bytes());
                hasher.finalize().to_vec()
            })
            .collect();
        
        let tree = MerkleTree::new(&tx_hashes);
        if hex::encode(&tree.root) != block.merkle_root {
            println!("❌ Merkle root ne odgovara!");
            return false;
        }

        // 4. Provera BLS agregacije
        if let Some(agg_sig) = &block.aggregated_signature {
            if !self.bls_validator.verify_aggregated(agg_sig, block.hash.as_bytes()) {
                println!("❌ BLS agregacija nevalidna!");
                return false;
            }
        }

        // 5. Provera vremena
        if block.timestamp <= prev.timestamp + self.block_time {

            println!("❌ Vreme nije validno!");
            return false;
        }

        true
    }

    fn update_state(&mut self, block: &UltraBlock) {
        for tx in &block.transactions {
            let sender_balance = self.balances.entry(tx.sender.clone()).or_insert(1000);
            *sender_balance = sender_balance.saturating_sub(tx.amount);
            
            let recipient_balance = self.balances.entry(tx.recipient.clone()).or_insert(1000);
            *recipient_balance = recipient_balance.saturating_add(tx.amount);
        }
    }

    fn calculate_state_root(&self) -> String {
        let mut state_hashes = Vec::new();
        for (address, balance) in &self.balances {
            let mut hasher = Sha3_256::new();
            hasher.update(address.as_bytes());
            hasher.update(&balance.to_le_bytes());
            state_hashes.push(hasher.finalize().to_vec());
        }
        
        let tree = MerkleTree::new(&state_hashes);
        hex::encode(&tree.root)
    }
}

fn calculate_hash(
    index: u64,
    timestamp: u64,
    prev_hash: &str,
    nonce: u64,
    transactions: &[Transaction],
    merkle_root: &str,
) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(&index.to_le_bytes());
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(prev_hash.as_bytes());
    hasher.update(&nonce.to_le_bytes());
    hasher.update(merkle_root.as_bytes());
    
    // Dodajemo hash transakcija
    for tx in transactions {
        hasher.update(tx.sender.as_bytes());
        hasher.update(tx.recipient.as_bytes());
        hasher.update(&tx.amount.to_le_bytes());
    }
    
    hex::encode(hasher.finalize())
}
fn calculate_genesis_hash() -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"ULTRA BLOCKCHAIN 3.0 - GENESIS");
    hasher.update(&Utc::now().timestamp().to_le_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

impl Blockchain {
    fn new() -> Self {
        Self {
            chain: vec![UltraBlock {
                index: 0,
                timestamp: 0,
                previous_hash: "0".to_string(),
                hash: "genesis_hash".to_string(),
                nonce: 0,
                transactions: vec![],
                merkle_root: "0".to_string(),
                aggregated_signature: None,
                state_root: "0".to_string(),
            }],
            bls_validator: BLSValidator::new(),
        }
    }

    fn add_block(&mut self, transactions: Vec<Transaction>, validator: &mut Validator) {
        let (last_index, last_hash) = {
            let last = self.chain.last().unwrap();
            (last.index, last.hash.clone())
        };

        // 1. Validacija svih transakcija
        let mut valid_txs = Vec::new();
        for tx in transactions {
            if validator.validate_transaction(&tx) {
                valid_txs.push(tx);
            }
        }

        if valid_txs.is_empty() {
            println!("❌ Nema validnih transakcija!");
            return;
        }

        // 2. Kreiranje Merkle stabla
        let tx_hashes: Vec<Vec<u8>> = valid_txs
            .iter()
            .map(|tx| {
                let mut hasher = Sha3_256::new();
                hasher.update(tx.sender.as_bytes());
                hasher.update(tx.recipient.as_bytes());
                hasher.update(&tx.amount.to_le_bytes());
                hasher.finalize().to_vec()
            })
            .collect();
        
        let merkle_tree = MerkleTree::new(&tx_hashes);
        let merkle_root = hex::encode(&merkle_tree.root);

        // 3. Rudarenje sa paralelizacijom
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        println!("⛏️ Rudarenje bloka {} (PoW težina 4)...", last_index + 1);

        let (nonce, hash) = (0..u64::MAX)
            .into_par_iter()
            .find_map_any(|n| {
                let h = calculate_hash(
                    last_index + 1,
                    start_time,
                    &last_hash,
                    n,
                    &valid_txs,
                    &merkle_root,
                );
                if h.starts_with("0000") {
                    Some((n, h))
                } else {
                    None
                }
            })
            .expect("Rudarenje nije uspelo!");

        // 4. Agregacija potpisa (BLS)
        let mut sigs = Vec::new();
        for tx in &valid_txs {
            sigs.push((tx.sender_public_key.clone(), tx.signature.clone()));
        }
        
        let aggregated = self.bls_validator.aggregate_signatures(
            hash.as_bytes(),
            sigs,
        );

        // 5. Kreiranje bloka
        let new_block = UltraBlock {
            index: last_index + 1,
            timestamp: start_time,
            previous_hash: last_hash,
            hash,
            nonce,
            transactions: valid_txs,
            merkle_root,
            aggregated_signature: aggregated,
            state_root: validator.calculate_state_root(),
        };

        // 6. Validacija bloka
        if validator.validate_block(&new_block, self.chain.last().unwrap()) {
            validator.update_state(&new_block);
            self.chain.push(new_block);
            println!("✨ Blok {} uspešno dodat!", last_index + 1);
            println!("🔍 Stanje balansa: {:?}", validator.balances);
        } else {
            println!("❌ Blok odbijen!");
        }
    }

    fn is_chain_valid(&self) -> bool {
        for i in 1..self.chain.len() {
            let current = &self.chain[i];
            let previous = &self.chain[i - 1];

            if current.previous_hash != previous.hash {
                println!("⚠️ NAPAD DETEKTOVAN: Veza prekinuta!");
                return false;
            }

            let recomputed = calculate_hash(
                current.index,
                current.timestamp,
                &current.previous_hash,
                current.nonce,
                &current.transactions,
                &current.merkle_root,
            );
            
            if current.hash != recomputed {
                println!("⚠️ NAPAD DETEKTOVAN: Sadržaj izmenjen!");
                return false;
            }
        }
        println!("✅ Lanac je validan!");
        true
    }
    fn calculate_block_hash(
    &self,
    index: u64,
    timestamp: u64,
    previous_hash: &[u8; 32],
    nonce: u64,
    transactions: &[Transaction],
    merkle_root: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(&index.to_le_bytes());
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(previous_hash);
    hasher.update(&nonce.to_le_bytes());
    hasher.update(merkle_root);
    
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

fn calculate_state_root(&self) -> [u8; 32] {
    let state = self.state.read();
    let mut state_hashes = Vec::new();
    
    for (address, balance) in state.iter() {
        let mut hasher = Sha3_256::new();
        hasher.update(address.as_bytes());
        hasher.update(&balance.to_le_bytes());
        state_hashes.push(hasher.finalize().to_vec());
    }
    
    if state_hashes.is_empty() {
        let mut root = [0u8; 32];
        root[0] = 1;
        return root;
    }
    
    let mut tree = MerkleTree::new(256);
    for hash in state_hashes {
        tree.insert(&hash, &hash);
    }
    let root = tree.get_root();
    let mut root_array = [0u8; 32];
    root_array.copy_from_slice(&root[0..32]);
    root_array
}

fn calculate_block_reward(&self) -> u64 {
    let halvings = self.chain.len() / 100000;
    let reward = 50 >> halvings;
    if reward == 0 { 1 } else { reward }
}
}

// ============ 6. NOVČANIK SA KVANTNOM OTPORNOŠĆU ============
struct Wallet {
    keypair: QuantumKeyPair,
    address: String,
}

impl Wallet {
    fn new() -> Self {
        let keypair = QuantumKeyPair::generate();
        let address = keypair.address();
        Self { keypair, address }
    }

    fn create_transaction(
        &self,
        recipient: String,
        amount: u64,
        zk_proof: Option<Vec<u8>>,
    ) -> Transaction {
        let timestamp = Utc::now().timestamp() as u64;
        let msg = self.create_message(&recipient, amount, timestamp);
        let signature = self.keypair.sign(&msg);

        Transaction {
            sender: self.address.clone(),
            sender_public_key: self.keypair.public_key.clone(),
            recipient,
            amount,
            signature,
            zk_proof,
            timestamp,
            merkle_proof: None,
        }
    }

    fn create_message(&self, recipient: &str, amount: u64, timestamp: u64) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(self.address.as_bytes());
        hasher.update(recipient.as_bytes());
        hasher.update(&amount.to_le_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hasher.finalize().to_vec()
    }
}

// ============ 7. GLAVNA FUNKCIJA ============
fn main() {
    println!("🔗 POKRETANJE ULTRANET 2.0 - NAPREDNI BLOCKCHAIN");
    println!("=================================================\n");

    let mut blockchain = Blockchain::new();
    let mut validator = Validator::new();

    // Kreiranje novčanika
    let alice = Wallet::new();
    let bob = Wallet::new();

    println!("👤 Alice adresa: {}", alice.address);
    println!("👤 Bob adresa: {}", bob.address);
    println!("💰 Početno stanje: 1000 tokena\n");

    // Kreiranje transakcije sa ZK dokazom (simulacija)
    let zk_proof = Some(vec![0x01, 0x02, 0x03, 0x04]); // Simulirani ZK dokaz
    let tx = alice.create_transaction(bob.address.clone(), 100, zk_proof);

    println!("📝 Transakcija: Alice -> Bob (100 tokena)");
    println!("🔐 Kvantno-otporni potpis: {}...", hex::encode(&tx.signature[..8]));

    // Dodavanje bloka
    blockchain.add_block(vec![tx], &mut validator);

    // Provera validnosti lanca
    println!("\n🔍 Provera validnosti lanca...");
    blockchain.is_chain_valid();

    // Simulacija napada
    println!("\n🚫 SIMULACIJA NAPADA:");
    if let Some(block) = blockchain.chain.get_mut(1) {
        block.transactions[0].amount = 999999;
        println!("💀 Izmenjena transakcija: amount = {}", block.transactions[0].amount);
    }

    println!("🔍 Provera nakon napada:");
    blockchain.is_chain_valid();

    println!("\n✅ KRAJ - Sistem je bezbedan!");
}