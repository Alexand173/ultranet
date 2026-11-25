//! The game catalog: any AppChain owner registers games on their own chain.
//!
//! Nothing here knows a game by name. A game is a row with a platform-generated, immutable id, the real
//! L1 `chain_id` it belongs to, and display metadata that is only ever a label.
//!
//! **Who may administer a game: the current L1 owner of its chain, read from the ledger every time.**
//! `created_by` records who registered it and is history, never authority. If the chain is handed to a new
//! owner, the old owner can no longer change the game and the new one can; the game's id, its records and
//! its audit trail stay as they were.
//!
//! **Every administrative action is signed and then checked against the ledger.** The owner signs a digest
//! with the wallet key (the transfer key); the service verifies the signature, derives the address from the
//! public key, asks the ledger who owns the chain *now*, and refuses unless they are the same address. A
//! player's wallet session (see `auth.rs`) is not accepted here and grants nothing.
//!
//! ```text
//! digest = SHA3-256("UltraNet/game-admin/v1"
//!                   || lp(action)            "register" | "update" | "status"
//!                   || u32le(chain_id)
//!                   || lp(game_id)           empty for "register"
//!                   || lp(name) || lp(description) || lp(status)     the final values, empty if not used
//!                   || lp(nonce)             16 to 64 lowercase hex characters, chosen by the signer, fresh each time
//!                   || u64le(timestamp))                              seconds, within 5 minutes of the server
//! lp(x) = u32le(len) || bytes
//! ```
//!
//! **A signed request works once.** The digest of every accepted action is stored with a UNIQUE constraint
//! in the same transaction as the change, so the identical request cannot be replayed (a replayed
//! `register` would otherwise create a second game), and the timestamp window keeps the table small. The
//! nonce makes two genuinely separate requests with the same fields distinct, so an owner can register two
//! games with the same label in the same second; it is not secret and need not be unpredictable, only fresh.
//!
//! **What a registered game can do today: nothing that moves money.** Its validation scheme is
//! `record_only`. A game pays rewards only once an independently verified scheme, an approved reward policy
//! and its own funded pool exist (later stages); `reward_policy_ref` stays empty until then.
//!
//! **Recovery.** The schema step is additive and transactional (`upgrade_to_catalog`). An older binary
//! refuses a newer database rather than misreading it, so the way back is the backup taken before the
//! upgrade; no existing table is altered.

use super::store::{to_i64, to_u64, GamesStore, StoreError};
use crate::QuantumKeyPair;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha3::{Digest, Sha3_256};

pub const ACTION_REGISTER: &str = "register";
pub const ACTION_UPDATE: &str = "update";
pub const ACTION_STATUS: &str = "status";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_DISABLED: &str = "disabled";
/// The only validation scheme a newly registered game may have until verified schemes exist.
pub const SCHEME_RECORD_ONLY: &str = "record_only";

/// A proof older or newer than this, against the server's clock, is refused.
pub const PROOF_WINDOW_SECONDS: u64 = 300;
pub const NAME_MAX_CHARS: usize = 64;
pub const DESCRIPTION_MAX_CHARS: usize = 280;
/// A hex public key or signature longer than this is refused before any decoding.
const MAX_HEX_CHARS: usize = 16_384;

pub(super) const CATALOG_SCHEMA: &str = "
CREATE TABLE platform_games (
    game_id TEXT PRIMARY KEY CHECK (length(game_id) = 32),
    chain_id INTEGER NOT NULL CHECK (chain_id > 0),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    validation_scheme TEXT NOT NULL,
    adapter_version INTEGER NOT NULL DEFAULT 0,
    adapter_ref TEXT,
    status TEXT NOT NULL CHECK (status IN ('active','disabled')),
    reward_policy_ref TEXT,
    created_by TEXT NOT NULL CHECK (length(created_by) = 64),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX platform_games_by_chain ON platform_games (chain_id);
CREATE TABLE platform_game_audit (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id TEXT NOT NULL,
    at INTEGER NOT NULL,
    actor TEXT NOT NULL CHECK (length(actor) = 64),
    action TEXT NOT NULL,
    chain_owner TEXT NOT NULL CHECK (length(chain_owner) = 64),
    detail TEXT NOT NULL,
    digest BLOB NOT NULL UNIQUE CHECK (length(digest) = 32)
);
CREATE INDEX platform_game_audit_by_game ON platform_game_audit (game_id, seq);
";

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A bad signature, a wrong key, a stale timestamp or a replayed request: one answer for all of them.
    #[error("the owner proof did not verify")]
    ProofInvalid,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("this AppChain already has the most games it may hold")]
    ChainFull,
}

impl From<rusqlite::Error> for CatalogError {
    fn from(error: rusqlite::Error) -> Self {
        CatalogError::Store(StoreError::Database(error))
    }
}

