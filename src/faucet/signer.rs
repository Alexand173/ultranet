use crate::{
    faucet::models::SignedTransferRequest,
    quantum_crypto::{PKTrait, SKTrait},
    QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain,
};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sha3::{Digest, Sha3_256};
use std::{fs, path::Path};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const DILITHIUM5_PUBLIC_KEY_BYTES: usize = 2_592;
const DILITHIUM5_SECRET_KEY_BYTES: usize = 4_896;
const DILITHIUM5_SIGNATURE_BYTES: usize = 4_627;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("faucet signer is unavailable")]
    Unavailable,
    #[error("faucet signer credential is invalid")]
    InvalidCredential,
    #[error("faucet signer address does not match configuration")]
    AddressMismatch,
    #[error("faucet signer produced an invalid signature")]
    InvalidSignature,
    #[error("faucet transaction envelope is invalid")]
    InvalidEnvelope,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignerRecord {
    public_key: EncodedBytes,
    #[serde(alias = "private_key")]
    secret_key: EncodedBytes,
}

impl Zeroize for SignerRecord {
    fn zeroize(&mut self) {
        self.public_key.zeroize();
        self.secret_key.zeroize();
    }
}

impl Drop for SignerRecord {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EncodedBytes {
    Hex(String),
    Bytes(Vec<u8>),
}

impl Zeroize for EncodedBytes {
    fn zeroize(&mut self) {
        match self {
            Self::Hex(value) => value.zeroize(),
            Self::Bytes(value) => value.zeroize(),
        }
    }
}

impl EncodedBytes {
    fn into_bytes(self) -> Result<Vec<u8>, SignerError> {
        match self {
            Self::Hex(mut value) => {
                let decoded = hex::decode(value.trim_start_matches("0x").trim_start_matches("0X"))
                    .map_err(|_| SignerError::InvalidCredential);
                value.zeroize();
                decoded
            }
            Self::Bytes(value) => Ok(value),
        }
    }
}

pub struct FaucetSigner {
    address: String,
    public_key: Vec<u8>,
    secret_key: Zeroizing<Vec<u8>>,
    key_id: String,
}

impl FaucetSigner {
    pub fn load(path: &Path, expected_address: &str) -> Result<Self, SignerError> {
        ensure_private_permissions(path)?;
        let raw = Zeroizing::new(fs::read_to_string(path).map_err(|_| SignerError::Unavailable)?);
        let mut record: SignerRecord =
            serde_json::from_str(&raw).map_err(|_| SignerError::InvalidCredential)?;
        let encoded_public_key =
            std::mem::replace(&mut record.public_key, EncodedBytes::Bytes(Vec::new()));
        let encoded_secret_key =
            std::mem::replace(&mut record.secret_key, EncodedBytes::Bytes(Vec::new()));
        let public_key = encoded_public_key.into_bytes()?;
        let secret_key = Zeroizing::new(encoded_secret_key.into_bytes()?);
        if public_key.len() != DILITHIUM5_PUBLIC_KEY_BYTES
            || secret_key.len() != DILITHIUM5_SECRET_KEY_BYTES
        {
            return Err(SignerError::InvalidCredential);
        }
        crate::quantum_crypto::PublicKey::from_bytes(&public_key)
            .map_err(|_| SignerError::InvalidCredential)?;
        crate::quantum_crypto::SecretKey::from_bytes(&secret_key)
            .map_err(|_| SignerError::InvalidCredential)?;
        let address = QuantumKeyPair::address_from_public_key(&public_key);
        if address != expected_address {
            return Err(SignerError::AddressMismatch);
        }
        let key_id = hex::encode(Sha3_256::digest(&public_key));
        let signer = Self {
            address,
            public_key,
            secret_key,
            key_id,
        };
        signer.probe()?;
        Ok(signer)
    }

