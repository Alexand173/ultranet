//! The registry's daily anchor, under the same fail-closed rules as a peer's.
//!
//! One row per registry day records where its anchor stands, in the states of `peer_usage.rs`:
//!
//! ```text
//! reserved ──► submitting ──► submitted
//!    ▲              │
//!    └── only "the signer could not be run" is a definite failure; every other failure leaves `submitting`
//! ```
//!
//! * `begin_registry_submission` records the attempt **before** the signer is called.
//! * A `submitted` anchor is **never** submitted again automatically, whatever time has passed.
//! * A `submitting` anchor is in doubt until reconciliation proves the attempt never reached the node
//!   (`mark_registry_not_submitted`) or an operator resolves it (`resolve_registry_anchor`).
//! * A day's root and chain are fixed when its row is made; a row that disagrees with the day it is asked
//!   about is an error, never an overwrite.
//!
//! There is no daily cap: a registry day is one anchor. Every change bumps the fence (`fence.rs`).

use super::fence::fence_bump;
use super::peer_usage::{PeerResolution, PeerState, SubmitDecision};
use super::store::{GamesStore, StoreError};
use crate::ultragrid;
use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};

pub(super) const REGISTRY_ANCHOR_SCHEMA: &str = "
CREATE TABLE registry_anchor_attempts (
    day TEXT PRIMARY KEY,
    chain_id INTEGER NOT NULL CHECK (chain_id > 0),
    root BLOB NOT NULL CHECK (length(root) = 32),
    state TEXT NOT NULL CHECK (state IN ('reserved','submitting','submitted')),
    tx_hash TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryAnchor {
    pub day: String,
    pub chain_id: u32,
    pub root: [u8; 32],
    pub state: PeerState,
    pub tx_hash: Option<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub updated_at: u64,
}

fn corrupt(detail: impl Into<String>) -> StoreError {
    StoreError::Malformed(format!(
        "the registry's anchor record is unusable and was not changed: {}",
        detail.into()
    ))
}

fn valid_tx(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

const COLUMNS: &str = "day, chain_id, root, state, tx_hash, attempts, last_error, updated_at";

fn decode(row: &Row<'_>) -> Result<RegistryAnchor, StoreError> {
    let day: String = row.get(0)?;
    let chain_id: i64 = row.get(1)?;
    let root: Vec<u8> = row.get(2)?;
    let state: String = row.get(3)?;
    let tx_hash: Option<String> = row.get(4)?;
    let attempts: i64 = row.get(5)?;
    let last_error: Option<String> = row.get(6)?;
    let updated_at: i64 = row.get(7)?;
    let state = match state.as_str() {
        "reserved" => PeerState::Reserved,
        "submitting" => PeerState::Submitting,
        "submitted" => PeerState::Submitted,
        _ => return Err(corrupt("a stored state is not known")),
    };
    if !ultragrid::valid_date(&day) {
        return Err(corrupt("a stored day is not a date"));
    }
    if tx_hash.as_deref().is_some_and(|h| !valid_tx(h)) {
        return Err(corrupt("a stored transaction hash is malformed"));
    }
    if state == PeerState::Submitted && tx_hash.is_none() {
        return Err(corrupt("a submitted anchor has no transaction hash"));
    }
    Ok(RegistryAnchor {
        day,
        chain_id: u32::try_from(chain_id).map_err(|_| corrupt("a stored chain id is out of range"))?,
        root: root.try_into().map_err(|_| corrupt("a stored root is not 32 bytes"))?,
        state,
        tx_hash,
        attempts: u32::try_from(attempts).map_err(|_| corrupt("a stored attempt count is out of range"))?,
        last_error,
        updated_at: u64::try_from(updated_at).map_err(|_| corrupt("a stored time is out of range"))?,
    })
}

impl GamesStore {
    /// The anchor record of a registry day, if it has one.
    pub fn registry_anchor(&self, day: &str) -> Result<Option<RegistryAnchor>, StoreError> {
        let guard = self.lock()?;
        guard
            .query_row(
                &format!("SELECT {COLUMNS} FROM registry_anchor_attempts WHERE day = ?1"),
                params![day],
                |row| Ok(decode(row)),
            )
            .optional()?
            .transpose()
    }

    /// Make (or find) the anchor record of a registry day. A record that names a different chain or root
    /// than the day does now is an error: a closed day is frozen.
    pub fn ensure_registry_anchor(
        &self,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        now: u64,
    ) -> Result<RegistryAnchor, StoreError> {
        if !ultragrid::valid_date(day) || chain_id == 0 {
            return Err(StoreError::Malformed("a registry anchor needs a date and a chain".into()));
        }
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now_i = i64::try_from(now).map_err(|_| StoreError::NumericRange)?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO registry_anchor_attempts (day, chain_id, root, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'reserved', 0, ?4, ?4)",
            params![day, i64::from(chain_id), root.as_slice(), now_i],
        )?;
        if inserted == 1 {
            fence_bump(&tx)?;
        }
        let row = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM registry_anchor_attempts WHERE day = ?1"),
                params![day],
                |row| Ok(decode(row)),
            )??;
        if row.chain_id != chain_id || &row.root != root {
            return Err(corrupt("it names a different chain or root than the closed day"));
        }
        tx.commit()?;
        drop(guard);
        if inserted == 1 {
            self.after_accounting_commit();
        }
        Ok(row)
    }

    /// Record an attempt before the signer is called, or say why none may be made.
    pub fn begin_registry_submission(
        &self,
        day: &str,
        max_attempts: u32,
        now: u64,
    ) -> Result<SubmitDecision, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM registry_anchor_attempts WHERE day = ?1"),
                params![day],
                |row| Ok(decode(row)),
            )
            .optional()?
            .ok_or_else(|| corrupt("a submission was begun for a day that has no record"))??;
        match current.state {
            PeerState::Submitted => {
                return Ok(SubmitDecision::Wait {
                    tx_hash: current.tx_hash.unwrap_or_default(),
                })
            }
            PeerState::Submitting => {
                return Ok(SubmitDecision::InDoubt {
                    begun_at: current.updated_at,
                })
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
            "UPDATE registry_anchor_attempts SET state = 'submitting', tx_hash = NULL, attempts = ?2, updated_at = ?3 WHERE day = ?1",
            params![day, i64::from(attempt), i64::try_from(now).map_err(|_| StoreError::NumericRange)?],
        )?;
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(SubmitDecision::Proceed { attempt })
    }

    /// Record how a submission ended (see `peer_usage.rs` for what `definite` means).
    pub fn finish_registry_submission(
        &self,
        day: &str,
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
                    "UPDATE registry_anchor_attempts SET state = 'submitted', tx_hash = ?2, last_error = NULL, updated_at = ?3 WHERE day = ?1",
                    params![day, hash, now_i],
                )?
            }
            Err((message, definite)) => tx.execute(
                "UPDATE registry_anchor_attempts SET state = ?2, last_error = ?3 WHERE day = ?1",
                params![day, if definite { "reserved" } else { "submitting" }, message.chars().take(200).collect::<String>()],
            )?,
        };
        if changed != 1 {
            return Err(corrupt("a finished submission has no record"));
        }
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(())
    }

    /// Return a `submitting` day to `reserved` because reconciliation proved its attempt never reached the node.
    pub fn mark_registry_not_submitted(&self, day: &str, begun_at: u64, note: &str) -> Result<bool, StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE registry_anchor_attempts SET state = 'reserved', tx_hash = NULL, last_error = ?3
             WHERE day = ?1 AND state = 'submitting' AND updated_at = ?2",
            params![
                day,
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

    /// An operator's decision about a registry day whose anchor the service could not settle.
    pub fn resolve_registry_anchor(
        &self,
        day: &str,
        resolution: &PeerResolution,
        note: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
                    "SELECT (SELECT COUNT(*) FROM registry_anchor_attempts WHERE lower(tx_hash) = ?1 AND day <> ?2)
                          + (SELECT COUNT(*) FROM peer_anchor_roots WHERE lower(tx_hash) = ?1)",
                    params![hash, day],
                    |row| row.get(0),
                )?;
                if taken > 0 {
                    return Err(StoreError::Malformed(
                        "that transaction is already recorded for another anchor; one transaction anchors one root".into(),
                    ));
                }
                let state = if matches!(resolution, PeerResolution::Landed { .. }) { "submitted" } else { "submitting" };
                tx.execute(
                    "UPDATE registry_anchor_attempts SET state = ?5, tx_hash = ?2, last_error = ?3, updated_at = ?4 WHERE day = ?1",
                    params![day, hash, note, now_i, state],
                )?
            }
            PeerResolution::NotSubmitted => tx.execute(
                "UPDATE registry_anchor_attempts SET state = 'reserved', tx_hash = NULL, last_error = ?2, updated_at = ?3 WHERE day = ?1",
                params![day, note, now_i],
            )?,
        };
        if changed != 1 {
            return Err(StoreError::Malformed("there is no anchor record for that registry day".into()));
        }
        fence_bump(&tx)?;
        tx.commit()?;
        drop(guard);
        self.after_accounting_commit();
        Ok(())
    }

    /// The `(chain, root)` of every registry anchor ever recorded. A registry day can be anchored days after
    /// it closes, so what the ledger shows today may belong to any earlier day's record.
    pub fn registry_known_roots(&self) -> Result<std::collections::HashSet<(u32, [u8; 32])>, StoreError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare(&format!("SELECT {COLUMNS} FROM registry_anchor_attempts"))?;
        let rows = statement.query_map([], |row| Ok(decode(row)))?;
        let mut known = std::collections::HashSet::new();
        for row in rows {
            let row = row??;
            known.insert((row.chain_id, row.root));
        }
        Ok(known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = "2026-11-16";
    const NOW: u64 = 1_794_800_000;

    fn store(name: &str) -> (GamesStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("registry-anchor-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.db");
        (GamesStore::open(&path).unwrap(), path)
    }

    #[test]
    fn the_states_follow_the_same_rules_as_a_peers_and_time_is_never_evidence() {
        let (store, _) = store("states");
        let root = [4u8; 32];
        assert_eq!(store.ensure_registry_anchor(DAY, 3, &root, NOW).unwrap().state, PeerState::Reserved);
        assert_eq!(store.begin_registry_submission(DAY, 5, NOW).unwrap(), SubmitDecision::Proceed { attempt: 1 });
        // Unknown outcome: in doubt at any later time.
        for later in [NOW + 1, NOW + 3_601, NOW + 365 * 86_400] {
            assert_eq!(store.begin_registry_submission(DAY, 5, later).unwrap(), SubmitDecision::InDoubt { begun_at: NOW });
        }
        // Only a reconciliation of exactly this attempt returns it to reserved.
        assert!(!store.mark_registry_not_submitted(DAY, NOW + 1, "stale").unwrap());
        assert!(store.mark_registry_not_submitted(DAY, NOW, "proved").unwrap());
        assert_eq!(store.begin_registry_submission(DAY, 5, NOW + 10).unwrap(), SubmitDecision::Proceed { attempt: 2 });
        // Accepted: answered with the transaction, forever.
        let tx = "ab".repeat(32);
        store.finish_registry_submission(DAY, Ok(&tx), NOW + 20).unwrap();
        assert_eq!(
            store.begin_registry_submission(DAY, 5, NOW + 365 * 86_400).unwrap(),
            SubmitDecision::Wait { tx_hash: tx }
        );
    }

    #[test]
    fn a_definite_failure_may_retry_but_an_ambiguous_one_may_not_and_the_cap_holds() {
        let (store, _) = store("failures");
        store.ensure_registry_anchor(DAY, 3, &[4; 32], NOW).unwrap();
        store.begin_registry_submission(DAY, 2, NOW).unwrap();
        store.finish_registry_submission(DAY, Err(("could not run", true)), NOW).unwrap();
        assert_eq!(store.begin_registry_submission(DAY, 2, NOW).unwrap(), SubmitDecision::Proceed { attempt: 2 });
        store.finish_registry_submission(DAY, Err(("could not run", true)), NOW).unwrap();
        assert_eq!(store.begin_registry_submission(DAY, 2, NOW).unwrap(), SubmitDecision::Exhausted { attempts: 2 });
        let (other, _) = self::store("ambiguous");
        other.ensure_registry_anchor(DAY, 3, &[4; 32], NOW).unwrap();
        other.begin_registry_submission(DAY, 9, NOW).unwrap();
        other.finish_registry_submission(DAY, Err(("timed out", false)), NOW).unwrap();
        assert!(matches!(other.begin_registry_submission(DAY, 9, NOW + 99_999).unwrap(), SubmitDecision::InDoubt { .. }));
    }

    #[test]
    fn a_closed_days_chain_and_root_are_frozen_and_a_record_that_disagrees_is_refused() {
        let (store, _) = store("frozen");
        store.ensure_registry_anchor(DAY, 3, &[4; 32], NOW).unwrap();
        assert!(store.ensure_registry_anchor(DAY, 3, &[4; 32], NOW + 5).is_ok(), "asking again is fine");
        assert!(store.ensure_registry_anchor(DAY, 3, &[5; 32], NOW).is_err(), "another root");
        assert!(store.ensure_registry_anchor(DAY, 4, &[4; 32], NOW).is_err(), "another chain");
        assert!(store.begin_registry_submission("2026-01-01", 3, NOW).is_err(), "a day with no record");
    }

    #[test]
    fn an_operator_can_settle_a_day_and_a_malformed_record_fails_closed() {
        let (store, path) = store("resolve");
        store.ensure_registry_anchor(DAY, 3, &[4; 32], NOW).unwrap();
        store.begin_registry_submission(DAY, 5, NOW).unwrap();
        let tx = "cd".repeat(32);
        store.resolve_registry_anchor(DAY, &PeerResolution::Landed { tx_hash: tx.clone() }, "found it", NOW).unwrap();
        assert_eq!(store.begin_registry_submission(DAY, 5, NOW + 1).unwrap(), SubmitDecision::Wait { tx_hash: tx });
        assert!(store.resolve_registry_anchor(DAY, &PeerResolution::Landed { tx_hash: "nope".into() }, "x", NOW).is_err());
        assert!(store.resolve_registry_anchor("2026-01-01", &PeerResolution::NotSubmitted, "x", NOW).is_err());
        // Tampering: a transaction hash that is not one, and a state nobody wrote.
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch("UPDATE registry_anchor_attempts SET tx_hash = 'junk';").unwrap();
        assert!(store.registry_anchor(DAY).is_err());
        assert!(store.begin_registry_submission(DAY, 5, NOW).is_err());
        raw.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE registry_anchor_attempts SET tx_hash = NULL, state = 'weird';").unwrap();
        assert!(store.registry_anchor(DAY).is_err());
    }

    #[test]
    fn one_transaction_cannot_be_claimed_for_two_registry_days_or_for_a_day_and_a_peer_root() {
        let (store, _) = store("duplicate-claim");
        let (day_two, root_two) = ("2026-11-17", [5u8; 32]);
        store.ensure_registry_anchor(DAY, 3, &[4; 32], NOW).unwrap();
        store.begin_registry_submission(DAY, 5, NOW).unwrap();
        store.ensure_registry_anchor(day_two, 3, &root_two, NOW).unwrap();
        store.begin_registry_submission(day_two, 5, NOW).unwrap();
        let tx = "ab".repeat(32);
        store.resolve_registry_anchor(DAY, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "claimed", NOW).unwrap();
        // A second registry day cannot name it, as a claim or as landed.
        assert!(store.resolve_registry_anchor(day_two, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "twice", NOW).is_err());
        assert!(store.resolve_registry_anchor(day_two, &PeerResolution::Landed { tx_hash: tx.clone() }, "twice", NOW).is_err());
        // Nor can a peer root.
        store.reserve_peer_anchor(DAY, 1, &[9; 32], 8, NOW).unwrap();
        assert!(store.resolve_peer_root(DAY, 1, &[9; 32], &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "peer", NOW).is_err());
        // The first claim is untouched; naming the same hash again for the SAME day is allowed (a repeat).
        assert_eq!(store.registry_anchor(DAY).unwrap().unwrap().tx_hash.as_deref(), Some(tx.as_str()));
        store.resolve_registry_anchor(DAY, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "repeat", NOW).unwrap();
        // And the other direction: a hash a peer root already holds cannot be claimed for a registry day.
        let other = "cd".repeat(32);
        store.begin_peer_submission(DAY, 1, &[9; 32], 3, NOW).unwrap();
        store.resolve_peer_root(DAY, 1, &[9; 32], &PeerResolution::PendingClaim { tx_hash: other.clone() }, "peer claim", NOW).unwrap();
        assert!(store.resolve_registry_anchor(day_two, &PeerResolution::PendingClaim { tx_hash: other }, "registry", NOW).is_err());
    }
}
