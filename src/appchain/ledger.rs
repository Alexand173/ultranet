// ============================================================
// L1 APPCHAIN LEDGER — APPCHAINS AS L1 OBJECTS
// ============================================================
//
// An AppChain has no validators of its own. It is an object on L1:
//
// * `AppChainRegister { name }` — signed by the future owner; the node assigns
//   the next chain ID and the signer becomes the owner.
// * `AppChainAnchor { chain_id, state_root }` — signed by the owner; records the
//   owner's 32-byte state root at the next anchor sequence number.
//
// L1 validators only check the signature, envelope and fee, and include the
// transaction in a block. The node never computes or judges an AppChain's
// state; it records what the owner signed.
//
// Block production, block validation and state application all derive their
// effects from `plan_block`, so the three paths cannot drift apart. Ledger
// writes are keyed by transaction hash, which makes replaying a block (restore,
// rollback) idempotent: an already-applied transaction reproduces the same ID
// and sequence instead of being assigned new ones.

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::HashMap;

use crate::{Transaction, TransactionPayload};

/// Signing-envelope version reserved for AppChain ledger transactions.
pub const APPCHAIN_TX_VERSION: u32 = 5;
/// Fixed recipient of every AppChain ledger transaction. It is not an account:
/// the transaction moves no amount, and the fee goes to L1 validators.
pub const APPCHAIN_TX_RECIPIENT: &str = "appchain";
/// Domain string bound into the version-5 signing preimage.
pub const APPCHAIN_ENVELOPE_DOMAIN: &[u8] = b"UltraNet/appchain-envelope/v5";

/// Registration fee: 0.01 $ULTRA. Registration is permanent; the fee is a nominal
/// barrier against throwaway names, not a revenue line, so it sits ten times the
/// anchor fee rather than driving what an AppChain costs to run.
pub const APPCHAIN_REGISTER_FEE: u64 = 10_000;
/// Anchor fee: 0.001 $ULTRA per owner-signed state root.
pub const APPCHAIN_ANCHOR_FEE: u64 = 1_000;
/// Ownership-transfer fee: 0.001 $ULTRA, one ledger write like an anchor. It is
/// deliberately cheap: when an owner's key is lost or leaked, the cost of the fix
/// must never be the reason nobody reaches for it.
pub const APPCHAIN_TRANSFER_FEE: u64 = 1_000;

pub const APPCHAIN_NAME_MAX_LEN: usize = 64;
/// Gas fields are fixed for ledger transactions so the envelope has one shape.
pub const APPCHAIN_TX_GAS_LIMIT: u64 = 100_000;
pub const APPCHAIN_TX_GAS_PRICE: u64 = 1;

const REGISTRATION_DIGEST_DOMAIN: &[u8] = b"UltraNet/appchain-registration/v1";

/// Activation height for AppChain ledger transactions.
///
/// Every validator must agree on this value, so it is read once from
/// `ULTRANET_APPCHAIN_ACTIVATION_HEIGHT`. Unset means disabled: a node build
/// carrying this code does not accept the transactions until operators set
/// the same height everywhere. Unit tests activate it from genesis.
pub fn activation_height() -> u64 {
    static HEIGHT: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *HEIGHT.get_or_init(|| {
        std::env::var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(if cfg!(test) { 0 } else { u64::MAX })
    })
}

pub fn is_active_at(block_index: u64) -> bool {
    block_index >= activation_height()
}

pub fn is_appchain_payload(payload: &TransactionPayload) -> bool {
    matches!(
        payload,
        TransactionPayload::AppChainRegister { .. }
            | TransactionPayload::AppChainAnchor { .. }
            | TransactionPayload::AppChainTransferOwner { .. }
    )
}

/// The fixed fee an AppChain ledger payload must pay at minimum.
pub fn required_fee(payload: &TransactionPayload) -> Option<u64> {
    match payload {
        TransactionPayload::AppChainRegister { .. } => Some(APPCHAIN_REGISTER_FEE),
        TransactionPayload::AppChainAnchor { .. } => Some(APPCHAIN_ANCHOR_FEE),
        TransactionPayload::AppChainTransferOwner { .. } => Some(APPCHAIN_TRANSFER_FEE),
        _ => None,
    }
}

