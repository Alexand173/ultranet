use crate::{TransactionPayload, UltraBlockchain};
use sha3::{Digest, Sha3_256};

/// Versioned transaction envelope reserved for the one-time genesis supply correction.
pub const SUPPLY_CORRECTION_TRANSACTION_VERSION: u32 = 4;
/// Fixed zero-value governance envelope fields for the correction transaction.
pub const SUPPLY_CORRECTION_RECIPIENT: &str = "0x0";
pub const SUPPLY_CORRECTION_GAS_LIMIT: u64 = 1_000_000;
pub const SUPPLY_CORRECTION_GAS_PRICE: u64 = 1;
/// Last observed production precondition used as the initial operator-runbook
/// example. The transaction itself binds the exact balance read during
/// preparation; it must not rely on this historical observation.
pub const EXPECTED_SOVEREIGN_BALANCE_BASE_UNITS: u64 = 1_000_000;
const SUPPLY_CORRECTION_DOMAIN: &[u8] = b"UltraNet/Sovereign/genesis-supply-correction/v1";
const SUPPLY_CORRECTION_SIGNING_DOMAIN: &[u8] = b"UltraNet/supply-correction-signing-envelope/v4";
const SUPPLY_CORRECTION_VARIANT: &[u8] = b"SovereignSupplyCorrection";

/// Derive the fixed correction identifier from a domain-separated protocol label.
///
/// This is intentionally not caller-selectable. A fixed identifier makes the
/// operation one-time even if someone creates a new transaction nullifier.
pub fn correction_id() -> [u8; 32] {
    Sha3_256::digest(SUPPLY_CORRECTION_DOMAIN).into()
}

pub fn target_balance_base_units() -> u64 {
    UltraBlockchain::GENESIS_ALLOCATION_BASE_UNITS
}

pub fn target_address() -> &'static str {
    UltraBlockchain::SOVEREIGN_ADDR
}

pub fn validate_payload(payload: &TransactionPayload) -> Result<(), String> {
    let TransactionPayload::SovereignSupplyCorrection {
        correction_id: requested_id,
        target_address: requested_target,
        expected_balance,
        target_balance,
    } = payload
    else {
        return Err("transaction does not contain a sovereign supply correction payload".into());
    };

    if requested_id != &correction_id() {
        return Err("supply correction identifier is not the fixed protocol identifier".into());
    }
    if requested_target != target_address() {
        return Err("supply correction target must be the sovereign genesis address".into());
    }
    if *target_balance != target_balance_base_units() {
        return Err(format!(
            "supply correction target balance must be {} base units",
            target_balance_base_units()
        ));
    }
    if *target_balance <= *expected_balance {
        return Err("supply correction target must exceed its expected balance".into());
    }
    target_balance
        .checked_sub(*expected_balance)
        .ok_or_else(|| "supply correction balance delta overflowed".to_string())?;
    Ok(())
}

pub fn is_supply_correction(payload: &TransactionPayload) -> bool {
    matches!(
        payload,
        TransactionPayload::SovereignSupplyCorrection { .. }
    )
}

/// Build the canonical SHA3-256 preimage for the version-4 correction.
///
/// The common transaction fields deliberately use the same little-endian
/// encoding as the legacy transfer envelope. The correction fields are then
/// domain-separated and length-prefixed so signatures cannot be moved to a
/// different target, precondition, or final balance.
pub fn canonical_message(
    sender: &str,
    recipient: &str,
    amount: u64,
    fee: u64,
    timestamp: u64,
    nullifier: &[u8; 32],
    nonce: u64,
    gas_limit: u64,
    gas_price: u64,
    chain_id: u32,
    version: u32,
    correction_id: &[u8; 32],
    target_address: &str,
    expected_balance: u64,
    target_balance: u64,
) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(sender.as_bytes());
    hasher.update(recipient.as_bytes());
    hasher.update(amount.to_le_bytes());
    hasher.update(fee.to_le_bytes());
    hasher.update(timestamp.to_le_bytes());
    hasher.update(nullifier);
    hasher.update(nonce.to_le_bytes());
    hasher.update(gas_limit.to_le_bytes());
    hasher.update(gas_price.to_le_bytes());
    hasher.update(SUPPLY_CORRECTION_SIGNING_DOMAIN);
    hasher.update(version.to_le_bytes());
    hasher.update(chain_id.to_le_bytes());
    hasher.update(SUPPLY_CORRECTION_VARIANT);
    hasher.update(correction_id);
    hasher.update((target_address.len() as u64).to_le_bytes());
    hasher.update(target_address.as_bytes());
    hasher.update(expected_balance.to_le_bytes());
    hasher.update(target_balance.to_le_bytes());
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_target_is_fixed_and_uses_base_units() {
        assert_eq!(target_address(), UltraBlockchain::SOVEREIGN_ADDR);
        assert_eq!(target_balance_base_units(), 1_000_000_000_000);
        assert_ne!(correction_id(), [0u8; 32]);
    }

    #[test]
    fn correction_payload_rejects_mutation() {
        let payload = TransactionPayload::SovereignSupplyCorrection {
            correction_id: correction_id(),
            target_address: target_address().to_string(),
            expected_balance: EXPECTED_SOVEREIGN_BALANCE_BASE_UNITS,
            target_balance: target_balance_base_units(),
        };
        validate_payload(&payload).unwrap();

        let mut tampered = payload.clone();
        if let TransactionPayload::SovereignSupplyCorrection { target_balance, .. } = &mut tampered
        {
            *target_balance -= 1;
        }
        assert!(validate_payload(&tampered).is_err());
    }
}
