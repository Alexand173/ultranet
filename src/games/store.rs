//! The games database: wins, balances, payouts and anchored days, in one SQLite file.
//!
//! Every operation that decides or moves money runs inside a single `IMMEDIATE` transaction, so the
//! counters a rule reads and the reward it writes cannot disagree, even when requests overlap. The
//! counters are not separate columns that could drift: they are `COUNT`s over `wins`, the only
//! source of truth. [`GamesStore::audit`] re-derives every balance from `wins` and `payouts` and
//! reports any disagreement, which is what an operator should run before trusting a restored backup.

use super::rules::{self, Counters, Policy, RecordedReason, Solvency, Verdict, GAME_CHESS};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use thiserror::Error;

const SCHEMA_VERSION: i64 = 1;
pub const UTC_DAY_SECONDS: u64 = 86_400;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("games database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("games database lock was poisoned")]
    LockPoisoned,
    #[error("games data encoding error: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("a number is outside SQLite's safe integer range")]
    NumericRange,
    #[error("games record is malformed: {0}")]
    Malformed(String),
    /// A chess game id is already recorded for a different address or day.
    #[error("that game id belongs to a different result")]
    Conflict,
    #[error("the balance {available} is below the withdrawal minimum {minimum}")]
    BelowMinimum { available: u64, minimum: u64 },
    #[error("today's withdrawal ceiling has been reached")]
    PayoutDailyCap,
    #[error("payout {0} cannot be settled from its current state")]
    NotSettleable(String),
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn to_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::NumericRange)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rewarded,
    Recorded,
}

impl Outcome {
    fn code(self) -> &'static str {
        match self {
            Outcome::Rewarded => "rewarded",
            Outcome::Recorded => "recorded",
        }
    }

    fn parse(value: &str) -> Result<Outcome, StoreError> {
        match value {
            "rewarded" => Ok(Outcome::Rewarded),
            "recorded" => Ok(Outcome::Recorded),
            other => Err(StoreError::Malformed(format!("unknown outcome {other}"))),
        }
    }
}

