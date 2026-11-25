//! Per-game reward pools, and the reward policies that decide what a verified win is worth.
//!
//! **Who pays.** The owner of a game's AppChain funds that game's pool, and nobody else does. UltraNet does
//! not fund any game, including UltraGrid and Ultra Chess, and there is no shared pool: a reward is paid
//! only from the funded, unreserved balance of the game it was earned in.
//!
//! **How a deposit is bound to a game.** The owner makes an ordinary L1 transfer to the platform payout
//! address, then sends an owner-signed *claim* naming that transaction and the game. The service checks the
//! ledger: the transaction is confirmed, its sender is the signer and the chain's current owner, its recipient
//! is the payout address. It then credits the game's pool with exactly that amount. The transaction hash is
//! the primary key of `game_pool_deposits`, so one transfer funds one game once and can never be claimed
//! again, by the same game or another. Nothing is credited that the ledger does not show.
//!
//! **Isolation is accounting, and it is enforced where money moves.** The on-chain funds sit under one payout
//! key. What keeps one game from spending another's is the pool ledger: every pool carries
//! `funded`, `reserved`, `paid` and `withdrawn`, with a database CHECK that
//! `reserved + paid + withdrawn <= funded`, and every change is one `BEGIN IMMEDIATE` transaction. A solvency
//! check compares everything the service owes, across all pools and the older Grid and Chess balances, with the
//! payout wallet's real balance; if the wallet holds less, reservations stop for everyone until an operator
//! looks. This is custody by the platform, stated plainly: the guarantee is the ledger, not a separate key.
//!
//! **What may earn money.** A credit is created only for a result that
//!   1. belongs to a game whose validation scheme is `verified_adapter` and whose approved adapter hash is the
//!      one that judged it (a platform decision; a game starts `record_only`),
//!   2. was judged by that adapter's [`ValidatedResult`] (two agreeing runs), with an outcome the active
//!      policy pays for, and
//!   3. fits the active policy's limits and the pool's unreserved balance.
//!   Anything else is refused and creates no promise. A result id can be credited once (`UNIQUE(game_id,
//!   result_id)`), so a replay, a retry or a concurrent duplicate adds nothing.
//!
//! **Reward policy: proposed by the owner, approved by the platform.** The owner proposes terms (a fixed reward
//! per paid outcome, which outcomes pay, per-player and per-game daily limits, when it takes effect), signed
//! like any owner action. The proposal is checked against the platform ceilings (the existing `MAX_*` limits)
//! and stored as an immutable row with a hash of its exact terms. It does nothing until a platform operator
//! approves it; the approver and time are recorded, and approving a version supersedes the one before. Terms
//! are never edited: a change is a new version, and every credit records the policy version and hash it was
//! computed under, so completed matches cannot be re-priced.
//!
//! **A policy can promise nothing the pool cannot pay.** [`GamesService::promised_reward`] answers with a
//! reward only while a policy is active *and* the pool's unreserved balance covers at least one.
//!
//! **Withdrawal.** The owner may take back unreserved funds. The request is signed, limited to the unreserved
//! balance, moves the amount from `funded` accounting into `withdrawn`, and queues one transfer to the chain's
//! current owner. Executing that transfer belongs to the payout worker (a later stage); nothing here sends it.
//!
//! **Reward money is separate from AppChain fees.** Registration, anchor and transfer fees are L1 fees paid by
//! the owner's wallet to validators; nothing in this module reads, changes or reuses them.

use super::adapter::{Outcome, ValidatedResult};
use super::catalog::{self, AuditRecord, CatalogError, CatalogGame, SCHEME_RECORD_ONLY};
use super::config::{MAX_REWARDED_PER_ADDRESS_PER_DAY, MAX_REWARDED_PER_GAME_PER_DAY, MAX_REWARD_UNITS};
use super::service::{AdminProof, GamesError, GamesService};
use super::store::{to_i64, to_u64, GamesStore, StoreError};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use sha3::{Digest, Sha3_256};

pub const SCHEME_VERIFIED_ADAPTER: &str = "verified_adapter";
pub const ACTION_FUND: &str = "pool-fund";
pub const ACTION_WITHDRAW: &str = "pool-withdraw";
pub const ACTION_POLICY_PROPOSE: &str = "policy-propose";
pub const ACTION_POLICY_APPROVE: &str = "policy-approve";
pub const ACTION_POLICY_REJECT: &str = "policy-reject";
pub const ACTION_ADAPTER_APPROVE: &str = "adapter-approve";
const UTC_DAY_SECONDS: u64 = 86_400;

