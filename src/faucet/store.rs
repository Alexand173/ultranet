use crate::faucet::{
    abuse::{address_digest, idempotency_digest, request_fingerprint},
    models::{ClaimStatus, SignedTransferRequest},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use thiserror::Error;

const SCHEMA_VERSION: i64 = 1;
const UTC_DAY_SECONDS: u64 = 86_400;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("faucet database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("faucet database lock was poisoned")]
    LockPoisoned,
    #[error("faucet data encoding error: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("faucet numeric value is outside SQLite's safe integer range")]
    NumericRange,
    #[error("faucet claim record is malformed: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone)]
pub struct AbuseControl {
    pub scope: &'static str,
    pub identity_digest: [u8; 32],
    pub window_seconds: u64,
    pub maximum: u32,
}

#[derive(Debug, Clone)]
pub struct ClaimRecord {
    pub claim_id: String,
    pub address: String,
    pub address_digest: [u8; 32],
    pub created_at: u64,
    pub cooldown_until: u64,
    pub status: ClaimStatus,
    pub amount_base_units: u64,
    pub fee_base_units: u64,
    pub source_debit_base_units: u64,
    pub budget_window_start: u64,
    pub failure_code: Option<String>,
    pub submitted_at: Option<u64>,
    pub confirmed_at: Option<u64>,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct PayoutRecord {
    pub claim_id: String,
    pub transaction_hash: Option<[u8; 32]>,
    pub nullifier: Option<[u8; 32]>,
    pub nonce: Option<u64>,
    pub signed_envelope: Option<SignedTransferRequest>,
    pub attempt_count: u32,
    pub last_error_code: Option<String>,
    pub last_attempt_at: Option<u64>,
    pub submitted_at: Option<u64>,
    pub confirmed_at: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ClaimBundle {
    pub claim: ClaimRecord,
    pub payout: PayoutRecord,
}

#[derive(Debug, Clone)]
pub enum AdmissionOutcome {
    Created(ClaimBundle),
    Existing(ClaimBundle),
    AddressCooldown { retry_after_seconds: u64 },
    IdempotencyConflict,
    Disabled,
    QueueFull,
    BudgetExhausted,
    RateLimited { retry_after_seconds: u64 },
}

#[derive(Debug, Clone, Copy)]
pub struct BudgetSnapshot {
    pub window_start: u64,
    pub window_end: u64,
    pub reserved_base_units: u64,
    pub confirmed_base_units: u64,
    pub claim_count: u64,
}

#[derive(Debug, Clone)]
pub struct ServiceSnapshot {
    pub enabled: bool,
    pub kill_switch_reason: Option<String>,
    pub signer_key_id: Option<String>,
    pub faucet_address: Option<String>,
    pub last_observed_nonce: Option<u64>,
    pub last_node_health_at: Option<u64>,
    pub schema_version: u32,
}

#[derive(Clone)]
pub struct FaucetStore {
    connection: Arc<Mutex<Connection>>,
}

impl FaucetStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if path != Path::new(":memory:") {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        StoreError::Malformed(format!(
                            "cannot create database directory {}: {error}",
                            parent.display()
                        ))
                    })?;
                }
            }
        }
        let connection = Connection::open(path)?;
        configure_connection(&connection)?;
        migrate(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        configure_connection(&connection)?;
        migrate(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        operation(&connection)
    }

    fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match operation(&transaction) {
            Ok(value) => {
                transaction.commit()?;
                Ok(value)
            }
            Err(error) => {
                let _ = transaction.rollback();
                Err(error)
            }
        }
    }

    pub fn admit_claim(
        &self,
        claim_id: &str,
        address: &str,
        abuse_key: &[u8],
        idempotency_key: &str,
        now: u64,
        cooldown_seconds: u64,
        amount_base_units: u64,
        fee_base_units: u64,
        daily_cap_base_units: u64,
        max_queue_length: usize,
        abuse_controls: &[AbuseControl],
    ) -> Result<AdmissionOutcome, StoreError> {
        let total_debit = amount_base_units
            .checked_add(fee_base_units)
            .ok_or(StoreError::NumericRange)?;
        let cooldown_until = now
            .checked_add(cooldown_seconds)
            .ok_or(StoreError::NumericRange)?;
        let window_start = utc_window_start(now);
        let window_end = window_start
            .checked_add(UTC_DAY_SECONDS)
            .ok_or(StoreError::NumericRange)?;
        let address_digest = address_digest(abuse_key, address);
        let idempotency_digest = idempotency_digest(abuse_key, idempotency_key);
        let request_fingerprint = request_fingerprint(abuse_key, address, amount_base_units);

        self.with_transaction(|transaction| {
            if let Some((stored_fingerprint, existing_claim_id)) = transaction
                .query_row(
                    "SELECT request_fingerprint, claim_id FROM idempotency_keys WHERE key_digest = ?1",
                    params![idempotency_digest.as_slice()],
                    |row| {
                        let fingerprint: Vec<u8> = row.get(0)?;
                        let claim_id: String = row.get(1)?;
                        Ok((fingerprint, claim_id))
                    },
                )
                .optional()?
            {
                if stored_fingerprint.as_slice() != request_fingerprint {
                    return Ok(AdmissionOutcome::IdempotencyConflict);
                }
                let existing = load_bundle(transaction, &existing_claim_id)?
                    .ok_or_else(|| StoreError::Malformed("idempotency points to missing claim".into()))?;
                return Ok(AdmissionOutcome::Existing(existing));
            }

            let enabled: i64 = transaction.query_row(
                "SELECT enabled FROM service_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )?;
            if enabled == 0 {
                return Ok(AdmissionOutcome::Disabled);
            }

            let queue_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM claims WHERE status IN ('queued', 'submitting', 'pending')",
                [],
                |row| row.get(0),
            )?;
            if queue_count >= i64::try_from(max_queue_length).map_err(|_| StoreError::NumericRange)? {
                return Ok(AdmissionOutcome::QueueFull);
            }

            transaction.execute(
                "DELETE FROM address_cooldowns WHERE cooldown_until <= ?1",
                params![to_i64(now)?],
            )?;
            if let Some(existing_until) = transaction
                .query_row(
                    "SELECT cooldown_until FROM address_cooldowns WHERE address_digest = ?1",
                    params![address_digest.as_slice()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
            {
                let existing_until = from_i64(existing_until)?;
                return Ok(AdmissionOutcome::AddressCooldown {
                    retry_after_seconds: existing_until.saturating_sub(now),
                });
            }

            for control in abuse_controls {
                if control.window_seconds == 0 || control.maximum == 0 {
                    return Err(StoreError::Malformed("invalid abuse control".into()));
                }
                let abuse_window = window_start_for(now, control.window_seconds);
                let count: Option<i64> = transaction
                    .query_row(
                        "SELECT count FROM abuse_buckets WHERE scope = ?1 AND identity_digest = ?2 AND window_start_utc = ?3",
                        params![control.scope, control.identity_digest.as_slice(), to_i64(abuse_window)?],
                        |row| row.get(0),
                    )
                    .optional()?;
                if count.map(from_i64).transpose()?.unwrap_or(0) >= u64::from(control.maximum) {
                    return Ok(AdmissionOutcome::RateLimited {
                        retry_after_seconds: abuse_window
                            .saturating_add(control.window_seconds)
                            .saturating_sub(now),
                    });
                }
            }

            transaction.execute(
                "INSERT INTO budget_windows (window_start_utc, window_end_utc, reserved_base_units, confirmed_base_units, claim_count, policy_version)
                 VALUES (?1, ?2, 0, 0, 0, 'mainnet-beta-v1')
                 ON CONFLICT(window_start_utc) DO NOTHING",
                params![to_i64(window_start)?, to_i64(window_end)?],
            )?;
            let reserved: i64 = transaction.query_row(
                "SELECT reserved_base_units FROM budget_windows WHERE window_start_utc = ?1",
                params![to_i64(window_start)?],
                |row| row.get(0),
            )?;
            let next_reserved = from_i64(reserved)?
                .checked_add(total_debit)
                .ok_or(StoreError::NumericRange)?;
            if next_reserved > daily_cap_base_units {
                return Ok(AdmissionOutcome::BudgetExhausted);
            }

            transaction.execute(
                "UPDATE budget_windows SET reserved_base_units = ?1, claim_count = claim_count + 1 WHERE window_start_utc = ?2",
                params![to_i64(next_reserved)?, to_i64(window_start)?],
            )?;
            for control in abuse_controls {
                let abuse_window = window_start_for(now, control.window_seconds);
                transaction.execute(
                    "INSERT INTO abuse_buckets (scope, identity_digest, window_start_utc, count, expires_at)
                     VALUES (?1, ?2, ?3, 1, ?4)
                     ON CONFLICT(scope, identity_digest, window_start_utc)
                     DO UPDATE SET count = count + 1",
                    params![
                        control.scope,
                        control.identity_digest.as_slice(),
                        to_i64(abuse_window)?,
                        to_i64(abuse_window.saturating_add(control.window_seconds))?,
                    ],
                )?;
            }
            transaction.execute(
                "INSERT INTO claims (claim_id, address, address_digest, created_at, cooldown_until, status,
                    amount_base_units, fee_base_units, source_debit_base_units, budget_window_start,
                    idempotency_fingerprint, failure_code, submitted_at, confirmed_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7, ?8, ?9, ?10, NULL, NULL, NULL, ?4)",
                params![
                    claim_id,
                    address,
                    address_digest.as_slice(),
                    to_i64(now)?,
                    to_i64(cooldown_until)?,
                    to_i64(amount_base_units)?,
                    to_i64(fee_base_units)?,
                    to_i64(total_debit)?,
                    to_i64(window_start)?,
                    request_fingerprint.as_slice(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO idempotency_keys (key_digest, request_fingerprint, claim_id, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    idempotency_digest.as_slice(),
                    request_fingerprint.as_slice(),
                    claim_id,
                    to_i64(now)?,
                    to_i64(cooldown_until.saturating_add(UTC_DAY_SECONDS))?,
                ],
            )?;
            transaction.execute(
                "INSERT INTO address_cooldowns (address_digest, claim_id, cooldown_until) VALUES (?1, ?2, ?3)",
                params![address_digest.as_slice(), claim_id, to_i64(cooldown_until)?],
            )?;
            let bundle = load_bundle(transaction, claim_id)?
                .ok_or_else(|| StoreError::Malformed("new claim could not be loaded".into()))?;
            Ok(AdmissionOutcome::Created(bundle))
        })
    }

    pub fn claim(&self, claim_id: &str) -> Result<Option<ClaimBundle>, StoreError> {
        self.with_connection(|connection| load_bundle(connection, claim_id))
    }

    pub fn lookup_idempotency(
        &self,
        abuse_key: &[u8],
        idempotency_key: &str,
        address: &str,
        amount_base_units: u64,
    ) -> Result<Option<Result<ClaimBundle, ()>>, StoreError> {
        let key_digest = idempotency_digest(abuse_key, idempotency_key);
        let fingerprint = request_fingerprint(abuse_key, address, amount_base_units);
        self.with_connection(|connection| {
            let record = connection
                .query_row(
                    "SELECT request_fingerprint, claim_id FROM idempotency_keys WHERE key_digest = ?1",
                    params![key_digest.as_slice()],
                    |row| {
                        let stored: Vec<u8> = row.get(0)?;
                        let claim_id: String = row.get(1)?;
                        Ok((stored, claim_id))
                    },
                )
                .optional()?;
            let Some((stored, claim_id)) = record else {
                return Ok(None);
            };
            if stored.as_slice() != fingerprint {
                return Ok(Some(Err(())));
            }
            let bundle = load_bundle(connection, &claim_id)?.ok_or_else(|| {
                StoreError::Malformed("idempotency points to missing claim".into())
            })?;
            Ok(Some(Ok(bundle)))
        })
    }

    pub fn requeue_submitting_without_envelope(
        &self,
        claim_id: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        self.with_transaction(|transaction| {
            transaction.execute(
                "UPDATE claims SET status = 'queued', updated_at = ?1
                 WHERE claim_id = ?2 AND status = 'submitting'
                   AND EXISTS (SELECT 1 FROM payouts WHERE claim_id = ?2 AND signed_envelope IS NULL)",
                params![to_i64(now)?, claim_id],
            )?;
            Ok(())
        })
    }

    pub fn take_next_queued_claim(&self, now: u64) -> Result<Option<String>, StoreError> {
        self.with_transaction(|transaction| {
            let claim_id = transaction
                .query_row(
                    "SELECT claim_id FROM claims WHERE status = 'queued'
                     AND EXISTS (SELECT 1 FROM service_state WHERE singleton = 1 AND enabled = 1)
                     ORDER BY created_at, claim_id LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(claim_id) = claim_id else {
                return Ok(None);
            };
            transaction.execute(
                "UPDATE claims SET status = 'submitting', updated_at = ?1 WHERE claim_id = ?2 AND status = 'queued'",
                params![to_i64(now)?, claim_id],
            )?;
            Ok(Some(claim_id))
        })
    }

    pub fn set_envelope(
        &self,
        claim_id: &str,
        envelope: &SignedTransferRequest,
        transaction_hash: &[u8; 32],
        now: u64,
    ) -> Result<(), StoreError> {
        let encoded = serde_json::to_string(envelope)?;
        let nullifier = envelope.nullifier_array().map_err(StoreError::Malformed)?;
        self.with_transaction(|transaction| {
            transaction.execute(
                "UPDATE payouts SET transaction_hash = ?1, nullifier = ?2, nonce = ?3,
                    signed_envelope = ?4, attempt_count = attempt_count + 1,
                    last_attempt_at = ?5, last_error_code = NULL
                 WHERE claim_id = ?6 AND signed_envelope IS NULL",
                params![
                    hex::encode(transaction_hash),
                    nullifier.as_slice(),
                    to_i64(envelope.nonce)?,
                    encoded,
                    to_i64(now)?,
                    claim_id,
                ],
            )?;
            transaction.execute(
                "UPDATE claims SET updated_at = ?1 WHERE claim_id = ?2",
                params![to_i64(now)?, claim_id],
            )?;
            Ok(())
        })
    }

    pub fn update_fee_reservation(
        &self,
        claim_id: &str,
        fee_base_units: u64,
        daily_cap_base_units: u64,
        now: u64,
    ) -> Result<bool, StoreError> {
        self.with_transaction(|transaction| {
            let (old_fee, old_debit, window_start): (i64, i64, i64) = transaction.query_row(
                "SELECT fee_base_units, source_debit_base_units, budget_window_start FROM claims WHERE claim_id = ?1",
                params![claim_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let old_debit = from_i64(old_debit)?;
            let old_fee = from_i64(old_fee)?;
            let amount = old_debit.checked_sub(old_fee).ok_or(StoreError::NumericRange)?;
            let new_debit = amount
                .checked_add(fee_base_units)
                .ok_or(StoreError::NumericRange)?;
            let reserved: i64 = transaction.query_row(
                "SELECT reserved_base_units FROM budget_windows WHERE window_start_utc = ?1",
                params![window_start],
                |row| row.get(0),
            )?;
            let reserved = from_i64(reserved)?;
            let adjusted_reserved = reserved
                .checked_sub(old_debit)
                .ok_or(StoreError::NumericRange)?
                .checked_add(new_debit)
                .ok_or(StoreError::NumericRange)?;
            if adjusted_reserved > daily_cap_base_units {
                return Ok(false);
            }
            transaction.execute(
                "UPDATE claims SET fee_base_units = ?1, source_debit_base_units = ?2, updated_at = ?3 WHERE claim_id = ?4",
                params![to_i64(fee_base_units)?, to_i64(new_debit)?, to_i64(now)?, claim_id],
            )?;
            transaction.execute(
                "UPDATE budget_windows SET reserved_base_units = ?1 WHERE window_start_utc = ?2",
                params![to_i64(adjusted_reserved)?, window_start],
            )?;
            Ok(true)
        })
    }

    pub fn record_submission_attempt(
        &self,
        claim_id: &str,
        now: u64,
        error_code: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_transaction(|transaction| {
            transaction.execute(
                "UPDATE payouts SET attempt_count = attempt_count + 1, last_attempt_at = ?1, last_error_code = ?2 WHERE claim_id = ?3",
                params![to_i64(now)?, error_code, claim_id],
            )?;
            transaction.execute(
                "UPDATE claims SET updated_at = ?1 WHERE claim_id = ?2",
                params![to_i64(now)?, claim_id],
            )?;
            Ok(())
        })
    }

    pub fn mark_pending(
        &self,
        claim_id: &str,
        transaction_hash: &[u8; 32],
        now: u64,
    ) -> Result<(), StoreError> {
        self.with_transaction(|transaction| {
            transaction.execute(
                "UPDATE payouts SET transaction_hash = ?1, submitted_at = COALESCE(submitted_at, ?2)
                 WHERE claim_id = ?3",
                params![hex::encode(transaction_hash), to_i64(now)?, claim_id],
            )?;
            transaction.execute(
                "UPDATE claims SET status = 'pending', submitted_at = COALESCE(submitted_at, ?1), updated_at = ?1 WHERE claim_id = ?2 AND status IN ('submitting', 'pending')",
                params![to_i64(now)?, claim_id],
            )?;
            Ok(())
        })
    }

    pub fn mark_confirmed(
        &self,
        claim_id: &str,
        transaction_hash: &[u8; 32],
        now: u64,
    ) -> Result<(), StoreError> {
        self.with_transaction(|transaction| {
            let status: String = transaction.query_row(
                "SELECT status FROM claims WHERE claim_id = ?1",
                params![claim_id],
                |row| row.get(0),
            )?;
            if status == "confirmed" {
                return Ok(());
            }
            let (debit, window_start, reserved, confirmed): (i64, i64, i64, i64) =
                transaction.query_row(
                    "SELECT c.source_debit_base_units, c.budget_window_start,
                        b.reserved_base_units, b.confirmed_base_units
                     FROM claims c
                     JOIN budget_windows b ON b.window_start_utc = c.budget_window_start
                     WHERE c.claim_id = ?1",
                    params![claim_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            let debit = from_i64(debit)?;
            let window_start = from_i64(window_start)?;
            let reserved = from_i64(reserved)?;
            let confirmed = from_i64(confirmed)?;
            let next_reserved = reserved
                .checked_sub(debit)
                .ok_or(StoreError::Malformed("confirmed debit exceeds reservation".into()))?;
            let next_confirmed = confirmed
                .checked_add(debit)
                .ok_or(StoreError::NumericRange)?;
            transaction.execute(
                "UPDATE payouts SET transaction_hash = ?1, confirmed_at = ?2 WHERE claim_id = ?3",
                params![hex::encode(transaction_hash), to_i64(now)?, claim_id],
            )?;
            transaction.execute(
                "UPDATE claims SET status = 'confirmed', confirmed_at = ?1, updated_at = ?1, failure_code = NULL WHERE claim_id = ?2",
                params![to_i64(now)?, claim_id],
            )?;
            transaction.execute(
                "UPDATE budget_windows SET reserved_base_units = ?1, confirmed_base_units = ?2 WHERE window_start_utc = ?3",
                params![to_i64(next_reserved)?, to_i64(next_confirmed)?, to_i64(window_start)?],
            )?;
            Ok(())
        })
    }

    pub fn mark_failed(
        &self,
        claim_id: &str,
        failure_code: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        self.with_transaction(|transaction| {
            let status: String = transaction.query_row(
                "SELECT status FROM claims WHERE claim_id = ?1",
                params![claim_id],
                |row| row.get(0),
            )?;
            if matches!(status.as_str(), "confirmed" | "failed") {
                return Ok(());
            }
            let (debit, window_start, address_digest, reserved, claim_count):
                (i64, i64, Vec<u8>, i64, i64) = transaction.query_row(
                    "SELECT c.source_debit_base_units, c.budget_window_start, c.address_digest,
                        b.reserved_base_units, b.claim_count
                     FROM claims c
                     JOIN budget_windows b ON b.window_start_utc = c.budget_window_start
                     WHERE c.claim_id = ?1",
                    params![claim_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )?;
            let debit = from_i64(debit)?;
            let window_start = from_i64(window_start)?;
            let reserved = from_i64(reserved)?;
            let claim_count = from_i64(claim_count)?;
            let next_reserved = reserved
                .checked_sub(debit)
                .ok_or(StoreError::Malformed("failed debit exceeds reservation".into()))?;
            let next_claim_count = claim_count
                .checked_sub(1)
                .ok_or(StoreError::Malformed("failed claim count is already zero".into()))?;
            transaction.execute(
                "UPDATE claims SET status = 'failed', failure_code = ?1, updated_at = ?2 WHERE claim_id = ?3",
                params![failure_code, to_i64(now)?, claim_id],
            )?;
            transaction.execute(
                "UPDATE budget_windows SET reserved_base_units = ?1, claim_count = ?2 WHERE window_start_utc = ?3",
                params![to_i64(next_reserved)?, to_i64(next_claim_count)?, to_i64(window_start)?],
            )?;
            transaction.execute(
                "DELETE FROM address_cooldowns WHERE address_digest = ?1 AND claim_id = ?2",
                params![address_digest, claim_id],
            )?;
            Ok(())
        })
    }

    pub fn recover_pending_claims(&self) -> Result<Vec<ClaimBundle>, StoreError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT claim_id FROM claims WHERE status IN ('submitting', 'pending') ORDER BY created_at, claim_id",
            )?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|claim_id| {
                    load_bundle(connection, &claim_id)?.ok_or_else(|| {
                        StoreError::Malformed("recovery claim disappeared during read".into())
                    })
                })
                .collect()
        })
    }

    pub fn set_service_state(
        &self,
        enabled: bool,
        kill_switch_reason: Option<&str>,
        signer_key_id: Option<&str>,
        faucet_address: Option<&str>,
    ) -> Result<(), StoreError> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE service_state SET enabled = ?1, kill_switch_reason = ?2,
                    signer_key_id = COALESCE(?3, signer_key_id),
                    faucet_address = COALESCE(?4, faucet_address)
                 WHERE singleton = 1",
                params![
                    if enabled { 1 } else { 0 },
                    kill_switch_reason,
                    signer_key_id,
                    faucet_address
                ],
            )?;
            Ok(())
        })
    }

    pub fn service_state(&self) -> Result<ServiceSnapshot, StoreError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT enabled, kill_switch_reason, signer_key_id, faucet_address,
                        last_observed_nonce, last_node_health_at, schema_version
                     FROM service_state WHERE singleton = 1",
                    [],
                    |row| {
                        let schema_version = u32::try_from(row.get::<_, i64>(6)?)
                            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(6, i64::MAX))?;
                        Ok(ServiceSnapshot {
                            enabled: row.get::<_, i64>(0)? != 0,
                            kill_switch_reason: row.get(1)?,
                            signer_key_id: row.get(2)?,
                            faucet_address: row.get(3)?,
                            last_observed_nonce: row
                                .get::<_, Option<i64>>(4)?
                                .map(|value| sqlite_u64(value, 4))
                                .transpose()?,
                            last_node_health_at: row
                                .get::<_, Option<i64>>(5)?
                                .map(|value| sqlite_u64(value, 5))
                                .transpose()?,
                            schema_version,
                        })
                    },
                )
                .map_err(StoreError::from)
        })
    }

    pub fn update_node_observation(&self, nonce: u64, observed_at: u64) -> Result<(), StoreError> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE service_state SET last_observed_nonce = ?1, last_node_health_at = ?2 WHERE singleton = 1",
                params![to_i64(nonce)?, to_i64(observed_at)?],
            )?;
            Ok(())
        })
    }

    pub fn budget_snapshot(&self, now: u64) -> Result<BudgetSnapshot, StoreError> {
        let window_start = utc_window_start(now);
        let window_end = window_start.saturating_add(UTC_DAY_SECONDS);
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT reserved_base_units, confirmed_base_units, claim_count FROM budget_windows WHERE window_start_utc = ?1",
                    params![to_i64(window_start)?],
                    |row| {
                        Ok(BudgetSnapshot {
                            window_start,
                            window_end,
                            reserved_base_units: sqlite_u64(row.get(0)?, 0)?,
                            confirmed_base_units: sqlite_u64(row.get(1)?, 1)?,
                            claim_count: sqlite_u64(row.get(2)?, 2)?,
                        })
                    },
                )
                .optional()
                .map(|snapshot| snapshot.unwrap_or(BudgetSnapshot {
                    window_start,
                    window_end,
                    reserved_base_units: 0,
                    confirmed_base_units: 0,
                    claim_count: 0,
                }))
                .map_err(StoreError::from)
        })
    }

    pub fn queue_depth(&self) -> Result<u64, StoreError> {
        self.with_connection(|connection| {
            let count: i64 = connection.query_row(
                "SELECT COUNT(*) FROM claims WHERE status IN ('queued', 'submitting', 'pending')",
                [],
                |row| row.get(0),
            )?;
            from_i64(count)
        })
    }
}

