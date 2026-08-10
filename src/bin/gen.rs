// src/bin/gen.rs - Wallet Generator
use rand::Rng;
use sha3::{Digest, Sha3_256};

fn main() {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);

    let public_key = Sha3_256::digest(&private_key);
    let address = Sha3_256::digest(&public_key);

    println!("==================================================");
    println!("🔐 ULTRANET WALLET GENERATOR");
    println!("==================================================");
    println!("\n📝 ADRESA: 0x{}", hex::encode(address));
    println!("\n🔑 PRIVATNI KLJUČ (hex): {}", hex::encode(private_key));
    println!("🔑 PRIVATNI KLJUČ (dec): {:?}", private_key);
    println!("\n==================================================");
    println!("⚠️  ČUVAJ PRIVATNI KLJUČ NA SIGURNOM MESTU!");
    println!("==================================================");
}
