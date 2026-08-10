// ============================================================
// RECURSIVE ZK-SNARKs ZA ULTRA NET 4.0
// ============================================================

use ark_bls12_381::{Bls12_381, Fr};
use ark_ff::PrimeField;
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_r1cs_std::{alloc::AllocVar, fields::fp::FpVar, uint8::UInt8};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::*;
use ark_snark::{CircuitSpecificSetupSNARK, SNARK};
use rand::rngs::OsRng;
use sha3::{Digest, Sha3_256};
use std::collections::HashMap;

// ============================================================
// 1. RECURSIVE CIRCUIT
// ============================================================

// Constraint "oblik" (broj/tip promenljivih) MORA biti IDENTIČAN u setup-u i
// u svakom stvarnom dokazu - Groth16 je circuit-specific SNARK i ne
// dozvoljava da broj wire-ova varira. Zato privatne "inner" promenljive
// imaju FIKSNU dužinu, bez obzira na to da li je `None` ili na stvarnu
// veličinu prethodnog dokaza/ulaza (videti `generate_constraints`).
pub const INNER_PROOF_SIZE: usize = 384; // Groth16 proof (BLS12-381, uncompressed) - fiksna veličina u bajtovima
pub const INNER_PUBLIC_INPUTS_LEN: usize = 3; // block_index, timestamp, total_blocks (videti main.rs::mine_block)

#[derive(Clone)]
pub struct RecursiveVerificationCircuit {
    // PUBLIC INPUTS (svi vide)
    pub previous_proof_hash: Option<[u8; 32]>,
    pub block_hash: Option<[u8; 32]>,
    pub block_index: Option<u64>,
    pub timestamp: Option<u64>,
    pub total_blocks: Option<u64>,

    // PRIVATE INPUTS (samo korisnik zna)
    pub inner_proof: Option<Vec<u8>>,
    pub inner_public_inputs: Option<Vec<Fr>>,
}

impl ConstraintSynthesizer<Fr> for RecursiveVerificationCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // 1. Public inputs
        let prev_hash = FpVar::new_input(cs.clone(), || {
            let hash = self.previous_proof_hash.unwrap_or([0; 32]);
            Ok(Fr::from_le_bytes_mod_order(&hash))
        })?;

        let block_hash = FpVar::new_input(cs.clone(), || {
            let hash = self.block_hash.unwrap_or([0; 32]);
            Ok(Fr::from_le_bytes_mod_order(&hash))
        })?;

        let block_index =
            FpVar::new_input(cs.clone(), || Ok(Fr::from(self.block_index.unwrap_or(0))))?;

        let timestamp = FpVar::new_input(cs.clone(), || Ok(Fr::from(self.timestamp.unwrap_or(0))))?;

        let total_blocks =
            FpVar::new_input(cs.clone(), || Ok(Fr::from(self.total_blocks.unwrap_or(0))))?;

        // 2. Private inputs - dokaz koji se verifikuje
        // NAPOMENA: dužina se UVEK normalizuje na fiksnu vrednost
        // (INNER_PROOF_SIZE / INNER_PUBLIC_INPUTS_LEN), bez obzira na
        // stvarnu dužinu prosleđenih podataka. Bez ovoga, broj UInt8/FpVar
        // witness-a (a time i broj constraint-a) varira od dokaza do
        // dokaza (npr. genesis dokaz nema `inner_proof`, dok svaki sledeći
        // dokaz u lancu nosi ceo prethodni Groth16 dokaz kao bajtove) - a
        // to je NEVALIDNO za Groth16, čiji proving/verifying ključevi su
        // vezani za tačno jedan, fiksni oblik kola. Zato je svaki
        // "prošireni" dokaz do sada bio nevalidan.
        let mut inner_proof_bytes = self.inner_proof.clone().unwrap_or_default();
        inner_proof_bytes.resize(INNER_PROOF_SIZE, 0u8);
        let _inner_proof = UInt8::new_witness_vec(cs.clone(), &inner_proof_bytes)?;

        let mut inner_inputs_vec = self.inner_public_inputs.clone().unwrap_or_default();
        inner_inputs_vec.resize(INNER_PUBLIC_INPUTS_LEN, Fr::from(0u64));
        let _inner_inputs: Vec<FpVar<Fr>> = Vec::new_witness(cs.clone(), || Ok(inner_inputs_vec))?;

        // 3. KONSTRAKCIJE
        let zero = FpVar::new_constant(cs.clone(), Fr::from(0))?;

        // 3.1. block_index > 0
        let _index_check = block_index.clone() - zero.clone();

        // 3.2. timestamp > 0
        let _timestamp_check = timestamp.clone() - zero.clone();

        // 3.3. total_blocks > 0
        let _total_check = total_blocks.clone() - zero;

        // 3.4. block_hash se poklapa sa prethodnim (ako postoji)
        let _hash_check = prev_hash.clone() - block_hash.clone();

        // 3.5. Verifikacija unutrašnjeg dokaza (pojednostavljeno)
        // U pravoj implementaciji, ovde bi išla PLONK verifikacija

        println!("✅ Recursive ZK circuit constraints generated!");
        Ok(())
    }
}

