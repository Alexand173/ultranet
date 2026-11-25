//! Paying rewards and executing owner withdrawals from a game's own pool.
//!
//! This is the second payout path. The older one, for Grid and Chess, is untouched and keeps its own tables and
//! its own accounting; the two share only the payout wallet's key, and they take turns under one lock so they never
//! sign at the same time.
//!
//! ```text
//! reserve ──► credit (reserved) + payout (queued) + network fee held     one transaction
//!                    │
//!                    ▼
//!  queued ──► signed ──► submitted ──► confirmed
//!    │          the exact signed bytes are stored BEFORE they are sent
//!    └─► failed (rewards only, never signed: the reservation is given back)
//! ```
//!
//! **Money is reserved before anything is sent.** Creating a credit and queueing its payout is one database
//! transaction: the reward is reserved from the game's pool and the network fee is held beside it, both from that
//! pool and no other. A payout that cannot be funded is never created, so no promise outlives the money.
//!
//! **The network fee is the game's, not the platform's.** The wallet signs a transfer that pays the winner the
//! full reward and the node its fee; the fee is charged to the game's pool. Owner withdrawals pay their own fee out
//! of the amount returned.
//!
//! **A signed payout is sent only as the same bytes.** `mark_signed` persists the envelope and its hash first;
//! every retry, after a lost reply or a restart, resends those exact bytes. The node treats an identical
//! transaction as the same one, so a response that never arrived cannot become a second payment. A payout is
//! never signed twice: its state only moves forward.
//!
//! **Settlement waits for the ledger.** A payout is `confirmed`, and the pool's `reserved` becomes `paid`, only
//! when the node reports the transaction confirmed. Pending, not found and unavailable are all "not yet" and
//! change nothing in the books.
//!
//! **When something is wrong the pools freeze, and nothing else does.** A node that answers about a different
//! transaction, rejects a signed transfer, or a wallet that holds less than a payout needs freezes the *pool*
//! payouts (and new reservations) until an operator clears it; the older Grid and Chess path is unaffected.
//!
//! **Withdrawals go to the chain's current owner.** The destination recorded when the owner asked is replaced, at
//! signing time, by whoever the ledger names as the chain's owner at that moment. Once the transfer is signed the
//! destination is fixed in the stored bytes.

use super::adapter::ValidatedResult;
use super::pools::{self, Credit, PoolError};
use super::rules::payout_fee;
use super::service::{GamesError, GamesService};
use super::store::{to_i64, to_u64, GamesStore, StoreError};
use crate::faucet::models::SignedTransferRequest;
use crate::faucet::node_client::NodeClientError;
use crate::UltraBlockchain;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;

pub const KIND_REWARD: &str = "reward";
pub const KIND_WITHDRAWAL: &str = "withdrawal";

