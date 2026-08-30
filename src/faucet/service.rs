use crate::faucet::{
    abuse::{client_digest, subnet_digest, validate_idempotency_key, IP_SCOPE, SUBNET_SCOPE},
    captcha::{CaptchaError, CaptchaVerifier},
    config::FaucetConfig,
    models::{NodeTransactionData, PublicStatusData},
    node_client::{NodeApi, NodeClientError},
    signer::{FaucetSigner, SignerError},
    store::{AbuseControl, AdmissionOutcome, ClaimBundle, FaucetStore, StoreError},
};
use crate::UltraBlockchain;
use rand::{rngs::OsRng, RngCore};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::sync::{watch, Mutex};
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum FaucetError {
    #[error("invalid faucet request")]
    InvalidRequest,
    #[error("faucet anti-bot verification failed")]
    CaptchaRejected,
    #[error("faucet anti-bot verification is unavailable")]
    CaptchaUnavailable,
    #[error("faucet state is unavailable")]
    Store(#[from] StoreError),
    #[error("faucet intake is disabled")]
    Disabled,
    #[error("faucet claim queue is full")]
    QueueFull,
    #[error("faucet budget is exhausted")]
    BudgetExhausted,
    #[error("destination address is cooling down")]
    AddressCooldown(u64),
    #[error("idempotency key conflict")]
    IdempotencyConflict,
    #[error("faucet request is rate limited")]
    RateLimited(u64),
    #[error("faucet node is unavailable")]
    NodeUnavailable,
    #[error("faucet node rejected the transaction")]
    NodeRejected,
    #[error("faucet signer is unavailable")]
    SignerUnavailable,
    #[error("faucet payout envelope is invalid")]
    SignerInvalid,
    #[error("faucet payout confirmation timed out")]
    ConfirmationTimeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionResult {
    Created,
    Existing,
}

pub struct FaucetService {
    pub config: FaucetConfig,
    pub store: FaucetStore,
    pub signer: Arc<FaucetSigner>,
    pub node: Arc<dyn NodeApi>,
    pub captcha: Arc<dyn CaptchaVerifier>,
    abuse_key: Zeroizing<Vec<u8>>,
    operator_token: Zeroizing<Vec<u8>>,
    consecutive_node_failures: AtomicU32,
    pub(crate) worker_lock: Mutex<()>,
    pub(crate) operator_lock: Mutex<()>,
}

impl FaucetService {
    pub fn new(
        config: FaucetConfig,
        store: FaucetStore,
        signer: Arc<FaucetSigner>,
        node: Arc<dyn NodeApi>,
        captcha: Arc<dyn CaptchaVerifier>,
        abuse_key: Zeroizing<Vec<u8>>,
        operator_token: Zeroizing<Vec<u8>>,
    ) -> Result<Self, FaucetError> {
        let config = config.validate().map_err(|_| FaucetError::InvalidRequest)?;
        if abuse_key.len() < 16 || operator_token.len() < 16 {
            return Err(FaucetError::InvalidRequest);
        }
        if signer.address() != config.faucet_address {
            return Err(FaucetError::SignerUnavailable);
        }
        let existing_state = store.service_state()?;
        if existing_state
            .faucet_address
            .as_deref()
            .is_some_and(|address| address != config.faucet_address)
            || existing_state
                .signer_key_id
                .as_deref()
                .is_some_and(|key_id| key_id != signer.key_id())
        {
            return Err(FaucetError::InvalidRequest);
        }
        let should_enable = if !config.enabled {
            false
        } else if existing_state.faucet_address.is_none()
            || existing_state.kill_switch_reason.as_deref() == Some("disabled by configuration")
        {
            true
        } else {
            existing_state.enabled
        };
        let kill_switch_reason = if should_enable {
            None
        } else if existing_state.kill_switch_reason.is_some() {
            existing_state.kill_switch_reason.as_deref()
        } else {
            Some("disabled by configuration")
        };
        store.set_service_state(
            should_enable,
            kill_switch_reason,
            Some(signer.key_id()),
            Some(signer.address()),
        )?;
        Ok(Self {
            config,
            store,
            signer,
            node,
            captcha,
            abuse_key,
            operator_token,
            consecutive_node_failures: AtomicU32::new(0),
            worker_lock: Mutex::new(()),
            operator_lock: Mutex::new(()),
        })
    }

    pub fn normalize_address(&self, value: &str) -> Result<String, FaucetError> {
        let address = value.trim_matches(|character: char| character.is_ascii_whitespace());
        if !UltraBlockchain::is_valid_address(address) || address == self.config.faucet_address {
            return Err(FaucetError::InvalidRequest);
        }
        Ok(address.to_string())
    }

    pub fn verify_operator_token(&self, candidate: &str) -> bool {
        let candidate = candidate.as_bytes();
        candidate.len() == self.operator_token.len()
            && candidate.ct_eq(self.operator_token.as_slice()).into()
    }

    pub async fn admit_claim(
        &self,
        address: &str,
        captcha_token: &str,
        idempotency_key: &str,
        client_identity: Option<&str>,
        now: u64,
    ) -> Result<(AdmissionResult, ClaimBundle), FaucetError> {
        let address = self.normalize_address(address)?;
        validate_idempotency_key(idempotency_key).map_err(|_| FaucetError::InvalidRequest)?;
        if let Some(existing) = self.store.lookup_idempotency(
            &self.abuse_key,
            idempotency_key,
            &address,
            self.config.claim_amount_base_units,
        )? {
            return match existing {
                Ok(bundle) => Ok((AdmissionResult::Existing, bundle)),
                Err(()) => Err(FaucetError::IdempotencyConflict),
            };
        }
        if captcha_token.trim().is_empty() || captcha_token.len() > 4096 {
            return Err(FaucetError::CaptchaRejected);
        }
        match self.captcha.verify(captcha_token, client_identity).await {
            Ok(true) => {}
            Ok(false) | Err(CaptchaError::Invalid) => return Err(FaucetError::CaptchaRejected),
            Err(CaptchaError::Unavailable | CaptchaError::Misconfigured) => {
                return Err(FaucetError::CaptchaUnavailable)
            }
        }

        let mut abuse_controls = Vec::new();
        if let Some(digest) = client_digest(&self.abuse_key, client_identity) {
            abuse_controls.push(AbuseControl {
                scope: IP_SCOPE,
                identity_digest: digest,
                window_seconds: 86_400,
                maximum: 3,
            });
        }
        if let Some(digest) = subnet_digest(&self.abuse_key, client_identity) {
            abuse_controls.push(AbuseControl {
                scope: SUBNET_SCOPE,
                identity_digest: digest,
                window_seconds: 86_400,
                maximum: 25,
            });
        }
        let fee = UltraBlockchain::minimum_transfer_fee(self.config.claim_amount_base_units);
        let claim_id = random_claim_id();
        let outcome = self.store.admit_claim(
            &claim_id,
            &address,
            &self.abuse_key,
            idempotency_key,
            now,
            self.config.address_cooldown_seconds,
            self.config.claim_amount_base_units,
            fee,
            self.config.daily_debit_cap_base_units,
            self.config.max_queue_length,
            &abuse_controls,
        )?;
        match outcome {
            AdmissionOutcome::Created(bundle) => Ok((AdmissionResult::Created, bundle)),
            AdmissionOutcome::Existing(bundle) => Ok((AdmissionResult::Existing, bundle)),
            AdmissionOutcome::AddressCooldown {
                retry_after_seconds,
            } => Err(FaucetError::AddressCooldown(retry_after_seconds)),
            AdmissionOutcome::IdempotencyConflict => Err(FaucetError::IdempotencyConflict),
            AdmissionOutcome::Disabled => Err(FaucetError::Disabled),
            AdmissionOutcome::QueueFull => Err(FaucetError::QueueFull),
            AdmissionOutcome::BudgetExhausted => Err(FaucetError::BudgetExhausted),
            AdmissionOutcome::RateLimited {
                retry_after_seconds,
            } => Err(FaucetError::RateLimited(retry_after_seconds)),
        }
    }

    pub fn public_status(&self) -> Result<PublicStatusData, FaucetError> {
        let state = self.store.service_state()?;
        let availability = if !state.enabled {
            "disabled"
        } else {
            "available"
        };
        Ok(PublicStatusData {
            enabled: state.enabled,
            availability,
            claim_amount_base_units: self.config.claim_amount_base_units,
            claim_amount_ultra: UltraBlockchain::format_base_units(
                self.config.claim_amount_base_units,
            ),
            decimals: UltraBlockchain::ULTRA_DECIMALS,
            cooldown_seconds: self.config.address_cooldown_seconds,
        })
    }

    pub fn claim_data(&self, bundle: &ClaimBundle) -> crate::faucet::models::ClaimStatusData {
        crate::faucet::models::ClaimStatusData {
            claim_id: bundle.claim.claim_id.clone(),
            status: bundle.claim.status,
            amount_base_units: bundle.claim.amount_base_units,
            amount_ultra: UltraBlockchain::format_base_units(bundle.claim.amount_base_units),
            decimals: UltraBlockchain::ULTRA_DECIMALS,
            transaction_hash: bundle.payout.transaction_hash.map(hex::encode),
            submitted_at: bundle.claim.submitted_at,
            confirmed_at: bundle.claim.confirmed_at,
            failure_code: bundle.claim.failure_code.clone(),
        }
    }

    pub fn disable(&self, reason: &str) -> Result<(), FaucetError> {
        self.store
            .set_service_state(false, Some(reason), None, None)?;
        Ok(())
    }

    pub fn enable(&self) -> Result<(), FaucetError> {
        self.store.set_service_state(true, None, None, None)?;
        Ok(())
    }

    pub async fn run_worker(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = interval.tick() => {
                    if let Err(error) = self.process_next().await {
                        tracing::error!(error = %error, "faucet worker cycle failed");
                    }
                }
            }
        }
    }

    pub async fn process_next(&self) -> Result<bool, FaucetError> {
        let _worker_guard = self.worker_lock.lock().await;
        self.process_next_unlocked().await
    }

    async fn process_next_unlocked(&self) -> Result<bool, FaucetError> {
        let now = crate::auth::now_seconds();
        let Some(claim_id) = self.store.take_next_queued_claim(now)? else {
            return self.reconcile_pending(now).await;
        };
        let bundle = self
            .store
            .claim(&claim_id)?
            .ok_or(FaucetError::InvalidRequest)?;
        match self.submit_claim(bundle, now).await {
            Ok(()) => self.consecutive_node_failures.store(0, Ordering::Relaxed),
            Err(error) => {
                if matches!(error, FaucetError::NodeUnavailable)
                    && self
                        .consecutive_node_failures
                        .fetch_add(1, Ordering::Relaxed)
                        + 1
                        >= 3
                {
                    let _ = self.disable("node unavailable after repeated payout failures");
                }
                let envelope_persisted = self
                    .store
                    .claim(&claim_id)
                    .ok()
                    .flatten()
                    .is_some_and(|claim| claim.payout.signed_envelope.is_some());
                if !envelope_persisted {
                    let _ = self
                        .store
                        .requeue_submitting_without_envelope(&claim_id, now);
                    if matches!(
                        error,
                        FaucetError::SignerUnavailable
                            | FaucetError::SignerInvalid
                            | FaucetError::NodeRejected
                            | FaucetError::BudgetExhausted
                    ) {
                        let _ = self.disable("payout preflight failed; operator review required");
                    }
                }
                tracing::warn!(claim_id = %claim_id, error = %error, "faucet payout attempt failed");
            }
        }
        Ok(true)
    }

    async fn submit_claim(&self, bundle: ClaimBundle, now: u64) -> Result<(), FaucetError> {
        if !self.store.service_state()?.enabled {
            return Err(FaucetError::Disabled);
        }
        if bundle.payout.signed_envelope.is_some() {
            return self.retry_or_poll(bundle, now).await;
        }
        let account = self
            .node
            .account(&self.config.faucet_address)
            .await
            .map_err(map_node_error)?;
        let balance = account
            .balance_base_units()
            .ok_or(FaucetError::NodeUnavailable)?;
        if account.address != self.config.faucet_address
            || account.decimals != UltraBlockchain::ULTRA_DECIMALS
        {
            return Err(FaucetError::NodeUnavailable);
        }
        self.store.update_node_observation(account.nonce, now)?;
        let estimate = self
            .node
            .estimate(&bundle.claim.address, bundle.claim.amount_base_units)
            .await
            .map_err(map_node_error)?;
        if estimate.recipient != bundle.claim.address
            || estimate.amount != bundle.claim.amount_base_units
            || estimate.gas_limit != 500_000
            || estimate.gas_price != 1
            || estimate.total
                != estimate
                    .amount
                    .checked_add(estimate.fee)
                    .ok_or(FaucetError::NodeRejected)?
        {
            return Err(FaucetError::NodeRejected);
        }
        let fee = estimate.fee.max(UltraBlockchain::minimum_transfer_fee(
            bundle.claim.amount_base_units,
        ));
        let debit = bundle
            .claim
            .amount_base_units
            .checked_add(fee)
            .ok_or(FaucetError::NodeRejected)?;
        let required_balance = debit
            .checked_add(self.config.min_balance_reserve_base_units)
            .ok_or(FaucetError::BudgetExhausted)?;
        if balance < required_balance {
            return Err(FaucetError::BudgetExhausted);
        }
        if !self.store.update_fee_reservation(
            &bundle.claim.claim_id,
            fee,
            self.config.daily_debit_cap_base_units,
            now,
        )? {
            return Err(FaucetError::BudgetExhausted);
        }
        let (envelope, hash) = self
            .signer
            .sign_transfer(
                &bundle.claim.address,
                bundle.claim.amount_base_units,
                fee,
                account.nonce,
                now,
            )
            .map_err(map_signer_error)?;
        self.store
            .set_envelope(&bundle.claim.claim_id, &envelope, &hash, now)?;
        self.submit_envelope(&bundle.claim.claim_id, &envelope, &hash, now)
            .await
    }

    async fn retry_or_poll(&self, bundle: ClaimBundle, now: u64) -> Result<(), FaucetError> {
        let envelope = bundle
            .payout
            .signed_envelope
            .ok_or(FaucetError::SignerInvalid)?;
        let hash = bundle
            .payout
            .transaction_hash
            .ok_or(FaucetError::SignerInvalid)?;
        let age = now.saturating_sub(
            bundle
                .payout
                .submitted_at
                .or(bundle.claim.submitted_at)
                .unwrap_or(bundle.claim.created_at),
        );
        match self.node.transaction_status(&hex::encode(hash)).await {
            Ok(status) if !transaction_status_matches(&status, &hash) => {
                let _ = self.disable("node returned a mismatched payout hash");
                return Err(FaucetError::NodeRejected);
            }
            Ok(status) if status.status == "confirmed" => {
                self.store
                    .mark_confirmed(&bundle.claim.claim_id, &hash, now)?;
                return Ok(());
            }
            Ok(_) => {
                if age >= self.config.confirmation_timeout_seconds
                    || bundle.payout.attempt_count >= self.config.max_submission_attempts
                {
                    let _ = self.disable("payout confirmation timed out; operator review required");
                    return Err(FaucetError::ConfirmationTimeout);
                }
                return Ok(());
            }
            Err(NodeClientError::Unavailable) => return Err(FaucetError::NodeUnavailable),
            Err(NodeClientError::Rejected | NodeClientError::InvalidResponse) => {
                if age >= self.config.confirmation_timeout_seconds
                    || bundle.payout.attempt_count >= self.config.max_submission_attempts
                {
                    let _ = self.disable("payout reconciliation failed; operator review required");
                    return Err(FaucetError::ConfirmationTimeout);
                }
            }
            Err(NodeClientError::NotFound) => {
                if age >= self.config.confirmation_timeout_seconds
                    || bundle.payout.attempt_count >= self.config.max_submission_attempts
                {
                    let _ = self.disable("payout confirmation timed out; operator review required");
                    return Err(FaucetError::ConfirmationTimeout);
                }
            }
            Err(NodeClientError::HashMismatch) => return Err(FaucetError::NodeRejected),
        }
        if !self.store.service_state()?.enabled {
            return Ok(());
        }
        self.submit_envelope(&bundle.claim.claim_id, &envelope, &hash, now)
            .await
    }

    async fn submit_envelope(
        &self,
        claim_id: &str,
        envelope: &crate::faucet::models::SignedTransferRequest,
        hash: &[u8; 32],
        now: u64,
    ) -> Result<(), FaucetError> {
        match self.node.submit(envelope).await {
            Ok(result) => {
                if result.hash.to_ascii_lowercase() != hex::encode(hash) {
                    self.store.record_submission_attempt(
                        claim_id,
                        now,
                        Some("NODE_HASH_MISMATCH"),
                    )?;
                    return Err(FaucetError::NodeRejected);
                }
                self.store.mark_pending(claim_id, hash, now)?;
                if result.status == "confirmed" {
                    self.store.mark_confirmed(claim_id, hash, now)?;
                }
                Ok(())
            }
            Err(NodeClientError::Unavailable) => {
                self.store
                    .record_submission_attempt(claim_id, now, Some("NODE_UNAVAILABLE"))?;
                Err(FaucetError::NodeUnavailable)
            }
            Err(NodeClientError::Rejected) => {
                self.store
                    .record_submission_attempt(claim_id, now, Some("NODE_REJECTED"))?;
                match self.node.transaction_status(&hex::encode(hash)).await {
                    Ok(status) if !transaction_status_matches(&status, hash) => {
                        let _ = self.disable("node returned a mismatched payout hash");
                        Err(FaucetError::NodeRejected)
                    }
                    Ok(status) if status.status == "confirmed" => {
                        self.store.mark_confirmed(claim_id, hash, now)?;
                        Ok(())
                    }
                    Ok(_) => {
                        self.store.mark_pending(claim_id, hash, now)?;
                        Err(FaucetError::NodeRejected)
                    }
                    Err(NodeClientError::NotFound | NodeClientError::Rejected) => {
                        self.store.mark_failed(claim_id, "NODE_REJECTED", now)?;
                        Err(FaucetError::NodeRejected)
                    }
                    Err(NodeClientError::Unavailable) => Err(FaucetError::NodeUnavailable),
                    Err(NodeClientError::InvalidResponse | NodeClientError::HashMismatch) => {
                        Err(FaucetError::NodeRejected)
                    }
                }
            }
            Err(NodeClientError::InvalidResponse) => {
                self.store
                    .record_submission_attempt(claim_id, now, Some("NODE_REJECTED"))?;
                Err(FaucetError::NodeRejected)
            }
            Err(NodeClientError::NotFound) => {
                self.store
                    .record_submission_attempt(claim_id, now, Some("NODE_UNAVAILABLE"))?;
                Err(FaucetError::NodeUnavailable)
            }
            Err(NodeClientError::HashMismatch) => Err(FaucetError::NodeRejected),
        }
    }

    async fn reconcile_pending(&self, now: u64) -> Result<bool, FaucetError> {
        let pending = self.store.recover_pending_claims()?;
        if let Some(bundle) = pending.into_iter().next() {
            self.retry_or_poll(bundle, now).await?;
            return Ok(true);
        }
        Ok(false)
    }
}

