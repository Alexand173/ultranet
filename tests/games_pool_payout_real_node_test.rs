// ============================================================
// REHEARSAL — per-game reward pools paying real transfers on a real (throwaway) node
// ============================================================
//
// The repository's real blockchain and API server run on a local port with a scratch database. Two game owners
// register real AppChains on it, register games, and fund their pools with real L1 transfers that the Games
// service verifies on that node. Verified results (a real WebAssembly adapter) become credits, and the real
// pool payout worker signs real transfers, loses replies, restarts, settles on confirmation and executes an
// owner withdrawal to the chain's new owner after a real handover. No FakeNode, no FakeChain.
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
use UltraNet::appchain::ledger::{APPCHAIN_REGISTER_FEE, APPCHAIN_TRANSFER_FEE, APPCHAIN_TX_VERSION};
use UltraNet::faucet::models::{NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest};
use UltraNet::faucet::node_client::{NodeApi, NodeClient, NodeClientError};
use UltraNet::faucet::signer::FaucetSigner;
use UltraNet::games::adapter::{AdapterModule, RuntimeLimits};
use UltraNet::games::catalog::admin_digest;
use UltraNet::games::chain::HttpChain;
use UltraNet::games::pools::PolicyTerms;
use UltraNet::games::rules::Policy;
use UltraNet::games::service::{system_clock, AdminProof, GamesError, GamesService, ServiceConfig};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn game_pools_pay_real_transfers_exactly_once_through_lost_replies_restarts_and_a_handover() {
    std::env::set_var("ULTRANET_APPCHAIN_ACTIVATION_HEIGHT", "0");
    let scratch = std::env::temp_dir().join(format!("games-pool-real-node-{}", std::process::id()));
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
    let owner_a = QuantumKeyPair::generate();
    let owner_b = QuantumKeyPair::generate();
    let (a, b) = (address_of(&owner_a), address_of(&owner_b));
    let storage = Arc::new(Storage::new(node_db.to_str().unwrap()).unwrap());
    for owner in [&a, &b] {
        storage.save_state(owner, 5_000_000).unwrap();
    }
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

    // ---- 1. two owners register two real AppChains ----------------------------------------------------
    submit_appchain(&base, &owner_a, APPCHAIN_REGISTER_FEE, AppChainOperation::Register { name: "Chain A".into() }).await;
    submit_appchain(&base, &owner_b, APPCHAIN_REGISTER_FEE, AppChainOperation::Register { name: "Chain B".into() }).await;
    mine();
    let ledger = get_json(&format!("{base}/api/appchain/ledger")).await;
    let chain_of = |owner: &str| ledger["chains"].as_array().unwrap().iter().find(|c| c["owner"] == owner).unwrap()["id"].as_u64().unwrap() as u32;
    let (chain_a, chain_b) = (chain_of(&a), chain_of(&b));
    assert_ne!(chain_a, chain_b);

    // ---- 2. games on those chains; the platform approves one real adapter for each -------------------
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
    let module = AdapterModule::compile(&wat::parse_str(wat).unwrap(), RuntimeLimits::default()).unwrap();
    let (service, lossy) = build_service(&games_db, &base, &payout_keys);
    let p = proof(&service, &owner_a, "register", chain_a, "", "Game A");
    let game_a = service.catalog_register(chain_a, "Game A", "", p).await.unwrap();
    let p = proof(&service, &owner_b, "register", chain_b, "", "Game B");
    let game_b = service.catalog_register(chain_b, "Game B", "", p).await.unwrap();
    for game in [&game_a, &game_b] {
        service.platform_approve_adapter(&game.game_id, module.hash(), 1, "ops").unwrap();
    }

    // ---- 3. the owners fund their own pools with real transfers, verified on the node -----------------
    let deposit_a = transfer(&base, &node, &owner_a, &payout_address, 10_000).await;
    let claim = proof(&service, &owner_a, "pool-fund", chain_a, &game_a.game_id, &deposit_a);
    assert!(
        matches!(service.pool_fund(&game_a.game_id, &deposit_a, claim).await, Err(GamesError::Invalid(_))),
        "a pending transfer is not a deposit"
    );
    let deposit_b = transfer(&base, &node, &owner_b, &payout_address, 300).await;
    mine();
    let claim = proof(&service, &owner_a, "pool-fund", chain_a, &game_a.game_id, &deposit_a);
    assert_eq!(service.pool_fund(&game_a.game_id, &deposit_a, claim).await.unwrap().funded, 10_000);
    let claim = proof(&service, &owner_b, "pool-fund", chain_b, &game_b.game_id, &deposit_b);
    assert_eq!(service.pool_fund(&game_b.game_id, &deposit_b, claim).await.unwrap().funded, 300);
    // The same real transfer cannot fund another game, or the same one twice.
    let again = proof(&service, &owner_b, "pool-fund", chain_b, &game_b.game_id, &deposit_a);
    assert!(service.pool_fund(&game_b.game_id, &deposit_a, again).await.is_err(), "A's deposit is not B's to claim");
    let again = proof(&service, &owner_a, "pool-fund", chain_a, &game_a.game_id, &deposit_a);
    assert!(matches!(service.pool_fund(&game_a.game_id, &deposit_a, again).await, Err(GamesError::Conflict)));
    assert_eq!(balance(&base, &payout_address).await, 10_300, "the payout wallet really holds both deposits");

    // ---- 4. policies: proposed by the owners, approved by the platform --------------------------------
    for (owner, game, chain) in [(&owner_a, &game_a, chain_a), (&owner_b, &game_b, chain_b)] {
        let t = terms();
        let summary = format!("{}|{}|{}|{}|{}", t.reward_units, t.eligible_outcomes, t.per_player_per_day, t.per_game_per_day, t.effective_from);
        let p = proof(&service, owner, "policy-propose", chain, &game.game_id, &summary);
        let row = service.policy_propose(&game.game_id, t, p).await.unwrap();
        service.platform_decide_policy(&game.game_id, row.version, true, "ops", "within the ceilings").unwrap();
    }

    // ---- 5. verified wins become credits with their payouts queued, once --------------------------------
    let player = |n: u8| hex::encode([n; 32]);
    let verdict = |n: u8| module.validate_result(&[n; 32], &[1]).unwrap();
    let first = service.reserve_reward_for_payout(&game_a.game_id, "match-1", &player(1), &verdict(1)).unwrap();
    let replay = service.reserve_reward_for_payout(&game_a.game_id, "match-1", &player(1), &verdict(1)).unwrap();
    assert_eq!(first.credit_id, replay.credit_id, "a duplicate request is the same credit");
    assert_eq!(service.store().pool_payouts_for(&game_a.game_id).unwrap().len(), 1, "and the same single payout");
    let pool = service.pool(&game_a.game_id).unwrap();
    assert_eq!((pool.reserved, pool.fees_reserved), (100, 1), "the reward and its network fee are held from game A's pool");
    // Eight concurrent wins by eight players, all in game A.
    let granted: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = (2..10u8)
            .map(|n| {
                let service = service.clone();
                let game = game_a.game_id.clone();
                let validated = verdict(n);
                scope.spawn(move || service.reserve_reward_for_payout(&game, &format!("match-{n}"), &player(n), &validated).is_ok())
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count()
    });
    assert_eq!(granted, 8);
    assert_eq!(service.store().pool_payouts_for(&game_a.game_id).unwrap().len(), 9);
    // Game B holds 300: two rewards of 100 plus their fees fit, a third does not, and refusing it promises nothing.
    service.reserve_reward_for_payout(&game_b.game_id, "b-1", &player(20), &verdict(20)).unwrap();
    service.reserve_reward_for_payout(&game_b.game_id, "b-2", &player(21), &verdict(21)).unwrap();
    assert!(matches!(
        service.reserve_reward_for_payout(&game_b.game_id, "b-3", &player(22), &verdict(22)),
        Err(GamesError::Forbidden(_))
    ), "insufficient funds in B's pool");
    assert_eq!(service.store().pool_payouts_for(&game_b.game_id).unwrap().len(), 2);
    assert!(service.pool(&game_a.game_id).unwrap().available > 8_000, "A's large pool cannot be used to cover B's shortfall");

    // ---- 6. the first payout: the reply to the submission is lost, the same bytes are sent again -------
    let head = service.store().pool_next_open_payout().unwrap().unwrap();
    lossy.lose_next_submit_reply.store(true, Ordering::SeqCst);
    assert!(service.process_pool_payout_once().await.unwrap());
    let signed = service.store().pool_payout(&head.payout_id).unwrap().unwrap();
    assert_eq!(signed.state, "signed", "the worker does not know the node has it");
    let stored_bytes = signed.envelope.clone().expect("the exact signed bytes are stored before they are sent");
    assert_eq!(mempool_count(&base).await, 1, "the node really has the transaction");
    assert_eq!(lossy.submissions.load(Ordering::SeqCst), 1);
    // The process dies here, signed but never confirmed as received. A new process finds the stored bytes in the
    // database and resends exactly those.
    drop(service);
    let (service, lossy) = build_service(&games_db, &base, &payout_keys);
    assert_eq!(service.store().pool_payout(&head.payout_id).unwrap().unwrap().state, "signed", "still signed after the restart");
    lossy.forget_next_status.store(true, Ordering::SeqCst);
    assert!(service.process_pool_payout_once().await.unwrap());
    let resent = service.store().pool_payout(&head.payout_id).unwrap().unwrap();
    assert_eq!((resent.state.as_str(), resent.envelope.as_deref()), ("submitted", Some(stored_bytes.as_str())), "identical bytes, never a re-signed copy");
    assert_eq!(lossy.submissions.load(Ordering::SeqCst), 1, "the restarted process sent the stored bytes once");
    assert_eq!(mempool_count(&base).await, 1, "the node treats it as the same transaction");
    // Pending is not confirmed: polling changes nothing in the books.
    let before = service.pool(&game_a.game_id).unwrap();
    assert!(service.process_pool_payout_once().await.unwrap());
    assert_eq!(service.pool(&game_a.game_id).unwrap(), before, "a pending payout settles nothing");

    // ---- 7. a restart with the payout in flight: nothing is lost and nothing is paid twice -------------
    drop(service);
    let (service, lossy) = build_service(&games_db, &base, &payout_keys);
    // A crash between a credit and its payout: a reservation with no payout is queued by the worker, not forgotten.
    let orphan = service
        .store()
        .pool_reserve(&game_a.game_id, "orphan", &player(30), &"cd".repeat(16), &verdict(30), service.now())
        .unwrap();
    assert!(service.store().pool_payouts_for(&game_a.game_id).unwrap().iter().all(|p| p.ref_id != orphan.credit_id));
    blockchain.write().mine_block().expect("a block"); // the in-flight payout is included
    drain(&service, &blockchain, &base, 60).await;
    assert!(service.store().pool_next_open_payout().unwrap().is_none(), "everything queued has been paid");
    assert_eq!(lossy.submissions.load(Ordering::SeqCst) > 0, true);

    // ---- 8. confirmed settlement, per game, to the unit ------------------------------------------------
    let pool_a = service.pool(&game_a.game_id).unwrap();
    let pool_b = service.pool(&game_b.game_id).unwrap();
    // Game A: ten rewards (1 + 8 + the orphan) of 100, each with a network fee of 1.
    assert_eq!((pool_a.paid, pool_a.reserved, pool_a.fees_reserved, pool_a.fees_paid), (1_000, 0, 0, 10));
    assert_eq!((pool_b.paid, pool_b.reserved, pool_b.fees_reserved, pool_b.fees_paid), (200, 0, 0, 2));
    for n in [1u8, 2, 9, 20, 21, 30] {
        assert_eq!(balance(&base, &player(n)).await, 100, "player {n} received exactly the award");
    }
    assert_eq!(balance(&base, &player(22)).await, 0, "the refused win paid nothing");
    // The payout wallet on the node: both deposits, minus every reward and every fee that was paid.
    assert_eq!(balance(&base, &payout_address).await, 10_300 - 1_000 - 10 - 200 - 2);
    assert_eq!(service.store().pool_audit().unwrap(), Vec::<String>::new(), "the books re-derive from their records");

    // ---- 9. withdrawal: the chain changes hands first; the funds go to the NEW owner -------------------
    let p = proof(&service, &owner_a, "pool-withdraw", chain_a, &game_a.game_id, "500");
    service.pool_withdraw(&game_a.game_id, 500, p).await.unwrap();
    submit_appchain(&base, &owner_a, APPCHAIN_TRANSFER_FEE, AppChainOperation::TransferOwner { chain_id: chain_a, new_owner: b.clone() }).await;
    mine();
    let (a_before, b_before) = (balance(&base, &a).await, balance(&base, &b).await);
    drain(&service, &blockchain, &base, 12).await;
    let withdrawal = service.store().pool_payouts_for(&game_a.game_id).unwrap().into_iter().find(|p| p.kind == "withdrawal").unwrap();
    assert_eq!((withdrawal.state.as_str(), withdrawal.to_address.as_str()), ("confirmed", b.as_str()), "paid to the current owner");
    assert_eq!(balance(&base, &b).await, b_before + 495, "500 returned, 5 of it the network fee");
    assert_eq!(balance(&base, &a).await, a_before, "the previous owner received nothing");
    assert_eq!(service.pool(&game_a.game_id).unwrap().withdrawn, 500);

    // ---- 10. the whole picture ---------------------------------------------------------------------------
    assert_eq!(service.store().pool_audit().unwrap(), Vec::<String>::new());
    service.pool_check_solvency().await.expect("the wallet covers everything the pools owe");
    assert!(service.store().pools_frozen().unwrap().is_none(), "nothing froze along the way");
    let commitments = |view: &UltraNet::games::pools::PoolView| view.reserved + view.paid + view.withdrawn + view.fees_reserved + view.fees_paid;
    let (final_a, final_b) = (service.pool(&game_a.game_id).unwrap(), service.pool(&game_b.game_id).unwrap());
    assert!(commitments(&final_a) <= final_a.funded && commitments(&final_b) <= final_b.funded, "commitments never exceed confirmed funding");
    assert_eq!(final_b.available, 300 - 200 - 2, "game B's pool moved only by game B's payouts");
    assert_eq!(mempool_count(&base).await, 0);

    println!(
        "POOL PAYOUT REHEARSAL OK: game A paid {} (+{} fees) withdrew {}; game B paid {} (+{} fees); wallet {}",
        final_a.paid, final_a.fees_paid, final_a.withdrawn, final_b.paid, final_b.fees_paid,
        balance(&base, &payout_address).await
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
