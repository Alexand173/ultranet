// ============================================================
// APPCHAIN AUTHORIZATION AND LIFECYCLE — over HTTP, across several owners
// ============================================================
//
// `appchain_api_test` walks one owner through register → anchor → hand over. This file exists for
// the claims that walk cannot make:
//
//   * an identity that owns one chain cannot anchor another owner's chain — including the shape
//     production actually has, where a service identity owns its own chain and the chains beside it
//     belong to somebody else;
//   * a signature is bound to the bytes it signed, so a modified payload or a substituted sender is
//     refused rather than admitted;
//   * the lifecycle answers are what the public contract says they are: idempotent re-submission,
//     sequence-addressed anchor pages, and "no anchors" as an empty list rather than a 404.
//
// Nothing here needs a private key that the test does not generate, and nothing here reaches the
// network: the node is exercised in-process through the same handlers the public API mounts.

use actix_web::{test as actix_test, web, App};
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::Value;
use sha3::{Digest, Sha3_256};
use std::fs;
use std::sync::Arc;
use UltraNet::api::{
    appchain_ledger_anchors, appchain_ledger_chain, appchain_request_to_transaction, submit_appchain_tx,
    AppChainOperation, AppChainTxRequest, AppState,
};
use UltraNet::appchain::ledger::{
    APPCHAIN_ANCHOR_FEE, APPCHAIN_REGISTER_FEE, APPCHAIN_TRANSFER_FEE, APPCHAIN_TX_VERSION,
};
use UltraNet::storage::Storage;
use UltraNet::*;

const START_BALANCE: u64 = 1_000_000;

/// Activation is read once per process, so every test in this binary sets it before touching a chain.
fn activate_from_genesis() {
    std::env::set_var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT", "0");
}

fn fresh_chain(path: &'static str, funded: &[&UltraWallet]) -> Arc<RwLock<UltraBlockchain>> {
    activate_from_genesis();
    let _ = fs::remove_dir_all(path);
    let storage = Arc::new(Storage::new(path).expect("test storage"));
    for wallet in funded {
        storage
            .save_state(&wallet.address, START_BALANCE)
            .expect("seed balance");
    }
    Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)))
}

/// The request a wallet client sends: the node rebuilds the transaction from these fields and checks
/// the signature over its own preimage, so a test signs exactly what a wallet signs.
fn signed_request(
    wallet: &UltraWallet,
    nonce: u64,
    fee: u64,
    operation: AppChainOperation,
) -> AppChainTxRequest {
    let mut nullifier = vec![0u8; 32];
    let mut hasher = Sha3_256::new();
    hasher.update(wallet.address.as_bytes());
    hasher.update(nonce.to_le_bytes());
    nullifier.copy_from_slice(&hasher.finalize());
    signed_request_with(wallet, nonce, fee, operation, nullifier, APPCHAIN_TX_VERSION)
}

/// Same, with the nullifier and the envelope version supplied, so replay and version rules can be
/// exercised without inventing a second signer.
fn signed_request_with(
    wallet: &UltraWallet,
    nonce: u64,
    fee: u64,
    operation: AppChainOperation,
    nullifier: Vec<u8>,
    version: u32,
) -> AppChainTxRequest {
    let mut request = AppChainTxRequest {
        sender: wallet.address.clone(),
        sender_public_key: wallet.keypair.public_key.clone(),
        nonce,
        timestamp: Utc::now().timestamp() as u64,
        nullifier,
        fee,
        signature: vec![],
        version,
        operation,
    };
    let tx = appchain_request_to_transaction(&request).expect("well-formed request");
    let message = UltraBlockchain::create_transaction_message_for(&tx);
    request.signature = wallet.keypair.sign(&message);
    request
}

/// The request type deliberately does not implement `Clone` — a signed transaction is not something
/// to copy casually — so a test that needs to re-deliver the same bytes round-trips it through JSON,
/// which is exactly what a client retrying an HTTP POST would send.
fn clone_request(request: &AppChainTxRequest) -> AppChainTxRequest {
    serde_json::from_value(serde_json::to_value(request).expect("serialisable request"))
        .expect("round-trippable request")
}

