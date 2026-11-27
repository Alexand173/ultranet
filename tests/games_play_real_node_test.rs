// ============================================================
// REHEARSAL — the whole universal Games flow, through the HTTP routes, against a real (throwaway) node
// ============================================================
//
// Real node code on a local port with a scratch database. A game owner registers a real AppChain and a game, the
// platform operator installs a real WebAssembly module, the owner funds the pool with a real L1 transfer and a
// policy is approved. A player signs in with a real wallet signature and plays through the real routes; the module
// runs twice and agrees; the reward is reserved from that game's pool; the payout is signed, its reply is lost, the
// service restarts, the identical bytes are resent, a block is mined, and only then does the API say "paid".
// No FakeNode, no FakeChain, no direct calls to the reservation function.
//
// Not production: every key is generated here and thrown away, there are no peers and no real value. What it
// proves is how the payout path behaves against real node code; it proves nothing about production,
// consensus with other validators or economics.

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::Value;
use sha3::{Digest, Sha3_256};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use UltraNet::api::{appchain_request_to_transaction, AppChainOperation, AppChainTxRequest};
use UltraNet::appchain::ledger::{APPCHAIN_REGISTER_FEE, APPCHAIN_TX_VERSION};
use UltraNet::faucet::models::{NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest};
use UltraNet::faucet::node_client::{NodeApi, NodeClient, NodeClientError};
use UltraNet::faucet::signer::FaucetSigner;
use UltraNet::games::adapter::{AdapterModule, RuntimeLimits};
use UltraNet::games::catalog::admin_digest;
use UltraNet::games::chain::HttpChain;
use UltraNet::games::pools::PolicyTerms;
use UltraNet::games::rules::Policy;
use UltraNet::games::service::{system_clock, AdminProof, GamesService, ServiceConfig};
use UltraNet::games::store::GamesStore;
use UltraNet::{api, storage::Storage, QuantumKeyPair, UltraBlockchain};

struct LossyNode {
    inner: NodeClient,
    lose_next_submit_reply: AtomicBool,
    forget_next_status: AtomicBool,
    submissions: AtomicUsize,
}

#[async_trait]
impl NodeApi for LossyNode {
    async fn account(&self, address: &str) -> Result<NodeAccountData, NodeClientError> {
        self.inner.account(address).await
    }
    async fn estimate(&self, recipient: &str, amount: u64) -> Result<NodeFeeEstimateData, NodeClientError> {
        self.inner.estimate(recipient, amount).await
    }
    async fn submit(&self, envelope: &SignedTransferRequest) -> Result<NodeTransactionData, NodeClientError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        let reply = self.inner.submit(envelope).await; // the request really reaches the node
        if self.lose_next_submit_reply.swap(false, Ordering::SeqCst) {
            return Err(NodeClientError::Unavailable); // and the answer is lost on the way back
        }
        reply
    }
    async fn transaction_status(&self, hash: &str) -> Result<NodeTransactionData, NodeClientError> {
        if self.forget_next_status.swap(false, Ordering::SeqCst) {
            return Err(NodeClientError::NotFound);
        }
        self.inner.transaction_status(hash).await
    }
}

async fn get_json(url: &str) -> Value {
    reqwest::get(url).await.expect("node answers").json().await.expect("json body")
}

fn address_of(keys: &QuantumKeyPair) -> String {
    QuantumKeyPair::address_from_public_key(&keys.public_key)
}

async fn balance(base: &str, address: &str) -> u64 {
    get_json(&format!("{base}/api/account/{address}")).await["data"]["balance_base_units"].as_u64().unwrap_or(0)
}

async fn nonce(base: &str, address: &str) -> u64 {
    get_json(&format!("{base}/api/account/{address}")).await["data"]["nonce"].as_u64().unwrap_or(0)
}

async fn mempool_count(base: &str) -> u64 {
    get_json(&format!("{base}/api/mempool")).await["count"].as_u64().unwrap()
}

