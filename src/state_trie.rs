// ============================================================
// MERKLE PATRICIA TRIE (MPT) - ARCHIVAL STATE FOR ULTRANET
// ============================================================

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use sled::Tree;
use std::collections::HashSet;

/// Konverzija bajtova u nibble (4 bita) za 16-way grananje
pub fn bytes_to_nibbles(bytes: &[u8]) -> Vec<u8> {
    let mut nibbles = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        nibbles.push(b >> 4);
        nibbles.push(b & 0x0F);
    }
    nibbles
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrieNode {
    Empty,
    Leaf {
        partial_path: Vec<u8>,
        value: Vec<u8>,
    },
    Extension {
        partial_path: Vec<u8>,
        child_hash: [u8; 32],
    },
    Branch {
        children: [Option<[u8; 32]>; 16],
        value: Option<Vec<u8>>,
    },
}

impl TrieNode {
    pub fn hash(&self) -> [u8; 32] {
        match self {
            TrieNode::Empty => [0u8; 32],
            _ => {
                let bytes = bincode::serialize(self).unwrap();
                let mut hasher = Sha3_256::new();
                hasher.update(&bytes);
                hasher.finalize().into()
            }
        }
    }
}

pub struct StateTrie {
    pub db: Tree,
    pub root_hash: [u8; 32],
}

/// ShardedStateTrie upravlja sa 16 nezavisnih MPT stabala
pub struct ShardedStateTrie {
    pub shards: Vec<StateTrie>,
}

impl ShardedStateTrie {
    pub fn new(trees: Vec<Tree>, shard_roots: Vec<[u8; 32]>) -> Self {
        let mut shards = Vec::new();
        for (i, tree) in trees.into_iter().enumerate() {
            let root = shard_roots.get(i).cloned().unwrap_or([0u8; 32]);
            shards.push(StateTrie::new(tree, root));
        }
        Self { shards }
    }

    pub fn insert(&mut self, shard_id: u8, key: &[u8], value: &[u8]) -> Result<[u8; 32], String> {
        if (shard_id as usize) >= self.shards.len() {
            return Err("Invalid shard ID".to_string());
        }
        self.shards[shard_id as usize].insert(key, value)
    }

    pub fn get(&self, shard_id: u8, key: &[u8]) -> Option<Vec<u8>> {
        if (shard_id as usize) >= self.shards.len() {
            return None;
        }
        self.shards[shard_id as usize].get(key)
    }

    /// Izračunava "Root of Roots" - Merkle koren svih shard korenskih heševa
    pub fn root_hash(&self) -> [u8; 32] {
        let mut state_hashes = Vec::new();
        for shard in &self.shards {
            state_hashes.push(shard.root_hash.to_vec());
        }

        let mut hasher = Sha3_256::new();
        for hash in state_hashes {
            hasher.update(&hash);
        }
        hasher.finalize().into()
    }

    pub fn prune(&mut self, shard_id: u8, history: Vec<[u8; 32]>) -> Result<usize, String> {
        if (shard_id as usize) >= self.shards.len() {
            return Err("Invalid shard ID".to_string());
        }
        self.shards[shard_id as usize].prune(history)
    }
}

impl StateTrie {
    pub fn new(db: Tree, root_hash: [u8; 32]) -> Self {
        Self { db, root_hash }
    }

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<[u8; 32], String> {
        let nibbles = bytes_to_nibbles(key);
        let new_root = self.update_recursive(self.root_hash, &nibbles, value)?;
        self.root_hash = new_root;
        Ok(new_root)
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let nibbles = bytes_to_nibbles(key);
        self.get_recursive(self.root_hash, &nibbles)
    }

