// scripts/gen_multisig_keys.rs
use UltraNet::QuantumKeyPair;

fn main() {
    println!("🛡️ Generating 3 Sovereign Owner Keys for 2-of-3 Multi-Sig...");
    for i in 1..=3 {
        let key = QuantumKeyPair::new();
        println!("\n🔑 OWNER KEY #{}", i);
        println!("   ADDRESS: {}", key.address);
        println!("   PUBLIC:  {}", hex::encode(&key.public_key));
        println!("   PRIVATE: {}", hex::encode(&key.private_key));
    }
}
