// ============================================================
// CROSS-SHARD MESSENGER - INTER-SHARD ATOMICITY
// ============================================================

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossShardMessage {
    pub source_shard: u8,
    pub target_shard: u8,
    pub payload: Vec<u8>,
    pub source_block_height: u64,
    pub merkle_proof: Vec<u8>,
}

pub struct CrossShardMessenger {
    pub pending_inbox: Vec<CrossShardMessage>,
}

impl CrossShardMessenger {
    pub fn new() -> Self {
        Self {
            pending_inbox: Vec::new(),
        }
    }

    /// Generiše poruku za drugi shard (npr. prenos balansa)
    pub fn create_transfer_message(
        &self,
        source: u8,
        target: u8,
        amount: u64,
        recipient: String,
    ) -> CrossShardMessage {
        let payload = bincode::serialize(&(amount, recipient)).unwrap();
        CrossShardMessage {
            source_shard: source,
            target_shard: target,
            payload,
            source_block_height: 0, // Biće popunjeno pri rudarenju
            merkle_proof: vec![],
        }
    }

    /// Verifikuje poruku na strani destinacije
    pub fn verify_message(&self, msg: &CrossShardMessage, source_root: [u8; 32]) -> bool {
        // U realnom sistemu ovde ide Merkle Proof verifikacija protiv source_root-a
        !msg.merkle_proof.is_empty() || source_root != [0u8; 32]
    }
}
