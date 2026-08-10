// ============================================================
// ULTRA NFT - DIGITAL ASSETS MODULE
// ============================================================
module 0x1::UltraNFT {
    use std::signer;
    use std::string::String;
    use std::vector;

    /// Individual NFT struct
    struct NFT has store, copy, drop {
        id: u64,
        name: String,
        uri: String,
    }

    /// Resource to hold a collection of NFTs for an address
    struct Collection has key {
        items: vector<NFT>,
        next_id: u64,
    }

    /// Error codes
    const ECOLLECTION_NOT_FOUND: u64 = 1;

    /// Initialize a collection for an account
    public fun initialize_collection(account: &signer) {
        let addr = signer::address_of(account);
        if (!exists<Collection>(addr)) {
            move_to(account, Collection {
                items: vector::empty<NFT>(),
                next_id: 0,
            });
        }
    }

    /// Mint a new NFT into the account's collection
    public fun mint(account: &signer, name: String, uri: String) acquires Collection {
        let addr = signer::address_of(account);
        
        // Ensure collection exists
        if (!exists<Collection>(addr)) {
            initialize_collection(account);
        };

        let collection = borrow_global_mut<Collection>(addr);
        
        // Create NFT
        let nft = NFT {
            id: collection.next_id,
            name,
            uri,
        };

        // Add to collection
        vector::push_back(&mut collection.items, nft);
        collection.next_id = collection.next_id + 1;
    }

    /// Transfer an NFT to another address
    public fun transfer_nft(from: &signer, to: address, nft_id: u64) acquires Collection {
        let from_addr = signer::address_of(from);
        let from_collection = borrow_global_mut<Collection>(from_addr);
        
        // Find and remove NFT from sender
        let i = 0;
        let len = vector::length(&from_collection.items);
        let found = false;
        let nft_to_move = NFT { id: 0, name: std::string::utf8(b""), uri: std::string::utf8(b"") };

        while (i < len) {
            let nft = vector::borrow(&from_collection.items, i);
            if (nft.id == nft_id) {
                nft_to_move = vector::remove(&mut from_collection.items, i);
                found = true;
                break
            };
            i = i + 1;
        };

        assert!(found, ECOLLECTION_NOT_FOUND);

        // Add to recipient collection
        // Note: Recipient must have a collection initialized in a real scenario
        let to_collection = borrow_global_mut<Collection>(to);
        vector::push_back(&mut to_collection.items, nft_to_move);
    }
}
