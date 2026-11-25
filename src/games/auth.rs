//! Proof that a player controls the wallet whose address they name.
//!
//! An address typed into a request proves nothing: anyone can write anyone's. This module lets the wallet
//! prove control without a password or a secret leaving the device, with a one-time signed challenge.
//!
//! ```text
//! 1. POST /api/games/auth/challenge {address, purpose}
//!       -> the server stores a random one-time challenge for exactly that address and purpose
//! 2. the wallet recomputes the digest below and signs it locally (Dilithium-5, the transfer key)
//! 3. POST /api/games/auth/verify {challenge_id, public_key, signature}
//!       -> the server checks the key hashes to the address and the signature holds, spends the
//!          challenge, and returns a session token bound to that address
//! 4. later requests send the token in `X-Wallet-Session`; the address in the body must be the session's
//!
//! digest = SHA3-256("UltraNet/wallet-challenge/v1"
//!                   || u32le(len) || purpose   (ASCII, today only "play")
//!                   || u32le(len) || address   (64 lowercase hex)
//!                   || challenge id            (32 bytes, the decoded 64-hex id)
//!                   || u64le(issued_at) || u64le(expires_at))      (seconds since the Unix epoch)
//! signature = Dilithium5 detached signature over the 32 digest bytes
//! address   = hex(SHA3-256(public key))
//! ```
//!
//! **Why the digest is safe to sign.** Its domain string is its own, so a signature made for it can never
//! be mistaken for a transaction or an owner proof (`UltraRecord/owner-proof/v1`), and the address, the
//! purpose, the one-time id and both times are inside it, so a signature cannot be moved to another wallet,
//! another purpose or another moment.
//!
//! **A challenge can be used once, and only briefly.** It expires after [`AuthConfig::challenge_ttl_seconds`];
//! spending it is one atomic `UPDATE ... WHERE used_at IS NULL`, so two concurrent verifications of the same
//! valid signature produce one session and one refusal. A challenge tolerates [`AuthConfig::max_attempts`]
//! failed tries and is then dead.
//!
//! **Every failure looks the same.** An unknown, expired, used, mismatched or badly signed challenge all
//! return [`AuthError::Rejected`], so a caller learns that the sign-in did not hold, not which part failed.
//!
//! **Sessions are bearer tokens, so only their hash is stored.** A session belongs to one address and proves
//! one thing: that its holder controlled that wallet when it was issued. It grants no ownership of any
//! AppChain or game: owner actions are authorised by the ledger's owner address, never by a session.
//!
//! **Limits.** Challenge issuance is bounded per address and overall (a challenge costs a row, and every
//! verification costs a signature check); the edge limits in the runbook sit in front of these.

use super::store::{to_i64, to_u64, GamesStore, StoreError};
use crate::QuantumKeyPair;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use sha3::{Digest, Sha3_256};

pub const PURPOSE_PLAY: &str = "play";
const DOMAIN: &[u8] = b"UltraNet/wallet-challenge/v1";
/// A hex public key or signature longer than this is refused before any decoding.
const MAX_HEX_CHARS: usize = 16_384;

pub(super) const AUTH_SCHEMA: &str = "
CREATE TABLE auth_challenges (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    address TEXT NOT NULL CHECK (length(address) = 64),
    purpose TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    used_at INTEGER
);
CREATE INDEX auth_challenges_by_address ON auth_challenges (address, issued_at);
CREATE INDEX auth_challenges_by_time ON auth_challenges (issued_at);
CREATE TABLE auth_sessions (
    token_hash BLOB PRIMARY KEY CHECK (length(token_hash) = 32),
    address TEXT NOT NULL CHECK (length(address) = 64),
    purpose TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);
CREATE INDEX auth_sessions_by_address ON auth_sessions (address);
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthConfig {
    /// How long a challenge may be signed after it is issued.
    pub challenge_ttl_seconds: u64,
    /// How long a session lasts.
    pub session_ttl_seconds: u64,
    /// Failed tries a challenge tolerates before it is dead.
    pub max_attempts: u32,
    /// Challenges one address may be issued per [`AuthConfig::window_seconds`].
    pub challenges_per_address_per_window: u32,
    pub window_seconds: u64,
    /// Challenges the whole service issues per minute.
    pub challenges_per_minute: u32,
    /// Live sessions one address may hold; the oldest are revoked beyond it.
    pub max_sessions_per_address: u32,
}