    fn get_recursive(&self, node_hash: [u8; 32], nibbles: &[u8]) -> Option<Vec<u8>> {
        if node_hash == [0u8; 32] {
            return None;
        }

        let node_bytes = self.db.get(node_hash).ok().flatten()?;
        let node: TrieNode = bincode::deserialize(&node_bytes).ok()?;

        match node {
            TrieNode::Empty => None,
            TrieNode::Leaf {
                partial_path,
                value,
            } => {
                if partial_path == nibbles {
                    Some(value)
                } else {
                    None
                }
            }
            TrieNode::Extension {
                partial_path,
                child_hash,
            } => {
                if nibbles.starts_with(&partial_path) {
                    self.get_recursive(child_hash, &nibbles[partial_path.len()..])
                } else {
                    None
                }
            }
            TrieNode::Branch { children, value } => {
                if nibbles.is_empty() {
                    value
                } else {
                    let index = nibbles[0] as usize;
                    if let Some(child) = children[index] {
                        self.get_recursive(child, &nibbles[1..])
                    } else {
                        None
                    }
                }
            }
        }
    }

    fn update_recursive(
        &mut self,
        node_hash: [u8; 32],
        nibbles: &[u8],
        value: &[u8],
    ) -> Result<[u8; 32], String> {
        if node_hash == [0u8; 32] {
            return self.save_node(TrieNode::Leaf {
                partial_path: nibbles.to_vec(),
                value: value.to_vec(),
            });
        }

        let node_bytes = self
            .db
            .get(node_hash)
            .map_err(|e| e.to_string())?
            .ok_or("Node not found")?;
        let node: TrieNode = bincode::deserialize(&node_bytes).map_err(|e| e.to_string())?;

        match node {
            TrieNode::Leaf {
                partial_path,
                value: old_val,
            } => {
                if partial_path == nibbles {
                    return self.save_node(TrieNode::Leaf {
                        partial_path: partial_path.to_vec(),
                        value: value.to_vec(),
                    });
                }

                let common = self.common_prefix(&partial_path, nibbles);
                let mut branch_children = [None; 16];
                let mut branch_value = None;

                // Handle old leaf
                if common == partial_path.len() {
                    branch_value = Some(old_val);
                } else {
                    let idx = partial_path[common] as usize;
                    branch_children[idx] = Some(self.save_node(TrieNode::Leaf {
                        partial_path: partial_path[common + 1..].to_vec(),
                        value: old_val,
                    })?);
                }

                // Handle new value
                if common == nibbles.len() {
                    branch_value = Some(value.to_vec());
                } else {
                    let idx = nibbles[common] as usize;
                    branch_children[idx] = Some(self.save_node(TrieNode::Leaf {
                        partial_path: nibbles[common + 1..].to_vec(),
                        value: value.to_vec(),
                    })?);
                }

                let branch = TrieNode::Branch {
                    children: branch_children,
                    value: branch_value,
                };

                if common > 0 {
                    self.save_node(TrieNode::Extension {
                        partial_path: nibbles[..common].to_vec(),
                        child_hash: self.save_node(branch)?,
                    })
                } else {
                    self.save_node(branch)
                }
            }
            TrieNode::Extension {
                partial_path,
                child_hash,
            } => {
                let common = self.common_prefix(&partial_path, nibbles);
                if common == partial_path.len() {
                    let new_child = self.update_recursive(child_hash, &nibbles[common..], value)?;
                    self.save_node(TrieNode::Extension {
                        partial_path,
                        child_hash: new_child,
                    })
                } else {
                    // Split extension
                    let mut branch_children = [None; 16];

                    // Old path continuation
                    let old_idx = partial_path[common] as usize;
                    let old_node = if partial_path.len() == common + 1 {
                        child_hash
                    } else {
                        self.save_node(TrieNode::Extension {
                            partial_path: partial_path[common + 1..].to_vec(),
                            child_hash,
                        })?
                    };
                    branch_children[old_idx] = Some(old_node);

                    // New path continuation
                    let mut branch_value = None;
                    if common == nibbles.len() {
                        branch_value = Some(value.to_vec());
                    } else {
                        let new_idx = nibbles[common] as usize;
                        branch_children[new_idx] = Some(self.save_node(TrieNode::Leaf {
                            partial_path: nibbles[common + 1..].to_vec(),
                            value: value.to_vec(),
                        })?);
                    }

                    let branch = TrieNode::Branch {
                        children: branch_children,
                        value: branch_value,
                    };

                    if common > 0 {
                        self.save_node(TrieNode::Extension {
                            partial_path: nibbles[..common].to_vec(),
                            child_hash: self.save_node(branch)?,
                        })
                    } else {
                        self.save_node(branch)
                    }
                }
            }
            TrieNode::Branch {
                mut children,
                value: old_val,
            } => {
                if nibbles.is_empty() {
                    self.save_node(TrieNode::Branch {
                        children,
                        value: Some(value.to_vec()),
                    })
                } else {
                    let idx = nibbles[0] as usize;
                    let child_hash_val = children[idx].unwrap_or([0u8; 32]);
                    children[idx] =
                        Some(self.update_recursive(child_hash_val, &nibbles[1..], value)?);
                    self.save_node(TrieNode::Branch {
                        children,
                        value: old_val,
                    })
                }
            }
            TrieNode::Empty => unreachable!(),
        }
    }

