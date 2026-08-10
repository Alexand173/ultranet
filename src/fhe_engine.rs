// ============================================================
// FHE ENGINE - HOMOMORPHIC COMPUTATION FOR ULTRANET
// ============================================================

use sled::Tree;
use tfhe::integer::{gen_keys_radix, RadixCiphertext, ServerKey as IntegerServerKey};
use tfhe::shortint::parameters::PARAM_MESSAGE_2_CARRY_2_KS_PBS;
use tfhe::{ClientKey, ConfigBuilder, PublicKey};

pub struct FheEngine {
    pub server_key: IntegerServerKey,
    pub public_key: PublicKey,
}

impl FheEngine {
    /// Inicijalizuje FHE motor. Ako ključevi postoje u Sled-u, učitava ih.
    /// U suprotnom generiše nove (što može potrajati par sekundi).
    pub fn new(db: Tree) -> Self {
        // 1. Provera postojanja ključeva
        if let (Some(sk_bytes), Some(pk_bytes)) = (
            db.get("server_key").ok().flatten(),
            db.get("public_key").ok().flatten(),
        ) {
            println!("💾 FHE: Loading persistent keys from Sled...");
            match (
                bincode::deserialize::<IntegerServerKey>(&sk_bytes),
                bincode::deserialize::<PublicKey>(&pk_bytes),
            ) {
                (Ok(server_key), Ok(public_key)) => {
                    return Self {
                        server_key,
                        public_key,
                    }
                }
                _ => println!("   ⚠️ FHE: Failed to deserialize keys, generating new ones..."),
            }
        }

        // 2. Generisanje ključeva
        println!("🏗️ FHE: Generating keys (Zama TFHE-rs)...");
        let config = ConfigBuilder::default().build();
        let client_key = ClientKey::generate(config);
        let public_key = PublicKey::new(&client_key);

        // Za UltraNet koristimo 4-bitne poruke sa 2-bitnim carry-jem (Radix)
        // Ovo omogućava rad sa 8, 16, 32 ili 64-bitnim brojevima (ovde koristimo 8 blokova za 16-bit)
        let (_radix_ck, integer_server_key) = gen_keys_radix(PARAM_MESSAGE_2_CARRY_2_KS_PBS, 8);

        // 3. Perzistentnost
        let sk_ser = bincode::serialize(&integer_server_key).unwrap();
        let pk_ser = bincode::serialize(&public_key).unwrap();
        let _ = db.insert("server_key", sk_ser);
        let _ = db.insert("public_key", pk_ser);
        let _ = db.flush();

        println!("✅ FHE: Keys generated and saved to Sled.");
        Self {
            server_key: integer_server_key,
            public_key,
        }
    }

    /// Homomorfno sabiranje: ct1 + ct2
    pub fn compute_add(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {}", e))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {}", e))?;

        // Unchecked_add je brži ako znamo da neće biti overflow-a koji carry ne može da hendluje
        let result = self.server_key.unchecked_add(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }

    /// Homomorfno oduzimanje: ct1 - ct2
    pub fn compute_sub(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {}", e))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {}", e))?;

        let result = self.server_key.unchecked_sub(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }

    /// Homomorfno množenje: ct1 * ct2
    pub fn compute_mul(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {}", e))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {}", e))?;

        let result = self.server_key.unchecked_mul(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }
}
