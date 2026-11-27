//! Playing a catalog game: a wallet-authenticated player submits an input, the game's approved adapter judges it
//! deterministically, and only a verdict the host stands behind can earn a reward from that game's own pool.
//!
//! Three things are kept apart, and each is a fact the player can read back:
//! * **verified**: two fresh runs of the approved module agreed on every output byte. No money has moved.
//! * **reward reserved**: the game's pool set the reward aside and a payout is queued or in flight.
//! * **reward paid**: the node confirmed the transfer. This is the only final state that pays.
//!
//! A game owner can never run code here: the module is stored by the platform operator under its hash, and a
//! game is bound to exactly one such hash. A result nobody approved an adapter for is not judged at all.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha3::{Digest, Sha3_256};

use super::{
    adapter::{AdapterError, AdapterModule, Outcome, RuntimeLimits, ValidatedResult},
    catalog::STATUS_ACTIVE,
    pools::{credit_from, read_credit, PoolError, CREDIT_COLUMNS, SCHEME_VERIFIED_ADAPTER},
    service::{GamesError, GamesService},
    store::{GamesStore, StoreError},
};

pub(super) const MODULES_SCHEMA: &str = "
CREATE TABLE game_adapter_modules (
    module_hash TEXT PRIMARY KEY CHECK (length(module_hash) = 64),
    module_bytes BLOB NOT NULL CHECK (length(module_bytes) BETWEEN 1 AND 262144),
    adapter_version INTEGER NOT NULL CHECK (adapter_version >= 1),
    registered_by TEXT NOT NULL,
    created_at INTEGER NOT NULL
);";

/// What a player is told about one result. `stage` is the whole story: nothing but `reward_paid` has paid.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayView {
    pub game_id: String,
    pub result_id: String,
    pub outcome: &'static str,
    pub score: u32,
    pub stage: &'static str,
    pub reward_units: Option<u64>,
    pub credit_id: Option<String>,
    pub payout_state: Option<String>,
    /// Why a verified result earned nothing (the policy does not pay for it, a limit, an empty pool, a closed gate).
    pub not_rewarded: Option<&'static str>,
}

pub const STAGE_VERIFIED: &str = "verified";
pub const STAGE_RESERVED: &str = "reward_reserved";
pub const STAGE_PAID: &str = "reward_paid";
pub const STAGE_RELEASED: &str = "reward_released";

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Invalid => "invalid",
        Outcome::Win => "win",
        Outcome::Loss => "loss",
        Outcome::Tie => "tie",
        Outcome::Cancelled => "cancelled",
    }
}

impl GamesStore {
    /// Keep a module's exact bytes under its hash. The hash is recomputed here, never trusted from the caller.
    pub fn module_put(&self, bytes: &[u8], adapter_version: u32, registered_by: &str, now: u64) -> Result<String, StoreError> {
        let hash = hex::encode(Sha3_256::digest(bytes));
        let guard = self.lock()?;
        guard.execute(
            "INSERT OR IGNORE INTO game_adapter_modules (module_hash, module_bytes, adapter_version, registered_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![hash, bytes, i64::from(adapter_version), registered_by, i64::try_from(now).map_err(|_| StoreError::NumericRange)?],
        )?;
        Ok(hash)
    }

    pub fn module_bytes(&self, module_hash: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let guard = self.lock()?;
        Ok(guard
            .query_row("SELECT module_bytes FROM game_adapter_modules WHERE module_hash = ?1", params![module_hash], |row| row.get(0))
            .optional()?)
    }

    pub fn pool_credit_for_result(&self, game_id: &str, result_id: &str) -> Result<Option<super::pools::Credit>, PoolError> {
        let guard = self.lock()?;
        let row = guard
            .query_row(
                &format!("SELECT {CREDIT_COLUMNS} FROM game_pool_credits WHERE game_id = ?1 AND result_id = ?2"),
                params![game_id, result_id],
                read_credit,
            )
            .optional()?;
        row.map(|row| credit_from(row).map_err(PoolError::from)).transpose()
    }
}

