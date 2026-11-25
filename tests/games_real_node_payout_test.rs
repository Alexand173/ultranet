// ============================================================
// REHEARSAL — a Grid win becomes a real ULTRA transfer on a real (throwaway) node
// ============================================================
//
// What this is: the repository's real blockchain and real API server, bound to a local port on a scratch
// database, and the real Games payout worker talking to it over HTTP with its real signer. No `FakeNode`,
// no `FakeChain`: the transfer the worker signs is validated, queued, mined and confirmed by the node's
// own code, and the balances at the end are read back from that node.
//
// What it is not: production. Every key here is generated in this process and thrown away; the chain has
// one block's worth of history and no peers. It proves how the payout path behaves against real node code
// (admission, idempotent resubmission, nonce, confirmation). It proves nothing about production, about
// consensus with other validators, or about the economics, which are deliberately not set here.
//
// The uncertain-submission step is the point of the second half: a wrapper forwards the signed transfer to
// the node for real and then reports a lost reply, exactly the situation in which a careless retry would
// pay twice. The worker must resend the identical stored bytes, the node must treat that as the same
// transaction, and the recipient must end up paid once.

use async_trait::async_trait;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use UltraNet::faucet::models::{
    NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest,
};
use UltraNet::faucet::node_client::{NodeApi, NodeClient, NodeClientError};
use UltraNet::faucet::signer::FaucetSigner;
use UltraNet::games::chain::HttpChain;
use UltraNet::games::rules::Policy;
use UltraNet::games::service::{
    system_clock, GamesError, GamesService, GridAnswerRequest, ServiceConfig, TurnInput,
};
use UltraNet::games::store::{GamesStore, PayoutState};
use UltraNet::{api, storage::Storage, ultragrid, QuantumKeyPair, UltraBlockchain, UltraWallet};