pub(super) const POOLS_SCHEMA: &str = "
CREATE TABLE game_pools (
    game_id TEXT PRIMARY KEY CHECK (length(game_id) = 32),
    funded INTEGER NOT NULL DEFAULT 0 CHECK (funded >= 0),
    reserved INTEGER NOT NULL DEFAULT 0 CHECK (reserved >= 0),
    paid INTEGER NOT NULL DEFAULT 0 CHECK (paid >= 0),
    withdrawn INTEGER NOT NULL DEFAULT 0 CHECK (withdrawn >= 0),
    CHECK (reserved + paid + withdrawn <= funded)
);
CREATE TABLE game_pool_deposits (
    tx_hash TEXT PRIMARY KEY CHECK (length(tx_hash) = 64),
    game_id TEXT NOT NULL,
    funder TEXT NOT NULL CHECK (length(funder) = 64),
    amount_units INTEGER NOT NULL CHECK (amount_units > 0),
    claimed_at INTEGER NOT NULL
);
CREATE INDEX game_pool_deposits_by_game ON game_pool_deposits (game_id);
CREATE TABLE game_pool_credits (
    credit_id TEXT PRIMARY KEY CHECK (length(credit_id) = 32),
    game_id TEXT NOT NULL,
    result_id TEXT NOT NULL,
    player TEXT NOT NULL CHECK (length(player) = 64),
    amount_units INTEGER NOT NULL CHECK (amount_units > 0),
    policy_version INTEGER NOT NULL,
    policy_hash BLOB NOT NULL CHECK (length(policy_hash) = 32),
    module_hash BLOB NOT NULL CHECK (length(module_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('reserved','paid','released')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (game_id, result_id)
);
CREATE INDEX game_pool_credits_by_day ON game_pool_credits (game_id, player, created_at);
CREATE TABLE game_pool_withdrawals (
    withdrawal_id TEXT PRIMARY KEY CHECK (length(withdrawal_id) = 32),
    game_id TEXT NOT NULL,
    to_address TEXT NOT NULL CHECK (length(to_address) = 64),
    amount_units INTEGER NOT NULL CHECK (amount_units > 0),
    state TEXT NOT NULL CHECK (state IN ('queued','paid')),
    created_at INTEGER NOT NULL
);
CREATE TABLE game_pool_ledger (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id TEXT NOT NULL,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('deposit','reserve','release','pay','withdraw')),
    amount_units INTEGER NOT NULL,
    reference TEXT NOT NULL
);
CREATE INDEX game_pool_ledger_by_game ON game_pool_ledger (game_id, seq);
CREATE TABLE game_reward_policies (
    game_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    reward_units INTEGER NOT NULL CHECK (reward_units > 0),
    eligible_outcomes TEXT NOT NULL CHECK (eligible_outcomes IN ('win','tie','win,tie')),
    per_player_per_day INTEGER NOT NULL CHECK (per_player_per_day > 0),
    per_game_per_day INTEGER NOT NULL CHECK (per_game_per_day > 0),
    effective_from INTEGER NOT NULL,
    policy_hash BLOB NOT NULL UNIQUE CHECK (length(policy_hash) = 32),
    proposed_by TEXT NOT NULL CHECK (length(proposed_by) = 64),
    proposed_at INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('proposed','approved','rejected','superseded')),
    decided_by TEXT,
    decided_at INTEGER,
    decision_note TEXT,
    PRIMARY KEY (game_id, version)
);
CREATE TABLE pool_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    frozen INTEGER NOT NULL CHECK (frozen IN (0,1)),
    reason TEXT
);
INSERT INTO pool_state (id, frozen, reason) VALUES (1, 0, NULL);
";

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    /// The game is `record_only`, the adapter is not the approved one, or the result is not a validated one.
    #[error("only a result verified by the game's approved adapter can earn a reward")]
    NotVerified,
    #[error("the game has no approved, active reward policy")]
    NoActivePolicy,
    #[error("the policy does not pay for this outcome")]
    NotEligible,
    #[error("a limit of the reward policy has been reached")]
    LimitReached,
    #[error("the pool holds {available} unreserved units and {needed} are needed")]
    InsufficientFunds { available: u64, needed: u64 },
    #[error("that transaction has already funded a game")]
    AlreadyClaimed,
    #[error("rewards are paused until the payout wallet's balance has been checked")]
    Frozen,
    #[error("a result with that id is already credited to someone else or for another amount")]
    Conflict,
    #[error("{0}")]
    State(String),
}

impl From<rusqlite::Error> for PoolError {
    fn from(error: rusqlite::Error) -> Self {
        PoolError::Store(StoreError::Database(error))
    }
}

/// What a pool holds, and what that leaves available.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolView {
    pub game_id: String,
    pub funded: u64,
    pub reserved: u64,
    pub paid: u64,
    pub withdrawn: u64,
    /// `funded - reserved - paid - withdrawn`: the only money a new reward or withdrawal may use.
    pub available: u64,
}

/// A reward promised for one verified result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Credit {
    pub credit_id: String,
    pub game_id: String,
    pub result_id: String,
    pub player: String,
    pub amount_units: u64,
    pub policy_version: u32,
    pub policy_hash: String,
    pub module_hash: String,
    pub state: String,
    pub created_at: u64,
}

/// The terms a game owner proposes. A fixed reward per paid outcome, and the limits around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyTerms {
    pub reward_units: u64,
    /// `win`, `tie` or `win,tie`. A loss, a cancellation or an invalid result never pays.
    pub eligible_outcomes: String,
    pub per_player_per_day: u32,
    pub per_game_per_day: u64,
    pub effective_from: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyRow {
    pub game_id: String,
    pub version: u32,
    pub reward_units: u64,
    pub eligible_outcomes: String,
    pub per_player_per_day: u32,
    pub per_game_per_day: u64,
    pub effective_from: u64,
    pub policy_hash: String,
    pub proposed_by: String,
    pub proposed_at: u64,
    pub status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<u64>,
    pub decision_note: Option<String>,
}

