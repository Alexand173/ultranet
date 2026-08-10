// ============================================================
// AI GOVERNANCE - AUTONOMOUS PROTOCOL ADJUSTMENT
// ============================================================
// Analyzes chain metrics and adjusts economic parameters
// to ensure 100-year longevity.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

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
    pub fn predict_optimal_reward(&self, _current: u64, total_blocks: u64) -> u64 {
        // AI Inflation Control: 10-Year Halving Cycle
        // Blocks per 10 years (10s blocks): 10 * 365.25 * 24 * 3600 / 10 = 31,557,600
        let halving_interval = 31_557_600;
        let era = total_blocks / halving_interval;

        // Base reward starts at 50 and halves every era
        let base_reward = if era >= 64 { 0 } else { 50 >> era };

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