fn register(name: &str) -> AppChainOperation {
    AppChainOperation::Register { name: name.into() }
}

fn anchor(chain_id: u32, root: String) -> AppChainOperation {
    AppChainOperation::Anchor { chain_id, state_root: root }
}

/// Post a signed request, returning the status and the JSON body.
macro_rules! post_signed {
    ($app:expr, $body:expr) => {{
        let response = actix_test::call_service(
            &$app,
            actix_test::TestRequest::post().uri("/api/appchain/tx").set_json($body).to_request(),
        )
        .await;
        let status = response.status().as_u16();
        let body: Value = actix_test::read_body_json(response).await;
        (status, body)
    }};
}

macro_rules! get_json {
    ($app:expr, $uri:expr) => {
        actix_test::call_and_read_body_json(&$app, actix_test::TestRequest::get().uri($uri).to_request())
            .await
    };
}

/// A chain id that owns nothing and is owned by nobody, mirroring production: one identity owns its
/// own chain, and the chains beside it belong to somebody else. It must not be able to anchor them.
#[actix_web::test]
async fn an_identity_that_owns_one_chain_cannot_anchor_another() {
    let alice = UltraWallet::new();
    let carol = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_isolation", &[&alice, &carol]);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx))
            .route("/api/appchain/ledger/{chain_id}", web::get().to(appchain_ledger_chain))
            .route(
                "/api/appchain/ledger/{chain_id}/anchors",
                web::get().to(appchain_ledger_anchors),
            ),
    )
    .await;

    // Alice owns #1 and #2; Carol owns #3. This is the production shape: the service identity owns
    // its own chain while the chains next to it belong to a different wallet.
    assert_eq!(post_signed!(app, signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Grid"))).0, 200);
    assert_eq!(post_signed!(app, signed_request(&alice, 1, APPCHAIN_REGISTER_FEE, register("Chess"))).0, 200);
    assert_eq!(
        post_signed!(app, signed_request(&carol, 0, APPCHAIN_REGISTER_FEE, register("Registry"))).0,
        200
    );
    blockchain.write().mine_block().expect("block with three registrations");

    let one: Value = get_json!(app, "/api/appchain/ledger/1");
    let two: Value = get_json!(app, "/api/appchain/ledger/2");
    let three: Value = get_json!(app, "/api/appchain/ledger/3");
    assert_eq!(one["chain"]["owner"], alice.address.as_str());
    assert_eq!(two["chain"]["owner"], alice.address.as_str());
    assert_eq!(three["chain"]["owner"], carol.address.as_str());

    // Carol owns #3 and therefore signs for #3.
    let own = signed_request(&carol, 1, APPCHAIN_ANCHOR_FEE, anchor(3, "11".repeat(32)));
    let (status, body) = post_signed!(app, own);
    assert_eq!(status, 200, "{body}");
    blockchain.write().mine_block().expect("block with Carol's anchor");

    // Owning #3 confers nothing over #1 or #2: the same identity, a valid signature, the wrong owner.
    for chain_id in [1u32, 2u32] {
        let foreign = signed_request(&carol, 1 + u64::from(chain_id), APPCHAIN_ANCHOR_FEE, anchor(chain_id, "22".repeat(32)));
        let (status, body) = post_signed!(app, foreign);
        assert_eq!(status, 400, "chain #{chain_id} must refuse a foreign owner: {body}");
        let message = body["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("Only the owner"),
            "chain #{chain_id} must name the ownership rule: {message}",
        );
    }

    // Nothing was recorded against the chains Carol does not own.
    let one: Value = get_json!(app, "/api/appchain/ledger/1");
    let two: Value = get_json!(app, "/api/appchain/ledger/2");
    let three: Value = get_json!(app, "/api/appchain/ledger/3");
    assert_eq!(one["chain"]["anchor_count"], 0);
    assert_eq!(two["chain"]["anchor_count"], 0);
    assert_eq!(three["chain"]["anchor_count"], 1);

    let _ = fs::remove_dir_all("test_db_appchain_auth_isolation");
}

/// A signature covers the bytes it signed. Change the root after signing and the node refuses it,
/// rather than anchoring a commitment the owner never saw.
#[actix_web::test]
async fn a_payload_modified_after_signing_is_rejected() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_modified", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    assert_eq!(post_signed!(app, signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Modified"))).0, 200);
    blockchain.write().mine_block().expect("block with the registration");

    let mut tampered = signed_request(&alice, 1, APPCHAIN_ANCHOR_FEE, anchor(1, "aa".repeat(32)));
    tampered.operation = anchor(1, "bb".repeat(32));

    let (status, body) = post_signed!(app, tampered);
    assert_eq!(status, 400, "{body}");
    let message = body["message"].as_str().unwrap_or_default().to_lowercase();
    assert!(message.contains("signature"), "the signature must be what fails: {message}");

    let _ = fs::remove_dir_all("test_db_appchain_auth_modified");
}

