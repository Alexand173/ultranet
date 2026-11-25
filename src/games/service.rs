//! The games service: intake, withdrawals, the payout worker, and the daily close.
//!
//! Three rules shape every line here, and each has a test:
//!
//! 1. **A receipt exists before money moves.** A win is recorded and credited in one transaction;
//!    only a later, separate step turns a balance into a transfer.
//! 2. **A payout is signed once.** The signed request is stored before it is submitted, and every retry
//!    submits those same bytes. The service never re-signs a payout whose fate is unknown, because a
//!    second signature could pay twice.
//! 3. **Anything unexpected pauses payments instead of guessing.** A mismatched hash, a rejected signed
//!    transfer, a confirmation that never comes: payments stop, the balance stays safe, and a human
//!    decides with the chain in front of them.

use super::{
    auth::{self, AuthConfig, AuthError},
    catalog::{self, AuditEntry, AuditRecord, CatalogError, CatalogGame},
    anchor::Anchorer,
    chain::{ChainError, GamesChain},
    registry::RegistryError,
    roots::{self, ChessResult, GridResult},
    rules::{self, Policy, RecordedReason, GAME_CHESS, GAME_GRID},
    store::{
        DayRow, GamesStore, Outcome as WinOutcome, PayoutRow, PayoutState, StoreError, WalletView,
        WinInput, UTC_DAY_SECONDS,
    },
};
use crate::{
    faucet::{
        abuse,
        models::SignedTransferRequest,
        node_client::{NodeApi, NodeClientError},
        signer::FaucetSigner,
    },
    ultragrid::{self, Puzzle, Turn, VerifyError, LEVELS_PER_DAY},
    UltraBlockchain,
};
use chrono::{Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::watch;

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(crate::auth::now_seconds)
}

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    /// The payout wallet's address. Its key is separate from the faucet's and from every chain owner's.
    pub payout_address: String,
    /// Seconds from signing before an unconfirmed payout pauses payments for a human to look.
    pub confirmation_timeout_seconds: u64,
    pub max_submission_attempts: u32,
    /// Anchor a day even when nobody won, so a listed chain is "live" (V2 C-2).
    pub anchor_empty_days: bool,
    /// Which games' days are anchored. A game whose chain is not registered yet is left out.
    pub anchor_games: Vec<String>,
    /// Do not retry a failed anchor for a day more often than this.
    pub anchor_retry_seconds: u64,
    /// How many past days the daily run looks at.
    pub catch_up_days: u32,
    /// An anchor submitted but not seen on the ledger after this long is submitted again.
    pub anchor_resubmit_after_seconds: u64,
    /// Wallet sign-in limits and lifetimes (see `auth.rs`).
    pub auth: AuthConfig,
    /// Require a wallet session for the routes that act for a player (answers, results, withdrawals). An
    /// address in a request body is not proof of anything; with this on, it must match the session's.
    pub require_player_auth: bool,
    /// The most games one AppChain may hold in the catalog (a platform limit against spam, not a price).
    pub catalog_max_games_per_chain: u32,
}

impl ServiceConfig {
    pub fn new(payout_address: String) -> ServiceConfig {
        ServiceConfig {
            payout_address,
            confirmation_timeout_seconds: 900,
            max_submission_attempts: 20,
            anchor_empty_days: true,
            anchor_games: vec![GAME_GRID.to_string()],
            anchor_retry_seconds: 600,
            catch_up_days: 7,
            anchor_resubmit_after_seconds: 3_600,
            auth: AuthConfig::default(),
            // The library default is open so existing in-process callers keep working; the service's own
            // configuration (`GamesConfig`) turns this on unless an operator says otherwise.
            require_player_auth: false,
            catalog_max_games_per_chain: 20,
        }
    }
}

#[derive(Debug, Error)]
pub enum GamesError {
    #[error("the games service is not accepting results right now")]
    Disabled,
    #[error("payouts are paused")]
    PayoutsPaused,
    #[error("{0}")]
    Invalid(String),
    #[error("that is not today's date in UTC")]
    DateMismatch { server_date: String },
    #[error("the answer was refused: {0}")]
    AnswerRefused(String),
    #[error("the chain could not be reached")]
    ChainUnavailable,
    #[error("that game id belongs to a different result")]
    Conflict,
    #[error("the balance {available} is below the withdrawal minimum {minimum}")]
    BelowMinimum { available: u64, minimum: u64 },
    #[error("today's withdrawal ceiling has been reached")]
    PayoutDailyCap,
    #[error("the game id or key is not valid")]
    InvalidKey,
    #[error("this game is suspended and is not accepting results")]
    GameSuspended,
    #[error("a result with that id was already recorded with different contents")]
    ResultConflict,
    #[error("this game has reached its limit of results for today")]
    GameLimit,
    #[error("this player has reached the limit of results for this game today")]
    PlayerLimit,
    #[error("this address already holds the most games one owner may have")]
    OwnerLimit,
    #[error("too many games were registered today; try again tomorrow")]
    RegistrationLimit,
    #[error("{0}")]
    Forbidden(String),
    #[error("anchoring is not enabled on this service")]
    AnchoringDisabled,
    #[error("{0}")]
    ReconciliationRequired(String),
    #[error("the anchor was not accepted: {0}")]
    AnchorFailed(String),
    #[error("the peer could not be authenticated")]
    PeerUnauthenticated,
    #[error("the owner proof did not verify")]
    AdminProofInvalid,
    #[error("this AppChain already has the most games it may hold")]
    CatalogFull,
    #[error("sign in with your wallet first")]
    AuthRequired,
    #[error("the sign-in could not be verified")]
    AuthFailed,
    #[error("that session belongs to a different wallet")]
    SessionMismatch,
    #[error("too many sign-in requests; try again shortly")]
    AuthRateLimited,
    #[error("{0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl GamesError {
    pub fn code(&self) -> &'static str {
        match self {
            GamesError::Disabled => "service_disabled",
            GamesError::PayoutsPaused => "payouts_paused",
            GamesError::Invalid(_) => "invalid_request",
            GamesError::DateMismatch { .. } => "date_mismatch",
            GamesError::AnswerRefused(_) => "answer_refused",
            GamesError::ChainUnavailable => "chain_unavailable",
            GamesError::Conflict => "game_id_conflict",
            GamesError::BelowMinimum { .. } => "below_minimum",
            GamesError::PayoutDailyCap => "payout_daily_cap",
            GamesError::InvalidKey => "invalid_key",
            GamesError::GameSuspended => "game_suspended",
            GamesError::ResultConflict => "result_conflict",
            GamesError::GameLimit => "game_daily_limit",
            GamesError::PlayerLimit => "player_daily_limit",
            GamesError::OwnerLimit => "owner_game_limit",
            GamesError::RegistrationLimit => "registration_limit",
            GamesError::Forbidden(_) => "forbidden",
            GamesError::AnchoringDisabled => "anchoring_disabled",
            GamesError::ReconciliationRequired(_) => "reconciliation_required",
            GamesError::AnchorFailed(_) => "anchor_failed",
            GamesError::PeerUnauthenticated => "peer_unauthenticated",
            GamesError::NotFound(_) => "not_found",
            GamesError::AdminProofInvalid => "proof_invalid",
            GamesError::CatalogFull => "chain_full",
            GamesError::AuthRequired => "auth_required",
            GamesError::AuthFailed => "auth_failed",
            GamesError::SessionMismatch => "session_mismatch",
            GamesError::AuthRateLimited => "auth_rate_limited",
            GamesError::Internal(_) => "internal_error",
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            GamesError::Invalid(_)
            | GamesError::DateMismatch { .. }
            | GamesError::AnswerRefused(_) => 400,
            GamesError::Conflict | GamesError::ResultConflict => 409,
            GamesError::InvalidKey
            | GamesError::PeerUnauthenticated
            | GamesError::AuthRequired
            | GamesError::AuthFailed
            | GamesError::AdminProofInvalid => 401,
            GamesError::GameSuspended | GamesError::Forbidden(_) | GamesError::SessionMismatch => 403,
            GamesError::AnchorFailed(_) => 502,
            GamesError::NotFound(_) => 404,
            GamesError::BelowMinimum { .. }
            | GamesError::PayoutDailyCap
            | GamesError::GameLimit
            | GamesError::PlayerLimit
            | GamesError::OwnerLimit
            | GamesError::RegistrationLimit
            | GamesError::AuthRateLimited
            | GamesError::CatalogFull => 429,
            GamesError::Disabled
            | GamesError::PayoutsPaused
            | GamesError::ChainUnavailable
            | GamesError::AnchoringDisabled
            | GamesError::ReconciliationRequired(_) => 503,
            GamesError::Internal(_) => 500,
        }
    }
}