fn configure_connection(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS claims (
            claim_id TEXT PRIMARY KEY NOT NULL,
            address TEXT NOT NULL,
            address_digest BLOB NOT NULL,
            created_at INTEGER NOT NULL,
            cooldown_until INTEGER NOT NULL,
            status TEXT NOT NULL CHECK (status IN ('queued', 'submitting', 'pending', 'confirmed', 'failed')),
            amount_base_units INTEGER NOT NULL CHECK (amount_base_units >= 0),
            fee_base_units INTEGER NOT NULL CHECK (fee_base_units >= 0),
            source_debit_base_units INTEGER NOT NULL CHECK (source_debit_base_units >= 0),
            budget_window_start INTEGER NOT NULL,
            idempotency_fingerprint BLOB NOT NULL,
            failure_code TEXT,
            submitted_at INTEGER,
            confirmed_at INTEGER,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS claims_status_created_idx ON claims(status, created_at);
        CREATE INDEX IF NOT EXISTS claims_address_digest_idx ON claims(address_digest, cooldown_until);
        CREATE TABLE IF NOT EXISTS idempotency_keys (
            key_digest BLOB PRIMARY KEY NOT NULL,
            request_fingerprint BLOB NOT NULL,
            claim_id TEXT NOT NULL REFERENCES claims(claim_id),
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS address_cooldowns (
            address_digest BLOB PRIMARY KEY NOT NULL,
            claim_id TEXT NOT NULL UNIQUE REFERENCES claims(claim_id),
            cooldown_until INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS payouts (
            claim_id TEXT PRIMARY KEY NOT NULL REFERENCES claims(claim_id),
            transaction_hash TEXT,
            nullifier BLOB,
            nonce INTEGER,
            signed_envelope TEXT,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            last_error_code TEXT,
            last_attempt_at INTEGER,
            submitted_at INTEGER,
            confirmed_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS payouts_transaction_hash_idx ON payouts(transaction_hash);
        CREATE TABLE IF NOT EXISTS budget_windows (
            window_start_utc INTEGER PRIMARY KEY NOT NULL,
            window_end_utc INTEGER NOT NULL,
            reserved_base_units INTEGER NOT NULL CHECK (reserved_base_units >= 0),
            confirmed_base_units INTEGER NOT NULL CHECK (confirmed_base_units >= 0),
            claim_count INTEGER NOT NULL CHECK (claim_count >= 0),
            policy_version TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS abuse_buckets (
            scope TEXT NOT NULL,
            identity_digest BLOB NOT NULL,
            window_start_utc INTEGER NOT NULL,
            count INTEGER NOT NULL CHECK (count >= 0),
            expires_at INTEGER NOT NULL,
            PRIMARY KEY (scope, identity_digest, window_start_utc)
        );
        CREATE TABLE IF NOT EXISTS service_state (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
            kill_switch_reason TEXT,
            signer_key_id TEXT,
            faucet_address TEXT,
            last_observed_nonce INTEGER,
            last_node_health_at INTEGER,
            schema_version INTEGER NOT NULL
        );
        INSERT INTO service_state (singleton, enabled, schema_version)
            VALUES (1, 0, 1)
            ON CONFLICT(singleton) DO NOTHING;
        CREATE TRIGGER IF NOT EXISTS claims_create_payout
            AFTER INSERT ON claims
            BEGIN
                INSERT INTO payouts (claim_id) VALUES (NEW.claim_id);
            END;
        PRAGMA user_version = 1;",
    )?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(StoreError::Malformed(format!(
            "unsupported faucet database schema version {version}"
        )));
    }
    Ok(())
}

fn load_bundle(connection: &Connection, claim_id: &str) -> Result<Option<ClaimBundle>, StoreError> {
    let claim = connection
        .query_row(
            "SELECT claim_id, address, address_digest, created_at, cooldown_until, status,
                amount_base_units, fee_base_units, source_debit_base_units, budget_window_start,
                failure_code, submitted_at, confirmed_at, updated_at
             FROM claims WHERE claim_id = ?1",
            params![claim_id],
            |row| {
                let digest: Vec<u8> = row.get(2)?;
                let status_value: String = row.get(5)?;
                let status = ClaimStatus::from_db(&status_value).ok_or_else(|| {
                    to_sql_error(StoreError::Malformed("unknown claim status".into()))
                })?;
                Ok(ClaimRecord {
                    claim_id: row.get(0)?,
                    address: row.get(1)?,
                    address_digest: fixed_32(&digest).map_err(to_sql_error)?,
                    created_at: sqlite_u64(row.get(3)?, 3)?,
                    cooldown_until: sqlite_u64(row.get(4)?, 4)?,
                    status,
                    amount_base_units: sqlite_u64(row.get(6)?, 6)?,
                    fee_base_units: sqlite_u64(row.get(7)?, 7)?,
                    source_debit_base_units: sqlite_u64(row.get(8)?, 8)?,
                    budget_window_start: sqlite_u64(row.get(9)?, 9)?,
                    failure_code: row.get(10)?,
                    submitted_at: row
                        .get::<_, Option<i64>>(11)?
                        .map(|value| sqlite_u64(value, 11))
                        .transpose()?,
                    confirmed_at: row
                        .get::<_, Option<i64>>(12)?
                        .map(|value| sqlite_u64(value, 12))
                        .transpose()?,
                    updated_at: sqlite_u64(row.get(13)?, 13)?,
                })
            },
        )
        .optional()?;
    let Some(claim) = claim else {
        return Ok(None);
    };
    let payout = connection.query_row(
        "SELECT claim_id, transaction_hash, nullifier, nonce, signed_envelope,
            attempt_count, last_error_code, last_attempt_at, submitted_at, confirmed_at
         FROM payouts WHERE claim_id = ?1",
        params![claim_id],
        |row| {
            let transaction_hash: Option<String> = row.get(1)?;
            let nullifier: Option<Vec<u8>> = row.get(2)?;
            let envelope: Option<String> = row.get(4)?;
            let signed_envelope = envelope
                .as_deref()
                .map(|value| serde_json::from_str(value).map_err(serde_to_sql_error))
                .transpose()?;
            Ok(PayoutRecord {
                claim_id: row.get(0)?,
                transaction_hash: transaction_hash
                    .as_deref()
                    .map(parse_hash)
                    .transpose()
                    .map_err(to_sql_error)?,
                nullifier: nullifier
                    .as_deref()
                    .map(fixed_32)
                    .transpose()
                    .map_err(to_sql_error)?,
                nonce: row
                    .get::<_, Option<i64>>(3)?
                    .map(|value| sqlite_u64(value, 3))
                    .transpose()?,
                signed_envelope,
                attempt_count: u32::try_from(row.get::<_, i64>(5)?)
                    .map_err(|_| to_sql_error(StoreError::NumericRange))?,
                last_error_code: row.get(6)?,
                last_attempt_at: row
                    .get::<_, Option<i64>>(7)?
                    .map(|value| sqlite_u64(value, 7))
                    .transpose()?,
                submitted_at: row
                    .get::<_, Option<i64>>(8)?
                    .map(|value| sqlite_u64(value, 8))
                    .transpose()?,
                confirmed_at: row
                    .get::<_, Option<i64>>(9)?
                    .map(|value| sqlite_u64(value, 9))
                    .transpose()?,
            })
        },
    )?;
    Ok(Some(ClaimBundle { claim, payout }))
}

fn utc_window_start(now: u64) -> u64 {
    now - (now % UTC_DAY_SECONDS)
}

fn window_start_for(now: u64, window_seconds: u64) -> u64 {
    now - (now % window_seconds)
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn from_i64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::NumericRange)
}

fn sqlite_u64(value: i64, column: usize) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}

fn fixed_32(value: &[u8]) -> Result<[u8; 32], StoreError> {
    value
        .try_into()
        .map_err(|_| StoreError::Malformed("digest must be exactly 32 bytes".into()))
}

fn parse_hash(value: &str) -> Result<[u8; 32], StoreError> {
    let decoded = hex::decode(value)
        .map_err(|_| StoreError::Malformed("transaction hash is not hexadecimal".into()))?;
    fixed_32(&decoded)
}

fn to_sql_error(error: StoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error))
}

