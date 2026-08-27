// ============================================================
// L3 APPCHAIN REGISTRY - RUST IMPLEMENTATION
// ============================================================

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::HashMap;

/// AppChain anchor fee in base units of $ULTRA.
pub const DEFAULT_APPCHAIN_ANCHOR_FEE: u64 = 1_000;

/// New AppChain treasuries start empty. They must be funded with a real L1
/// transfer before an anchor can be charged.
pub const DEFAULT_APPCHAIN_INITIAL_BALANCE: u64 = 0;

const APPCHAIN_TREASURY_DOMAIN: &[u8] = b"UltraNet/AppChain/treasury/v1";

/// Derive a stable, dedicated protocol treasury address for an AppChain.
///
/// The address is an on-chain account identifier, not a private-key export.
/// Anyone can fund it through a normal L1 transfer; only the node's AppChain
/// anchor accounting path can debit it for an anchor fee.
pub fn derive_appchain_treasury_address(chain_id: u32) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(APPCHAIN_TREASURY_DOMAIN);
    hasher.update(chain_id.to_le_bytes());
    hex::encode(hasher.finalize())
}

fn default_anchor_fee() -> u64 {
    DEFAULT_APPCHAIN_ANCHOR_FEE
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppChainConfig {
    pub id: u32,
    pub name: String,
    /// Human-readable owner address or alias displayed by the operator UI.
    pub owner: String,
    /// Dedicated canonical L1 treasury address for this AppChain.
    pub account_address: String,
    pub genesis_root: [u8; 32],
    /// Cost charged from the AppChain treasury per anchor, in base units.
    #[serde(default = "default_anchor_fee")]
    pub anchor_fee: u64,
    /// Cumulative protocol debits charged from the treasury, in base units.
    #[serde(default)]
    pub anchor_spend: u64,
    /// Number of successfully recorded anchors.
    #[serde(default)]
    pub anchor_count: u64,
    /// Timestamp of the latest recorded anchor.
    #[serde(default)]
    pub latest_anchor_at: Option<u64>,
    /// State root of the latest recorded anchor.
    #[serde(default)]
    pub latest_state_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnchoredState {
    pub chain_id: u32,
    /// Monotonic anchor number within the AppChain.
    #[serde(default)]
    pub anchor_number: u64,
    pub state_root: String,
    /// Serialized server-generated state proof envelope.
    pub proof: String,
    pub timestamp: u64,
    /// Fee charged from the real treasury address in base units.
    #[serde(default)]
    pub fee_charged: u64,
    /// True when invoked through the development-only endpoint.
    #[serde(default)]
    pub is_test: bool,
}

pub struct AppChainRegistry {
    pub active_chains: HashMap<u32, AppChainConfig>,
    pub anchoring_history: Vec<AnchoredState>,
}

impl Default for AppChainRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AppChainRegistry {
    pub fn new() -> Self {
        Self {
            active_chains: HashMap::new(),
            anchoring_history: Vec::new(),
        }
    }

    pub fn from_persisted(
        chains: Vec<AppChainConfig>,
        anchoring_history: Vec<AnchoredState>,
    ) -> Self {
        let active_chains = chains.into_iter().map(|chain| (chain.id, chain)).collect();
        Self {
            active_chains,
            anchoring_history,
        }
    }

    pub fn next_chain_id(&self) -> Result<u32, String> {
        self.active_chains
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "AppChain ID space is exhausted".to_string())
    }

    pub fn register_chain(&mut self, config: AppChainConfig) -> Result<(), String> {
        if self.active_chains.contains_key(&config.id) {
            return Err(format!("AppChain #{} already exists", config.id));
        }
        println!(
            "🚀 Registry: AppChain #{} ('{}') registered with treasury {}",
            config.id, config.name, config.account_address
        );
        self.active_chains.insert(config.id, config);
        Ok(())
    }

    pub fn get_chain(&self, chain_id: u32) -> Option<&AppChainConfig> {
        self.active_chains.get(&chain_id)
    }

    /// Calculate a priced anchor without mutating the registry.
    ///
    /// `treasury_balance` is the current spendable balance of the dedicated
    /// L1 treasury, after previous protocol debits. Persistence should succeed
    /// before the returned values are applied with `apply_anchor`.
    pub fn preview_anchor(
        &self,
        chain_id: u32,
        treasury_balance: u64,
        state_root: String,
        proof: String,
        timestamp: u64,
        is_test: bool,
    ) -> Result<(AppChainConfig, AnchoredState), String> {
        let chain = self
            .active_chains
            .get(&chain_id)
            .ok_or_else(|| format!("AppChain #{chain_id} was not found"))?;
        if treasury_balance < chain.anchor_fee {
            return Err(format!(
                "AppChain treasury {} has insufficient balance: current {}, required {}",
                chain.account_address, treasury_balance, chain.anchor_fee
            ));
        }

        let anchor_count = chain
            .anchor_count
            .checked_add(1)
            .ok_or_else(|| format!("AppChain #{chain_id} anchor count overflowed"))?;
        let anchor_spend = chain
            .anchor_spend
            .checked_add(chain.anchor_fee)
            .ok_or_else(|| format!("AppChain #{chain_id} anchor spend overflowed"))?;

        let mut updated = chain.clone();
        updated.anchor_spend = anchor_spend;
        updated.anchor_count = anchor_count;
        updated.latest_anchor_at = Some(timestamp);
        updated.latest_state_root = Some(state_root.clone());

        let anchor = AnchoredState {
            chain_id,
            anchor_number: anchor_count,
            state_root,
            proof,
            timestamp,
            fee_charged: chain.anchor_fee,
            is_test,
        };
        Ok((updated, anchor))
    }

    pub fn apply_anchor(
        &mut self,
        updated: AppChainConfig,
        anchor: AnchoredState,
    ) -> Result<(), String> {
        if updated.id != anchor.chain_id {
            return Err("AppChain config and anchor IDs do not match".to_string());
        }
        if !self.active_chains.contains_key(&anchor.chain_id) {
            return Err(format!("AppChain #{} was not found", anchor.chain_id));
        }
        println!(
            "⚓ Registry: Recorded anchor #{} for AppChain #{}; charged {} base units",
            anchor.anchor_number, anchor.chain_id, anchor.fee_charged
        );
        self.active_chains.insert(updated.id, updated);
        self.anchoring_history.push(anchor);
        Ok(())
    }

    /// Apply an already-priced anchor for internal callers and tests.
    pub fn record_anchor(
        &mut self,
        anchor: AnchoredState,
        treasury_balance: u64,
    ) -> Result<(), String> {
        let (updated, generated) = self.preview_anchor(
            anchor.chain_id,
            treasury_balance,
            anchor.state_root,
            anchor.proof,
            anchor.timestamp,
            anchor.is_test,
        )?;
        self.apply_anchor(updated, generated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_chain(balance: u64) -> (AppChainConfig, u64) {
        (
            AppChainConfig {
                id: 1,
                name: "TestChain".to_string(),
                owner: "test-owner".to_string(),
                account_address: derive_appchain_treasury_address(1),
                genesis_root: [0; 32],
                anchor_fee: 100,
                anchor_spend: 0,
                anchor_count: 0,
                latest_anchor_at: None,
                latest_state_root: None,
            },
            balance,
        )
    }

    #[test]
    fn treasury_address_is_stable_and_canonical() {
        let address = derive_appchain_treasury_address(7);
        assert_eq!(address, derive_appchain_treasury_address(7));
        assert_eq!(address.len(), 64);
        assert!(crate::UltraBlockchain::is_valid_address(&address));
        assert_ne!(address, derive_appchain_treasury_address(8));
    }

    #[test]
    fn anchoring_charges_the_configured_fee_and_updates_counters() {
        let mut registry = AppChainRegistry::new();
        let (chain, treasury_balance) = test_chain(250);
        registry.register_chain(chain).unwrap();

        registry
            .record_anchor(
                AnchoredState {
                    chain_id: 1,
                    anchor_number: 0,
                    state_root: "root-1".to_string(),
                    proof: "server-proof".to_string(),
                    timestamp: 42,
                    fee_charged: 0,
                    is_test: false,
                },
                treasury_balance,
            )
            .unwrap();

        let chain = registry.get_chain(1).unwrap();
        assert_eq!(chain.anchor_spend, 100);
        assert_eq!(chain.anchor_count, 1);
        assert_eq!(chain.latest_anchor_at, Some(42));
        assert_eq!(registry.anchoring_history.len(), 1);
    }

    #[test]
    fn anchoring_rejects_an_insufficient_treasury_balance() {
        let mut registry = AppChainRegistry::new();
        let (chain, treasury_balance) = test_chain(99);
        registry.register_chain(chain).unwrap();

        let error = registry
            .record_anchor(
                AnchoredState {
                    chain_id: 1,
                    anchor_number: 0,
                    state_root: "root-1".to_string(),
                    proof: "server-proof".to_string(),
                    timestamp: 42,
                    fee_charged: 0,
                    is_test: true,
                },
                treasury_balance,
            )
            .unwrap_err();

        assert!(error.contains("insufficient balance"));
        assert_eq!(registry.get_chain(1).unwrap().anchor_count, 0);
        assert!(registry.anchoring_history.is_empty());
    }
}
