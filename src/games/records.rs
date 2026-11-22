//! The Records Service: third-party games and every other application that anchors records.
//!
//! This is the half of the old combined service that owns the **chain owner's signing capability**. It
//! accepts records from applications, closes each day into one root, anchors it through `appchain-cli`, and
//! anchors first-party game days that the Games Reward Service asks it to (and only on the chains it
//! allows). It holds no payout key and has no code path that moves money; the Games Reward Service holds
//! the payout key and has no code path that signs an anchor. See `docs/APPCHAIN_SERVICE_ARCHITECTURE.md` §3.

use super::{
    anchor::{AnchorError, Anchorer},
    peer_usage::{PeerState, Reservation, SubmitDecision},
    chain::{ChainError, GamesChain},
    fence::FenceCheck,
    ownership,
    peer::PeerVerifier,
    registry::{self, RegistryCaps, RegistryDay, ResultInput},
    service::{date_start, shift_date, Clock, DailyReport, GamesError},
    store::GamesStore,
};
use crate::ultragrid;
use serde::{Deserialize, Serialize};

use super::service::date_of;

#[derive(Clone, Debug)]
pub struct RecordsConfig {
    /// The AppChain the registry's daily root is anchored to. Unset: days are closed (so a developer can
    /// still be given a proof) but not anchored.
    pub registry_chain_id: Option<u32>,
    /// Self-registration needs the owner's address to exist on the chain.
    pub require_chain_presence: bool,
    /// How many past days the daily run looks at.
    pub catch_up_days: u32,
    pub anchor_retry_seconds: u64,
    /// First-party and registry days only (the Games service's own pass): an anchor submitted but not seen on
    /// the ledger after this long is asked for again. The Records Service answers such a repeat from its own
    /// record and never signs a second time for a root whose outcome is unknown.
    pub anchor_resubmit_after_seconds: u64,
    /// The only chains a peer service (the Games Reward Service) may ask this one to anchor. Never the
    /// registry's own chain, never the L1: a compromised peer must not be able to anchor *as* the registry.
    pub peer_anchor_chains: Vec<u32>,
    /// The most distinct roots a peer may have anchored in one UTC day, across all chains. Every anchor
    /// costs the owner wallet a fee, so a compromised peer must not be able to drain it. A repeated ask
    /// for the same root, and a root already on the ledger, do not count. Kept in the database
    /// (`peer_usage.rs`), so a restart cannot reset it.
    pub peer_anchors_per_day: u32,
    /// The most times one root may be submitted in one UTC day: a root that keeps failing cannot be
    /// retried into a pile of fees.
    pub peer_submissions_per_root: u32,
    /// The owner key's address. Reconciling an anchor whose outcome is unknown means reading the owner's
    /// transaction history, so peer anchoring refuses to run without it.
    pub owner_address: Option<String>,
    /// How long `appchain-cli` may run: it signs, at the latest, this long after an attempt is recorded.
    pub anchor_timeout_seconds: u64,
}

impl RecordsConfig {
    pub fn new() -> RecordsConfig {
        RecordsConfig {
            registry_chain_id: None,
            require_chain_presence: true,
            catch_up_days: 7,
            anchor_retry_seconds: 600,
            anchor_resubmit_after_seconds: 3_600,
            peer_anchor_chains: vec![1, 2],
            peer_anchors_per_day: 8,
            peer_submissions_per_root: 3,
            owner_address: None,
            anchor_timeout_seconds: 60,
        }
    }
}

impl Default for RecordsConfig {
    fn default() -> Self {
        Self::new()
    }
}

enum InDoubtOutcome {
    /// Proved that the first attempt never reached the node.
    NotSubmitted,
    /// An operator's claim was verified against the mined anchor.
    Landed(String),
}

/// A registry day is one anchor; this only bounds a pathological loop of definite failures.
const REGISTRY_MAX_ATTEMPTS: u32 = 50;

/// What checking an operator's claim established.
#[derive(Debug, PartialEq, Eq)]
pub enum LandedCheck {
    /// Mined, signed by the owner, and the anchors index holds exactly this root, on exactly this chain, under
    /// exactly this transaction. Safe to record as landed.
    Verified(String),
    /// Pending: owner and AppChain are checked, but chain and root cannot be read yet. This is a **claim only**.
    /// It must never be recorded as landed; the service verifies it itself once the transaction is mined.
    PendingClaim(String),
}