/// Forwards everything to the real node, and can lose one reply or forget one status answer.
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

    async fn estimate(
        &self,
        recipient: &str,
        amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError> {
        self.inner.estimate(recipient, amount_base_units).await
    }

    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        // The request really reaches the node...
        let reply = self.inner.submit(envelope).await;
        // ...and then the answer is lost on the way back.
        if self.lose_next_submit_reply.swap(false, Ordering::SeqCst) {
            return Err(NodeClientError::Unavailable);
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

async fn balance(base: &str, address: &str) -> u64 {
    get_json(&format!("{base}/api/account/{address}")).await["data"]["balance_base_units"]
        .as_u64()
        .expect("balance")
}

async fn mempool_count(base: &str) -> u64 {
    get_json(&format!("{base}/api/mempool")).await["count"].as_u64().expect("count")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_grid_win_is_paid_once_on_a_real_node_even_when_the_submission_reply_is_lost() {
    // ---- a throwaway node, on a scratch database, on a local port ------------------------------
    let scratch = std::env::temp_dir().join(format!("games-real-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let base = format!("http://127.0.0.1:{port}");
    std::env::set_var("ULTRANET_API_BIND", format!("127.0.0.1:{port}"));
    std::env::set_var("ULTRANET_CORS_ORIGINS", &base);
    std::env::set_var("ULTRANET_ADMIN_TOKEN", "rehearsal-token-never-used-outside-this-process");
    std::env::set_var("ULTRANET_DB_PATH", &scratch);

    let payout_keys = QuantumKeyPair::generate();
    let signer = FaucetSigner::from_keypair_for_tests(payout_keys).expect("payout signer");
    let payout_address = signer.address().to_string();
    let player = UltraWallet::new();
    const PAYOUT_FUNDING: u64 = 10_000_000;
    const PLAYER_START: u64 = 1_000_000;

    let storage = Arc::new(Storage::new(scratch.to_str().unwrap()).expect("scratch storage"));
    storage.save_state(&payout_address, PAYOUT_FUNDING).unwrap();
    storage.save_state(&player.address, PLAYER_START).unwrap();
    let blockchain = Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)));
    // The API server's future is not `Send`, so it gets its own thread and its own actix runtime.
    let served = blockchain.clone();
    std::thread::spawn(move || {
        let _ = actix_web::rt::System::new().block_on(api::run_server(served));
    });
    let mut up = false;
    for _ in 0..100 {
        if reqwest::get(format!("{base}/api/stats")).await.map(|r| r.status().is_success()).unwrap_or(false) {
            up = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(up, "the rehearsal node did not come up");

    // ---- the real Games service, pointed at that node ------------------------------------------
    let mut policy = Policy::standard(3);
    policy.grid.reward_units = 100; // the runbook's rehearsal value: one win reaches the withdrawal minimum
    let node = Arc::new(LossyNode {
        inner: NodeClient::new(base.clone()).unwrap(),
        lose_next_submit_reply: AtomicBool::new(false),
        forget_next_status: AtomicBool::new(false),
        submissions: AtomicUsize::new(0),
    });
    let service = GamesService::new(
        GamesStore::open(std::path::Path::new(":memory:")).unwrap(),
        policy,
        ServiceConfig::new(payout_address.clone()),
        Arc::new(HttpChain::new(&base).unwrap()),
        node.clone(),
        Some(Arc::new(signer)),
        None,
        b"rehearsal-abuse-key".to_vec(),
        system_clock(),
    );

    // ---- 1. a real, verified win credits the player exactly once -------------------------------
    let today = service.today();
    let day = service.grid_day(&today).await.expect("today's puzzles");
    let answer: Vec<TurnInput> = ultragrid::generate(&day.seed, 0)
        .reference
        .iter()
        .map(|t| TurnInput { cell: t.cell, turns: t.turns })
        .collect();
    let request = || GridAnswerRequest {
        address: player.address.clone(),
        date: today.clone(),
        level: 0,
        answer: answer
            .iter()
            .map(|t| TurnInput { cell: t.cell, turns: t.turns })
            .collect(),
    };
    let win = service.submit_grid_answer(request(), None).await.expect("a verified win");
    assert_eq!((win.win, win.verified, win.outcome), (true, true, Some("rewarded")));
    assert_eq!(win.reward_units, 100);
    let again = service.submit_grid_answer(request(), None).await.expect("a resubmission");
    assert!(again.duplicate, "the same win submitted again adds nothing");
    assert_eq!(service.wallet(&player.address).unwrap().credited_units, 100, "credited once");

    // ---- 2. a withdrawal is one payout, however many times it is asked for ----------------------
    let payout = service.withdraw(&player.address).expect("withdrawal");
    assert_eq!(payout.amount_units, 100);
    assert_eq!(service.withdraw(&player.address).expect("asked twice").id, payout.id, "asking twice returns the same payout");

    // ---- 3. the signed transfer reaches the node, but the reply is lost -------------------------
    node.lose_next_submit_reply.store(true, Ordering::SeqCst);
    assert!(service.process_payout_once().await.unwrap());
    let row = service.store().payout(&payout.id).unwrap().unwrap();
    assert!(matches!(row.state, PayoutState::Signed), "the worker does not know it was accepted: {:?}", row.state.code());
    let stored_bytes = row.envelope.clone().expect("the signed bytes are stored before submission");
    let hash = row.tx_hash.clone().expect("and so is the hash");
    assert_eq!(node.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(mempool_count(&base).await, 1, "the node really has the transaction pending");
    assert_eq!(balance(&base, &player.address).await, PLAYER_START, "nothing is paid until a block includes it");

    // ---- 4. the node is asked, does not answer for it, and the worker resends the identical bytes --
    node.forget_next_status.store(true, Ordering::SeqCst);
    assert!(service.process_payout_once().await.unwrap());
    assert_eq!(node.submissions.load(Ordering::SeqCst), 2, "one resubmission");
    let row = service.store().payout(&payout.id).unwrap().unwrap();
    assert_eq!(row.envelope.as_deref(), Some(stored_bytes.as_str()), "the identical stored bytes, never a re-signed copy");
    assert_eq!(row.tx_hash.as_deref(), Some(hash.as_str()));
    assert!(matches!(row.state, PayoutState::Submitted), "{:?}", row.state.code());
    assert_eq!(mempool_count(&base).await, 1, "the node treats it as the same transaction: still one pending");

    // ---- 5. a block is mined; only now is the payout confirmed ----------------------------------
    let block = blockchain.write().mine_block().expect("a block containing the payout");
    assert_eq!(block.transactions.len(), 1, "exactly one transfer on the chain");
    assert!(service.process_payout_once().await.unwrap());
    let row = service.store().payout(&payout.id).unwrap().unwrap();
    assert!(matches!(row.state, PayoutState::Confirmed), "{:?}", row.state.code());

    // ---- 6. the balances on the node agree, to the unit -----------------------------------------
    let fee = row.fee_units.expect("the fee that was signed");
    assert_eq!(balance(&base, &player.address).await, PLAYER_START + 100, "the winner received exactly the award");
    assert_eq!(
        balance(&base, &payout_address).await,
        PAYOUT_FUNDING - 100 - fee,
        "the payout wallet paid the award and the fee, once"
    );
    let wallet = service.wallet(&player.address).unwrap();
    assert_eq!((wallet.credited_units, wallet.paid_units, wallet.available_units), (100, 100, 0));

    // ---- 7. asking again pays nothing, and the worker has nothing left to do --------------------
    assert!(matches!(service.withdraw(&player.address), Err(GamesError::BelowMinimum { .. })));
    assert!(!service.process_payout_once().await.unwrap());
    assert_eq!(mempool_count(&base).await, 0);
    assert_eq!(balance(&base, &player.address).await, PLAYER_START + 100);

    println!(
        "REAL-NODE REHEARSAL OK: payout {} tx {} award 100 fee {} recipient {} -> {} (node mempool 0, block {} with 1 tx)",
        payout.id,
        hash,
        fee,
        player.address,
        PLAYER_START + 100,
        block.index
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
