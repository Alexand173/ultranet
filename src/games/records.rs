//! The Records Service: third-party games and every other application that anchors records.
//!
//! This is the half of the old combined service that owns the **chain owner's signing capability**. It
//! accepts records from applications, closes each day into one root, anchors it through `appchain-cli`, and
//! anchors first-party game days that the Games Reward Service asks it to (and only on the chains it
//! allows). It holds no payout key and has no code path that moves money; the Games Reward Service holds
//! the payout key and has no code path that signs an anchor. See `docs/APPCHAIN_SERVICE_ARCHITECTURE.md` §3.

use super::{
    anchor::Anchorer,
    chain::GamesChain,
    ownership,
    peer::PeerVerifier,
    registry::{self, RegistryCaps, RegistryDay, ResultInput},
    service::{shift_date, Clock, DailyReport, GamesError},
    store::GamesStore,
};
use crate::ultragrid;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};

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
    /// An anchor submitted but not seen on the ledger after this long is submitted again.
    pub anchor_resubmit_after_seconds: u64,
    /// The only chains a peer service (the Games Reward Service) may ask this one to anchor. Never the
    /// registry's own chain, never the L1: a compromised peer must not be able to anchor *as* the registry.
    pub peer_anchor_chains: Vec<u32>,
    /// The most distinct roots a peer may have anchored in one UTC day, across all chains. Every anchor
    /// costs the owner wallet a fee, so a compromised peer must not be able to drain it. A repeated ask
    /// for the same root, and a root already on the ledger, do not count.
    pub peer_anchors_per_day: u32,
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
        }
    }
}

impl Default for RecordsConfig {
    fn default() -> Self {
        Self::new()
    }
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
    /// Anchors submitted for a peer a moment ago, so a repeated request returns the same transaction
    /// instead of paying for a second one while the first waits for a block.
    recent_peer_anchors: Mutex<HashMap<(u32, [u8; 32]), (String, u64)>>,
    /// The UTC day and how many peer anchors were signed on it. In memory: a restart resets it, which is
    /// bounded by the cap itself.
    peer_anchor_day: Mutex<(String, u32)>,
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
            recent_peer_anchors: Mutex::new(HashMap::new()),
            peer_anchor_day: Mutex::new((String::new(), 0)),
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
        if let Some((tx, at)) = self
            .recent_peer_anchors
            .lock()
            .map_err(|_| GamesError::Internal("peer anchor cache poisoned".into()))?
            .get(&(chain_id, root))
            .cloned()
        {
            if now.saturating_sub(at) < self.config.anchor_resubmit_after_seconds {
                return Ok(reply(tx, "submitted"));
            }
        }
        {
            let today = self.today();
            let mut day = self
                .peer_anchor_day
                .lock()
                .map_err(|_| GamesError::Internal("peer anchor counter poisoned".into()))?;
            if day.0 != today {
                *day = (today, 0);
            }
            if day.1 >= self.config.peer_anchors_per_day {
                return Err(GamesError::Forbidden(
                    "the daily limit of anchors a peer may request has been reached".into(),
                ));
            }
            day.1 += 1;
        }
        match anchorer.anchor(chain_id, &root).await {
            Ok(tx) => {
                self.recent_peer_anchors
                    .lock()
                    .map_err(|_| GamesError::Internal("peer anchor cache poisoned".into()))?
                    .insert((chain_id, root), (tx.clone(), now));
                Ok(reply(tx, "submitted"))
            }
            Err(error) => Err(GamesError::AnchorFailed(error.to_string())),
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
        // Look before submitting, exactly as the first-party days do: an anchor of unclear outcome is
        // settled by finding its root on the ledger, never by paying for a second one.
        match self.chain.find_anchor(chain_id, &day.day_root).await {
            Ok(Some(point)) => {
                self.store
                    .set_registry_anchor(date, Some(&point.tx_hash), "confirmed", now)?
            }
            Ok(None) => {
                let recent = day.state == "anchored"
                    && now.saturating_sub(day.updated_at)
                        < self.config.anchor_resubmit_after_seconds;
                if !recent {
                    match anchorer.anchor(chain_id, &day.day_root).await {
                        Ok(transaction) => self.store.set_registry_anchor(
                            date,
                            Some(&transaction),
                            "anchored",
                            now,
                        )?,
                        Err(error) => self.store.record_registry_anchor_error(
                            date,
                            &error.to_string(),
                            now,
                        )?,
                    }
                }
            }
            Err(_) => self.store.record_registry_anchor_error(
                date,
                "the ledger could not be read to check the anchor",
                now,
            )?,
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
