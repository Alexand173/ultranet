// ============================================================
// MULTI-VERSION MEMORY - OSNOVA BLOCK-STM-A
// ============================================================

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Multi-version memory - omogućava paralelno izvršavanje
/// Svaka transakcija vidi svoju verziju stanja
pub struct MultiVersionMemory {
    /// Verzije stanja po account-u: account → (version, value)
    versions: Arc<RwLock<HashMap<String, Vec<(u64, u64)>>>>,
    /// Trenutna globalna verzija
    current_version: Arc<RwLock<u64>>,
}

impl MultiVersionMemory {
    pub fn new() -> Self {
        Self {
            versions: Arc::new(RwLock::new(HashMap::new())),
            current_version: Arc::new(RwLock::new(0)),
        }
    }

    /// Čitanje vrijednosti na određenoj verziji
    pub fn read(&self, account: &str, version: u64) -> Option<u64> {
        let versions = self.versions.read();
        if let Some(history) = versions.get(account) {
            // Pronađi najnoviju verziju <= željena verzija
            for (v, value) in history.iter().rev() {
                if *v <= version {
                    return Some(*value);
                }
            }
        }
        None
    }

    /// Pisanje vrijednosti u trenutnu verziju
    pub fn write(&self, account: &str, value: u64) -> u64 {
        let mut versions = self.versions.write();
        let current_version = *self.current_version.read();

        let history = versions.entry(account.to_string()).or_insert_with(Vec::new);
        history.push((current_version, value));

        current_version
    }

    /// Započni novu verziju (nova runda izvršavanja)
    pub fn new_version(&self) -> u64 {
        let mut version = self.current_version.write();
        *version += 1;
        *version
    }

    /// Dohvati trenutnu verziju
    pub fn current_version(&self) -> u64 {
        *self.current_version.read()
    }

    /// Dohvati sve account-e koji su dirani u ovoj verziji
    pub fn get_touched_accounts(&self) -> Vec<String> {
        let versions = self.versions.read();
        versions.keys().cloned().collect()
    }

    /// Dohvati historiju za account
    pub fn get_history(&self, account: &str) -> Vec<(u64, u64)> {
        let versions = self.versions.read();
        versions.get(account).cloned().unwrap_or_default()
    }

    /// Rollback na prethodnu verziju (za ponovno izvršavanje)
    pub fn rollback_to(&self, version: u64) {
        let mut versions = self.versions.write();
        for (_, history) in versions.iter_mut() {
            history.retain(|(v, _)| *v <= version);
        }
        *self.current_version.write() = version;
    }

    /// Čišćenje starih verzija (pruning)
    pub fn prune(&self, keep_versions: u64) {
        let mut versions = self.versions.write();
        let current = *self.current_version.read();
        let min_version = current.saturating_sub(keep_versions);

        for (_, history) in versions.iter_mut() {
            history.retain(|(v, _)| *v >= min_version);
        }
    }
}

impl Default for MultiVersionMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mv_memory() {
        let memory = MultiVersionMemory::new();

        // Verzija 0: Alice = 1000
        memory.write("Alice", 1000);
        assert_eq!(memory.read("Alice", 0), Some(1000));

        // Nova verzija: Alice = 900
        memory.new_version();
        memory.write("Alice", 900);
        assert_eq!(memory.read("Alice", 1), Some(900));

        // Stara verzija još uvijek postoji
        assert_eq!(memory.read("Alice", 0), Some(1000));

        // Bob piše u svojoj verziji
        memory.write("Bob", 500);
        assert_eq!(memory.read("Bob", 1), Some(500));

        println!("✅ Multi-version memory test passed!");
    }
}