pub fn policy_hash(game_id: &str, version: u32, terms: &PolicyTerms) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraNet/reward-policy/v1");
    for part in [game_id, terms.eligible_outcomes.as_str()] {
        hasher.update((part.len() as u32).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(version.to_le_bytes());
    hasher.update(terms.reward_units.to_le_bytes());
    hasher.update(terms.per_player_per_day.to_le_bytes());
    hasher.update(terms.per_game_per_day.to_le_bytes());
    hasher.update(terms.effective_from.to_le_bytes());
    hasher.finalize().into()
}

/// The platform's ceilings on what any policy may promise. They are the limits the platform already enforces
/// for its own games; no new number is introduced here.
pub fn check_platform_limits(terms: &PolicyTerms) -> Result<(), PoolError> {
    if terms.reward_units == 0 || terms.reward_units > MAX_REWARD_UNITS {
        return Err(PoolError::Invalid(format!("the reward must be 1 to {MAX_REWARD_UNITS} base units")));
    }
    if terms.per_player_per_day == 0 || terms.per_player_per_day > MAX_REWARDED_PER_ADDRESS_PER_DAY {
        return Err(PoolError::Invalid(format!("rewards per player per day must be 1 to {MAX_REWARDED_PER_ADDRESS_PER_DAY}")));
    }
    if terms.per_game_per_day == 0 || terms.per_game_per_day > MAX_REWARDED_PER_GAME_PER_DAY {
        return Err(PoolError::Invalid(format!("rewards per game per day must be 1 to {MAX_REWARDED_PER_GAME_PER_DAY}")));
    }
    if !matches!(terms.eligible_outcomes.as_str(), "win" | "tie" | "win,tie") {
        return Err(PoolError::Invalid("eligible outcomes must be win, tie or win,tie".into()));
    }
    Ok(())
}

fn outcome_name(outcome: Outcome) -> Option<&'static str> {
    match outcome {
        Outcome::Win => Some("win"),
        Outcome::Tie => Some("tie"),
        Outcome::Loss | Outcome::Cancelled | Outcome::Invalid => None,
    }
}

fn day_start(now: u64) -> u64 {
    now - now % UTC_DAY_SECONDS
}

fn ledger(tx: &Transaction<'_>, game_id: &str, now: u64, kind: &str, amount: u64, reference: &str) -> Result<(), PoolError> {
    tx.execute(
        "INSERT INTO game_pool_ledger (game_id, at, kind, amount_units, reference) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![game_id, to_i64(now)?, kind, to_i64(amount)?, reference],
    )?;
    Ok(())
}

fn read_pool(tx: &Transaction<'_>, game_id: &str) -> Result<PoolView, PoolError> {
    let row = tx
        .query_row(
            "SELECT funded, reserved, paid, withdrawn FROM game_pools WHERE game_id = ?1",
            params![game_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
        )
        .optional()?;
    let (funded, reserved, paid, withdrawn) = match row {
        Some(row) => (to_u64(row.0)?, to_u64(row.1)?, to_u64(row.2)?, to_u64(row.3)?),
        None => (0, 0, 0, 0),
    };
    Ok(PoolView {
        game_id: game_id.to_string(),
        funded,
        reserved,
        paid,
        withdrawn,
        available: funded.saturating_sub(reserved + paid + withdrawn),
    })
}

fn require_game(tx: &Transaction<'_>, game_id: &str) -> Result<(String, String, Option<String>), PoolError> {
    tx.query_row(
        "SELECT status, validation_scheme, adapter_ref FROM platform_games WHERE game_id = ?1",
        params![game_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
    )
    .optional()?
    .ok_or_else(|| PoolError::NotFound("no such game".into()))
}

type PolicySql = (String, i64, i64, String, i64, i64, i64, Vec<u8>, String, i64, String, Option<String>, Option<i64>, Option<String>);
const POLICY_COLUMNS: &str = "game_id, version, reward_units, eligible_outcomes, per_player_per_day, per_game_per_day, effective_from, policy_hash, proposed_by, proposed_at, status, decided_by, decided_at, decision_note";

fn read_policy(row: &rusqlite::Row<'_>) -> rusqlite::Result<PolicySql> {
    Ok((
        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?,
        row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?,
    ))
}

fn policy_from(row: PolicySql) -> Result<PolicyRow, StoreError> {
    Ok(PolicyRow {
        game_id: row.0,
        version: u32::try_from(row.1).map_err(|_| StoreError::NumericRange)?,
        reward_units: to_u64(row.2)?,
        eligible_outcomes: row.3,
        per_player_per_day: u32::try_from(row.4).map_err(|_| StoreError::NumericRange)?,
        per_game_per_day: to_u64(row.5)?,
        effective_from: to_u64(row.6)?,
        policy_hash: hex::encode(row.7),
        proposed_by: row.8,
        proposed_at: to_u64(row.9)?,
        status: row.10,
        decided_by: row.11,
        decided_at: row.12.map(to_u64).transpose()?,
        decision_note: row.13,
    })
}

fn credit_from(row: (String, String, String, String, i64, i64, Vec<u8>, Vec<u8>, String, i64)) -> Result<Credit, StoreError> {
    Ok(Credit {
        credit_id: row.0,
        game_id: row.1,
        result_id: row.2,
        player: row.3,
        amount_units: to_u64(row.4)?,
        policy_version: u32::try_from(row.5).map_err(|_| StoreError::NumericRange)?,
        policy_hash: hex::encode(row.6),
        module_hash: hex::encode(row.7),
        state: row.8,
        created_at: to_u64(row.9)?,
    })
}

const CREDIT_COLUMNS: &str = "credit_id, game_id, result_id, player, amount_units, policy_version, policy_hash, module_hash, state, created_at";

fn read_credit(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, String, i64, i64, Vec<u8>, Vec<u8>, String, i64)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?))
}

fn active_policy_in(tx: &Transaction<'_>, game_id: &str, now: u64) -> Result<Option<PolicyRow>, PoolError> {
    // Active from the later of its own effective time and the moment the platform approved it.
    let row = tx
        .query_row(
            &format!(
                "SELECT {POLICY_COLUMNS} FROM game_reward_policies
                 WHERE game_id = ?1 AND status = 'approved' AND effective_from <= ?2 AND decided_at <= ?2
                 ORDER BY version DESC LIMIT 1"
            ),
            params![game_id, to_i64(now)?],
            read_policy,
        )
        .optional()?;
    Ok(row.map(policy_from).transpose()?)
}

