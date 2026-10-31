//! Verify a `send_ultra.py --check-file` artifact with the node's own code.
//!
//! Two things are checked, and the second is the one that catches signing-preimage drift:
//!
//!   1. owner0's Dilithium-5 signature verifies under owner0's public key.
//!   2. The digest the script signed equals the digest `create_transaction_message_for` builds
//!      from the same fields — the exact preimage the node verifies against. Without this, a
//!      layout mismatch in the Python message would only surface as "invalid signature" after the
//!      transfer had already been signed.
//!
//! Run:
//!   cargo run --locked --example verify_sig_artifact -- <artifact.json>
use UltraNet::quantum_crypto::QuantumKeyPair;
use UltraNet::{ProofType, Transaction, TransactionPayload, UltraBlockchain};

fn u64_field(source: &serde_json::Value, key: &str) -> u64 {
    source[key]
        .as_u64()
        .unwrap_or_else(|| panic!("artifact field `{key}` is missing or not a number"))
}

fn str_field(source: &serde_json::Value, key: &str) -> String {
    source[key]
        .as_str()
        .unwrap_or_else(|| panic!("artifact field `{key}` is missing or not a string"))
        .to_string()
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: verify_sig_artifact <artifact.json>");
    let raw = std::fs::read_to_string(&path).expect("read artifact");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse artifact");

    let digest = hex::decode(str_field(&value, "digest")).expect("hex digest");
    let signature = hex::decode(str_field(&value, "signature")).expect("hex signature");
    let public_key = hex::decode(str_field(&value, "public_key")).expect("hex public key");

    let signed = &value["signed_fields"];
    let mut nullifier = [0u8; 32];
    nullifier.copy_from_slice(&hex::decode(str_field(signed, "nullifier")).expect("hex nullifier"));

    let tx = Transaction {
        sender: str_field(signed, "sender"),
        sender_public_key: public_key.clone(),
        recipient: str_field(signed, "recipient"),
        amount: u64_field(signed, "amount"),
        // The signature is not part of the version-1 preimage, so any value is equivalent here.
        signature: signature.clone(),
        zk_proof: Vec::new(),
        nullifier,
        timestamp: u64_field(signed, "timestamp"),
        fee: u64_field(signed, "fee"),
        nonce: u64_field(signed, "nonce"),
        gas_limit: u64_field(signed, "gas_limit"),
        gas_price: u64_field(signed, "gas_price"),
        proof_type: ProofType::Transaction,
        payload: TransactionPayload::StandardTransfer,
        chain_id: u64_field(signed, "chain_id") as u32,
        version: u64_field(signed, "version") as u32,
    };

    let node_digest = UltraBlockchain::create_transaction_message_for(&tx);
    let matches = node_digest == digest;

    println!("sender           : {}", tx.sender);
    println!("recipient        : {}", tx.recipient);
    println!(
        "amount / fee     : {}.{:06} / {}.{:06} ULTRA",
        tx.amount / 1_000_000,
        tx.amount % 1_000_000,
        tx.fee / 1_000_000,
        tx.fee % 1_000_000
    );
    println!("nonce / timestamp: {} / {}", tx.nonce, tx.timestamp);
    println!("signature bytes  : {}", signature.len());
    println!("public key bytes : {}", public_key.len());
    println!("python digest    : {}", hex::encode(&digest));
    println!("node digest      : {}", hex::encode(&node_digest));
    println!("digest matches   : {}", matches);
    let owner0_address = QuantumKeyPair::address_from_public_key(&public_key);
    let owner0_verifies = QuantumKeyPair::verify(&public_key, &node_digest, &signature);

    // The Sovereign account needs 2-of-3: both signatures must verify, and the owners must differ.
    let second_signature = hex::decode(str_field(&value, "second_signature")).expect("hex second signature");
    let second_public_key = hex::decode(str_field(&value, "second_public_key")).expect("hex second key");
    let owner1_address = QuantumKeyPair::address_from_public_key(&second_public_key);
    let owner1_verifies = QuantumKeyPair::verify(&second_public_key, &node_digest, &second_signature);
    let owners_differ = owner0_address != owner1_address;

    println!("owner0 address   : {owner0_address}");
    println!("owner0 verifies  : {owner0_verifies}");
    println!("owner1 address   : {owner1_address}");
    println!("owner1 verifies  : {owner1_verifies}");
    println!("owners differ    : {owners_differ}");
    println!(
        "combined size    : {} bytes",
        signature.len() + second_signature.len()
    );

    if !owner0_verifies || !owner1_verifies || !owners_differ {
        eprintln!("\nFAIL: the 2-of-3 envelope is not valid. Do not submit this transfer.");
        std::process::exit(1);
    }

    if !matches {
        eprintln!(
            "\nFAIL: the signed bytes differ from the node's canonical preimage. \
             Do not submit this transfer."
        );
        std::process::exit(1);
    }
}