/// Check an operator's claim that `tx_hash` is the anchor of `root` on `chain_id`, signed by `owner`, before it
/// is recorded as landed. The node reports a transaction's sender, recipient and status but not its payload,
/// so what can be established depends on its status:
///
/// * **confirmed:** it must be the owner's AppChain transaction **and** the anchors index must hold exactly this
///   root, on exactly this chain, under exactly this transaction. Anything less is refused.
/// * **pending:** the sender and recipient are checked, but chain and root cannot be read until it is mined, so
///   it is refused unless the operator states they have checked that (`accept_pending_unverified`).
///
/// The `--pending-unverified-ok` flag (`accept_pending_unverified`) only lets a **pending** transaction be
/// recorded as a claim. It changes nothing for a mined one, which is always verified in full.
pub async fn validate_landed_transaction(
    chain: &dyn GamesChain,
    owner: &str,
    chain_id: u32,
    root: &[u8; 32],
    tx_hash: &str,
    accept_pending_unverified: bool,
) -> Result<LandedCheck, String> {
    let tx = chain
        .transaction(tx_hash)
        .await
        .map_err(|_| "the node could not be asked about that transaction".to_string())?
        .ok_or("the node has no such transaction, mined or pending")?;
    if tx.sender != owner {
        return Err(format!("that transaction was signed by {}, not by the owner {owner}", tx.sender));
    }
    if tx.recipient != "appchain" {
        return Err("that transaction is not an AppChain transaction".into());
    }
    if tx.pending {
        if !accept_pending_unverified {
            return Err("it is still pending, so its chain and root cannot be read yet. Wait for it to be mined, or pass --pending-unverified-ok after checking the node yourself".into());
        }
        return Ok(LandedCheck::PendingClaim(
            "the owner's pending AppChain transaction; its chain and root were NOT verified. It is recorded as a claim only: the root stays in doubt and is not resubmitted, and the service verifies the claim itself once the transaction is mined and indexed".into(),
        ));
    }
    match chain.find_anchor(chain_id, root).await {
        Ok(Some(point)) if point.tx_hash.eq_ignore_ascii_case(tx_hash) => {
            Ok(LandedCheck::Verified(format!("verified: the owner's confirmed anchor of this root on chain {chain_id} (sequence {})", point.sequence)))
        }
        Ok(Some(_)) => Err("the ledger holds this root on this chain under a different transaction".into()),
        Ok(None) => Err("it is confirmed, but the anchors index does not show this root on this chain under it: wait for the index to catch up, or it is not this anchor".into()),
        Err(_) => Err("the ledger could not be read to verify the anchor".into()),
    }
}

fn unusable(error: super::store::StoreError) -> GamesError {
    tracing::error!(error = %error, "peer anchor accounting is unusable; refusing to anchor");
    GamesError::Internal(error.to_string())
}

fn in_doubt() -> GamesError {
    GamesError::AnchorFailed(
        "an earlier submission of this root is in doubt; it is not repeated automatically: see the runbook's in-doubt procedure"
            .into(),
    )
}

pub struct RecordsService {
    store: GamesStore,
    caps: RegistryCaps,
    config: RecordsConfig,
    chain: std::sync::Arc<dyn GamesChain>,
    anchorer: Option<std::sync::Arc<dyn Anchorer>>,
    peer: Option<PeerVerifier>,
    clock: Clock,
    anchor_lock: tokio::sync::Mutex<()>,
}

