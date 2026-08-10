// ============================================================
// BLOCK-STM - PARALLEL EXECUTION ENGINE
// ============================================================

use crate::multi_version_memory::MultiVersionMemory;
use crate::Transaction;
use parking_lot::RwLock;
use serde::Serialize; // ← DODAJ NA VRH FAJLA!
use std::collections::{HashMap, HashSet};
use std::sync::Arc; // ← Transaction iz main.rs

/// Rezultat izvršavanja jedne transakcije
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    pub tx_hash: [u8; 32],
    pub success: bool,
    pub gas_used: u64,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub version: u64,
}

/// Block-STM - optimističko paralelno izvršavanje
pub struct BlockSTM {
    pub memory: MultiVersionMemory,
    pub max_retries: u32,
    pub stats: Arc<RwLock<STMStats>>,
}

#[derive(Default, Clone, Debug, Serialize)] // ← DODAJ Serialize!
pub struct STMStats {
    pub total_executions: u64,
    pub conflicts: u64,
    pub retries: u64,
    pub peak_parallelism: u64,
}

impl BlockSTM {
    pub fn new() -> Self {
        Self {
            memory: MultiVersionMemory::new(),
            max_retries: 10,
            stats: Arc::new(RwLock::new(STMStats::default())),
        }
    }

    /// Paralelno izvršavanje transakcija
    pub fn execute_parallel(&self, transactions: &[Transaction]) -> Vec<ExecutionResult> {
        let start_version = self.memory.current_version();
        self.memory.new_version();

        let mut results: Vec<ExecutionResult> = Vec::with_capacity(transactions.len());
        let mut retry_counts: Vec<u32> = vec![0; transactions.len()];

        for tx in transactions {
            let result = self.execute_tx_optimistically(tx, start_version);
            results.push(result);
        }

        let mut round = 0;
        while round < self.max_retries {
            let conflicts = self.detect_conflicts(&results);

            if conflicts.is_empty() {
                break;
            }

            {
                let mut stats = self.stats.write();
                stats.conflicts += conflicts.len() as u64;
                stats.retries += conflicts.len() as u64;
            }

            for idx in conflicts {
                retry_counts[idx] += 1;
                if retry_counts[idx] <= self.max_retries {
                    let tx = &transactions[idx];
                    self.memory.rollback_to(start_version);
                    results[idx] = self.execute_tx_optimistically(tx, start_version);
                }
            }

            round += 1;
        }

        {
            let mut stats = self.stats.write();
            stats.total_executions += transactions.len() as u64;
            if transactions.len() as u64 > stats.peak_parallelism {
                stats.peak_parallelism = transactions.len() as u64;
            }
        }

        results
    }

    /// Optimističko izvršavanje jedne transakcije
    pub fn execute_tx_optimistically(&self, tx: &Transaction, version: u64) -> ExecutionResult {
        let mut reads = Vec::new();
        let mut writes = Vec::new();

        let sender_balance = self.memory.read(&tx.sender, version);
        reads.push(tx.sender.clone());

        if let Some(balance) = sender_balance {
            let total_debit = tx.amount.checked_add(tx.fee);
            if total_debit.is_some() && balance >= total_debit.unwrap() {
                let new_sender_balance = balance - total_debit.unwrap();
                self.memory.write(&tx.sender, new_sender_balance);
                writes.push(tx.sender.clone());

                let recipient_balance = self.memory.read(&tx.recipient, version).unwrap_or(0);
                self.memory
                    .write(&tx.recipient, recipient_balance + tx.amount);
                writes.push(tx.recipient.clone());

                ExecutionResult {
                    tx_hash: tx.get_hash(),
                    success: true,
                    gas_used: tx.calculate_gas(),
                    reads,
                    writes,
                    version: self.memory.current_version(),
                }
            } else {
                ExecutionResult {
                    tx_hash: tx.get_hash(),
                    success: false,
                    gas_used: 0,
                    reads,
                    writes,
                    version: self.memory.current_version(),
                }
            }
        } else {
            ExecutionResult {
                tx_hash: tx.get_hash(),
                success: false,
                gas_used: 0,
                reads,
                writes,
                version: self.memory.current_version(),
            }
        }
    }

    /// Detekcija konflikata
    pub fn detect_conflicts(&self, results: &[ExecutionResult]) -> Vec<usize> {
        let mut conflicts = HashSet::new();
        let mut write_sets: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, result) in results.iter().enumerate() {
            if result.success {
                for account in &result.writes {
                    write_sets
                        .entry(account.clone())
                        .or_insert_with(Vec::new)
                        .push(idx);
                }
            }
        }

        for (idx, result) in results.iter().enumerate() {
            if result.success {
                for account in &result.reads {
                    if let Some(writers) = write_sets.get(account) {
                        for &writer_idx in writers {
                            if writer_idx != idx && writer_idx > idx {
                                conflicts.insert(idx);
                                conflicts.insert(writer_idx);
                            }
                        }
                    }
                }
            }
        }

        conflicts.into_iter().collect()
    }

    pub fn get_stats(&self) -> STMStats {
        self.stats.read().clone()
    }

    pub fn reset_stats(&self) {
        let mut stats = self.stats.write();
        *stats = STMStats::default();
    }
}

impl Default for BlockSTM {
    fn default() -> Self {
        Self::new()
    }
}