fn frozen_in(tx: &Transaction<'_>) -> Result<bool, PoolError> {
    let frozen: i64 = tx.query_row("SELECT frozen FROM pool_state WHERE id = 1", [], |row| row.get(0))?;
    Ok(frozen != 0)
}

fn audit(tx: &Transaction<'_>, game_id: &str, now: u64, record: &AuditRecord<'_>) -> Result<(), PoolError> {
    catalog::write_audit(tx, game_id, now, record).map_err(|error| match error {
        CatalogError::ProofInvalid => PoolError::AlreadyClaimed,
        other => PoolError::Catalog(other),
    })
}

impl GamesStore {
    /// Credit a game's pool with a verified L1 transfer. One transaction hash funds one game once.
    pub fn pool_record_deposit(
        &self,
        game_id: &str,
        tx_hash: &str,
        funder: &str,
        amount: u64,
        now: u64,
        record: &AuditRecord<'_>,
    ) -> Result<PoolView, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_game(&tx, game_id)?;
        let inserted = tx.execute(
            "INSERT INTO game_pool_deposits (tx_hash, game_id, funder, amount_units, claimed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![tx_hash, game_id, funder, to_i64(amount)?, to_i64(now)?],
        );
        match inserted {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::ConstraintViolation => {
                return Err(PoolError::AlreadyClaimed)
            }
            Err(other) => return Err(other.into()),
        }
        tx.execute("INSERT OR IGNORE INTO game_pools (game_id) VALUES (?1)", params![game_id])?;
        tx.execute("UPDATE game_pools SET funded = funded + ?2 WHERE game_id = ?1", params![game_id, to_i64(amount)?])?;
        ledger(&tx, game_id, now, "deposit", amount, tx_hash)?;
        audit(&tx, game_id, now, record)?;
        let view = read_pool(&tx, game_id)?;
        tx.commit()?;
        Ok(view)
    }

    pub fn pool_view(&self, game_id: &str) -> Result<PoolView, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Deferred)?;
        read_pool(&tx, game_id)
    }

    /// Everything the pools hold and have not paid out or returned: what the payout wallet must cover.
    pub fn pool_unspent_total(&self) -> Result<u64, PoolError> {
        let guard = self.lock()?;
        let total: i64 = guard.query_row("SELECT COALESCE(SUM(funded - paid - withdrawn), 0) FROM game_pools", [], |row| row.get(0))?;
        Ok(to_u64(total)?)
    }

    /// What the older Grid and Chess balances owe: credited and not yet paid.
    pub fn legacy_owed_total(&self) -> Result<u64, PoolError> {
        let guard = self.lock()?;
        let total: i64 = guard.query_row("SELECT COALESCE(SUM(credited - paid), 0) FROM balances", [], |row| row.get(0))?;
        Ok(to_u64(total)?)
    }

    pub fn pools_frozen(&self) -> Result<Option<String>, PoolError> {
        let guard = self.lock()?;
        let row: (i64, Option<String>) = guard.query_row("SELECT frozen, reason FROM pool_state WHERE id = 1", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok((row.0 != 0).then(|| row.1.unwrap_or_default()))
    }

    pub fn pools_set_frozen(&self, frozen: bool, reason: Option<&str>) -> Result<(), PoolError> {
        let guard = self.lock()?;
        guard.execute("UPDATE pool_state SET frozen = ?1, reason = ?2 WHERE id = 1", params![i64::from(frozen), reason])?;
        Ok(())
    }

    /// Reserve a reward for one verified result: every check and every write in one transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn pool_reserve(
        &self,
        game_id: &str,
        result_id: &str,
        player: &str,
        credit_id: &str,
        validated: &ValidatedResult,
        now: u64,
    ) -> Result<Credit, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if frozen_in(&tx)? {
            return Err(PoolError::Frozen);
        }
        let (status, scheme, adapter_ref) = require_game(&tx, game_id)?;
        let module_hex = hex::encode(validated.module_hash);
        if status != catalog::STATUS_ACTIVE || scheme != SCHEME_VERIFIED_ADAPTER || adapter_ref.as_deref() != Some(module_hex.as_str()) {
            return Err(PoolError::NotVerified);
        }
        let Some(outcome) = outcome_name(validated.output.outcome) else {
            return Err(PoolError::NotEligible);
        };
        // A result already credited is answered with its credit, and takes nothing a second time.
        let existing = tx
            .query_row(&format!("SELECT {CREDIT_COLUMNS} FROM game_pool_credits WHERE game_id = ?1 AND result_id = ?2"), params![game_id, result_id], read_credit)
            .optional()?;
        if let Some(row) = existing {
            let credit = credit_from(row)?;
            if credit.player != player || credit.module_hash != module_hex {
                return Err(PoolError::Conflict);
            }
            return Ok(credit);
        }
        let policy = active_policy_in(&tx, game_id, now)?.ok_or(PoolError::NoActivePolicy)?;
        if !policy.eligible_outcomes.split(',').any(|name| name == outcome) {
            return Err(PoolError::NotEligible);
        }
        let today = to_i64(day_start(now))?;
        let by_player: i64 = tx.query_row(
            "SELECT COUNT(*) FROM game_pool_credits WHERE game_id = ?1 AND player = ?2 AND created_at >= ?3 AND state != 'released'",
            params![game_id, player, today],
            |row| row.get(0),
        )?;
        let by_game: i64 = tx.query_row(
            "SELECT COUNT(*) FROM game_pool_credits WHERE game_id = ?1 AND created_at >= ?2 AND state != 'released'",
            params![game_id, today],
            |row| row.get(0),
        )?;
        if to_u64(by_player)? >= u64::from(policy.per_player_per_day) || to_u64(by_game)? >= policy.per_game_per_day {
            return Err(PoolError::LimitReached);
        }
        let pool = read_pool(&tx, game_id)?;
        if pool.available < policy.reward_units {
            return Err(PoolError::InsufficientFunds { available: pool.available, needed: policy.reward_units });
        }
        let policy_hash = hex::decode(&policy.policy_hash).map_err(|_| PoolError::State("stored policy hash is malformed".into()))?;
        tx.execute(
            "INSERT INTO game_pool_credits (credit_id, game_id, result_id, player, amount_units, policy_version, policy_hash, module_hash, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'reserved', ?9, ?9)",
            params![credit_id, game_id, result_id, player, to_i64(policy.reward_units)?, i64::from(policy.version), policy_hash, validated.module_hash.as_slice(), to_i64(now)?],
        )?;
        tx.execute("INSERT OR IGNORE INTO game_pools (game_id) VALUES (?1)", params![game_id])?;
        tx.execute("UPDATE game_pools SET reserved = reserved + ?2 WHERE game_id = ?1", params![game_id, to_i64(policy.reward_units)?])?;
        ledger(&tx, game_id, now, "reserve", policy.reward_units, credit_id)?;
        let row = tx.query_row(&format!("SELECT {CREDIT_COLUMNS} FROM game_pool_credits WHERE credit_id = ?1"), params![credit_id], read_credit)?;
        let credit = credit_from(row)?;
        tx.commit()?;
        Ok(credit)
    }

    /// Settle a reserved credit: paid (the transfer was confirmed) or released (it will never be paid).
    /// A credit settles once; settling it again is an error, never a second movement of money.
    pub fn pool_settle(&self, credit_id: &str, paid: bool, now: u64) -> Result<PoolView, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row = tx
            .query_row(&format!("SELECT {CREDIT_COLUMNS} FROM game_pool_credits WHERE credit_id = ?1"), params![credit_id], read_credit)
            .optional()?
            .ok_or_else(|| PoolError::NotFound("no such credit".into()))?;
        let credit = credit_from(row)?;
        if credit.state != "reserved" {
            return Err(PoolError::State(format!("that credit is already {}", credit.state)));
        }
        let (new_state, column, kind) = if paid { ("paid", "paid", "pay") } else { ("released", "", "release") };
        tx.execute("UPDATE game_pool_credits SET state = ?2, updated_at = ?3 WHERE credit_id = ?1", params![credit_id, new_state, to_i64(now)?])?;
        let amount = to_i64(credit.amount_units)?;
        if paid {
            tx.execute(&format!("UPDATE game_pools SET reserved = reserved - ?2, {column} = {column} + ?2 WHERE game_id = ?1"), params![credit.game_id, amount])?;
        } else {
            tx.execute("UPDATE game_pools SET reserved = reserved - ?2 WHERE game_id = ?1", params![credit.game_id, amount])?;
        }
        ledger(&tx, &credit.game_id, now, kind, credit.amount_units, credit_id)?;
        let view = read_pool(&tx, &credit.game_id)?;
        tx.commit()?;
        Ok(view)
    }

    /// Queue the return of unreserved funds to `to`. The amount leaves the pool's accounting now, so it can
    /// neither be rewarded nor withdrawn twice; the transfer itself is the payout worker's job.
    pub fn pool_withdraw(
        &self,
        game_id: &str,
        withdrawal_id: &str,
        to: &str,
        amount: u64,
        now: u64,
        record: &AuditRecord<'_>,
    ) -> Result<PoolView, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if frozen_in(&tx)? {
            return Err(PoolError::Frozen);
        }
        require_game(&tx, game_id)?;
        let pool = read_pool(&tx, game_id)?;
        if amount == 0 || pool.available < amount {
            return Err(PoolError::InsufficientFunds { available: pool.available, needed: amount });
        }
        tx.execute("UPDATE game_pools SET withdrawn = withdrawn + ?2 WHERE game_id = ?1", params![game_id, to_i64(amount)?])?;
        tx.execute(
            "INSERT INTO game_pool_withdrawals (withdrawal_id, game_id, to_address, amount_units, state, created_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5)",
            params![withdrawal_id, game_id, to, to_i64(amount)?, to_i64(now)?],
        )?;
        ledger(&tx, game_id, now, "withdraw", amount, withdrawal_id)?;
        audit(&tx, game_id, now, record)?;
        let view = read_pool(&tx, game_id)?;
        tx.commit()?;
        Ok(view)
    }

    /// Re-derive every pool from the deposits, credits and withdrawals that should explain it.
    pub fn pool_audit(&self) -> Result<Vec<String>, PoolError> {
        let guard = self.lock()?;
        let mut issues = Vec::new();
        let mut statement = guard.prepare("SELECT game_id, funded, reserved, paid, withdrawn FROM game_pools")?;
        let pools = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (game_id, funded, reserved, paid, withdrawn) in pools {
            let sum = |sql: &str| -> Result<i64, rusqlite::Error> { guard.query_row(sql, params![game_id], |row| row.get(0)) };
            let deposits = sum("SELECT COALESCE(SUM(amount_units),0) FROM game_pool_deposits WHERE game_id = ?1")?;
            let reserved_credits = sum("SELECT COALESCE(SUM(amount_units),0) FROM game_pool_credits WHERE game_id = ?1 AND state = 'reserved'")?;
            let paid_credits = sum("SELECT COALESCE(SUM(amount_units),0) FROM game_pool_credits WHERE game_id = ?1 AND state = 'paid'")?;
            let withdrawals = sum("SELECT COALESCE(SUM(amount_units),0) FROM game_pool_withdrawals WHERE game_id = ?1")?;
            for (what, held, derived) in [
                ("funded", funded, deposits),
                ("reserved", reserved, reserved_credits),
                ("paid", paid, paid_credits),
                ("withdrawn", withdrawn, withdrawals),
            ] {
                if held != derived {
                    issues.push(format!("{game_id}: {what} is {held} but its records add up to {derived}"));
                }
            }
        }
        // A credit or deposit for a game with no pool row is a record the books do not carry.
        let orphans: i64 = guard.query_row(
            "SELECT COUNT(*) FROM game_pool_deposits d WHERE NOT EXISTS (SELECT 1 FROM game_pools p WHERE p.game_id = d.game_id)",
            [],
            |row| row.get(0),
        )?;
        if orphans > 0 {
            issues.push(format!("{orphans} deposit(s) belong to a game with no pool"));
        }
        Ok(issues)
    }

    pub fn pool_credit(&self, credit_id: &str) -> Result<Option<Credit>, PoolError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(&format!("SELECT {CREDIT_COLUMNS} FROM game_pool_credits WHERE credit_id = ?1"), params![credit_id], read_credit)
            .optional()?;
        Ok(row.map(credit_from).transpose()?)
    }

    // --- policies ---------------------------------------------------------------------------

    /// Store a proposed policy as the game's next version. Terms are never edited afterwards.
    pub fn policy_propose(
        &self,
        game_id: &str,
        terms: &PolicyTerms,
        proposer: &str,
        now: u64,
        record: &AuditRecord<'_>,
    ) -> Result<PolicyRow, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_game(&tx, game_id)?;
        let last: i64 = tx.query_row("SELECT COALESCE(MAX(version), 0) FROM game_reward_policies WHERE game_id = ?1", params![game_id], |row| row.get(0))?;
        let version = u32::try_from(last + 1).map_err(|_| StoreError::NumericRange)?;
        let hash = policy_hash(game_id, version, terms);
        tx.execute(
            "INSERT INTO game_reward_policies (game_id, version, reward_units, eligible_outcomes, per_player_per_day, per_game_per_day, effective_from, policy_hash, proposed_by, proposed_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'proposed')",
            params![game_id, i64::from(version), to_i64(terms.reward_units)?, terms.eligible_outcomes, i64::from(terms.per_player_per_day), to_i64(terms.per_game_per_day)?, to_i64(terms.effective_from)?, hash.as_slice(), proposer, to_i64(now)?],
        )?;
        audit(&tx, game_id, now, record)?;
        let row = tx.query_row(&format!("SELECT {POLICY_COLUMNS} FROM game_reward_policies WHERE game_id = ?1 AND version = ?2"), params![game_id, i64::from(version)], read_policy)?;
        let policy = policy_from(row)?;
        tx.commit()?;
        Ok(policy)
    }

    pub fn policy(&self, game_id: &str, version: u32) -> Result<Option<PolicyRow>, PoolError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(&format!("SELECT {POLICY_COLUMNS} FROM game_reward_policies WHERE game_id = ?1 AND version = ?2"), params![game_id, i64::from(version)], read_policy)
            .optional()?;
        Ok(row.map(policy_from).transpose()?)
    }

    pub fn policies(&self, game_id: &str) -> Result<Vec<PolicyRow>, PoolError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare(&format!("SELECT {POLICY_COLUMNS} FROM game_reward_policies WHERE game_id = ?1 ORDER BY version"))?;
        let rows = statement.query_map(params![game_id], read_policy)?.collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(|row| policy_from(row).map_err(PoolError::from)).collect()
    }

    pub fn policy_active(&self, game_id: &str, now: u64) -> Result<Option<PolicyRow>, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Deferred)?;
        active_policy_in(&tx, game_id, now)
    }

    /// The platform's decision on a proposal. Approving supersedes the previous approved version and points the
    /// catalog's `reward_policy_ref` at the new hash; rejecting changes nothing else. Only a `proposed` policy can
    /// be decided, and only once.
    pub fn policy_decide(
        &self,
        game_id: &str,
        version: u32,
        approve: bool,
        decided_by: &str,
        note: &str,
        now: u64,
        record: &AuditRecord<'_>,
    ) -> Result<PolicyRow, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let status: Option<String> = tx
            .query_row("SELECT status FROM game_reward_policies WHERE game_id = ?1 AND version = ?2", params![game_id, i64::from(version)], |row| row.get(0))
            .optional()?;
        match status.as_deref() {
            None => return Err(PoolError::NotFound("no such policy version".into())),
            Some("proposed") => {}
            Some(other) => return Err(PoolError::State(format!("that policy is already {other}"))),
        }
        if approve {
            tx.execute("UPDATE game_reward_policies SET status = 'superseded' WHERE game_id = ?1 AND status = 'approved'", params![game_id])?;
        }
        tx.execute(
            "UPDATE game_reward_policies SET status = ?3, decided_by = ?4, decided_at = ?5, decision_note = ?6 WHERE game_id = ?1 AND version = ?2",
            params![game_id, i64::from(version), if approve { "approved" } else { "rejected" }, decided_by, to_i64(now)?, note],
        )?;
        let row = tx.query_row(&format!("SELECT {POLICY_COLUMNS} FROM game_reward_policies WHERE game_id = ?1 AND version = ?2"), params![game_id, i64::from(version)], read_policy)?;
        let policy = policy_from(row)?;
        if approve {
            tx.execute("UPDATE platform_games SET reward_policy_ref = ?2, updated_at = ?3 WHERE game_id = ?1", params![game_id, policy.policy_hash, to_i64(now)?])?;
        }
        audit(&tx, game_id, now, record)?;
        tx.commit()?;
        Ok(policy)
    }

    /// The platform names the one adapter whose verdicts may earn rewards for a game. Until then the game is
    /// `record_only`. This is an operator action, never an owner's: an approved adapter is what lets results be paid.
    pub fn platform_approve_adapter(
        &self,
        game_id: &str,
        module_hash: &[u8; 32],
        adapter_version: u32,
        now: u64,
        record: &AuditRecord<'_>,
    ) -> Result<CatalogGame, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_game(&tx, game_id)?;
        tx.execute(
            "UPDATE platform_games SET validation_scheme = ?2, adapter_ref = ?3, adapter_version = ?4, updated_at = ?5 WHERE game_id = ?1",
            params![game_id, SCHEME_VERIFIED_ADAPTER, hex::encode(module_hash), i64::from(adapter_version), to_i64(now)?],
        )?;
        audit(&tx, game_id, now, record)?;
        tx.commit()?;
        drop(guard);
        Ok(self.catalog_game(game_id)?.ok_or_else(|| PoolError::NotFound("game vanished".into()))?)
    }
}