impl GamesService {
    /// The platform operator's step; see `operator::register_adapter`.
    pub fn platform_register_adapter(&self, game_id: &str, bytes: &[u8], adapter_version: u32, operator: &str) -> Result<String, GamesError> {
        super::operator::register_adapter(self.store(), game_id, bytes, adapter_version, operator, self.now())
    }

    /// Judge one input for a signed-in player and, if the policy pays for the verdict, reserve the reward from
    /// the game's own pool. Idempotent per `(game, result_id)`: asking again returns the same facts.
    pub async fn play(&self, game_id: &str, player: &str, result_id: &str, payload_hex: &str) -> Result<PlayView, GamesError> {
        let player = self.normalize_address(player)?;
        if result_id.is_empty() || result_id.len() > 128 {
            return Err(GamesError::Invalid("result_id must be 1 to 128 characters".into()));
        }
        self.check_play_rate(&player)?;
        let payload = hex::decode(payload_hex).map_err(|_| GamesError::Invalid("payload must be hexadecimal".into()))?;
        let game = self.catalog_game(game_id)?;
        // A payload that declares which day it judges must declare this chain's current one. The module
        // cannot check a clock, so this is the host's rule; see `play_context`.
        if let Some(declared) = super::play_context::declared_day_context(&payload) {
            self.require_current_day(game.chain_id, &declared, self.now()).await?;
        }
        if game.status != STATUS_ACTIVE {
            return Err(GamesError::Forbidden("this game is not active".into()));
        }
        // No approved adapter, no judgement: a game that cannot be verified is never run and never pays.
        let module_hash = match (game.validation_scheme.as_str(), game.adapter_ref.as_deref()) {
            (SCHEME_VERIFIED_ADAPTER, Some(hash)) => hash.to_string(),
            _ => return Err(GamesError::Forbidden("this game has no approved adapter, so its results are record-only".into())),
        };
        let bytes = self
            .store()
            .module_bytes(&module_hash)?
            .ok_or_else(|| GamesError::Internal("the approved adapter's bytes are not stored".into()))?;
        let player_bytes: [u8; 32] = hex::decode(&player).ok().and_then(|raw| raw.try_into().ok()).ok_or_else(|| GamesError::Invalid("bad player address".into()))?;
        let expected = module_hash.clone();
        let validated = tokio::task::spawn_blocking(move || -> Result<ValidatedResult, AdapterError> {
            let module = AdapterModule::compile(&bytes, RuntimeLimits::default())?;
            if hex::encode(module.hash()) != expected {
                return Err(AdapterError::Instantiate("the stored module is not the approved one".into()));
            }
            module.validate_result(&player_bytes, &payload)
        })
        .await
        .map_err(|_| GamesError::Internal("the adapter task failed".into()))?
        .map_err(|error| GamesError::Invalid(format!("the adapter could not judge this input: {error}")))?;

        let base = PlayView {
            game_id: game_id.to_string(),
            result_id: result_id.to_string(),
            outcome: outcome_name(validated.output.outcome),
            score: validated.output.score,
            stage: STAGE_VERIFIED,
            reward_units: None,
            credit_id: None,
            payout_state: None,
            not_rewarded: None,
        };
        if !self.service_config().pool_payouts_enabled {
            return Ok(PlayView { not_rewarded: Some("rewards_not_enabled"), ..base });
        }
        match self.reserve_reward_for_payout(game_id, result_id, &player, &validated) {
            Ok(credit) => self.result_view(game_id, result_id, Some(credit)),
            Err(GamesError::Forbidden(_)) => {
                // A replay of a released reward is a refusal with its own stage; the rest are plain "not paid".
                if let Some(credit) = self.store().pool_credit_for_result(game_id, result_id)? {
                    if credit.player == player {
                        return self.result_view(game_id, result_id, Some(credit));
                    }
                }
                Ok(PlayView { not_rewarded: Some(self.why_not_rewarded(game_id, &validated)), ..base })
            }
            Err(GamesError::GameLimit) => Ok(PlayView { not_rewarded: Some("daily_limit_reached"), ..base }),
            Err(other) => Err(other),
        }
    }

    pub fn pool_policies(&self, game_id: &str) -> Result<Vec<super::pools::PolicyRow>, GamesError> {
        self.catalog_game(game_id)?;
        Ok(self.store().policies(game_id)?)
    }

