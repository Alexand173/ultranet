//! Authenticating one service to another.
//!
//! The Games Reward Service asks the Records Service to anchor a day's root. The Records Service holds the
//! chain owner's key; the Games Reward Service must never hold it. Between them is one authenticated
//! request, and this module is that authentication.
//!
//! A request carries four headers and a body. The signature is an HMAC-SHA3-256 (RFC 2104, block size 136)
//! over a canonical string, keyed by a secret both sides hold as a systemd credential:
//!
//! ```text
//! X-Peer-Id:        games                       who is calling (a fixed name, not a user)
//! X-Peer-Timestamp: seconds since the Unix epoch
//! X-Peer-Nonce:     16-64 hex characters, never reused inside the window
//! X-Peer-Signature: hex(HMAC-SHA3-256(secret, canonical))
//!
//! canonical = peer_id "\n" METHOD "\n" path "\n" timestamp "\n" nonce "\n" hex(SHA3-256(body))
//! ```
//!
//! The method, the path and the body are inside the signature, so a captured request cannot be replayed
//! against another route or with another root. The timestamp must be within [`WINDOW_SECONDS`] of the
//! receiver's clock and a nonce is accepted once inside that window, so a captured request cannot be
//! replayed at all. The dependency is one-way: the Records Service never calls the Games Reward Service,
//! so the secret authenticates a caller to a server and never the reverse.
//!
//! What this does **not** give: confidentiality (use loopback or a unix socket, never a public network)
//! and protection from a compromised caller, which holds the secret. That is why the Records Service
//! limits what an authenticated caller may ask for (see `records.rs`), instead of trusting the caller.

use sha3::{Digest, Sha3_256};
use std::{collections::HashMap, sync::Mutex};
use subtle::ConstantTimeEq;
use thiserror::Error;

pub const WINDOW_SECONDS: u64 = 60;
const BLOCK: usize = 136;
/// Shorter secrets are refused at load: a guessable key makes the signature decorative.
pub const MIN_SECRET_BYTES: usize = 32;
const MAX_CACHED_NONCES: usize = 20_000;

pub const HEADER_PEER: &str = "X-Peer-Id";
pub const HEADER_TIMESTAMP: &str = "X-Peer-Timestamp";
pub const HEADER_NONCE: &str = "X-Peer-Nonce";
pub const HEADER_SIGNATURE: &str = "X-Peer-Signature";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PeerError {
    /// Every failure to authenticate is this one answer: the caller learns that it was refused, not why.
    #[error("the peer could not be authenticated")]
    Unauthenticated,
}

/// HMAC-SHA3-256 as RFC 2104 defines it, with SHA3-256's 136-byte rate as the block size.
pub fn hmac_sha3_256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha3_256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha3_256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha3_256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

pub fn canonical(
    peer_id: &str,
    method: &str,
    path: &str,
    timestamp: u64,
    nonce: &str,
    body: &[u8],
) -> Vec<u8> {
    format!(
        "{peer_id}\n{}\n{path}\n{timestamp}\n{nonce}\n{}",
        method.to_ascii_uppercase(),
        hex::encode(Sha3_256::digest(body))
    )
    .into_bytes()
}

pub fn sign(
    secret: &[u8],
    peer_id: &str,
    method: &str,
    path: &str,
    timestamp: u64,
    nonce: &str,
    body: &[u8],
) -> String {
    hex::encode(hmac_sha3_256(
        secret,
        &canonical(peer_id, method, path, timestamp, nonce, body),
    ))
}

/// The four headers a caller adds, with a fresh nonce.
pub struct SignedHeaders {
    pub peer: String,
    pub timestamp: u64,
    pub nonce: String,
    pub signature: String,
}

pub fn sign_request(
    secret: &[u8],
    peer_id: &str,
    method: &str,
    path: &str,
    body: &[u8],
    now: u64,
) -> SignedHeaders {
    use rand::{rngs::OsRng, RngCore};
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    let nonce = hex::encode(bytes);
    SignedHeaders {
        peer: peer_id.to_string(),
        timestamp: now,
        signature: sign(secret, peer_id, method, path, now, &nonce, body),
        nonce,
    }
}