impl From<CatalogError> for GamesError {
    fn from(error: CatalogError) -> Self {
        match error {
            CatalogError::Store(error) => error.into(),
            CatalogError::ProofInvalid => GamesError::AdminProofInvalid,
            CatalogError::Invalid(message) => GamesError::Invalid(message),
            CatalogError::NotFound(message) => GamesError::NotFound(message),
            CatalogError::ChainFull => GamesError::CatalogFull,
        }
    }
}

impl From<AuthError> for GamesError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::Store(error) => error.into(),
            AuthError::RateLimited => GamesError::AuthRateLimited,
            AuthError::Rejected => GamesError::AuthFailed,
            AuthError::Invalid(message) => GamesError::Invalid(message),
        }
    }
}

impl From<StoreError> for GamesError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Conflict => GamesError::Conflict,
            StoreError::BelowMinimum { available, minimum } => {
                GamesError::BelowMinimum { available, minimum }
            }
            StoreError::PayoutDailyCap => GamesError::PayoutDailyCap,
            other => GamesError::Internal(other.to_string()),
        }
    }
}

impl From<serde_json::Error> for GamesError {
    fn from(error: serde_json::Error) -> Self {
        GamesError::Internal(format!("stored data could not be decoded: {error}"))
    }
}

impl From<RegistryError> for GamesError {
    fn from(error: RegistryError) -> Self {
        match error {
            RegistryError::Invalid(message) => GamesError::Invalid(message),
            RegistryError::ReservedId => GamesError::Invalid("that game id is reserved".into()),
            RegistryError::IdTaken => GamesError::Invalid("that game id is already taken".into()),
            RegistryError::BadKey => GamesError::InvalidKey,
            RegistryError::Suspended => GamesError::GameSuspended,
            RegistryError::Conflict => GamesError::ResultConflict,
            RegistryError::DailyCap => GamesError::GameLimit,
            RegistryError::PlayerDailyCap => GamesError::PlayerLimit,
            RegistryError::UnknownGame => GamesError::NotFound("no such game".into()),
            RegistryError::OwnerLimit => GamesError::OwnerLimit,
            RegistryError::RegistrationLimit => GamesError::RegistrationLimit,
            RegistryError::Store(detail) => GamesError::Internal(detail),
        }
    }
}

impl From<ChainError> for GamesError {
    fn from(_: ChainError) -> Self {
        GamesError::ChainUnavailable
    }
}

// --- requests and responses ------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnInput {
    pub cell: u16,
    pub turns: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridAnswerRequest {
    pub address: String,
    pub date: String,
    pub level: u8,
    pub answer: Vec<TurnInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChessResultRequest {
    pub address: String,
    pub game_id: String,
    pub mode: String,
    pub result: String,
    pub plies: u32,
    #[serde(default)]
    pub app: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct GridOutcomeView {
    pub moves: u32,
    pub connected: bool,
    pub par: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize)]
pub struct WinResponse {
    pub game: &'static str,
    /// Whether the submission was a win. A Grid answer that does not solve the puzzle is not recorded.
    pub win: bool,
    /// Whether the server replayed and proved the win. Chess results are reported by the phone.
    pub verified: bool,
    /// `rewarded` or `recorded`; null when it was not a win.
    pub outcome: Option<&'static str>,
    pub reward_units: u64,
    pub reason: Option<&'static str>,
    pub message: Option<&'static str>,
    /// The same win had already been recorded; nothing was added.
    pub duplicate: bool,
    /// What a withdrawal would pay now.
    pub balance_units: u64,
    pub grid: Option<GridOutcomeView>,
}

#[derive(Debug, Serialize)]
pub struct PayoutView {
    pub id: String,
    pub amount_units: u64,
    pub amount_ultra: String,
    pub state: &'static str,
    pub transaction_hash: Option<String>,
    pub message: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DayCountView {
    pub game: String,
    pub rewarded: u64,
    pub recorded: u64,
}

#[derive(Debug, Serialize)]
pub struct RecentWinView {
    pub game: String,
    pub day: String,
    pub item: String,
    pub outcome: &'static str,
    pub reward_units: u64,
    pub reason: Option<&'static str>,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub address: String,
    pub credited_units: u64,
    pub paid_units: u64,
    pub reserved_units: u64,
    pub available_units: u64,
    pub available_ultra: String,
    pub min_withdraw_units: u64,
    pub can_withdraw: bool,
    pub payouts_paused: bool,
    pub open_payout: Option<PayoutView>,
    pub today: Vec<DayCountView>,
    pub recent: Vec<RecentWinView>,
}

#[derive(Debug, Serialize)]
pub struct GamePolicyView {
    pub game: &'static str,
    pub chain_id: u32,
    pub pays: bool,
    pub reward_units: u64,
    pub rewarded_per_address_per_day: u32,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub accepting: bool,
    pub paying: bool,
    pub date: String,
    pub withdraw_min_units: u64,
    pub games: Vec<GamePolicyView>,
}

// --- the grid day -----------------------------------------------------------------------------

#[derive(Clone)]
pub struct GridDay {
    pub date: String,
    pub prev_root: [u8; 32],
    pub prev_anchor_sequence: Option<u64>,
    pub seed: [u8; 32],
    pub commitment: [u8; 32],
    pub puzzles: Arc<Vec<Puzzle>>,
}

pub struct DailyReport {
    pub anchored: Vec<(String, String)>,
    pub confirmed: Vec<(String, String)>,
    pub errors: Vec<(String, String, String)>,
}

/// A signed administrative request: the owner's public key and signature over the digest of the action.
/// A player's wallet session is not accepted in its place.
#[derive(Debug, Clone)]
pub struct AdminProof {
    /// Fresh each time, chosen by the signer (16 to 64 lowercase hex characters).
    pub nonce: String,
    pub timestamp: u64,
    pub public_key: String,
    pub signature: String,
}

/// What a wallet needs in order to sign in: the challenge and the digest it must sign.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengeView {
    pub challenge_id: String,
    pub address: String,
    pub purpose: String,
    pub issued_at: u64,
    pub expires_at: u64,
    /// The 32 bytes to sign, hex. A wallet recomputes it from the fields above and signs its own result.
    pub digest: String,
}

/// A verified sign-in. The token is shown once; only its hash is kept.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub token: String,
    pub address: String,
    pub expires_at: u64,
}

