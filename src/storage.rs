// ============================================================
// PERSISTENT STORAGE ZA ULTRA NET 4.0
// ============================================================

use sha3::Digest;
use sled::{Batch, Db, Tree};
use std::collections::HashMap;

use crate::appchain::{AnchoredState, AppChainConfig};
use crate::bls_aggregation::ValidatorInfo;
use crate::dag_mysticeti::{MysticetiVertex, ValidatorStats};
use crate::governance::approval::{
    ApprovalAuditRecord, ApprovalIntentRecord, ApprovalNonceReservation, ApprovalSignatureRecord,
};
use crate::{
    Transaction, TransactionPayload, UltraBlock, ValidatorApprovalRecord, ValidatorJoinProposalData,
};

pub const INITIAL_SHARD_COUNT: u8 = 16;

pub struct Storage {
    pub db: Db,
    pub blocks: Tree,
    pub transactions: Tree,
    pub pending_transactions: Tree,
    pub nullifiers: Tree,
    pub account_nonces: Tree,
    pub validators: Tree,
    pub checkpoints: Tree,
    pub dag_vertices: Tree,
    pub validator_stats: Tree,
    pub pending_proposals: Tree,
    pub approval_journal: Tree,
    pub approval_journal_index: Tree,
    pub approval_intents: Tree,
    pub approval_signatures: Tree,
    pub approval_audit: Tree,
    pub approval_nonce_reservations: Tree,
    pub auth_challenges: Tree,
    pub auth_sessions: Tree,
    pub appchain_configs: Tree,
    pub appchain_anchors: Tree,
    pub supply_corrections: Tree,
    pub move_modules: Tree,
    pub move_resources: Tree,
    pub fhe_keys: Tree,
    // Sharded state and tries
    pub state_shards: Vec<Tree>,
    pub trie_shards: Vec<Tree>,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self, sled::Error> {
        let db = sled::open(path)?;

        let mut state_shards = Vec::new();
        let mut trie_shards = Vec::new();

        for i in 0..INITIAL_SHARD_COUNT {
            state_shards.push(db.open_tree(format!("state_shard_{}", i))?);
            trie_shards.push(db.open_tree(format!("trie_shard_{}", i))?);
        }

        let storage = Self {
            blocks: db.open_tree("blocks")?,
            transactions: db.open_tree("transactions")?,
            pending_transactions: db.open_tree("pending_transactions")?,
            nullifiers: db.open_tree("nullifiers")?,
            account_nonces: db.open_tree("account_nonces")?,
            validators: db.open_tree("validators")?,
            checkpoints: db.open_tree("checkpoints")?,
            dag_vertices: db.open_tree("dag_vertices")?,
            validator_stats: db.open_tree("validator_stats")?,
            pending_proposals: db.open_tree("pending_proposals")?,
            approval_journal: db.open_tree("approval_journal")?,
            approval_journal_index: db.open_tree("approval_journal_index")?,
            approval_intents: db.open_tree("approval_intents")?,
            approval_signatures: db.open_tree("approval_signatures")?,
            approval_audit: db.open_tree("approval_audit")?,
            approval_nonce_reservations: db.open_tree("approval_nonce_reservations")?,
            auth_challenges: db.open_tree("auth_challenges")?,
            auth_sessions: db.open_tree("auth_sessions")?,
            appchain_configs: db.open_tree("appchain_configs")?,
            appchain_anchors: db.open_tree("appchain_anchors")?,
            supply_corrections: db.open_tree("supply_corrections")?,
            move_modules: db.open_tree("move_modules")?,
            move_resources: db.open_tree("move_resources")?,
            fhe_keys: db.open_tree("fhe_keys")?,
            state_shards,
            trie_shards,
            db,
        };

        if storage.approval_journal_index.len() != storage.approval_journal.len() {
            storage
                .rebuild_approval_journal_index()
                .map_err(sled::Error::Unsupported)?;
        }
        storage
            .reconcile_supply_correction_markers()
            .map_err(sled::Error::Unsupported)?;
        Ok(storage)
    }

    pub fn get_shard_id(&self, key: &[u8]) -> u8 {
        if key.is_empty() {
            return 0;
        }
        // Koristimo prvi bajt heša ključa za uniformnu distribuciju po shardovima
        let mut hasher = sha3::Sha3_256::new();
        sha3::Digest::update(&mut hasher, key);
        let hash = hasher.finalize();
        hash[0] % INITIAL_SHARD_COUNT
    }

    pub fn save_block(&self, block: &UltraBlock) -> Result<(), sled::Error> {
        // Validate/reserve correction-marker ownership before writing any
        // block data. This keeps a conflicting one-time correction from being
        // persisted under a different transaction identity and also makes a
        // remote block self-describing after restart.
        for tx in &block.transactions {
            if let TransactionPayload::SovereignSupplyCorrection { correction_id, .. } = &tx.payload
            {
                let tx_hash = tx.get_hash();
                if !self.supply_correction_matches_or_reserve(correction_id, &tx_hash)? {
                    return Err(sled::Error::Unsupported(
                        "supply correction marker is bound to another transaction".into(),
                    ));
                }
            }
        }

        let key = block.index.to_be_bytes();
        let value = bincode::serialize(block).unwrap();
        self.blocks.insert(key, value)?;

        // Indeksiraj svaku transakciju po njenom hešu i atomarno promoviši
        // pending zapis u potvrđenu istoriju.
        for tx in &block.transactions {
            let tx_hash = tx.get_hash();
            let tx_val = bincode::serialize(tx)
                .map_err(|_| sled::Error::Unsupported("serialize tx failed".into()))?;
            self.transactions.insert(&tx_hash, tx_val)?;
            self.pending_transactions.remove(&tx_hash)?;
            self.nullifiers.insert(tx.nullifier, &tx_hash)?;

            let next_nonce = tx.nonce.saturating_add(1);
            if self
                .get_account_nonce(&tx.sender)
                .map_or(true, |current| current < next_nonce)
            {
                self.save_account_nonce(&tx.sender, next_nonce)?;
            }
        }
        self.db.flush()?;

        Ok(())
    }

    pub fn get_transaction(&self, hash: &[u8; 32]) -> Option<Transaction> {
        self.get_confirmed_transaction(hash)
            .or_else(|| self.get_pending_transaction(hash))
    }