pub(super) const POOL_PAYOUTS_SCHEMA: &str = "
CREATE TABLE game_pool_payouts (
    payout_id TEXT PRIMARY KEY CHECK (length(payout_id) = 32),
    kind TEXT NOT NULL CHECK (kind IN ('reward','withdrawal')),
    game_id TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    to_address TEXT NOT NULL CHECK (length(to_address) = 64),
    amount_units INTEGER NOT NULL CHECK (amount_units > 0),
    fee_reserved INTEGER NOT NULL DEFAULT 0 CHECK (fee_reserved >= 0),
    fee_paid INTEGER CHECK (fee_paid IS NULL OR fee_paid >= 0),
    state TEXT NOT NULL CHECK (state IN ('queued','signed','submitted','confirmed','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    envelope TEXT,
    tx_hash TEXT,
    nonce INTEGER,
    fee_units INTEGER,
    sent_units INTEGER,
    signed_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (kind, ref_id)
);
CREATE INDEX game_pool_payouts_open ON game_pool_payouts (state, created_at);
CREATE INDEX game_pool_payouts_by_game ON game_pool_payouts (game_id);
";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolPayout {
    pub payout_id: String,
    pub kind: String,
    pub game_id: String,
    pub ref_id: String,
    pub to_address: String,
    pub amount_units: u64,
    pub fee_reserved: u64,
    pub fee_paid: Option<u64>,
    pub state: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub envelope: Option<String>,
    pub tx_hash: Option<String>,
    pub nonce: Option<u64>,
    pub fee_units: Option<u64>,
    pub sent_units: Option<u64>,
    pub signed_at: Option<u64>,
    pub created_at: u64,
}

const PAYOUT_COLUMNS: &str = "payout_id, kind, game_id, ref_id, to_address, amount_units, fee_reserved, fee_paid, state, attempts, last_error, envelope, tx_hash, nonce, fee_units, sent_units, signed_at, created_at";

type PayoutSql = (String, String, String, String, String, i64, i64, Option<i64>, String, i64, Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, i64);

fn read_payout(row: &rusqlite::Row<'_>) -> rusqlite::Result<PayoutSql> {
    Ok((
        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?,
        row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?, row.get(16)?, row.get(17)?,
    ))
}

fn payout_from(row: PayoutSql) -> Result<PoolPayout, StoreError> {
    let opt = |v: Option<i64>| v.map(to_u64).transpose();
    Ok(PoolPayout {
        payout_id: row.0,
        kind: row.1,
        game_id: row.2,
        ref_id: row.3,
        to_address: row.4,
        amount_units: to_u64(row.5)?,
        fee_reserved: to_u64(row.6)?,
        fee_paid: opt(row.7)?,
        state: row.8,
        attempts: u32::try_from(row.9).map_err(|_| StoreError::NumericRange)?,
        last_error: row.10,
        envelope: row.11,
        tx_hash: row.12,
        nonce: opt(row.13)?,
        fee_units: opt(row.14)?,
        sent_units: opt(row.15)?,
        signed_at: opt(row.16)?,
        created_at: to_u64(row.17)?,
    })
}

fn random_id() -> String {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    hex::encode(bytes)
}

fn payout_in(tx: &Transaction<'_>, id: &str) -> Result<PoolPayout, PoolError> {
    let row = tx
        .query_row(&format!("SELECT {PAYOUT_COLUMNS} FROM game_pool_payouts WHERE payout_id = ?1"), params![id], read_payout)
        .optional()?
        .ok_or_else(|| PoolError::NotFound("no such payout".into()))?;
    Ok(payout_from(row)?)
}

/// Queue the payout for a reserved credit, holding its network fee from the same pool. Idempotent: a credit has
/// one payout, and asking again returns it.
fn enqueue_reward_in(tx: &Transaction<'_>, credit_id: &str, now: u64) -> Result<PoolPayout, PoolError> {
    if let Some(existing) = tx
        .query_row(&format!("SELECT {PAYOUT_COLUMNS} FROM game_pool_payouts WHERE kind = 'reward' AND ref_id = ?1"), params![credit_id], read_payout)
        .optional()?
    {
        return Ok(payout_from(existing)?);
    }
    let credit = tx
        .query_row(&format!("SELECT {} FROM game_pool_credits WHERE credit_id = ?1", pools::CREDIT_COLUMNS), params![credit_id], pools::read_credit)
        .optional()?
        .ok_or_else(|| PoolError::NotFound("no such credit".into()))?;
    let credit = pools::credit_from(credit)?;
    if credit.state != "reserved" {
        return Err(PoolError::State(format!("that credit is already {}", credit.state)));
    }
    let fee = payout_fee(credit.amount_units);
    let pool = pools::read_pool(tx, &credit.game_id)?;
    if pool.available < fee {
        return Err(PoolError::InsufficientFunds { available: pool.available, needed: fee });
    }
    let id = random_id();
    tx.execute(
        "INSERT INTO game_pool_payouts (payout_id, kind, game_id, ref_id, to_address, amount_units, fee_reserved, state, created_at, updated_at)
         VALUES (?1, 'reward', ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?7)",
        params![id, credit.game_id, credit_id, credit.player, to_i64(credit.amount_units)?, to_i64(fee)?, to_i64(now)?],
    )?;
    payout_in(tx, &id)
}

impl GamesStore {
    /// Reserve a reward and queue its payout, with the network fee held, in one transaction: either the whole
    /// commitment exists or none of it does.
    pub fn pool_reserve_queued(
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
        let (credit, _) = pools::reserve_in(&tx, game_id, result_id, player, credit_id, validated, now)?;
        if credit.state == "reserved" {
            // For a replay this returns the payout that already exists; for a new credit it creates it, and if
            // the fee cannot be held the whole reservation rolls back.
            enqueue_reward_in(&tx, &credit.credit_id, now)?;
        }
        tx.commit()?;
        Ok(credit)
    }

    /// Queue what is missing after a crash or a gap: rewards reserved without a payout, and owner withdrawals that
    /// have no transfer yet. Returns (queued, blocked): blocked ones are waiting for the pool to cover their fee.
    pub fn pool_enqueue_pending(&self, now: u64) -> Result<(usize, usize), PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let credit_ids: Vec<String> = {
            let mut statement = tx.prepare(
                "SELECT credit_id FROM game_pool_credits c WHERE state = 'reserved'
                 AND NOT EXISTS (SELECT 1 FROM game_pool_payouts p WHERE p.kind = 'reward' AND p.ref_id = c.credit_id)
                 ORDER BY created_at",
            )?;
            let ids = statement.query_map([], |row| row.get(0))?.collect::<Result<Vec<String>, _>>()?;
            ids
        };
        let (mut queued, mut blocked) = (0, 0);
        for credit_id in credit_ids {
            tx.execute_batch("SAVEPOINT enqueue")?;
            match enqueue_reward_in(&tx, &credit_id, now) {
                Ok(_) => {
                    tx.execute_batch("RELEASE enqueue")?;
                    queued += 1;
                }
                Err(PoolError::InsufficientFunds { .. }) => {
                    tx.execute_batch("ROLLBACK TO enqueue; RELEASE enqueue")?;
                    blocked += 1;
                }
                Err(other) => return Err(other),
            }
        }
        let withdrawals: Vec<(String, String, String, i64)> = {
            let mut statement = tx.prepare(
                "SELECT withdrawal_id, game_id, to_address, amount_units FROM game_pool_withdrawals w WHERE state = 'queued'
                 AND NOT EXISTS (SELECT 1 FROM game_pool_payouts p WHERE p.kind = 'withdrawal' AND p.ref_id = w.withdrawal_id)
                 ORDER BY created_at",
            )?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?.collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (withdrawal_id, game_id, to, amount) in withdrawals {
            tx.execute(
                "INSERT INTO game_pool_payouts (payout_id, kind, game_id, ref_id, to_address, amount_units, fee_reserved, state, created_at, updated_at)
                 VALUES (?1, 'withdrawal', ?2, ?3, ?4, ?5, 0, 'queued', ?6, ?6)",
                params![random_id(), game_id, withdrawal_id, to, amount, to_i64(now)?],
            )?;
            queued += 1;
        }
        tx.commit()?;
        Ok((queued, blocked))
    }

    /// The oldest payout that is not finished.
    pub fn pool_next_open_payout(&self) -> Result<Option<PoolPayout>, PoolError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(
                &format!("SELECT {PAYOUT_COLUMNS} FROM game_pool_payouts WHERE state IN ('queued','signed','submitted') ORDER BY created_at, rowid LIMIT 1"),
                [],
                read_payout,
            )
            .optional()?;
        Ok(row.map(payout_from).transpose()?)
    }

    pub fn pool_payout(&self, id: &str) -> Result<Option<PoolPayout>, PoolError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(&format!("SELECT {PAYOUT_COLUMNS} FROM game_pool_payouts WHERE payout_id = ?1"), params![id], read_payout)
            .optional()?;
        Ok(row.map(payout_from).transpose()?)
    }

    pub fn pool_payouts_for(&self, game_id: &str) -> Result<Vec<PoolPayout>, PoolError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare(&format!("SELECT {PAYOUT_COLUMNS} FROM game_pool_payouts WHERE game_id = ?1 ORDER BY created_at, rowid"))?;
        let rows = statement.query_map(params![game_id], read_payout)?.collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(|row| payout_from(row).map_err(PoolError::from)).collect()
    }

    pub fn pool_payout_record_attempt(&self, id: &str, error: Option<&str>, now: u64) -> Result<(), PoolError> {
        let guard = self.lock()?;
        guard.execute(
            "UPDATE game_pool_payouts SET attempts = attempts + 1, last_error = ?2, updated_at = ?3 WHERE payout_id = ?1",
            params![id, error, to_i64(now)?],
        )?;
        Ok(())
    }

    /// Withdrawals follow the chain's current owner: the stored destination is replaced before signing.
    pub fn pool_payout_set_destination(&self, id: &str, to: &str, now: u64) -> Result<(), PoolError> {
        let guard = self.lock()?;
        guard.execute(
            "UPDATE game_pool_payouts SET to_address = ?2, updated_at = ?3 WHERE payout_id = ?1 AND state = 'queued'",
            params![id, to, to_i64(now)?],
        )?;
        Ok(())
    }

    /// Hold more of the pool for a payout whose real fee turned out higher than the estimate it was queued with.
    pub fn pool_payout_raise_fee(&self, id: &str, fee: u64, now: u64) -> Result<(), PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payout = payout_in(&tx, id)?;
        if payout.state != "queued" {
            return Err(PoolError::State("the fee can only be raised before signing".into()));
        }
        if fee > payout.fee_reserved {
            let pool = pools::read_pool(&tx, &payout.game_id)?;
            let extra = fee - payout.fee_reserved;
            if pool.available < extra {
                return Err(PoolError::InsufficientFunds { available: pool.available, needed: extra });
            }
            tx.execute("UPDATE game_pool_payouts SET fee_reserved = ?2, updated_at = ?3 WHERE payout_id = ?1", params![id, to_i64(fee)?, to_i64(now)?])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Persist the exact signed transfer before it is sent. From here on a retry is these bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn pool_payout_mark_signed(&self, id: &str, envelope: &str, hash: &str, nonce: u64, fee: u64, sent: u64, now: u64) -> Result<(), PoolError> {
        let guard = self.lock()?;
        let changed = guard.execute(
            "UPDATE game_pool_payouts SET state = 'signed', envelope = ?2, tx_hash = ?3, nonce = ?4, fee_units = ?5, sent_units = ?6, signed_at = ?7, updated_at = ?7
             WHERE payout_id = ?1 AND state = 'queued'",
            params![id, envelope, hash, to_i64(nonce)?, to_i64(fee)?, to_i64(sent)?, to_i64(now)?],
        )?;
        if changed != 1 {
            return Err(PoolError::State("a payout is signed once, from the queue".into()));
        }
        Ok(())
    }

    pub fn pool_payout_mark_submitted(&self, id: &str, now: u64) -> Result<(), PoolError> {
        let guard = self.lock()?;
        let changed = guard.execute(
            "UPDATE game_pool_payouts SET state = 'submitted', updated_at = ?2 WHERE payout_id = ?1 AND state IN ('signed','submitted')",
            params![id, to_i64(now)?],
        )?;
        if changed != 1 {
            return Err(PoolError::State("only a signed payout can be marked submitted".into()));
        }
        Ok(())
    }

    /// The ledger confirmed the transfer: settle the pool. The only place `reserved` becomes `paid`.
    pub fn pool_payout_confirm(&self, id: &str, now: u64) -> Result<PoolPayoutSettled, PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payout = payout_in(&tx, id)?;
        if payout.state != "signed" && payout.state != "submitted" {
            return Err(PoolError::State(format!("that payout is already {}", payout.state)));
        }
        let fee = payout.fee_units.unwrap_or(0);
        if payout.kind == KIND_REWARD {
            // reserved -> paid, once: the credit moves out of 'reserved' in the same statement sequence.
            let moved = tx.execute("UPDATE game_pool_credits SET state = 'paid', updated_at = ?2 WHERE credit_id = ?1 AND state = 'reserved'", params![payout.ref_id, to_i64(now)?])?;
            if moved != 1 {
                return Err(PoolError::State("the credit behind this payout is not reserved".into()));
            }
            tx.execute(
                "UPDATE game_pools SET reserved = reserved - ?2, paid = paid + ?2 WHERE game_id = ?1",
                params![payout.game_id, to_i64(payout.amount_units)?],
            )?;
            pools::ledger(&tx, &payout.game_id, now, "pay", payout.amount_units, &payout.ref_id)?;
            tx.execute("UPDATE game_pool_payouts SET state = 'confirmed', fee_paid = ?2, updated_at = ?3 WHERE payout_id = ?1", params![id, to_i64(fee)?, to_i64(now)?])?;
        } else {
            let moved = tx.execute("UPDATE game_pool_withdrawals SET state = 'paid' WHERE withdrawal_id = ?1 AND state = 'queued'", params![payout.ref_id])?;
            if moved != 1 {
                return Err(PoolError::State("the withdrawal behind this payout is not queued".into()));
            }
            // The amount already left the pool's books when it was requested; the fee came out of it.
            tx.execute("UPDATE game_pool_payouts SET state = 'confirmed', fee_paid = 0, updated_at = ?2 WHERE payout_id = ?1", params![id, to_i64(now)?])?;
        }
        let view = pools::read_pool(&tx, &payout.game_id)?;
        tx.commit()?;
        Ok(PoolPayoutSettled { payout_id: id.to_string(), pool: view })
    }

    /// A reward payout that was never signed and never will be: give the reservation and the held fee back.
    pub fn pool_payout_fail_unsigned_reward(&self, id: &str, reason: &str, now: u64) -> Result<(), PoolError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payout = payout_in(&tx, id)?;
        if payout.kind != KIND_REWARD || payout.state != "queued" || payout.envelope.is_some() {
            return Err(PoolError::State("only a reward that was never signed can be given up".into()));
        }
        tx.execute("UPDATE game_pool_credits SET state = 'released', updated_at = ?2 WHERE credit_id = ?1 AND state = 'reserved'", params![payout.ref_id, to_i64(now)?])?;
        tx.execute("UPDATE game_pools SET reserved = reserved - ?2 WHERE game_id = ?1", params![payout.game_id, to_i64(payout.amount_units)?])?;
        pools::ledger(&tx, &payout.game_id, now, "release", payout.amount_units, &payout.ref_id)?;
        tx.execute("UPDATE game_pool_payouts SET state = 'failed', last_error = ?2, updated_at = ?3 WHERE payout_id = ?1", params![id, reason, to_i64(now)?])?;
        tx.commit()?;
        Ok(())
    }
}