/// One registered game, as the catalog shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogGame {
    pub game_id: String,
    pub chain_id: u32,
    /// A label. Not unique and never an identity.
    pub name: String,
    pub description: String,
    pub validation_scheme: String,
    pub adapter_version: u32,
    pub adapter_ref: Option<String>,
    pub status: String,
    pub reward_policy_ref: Option<String>,
    /// Who registered it. History only: authority is the chain's current owner, read from the ledger.
    pub created_by: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// One administrative action, as recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub game_id: String,
    pub at: u64,
    /// The address that signed.
    pub actor: String,
    pub action: String,
    /// The chain's owner on the ledger when the action was accepted.
    pub chain_owner: String,
    pub detail: String,
}

/// What is written beside a change, in the same transaction.
pub struct AuditRecord<'a> {
    pub actor: &'a str,
    pub action: &'a str,
    pub chain_owner: &'a str,
    pub detail: &'a str,
    pub digest: [u8; 32],
}

fn lp(hasher: &mut Sha3_256, part: &str) {
    hasher.update((part.len() as u32).to_le_bytes());
    hasher.update(part.as_bytes());
}

#[allow(clippy::too_many_arguments)]
pub fn admin_digest(
    action: &str,
    chain_id: u32,
    game_id: &str,
    name: &str,
    description: &str,
    status: &str,
    nonce: &str,
    timestamp: u64,
) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraNet/game-admin/v1");
    lp(&mut hasher, action);
    hasher.update(chain_id.to_le_bytes());
    for part in [game_id, name, description, status, nonce] {
        lp(&mut hasher, part);
    }
    hasher.update(timestamp.to_le_bytes());
    hasher.finalize().into()
}

fn decode(value: &str) -> Result<Vec<u8>, CatalogError> {
    if value.is_empty() || value.len() > MAX_HEX_CHARS {
        return Err(CatalogError::ProofInvalid);
    }
    hex::decode(value).map_err(|_| CatalogError::ProofInvalid)
}

/// Check a signature over an admin digest and return the address that made it. Pure: no clock, no ledger.
pub fn verify_admin_signature(digest: &[u8; 32], public_key_hex: &str, signature_hex: &str) -> Result<String, CatalogError> {
    let public_key = decode(public_key_hex)?;
    let signature = decode(signature_hex)?;
    if !QuantumKeyPair::verify(&public_key, digest, &signature) {
        return Err(CatalogError::ProofInvalid);
    }
    Ok(QuantumKeyPair::address_from_public_key(&public_key))
}

/// A nonce is 16 to 64 lowercase hex characters. Anything else is a proof that cannot be right.
pub fn check_nonce(nonce: &str) -> Result<(), CatalogError> {
    let well_formed = (16..=64).contains(&nonce.len()) && nonce.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if well_formed { Ok(()) } else { Err(CatalogError::ProofInvalid) }
}

pub fn check_timestamp(timestamp: u64, now: u64) -> Result<(), CatalogError> {
    if timestamp.abs_diff(now) > PROOF_WINDOW_SECONDS {
        return Err(CatalogError::ProofInvalid);
    }
    Ok(())
}

/// Names and descriptions are labels: trimmed, bounded, free of control characters. Never unique.
pub fn clean_label(value: &str, what: &str, max_chars: usize, required: bool) -> Result<String, CatalogError> {
    let trimmed = value.trim();
    if required && trimmed.is_empty() {
        return Err(CatalogError::Invalid(format!("{what} is required")));
    }
    if trimmed.chars().count() > max_chars {
        return Err(CatalogError::Invalid(format!("{what} may be at most {max_chars} characters")));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(CatalogError::Invalid(format!("{what} may not contain control characters")));
    }
    Ok(trimmed.to_string())
}

type GameRow = (String, i64, String, String, String, i64, Option<String>, String, Option<String>, String, i64, i64);

fn game_from(row: GameRow) -> Result<CatalogGame, StoreError> {
    Ok(CatalogGame {
        game_id: row.0,
        chain_id: u32::try_from(row.1).map_err(|_| StoreError::NumericRange)?,
        name: row.2,
        description: row.3,
        validation_scheme: row.4,
        adapter_version: u32::try_from(row.5).map_err(|_| StoreError::NumericRange)?,
        adapter_ref: row.6,
        status: row.7,
        reward_policy_ref: row.8,
        created_by: row.9,
        created_at: to_u64(row.10)?,
        updated_at: to_u64(row.11)?,
    })
}

const GAME_COLUMNS: &str = "game_id, chain_id, name, description, validation_scheme, adapter_version, adapter_ref, status, reward_policy_ref, created_by, created_at, updated_at";

fn read_game(row: &rusqlite::Row<'_>) -> rusqlite::Result<GameRow> {
    Ok((
        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
        row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
    ))
}