// --- the service ---------------------------------------------------------------------------------

impl From<PoolError> for GamesError {
    fn from(error: PoolError) -> Self {
        match error {
            PoolError::Store(error) => error.into(),
            PoolError::Catalog(error) => error.into(),
            PoolError::NotFound(message) => GamesError::NotFound(message),
            PoolError::Invalid(message) => GamesError::Invalid(message),
            PoolError::NotVerified => GamesError::Forbidden("only a result verified by the game's approved adapter can earn a reward".into()),
            PoolError::NoActivePolicy => GamesError::Forbidden("the game has no approved, active reward policy".into()),
            PoolError::NotEligible => GamesError::Forbidden("the reward policy does not pay for this outcome".into()),
            PoolError::LimitReached => GamesError::GameLimit,
            PoolError::InsufficientFunds { .. } => GamesError::Forbidden("the game's reward pool cannot cover this reward".into()),
            PoolError::AlreadyClaimed => GamesError::Conflict,
            PoolError::Frozen => GamesError::PayoutsPaused,
            PoolError::Conflict => GamesError::ResultConflict,
            PoolError::State(message) => GamesError::Invalid(message),
        }
    }
}

fn operator_actor(label: &str) -> String {
    hex::encode(Sha3_256::digest(format!("UltraNet/platform-operator/{label}").as_bytes()))
}

