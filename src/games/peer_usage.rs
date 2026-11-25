//! Durable accounting of the anchors a peer service has asked for, so a restart cannot reset a limit.
//!
//! The Records Service lets the Games Reward Service have at most N distinct roots anchored per UTC day
//! (each real anchor costs the owner wallet a fee). That limit used to live in memory. It now lives in the
//! Records database, in two tables that are always changed together, inside one transaction:
//!
//! ```text
//! peer_anchor_days   (day PK, used)                                  how many slots the day has consumed
//! peer_anchor_roots  (day, chain_id, root) PK, state, tx_hash,       one row per distinct root asked for that
//!                    attempts, last_error, created_at, updated_at    day, and where its submission stands
//! ```
//!
//! **A slot is consumed when a root is *reserved*, before anything is submitted.** So a crash at any point
//! can waste a slot but can never bypass the limit: the worst case is a day with fewer anchors than its
//! allowance, never more.
//!
//! ```text
//! reserve ──► reserved ──► submitting ──► submitted
//!                ▲              │
//!                └── definite failure (the signer ran and refused, or could not run)
//!                       an ambiguous failure (timeout, unreadable output) stays `submitting`
//! ```
//!
//! * `reserve_peer_anchor` takes a slot for a *new* root. A root already reserved today is `Existing`
//!   and takes none, so a retry, a duplicate request or a restart never counts twice.
//! * `begin_peer_submission` records an attempt **before** the signer is called. A crash during the call
//!   leaves `submitting`, with the attempt already counted.
//! * `finish_peer_submission` records the transaction, or the failure.
//!
//! **Time is never evidence.** The ledger does not refuse a repeated root (each anchor takes the next
//! sequence), so a second submission pays a second fee. A root in `submitting` (its first outcome is unknown)
//! or `submitted` (accepted, maybe still pending) is therefore **never submitted again because time has
//! passed**. `submitting` is `InDoubt` until reconciliation *proves* the first attempt never reached the node
//! (`mark_peer_not_submitted`, see `records.rs`) or an operator resolves it (`resolve_peer_root`);
//! `submitted` answers with its recorded transaction until the ledger shows it or an operator resolves it.
//!
//! Every function is one `BEGIN IMMEDIATE` transaction on a WAL database with `synchronous=FULL`, so it is
//! atomic across threads, processes and power loss.
//!
//! **Corruption fails closed.** The counter and the rows must agree (`used == number of rows`); a row must
//! decode to a known state, a 32-byte root and a well-formed transaction hash. Anything else is an error
//! and nothing is submitted: the state is never repaired, reset or guessed. An operator decides (see
//! `docs/runbooks/records-service.md` §5).

use super::fence::fence_bump;
use super::store::{GamesStore, StoreError};
use crate::ultragrid;
use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};

pub(super) const PEER_USAGE_SCHEMA: &str = "
CREATE TABLE peer_anchor_days (
    day TEXT PRIMARY KEY,
    used INTEGER NOT NULL CHECK (used >= 0)
);
CREATE TABLE peer_anchor_roots (
    day TEXT NOT NULL,
    chain_id INTEGER NOT NULL CHECK (chain_id > 0),
    root BLOB NOT NULL CHECK (length(root) = 32),
    state TEXT NOT NULL CHECK (state IN ('reserved','submitting','submitted')),
    tx_hash TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (day, chain_id, root)
);";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerState {
    Reserved,
    Submitting,
    Submitted,
}

