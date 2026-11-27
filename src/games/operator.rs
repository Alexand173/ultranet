//! The platform operator's actions, written against the store alone so the CLI can run them without a signer, a
//! node or a network, and the service can offer the same code. None of these is reachable over HTTP: a game owner
//! can propose and fund, but only the operator can approve a policy, name an adapter, or settle a payout in doubt.
//!
//! Every action writes an audit row naming the operator, the exact subject and the reason.

use rusqlite::TransactionBehavior;

use super::{
    adapter::{AdapterModule, RuntimeLimits},
    catalog::{self, AuditRecord, CatalogGame},
    pool_payouts::PoolPayoutSettled,
    pools::{
        check_platform_limits, operator_actor, operator_digest, PolicyRow, PolicyTerms, ACTION_ADAPTER_APPROVE, ACTION_POLICY_APPROVE, ACTION_POLICY_REJECT,
    },
    service::GamesError,
    store::{GamesStore, StoreError},
};

pub const ACTION_PAYOUT_RESOLVE: &str = "pool-payout-resolve";

fn known(store: &GamesStore, game_id: &str) -> Result<CatalogGame, GamesError> {
    store.catalog_game(game_id)?.ok_or_else(|| GamesError::NotFound("no such game".into()))
}

/// Approve or reject a proposed policy. Approval re-checks the platform ceilings and requires that the pool can
/// pay at least one reward, so an approved policy never promises what cannot be paid. It funds nothing.
pub fn decide_policy(store: &GamesStore, game_id: &str, version: u32, approve: bool, operator: &str, note: &str, now: u64) -> Result<PolicyRow, GamesError> {
    known(store, game_id)?;
    // The CLI also insists on a note; the audit row always names the operator.
    if operator.trim().is_empty() {
        return Err(GamesError::Invalid("an operator name is required".into()));
    }
    let row = store.policy(game_id, version)?.ok_or_else(|| GamesError::NotFound("no such policy version".into()))?;
    if approve {
        check_platform_limits(&PolicyTerms {
            reward_units: row.reward_units,
            eligible_outcomes: row.eligible_outcomes.clone(),
            per_player_per_day: row.per_player_per_day,
            per_game_per_day: row.per_game_per_day,
            effective_from: row.effective_from,
        })?;
        let pool = store.pool_view(game_id)?;
        if pool.available < row.reward_units {
            return Err(GamesError::Forbidden("the game's pool cannot pay even one reward at this level".into()));
        }
    }
    let action = if approve { ACTION_POLICY_APPROVE } else { ACTION_POLICY_REJECT };
    let actor = operator_actor(operator);
    let zeros = "0".repeat(64);
    let detail = serde_json::json!({ "operator": operator, "version": version, "note": note, "policy_hash": row.policy_hash }).to_string();
    Ok(store.policy_decide(
        game_id,
        version,
        approve,
        &actor,
        note,
        now,
        &AuditRecord { actor: &actor, action, chain_owner: &zeros, detail: &detail, digest: operator_digest(action, game_id, &detail, now) },
    )?)
}

/// Bind a game to an adapter by hash. Playing needs the bytes to be stored as well (see [`register_adapter`]).
pub fn approve_adapter(store: &GamesStore, game_id: &str, module_hash: [u8; 32], adapter_version: u32, operator: &str, now: u64) -> Result<CatalogGame, GamesError> {
    known(store, game_id)?;
    let actor = operator_actor(operator);
    let zeros = "0".repeat(64);
    let detail = serde_json::json!({ "operator": operator, "module_hash": hex::encode(module_hash), "adapter_version": adapter_version }).to_string();
    Ok(store.platform_approve_adapter(
        game_id,
        &module_hash,
        adapter_version,
        now,
        &AuditRecord { actor: &actor, action: ACTION_ADAPTER_APPROVE, chain_owner: &zeros, detail: &detail, digest: operator_digest(ACTION_ADAPTER_APPROVE, game_id, &detail, now) },
    )?)
}

/// Check a module's structure with the same loader the runtime uses, keep its exact bytes and bind the game to its
/// hash. Nothing runs here. A module the host rejects is never stored and never replaces an earlier approval.
pub fn register_adapter(store: &GamesStore, game_id: &str, bytes: &[u8], adapter_version: u32, operator: &str, now: u64) -> Result<String, GamesError> {
    known(store, game_id)?;
    let module = AdapterModule::compile(bytes, RuntimeLimits::default()).map_err(|error| GamesError::Invalid(format!("the module was refused: {error}")))?;
    let hash = store.module_put(bytes, adapter_version, operator, now)?;
    if hash != hex::encode(module.hash()) {
        return Err(GamesError::Internal("the module hash disagrees with the stored copy".into()));
    }
    approve_adapter(store, game_id, module.hash(), adapter_version, operator, now)?;
    Ok(hash)
}

/// The operator settles a pool payout as confirmed after looking its transaction up on the chain. This is the only
/// manual way to end a payout in doubt, and it is audited with the operator's note. It only moves a payout that is
/// signed or submitted, so a finished payout cannot be changed, and it never releases money that may have left.
pub fn confirm_pool_payout(store: &GamesStore, payout_id: &str, operator: &str, note: &str, now: u64) -> Result<PoolPayoutSettled, GamesError> {
    if operator.trim().is_empty() || note.trim().is_empty() {
        return Err(GamesError::Invalid("an operator name and what was checked on the chain are required".into()));
    }
    let payout = store.pool_payout(payout_id)?.ok_or_else(|| GamesError::NotFound("no such payout".into()))?;
    let settled = store.pool_payout_confirm(payout_id, now)?;
    let actor = operator_actor(operator);
    let zeros = "0".repeat(64);
    let detail = serde_json::json!({ "operator": operator, "payout_id": payout_id, "kind": payout.kind, "tx_hash": payout.tx_hash, "note": note }).to_string();
    let record = AuditRecord { actor: &actor, action: ACTION_PAYOUT_RESOLVE, chain_owner: &zeros, detail: &detail, digest: operator_digest(ACTION_PAYOUT_RESOLVE, &payout.game_id, &detail, now) };
    let mut guard = store.lock()?;
    let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate).map_err(StoreError::from)?;
    catalog::write_audit(&tx, &payout.game_id, now, &record)?;
    tx.commit().map_err(StoreError::from)?;
    Ok(settled)
}