    pub fn pool_payout_list(&self, game_id: &str) -> Result<Vec<super::pool_payouts::PoolPayout>, GamesError> {
        self.catalog_game(game_id)?;
        Ok(self.store().pool_payouts_for(game_id)?)
    }

    fn why_not_rewarded(&self, game_id: &str, validated: &ValidatedResult) -> &'static str {
        match self.store().policy_active(game_id, self.now()) {
            Ok(Some(policy)) => {
                if !policy.eligible_outcomes.split(',').any(|name| name == outcome_name(validated.output.outcome)) {
                    "policy_does_not_pay_this_outcome"
                } else {
                    "pool_cannot_cover_the_reward"
                }
            }
            _ => "no_approved_reward_policy",
        }
    }

    /// Read back one result's stage: verified, reserved, paid or released. `paid` means the node confirmed it.
    pub fn result_view(&self, game_id: &str, result_id: &str, credit: Option<super::pools::Credit>) -> Result<PlayView, GamesError> {
        self.catalog_game(game_id)?;
        let credit = match credit {
            Some(credit) => credit,
            None => self
                .store()
                .pool_credit_for_result(game_id, result_id)?
                .ok_or_else(|| GamesError::NotFound("no reward was reserved for that result".into()))?,
        };
        let payout_state = self
            .store()
            .pool_payouts_for(game_id)?
            .into_iter()
            .find(|payout| payout.kind == "reward" && payout.ref_id == credit.credit_id)
            .map(|payout| payout.state);
        let stage = match credit.state.as_str() {
            "paid" => STAGE_PAID,
            "released" => STAGE_RELEASED,
            _ => STAGE_RESERVED,
        };
        Ok(PlayView {
            game_id: game_id.to_string(),
            result_id: result_id.to_string(),
            outcome: "win",
            score: 0,
            stage,
            reward_units: Some(credit.amount_units),
            credit_id: Some(credit.credit_id),
            payout_state,
            not_rewarded: None,
        })
    }
}

// --- what a phone needs to show: the store, a player's rewards, a pool's funding ---------------------

/// One game as the store lists it. `owner` is read from the ledger now; it is `None` when the node cannot say.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StoreEntry {
    pub game_id: String,
    pub chain_id: u32,
    pub name: String,
    pub description: String,
    pub status: String,
    /// `record_only` or `verified_adapter`: how this game's results are judged.
    pub validation_scheme: String,
    /// The SHA3-256 of the approved verifier module (the game's stable verifier identity), when one is bound.
    pub adapter_hash: Option<String>,
    pub owner: Option<String>,
    pub playable: bool,
    /// Why it cannot be played, when it cannot.
    pub playable_reason: Option<&'static str>,
    pub rewards: &'static str,
    pub reward_units: Option<u64>,
    /// Why no reward is on offer, when none is.
    pub rewards_reason: Option<&'static str>,
}

/// One of a player's results that earned or reserved a reward, with where its payout stands.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerResult {
    pub game_id: String,
    pub result_id: String,
    pub credit_id: String,
    pub reward_units: u64,
    pub stage: &'static str,
    pub payout_state: Option<String>,
    pub transaction_hash: Option<String>,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Deposit {
    pub tx_hash: String,
    pub funder: String,
    pub amount_units: u64,
    pub claimed_at: u64,
}

fn stage_of(state: &str) -> &'static str {
    match state {
        "paid" => STAGE_PAID,
        "released" => STAGE_RELEASED,
        _ => STAGE_RESERVED,
    }
}

impl GamesStore {
    /// A player's credits, newest first, with each payout's state and transaction hash where there is one.
    pub fn player_results(&self, player: &str, limit: u32) -> Result<Vec<PlayerResult>, PoolError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare(
            "SELECT c.game_id, c.result_id, c.credit_id, c.amount_units, c.state, p.state, p.tx_hash, c.created_at
             FROM game_pool_credits c
             LEFT JOIN game_pool_payouts p ON p.kind = 'reward' AND p.ref_id = c.credit_id
             WHERE c.player = ?1 ORDER BY c.created_at DESC, c.credit_id LIMIT ?2",
        )?;
        let rows = statement.query_map(params![player, i64::from(limit)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, i64>(7)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (game_id, result_id, credit_id, amount, state, payout_state, tx, at) = row?;
            out.push(PlayerResult {
                game_id,
                result_id,
                credit_id,
                reward_units: super::store::to_u64(amount)?,
                stage: stage_of(&state),
                payout_state,
                transaction_hash: tx,
                at: super::store::to_u64(at)?,
            });
        }
        Ok(out)
    }