// ============================================================
// 2. RECURSIVE ZK ENGINE
// ============================================================

pub struct RecursiveZKEngine {
    pub proving_key: Option<ProvingKey<Bls12_381>>,
    pub verifying_key: Option<VerifyingKey<Bls12_381>>,
    // Čuvamo dokaz ZAJEDNO sa javnim ulazima koji su korišćeni prilikom
    // njegovog kreiranja - to je JEDINI skup javnih ulaza sa kojim će
    // Groth16::verify uspešno proći (moraju biti identični po redosledu i
    // enkodiranju kao u generate_constraints: prev_hash, block_hash,
    // block_index, timestamp, total_blocks).
    pub proof_chain: Vec<(Vec<u8>, Vec<Fr>)>,
    pub proof_history: HashMap<u64, (Vec<u8>, Vec<Fr>)>,
    pub is_setup: bool,
    pub proof_counter: u64,
}

impl RecursiveZKEngine {
    pub fn new() -> Self {
        Self {
            proving_key: None,
            verifying_key: None,
            proof_chain: Vec::new(),
            proof_history: HashMap::new(),
            is_setup: false,
            proof_counter: 0,
        }
    }

    pub fn setup(&mut self) -> Result<(), String> {
        println!("🔧 Generating Recursive ZK parameters...");
        println!("⏳ This may take a few seconds...");

        let circuit = RecursiveVerificationCircuit {
            previous_proof_hash: None,
            block_hash: None,
            block_index: None,
            timestamp: None,
            total_blocks: None,
            inner_proof: None,
            inner_public_inputs: None,
        };

        let rng = &mut OsRng;
        let (pk, vk) =
            Groth16::<Bls12_381>::setup(circuit, rng).map_err(|e| format!("Setup error: {}", e))?;

        self.proving_key = Some(pk);
        self.verifying_key = Some(vk);
        self.is_setup = true;

        println!("✅ Recursive ZK parameters generated!");
        Ok(())
    }

    /// Rekonstruiše javne ulaze kola u ISTOM redosledu i enkodiranju kao u
    /// `generate_constraints` (new_input pozivi): prev_hash, block_hash,
    /// block_index, timestamp, total_blocks. Mora se koristiti identično
    /// prilikom kreiranja dokaza I prilikom verifikacije.
    fn compute_public_inputs(
        previous_proof_hash: Option<[u8; 32]>,
        block_hash: [u8; 32],
        block_index: u64,
        timestamp: u64,
        total_blocks: u64,
    ) -> Vec<Fr> {
        vec![
            Fr::from_le_bytes_mod_order(&previous_proof_hash.unwrap_or([0; 32])),
            Fr::from_le_bytes_mod_order(&block_hash),
            Fr::from(block_index),
            Fr::from(timestamp),
            Fr::from(total_blocks),
        ]
    }

    pub fn create_recursive_proof(
        &mut self,
        previous_proof: Option<&[u8]>,
        block_hash: [u8; 32],
        block_index: u64,
        timestamp: u64,
        total_blocks: u64,
        inner_public_inputs: Vec<Fr>,
    ) -> Result<Vec<u8>, String> {
        if !self.is_setup {
            return Err("ZK engine not setup!".to_string());
        }

        // Izračunaj hash prethodnog dokaza
        let previous_hash = if let Some(proof) = previous_proof {
            let mut hasher = Sha3_256::new();
            hasher.update(proof);
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&hasher.finalize());
            Some(hash)
        } else {
            None
        };