impl PeerState {
    fn parse(value: &str) -> Option<PeerState> {
        match value {
            "reserved" => Some(PeerState::Reserved),
            "submitting" => Some(PeerState::Submitting),
            "submitted" => Some(PeerState::Submitted),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            PeerState::Reserved => "reserved",
            PeerState::Submitting => "submitting",
            PeerState::Submitted => "submitted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerRoot {
    pub day: String,
    pub chain_id: u32,
    pub root: [u8; 32],
    pub state: PeerState,
    pub tx_hash: Option<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub updated_at: u64,
}

/// What an operator has established about a root the service could not settle by itself.
#[derive(Debug, PartialEq, Eq)]
pub enum PeerResolution {
    /// The transaction is **verified** on the ledger: mined, signed by the owner, and the anchors index holds
    /// exactly this root on exactly this chain under this hash. Never used for a pending transaction.
    Landed { tx_hash: String },
    /// The operator names a transaction that is still **pending**. Its chain and root cannot be read until it is
    /// mined, so this is only a claim: the root stays in doubt (never resubmitted, never marked landed), and the
    /// service verifies the claim itself once the transaction is mined and indexed. See `records.rs`.
    PendingClaim { tx_hash: String },
    /// Nothing was submitted for it: the signer never ran, or the node never accepted anything.
    NotSubmitted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reservation {
    /// A new root took a slot.
    Granted(PeerRoot),
    /// The root was already reserved today. No slot was taken.
    Existing(PeerRoot),
    /// The day's allowance is used up and this root is not one of its roots.
    LimitReached { used: u32, limit: u32 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmitDecision {
    /// An attempt was recorded; the caller may now call the signer, then `finish_peer_submission`.
    Proceed { attempt: u32 },
    /// Accepted by the node earlier: answer with the recorded transaction. Never resubmitted automatically,
    /// however long ago, because only the ledger or an operator can say it did not land.
    Wait { tx_hash: String },
    /// A submission's outcome is unknown (a crash, a timeout, unreadable output). `begun_at` is when the
    /// attempt was recorded, which bounds when its transaction can have been signed.
    InDoubt { begun_at: u64 },
    /// The root has been attempted the most times it may be today.
    Exhausted { attempts: u32 },
}

fn corrupt(detail: impl Into<String>) -> StoreError {
    StoreError::Malformed(format!(
        "peer anchor usage is unusable and was not changed: {}",
        detail.into()
    ))
}

fn valid_tx(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn decode(row: &Row<'_>) -> Result<PeerRoot, StoreError> {
    let day: String = row.get(0)?;
    let chain_id: i64 = row.get(1)?;
    let root: Vec<u8> = row.get(2)?;
    let state: String = row.get(3)?;
    let tx_hash: Option<String> = row.get(4)?;
    let attempts: i64 = row.get(5)?;
    let last_error: Option<String> = row.get(6)?;
    let updated_at: i64 = row.get(7)?;
    let root: [u8; 32] = root
        .try_into()
        .map_err(|_| corrupt("a stored root is not 32 bytes"))?;
    let state = PeerState::parse(&state).ok_or_else(|| corrupt("a stored state is not known"))?;
    if !ultragrid::valid_date(&day) {
        return Err(corrupt("a stored day is not a date"));
    }
    if let Some(hash) = &tx_hash {
        if !valid_tx(hash) {
            return Err(corrupt("a stored transaction hash is malformed"));
        }
    }
    if state == PeerState::Submitted && tx_hash.is_none() {
        return Err(corrupt("a submitted root has no transaction hash"));
    }
    Ok(PeerRoot {
        day,
        chain_id: u32::try_from(chain_id).map_err(|_| corrupt("a stored chain id is out of range"))?,
        root,
        state,
        tx_hash,
        attempts: u32::try_from(attempts).map_err(|_| corrupt("a stored attempt count is out of range"))?,
        last_error,
        updated_at: u64::try_from(updated_at).map_err(|_| corrupt("a stored time is out of range"))?,
    })
}

const COLUMNS: &str = "day, chain_id, root, state, tx_hash, attempts, last_error, updated_at";

/// The counter and the rows of one day, checked against each other. Called inside a transaction.
fn checked_usage(tx: &rusqlite::Transaction<'_>, day: &str) -> Result<u32, StoreError> {
    let counter: Option<i64> = tx
        .query_row(
            "SELECT used FROM peer_anchor_days WHERE day = ?1",
            params![day],
            |row| row.get(0),
        )
        .optional()?;
    let rows: i64 = tx.query_row(
        "SELECT COUNT(*) FROM peer_anchor_roots WHERE day = ?1",
        params![day],
        |row| row.get(0),
    )?;
    let counter = counter.unwrap_or(0);
    if counter != rows {
        return Err(corrupt(format!(
            "the counter for {day} says {counter} but there are {rows} recorded roots"
        )));
    }
    // Every row of the day must decode, not only the one being asked about: a tampered row for another
    // root would otherwise go unnoticed until someone asked for it. A day is capped, so this is a handful
    // of rows.
    let mut statement = tx.prepare(&format!(
        "SELECT {COLUMNS} FROM peer_anchor_roots WHERE day = ?1"
    ))?;
    let decoded = statement.query_map(params![day], |row| Ok(decode(row)))?;
    for row in decoded {
        row??;
    }
    u32::try_from(counter).map_err(|_| corrupt("the counter is out of range"))
}

impl GamesStore {
    /// Take a slot for `root` on `day`, or find that it already has one. Atomic and durable.
    pub fn reserve_peer_anchor(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        limit: u32,
        now: u64,
    ) -> Result<Reservation, StoreError> {
        if !ultragrid::valid_date(day) || chain_id == 0 {
            return Err(StoreError::Malformed("a reservation needs a date and a chain".into()));
        }
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let used = checked_usage(&tx, day)?;
        let existing = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM peer_anchor_roots WHERE day = ?1 AND chain_id = ?2 AND root = ?3"),
                params![day, i64::from(chain_id), root.as_slice()],
                |row| Ok(decode(row)),
            )
            .optional()?;
        if let Some(existing) = existing {
            return Ok(Reservation::Existing(existing?));
        }
        if used >= limit {
            return Ok(Reservation::LimitReached { used, limit });
        }
        let now_i = i64::try_from(now).map_err(|_| StoreError::NumericRange)?;
        tx.execute(
            "INSERT INTO peer_anchor_roots (day, chain_id, root, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'reserved', 0, ?4, ?4)",
            params![day, i64::from(chain_id), root.as_slice(), now_i],
        )?;
        tx.execute(
            "INSERT INTO peer_anchor_days (day, used) VALUES (?1, 1)
             ON CONFLICT(day) DO UPDATE SET used = used + 1",
            params![day],
        )?;
        let granted = tx.query_row(
            &format!("SELECT {COLUMNS} FROM peer_anchor_roots WHERE day = ?1 AND chain_id = ?2 AND root = ?3"),
            params![day, i64::from(chain_id), root.as_slice()],
            |row| Ok(decode(row)),
        )??;
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(Reservation::Granted(granted))
    }

    /// Decide whether a reserved root may be submitted now, and if so record the attempt first.
    pub fn begin_peer_submission(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        max_attempts: u32,
        now: u64,
    ) -> Result<SubmitDecision, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        checked_usage(&tx, day)?;
        let current = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM peer_anchor_roots WHERE day = ?1 AND chain_id = ?2 AND root = ?3"),
                params![day, i64::from(chain_id), root.as_slice()],
                |row| Ok(decode(row)),
            )
            .optional()?
            .ok_or_else(|| corrupt("a submission was begun for a root that was never reserved"))??;
        match current.state {
            PeerState::Submitted => {
                return Ok(SubmitDecision::Wait {
                    tx_hash: current.tx_hash.unwrap_or_default(),
                });
            }
            PeerState::Submitting => {
                return Ok(SubmitDecision::InDoubt {
                    begun_at: current.updated_at,
                });
            }
            PeerState::Reserved => {}
        }
        if current.attempts >= max_attempts {
            return Ok(SubmitDecision::Exhausted {
                attempts: current.attempts,
            });
        }
        let attempt = current.attempts + 1;
        tx.execute(
            "UPDATE peer_anchor_roots SET state = 'submitting', tx_hash = NULL, attempts = ?4, updated_at = ?5
             WHERE day = ?1 AND chain_id = ?2 AND root = ?3",
            params![
                day,
                i64::from(chain_id),
                root.as_slice(),
                i64::from(attempt),
                i64::try_from(now).map_err(|_| StoreError::NumericRange)?
            ],
        )?;
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(SubmitDecision::Proceed { attempt })
    }

    /// Record how a submission ended. `Ok(tx)`: accepted. `Err((message, definite))`: it failed; a
    /// `definite` failure (the signer ran and refused, or never ran) returns the root to `reserved`, an
    /// ambiguous one (a timeout, unreadable output) leaves it `submitting` because it may have been accepted.
    pub fn finish_peer_submission(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        outcome: Result<&str, (&str, bool)>,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now_i = i64::try_from(now).map_err(|_| StoreError::NumericRange)?;
        let changed = match outcome {
            Ok(hash) => {
                if !valid_tx(hash) {
                    return Err(StoreError::Malformed("an anchor transaction hash is malformed".into()));
                }
                tx.execute(
                    "UPDATE peer_anchor_roots SET state = 'submitted', tx_hash = ?4, last_error = NULL, updated_at = ?5
                     WHERE day = ?1 AND chain_id = ?2 AND root = ?3",
                    params![day, i64::from(chain_id), root.as_slice(), hash, now_i],
                )?
            }
            Err((message, definite)) => {
                let state = if definite { "reserved" } else { "submitting" };
                let message: String = message.chars().take(200).collect();
                tx.execute(
                    "UPDATE peer_anchor_roots SET state = ?4, last_error = ?5
                     WHERE day = ?1 AND chain_id = ?2 AND root = ?3",
                    params![day, i64::from(chain_id), root.as_slice(), state, message],
                )?
            }
        };
        if changed != 1 {
            return Err(corrupt("a finished submission has no reserved row"));
        }
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(())
    }

    /// Return a `submitting` root to `reserved` because reconciliation **proved** its attempt never reached
    /// the node. Only if the row is still exactly the attempt that was examined (`begun_at`), so a concurrent
    /// change is never overwritten. The attempt stays counted.
    pub fn mark_peer_not_submitted(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        begun_at: u64,
        note: &str,
    ) -> Result<bool, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        checked_usage(&tx, day)?;
        let changed = tx.execute(
            "UPDATE peer_anchor_roots SET state = 'reserved', tx_hash = NULL, last_error = ?5
             WHERE day = ?1 AND chain_id = ?2 AND root = ?3 AND state = 'submitting' AND updated_at = ?4",
            params![
                day,
                i64::from(chain_id),
                root.as_slice(),
                i64::try_from(begun_at).map_err(|_| StoreError::NumericRange)?,
                note.chars().take(200).collect::<String>()
            ],
        )?;
        if changed == 1 {
            fence_bump(&tx)?;
        }
        tx.commit()?;
        drop(guard);
        if changed == 1 {
            self.after_accounting_commit();
        }
        Ok(changed == 1)
    }

    /// An operator's decision about a root whose outcome the service could not establish. `Landed` records
    /// the transaction the operator found on the ledger; `NotSubmitted` returns the root to `reserved` so it
    /// may be submitted again. The attempt count is kept either way, and the note is kept as the reason.
    pub fn resolve_peer_root(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        resolution: &PeerResolution,
        note: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        checked_usage(&tx, day)?;
        let note = format!("operator: {note}").chars().take(200).collect::<String>();
        let now_i = i64::try_from(now).map_err(|_| StoreError::NumericRange)?;
        let changed = match resolution {
            PeerResolution::Landed { tx_hash } | PeerResolution::PendingClaim { tx_hash } => {
                if !valid_tx(tx_hash) {
                    return Err(StoreError::Malformed("a transaction hash must be 64 hex characters".into()));
                }
                let hash = tx_hash.to_ascii_lowercase();
                // One transaction anchors one root: a hash already held by another record cannot be named again.
                let taken: i64 = tx.query_row(
                    "SELECT (SELECT COUNT(*) FROM peer_anchor_roots WHERE lower(tx_hash) = ?1
                                AND NOT (day = ?2 AND chain_id = ?3 AND root = ?4))
                          + (SELECT COUNT(*) FROM registry_anchor_attempts WHERE lower(tx_hash) = ?1)",
                    params![hash, day, i64::from(chain_id), root.as_slice()],
                    |row| row.get(0),
                )?;
                if taken > 0 {
                    return Err(StoreError::Malformed(
                        "that transaction is already recorded for another anchor; one transaction anchors one root".into(),
                    ));
                }
                let state = if matches!(resolution, PeerResolution::Landed { .. }) { "submitted" } else { "submitting" };
                tx.execute(
                    "UPDATE peer_anchor_roots SET state = ?7, tx_hash = ?4, last_error = ?5, updated_at = ?6
                     WHERE day = ?1 AND chain_id = ?2 AND root = ?3",
                    params![day, i64::from(chain_id), root.as_slice(), hash, note, now_i, state],
                )?
            }
            PeerResolution::NotSubmitted => tx.execute(
                "UPDATE peer_anchor_roots SET state = 'reserved', tx_hash = NULL, last_error = ?4, updated_at = ?5
                 WHERE day = ?1 AND chain_id = ?2 AND root = ?3",
                params![day, i64::from(chain_id), root.as_slice(), note, now_i],
            )?,
        };
        if changed != 1 {
            return Err(StoreError::Malformed("there is no such root recorded for that day".into()));
        }
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(())
    }

    /// Every record of this root on this chain on any day, newest first. A root's history does not end at
    /// midnight: the same root asked for again later is the same anchor.
    pub fn peer_root_history(&self, chain_id: u32, root: &[u8; 32]) -> Result<Vec<PeerRoot>, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let mut statement = tx.prepare(&format!(
            "SELECT {COLUMNS} FROM peer_anchor_roots WHERE chain_id = ?1 AND root = ?2 ORDER BY day DESC"
        ))?;
        let rows = statement.query_map(params![i64::from(chain_id), root.as_slice()], |row| Ok(decode(row)))?;
        let mut history = Vec::new();
        for row in rows {
            history.push(row??);
        }
        Ok(history)
    }

