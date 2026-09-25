//! Opt-in, redacted local validator-state diagnostics.
//!
//! This route deliberately emits allow-listed summary DTOs instead of serializing
//! persisted governance and block records directly. It is absent unless explicitly
//! enabled at startup, and it is protected by both loopback-peer and admin checks.

use crate::{
    bls_aggregation::ValidatorInfo, UltraBlock, ValidatorApprovalRecord, ValidatorJoinProposalData,
};
use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    middleware::{from_fn, Next},
    web, Error, HttpRequest, HttpResponse, Responder,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    env,
    io::{self, ErrorKind},
    net::SocketAddr,
};

const ENABLE_ENV: &str = "ULTRANET_ENABLE_LOCAL_VALIDATOR_SNAPSHOT";
const ROUTE: &str = "/api/local/validator-snapshot";
const MAX_DETAIL_ROWS: usize = 100;

#[derive(Debug, Serialize)]
struct Snapshot {
    generated_at: u64,
    source: &'static str,
    consistency: ConsistencySummary,
    persisted_validators: ValidatorSetSummary,
    in_memory_validators: ValidatorSetSummary,
    pending_proposals: ProposalSummary,
    approval_journal: ApprovalSummary,
    block_history: BlockHistorySummary,
}

#[derive(Debug, Serialize)]
struct ConsistencySummary {
    block_height_stable_during_capture: bool,
    cross_tree_snapshot_atomic: bool,
    note: &'static str,
}

#[derive(Debug, Serialize)]
struct ValidatorSetSummary {
    total: usize,
    active: usize,
    inactive: usize,
    active_weight: u64,
    key_length_distribution: BTreeMap<usize, usize>,
    approval_journal_match_count: usize,
    entries_truncated: bool,
    entries: Vec<ValidatorEntry>,
}

#[derive(Debug, Serialize)]
struct ValidatorEntry {
    public_key_bytes: usize,
    is_active: bool,
    weight: u64,
    joined_at: u64,
    last_epoch: u64,
    stake: u64,
    rewards: u64,
    slash_count: u32,
    approval_journal_match: bool,
}

#[derive(Debug, Serialize)]
struct ProposalSummary {
    total: usize,
    details_truncated: bool,
    entries: Vec<ProposalEntry>,
}

#[derive(Debug, Serialize)]
struct ProposalEntry {
    public_key_bytes: usize,
    timestamp: u64,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ApprovalSummary {
    total: usize,
    active_validator_records: usize,
    details_truncated: bool,
    entries: Vec<ApprovalEntry>,
}

#[derive(Debug, Serialize)]
struct ApprovalEntry {
    recorded_at: u64,
    approval_transaction_timestamp: u64,
    proposed_public_key_bytes: usize,
    activated_public_key_bytes: usize,
    is_active: bool,
    weight: u64,
    joined_at: u64,
    last_epoch: u64,
    stake: u64,
    rewards: u64,
    slash_count: u32,
}

#[derive(Debug, Serialize)]
struct BlockHistorySummary {
    total_blocks: usize,
    first_index: Option<u64>,
    last_index: Option<u64>,
    detail_limit: usize,
    details_truncated: bool,
    key_length_distribution: BTreeMap<usize, usize>,
    blocks: Vec<BlockEntry>,
}

#[derive(Debug, Serialize)]
struct BlockEntry {
    index: u64,
    timestamp: u64,
    transaction_count: usize,
    validator_set_entries: usize,
    unique_validator_keys: usize,
    validator_key_length_distribution: BTreeMap<usize, usize>,
    entries_matching_active_persisted_keys: usize,
    entries_matching_active_in_memory_keys: usize,
    entries_matching_approval_journal_keys: usize,
    has_aggregated_signature: bool,
    aggregate_signer_count: Option<usize>,
    aggregate_weight: Option<u64>,
}

pub(super) fn configured_enabled() -> io::Result<bool> {
    let raw = match env::var(ENABLE_ENV) {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("{ENABLE_ENV} must be valid UTF-8"),
            ));
        }
    };
    parse_enabled(raw.as_deref())
}

fn parse_enabled(raw: Option<&str>) -> io::Result<bool> {
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        None | Some("0") | Some("false") | Some("no") | Some("") => Ok(false),
        Some("1") | Some("true") | Some("yes") => Ok(true),
        Some(_) => Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("{ENABLE_ENV} must be true/false or 1/0"),
        )),
    }
}