fn operator_digest(action: &str, game_id: &str, detail: &str, now: u64) -> [u8; 32] {
    let mut nonce = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraNet/platform-decision/v1");
    for part in [action, game_id, detail] {
        hasher.update((part.len() as u32).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(now.to_le_bytes());
    hasher.update(nonce);
    hasher.finalize().into()
}

impl GamesService {
    fn known(&self, game_id: &str) -> Result<CatalogGame, GamesError> {
        self.store().catalog_game(game_id).map_err(GamesError::from)?.ok_or_else(|| GamesError::NotFound("no such game".into()))
    }

    pub fn pool(&self, game_id: &str) -> Result<PoolView, GamesError> {
        self.known(game_id)?;
        Ok(self.store().pool_view(game_id)?)
    }

    /// Claim an L1 transfer as a deposit into a game's pool. Owner-signed; the ledger must show a confirmed
    /// transfer from that owner to the payout address, and it counts once.
    pub async fn pool_fund(&self, game_id: &str, tx_hash: &str, proof: AdminProof) -> Result<PoolView, GamesError> {
        if tx_hash.len() != 64 || !tx_hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(GamesError::Invalid("tx_hash must be 64 lowercase hexadecimal characters".into()));
        }
        let game = self.known(game_id)?;
        let (actor, owner, digest) = self.authorize_admin(ACTION_FUND, game.chain_id, game_id, tx_hash, "", "", &proof).await?;
        let transfer = self
            .chain()
            .transfer(tx_hash)
            .await
            .map_err(|_| GamesError::ChainUnavailable)?
            .ok_or_else(|| GamesError::NotFound("the ledger has no such transaction".into()))?;
        if transfer.pending {
            return Err(GamesError::Invalid("that transaction is not confirmed yet".into()));
        }
        if transfer.sender != actor || transfer.recipient != self.payout_address() || transfer.amount == 0 {
            return Err(GamesError::Forbidden("that transaction is not a transfer from you to the payout address".into()));
        }
        let detail = serde_json::json!({ "tx_hash": tx_hash, "amount_units": transfer.amount }).to_string();
        Ok(self.store().pool_record_deposit(
            game_id,
            tx_hash,
            &actor,
            transfer.amount,
            self.now(),
            &AuditRecord { actor: &actor, action: ACTION_FUND, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    /// Propose a reward policy for a game. Owner-signed over the exact terms; checked against the platform
    /// ceilings; stored, and inert until the platform approves it.
    pub async fn policy_propose(&self, game_id: &str, terms: PolicyTerms, proof: AdminProof) -> Result<PolicyRow, GamesError> {
        check_platform_limits(&terms)?;
        let game = self.known(game_id)?;
        let summary = format!("{}|{}|{}|{}|{}", terms.reward_units, terms.eligible_outcomes, terms.per_player_per_day, terms.per_game_per_day, terms.effective_from);
        let (actor, owner, digest) = self.authorize_admin(ACTION_POLICY_PROPOSE, game.chain_id, game_id, &summary, "", "", &proof).await?;
        let detail = serde_json::json!({ "terms": summary }).to_string();
        Ok(self.store().policy_propose(
            game_id,
            &terms,
            &actor,
            self.now(),
            &AuditRecord { actor: &actor, action: ACTION_POLICY_PROPOSE, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    /// The platform's decision (an operator action, not exposed over HTTP). Approval re-checks the ceilings and
    /// requires that the pool can pay at least one reward, so an approved policy never promises what cannot be paid.
    pub fn platform_decide_policy(&self, game_id: &str, version: u32, approve: bool, operator: &str, note: &str) -> Result<PolicyRow, GamesError> {
        self.known(game_id)?;
        let row = self.store().policy(game_id, version)?.ok_or_else(|| GamesError::NotFound("no such policy version".into()))?;
        if approve {
            check_platform_limits(&PolicyTerms {
                reward_units: row.reward_units,
                eligible_outcomes: row.eligible_outcomes.clone(),
                per_player_per_day: row.per_player_per_day,
                per_game_per_day: row.per_game_per_day,
                effective_from: row.effective_from,
            })?;
            let pool = self.store().pool_view(game_id)?;
            if pool.available < row.reward_units {
                return Err(GamesError::Forbidden("the game's pool cannot pay even one reward at this level".into()));
            }
        }
        let action = if approve { ACTION_POLICY_APPROVE } else { ACTION_POLICY_REJECT };
        let now = self.now();
        let actor = operator_actor(operator);
        let zeros = "0".repeat(64);
        let detail = serde_json::json!({ "operator": operator, "version": version, "note": note, "policy_hash": row.policy_hash }).to_string();
        Ok(self.store().policy_decide(
            game_id,
            version,
            approve,
            &actor,
            note,
            now,
            &AuditRecord { actor: &actor, action, chain_owner: &zeros, detail: &detail, digest: operator_digest(action, game_id, &detail, now) },
        )?)
    }

    /// The platform names the adapter whose verdicts may earn rewards for a game (an operator action).
    pub fn platform_approve_adapter(&self, game_id: &str, module_hash: [u8; 32], adapter_version: u32, operator: &str) -> Result<CatalogGame, GamesError> {
        self.known(game_id)?;
        let now = self.now();
        let actor = operator_actor(operator);
        let zeros = "0".repeat(64);
        let detail = serde_json::json!({ "operator": operator, "module_hash": hex::encode(module_hash), "adapter_version": adapter_version }).to_string();
        Ok(self.store().platform_approve_adapter(
            game_id,
            &module_hash,
            adapter_version,
            now,
            &AuditRecord { actor: &actor, action: ACTION_ADAPTER_APPROVE, chain_owner: &zeros, detail: &detail, digest: operator_digest(ACTION_ADAPTER_APPROVE, game_id, &detail, now) },
        )?)
    }

    /// Reserve the reward for one verified result, or refuse. Nothing is promised unless the whole check passes.
    pub fn reserve_reward(&self, game_id: &str, result_id: &str, player: &str, validated: &ValidatedResult) -> Result<Credit, GamesError> {
        if result_id.is_empty() || result_id.len() > 128 {
            return Err(GamesError::Invalid("result_id must be 1 to 128 characters".into()));
        }
        let player = self.normalize_address(player)?;
        let mut id = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);
        Ok(self.store().pool_reserve(game_id, result_id, &player, &hex::encode(id), validated, self.now())?)
    }

    /// Settle a credit once its payout is confirmed (`paid`) or will never happen (`released`).
    pub fn settle_reward(&self, credit_id: &str, paid: bool) -> Result<PoolView, GamesError> {
        Ok(self.store().pool_settle(credit_id, paid, self.now())?)
    }

    /// What a player may be told a win is worth right now: only while a policy is active and the pool covers it.
    pub fn promised_reward(&self, game_id: &str) -> Result<Option<u64>, GamesError> {
        let game = self.known(game_id)?;
        if game.status != catalog::STATUS_ACTIVE || game.validation_scheme == SCHEME_RECORD_ONLY {
            return Ok(None);
        }
        let Some(policy) = self.store().policy_active(game_id, self.now())? else {
            return Ok(None);
        };
        let pool = self.store().pool_view(game_id)?;
        Ok((pool.available >= policy.reward_units).then_some(policy.reward_units))
    }

    /// Owner-signed return of unreserved funds, queued for the payout worker.
    pub async fn pool_withdraw(&self, game_id: &str, amount: u64, proof: AdminProof) -> Result<PoolView, GamesError> {
        let game = self.known(game_id)?;
        let summary = amount.to_string();
        let (actor, owner, digest) = self.authorize_admin(ACTION_WITHDRAW, game.chain_id, game_id, &summary, "", "", &proof).await?;
        let mut id = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);
        let detail = serde_json::json!({ "amount_units": amount, "to": owner }).to_string();
        Ok(self.store().pool_withdraw(
            game_id,
            &hex::encode(id),
            &owner,
            amount,
            self.now(),
            &AuditRecord { actor: &actor, action: ACTION_WITHDRAW, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    /// Compare what the service owes (every pool's unspent balance, plus the older Grid and Chess balances)
    /// with what the payout wallet actually holds. A shortfall freezes rewards for everyone until cleared.
    pub async fn pool_check_solvency(&self) -> Result<(), GamesError> {
        let balance = self.payout_balance().await?;
        let owed_legacy = self.store().legacy_owed_total()?;
        let owed_pools = self.store().pool_unspent_total()?;
        let owed = owed_legacy.saturating_add(owed_pools);
        if balance < owed {
            let reason = format!("the payout wallet holds {balance} but {owed} is owed across the pools and the older balances");
            self.store().pools_set_frozen(true, Some(&reason))?;
            return Err(GamesError::PayoutsPaused);
        }
        Ok(())
    }
}