#[derive(Debug, Serialize)]
pub struct PeerAnchorReply {
    pub chain_id: u32,
    pub root: String,
    pub tx_hash: String,
    /// `confirmed`: already on the ledger. `submitted`: accepted as pending a block.
    pub state: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RecordsStatus {
    pub accepting: bool,
    pub date: String,
    pub registry_chain_id: Option<u32>,
    pub anchoring: bool,
    /// Anchoring is held after a restore until it has been reconciled with the ledger.
    pub held: bool,
}

impl RecordsService {
    pub fn new(
        store: GamesStore,
        caps: RegistryCaps,
        config: RecordsConfig,
        chain: std::sync::Arc<dyn GamesChain>,
        anchorer: Option<std::sync::Arc<dyn Anchorer>>,
        peer: Option<PeerVerifier>,
        clock: Clock,
    ) -> RecordsService {
        RecordsService {
            store,
            caps,
            config,
            chain,
            anchorer,
            peer,
            clock,
            anchor_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn store(&self) -> &GamesStore {
        &self.store
    }

    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    pub fn today(&self) -> String {
        date_of(self.now())
    }

    pub fn peer(&self) -> Option<&PeerVerifier> {
        self.peer.as_ref()
    }

    fn require_accepting(&self) -> Result<(), GamesError> {
        if self.store.state()?.accepting {
            Ok(())
        } else {
            Err(GamesError::Disabled)
        }
    }

    pub fn status(&self) -> Result<RecordsStatus, GamesError> {
        Ok(RecordsStatus {
            accepting: self.store.state()?.accepting,
            date: self.today(),
            registry_chain_id: self.config.registry_chain_id,
            anchoring: self.anchorer.is_some(),
            held: self.store.peer_hold()?.is_some(),
        })
    }

    /// The daily pass: close and anchor the registry's finished days. Safe to run any number of times.
    pub async fn run_daily(&self) -> Result<DailyReport, GamesError> {
        let _guard = self.anchor_lock.lock().await;
        let mut report = DailyReport {
            anchored: vec![],
            confirmed: vec![],
            errors: vec![],
        };
        let today = self.today();
        let now = self.now();
        self.registry_daily(&mut report, &today, now).await?;
        Ok(report)
    }

    /// Close and anchor the registry's days every ten minutes until told to stop.
    pub async fn run_worker(
        self: std::sync::Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut last_daily = 0u64;
        loop {
            if *shutdown.borrow() {
                break;
            }
            let now = self.now();
            if now.saturating_sub(last_daily) >= 600 {
                last_daily = now;
                match self.run_daily().await {
                    Ok(report) => {
                        for (game, date, error) in &report.errors {
                            tracing::warn!(game, date, error, "daily anchor step failed");
                        }
                    }
                    Err(error) => tracing::warn!(error = %error, "daily run failed"),
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
                _ = shutdown.changed() => {}
            }
        }
    }

    /// Everything that must be true before this service signs ANY anchor, a peer's or the registry's.
    async fn require_anchoring_allowed(&self, owner: &str, day: &str, now: u64) -> Result<(), GamesError> {
        if let Some(note) = self.store.peer_hold().map_err(unusable)? {
            return Err(GamesError::ReconciliationRequired(format!(
                "anchoring is held after a restore ({note}); run `ultranet-records peer-reconcile`"
            )));
        }
        match self.store.check_fence().map_err(unusable)? {
            FenceCheck::Intact => {}
            FenceCheck::Regressed(why) => {
                // Held durably, so a restart does not forget it, until an operator reconciles.
                self.store
                    .set_peer_hold(&format!("automatic: {why}"), now)
                    .map_err(unusable)?;
                return Err(GamesError::ReconciliationRequired(format!(
                    "this database looks restored from a backup ({why}); anchoring is held: run `ultranet-records peer-reconcile`"
                )));
            }
        }
        self.require_ledger_known(day, now).await?;
        self.require_owner_history_accounted(owner, day, now).await
    }

    /// The owner's AppChain transactions since yesterday must not outnumber the signer attempts this database
    /// recorded. Every transaction this service signs is preceded by a counted attempt, so more transactions
    /// than attempts means either the owner key was used elsewhere or the database has forgotten attempts
    /// (a restore), including an anchor that is mined while the anchors index still lags. The confirmed
    /// history comes from the chain itself, not from that index.
    ///
    /// Horizon: since 00:00 UTC yesterday (or the operator's last reconciliation, if later). Older
    /// discrepancies are not detected here. See `docs/runbooks/records-service.md` §5.1.
    async fn require_owner_history_accounted(&self, owner: &str, day: &str, _now: u64) -> Result<(), GamesError> {
        let Some(today_start) = date_start(day) else {
            return Err(GamesError::Internal("today is not a date".into()));
        };
        let since = today_start.saturating_sub(86_400);
        let (ack_ts, ack_attempts) = self.store.owner_ack().map_err(unusable)?;
        let (floor, from_day, baseline) = if ack_ts >= since {
            (ack_ts, date_of(ack_ts), ack_attempts)
        } else {
            (since, date_of(since), 0)
        };
        let activity = self
            .chain
            .owner_appchain_activity(owner, since)
            .await
            .map_err(|_| GamesError::ChainUnavailable)?;
        if !activity.confirmed_complete {
            return Err(GamesError::ReconciliationRequired(
                "the owner's confirmed history does not reach back far enough to be checked; reconcile with `ultranet-records peer-reconcile`".into(),
            ));
        }
        let transactions = activity
            .transactions
            .iter()
            .filter(|tx| if ack_ts >= since { tx.timestamp > ack_ts } else { tx.timestamp >= floor })
            .count() as u64;
        let mut days = Vec::new();
        let mut cursor = from_day;
        loop {
            days.push(cursor.clone());
            if cursor.as_str() >= day || days.len() > 4 {
                break;
            }
            cursor = shift_date(&cursor, 1).unwrap_or_else(|| day.to_string());
        }
        let day_refs: Vec<&str> = days.iter().map(String::as_str).collect();
        let recorded = self
            .store
            .recorded_attempts(&day_refs)
            .map_err(unusable)?
            .saturating_sub(baseline);
        if transactions > recorded {
            return Err(GamesError::ReconciliationRequired(format!(
                "the owner has signed {transactions} AppChain transaction(s) since {} but this database recorded only {recorded} attempt(s): it may have been restored from an older backup, or the owner key was used elsewhere. Reconcile with `ultranet-records peer-reconcile --acknowledge-owner-transactions`",
                date_of(floor)
            )));
        }
        Ok(())
    }

    /// Anchor a closed registry day under the same fail-closed rules as a peer's anchor. `Err(message)` means
    /// nothing was signed and `message` is recorded on the day for the operator.
    async fn anchor_registry_day(
        &self,
        day: &RegistryDay,
        chain_id: u32,
        anchorer: &std::sync::Arc<dyn Anchorer>,
        now: u64,
    ) -> Result<(), String> {
        let date = day.date.as_str();
        let Some(owner) = self.config.owner_address.clone() else {
            return Err("RECORDS_OWNER_ADDRESS is required to anchor the registry day".into());
        };
        let text = |e: GamesError| e.to_string();
        // Look before submitting: a root already on the ledger is simply confirmed.
        match self.chain.find_anchor(chain_id, &day.day_root).await {
            Ok(Some(point)) => {
                self.store
                    .set_registry_anchor(date, Some(&point.tx_hash), "confirmed", now)
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
            Ok(None) => {}
            Err(_) => return Err("the ledger could not be read to check the anchor".into()),
        }
        self.require_anchoring_allowed(&owner, &self.today(), now).await.map_err(text)?;
        let row = self
            .store
            .ensure_registry_anchor(date, chain_id, &day.day_root, now)
            .map_err(|e| e.to_string())?;
        let sync = |tx: &str| {
            self.store
                .set_registry_anchor(date, Some(tx), "anchored", now)
                .map_err(|e| e.to_string())
        };
        match row.state {
            // Accepted by the node earlier: never submitted again, whatever time has passed.
            PeerState::Submitted => return sync(&row.tx_hash.unwrap_or_default()),
            PeerState::Submitting => {
                match self
                    .settle_in_doubt(&owner, chain_id, &day.day_root, row.updated_at, row.tx_hash.as_deref(), now)
                    .await
                    .map_err(text)?
                {
                    InDoubtOutcome::Landed(tx) => {
                        self.store
                            .finish_registry_submission(date, Ok(&tx), now)
                            .map_err(|e| e.to_string())?;
                        return sync(&tx);
                    }
                    InDoubtOutcome::NotSubmitted => {
                        let reset = self
                            .store
                            .mark_registry_not_submitted(
                                date,
                                row.updated_at,
                                "reconciled: the owner has no AppChain transaction in the attempt's window",
                            )
                            .map_err(|e| e.to_string())?;
                        if !reset {
                            return Err(in_doubt().to_string());
                        }
                    }
                }
            }
            PeerState::Reserved => {}
        }
        self.require_no_pending_owner_tx(&owner, now).await.map_err(text)?;
        match self
            .store
            .begin_registry_submission(date, REGISTRY_MAX_ATTEMPTS, now)
            .map_err(|e| e.to_string())?
        {
            SubmitDecision::Wait { tx_hash } => return sync(&tx_hash),
            SubmitDecision::InDoubt { .. } => return Err(in_doubt().to_string()),
            SubmitDecision::Exhausted { attempts } => {
                return Err(format!("this day has been submitted {attempts} times; resolve it with `ultranet-records registry-resolve`"))
            }
            SubmitDecision::Proceed { .. } => {}
        }
        match anchorer.anchor(chain_id, &day.day_root).await {
            Ok(tx) => {
                self.store
                    .finish_registry_submission(date, Ok(&tx), self.now())
                    .map_err(|e| e.to_string())?;
                sync(&tx)
            }
            Err(error) => {
                let definite = matches!(error, AnchorError::Spawn(_));
                self.store
                    .finish_registry_submission(date, Err((&error.to_string(), definite)), self.now())
                    .map_err(|e| e.to_string())?;
                Err(error.to_string())
            }
        }
    }

    /// Refuse to sign while the ledger shows anchors, on the chains a peer may use since yesterday, that
    /// this database has no record of. Adopting them (`peer-reconcile --apply`) is what restores the
    /// allowance they used.
    async fn require_ledger_known(&self, day: &str, _now: u64) -> Result<(), GamesError> {
        let Some(today_start) = date_start(day) else {
            return Err(GamesError::Internal("today is not a date".into()));
        };
        let since = today_start.saturating_sub(86_400);
        let yesterday = date_of(since);
        let known = self
            .store
            .peer_known_roots(&[day, &yesterday])
            .map_err(unusable)?;
        let registry_known = self.store.registry_known_roots().map_err(unusable)?;
        let mut chains: Vec<(u32, bool)> = self.config.peer_anchor_chains.iter().map(|c| (*c, false)).collect();
        if let Some(registry_chain) = self.config.registry_chain_id {
            chains.push((registry_chain, true));
        }
        for (chain_id, is_registry) in chains {
            let anchors = self
                .chain
                .anchors_since(chain_id, since)
                .await
                .map_err(|_| GamesError::ChainUnavailable)?;
            let set = if is_registry { &registry_known } else { &known };
            if let Some(point) = anchors.iter().find(|p| !set.contains(&(chain_id, p.root))) {
                return Err(GamesError::ReconciliationRequired(format!(
                    "the ledger shows an anchor on chain {chain_id} (root {}…, {}) that this database has no record of; \
                     run `ultranet-records peer-reconcile --apply`",
                    hex::encode(&point.root[..6]),
                    point.tx_hash.get(..12).unwrap_or(&point.tx_hash)
                )));
            }
        }
        Ok(())
    }

    /// Refuse to submit on top of a pending transaction of the owner's.
    ///
    /// **The node does not protect against this.** On the real node the account nonce counts pending transactions,
    /// so the CLI signs a second transaction with the next nonce and the node accepts it: a duplicate pays a second
    /// fee (shown by `tools/verify/records-rehearsal.sh`). This check is therefore the only thing between a pending
    /// anchor and another one.
    ///
    /// The node's address history lists pending transactions only if the confirmed history leaves room in its
    /// 100-entry list. When it does not, they are hidden, so the check uses a second measure that is always
    /// available: the account's nonce counts pending transactions and the newest confirmed one the owner sent is
    /// always listed, so `account nonce − (that nonce + 1)` is exactly how many are pending. Anything that leaves
    /// the answer unknown refuses.
    async fn require_no_pending_owner_tx(&self, owner: &str, now: u64) -> Result<(), GamesError> {
        let activity = self
            .chain
            .owner_appchain_activity(owner, now.saturating_sub(7_200))
            .await
            .map_err(|_: ChainError| GamesError::ChainUnavailable)?;
        let pending_msg = || {
            GamesError::AnchorFailed("a transaction from the owner is still pending; try again later".into())
        };
        if activity.transactions.iter().any(|tx| tx.pending) {
            return Err(pending_msg());
        }
        if activity.complete {
            return Ok(()); // the whole pending list was shown, and it holds none
        }
        // The pending list may be hidden. Count them by the nonce instead.
        let Some(confirmed_next) = activity.confirmed_next_nonce else {
            return Err(GamesError::AnchorFailed(
                "the node's history cannot show whether the owner has a pending transaction, and no confirmed transaction of the owner's is listed to measure against; refusing until that can be established".into(),
            ));
        };
        let account_nonce = self
            .chain
            .account_nonce(owner)
            .await
            .map_err(|_: ChainError| GamesError::ChainUnavailable)?;
        if account_nonce > confirmed_next {
            return Err(pending_msg());
        }
        if account_nonce < confirmed_next {
            return Err(GamesError::AnchorFailed(
                "the owner's account nonce is behind its newest confirmed transaction: the node's answers disagree, so nothing is submitted".into(),
            ));
        }
        Ok(())
    }

    /// Decide whether a root left in `submitting` can be shown never to have reached the node.
    ///
    /// The attempt was recorded at `begun_at`, before `appchain-cli` ran, and the CLI signs with the time
    /// it runs, so any transaction it made carries a timestamp in `[begun_at - margin, begun_at + timeout +
    /// margin]`. If, after that window has closed, the owner's history (pending and confirmed, read
    /// completely) holds **no** AppChain transaction in it, the attempt did not reach the node and the root
    /// returns to `reserved`. Anything less than that proof, a transaction in the window, a history that does
    /// not reach back, a node that cannot be read, is `InDoubt`, and nothing is submitted.
    async fn reconcile_in_doubt(
        &self,
        owner: &str,
        day: &str,
        chain_id: u32,
        root: &[u8; 32],
        begun_at: u64,
        claim: Option<&str>,
        now: u64,
    ) -> Result<Option<String>, GamesError> {
        match self.settle_in_doubt(owner, chain_id, root, begun_at, claim, now).await? {
            InDoubtOutcome::Landed(tx) => {
                self.store
                    .finish_peer_submission(day, chain_id, root, Ok(&tx), now)
                    .map_err(unusable)?;
                Ok(Some(tx))
            }
            InDoubtOutcome::NotSubmitted => {
                let reset = self
                    .store
                    .mark_peer_not_submitted(
                        day,
                        chain_id,
                        root,
                        begun_at,
                        "reconciled: the owner has no AppChain transaction in the attempt's window",
                    )
                    .map_err(unusable)?;
                if reset {
                    Ok(None)
                } else {
                    Err(in_doubt())
                }
            }
        }
    }

    /// Settle a `submitting` root or registry day, or leave it in doubt.
    ///
    /// *No claim:* the proof of non-submission (below). *An operator's claim of a pending transaction:* the
    /// claim is never accepted on its word. Only a transaction that is now **mined**, signed by the owner, and
    /// shown by the anchors index as exactly this root on exactly this chain under exactly this hash settles it,
    /// as landed. A claim whose transaction is still pending, unknown to the node, mined but not (yet) shown, or
    /// shown as something else, stays in doubt with the reason, is never resubmitted, and is never proved
    /// absent: the operator resolves it with `--not-submitted` when they have seen enough.
    async fn settle_in_doubt(
        &self,
        owner: &str,
        chain_id: u32,
        root: &[u8; 32],
        begun_at: u64,
        claim: Option<&str>,
        now: u64,
    ) -> Result<InDoubtOutcome, GamesError> {
        let Some(tx_hash) = claim else {
            self.prove_not_submitted(owner, begun_at, now).await?;
            return Ok(InDoubtOutcome::NotSubmitted);
        };
        let doubt = |why: &str| {
            GamesError::AnchorFailed(format!(
                "an operator named transaction {}… for this anchor but it is not verified: {why}; it is not repeated automatically (resolve it with --not-submitted once you are sure)",
                tx_hash.get(..12).unwrap_or(tx_hash)
            ))
        };
        let tx = self
            .chain
            .transaction(tx_hash)
            .await
            .map_err(|_| GamesError::ChainUnavailable)?
            .ok_or_else(|| doubt("the node does not know that transaction"))?;
        if tx.pending {
            return Err(doubt("it is still pending"));
        }
        if tx.sender != owner || tx.recipient != "appchain" {
            return Err(doubt("it is not an AppChain transaction signed by the owner"));
        }
        match self.chain.find_anchor(chain_id, root).await {
            Ok(Some(point)) if point.tx_hash.eq_ignore_ascii_case(tx_hash) => {
                Ok(InDoubtOutcome::Landed(tx_hash.to_ascii_lowercase()))
            }
            Ok(Some(_)) => Err(doubt("the ledger holds this root under a different transaction")),
            Ok(None) => Err(doubt("it is mined but the anchors index does not show this root on this chain under it (yet)")),
            Err(_) => Err(GamesError::ChainUnavailable),
        }
    }

    /// The proof itself, for a peer's root or a registry day alike (see `reconcile_in_doubt`'s contract).
    async fn prove_not_submitted(&self, owner: &str, begun_at: u64, now: u64) -> Result<(), GamesError> {
        const MARGIN: u64 = 120;
        let window_start = begun_at.saturating_sub(MARGIN);
        let window_end = begun_at + self.config.anchor_timeout_seconds + MARGIN;
        if now < window_end + MARGIN {
            return Err(in_doubt());
        }
        let activity = self
            .chain
            .owner_appchain_activity(owner, window_start)
            .await
            .map_err(|_| GamesError::ChainUnavailable)?;
        let in_window = activity
            .transactions
            .iter()
            .any(|tx| tx.timestamp >= window_start && tx.timestamp <= window_end);
        if !activity.complete || in_window {
            return Err(in_doubt());
        }
        Ok(())
    }

    /// Anchor a first-party game's day root on behalf of the Games Reward Service. The caller has already
    /// authenticated (see `peer.rs`); what it may ask for is limited here, because an authenticated peer
    /// that has been compromised still holds a valid secret.
    ///
    /// Looks at the ledger before submitting: a root already on the chain is answered with its transaction,
    /// never anchored twice. If the ledger cannot be read nothing is submitted.
    pub async fn anchor_for_peer(
        &self,
        chain_id: u32,
        root: [u8; 32],
    ) -> Result<PeerAnchorReply, GamesError> {
        if chain_id == 0
            || Some(chain_id) == self.config.registry_chain_id
            || !self.config.peer_anchor_chains.contains(&chain_id)
        {
            return Err(GamesError::Forbidden(
                "that chain may not be anchored on a peer's behalf".into(),
            ));
        }
        self.require_accepting()?;
        let Some(anchorer) = &self.anchorer else {
            return Err(GamesError::AnchoringDisabled);
        };
        let _guard = self.anchor_lock.lock().await;
        let now = self.now();
        let reply = |tx_hash: String, state: &'static str| PeerAnchorReply {
            chain_id,
            root: hex::encode(root),
            tx_hash,
            state,
        };
        match self.chain.find_anchor(chain_id, &root).await {
            Ok(Some(point)) => return Ok(reply(point.tx_hash, "confirmed")),
            Ok(None) => {}
            Err(_) => return Err(GamesError::ChainUnavailable),
        }

        let Some(owner) = self.config.owner_address.clone() else {
            return Err(GamesError::Internal(
                "peer anchoring needs RECORDS_OWNER_ADDRESS to reconcile ambiguous anchors".into(),
            ));
        };

        // 1. Everything that must hold before ANY anchor is signed: not held after a restore, the database is
        //    the one the fence describes, the ledger shows no anchor it has forgotten, and the owner has signed
        //    no more transactions than it recorded attempts for.
        let day = self.today();
        self.require_anchoring_allowed(&owner, &day, now).await?;

        // 1b. The same root recorded on an earlier day is the same anchor, not a new one: a day boundary is not
        //     evidence about a transaction either. A submitted one is answered from its record; one whose
        //     outcome is unknown is reconciled (or stays in doubt), whatever day it was asked on.
        let history = self.store.peer_root_history(chain_id, &root).map_err(unusable)?;
        for prior in history.into_iter().filter(|r| r.day != day) {
            match prior.state {
                PeerState::Submitted => {
                    return Ok(reply(prior.tx_hash.unwrap_or_default(), "submitted"));
                }
                PeerState::Submitting => {
                    if let Some(tx) = self
                        .reconcile_in_doubt(&owner, &prior.day, chain_id, &root, prior.updated_at, prior.tx_hash.as_deref(), now)
                        .await?
                    {
                        return Ok(reply(tx, "submitted"));
                    }
                }
                PeerState::Reserved => {}
            }
        }

        // 2. Take a slot for this root, durably, BEFORE anything is submitted. A root already reserved
        //    today takes none. If the stored state is unusable, nothing is submitted (never repaired).
        let row = match self
            .store
            .reserve_peer_anchor(&day, chain_id, &root, self.config.peer_anchors_per_day, now)
            .map_err(unusable)?
        {
            Reservation::LimitReached { .. } => {
                return Err(GamesError::Forbidden(
                    "the daily limit of anchors a peer may request has been reached".into(),
                ));
            }
            Reservation::Granted(row) | Reservation::Existing(row) => row,
        };

        // 3. A submission whose outcome is unknown is never repeated because time has passed. It is
        //    repeated only if reconciliation PROVES the first attempt never reached the node.
        if row.state == PeerState::Submitting {
            if let Some(tx) = self
                .reconcile_in_doubt(&owner, &day, chain_id, &root, row.updated_at, row.tx_hash.as_deref(), now)
                .await?
            {
                return Ok(reply(tx, "submitted"));
            }
        }
        // Never submit on top of a transaction of the owner's that is still pending. The node would accept it.
        if row.state != PeerState::Submitted {
            self.require_no_pending_owner_tx(&owner, now).await?;
        }

        // 4. Record the attempt, durably, BEFORE the signer is called.
        match self
            .store
            .begin_peer_submission(&day, chain_id, &root, self.config.peer_submissions_per_root, now)
            .map_err(unusable)?
        {
            SubmitDecision::Wait { tx_hash } => return Ok(reply(tx_hash, "submitted")),
            SubmitDecision::InDoubt { .. } => return Err(in_doubt()),
            SubmitDecision::Exhausted { .. } => {
                return Err(GamesError::Forbidden(
                    "this root has been submitted the most times it may be today".into(),
                ))
            }
            SubmitDecision::Proceed { .. } => {}
        }

        // 5. Submit, then record how it ended. Only a failure to RUN the signer at all is known not to
        //    have submitted anything. Any other failure (the signer ran and the node did not answer,
        //    a timeout, unreadable output) may have been an accepted transaction, so the root stays in
        //    doubt until it is reconciled.
        match anchorer.anchor(chain_id, &root).await {
            Ok(tx) => {
                self.store
                    .finish_peer_submission(&day, chain_id, &root, Ok(&tx), self.now())
                    .map_err(unusable)?;
                Ok(reply(tx, "submitted"))
            }
            Err(error) => {
                let definite = matches!(error, AnchorError::Spawn(_));
                self.store
                    .finish_peer_submission(
                        &day,
                        chain_id,
                        &root,
                        Err((&error.to_string(), definite)),
                        self.now(),
                    )
                    .map_err(unusable)?;
                Err(GamesError::AnchorFailed(error.to_string()))
            }
        }
    }
}

// --- the registry: third-party games ----------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryResultRequest {
    pub player: String,
    pub result_id: String,
    #[serde(default)]
    pub score: u64,
    /// 64 hex characters: a hash of whatever the game wants to be able to prove later. Optional.
    #[serde(default)]
    pub data_hash: Option<String>,
}

/// A developer registering their own game. The owner is not named: it is the address the public key
/// hashes to, so naming one that is not yours is not possible.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryRegisterRequest {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryRotateRequest {
    pub public_key: String,
    pub timestamp: u64,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryKeyResponse {
    pub game: RegistryGameView,
    /// Shown once. Only its hash is stored.
    pub api_key: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryResultResponse {
    pub game_id: String,
    pub result_id: String,
    pub day: String,
    /// The same result had already been recorded; nothing was added.
    pub duplicate: bool,
    pub status: &'static str,
    /// Whether this result will be part of the day's anchored root. A sandbox game's never is.
    pub anchored: bool,
}

#[derive(Debug, Serialize)]
pub struct RegistryGameView {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RegistryGameRootView {
    pub id: String,
    pub root: String,
}

#[derive(Debug, Serialize)]
pub struct RegistryDayView {
    pub date: String,
    pub merkle_root: String,
    /// The 32 bytes the chain's owner key anchors.
    pub day_root: String,
    pub state: String,
    pub anchor_tx: Option<String>,
    pub chain_id: Option<u32>,
    pub games: Vec<RegistryGameRootView>,
}

#[derive(Debug, Serialize)]
pub struct ProofStepView {
    pub sibling: String,
    /// `left` or `right`: which side the sibling is on when the two are hashed together.
    pub side: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RegistryProofView {
    pub game_id: String,
    pub date: String,
    pub game_root: String,
    pub leaf: String,
    pub steps: Vec<ProofStepView>,
    pub merkle_root: String,
    pub day_root: String,
    pub state: String,
    pub anchor_tx: Option<String>,
    pub chain_id: Option<u32>,
}

impl RecordsService {
    /// Record a result a third-party game reports. The game proves who it is with its API key, which
    /// may submit results for **that game only**. No money moves here: a result is recorded, and a
    /// registered game's results are folded into the day's anchored root.
    pub fn submit_registry_result(
        &self,
        game_id: &str,
        key: &str,
        request: RegistryResultRequest,
    ) -> Result<RegistryResultResponse, GamesError> {
        self.require_accepting()?;
        let game = self.store.authenticate(game_id, key)?;
        let data_hash = match request.data_hash.as_deref() {
            None => [0u8; 32],
            Some(text) => hex::decode(text)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| {
                    GamesError::Invalid("data_hash must be 64 hexadecimal characters".into())
                })?,
        };
        let input = ResultInput {
            player: request.player.trim().to_ascii_lowercase(),
            result_id: request.result_id,
            score: request.score,
            data_hash,
        };
        let day = self.today();
        let (row, duplicate) =
            self.store
                .record_result(&game.id, &input, &day, &self.caps, self.now())?;
        Ok(RegistryResultResponse {
            game_id: game.id,
            result_id: row.result_id,
            day: row.day,
            duplicate,
            status: game.status.code(),
            anchored: game.status.is_anchored(),
        })
    }

    fn game_view(game: registry::GameRecord) -> RegistryGameView {
        RegistryGameView {
            id: game.id,
            name: game.name,
            owner: game.owner,
            status: game.status.code(),
        }
    }

    /// Register a game on the owner's signed request. The game starts in the sandbox, as every game
    /// does; the operator decides when it is promoted. Costs the developer nothing but is bounded per
    /// owner and per day, and the owner's address must already exist on the chain, so each game costs
    /// its maker at least one faucet claim or a payment received.
    pub async fn self_register(
        &self,
        request: RegistryRegisterRequest,
    ) -> Result<RegistryKeyResponse, GamesError> {
        let now = self.now();
        let name = registry::validate_game_name(&request.name)?;
        registry::validate_game_id(&request.id)?;
        let owner = ownership::verify_owner_proof(
            ownership::ACTION_REGISTER,
            &request.id,
            &name,
            request.timestamp,
            &request.public_key,
            &request.signature,
            now,
        )?;
        if self.config.require_chain_presence && !self.chain.address_active(&owner).await? {
            return Err(GamesError::Invalid(
                "the owner address has no activity on UltraNet yet: claim from the faucet first".into(),
            ));
        }
        let day_start = now - now % 86_400;
        let (game, api_key) = self.store.register_game_self_service(
            &request.id,
            &name,
            &owner,
            day_start,
            &self.caps,
            now,
        )?;
        Ok(RegistryKeyResponse {
            game: Self::game_view(game),
            api_key,
        })
    }

    /// Replace a game's key on its owner's signed request: the recovery path for a lost key, with no
    /// operator involved. The old key stops working at once.
    pub fn self_rotate_key(
        &self,
        id: &str,
        request: RegistryRotateRequest,
    ) -> Result<RegistryKeyResponse, GamesError> {
        let now = self.now();
        let owner = ownership::verify_owner_proof(
            ownership::ACTION_ROTATE_KEY,
            id,
            "",
            request.timestamp,
            &request.public_key,
            &request.signature,
            now,
        )?;
        let api_key = self.store.rotate_key_signed(id, &owner, request.timestamp, now)?;
        let game = self.store.game(id)?.ok_or(GamesError::InvalidKey)?;
        Ok(RegistryKeyResponse {
            game: Self::game_view(game),
            api_key,
        })
    }

    /// A game's public page. A sandbox game is private and a suspended one is gone from view: both read
    /// as "no such game", so a stranger cannot learn that an unreleased game exists.
    pub fn registry_game(&self, id: &str) -> Result<RegistryGameView, GamesError> {
        match self.store.game(id)? {
            Some(game) if game.status.is_anchored() => Ok(Self::game_view(game)),
            _ => Err(GamesError::NotFound("no such game".into())),
        }
    }

    /// The curated directory: only games the operator has listed.
    pub fn registry_listed(&self) -> Result<Vec<RegistryGameView>, GamesError> {
        Ok(self
            .store
            .games()?
            .into_iter()
            .filter(|game| game.status == registry::GameStatus::Listed)
            .map(Self::game_view)
            .collect())
    }

    fn day_arg(date: &str) -> Result<(), GamesError> {
        if ultragrid::valid_date(date) {
            Ok(())
        } else {
            Err(GamesError::Invalid("date must look like 2026-11-16".into()))
        }
    }

    pub fn registry_day_view(&self, date: &str) -> Result<RegistryDayView, GamesError> {
        Self::day_arg(date)?;
        let day = self
            .store
            .registry_day(date)?
            .ok_or_else(|| GamesError::NotFound("that day has not been closed yet".into()))?;
        Ok(RegistryDayView {
            date: day.date,
            merkle_root: hex::encode(day.merkle_root),
            day_root: hex::encode(day.day_root),
            state: day.state,
            anchor_tx: day.anchor_tx,
            chain_id: self.config.registry_chain_id,
            games: day
                .games
                .iter()
                .map(|(id, root)| RegistryGameRootView {
                    id: id.clone(),
                    root: hex::encode(root),
                })
                .collect(),
        })
    }

    /// The public day list: newest first, with an exclusive date cursor. A caller that wants `limit`
    /// rows asks for `limit + 1` so it can tell whether more exist.
    pub fn registry_days_page(
        &self,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Vec<registry::RegistryDaySummary>, GamesError> {
        if let Some(before) = before {
            Self::day_arg(before)?;
        }
        Ok(self.store.registry_day_page(before, limit)?)
    }

    /// Counts over **every** stored day — one window for all of them, never a rolling one, and never
    /// influenced by a pagination limit.
    pub fn registry_day_totals(&self) -> Result<registry::RegistryDayTotals, GamesError> {
        Ok(self.store.registry_day_totals()?)
    }

    /// The short proof that one game's day is inside the day's anchored root.
    pub fn registry_proof(
        &self,
        game_id: &str,
        date: &str,
    ) -> Result<RegistryProofView, GamesError> {
        Self::day_arg(date)?;
        let day = self
            .store
            .registry_day(date)?
            .ok_or_else(|| GamesError::NotFound("that day has not been closed yet".into()))?;
        let (leaf, steps) = day.proof_for(game_id).ok_or_else(|| {
            GamesError::NotFound("that game has no results in that day's root".into())
        })?;
        let game_root = day
            .games
            .iter()
            .find(|(id, _)| id == game_id)
            .map(|(_, root)| *root)
            .unwrap_or_default();
        Ok(RegistryProofView {
            game_id: game_id.to_string(),
            date: day.date,
            game_root: hex::encode(game_root),
            leaf: hex::encode(leaf),
            steps: steps
                .iter()
                .map(|step| ProofStepView {
                    sibling: hex::encode(step.sibling),
                    side: if step.sibling_on_left {
                        "left"
                    } else {
                        "right"
                    },
                })
                .collect(),
            merkle_root: hex::encode(day.merkle_root),
            day_root: hex::encode(day.day_root),
            state: day.state,
            anchor_tx: day.anchor_tx,
            chain_id: self.config.registry_chain_id,
        })
    }

    /// Close a finished registry day, then anchor it if a chain and an anchorer are configured.
    pub async fn close_registry_day(&self, date: &str) -> Result<RegistryDay, GamesError> {
        let _guard = self.anchor_lock.lock().await;
        self.close_registry_locked(date).await
    }

    async fn close_registry_locked(&self, date: &str) -> Result<RegistryDay, GamesError> {
        Self::day_arg(date)?;
        if date >= self.today().as_str() {
            return Err(GamesError::Invalid(
                "only a finished day can be closed".into(),
            ));
        }
        let now = self.now();
        let day = self.store.close_registry_day(date, now)?;
        let (Some(chain_id), Some(anchorer)) = (self.config.registry_chain_id, &self.anchorer)
        else {
            return Ok(day);
        };
        // Nothing to anchor for a day no registered game played in, and nothing more to do once confirmed.
        if day.games.is_empty() || day.state == "confirmed" {
            return Ok(day);
        }
        if let Err(message) = self.anchor_registry_day(&day, chain_id, anchorer, now).await {
            self.store.record_registry_anchor_error(date, &message, now)?;
        }
        Ok(self
            .store
            .registry_day(date)?
            .ok_or_else(|| GamesError::Internal("registry day vanished".into()))?)
    }

    /// The registry's part of the daily run: close each finished day that has results, anchor it where
    /// possible, and never retry a failing day faster than the retry interval.
    async fn registry_daily(
        &self,
        report: &mut DailyReport,
        today: &str,
        now: u64,
    ) -> Result<(), GamesError> {
        for ago in (1..=self.config.catch_up_days as i64).rev() {
            let Some(date) = shift_date(today, -ago) else {
                continue;
            };
            if let Some(day) = self.store.registry_day(&date)? {
                if day.state == "confirmed" || day.games.is_empty() {
                    continue;
                }
                if day.last_error.is_some()
                    && now.saturating_sub(day.updated_at) < self.config.anchor_retry_seconds
                {
                    continue;
                }
            }
            match self.close_registry_locked(&date).await {
                Ok(day) => match day.state.as_str() {
                    "confirmed" => report.confirmed.push(("registry".into(), date)),
                    "anchored" => report.anchored.push(("registry".into(), date)),
                    _ => {
                        if let Some(error) = day.last_error {
                            report.errors.push(("registry".into(), date, error));
                        }
                    }
                },
                Err(error) => report
                    .errors
                    .push(("registry".into(), date, error.to_string())),
            }
        }
        Ok(())
    }
}
