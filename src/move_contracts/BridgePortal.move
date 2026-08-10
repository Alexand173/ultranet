// ============================================================
// CROSS-LAYER BRIDGE - L1 ↔ L3 PORTAL
// ============================================================
module 0x1::BridgePortal {
    use std::signer;
    use std::vector;

    /// Message representing a cross-layer transfer
    struct BridgeMessage has key, store, copy, drop {
        nonce: u64,
        source_chain: u32,
        target_chain: u32,
        sender: address,
        recipient: address,
        amount: u64,
    }

    /// Store pending outbox messages
    struct Outbox has key {
        messages: vector<BridgeMessage>,
        next_nonce: u64,
    }

    /// Initialize the portal on a chain
    public fun initialize(account: &signer) {
        move_to(account, Outbox {
            messages: vector::empty<BridgeMessage>(),
            next_nonce: 0,
        });
    }

    /// Trigger a cross-layer transfer
    public fun transfer_cross_layer(
        sender: &signer,
        target_chain: u32,
        recipient: address,
        amount: u64
    ) acquires Outbox {
        let outbox = borrow_global_mut<Outbox>(@0x1);
        let msg = BridgeMessage {
            nonce: outbox.next_nonce,
            source_chain: 0, // Placeholder
            target_chain,
            sender: signer::address_of(sender),
            recipient,
            amount,
        };
        vector::push_back(&mut outbox.messages, msg);
        outbox.next_nonce = outbox.next_nonce + 1;
    }
}