fn serde_to_sql_error(error: serde_json::Error) -> rusqlite::Error {
    to_sql_error(StoreError::Encoding(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faucet::config::DEFAULT_FAUCET_ADDRESS;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn store() -> FaucetStore {
        FaucetStore::open_in_memory().unwrap()
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn admission_is_idempotent_and_budgeted() {
        let store = store();
        store
            .set_service_state(true, None, None, Some(DEFAULT_FAUCET_ADDRESS))
            .unwrap();
        let first = store
            .admit_claim(
                "claim-one",
                &"a".repeat(64),
                b"abuse-key",
                &"i".repeat(16),
                now(),
                86_400,
                1_000_000,
                10_000,
                1_010_000,
                10,
                &[],
            )
            .unwrap();
        assert!(matches!(first, AdmissionOutcome::Created(_)));
        let duplicate = store
            .admit_claim(
                "claim-two",
                &"a".repeat(64),
                b"abuse-key",
                &"i".repeat(16),
                now(),
                86_400,
                1_000_000,
                10_000,
                1_010_000,
                10,
                &[],
            )
            .unwrap();
        assert!(matches!(duplicate, AdmissionOutcome::Existing(_)));
    }

    #[test]
    fn changed_idempotency_fingerprint_is_rejected() {
        let store = store();
        store.set_service_state(true, None, None, None).unwrap();
        store
            .admit_claim(
                "claim-one",
                &"a".repeat(64),
                b"key",
                &"i".repeat(16),
                now(),
                86_400,
                1_000_000,
                10_000,
                100_000_000,
                10,
                &[],
            )
            .unwrap();
        let changed = store
            .admit_claim(
                "claim-two",
                &"b".repeat(64),
                b"key",
                &"i".repeat(16),
                now(),
                86_400,
                1_000_000,
                10_000,
                100_000_000,
                10,
                &[],
            )
            .unwrap();
        assert!(matches!(changed, AdmissionOutcome::IdempotencyConflict));
    }
}
