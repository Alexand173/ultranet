use sha3::{Digest, Sha3_256};

pub const ADDRESS_SCOPE: &str = "address";
pub const IP_SCOPE: &str = "ip";
pub const SUBNET_SCOPE: &str = "subnet";

pub fn keyed_digest(key: &[u8], domain: &str, value: &str) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"ULTRANET_FAUCET_DIGEST_V1");
    hasher.update((domain.len() as u64).to_le_bytes());
    hasher.update(domain.as_bytes());
    hasher.update((key.len() as u64).to_le_bytes());
    hasher.update(key);
    hasher.update(value.as_bytes());
    hasher.finalize().into()
}

pub fn request_fingerprint(key: &[u8], address: &str, amount_base_units: u64) -> [u8; 32] {
    keyed_digest(key, "request", &format!("{address}:{amount_base_units}"))
}

pub fn idempotency_digest(key: &[u8], idempotency_key: &str) -> [u8; 32] {
    keyed_digest(key, "idempotency", idempotency_key)
}

pub fn address_digest(key: &[u8], address: &str) -> [u8; 32] {
    keyed_digest(key, ADDRESS_SCOPE, address)
}

pub fn client_digest(key: &[u8], client_identity: Option<&str>) -> Option<[u8; 32]> {
    client_identity
        .filter(|value| !value.trim().is_empty())
        .map(|value| keyed_digest(key, IP_SCOPE, value))
}

pub fn subnet_digest(key: &[u8], client_identity: Option<&str>) -> Option<[u8; 32]> {
    let value = client_identity?.trim();
    let subnet = if let Ok(ip) = value.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(ip) => {
                let octets = ip.octets();
                format!("{}.{}.{}", octets[0], octets[1], octets[2])
            }
            std::net::IpAddr::V6(ip) => {
                let segments = ip.segments();
                format!(
                    "{:x}:{:x}:{:x}:{:x}",
                    segments[0], segments[1], segments[2], segments[3]
                )
            }
        }
    } else {
        value.to_string()
    };
    Some(keyed_digest(key, SUBNET_SCOPE, &subnet))
}

pub fn validate_idempotency_key(value: &str) -> Result<(), String> {
    let value = value.trim();
    if !(16..=200).contains(&value.len())
        || !value.is_ascii()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err("Idempotency-Key must be 16-200 non-whitespace ASCII characters".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_digests_are_stable_but_keyed() {
        assert_eq!(
            keyed_digest(b"key", "scope", "value"),
            keyed_digest(b"key", "scope", "value")
        );
        assert_ne!(
            keyed_digest(b"key", "scope", "value"),
            keyed_digest(b"other", "scope", "value")
        );
    }

    #[test]
    fn idempotency_keys_have_a_bounded_shape() {
        assert!(validate_idempotency_key(&"a".repeat(16)).is_ok());
        assert!(validate_idempotency_key("short").is_err());
        assert!(validate_idempotency_key(&"a".repeat(201)).is_err());
    }
}
