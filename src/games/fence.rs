//! Detecting that the database file has been replaced by an older one, and the owner-history baseline.
//!
//! ## The fence
//!
//! Every change to the anchor accounting (`peer_usage.rs`, `registry_anchor.rs`) also bumps a counter in the
//! database's `meta` table, in the same transaction. After the commit, the counter and a random identity are
//! written to a small file beside the database (`<db>.fence`), by write-temp, fsync, rename.
//!
//! ```text
//! meta.fence_id       random, made once
//! meta.fence_counter  incremented by every anchor-accounting change
//! <db>.fence          "<fence_id> <fence_counter>", written after each commit
//! ```
//!
//! If the database is replaced by an older copy, its counter is *behind* the file's (or its identity differs),
//! and the service knows without being told: it holds anchoring (`peer_hold`) until reconciled. A crash between
//! the commit and the file write leaves the database *ahead* of the file, which is harmless and repaired.
//!
//! **What this does not detect:** replacing the whole state directory (database *and* fence file) with an older
//! copy. The fence regresses with it. That case is covered only by the owner-history check below, over its
//! horizon, and otherwise by the explicit restore procedure (`restore-mark`).
//!
//! ## The owner-history baseline
//!
//! Every AppChain transaction this service signs is preceded by a counted attempt. So the owner's AppChain
//! transactions since yesterday can never outnumber the attempts the database recorded for those days, unless
//! the owner key was used elsewhere, or the database has forgotten attempts (a restore). The baseline
//! `owner_ack = "<timestamp> <attempts>"` lets an operator who has reconciled say "everything up to this moment is
//! accounted for": only newer transactions, against only newer attempts, are compared afterwards.

use super::store::{GamesStore, StoreError};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, OptionalExtension, Transaction};
use std::io::Write;

#[derive(Debug, PartialEq, Eq)]
pub enum FenceCheck {
    /// The database is the one the fence file describes (or is ahead of it, after a crash).
    Intact,
    /// The database is older than, or not the same as, the one the fence file describes.
    Regressed(String),
}

fn meta_get(tx: &Transaction<'_>, key: &str) -> Result<Option<String>, StoreError> {
    Ok(tx
        .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| row.get(0))
        .optional()?)
}

