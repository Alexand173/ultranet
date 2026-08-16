// ============================================================
// MYSTICETI DAG DEMO - SAMOSTALNI BIN
// ============================================================

use std::collections::HashMap;
use std::time::Instant;

// ============================================================
// 1. MYSTICETI VERTEX
// ============================================================

#[derive(Debug, Clone)]
pub struct MysticetiVertex {
    pub id: u64,
    pub round: u64,
    pub validator_id: u64,
    pub hash: [u8; 32],
    pub parents: Vec<[u8; 32]>,
    pub transactions: Vec<Vec<u8>>,
    pub timestamp: u64,
    pub is_anchor: bool,
    pub referenced_by: std::collections::HashSet<[u8; 32]>,
}

impl MysticetiVertex {
    pub fn new(validator_id: u64, round: u64, parents: Vec<[u8; 32]>) -> Self {
        use sha3::{Digest, Sha3_256};
        let mut hasher = Sha3_256::new();
        hasher.update(&round.to_le_bytes());
        hasher.update(&validator_id.to_le_bytes());
        for parent in &parents {
            hasher.update(parent);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hasher.finalize());

        Self {
            id: round,
            round,
            validator_id,
            hash,
            parents,
            transactions: Vec::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            is_anchor: false,
            referenced_by: std::collections::HashSet::new(),
        }
    }
}

// ============================================================
// 2. MYSTICETI DAG
// ============================================================

pub struct MysticetiDAG {
    pub vertices: std::collections::HashMap<[u8; 32], MysticetiVertex>,
    pub by_round: std::collections::HashMap<u64, Vec<[u8; 32]>>,
    pub current_round: u64,
    pub committed: std::collections::HashSet<[u8; 32]>,
    pub validator_count: usize,
    pub faulty: usize,
}

impl MysticetiDAG {
    pub fn new(validator_count: usize, faulty: usize) -> Self {
        Self {
            vertices: std::collections::HashMap::new(),
            by_round: std::collections::HashMap::new(),
            current_round: 0,
            committed: std::collections::HashSet::new(),
            validator_count,
            faulty,
        }
    }

    pub fn add_vertex(&mut self, vertex: MysticetiVertex) -> Result<(), String> {
        for parent_hash in &vertex.parents {
            if !self.vertices.contains_key(parent_hash) && !parent_hash.iter().all(|&b| b == 0) {
                return Err(format!("Parent {:x?} not found", parent_hash));
            }
        }

        self.vertices.insert(vertex.hash, vertex.clone());
        self.by_round
            .entry(vertex.round)
            .or_insert_with(Vec::new)
            .push(vertex.hash);

        for parent_hash in &vertex.parents {
            if let Some(parent) = self.vertices.get_mut(parent_hash) {
                parent.referenced_by.insert(vertex.hash);
            }
        }

        self.check_implicit_quorum(&vertex);
        Ok(())
    }

    fn check_implicit_quorum(&mut self, vertex: &MysticetiVertex) {
        let required = self.validator_count - self.faulty;
        let mut referencing_validators = std::collections::HashSet::new();

        for ref_hash in &vertex.referenced_by {
            if let Some(ref_vertex) = self.vertices.get(ref_hash) {
                referencing_validators.insert(ref_vertex.validator_id);
            }
        }

        if referencing_validators.len() >= required {
            self.committed.insert(vertex.hash);
            println!(
                "✅ Mysticeti: Vertex {:x?} committed implicitly!",
                &vertex.hash[..4]
            );
        }
    }

    pub fn get_anchor(&self, round: u64) -> Option<&MysticetiVertex> {
        let leader_id = (round as usize) % self.validator_count;

        if let Some(vertices) = self.by_round.get(&round) {
            for hash in vertices {
                if let Some(vertex) = self.vertices.get(hash) {
                    if vertex.validator_id == leader_id as u64 {
                        return Some(vertex);
                    }
                }
            }
        }
        None
    }

    pub fn get_leader_with_reputation(
        &self,
        round: u64,
        reputation: &HashMap<u64, f64>,
    ) -> Option<&MysticetiVertex> {
        if let Some(vertices) = self.by_round.get(&round) {
            let mut best_vertex: Option<&MysticetiVertex> = None;
            let mut best_score = -1.0;

            for hash in vertices {
                if let Some(vertex) = self.vertices.get(hash) {
                    let score = reputation.get(&vertex.validator_id).unwrap_or(&0.5);
                    if *score > best_score {
                        best_score = *score;
                        best_vertex = Some(vertex);
                    }
                }
            }
            return best_vertex;
        }
        None
    }

    pub fn get_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();
        stats.insert(
            "total_vertices".to_string(),
            self.vertices.len().to_string(),
        );
        stats.insert("total_rounds".to_string(), self.by_round.len().to_string());
        stats.insert("committed".to_string(), self.committed.len().to_string());
        stats.insert(
            "validator_count".to_string(),
            self.validator_count.to_string(),
        );
        stats.insert("faulty".to_string(), self.faulty.to_string());
        stats.insert("current_round".to_string(), self.current_round.to_string());
        stats
    }
}