/// What confirming a payout left behind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolPayoutSettled {
    pub payout_id: String,
    pub pool: pools::PoolView,
}

// --- the worker ------------------------------------------------------------------------------------

impl GamesService {
    /// Reserve a reward for a verified result and queue its payout, atomically. Nothing is sent here; the worker
    /// does that, and only for payouts that exist.
    pub fn reserve_reward_for_payout(&self, game_id: &str, result_id: &str, player: &str, validated: &ValidatedResult) -> Result<Credit, GamesError> {
        if result_id.is_empty() || result_id.len() > 128 {
            return Err(GamesError::Invalid("result_id must be 1 to 128 characters".into()));
        }
        let player = self.normalize_address(player)?;
        Ok(self.store().pool_reserve_queued(game_id, result_id, &player, &random_id(), validated, self.now())?)
    }

    fn freeze_pools(&self, reason: &str) {
        let _ = self.store().pools_set_frozen(true, Some(reason));
        tracing::error!(reason, "reward pools frozen; operator review required");
    }

    /// One step of the pool payout worker: finish what is in flight, or start the oldest queued payout. Returns
    /// whether there was anything to do. The older Grid and Chess path and this one take turns under one lock.
    pub async fn process_pool_payout_once(&self) -> Result<bool, GamesError> {
        let _guard = self.payout_lock().lock().await;
        if self.store().pools_frozen()?.is_some() {
            return Ok(false);
        }
        // Before anything is queued or signed: does the wallet still hold what the pools and the older balances are
        // owed? If not the pools freeze here, for every game, until an operator clears it. An unreadable node means
        // nothing can be checked, so nothing is signed this round.
        match self.pool_check_solvency().await {
            Ok(()) => {}
            Err(GamesError::PayoutsPaused) | Err(GamesError::ChainUnavailable) => return Ok(false),
            Err(other) => return Err(other),
        }
        let now = self.now();
        self.store().pool_enqueue_pending(now)?;
        let Some(payout) = self.store().pool_next_open_payout()? else {
            return Ok(false);
        };
        match payout.state.as_str() {
            "queued" => self.pool_sign_and_submit(&payout, now).await?,
            "signed" | "submitted" => self.pool_poll_or_resubmit(&payout, now).await?,
            _ => {}
        }
        Ok(true)
    }

