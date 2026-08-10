// ============================================================
// L3 APPCHAIN REGISTRY - RUST IMPLEMENTATION
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppChainConfig {
    pub id: u32,
    pub name: String,
    pub owner: String,
    pub genesis_root: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchoredState {
    pub chain_id: u32,
    pub state_root: String,
    pub proof: String,
    pub timestamp: u64,
}

pub struct AppChainRegistry {
    pub active_chains: HashMap<u32, AppChainConfig>,
    pub anchoring_history: Vec<AnchoredState>,
}

impl AppChainRegistry {
    pub fn new() -> Self {
        Self {
            active_chains: HashMap::new(),
            anchoring_history: Vec::new(),
        }
    }

    pub fn register_chain(&mut self, config: AppChainConfig) {
        println!(
            "🚀 Registry: AppChain #{} ('{}') registered on L1!",
            config.id, config.name
        );
        self.active_chains.insert(config.id, config);
    }

    pub fn record_anchor(&mut self, anchor: AnchoredState) {
        println!(
            "⚓ Registry: Recorded anchor for AppChain #{}",
            anchor.chain_id
        );
        self.anchoring_history.push(anchor);
        if self.anchoring_history.len() > 100 {
            self.anchoring_history.remove(0);
        }
    }
}
