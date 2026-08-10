// ============================================================
// APPCHAIN FACTORY - L1 REGISTRY FOR L3 CHAINS
// ============================================================
module 0x1::AppChainFactory {
    use std::signer;
    use std::vector;

    /// Meta-information about an AppChain
    struct AppChain has key, store {
        id: u32,
        owner: address,
        state_root: vector<u8>,
        last_anchored_height: u64,
        is_active: bool,
    }

    /// Global registry stored at the Factory address
    struct Registry has key {
        chains: vector<AppChain>,
    }

    /// Error codes
    const ENOT_OWNER: u64 = 1;
    const ECHAIN_NOT_FOUND: u64 = 2;

    /// Initialize the factory registry (called once)
    public fun initialize(account: &signer) {
        move_to(account, Registry {
            chains: vector::empty<AppChain>(),
        });
    }

    /// Spawn a new AppChain on L1
    public fun spawn_chain(creator: &signer, id: u32) acquires Registry {
        let registry = borrow_global_mut<Registry>(@0x1);
        vector::push_back(&mut registry.chains, AppChain {
            id,
            owner: signer::address_of(creator),
            state_root: vector::empty<u8>(),
            last_anchored_height: 0,
            is_active: true,
        });
    }

    /// Anchor a new L3 state to L1 using a ZK Proof
    /// In production, this would verify a Recursive ZK proof.
    public fun anchor_state(
        owner: &signer, 
        chain_id: u32, 
        new_root: vector<u8>, 
        _proof: vector<u8>
    ) acquires Registry {
        let registry = borrow_global_mut<Registry>(@0x1);
        let len = vector::length(&registry.chains);
        let i = 0;
        let found = false;

        while (i < len) {
            let chain = vector::borrow_mut(&mut registry.chains, i);
            if (chain.id == chain_id) {
                assert!(chain.owner == signer::address_of(owner), ENOT_OWNER);
                chain.state_root = new_root;
                chain.last_anchored_height = chain.last_anchored_height + 1;
                found = true;
                break
            };
            i = i + 1;
        };

        assert!(found, ECHAIN_NOT_FOUND);
    }
}