impl Default for AuthConfig {
    fn default() -> AuthConfig {
        AuthConfig {
            challenge_ttl_seconds: 120,
            session_ttl_seconds: 3_600,
            max_attempts: 5,
            challenges_per_address_per_window: 10,
            window_seconds: 600,
            challenges_per_minute: 120,
            max_sessions_per_address: 5,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Too many challenges for this address, or for the service.
    #[error("too many sign-in requests; try again shortly")]
    RateLimited,
    /// Anything that makes a sign-in not hold. One answer for every cause.
    #[error("the sign-in could not be verified")]
    Rejected,
    /// The request itself is malformed (an address or purpose that cannot be right).
    #[error("{0}")]
    Invalid(String),
}

impl From<rusqlite::Error> for AuthError {
    fn from(error: rusqlite::Error) -> Self {
        AuthError::Store(StoreError::Database(error))
    }
}

/// What the wallet is asked to sign, and when it stops being valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    pub address: String,
    pub purpose: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl Challenge {
    pub fn digest(&self) -> Result<[u8; 32], AuthError> {
        challenge_digest(&self.purpose, &self.address, &self.id, self.issued_at, self.expires_at)
    }
}

/// A verified sign-in: the wallet that proved control, and until when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub address: String,
    pub purpose: String,
    pub expires_at: u64,
}

pub fn is_address(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn id_bytes(id: &str) -> Result<[u8; 32], AuthError> {
    if !is_address(id) {
        return Err(AuthError::Rejected);
    }
    let bytes = hex::decode(id).map_err(|_| AuthError::Rejected)?;
    bytes.try_into().map_err(|_| AuthError::Rejected)
}

pub fn challenge_digest(
    purpose: &str,
    address: &str,
    challenge_id: &str,
    issued_at: u64,
    expires_at: u64,
) -> Result<[u8; 32], AuthError> {
    let id = id_bytes(challenge_id)?;
    let mut hasher = Sha3_256::new();
    hasher.update(DOMAIN);
    for part in [purpose, address] {
        hasher.update((part.len() as u32).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(id);
    hasher.update(issued_at.to_le_bytes());
    hasher.update(expires_at.to_le_bytes());
    Ok(hasher.finalize().into())
}

fn decode(value: &str) -> Result<Vec<u8>, AuthError> {
    if value.is_empty() || value.len() > MAX_HEX_CHARS {
        return Err(AuthError::Rejected);
    }
    hex::decode(value).map_err(|_| AuthError::Rejected)
}

pub fn token_hash(token: &str) -> [u8; 32] {
    Sha3_256::digest(token.as_bytes()).into()
}

/// Check a signature over a challenge and return the address it proves. Pure: no clock, no database.
pub fn verify_signature(challenge: &Challenge, public_key_hex: &str, signature_hex: &str) -> Result<String, AuthError> {
    let public_key = decode(public_key_hex)?;
    let signature = decode(signature_hex)?;
    // The key must hash to the address the challenge was issued for, before any signature work.
    if QuantumKeyPair::address_from_public_key(&public_key) != challenge.address {
        return Err(AuthError::Rejected);
    }
    let digest = challenge.digest()?;
    if !QuantumKeyPair::verify(&public_key, &digest, &signature) {
        return Err(AuthError::Rejected);
    }
    Ok(challenge.address.clone())
}

impl GamesStore {
    /// Issue a one-time challenge for `address`, or refuse if this address or the service asks too often.
    /// The count and the insert are one transaction, so concurrent callers cannot slip past the limit.
    pub fn issue_challenge(
        &self,
        id: &str,
        address: &str,
        purpose: &str,
        now: u64,
        config: &AuthConfig,
    ) -> Result<Challenge, AuthError> {
        if !is_address(address) {
            return Err(AuthError::Invalid("address must be 64 lowercase hexadecimal characters".into()));
        }
        if purpose != PURPOSE_PLAY {
            return Err(AuthError::Invalid("purpose must be \"play\"".into()));
        }
        id_bytes(id)?;
        let expires_at = now.saturating_add(config.challenge_ttl_seconds);
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let per_address: i64 = tx.query_row(
            "SELECT COUNT(*) FROM auth_challenges WHERE address = ?1 AND issued_at > ?2",
            params![address, to_i64(now.saturating_sub(config.window_seconds))?],
            |row| row.get(0),
        )?;
        let overall: i64 = tx.query_row(
            "SELECT COUNT(*) FROM auth_challenges WHERE issued_at > ?1",
            params![to_i64(now.saturating_sub(60))?],
            |row| row.get(0),
        )?;
        if to_u64(per_address)? >= u64::from(config.challenges_per_address_per_window)
            || to_u64(overall)? >= u64::from(config.challenges_per_minute)
        {
            return Err(AuthError::RateLimited);
        }
        tx.execute(
            "INSERT INTO auth_challenges (id, address, purpose, issued_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, address, purpose, to_i64(now)?, to_i64(expires_at)?],
        )?;
        // Old rows are of no use to anyone; keeping a day of them is enough to count against.
        tx.execute(
            "DELETE FROM auth_challenges WHERE issued_at < ?1",
            params![to_i64(now.saturating_sub(86_400))?],
        )?;
        tx.commit()?;
        Ok(Challenge {
            id: id.to_string(),
            address: address.to_string(),
            purpose: purpose.to_string(),
            issued_at: now,
            expires_at,
        })
    }

    /// Count one verification attempt against a challenge and return it, only if it is still usable.
    /// Unknown, used, expired and over-tried challenges are all `Rejected`.
    pub fn begin_verification(&self, id: &str, now: u64, config: &AuthConfig) -> Result<Challenge, AuthError> {
        id_bytes(id)?;
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row = tx
            .query_row(
                "SELECT address, purpose, issued_at, expires_at, attempts, used_at FROM auth_challenges WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((address, purpose, issued_at, expires_at, attempts, used_at)) = row else {
            return Err(AuthError::Rejected);
        };
        let expires_at = to_u64(expires_at)?;
        if used_at.is_some() || now > expires_at || to_u64(attempts)? >= u64::from(config.max_attempts) {
            return Err(AuthError::Rejected);
        }
        tx.execute("UPDATE auth_challenges SET attempts = attempts + 1 WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(Challenge { id: id.to_string(), address, purpose, issued_at: to_u64(issued_at)?, expires_at })
    }

    /// Spend a verified challenge and open the session, atomically. If the challenge was already spent
    /// (a replay, or the loser of a race between two identical requests) nothing is created.
    pub fn finish_verification(
        &self,
        challenge: &Challenge,
        token_hash: &[u8; 32],
        now: u64,
        config: &AuthConfig,
    ) -> Result<Session, AuthError> {
        let expires_at = now.saturating_add(config.session_ttl_seconds);
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let spent = tx.execute(
            "UPDATE auth_challenges SET used_at = ?2 WHERE id = ?1 AND used_at IS NULL AND expires_at >= ?2",
            params![challenge.id, to_i64(now)?],
        )?;
        if spent != 1 {
            return Err(AuthError::Rejected);
        }
        tx.execute(
            "INSERT INTO auth_sessions (token_hash, address, purpose, issued_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![token_hash.as_slice(), challenge.address, challenge.purpose, to_i64(now)?, to_i64(expires_at)?],
        )?;
        // Keep the newest sessions only: a stolen-and-forgotten token cannot pile up beside live ones.
        tx.execute(
            "UPDATE auth_sessions SET revoked_at = ?3
             WHERE address = ?1 AND revoked_at IS NULL AND token_hash NOT IN (
                 SELECT token_hash FROM auth_sessions
                 WHERE address = ?1 AND revoked_at IS NULL ORDER BY issued_at DESC, rowid DESC LIMIT ?2)",
            params![challenge.address, i64::from(config.max_sessions_per_address), to_i64(now)?],
        )?;
        tx.commit()?;
        Ok(Session { address: challenge.address.clone(), purpose: challenge.purpose.clone(), expires_at })
    }

    /// The session behind a token, if it is live: known, not revoked, not expired.
    pub fn session_for(&self, token_hash: &[u8; 32], now: u64) -> Result<Option<Session>, AuthError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(
                "SELECT address, purpose, expires_at, revoked_at FROM auth_sessions WHERE token_hash = ?1",
                params![token_hash.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((address, purpose, expires_at, revoked_at)) = row else {
            return Ok(None);
        };
        let expires_at = to_u64(expires_at)?;
        if revoked_at.is_some() || now > expires_at {
            return Ok(None);
        }
        Ok(Some(Session { address, purpose, expires_at }))
    }

    /// End one session. Returns whether a live session was ended.
    pub fn revoke_session(&self, token_hash: &[u8; 32], now: u64) -> Result<bool, AuthError> {
        let guard = self.lock()?;
        let changed = guard.execute(
            "UPDATE auth_sessions SET revoked_at = ?2 WHERE token_hash = ?1 AND revoked_at IS NULL",
            params![token_hash.as_slice(), to_i64(now)?],
        )?;
        Ok(changed == 1)
    }

    /// End every live session of an address (an operator's tool, or a wallet that was reported lost).
    pub fn revoke_sessions_for(&self, address: &str, now: u64) -> Result<u64, AuthError> {
        let guard = self.lock()?;
        let changed = guard.execute(
            "UPDATE auth_sessions SET revoked_at = ?2 WHERE address = ?1 AND revoked_at IS NULL",
            params![address, to_i64(now)?],
        )?;
        Ok(changed as u64)
    }
}