pub(super) fn validate_listener_bind(enabled: bool, bind: &str) -> io::Result<()> {
    if !enabled {
        return Ok(());
    }
    let address = bind.parse::<SocketAddr>().map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "local validator snapshot requires a valid loopback API bind address",
        )
    })?;
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "local validator snapshot may only be enabled on a loopback API bind address",
        ));
    }
    Ok(())
}

pub(super) fn configure(config: &mut web::ServiceConfig, enabled: bool) {
    if enabled {
        config.service(
            web::resource(ROUTE)
                .wrap(from_fn(require_loopback_peer))
                .wrap(from_fn(require_admin_bearer_token))
                .route(web::get().to(snapshot_handler)),
        );
    }
}

async fn require_loopback_peer<B>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    if request
        .peer_addr()
        .is_some_and(|address| address.ip().is_loopback())
    {
        return Ok(next.call(request).await?.map_into_left_body());
    }

    let response = HttpResponse::Forbidden()
        .json(serde_json::json!({"success": false, "message": "Loopback peer required"}))
        .map_into_right_body();
    Ok(request.into_response(response))
}

async fn require_admin_bearer_token<B>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    let authorized = request
        .app_data::<web::Data<super::AdminAuthConfig>>()
        .is_some_and(|config| super::bearer_token_matches(&request, config));
    if authorized {
        return Ok(next.call(request).await?.map_into_left_body());
    }

    let response = HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Bearer"))
        .json(serde_json::json!({
            "success": false,
            "message": "Administrator bearer token required"
        }))
        .map_into_right_body();
    Ok(request.into_response(response))
}

async fn snapshot_handler(
    request: HttpRequest,
    state: web::Data<super::AppState>,
) -> impl Responder {
    if !request
        .peer_addr()
        .is_some_and(|address| address.ip().is_loopback())
    {
        return HttpResponse::Forbidden()
            .insert_header(("Cache-Control", "no-store"))
            .json(serde_json::json!({"success": false, "message": "Loopback peer required"}));
    }

    let blockchain = state.blockchain.read();
    let start_height = blockchain.chain.last().map(|block| block.index);
    let persisted = match blockchain.storage.get_all_validators() {
        Ok(records) => records,
        Err(_) => return unavailable(),
    };
    let proposals = match blockchain.storage.get_all_pending_proposals() {
        Ok(records) => records.into_values().collect::<Vec<_>>(),
        Err(_) => return unavailable(),
    };
    let approvals = match blockchain.storage.get_all_approval_records() {
        Ok(records) => records,
        Err(_) => return unavailable(),
    };
    let in_memory = blockchain
        .validator
        .read()
        .validators
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let end_height = blockchain.chain.last().map(|block| block.index);
    let snapshot = build_snapshot(
        now_seconds(),
        &blockchain.chain,
        &persisted,
        &in_memory,
        &proposals,
        &approvals,
        start_height == end_height,
    );

    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(snapshot)
}

fn unavailable() -> HttpResponse {
    HttpResponse::InternalServerError()
        .insert_header(("Cache-Control", "no-store"))
        .json(serde_json::json!({
            "success": false,
            "message": "Local validator snapshot unavailable; no record details were returned"
        }))
}

