// ============================================================
// ULTRA COIN - STABLECOIN MODULE
// ============================================================
module 0x1::UltraCoin {
    use std::signer;

    /// Coin structure representing a balance
    struct Coin has key, store {
        value: u64,
    }

    /// Treasury to manage supply and admin control
    struct Treasury has key {
        total_supply: u64,
        admin: address,
    }

    /// Error codes
    const ENOT_ADMIN: u64 = 1;
    const EINSUFFICIENT_BALANCE: u64 = 2;

    /// Initialize the treasury (called once at genesis)
    public fun initialize(account: &signer) {
        let admin_addr = signer::address_of(account);
        move_to(account, Treasury {
            total_supply: 0,
            admin: admin_addr,
        });
    }

    /// Mint new coins (Admin only)
    public fun mint(admin: &signer, to: address, amount: u64) acquires Treasury, Coin {
        let admin_addr = signer::address_of(admin);
        let treasury = borrow_global_mut<Treasury>(admin_addr);
        
        // Check if caller is admin
        assert!(treasury.admin == admin_addr, ENOT_ADMIN);

        // Update total supply
        treasury.total_supply = treasury.total_supply + amount;

        // Add to recipient balance
        if (exists<Coin>(to)) {
            let coin = borrow_global_mut<Coin>(to);
            coin.value = coin.value + amount;
        } else {
            // Logic for creating new account resource omitted for simulation
        }
    }

    /// Transfer coins between accounts
    public fun transfer(from: &signer, to: address, amount: u64) acquires Coin {
        let from_addr = signer::address_of(from);
        let from_coin = borrow_global_mut<Coin>(from_addr);
        
        // Check balance
        assert!(from_coin.value >= amount, EINSUFFICIENT_BALANCE);

        // Deduct from sender
        from_coin.value = from_coin.value - amount;

        // Add to recipient
        let to_coin = borrow_global_mut<Coin>(to);
        to_coin.value = to_coin.value + amount;
    }

    /// Public view for balance
    public fun balance(owner: address): u64 acquires Coin {
        if (exists<Coin>(owner)) {
            borrow_global<Coin>(owner).value
        } else {
            0
        }
    }
}
