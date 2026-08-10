// ============================================================
// BULLSHARK DAG - NADOGRADNJA MYSTICETI-A
// ============================================================

use crate::dag_mysticeti::{MysticetiDAG, MysticetiVertex};
use crate::shared_storage::SharedStorage;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

pub struct BullsharkDAG {
    pub mysticeti: Arc<RwLock<MysticetiDAG>>,
    pub anchors: HashMap<u64, [u8; 32]>,
    pub leaders: HashMap<u64, u64>,
    pub committed_rounds: Vec<u64>,
    pub validator_count: usize,
    pub faulty: usize,
}

impl BullsharkDAG {
    pub fn new_with_dag(
        validator_count: usize,
        faulty: usize,
        mysticeti: Arc<RwLock<MysticetiDAG>>,
        _storage: Arc<SharedStorage>,
    ) -> Self {
        Self {
            mysticeti,
            anchors: HashMap::new(),
            leaders: HashMap::new(),
            committed_rounds: Vec::new(),
            validator_count,
            faulty,
        }
    }

    pub fn add_vertex(&mut self, vertex: MysticetiVertex) -> Result<(), String> {
        let mut dag = self.mysticeti.write();
        dag.add_vertex(vertex.clone())?;
        drop(dag);

        let leader_id = (vertex.round as usize) % self.validator_count;
        if vertex.validator_id == leader_id as u64 {
            self.anchors.insert(vertex.round, vertex.hash);
            self.leaders.insert(vertex.round, vertex.validator_id);
        }

        let dag = self.mysticeti.read();
        if self.has_quorum(&vertex, &dag) {
            self.committed_rounds.push(vertex.round);
            println!("✅ Bullshark: Round {} committed!", vertex.round);
        }

        Ok(())
    }

    pub fn has_quorum(&self, vertex: &MysticetiVertex, dag: &MysticetiDAG) -> bool {
        let required = self.validator_count - self.faulty;
        let mut referencing_validators = HashSet::new();

        for ref_hash in &vertex.referenced_by {
            if let Some(ref_vertex) = dag.vertices.get(ref_hash) {
                referencing_validators.insert(ref_vertex.validator_id);
            }
        }

        referencing_validators.len() >= required
    }

    pub fn get_anchor(&self, round: u64) -> Option<&[u8; 32]> {
        self.anchors.get(&round)
    }

    pub fn get_leader(&self, round: u64) -> Option<&u64> {
        self.leaders.get(&round)
    }

    pub fn get_committed_rounds(&self) -> &Vec<u64> {
        &self.committed_rounds
    }

    pub fn prune_old_rounds(&mut self, keep_rounds: u64) {
        let mut dag = self.mysticeti.write();
        dag.prune_old_rounds(keep_rounds);
        drop(dag);

        let current_round = self.mysticeti.read().current_round;
        if current_round > keep_rounds {
            let prune_below = current_round - keep_rounds;
            self.anchors.retain(|r, _| *r >= prune_below);
            self.leaders.retain(|r, _| *r >= prune_below);
        }
    }

    pub fn get_stats(&self) -> HashMap<String, String> {
        let dag = self.mysticeti.read();
        let mut stats = dag.get_stats();
        stats.insert("anchors".to_string(), self.anchors.len().to_string());
        stats.insert("leaders".to_string(), self.leaders.len().to_string());
        stats.insert(
            "committed_rounds".to_string(),
            self.committed_rounds.len().to_string(),
        );
        stats.insert(
            "validator_count".to_string(),
            self.validator_count.to_string(),
        );
        stats
    }
}