/// The sender field is not a label: it must be the address the public key derives to, or an attacker
/// could sign with their own key and claim somebody else's chain.
#[actix_web::test]
async fn a_substituted_sender_is_rejected() {
    let alice = UltraWallet::new();
    let mallory = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_spoof", &[&alice, &mallory]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    let mut spoofed = signed_request(&mallory, 0, APPCHAIN_REGISTER_FEE, register("Spoofed"));
    spoofed.sender = alice.address.clone();

    let (status, body) = post_signed!(app, spoofed);
    assert_eq!(status, 400, "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("identity spoofing"),
        "the sender/public-key binding must be what fails: {message}",
    );

    let _ = fs::remove_dir_all("test_db_appchain_auth_spoof");
}

/// Ownership moves only by the owner's signature, and only to a different address.
#[actix_web::test]
async fn a_foreign_owner_cannot_transfer_and_an_owner_cannot_transfer_to_themselves() {
    let alice = UltraWallet::new();
    let bob = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_transfer", &[&alice, &bob]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    assert_eq!(post_signed!(app, signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Held"))).0, 200);
    blockchain.write().mine_block().expect("block with the registration");

    // Not to himself: transferring a chain to your own address is refused by a stateless envelope
    // rule, which would answer before the ownership rule this case exists to prove.
    let foreign = signed_request(
        &bob,
        0,
        APPCHAIN_TRANSFER_FEE,
        AppChainOperation::TransferOwner { chain_id: 1, new_owner: alice.address.clone() },
    );
    let (status, body) = post_signed!(app, foreign);
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"].as_str().unwrap_or_default().contains("Only the owner"),
        "{body}",
    );

    let to_self = signed_request(
        &alice,
        1,
        APPCHAIN_TRANSFER_FEE,
        AppChainOperation::TransferOwner { chain_id: 1, new_owner: alice.address.clone() },
    );
    let (status, body) = post_signed!(app, to_self);
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"].as_str().unwrap_or_default().contains("itself"),
        "{body}",
    );

    let _ = fs::remove_dir_all("test_db_appchain_auth_transfer");
}

/// A nullifier is a global claim on one transaction. Reusing it for different content is refused; a
/// re-delivery of the identical transaction is not an error, because a client that retried after a
/// timeout must not see two different answers.
#[actix_web::test]
async fn a_nullifier_cannot_be_reused_but_an_identical_resubmission_is_accepted() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_nullifier", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    let registration = signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Replay"));
    let (status, first) = post_signed!(app, clone_request(&registration));
    assert_eq!(status, 200, "{first}");
    blockchain.write().mine_block().expect("block with the registration");

    // Same transaction again, after it is confirmed: still accepted, with the same hash.
    let (status, again) = post_signed!(app, clone_request(&registration));
    assert_eq!(status, 200, "an identical resubmission is not an error: {again}");
    assert_eq!(again["data"]["hash"], first["data"]["hash"]);

    // Same nullifier, different content: refused, because one nullifier may mean only one thing.
    let reused = signed_request_with(
        &alice,
        1,
        APPCHAIN_ANCHOR_FEE,
        anchor(1, "cc".repeat(32)),
        registration.nullifier.clone(),
        APPCHAIN_TX_VERSION,
    );
    let (status, body) = post_signed!(app, reused);
    assert_eq!(status, 400, "{body}");
    let message = body["message"].as_str().unwrap_or_default().to_lowercase();
    assert!(message.contains("nullifier"), "{message}");

    let _ = fs::remove_dir_all("test_db_appchain_auth_nullifier");
}

