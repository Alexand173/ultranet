// ============================================================
// FHE ENGINE - HOMOMORPHIC COMPUTATION FOR ULTRANET
// ============================================================

use rand::{rngs::OsRng, RngCore};
use sled::Tree;
use tfhe::core_crypto::commons::math::random::Seed;
use tfhe::core_crypto::fft_impl::fft64::crypto::bootstrap::FourierLweBootstrapKey;
use tfhe::core_crypto::prelude::{
    allocate_and_generate_new_lwe_keyswitch_key, par_allocate_and_generate_new_lwe_bootstrap_key,
    par_allocate_and_generate_new_lwe_public_key,
    par_convert_standard_lwe_bootstrap_key_to_fourier, ActivatedRandomGenerator,
    EncryptionRandomGenerator, LwePublicKeyZeroEncryptionCount, Seeder,
};
use tfhe::integer::{RadixCiphertext, ServerKey as IntegerServerKey};
use tfhe::shortint::server_key::ShortintBootstrappingKey;
use tfhe::shortint::{
    ciphertext::MaxDegree, ClientKey as ShortintClientKey, PBSOrder, PBSParameters,
};
use tfhe::{ClientKey, ConfigBuilder, PublicKey};

pub struct FheEngine {
    pub server_key: IntegerServerKey,
    pub public_key: PublicKey,
}

/// TFHE-rs 0.7 only exposes RDSEED and Unix seeders. Windows machines without
/// RDSEED therefore need an application-provided seed from the operating
/// system CSPRNG. The seed is used only to initialize TFHE's deterministic
/// generators; it is never persisted or logged.
struct OsEntropySeeder;

impl Seeder for OsEntropySeeder {
    fn seed(&mut self) -> Seed {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        Seed(u128::from_le_bytes(bytes))
    }

    fn is_available() -> bool {
        true
    }
}

fn secure_seed() -> Seed {
    let mut seeder = OsEntropySeeder;
    seeder.seed()
}

/// Build the FHE public/server keys without touching TFHE's thread-local
/// `ShortintEngine::new`, which calls the RDSEED-only seeder on Windows.
/// Both keys are derived from the same client key so public-key encryption and
/// server-side radix evaluation use one consistent key set.
fn generate_keys() -> (PublicKey, IntegerServerKey) {
    let client_key = ClientKey::generate_with_seed(ConfigBuilder::default().build(), secure_seed());
    let integer_client_key = client_key.as_ref();
    let shortint_client_key: &ShortintClientKey = integer_client_key.as_ref();
    let mut root_seeder = OsEntropySeeder;
    let public_key = generate_public_key(shortint_client_key, &mut root_seeder);
    let (glwe_secret_key, lwe_secret_key, parameters) =
        shortint_client_key.clone().into_raw_parts();
    let integer_server_key = generate_integer_server_key(
        glwe_secret_key,
        lwe_secret_key,
        parameters,
        &mut root_seeder,
    );

    (public_key, integer_server_key)
}

fn generate_public_key(client_key: &ShortintClientKey, root_seeder: &mut impl Seeder) -> PublicKey {
    let (secret_encryption_key, encryption_noise_distribution) =
        client_key.encryption_key_and_noise();
    let zero_encryption_count = LwePublicKeyZeroEncryptionCount(
        secret_encryption_key.lwe_dimension().to_lwe_size().0 * 64 + 128,
    );
    let mut encryption_generator =
        EncryptionRandomGenerator::<ActivatedRandomGenerator>::new(root_seeder.seed(), root_seeder);
    let lwe_public_key = par_allocate_and_generate_new_lwe_public_key(
        &secret_encryption_key,
        zero_encryption_count,
        encryption_noise_distribution,
        client_key.parameters.ciphertext_modulus(),
        &mut encryption_generator,
    );
    let shortint_public_key = tfhe::shortint::PublicKey::from_raw_parts(
        lwe_public_key,
        client_key.parameters,
        client_key.parameters.encryption_key_choice().into(),
    );
    let integer_public_key = tfhe::integer::PublicKey::from_raw_parts(shortint_public_key);
    PublicKey::from_raw_parts(integer_public_key)
}