/// The receiving side: one expected peer, one secret, a replay cache.
pub struct PeerVerifier {
    secret: Vec<u8>,
    peer_id: String,
    seen: Mutex<HashMap<String, u64>>,
}

impl PeerVerifier {
    pub fn new(secret: Vec<u8>, peer_id: &str) -> Result<PeerVerifier, String> {
        if secret.len() < MIN_SECRET_BYTES {
            return Err(format!(
                "the peer secret must be at least {MIN_SECRET_BYTES} bytes"
            ));
        }
        Ok(PeerVerifier {
            secret,
            peer_id: peer_id.to_string(),
            seen: Mutex::new(HashMap::new()),
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Check a request. The nonce is recorded only after the signature has verified, so an unauthenticated
    /// caller cannot fill the replay cache.
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        &self,
        presented_peer: &str,
        timestamp: &str,
        nonce: &str,
        signature: &str,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
    ) -> Result<(), PeerError> {
        let timestamp: u64 = timestamp.parse().map_err(|_| PeerError::Unauthenticated)?;
        let nonce_ok = (16..=64).contains(&nonce.len()) && nonce.bytes().all(|b| b.is_ascii_hexdigit());
        let expected = sign(
            &self.secret,
            &self.peer_id,
            method,
            path,
            timestamp,
            nonce,
            body,
        );
        // Every check runs, and the comparison is constant-time, so the answer's timing does not say
        // which part was wrong.
        let peer_ok = presented_peer.as_bytes().ct_eq(self.peer_id.as_bytes());
        let signature_ok = signature.as_bytes().ct_eq(expected.as_bytes());
        let fresh = timestamp.abs_diff(now) <= WINDOW_SECONDS;
        if !(bool::from(peer_ok & signature_ok) && nonce_ok && fresh) {
            return Err(PeerError::Unauthenticated);
        }
        let mut seen = self.seen.lock().map_err(|_| PeerError::Unauthenticated)?;
        seen.retain(|_, at| now.saturating_sub(*at) <= WINDOW_SECONDS * 2);
        if seen.contains_key(nonce) || seen.len() >= MAX_CACHED_NONCES {
            return Err(PeerError::Unauthenticated);
        }
        seen.insert(nonce.to_string(), now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;
    const SECRET: &[u8; 40] = b"a-peer-secret-of-more-than-32-bytes-long";

    fn verifier() -> PeerVerifier {
        PeerVerifier::new(SECRET.to_vec(), "games").unwrap()
    }

    fn signed(body: &[u8]) -> SignedHeaders {
        sign_request(SECRET, "games", "POST", "/internal/v1/anchor", body, NOW)
    }

    fn check(
        v: &PeerVerifier,
        h: &SignedHeaders,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
    ) -> Result<(), PeerError> {
        v.verify(
            &h.peer,
            &h.timestamp.to_string(),
            &h.nonce,
            &h.signature,
            method,
            path,
            body,
            now,
        )
    }

    #[test]
    fn hmac_matches_published_vectors_from_an_independent_implementation() {
        // Computed with Python's `hmac.new(key, msg, "sha3_256")`, which uses the 136-byte block.
        assert_eq!(
            hex::encode(hmac_sha3_256(b"key", b"The quick brown fox jumps over the lazy dog")),
            "8c6e0683409427f8931711b10ca92a506eb1fafa48fadd66d76126f47ac2c333"
        );
        // A key longer than the block is hashed first.
        assert_eq!(
            hex::encode(hmac_sha3_256(&[0xaa; 200], b"Test Using Larger Than Block-Size Key")),
            LONG_KEY_VECTOR
        );
    }
    const LONG_KEY_VECTOR: &str = "2a281faed560e4fa31e8f3e7d01d5413118308c07804aeb1a5c71a601edb7041";

    #[test]
    fn the_signature_is_pinned_to_the_documented_canonical_string() {
        let signature = sign(SECRET, "games", "post", "/internal/v1/anchor", NOW, &"ab".repeat(16), b"{}");
        assert_eq!(signature, PINNED_SIGNATURE);
    }
    const PINNED_SIGNATURE: &str = "33d98fcdbf3d5b1af6d5d829d33cb005fa3c720d0baa8b6c8832720e0ed553c8";

    #[test]
    fn a_genuine_request_verifies_once_and_a_replay_does_not() {
        let v = verifier();
        let h = signed(b"{\"x\":1}");
        assert_eq!(check(&v, &h, "POST", "/internal/v1/anchor", b"{\"x\":1}", NOW), Ok(()));
        assert_eq!(
            check(&v, &h, "POST", "/internal/v1/anchor", b"{\"x\":1}", NOW + 1),
            Err(PeerError::Unauthenticated),
            "the same nonce twice is a replay"
        );
    }

    #[test]
    fn nothing_that_was_signed_can_be_changed() {
        let body = b"{\"chain_id\":1}";
        let h = signed(body);
        let v = verifier();
        for (method, path, b) in [
            ("GET", "/internal/v1/anchor", &body[..]),
            ("POST", "/internal/v1/other", &body[..]),
            ("POST", "/internal/v1/anchor", b"{\"chain_id\":3}"),
        ] {
            assert_eq!(check(&v, &h, method, path, b, NOW), Err(PeerError::Unauthenticated), "{method} {path}");
        }
        let mut other_peer = signed(body);
        other_peer.peer = "records".into();
        assert!(check(&v, &other_peer, "POST", "/internal/v1/anchor", body, NOW).is_err());
        let mut other_time = signed(body);
        other_time.timestamp += 1;
        assert!(check(&v, &other_time, "POST", "/internal/v1/anchor", body, NOW).is_err());
        let wrong_secret = sign_request(b"another-secret-also-longer-than-32-bytes!!", "games", "POST", "/internal/v1/anchor", body, NOW);
        assert!(check(&v, &wrong_secret, "POST", "/internal/v1/anchor", body, NOW).is_err());
        // None of those failures used up the genuine request's nonce.
        assert_eq!(check(&v, &h, "POST", "/internal/v1/anchor", body, NOW), Ok(()));
    }

    #[test]
    fn the_window_is_enforced_both_ways_and_inclusive() {
        let v = verifier();
        for (offset, ok) in [(0i64, true), (60, true), (-60, true), (61, false), (-61, false)] {
            let h = signed(b"");
            let now = (NOW as i64 + offset) as u64;
            assert_eq!(check(&v, &h, "POST", "/internal/v1/anchor", b"", now).is_ok(), ok, "{offset}");
        }
    }

    #[test]
    fn malformed_headers_are_refused_without_panicking_and_a_short_secret_is_refused_at_load() {
        let v = verifier();
        let h = signed(b"");
        for (timestamp, nonce) in [("not-a-number", h.nonce.as_str()), ("", h.nonce.as_str()), ("1", "short"), ("1", "zz".repeat(16).leak())] {
            assert!(v.verify("games", timestamp, nonce, &h.signature, "POST", "/x", b"", NOW).is_err());
        }
        assert!(PeerVerifier::new(vec![1u8; 31], "games").is_err());
        assert!(PeerVerifier::new(vec![1u8; 32], "games").is_ok());
    }

    #[test]
    fn the_replay_cache_is_pruned_and_cannot_be_filled_by_an_unauthenticated_caller() {
        let v = verifier();
        let h = signed(b"");
        assert!(check(&v, &h, "POST", "/internal/v1/anchor", b"", NOW).is_ok());
        for i in 0..50 {
            let junk = format!("{:032x}", i);
            let _ = v.verify("games", &NOW.to_string(), &junk, "00", "POST", "/x", b"", NOW);
        }
        assert_eq!(v.seen.lock().unwrap().len(), 1, "only a verified nonce is remembered");
        let later = signed(b"");
        // Long after the window, the old entry is gone.
        let mut later = later;
        later.timestamp = NOW + 500;
        later.signature = sign(SECRET, "games", "POST", "/internal/v1/anchor", later.timestamp, &later.nonce, b"");
        assert!(check(&v, &later, "POST", "/internal/v1/anchor", b"", NOW + 500).is_ok());
        assert_eq!(v.seen.lock().unwrap().len(), 1);
    }
}