/// A registration costs the published fee. A wallet that cannot pay is refused before anything is
/// recorded.
#[actix_web::test]
async fn a_registration_without_balance_is_rejected() {
    let broke = UltraWallet::new();
    // Deliberately unfunded: this wallet exists only to prove the fee is enforced.
    let blockchain = fresh_chain("test_db_appchain_auth_fee", &[]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    let (status, body) = post_signed!(
        app,
        signed_request(&broke, 0, APPCHAIN_REGISTER_FEE, register("Unfunded"))
    );
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"].as_str().unwrap_or_default().contains("Insufficient balance"),
        "{body}",
    );
    assert_eq!(blockchain.read().storage.ledger_chains().expect("ledger").len(), 0);

    let _ = fs::remove_dir_all("test_db_appchain_auth_fee");
}

/// Only version 5 carries an AppChain payload. An older envelope is refused rather than reinterpreted.
#[actix_web::test]
async fn an_envelope_that_is_not_version_five_is_rejected() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_version", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    let mut legacy_version = signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Old Envelope"));
    legacy_version.version = APPCHAIN_TX_VERSION - 1;
    // Re-sign over the envelope as it now stands, so the version is the only thing wrong with it.
    let tx = appchain_request_to_transaction(&legacy_version).expect("well-formed request");
    let message = UltraBlockchain::create_transaction_message_for(&tx);
    legacy_version.signature = alice.keypair.sign(&message);

    let (status, body) = post_signed!(app, legacy_version);
    assert_eq!(status, 400, "{body}");
    let message = body["message"].as_str().unwrap_or_default().to_lowercase();
    assert!(message.contains("version"), "{message}");

    let _ = fs::remove_dir_all("test_db_appchain_auth_version");
}

/// Strict parsing: a field the node does not know is a rejection, not something to ignore. D-4 cannot
/// publish an envelope that silently tolerates extensions.
#[actix_web::test]
async fn an_unknown_field_is_rejected() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_unknown", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    let mut body = serde_json::to_value(signed_request(
        &alice,
        0,
        APPCHAIN_REGISTER_FEE,
        register("Extended"),
    ))
    .expect("serialisable request");
    body["extension"] = serde_json::json!(1);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&body).to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);

    let _ = fs::remove_dir_all("test_db_appchain_auth_unknown");
}

/// Strictness is claimed at two levels — the envelope and the operation — but the test above only
/// asserts the envelope. A client that adds an extension to one operation type will add it to the
/// others, so all three are checked.
///
/// The assertion is on the *reason*, not just the status: an extra field must be refused by the
/// deserialiser. Checking the status alone would pass for the wrong reason, because an anchor for an
/// unregistered chain also answers 400.
#[actix_web::test]
async fn an_unknown_field_inside_each_operation_is_rejected() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_unknown_nested", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx)),
    )
    .await;

    // Values are valid for each variant, so the only defect in each body is the extra field.
    let cases: [(&str, AppChainOperation, u64); 3] = [
        ("register", register("Extended"), APPCHAIN_REGISTER_FEE),
        ("anchor", anchor(1, "ab".repeat(32)), APPCHAIN_ANCHOR_FEE),
        (
            "transfer_owner",
            AppChainOperation::TransferOwner {
                chain_id: 1,
                new_owner: "cd".repeat(32),
            },
            APPCHAIN_TRANSFER_FEE,
        ),
    ];

    for (operation_name, operation, fee) in cases {
        let mut body = serde_json::to_value(signed_request(&alice, 0, fee, operation))
            .expect("serialisable request");
        body["operation"][operation_name]["extension"] = serde_json::json!(1);

        let response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&body).to_request(),
        )
        .await;

        let status = response.status().as_u16();
        let bytes = actix_test::read_body(response).await;
        let text = String::from_utf8_lossy(&bytes);

        assert_eq!(status, 400, "{operation_name}: nested unknown field was not refused");
        assert!(
            text.contains("unknown field"),
            "{operation_name}: 400 was not a deserialisation refusal: {text}"
        );
        assert!(
            text.contains("extension"),
            "{operation_name}: refusal does not name the offending field: {text}"
        );
    }

    let _ = fs::remove_dir_all("test_db_appchain_auth_unknown_nested");
}