pub struct GamesService {
    store: GamesStore,
    policy: Policy,
    config: ServiceConfig,
    chain: Arc<dyn GamesChain>,
    node: Arc<dyn NodeApi>,
    signer: Option<Arc<FaucetSigner>>,
    anchorer: Option<Arc<dyn Anchorer>>,
    abuse_key: Vec<u8>,
    clock: Clock,
    days: Mutex<HashMap<String, GridDay>>,
    payout_lock: tokio::sync::Mutex<()>,
    anchor_lock: tokio::sync::Mutex<()>,
}

pub fn date_of(timestamp: u64) -> String {
    Utc.timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|moment| moment.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

/// Seconds since the epoch at 00:00:00 UTC of `date`.
pub fn date_start(date: &str) -> Option<u64> {
    if !ultragrid::valid_date(date) {
        return None;
    }
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let moment = Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0)?);
    u64::try_from(moment.timestamp()).ok()
}

pub fn shift_date(date: &str, days: i64) -> Option<String> {
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(
        (day + ChronoDuration::days(days))
            .format("%Y-%m-%d")
            .to_string(),
    )
}

fn random_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn valid_game_id(value: &str) -> bool {
    (16..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl GamesService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: GamesStore,
        policy: Policy,
        config: ServiceConfig,
        chain: Arc<dyn GamesChain>,
        node: Arc<dyn NodeApi>,
        signer: Option<Arc<FaucetSigner>>,
        anchorer: Option<Arc<dyn Anchorer>>,
        abuse_key: Vec<u8>,
        clock: Clock,
    ) -> GamesService {
        GamesService {
            store,
            policy,
            config,
            chain,
            node,
            signer,
            anchorer,
            abuse_key,
            clock,
            days: Mutex::new(HashMap::new()),
            payout_lock: tokio::sync::Mutex::new(()),
            anchor_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn store(&self) -> &GamesStore {
        &self.store
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    pub fn today(&self) -> String {
        date_of(self.now())
    }

    // --- the game catalog ------------------------------------------------------------------

    /// Check a signed administrative request and return who signed it, the chain's current owner and the
    /// request's digest. The signature is checked first (cheap, no I/O), then the ledger is asked who owns
    /// the chain *now*; the two must be the same address.
    #[allow(clippy::too_many_arguments)]
    async fn authorize_admin(
        &self,
        action: &str,
        chain_id: u32,
        game_id: &str,
        name: &str,
        description: &str,
        status: &str,
        proof: &AdminProof,
    ) -> Result<(String, String, [u8; 32]), GamesError> {
        catalog::check_timestamp(proof.timestamp, self.now())?;
        catalog::check_nonce(&proof.nonce)?;
        let digest = catalog::admin_digest(action, chain_id, game_id, name, description, status, &proof.nonce, proof.timestamp);
        let actor = catalog::verify_admin_signature(&digest, &proof.public_key, &proof.signature)?;
        let owner = self
            .chain
            .chain_owner(chain_id)
            .await
            .map_err(|_| GamesError::ChainUnavailable)?
            .ok_or_else(|| GamesError::NotFound("that AppChain is not registered on the ledger".into()))?;
        if owner != actor {
            return Err(GamesError::Forbidden(format!(
                "only the current owner of AppChain #{chain_id} may do that"
            )));
        }
        Ok((actor, owner, digest))
    }

    /// Register a game on an AppChain. The game id is generated here and never changes; the name is a label.
    pub async fn catalog_register(
        &self,
        chain_id: u32,
        name: &str,
        description: &str,
        proof: AdminProof,
    ) -> Result<CatalogGame, GamesError> {
        if chain_id == 0 {
            return Err(GamesError::Invalid("chain_id must be an AppChain, not the L1".into()));
        }
        let name = catalog::clean_label(name, "name", catalog::NAME_MAX_CHARS, true)?;
        let description = catalog::clean_label(description, "description", catalog::DESCRIPTION_MAX_CHARS, false)?;
        let (actor, owner, digest) = self
            .authorize_admin(catalog::ACTION_REGISTER, chain_id, "", &name, &description, "", &proof)
            .await?;
        let mut id = [0u8; 16];
        OsRng.fill_bytes(&mut id);
        let game_id = hex::encode(id);
        let detail = serde_json::json!({ "chain_id": chain_id, "name": name }).to_string();
        Ok(self.store.catalog_register(
            &game_id,
            chain_id,
            &name,
            &description,
            self.now(),
            self.config.catalog_max_games_per_chain,
            &AuditRecord { actor: &actor, action: catalog::ACTION_REGISTER, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    fn known_game(&self, game_id: &str) -> Result<CatalogGame, GamesError> {
        self.store
            .catalog_game(game_id)?
            .ok_or_else(|| GamesError::NotFound("no such game".into()))
    }

    /// Change a game's display metadata. Authority is the chain's current owner, not whoever registered it.
    pub async fn catalog_update(
        &self,
        game_id: &str,
        name: &str,
        description: &str,
        proof: AdminProof,
    ) -> Result<CatalogGame, GamesError> {
        let game = self.known_game(game_id)?;
        let name = catalog::clean_label(name, "name", catalog::NAME_MAX_CHARS, true)?;
        let description = catalog::clean_label(description, "description", catalog::DESCRIPTION_MAX_CHARS, false)?;
        let (actor, owner, digest) = self
            .authorize_admin(catalog::ACTION_UPDATE, game.chain_id, game_id, &name, &description, "", &proof)
            .await?;
        let detail = serde_json::json!({ "name": name }).to_string();
        Ok(self.store.catalog_update(
            game_id,
            &name,
            &description,
            self.now(),
            &AuditRecord { actor: &actor, action: catalog::ACTION_UPDATE, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    /// Enable or disable a game. Its id, records and history are untouched either way.
    pub async fn catalog_set_status(
        &self,
        game_id: &str,
        status: &str,
        proof: AdminProof,
    ) -> Result<CatalogGame, GamesError> {
        if status != catalog::STATUS_ACTIVE && status != catalog::STATUS_DISABLED {
            return Err(GamesError::Invalid("status must be \"active\" or \"disabled\"".into()));
        }
        let game = self.known_game(game_id)?;
        let (actor, owner, digest) = self
            .authorize_admin(catalog::ACTION_STATUS, game.chain_id, game_id, "", "", status, &proof)
            .await?;
        let detail = serde_json::json!({ "status": status }).to_string();
        Ok(self.store.catalog_set_status(
            game_id,
            status,
            self.now(),
            &AuditRecord { actor: &actor, action: catalog::ACTION_STATUS, chain_owner: &owner, detail: &detail, digest },
        )?)
    }

    pub fn catalog_games(&self, chain_id: Option<u32>) -> Result<Vec<CatalogGame>, GamesError> {
        Ok(self.store.catalog_games(chain_id)?)
    }

    pub fn catalog_game(&self, game_id: &str) -> Result<CatalogGame, GamesError> {
        self.known_game(game_id)
    }

    pub fn catalog_history(&self, game_id: &str) -> Result<Vec<AuditEntry>, GamesError> {
        self.known_game(game_id)?;
        Ok(self.store.catalog_history(game_id)?)
    }

    /// Issue a one-time sign-in challenge for `address`.
    pub fn auth_challenge(&self, address: &str, purpose: &str) -> Result<ChallengeView, GamesError> {
        let address = self.normalize_address(address)?;
        let mut id = [0u8; 32];
        OsRng.fill_bytes(&mut id);
        let challenge = self.store.issue_challenge(
            &hex::encode(id),
            &address,
            purpose,
            self.now(),
            &self.config.auth,
        )?;
        let digest = challenge.digest()?;
        Ok(ChallengeView {
            challenge_id: challenge.id,
            address: challenge.address,
            purpose: challenge.purpose,
            issued_at: challenge.issued_at,
            expires_at: challenge.expires_at,
            digest: hex::encode(digest),
        })
    }

    /// Check the wallet's signature over a challenge and open a session bound to that wallet.
    pub fn auth_verify(
        &self,
        challenge_id: &str,
        public_key_hex: &str,
        signature_hex: &str,
    ) -> Result<SessionView, GamesError> {
        let now = self.now();
        let challenge = self
            .store
            .begin_verification(challenge_id, now, &self.config.auth)?;
        auth::verify_signature(&challenge, public_key_hex, signature_hex)?;
        let mut token = [0u8; 32];
        OsRng.fill_bytes(&mut token);
        let token = hex::encode(token);
        let session = self.store.finish_verification(
            &challenge,
            &auth::token_hash(&token),
            now,
            &self.config.auth,
        )?;
        Ok(SessionView {
            token,
            address: session.address,
            expires_at: session.expires_at,
        })
    }

    /// End the session behind `token`. Always succeeds from the caller's side: whether it existed is not told.
    pub fn auth_logout(&self, token: &str) -> Result<(), GamesError> {
        if auth::is_address(token) {
            self.store.revoke_session(&auth::token_hash(token), self.now())?;
        }
        Ok(())
    }

    /// The wallet a session token proves, if it is live.
    pub fn session_address(&self, token: &str) -> Result<Option<String>, GamesError> {
        if !auth::is_address(token) {
            return Ok(None);
        }
        Ok(self
            .store
            .session_for(&auth::token_hash(token), self.now())?
            .filter(|session| session.purpose == auth::PURPOSE_PLAY)
            .map(|session| session.address))
    }

    /// Whether a request that acts for `claimed_address` may proceed. A token, when one is sent, must be a
    /// live session for exactly that wallet: a wrong session is refused even where sign-in is optional,
    /// never ignored. With no token, sign-in is required when `require_player_auth` is on.
    pub fn require_player(&self, token: Option<&str>, claimed_address: &str) -> Result<(), GamesError> {
        let claimed = self.normalize_address(claimed_address)?;
        match token {
            Some(token) => match self.session_address(token)? {
                Some(address) if address == claimed => Ok(()),
                Some(_) => Err(GamesError::SessionMismatch),
                None => Err(GamesError::AuthRequired),
            },
            None if self.config.require_player_auth => Err(GamesError::AuthRequired),
            None => Ok(()),
        }
    }

    fn normalize_address(&self, address: &str) -> Result<String, GamesError> {
        let address = address.trim().to_ascii_lowercase();
        let address = address.strip_prefix("0x").unwrap_or(&address).to_string();
        if roots::address_bytes(&address).is_none() {
            return Err(GamesError::Invalid(
                "address must be 64 hexadecimal characters".into(),
            ));
        }
        // The rewards wallet cannot pay itself. Crediting it would queue a payout the signer must refuse,
        // and a refusal pauses payments for everyone: a stranger could do that by naming it. Keep it out.
        if address == self.config.payout_address {
            return Err(GamesError::Invalid(
                "that address belongs to the rewards wallet and cannot take part".into(),
            ));
        }
        Ok(address)
    }

    fn require_accepting(&self) -> Result<(), GamesError> {
        if self.store.state()?.accepting {
            Ok(())
        } else {
            Err(GamesError::Disabled)
        }
    }

    /// Whether this address should count as existing on the chain, asking the node only when the answer
    /// could change the outcome.
    async fn on_chain(&self, game: &rules::GamePolicy, address: &str) -> Result<bool, GamesError> {
        if !game.pays || !self.policy.require_chain_presence {
            return Ok(true);
        }
        Ok(self.chain.address_active(address).await?)
    }

    /// The payout wallet's balance on the chain right now. It bounds what can be promised: a reward is
    /// granted only if it fits, with everything already owed, inside this. If it cannot be read the win is
    /// not recorded, and the phone sends it again, rather than guessing that the pool is full.
    async fn pool_units(&self, game: &rules::GamePolicy) -> Result<u64, GamesError> {
        if !game.pays {
            return Ok(0);
        }
        match self.node.account(&self.config.payout_address).await {
            Ok(account) => account
                .balance_base_units()
                .ok_or(GamesError::ChainUnavailable),
            Err(_) => Err(GamesError::ChainUnavailable),
        }
    }

    fn balance_after(&self, address: &str, today: &str) -> Result<u64, GamesError> {
        Ok(self.store.wallet(address, today)?.available)
    }

    // --- UltraGrid -------------------------------------------------------------------------

    /// The seed inputs and puzzles of a Grid day, read from the ledger once and then fixed.
    pub async fn grid_day(&self, date: &str) -> Result<GridDay, GamesError> {
        if !ultragrid::valid_date(date) {
            return Err(GamesError::Invalid("date must look like 2026-11-16".into()));
        }
        if let Some(day) = self
            .days
            .lock()
            .map_err(|_| GamesError::Internal("lock".into()))?
            .get(date)
        {
            return Ok(day.clone());
        }
        let chain_id = self.policy.grid.chain_id;
        let (prev_root, prev_sequence) = match self.store.day_row(date, GAME_GRID)? {
            Some(row) if row.seed.is_some() => {
                let root: [u8; 32] = row
                    .prev_root
                    .clone()
                    .and_then(|v| v.try_into().ok())
                    .ok_or_else(|| {
                        GamesError::Internal("stored previous root is malformed".into())
                    })?;
                (root, row.prev_anchor_sequence)
            }
            _ => {
                let start =
                    date_start(date).ok_or_else(|| GamesError::Invalid("bad date".into()))?;
                match self.chain.latest_anchor_before(chain_id, start).await? {
                    Some(point) => (point.root, Some(point.sequence)),
                    None => ([0u8; 32], None),
                }
            }
        };
        let seed = ultragrid::daily_seed(chain_id, date, &prev_root)
            .ok_or_else(|| GamesError::Internal("could not derive a seed".into()))?;
        let puzzles = ultragrid::daily_set(&seed);
        let commitment = ultragrid::set_commitment(&puzzles);
        self.store.put_grid_day(
            date,
            &prev_root,
            prev_sequence,
            &seed,
            &commitment,
            self.now(),
        )?;
        // Whatever was stored first is the truth; if a concurrent call stored different inputs, use those.
        let stored = self
            .store
            .day_row(date, GAME_GRID)?
            .ok_or_else(|| GamesError::Internal("day row vanished".into()))?;
        if stored.seed.as_deref() != Some(seed.as_slice()) {
            return Err(GamesError::Internal(
                "stored seed disagrees with the derived one".into(),
            ));
        }
        let day = GridDay {
            date: date.to_string(),
            prev_root,
            prev_anchor_sequence: prev_sequence,
            seed,
            commitment,
            puzzles: Arc::new(puzzles),
        };
        let mut days = self
            .days
            .lock()
            .map_err(|_| GamesError::Internal("lock".into()))?;
        if days.len() >= 8 {
            days.clear();
        }
        days.insert(date.to_string(), day.clone());
        Ok(day)
    }

    pub async fn submit_grid_answer(
        &self,
        request: GridAnswerRequest,
        client: Option<&str>,
    ) -> Result<WinResponse, GamesError> {
        self.require_accepting()?;
        let address = self.normalize_address(&request.address)?;
        let today = self.today();
        if request.date != today {
            return Err(GamesError::DateMismatch { server_date: today });
        }
        if request.level as usize >= LEVELS_PER_DAY {
            return Err(GamesError::Invalid(format!(
                "level must be 0 to {}",
                LEVELS_PER_DAY - 1
            )));
        }
        let day = self.grid_day(&request.date).await?;
        let puzzle = &day.puzzles[request.level as usize];
        let answer: Vec<Turn> = request
            .answer
            .iter()
            .map(|entry| Turn {
                cell: entry.cell,
                turns: entry.turns,
            })
            .collect();
        let outcome = ultragrid::verify(puzzle, &answer).map_err(|error| {
            GamesError::AnswerRefused(match error {
                VerifyError::TooManyEntries => "more entries than the grid has tiles".into(),
                VerifyError::CellOutOfRange(cell) => format!("tile {cell} is not on the grid"),
                VerifyError::NotRotatable(cell) => format!("tile {cell} cannot be turned"),
                VerifyError::BadTurnCount { cell, turns } => {
                    format!("{turns} turns of tile {cell} is not a move")
                }
                VerifyError::DuplicateCell(cell) => format!("tile {cell} appears twice"),
            })
        })?;
        let grid = GridOutcomeView {
            moves: outcome.moves,
            connected: outcome.connected,
            par: outcome.par as u32,
            limit: outcome.limit,
        };
        if !outcome.win {
            // A position that does not solve the puzzle within the limit is not a win and is not recorded.
            return Ok(WinResponse {
                game: GAME_GRID,
                win: false,
                verified: true,
                outcome: None,
                reward_units: 0,
                reason: None,
                message: None,
                duplicate: false,
                balance_units: self.balance_after(&address, &today)?,
                grid: Some(grid),
            });
        }

        let on_chain = self.on_chain(&self.policy.grid, &address).await?;
        let pool = self.pool_units(&self.policy.grid).await?;
        let input = WinInput {
            game: GAME_GRID,
            address: address.clone(),
            day: today.clone(),
            item: request.level.to_string(),
            moves: outcome.moves,
            detail: serde_json::json!({
                "puzzle_id": hex::encode(puzzle.id()),
                "moves": outcome.moves,
                "par": outcome.par,
            })
            .to_string(),
            verified: true,
            client_digest: abuse::client_digest(&self.abuse_key, client),
        };
        let recorded = self
            .store
            .record_win(&input, &self.policy, on_chain, pool, self.now())?;
        Ok(WinResponse {
            game: GAME_GRID,
            win: true,
            verified: true,
            outcome: Some(match recorded.win.outcome {
                WinOutcome::Rewarded => "rewarded",
                WinOutcome::Recorded => "recorded",
            }),
            reward_units: recorded.win.reward_units,
            reason: recorded.win.reason.map(RecordedReason::code),
            message: recorded.win.reason.map(RecordedReason::message),
            duplicate: recorded.duplicate,
            balance_units: self.balance_after(&address, &today)?,
            grid: Some(grid),
        })
    }

    // --- Ultra Chess -----------------------------------------------------------------------

    /// Record a chess win the phone reports. The server cannot replay it: the opponent runs on the phone.
    /// The receipt is therefore marked unverified, and the caps are the only protection, as documented.
    pub async fn submit_chess_result(
        &self,
        request: ChessResultRequest,
        client: Option<&str>,
    ) -> Result<WinResponse, GamesError> {
        self.require_accepting()?;
        let address = self.normalize_address(&request.address)?;
        if request.result != "player_win" {
            return Err(GamesError::Invalid("only wins are reported".into()));
        }
        let mode = request.mode.as_str();
        if roots::chess_mode_code(mode).is_none() {
            return Err(GamesError::Invalid(
                "mode must be classic, random or anti".into(),
            ));
        }
        if request.plies < self.policy.chess_min_plies
            || request.plies > self.policy.chess_max_plies
        {
            return Err(GamesError::Invalid(format!(
                "a game of {} plies is outside {} to {}",
                request.plies, self.policy.chess_min_plies, self.policy.chess_max_plies
            )));
        }
        if !valid_game_id(&request.game_id) {
            return Err(GamesError::Invalid(
                "game_id must be 16 to 64 letters, digits, - or _".into(),
            ));
        }
        if request.app.as_ref().is_some_and(|app| app.len() > 32) {
            return Err(GamesError::Invalid("app is too long".into()));
        }
        let today = self.today();
        let on_chain = self.on_chain(&self.policy.chess, &address).await?;
        let pool = self.pool_units(&self.policy.chess).await?;
        let input = WinInput {
            game: GAME_CHESS,
            address: address.clone(),
            day: today.clone(),
            item: request.game_id.clone(),
            moves: request.plies,
            detail: serde_json::json!({ "mode": mode, "plies": request.plies }).to_string(),
            verified: false,
            client_digest: abuse::client_digest(&self.abuse_key, client),
        };
        let recorded = self
            .store
            .record_win(&input, &self.policy, on_chain, pool, self.now())?;
        Ok(WinResponse {
            game: GAME_CHESS,
            win: true,
            verified: false,
            outcome: Some(match recorded.win.outcome {
                WinOutcome::Rewarded => "rewarded",
                WinOutcome::Recorded => "recorded",
            }),
            reward_units: recorded.win.reward_units,
            reason: recorded.win.reason.map(RecordedReason::code),
            message: recorded.win.reason.map(RecordedReason::message),
            duplicate: recorded.duplicate,
            balance_units: self.balance_after(&address, &today)?,
            grid: None,
        })
    }

    // --- wallets and withdrawals -----------------------------------------------------------

    fn payout_view(&self, payout: &PayoutRow) -> PayoutView {
        PayoutView {
            id: payout.id.clone(),
            amount_units: payout.amount_units,
            amount_ultra: rules::format_units(payout.amount_units),
            state: payout.state.code(),
            transaction_hash: payout.tx_hash.clone(),
            message: match payout.state {
                PayoutState::Queued => "Queued. It is sent in the order requests arrived.",
                PayoutState::Signed | PayoutState::Submitted => {
                    "Sent to the network; waiting for a block."
                }
                PayoutState::Confirmed => "Paid. The transfer is on the chain.",
                PayoutState::Failed => "Not sent. The balance is back in your account.",
            },
        }
    }

    pub fn wallet(&self, address: &str) -> Result<WalletResponse, GamesError> {
        let address = self.normalize_address(address)?;
        let today = self.today();
        let view: WalletView = self.store.wallet(&address, &today)?;
        let state = self.store.state()?;
        Ok(WalletResponse {
            address: view.address.clone(),
            credited_units: view.credited,
            paid_units: view.paid,
            reserved_units: view.reserved,
            available_units: view.available,
            available_ultra: rules::format_units(view.available),
            min_withdraw_units: self.policy.withdraw_min_units,
            can_withdraw: view.available >= self.policy.withdraw_min_units
                && view.open_payout.is_none()
                && state.paying,
            payouts_paused: !state.paying,
            open_payout: view
                .open_payout
                .as_ref()
                .map(|payout| self.payout_view(payout)),
            today: view
                .today
                .into_iter()
                .map(|c| DayCountView {
                    game: c.game,
                    rewarded: c.rewarded,
                    recorded: c.recorded,
                })
                .collect(),
            recent: view
                .recent
                .into_iter()
                .map(|win| RecentWinView {
                    game: win.game,
                    day: win.day,
                    item: win.item,
                    outcome: match win.outcome {
                        WinOutcome::Rewarded => "rewarded",
                        WinOutcome::Recorded => "recorded",
                    },
                    reward_units: win.reward_units,
                    reason: win.reason.map(RecordedReason::code),
                    verified: win.verified,
                })
                .collect(),
        })
    }

    /// Queue the address's whole balance for withdrawal. Asking again returns the payout already open.
    pub fn withdraw(&self, address: &str) -> Result<PayoutView, GamesError> {
        let address = self.normalize_address(address)?;
        if !self.store.state()?.paying {
            return Err(GamesError::PayoutsPaused);
        }
        let now = self.now();
        let day_start = now - now % UTC_DAY_SECONDS;
        let (payout, _created) =
            self.store
                .create_payout(&address, &self.policy, &random_id(), day_start, now)?;
        Ok(self.payout_view(&payout))
    }

    pub fn payout_status(&self, id: &str) -> Result<Option<PayoutView>, GamesError> {
        Ok(self
            .store
            .payout(id)?
            .map(|payout| self.payout_view(&payout)))
    }

    pub fn status(&self) -> Result<StatusResponse, GamesError> {
        let state = self.store.state()?;
        let game = |policy: &rules::GamePolicy, verified: bool| GamePolicyView {
            game: policy.game,
            chain_id: policy.chain_id,
            pays: policy.pays,
            reward_units: policy.reward_units,
            rewarded_per_address_per_day: policy.rewarded_per_address_per_day,
            verified,
        };
        Ok(StatusResponse {
            accepting: state.accepting,
            paying: state.paying,
            date: self.today(),
            withdraw_min_units: self.policy.withdraw_min_units,
            games: vec![
                game(&self.policy.grid, true),
                game(&self.policy.chess, false),
            ],
        })
    }

    // --- the payout worker -----------------------------------------------------------------

    fn pause_payments(&self, reason: &str) {
        let _ = self
            .store
            .set_state(None, Some(false), Some(reason), self.now());
        tracing::error!(reason, "games payouts paused; operator review required");
    }

    /// Handle the oldest unfinished payout, one step. Returns whether there was anything to do.
    pub async fn process_payout_once(&self) -> Result<bool, GamesError> {
        let _guard = self.payout_lock.lock().await;
        if !self.store.state()?.paying {
            return Ok(false);
        }
        let Some(payout) = self.store.next_open_payout()? else {
            return Ok(false);
        };
        let now = self.now();
        match payout.state {
            PayoutState::Queued => self.sign_and_submit(&payout, now).await?,
            PayoutState::Signed | PayoutState::Submitted => {
                self.poll_or_resubmit(&payout, now).await?
            }
            PayoutState::Confirmed | PayoutState::Failed => {}
        }
        Ok(true)
    }

    async fn sign_and_submit(&self, payout: &PayoutRow, now: u64) -> Result<(), GamesError> {
        if payout.address == self.config.payout_address {
            // Intake refuses this address, so it can only be here through a bug or a hand-edited database.
            // Release the balance and move on: pausing every player's payout for one bad row is the worse harm.
            self.store.settle_payout(
                &payout.id,
                false,
                Some("the recipient is the payout wallet itself"),
                now,
            )?;
            return Ok(());
        }
        let Some(signer) = &self.signer else {
            self.pause_payments("the payout signer is not loaded");
            return Ok(());
        };
        if payout.attempts >= self.config.max_submission_attempts {
            // Never signed, so nothing can have been paid: release the balance and say why.
            self.store
                .settle_payout(&payout.id, false, Some("gave up before signing"), now)?;
            return Ok(());
        }
        let account = match self.node.account(&self.config.payout_address).await {
            Ok(account) => account,
            Err(NodeClientError::Unavailable) => {
                self.store
                    .record_attempt(&payout.id, Some("node unavailable"), now)?;
                return Ok(());
            }
            Err(_) => {
                self.store.record_attempt(
                    &payout.id,
                    Some("node did not answer the account query"),
                    now,
                )?;
                return Ok(());
            }
        };
        if account.address != self.config.payout_address
            || account.decimals != UltraBlockchain::ULTRA_DECIMALS
        {
            self.pause_payments("the node described a different payout account than configured");
            return Ok(());
        }
        let Some(balance) = account.balance_base_units() else {
            self.store
                .record_attempt(&payout.id, Some("node did not report a balance"), now)?;
            return Ok(());
        };
        let estimate = match self
            .node
            .estimate(&payout.address, payout.amount_units)
            .await
        {
            Ok(estimate) => estimate,
            Err(NodeClientError::Unavailable) => {
                self.store
                    .record_attempt(&payout.id, Some("node unavailable"), now)?;
                return Ok(());
            }
            Err(_) => {
                self.store.record_attempt(
                    &payout.id,
                    Some("node refused the fee estimate"),
                    now,
                )?;
                return Ok(());
            }
        };
        let estimate_ok = estimate.recipient == payout.address
            && estimate.amount == payout.amount_units
            && estimate.gas_limit == 500_000
            && estimate.gas_price == 1
            && estimate.amount.checked_add(estimate.fee) == Some(estimate.total);
        if !estimate_ok {
            self.pause_payments(
                "the node's fee estimate does not match the payout it was asked about",
            );
            return Ok(());
        }
        let fee = estimate.fee.max(rules::payout_fee(payout.amount_units));
        let required = payout
            .amount_units
            .checked_add(fee)
            .and_then(|sum| sum.checked_add(self.policy.payout_reserve_units))
            .ok_or_else(|| GamesError::Internal("payout arithmetic overflowed".into()))?;
        if balance < required {
            // The pool is short. Keep the payout queued and the player's balance intact; stop paying so
            // an operator tops the wallet up, and nothing is lost in the meantime.
            self.store.record_attempt(
                &payout.id,
                Some("the payout wallet balance is below the amount, fee and reserve"),
                now,
            )?;
            self.pause_payments("the payout wallet balance is below what the next payout needs");
            return Ok(());
        }
        let (envelope, hash) = match signer.sign_transfer(
            &payout.address,
            payout.amount_units,
            fee,
            account.nonce,
            now,
        ) {
            Ok(signed) => signed,
            Err(_) => {
                self.pause_payments("the payout signer refused to sign");
                return Ok(());
            }
        };
        let json = serde_json::to_string(&envelope)?;
        // Persist before submitting: from here on, every retry is these exact bytes.
        self.store.mark_signed(
            &payout.id,
            &json,
            &hex::encode(hash),
            account.nonce,
            fee,
            now,
        )?;
        self.submit_envelope(&payout.id, &envelope, &hash, now)
            .await
    }

    async fn submit_envelope(
        &self,
        id: &str,
        envelope: &SignedTransferRequest,
        hash: &[u8; 32],
        now: u64,
    ) -> Result<(), GamesError> {
        match self.node.submit(envelope).await {
            Ok(accepted) => {
                if !accepted.hash.eq_ignore_ascii_case(&hex::encode(hash)) {
                    self.pause_payments("the node returned a different hash for the signed payout");
                    return Ok(());
                }
                self.store.mark_submitted(id, now)?;
                Ok(())
            }
            Err(NodeClientError::Unavailable) | Err(NodeClientError::InvalidResponse) => {
                // Ambiguous: it may have arrived. Keep the envelope and retry the same bytes.
                self.store
                    .record_attempt(id, Some("submission outcome unknown"), now)?;
                Ok(())
            }
            Err(_) => {
                // The node refused a request we signed. It is not on the chain, but re-signing
                // automatically could race a stale nonce, so a human decides.
                self.store
                    .record_attempt(id, Some("the node rejected the signed payout"), now)?;
                self.pause_payments(
                    "the node rejected a signed payout; resolve it with the operator command",
                );
                Ok(())
            }
        }
    }

    async fn poll_or_resubmit(&self, payout: &PayoutRow, now: u64) -> Result<(), GamesError> {
        let (Some(json), Some(hash_hex)) = (&payout.envelope, &payout.tx_hash) else {
            self.pause_payments("a signed payout is missing its envelope or hash");
            return Ok(());
        };
        let expected: [u8; 32] = hex::decode(hash_hex)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| GamesError::Internal("stored payout hash is malformed".into()))?;
        let signed_at = payout.signed_at.unwrap_or(payout.created_at);
        let age = now.saturating_sub(signed_at);
        let timed_out = age >= self.config.confirmation_timeout_seconds
            || payout.attempts >= self.config.max_submission_attempts;

        match self.node.transaction_status(hash_hex).await {
            Ok(status) if !status.hash.eq_ignore_ascii_case(hash_hex) => {
                self.pause_payments(
                    "the node answered about a different transaction than the payout",
                );
                Ok(())
            }
            Ok(status) if status.status == "confirmed" => {
                self.store.settle_payout(&payout.id, true, None, now)?;
                Ok(())
            }
            Ok(_) => {
                if timed_out {
                    self.pause_payments(
                        "a payout was not confirmed in time; resolve it with the operator command",
                    );
                }
                Ok(())
            }
            Err(NodeClientError::Unavailable) => {
                self.store.record_attempt(
                    &payout.id,
                    Some("node unavailable while checking"),
                    now,
                )?;
                Ok(())
            }
            Err(_) => {
                if timed_out {
                    self.pause_payments("a payout could not be found on the node in time; resolve it with the operator command");
                    return Ok(());
                }
                // The node does not know it (it may have dropped it): submit the identical bytes again.
                let envelope: SignedTransferRequest = serde_json::from_str(json)?;
                self.submit_envelope(&payout.id, &envelope, &expected, now)
                    .await
            }
        }
    }

    pub async fn run_worker(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let mut last_daily = 0u64;
        loop {
            if *shutdown.borrow() {
                return;
            }
            match self.process_payout_once().await {
                Ok(true) => {}
                Ok(false) => {}
                Err(error) => tracing::warn!(error = %error, "games payout step failed"),
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
                _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                _ = shutdown.changed() => {}
            }
        }
    }

    // --- closing and anchoring a day --------------------------------------------------------

    fn results_roots(
        &self,
        game: &str,
        date: &str,
        commitment: Option<&[u8; 32]>,
    ) -> Result<([u8; 32], [u8; 32]), GamesError> {
        let wins = self.store.wins_for_day(game, date)?;
        let malformed =
            |what: &str| GamesError::Internal(format!("a stored win is malformed: {what}"));
        match game {
            GAME_GRID => {
                let mut results = Vec::with_capacity(wins.len());
                for win in &wins {
                    let detail: serde_json::Value = serde_json::from_str(&win.detail)?;
                    let puzzle_id: [u8; 32] = detail["puzzle_id"]
                        .as_str()
                        .and_then(|id| hex::decode(id).ok())
                        .and_then(|bytes| bytes.try_into().ok())
                        .ok_or_else(|| malformed("puzzle id"))?;
                    results.push(GridResult {
                        address: roots::address_bytes(&win.address)
                            .ok_or_else(|| malformed("address"))?,
                        level: win.item.parse().map_err(|_| malformed("level"))?,
                        moves: u16::try_from(win.moves).map_err(|_| malformed("moves"))?,
                        puzzle_id,
                    });
                }
                let results_root = roots::grid_results_root(results);
                let commitment = commitment.ok_or_else(|| {
                    GamesError::Internal("a grid day needs its commitment".into())
                })?;
                Ok((
                    results_root,
                    roots::grid_day_root(date, commitment, &results_root),
                ))
            }
            GAME_CHESS => {
                let mut results = Vec::with_capacity(wins.len());
                for win in &wins {
                    let detail: serde_json::Value = serde_json::from_str(&win.detail)?;
                    results.push(ChessResult {
                        address: roots::address_bytes(&win.address)
                            .ok_or_else(|| malformed("address"))?,
                        game_id_hash: roots::chess_game_id_hash(&win.item),
                        mode: detail["mode"]
                            .as_str()
                            .and_then(roots::chess_mode_code)
                            .ok_or_else(|| malformed("mode"))?,
                        plies: u16::try_from(win.moves).map_err(|_| malformed("plies"))?,
                    });
                }
                let results_root = roots::chess_results_root(results);
                Ok((results_root, roots::chess_day_root(date, &results_root)))
            }
            other => Err(GamesError::Internal(format!("unknown game {other}"))),
        }
    }

    /// Fix a finished day's roots, then anchor it. Safe to run twice: a day closes once, and an anchor
    /// is only submitted after the ledger has been checked for it.
    pub async fn close_and_anchor(&self, game: &str, date: &str) -> Result<DayRow, GamesError> {
        let _guard = self.anchor_lock.lock().await;
        self.close_and_anchor_locked(game, date).await
    }

    async fn close_and_anchor_locked(&self, game: &str, date: &str) -> Result<DayRow, GamesError> {
        let today = self.today();
        if date >= today.as_str() {
            return Err(GamesError::Invalid(
                "only a finished day can be closed".into(),
            ));
        }
        let policy = self
            .policy
            .game(game)
            .ok_or_else(|| GamesError::Invalid("unknown game".into()))?;
        let now = self.now();
        let existing = self.store.day_row(date, game)?;
        let row = match existing {
            Some(row) if row.day_root.is_some() => row,
            _ => {
                let commitment = if game == GAME_GRID {
                    Some(self.grid_day(date).await?.commitment)
                } else {
                    None
                };
                let (results_root, day_root) =
                    self.results_roots(game, date, commitment.as_ref())?;
                self.store
                    .close_day(date, game, &results_root, &day_root, now)?
            }
        };
        if matches!(row.state.as_str(), "confirmed") {
            return Ok(row);
        }
        let Some(anchorer) = &self.anchorer else {
            return Ok(row);
        };
        let root: [u8; 32] = row
            .day_root
            .clone()
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| GamesError::Internal("stored day root is malformed".into()))?;

        // Look before submitting: if the root is already on the ledger, there is nothing to do, and an
        // earlier attempt whose outcome was unclear is settled without paying for a second anchor.
        match self.chain.find_anchor(policy.chain_id, &root).await {
            Ok(Some(point)) => {
                self.store
                    .set_anchor(date, game, Some(&point.tx_hash), "confirmed", now)?;
            }
            Ok(None) => {
                if row.state == "anchored"
                    && now.saturating_sub(row.updated_at)
                        < self.config.anchor_resubmit_after_seconds
                {
                    // Submitted recently and still pending a block: wait rather than submit a duplicate.
                } else {
                    match anchorer.anchor(policy.chain_id, &root).await {
                        Ok(transaction) => self.store.set_anchor(
                            date,
                            game,
                            Some(&transaction),
                            "anchored",
                            now,
                        )?,
                        Err(error) => {
                            self.store
                                .record_anchor_error(date, game, &error.to_string(), now)?
                        }
                    }
                }
            }
            Err(_) => {
                self.store.record_anchor_error(
                    date,
                    game,
                    "the ledger could not be read to check the anchor",
                    now,
                )?;
            }
        }
        self.store
            .day_row(date, game)?
            .ok_or_else(|| GamesError::Internal("day row vanished".into()))
    }

    /// The daily pass: for each anchored game, close and anchor yesterday, and settle any earlier day
    /// still waiting for a block. Yesterday is anchored even if nobody won (a heartbeat), earlier days
    /// only if something happened on them, so a first start never back-fills a week of empty anchors.
    pub async fn run_daily(&self) -> Result<DailyReport, GamesError> {
        let _guard = self.anchor_lock.lock().await;
        let mut report = DailyReport {
            anchored: vec![],
            confirmed: vec![],
            errors: vec![],
        };
        let today = self.today();
        let now = self.now();
        if self.anchorer.is_none() {
            return Ok(report);
        }
        for game in self.config.anchor_games.clone() {
            for ago in (1..=self.config.catch_up_days as i64).rev() {
                let Some(date) = shift_date(&today, -ago) else {
                    continue;
                };
                let row = self.store.day_row(&date, &game)?;
                match &row {
                    Some(row) if row.state == "confirmed" => continue,
                    Some(row)
                        if row.last_error.is_some()
                            && now.saturating_sub(row.updated_at)
                                < self.config.anchor_retry_seconds =>
                    {
                        continue
                    }
                    _ => {}
                }
                let had_wins = !self.store.wins_for_day(&game, &date)?.is_empty();
                let heartbeat = ago == 1 && self.config.anchor_empty_days;
                if row.is_none() && !had_wins && !heartbeat {
                    continue;
                }
                match self.close_and_anchor_locked(&game, &date).await {
                    Ok(updated) => match updated.state.as_str() {
                        "confirmed" => report.confirmed.push((game.clone(), date.clone())),
                        "anchored" => report.anchored.push((game.clone(), date.clone())),
                        _ => {
                            if let Some(error) = updated.last_error {
                                report.errors.push((game.clone(), date.clone(), error));
                            }
                        }
                    },
                    Err(error) => {
                        report
                            .errors
                            .push((game.clone(), date.clone(), error.to_string()))
                    }
                }
            }
        }
        Ok(report)
    }
}
