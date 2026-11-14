// ============================================================
// APPCHAIN LEDGER OVER HTTP — the path a wallet client takes
// ============================================================
//
// A client (the Android app, the website wallet, or the game server) builds an
// AppChainTxRequest, signs the version-5 preimage of the transaction the node
// will rebuild from it, and posts JSON. This test does exactly that, then mines
// and reads the result back through the public ledger routes.

use actix_web::{test as actix_test, web, App};
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::Value;
use std::fs;
use std::sync::Arc;
use UltraNet::api::{
    appchain_ledger, appchain_ledger_anchors, appchain_ledger_chain,
    appchain_request_to_transaction, submit_appchain_tx, AppChainOperation, AppChainTxRequest,
    AppState,
};
use UltraNet::appchain::ledger::{
    APPCHAIN_ANCHOR_FEE, APPCHAIN_REGISTER_FEE, APPCHAIN_TX_VERSION,
};
use UltraNet::storage::Storage;
use UltraNet::*;

const DB_PATH: &str = "test_db_appchain_api";
const START_BALANCE: u64 = 1_000_000;

/// Build the request a client would send, signed over the node's own preimage.
fn signed_request(wallet: &UltraWallet, nonce: u64, fee: u64, operation: AppChainOperation) -> AppChainTxRequest {
    let mut nullifier = vec![0u8; 32];
    nullifier[..8].copy_from_slice(&nonce.to_le_bytes());
    nullifier[31] = 0x5A;
    let mut request = AppChainTxRequest {
        sender: wallet.address.clone(),
        sender_public_key: wallet.keypair.public_key.clone(),
        nonce,
        timestamp: Utc::now().timestamp() as u64,
        nullifier,
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
async fn register_then_anchor_through_the_public_api() {
    // This binary only exercises AppChain routes, so it can activate them from genesis.
    std::env::set_var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT", "0");
    let _ = fs::remove_dir_all(DB_PATH);

    let owner = UltraWallet::new();
    let storage = Arc::new(Storage::new(DB_PATH).expect("test storage"));
    storage.save_state(&owner.address, START_BALANCE).expect("seed owner balance");
    let blockchain = Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)));

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(AppState { blockchain: blockchain.clone() }))
            .route("/api/appchain/tx", web::post().to(submit_appchain_tx))
            .route("/api/appchain/ledger", web::get().to(appchain_ledger))
            .route("/api/appchain/ledger/{chain_id}", web::get().to(appchain_ledger_chain))
            .route("/api/appchain/ledger/{chain_id}/anchors", web::get().to(appchain_ledger_anchors)),
    )
    .await;

    let get = |uri: &'static str| actix_test::TestRequest::get().uri(uri).to_request();

    // The ledger publishes its terms, so a client can show the fee before signing.
    let ledger: Value = actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger")).await;
    assert_eq!(ledger["terms"]["register_fee_base_units"], APPCHAIN_REGISTER_FEE);
    assert_eq!(ledger["terms"]["anchor_fee_base_units"], APPCHAIN_ANCHOR_FEE);
    assert_eq!(ledger["terms"]["activation_height"], 0);
    assert_eq!(ledger["chains"].as_array().unwrap().len(), 0);

    // An unsigned request is refused.
    let mut unsigned = signed_request(
        &owner,
        0,
        APPCHAIN_REGISTER_FEE,
        AppChainOperation::Register { name: "UltraNet Grid".into() },
    );
    unsigned.signature = vec![0u8; unsigned.signature.len()];
    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&unsigned).to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);

    // A fee below the published minimum is refused.
    let cheap = signed_request(
        &owner,
        0,
        APPCHAIN_REGISTER_FEE - 1,
        AppChainOperation::Register { name: "UltraNet Grid".into() },
    );
    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&cheap).to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);

    // Create chain: the wallet signs a registration. No admin token, no owner field.
    let register = signed_request(
        &owner,
        0,
        APPCHAIN_REGISTER_FEE,
        AppChainOperation::Register { name: "UltraNet Grid".into() },
    );
    let accepted: Value = actix_test::call_and_read_body_json(
        &app,
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&register).to_request(),
    )
    .await;
    assert_eq!(accepted["success"], true, "{accepted}");
    assert_eq!(accepted["data"]["status"], "pending");
    blockchain.write().mine_block().expect("block with the registration");

    let ledger: Value = actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger")).await;
    let chains = ledger["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0]["id"], 1);
    assert_eq!(chains[0]["name"], "UltraNet Grid");
    assert_eq!(chains[0]["owner"], owner.address.as_str(), "the signer is the owner");

    // Anchor: the owner signs a state root for chain #1.
    let root = "ab".repeat(32);
    let anchor = signed_request(
        &owner,
        1,
        APPCHAIN_ANCHOR_FEE,
        AppChainOperation::Anchor { chain_id: 1, state_root: root.clone() },
    );
    let accepted: Value = actix_test::call_and_read_body_json(
        &app,
        actix_test::TestRequest::post().uri("/api/appchain/tx").set_json(&anchor).to_request(),
    )
    .await;
    assert_eq!(accepted["success"], true, "{accepted}");
    blockchain.write().mine_block().expect("block with the anchor");

    let anchors: Value =
        actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1/anchors")).await;
    let anchors = anchors["anchors"].as_array().unwrap();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0]["sequence"], 1);
    assert_eq!(anchors[0]["state_root"], root.as_str());

    let chain: Value = actix_test::call_and_read_body_json(&app, get("/api/appchain/ledger/1")).await;
    assert_eq!(chain["chain"]["anchor_count"], 1);
    let missing = actix_test::call_service(&app, get("/api/appchain/ledger/7")).await;
    assert_eq!(missing.status(), 404);

    // Fees came out of the owner's balance and nothing else moved.
    assert_eq!(
        blockchain.read().get_balance(&owner.address),
        START_BALANCE - APPCHAIN_REGISTER_FEE - APPCHAIN_ANCHOR_FEE
    );

    drop(app);
    drop(blockchain);
    let _ = fs::remove_dir_all(DB_PATH);
}