/// The published read surface, as a client sees it: the chain route 404s for an unknown id, the
/// anchor route answers an empty list, and `from` addresses a sequence rather than an offset.
#[actix_web::test]
async fn the_read_surface_answers_unknown_chains_and_pages_by_sequence() {
    let alice = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_reads", &[&alice]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx))
            .route("/api/appchain/ledger/{chain_id}", web::get().to(appchain_ledger_chain))
            .route(
                "/api/appchain/ledger/{chain_id}/anchors",
                web::get().to(appchain_ledger_anchors),
            ),
    )
    .await;

    let missing = actix_test::call_service(
        &app,
        actix_test::TestRequest::get().uri("/api/appchain/ledger/9").to_request(),
    )
    .await;
    assert_eq!(missing.status(), 404);

    let no_anchors: Value = get_json!(app, "/api/appchain/ledger/9/anchors");
    assert_eq!(
        no_anchors["anchors"].as_array().map(Vec::len),
        Some(0),
        "an unknown chain has no anchors, which is different from not existing",
    );

    assert_eq!(post_signed!(app, signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("Paged"))).0, 200);
    // The registration must be in a block before an anchor for it can be admitted: admission checks
    // the persisted ledger, so an anchor sent in the same block as its registration is refused.
    blockchain.write().mine_block().expect("block with the registration");

    assert_eq!(post_signed!(app, signed_request(&alice, 1, APPCHAIN_ANCHOR_FEE, anchor(1, "01".repeat(32)))).0, 200);
    assert_eq!(post_signed!(app, signed_request(&alice, 2, APPCHAIN_ANCHOR_FEE, anchor(1, "02".repeat(32)))).0, 200);
    blockchain.write().mine_block().expect("block with the two anchors");

    let all: Value = get_json!(app, "/api/appchain/ledger/1/anchors");
    let sequences: Vec<u64> = all["anchors"]
        .as_array()
        .expect("anchors array")
        .iter()
        .filter_map(|entry| entry["sequence"].as_u64())
        .collect();
    assert_eq!(sequences, vec![1, 2]);

    let from_second: Value = get_json!(app, "/api/appchain/ledger/1/anchors?from=2");
    let sequences: Vec<u64> = from_second["anchors"]
        .as_array()
        .expect("anchors array")
        .iter()
        .filter_map(|entry| entry["sequence"].as_u64())
        .collect();
    assert_eq!(
        sequences,
        vec![2],
        "from=2 is the second sequence, not two rows skipped",
    );

    let _ = fs::remove_dir_all("test_db_appchain_auth_reads");
}