/// Display names: 1–64 ASCII letters, digits, spaces, hyphens or underscores,
/// with no leading or trailing space. Names are labels, not identities: two
/// chains may share a name, and the chain ID is the identity.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > APPCHAIN_NAME_MAX_LEN {
        return Err(format!(
            "AppChain name must be 1 to {APPCHAIN_NAME_MAX_LEN} characters"
        ));
    }
    if name.trim() != name {
        return Err("AppChain name must not start or end with a space".to_string());
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || " -_".contains(character))
    {
        return Err(
            "AppChain name may contain only letters, numbers, spaces, hyphens and underscores"
                .to_string(),
        );
    }
    Ok(())
}

/// Stateless envelope rules shared by mempool admission and block authorization.
pub fn validate_envelope(tx: &Transaction) -> Result<(), String> {
    if tx.version != APPCHAIN_TX_VERSION {
        return Err(format!(
            "AppChain transactions require signing-envelope version {APPCHAIN_TX_VERSION}"
        ));
    }
    if tx.chain_id != crate::UltraBlockchain::L1_CHAIN_ID {
        return Err("AppChain transactions are L1 transactions (chain_id 0)".to_string());
    }
    if tx.recipient != APPCHAIN_TX_RECIPIENT {
        return Err(format!(
            "AppChain transactions use the fixed recipient \"{APPCHAIN_TX_RECIPIENT}\""
        ));
    }
    if tx.amount != 0 {
        return Err("AppChain transactions move no amount".to_string());
    }
    if !tx.zk_proof.is_empty() {
        return Err("AppChain transactions carry no ZK proof".to_string());
    }
    if tx.proof_type != crate::ProofType::Ownership {
        return Err("AppChain transactions use the Ownership proof type".to_string());
    }
    if tx.gas_limit != APPCHAIN_TX_GAS_LIMIT || tx.gas_price != APPCHAIN_TX_GAS_PRICE {
        return Err("AppChain transactions use fixed gas fields".to_string());
    }
    if !crate::UltraBlockchain::is_valid_address(&tx.sender) {
        return Err("AppChain transaction sender must be a 64-hex address".to_string());
    }
    let fee = required_fee(&tx.payload).ok_or("Not an AppChain transaction")?;
    if tx.fee < fee {
        return Err(format!("AppChain transaction fee is too low! Minimum: {fee}"));
    }
    if let TransactionPayload::AppChainRegister { name } = &tx.payload {
        validate_name(name)?;
    }
    if let TransactionPayload::AppChainTransferOwner {
        chain_id,
        new_owner,
    } = &tx.payload
    {
        if *chain_id == 0 {
            return Err("An ownership transfer must name the chain it transfers".to_string());
        }
        if !crate::UltraBlockchain::is_valid_address(new_owner) {
            return Err("The new owner must be a 64-hex address".to_string());
        }
        if *new_owner == tx.sender {
            return Err("An AppChain owner cannot transfer it to itself".to_string());
        }
    }
    Ok(())
}

/// Version-5 signing preimage: the legacy v1 field hash followed by a
/// domain-separated, length-prefixed payload binding.
pub fn append_signing_payload(hasher: &mut Sha3_256, tx: &Transaction) {
    hasher.update(APPCHAIN_ENVELOPE_DOMAIN);
    hasher.update(tx.version.to_le_bytes());
    hasher.update(tx.chain_id.to_le_bytes());
    match &tx.payload {
        TransactionPayload::AppChainRegister { name } => {
            hasher.update(b"AppChainRegister");
            hasher.update((name.len() as u64).to_le_bytes());
            hasher.update(name.as_bytes());
        }
        TransactionPayload::AppChainAnchor {
            chain_id,
            state_root,
        } => {
            hasher.update(b"AppChainAnchor");
            hasher.update(chain_id.to_le_bytes());
            hasher.update(state_root);
        }
        TransactionPayload::AppChainTransferOwner {
            chain_id,
            new_owner,
        } => {
            hasher.update(b"AppChainTransferOwner");
            hasher.update(chain_id.to_le_bytes());
            hasher.update((new_owner.len() as u64).to_le_bytes());
            hasher.update(new_owner.as_bytes());
        }
        _ => {}
    }
}

/// One registered AppChain, as stored in the node's ledger tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerChain {
    pub id: u32,
    pub name: String,
    /// The L1 address that signed the registration. Only it may anchor.
    pub owner: String,
    pub registered_tx: String,
    pub registered_block: u64,
    pub anchor_count: u64,
    pub latest_state_root: Option<String>,
    pub latest_anchor_block: Option<u64>,
}