fn transaction_status_matches(status: &NodeTransactionData, expected_hash: &[u8; 32]) -> bool {
    status
        .hash
        .eq_ignore_ascii_case(&hex::encode(expected_hash))
}

fn map_node_error(error: NodeClientError) -> FaucetError {
    match error {
        NodeClientError::Unavailable => FaucetError::NodeUnavailable,
        NodeClientError::Rejected
        | NodeClientError::InvalidResponse
        | NodeClientError::NotFound
        | NodeClientError::HashMismatch => FaucetError::NodeRejected,
    }
}

fn map_signer_error(error: SignerError) -> FaucetError {
    match error {
        SignerError::Unavailable | SignerError::AddressMismatch => FaucetError::SignerUnavailable,
        SignerError::InvalidCredential
        | SignerError::InvalidSignature
        | SignerError::InvalidEnvelope => FaucetError::SignerInvalid,
    }
}

fn random_claim_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faucet::{
        captcha::StaticCaptchaVerifier, config::FaucetConfig, node_client::NodeClient,
        signer::FaucetSigner, store::FaucetStore,
    };

    #[test]
    fn address_normalization_rejects_self_claims() {
        let keypair = crate::QuantumKeyPair::generate();
        let address = keypair.address();
        let signer = Arc::new(FaucetSigner::from_keypair_for_tests(keypair).unwrap());
        let config = FaucetConfig::for_tests("service-test.db".into(), address.clone());
        let store = FaucetStore::open_in_memory().unwrap();
        let node = Arc::new(NodeClient::new("http://127.0.0.1:8081".into()).unwrap());
        let service = FaucetService::new(
            config,
            store,
            signer,
            node,
            StaticCaptchaVerifier::new(true),
            Zeroizing::new(vec![1; 32]),
            Zeroizing::new(vec![2; 32]),
        )
        .unwrap();
        assert!(service.normalize_address(&address).is_err());
        assert_eq!(
            service
                .normalize_address(&format!(" {} ", "a".repeat(64)))
                .unwrap(),
            "a".repeat(64)
        );
        assert!(service
            .normalize_address(&format!("\u{2003}{}\u{2003}", "a".repeat(64)))
            .is_err());
    }
}
