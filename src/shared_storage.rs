// ============================================================
// SHARED STORAGE - JEDNA BAZA ZA SVE INSTANCE
// ============================================================

use crate::Storage;
use std::sync::Arc;

pub struct SharedStorage {
    pub storage: Arc<Storage>,
    pub dag_tree: sled::Tree,
    pub move_modules: sled::Tree,
    pub move_resources: sled::Tree,
    pub fhe_keys: sled::Tree,
    pub trie_shards: Vec<sled::Tree>,
    pub reference_count: usize,
}

impl SharedStorage {
    pub fn new(path: &str) -> Result<Self, sled::Error> {
        let storage = Arc::new(Storage::new(path)?);

        // ✅ OTVORI TREES
        let dag_tree = storage
            .db
            .open_tree("dag_vertices")
            .expect("Failed to open dag_vertices tree");
        let move_modules = storage
            .db
            .open_tree("move_modules")
            .expect("Failed to open move_modules tree");
        let move_resources = storage
            .db
            .open_tree("move_resources")
            .expect("Failed to open move_resources tree");
        let fhe_keys = storage
            .db
            .open_tree("fhe_keys")
            .expect("Failed to open fhe_keys tree");

        let mut trie_shards = Vec::new();
        for i in 0..crate::storage::INITIAL_SHARD_COUNT {
            trie_shards.push(storage.db.open_tree(format!("trie_shard_{}", i))?);
        }

        Ok(Self {
            storage,
            dag_tree,
            move_modules,
            move_resources,
            fhe_keys,
            trie_shards,
            reference_count: 1,
        })
    }

    pub fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            dag_tree: self.dag_tree.clone(),
            move_modules: self.move_modules.clone(),
            move_resources: self.move_resources.clone(),
            fhe_keys: self.fhe_keys.clone(),
            trie_shards: self.trie_shards.clone(),
            reference_count: self.reference_count + 1,
        }
    }

    pub fn get_storage(&self) -> Arc<Storage> {
        self.storage.clone()
    }

    pub fn print_stats(&self) {
        println!("📊 SHARED STORAGE STATS:");
        println!("   References: {}", self.reference_count);
        println!("   Blocks: {}", self.storage.blocks.len());
        println!(
            "   State entries: {}",
            self.storage
                .state_shards
                .iter()
                .map(|s| s.len())
                .sum::<usize>()
        );
        println!("   Transactions: {}", self.storage.transactions.len());
        println!("   Validators: {}", self.storage.validators.len());
        println!("   Checkpoints: {}", self.storage.checkpoints.len());
    }
}

impl Clone for SharedStorage {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            dag_tree: self.dag_tree.clone(),
            move_modules: self.move_modules.clone(),
            move_resources: self.move_resources.clone(),
            fhe_keys: self.fhe_keys.clone(),
            trie_shards: self.trie_shards.clone(),
            reference_count: self.reference_count + 1,
        }
    }
}
