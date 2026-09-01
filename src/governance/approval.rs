use crate::{quantum_crypto::QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::HashSet;

pub const APPROVAL_INTENT_TTL_SECONDS: u64 = 10 * 60;
pub const APPROVAL_MAX_FUTURE_SKEW_SECONDS: u64 = 60;
pub const APPROVAL_MAX_AGE_SECONDS: u64 = 60 * 60;
pub const NULLIFIER_BYTES: usize = 32;
pub const PARTIAL_SIGNATURE_BYTES: usize = UltraBlockchain::SOVEREIGN_SIGNATURE_BYTES;
pub const COMBINED_SIGNATURE_BYTES: usize = PARTIAL_SIGNATURE_BYTES * 2;
const PROPOSAL_RECIPIENT: &str = "0x0";
const APPROVAL_AMOUNT: u64 = 0;
const APPROVAL_FEE: u64 = 0;
const APPROVAL_GAS_LIMIT: u64 = 1_000_000;
const APPROVAL_GAS_PRICE: u64 = 1;
const APPROVAL_DOMAIN: &[u8] = b"UltraNet/approval-signing-envelope/v3";
const APPROVAL_KIND: &[u8] = b"ValidatorApproval";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDraft {
    pub proposal_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub nullifier: Vec<u8>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalPayload {
    #[serde(flatten)]
    pub draft: ApprovalDraft,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedApprovalArtifact {
    #[serde(flatten)]
    pub draft: ApprovalDraft,
    pub owner_address: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalIntentStage {
    Created,
    Signing,
    AwaitingSecondOwner,
    Finalizing,
    Approved,
    Activated,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalIntentRecord {
    pub intent_id: String,
    pub proposal_hash: [u8; 32],
    pub timestamp: u64,
    pub nonce: u64,
    pub nullifier: [u8; NULLIFIER_BYTES],
    pub digest: [u8; 32],
    pub created_by_session_hash: [u8; 32],
    pub expires_at: u64,
    pub stage: ApprovalIntentStage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalSignatureRecord {
    pub intent_id: String,
    pub owner_index: usize,
    pub owner_address: String,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub recorded_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalNonceReservation {
    pub sender: String,
    pub nonce: u64,
    pub intent_id: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalAuditRecord {
    pub event_id: String,
    pub intent_id: String,
    pub proposal_hash: [u8; 32],
    pub event_type: String,
    pub owner_address: Option<String>,
    pub outcome: String,
    pub occurred_at: u64,
}

pub fn parse_proposal_hash(value: &str) -> Result<[u8; 32], String> {
    let normalized = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim());
    let bytes = hex::decode(normalized)
        .map_err(|_| "proposal_hash must contain only hexadecimal characters".to_string())?;
    if bytes.len() != 32 {
        return Err("proposal_hash must be exactly 32 bytes (64 hexadecimal characters)".into());
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

pub fn normalize_proposal_hash(value: &str) -> Result<String, String> {
    Ok(hex::encode(parse_proposal_hash(value)?))
}

pub fn parse_nullifier(value: &[u8]) -> Result<[u8; NULLIFIER_BYTES], String> {
    if value.len() != NULLIFIER_BYTES {
        return Err(format!(
            "nullifier must contain exactly {NULLIFIER_BYTES} bytes; received {}",
            value.len()
        ));
    }
    let mut nullifier = [0u8; NULLIFIER_BYTES];
    nullifier.copy_from_slice(value);
    Ok(nullifier)
}

pub fn validate_approval_timestamp(timestamp: u64, now: u64) -> Result<(), String> {
    if timestamp > now.saturating_add(APPROVAL_MAX_FUTURE_SKEW_SECONDS) {
        return Err("approval timestamp is in the future".into());
    }
    if now.saturating_sub(timestamp) > APPROVAL_MAX_AGE_SECONDS {
        return Err("approval timestamp is too old".into());
    }
    Ok(())
}

pub fn validate_draft(draft: &ApprovalDraft) -> Result<(), String> {
    parse_proposal_hash(&draft.proposal_hash)?;
    parse_nullifier(&draft.nullifier)?;
    if draft.version != UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION {
        return Err(format!(
            "validator approvals require signing-envelope version {}",
            UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION
        ));
    }
    Ok(())
}

/// Build the exact SHA3-256 approval digest accepted by the node's v3
/// ValidatorApproval transaction envelope. This function intentionally accepts
/// only the public draft fields and never handles private key material.
pub fn canonical_approval_message(draft: &ApprovalDraft) -> Result<Vec<u8>, String> {
    validate_draft(draft)?;
    let proposal_hash = parse_proposal_hash(&draft.proposal_hash)?;
    let nullifier = parse_nullifier(&draft.nullifier)?;

    let mut hasher = Sha3_256::new();
    hasher.update(UltraBlockchain::SOVEREIGN_ADDR.as_bytes());
    hasher.update(PROPOSAL_RECIPIENT.as_bytes());
    hasher.update(&APPROVAL_AMOUNT.to_le_bytes());
    hasher.update(&APPROVAL_FEE.to_le_bytes());
    hasher.update(&draft.timestamp.to_le_bytes());
    hasher.update(&nullifier);
    hasher.update(&draft.nonce.to_le_bytes());
    hasher.update(&APPROVAL_GAS_LIMIT.to_le_bytes());
    hasher.update(&APPROVAL_GAS_PRICE.to_le_bytes());
    hasher.update(APPROVAL_DOMAIN);
    hasher.update(&draft.version.to_le_bytes());
    hasher.update(&UltraBlockchain::L1_CHAIN_ID.to_le_bytes());
    hasher.update(APPROVAL_KIND);
    hasher.update(&proposal_hash);
    Ok(hasher.finalize().to_vec())
}

pub fn approval_transaction_for(
    draft: &ApprovalDraft,
    signature: Vec<u8>,
) -> Result<Transaction, String> {
    let proposal_hash = parse_proposal_hash(&draft.proposal_hash)?;
    let nullifier = parse_nullifier(&draft.nullifier)?;
    Ok(Transaction {
        sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        sender_public_key: vec![],
        recipient: PROPOSAL_RECIPIENT.to_string(),
        amount: APPROVAL_AMOUNT,
        signature,
        zk_proof: vec![],
        nullifier,
        timestamp: draft.timestamp,
        fee: APPROVAL_FEE,
        nonce: draft.nonce,
        gas_limit: APPROVAL_GAS_LIMIT,
        gas_price: APPROVAL_GAS_PRICE,
        proof_type: crate::ProofType::Ownership,
        payload: TransactionPayload::ValidatorApproval { proposal_hash },
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: draft.version,
    })
}

pub fn verify_partial_signature(
    public_key: &[u8],
    signature: &[u8],
    draft: &ApprovalDraft,
) -> Result<(), String> {
    if public_key.len() != 2_592 {
        return Err(format!(
            "owner public key must contain exactly 2,592 bytes; received {}",
            public_key.len()
        ));
    }
    if signature.len() != PARTIAL_SIGNATURE_BYTES {
        return Err(format!(
            "owner signature must contain exactly {PARTIAL_SIGNATURE_BYTES} bytes; received {}",
            signature.len()
        ));
    }
    let message = canonical_approval_message(draft)?;
    if !QuantumKeyPair::verify(public_key, &message, signature) {
        return Err("owner signature does not verify against the approval draft".into());
    }
    Ok(())
}

pub fn owner_address(public_key: &[u8]) -> String {
    QuantumKeyPair::address_from_public_key(public_key)
}

pub fn find_authorized_owner_index(
    public_key: &[u8],
    authorized_owners: &[Vec<u8>],
) -> Result<usize, String> {
    let mut unique = HashSet::new();
    for owner in authorized_owners {
        if owner.len() != 2_592 || !unique.insert(owner) {
            return Err(
                "configured Sovereign owner set is invalid or contains duplicate keys".into(),
            );
        }
    }
    authorized_owners
        .iter()
        .position(|owner| owner.as_slice() == public_key)
        .ok_or_else(|| "signer public key is not an authorized Sovereign owner".into())
}

pub fn combine_two_signatures(
    first_index: usize,
    first_signature: &[u8],
    second_index: usize,
    second_signature: &[u8],
) -> Result<Vec<u8>, String> {
    if first_index == second_index {
        return Err("approval signatures must come from different Sovereign owners".into());
    }
    if first_signature.len() != PARTIAL_SIGNATURE_BYTES
        || second_signature.len() != PARTIAL_SIGNATURE_BYTES
    {
        return Err(format!(
            "approval signatures must each contain exactly {PARTIAL_SIGNATURE_BYTES} bytes"
        ));
    }
    let mut combined = Vec::with_capacity(COMBINED_SIGNATURE_BYTES);
    if first_index <= second_index {
        combined.extend_from_slice(first_signature);
        combined.extend_from_slice(second_signature);
    } else {
        combined.extend_from_slice(second_signature);
        combined.extend_from_slice(first_signature);
    }
    Ok(combined)
}

pub fn build_payload(
    draft: ApprovalDraft,
    first: (usize, Vec<u8>),
    second: (usize, Vec<u8>),
) -> Result<ApprovalPayload, String> {
    let signature = combine_two_signatures(first.0, &first.1, second.0, &second.1)?;
    if signature.len() != COMBINED_SIGNATURE_BYTES {
        return Err(format!(
            "combined approval signature must contain exactly {COMBINED_SIGNATURE_BYTES} bytes"
        ));
    }
    Ok(ApprovalPayload { draft, signature })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProofType, Transaction};

    fn draft() -> ApprovalDraft {
        ApprovalDraft {
            proposal_hash: "11".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 7,
            nullifier: vec![0x22; NULLIFIER_BYTES],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        }
    }

    #[test]
    fn digest_matches_node_transaction_message() {
        let draft = draft();
        let transaction = approval_transaction_for(&draft, vec![]).unwrap();
        let expected = UltraBlockchain::create_transaction_message_for(&transaction);
        assert_eq!(canonical_approval_message(&draft).unwrap(), expected);
    }

    #[test]
    fn combines_two_distinct_signatures_in_owner_order() {
        let draft = draft();
        let message = canonical_approval_message(&draft).unwrap();
        let first = QuantumKeyPair::generate();
        let second = QuantumKeyPair::generate();
        let first_signature = first.sign(&message);
        let second_signature = second.sign(&message);
        let combined = combine_two_signatures(1, &first_signature, 0, &second_signature).unwrap();
        assert_eq!(combined.len(), COMBINED_SIGNATURE_BYTES);
        assert_eq!(
            &combined[..PARTIAL_SIGNATURE_BYTES],
            second_signature.as_slice()
        );
        assert_eq!(
            &combined[PARTIAL_SIGNATURE_BYTES..],
            first_signature.as_slice()
        );
    }

    #[test]
    fn rejects_bad_lengths_and_stale_time() {
        let mut invalid = draft();
        invalid.nullifier.pop();
        assert!(validate_draft(&invalid).is_err());
        assert!(validate_approval_timestamp(10, 10 + APPROVAL_MAX_AGE_SECONDS + 1).is_err());
        assert!(
            validate_approval_timestamp(10 + APPROVAL_MAX_FUTURE_SKEW_SECONDS + 1, 10).is_err()
        );
    }

    #[test]
    fn transaction_shape_remains_version_three_approval() {
        let transaction: Transaction = approval_transaction_for(&draft(), vec![]).unwrap();
        assert_eq!(transaction.sender, UltraBlockchain::SOVEREIGN_ADDR);
        assert_eq!(
            transaction.version,
            UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION
        );
        assert_eq!(transaction.chain_id, UltraBlockchain::L1_CHAIN_ID);
        assert!(matches!(transaction.proof_type, ProofType::Ownership));
        assert!(matches!(
            transaction.payload,
            TransactionPayload::ValidatorApproval { .. }
        ));
    }
}
