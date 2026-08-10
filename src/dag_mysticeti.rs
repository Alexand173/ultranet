// ============================================================
// MYSTICETI DAG - BEZ KERTIFIKACIJE (Sui 2026)
// ============================================================

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::shared_storage::SharedStorage;

// ============================================================
// 1. MYSTICETI VERTEX (BEZ KERTIFIKACIJE)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysticetiVertex {
    pub id: u64,
    pub round: u64,
    pub validator_id: u64,
    pub hash: [u8; 32],
    pub parents: Vec<[u8; 32]>, // Reference na prethodne vertexe
    pub transactions: Vec<Vec<u8>>,
    pub timestamp: u64,
    pub is_anchor: bool,                  // Shoal: svaka runda ima anchor
    pub referenced_by: HashSet<[u8; 32]>, // Ko nas je referencirao (implicitni glas)
}

impl MysticetiVertex {
    pub fn new(validator_id: u64, round: u64, parents: Vec<[u8; 32]>) -> Self {
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
            referenced_by: HashSet::new(),
        }
    }
}

// ============================================================
// 2. MYSTICETI DAG - BEZ KERTIFIKACIJE
// ============================================================

#[derive(Clone)] // ← DODAJ OVO!
pub struct MysticetiDAG {
    pub vertices: HashMap<[u8; 32], MysticetiVertex>,
    pub by_round: HashMap<u64, Vec<[u8; 32]>>,
    pub current_round: u64,
    pub committed: HashSet<[u8; 32]>,
    pub validator_count: usize,
    pub faulty: usize,
    // NOVO: Keš svih poznatih hash-eva (uključujući one na disku)
    // Ovo omogućava da provjerimo reference bez držanja cijelog DAG-a u RAM-u.
    pub all_known_hashes: HashSet<[u8; 32]>,
    pub storage: Arc<SharedStorage>, // ← DODATI!
}

impl MysticetiDAG {
    pub fn new(validator_count: usize, faulty: usize, storage: Arc<SharedStorage>) -> Self {
        Self {
            vertices: HashMap::new(),
            by_round: HashMap::new(),
            current_round: 0,
            committed: HashSet::new(),
            validator_count,
            faulty,
            all_known_hashes: HashSet::new(),
            storage, // ← OVO JE DODATO!
        }
    }

    // ============================================================
    // MYSTICETI: DODAJ VERTEX BEZ KERTIFIKACIJE
    // ============================================================

    pub fn add_vertex(&mut self, vertex: MysticetiVertex) -> Result<(), String> {
        // ✅ MYSTICETI: Ne čekamo potpise!
        // Samo proveravamo da li su reference validne

        // 1. Proveri reference (gledamo i disk-cache HashSet)
        for parent_hash in &vertex.parents {
            if !self.all_known_hashes.contains(parent_hash)
                && !self.vertices.contains_key(parent_hash)
                && !parent_hash.iter().all(|&b| b == 0)
            {
                return Err(format!(
                    "Parent {:x?} not found (neither in RAM nor in Index)",
                    parent_hash
                ));
            }
        }

        // 2. Dodaj vertex u RAM
        self.vertices.insert(vertex.hash, vertex.clone());
        self.all_known_hashes.insert(vertex.hash);
        self.by_round
            .entry(vertex.round)
            .or_insert_with(Vec::new)
            .push(vertex.hash);

        if vertex.round > self.current_round {
            self.current_round = vertex.round;
        }
        //          // ✅ 2.5 SAČUVAJ NA DISK (NOVO!)
        println!("📝 add_vertex: About to save vertex round {}", vertex.round);
        if let Err(e) = self.save_vertex(&vertex) {
            eprintln!("⚠️ Failed to save vertex to disk: {}", e);
            // Ne vraćamo grešku jer je RAM već ažuriran
        } else {
            println!("   ✅ save_vertex succeeded!");
        }

        for parent_hash in &vertex.parents {
            if let Some(parent) = self.vertices.get_mut(parent_hash) {
                parent.referenced_by.insert(vertex.hash);
            }
            // Napomena: Ako je parent samo na disku, ovdje se ne ažurira
            // `referenced_by` u RAM-u. U produkciji bi se ovdje koristio
            // Write-Through Cache, ali za UltraNet je dovoljno da pratimo
            // konsenzus za najnovije runde.
        }

        // 4. Proveri da li je quorum postignut (implicitno)
        self.check_implicit_quorum(&vertex);

        Ok(())
    }

    /// NOVO: Pruning mehanizam - Briše stare vertexe iz RAM-a, ali ostavlja
    /// njihov hash u indeksu (all_known_hashes).
    pub fn prune_old_rounds(&mut self, keep_rounds: u64) {
        if self.current_round <= keep_rounds {
            return;
        }

        let prune_below = self.current_round - keep_rounds;

        // 1. Očisti vertices mapu
        self.vertices.retain(|_, v| v.round >= prune_below);

        // 2. Očisti by_round mapu
        self.by_round.retain(|r, _| *r >= prune_below);

        // 3. committed HashSet ostavljamo (on je samo set hash-eva)

        println!("🧹 DAG Pruning: RAM cleared for rounds < {}", prune_below);
        println!("   Vertices in RAM: {}", self.vertices.len());
    }

    // ============================================================
    // MYSTICETI: IMRACUNATI KONSENZUS IZ REFERENCI
    // ============================================================