pub(super) fn write_audit(tx: &rusqlite::Transaction<'_>, game_id: &str, now: u64, audit: &AuditRecord<'_>) -> Result<(), CatalogError> {
    let inserted = tx.execute(
        "INSERT INTO platform_game_audit (game_id, at, actor, action, chain_owner, detail, digest) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![game_id, to_i64(now)?, audit.actor, audit.action, audit.chain_owner, audit.detail, audit.digest.as_slice()],
    );
    match inserted {
        Ok(_) => Ok(()),
        // The digest is UNIQUE: the identical signed request has been used before.
        Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(CatalogError::ProofInvalid)
        }
        Err(other) => Err(other.into()),
    }
}

impl GamesStore {
    /// Register a game. The count, the insert and the audit row are one transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn catalog_register(
        &self,
        game_id: &str,
        chain_id: u32,
        name: &str,
        description: &str,
        now: u64,
        max_per_chain: u32,
        audit: &AuditRecord<'_>,
    ) -> Result<CatalogGame, CatalogError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let held: i64 = tx.query_row("SELECT COUNT(*) FROM platform_games WHERE chain_id = ?1", params![i64::from(chain_id)], |row| row.get(0))?;
        if to_u64(held)? >= u64::from(max_per_chain) {
            return Err(CatalogError::ChainFull);
        }
        tx.execute(
            "INSERT INTO platform_games (game_id, chain_id, name, description, validation_scheme, status, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?7)",
            params![game_id, i64::from(chain_id), name, description, SCHEME_RECORD_ONLY, audit.actor, to_i64(now)?],
        )?;
        write_audit(&tx, game_id, now, audit)?;
        tx.commit()?;
        drop(guard);
        self.catalog_game(game_id)?.ok_or_else(|| CatalogError::NotFound("game vanished".into()))
    }

    pub fn catalog_update(&self, game_id: &str, name: &str, description: &str, now: u64, audit: &AuditRecord<'_>) -> Result<CatalogGame, CatalogError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE platform_games SET name = ?2, description = ?3, updated_at = ?4 WHERE game_id = ?1",
            params![game_id, name, description, to_i64(now)?],
        )?;
        if changed != 1 {
            return Err(CatalogError::NotFound("no such game".into()));
        }
        write_audit(&tx, game_id, now, audit)?;
        tx.commit()?;
        drop(guard);
        self.catalog_game(game_id)?.ok_or_else(|| CatalogError::NotFound("game vanished".into()))
    }

    pub fn catalog_set_status(&self, game_id: &str, status: &str, now: u64, audit: &AuditRecord<'_>) -> Result<CatalogGame, CatalogError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE platform_games SET status = ?2, updated_at = ?3 WHERE game_id = ?1",
            params![game_id, status, to_i64(now)?],
        )?;
        if changed != 1 {
            return Err(CatalogError::NotFound("no such game".into()));
        }
        write_audit(&tx, game_id, now, audit)?;
        tx.commit()?;
        drop(guard);
        self.catalog_game(game_id)?.ok_or_else(|| CatalogError::NotFound("game vanished".into()))
    }

    pub fn catalog_game(&self, game_id: &str) -> Result<Option<CatalogGame>, CatalogError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(&format!("SELECT {GAME_COLUMNS} FROM platform_games WHERE game_id = ?1"), params![game_id], read_game)
            .optional()?;
        Ok(row.map(game_from).transpose()?)
    }

    /// Games in registration order, optionally of one chain.
    pub fn catalog_games(&self, chain_id: Option<u32>) -> Result<Vec<CatalogGame>, CatalogError> {
        let guard = self.lock()?;
        let (sql, bound): (String, Vec<i64>) = match chain_id {
            Some(chain) => (format!("SELECT {GAME_COLUMNS} FROM platform_games WHERE chain_id = ?1 ORDER BY created_at, game_id"), vec![i64::from(chain)]),
            None => (format!("SELECT {GAME_COLUMNS} FROM platform_games ORDER BY created_at, game_id"), vec![]),
        };
        let mut statement = guard.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(bound.iter()), read_game)?;
        let mut games = Vec::new();
        for row in rows {
            games.push(game_from(row?)?);
        }
        Ok(games)
    }

    pub fn catalog_history(&self, game_id: &str) -> Result<Vec<AuditEntry>, CatalogError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare(
            "SELECT seq, game_id, at, actor, action, chain_owner, detail FROM platform_game_audit WHERE game_id = ?1 ORDER BY seq",
        )?;
        let rows = statement.query_map(params![game_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let row = row?;
            entries.push(AuditEntry { seq: to_u64(row.0)?, game_id: row.1, at: to_u64(row.2)?, actor: row.3, action: row.4, chain_owner: row.5, detail: row.6 });
        }
        Ok(entries)
    }
}