    async fn pool_sign_and_submit(&self, payout: &PoolPayout, now: u64) -> Result<(), GamesError> {
        let config = self.service_config();
        let Some(signer) = self.signer() else {
            self.freeze_pools("the payout signer is not loaded");
            return Ok(());
        };
        let Some(game) = self.store().catalog_game(&payout.game_id).map_err(GamesError::from)? else {
            self.freeze_pools("a payout names a game that is not in the catalog");
            return Ok(());
        };
        let reward = payout.kind == KIND_REWARD;
        // Where it goes. A reward goes to its winner; a withdrawal goes to whoever owns the chain now.
        let destination = if reward {
            payout.to_address.clone()
        } else {
            match self.chain().chain_owner(game.chain_id).await {
                Ok(Some(owner)) => owner,
                Ok(None) => {
                    self.freeze_pools("a withdrawal names a chain the ledger does not list");
                    return Ok(());
                }
                Err(_) => {
                    self.store().pool_payout_record_attempt(&payout.payout_id, Some("the ledger could not be read for the chain owner"), now)?;
                    return Ok(());
                }
            }
        };
        if destination != payout.to_address {
            self.store().pool_payout_set_destination(&payout.payout_id, &destination, now)?;
        }
        if destination == config.payout_address {
            if reward {
                self.store().pool_payout_fail_unsigned_reward(&payout.payout_id, "the recipient is the payout wallet itself", now)?;
            } else {
                self.freeze_pools("a withdrawal would pay the payout wallet itself");
            }
            return Ok(());
        }
        if reward {
            // The reward must still be backed by the policy it was priced under, byte for byte.
            let credit = self.store().pool_credit(&payout.ref_id)?;
            let backed = match &credit {
                Some(credit) if credit.state == "reserved" && credit.amount_units == payout.amount_units => {
                    match self.store().policy(&payout.game_id, credit.policy_version)? {
                        Some(policy) => policy.policy_hash == credit.policy_hash && (policy.status == "approved" || policy.status == "superseded"),
                        None => false,
                    }
                }
                _ => false,
            };
            if !backed {
                self.freeze_pools("a reward is not backed by an approved policy and a reserved credit");
                return Ok(());
            }
            if payout.attempts >= config.max_submission_attempts {
                self.store().pool_payout_fail_unsigned_reward(&payout.payout_id, "gave up before signing", now)?;
                return Ok(());
            }
        }
        let account = match self.node().account(&config.payout_address).await {
            Ok(account) => account,
            Err(_) => {
                self.store().pool_payout_record_attempt(&payout.payout_id, Some("the node could not describe the payout account"), now)?;
                return Ok(());
            }
        };
        if account.address != config.payout_address || account.decimals != UltraBlockchain::ULTRA_DECIMALS {
            self.freeze_pools("the node described a different payout account than configured");
            return Ok(());
        }
        let Some(balance) = account.balance_base_units() else {
            self.store().pool_payout_record_attempt(&payout.payout_id, Some("the node did not report a balance"), now)?;
            return Ok(());
        };
        let estimate = match self.node().estimate(&destination, payout.amount_units).await {
            Ok(estimate) => estimate,
            Err(_) => {
                self.store().pool_payout_record_attempt(&payout.payout_id, Some("the node would not estimate the fee"), now)?;
                return Ok(());
            }
        };
        let estimate_ok = estimate.recipient == destination
            && estimate.amount == payout.amount_units
            && estimate.gas_limit == 500_000
            && estimate.gas_price == 1
            && estimate.amount.checked_add(estimate.fee) == Some(estimate.total);
        if !estimate_ok {
            self.freeze_pools("the node's fee estimate does not match the payout it was asked about");
            return Ok(());
        }
        let fee = estimate.fee.max(payout_fee(payout.amount_units));
        // What the winner is sent, and what the node is paid. A reward pays the full amount and the pool bears the
        // fee; a withdrawal pays its own fee out of the amount returned.
        let sent = if reward {
            payout.amount_units
        } else {
            match payout.amount_units.checked_sub(fee).filter(|sent| *sent > 0) {
                Some(sent) => sent,
                None => {
                    self.freeze_pools("a withdrawal is too small to cover its own network fee");
                    return Ok(());
                }
            }
        };
        if reward && fee > payout.fee_reserved {
            if let Err(error) = self.store().pool_payout_raise_fee(&payout.payout_id, fee, now) {
                self.store().pool_payout_record_attempt(&payout.payout_id, Some(&format!("the pool cannot hold the network fee: {error}")), now)?;
                return Ok(());
            }
        }
        let required = sent
            .checked_add(fee)
            .and_then(|sum| sum.checked_add(self.policy().payout_reserve_units))
            .ok_or_else(|| GamesError::Internal("payout arithmetic overflowed".into()))?;
        if balance < required {
            self.store().pool_payout_record_attempt(&payout.payout_id, Some("the payout wallet balance is below the amount, fee and reserve"), now)?;
            self.freeze_pools("the payout wallet balance is below what the next payout needs");
            return Ok(());
        }
        let (envelope, hash) = match signer.sign_transfer(&destination, sent, fee, account.nonce, now) {
            Ok(signed) => signed,
            Err(_) => {
                self.freeze_pools("the payout signer refused to sign");
                return Ok(());
            }
        };
        let json = serde_json::to_string(&envelope)?;
        // Persist before submitting: from here on, every retry is these exact bytes.
        self.store().pool_payout_mark_signed(&payout.payout_id, &json, &hex::encode(hash), account.nonce, fee, sent, now)?;
        self.pool_submit(&payout.payout_id, &envelope, &hash, now).await
    }