    pub fn get_confirmed_transaction(&self, hash: &[u8; 32]) -> Option<Transaction> {
        self.transactions
            .get(hash)
            .ok()
            .flatten()
            .and_then(|value| bincode::deserialize(&value).ok())
    }

    pub fn is_pending_transaction(&self, hash: &[u8; 32]) -> bool {
        self.pending_transactions
            .contains_key(hash)
            .unwrap_or(false)
    }

    pub fn get_transaction_by_nullifier(&self, nullifier: &[u8; 32]) -> Option<Transaction> {
        if let Some(hash) = self.get_nullifier(nullifier) {
            return self.get_transaction(&hash);
        }
        self.get_pending_transaction_by_nullifier(nullifier)
            .ok()
            .flatten()
    }

    pub fn delete_nullifier_if_matches(
        &self,
        nullifier: &[u8; 32],
        hash: &[u8; 32],
    ) -> Result<bool, sled::Error> {
        match self.nullifiers.compare_and_swap(
            nullifier,
            Some(hash.as_slice()),
            None as Option<&[u8]>,
        )? {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn save_pending_transaction(&self, tx: &Transaction) -> Result<(), sled::Error> {
        let hash = tx.get_hash();
        let value = bincode::serialize(tx)
            .map_err(|_| sled::Error::Unsupported("serialize pending transaction failed".into()))?;
        self.pending_transactions.insert(hash, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_pending_transaction(&self, hash: &[u8; 32]) -> Option<Transaction> {
        self.pending_transactions
            .get(hash)
            .ok()
            .flatten()
            .and_then(|value| bincode::deserialize(&value).ok())
    }

    pub fn delete_pending_transaction(&self, hash: &[u8; 32]) -> Result<(), sled::Error> {
        self.pending_transactions.remove(hash)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_pending_transactions_for_address(
        &self,
        address: &str,
    ) -> Result<Vec<Transaction>, String> {
        self.get_all_pending_transactions().map(|transactions| {
            transactions
                .into_iter()
                .filter(|tx| tx.sender == address || tx.recipient == address)
                .collect()
        })
    }

    pub fn get_pending_transaction_by_nullifier(
        &self,
        nullifier: &[u8; 32],
    ) -> Result<Option<Transaction>, String> {
        Ok(self
            .get_all_pending_transactions()?
            .into_iter()
            .find(|tx| &tx.nullifier == nullifier))
    }

    pub fn get_all_pending_transactions(&self) -> Result<Vec<Transaction>, String> {
        self.pending_transactions
            .iter()
            .map(|item| {
                let (key, value) = item.map_err(|error| error.to_string())?;
                let hash: [u8; 32] = key
                    .as_ref()
                    .try_into()
                    .map_err(|_| "pending transaction key must be exactly 32 bytes".to_string())?;
                let tx = bincode::deserialize::<Transaction>(&value).map_err(|error| {
                    format!("invalid pending transaction {}: {error}", hex::encode(hash))
                })?;
                if tx.get_hash() != hash {
                    return Err(format!(
                        "pending transaction hash mismatch {}",
                        hex::encode(hash)
                    ));
                }
                Ok(tx)
            })
            .collect()
    }

    pub fn get_nullifier(&self, nullifier: &[u8; 32]) -> Option<[u8; 32]> {
        self.nullifiers
            .get(nullifier)
            .ok()
            .flatten()
            .and_then(|value| value.as_ref().try_into().ok())
    }

    fn reconcile_supply_correction_markers(&self) -> Result<(), String> {
        let markers = self
            .supply_corrections
            .iter()
            .map(|item| item.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut removed_orphans = false;
        for (correction_id, transaction_hash) in markers {
            let correction_id: [u8; 32] = correction_id
                .as_ref()
                .try_into()
                .map_err(|_| "supply correction ID must be exactly 32 bytes".to_string())?;
            let transaction_hash: [u8; 32] =
                transaction_hash.as_ref().try_into().map_err(|_| {
                    "supply correction transaction hash must be exactly 32 bytes".to_string()
                })?;
            let is_live = self
                .get_transaction(&transaction_hash)
                .is_some_and(|transaction| {
                    matches!(
                        transaction.payload,
                        TransactionPayload::SovereignSupplyCorrection {
                            correction_id: transaction_id,
                            ..
                        } if transaction_id == correction_id
                    )
                });
            if !is_live {
                self.supply_corrections
                    .remove(correction_id)
                    .map_err(|error| error.to_string())?;
                removed_orphans = true;
            }
        }
        if removed_orphans {
            self.db.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn has_supply_correction(&self, correction_id: &[u8; 32]) -> bool {
        self.supply_corrections
            .contains_key(correction_id)
            .unwrap_or(false)
    }

    pub fn reserve_supply_correction(
        &self,
        correction_id: &[u8; 32],
        transaction_hash: &[u8; 32],
    ) -> Result<bool, sled::Error> {
        match self.supply_corrections.compare_and_swap(
            correction_id,
            None as Option<&[u8]>,
            Some(transaction_hash.as_slice()),
        )? {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn supply_correction_matches_or_reserve(
        &self,
        correction_id: &[u8; 32],
        transaction_hash: &[u8; 32],
    ) -> Result<bool, sled::Error> {
        if let Some(existing) = self.get_supply_correction_hash(correction_id) {
            return Ok(existing == *transaction_hash);
        }
        self.reserve_supply_correction(correction_id, transaction_hash)
    }

    pub fn delete_supply_correction_if_matches(
        &self,
        correction_id: &[u8; 32],
        transaction_hash: &[u8; 32],
    ) -> Result<bool, sled::Error> {
        match self.supply_corrections.compare_and_swap(
            correction_id,
            Some(transaction_hash.as_slice()),
            None as Option<&[u8]>,
        )? {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn get_supply_correction_hash(&self, correction_id: &[u8; 32]) -> Option<[u8; 32]> {
        self.supply_corrections
            .get(correction_id)
            .ok()
            .flatten()
            .and_then(|value| value.as_ref().try_into().ok())
    }

    pub fn reserve_nullifier(
        &self,
        nullifier: &[u8; 32],
        hash: &[u8; 32],
    ) -> Result<bool, sled::Error> {
        match self.nullifiers.compare_and_swap(
            nullifier,
            None as Option<&[u8]>,
            Some(hash.as_slice()),
        )? {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn get_account_nonce(&self, address: &str) -> Option<u64> {
        let value = self.account_nonces.get(address.as_bytes()).ok().flatten()?;
        let bytes: [u8; 8] = value.as_ref().try_into().ok()?;
        Some(u64::from_be_bytes(bytes))
    }

    pub fn save_account_nonce(&self, address: &str, nonce: u64) -> Result<(), sled::Error> {
        self.account_nonces
            .insert(address.as_bytes(), nonce.to_be_bytes().as_slice())?;
        Ok(())
    }

    pub fn get_block(&self, index: u64) -> Option<UltraBlock> {
        let key = index.to_be_bytes();
        if let Some(value) = self.blocks.get(key).ok().flatten() {
            return bincode::deserialize(&value).ok();
        }
        None
    }

    pub fn get_last_block(&self) -> Option<UltraBlock> {
        if let Some((_key, value)) = self.blocks.last().ok().flatten() {
            return bincode::deserialize(&value).ok();
        }
        None
    }

    pub fn get_chain_length(&self) -> u64 {
        self.blocks.len() as u64
    }

    /// NOVO: Učitava SVE blokove sa diska, sortirane po indeksu (rastuće).
    /// Sled `blocks` stablo koristi big-endian bajtove indeksa kao ključ,
    /// pa je iteracija po ključu već prirodno sortirana.
    pub fn get_all_blocks(&self) -> Vec<UltraBlock> {
        let mut blocks = Vec::new();
        for item in self.blocks.iter() {
            if let Ok((_, value)) = item {
                if let Ok(block) = bincode::deserialize::<UltraBlock>(&value) {
                    blocks.push(block);
                }
            }
        }
        blocks
    }

    pub fn save_state(&self, address: &str, balance: u64) -> Result<(), sled::Error> {
        let key = address.as_bytes();
        let shard_id = self.get_shard_id(key);
        self.state_shards[shard_id as usize].insert(key, &balance.to_be_bytes())?;
        Ok(())
    }

    pub fn get_state(&self, address: &str) -> Option<u64> {
        let key = address.as_bytes();
        let shard_id = self.get_shard_id(key);
        if let Some(value) = self.state_shards[shard_id as usize].get(key).ok().flatten() {
            let bytes: [u8; 8] = value.as_ref().try_into().ok()?;
            return Some(u64::from_be_bytes(bytes));
        }
        None
    }

    pub fn get_all_state(&self) -> HashMap<String, u64> {
        let mut map = HashMap::new();
        for shard in &self.state_shards {
            for item in shard.iter() {
                if let Ok((key, value)) = item {
                    if let Ok(address) = String::from_utf8(key.to_vec()) {
                        let mut bytes = [0u8; 8];
                        bytes.copy_from_slice(&value[0..8]);
                        map.insert(address, u64::from_be_bytes(bytes));
                    }
                }
            }
        }
        map
    }

    pub fn save_appchain_config(&self, config: &AppChainConfig) -> Result<(), sled::Error> {
        let value = bincode::serialize(config)
            .map_err(|_| sled::Error::Unsupported("serialize appchain config failed".into()))?;
        self.appchain_configs
            .insert(config.id.to_be_bytes(), value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn delete_appchain_config(&self, chain_id: u32) -> Result<(), sled::Error> {
        self.appchain_configs.remove(chain_id.to_be_bytes())?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_all_appchain_configs(&self) -> Result<Vec<AppChainConfig>, String> {
        let mut configs = Vec::new();
        for item in self.appchain_configs.iter() {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let id_bytes: [u8; 4] = key
                .as_ref()
                .try_into()
                .map_err(|_| "AppChain config key must be exactly 4 bytes".to_string())?;
            let config = bincode::deserialize::<AppChainConfig>(&value)
                .map_err(|error| format!("invalid AppChain config: {error}"))?;
            if config.id != u32::from_be_bytes(id_bytes) {
                return Err(format!(
                    "AppChain config key does not match AppChain #{}",
                    config.id
                ));
            }
            configs.push(config);
        }
        configs.sort_by_key(|config| config.id);
        Ok(configs)
    }

    fn appchain_anchor_storage_key(anchor: &AnchoredState) -> Result<[u8; 32], sled::Error> {
        let value = bincode::serialize(anchor)
            .map_err(|_| sled::Error::Unsupported("serialize AppChain anchor key failed".into()))?;
        Ok(sha3::Sha3_256::digest(value).into())
    }

    pub fn save_appchain_anchor(&self, anchor: &AnchoredState) -> Result<(), sled::Error> {
        let value = bincode::serialize(anchor)
            .map_err(|_| sled::Error::Unsupported("serialize AppChain anchor failed".into()))?;
        let key: [u8; 32] = sha3::Sha3_256::digest(&value).into();
        self.appchain_anchors.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn delete_appchain_anchor(&self, anchor: &AnchoredState) -> Result<(), sled::Error> {
        let key = Self::appchain_anchor_storage_key(anchor)?;
        self.appchain_anchors.remove(key)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_all_appchain_anchors(&self) -> Result<Vec<AnchoredState>, String> {
        let mut anchors = Vec::new();
        for item in self.appchain_anchors.iter() {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let anchor = bincode::deserialize::<AnchoredState>(&value)
                .map_err(|error| format!("invalid AppChain anchor: {error}"))?;
            anchors.push(anchor);
        }
        anchors.sort_by_key(|anchor| (anchor.timestamp, anchor.chain_id, anchor.anchor_number));
        Ok(anchors)
    }

    pub fn save_transaction(&self, tx: &Transaction) -> Result<(), sled::Error> {
        let hash = tx.get_hash();
        let value = bincode::serialize(tx).unwrap();
        self.transactions.insert(hash, value)?;
        self.pending_transactions.remove(hash)?;
        self.nullifiers.insert(tx.nullifier, &hash)?;
        let next_nonce = tx.nonce.saturating_add(1);
        if self
            .get_account_nonce(&tx.sender)
            .map_or(true, |current| current < next_nonce)
        {
            self.save_account_nonce(&tx.sender, next_nonce)?;
        }
        self.db.flush()?;
        Ok(())
    }

    pub fn save_pending_proposal(
        &self,
        hash: &[u8; 32],
        proposal: &ValidatorJoinProposalData,
    ) -> Result<(), sled::Error> {
        let value = bincode::serialize(proposal)
            .map_err(|_| sled::Error::Unsupported("serialize pending proposal failed".into()))?;
        self.pending_proposals.insert(hash, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn delete_pending_proposal(&self, hash: &[u8; 32]) -> Result<(), sled::Error> {
        self.pending_proposals.remove(hash)?;
        self.db.flush()?;
        Ok(())
    }

    fn approval_index_key(record: &ValidatorApprovalRecord) -> [u8; 40] {
        let mut key = [0u8; 40];
        key[..8].copy_from_slice(&record.recorded_at.to_be_bytes());
        key[8..].copy_from_slice(&record.proposal_hash);
        key
    }

    pub fn save_approval_record(
        &self,
        record: &ValidatorApprovalRecord,
    ) -> Result<(), sled::Error> {
        let value = bincode::serialize(record)
            .map_err(|_| sled::Error::Unsupported("serialize approval record failed".into()))?;
        let index_key = Self::approval_index_key(record);
        let index_value = record.proposal_hash;
        self.approval_journal.insert(record.proposal_hash, value)?;
        self.approval_journal_index
            .insert(index_key, &index_value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_approval_record(
        &self,
        proposal_hash: &[u8; 32],
    ) -> Result<Option<ValidatorApprovalRecord>, String> {
        self.approval_journal
            .get(proposal_hash)
            .map_err(|error| format!("failed to read approval journal: {error}"))?
            .map(|value| {
                bincode::deserialize(&value).map_err(|error| {
                    format!(
                        "invalid approval journal record {}: {error}",
                        hex::encode(proposal_hash)
                    )
                })
            })
            .transpose()
    }

    fn rebuild_approval_journal_index(&self) -> Result<(), String> {
        self.approval_journal_index
            .clear()
            .map_err(|error| error.to_string())?;
        for item in self.approval_journal.iter() {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let proposal_hash: [u8; 32] = key
                .as_ref()
                .try_into()
                .map_err(|_| "approval journal key must be exactly 32 bytes".to_string())?;
            let record =
                bincode::deserialize::<ValidatorApprovalRecord>(&value).map_err(|error| {
                    format!(
                        "invalid approval record {}: {error}",
                        hex::encode(proposal_hash)
                    )
                })?;
            if record.proposal_hash != proposal_hash {
                return Err(format!(
                    "approval journal key does not match record {}",
                    hex::encode(proposal_hash)
                ));
            }
            self.approval_journal_index
                .insert(Self::approval_index_key(&record), &proposal_hash)
                .map_err(|error| error.to_string())?;
        }
        self.db.flush().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_all_approval_records(&self) -> Result<Vec<ValidatorApprovalRecord>, String> {
        let mut records = Vec::new();
        for item in self.approval_journal.iter() {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let proposal_hash: [u8; 32] = key
                .as_ref()
                .try_into()
                .map_err(|_| "approval journal key must be exactly 32 bytes".to_string())?;
            let record =
                bincode::deserialize::<ValidatorApprovalRecord>(&value).map_err(|error| {
                    format!(
                        "invalid approval record {}: {error}",
                        hex::encode(proposal_hash)
                    )
                })?;
            if record.proposal_hash != proposal_hash {
                return Err(format!(
                    "approval journal key does not match record {}",
                    hex::encode(proposal_hash)
                ));
            }
            records.push(record);
        }
        records.sort_by_key(|record| (record.recorded_at, record.proposal_hash));
        Ok(records)
    }

    pub fn get_approval_page(
        &self,
        after: Option<[u8; 40]>,
        limit: usize,
    ) -> Result<(usize, Vec<ValidatorApprovalRecord>, Option<[u8; 40]>), String> {
        if limit == 0 {
            return Err("approval page limit must be greater than zero".to_string());
        }

        let total = self.approval_journal.len();
        let start_key = after.map(|key| key.to_vec()).unwrap_or_default();
        let mut page = Vec::with_capacity(limit);
        let mut next_cursor = None;

        for item in self.approval_journal_index.range(start_key..) {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let index_key: [u8; 40] = key
                .as_ref()
                .try_into()
                .map_err(|_| "approval journal index key must be exactly 40 bytes".to_string())?;
            if after == Some(index_key) {
                continue;
            }
            let proposal_hash: [u8; 32] = value
                .as_ref()
                .try_into()
                .map_err(|_| "approval journal index value must be exactly 32 bytes".to_string())?;
            let raw_record = self
                .approval_journal
                .get(proposal_hash)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!(
                        "approval journal record {} is missing",
                        hex::encode(proposal_hash)
                    )
                })?;
            let record =
                bincode::deserialize::<ValidatorApprovalRecord>(&raw_record).map_err(|error| {
                    format!(
                        "invalid approval record {}: {error}",
                        hex::encode(proposal_hash)
                    )
                })?;
            if record.proposal_hash != proposal_hash
                || Self::approval_index_key(&record) != index_key
            {
                return Err(format!(
                    "approval journal index mismatch for {}",
                    hex::encode(proposal_hash)
                ));
            }

            if page.len() == limit {
                next_cursor = page.last().map(Self::approval_index_key);
                break;
            }
            page.push(record);
        }

        Ok((total, page, next_cursor))
    }

    pub fn save_approval_intent(&self, intent: &ApprovalIntentRecord) -> Result<(), String> {
        let value = bincode::serialize(intent)
            .map_err(|error| format!("failed to encode approval intent: {error}"))?;
        self.approval_intents
            .insert(intent.intent_id.as_bytes(), value)
            .map_err(|error| format!("failed to persist approval intent: {error}"))?;
        self.db
            .flush()
            .map_err(|error| format!("failed to flush approval intent: {error}"))?;
        Ok(())
    }

    pub fn get_approval_intent(
        &self,
        intent_id: &str,
    ) -> Result<Option<ApprovalIntentRecord>, String> {
        self.approval_intents
            .get(intent_id.as_bytes())
            .map_err(|error| format!("failed to read approval intent: {error}"))?
            .map(|value| {
                bincode::deserialize(&value)
                    .map_err(|error| format!("invalid approval intent {intent_id}: {error}"))
            })
            .transpose()
    }

    pub fn compare_and_swap_approval_intent(
        &self,
        expected: &ApprovalIntentRecord,
        next: &ApprovalIntentRecord,
    ) -> Result<bool, String> {
        if expected.intent_id != next.intent_id || expected.proposal_hash != next.proposal_hash {
            return Err("approval intent compare-and-swap identity mismatch".into());
        }
        let expected_bytes = bincode::serialize(expected)
            .map_err(|error| format!("failed to encode expected approval intent: {error}"))?;
        let next_bytes = bincode::serialize(next)
            .map_err(|error| format!("failed to encode next approval intent: {error}"))?;
        let result = self
            .approval_intents
            .compare_and_swap(
                expected.intent_id.as_bytes(),
                Some(expected_bytes),
                Some(next_bytes),
            )
            .map_err(|error| format!("failed to update approval intent: {error}"))?;
        if result.is_ok() {
            self.db
                .flush()
                .map_err(|error| format!("failed to flush approval intent update: {error}"))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn find_active_approval_intent(
        &self,
        proposal_hash: &[u8; 32],
    ) -> Result<Option<ApprovalIntentRecord>, String> {
        let mut active = None;
        for item in self.approval_intents.iter() {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let intent: ApprovalIntentRecord = bincode::deserialize(&value)
                .map_err(|error| format!("invalid approval intent: {error}"))?;
            if &intent.proposal_hash == proposal_hash
                && !matches!(
                    intent.stage,
                    crate::governance::approval::ApprovalIntentStage::Expired
                        | crate::governance::approval::ApprovalIntentStage::Rejected
                        | crate::governance::approval::ApprovalIntentStage::Approved
                        | crate::governance::approval::ApprovalIntentStage::Activated
                )
            {
                if active.is_some() {
                    return Err("multiple active approval intents exist for one proposal".into());
                }
                active = Some(intent);
            }
        }
        Ok(active)
    }

    fn approval_signature_key(intent_id: &str, owner_index: usize) -> Vec<u8> {
        format!("{intent_id}:{owner_index}").into_bytes()
    }

    pub fn save_approval_signature(
        &self,
        signature: &ApprovalSignatureRecord,
    ) -> Result<(), String> {
        let value = bincode::serialize(signature)
            .map_err(|error| format!("failed to encode approval signature: {error}"))?;
        self.approval_signatures
            .insert(
                Self::approval_signature_key(&signature.intent_id, signature.owner_index),
                value,
            )
            .map_err(|error| format!("failed to persist approval signature: {error}"))?;
        self.db
            .flush()
            .map_err(|error| format!("failed to flush approval signature: {error}"))?;
        Ok(())
    }

    pub fn save_approval_signature_if_absent(
        &self,
        signature: &ApprovalSignatureRecord,
    ) -> Result<bool, String> {
        let key = Self::approval_signature_key(&signature.intent_id, signature.owner_index);
        let value = bincode::serialize(signature)
            .map_err(|error| format!("failed to encode approval signature: {error}"))?;
        let inserted = self
            .approval_signatures
            .compare_and_swap(key, None as Option<Vec<u8>>, Some(value))
            .map_err(|error| format!("failed to reserve approval signature: {error}"))?
            .is_ok();
        if inserted {
            self.db
                .flush()
                .map_err(|error| format!("failed to flush approval signature: {error}"))?;
        }
        Ok(inserted)
    }

    pub fn get_approval_signatures(
        &self,
        intent_id: &str,
    ) -> Result<Vec<ApprovalSignatureRecord>, String> {
        let prefix = format!("{intent_id}:");
        let mut signatures = Vec::new();
        for item in self.approval_signatures.scan_prefix(prefix.as_bytes()) {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let signature: ApprovalSignatureRecord = bincode::deserialize(&value)
                .map_err(|error| format!("invalid approval signature for {intent_id}: {error}"))?;
            if signature.intent_id != intent_id {
                return Err("approval signature key does not match intent".into());
            }
            signatures.push(signature);
        }
        signatures.sort_by_key(|signature| signature.owner_index);
        Ok(signatures)
    }

    pub fn reserve_approval_nonce(
        &self,
        reservation: &ApprovalNonceReservation,
        now: u64,
    ) -> Result<bool, String> {
        let key = format!("{}:{}", reservation.sender, reservation.nonce);
        let value = bincode::serialize(reservation)
            .map_err(|error| format!("failed to encode nonce reservation: {error}"))?;
        loop {
            let existing = self
                .approval_nonce_reservations
                .get(key.as_bytes())
                .map_err(|error| format!("failed to read nonce reservation: {error}"))?;
            let result = match existing {
                None => self.approval_nonce_reservations.compare_and_swap(
                    key.as_bytes(),
                    None as Option<Vec<u8>>,
                    Some(value.clone()),
                ),
                Some(existing_bytes) => {
                    let existing_reservation: ApprovalNonceReservation =
                        bincode::deserialize(&existing_bytes).map_err(|error| {
                            format!("invalid approval nonce reservation: {error}")
                        })?;
                    if existing_reservation.expires_at > now {
                        return Ok(false);
                    }
                    self.approval_nonce_reservations.compare_and_swap(
                        key.as_bytes(),
                        Some(existing_bytes),
                        Some(value.clone()),
                    )
                }
            }
            .map_err(|error| format!("failed to reserve approval nonce: {error}"))?;
            if result.is_ok() {
                self.db
                    .flush()
                    .map_err(|error| format!("failed to flush nonce reservation: {error}"))?;
                return Ok(true);
            }
        }
    }

    pub fn release_approval_nonce(
        &self,
        sender: &str,
        nonce: u64,
        intent_id: &str,
    ) -> Result<bool, String> {
        let key = format!("{sender}:{nonce}");
        let Some(existing_bytes) = self
            .approval_nonce_reservations
            .get(key.as_bytes())
            .map_err(|error| format!("failed to read nonce reservation: {error}"))?
        else {
            return Ok(false);
        };
        let existing: ApprovalNonceReservation = bincode::deserialize(&existing_bytes)
            .map_err(|error| format!("invalid approval nonce reservation: {error}"))?;
        if existing.intent_id != intent_id {
            return Ok(false);
        }
        let removed = self
            .approval_nonce_reservations
            .compare_and_swap(key.as_bytes(), Some(existing_bytes), None::<Vec<u8>>)
            .map_err(|error| format!("failed to release nonce reservation: {error}"))?
            .is_ok();
        if removed {
            self.db
                .flush()
                .map_err(|error| format!("failed to flush nonce release: {error}"))?;
        }
        Ok(removed)
    }

    pub fn append_approval_audit(&self, event: &ApprovalAuditRecord) -> Result<(), String> {
        let value = bincode::serialize(event)
            .map_err(|error| format!("failed to encode approval audit event: {error}"))?;
        self.approval_audit
            .insert(event.event_id.as_bytes(), value)
            .map_err(|error| format!("failed to persist approval audit event: {error}"))?;
        self.db
            .flush()
            .map_err(|error| format!("failed to flush approval audit event: {error}"))?;
        Ok(())
    }

    pub fn has_live_approval_transaction(
        &self,
        proposal_hash: &[u8; 32],
        nonce: u64,
    ) -> Result<bool, String> {
        let matches = |transaction: &Transaction| {
            transaction.sender == crate::UltraBlockchain::SOVEREIGN_ADDR
                && transaction.nonce == nonce
                && matches!(
                    transaction.payload,
                    TransactionPayload::ValidatorApproval { proposal_hash: candidate }
                        if &candidate == proposal_hash
                )
        };
        for item in self.pending_transactions.iter() {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let transaction: Transaction = bincode::deserialize(&value).map_err(|error| {
                format!("invalid pending transaction during approval recovery: {error}")
            })?;
            if matches(&transaction) {
                return Ok(true);
            }
        }
        for item in self.transactions.iter() {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let transaction: Transaction = bincode::deserialize(&value).map_err(|error| {
                format!("invalid transaction during approval recovery: {error}")
            })?;
            if matches(&transaction) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn reap_expired_approval_intents(&self, now: u64) -> Result<usize, String> {
        let mut expired = 0;
        for item in self.approval_intents.iter() {
            let (_key, value) = item.map_err(|error| error.to_string())?;
            let intent: ApprovalIntentRecord = bincode::deserialize(&value)
                .map_err(|error| format!("invalid approval intent during cleanup: {error}"))?;
            if self.get_approval_record(&intent.proposal_hash)?.is_some() {
                let mut activated = intent.clone();
                activated.stage = crate::governance::approval::ApprovalIntentStage::Activated;
                let _ = self.compare_and_swap_approval_intent(&intent, &activated)?;
                continue;
            }
            if intent.expires_at > now
                || matches!(
                    intent.stage,
                    crate::governance::approval::ApprovalIntentStage::Expired
                        | crate::governance::approval::ApprovalIntentStage::Rejected
                        | crate::governance::approval::ApprovalIntentStage::Approved
                        | crate::governance::approval::ApprovalIntentStage::Activated
                )
            {
                continue;
            }
            if matches!(
                intent.stage,
                crate::governance::approval::ApprovalIntentStage::Finalizing
            ) && self.has_live_approval_transaction(&intent.proposal_hash, intent.nonce)?
            {
                continue;
            }
            let mut next = intent.clone();
            next.stage = crate::governance::approval::ApprovalIntentStage::Expired;
            if self.compare_and_swap_approval_intent(&intent, &next)? {
                let _ = self.release_approval_nonce(
                    crate::UltraBlockchain::SOVEREIGN_ADDR,
                    intent.nonce,
                    &intent.intent_id,
                )?;
                expired += 1;
            }
        }
        Ok(expired)
    }

    pub fn get_all_pending_proposals(
        &self,
    ) -> Result<HashMap<[u8; 32], ValidatorJoinProposalData>, String> {
        let mut proposals = HashMap::new();
        for item in self.pending_proposals.iter() {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let hash: [u8; 32] = key
                .as_ref()
                .try_into()
                .map_err(|_| "pending proposal key must be exactly 32 bytes".to_string())?;
            let proposal =
                bincode::deserialize::<ValidatorJoinProposalData>(&value).map_err(|error| {
                    format!("invalid pending proposal {}: {error}", hex::encode(hash))
                })?;
            proposals.insert(hash, proposal);
        }
        Ok(proposals)
    }

    pub fn save_vertex(&self, vertex: &MysticetiVertex) -> Result<(), sled::Error> {
        let value = bincode::serialize(vertex).unwrap();
        self.dag_vertices.insert(vertex.hash, value)?;
        Ok(())
    }

    pub fn get_vertex(&self, hash: &[u8; 32]) -> Option<MysticetiVertex> {
        if let Some(value) = self.dag_vertices.get(hash).ok().flatten() {
            return bincode::deserialize(&value).ok();
        }
        None
    }

    pub fn get_all_vertices(&self) -> Vec<MysticetiVertex> {
        let mut vertices = Vec::new();
        for item in self.dag_vertices.iter() {
            if let Ok((_, value)) = item {
                if let Ok(vertex) = bincode::deserialize::<MysticetiVertex>(&value) {
                    vertices.push(vertex);
                }
            }
        }
        vertices
    }

    pub fn save_validator(&self, validator: &ValidatorInfo) -> Result<(), sled::Error> {
        let value = bincode::serialize(validator)
            .map_err(|_| sled::Error::Unsupported("serialize validator failed".into()))?;
        self.validators.insert(&validator.public_key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn delete_validator(&self, public_key: &[u8]) -> Result<(), sled::Error> {
        self.validators.remove(public_key)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_all_validators(&self) -> Result<Vec<ValidatorInfo>, String> {
        let mut validators = Vec::new();
        for item in self.validators.iter() {
            let (key, value) = item.map_err(|error| error.to_string())?;
            let validator = bincode::deserialize::<ValidatorInfo>(&value)
                .map_err(|error| format!("invalid validator record: {error}"))?;
            if key.as_ref() != validator.public_key.as_slice() {
                return Err("validator storage key does not match public key".to_string());
            }
            validators.push(validator);
        }
        Ok(validators)
    }

    pub fn replace_validators(
        &self,
        validators: &HashMap<Vec<u8>, ValidatorInfo>,
    ) -> Result<(), sled::Error> {
        let mut batch = Batch::default();
        for validator in validators.values() {
            let value = bincode::serialize(validator)
                .map_err(|_| sled::Error::Unsupported("serialize validator failed".into()))?;
            batch.insert(validator.public_key.as_slice(), value);
        }
        self.validators.clear()?;
        self.validators.apply_batch(batch)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn save_validator_stats(&self, id: u64, stats: &ValidatorStats) -> Result<(), sled::Error> {
        let key = id.to_be_bytes();
        let value = bincode::serialize(stats).unwrap();
        self.validator_stats.insert(key, value)?;
        Ok(())
    }

    pub fn get_all_validator_stats(&self) -> HashMap<u64, ValidatorStats> {
        let mut map = HashMap::new();
        for item in self.validator_stats.iter() {
            if let Ok((key, value)) = item {
                let mut id_bytes = [0u8; 8];
                id_bytes.copy_from_slice(&key[0..8]);
                let id = u64::from_be_bytes(id_bytes);
                if let Ok(stats) = bincode::deserialize::<ValidatorStats>(&value) {
                    map.insert(id, stats);
                }
            }
        }
        map
    }

    pub fn clear(&self) -> Result<(), sled::Error> {
        self.blocks.clear()?;
        for shard in &self.state_shards {
            shard.clear()?;
        }
        for shard in &self.trie_shards {
            shard.clear()?;
        }
        self.transactions.clear()?;
        self.pending_transactions.clear()?;
        self.nullifiers.clear()?;
        self.account_nonces.clear()?;
        self.validators.clear()?;
        self.checkpoints.clear()?;
        self.dag_vertices.clear()?;
        self.validator_stats.clear()?;
        self.pending_proposals.clear()?;
        self.approval_journal.clear()?;
        self.approval_journal_index.clear()?;
        self.approval_intents.clear()?;
        self.approval_signatures.clear()?;
        self.approval_audit.clear()?;
        self.approval_nonce_reservations.clear()?;
        self.auth_challenges.clear()?;
        self.auth_sessions.clear()?;
        self.appchain_configs.clear()?;
        self.appchain_anchors.clear()?;
        self.supply_corrections.clear()?;
        self.move_modules.clear()?;
        self.move_resources.clear()?;
        Ok(())
    }

    pub fn flush(&self) -> Result<(), sled::Error> {
        self.db.flush()?;
        Ok(())
    }
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            blocks: self.blocks.clone(),
            transactions: self.transactions.clone(),
            pending_transactions: self.pending_transactions.clone(),
            nullifiers: self.nullifiers.clone(),
            account_nonces: self.account_nonces.clone(),
            validators: self.validators.clone(),
            checkpoints: self.checkpoints.clone(),
            dag_vertices: self.dag_vertices.clone(),
            validator_stats: self.validator_stats.clone(),
            pending_proposals: self.pending_proposals.clone(),
            approval_journal: self.approval_journal.clone(),
            approval_journal_index: self.approval_journal_index.clone(),
            approval_intents: self.approval_intents.clone(),
            approval_signatures: self.approval_signatures.clone(),
            approval_audit: self.approval_audit.clone(),
            approval_nonce_reservations: self.approval_nonce_reservations.clone(),
            auth_challenges: self.auth_challenges.clone(),
            auth_sessions: self.auth_sessions.clone(),
            appchain_configs: self.appchain_configs.clone(),
            appchain_anchors: self.appchain_anchors.clone(),
            supply_corrections: self.supply_corrections.clone(),
            move_modules: self.move_modules.clone(),
            move_resources: self.move_resources.clone(),
            fhe_keys: self.fhe_keys.clone(),
            state_shards: self.state_shards.clone(),
            trie_shards: self.trie_shards.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{Arc, Barrier},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn malformed_pending_proposal_fails_closed() {
        let path = format!("test_db_storage_pending_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let result = {
            let storage = Storage::new(&path).expect("storage should open");
            storage
                .pending_proposals
                .insert([0u8; 32], b"not-a-bincode-record")
                .expect("test record should insert");
            storage.get_all_pending_proposals()
        };
        assert!(result.is_err(), "corrupt governance state must fail closed");
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn malformed_validator_record_fails_closed() {
        let path = format!("test_db_storage_validator_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let result = {
            let storage = Storage::new(&path).expect("storage should open");
            storage
                .validators
                .insert([1u8, 2, 3], b"not-a-validator-record")
                .expect("test record should insert");
            storage.get_all_validators()
        };
        assert!(result.is_err(), "corrupt validator state must fail closed");
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn malformed_approval_record_fails_closed() {
        let path = format!("test_db_storage_approval_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let result = {
            let storage = Storage::new(&path).expect("storage should open");
            storage
                .approval_journal
                .insert([4u8; 32], b"not-an-approval-record")
                .expect("test record should insert");
            storage.get_all_approval_records()
        };
        assert!(result.is_err(), "corrupt approval state must fail closed");
        let _ = fs::remove_dir_all(&path);
    }

    fn test_approval_record(id: u8, recorded_at: u64) -> ValidatorApprovalRecord {
        let proposal_hash = [id; 32];
        ValidatorApprovalRecord {
            proposal_hash,
            approval_transaction: Transaction {
                sender: "sovereign".to_string(),
                sender_public_key: vec![],
                recipient: "governance".to_string(),
                amount: 0,
                signature: vec![],
                zk_proof: vec![],
                nullifier: [id; 32],
                timestamp: recorded_at,
                fee: 0,
                nonce: 0,
                gas_limit: 1_000_000,
                gas_price: 1,
                proof_type: crate::ProofType::Ownership,
                payload: crate::TransactionPayload::ValidatorApproval { proposal_hash },
                chain_id: 0,
                version: 3,
            },
            proposal: ValidatorJoinProposalData {
                public_key: vec![id],
                metadata: format!("validator-{id}"),
                proposer: format!("proposer-{id}"),
                timestamp: recorded_at,
            },
            activated_validator: ValidatorInfo {
                public_key: vec![id],
                weight: 1,
                is_active: true,
                joined_at: recorded_at,
                last_epoch: 0,
                stake: 1_000,
                rewards: 0,
                slash_count: 0,
            },
            recorded_at,
        }
    }

    #[test]
    fn appchain_registry_records_round_trip_through_storage() {
        let path = format!("test_db_storage_appchain_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let config = AppChainConfig {
            id: 7,
            name: "Test AppChain".to_string(),
            owner: "Test Owner".to_string(),
            account_address: crate::appchain::derive_appchain_treasury_address(7),
            genesis_root: [1u8; 32],
            anchor_fee: 1_000,
            anchor_spend: 1_000,
            anchor_count: 1,
            latest_anchor_at: Some(42),
            latest_state_root: Some("a".repeat(64)),
        };
        let anchor = AnchoredState {
            chain_id: 7,
            anchor_number: 1,
            state_root: "a".repeat(64),
            proof: "test-fixture".to_string(),
            timestamp: 42,
            fee_charged: 1_000,
            is_test: true,
        };
        {
            let storage = Storage::new(&path).expect("storage should open");
            storage.save_appchain_config(&config).unwrap();
            storage.save_appchain_anchor(&anchor).unwrap();
        }
        let storage = Storage::new(&path).expect("storage should reopen");
        assert_eq!(storage.get_all_appchain_configs().unwrap(), vec![config]);
        assert_eq!(storage.get_all_appchain_anchors().unwrap(), vec![anchor]);
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn approval_page_is_stably_sorted_and_bounded() {
        let path = format!("test_db_storage_page_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let storage = Storage::new(&path).expect("storage should open");
        storage
            .save_approval_record(&test_approval_record(1, 20))
            .unwrap();
        storage
            .save_approval_record(&test_approval_record(2, 10))
            .unwrap();
        storage
            .save_approval_record(&test_approval_record(3, 30))
            .unwrap();

        let first_cursor = {
            let (_, first_page, next_cursor) = storage.get_approval_page(None, 1).unwrap();
            assert_eq!(first_page.len(), 1);
            assert_eq!(first_page[0].proposal_hash, [2u8; 32]);
            next_cursor.expect("first page should have a cursor")
        };
        let (total, page, next_cursor) = storage.get_approval_page(Some(first_cursor), 1).unwrap();
        assert_eq!(total, 3);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].proposal_hash, [1u8; 32]);
        assert!(next_cursor.is_some());

        let (_, final_page, final_cursor) = storage.get_approval_page(next_cursor, 10).unwrap();
        assert_eq!(final_page.len(), 1);
        assert_eq!(final_page[0].proposal_hash, [3u8; 32]);
        assert!(final_cursor.is_none());
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    fn temporary_concurrency_path(label: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        format!("test_db_storage_{label}_{}_{}", std::process::id(), nonce)
    }

    fn test_approval_intent(
        intent_id: &str,
        stage: crate::governance::approval::ApprovalIntentStage,
    ) -> ApprovalIntentRecord {
        let draft = crate::governance::approval::ApprovalDraft {
            proposal_hash: "11".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 7,
            nullifier: vec![0x22; crate::governance::approval::NULLIFIER_BYTES],
            version: crate::UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let digest: [u8; 32] = crate::governance::approval::canonical_approval_message(&draft)
            .expect("test approval draft should hash")
            .try_into()
            .expect("approval digest should be 32 bytes");
        ApprovalIntentRecord {
            intent_id: intent_id.to_string(),
            proposal_hash: [0x11; 32],
            timestamp: draft.timestamp,
            nonce: draft.nonce,
            nullifier: [0x22; 32],
            digest,
            created_by_session_hash: [0x33; 32],
            expires_at: 1_785_184_000,
            stage,
        }
    }

    fn test_approval_signature(intent_id: &str) -> ApprovalSignatureRecord {
        ApprovalSignatureRecord {
            intent_id: intent_id.to_string(),
            owner_index: 0,
            owner_address: "owner-0".to_string(),
            public_key: vec![0x44; 2_592],
            signature: vec![0x55; crate::governance::approval::PARTIAL_SIGNATURE_BYTES],
            recorded_at: 1_785_183_488,
        }
    }

    #[test]
    fn approval_nonce_reservation_has_one_winner_under_contention() {
        let path = temporary_concurrency_path("nonce");
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let barrier = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for index in 0..8 {
            let storage = storage.clone();
            let barrier = barrier.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                let reservation = ApprovalNonceReservation {
                    sender: crate::UltraBlockchain::SOVEREIGN_ADDR.to_string(),
                    nonce: 42,
                    intent_id: format!("nonce-race-{index}"),
                    expires_at: 2_000,
                };
                storage
                    .reserve_approval_nonce(&reservation, 1_000)
                    .expect("nonce reservation should not fail")
            }));
        }

        let winners = workers
            .into_iter()
            .map(|worker| worker.join().expect("nonce worker should not panic"))
            .filter(|inserted| *inserted)
            .count();
        assert_eq!(
            winners, 1,
            "exactly one concurrent nonce reservation may win"
        );

        let competing_reservation = ApprovalNonceReservation {
            sender: crate::UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            nonce: 42,
            intent_id: "nonce-race-afterward".to_string(),
            expires_at: 2_000,
        };
        assert!(!storage
            .reserve_approval_nonce(&competing_reservation, 1_000)
            .expect("existing nonce reservation should be readable"));
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn approval_signature_insert_is_idempotent_under_contention() {
        let path = temporary_concurrency_path("signature");
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let barrier = Arc::new(Barrier::new(8));
        let signature = test_approval_signature("signature-race");
        let mut workers = Vec::new();
        for _ in 0..8 {
            let storage = storage.clone();
            let barrier = barrier.clone();
            let signature = signature.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                storage
                    .save_approval_signature_if_absent(&signature)
                    .expect("signature insertion should not fail")
            }));
        }

        let winners = workers
            .into_iter()
            .map(|worker| worker.join().expect("signature worker should not panic"))
            .filter(|inserted| *inserted)
            .count();
        assert_eq!(winners, 1, "exactly one signature insertion may win");
        let signatures = storage
            .get_approval_signatures("signature-race")
            .expect("signature record should be readable");
        assert_eq!(signatures, vec![signature]);
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn approval_intent_compare_and_swap_has_one_transition_winner() {
        let path = temporary_concurrency_path("intent-cas");
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let initial = test_approval_intent(
            "intent-cas-race",
            crate::governance::approval::ApprovalIntentStage::Created,
        );
        let mut next = initial.clone();
        next.stage = crate::governance::approval::ApprovalIntentStage::Signing;
        storage
            .save_approval_intent(&initial)
            .expect("initial approval intent should persist");

        let barrier = Arc::new(Barrier::new(8));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let storage = storage.clone();
            let barrier = barrier.clone();
            let initial = initial.clone();
            let next = next.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                storage
                    .compare_and_swap_approval_intent(&initial, &next)
                    .expect("intent CAS should not fail")
            }));
        }

        let winners = workers
            .into_iter()
            .map(|worker| worker.join().expect("intent worker should not panic"))
            .filter(|updated| *updated)
            .count();
        assert_eq!(winners, 1, "exactly one intent transition may win");
        assert_eq!(
            storage
                .get_approval_intent("intent-cas-race")
                .expect("intent should be readable")
                .expect("intent should exist")
                .stage,
            crate::governance::approval::ApprovalIntentStage::Signing
        );
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }
}
