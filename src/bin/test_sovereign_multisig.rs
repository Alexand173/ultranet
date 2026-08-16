// ============================================================
// TEST: Sovereign 2-of-3 Multi-Sig verification (offline, no network)
// ============================================================
// Ovaj binarni fajl NE pokreće node, NE otvara portove i NE dodiruje
// mainnet bazu. Kreira potpuno izolovanu privremenu bazu, konstruiše
// transakciju sa sender = SOVEREIGN_ADDR, potpisuje je sa 2 od 3
// stvarna secret_key-a iz sigurnosnog backup fajla i poziva
// `UltraBlockchain::add_transaction` (koji interno zove
// `validate_transaction` i tako testira PRAVU multisig logiku).
//
// Backup fajl se čita samo lokalno sa diska (van repo-a) i NIKAD
// se ne upisuje nazad niti loguje u celosti.
// ============================================================

use chrono::Utc;
use serde::Deserialize;
use sha3::{Digest, Sha3_256};
use std::fs;
use UltraNet::{ProofType, QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain};

#[derive(Deserialize)]
struct OwnerEntry {
    #[allow(dead_code)]
    address: String,
    name: String,
    #[allow(dead_code)]
    public_key: String,
    secret_key: String,
}

#[derive(Deserialize)]
struct BackupFile {
    owners: Vec<OwnerEntry>,
}

const BACKUP_PATH: &str = "/home/valerian/.kombai/sovereign_keys_backup_20260720_185830.json";

/// Replika `create_transaction_message` iz src/lib.rs (private metod, pa ga
/// ovde repliciramo identično da bismo potpisali isti message koji node
/// verifikuje).
fn create_transaction_message(tx: &Transaction) -> Vec<u8> {
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

    if tx.version == UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION {
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

fn sign_with_secret_key(secret_key_hex: &str, message: &[u8]) -> Vec<u8> {
    let secret_key = hex::decode(secret_key_hex).expect("Invalid secret key hex");
    let kp = QuantumKeyPair {
        public_key: vec![],
        secret_key,
        key_id: [0u8; 32],
        created_at: 0,
        version: 1,
    };
    kp.sign(message)
}

fn build_base_tx() -> Transaction {
    Transaction {
        sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        sender_public_key: vec![],
        recipient: "test_recipient_offline_check".to_string(),
        amount: 0,
        signature: vec![],
        zk_proof: vec![],
        nullifier: [0u8; 32],
        timestamp: Utc::now().timestamp() as u64,
        fee: 0,
        nonce: 0,
        gas_limit: 10_000_000,
        gas_price: 1,
        proof_type: ProofType::Transaction,
        payload: TransactionPayload::ValidatorApproval {
            proposal_hash: [0u8; 32],
        },
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
    }
}

fn main() {
    println!("🧪 Sovereign 2-of-3 Multi-Sig offline test\n");

    let raw = fs::read_to_string(BACKUP_PATH).expect("Cannot read the key backup file");
    let backup: BackupFile = serde_json::from_str(&raw).expect("Invalid backup JSON");
    assert_eq!(
        backup.owners.len(),
        3,
        "The backup must contain exactly 3 owners"
    );

    let db_path = "test_sovereign_sig_db";
    let _ = fs::remove_dir_all(db_path);
    let blockchain = UltraBlockchain::new(db_path);

    // ---------- TEST 1: 2 ispravna potpisa (Owner #1 + Owner #2) → treba da PROĐE ----------
    let mut tx_ok = build_base_tx();
    let msg = create_transaction_message(&tx_ok);
    let sig1 = sign_with_secret_key(&backup.owners[0].secret_key, &msg);
    let sig2 = sign_with_secret_key(&backup.owners[1].secret_key, &msg);
    let mut combined = Vec::new();
    combined.extend_from_slice(&sig1);
    combined.extend_from_slice(&sig2);
    tx_ok.signature = combined;

    println!(
        "Test 1: signatures from '{}' + '{}' (2-of-3)",
        backup.owners[0].name, backup.owners[1].name
    );
    match blockchain.add_transaction(tx_ok) {
        Ok(()) => println!("  ✅ PASSED — transaction accepted (2-of-3 threshold satisfied)\n"),
        Err(e) => println!("  ❌ FAILED — error: {}\n", e),
    }

    // ---------- TEST 2: samo 1 potpis (Owner #3) → treba da PUKNE (Insufficient signatures) ----------
    let mut tx_fail = build_base_tx();
    tx_fail.nonce = 0; // get_nonce demo uvek vraća 0
    let msg2 = create_transaction_message(&tx_fail);
    let sig3 = sign_with_secret_key(&backup.owners[2].secret_key, &msg2);
    tx_fail.signature = sig3;

    println!(
        "Test 2: only 1 signature ('{}') — this must NOT pass",
        backup.owners[2].name
    );
    match blockchain.add_transaction(tx_fail) {
        Ok(()) => {
            println!("  ❌ UNEXPECTED PASS — security error: 1 signature must not be sufficient!\n")
        }
        Err(e) => println!("  ✅ PASSED (expected rejection) — {}\n", e),
    }

    let _ = fs::remove_dir_all(db_path);
    println!(
        "🧹 Temporary test database removed. The mainnet database (ultranet_db) was not touched."
    );
}