    /// Every (chain, root) recorded on any of `days`, after checking each day.
    pub fn peer_known_roots(
        &self,
        days: &[&str],
    ) -> Result<std::collections::HashSet<(u32, [u8; 32])>, StoreError> {
        let mut known = std::collections::HashSet::new();
        for day in days {
            for root in self.peer_anchor_roots(day)? {
                known.insert((root.chain_id, root.root));
            }
        }
        Ok(known)
    }

    /// Record an anchor that the ledger shows and this database does not know of, as a used slot. This is
    /// how a database restored from an older backup is brought back in line with what the chain holds.
    /// Returns whether a row was added (false: the root was already recorded).
    pub fn adopt_ledger_anchor(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        tx_hash: &str,
        now: u64,
    ) -> Result<bool, StoreError> {
        if !valid_tx(tx_hash) {
            return Err(StoreError::Malformed("a ledger anchor has no usable transaction hash".into()));
        }
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        checked_usage(&tx, day)?;
        let now_i = i64::try_from(now).map_err(|_| StoreError::NumericRange)?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO peer_anchor_roots (day, chain_id, root, state, tx_hash, attempts, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'submitted', ?4, 1, 'adopted from the ledger', ?5, ?5)",
            params![day, i64::from(chain_id), root.as_slice(), tx_hash.to_ascii_lowercase(), now_i],
        )?;
        if inserted == 1 {
            tx.execute(
                "INSERT INTO peer_anchor_days (day, used) VALUES (?1, 1)
                 ON CONFLICT(day) DO UPDATE SET used = used + 1",
                params![day],
            )?;
            fence_bump(&tx)?;
        }
        tx.commit()?;
        drop(guard);
        if inserted == 1 {
            self.after_accounting_commit();
        }
        Ok(inserted == 1)
    }

    /// While set, no peer anchor and no registry anchor is signed. Set by the operator after a restore and
    /// cleared only by a successful reconciliation (`ultranet-records peer-reconcile`).
    pub fn peer_hold(&self) -> Result<Option<String>, StoreError> {
        let guard = self.lock()?;
        Ok(guard
            .query_row("SELECT value FROM meta WHERE key = 'peer_hold'", [], |row| row.get::<_, String>(0))
            .optional()?)
    }

    pub fn set_peer_hold(&self, note: &str, now: u64) -> Result<(), StoreError> {
        let guard = self.lock()?;
        let value = format!("{now}: {note}").chars().take(300).collect::<String>();
        guard.execute(
            "INSERT INTO meta (key, value) VALUES ('peer_hold', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![value],
        )?;
        Ok(())
    }

    pub fn clear_peer_hold(&self) -> Result<(), StoreError> {
        let guard = self.lock()?;
        guard.execute("DELETE FROM meta WHERE key = 'peer_hold'", [])?;
        Ok(())
    }

    /// How many slots `day` has used, after checking the counter against the rows.
    pub fn peer_anchor_usage(&self, day: &str) -> Result<u32, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Deferred)?;
        checked_usage(&tx, day)
    }

    /// Every root recorded for `day`, for an operator.
    pub fn peer_anchor_roots(&self, day: &str) -> Result<Vec<PeerRoot>, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Deferred)?;
        checked_usage(&tx, day)?;
        let mut statement = tx.prepare(&format!(
            "SELECT {COLUMNS} FROM peer_anchor_roots WHERE day = ?1 ORDER BY created_at, chain_id, root"
        ))?;
        let rows = statement.query_map(params![day], |row| Ok(decode(row)))?;
        let mut roots = Vec::new();
        for row in rows {
            roots.push(row??);
        }
        Ok(roots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::Arc, thread};

    const DAY: &str = "2026-11-16";
    const NEXT: &str = "2026-11-17";
    const NOW: u64 = 1_794_800_000;

    fn db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("peer-usage-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("records.db")
    }

    fn root(n: u8) -> [u8; 32] {
        [n; 32]
    }

    fn reserve(store: &GamesStore, day: &str, n: u8, limit: u32) -> Reservation {
        store.reserve_peer_anchor(day, 1, &root(n), limit, NOW).unwrap()
    }

    /// A second, independent connection to the same file, for tampering and for concurrency.
    fn raw(path: &PathBuf) -> rusqlite::Connection {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection.execute_batch("PRAGMA busy_timeout = 5000;").unwrap();
        connection
    }

    #[test]
    fn eight_distinct_roots_are_accepted_and_the_ninth_is_not() {
        let store = GamesStore::open(&db("eight")).unwrap();
        for n in 1..=8 {
            assert!(matches!(reserve(&store, DAY, n, 8), Reservation::Granted(_)), "root {n}");
        }
        assert_eq!(reserve(&store, DAY, 9, 8), Reservation::LimitReached { used: 8, limit: 8 });
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 8);
        assert_eq!(store.peer_anchor_roots(DAY).unwrap().len(), 8, "the refused root left no row");
    }

    #[test]
    fn repeating_a_root_takes_no_slot_even_when_the_day_is_full() {
        let store = GamesStore::open(&db("repeat")).unwrap();
        for n in 1..=8 {
            reserve(&store, DAY, n, 8);
        }
        for _ in 0..5 {
            assert!(matches!(reserve(&store, DAY, 3, 8), Reservation::Existing(_)));
        }
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 8);
        // The same bytes on another chain are another root: they need a slot of their own.
        assert!(matches!(
            store.reserve_peer_anchor(DAY, 2, &root(3), 8, NOW).unwrap(),
            Reservation::LimitReached { .. }
        ));
    }

    #[test]
    fn a_restart_does_not_reset_the_allowance() {
        let path = db("restart");
        {
            let store = GamesStore::open(&path).unwrap();
            for n in 1..=8 {
                reserve(&store, DAY, n, 8);
            }
        } // the process "exits": every handle is dropped
        let reopened = GamesStore::open(&path).unwrap();
        assert_eq!(reopened.peer_anchor_usage(DAY).unwrap(), 8);
        assert!(matches!(reserve(&reopened, DAY, 9, 8), Reservation::LimitReached { .. }));
        assert!(matches!(reserve(&reopened, DAY, 2, 8), Reservation::Existing(_)));
        // And again, and again: a crash-loop cannot wear the limit down.
        for _ in 0..3 {
            let again = GamesStore::open(&path).unwrap();
            assert!(matches!(reserve(&again, DAY, 9, 8), Reservation::LimitReached { .. }));
        }
    }

    #[test]
    fn concurrent_connections_cannot_exceed_the_limit() {
        // Separate connections, as separate processes would have: SQLite's write lock is the arbiter.
        let path = Arc::new(db("concurrent"));
        GamesStore::open(&path).unwrap();
        let handles: Vec<_> = (1..=24u8)
            .map(|n| {
                let path = path.clone();
                thread::spawn(move || {
                    let store = GamesStore::open(&path).unwrap();
                    store.reserve_peer_anchor(DAY, 1, &[n; 32], 8, NOW).unwrap()
                })
            })
            .collect();
        let results: Vec<Reservation> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let granted = results.iter().filter(|r| matches!(r, Reservation::Granted(_))).count();
        let refused = results.iter().filter(|r| matches!(r, Reservation::LimitReached { .. })).count();
        assert_eq!((granted, refused), (8, 16), "exactly the allowance, never more");
        let store = GamesStore::open(&path).unwrap();
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 8);
        assert_eq!(store.peer_anchor_roots(DAY).unwrap().len(), 8);
    }

    #[test]
    fn concurrent_duplicates_take_exactly_one_slot() {
        let path = Arc::new(db("dupes"));
        GamesStore::open(&path).unwrap();
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let path = path.clone();
                thread::spawn(move || {
                    GamesStore::open(&path)
                        .unwrap()
                        .reserve_peer_anchor(DAY, 1, &[7; 32], 8, NOW)
                        .unwrap()
                })
            })
            .collect();
        let results: Vec<Reservation> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| matches!(r, Reservation::Granted(_))).count(), 1);
        assert_eq!(results.iter().filter(|r| matches!(r, Reservation::Existing(_))).count(), 15);
        assert_eq!(GamesStore::open(&path).unwrap().peer_anchor_usage(DAY).unwrap(), 1);
    }

    #[test]
    fn utc_midnight_starts_a_new_allowance_and_leaves_the_old_one_alone() {
        let store = GamesStore::open(&db("midnight")).unwrap();
        for n in 1..=8 {
            reserve(&store, DAY, n, 8);
        }
        assert!(matches!(reserve(&store, DAY, 9, 8), Reservation::LimitReached { .. }));
        for n in 1..=8 {
            assert!(matches!(reserve(&store, NEXT, n, 8), Reservation::Granted(_)), "next day, root {n}");
        }
        assert!(matches!(reserve(&store, NEXT, 9, 8), Reservation::LimitReached { .. }));
        assert_eq!((store.peer_anchor_usage(DAY).unwrap(), store.peer_anchor_usage(NEXT).unwrap()), (8, 8));
    }

    #[test]
    fn a_crash_after_reserving_keeps_the_slot_and_a_crash_during_submission_keeps_the_attempt() {
        let path = db("crash");
        {
            let store = GamesStore::open(&path).unwrap();
            // Crash point 1: reserved, never submitted.
            reserve(&store, DAY, 1, 8);
            // Crash point 2: the attempt was recorded, the signer was called, the process died.
            reserve(&store, DAY, 2, 8);
            assert_eq!(
                store.begin_peer_submission(DAY, 1, &root(2), 3, NOW).unwrap(),
                SubmitDecision::Proceed { attempt: 1 }
            );
        }
        let store = GamesStore::open(&path).unwrap();
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 2, "both slots survive the crash");
        let roots = store.peer_anchor_roots(DAY).unwrap();
        let by = |n: u8| roots.iter().find(|r| r.root == root(n)).unwrap().clone();
        assert_eq!((by(1).state, by(1).attempts), (PeerState::Reserved, 0));
        assert_eq!((by(2).state, by(2).attempts), (PeerState::Submitting, 1));
        // The in-doubt submission is NEVER repeated because time has passed: an hour, a day, a year.
        for later in [NOW + 3_599, NOW + 3_600, NOW + 86_400, NOW + 365 * 86_400] {
            assert_eq!(
                store.begin_peer_submission(DAY, 1, &root(2), 3, later).unwrap(),
                SubmitDecision::InDoubt { begun_at: NOW },
                "at +{}s", later - NOW
            );
        }
        // Only reconciliation that examined exactly this attempt can return it to `reserved`...
        assert!(!store.mark_peer_not_submitted(DAY, 1, &root(2), NOW + 1, "stale").unwrap());
        assert!(store.mark_peer_not_submitted(DAY, 1, &root(2), NOW, "proved").unwrap());
        // ...and then a retry is allowed, and counted.
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(2), 3, NOW + 10).unwrap(),
            SubmitDecision::Proceed { attempt: 2 }
        );
        // The slot count never moved: surviving a crash costs attempts, not allowance.
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 2);
    }

    #[test]
    fn a_root_that_keeps_failing_is_capped_and_never_takes_another_slot() {
        let store = GamesStore::open(&db("failing")).unwrap();
        reserve(&store, DAY, 1, 8);
        for attempt in 1..=3 {
            assert_eq!(
                store.begin_peer_submission(DAY, 1, &root(1), 3, NOW).unwrap(),
                SubmitDecision::Proceed { attempt }
            );
            // A definite failure returns it to `reserved`, so the next attempt is allowed at once.
            store
                .finish_peer_submission(DAY, 1, &root(1), Err(("wallet empty", true)), NOW)
                .unwrap();
        }
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(1), 3, NOW).unwrap(),
            SubmitDecision::Exhausted { attempts: 3 }
        );
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 1, "three failures, still one slot");
        // An ambiguous failure stays in doubt; a success is remembered and answered, not repeated.
        reserve(&store, DAY, 2, 8);
        store.begin_peer_submission(DAY, 1, &root(2), 3, NOW).unwrap();
        store.finish_peer_submission(DAY, 1, &root(2), Err(("timed out", false)), NOW).unwrap();
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(2), 3, NOW + 30 * 86_400).unwrap(),
            SubmitDecision::InDoubt { begun_at: NOW },
            "an ambiguous failure stays in doubt however long ago it was"
        );
        reserve(&store, DAY, 3, 8);
        store.begin_peer_submission(DAY, 1, &root(3), 3, NOW).unwrap();
        let tx = "ab".repeat(32);
        store.finish_peer_submission(DAY, 1, &root(3), Ok(&tx), NOW).unwrap();
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(3), 3, NOW + 30 * 86_400).unwrap(),
            SubmitDecision::Wait { tx_hash: tx },
            "a submitted root is answered, not submitted again, however long ago it was"
        );
    }

    /// Run `tamper` on the file, then show that every operation refuses and the data is left as found.
    fn assert_fails_closed(name: &str, tamper: &str, check_constraints: bool) {
        let path = db(name);
        {
            let store = GamesStore::open(&path).unwrap();
            for n in 1..=3 {
                reserve(&store, DAY, n, 8);
            }
        }
        let connection = raw(&path);
        if !check_constraints {
            connection.execute_batch("PRAGMA ignore_check_constraints = ON;").unwrap();
        }
        connection.execute_batch(tamper).unwrap();
        drop(connection);
        let store = GamesStore::open(&path).unwrap();
        assert!(store.reserve_peer_anchor(DAY, 1, &root(9), 8, NOW).is_err(), "{name}: a new root");
        assert!(store.reserve_peer_anchor(DAY, 1, &root(1), 8, NOW).is_err(), "{name}: an existing root");
        assert!(store.begin_peer_submission(DAY, 1, &root(1), 3, NOW).is_err(), "{name}: a submission");
        assert!(store.peer_anchor_usage(DAY).is_err(), "{name}: usage");
        // Nothing was granted, repaired or reset by trying.
        let after = raw(&path);
        // (In the "dropped" case there is no table left to count.)
        if let Ok(rows) = after.query_row("SELECT COUNT(*) FROM peer_anchor_roots", [], |r| r.get::<_, i64>(0)) {
            assert!(rows <= 3, "{name}: no row was added");
        }
    }

    #[test]
    fn corrupted_or_tampered_state_fails_closed_and_is_never_repaired_or_reset() {
        // The counter was reset to zero: the rows say 3, the counter says 0.
        assert_fails_closed("zeroed", "UPDATE peer_anchor_days SET used = 0;", true);
        // The counter was inflated.
        assert_fails_closed("inflated", "UPDATE peer_anchor_days SET used = 99;", true);
        // Rows were deleted while the counter stayed.
        assert_fails_closed("rows-gone", "DELETE FROM peer_anchor_roots;", true);
        // The counter row was deleted while the rows stayed.
        assert_fails_closed("counter-gone", "DELETE FROM peer_anchor_days;", true);
        // A state nobody wrote.
        assert_fails_closed("state", "UPDATE peer_anchor_roots SET state = 'weird' WHERE rowid = 1;", false);
        // A root of the wrong length.
        assert_fails_closed("root-length", "UPDATE peer_anchor_roots SET root = x'00' WHERE rowid = 1;", false);
        // A malformed transaction hash.
        assert_fails_closed("tx-hash", "UPDATE peer_anchor_roots SET tx_hash = 'not-a-hash' WHERE rowid = 1;", true);
        // A submitted root without a transaction.
        assert_fails_closed("no-tx", "UPDATE peer_anchor_roots SET state = 'submitted', tx_hash = NULL WHERE rowid = 1;", true);
        // A day that is not a date.
        assert_fails_closed("day", "UPDATE peer_anchor_roots SET day = 'someday' WHERE rowid = 1; UPDATE peer_anchor_days SET day = 'someday';", true);
        // The table itself is gone.
        assert_fails_closed("dropped", "DROP TABLE peer_anchor_roots;", true);
    }

    #[test]
    fn a_zeroed_counter_is_reported_not_silently_accepted() {
        let path = db("zeroed-message");
        let store = GamesStore::open(&path).unwrap();
        for n in 1..=8 {
            reserve(&store, DAY, n, 8);
        }
        raw(&path).execute_batch("UPDATE peer_anchor_days SET used = 0;").unwrap();
        let error = store.reserve_peer_anchor(DAY, 1, &root(9), 8, NOW).unwrap_err();
        assert!(error.to_string().contains("unusable and was not changed"), "{error}");
        assert!(error.to_string().contains("says 0 but there are 8"), "{error}");
        // The tampered counter is still exactly as it was left: it was not "healed" into a fresh allowance.
        let used: i64 = raw(&path).query_row("SELECT used FROM peer_anchor_days", [], |r| r.get(0)).unwrap();
        assert_eq!(used, 0);
    }

    #[test]
    fn a_database_from_before_the_accounting_upgrades_in_place_and_keeps_its_data() {
        let path = db("upgrade");
        {
            let store = GamesStore::open(&path).unwrap();
            store
                .register_game("tetris", "Tetris", &"7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e", NOW)
                .unwrap();
        }
        raw(&path)
            .execute_batch(
                "DROP TABLE game_pool_payouts; DROP TABLE pool_state; DROP TABLE game_reward_policies; DROP TABLE game_pool_ledger; DROP TABLE game_pool_withdrawals; DROP TABLE game_pool_credits; DROP TABLE game_pool_deposits; DROP TABLE game_pools; DROP TABLE platform_game_audit; DROP TABLE platform_games; DROP TABLE auth_sessions; DROP TABLE auth_challenges; DROP TABLE registry_anchor_attempts; DROP TABLE peer_anchor_roots; DROP TABLE peer_anchor_days;
                 UPDATE meta SET value = '2' WHERE key = 'schema_version';",
            )
            .unwrap();
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(upgraded.games().unwrap().len(), 1, "existing data is untouched");
        assert!(matches!(reserve(&upgraded, DAY, 1, 8), Reservation::Granted(_)));
        let version: String = raw(&path)
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "8", "2 went through 3, 4, 5, 6 and 7 and arrived at 8");
        drop(upgraded);
        assert!(GamesStore::open(&path).is_ok(), "opening again is a no-op");
        assert_eq!(GamesStore::open(&path).unwrap().peer_anchor_usage(DAY).unwrap(), 1);
    }

    #[test]
    fn an_operator_can_settle_a_root_either_way_and_the_attempt_count_is_kept() {
        let store = GamesStore::open(&db("resolve")).unwrap();
        reserve(&store, DAY, 1, 8);
        reserve(&store, DAY, 2, 8);
        for n in [1, 2] {
            store.begin_peer_submission(DAY, 1, &root(n), 3, NOW).unwrap();
        }
        let tx = "cd".repeat(32);
        store
            .resolve_peer_root(DAY, 1, &root(1), &PeerResolution::Landed { tx_hash: tx.clone() }, "found on the ledger", NOW)
            .unwrap();
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(1), 3, NOW + 1).unwrap(),
            SubmitDecision::Wait { tx_hash: tx }
        );
        store
            .resolve_peer_root(DAY, 1, &root(2), &PeerResolution::NotSubmitted, "node never saw it", NOW)
            .unwrap();
        assert_eq!(
            store.begin_peer_submission(DAY, 1, &root(2), 3, NOW + 1).unwrap(),
            SubmitDecision::Proceed { attempt: 2 },
            "the first attempt is still counted"
        );
        let row = store.peer_anchor_roots(DAY).unwrap().into_iter().find(|r| r.root == root(1)).unwrap();
        assert!(row.last_error.unwrap().starts_with("operator: found on the ledger"));
        // Unknown roots and malformed hashes are refused, not invented.
        assert!(store.resolve_peer_root(DAY, 1, &root(9), &PeerResolution::NotSubmitted, "x", NOW).is_err());
        assert!(store
            .resolve_peer_root(DAY, 1, &root(1), &PeerResolution::Landed { tx_hash: "nope".into() }, "x", NOW)
            .is_err());
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 2, "settling a root never changes the slot count");
    }

    #[test]
    fn adopting_a_ledger_anchor_restores_the_slot_it_used_and_does_it_once() {
        let store = GamesStore::open(&db("adopt")).unwrap();
        reserve(&store, DAY, 1, 8);
        let tx = "ef".repeat(32);
        for n in 2..=8u8 {
            assert!(store.adopt_ledger_anchor(DAY, 1, &root(n), &tx, NOW).unwrap(), "root {n}");
        }
        assert!(!store.adopt_ledger_anchor(DAY, 1, &root(2), &tx, NOW).unwrap(), "adopting twice adds nothing");
        assert_eq!(store.peer_anchor_usage(DAY).unwrap(), 8);
        // The allowance now reflects the ledger: the day is full.
        assert!(matches!(reserve(&store, DAY, 9, 8), Reservation::LimitReached { .. }));
        let adopted = store.peer_anchor_roots(DAY).unwrap().into_iter().find(|r| r.root == root(5)).unwrap();
        assert_eq!((adopted.state, adopted.tx_hash.as_deref(), adopted.attempts), (PeerState::Submitted, Some(tx.as_str()), 1));
        assert!(store.adopt_ledger_anchor(DAY, 1, &root(30), "short", NOW).is_err());
    }

    #[test]
    fn the_hold_is_durable_and_only_clearing_it_releases_it() {
        let path = db("hold");
        let store = GamesStore::open(&path).unwrap();
        assert_eq!(store.peer_hold().unwrap(), None);
        store.set_peer_hold("restored from backup 2026-11-10", NOW).unwrap();
        drop(store);
        let reopened = GamesStore::open(&path).unwrap();
        assert!(reopened.peer_hold().unwrap().unwrap().contains("restored from backup"));
        reopened.clear_peer_hold().unwrap();
        assert_eq!(reopened.peer_hold().unwrap(), None);
    }

    #[test]
    fn a_schema_7_database_gains_the_pool_payout_table_and_keeps_everything_else() {
        let path = db("upgrade7");
        {
            let store = GamesStore::open(&path).unwrap();
            reserve(&store, DAY, 1, 8);
            store.set_peer_hold("kept across the upgrade", NOW).unwrap();
        }
        raw(&path)
            .execute_batch("DROP TABLE game_pool_payouts; UPDATE meta SET value = '7' WHERE key = 'schema_version';")
            .unwrap();
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(upgraded.peer_anchor_usage(DAY).unwrap(), 1, "peer usage is untouched");
        assert!(upgraded.peer_hold().unwrap().is_some(), "the hold is untouched");
        assert!(upgraded.pool_next_open_payout().unwrap().is_none(), "the new table is there, and empty");
        let version: String = raw(&path)
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "8");
        drop(upgraded);
        assert!(GamesStore::open(&path).is_ok(), "opening again is a no-op");
    }

    #[test]
    fn a_schema_5_database_gains_the_catalog_tables_and_keeps_everything_else() {
        let path = db("upgrade5");
        {
            let store = GamesStore::open(&path).unwrap();
            reserve(&store, DAY, 1, 8);
            store.set_peer_hold("kept across the upgrade", NOW).unwrap();
        }
        raw(&path)
            .execute_batch("DROP TABLE game_pool_payouts; DROP TABLE pool_state; DROP TABLE game_reward_policies; DROP TABLE game_pool_ledger; DROP TABLE game_pool_withdrawals; DROP TABLE game_pool_credits; DROP TABLE game_pool_deposits; DROP TABLE game_pools; DROP TABLE platform_game_audit; DROP TABLE platform_games; UPDATE meta SET value = '5' WHERE key = 'schema_version';")
            .unwrap();
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(upgraded.peer_anchor_usage(DAY).unwrap(), 1, "peer usage is untouched");
        assert!(upgraded.peer_hold().unwrap().is_some(), "the hold is untouched");
        assert!(upgraded.catalog_games(None).unwrap().is_empty(), "the new table is there, and empty");
        let version: String = raw(&path)
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "8");
        drop(upgraded);
        assert!(GamesStore::open(&path).is_ok(), "opening again is a no-op");
    }

    #[test]
    fn a_schema_4_database_gains_the_sign_in_tables_and_keeps_everything_else() {
        let path = db("upgrade4");
        {
            let store = GamesStore::open(&path).unwrap();
            reserve(&store, DAY, 1, 8);
            store.set_peer_hold("kept across the upgrade", NOW).unwrap();
        }
        raw(&path)
            .execute_batch("DROP TABLE game_pool_payouts; DROP TABLE pool_state; DROP TABLE game_reward_policies; DROP TABLE game_pool_ledger; DROP TABLE game_pool_withdrawals; DROP TABLE game_pool_credits; DROP TABLE game_pool_deposits; DROP TABLE game_pools; DROP TABLE platform_game_audit; DROP TABLE platform_games; DROP TABLE auth_sessions; DROP TABLE auth_challenges; UPDATE meta SET value = '4' WHERE key = 'schema_version';")
            .unwrap();
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(upgraded.peer_anchor_usage(DAY).unwrap(), 1, "peer usage is untouched");
        assert!(upgraded.peer_hold().unwrap().is_some(), "the hold is untouched");
        let config = crate::games::auth::AuthConfig::default();
        let id = "ab".repeat(32);
        let address = "cd".repeat(32);
        upgraded.issue_challenge(&id, &address, "play", NOW, &config).unwrap();
        let version: String = raw(&path)
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "8");
        drop(upgraded);
        assert!(GamesStore::open(&path).is_ok(), "opening again is a no-op");
    }

    #[test]
    fn a_schema_3_database_gains_the_registry_anchor_table_and_keeps_everything_else() {
        let path = db("upgrade3");
        {
            let store = GamesStore::open(&path).unwrap();
            reserve(&store, DAY, 1, 8);
            store.set_peer_hold("kept across the upgrade", NOW).unwrap();
        }
        raw(&path)
            .execute_batch("DROP TABLE game_pool_payouts; DROP TABLE pool_state; DROP TABLE game_reward_policies; DROP TABLE game_pool_ledger; DROP TABLE game_pool_withdrawals; DROP TABLE game_pool_credits; DROP TABLE game_pool_deposits; DROP TABLE game_pools; DROP TABLE platform_game_audit; DROP TABLE platform_games; DROP TABLE auth_sessions; DROP TABLE auth_challenges; DROP TABLE registry_anchor_attempts; UPDATE meta SET value = '3' WHERE key = 'schema_version';")
            .unwrap();
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(upgraded.peer_anchor_usage(DAY).unwrap(), 1, "peer usage is untouched");
        assert!(upgraded.peer_hold().unwrap().is_some(), "the hold is untouched");
        let root = [9u8; 32];
        let row = upgraded.ensure_registry_anchor(DAY, 3, &root, NOW).unwrap();
        assert_eq!(row.state, PeerState::Reserved);
        let version: String = raw(&path)
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "8");
        drop(upgraded);
        assert!(GamesStore::open(&path).is_ok(), "opening again is a no-op");
    }

    #[test]
    fn a_pending_claim_never_becomes_submitted_by_itself_and_clearing_it_forgets_the_hash() {
        let store = GamesStore::open(&db("claim")).unwrap();
        reserve(&store, DAY, 1, 8);
        store.begin_peer_submission(DAY, 1, &root(1), 3, NOW).unwrap();
        let tx = "ab".repeat(32);
        store.resolve_peer_root(DAY, 1, &root(1), &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "pending", NOW).unwrap();
        let row = store.peer_anchor_roots(DAY).unwrap().remove(0);
        assert_eq!((row.state, row.tx_hash.as_deref()), (PeerState::Submitting, Some(tx.as_str())), "a claim is not a landing");
        assert!(matches!(store.begin_peer_submission(DAY, 1, &root(1), 3, NOW + 99_999).unwrap(), SubmitDecision::InDoubt { .. }));
        // Clearing it returns the root to reserved and FORGETS the hash: otherwise a later ambiguity would read the
        // stale hash as a claim.
        store.resolve_peer_root(DAY, 1, &root(1), &PeerResolution::NotSubmitted, "it was something else", NOW).unwrap();
        let row = store.peer_anchor_roots(DAY).unwrap().remove(0);
        assert_eq!((row.state, row.tx_hash), (PeerState::Reserved, None));
        // A new attempt starts clean as well.
        store.begin_peer_submission(DAY, 1, &root(1), 3, NOW + 1).unwrap();
        store.finish_peer_submission(DAY, 1, &root(1), Err(("timed out", false)), NOW + 1).unwrap();
        assert_eq!(store.peer_anchor_roots(DAY).unwrap().remove(0).tx_hash, None);
        // The same hash cannot be claimed for a second root.
        reserve(&store, DAY, 2, 8);
        store.resolve_peer_root(DAY, 1, &root(1), &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "again", NOW).unwrap();
        assert!(store.resolve_peer_root(DAY, 1, &root(2), &PeerResolution::PendingClaim { tx_hash: tx }, "twice", NOW).is_err());
    }

    #[test]
    fn a_new_attempt_never_inherits_a_stale_transaction_hash() {
        let path = db("stale-hash");
        let store = GamesStore::open(&path).unwrap();
        reserve(&store, DAY, 1, 8);
        // A reserved row that somehow carries a hash (an older build, a manual edit): the attempt must start clean.
        raw(&path)
            .execute_batch(&format!("UPDATE peer_anchor_roots SET tx_hash = '{}' WHERE state = 'reserved';", "ab".repeat(32)))
            .unwrap();
        assert_eq!(store.begin_peer_submission(DAY, 1, &root(1), 3, NOW).unwrap(), SubmitDecision::Proceed { attempt: 1 });
        assert_eq!(store.peer_anchor_roots(DAY).unwrap().remove(0).tx_hash, None, "the stale hash is gone");
        // Otherwise an ambiguous failure would later read it as an operator's claim.
        store.finish_peer_submission(DAY, 1, &root(1), Err(("timed out", false)), NOW).unwrap();
        assert_eq!(store.peer_anchor_roots(DAY).unwrap().remove(0).tx_hash, None);
    }
}