    fn save_node(&self, node: TrieNode) -> Result<[u8; 32], String> {
        let hash = node.hash();
        let bytes = bincode::serialize(&node).unwrap();
        self.db.insert(hash, bytes).map_err(|e| e.to_string())?;
        Ok(hash)
    }

    pub fn prune(&mut self, active_roots: Vec<[u8; 32]>) -> Result<usize, String> {
        println!(
            "🧹 MPT: Starting pruning cycle for {} active roots...",
            active_roots.len()
        );
        let mut keep_set = HashSet::new();

        // 1. Mark phase: Obidji sva aktivna stanja i zabeleži čvorove koje treba čuvati
        for root in active_roots {
            self.mark_recursive(root, &mut keep_set);
        }

        // 2. Sweep phase: Obriši sve čvorove koji nisu u keep_set
        let mut deleted_count = 0;
        let mut batch = sled::Batch::default();

        for item in self.db.iter() {
            if let Ok((key, _)) = item {
                let mut hash = [0u8; 32];
                if key.len() == 32 {
                    hash.copy_from_slice(&key);
                    // Čuvaj samo ako je u keep_set i nije prazan koren
                    if !keep_set.contains(&hash) && hash != [0u8; 32] {
                        batch.remove(key);
                        deleted_count += 1;
                    }
                }
            }
        }

        self.db.apply_batch(batch).map_err(|e| e.to_string())?;
        self.db.flush().map_err(|e| e.to_string())?;

        println!(
            "✅ MPT: Pruning finished. Deleted {} unreachable nodes.",
            deleted_count
        );
        Ok(deleted_count)
    }

    fn mark_recursive(&self, node_hash: [u8; 32], keep_set: &mut HashSet<[u8; 32]>) {
        if node_hash == [0u8; 32] || keep_set.contains(&node_hash) {
            return;
        }

        if let Ok(Some(node_bytes)) = self.db.get(node_hash) {
            keep_set.insert(node_hash);
            if let Ok(node) = bincode::deserialize::<TrieNode>(&node_bytes) {
                match node {
                    TrieNode::Extension { child_hash, .. } => {
                        self.mark_recursive(child_hash, keep_set);
                    }
                    TrieNode::Branch { children, .. } => {
                        for child in children.iter().flatten() {
                            self.mark_recursive(*child, keep_set);
                        }
                    }
                    _ => {} // Leaf i Empty nemaju decu-heševe
                }
            }
        }
    }

    fn common_prefix(&self, a: &[u8], b: &[u8]) -> usize {
        let mut count = 0;
        while count < a.len() && count < b.len() && a[count] == b[count] {
            count += 1;
        }
        count
    }
}