fn generate_integer_server_key(
    glwe_secret_key: tfhe::core_crypto::entities::GlweSecretKeyOwned<u64>,
    lwe_secret_key: tfhe::core_crypto::entities::LweSecretKeyOwned<u64>,
    parameters: tfhe::shortint::ShortintParameterSet,
    root_seeder: &mut impl Seeder,
) -> IntegerServerKey {
    let PBSParameters::PBS(pbs_parameters) = parameters
        .pbs_parameters()
        .expect("TFHE parameters must contain classic PBS parameters")
    else {
        panic!("TFHE multi-bit PBS parameters are not supported by the Windows fallback")
    };

    let mut encryption_generator =
        EncryptionRandomGenerator::<ActivatedRandomGenerator>::new(root_seeder.seed(), root_seeder);
    let input_lwe_secret_key = lwe_secret_key.as_view();
    let output_glwe_secret_key = &glwe_secret_key;
    let bootstrap_key = par_allocate_and_generate_new_lwe_bootstrap_key(
        &input_lwe_secret_key,
        output_glwe_secret_key,
        pbs_parameters.pbs_base_log,
        pbs_parameters.pbs_level,
        pbs_parameters.glwe_noise_distribution,
        pbs_parameters.ciphertext_modulus,
        &mut encryption_generator,
    );
    let mut fourier_bootstrap_key = FourierLweBootstrapKey::new(
        bootstrap_key.input_lwe_dimension(),
        bootstrap_key.glwe_size(),
        bootstrap_key.polynomial_size(),
        bootstrap_key.decomposition_base_log(),
        bootstrap_key.decomposition_level_count(),
    );
    par_convert_standard_lwe_bootstrap_key_to_fourier(&bootstrap_key, &mut fourier_bootstrap_key);

    let output_lwe_secret_key = output_glwe_secret_key.as_lwe_secret_key();
    let key_switching_key = allocate_and_generate_new_lwe_keyswitch_key(
        &output_lwe_secret_key,
        &input_lwe_secret_key,
        parameters.ks_base_log(),
        parameters.ks_level(),
        parameters.lwe_noise_distribution(),
        parameters.ciphertext_modulus(),
        &mut encryption_generator,
    );
    let max_degree =
        MaxDegree::new(parameters.carry_modulus().0 * (parameters.message_modulus().0 - 1));
    let shortint_server_key = tfhe::shortint::ServerKey::from_raw_parts(
        key_switching_key,
        ShortintBootstrappingKey::Classic(fourier_bootstrap_key),
        parameters.message_modulus(),
        parameters.carry_modulus(),
        max_degree,
        parameters.max_noise_level(),
        parameters.ciphertext_modulus(),
        PBSOrder::from(parameters.encryption_key_choice()),
    );

    IntegerServerKey::new_radix_server_key_from_shortint(shortint_server_key)
}

impl FheEngine {
    /// Initializes the FHE engine. Existing keys are loaded from Sled;
    /// otherwise new keys are generated using OS-backed entropy.
    pub fn new(db: Tree) -> Self {
        // 1. Check for persisted keys
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

        // 2. Generate a single consistent client/public/server key set
        println!("🏗️ FHE: Generating keys (Zama TFHE-rs)...");
        let (public_key, integer_server_key) = generate_keys();

        // 3. Persist the public/server keys
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

    /// Homomorphic addition: ct1 + ct2
    pub fn compute_add(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {e}"))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {e}"))?;

        // Unchecked_add is faster when the configured carry can handle overflow.
        let result = self.server_key.unchecked_add(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }

    /// Homomorphic subtraction: ct1 - ct2
    pub fn compute_sub(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {e}"))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {e}"))?;

        let result = self.server_key.unchecked_sub(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }

    /// Homomorphic multiplication: ct1 * ct2
    pub fn compute_mul(&self, ct1_bytes: &[u8], ct2_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let ct1: RadixCiphertext =
            bincode::deserialize(ct1_bytes).map_err(|e| format!("CT1 Error: {e}"))?;
        let ct2: RadixCiphertext =
            bincode::deserialize(ct2_bytes).map_err(|e| format!("CT2 Error: {e}"))?;

        let result = self.server_key.unchecked_mul(&ct1, &ct2);
        Ok(bincode::serialize(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_entropy_seeder_produces_distinct_seeds() {
        assert!(OsEntropySeeder::is_available());
        assert_ne!(secure_seed(), secure_seed());
    }
}