fn build_snapshot(
    generated_at: u64,
    blocks: &[UltraBlock],
    persisted: &[ValidatorInfo],
    in_memory: &[ValidatorInfo],
    proposals: &[ValidatorJoinProposalData],
    approvals: &[ValidatorApprovalRecord],
    block_height_stable: bool,
) -> Snapshot {
    let journal_keys = approvals
        .iter()
        .map(|record| record.activated_validator.public_key.clone())
        .collect::<HashSet<_>>();
    let persisted_active_keys = persisted
        .iter()
        .filter(|record| record.is_active)
        .map(|record| record.public_key.clone())
        .collect::<HashSet<_>>();
    let in_memory_active_keys = in_memory
        .iter()
        .filter(|record| record.is_active)
        .map(|record| record.public_key.clone())
        .collect::<HashSet<_>>();

    let persisted_validators = validator_summary(persisted, &journal_keys);
    let in_memory_validators = validator_summary(in_memory, &journal_keys);
    let mut proposal_entries = proposals
        .iter()
        .map(|proposal| ProposalEntry {
            public_key_bytes: proposal.public_key.len(),
            timestamp: proposal.timestamp,
            status: "pending",
        })
        .collect::<Vec<_>>();
    proposal_entries.sort_by_key(|entry| entry.timestamp);
    let pending_proposals = ProposalSummary {
        total: proposals.len(),
        details_truncated: proposal_entries.len() > MAX_DETAIL_ROWS,
        entries: proposal_entries
            .into_iter()
            .rev()
            .take(MAX_DETAIL_ROWS)
            .collect(),
    };

    let mut approval_entries = approvals
        .iter()
        .map(|record| {
            let validator = &record.activated_validator;
            ApprovalEntry {
                recorded_at: record.recorded_at,
                approval_transaction_timestamp: record.approval_transaction.timestamp,
                proposed_public_key_bytes: record.proposal.public_key.len(),
                activated_public_key_bytes: validator.public_key.len(),
                is_active: validator.is_active,
                weight: validator.weight,
                joined_at: validator.joined_at,
                last_epoch: validator.last_epoch,
                stake: validator.stake,
                rewards: validator.rewards,
                slash_count: validator.slash_count,
            }
        })
        .collect::<Vec<_>>();
    approval_entries.sort_by_key(|entry| entry.recorded_at);
    let active_validator_records = approvals
        .iter()
        .filter(|record| record.activated_validator.is_active)
        .count();
    let approval_journal = ApprovalSummary {
        total: approvals.len(),
        active_validator_records,
        details_truncated: approval_entries.len() > MAX_DETAIL_ROWS,
        entries: approval_entries
            .into_iter()
            .rev()
            .take(MAX_DETAIL_ROWS)
            .collect(),
    };

    let mut block_key_lengths = BTreeMap::new();
    for block in blocks {
        for public_key in &block.validator_set {
            *block_key_lengths.entry(public_key.len()).or_insert(0) += 1;
        }
    }
    let start = blocks.len().saturating_sub(MAX_DETAIL_ROWS);
    let block_entries = blocks[start..]
        .iter()
        .map(|block| {
            block_summary(
                block,
                &persisted_active_keys,
                &in_memory_active_keys,
                &journal_keys,
            )
        })
        .collect();
    let block_history = BlockHistorySummary {
        total_blocks: blocks.len(),
        first_index: blocks.first().map(|block| block.index),
        last_index: blocks.last().map(|block| block.index),
        detail_limit: MAX_DETAIL_ROWS,
        details_truncated: blocks.len() > MAX_DETAIL_ROWS,
        key_length_distribution: block_key_lengths,
        blocks: block_entries,
    };

    Snapshot {
        generated_at,
        source: "local node process; loopback only",
        consistency: ConsistencySummary {
            block_height_stable_during_capture: block_height_stable,
            cross_tree_snapshot_atomic: false,
            note: "Sled validator, proposal, and approval trees are read sequentially; this is not a cross-tree transactional snapshot.",
        },
        persisted_validators,
        in_memory_validators,
        pending_proposals,
        approval_journal,
        block_history,
    }
}