/// Names are labels, not identities (decided: option C, O-3). Two different owners may register the same
/// display name; each registration receives its own chain id, and the shared name confers authority on
/// neither side. This is the test the decision requires, so it is deliberately adversarial: every
/// cross-owner action below uses a shared name as its only connection.
#[actix_web::test]
async fn two_owners_may_register_the_same_display_name_and_neither_gains_authority() {
    let alice = UltraWallet::new();
    let bob = UltraWallet::new();
    // Only ever the destination of a transfer nobody is allowed to make: it owns nothing.
    let carol = UltraWallet::new();
    let blockchain = fresh_chain("test_db_appchain_auth_duplicate_names", &[&alice, &bob, &carol]);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx))
            .route("/api/appchain/ledger/{chain_id}", web::get().to(appchain_ledger_chain))
            .route(
                "/api/appchain/ledger/{chain_id}/anchors",
                web::get().to(appchain_ledger_anchors),
            ),
    )
    .await;

    // The same display name, twice, from two different wallets. Neither is refused, neither is merged.
    assert_eq!(
        post_signed!(app, signed_request(&alice, 0, APPCHAIN_REGISTER_FEE, register("UltraGrid"))).0,
        200
    );
    assert_eq!(
        post_signed!(app, signed_request(&bob, 0, APPCHAIN_REGISTER_FEE, register("UltraGrid"))).0,
        200
    );
    blockchain.write().mine_block().expect("block with both registrations");

    // Two chains, two ids, one name, two owners — read from the ledger the node committed.
    let chains = blockchain.read().storage.ledger_chains().expect("ledger chains");
    let ids: Vec<u32> = chains.iter().map(|chain| chain.id).collect();
    assert_eq!(ids, vec![1, 2], "each registration receives its own id");
    assert!(chains.iter().all(|chain| chain.name == "UltraGrid"), "both keep the registered name");
    assert_eq!(chains[0].owner, alice.address, "chain #1 belongs to its registrant");
    assert_eq!(chains[1].owner, bob.address, "chain #2 belongs to its registrant");

    // A name is not an address. The read route takes a numeric id, so a name cannot select a chain —
    // which is what makes duplicates harmless rather than ambiguous.
    let by_name = actix_test::call_service(
        &app,
        actix_test::TestRequest::get().uri("/api/appchain/ledger/UltraGrid").to_request(),
    )
    .await;
    assert_eq!(by_name.status(), 404, "a display name must never resolve a chain");

    // Each owner anchors their own chain, and the two anchors land on different chains.
    assert_eq!(
        post_signed!(app, signed_request(&alice, 1, APPCHAIN_ANCHOR_FEE, anchor(1, "a1".repeat(32)))).0,
        200
    );
    assert_eq!(
        post_signed!(app, signed_request(&bob, 1, APPCHAIN_ANCHOR_FEE, anchor(2, "b2".repeat(32)))).0,
        200
    );
    blockchain.write().mine_block().expect("block with both anchors");

    // The shared name authorises nothing. Same name, valid signature, wrong owner.
    for (wallet, chain_id) in [(&alice, 2u32), (&bob, 1u32)] {
        let foreign = signed_request(wallet, 2, APPCHAIN_ANCHOR_FEE, anchor(chain_id, "ff".repeat(32)));
        let (status, body) = post_signed!(app, foreign);
        assert_eq!(status, 400, "a shared name must not authorise chain #{chain_id}: {body}");
        assert!(
            body["message"].as_str().unwrap_or_default().contains("Only the owner"),
            "{body}",
        );
    }

    // Nor does it authorise handing the other chain to a third address. The destination is somebody
    // else on purpose: transferring to yourself is refused by a stateless rule that answers before
    // the ownership rule this case exists to prove.
    let foreign_transfer = signed_request(
        &alice,
        3,
        APPCHAIN_TRANSFER_FEE,
        AppChainOperation::TransferOwner { chain_id: 2, new_owner: carol.address.clone() },
    );
    let (status, body) = post_signed!(app, foreign_transfer);
    assert_eq!(status, 400, "{body}");
    assert!(
        body["message"].as_str().unwrap_or_default().contains("Only the owner"),
        "{body}",
    );

    // Exactly one anchor on each chain: the refused attempts recorded nothing, and no anchor strayed
    // onto the chain whose owner merely shares the name.
    let one: Value = get_json!(app, "/api/appchain/ledger/1");
    let two: Value = get_json!(app, "/api/appchain/ledger/2");
    assert_eq!(one["chain"]["anchor_count"], 1);
    assert_eq!(two["chain"]["anchor_count"], 1);
    // Each root landed on its own chain. Compared by suffix so the assertion does not depend on
    // whether the API renders the root with a 0x prefix.
    let one_root = one["chain"]["latest_state_root"].as_str().unwrap_or_default().to_lowercase();
    let two_root = two["chain"]["latest_state_root"].as_str().unwrap_or_default().to_lowercase();
    assert!(one_root.ends_with(&"a1".repeat(32)), "chain #1 kept its own root: {one_root}");
    assert!(two_root.ends_with(&"b2".repeat(32)), "chain #2 kept its own root: {two_root}");

    let _ = fs::remove_dir_all("test_db_appchain_auth_duplicate_names");
}