    fn check_implicit_quorum(&mut self, vertex: &MysticetiVertex) {
        // MYSTICETI: Kada vertex bude referenciran od dovoljno validatora,
        // implicitno je odobren
        let required = self.validator_count - self.faulty;

        // Brojimo jedinstvene validatore koji su referencirali ovaj vertex
        let mut referencing_validators = HashSet::new();
        for ref_hash in &vertex.referenced_by {
            if let Some(ref_vertex) = self.vertices.get(ref_hash) {
                referencing_validators.insert(ref_vertex.validator_id);
            }
        }

        if referencing_validators.len() >= required {
            // ✅ MYSTICETI: Konsenzus postignut bez kertifikacije!
            self.committed.insert(vertex.hash);
            println!(
                "✅ Mysticeti: Vertex {:x?} committed implicitly!",
                &vertex.hash[..4]
            );
        }
    }

    // ============================================================
    // SHOAL: ANCHOR U SVAKOJ RUNDI
    // ============================================================

    pub fn get_anchor(&self, round: u64) -> Option<&MysticetiVertex> {
        // SHOAL: Svaka runda ima anchor (lidera)
        // Lider se bira deterministički na osnovu round-a
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

    // ============================================================
    // SHOAL: LEADER REPUTATION (Aptos)
    // ============================================================

    pub fn get_leader_with_reputation(
        &self,
        round: u64,
        reputation: &HashMap<u64, f64>,
    ) -> Option<&MysticetiVertex> {
        // APTOS: Biramo lidera na osnovu reputacije
        // Validatori sa boljom reputacijom češće postaju lideri

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

    // ============================================================
    // STATISTIKA
    // ============================================================

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
// 3. VALIDATOR STATISTIKA ZA LEADER REPUTATION
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        // Kombinujemo latenciju i uspešnost
        let avg_latency = if self.latency_history.is_empty() {
            100.0
        } else {
            self.latency_history.iter().sum::<u64>() as f64 / self.latency_history.len() as f64
        };

        // Normalizujemo: bolja latencija → veća reputacija
        let latency_score = if avg_latency < 50.0 {
            1.0
        } else if avg_latency < 100.0 {
            0.8
        } else if avg_latency < 200.0 {
            0.5
        } else {
            0.2
        };

        // Kombinujemo sa uspešnošću
        (latency_score + self.success_rate) / 2.0
    }
}
// ============================================================
// DISK-BACKED STORAGE - save_vertex & load_vertex
// ============================================================

impl MysticetiDAG {
    /// Čuva vertex na disk (Sled database)
    // ✅ KORISTI dag_tree (ne dag_vertices)
    pub fn save_vertex(&self, vertex: &MysticetiVertex) -> Result<(), String> {
        // 🔍 DEBUG: Ispiši da se funkcija poziva
        println!("💾 save_vertex: Called for vertex round {}", vertex.round);

        let value =
            bincode::serialize(vertex).map_err(|e| format!("Failed to serialize vertex: {}", e))?;

        println!("   Serialized size: {} bytes", value.len());

        // 🔍 DEBUG: Ispiši hash
        println!("   Hash: {:x?}", &vertex.hash[..4]);

        self.storage
            .dag_tree
            .insert(vertex.hash, value)
            .map_err(|e| format!("Failed to save vertex: {}", e))?;

        println!("   ✅ Vertex saved to disk!");
        Ok(())
    }

    pub fn load_vertex(&self, hash: &[u8; 32]) -> Option<MysticetiVertex> {
        let value = self.storage.dag_tree.get(hash).ok()??; // ← dag_tree!
        bincode::deserialize(&value).ok()
    }
    /// Dohvati vertex (prvo iz RAM-a, onda sa diska)
    pub fn get_vertex(&self, hash: &[u8; 32]) -> Option<MysticetiVertex> {
        // 1. Prvo provjeri RAM
        if let Some(v) = self.vertices.get(hash) {
            return Some(v.clone());
        }

        // 2. Ako nije u RAM-u, provjeri indeks
        if self.all_known_hashes.contains(hash) {
            // 3. Učitaj sa diska
            return self.load_vertex(hash);
        }

        None
    }

    /// Statistika o disku
    pub fn get_disk_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();

        // Broj vertexa u RAM-u
        stats.insert("ram_vertices".to_string(), self.vertices.len().to_string());

        // Broj vertexa u indeksu (ukupno poznatih)
        stats.insert(
            "total_known".to_string(),
            self.all_known_hashes.len().to_string(),
        );

        // Broj vertexa na disku (aproksimacija)
        let disk_count = self
            .all_known_hashes
            .len()
            .saturating_sub(self.vertices.len());
        stats.insert("disk_vertices".to_string(), disk_count.to_string());

        stats
    }
}

// ============================================================
// 4. TESTOVI
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysticeti_dag() {
        let storage = Arc::new(SharedStorage::new("test_db").unwrap());
        let mut dag = MysticetiDAG::new(5, 1, storage);
        // Kreiraj genesis vertex
        let genesis = MysticetiVertex::new(0, 0, vec![]);
        let genesis_hash = genesis.hash; // ✅ Sačuvaj hash
        dag.add_vertex(genesis).unwrap();

        // Kreiraj validator vertexe
        for i in 0..5 {
            let vertex = MysticetiVertex::new(i, 1, vec![genesis_hash]);
            dag.add_vertex(vertex).unwrap();
        }

        // Proveri da li je quorum postignut
        let stats = dag.get_stats();
        println!("📊 Mysticeti Stats:");
        for (key, value) in stats {
            println!("   {}: {}", key, value);
        }

        assert!(dag.vertices.len() >= 5);
    }

    #[test]
    fn test_validator_reputation() {
        let mut stats = ValidatorStats::new();
        stats.update(30, true);
        stats.update(45, true);
        stats.update(200, false);

        let reputation = stats.get_reputation();
        println!("Reputation: {:.2}", reputation);
        assert!(reputation > 0.0 && reputation <= 1.0);
    }
}
