use UltraNet::QuantumKeyPair;

/// Ephemeral sovereign owner keys used only by integration tests.
///
/// The fixture deliberately generates keys at test time instead of storing
/// private material in the repository or reading the active sovereign backup.
pub struct SovereignKeyFixture {
    pub owners: [QuantumKeyPair; 3],
}

impl SovereignKeyFixture {
    pub fn generate() -> Self {
        Self {
            owners: std::array::from_fn(|_| QuantumKeyPair::generate()),
        }
    }

    pub fn public_keys(&self) -> Vec<Vec<u8>> {
        self.owners
            .iter()
            .map(|owner| owner.public_key.clone())
            .collect()
    }

    pub fn sign_with_owner(&self, owner_index: usize, message: &[u8]) -> Vec<u8> {
        self.owners[owner_index].sign(message)
    }

    pub fn sign_with_threshold(&self, message: &[u8]) -> Vec<u8> {
        let mut signatures = self.sign_with_owner(0, message);
        signatures.extend_from_slice(&self.sign_with_owner(1, message));
        signatures
    }
}