fn validator_summary(
    records: &[ValidatorInfo],
    journal_keys: &HashSet<Vec<u8>>,
) -> ValidatorSetSummary {
    let mut distribution = BTreeMap::new();
    for record in records {
        *distribution.entry(record.public_key.len()).or_insert(0) += 1;
    }
    let mut entries = records
        .iter()
        .map(|record| ValidatorEntry {
            public_key_bytes: record.public_key.len(),
            is_active: record.is_active,
            weight: record.weight,
            joined_at: record.joined_at,
            last_epoch: record.last_epoch,
            stake: record.stake,
            rewards: record.rewards,
            slash_count: record.slash_count,
            approval_journal_match: journal_keys.contains(&record.public_key),
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| (entry.joined_at, entry.public_key_bytes, entry.weight));
    let active = records.iter().filter(|record| record.is_active).count();
    let active_weight = records
        .iter()
        .filter(|record| record.is_active)
        .fold(0u64, |total, record| total.saturating_add(record.weight));
    ValidatorSetSummary {
        total: records.len(),
        active,
        inactive: records.len().saturating_sub(active),
        active_weight,
        key_length_distribution: distribution,
        approval_journal_match_count: records
            .iter()
            .filter(|record| journal_keys.contains(&record.public_key))
            .count(),
        entries_truncated: entries.len() > MAX_DETAIL_ROWS,
        entries: entries.into_iter().take(MAX_DETAIL_ROWS).collect(),
    }
}

fn block_summary(
    block: &UltraBlock,
    persisted_active_keys: &HashSet<Vec<u8>>,
    in_memory_active_keys: &HashSet<Vec<u8>>,
    journal_keys: &HashSet<Vec<u8>>,
) -> BlockEntry {
    let mut key_lengths = BTreeMap::new();
    for public_key in &block.validator_set {
        *key_lengths.entry(public_key.len()).or_insert(0) += 1;
    }
    let unique_keys = block.validator_set.iter().collect::<HashSet<_>>();
    let matches_persisted_active = block
        .validator_set
        .iter()
        .filter(|key| persisted_active_keys.contains(*key))
        .count();
    let matches_memory_active = block
        .validator_set
        .iter()
        .filter(|key| in_memory_active_keys.contains(*key))
        .count();
    let matches_journal = block
        .validator_set
        .iter()
        .filter(|key| journal_keys.contains(*key))
        .count();
    BlockEntry {
        index: block.index,
        timestamp: block.timestamp,
        transaction_count: block.transactions.len(),
        validator_set_entries: block.validator_set.len(),
        unique_validator_keys: unique_keys.len(),
        validator_key_length_distribution: key_lengths,
        entries_matching_active_persisted_keys: matches_persisted_active,
        entries_matching_active_in_memory_keys: matches_memory_active,
        entries_matching_approval_journal_keys: matches_journal,
        has_aggregated_signature: block.aggregated_signature.is_some(),
        aggregate_signer_count: block
            .aggregated_signature
            .as_ref()
            .map(|signature| signature.aggregation_count),
        aggregate_weight: block
            .aggregated_signature
            .as_ref()
            .map(|signature| signature.total_weight),
    }
}

fn now_seconds() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProofType, Transaction, TransactionPayload};
    use actix_web::{
        http::StatusCode,
        test as actix_test,
        web::{self, ServiceConfig},
        App, HttpResponse,
    };

    fn test_validator(key: Vec<u8>, is_active: bool) -> ValidatorInfo {
        ValidatorInfo {
            public_key: key,
            weight: 7,
            is_active,
            joined_at: 10,
            last_epoch: 2,
            stake: 700,
            rewards: 4,
            slash_count: 1,
        }
    }

    fn test_approval(key: Vec<u8>) -> ValidatorApprovalRecord {
        ValidatorApprovalRecord {
            proposal_hash: [0xAB; 32],
            approval_transaction: Transaction {
                sender: "PRIVATE_SENDER_SENTINEL".to_string(),
                sender_public_key: vec![0xCD; 7],
                recipient: "PRIVATE_RECIPIENT_SENTINEL".to_string(),
                amount: 0,
                signature: vec![0xEF; 9],
                zk_proof: vec![0xFA; 11],
                nullifier: [0xBC; 32],
                timestamp: 99,
                fee: 0,
                nonce: 3,
                gas_limit: 1,
                gas_price: 1,
                proof_type: ProofType::Ownership,
                payload: TransactionPayload::ValidatorApproval {
                    proposal_hash: [0xAB; 32],
                },
                chain_id: 0,
                version: 3,
            },
            proposal: ValidatorJoinProposalData {
                public_key: key.clone(),
                metadata: "PRIVATE_METADATA_SENTINEL".to_string(),
                proposer: "PRIVATE_PROPOSER_SENTINEL".to_string(),
                timestamp: 98,
            },
            activated_validator: test_validator(key, true),
            recorded_at: 100,
        }
    }

    #[test]
    fn local_snapshot_is_disabled_unless_explicitly_enabled() {
        assert!(!parse_enabled(None).unwrap());
        assert!(!parse_enabled(Some("false")).unwrap());
        assert!(parse_enabled(Some("true")).unwrap());
        assert!(parse_enabled(Some("1")).unwrap());
        assert!(parse_enabled(Some("maybe")).is_err());
    }

    #[test]
    fn enabled_snapshot_requires_loopback_listener() {
        assert!(validate_listener_bind(true, "127.0.0.1:8081").is_ok());
        assert!(validate_listener_bind(true, "[::1]:8081").is_ok());
        assert!(validate_listener_bind(true, "0.0.0.0:8081").is_err());
        assert!(validate_listener_bind(true, "192.0.2.10:8081").is_err());
        assert!(validate_listener_bind(false, "0.0.0.0:8081").is_ok());
    }

    #[actix_web::test]
    async fn snapshot_route_is_not_registered_by_default() {
        let app = actix_test::init_service(
            App::new().configure(|config: &mut ServiceConfig| configure(config, false)),
        )
        .await;
        let response =
            actix_test::call_service(&app, actix_test::TestRequest::get().uri(ROUTE).to_request())
                .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn enabled_snapshot_route_remains_bearer_protected() {
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(super::super::AdminAuthConfig {
                    token: vec![b'a'; 32],
                }))
                .configure(|config: &mut ServiceConfig| configure(config, true)),
        )
        .await;
        let response = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri(ROUTE)
                .peer_addr("127.0.0.1:50000".parse().unwrap())
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn loopback_guard_rejects_remote_and_missing_peers() {
        let app = actix_test::init_service(
            App::new().service(
                web::resource("/guarded")
                    .wrap(from_fn(require_loopback_peer))
                    .route(web::get().to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        let remote = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/guarded")
                .peer_addr("192.0.2.4:50000".parse().unwrap())
                .to_request(),
        )
        .await;
        assert_eq!(remote.status(), StatusCode::FORBIDDEN);
        let missing = actix_test::call_service(
            &app,
            actix_test::TestRequest::get().uri("/guarded").to_request(),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn snapshot_auth_requires_the_administrator_bearer_token() {
        let token = "a".repeat(32);
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(super::super::AdminAuthConfig {
                    token: token.as_bytes().to_vec(),
                }))
                .service(
                    web::resource("/guarded")
                        .wrap(from_fn(require_loopback_peer))
                        .wrap(from_fn(require_admin_bearer_token))
                        .route(web::get().to(|| async { HttpResponse::Ok().finish() })),
                ),
        )
        .await;
        let missing = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/guarded")
                .peer_addr("127.0.0.1:50000".parse().unwrap())
                .to_request(),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let authorized = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/guarded")
                .peer_addr("127.0.0.1:50000".parse().unwrap())
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[test]
    fn snapshot_schema_excludes_keys_hashes_signatures_and_payloads() {
        let key = vec![0x42; 48];
        let persisted = vec![test_validator(key.clone(), true)];
        let proposals = vec![ValidatorJoinProposalData {
            public_key: vec![0x51; 2592],
            metadata: "PENDING_METADATA_SENTINEL".to_string(),
            proposer: "PENDING_PROPOSER_SENTINEL".to_string(),
            timestamp: 80,
        }];
        let approvals = vec![test_approval(key.clone())];
        let blocks = vec![UltraBlock {
            index: 1,
            timestamp: 101,
            previous_hash: [0; 32],
            hash: [1; 32],
            nonce: 0,
            transactions: vec![],
            merkle_root: [2; 32],
            state_root: [3; 32],
            shard_roots: vec![],
            aggregated_signature: None,
            validator_set: vec![key],
            epoch: 0,
            gas_used: 0,
            gas_limit: 1_000,
            block_reward: 0,
            size: 0,
            version: 1,
            parent_hash: [4; 32],
            difficulty: 1,
            total_difficulty: 1,
        }];
        let snapshot = build_snapshot(
            200, &blocks, &persisted, &persisted, &proposals, &approvals, true,
        );
        let serialized = serde_json::to_string(&snapshot).unwrap();

        for forbidden in [
            "PRIVATE_SENDER_SENTINEL",
            "PRIVATE_RECIPIENT_SENTINEL",
            "PRIVATE_METADATA_SENTINEL",
            "PRIVATE_PROPOSER_SENTINEL",
            "PENDING_METADATA_SENTINEL",
            "PENDING_PROPOSER_SENTINEL",
            &"42".repeat(48),
            &"ab".repeat(32),
            "\"approval_transaction\":",
            "proposal_hash",
            "public_key\"",
            "\"signature\":",
            "nullifier",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
        assert!(serialized.contains("\"public_key_bytes\":48"));
        assert!(serialized.contains("\"public_key_bytes\":2592"));
        assert!(serialized.contains("\"cross_tree_snapshot_atomic\":false"));
        assert!(serialized.contains("\"has_aggregated_signature\":false"));
    }
}
