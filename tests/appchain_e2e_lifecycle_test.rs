// ============================================================
// E2E — the generic AppChain lifecycle, driven as an outside developer
// ============================================================
//
// This is the check a third-party backend or a mobile wallet actually has to pass. The owner key is
// generated **here**, in the client's own process, exactly as `AppChainSigner.kt` generates one on a
// phone: the node never sees it. What crosses the wire is a public key and a signature over the
// version-5 preimage, and nothing else.
//
// Nothing in this flow is Registry- or Chess-specific: the chain is registered with an arbitrary
// name, the node assigns its L1 chain id, and every subsequent authorisation is
// `sender == chain.owner`. The two negative controls (a foreign wallet anchoring, and the previous
// owner anchoring after a handover) are the property that makes the rest mean something.
//
// Deliberately NOT here: any UltraNet-held key, any admin token, any server-side anchor route.
// The environment is a throwaway in-process node; no production or L1 transaction is involved.

use actix_web::{test as actix_test, web, App};
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::Value;
use sha3::{Digest, Sha3_256};
use std::fs;
use std::sync::Arc;
use UltraNet::api::{
    appchain_ledger, appchain_ledger_anchors, appchain_ledger_chain, appchain_overview,
    appchain_request_to_transaction, list_appchains, submit_appchain_tx, AppChainOperation,
    AppChainTxRequest, AppState,
};
use UltraNet::appchain::ledger::{
    APPCHAIN_ANCHOR_FEE, APPCHAIN_REGISTER_FEE, APPCHAIN_TRANSFER_FEE, APPCHAIN_TX_VERSION,
};
use UltraNet::storage::Storage;
use UltraNet::*;

const DB_PATH: &str = "test_db_appchain_e2e";
const START_BALANCE: u64 = 1_000_000;

/// What a client sends. The nullifier is derived from (sender, nonce) only so a run is repeatable;
/// a real client randomises it.
fn client_request(
    wallet: &UltraWallet,
    nonce: u64,
    fee: u64,
    operation: AppChainOperation,
) -> AppChainTxRequest {
    let mut nullifier = [0u8; 32];
    let mut hasher = Sha3_256::new();
    hasher.update(wallet.address.as_bytes());
    hasher.update(nonce.to_le_bytes());
    nullifier.copy_from_slice(&hasher.finalize());

    let mut request = AppChainTxRequest {
        sender: wallet.address.clone(),
        sender_public_key: wallet.keypair.public_key.clone(),
        nonce,
        timestamp: Utc::now().timestamp() as u64,
        nullifier: nullifier.to_vec(),
        fee,
        signature: vec![],
        version: APPCHAIN_TX_VERSION,
        operation,
    };
    let tx = appchain_request_to_transaction(&request).expect("well-formed request");
    let message = UltraBlockchain::create_transaction_message_for(&tx);
    request.signature = wallet.keypair.sign(&message);
    request
}

