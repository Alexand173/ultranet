//! Proof that the person registering a game controls the address they name as its owner.
//!
//! The owner signs a short message with the wallet key (the same post-quantum key that signs
//! transfers) and sends the public key beside it. The address is the hash of that public key, so a
//! valid signature proves control without any password, login or secret leaving the wallet.
//!
//! ```text
//! digest = SHA3-256("UltraGame/owner-proof/v1"
//!                   || u32le(len) || action   (ASCII: "register" or "rotate-key")
//!                   || u32le(len) || game id
//!                   || u32le(len) || name     (the trimmed name; empty for rotate-key)
//!                   || u64le(timestamp))      (seconds since the Unix epoch)
//! signature = Dilithium5 detached signature over the 32 digest bytes
//! owner     = hex(SHA3-256(public key))
//! ```
//!
//! The action, the id and the name are all inside the digest, so a signature for one purpose cannot be
//! replayed for another. The timestamp must be within five minutes of the server's clock; for key
//! rotation it must also be later than the game's last change, so a captured rotation cannot be played
//! again once it has been used.

use super::registry::RegistryError;
use crate::QuantumKeyPair;
use sha3::{Digest, Sha3_256};

pub const PROOF_WINDOW_SECONDS: u64 = 300;
/// A hex public key or signature longer than this is refused before any decoding.
const MAX_HEX_CHARS: usize = 16_384;

pub const ACTION_REGISTER: &str = "register";
pub const ACTION_ROTATE_KEY: &str = "rotate-key";

pub fn proof_digest(action: &str, game_id: &str, name: &str, timestamp: u64) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraGame/owner-proof/v1");
    for part in [action, game_id, name] {
        hasher.update((part.len() as u32).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(timestamp.to_le_bytes());
    hasher.finalize().into()
}

fn decode(label: &str, value: &str) -> Result<Vec<u8>, RegistryError> {
    if value.is_empty() || value.len() > MAX_HEX_CHARS {
        return Err(RegistryError::Invalid(format!("{label} is not valid")));
    }
    hex::decode(value).map_err(|_| RegistryError::Invalid(format!("{label} must be hex")))
}

/// Check an owner's proof and return the address it proves. Any failure is the same `BadKey`: the
/// caller learns that the proof did not hold, not which part of it was wrong.
pub fn verify_owner_proof(
    action: &str,
    game_id: &str,
    name: &str,
    timestamp: u64,
    public_key_hex: &str,
    signature_hex: &str,
    now: u64,
) -> Result<String, RegistryError> {
    let public_key = decode("public_key", public_key_hex)?;
    let signature = decode("signature", signature_hex)?;
    if timestamp.abs_diff(now) > PROOF_WINDOW_SECONDS {
        return Err(RegistryError::BadKey);
    }
    let digest = proof_digest(action, game_id, name, timestamp);
    if !QuantumKeyPair::verify(&public_key, &digest, &signature) {
        return Err(RegistryError::BadKey);
    }
    Ok(QuantumKeyPair::address_from_public_key(&public_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn proof(
        pair: &QuantumKeyPair,
        action: &str,
        id: &str,
        name: &str,
        ts: u64,
    ) -> (String, String) {
        let signature = pair.sign(&proof_digest(action, id, name, ts));
        (hex::encode(&pair.public_key), hex::encode(signature))
    }

    #[test]
    fn a_valid_proof_returns_the_address_the_key_hashes_to() {
        let pair = QuantumKeyPair::generate();
        let (pk, sig) = proof(&pair, ACTION_REGISTER, "tetris", "Tetris", NOW);
        let owner = verify_owner_proof(ACTION_REGISTER, "tetris", "Tetris", NOW, &pk, &sig, NOW)
            .unwrap();
        assert_eq!(owner, pair.address());
    }

    #[test]
    fn a_proof_cannot_be_reused_for_another_action_id_name_or_time() {
        let pair = QuantumKeyPair::generate();
        let (pk, sig) = proof(&pair, ACTION_REGISTER, "tetris", "Tetris", NOW);
        let check = |action: &str, id: &str, name: &str, ts: u64| {
            verify_owner_proof(action, id, name, ts, &pk, &sig, NOW)
        };
        assert_eq!(check(ACTION_ROTATE_KEY, "tetris", "Tetris", NOW), Err(RegistryError::BadKey));
        assert_eq!(check(ACTION_REGISTER, "snake", "Tetris", NOW), Err(RegistryError::BadKey));
        assert_eq!(check(ACTION_REGISTER, "tetris", "Tetris2", NOW), Err(RegistryError::BadKey));
        assert_eq!(check(ACTION_REGISTER, "tetris", "Tetris", NOW + 1), Err(RegistryError::BadKey));
    }

    #[test]
    fn another_keys_signature_proves_nothing_about_this_key() {
        let pair = QuantumKeyPair::generate();
        let other = QuantumKeyPair::generate();
        let (_, sig) = proof(&other, ACTION_REGISTER, "tetris", "Tetris", NOW);
        let result = verify_owner_proof(
            ACTION_REGISTER,
            "tetris",
            "Tetris",
            NOW,
            &hex::encode(&pair.public_key),
            &sig,
            NOW,
        );
        assert_eq!(result, Err(RegistryError::BadKey));
    }

    #[test]
    fn a_stale_or_future_timestamp_is_refused_but_the_window_edge_is_not() {
        let pair = QuantumKeyPair::generate();
        for (ts, ok) in [
            (NOW - PROOF_WINDOW_SECONDS, true),
            (NOW + PROOF_WINDOW_SECONDS, true),
            (NOW - PROOF_WINDOW_SECONDS - 1, false),
            (NOW + PROOF_WINDOW_SECONDS + 1, false),
        ] {
            let (pk, sig) = proof(&pair, ACTION_REGISTER, "tetris", "Tetris", ts);
            let result =
                verify_owner_proof(ACTION_REGISTER, "tetris", "Tetris", ts, &pk, &sig, NOW);
            assert_eq!(result.is_ok(), ok, "timestamp offset {}", ts as i64 - NOW as i64);
        }
    }

    #[test]
    fn malformed_or_oversized_input_is_refused_without_panicking() {
        let pair = QuantumKeyPair::generate();
        let (pk, sig) = proof(&pair, ACTION_REGISTER, "tetris", "Tetris", NOW);
        let run = |pk: &str, sig: &str| {
            verify_owner_proof(ACTION_REGISTER, "tetris", "Tetris", NOW, pk, sig, NOW)
        };
        assert!(run("", &sig).is_err());
        assert!(run(&pk, "").is_err());
        assert!(run("zz", &sig).is_err());
        assert!(run(&pk, "zz").is_err());
        assert!(run(&"a".repeat(MAX_HEX_CHARS + 2), &sig).is_err());
        assert!(run(&pk, &sig[..sig.len() - 2]).is_err());
        assert!(run("00", "00").is_err());
    }

    #[test]
    fn the_digest_is_pinned() {
        // Reproduced independently in Python from the layout in the module documentation.
        let digest = proof_digest(ACTION_REGISTER, "tetris", "Tetris", 1_800_000_000);
        println!("OWNER_PROOF_DIGEST={}", hex::encode(digest));
        assert_eq!(hex::encode(digest), PINNED_DIGEST);
    }

    const PINNED_DIGEST: &str = "47209efea37a0f39e70653b9551dbdf1fc1ba0d9e28a3d7ffb4c4571a51b389b";
}