    pub fn pool_deposits_for(&self, game_id: &str) -> Result<Vec<Deposit>, PoolError> {
        let guard = self.lock()?;
        let mut statement = guard.prepare("SELECT tx_hash, funder, amount_units, claimed_at FROM game_pool_deposits WHERE game_id = ?1 ORDER BY claimed_at DESC, tx_hash")?;
        let rows = statement.query_map(params![game_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (tx_hash, funder, amount, at) = row?;
            out.push(Deposit { tx_hash, funder, amount_units: super::store::to_u64(amount)?, claimed_at: super::store::to_u64(at)? });
        }
        Ok(out)
    }
}

impl GamesService {
    /// Every game, with who owns its chain now and whether it can be played and rewarded, so a screen never has to
    /// guess. `query` matches the name or the game id; `chain_id` narrows to one AppChain. At most 100 entries.
    pub async fn store_listing(&self, query: Option<&str>, chain_id: Option<u32>) -> Result<Vec<StoreEntry>, GamesError> {
        let needle = query.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
        let games: Vec<_> = self
            .catalog_games(chain_id)?
            .into_iter()
            .filter(|game| needle.as_deref().map_or(true, |n| game.name.to_lowercase().contains(n) || game.game_id.contains(n)))
            .take(100)
            .collect();
        let mut owners: std::collections::HashMap<u32, Option<String>> = std::collections::HashMap::new();
        for game in &games {
            if !owners.contains_key(&game.chain_id) {
                owners.insert(game.chain_id, self.chain().chain_owner(game.chain_id).await.ok().flatten());
            }
        }
        let mut entries = Vec::with_capacity(games.len());
        for game in games {
            let has_module = match game.adapter_ref.as_deref() {
                Some(hash) => self.store().module_bytes(hash)?.is_some(),
                None => false,
            };
            let playable_reason = if game.status != STATUS_ACTIVE {
                Some("game_not_active")
            } else if game.validation_scheme != SCHEME_VERIFIED_ADAPTER || game.adapter_ref.is_none() {
                Some("record_only_no_approved_adapter")
            } else if !has_module {
                Some("adapter_module_not_installed")
            } else {
                None
            };
            let (rewards, reward_units, rewards_reason) = if playable_reason.is_some() {
                ("none", None, Some("not_playable"))
            } else if !self.service_config().pool_payouts_enabled {
                ("none", None, Some("rewards_not_enabled"))
            } else {
                match self.store().policy_active(&game.game_id, self.now())? {
                    None => ("none", None, Some("no_approved_reward_policy")),
                    Some(policy) => {
                        if self.store().pool_view(&game.game_id)?.available < policy.reward_units {
                            ("none", Some(policy.reward_units), Some("pool_cannot_cover_the_reward"))
                        } else {
                            ("available", Some(policy.reward_units), None)
                        }
                    }
                }
            };
            entries.push(StoreEntry {
                owner: owners.get(&game.chain_id).cloned().flatten(),
                validation_scheme: game.validation_scheme,
                adapter_hash: game.adapter_ref,
                game_id: game.game_id,
                chain_id: game.chain_id,
                name: game.name,
                description: game.description,
                status: game.status,
                playable: playable_reason.is_none(),
                playable_reason,
                rewards,
                reward_units,
                rewards_reason,
            });
        }
        Ok(entries)
    }

    pub fn player_result_list(&self, address: &str) -> Result<Vec<PlayerResult>, GamesError> {
        let address = self.normalize_address(address)?;
        Ok(self.store().player_results(&address, 100)?)
    }

    pub fn pool_deposit_list(&self, game_id: &str) -> Result<Vec<Deposit>, GamesError> {
        self.catalog_game(game_id)?;
        Ok(self.store().pool_deposits_for(game_id)?)
    }
}