    async fn pool_submit(&self, id: &str, envelope: &SignedTransferRequest, hash: &[u8; 32], now: u64) -> Result<(), GamesError> {
        match self.node().submit(envelope).await {
            Ok(accepted) => {
                if !accepted.hash.eq_ignore_ascii_case(&hex::encode(hash)) {
                    self.freeze_pools("the node returned a different hash for the signed payout");
                    return Ok(());
                }
                self.store().pool_payout_mark_submitted(id, now)?;
                Ok(())
            }
            Err(NodeClientError::Unavailable) | Err(NodeClientError::InvalidResponse) => {
                // It may have arrived. The bytes are kept and the same bytes are tried again.
                self.store().pool_payout_record_attempt(id, Some("submission outcome unknown"), now)?;
                Ok(())
            }
            Err(_) => {
                self.store().pool_payout_record_attempt(id, Some("the node rejected the signed payout"), now)?;
                self.freeze_pools("the node rejected a signed payout; an operator must look before anything is re-signed");
                Ok(())
            }
        }
    }

    async fn pool_poll_or_resubmit(&self, payout: &PoolPayout, now: u64) -> Result<(), GamesError> {
        let (Some(json), Some(hash_hex)) = (&payout.envelope, &payout.tx_hash) else {
            self.freeze_pools("a signed payout is missing its envelope or hash");
            return Ok(());
        };
        let expected: [u8; 32] = hex::decode(hash_hex)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| GamesError::Internal("stored payout hash is malformed".into()))?;
        let config = self.service_config();
        let signed_at = payout.signed_at.unwrap_or(payout.created_at);
        let timed_out = now.saturating_sub(signed_at) >= config.confirmation_timeout_seconds || payout.attempts >= config.max_submission_attempts;
        match self.node().transaction_status(hash_hex).await {
            Ok(status) if !status.hash.eq_ignore_ascii_case(hash_hex) => {
                self.freeze_pools("the node answered about a different transaction than the payout");
                Ok(())
            }
            Ok(status) if status.status == "confirmed" => {
                self.store().pool_payout_confirm(&payout.payout_id, now)?;
                Ok(())
            }
            Ok(_) => {
                if timed_out {
                    self.freeze_pools("a payout was not confirmed in time; an operator must check it against the chain");
                }
                Ok(())
            }
            Err(NodeClientError::Unavailable) => {
                self.store().pool_payout_record_attempt(&payout.payout_id, Some("node unavailable while checking"), now)?;
                Ok(())
            }
            Err(_) => {
                if timed_out {
                    self.freeze_pools("a payout could not be found on the node in time; an operator must check it");
                    return Ok(());
                }
                // The node does not know it (it may have dropped it): send the identical stored bytes again.
                let envelope: SignedTransferRequest = serde_json::from_str(json)?;
                self.pool_submit(&payout.payout_id, &envelope, &expected, now).await
            }
        }
    }
}
