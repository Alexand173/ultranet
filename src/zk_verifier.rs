// ============================================================
// ZK VERIFIER - INTEGRACIJA SA SNARKJS
// ============================================================

use std::fs;
use std::process::Command;

pub struct ZKVerifier {
    pub verification_key: String,
    pub circuit_name: String,
}

impl ZKVerifier {
    pub fn new(verification_key_path: &str, circuit_name: &str) -> Self {
        Self {
            verification_key: verification_key_path.to_string(),
            circuit_name: circuit_name.to_string(),
        }
    }

    pub fn verify_proof(&self, proof_path: &str, public_path: &str) -> bool {
        // 1. Proveri da li fajlovi postoje
        if !fs::metadata(proof_path).is_ok() || !fs::metadata(public_path).is_ok() {
            eprintln!("❌ Proof or public file not found!");
            return false;
        }

        // 2. Pokreni snarkjs verifikaciju
        let output = Command::new("npx")
            .arg("snarkjs")
            .arg("groth16")
            .arg("verify")
            .arg(&self.verification_key)
            .arg(proof_path)
            .arg(public_path)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if stdout.contains("OK!") {
                    println!("✅ ZK Proof verified successfully!");
                    true
                } else {
                    eprintln!("❌ ZK Proof verification failed!");
                    eprintln!("   stdout: {}", stdout);
                    eprintln!("   stderr: {}", stderr);
                    false
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to run snarkjs: {}", e);
                false
            }
        }
    }

    pub fn verify_proof_data(&self, proof_json: &[u8], public_inputs: &[u8]) -> bool {
        // ✅ PROVERI DA LI JE PROOF VALIDAN JSON
        let proof_str = String::from_utf8_lossy(proof_json);

        // ✅ POKUŠAJ DA PARSIRAŠ JSON
        let proof_json_value: Result<serde_json::Value, _> = serde_json::from_str(&proof_str);
        if let Err(e) = proof_json_value {
            eprintln!("❌ Proof is not valid JSON: {}", e);
            // Koristimo char_indices da izbegnemo panic pri sečenju stringa na
            // nevalidnoj UTF-8 granici (proof_json može biti binarni sadržaj).
            let preview_end = proof_str
                .char_indices()
                .map(|(i, c)| i + c.len_utf8())
                .take_while(|&i| i <= 100)
                .last()
                .unwrap_or(0);
            eprintln!("   First 100 chars: {}", &proof_str[..preview_end]);
            return false;
        }
        println!("✅ Proof is valid JSON!");

        // ✅ SAČUVAJ U FAJL
        let proof_path = "/tmp/proof.json";
        let public_path = "/tmp/public.json";

        if let Err(e) = fs::write(proof_path, proof_json) {
            eprintln!("❌ Failed to write proof: {}", e);
            return false;
        }

        if let Err(e) = fs::write(public_path, public_inputs) {
            eprintln!("❌ Failed to write public inputs: {}", e);
            return false;
        }

        // ✅ POKRENI SNARKJS VERIFIKACIJU
        let output = Command::new("npx")
            .arg("snarkjs")
            .arg("groth16")
            .arg("verify")
            .arg(&self.verification_key)
            .arg(proof_path)
            .arg(public_path)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("OK!") {
                    println!("✅ ZK proof verified successfully!");
                    return true;
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("❌ ZK proof verification failed!");
                    eprintln!("   stdout: {}", stdout);
                    eprintln!("   stderr: {}", stderr);
                    return false;
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to run snarkjs: {}", e);
                false
            }
        }
    }
}