/// A verified (or, for chess, reported) win about to be recorded.
#[derive(Clone, Debug)]
pub struct WinInput {
    pub game: &'static str,
    pub address: String,
    /// UTC day, `YYYY-MM-DD`.
    pub day: String,
    /// Grid: the level `0..9`. Chess: the game id.
    pub item: String,
    pub moves: u32,
    /// JSON the roots are rebuilt from (Grid: the puzzle id; Chess: mode and plies).
    pub detail: String,
    /// Whether the server replayed and proved this win. Grid yes, chess no, and the record says so.
    pub verified: bool,
    pub client_digest: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinRecord {
    pub id: i64,
    pub game: String,
    pub address: String,
    pub day: String,
    pub item: String,
    pub moves: u32,
    pub detail: String,
    pub verified: bool,
    pub reward_units: u64,
    pub outcome: Outcome,
    pub reason: Option<RecordedReason>,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct Recorded {
    pub win: WinRecord,
    /// True when this exact win had already been recorded: the answer is replayed, nothing is added.
    pub duplicate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayoutState {
    Queued,
    Signed,
    Submitted,
    Confirmed,
    Failed,
}

impl PayoutState {
    pub fn code(self) -> &'static str {
        match self {
            PayoutState::Queued => "queued",
            PayoutState::Signed => "signed",
            PayoutState::Submitted => "submitted",
            PayoutState::Confirmed => "confirmed",
            PayoutState::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<PayoutState, StoreError> {
        Ok(match value {
            "queued" => PayoutState::Queued,
            "signed" => PayoutState::Signed,
            "submitted" => PayoutState::Submitted,
            "confirmed" => PayoutState::Confirmed,
            "failed" => PayoutState::Failed,
            other => {
                return Err(StoreError::Malformed(format!(
                    "unknown payout state {other}"
                )))
            }
        })
    }

    pub fn is_open(self) -> bool {
        matches!(
            self,
            PayoutState::Queued | PayoutState::Signed | PayoutState::Submitted
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutRow {
    pub id: String,
    pub address: String,
    pub amount_units: u64,
    pub fee_units: Option<u64>,
    pub state: PayoutState,
    /// The signed request, persisted before it is ever submitted so a retry is always the same bytes.
    pub envelope: Option<String>,
    pub tx_hash: Option<String>,
    pub nonce: Option<u64>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// When the request was signed. Confirmation timeouts run from here, not from when it was queued,
    /// so a payout held back while payments were paused is not timed out the moment they resume.
    pub signed_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayCount {
    pub game: String,
    pub rewarded: u64,
    pub recorded: u64,
}

#[derive(Clone, Debug)]
pub struct WalletView {
    pub address: String,
    pub credited: u64,
    pub paid: u64,
    pub reserved: u64,
    /// `credited - paid - reserved`: what a withdrawal would pay.
    pub available: u64,
    pub open_payout: Option<PayoutRow>,
    pub today: Vec<DayCount>,
    pub recent: Vec<WinRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceState {
    /// Whether new wins are accepted at all.
    pub accepting: bool,
    /// Whether payouts are signed and submitted.
    pub paying: bool,
    pub reason: Option<String>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DayRow {
    pub day: String,
    pub game: String,
    pub prev_root: Option<Vec<u8>>,
    pub prev_anchor_sequence: Option<u64>,
    pub seed: Option<Vec<u8>>,
    pub commitment: Option<Vec<u8>>,
    pub results_root: Option<Vec<u8>>,
    pub day_root: Option<Vec<u8>>,
    /// `open`, `closing`, `anchored` (submitted) or `confirmed` (seen on the ledger).
    pub state: String,
    pub anchor_tx: Option<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditReport {
    pub issues: Vec<String>,
}

impl AuditReport {
    pub fn ok(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Clone)]
pub struct GamesStore {
    connection: Arc<Mutex<Connection>>,
}

const WIN_COLUMNS: &str =
    "id, game, address, day, item, moves, detail, verified, reward_units, outcome, reason, created_at";

fn win_from_row(row: &rusqlite::Row<'_>) -> Result<WinRecord, StoreError> {
    let reason: Option<String> = row.get(10)?;
    Ok(WinRecord {
        id: row.get(0)?,
        game: row.get(1)?,
        address: row.get(2)?,
        day: row.get(3)?,
        item: row.get(4)?,
        moves: u32::try_from(row.get::<_, i64>(5)?).map_err(|_| StoreError::NumericRange)?,
        detail: row.get(6)?,
        verified: row.get::<_, i64>(7)? != 0,
        reward_units: to_u64(row.get(8)?)?,
        outcome: Outcome::parse(&row.get::<_, String>(9)?)?,
        reason: reason.as_deref().and_then(RecordedReason::from_code),
        created_at: to_u64(row.get(11)?)?,
    })
}

const PAYOUT_COLUMNS: &str =
    "id, address, amount_units, fee_units, state, envelope, tx_hash, nonce, attempts, last_error, created_at, updated_at, signed_at";

fn payout_from_row(row: &rusqlite::Row<'_>) -> Result<PayoutRow, StoreError> {
    Ok(PayoutRow {
        id: row.get(0)?,
        address: row.get(1)?,
        amount_units: to_u64(row.get(2)?)?,
        fee_units: row.get::<_, Option<i64>>(3)?.map(to_u64).transpose()?,
        state: PayoutState::parse(&row.get::<_, String>(4)?)?,
        envelope: row.get(5)?,
        tx_hash: row.get(6)?,
        nonce: row.get::<_, Option<i64>>(7)?.map(to_u64).transpose()?,
        attempts: u32::try_from(row.get::<_, i64>(8)?).map_err(|_| StoreError::NumericRange)?,
        last_error: row.get(9)?,
        created_at: to_u64(row.get(10)?)?,
        updated_at: to_u64(row.get(11)?)?,
        signed_at: row.get::<_, Option<i64>>(12)?.map(to_u64).transpose()?,
    })
}

impl GamesStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if path != Path::new(":memory:") {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        StoreError::Malformed(format!(
                            "cannot create {}: {error}",
                            parent.display()
                        ))
                    })?;
                }
            }
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        migrate(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::LockPoisoned)
    }

    // --- service state -------------------------------------------------------------------

    pub fn state(&self) -> Result<ServiceState, StoreError> {
        let connection = self.lock()?;
        let (accepting, paying, reason, updated_at) = connection.query_row(
            "SELECT accepting, paying, reason, updated_at FROM service_state WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )?;
        Ok(ServiceState {
            accepting: accepting != 0,
            paying: paying != 0,
            reason,
            updated_at: to_u64(updated_at)?,
        })
    }

    /// Changes only the switches that are `Some`. The reason is kept for the operator who reads it later.
    pub fn set_state(
        &self,
        accepting: Option<bool>,
        paying: Option<bool>,
        reason: Option<&str>,
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        if let Some(value) = accepting {
            connection.execute(
                "UPDATE service_state SET accepting = ?1 WHERE id = 1",
                params![value as i64],
            )?;
        }
        if let Some(value) = paying {
            connection.execute(
                "UPDATE service_state SET paying = ?1 WHERE id = 1",
                params![value as i64],
            )?;
        }
        connection.execute(
            "UPDATE service_state SET reason = ?1, updated_at = ?2 WHERE id = 1",
            params![reason, to_i64(now)?],
        )?;
        Ok(())
    }

    // --- wins ----------------------------------------------------------------------------

    /// Record one win and decide its reward, atomically.
    ///
    /// A win already recorded for the same key is returned as it was (`duplicate`), never rewarded
    /// again. Chess game ids are unique across everything: reusing one for a different address or day
    /// is a [`StoreError::Conflict`], not a second prize.
    ///
    /// `pool_units` is the payout wallet's balance as just read from the chain. The reward is granted
    /// only if everything already owed, plus this reward, plus the reserve fits inside it: the service
    /// never credits a balance it could not pay.
    pub fn record_win(
        &self,
        input: &WinInput,
        policy: &Policy,
        on_chain: bool,
        pool_units: u64,
        now: u64,
    ) -> Result<Recorded, StoreError> {
        let game = policy
            .game(input.game)
            .ok_or_else(|| StoreError::Malformed(format!("unknown game {}", input.game)))?;
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let existing = if input.game == GAME_CHESS {
            tx.query_row(
                &format!("SELECT {WIN_COLUMNS} FROM wins WHERE game = ?1 AND item = ?2"),
                params![input.game, input.item],
                |row| Ok(win_from_row(row)),
            )
            .optional()?
        } else {
            tx.query_row(
                &format!(
                    "SELECT {WIN_COLUMNS} FROM wins WHERE game = ?1 AND address = ?2 AND day = ?3 AND item = ?4"
                ),
                params![input.game, input.address, input.day, input.item],
                |row| Ok(win_from_row(row)),
            )
            .optional()?
        };
        if let Some(found) = existing {
            let win = found?;
            if win.address != input.address || win.day != input.day {
                return Err(StoreError::Conflict);
            }
            return Ok(Recorded {
                win,
                duplicate: true,
            });
        }

        let count = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<u64, StoreError> {
            let value: i64 = tx.query_row(sql, args, |row| row.get(0))?;
            to_u64(value)
        };
        let counters = Counters {
            address_in_game: count(
                "SELECT COUNT(*) FROM wins WHERE game = ?1 AND address = ?2 AND day = ?3 AND outcome = 'rewarded'",
                &[&input.game, &input.address, &input.day],
            )? as u32,
            game: count(
                "SELECT COUNT(*) FROM wins WHERE game = ?1 AND day = ?2 AND outcome = 'rewarded'",
                &[&input.game, &input.day],
            )?,
            global: count(
                "SELECT COUNT(*) FROM wins WHERE day = ?1 AND outcome = 'rewarded'",
                &[&input.day],
            )?,
            client: match &input.client_digest {
                Some(digest) => count(
                    "SELECT COUNT(*) FROM wins WHERE day = ?1 AND client = ?2 AND outcome = 'rewarded'",
                    &[&input.day, &digest.as_slice()],
                )? as u32,
                None => 0,
            },
        };

        let owed: i64 = tx.query_row(
            "SELECT COALESCE(SUM(credited - paid), 0) FROM balances",
            [],
            |row| row.get(0),
        )?;
        let solvency = Solvency {
            pool_units,
            outstanding_units: to_u64(owed)?,
            reserve_units: policy.payout_reserve_units,
        };
        let verdict = rules::decide(policy, game, &counters, on_chain, &solvency);
        let (outcome, reward, reason) = match verdict {
            Verdict::Rewarded { units } => (Outcome::Rewarded, units, None),
            Verdict::Recorded(reason) => (Outcome::Recorded, 0, Some(reason)),
        };

        tx.execute(
            "INSERT INTO wins (game, address, day, item, moves, detail, verified, reward_units, outcome, reason, client, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                input.game,
                input.address,
                input.day,
                input.item,
                input.moves as i64,
                input.detail,
                input.verified as i64,
                to_i64(reward)?,
                outcome.code(),
                reason.map(|r| r.code()),
                input.client_digest.as_ref().map(|d| d.as_slice()),
                to_i64(now)?,
            ],
        )?;
        let id = tx.last_insert_rowid();
        if reward > 0 {
            tx.execute(
                "INSERT INTO balances (address, credited) VALUES (?1, ?2)
                 ON CONFLICT(address) DO UPDATE SET credited = credited + ?2",
                params![input.address, to_i64(reward)?],
            )?;
        }
        tx.commit()?;
        Ok(Recorded {
            win: WinRecord {
                id,
                game: input.game.to_string(),
                address: input.address.clone(),
                day: input.day.clone(),
                item: input.item.clone(),
                moves: input.moves,
                detail: input.detail.clone(),
                verified: input.verified,
                reward_units: reward,
                outcome,
                reason,
                created_at: now,
            },
            duplicate: false,
        })
    }

    /// For tests of every rule other than solvency: a pool so large it never limits anything.
    #[cfg(test)]
    pub fn record_win_unlimited(
        &self,
        input: &WinInput,
        policy: &Policy,
        on_chain: bool,
        now: u64,
    ) -> Result<Recorded, StoreError> {
        self.record_win(input, policy, on_chain, u64::MAX / 4, now)
    }

    pub fn wins_for_day(&self, game: &str, day: &str) -> Result<Vec<WinRecord>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {WIN_COLUMNS} FROM wins WHERE game = ?1 AND day = ?2 ORDER BY id"
        ))?;
        let rows = statement.query_map(params![game, day], |row| Ok(win_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    // --- wallets -------------------------------------------------------------------------

    pub fn wallet(&self, address: &str, today: &str) -> Result<WalletView, StoreError> {
        let connection = self.lock()?;
        let (credited, paid, reserved) = connection
            .query_row(
                "SELECT credited, paid, reserved FROM balances WHERE address = ?1",
                params![address],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or((0, 0, 0));
        let (credited, paid, reserved) = (to_u64(credited)?, to_u64(paid)?, to_u64(reserved)?);
        let open_payout = connection
            .query_row(
                &format!(
                    "SELECT {PAYOUT_COLUMNS} FROM payouts WHERE address = ?1 AND state IN ('queued','signed','submitted')"
                ),
                params![address],
                |row| Ok(payout_from_row(row)),
            )
            .optional()?
            .transpose()?;

        let mut today_counts = Vec::new();
        {
            let mut statement = connection.prepare(
                "SELECT game,
                        SUM(CASE WHEN outcome = 'rewarded' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN outcome = 'recorded' THEN 1 ELSE 0 END)
                 FROM wins WHERE address = ?1 AND day = ?2 GROUP BY game ORDER BY game",
            )?;
            let rows = statement.query_map(params![address, today], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (game, rewarded, recorded) = row?;
                today_counts.push(DayCount {
                    game,
                    rewarded: to_u64(rewarded)?,
                    recorded: to_u64(recorded)?,
                });
            }
        }

        let mut recent = Vec::new();
        {
            let mut statement = connection.prepare(&format!(
                "SELECT {WIN_COLUMNS} FROM wins WHERE address = ?1 ORDER BY id DESC LIMIT 20"
            ))?;
            let rows = statement.query_map(params![address], |row| Ok(win_from_row(row)))?;
            for row in rows {
                recent.push(row??);
            }
        }

        Ok(WalletView {
            address: address.to_string(),
            credited,
            paid,
            reserved,
            available: credited.saturating_sub(paid).saturating_sub(reserved),
            open_payout,
            today: today_counts,
            recent,
        })
    }

    // --- payouts -------------------------------------------------------------------------

    /// Queue a withdrawal of the address's whole available balance, or return the one already open.
    /// The amount is reserved at once, so the same credit can never be queued twice.
    pub fn create_payout(
        &self,
        address: &str,
        policy: &Policy,
        id: &str,
        day_start: u64,
        now: u64,
    ) -> Result<(PayoutRow, bool), StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let open = tx
            .query_row(
                &format!(
                    "SELECT {PAYOUT_COLUMNS} FROM payouts WHERE address = ?1 AND state IN ('queued','signed','submitted')"
                ),
                params![address],
                |row| Ok(payout_from_row(row)),
            )
            .optional()?
            .transpose()?;
        if let Some(existing) = open {
            return Ok((existing, false));
        }

        let (credited, paid, reserved) = tx
            .query_row(
                "SELECT credited, paid, reserved FROM balances WHERE address = ?1",
                params![address],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or((0, 0, 0));
        let available = to_u64(credited)?
            .saturating_sub(to_u64(paid)?)
            .saturating_sub(to_u64(reserved)?);
        if available < policy.withdraw_min_units {
            return Err(StoreError::BelowMinimum {
                available,
                minimum: policy.withdraw_min_units,
            });
        }

        let queued_today: i64 = tx.query_row(
            "SELECT COALESCE(SUM(amount_units), 0) FROM payouts WHERE created_at >= ?1 AND state != 'failed'",
            params![to_i64(day_start)?],
            |row| row.get(0),
        )?;
        if to_u64(queued_today)?.saturating_add(available) > policy.payout_daily_cap_units {
            return Err(StoreError::PayoutDailyCap);
        }

        tx.execute(
            "INSERT INTO payouts (id, address, amount_units, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'queued', 0, ?4, ?4)",
            params![id, address, to_i64(available)?, to_i64(now)?],
        )?;
        tx.execute(
            "UPDATE balances SET reserved = reserved + ?2 WHERE address = ?1",
            params![address, to_i64(available)?],
        )?;
        let row = tx.query_row(
            &format!("SELECT {PAYOUT_COLUMNS} FROM payouts WHERE id = ?1"),
            params![id],
            |row| Ok(payout_from_row(row)),
        )??;
        tx.commit()?;
        Ok((row, true))
    }

    pub fn payout(&self, id: &str) -> Result<Option<PayoutRow>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!("SELECT {PAYOUT_COLUMNS} FROM payouts WHERE id = ?1"),
                params![id],
                |row| Ok(payout_from_row(row)),
            )
            .optional()?
            .transpose()
    }

    /// The oldest payout that is not finished. Payouts are handled strictly one at a time, in order,
    /// because they share one wallet and therefore one nonce sequence.
    pub fn next_open_payout(&self) -> Result<Option<PayoutRow>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                &format!(
                    "SELECT {PAYOUT_COLUMNS} FROM payouts WHERE state IN ('queued','signed','submitted')
                     ORDER BY created_at, id LIMIT 1"
                ),
                [],
                |row| Ok(payout_from_row(row)),
            )
            .optional()?
            .transpose()
    }

    pub fn open_payouts(&self) -> Result<Vec<PayoutRow>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {PAYOUT_COLUMNS} FROM payouts WHERE state IN ('queued','signed','submitted') ORDER BY created_at, id"
        ))?;
        let rows = statement.query_map([], |row| Ok(payout_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    /// Persist the signed request. Only a queued payout can be signed, and it is signed exactly once.
    pub fn mark_signed(
        &self,
        id: &str,
        envelope: &str,
        tx_hash: &str,
        nonce: u64,
        fee_units: u64,
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        let changed = connection.execute(
            "UPDATE payouts SET state = 'signed', envelope = ?2, tx_hash = ?3, nonce = ?4, fee_units = ?5, updated_at = ?6, signed_at = ?6
             WHERE id = ?1 AND state = 'queued' AND envelope IS NULL",
            params![id, envelope, tx_hash, to_i64(nonce)?, to_i64(fee_units)?, to_i64(now)?],
        )?;
        if changed != 1 {
            return Err(StoreError::NotSettleable(id.to_string()));
        }
        Ok(())
    }

    pub fn mark_submitted(&self, id: &str, now: u64) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE payouts SET state = 'submitted', updated_at = ?2 WHERE id = ?1 AND state IN ('signed','submitted')",
            params![id, to_i64(now)?],
        )?;
        Ok(())
    }

    pub fn record_attempt(
        &self,
        id: &str,
        error: Option<&str>,
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE payouts SET attempts = attempts + 1, last_error = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, error, to_i64(now)?],
        )?;
        Ok(())
    }

    /// Settle a payout one way or the other, once.
    ///
    /// `confirmed`: the transfer is on the chain, so the amount becomes `paid`.
    /// `failed`: it never landed, so the reservation is released and the player's balance is whole again.
    /// A payout whose envelope exists must be settled by evidence (the chain), never by guessing: the
    /// service confirms from the node, and an operator resolves the ambiguous ones.
    pub fn settle_payout(
        &self,
        id: &str,
        confirmed: bool,
        error: Option<&str>,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (address, amount, state): (String, i64, String) = tx
            .query_row(
                "SELECT address, amount_units, state FROM payouts WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotSettleable(id.to_string()))?;
        if !PayoutState::parse(&state)?.is_open() {
            return Err(StoreError::NotSettleable(id.to_string()));
        }
        if confirmed {
            tx.execute(
                "UPDATE payouts SET state = 'confirmed', updated_at = ?2 WHERE id = ?1",
                params![id, to_i64(now)?],
            )?;
            tx.execute(
                "UPDATE balances SET paid = paid + ?2, reserved = reserved - ?2 WHERE address = ?1",
                params![address, amount],
            )?;
        } else {
            tx.execute(
                "UPDATE payouts SET state = 'failed', last_error = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, error, to_i64(now)?],
            )?;
            tx.execute(
                "UPDATE balances SET reserved = reserved - ?2 WHERE address = ?1",
                params![address, amount],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // --- days and anchors ----------------------------------------------------------------

    pub fn day_row(&self, day: &str, game: &str) -> Result<Option<DayRow>, StoreError> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT day, game, prev_root, prev_anchor_seq, seed, commitment, results_root, day_root,
                        state, anchor_tx, attempts, last_error, updated_at
                 FROM days WHERE day = ?1 AND game = ?2",
                params![day, game],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, Option<Vec<u8>>>(6)?,
                        row.get::<_, Option<Vec<u8>>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, i64>(12)?,
                    ))
                },
            )
            .optional()?
            .map(|r| {
                Ok(DayRow {
                    day: r.0,
                    game: r.1,
                    prev_root: r.2,
                    prev_anchor_sequence: r.3.map(to_u64).transpose()?,
                    seed: r.4,
                    commitment: r.5,
                    results_root: r.6,
                    day_root: r.7,
                    state: r.8,
                    anchor_tx: r.9,
                    attempts: u32::try_from(r.10).map_err(|_| StoreError::NumericRange)?,
                    last_error: r.11,
                    updated_at: to_u64(r.12)?,
                })
            })
            .transpose()
    }

    /// Record the seed inputs of a Grid day the first time they are read from the ledger. Never
    /// overwritten: once a day's puzzles are published they must not change, whatever the ledger does.
    pub fn put_grid_day(
        &self,
        day: &str,
        prev_root: &[u8; 32],
        prev_anchor_sequence: Option<u64>,
        seed: &[u8; 32],
        commitment: &[u8; 32],
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "INSERT OR IGNORE INTO days (day, game, prev_root, prev_anchor_seq, seed, commitment, state, attempts, updated_at)
             VALUES (?1, 'grid', ?2, ?3, ?4, ?5, 'open', 0, ?6)",
            params![
                day,
                prev_root.as_slice(),
                prev_anchor_sequence.map(to_i64).transpose()?,
                seed.as_slice(),
                commitment.as_slice(),
                to_i64(now)?
            ],
        )?;
        Ok(())
    }

    /// Fix a day's roots. The first computation wins: a day closed once is never recomputed, so the
    /// root that gets anchored is the root that was stored.
    pub fn close_day(
        &self,
        day: &str,
        game: &str,
        results_root: &[u8; 32],
        day_root: &[u8; 32],
        now: u64,
    ) -> Result<DayRow, StoreError> {
        {
            let connection = self.lock()?;
            connection.execute(
                "INSERT OR IGNORE INTO days (day, game, state, attempts, updated_at) VALUES (?1, ?2, 'open', 0, ?3)",
                params![day, game, to_i64(now)?],
            )?;
            connection.execute(
                "UPDATE days SET results_root = ?3, day_root = ?4, state = 'closing', updated_at = ?5
                 WHERE day = ?1 AND game = ?2 AND day_root IS NULL",
                params![day, game, results_root.as_slice(), day_root.as_slice(), to_i64(now)?],
            )?;
        }
        self.day_row(day, game)?
            .ok_or_else(|| StoreError::Malformed("day row vanished".into()))
    }

    pub fn set_anchor(
        &self,
        day: &str,
        game: &str,
        tx_hash: Option<&str>,
        state: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE days SET state = ?3, anchor_tx = COALESCE(?4, anchor_tx), updated_at = ?5 WHERE day = ?1 AND game = ?2",
            params![day, game, state, tx_hash, to_i64(now)?],
        )?;
        Ok(())
    }

    pub fn record_anchor_error(
        &self,
        day: &str,
        game: &str,
        error: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE days SET attempts = attempts + 1, last_error = ?3, updated_at = ?4 WHERE day = ?1 AND game = ?2",
            params![day, game, error, to_i64(now)?],
        )?;
        Ok(())
    }

    // --- audit ---------------------------------------------------------------------------

    /// Re-derive every balance from the wins and payouts that are supposed to explain it.
    pub fn audit(&self) -> Result<AuditReport, StoreError> {
        let connection = self.lock()?;
        let mut issues = Vec::new();

        let mut statement = connection.prepare(
            "SELECT b.address, b.credited, b.paid, b.reserved,
                    COALESCE((SELECT SUM(reward_units) FROM wins w WHERE w.address = b.address AND w.outcome = 'rewarded'), 0),
                    COALESCE((SELECT SUM(amount_units) FROM payouts p WHERE p.address = b.address AND p.state = 'confirmed'), 0),
                    COALESCE((SELECT SUM(amount_units) FROM payouts p WHERE p.address = b.address AND p.state IN ('queued','signed','submitted')), 0)
             FROM balances b",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        for row in rows {
            let (address, credited, paid, reserved, from_wins, from_confirmed, from_open) = row?;
            if credited != from_wins {
                issues.push(format!(
                    "{address}: credited {credited} but its rewarded wins sum to {from_wins}"
                ));
            }
            if paid != from_confirmed {
                issues.push(format!(
                    "{address}: paid {paid} but confirmed payouts sum to {from_confirmed}"
                ));
            }
            if reserved != from_open {
                issues.push(format!(
                    "{address}: reserved {reserved} but open payouts sum to {from_open}"
                ));
            }
            if paid + reserved > credited {
                issues.push(format!(
                    "{address}: paid {paid} + reserved {reserved} exceeds credited {credited}"
                ));
            }
        }

        let orphans: i64 = connection.query_row(
            "SELECT COUNT(*) FROM (SELECT address FROM wins WHERE outcome = 'rewarded' GROUP BY address
                                   HAVING address NOT IN (SELECT address FROM balances))",
            [],
            |row| row.get(0),
        )?;
        if orphans > 0 {
            issues.push(format!(
                "{orphans} address(es) have rewarded wins but no balance row"
            ));
        }
        let mismatched: i64 = connection.query_row(
            "SELECT COUNT(*) FROM wins WHERE (outcome = 'rewarded' AND reward_units <= 0) OR (outcome = 'recorded' AND reward_units != 0)",
            [],
            |row| row.get(0),
        )?;
        if mismatched > 0 {
            issues.push(format!(
                "{mismatched} win(s) have a reward that contradicts their outcome"
            ));
        }
        Ok(AuditReport { issues })
    }

    /// A consistent copy of the whole database, safe to take while the service is running (SQLite's
    /// `VACUUM INTO` reads one snapshot). The host has no `sqlite3` command, so this is how a backup is
    /// made. It refuses to overwrite: a backup that silently replaced a good one is worse than none.
    pub fn backup_to(&self, destination: &Path) -> Result<(), StoreError> {
        if destination.exists() {
            return Err(StoreError::Malformed(format!(
                "refusing to overwrite {}; choose a new file name",
                destination.display()
            )));
        }
        let connection = self.lock()?;
        connection.execute("VACUUM INTO ?1", params![destination.to_string_lossy()])?;
        Ok(())
    }

    /// Totals for the operator's status line.
    pub fn totals(&self) -> Result<(u64, u64, u64), StoreError> {
        let connection = self.lock()?;
        let row = connection.query_row(
            "SELECT COALESCE(SUM(credited),0), COALESCE(SUM(paid),0), COALESCE(SUM(reserved),0) FROM balances",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
        )?;
        Ok((to_u64(row.0)?, to_u64(row.1)?, to_u64(row.2)?))
    }
}

fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    let version: Option<i64> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| value.parse().ok());
    match version {
        Some(SCHEMA_VERSION) => return Ok(()),
        Some(other) => {
            return Err(StoreError::Malformed(format!(
                "database schema {other} is not understood by this build (expects {SCHEMA_VERSION})"
            )))
        }
        None => {}
    }
    connection.execute_batch(
        "CREATE TABLE service_state (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             accepting INTEGER NOT NULL,
             paying INTEGER NOT NULL,
             reason TEXT,
             updated_at INTEGER NOT NULL
         );
         INSERT INTO service_state (id, accepting, paying, reason, updated_at) VALUES (1, 1, 1, NULL, 0);

         CREATE TABLE wins (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             game TEXT NOT NULL,
             address TEXT NOT NULL,
             day TEXT NOT NULL,
             item TEXT NOT NULL,
             moves INTEGER NOT NULL,
             detail TEXT NOT NULL,
             verified INTEGER NOT NULL,
             reward_units INTEGER NOT NULL CHECK (reward_units >= 0),
             outcome TEXT NOT NULL CHECK (outcome IN ('rewarded','recorded')),
             reason TEXT,
             client BLOB,
             created_at INTEGER NOT NULL,
             UNIQUE (game, address, day, item)
         );
         CREATE UNIQUE INDEX wins_chess_game ON wins (game, item) WHERE game = 'chess';
         CREATE INDEX wins_by_address_day ON wins (game, address, day, outcome);
         CREATE INDEX wins_by_day ON wins (day, outcome);
         CREATE INDEX wins_by_client_day ON wins (day, client, outcome);

         CREATE TABLE balances (
             address TEXT PRIMARY KEY,
             credited INTEGER NOT NULL DEFAULT 0 CHECK (credited >= 0),
             paid INTEGER NOT NULL DEFAULT 0 CHECK (paid >= 0),
             reserved INTEGER NOT NULL DEFAULT 0 CHECK (reserved >= 0),
             CHECK (paid + reserved <= credited)
         );

         CREATE TABLE payouts (
             id TEXT PRIMARY KEY,
             address TEXT NOT NULL,
             amount_units INTEGER NOT NULL CHECK (amount_units > 0),
             fee_units INTEGER,
             state TEXT NOT NULL CHECK (state IN ('queued','signed','submitted','confirmed','failed')),
             envelope TEXT,
             tx_hash TEXT,
             nonce INTEGER,
             attempts INTEGER NOT NULL DEFAULT 0,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             signed_at INTEGER
         );
         CREATE UNIQUE INDEX payouts_one_open_per_address ON payouts (address)
             WHERE state IN ('queued','signed','submitted');
         CREATE INDEX payouts_by_state ON payouts (state, created_at);

         CREATE TABLE days (
             day TEXT NOT NULL,
             game TEXT NOT NULL,
             prev_root BLOB,
             prev_anchor_seq INTEGER,
             seed BLOB,
             commitment BLOB,
             results_root BLOB,
             day_root BLOB,
             state TEXT NOT NULL,
             anchor_tx TEXT,
             attempts INTEGER NOT NULL DEFAULT 0,
             last_error TEXT,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (day, game)
         );",
    )?;
    connection.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
        params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    const B: &str = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
    const DAY: &str = "2026-11-16";
    const NOW: u64 = 1_794_800_000;

    fn store() -> GamesStore {
        GamesStore::open(Path::new(":memory:")).unwrap()
    }

    fn grid_win(address: &str, level: u32) -> WinInput {
        WinInput {
            game: "grid",
            address: address.to_string(),
            day: DAY.to_string(),
            item: level.to_string(),
            moves: 5,
            detail: format!("{{\"puzzle\":\"p{level}\"}}"),
            verified: true,
            client_digest: None,
        }
    }

    fn chess_win(address: &str, game_id: &str) -> WinInput {
        WinInput {
            game: "chess",
            address: address.to_string(),
            day: DAY.to_string(),
            item: game_id.to_string(),
            moves: 40,
            detail: "{\"mode\":\"classic\",\"plies\":40}".to_string(),
            verified: false,
            client_digest: None,
        }
    }

    #[test]
    fn a_rewarded_win_credits_the_balance_and_the_audit_agrees() {
        let store = store();
        let policy = Policy::standard();
        let recorded = store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW)
            .unwrap();
        assert!(!recorded.duplicate);
        assert_eq!(recorded.win.outcome, Outcome::Rewarded);
        assert_eq!(recorded.win.reward_units, 10);
        let wallet = store.wallet(A, DAY).unwrap();
        assert_eq!((wallet.credited, wallet.available), (10, 10));
        assert!(store.audit().unwrap().ok());
    }

    #[test]
    fn the_same_win_twice_is_replayed_and_paid_once() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW)
            .unwrap();
        let again = store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW + 5)
            .unwrap();
        assert!(again.duplicate);
        assert_eq!(again.win.reward_units, 10);
        assert_eq!(store.wallet(A, DAY).unwrap().credited, 10);
        assert_eq!(store.wins_for_day("grid", DAY).unwrap().len(), 1);
    }

    #[test]
    fn the_fourth_grid_win_of_a_day_is_recorded_and_unpaid() {
        let store = store();
        let policy = Policy::standard();
        for level in 0..3 {
            let r = store
                .record_win_unlimited(&grid_win(A, level), &policy, true, NOW)
                .unwrap();
            assert_eq!(r.win.outcome, Outcome::Rewarded, "level {level}");
        }
        let fourth = store
            .record_win_unlimited(&grid_win(A, 3), &policy, true, NOW)
            .unwrap();
        assert_eq!(fourth.win.outcome, Outcome::Recorded);
        assert_eq!(fourth.win.reason, Some(RecordedReason::AddressLimit));
        assert_eq!(fourth.win.reward_units, 0);
        // The win still exists, so it still anchors; it just carries no money.
        assert_eq!(store.wins_for_day("grid", DAY).unwrap().len(), 4);
        assert_eq!(store.wallet(A, DAY).unwrap().credited, 30);
        // Another address is unaffected by A's limit.
        assert_eq!(
            store
                .record_win_unlimited(&grid_win(B, 0), &policy, true, NOW)
                .unwrap()
                .win
                .outcome,
            Outcome::Rewarded
        );
        assert!(store.audit().unwrap().ok());
    }

    #[test]
    fn limits_are_per_game_so_chess_still_pays_after_grid_is_spent() {
        let store = store();
        let policy = Policy::standard();
        for level in 0..4 {
            store
                .record_win_unlimited(&grid_win(A, level), &policy, true, NOW)
                .unwrap();
        }
        let chess = store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        assert_eq!(chess.win.outcome, Outcome::Rewarded);
        assert_eq!(chess.win.reward_units, 100);
        assert_eq!(store.wallet(A, DAY).unwrap().credited, 130);
    }

    #[test]
    fn an_address_that_is_not_on_the_chain_is_recorded_with_a_reason() {
        let store = store();
        let r = store
            .record_win_unlimited(&grid_win(A, 0), &Policy::standard(), false, NOW)
            .unwrap();
        assert_eq!(r.win.outcome, Outcome::Recorded);
        assert_eq!(r.win.reason, Some(RecordedReason::NotOnChain));
        assert_eq!(store.wallet(A, DAY).unwrap().credited, 0);
    }

    #[test]
    fn a_chess_game_id_pays_once_and_cannot_be_reused_by_someone_else() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        // Same address, same game: replayed.
        assert!(
            store
                .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
                .unwrap()
                .duplicate
        );
        // Another address claiming the same game id is a conflict, not a second prize.
        assert!(matches!(
            store.record_win_unlimited(&chess_win(B, "game-aaaa-0001"), &policy, true, NOW),
            Err(StoreError::Conflict)
        ));
        assert_eq!(store.wallet(B, DAY).unwrap().credited, 0);
    }

    #[test]
    fn the_per_client_limit_counts_across_addresses() {
        let store = store();
        let mut policy = Policy::standard();
        policy.rewarded_per_client_per_day = 2;
        let digest = Some([9u8; 32]);
        let win = |address: &str, level| WinInput {
            client_digest: digest,
            ..grid_win(address, level)
        };
        assert_eq!(
            store
                .record_win_unlimited(&win(A, 0), &policy, true, NOW)
                .unwrap()
                .win
                .outcome,
            Outcome::Rewarded
        );
        assert_eq!(
            store
                .record_win_unlimited(&win(B, 0), &policy, true, NOW)
                .unwrap()
                .win
                .outcome,
            Outcome::Rewarded
        );
        let third = store
            .record_win_unlimited(&win(A, 1), &policy, true, NOW)
            .unwrap();
        assert_eq!(third.win.reason, Some(RecordedReason::ClientLimit));
    }

    #[test]
    fn the_games_and_global_ceilings_stop_payment_not_recording() {
        let store = store();
        let mut policy = Policy::standard();
        policy.grid.rewarded_per_day = 2;
        let addresses: Vec<String> = (0..4).map(|i| format!("{:064x}", i + 1)).collect();
        let outcomes: Vec<Option<RecordedReason>> = addresses
            .iter()
            .map(|a| {
                store
                    .record_win_unlimited(&grid_win(a, 0), &policy, true, NOW)
                    .unwrap()
                    .win
                    .reason
            })
            .collect();
        assert_eq!(
            outcomes,
            vec![
                None,
                None,
                Some(RecordedReason::GameLimit),
                Some(RecordedReason::GameLimit)
            ]
        );

        let mut policy = Policy::standard();
        policy.rewarded_global_per_day = 1;
        let store = self::store();
        let first = store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW)
            .unwrap();
        let second = store
            .record_win_unlimited(&grid_win(B, 0), &policy, true, NOW)
            .unwrap();
        assert_eq!(first.win.outcome, Outcome::Rewarded);
        assert_eq!(second.win.reason, Some(RecordedReason::GlobalLimit));
    }

    #[test]
    fn the_pool_is_checked_against_everything_owed_inside_the_same_transaction() {
        let store = store();
        let policy = Policy::standard();
        // The reserve is 1,000, so a wallet of 1,020 leaves room for exactly 20 units of promises.
        let pool = 1_020;
        let first = store
            .record_win(&grid_win(A, 0), &policy, true, pool, NOW)
            .unwrap();
        let second = store
            .record_win(&grid_win(B, 0), &policy, true, pool, NOW)
            .unwrap();
        let third = store
            .record_win(&grid_win(A, 1), &policy, true, pool, NOW)
            .unwrap();
        assert_eq!(first.win.outcome, Outcome::Rewarded);
        assert_eq!(second.win.outcome, Outcome::Rewarded);
        assert_eq!(
            (third.win.outcome, third.win.reason),
            (Outcome::Recorded, Some(RecordedReason::PoolLow))
        );
        assert_eq!(store.totals().unwrap().0, 20);
        // Paying a balance out frees room for new promises: the wallet shrank by the same amount.
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store.settle_payout("p1", true, None, NOW).unwrap();
        assert_eq!(store.totals().unwrap(), (120, 110, 0));
    }

    #[test]
    fn a_new_day_resets_the_limits() {
        let store = store();
        let policy = Policy::standard();
        for level in 0..4 {
            store
                .record_win_unlimited(&grid_win(A, level), &policy, true, NOW)
                .unwrap();
        }
        let next = WinInput {
            day: "2026-11-17".to_string(),
            ..grid_win(A, 0)
        };
        assert_eq!(
            store
                .record_win_unlimited(&next, &policy, true, NOW + 86_400)
                .unwrap()
                .win
                .outcome,
            Outcome::Rewarded
        );
    }

    #[test]
    fn a_withdrawal_needs_the_minimum_and_pays_the_whole_balance() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW)
            .unwrap();
        assert!(matches!(
            store.create_payout(A, &policy, "p1", NOW, NOW),
            Err(StoreError::BelowMinimum {
                available: 10,
                minimum: 100
            })
        ));
        // Chess pays 100, so one chess win clears the minimum; the whole 110 is withdrawn.
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        let (payout, created) = store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        assert!(created);
        assert_eq!(payout.amount_units, 110);
        assert_eq!(payout.state, PayoutState::Queued);
        let wallet = store.wallet(A, DAY).unwrap();
        assert_eq!(
            (wallet.credited, wallet.reserved, wallet.available),
            (110, 110, 0)
        );
        assert!(store.audit().unwrap().ok());
    }

    #[test]
    fn asking_again_returns_the_open_payout_and_reserves_nothing_twice() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        let (first, created) = store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        assert!(created);
        let (again, created_again) = store.create_payout(A, &policy, "p2", NOW, NOW + 1).unwrap();
        assert!(!created_again);
        assert_eq!(again.id, first.id);
        assert_eq!(store.wallet(A, DAY).unwrap().reserved, 100);
    }

    #[test]
    fn a_confirmed_payout_moves_reserved_to_paid_and_new_wins_can_be_withdrawn_later() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store.mark_signed("p1", "{}", "abcd", 7, 1, NOW).unwrap();
        store.mark_submitted("p1", NOW).unwrap();
        store.settle_payout("p1", true, None, NOW + 10).unwrap();
        let wallet = store.wallet(A, DAY).unwrap();
        assert_eq!(
            (
                wallet.credited,
                wallet.paid,
                wallet.reserved,
                wallet.available
            ),
            (100, 100, 0, 0)
        );
        assert!(wallet.open_payout.is_none());
        assert!(store.audit().unwrap().ok());

        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0002"), &policy, true, NOW + 20)
            .unwrap();
        let (second, created) = store
            .create_payout(A, &policy, "p2", NOW, NOW + 30)
            .unwrap();
        assert!(created);
        assert_eq!(second.amount_units, 100);
    }

    #[test]
    fn a_failed_payout_releases_the_reservation() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store
            .settle_payout("p1", false, Some("pool empty"), NOW + 1)
            .unwrap();
        let wallet = store.wallet(A, DAY).unwrap();
        assert_eq!(
            (
                wallet.credited,
                wallet.paid,
                wallet.reserved,
                wallet.available
            ),
            (100, 0, 0, 100)
        );
        assert!(store.audit().unwrap().ok());
        // The player can ask again.
        assert!(
            store
                .create_payout(A, &policy, "p2", NOW, NOW + 2)
                .unwrap()
                .1
        );
    }

    #[test]
    fn a_payout_settles_once() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store.settle_payout("p1", true, None, NOW).unwrap();
        assert!(matches!(
            store.settle_payout("p1", true, None, NOW),
            Err(StoreError::NotSettleable(_))
        ));
        assert!(matches!(
            store.settle_payout("p1", false, None, NOW),
            Err(StoreError::NotSettleable(_))
        ));
        assert_eq!(store.wallet(A, DAY).unwrap().paid, 100);
    }

    #[test]
    fn a_payout_is_signed_exactly_once() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store
            .mark_signed("p1", "{\"a\":1}", "abcd", 3, 1, NOW)
            .unwrap();
        assert!(matches!(
            store.mark_signed("p1", "{\"a\":2}", "ffff", 4, 1, NOW),
            Err(StoreError::NotSettleable(_))
        ));
        let row = store.payout("p1").unwrap().unwrap();
        assert_eq!(
            row.envelope.as_deref(),
            Some("{\"a\":1}"),
            "the persisted envelope never changes"
        );
        assert_eq!(row.tx_hash.as_deref(), Some("abcd"));
    }

    #[test]
    fn the_daily_withdrawal_ceiling_holds() {
        let store = store();
        let mut policy = Policy::standard();
        policy.payout_daily_cap_units = 150;
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store
            .record_win_unlimited(&chess_win(B, "game-bbbb-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        assert!(matches!(
            store.create_payout(B, &policy, "p2", NOW, NOW),
            Err(StoreError::PayoutDailyCap)
        ));
        // B's balance is untouched and still withdrawable tomorrow.
        assert_eq!(store.wallet(B, DAY).unwrap().available, 100);
        assert!(store
            .create_payout(B, &policy, "p3", NOW + 86_400, NOW + 86_400)
            .is_ok());
    }

    #[test]
    fn payouts_are_handled_oldest_first() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store
            .record_win_unlimited(&chess_win(B, "game-bbbb-0001"), &policy, true, NOW)
            .unwrap();
        store
            .create_payout(B, &policy, "second", NOW, NOW + 5)
            .unwrap();
        store.create_payout(A, &policy, "first", NOW, NOW).unwrap();
        assert_eq!(store.next_open_payout().unwrap().unwrap().id, "first");
        store.settle_payout("first", true, None, NOW + 9).unwrap();
        assert_eq!(store.next_open_payout().unwrap().unwrap().id, "second");
    }

    #[test]
    fn the_audit_catches_a_balance_that_money_does_not_explain() {
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&grid_win(A, 0), &policy, true, NOW)
            .unwrap();
        assert!(store.audit().unwrap().ok());
        // Tamper: credit that no win accounts for.
        store
            .lock()
            .unwrap()
            .execute(
                "UPDATE balances SET credited = credited + 7 WHERE address = ?1",
                params![A],
            )
            .unwrap();
        let report = store.audit().unwrap();
        assert!(!report.ok());
        assert!(
            report.issues[0].contains("credited 17 but its rewarded wins sum to 10"),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn the_database_refuses_to_over_reserve() {
        let store = store();
        store
            .record_win_unlimited(&grid_win(A, 0), &Policy::standard(), true, NOW)
            .unwrap();
        // The schema itself rejects paid + reserved > credited, independent of the service code.
        let result = store.lock().unwrap().execute(
            "UPDATE balances SET reserved = 11 WHERE address = ?1",
            params![A],
        );
        assert!(result.is_err());
    }

    #[test]
    fn the_service_state_defaults_to_open_and_changes_one_switch_at_a_time() {
        let store = store();
        let state = store.state().unwrap();
        assert!(state.accepting && state.paying);
        store
            .set_state(None, Some(false), Some("pool below reserve"), NOW)
            .unwrap();
        let state = store.state().unwrap();
        assert!(state.accepting && !state.paying);
        assert_eq!(state.reason.as_deref(), Some("pool below reserve"));
    }

    #[test]
    fn a_grid_days_seed_inputs_are_fixed_the_first_time_they_are_stored() {
        let store = store();
        store
            .put_grid_day(DAY, &[1u8; 32], Some(1), &[2u8; 32], &[3u8; 32], NOW)
            .unwrap();
        // A second write with different inputs must not change the day: its puzzles are already public.
        store
            .put_grid_day(DAY, &[9u8; 32], Some(5), &[8u8; 32], &[7u8; 32], NOW + 1)
            .unwrap();
        let row = store.day_row(DAY, "grid").unwrap().unwrap();
        assert_eq!(row.prev_root, Some(vec![1u8; 32]));
        assert_eq!(row.seed, Some(vec![2u8; 32]));
        assert_eq!(row.prev_anchor_sequence, Some(1));
    }

    #[test]
    fn a_day_closes_once_and_the_first_root_is_the_one_kept() {
        let store = store();
        let first = store
            .close_day(DAY, "grid", &[1u8; 32], &[2u8; 32], NOW)
            .unwrap();
        assert_eq!(first.day_root, Some(vec![2u8; 32]));
        assert_eq!(first.state, "closing");
        // Recomputing (even with different results) must never replace what may already be anchored.
        let again = store
            .close_day(DAY, "grid", &[5u8; 32], &[6u8; 32], NOW + 1)
            .unwrap();
        assert_eq!(again.day_root, Some(vec![2u8; 32]));
        assert_eq!(again.results_root, Some(vec![1u8; 32]));
        store
            .set_anchor(DAY, "grid", Some("abc123"), "anchored", NOW + 2)
            .unwrap();
        assert_eq!(
            store
                .day_row(DAY, "grid")
                .unwrap()
                .unwrap()
                .anchor_tx
                .as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn a_backup_is_a_complete_consistent_copy_that_audits_clean_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("games-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = store();
        let policy = Policy::standard();
        store
            .record_win_unlimited(&chess_win(A, "game-aaaa-0001"), &policy, true, NOW)
            .unwrap();
        store.create_payout(A, &policy, "p1", NOW, NOW).unwrap();
        store
            .record_win_unlimited(&grid_win(B, 0), &policy, true, NOW)
            .unwrap();

        let copy = dir.join("games-backup.db");
        store.backup_to(&copy).unwrap();
        let restored = GamesStore::open(&copy).unwrap();
        assert_eq!(restored.totals().unwrap(), store.totals().unwrap());
        assert_eq!(
            restored.open_payouts().unwrap(),
            store.open_payouts().unwrap()
        );
        assert_eq!(
            restored.wins_for_day("grid", DAY).unwrap(),
            store.wins_for_day("grid", DAY).unwrap()
        );
        assert!(
            restored.audit().unwrap().ok(),
            "a restored backup must pass the audit"
        );

        // Taking a second backup to the same name must fail and leave the first intact.
        assert!(matches!(
            store.backup_to(&copy),
            Err(StoreError::Malformed(_))
        ));
        assert_eq!(
            GamesStore::open(&copy).unwrap().totals().unwrap(),
            store.totals().unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_schema_version_is_checked_on_open() {
        let dir = std::env::temp_dir().join(format!("games-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("games.db");
        {
            let store = GamesStore::open(&path).unwrap();
            store
                .lock()
                .unwrap()
                .execute(
                    "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
                    [],
                )
                .unwrap();
        }
        assert!(matches!(
            GamesStore::open(&path),
            Err(StoreError::Malformed(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