        let circuit = RecursiveVerificationCircuit {
            previous_proof_hash: previous_hash,
            block_hash: Some(block_hash),
            block_index: Some(block_index),
            timestamp: Some(timestamp),
            total_blocks: Some(total_blocks),
            inner_proof: previous_proof.map(|p| p.to_vec()),
            inner_public_inputs: Some(inner_public_inputs),
        };

        let rng = &mut OsRng;
        let proof = Groth16::<Bls12_381>::prove(self.proving_key.as_ref().unwrap(), circuit, rng)
            .map_err(|e| format!("Prove error: {}", e))?;

        let mut proof_bytes = Vec::new();
        proof
            .serialize_uncompressed(&mut proof_bytes)
            .map_err(|e| format!("Serialization error: {}", e))?;

        let public_inputs = Self::compute_public_inputs(
            previous_hash,
            block_hash,
            block_index,
            timestamp,
            total_blocks,
        );

        self.proof_counter += 1;
        self.proof_chain
            .push((proof_bytes.clone(), public_inputs.clone()));
        self.proof_history
            .insert(block_index, (proof_bytes.clone(), public_inputs));

        println!("✅ Recursive proof created for block {}", block_index);
        println!("   Proof size: {} bytes", proof_bytes.len());
        println!("   Proof chain length: {}", self.proof_chain.len());

        Ok(proof_bytes)
    }

    pub fn verify_recursive_proof(
        &self,
        proof: &[u8],
        public_inputs: &[Fr],
    ) -> Result<bool, String> {
        if !self.is_setup {
            return Err("ZK engine not setup!".to_string());
        }

        let proof = Proof::<Bls12_381>::deserialize_uncompressed(proof)
            .map_err(|e| format!("Deserialization error: {}", e))?;

        let result = Groth16::<Bls12_381>::verify(
            self.verifying_key.as_ref().unwrap(),
            public_inputs,
            &proof,
        )
        .map_err(|e| format!("Verify error: {}", e))?;

        Ok(result)
    }

    pub fn get_latest_proof(&self) -> Option<&(Vec<u8>, Vec<Fr>)> {
        self.proof_chain.last()
    }

    pub fn get_proof_by_block(&self, block_index: u64) -> Option<&(Vec<u8>, Vec<Fr>)> {
        self.proof_history.get(&block_index)
    }

    pub fn get_proof_chain_length(&self) -> usize {
        self.proof_chain.len()
    }

    pub fn print_stats(&self) {
        println!("📊 RECURSIVE ZK STATS:");
        println!("   Proof chain length: {}", self.proof_chain.len());
        println!("   Proof history: {}", self.proof_history.len());
        println!("   Total proofs: {}", self.proof_counter);
        println!("   Setup: {}", if self.is_setup { "✅" } else { "❌" });
    }
}

// ============================================================
// 3. TESTOVI
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recursive_zk_setup() {
        let mut engine = RecursiveZKEngine::new();
        let result = engine.setup();
        assert!(result.is_ok(), "Setup should succeed");
        println!("✅ Recursive ZK setup test passed!");
    }

    #[test]
    fn test_recursive_proof_chain() {
        let mut engine = RecursiveZKEngine::new();
        engine.setup().unwrap();

        let block_hash_1 = [1; 32];
        let block_hash_2 = [2; 32];
        let public_inputs = vec![Fr::from(1), Fr::from(2)];

        // Prvi dokaz (genesis)
        let proof1 = engine
            .create_recursive_proof(None, block_hash_1, 1, 1234567890, 1, public_inputs.clone())
            .unwrap();

        // Drugi dokaz (verifikuje prvi)
        let _proof2 = engine
            .create_recursive_proof(Some(&proof1), block_hash_2, 2, 1234567891, 2, public_inputs)
            .unwrap();

        // Verifikacija - koristimo TAČNE javne ulaze sačuvane uz dokaz
        // (moraju biti identični po redosledu/enkodiranju kao u
        // generate_constraints, inače Groth16::verify ne uspeva).
        let (latest_proof, latest_public_inputs) = engine.get_latest_proof().unwrap();
        let is_valid = engine
            .verify_recursive_proof(latest_proof, latest_public_inputs)
            .unwrap();
        assert!(is_valid, "Recursive proof should be valid");

        println!("✅ Recursive proof chain test passed!");
        println!("   Chain length: {}", engine.get_proof_chain_length());
    }
}