// ============================================================
// 3. VALIDATOR STATS
// ============================================================

pub struct ValidatorStats {
    pub latency_history: Vec<u64>,
    pub success_rate: f64,
    pub last_round_participated: u64,
    pub total_proposed: u64,
    pub total_committed: u64,
}

impl ValidatorStats {
    pub fn new() -> Self {
        Self {
            latency_history: Vec::new(),
            success_rate: 0.5,
            last_round_participated: 0,
            total_proposed: 0,
            total_committed: 0,
        }
    }

    pub fn update(&mut self, latency: u64, committed: bool) {
        self.latency_history.push(latency);
        if self.latency_history.len() > 100 {
            self.latency_history.remove(0);
        }

        self.total_proposed += 1;
        if committed {
            self.total_committed += 1;
        }
        self.success_rate = self.total_committed as f64 / self.total_proposed as f64;
    }

    pub fn get_reputation(&self) -> f64 {
        let avg_latency = if self.latency_history.is_empty() {
            100.0
        } else {
            self.latency_history.iter().sum::<u64>() as f64 / self.latency_history.len() as f64
        };

        let latency_score = if avg_latency < 50.0 {
            1.0
        } else if avg_latency < 100.0 {
            0.8
        } else if avg_latency < 200.0 {
            0.5
        } else {
            0.2
        };

        (latency_score + self.success_rate) / 2.0
    }
}

// ============================================================
// 4. MAIN
// ============================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 MYSTICETI DAG DEMO");
    println!("======================");
    println!();

    let mut dag = MysticetiDAG::new(5, 1);
    println!("✅ MysticetiDAG created!");
    println!("   Validators: {}", dag.validator_count);
    println!("   Faulty: {}", dag.faulty);
    println!();

    println!("📦 Creating genesis vertex...");
    let genesis = MysticetiVertex::new(0, 0, vec![]);
    let genesis_hash = genesis.hash;
    dag.add_vertex(genesis)?;
    println!("   Genesis vertex added!");
    println!();

    println!("📦 Creating validator vertices...");
    let start = Instant::now();

    // Round 1 - all validators create vertices
    let mut round_1_hashes = Vec::new();
    for i in 0..5 {
        let vertex = MysticetiVertex::new(i, 1, vec![genesis_hash]);
        let hash = vertex.hash;
        dag.add_vertex(vertex)?;
        round_1_hashes.push(hash);
        println!("   Validator {} added a vertex", i);
    }

    let duration = start.elapsed();
    println!("   Time: {:?}", duration);
    println!();

    let stats = dag.get_stats();
    println!("📊 MYSTICETI STATS:");
    for (key, value) in stats {
        println!("   {}: {}", key, value);
    }
    println!();

    println!("⭐ VALIDATOR REPUTATION:");
    let mut reputation_map = HashMap::new();

    for i in 0..5 {
        let mut stats = ValidatorStats::new();
        stats.update(30 + i * 10, true);
        stats.update(40 + i * 5, i % 2 == 0);
        let rep = stats.get_reputation();
        reputation_map.insert(i, rep);
        println!("   Validator {}: {:.2}", i, rep);
    }
    println!();

    // ============================================================
    // SHOAL ANCHOR TEST
    // ============================================================
    println!("⚓ SHOAL ANCHOR TEST:");
    let mut last_round_hashes = round_1_hashes.clone();

    for round in 2..5 {
        let mut current_round_hashes = Vec::new();

        for i in 0..5 {
            let vertex = MysticetiVertex::new(i, round, last_round_hashes.clone());
            let hash = vertex.hash;
            dag.add_vertex(vertex)?;
            current_round_hashes.push(hash);
        }

        if let Some(anchor) = dag.get_anchor(round) {
            println!(
                "   Round {}: Anchor is validator {}",
                round, anchor.validator_id
            );
        } else {
            println!("   Round {}: No anchor", round);
        }

        last_round_hashes = current_round_hashes;
    }
    println!();

    // ============================================================
    // LEADER REPUTATION
    // ============================================================
    println!("🏆 LEADER REPUTATION:");

    for round in 5..8 {
        let mut current_round_hashes = Vec::new();

        for i in 0..5 {
            let vertex = MysticetiVertex::new(i, round, last_round_hashes.clone());
            let hash = vertex.hash;
            dag.add_vertex(vertex)?;
            current_round_hashes.push(hash);
        }

        if let Some(leader) = dag.get_leader_with_reputation(round, &reputation_map) {
            println!(
                "   Round {}: Leader is validator {} (reputation: {:.2})",
                round,
                leader.validator_id,
                reputation_map.get(&leader.validator_id).unwrap_or(&0.0)
            );
        }

        last_round_hashes = current_round_hashes;
    }
    println!();

    println!("✅ IMPLICIT CONSENSUS (Mysticeti):");
    println!("   Committed vertices: {}", dag.committed.len());
    println!();

    println!("🎉 MYSTICETI DAG DEMO COMPLETE!");
    println!("   Total vertices: {}", dag.vertices.len());
    println!("   Total rounds: {}", dag.by_round.len());
    println!("   Committed: {}", dag.committed.len());

    Ok(())
}
