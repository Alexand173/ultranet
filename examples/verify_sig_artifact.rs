//! Temporary cross-check: verify a signature produced by the Python `pqc` binding with the same
//! dilithium5 implementation the node uses. Run:
//!   cargo run --locked --example verify_sig_artifact -- <artifact.json>
use UltraNet::quantum_crypto::QuantumKeyPair;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: verify_sig_artifact <artifact.json>");
    let raw = std::fs::read_to_string(&path).expect("read artifact");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse artifact");

    let digest = hex::decode(value["digest"].as_str().expect("digest")).expect("hex digest");
    let signature = hex::decode(value["signature"].as_str().expect("signature")).expect("hex signature");
    let public_key = hex::decode(value["public_key"].as_str().expect("public_key")).expect("hex public key");

    println!("digest bytes     : {}", digest.len());
    println!("signature bytes  : {}", signature.len());
    println!("public key bytes : {}", public_key.len());
    println!(
        "derived address  : {}",
        QuantumKeyPair::address_from_public_key(&public_key)
    );
    println!(
        "node verifies    : {}",
        QuantumKeyPair::verify(&public_key, &digest, &signature)
    );
}