fn meta_put(tx: &Transaction<'_>, key: &str, value: &str) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}

/// Called inside every anchor-accounting transaction: bump the counter (creating the identity on first use).
pub(super) fn fence_bump(tx: &Transaction<'_>) -> Result<(), StoreError> {
    if meta_get(tx, "fence_id")?.is_none() {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        meta_put(tx, "fence_id", &hex::encode(bytes))?;
    }
    let counter: u64 = meta_get(tx, "fence_counter")?
        .map(|v| v.parse().map_err(|_| StoreError::Malformed("the fence counter is not a number".into())))
        .transpose()?
        .unwrap_or(0);
    meta_put(tx, "fence_counter", &(counter + 1).to_string())
}

impl GamesStore {
    fn fence_values(&self) -> Result<(Option<String>, u64), StoreError> {
        let guard = self.lock()?;
        let id: Option<String> = guard
            .query_row("SELECT value FROM meta WHERE key = 'fence_id'", [], |r| r.get(0))
            .optional()?;
        let counter: Option<String> = guard
            .query_row("SELECT value FROM meta WHERE key = 'fence_counter'", [], |r| r.get(0))
            .optional()?;
        let counter = match counter {
            None => 0,
            Some(v) => v
                .parse()
                .map_err(|_| StoreError::Malformed("the fence counter is not a number".into()))?,
        };
        Ok((id, counter))
    }

    fn fence_path(&self) -> Option<std::path::PathBuf> {
        self.path.as_ref().map(|p| {
            let mut name = p.file_name().unwrap_or_default().to_os_string();
            name.push(".fence");
            p.with_file_name(name)
        })
    }

    /// Write the fence file for the database as it is now. Atomic: temp, fsync, rename.
    pub fn write_fence_file(&self) -> Result<(), StoreError> {
        let Some(path) = self.fence_path() else {
            return Ok(());
        };
        let (id, counter) = self.fence_values()?;
        let Some(id) = id else {
            return Ok(()); // nothing has ever been recorded: there is nothing to fence
        };
        let temp = path.with_extension("fence.tmp");
        let io = |e: std::io::Error| StoreError::Malformed(format!("cannot write {}: {e}", path.display()));
        let mut file = std::fs::File::create(&temp).map_err(io)?;
        file.write_all(format!("{id} {counter}\n").as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        std::fs::rename(&temp, &path).map_err(io)?;
        Ok(())
    }

    /// Called after a commit that changed the accounting. A failure is logged, not fatal: the database is
    /// then ahead of the file, which the next check repairs.
    pub(super) fn after_accounting_commit(&self) {
        if let Err(error) = self.write_fence_file() {
            tracing::warn!(error = %error, "could not update the fence file");
        }
    }

    /// Compare the database with its fence file. See the module documentation.
    pub fn check_fence(&self) -> Result<FenceCheck, StoreError> {
        let Some(path) = self.fence_path() else {
            return Ok(FenceCheck::Intact);
        };
        let (id, counter) = self.fence_values()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return if counter == 0 {
                    Ok(FenceCheck::Intact)
                } else {
                    Ok(FenceCheck::Regressed(
                        "the fence file is missing but the database records anchor activity".into(),
                    ))
                };
            }
            Err(error) => {
                return Ok(FenceCheck::Regressed(format!("the fence file cannot be read: {error}")))
            }
        };
        let mut parts = text.split_whitespace();
        let (file_id, file_counter) = match (parts.next(), parts.next().and_then(|c| c.parse::<u64>().ok())) {
            (Some(id), Some(counter)) => (id.to_string(), counter),
            _ => return Ok(FenceCheck::Regressed("the fence file is malformed".into())),
        };
        if id.as_deref() != Some(file_id.as_str()) {
            return Ok(FenceCheck::Regressed(
                "this is not the database the fence file describes".into(),
            ));
        }
        if file_counter > counter {
            return Ok(FenceCheck::Regressed(format!(
                "the database is older than the last state it recorded (counter {counter}, the fence file says {file_counter}): it was restored from a backup"
            )));
        }
        if file_counter < counter {
            self.write_fence_file()?; // the database is ahead after a crash: catch the file up
        }
        Ok(FenceCheck::Intact)
    }

    /// An operator has reconciled a regressed database: make the fence file describe it as it is now.
    pub fn accept_fence(&self) -> Result<(), StoreError> {
        let guard = self.lock()?;
        let has_id: Option<String> = guard
            .query_row("SELECT value FROM meta WHERE key = 'fence_id'", [], |r| r.get(0))
            .optional()?;
        drop(guard);
        if has_id.is_none() {
            let mut guard = self.lock()?;
            let tx = guard.transaction()?;
            fence_bump(&tx)?;
            tx.commit()?;
        }
        self.write_fence_file()
    }

    /// `(timestamp, attempts)` of the last reconciliation, or `(0, 0)`.
    pub fn owner_ack(&self) -> Result<(u64, u64), StoreError> {
        let guard = self.lock()?;
        let value: Option<String> = guard
            .query_row("SELECT value FROM meta WHERE key = 'owner_ack'", [], |r| r.get(0))
            .optional()?;
        let Some(value) = value else { return Ok((0, 0)) };
        let mut parts = value.split_whitespace().map(|p| p.parse::<u64>());
        match (parts.next(), parts.next()) {
            (Some(Ok(ts)), Some(Ok(attempts))) => Ok((ts, attempts)),
            _ => Err(StoreError::Malformed("the owner baseline is malformed and was not changed".into())),
        }
    }

    pub fn set_owner_ack(&self, timestamp: u64, attempts: u64) -> Result<(), StoreError> {
        let guard = self.lock()?;
        guard.execute(
            "INSERT INTO meta (key, value) VALUES ('owner_ack', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![format!("{timestamp} {attempts}")],
        )?;
        Ok(())
    }

    /// Every signer invocation this database has recorded since the start of the earliest of `days` (days are
    /// given oldest first). Peer roots are counted by the day they were asked on. A registry day is counted by
    /// when its anchor was last attempted, not by the day it covers: a day closed on the 10th and anchored
    /// on the 13th made its attempt on the 13th. (A day attempted earlier and touched later is over-counted,
    /// which can only make the check more lenient, never raise a false alarm.)
    pub fn recorded_attempts(&self, days: &[&str]) -> Result<u64, StoreError> {
        let guard = self.lock()?;
        let mut total: i64 = 0;
        for day in days {
            let peer: i64 = guard.query_row(
                "SELECT COALESCE(SUM(attempts), 0) FROM peer_anchor_roots WHERE day = ?1",
                params![day],
                |r| r.get(0),
            )?;
            total += peer;
        }
        let since = days
            .first()
            .and_then(|day| super::service::date_start(day))
            .ok_or_else(|| StoreError::Malformed("recorded_attempts needs at least one date".into()))?;
        let registry: i64 = guard.query_row(
            "SELECT COALESCE(SUM(attempts), 0) FROM registry_anchor_attempts WHERE updated_at >= ?1",
            params![i64::try_from(since).map_err(|_| StoreError::NumericRange)?],
            |r| r.get(0),
        )?;
        u64::try_from(total + registry).map_err(|_| StoreError::NumericRange)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DAY: &str = "2026-11-16";
    const NOW: u64 = 1_794_800_000;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fence-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fence_file(db: &std::path::Path) -> PathBuf {
        PathBuf::from(format!("{}.fence", db.display()))
    }

    fn activity(store: &GamesStore, n: u8) {
        store.reserve_peer_anchor(DAY, 1, &[n; 32], 99, NOW).unwrap();
    }

    fn restore(db: &std::path::Path, backup: &std::path::Path) {
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
        }
        std::fs::copy(backup, db).unwrap();
    }

    #[test]
    fn a_new_database_is_intact_and_writes_no_fence_until_something_is_recorded() {
        let db = dir("fresh").join("r.db");
        let store = GamesStore::open(&db).unwrap();
        assert_eq!(store.check_fence().unwrap(), FenceCheck::Intact);
        assert!(!fence_file(&db).exists());
        activity(&store, 1);
        assert!(fence_file(&db).exists(), "the first change writes the fence");
        assert_eq!(store.check_fence().unwrap(), FenceCheck::Intact);
        let text = std::fs::read_to_string(fence_file(&db)).unwrap();
        assert!(text.trim().ends_with(" 1"), "{text}");
        // An in-memory database has nothing to fence.
        assert_eq!(GamesStore::open(std::path::Path::new(":memory:")).unwrap().check_fence().unwrap(), FenceCheck::Intact);
    }

    #[test]
    fn every_accounting_change_moves_the_fence_forward() {
        let db = dir("moves").join("r.db");
        let store = GamesStore::open(&db).unwrap();
        let counter = |s: &GamesStore| s.fence_values().unwrap().1;
        activity(&store, 1);
        let after_reserve = counter(&store);
        store.begin_peer_submission(DAY, 1, &[1; 32], 3, NOW).unwrap();
        let after_begin = counter(&store);
        store.finish_peer_submission(DAY, 1, &[1; 32], Ok(&"ab".repeat(32)), NOW).unwrap();
        let after_finish = counter(&store);
        store.ensure_registry_anchor(DAY, 3, &[9; 32], NOW).unwrap();
        let after_registry = counter(&store);
        store.begin_registry_submission(DAY, 5, NOW).unwrap();
        assert!(after_reserve < after_begin && after_begin < after_finish && after_finish < after_registry && after_registry < counter(&store));
        // A read, and a reservation that only finds an existing root, change nothing.
        let before = counter(&store);
        store.reserve_peer_anchor(DAY, 1, &[1; 32], 99, NOW).unwrap();
        store.peer_anchor_roots(DAY).unwrap();
        assert_eq!(counter(&store), before);
    }

    #[test]
    fn replacing_the_database_with_an_older_copy_is_detected_and_the_file_is_not_rewritten() {
        let d = dir("older");
        let db = d.join("r.db");
        let backup = d.join("backup.db");
        let store = GamesStore::open(&db).unwrap();
        activity(&store, 1);
        store.backup_to(&backup).unwrap();
        activity(&store, 2);
        activity(&store, 3);
        drop(store);
        let fence_before = std::fs::read_to_string(fence_file(&db)).unwrap();
        restore(&db, &backup);
        let restored = GamesStore::open(&db).unwrap();
        match restored.check_fence().unwrap() {
            FenceCheck::Regressed(why) => assert!(why.contains("restored from a backup"), "{why}"),
            other => panic!("expected a regression, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(fence_file(&db)).unwrap(), fence_before, "detection must not heal the fence");
        // Still detected on every later check, and after a restart.
        drop(restored);
        assert!(matches!(GamesStore::open(&db).unwrap().check_fence().unwrap(), FenceCheck::Regressed(_)));
        // An operator who has reconciled accepts the database as it now is.
        let again = GamesStore::open(&db).unwrap();
        again.accept_fence().unwrap();
        assert_eq!(again.check_fence().unwrap(), FenceCheck::Intact);
    }

    #[test]
    fn a_different_database_a_missing_fence_and_a_damaged_fence_are_all_regressions() {
        let d = dir("others");
        let db = d.join("r.db");
        let store = GamesStore::open(&db).unwrap();
        activity(&store, 1);
        // The fence file is gone but the database records activity.
        std::fs::remove_file(fence_file(&db)).unwrap();
        assert!(matches!(store.check_fence().unwrap(), FenceCheck::Regressed(why) if why.contains("missing")));
        store.accept_fence().unwrap();
        assert_eq!(store.check_fence().unwrap(), FenceCheck::Intact);
        // The fence file describes some other database.
        std::fs::write(fence_file(&db), "00000000000000000000000000000000 1\n").unwrap();
        assert!(matches!(store.check_fence().unwrap(), FenceCheck::Regressed(why) if why.contains("not the database")));
        // It is damaged.
        for junk in ["", "garbage", "abc def", "abc"] {
            std::fs::write(fence_file(&db), junk).unwrap();
            assert!(matches!(store.check_fence().unwrap(), FenceCheck::Regressed(_)), "{junk:?}");
        }
    }

    #[test]
    fn a_database_that_is_ahead_of_its_fence_after_a_crash_is_caught_up_not_held() {
        let d = dir("ahead");
        let db = d.join("r.db");
        let store = GamesStore::open(&db).unwrap();
        activity(&store, 1);
        let (id, counter) = store.fence_values().unwrap();
        // The process died after the commit and before the file write: the file is one step behind.
        std::fs::write(fence_file(&db), format!("{} {}\n", id.unwrap(), counter - 1)).unwrap();
        assert_eq!(store.check_fence().unwrap(), FenceCheck::Intact);
        let text = std::fs::read_to_string(fence_file(&db)).unwrap();
        assert!(text.trim().ends_with(&format!(" {counter}")), "caught up: {text}");
    }

    #[test]
    fn the_owner_baseline_round_trips_and_a_malformed_one_is_refused() {
        let store = GamesStore::open(&dir("ack").join("r.db")).unwrap();
        assert_eq!(store.owner_ack().unwrap(), (0, 0));
        store.set_owner_ack(1_794_800_123, 7).unwrap();
        assert_eq!(store.owner_ack().unwrap(), (1_794_800_123, 7));
        store.lock().unwrap().execute("UPDATE meta SET value = 'garbage' WHERE key = 'owner_ack'", []).unwrap();
        assert!(store.owner_ack().is_err(), "a damaged baseline is an error, never a silent zero");
    }

    #[test]
    fn recorded_attempts_add_peer_roots_and_registry_days_for_the_days_asked() {
        let store = GamesStore::open(&dir("attempts").join("r.db")).unwrap();
        activity(&store, 1);
        store.begin_peer_submission(DAY, 1, &[1; 32], 3, NOW).unwrap();
        store.ensure_registry_anchor(DAY, 3, &[9; 32], NOW).unwrap();
        store.begin_registry_submission(DAY, 5, NOW).unwrap();
        store.ensure_registry_anchor("2026-11-15", 3, &[8; 32], NOW).unwrap();
        store.begin_registry_submission("2026-11-15", 5, NOW).unwrap();
        // The registry day for the 15th was attempted at NOW (the 16th): it counts for the 16th.
        assert_eq!(store.recorded_attempts(&[DAY]).unwrap(), 3);
        assert_eq!(store.recorded_attempts(&["2026-11-15", DAY]).unwrap(), 3);
        // Nothing was attempted from the 17th on.
        assert_eq!(store.recorded_attempts(&["2026-11-17"]).unwrap(), 0);
    }
}