#[actix_web::test]
async fn a_third_party_developer_can_register_discover_anchor_and_hand_over_a_chain() {
    std::env::set_var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT", "0");
    let _ = fs::remove_dir_all(DB_PATH);

    // The developer's key, generated client-side. `successor` stands in for a second, unrelated party.
    let developer = UltraWallet::new();
    let successor = UltraWallet::new();
    let stranger = UltraWallet::new();

    let storage = Arc::new(Storage::new(DB_PATH).expect("test storage"));
    for wallet in [&developer, &successor] {
        storage.save_state(&wallet.address, START_BALANCE).expect("seed balance");
    }
    let blockchain = Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)));

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx))
            .route("/api/appchain/ledger", web::get().to(appchain_ledger))
            .route("/api/appchain/ledger/{chain_id}", web::get().to(appchain_ledger_chain))
            .route("/api/appchain/ledger/{chain_id}/anchors", web::get().to(appchain_ledger_anchors))
            .route("/api/appchain/list", web::get().to(list_appchains))
            .route("/api/appchain/overview", web::get().to(appchain_overview)),
    )
    .await;

    let get = |uri: &'static str| actix_test::TestRequest::get().uri(uri).to_request();
    let post = |body: &AppChainTxRequest| {
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(body).to_request()
    };

    // ---- 1. The client reads the published terms, so it can show the fee before signing ----------
    let terms: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger")).await;
    assert_eq!(terms["terms"]["register_fee_base_units"], APPCHAIN_REGISTER_FEE);
    assert_eq!(terms["terms"]["version"], APPCHAIN_TX_VERSION);
    assert_eq!(terms["chains"].as_array().unwrap().len(), 0, "a fresh network has no chains");

    // ---- 2. Register: the developer signs, and the node assigns the identity --------------------
    let register = client_request(
        &developer,
        0,
        APPCHAIN_REGISTER_FEE,
        AppChainOperation::Register { name: "Third Party App".into() },
    );

    // What the node is given, and all it is given: no private key, and no key material beyond the
    // public key. This is the property that lets a phone hold custody.
    let wire = serde_json::to_value(&register).expect("request serialises");
    let mut fields: Vec<&str> = wire.as_object().unwrap().keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        vec![
            "fee",
            "nonce",
            "nullifier",
            "operation",
            "sender",
            "sender_public_key",
            "signature",
            "timestamp",
            "version"
        ],
        "the wire format carries a signature, never a key"
    );

    let accepted: Value = actix_test::call_and_read_body_json(&app, post(&register)).await;
    assert_eq!(accepted["success"], true, "{accepted}");
    assert_eq!(accepted["data"]["status"], "pending", "accepted is not yet final");
    blockchain.write().mine_block().expect("a block containing the registration");

    // ---- 3. Discover it by Chain ID, by name only as a label -----------------------------------
    let chain: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1")).await;
    assert_eq!(chain["chain"]["id"], 1);
    assert_eq!(chain["chain"]["name"], "Third Party App", "names are the developer's label");
    assert_eq!(chain["chain"]["owner"], developer.address.as_str(), "the signer is the owner");
    assert_eq!(chain["chain"]["anchor_count"], 0);

    let overview: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/overview")).await;
    assert_eq!(overview["chains"][0]["source"], "ledger");
    assert_eq!(overview["proof_scheme"], UltraNet::api::APPCHAIN_PROOF_SCHEME_LEDGER);

    // ---- 4. Authority: a wallet that does not own chain #1 cannot anchor it ---------------------
    let root_one = "aa".repeat(32);
    let outsider = client_request(
        &stranger,
        0,
        APPCHAIN_ANCHOR_FEE,
        AppChainOperation::Anchor { chain_id: 1, state_root: root_one.clone() },
    );
    let refused = actix_test::call_service(&app, post(&outsider)).await;
    assert_eq!(refused.status(), 400, "a non-owner's anchor is a rejection, not a queue entry");
    let body: Value = actix_test::read_body_json(refused).await;
    assert!(
        body["message"].as_str().unwrap_or_default().contains("Only the owner"),
        "the refusal names the rule: {body}"
    );
    // Nothing was recorded for the refused attempt.
    let anchors: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1/anchors")).await;
    assert_eq!(anchors["anchors"].as_array().unwrap().len(), 0, "{anchors}");

    // ---- 5. The owner prepares and signs an anchor; the node verifies and includes it -----------
    let anchor = client_request(
        &developer,
        1,
        APPCHAIN_ANCHOR_FEE,
        AppChainOperation::Anchor { chain_id: 1, state_root: root_one.clone() },
    );
    let accepted: Value = actix_test::call_and_read_body_json(&app, post(&anchor)).await;
    assert_eq!(accepted["success"], true, "{accepted}");
    blockchain.write().mine_block().expect("a block containing the anchor");

    // ---- 6. The resulting state is observable through the public read surface -------------------
    let anchors: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1/anchors")).await;
    let recorded = anchors["anchors"].as_array().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["sequence"], 1, "sequence comes from consensus, not the client");
    assert_eq!(recorded[0]["state_root"], root_one, "the owner's root is what L1 recorded");

    let chain: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1")).await;
    assert_eq!(chain["chain"]["anchor_count"], 1);

    // ---- 7. Handover: only the owner can transfer, and the old owner loses the power -----------
    let transfer = client_request(
        &developer,
        2,
        APPCHAIN_TRANSFER_FEE,
        AppChainOperation::TransferOwner {
            chain_id: 1,
            new_owner: successor.address.clone(),
        },
    );
    let accepted: Value = actix_test::call_and_read_body_json(&app, post(&transfer)).await;
    assert_eq!(accepted["success"], true, "{accepted}");
    blockchain.write().mine_block().expect("a block containing the handover");

    let chain: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1")).await;
    assert_eq!(chain["chain"]["owner"], successor.address.as_str());
    assert_eq!(chain["chain"]["name"], "Third Party App", "a handover does not rename the chain");

    // The previous owner can no longer anchor it.
    let stale = client_request(
        &developer,
        3,
        APPCHAIN_ANCHOR_FEE,
        AppChainOperation::Anchor { chain_id: 1, state_root: "bb".repeat(32) },
    );
    let refused = actix_test::call_service(&app, post(&stale)).await;
    assert_eq!(refused.status(), 400, "the handover moved the authority with the chain");

    // The new owner can.
    let root_two = "cc".repeat(32);
    let fresh = client_request(
        &successor,
        0,
        APPCHAIN_ANCHOR_FEE,
        AppChainOperation::Anchor { chain_id: 1, state_root: root_two.clone() },
    );
    let accepted: Value = actix_test::call_and_read_body_json(&app, post(&fresh)).await;
    assert_eq!(accepted["success"], true, "{accepted}");
    blockchain.write().mine_block().expect("a block containing the successor's anchor");

    let anchors: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1/anchors")).await;
    let recorded = anchors["anchors"].as_array().unwrap();
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[1]["sequence"], 2);
    assert_eq!(recorded[1]["state_root"], root_two);

    // ---- 8. The developer's identity space is untouched by any of this -------------------------
    let list: Value = actix_test::call_and_read_body_json(&app, get("/api/appchain/list")).await;
    let chains = list["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 1, "one developer registered one chain");
    assert_eq!(chains[0]["id"], 1);

    let _ = fs::remove_dir_all(DB_PATH);
}