fn client_request(keys: &QuantumKeyPair, nonce: u64, fee: u64, operation: AppChainOperation) -> AppChainTxRequest {
    let sender = address_of(keys);
    let mut hasher = Sha3_256::new();
    hasher.update(sender.as_bytes());
    hasher.update(nonce.to_le_bytes());
    let nullifier = hasher.finalize().to_vec();
    let mut request = AppChainTxRequest {
        sender,
        sender_public_key: keys.public_key.clone(),
        nonce,
        timestamp: Utc::now().timestamp() as u64,
        nullifier,
        fee,
        signature: vec![],
        version: APPCHAIN_TX_VERSION,
        operation,
    };
    let tx = appchain_request_to_transaction(&request).expect("well-formed request");
    request.signature = keys.sign(&UltraBlockchain::create_transaction_message_for(&tx));
    request
}

async fn submit_appchain(base: &str, keys: &QuantumKeyPair, fee: u64, operation: AppChainOperation) {
    let request = client_request(keys, nonce(base, &address_of(keys)).await, fee, operation);
    let reply: Value = reqwest::Client::new()
        .post(format!("{base}/api/appchain/tx"))
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["success"], true, "{reply}");
}

/// A real L1 transfer signed by `keys`, submitted to the node. Returns its hash.
async fn transfer(base: &str, node: &NodeClient, keys: &QuantumKeyPair, to: &str, amount: u64) -> String {
    let signer = FaucetSigner::from_keypair_for_tests(keys.clone()).unwrap();
    let fee = UltraBlockchain::minimum_transfer_fee(amount);
    let (envelope, _) = signer
        .sign_transfer(to, amount, fee, nonce(base, &address_of(keys)).await, Utc::now().timestamp() as u64)
        .unwrap();
    node.submit(&envelope).await.expect("the node accepts the transfer").hash
}

fn proof(service: &GamesService, keys: &QuantumKeyPair, action: &str, chain: u32, game_id: &str, name: &str) -> AdminProof {
    let mut fresh = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
    let nonce = hex::encode(fresh);
    let timestamp = service.now();
    let digest = admin_digest(action, chain, game_id, name, "", "", &nonce, timestamp);
    AdminProof { nonce, timestamp, public_key: hex::encode(&keys.public_key), signature: hex::encode(keys.sign(&digest)) }
}

fn terms() -> PolicyTerms {
    PolicyTerms { reward_units: 100, eligible_outcomes: "win".into(), per_player_per_day: 3, per_game_per_day: 100, effective_from: 0 }
}

fn build_service(path: &std::path::Path, base: &str, payout_keys: &QuantumKeyPair) -> (Arc<GamesService>, Arc<LossyNode>) {
    let node = Arc::new(LossyNode {
        inner: NodeClient::new(base.to_string()).unwrap(),
        lose_next_submit_reply: AtomicBool::new(false),
        forget_next_status: AtomicBool::new(false),
        submissions: AtomicUsize::new(0),
    });
    let signer = FaucetSigner::from_keypair_for_tests(payout_keys.clone()).unwrap();
    let mut policy = Policy::standard(3);
    policy.payout_reserve_units = 100; // the lowest the platform allows; the wallet needs this much beyond a payout
    let mut config = ServiceConfig::new(address_of(payout_keys));
    config.pool_payouts_enabled = true; // the rehearsal runs the gated path on purpose; the default is off
    config.require_player_auth = true; // as in production: a wallet session is required to play
    let service = GamesService::new(
        GamesStore::open(path).unwrap(),
        policy,
        config,
        Arc::new(HttpChain::new(base).unwrap()),
        node.clone(),
        Some(Arc::new(signer)),
        None,
        b"rehearsal-abuse-key".to_vec(),
        system_clock(),
    );
    (Arc::new(service), node)
}

/// Run the worker, mining after every step that submitted something, until nothing is left in flight.
async fn drain(service: &GamesService, blockchain: &Arc<RwLock<UltraBlockchain>>, base: &str, rounds: usize) {
    for _ in 0..rounds {
        let worked = service.process_pool_payout_once().await.expect("worker step");
        if mempool_count(base).await > 0 {
            blockchain.write().mine_block().expect("a block");
        }
        if !worked {
            break;
        }
    }
}