/// One owner-signed state root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerAnchor {
    pub chain_id: u32,
    pub sequence: u64,
    pub state_root: String,
    pub tx_hash: String,
    pub block_index: u64,
    pub block_timestamp: u64,
}

/// Read access to the persisted ledger, as of the parent of the block being planned.
pub trait LedgerView {
    fn chain(&self, id: u32) -> Option<LedgerChain>;
    fn next_chain_id(&self) -> u32;
    fn registration_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u32>;
    fn anchor_for_tx(&self, tx_hash: &[u8; 32]) -> Option<(u32, u64)>;
    fn transfer_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u32>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerEffect {
    Register {
        chain: LedgerChain,
        /// False when replaying a block whose registration is already stored.
        fresh: bool,
    },
    Anchor {
        anchor: LedgerAnchor,
        /// The chain record after this anchor.
        chain: LedgerChain,
        fresh: bool,
    },
    TransferOwnership {
        /// The chain record after the handover: `owner` is the new owner.
        chain: LedgerChain,
        tx_hash: String,
        fresh: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTx {
    pub tx_index: usize,
    pub sender: String,
    pub fee: u64,
    pub effect: LedgerEffect,
}

impl LedgerEffect {
    /// State-trie entries committed by this effect. Values are derived only
    /// from immutable records, so a replay writes identical bytes.
    pub fn trie_entries(&self) -> Vec<(String, Vec<u8>)> {
        match self {
            LedgerEffect::Register { chain, .. } => vec![(
                format!("appchain:{}", chain.id),
                registration_digest(chain).to_vec(),
            )],
            LedgerEffect::Anchor { anchor, .. } => vec![(
                format!("appchain:{}:anchor:{}", anchor.chain_id, anchor.sequence),
                hex::decode(&anchor.state_root).unwrap_or_default(),
            )],
            // The registration digest includes the owner, so a handover rewrites it.
            LedgerEffect::TransferOwnership { chain, .. } => vec![(
                format!("appchain:{}", chain.id),
                registration_digest(chain).to_vec(),
            )],
        }
    }
}

pub fn registration_digest(chain: &LedgerChain) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(REGISTRATION_DIGEST_DOMAIN);
    hasher.update(chain.id.to_le_bytes());
    hasher.update((chain.name.len() as u64).to_le_bytes());
    hasher.update(chain.name.as_bytes());
    hasher.update(chain.owner.as_bytes());
    hasher.update(chain.registered_tx.as_bytes());
    hasher.finalize().into()
}

/// Derive the ledger effects of a block's AppChain transactions, in block order.
///
/// Fails when an anchor names an unknown chain or is not signed by its owner —
/// a block carrying such a transaction is invalid. Registrations and anchors in
/// the same block are visible to later transactions of that block.
pub fn plan_block<V: LedgerView>(
    view: &V,
    block_index: u64,
    block_timestamp: u64,
    transactions: &[Transaction],
) -> Result<Vec<PlannedTx>, String> {
    let mut overlay: HashMap<u32, LedgerChain> = HashMap::new();
    let mut next_id = view.next_chain_id();
    let mut planned = Vec::new();

    for (tx_index, tx) in transactions.iter().enumerate() {
        if !is_appchain_payload(&tx.payload) {
            continue;
        }
        if !is_active_at(block_index) {
            return Err("AppChain transactions are not active at this height".to_string());
        }
        let tx_hash = tx.get_hash();
        let effect = match &tx.payload {
            TransactionPayload::AppChainRegister { name } => {
                validate_name(name)?;
                if let Some(existing_id) = view.registration_for_tx(&tx_hash) {
                    let chain = overlay
                        .get(&existing_id)
                        .cloned()
                        .or_else(|| view.chain(existing_id))
                        .ok_or_else(|| {
                            format!("AppChain #{existing_id} registration record is missing")
                        })?;
                    next_id = next_id.max(existing_id.saturating_add(1));
                    LedgerEffect::Register {
                        chain,
                        fresh: false,
                    }
                } else {
                    let id = next_id;
                    next_id = next_id
                        .checked_add(1)
                        .ok_or("AppChain ID space is exhausted")?;
                    let chain = LedgerChain {
                        id,
                        name: name.clone(),
                        owner: tx.sender.clone(),
                        registered_tx: hex::encode(tx_hash),
                        registered_block: block_index,
                        anchor_count: 0,
                        latest_state_root: None,
                        latest_anchor_block: None,
                    };
                    overlay.insert(id, chain.clone());
                    LedgerEffect::Register { chain, fresh: true }
                }
            }
            TransactionPayload::AppChainAnchor {
                chain_id,
                state_root,
            } => {
                let current = overlay
                    .get(chain_id)
                    .cloned()
                    .or_else(|| view.chain(*chain_id))
                    .ok_or_else(|| format!("AppChain #{chain_id} is not registered"))?;
                let stored = view.anchor_for_tx(&tx_hash);
                // A replayed transaction is reproduced, not authorised: by now its signer
                // may have transferred the chain, so re-checking ownership would reject a
                // block that is already in the chain.
                if stored.is_none() && current.owner != tx.sender {
                    return Err(format!(
                        "Only the owner of AppChain #{chain_id} may anchor it"
                    ));
                }
                if let Some((stored_chain, sequence)) = stored {
                    if stored_chain != *chain_id {
                        return Err("AppChain anchor record names a different chain".to_string());
                    }
                    let anchor = LedgerAnchor {
                        chain_id: *chain_id,
                        sequence,
                        state_root: hex::encode(state_root),
                        tx_hash: hex::encode(tx_hash),
                        block_index,
                        block_timestamp,
                    };
                    LedgerEffect::Anchor {
                        anchor,
                        chain: current,
                        fresh: false,
                    }
                } else {
                    let sequence = current
                        .anchor_count
                        .checked_add(1)
                        .ok_or("AppChain anchor sequence overflowed")?;
                    let anchor = LedgerAnchor {
                        chain_id: *chain_id,
                        sequence,
                        state_root: hex::encode(state_root),
                        tx_hash: hex::encode(tx_hash),
                        block_index,
                        block_timestamp,
                    };
                    let mut updated = current;
                    updated.anchor_count = sequence;
                    updated.latest_state_root = Some(anchor.state_root.clone());
                    updated.latest_anchor_block = Some(block_index);
                    overlay.insert(*chain_id, updated.clone());
                    LedgerEffect::Anchor {
                        anchor,
                        chain: updated,
                        fresh: true,
                    }
                }
            }
            TransactionPayload::AppChainTransferOwner {
                chain_id,
                new_owner,
            } => {
                let current = overlay
                    .get(chain_id)
                    .cloned()
                    .or_else(|| view.chain(*chain_id))
                    .ok_or_else(|| format!("AppChain #{chain_id} is not registered"))?;
                let replaying = view.transfer_for_tx(&tx_hash).is_some();
                if !replaying && current.owner != tx.sender {
                    return Err(format!(
                        "Only the owner of AppChain #{chain_id} may transfer it"
                    ));
                }
                if replaying {
                    // Replaying: the stored record already names the final owner.
                    LedgerEffect::TransferOwnership {
                        chain: current,
                        tx_hash: hex::encode(tx_hash),
                        fresh: false,
                    }
                } else {
                    if current.owner == *new_owner {
                        return Err(format!(
                            "AppChain #{chain_id} is already owned by that address"
                        ));
                    }
                    let mut updated = current;
                    updated.owner = new_owner.clone();
                    overlay.insert(*chain_id, updated.clone());
                    LedgerEffect::TransferOwnership {
                        chain: updated,
                        tx_hash: hex::encode(tx_hash),
                        fresh: true,
                    }
                }
            }
            _ => unreachable!("filtered by is_appchain_payload"),
        };
        planned.push(PlannedTx {
            tx_index,
            sender: tx.sender.clone(),
            fee: tx.fee,
            effect,
        });
    }
    Ok(planned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProofType;

    #[derive(Default)]
    struct MemoryView {
        chains: HashMap<u32, LedgerChain>,
        registrations: HashMap<[u8; 32], u32>,
        anchors: HashMap<[u8; 32], (u32, u64)>,
        transfers: HashMap<[u8; 32], u32>,
    }

    impl MemoryView {
        fn apply(&mut self, planned: &[PlannedTx], txs: &[Transaction]) {
            for item in planned {
                let hash = txs[item.tx_index].get_hash();
                match &item.effect {
                    LedgerEffect::Register { chain, .. } => {
                        self.registrations.insert(hash, chain.id);
                        self.chains.insert(chain.id, chain.clone());
                    }
                    LedgerEffect::Anchor { anchor, chain, .. } => {
                        self.anchors.insert(hash, (anchor.chain_id, anchor.sequence));
                        self.chains.insert(chain.id, chain.clone());
                    }
                    LedgerEffect::TransferOwnership { chain, .. } => {
                        self.transfers.insert(hash, chain.id);
                        self.chains.insert(chain.id, chain.clone());
                    }
                }
            }
        }
    }

    impl LedgerView for MemoryView {
        fn chain(&self, id: u32) -> Option<LedgerChain> {
            self.chains.get(&id).cloned()
        }
        fn next_chain_id(&self) -> u32 {
            self.chains.keys().max().map_or(1, |id| id + 1)
        }
        fn registration_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u32> {
            self.registrations.get(tx_hash).copied()
        }
        fn anchor_for_tx(&self, tx_hash: &[u8; 32]) -> Option<(u32, u64)> {
            self.anchors.get(tx_hash).copied()
        }
        fn transfer_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u32> {
            self.transfers.get(tx_hash).copied()
        }
    }

    fn tx(sender: &str, nonce: u64, payload: TransactionPayload) -> Transaction {
        Transaction {
            sender: sender.to_string(),
            sender_public_key: vec![],
            recipient: APPCHAIN_TX_RECIPIENT.to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [nonce as u8; 32],
            timestamp: 1_800_000_000,
            fee: required_fee(&payload).unwrap(),
            nonce,
            gas_limit: APPCHAIN_TX_GAS_LIMIT,
            gas_price: APPCHAIN_TX_GAS_PRICE,
            proof_type: ProofType::Ownership,
            payload,
            chain_id: 0,
            version: APPCHAIN_TX_VERSION,
        }
    }

    fn register(sender: &str, nonce: u64, name: &str) -> Transaction {
        tx(sender, nonce, TransactionPayload::AppChainRegister { name: name.into() })
    }

    fn anchor(sender: &str, nonce: u64, chain_id: u32, byte: u8) -> Transaction {
        tx(
            sender,
            nonce,
            TransactionPayload::AppChainAnchor {
                chain_id,
                state_root: [byte; 32],
            },
        )
    }

    fn transfer(sender: &str, nonce: u64, chain_id: u32, new_owner: &str) -> Transaction {
        tx(
            sender,
            nonce,
            TransactionPayload::AppChainTransferOwner {
                chain_id,
                new_owner: new_owner.to_string(),
            },
        )
    }

    const OWNER: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const OTHER: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn registration_assigns_sequential_ids_and_the_signer_owns_the_chain() {
        let view = MemoryView::default();
        let txs = vec![register(OWNER, 0, "UltraNet Grid"), register(OTHER, 0, "Second")];
        let planned = plan_block(&view, 5, 100, &txs).unwrap();
        let ids: Vec<(u32, &str)> = planned
            .iter()
            .map(|item| match &item.effect {
                LedgerEffect::Register { chain, fresh } => {
                    assert!(fresh);
                    (chain.id, chain.owner.as_str())
                }
                _ => panic!("expected registrations"),
            })
            .collect();
        assert_eq!(ids, vec![(1, OWNER), (2, OTHER)]);
    }

    #[test]
    fn anchors_get_consecutive_sequences_within_and_across_blocks() {
        let mut view = MemoryView::default();
        let block1 = vec![
            register(OWNER, 0, "Grid"),
            anchor(OWNER, 1, 1, 0xAA),
            anchor(OWNER, 2, 1, 0xBB),
        ];
        let planned = plan_block(&view, 1, 10, &block1).unwrap();
        let sequences: Vec<u64> = planned
            .iter()
            .filter_map(|item| match &item.effect {
                LedgerEffect::Anchor { anchor, .. } => Some(anchor.sequence),
                _ => None,
            })
            .collect();
        assert_eq!(sequences, vec![1, 2]);
        view.apply(&planned, &block1);

        let block2 = vec![anchor(OWNER, 3, 1, 0xCC)];
        let planned = plan_block(&view, 2, 20, &block2).unwrap();
        match &planned[0].effect {
            LedgerEffect::Anchor { anchor, chain, fresh } => {
                assert!(fresh);
                assert_eq!(anchor.sequence, 3);
                assert_eq!(chain.anchor_count, 3);
                assert_eq!(chain.latest_state_root.as_deref(), Some(&"cc".repeat(32)[..]));
            }
            _ => panic!("expected an anchor"),
        }
    }

    #[test]
    fn only_the_owner_may_anchor_and_unknown_chains_are_rejected() {
        let mut view = MemoryView::default();
        let block1 = vec![register(OWNER, 0, "Grid")];
        let planned = plan_block(&view, 1, 10, &block1).unwrap();
        view.apply(&planned, &block1);

        let error = plan_block(&view, 2, 20, &[anchor(OTHER, 0, 1, 1)]).unwrap_err();
        assert!(error.contains("Only the owner"), "{error}");
        let error = plan_block(&view, 2, 20, &[anchor(OWNER, 1, 9, 1)]).unwrap_err();
        assert!(error.contains("not registered"), "{error}");
    }

    #[test]
    fn replaying_an_applied_block_reproduces_the_same_ids_and_sequences() {
        let mut view = MemoryView::default();
        let block1 = vec![register(OWNER, 0, "Grid"), anchor(OWNER, 1, 1, 0xAA)];
        let first = plan_block(&view, 1, 10, &block1).unwrap();
        view.apply(&first, &block1);
        // A later registration exists; replaying block 1 must not hand out ID 2 or 3.
        let block2 = vec![register(OTHER, 0, "Other")];
        let second = plan_block(&view, 2, 20, &block2).unwrap();
        view.apply(&second, &block2);

        let replay = plan_block(&view, 1, 10, &block1).unwrap();
        let entries: Vec<_> = replay.iter().flat_map(|item| item.effect.trie_entries()).collect();
        let original: Vec<_> = first.iter().flat_map(|item| item.effect.trie_entries()).collect();
        assert_eq!(entries, original);
        assert!(replay.iter().all(|item| match &item.effect {
            LedgerEffect::Register { fresh, .. }
            | LedgerEffect::Anchor { fresh, .. }
            | LedgerEffect::TransferOwnership { fresh, .. } => !fresh,
        }));
    }

    #[test]
    fn only_the_owner_may_transfer_and_the_new_owner_may_anchor() {
        let mut view = MemoryView::default();
        let block1 = vec![register(OWNER, 0, "UltraNet Grid")];
        view.apply(&plan_block(&view, 1, 10, &block1).unwrap(), &block1);

        // A stranger cannot take the chain.
        let hostile = vec![transfer(OTHER, 1, 1, OTHER)];
        let error = plan_block(&view, 2, 20, &hostile).unwrap_err();
        assert!(error.contains("Only the owner"), "{error}");

        // The owner can hand it over.
        let handover = vec![transfer(OWNER, 1, 1, OTHER)];
        let planned = plan_block(&view, 2, 20, &handover).unwrap();
        match &planned[0].effect {
            LedgerEffect::TransferOwnership { chain, fresh, .. } => {
                assert!(fresh);
                assert_eq!(chain.id, 1);
                assert_eq!(chain.owner, OTHER);
            }
            _ => panic!("expected a transfer"),
        }
        view.apply(&planned, &handover);

        // The old owner can no longer anchor it...
        assert!(plan_block(&view, 3, 30, &[anchor(OWNER, 2, 1, 0xAA)]).is_err());
        // ...and the new owner can.
        assert!(plan_block(&view, 3, 30, &[anchor(OTHER, 3, 1, 0xBB)]).is_ok());
    }

    #[test]
    fn replaying_a_transfer_leaves_one_owner() {
        let mut view = MemoryView::default();
        let block1 = vec![register(OWNER, 0, "Grid")];
        view.apply(&plan_block(&view, 1, 10, &block1).unwrap(), &block1);
        let block2 = vec![transfer(OWNER, 1, 1, OTHER)];
        let planned = plan_block(&view, 2, 20, &block2).unwrap();
        view.apply(&planned, &block2);

        // Re-planning the applied block must reproduce the record, not hand it over twice.
        let replay = plan_block(&view, 2, 20, &block2).unwrap();
        match &replay[0].effect {
            LedgerEffect::TransferOwnership { chain, fresh, .. } => {
                assert!(!fresh);
                assert_eq!(chain.owner, OTHER);
            }
            _ => panic!("expected a transfer"),
        }
    }

    #[test]
    fn a_transfer_needs_a_usable_address_a_chain_and_a_new_owner() {
        // Self-transfer, a malformed address and chain 0 are stateless rejections.
        assert!(validate_envelope(&transfer(OWNER, 0, 1, OWNER)).is_err());
        assert!(validate_envelope(&transfer(OWNER, 0, 1, "not-an-address")).is_err());
        assert!(validate_envelope(&transfer(OWNER, 0, 0, OTHER)).is_err());

        // The fee is the transfer fee, not the anchor fee.
        let mut underpaid = transfer(OWNER, 0, 1, OTHER);
        underpaid.fee = APPCHAIN_TRANSFER_FEE - 1;
        assert!(validate_envelope(&underpaid)
            .unwrap_err()
            .contains("fee is too low"));
        assert_eq!(
            required_fee(&TransactionPayload::AppChainTransferOwner {
                chain_id: 1,
                new_owner: OTHER.to_string(),
            }),
            Some(APPCHAIN_TRANSFER_FEE)
        );

        // Transferring to the address that already owns it is a stateful rejection.
        let view = MemoryView::default();
        let block1 = vec![register(OWNER, 0, "Grid")];
        let mut view = view;
        view.apply(&plan_block(&view, 1, 10, &block1).unwrap(), &block1);
        let error = plan_block(&view, 2, 20, &[transfer(OWNER, 1, 1, OWNER)]).unwrap_err();
        assert!(error.contains("already owned"), "{error}");
    }

    #[test]
    fn envelope_rules_fix_the_shape_and_minimum_fee() {
        let mut good = register(OWNER, 0, "Grid");
        assert!(validate_envelope(&good).is_ok());

        good.fee = APPCHAIN_REGISTER_FEE - 1;
        assert!(validate_envelope(&good).unwrap_err().contains("fee is too low"));

        let mut moved = register(OWNER, 0, "Grid");
        moved.amount = 1;
        assert!(validate_envelope(&moved).is_err());

        let mut wrong_recipient = anchor(OWNER, 0, 1, 1);
        wrong_recipient.recipient = OTHER.to_string();
        assert!(validate_envelope(&wrong_recipient).is_err());

        assert!(validate_envelope(&register(OWNER, 0, " padded")).is_err());
        assert!(validate_envelope(&register(OWNER, 0, "semi;colon")).is_err());
        assert!(validate_envelope(&register(OWNER, 0, &"x".repeat(65))).is_err());
    }
}

/// Shared signing vectors. Every client (Android, website, game server) must
/// reproduce these digests byte for byte; they are pinned here so the node
/// cannot change the version-5 preimage without failing this test.
#[cfg(test)]
mod signing_vectors {
    use super::*;
    use crate::ProofType;

    pub const VECTOR_SENDER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    pub const VECTOR_NEW_OWNER: &str = "2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed";
    pub const VECTOR_OTHER_OWNER: &str = "e6766eab2df27d29a224fe8554b0ca22ce100323993e3554ec7eeb9d590fa37b";

    fn vector_tx(payload: TransactionPayload, nonce: u64) -> Transaction {
        Transaction {
            sender: VECTOR_SENDER.to_string(),
            sender_public_key: vec![],
            recipient: APPCHAIN_TX_RECIPIENT.to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [0x11; 32],
            timestamp: 1_800_000_000,
            fee: required_fee(&payload).unwrap(),
            nonce,
            gas_limit: APPCHAIN_TX_GAS_LIMIT,
            gas_price: APPCHAIN_TX_GAS_PRICE,
            proof_type: ProofType::Ownership,
            payload,
            chain_id: 0,
            version: APPCHAIN_TX_VERSION,
        }
    }

    #[test]
    fn version_5_digests_are_pinned() {
        let register = vector_tx(
            TransactionPayload::AppChainRegister { name: "UltraNet Grid".into() },
            0,
        );
        let anchor = vector_tx(
            TransactionPayload::AppChainAnchor { chain_id: 1, state_root: [0xAB; 32] },
            1,
        );
        let transfer = vector_tx(
            TransactionPayload::AppChainTransferOwner {
                chain_id: 1,
                new_owner: VECTOR_NEW_OWNER.to_string(),
            },
            2,
        );
        let register_digest = hex::encode(crate::UltraBlockchain::create_transaction_message_for(&register));
        let anchor_digest = hex::encode(crate::UltraBlockchain::create_transaction_message_for(&anchor));
        let transfer_digest = hex::encode(crate::UltraBlockchain::create_transaction_message_for(&transfer));
        println!("REGISTER_DIGEST={register_digest}");
        println!("ANCHOR_DIGEST={anchor_digest}");
        println!("TRANSFER_DIGEST={transfer_digest}");
        assert_eq!(register_digest, REGISTER_DIGEST);
        assert_eq!(anchor_digest, ANCHOR_DIGEST);
        assert_eq!(transfer_digest, TRANSFER_DIGEST);
    }

    const REGISTER_DIGEST: &str = "7dcaf29f0809d289fdf6e10223ae79c6e7e1b319d45c51de8b17550756130f90";
    const ANCHOR_DIGEST: &str = "f1c64d0830b020aa34c0add54ae7815f61cbe0007e2b6795e844bbbfadd43a72";
    const TRANSFER_DIGEST: &str = "702b1dd8dcdf04061511aea85a9d049843b3e2e531cca690b9176d50b8bcf9ab";

    /// A pinned digest proves the bytes are reproducible, not that every field is in them: a field
    /// silently dropped from the preimage would still match a regenerated constant. Changing one
    /// field at a time has to change the digest, and that is what this asserts.
    #[test]
    fn every_envelope_field_changes_the_digest() {
        let digest = |tx: &Transaction| crate::UltraBlockchain::create_transaction_message_for(tx);
        let base = vector_tx(
            TransactionPayload::AppChainRegister { name: "UltraNet Grid".into() },
            0,
        );
        let baseline = digest(&base);

        let mut nonce = base.clone();
        nonce.nonce = 1;
        assert_ne!(baseline, digest(&nonce), "nonce must be in the preimage");

        let mut timestamp = base.clone();
        timestamp.timestamp += 1;
        assert_ne!(baseline, digest(&timestamp), "timestamp must be in the preimage");

        let mut fee = base.clone();
        fee.fee += 1;
        assert_ne!(baseline, digest(&fee), "fee must be in the preimage");

        let mut name = base.clone();
        name.payload = TransactionPayload::AppChainRegister { name: "UltraNet Grids".into() };
        assert_ne!(baseline, digest(&name), "the registered name must be in the preimage");

        let mut version = base.clone();
        version.version = 4;
        assert_ne!(baseline, digest(&version), "the envelope version must be in the preimage");

        let mut sender = base.clone();
        sender.sender = VECTOR_OTHER_OWNER.to_string();
        assert_ne!(baseline, digest(&sender), "the sender must be in the preimage");

        let mut nullifier = base.clone();
        nullifier.nullifier = [0x12; 32];
        assert_ne!(baseline, digest(&nullifier), "the nullifier must be in the preimage");

        // The transaction's own L1 chain id, which is a different field from the
        // anchored chain number asserted further down.
        let mut l1_chain = base.clone();
        l1_chain.chain_id = 1;
        assert_ne!(baseline, digest(&l1_chain), "the L1 chain id must be in the preimage");

        let anchor_base = vector_tx(
            TransactionPayload::AppChainAnchor { chain_id: 1, state_root: [0xAB; 32] },
            1,
        );
        let anchor_digest = digest(&anchor_base);
        let mut chain = anchor_base.clone();
        chain.payload = TransactionPayload::AppChainAnchor { chain_id: 2, state_root: [0xAB; 32] };
        assert_ne!(anchor_digest, digest(&chain), "the anchored chain id must be in the preimage");

        let mut root = anchor_base.clone();
        root.payload = TransactionPayload::AppChainAnchor { chain_id: 1, state_root: [0xAC; 32] };
        assert_ne!(anchor_digest, digest(&root), "the state root must be in the preimage");

        let handover = vector_tx(
            TransactionPayload::AppChainTransferOwner {
                chain_id: 1,
                new_owner: VECTOR_NEW_OWNER.to_string(),
            },
            2,
        );
        let handover_digest = digest(&handover);
        let mut redirect = handover;
        redirect.payload = TransactionPayload::AppChainTransferOwner {
            chain_id: 1,
            new_owner: VECTOR_OTHER_OWNER.to_string(),
        };
        assert_ne!(
            handover_digest,
            digest(&redirect),
            "the new owner must be in the preimage"
        );
    }
}