    pub fn from_keypair_for_tests(keypair: QuantumKeyPair) -> Result<Self, SignerError> {
        let address = keypair.address();
        let public_key = keypair.public_key.clone();
        let secret_key = keypair.secret_key.clone();
        let key_id = hex::encode(Sha3_256::digest(&public_key));
        let signer = Self {
            address,
            public_key,
            secret_key: Zeroizing::new(secret_key),
            key_id,
        };
        signer.probe()?;
        Ok(signer)
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn sign_transfer(
        &self,
        recipient: &str,
        amount_base_units: u64,
        fee_base_units: u64,
        nonce: u64,
        timestamp: u64,
    ) -> Result<(SignedTransferRequest, [u8; 32]), SignerError> {
        if !UltraBlockchain::is_valid_address(recipient)
            || recipient == self.address
            || amount_base_units == 0
            || fee_base_units < UltraBlockchain::minimum_transfer_fee(amount_base_units)
        {
            return Err(SignerError::InvalidEnvelope);
        }
        let mut nullifier = [0u8; 32];
        OsRng.fill_bytes(&mut nullifier);
        let mut transaction = Transaction {
            sender: self.address.clone(),
            sender_public_key: self.public_key.clone(),
            recipient: recipient.to_string(),
            amount: amount_base_units,
            signature: Vec::new(),
            zk_proof: Vec::new(),
            nullifier,
            timestamp,
            fee: fee_base_units,
            nonce,
            gas_limit: 500_000,
            gas_price: 1,
            proof_type: crate::ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::LEGACY_TRANSACTION_VERSION,
        };
        let message = UltraBlockchain::create_transaction_message_for(&transaction);
        let keypair = QuantumKeyPair {
            public_key: self.public_key.clone(),
            secret_key: self.secret_key.to_vec(),
            key_id: [0; 32],
            created_at: 0,
            version: 1,
        };
        let signature = keypair.sign(&message);
        if signature.len() != DILITHIUM5_SIGNATURE_BYTES
            || !QuantumKeyPair::verify(&self.public_key, &message, &signature)
        {
            return Err(SignerError::InvalidSignature);
        }
        transaction.signature = signature;
        let hash = transaction.get_hash();
        Ok((SignedTransferRequest::from_transaction(&transaction), hash))
    }

    fn probe(&self) -> Result<(), SignerError> {
        let keypair = QuantumKeyPair {
            public_key: self.public_key.clone(),
            secret_key: self.secret_key.to_vec(),
            key_id: [0; 32],
            created_at: 0,
            version: 1,
        };
        let message = b"ULTRANET_FAUCET_SIGNER_PROBE_V1";
        let signature = keypair.sign(message);
        if QuantumKeyPair::verify(&self.public_key, message, &signature) {
            Ok(())
        } else {
            Err(SignerError::InvalidSignature)
        }
    }
}

impl Drop for FaucetSigner {
    fn drop(&mut self) {
        self.public_key.zeroize();
        self.secret_key.zeroize();
    }
}

fn ensure_private_permissions(path: &Path) -> Result<(), SignerError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::metadata(path).map_err(|_| SignerError::Unavailable)?;
        let mode = metadata.permissions().mode();
        let owner_only = mode & 0o077 == 0;
        // systemd credentials are projected into a protected runtime directory
        // with a root-owned group-readable mode and a service-user ACL.
        let systemd_projection =
            mode & 0o007 == 0 && mode & 0o020 == 0 && mode & 0o040 != 0 && metadata.gid() == 0;
        if !owner_only && !systemd_projection {
            return Err(SignerError::InvalidCredential);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signer_derives_the_expected_address_and_signs() {
        let keypair = QuantumKeyPair::generate();
        let address = keypair.address();
        let signer = FaucetSigner::from_keypair_for_tests(keypair).unwrap();
        let (envelope, hash) = signer
            .sign_transfer(&"b".repeat(64), 1_000_000, 10_000, 0, 1_785_000_000)
            .unwrap();
        assert_eq!(signer.address(), address);
        assert_eq!(envelope.sender, address);
        assert_eq!(hash.len(), 32);
        assert_eq!(envelope.nullifier.len(), 32);
    }

    #[test]
    fn version_one_signing_vector_matches_the_protocol_contract() {
        let transaction = Transaction {
            sender: "a".repeat(64),
            sender_public_key: vec![],
            recipient: "b".repeat(64),
            amount: 1_000_000,
            signature: vec![],
            zk_proof: vec![],
            nullifier: std::array::from_fn(|index| index as u8),
            timestamp: 1_785_000_000,
            fee: 10_000,
            nonce: 7,
            gas_limit: 500_000,
            gas_price: 1,
            proof_type: crate::ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::LEGACY_TRANSACTION_VERSION,
        };
        let digest = UltraBlockchain::create_transaction_message_for(&transaction);
        assert_eq!(
            hex::encode(digest),
            "792466b39f368af832c411b0286cca4810b63a03a581318133bb8c3a8f7f4461"
        );
    }
}