/// One request through the real route table, in process. Returns the status and the JSON body.
async fn http(service: &Arc<GamesService>, post: bool, uri: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
    use actix_web::test::{call_service, init_service, read_body_json, TestRequest};
    let app = init_service(
        actix_web::App::new().app_data(actix_web::web::Data::from(service.clone())).configure(UltraNet::games::api::configure_routes),
    )
    .await;
    let mut request = if post { TestRequest::post() } else { TestRequest::get() }.uri(uri);
    if let Some(token) = token {
        request = request.insert_header(("X-Wallet-Session", token.to_string()));
    }
    if let Some(body) = body {
        request = request.set_json(body);
    }
    let response = call_service(&app, request.to_request()).await;
    let status = response.status().as_u16();
    (status, read_body_json(response).await)
}

/// A real sign-in: the wallet recomputes the digest, signs it with its Dilithium key, and gets a session.
fn sign_in(service: &GamesService, keys: &QuantumKeyPair) -> String {
    let address = address_of(keys);
    let challenge = service.auth_challenge(&address, "play").unwrap();
    let digest = UltraNet::games::auth::challenge_digest("play", &address, &challenge.challenge_id, challenge.issued_at, challenge.expires_at).unwrap();
    assert_eq!(hex::encode(digest), challenge.digest, "the wallet's own digest is the one the service offered");
    service.auth_verify(&challenge.challenge_id, &hex::encode(&keys.public_key), &hex::encode(keys.sign(&digest))).unwrap().token
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_player_plays_a_real_module_through_the_routes_and_is_paid_only_after_the_node_confirms() {
    std::env::set_var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT", "0");
    let scratch = std::env::temp_dir().join(format!("games-play-real-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let node_db = scratch.join("node");
    let games_db = scratch.join("games.db");
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let base = format!("http://127.0.0.1:{port}");
    std::env::set_var("ULTRANET_API_BIND", format!("127.0.0.1:{port}"));
    std::env::set_var("ULTRANET_CORS_ORIGINS", &base);
    std::env::set_var("ULTRANET_ADMIN_TOKEN", "rehearsal-token-never-used-outside-this-process");
    std::env::set_var("ULTRANET_DB_PATH", &node_db);

    let payout_keys = QuantumKeyPair::generate();
    let payout_address = address_of(&payout_keys);
    let owner = QuantumKeyPair::generate();
    let owner_address = address_of(&owner);
    let player = QuantumKeyPair::generate();
    let player_address = address_of(&player);
    let stranger = QuantumKeyPair::generate();
    let storage = Arc::new(Storage::new(node_db.to_str().unwrap()).unwrap());
    storage.save_state(&owner_address, 5_000_000).unwrap();
    let blockchain = Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)));
    let served = blockchain.clone();
    std::thread::spawn(move || {
        let _ = actix_web::rt::System::new().block_on(api::run_server(served));
    });
    for _ in 0..100 {
        if reqwest::get(format!("{base}/api/stats")).await.map(|r| r.status().is_success()).unwrap_or(false) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let node = NodeClient::new(base.clone()).unwrap();
    let mine = || blockchain.write().mine_block().expect("a block");

    // ---- 1. a real AppChain, a game on it, and the operator's module ------------------------------------
    submit_appchain(&base, &owner, APPCHAIN_REGISTER_FEE, AppChainOperation::Register { name: "Arcade".into() }).await;
    mine();
    let ledger = get_json(&format!("{base}/api/appchain/ledger")).await;
    let chain_id = ledger["chains"].as_array().unwrap().iter().find(|c| c["owner"] == owner_address.as_str()).unwrap()["id"].as_u64().unwrap() as u32;
    let (service, lossy) = build_service(&games_db, &base, &payout_keys);
    let p = proof(&service, &owner, "register", chain_id, "", "Echo Game");
    let game = service.catalog_register(chain_id, "Echo Game", "", p).await.unwrap();
    let wat = r#"(module (memory (export "memory") 1 2)
      (func (export "adapter_abi_version") (result i32) i32.const 1)
      (func (export "input_buffer") (result i32) i32.const 1024)
      (func (export "input_capacity") (result i32) i32.const 4200)
      (func (export "output_buffer") (result i32) i32.const 8192)
      (func (export "adapter_run") (param i32) (result i32)
        (i32.store8 (i32.const 8192) (i32.const 1))
        (i32.store8 (i32.const 8193) (i32.load8_u (i32.const 1061)))
        (i32.store (i32.const 8194) (i32.const 7))
        i32.const 38))"#;
    let bytes = wat::parse_str(wat).unwrap();
    // The same module, run twice outside the service, agrees with itself: the determinism the service relies on.
    let module = AdapterModule::compile(&bytes, RuntimeLimits::default()).unwrap();
    assert_eq!(module.validate_result(&[1; 32], &[1]).unwrap(), module.validate_result(&[1; 32], &[1]).unwrap());
    let before_install = http(&service, false, "/api/games/store", None, None).await.1;
    assert_eq!(before_install["games"][0]["playable"], false, "no module yet");
    service.platform_register_adapter(&game.game_id, &bytes, 1, "ops").unwrap();

    // ---- 2. the owner funds the pool with a real transfer; the platform approves a policy ---------------
    let deposit = transfer(&base, &node, &owner, &payout_address, 10_000).await;
    mine();
    let claim = proof(&service, &owner, "pool-fund", chain_id, &game.game_id, &deposit);
    assert_eq!(service.pool_fund(&game.game_id, &deposit, claim).await.unwrap().funded, 10_000);
    let t = terms();
    let summary = format!("{}|{}|{}|{}|{}", t.reward_units, t.eligible_outcomes, t.per_player_per_day, t.per_game_per_day, t.effective_from);
    let p = proof(&service, &owner, "policy-propose", chain_id, &game.game_id, &summary);
    let row = service.policy_propose(&game.game_id, t, p).await.unwrap();
    service.platform_decide_policy(&game.game_id, row.version, true, "ops", "within the ceilings").unwrap();

    // ---- 3. the store shows the game as playable, owned by the ledger's owner, with a reward on offer ----
    let (status, listing) = http(&service, false, "/api/games/store", None, None).await;
    assert_eq!(status, 200);
    let entry = &listing["games"][0];
    assert_eq!((entry["game_id"].as_str(), entry["chain_id"].as_u64(), entry["owner"].as_str()), (Some(game.game_id.as_str()), Some(u64::from(chain_id)), Some(owner_address.as_str())));
    assert_eq!((entry["playable"].clone(), entry["rewards"].as_str(), entry["reward_units"].as_u64()), (true.into(), Some("available"), Some(100)));

    // ---- 4. wallet authentication: nothing plays without a session for that very wallet -----------------
    let uri = format!("/api/games/play/{}", game.game_id);
    let body = serde_json::json!({ "address": player_address, "result_id": "match-1", "payload": "01" });
    assert_eq!(http(&service, true, &uri, None, Some(body.clone())).await.0, 401, "no session");
    let token = sign_in(&service, &player);
    let strangers = sign_in(&service, &stranger);
    assert_eq!(http(&service, true, &uri, Some(&strangers), Some(body.clone())).await.0, 403, "another wallet's session");
    assert_eq!(service.pool(&game.game_id).unwrap().reserved, 0, "nothing was judged or reserved yet");

    // ---- 5. play: judged by the module (twice, in agreement), reserved from this game's pool ------------
    let (status, played) = http(&service, true, &uri, Some(&token), Some(body.clone())).await;
    assert_eq!(status, 200, "{played}");
    assert_eq!((played["outcome"].as_str(), played["stage"].as_str(), played["reward_units"].as_u64(), played["payout_state"].as_str()), (Some("win"), Some("reward_reserved"), Some(100), Some("queued")));
    let (_, again) = http(&service, true, &uri, Some(&token), Some(body.clone())).await;
    assert_eq!(again["credit_id"], played["credit_id"], "the same result is the same credit");
    let loss = serde_json::json!({ "address": player_address, "result_id": "match-2", "payload": "02" });
    let (_, lost) = http(&service, true, &uri, Some(&token), Some(loss)).await;
    assert_eq!((lost["stage"].as_str(), lost["not_rewarded"].as_str()), (Some("verified"), Some("policy_does_not_pay_this_outcome")), "verified is not paid");
    assert_eq!(service.pool(&game.game_id).unwrap().reserved, 100);

    // ---- 6. the payout is signed, its reply is lost, the service restarts, the same bytes are resent ----
    lossy.lose_next_submit_reply.store(true, Ordering::SeqCst);
    assert!(service.process_pool_payout_once().await.unwrap());
    let signed = service.store().pool_payouts_for(&game.game_id).unwrap().remove(0);
    assert_eq!(signed.state, "signed", "the worker does not know the node has it");
    let bytes_before = signed.envelope.clone().unwrap();
    assert_eq!(mempool_count(&base).await, 1, "the node really has the transaction");
    let (_, mid) = http(&service, false, &format!("/api/games/results/{}/match-1", game.game_id), None, None).await;
    assert_eq!((mid["stage"].as_str(), mid["payout_state"].as_str()), (Some("reward_reserved"), Some("signed")), "signed is not paid");
    drop(service);
    let (service, lossy) = build_service(&games_db, &base, &payout_keys);
    lossy.forget_next_status.store(true, Ordering::SeqCst);
    assert!(service.process_pool_payout_once().await.unwrap());
    let resent = service.store().pool_payouts_for(&game.game_id).unwrap().remove(0);
    assert_eq!((resent.state.as_str(), resent.envelope.as_deref()), ("submitted", Some(bytes_before.as_str())), "identical bytes");
    assert_eq!(mempool_count(&base).await, 1, "one transaction, not two");
    let (_, pending) = http(&service, false, &format!("/api/games/results/{}/match-1", game.game_id), None, None).await;
    assert_eq!(pending["stage"], "reward_reserved", "in the mempool is still not paid");
    assert_eq!(balance(&base, &player_address).await, 0);

    // ---- 7. a real block: only now does the API say paid, and the node's balance agrees ---------------
    mine();
    drain(&service, &blockchain, &base, 6).await;
    let (_, paid) = http(&service, false, &format!("/api/games/results/{}/match-1", game.game_id), None, None).await;
    assert_eq!((paid["stage"].as_str(), paid["payout_state"].as_str()), (Some("reward_paid"), Some("confirmed")));
    assert_eq!(balance(&base, &player_address).await, 100, "the player really holds the reward on the node");
    // The session survived the restart (it lives in the database), and the history carries the transaction.
    let (status, history) = http(&service, false, &format!("/api/games/players/{player_address}/results"), None, None).await;
    assert_eq!(status, 200);
    let rows = history["results"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0]["stage"].as_str(), rows[0]["reward_units"].as_u64()), (Some("reward_paid"), Some(100)));
    assert_eq!(rows[0]["transaction_hash"].as_str(), resent.tx_hash.as_deref());
    let (status, replay) = http(&service, true, &uri, Some(&token), Some(body)).await;
    assert_eq!((status, replay["stage"].as_str()), (200, Some("reward_paid")), "the old session still works and replays as paid, not paid again");
    // The books: 100 paid, 1 fee, nothing left reserved, and the wallet's balance is exactly what they say.
    let pool = service.pool(&game.game_id).unwrap();
    assert_eq!((pool.paid, pool.reserved, pool.fees_reserved, pool.fees_paid), (100, 0, 0, 1));
    assert_eq!(balance(&base, &payout_address).await, 10_000 - 100 - 1);
    assert_eq!(service.store().pool_audit().unwrap(), Vec::<String>::new());
    let (_, deposits) = http(&service, false, &format!("/api/games/pools/{}/deposits", game.game_id), None, None).await;
    assert_eq!((deposits["deposits"][0]["amount_units"].as_u64(), deposits["deposits"][0]["funder"].as_str()), (Some(10_000), Some(owner_address.as_str())));
}
