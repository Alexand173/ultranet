// ============================================================
// AI GOVERNANCE - AUTONOMOUS PROTOCOL ADJUSTMENT
// ============================================================
// Analyzes chain metrics and adjusts economic parameters
// to ensure 100-year longevity.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Blocks between halvings: ten years at the 10 s block target
/// (10 × 365.25 days × 86,400 s ÷ 10 s = 31,557,600). This is the single definition of that
/// number; the reward schedule, the supply cap and `genesis.json` all derive from it.
pub const HALVING_INTERVAL_BLOCKS: u64 = 31_557_600;

#[cfg(test)]
mod emission_tests {
    use super::*;

    fn reward(base: u64, blocks: u64) -> u64 {
        AIGovernor::new().predict_optimal_reward(base, blocks)
    }

    /// The regression that matters: the base must come from the argument.
    #[test]
    fn the_reward_uses_the_base_it_is_given() {
        assert_eq!(reward(500_000, 0), 500_000);
        assert_eq!(reward(1_000_000, 0), 1_000_000, "the argument must not be ignored");
        assert_eq!(reward(500_000, HALVING_INTERVAL_BLOCKS), 250_000);
        assert_eq!(reward(500_000, HALVING_INTERVAL_BLOCKS * 2), 125_000);
        // 500,000 >> 19 is 0, so the schedule ends in era 19.
        assert_eq!(reward(500_000, HALVING_INTERVAL_BLOCKS * 19), 0);
        // The old hardcoded schedule would have produced 50 here, not 500,000.
        assert_ne!(reward(500_000, 0), 50);
    }

    /// A halving interval of 31,557,600 blocks is exactly ten years at 10 s blocks.
    #[test]
    fn one_halving_interval_is_ten_years() {
        let seconds = HALVING_INTERVAL_BLOCKS * 10;
        let years = seconds as f64 / (365.25 * 86_400.0);
        assert!((years - 10.0).abs() < 1e-9, "{years} years");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainMetrics {
    pub avg_block_time: f64,
    pub avg_gas_price: u64,
    pub active_validators: usize,
    pub transaction_density: f64,
}

pub struct AIGovernor {
    pub history: VecDeque<ChainMetrics>,
    pub max_history: usize,
    pub sustainability_score: f64,
}

impl AIGovernor {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(100),
            max_history: 100,
            sustainability_score: 100.0,
        }
    }

    pub fn record_metrics(&mut self, metrics: ChainMetrics) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(metrics);
        self.update_sustainability_score();
    }

    /// Izračunava "Longevity Index" za narednih 100 godina.
    /// Balansira opterećenje, decentralizaciju i ekonomsku stabilnost.
    fn update_sustainability_score(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let avg_density: f64 = self
            .history
            .iter()
            .map(|m| m.transaction_density)
            .sum::<f64>()
            / self.history.len() as f64;
        let avg_validators = self
            .history
            .iter()
            .map(|m| m.active_validators)
            .sum::<usize>() as f64
            / self.history.len() as f64;

        // Kazni za preveliku gustinu (rizik od spama) ili premalo validatora
        let density_penalty = if avg_density > 0.8 {
            (avg_density - 0.8) * 50.0
        } else {
            0.0
        };
        let validator_bonus = (avg_validators / 5.0) * 10.0;

        self.sustainability_score = (100.0 - density_penalty + validator_bonus).clamp(0.0, 100.0);
    }

    /// EMA (Exponential Moving Average) baziran "AI" za predviđanje parametara
    pub fn predict_optimal_difficulty(&self, current: u64) -> u64 {
        if self.history.is_empty() {
            return current;
        }

        let avg_time: f64 =
            self.history.iter().map(|m| m.avg_block_time).sum::<f64>() / self.history.len() as f64;

        // Century Target: 10 seconds per block
        if avg_time < 9.0 {
            current + 1 // Too fast
        } else if avg_time > 11.0 {
            current.saturating_sub(1) // Too slow
        } else {
            current
        }
    }

    /// Dynamic AI-driven emission (100-year target)
    ///
    /// `current` is the base block reward the caller wants halved — `UltraBlockchain::GENESIS_REWARD`.
    /// This function used to ignore it and halve a literal `50`, which meant the constant and
    /// `genesis.json` were decorative: changing the emission anywhere had no effect on the chain. The
    /// base must come from the caller; `the_reward_uses_the_base_it_is_given` pins that.
    pub fn predict_optimal_reward(&self, current: u64, total_blocks: u64) -> u64 {
        let era = total_blocks / HALVING_INTERVAL_BLOCKS;

        // Halve the caller's base, never a literal.
        let base_reward = if era >= 64 { 0 } else { current >> era };

        if self.history.is_empty() {
            return base_reward as u64;
        }

        let density: f64 = self
            .history
            .iter()
            .map(|m| m.transaction_density)
            .sum::<f64>()
            / self.history.len() as f64;
        if density > 0.95 {
            (base_reward as f64 * 0.8) as u64 // Anti-inflationary pressure
        } else {
            base_reward as u64
        }
    }

    /// Signalizira da li shard treba da se podeli (Dynamic Resharding)
    pub fn should_split_shard(&self, shard_id: u8) -> bool {
        if self.history.len() < 50 {
            return false;
        }
        let avg_density: f64 = self
            .history
            .iter()
            .map(|m| m.transaction_density)
            .sum::<f64>()
            / self.history.len() as f64;

        // Ako je opterećenje preko 95% duže vreme
        avg_density > 0.95 && shard_id < 128 // Limit na 256 shardova
    }
}
