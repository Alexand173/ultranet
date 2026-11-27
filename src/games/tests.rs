//! Service-level tests: the whole pipeline against a fake chain, a fake node and a fake anchorer.
//!
//! Each test names a rule a player or an operator would care about. The fakes record what they were
//! asked, so a test can assert not only the answer but also what the service did to get it, which is
//! how "signed once", "retried with identical bytes" and "anchored once" are proved.

use super::{
    anchor::{AnchorError, Anchorer},
    chain::{AnchorPoint, ChainError, GamesChain, OwnerActivity, OwnerTx, TransferView, TxView},
    roots,
    rules::{Policy, GAME_CHESS, GAME_GRID},
    peer::PeerVerifier,
    records::{RecordsConfig, RecordsService},
    registry::RegistryCaps,
    service::{
        date_start, shift_date, ChessResultRequest, GamesError, GamesService, GridAnswerRequest,
        ServiceConfig, TurnInput,
    },
    store::{GamesStore, PayoutState},
};
use crate::{
    faucet::{
        models::{
            NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest,
        },
        node_client::{NodeApi, NodeClientError},
        signer::FaucetSigner,
    },
    ultragrid, ProofType, QuantumKeyPair, Transaction, TransactionPayload,
};
use async_trait::async_trait;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

const A: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
const B: &str = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
const DAY: &str = "2026-11-16";

/// The current production arrangement, named once so a fixture that stands for it cannot drift: UltraGrid
/// is #1, the Records Registry #2, Ultra Chess #3. These are today's deployment values, not rules of the
/// system: nothing outside these fixtures may assume them.
const GRID_CHAIN: u32 = 1;
const REGISTRY_CHAIN_ID: u32 = 2;
const CHESS_CHAIN: u32 = crate::games::rules::TEST_CHESS_CHAIN_ID;

// --- fakes -----------------------------------------------------------------------------------

#[derive(Default)]
struct FakeChain {
    active: Mutex<HashSet<String>>,
    everyone_active: AtomicBool,
    anchors: Mutex<Vec<(u32, AnchorPoint)>>,
    down: AtomicBool,
    /// The owner's AppChain transactions as the node's address history would list them.
    owner_txs: Mutex<Vec<OwnerTx>>,
    /// The node's history does not reach back far enough (or hides its pending list): absence proves nothing.
    history_incomplete: AtomicBool,
    /// The anchors index lags the chain: accepted anchors are not visible to the ledger lookups yet.
    index_lag: AtomicBool,
    /// Transactions by some other sender, for tests of an operator naming the wrong transaction.
    other_txs: Mutex<HashMap<String, TxView>>,
    /// The node's history list is full of confirmed entries, so it leaves its pending transactions out (as the
    /// real node does once the owner has 100 of them). The account nonce still counts them.
    hide_pending: AtomicBool,
    /// Report this as the account nonce instead of the number of owner transactions.
    nonce_override: Mutex<Option<u64>>,
    /// The chains the ledger knows. `None` (the default) means every chain is registered.
    registered: Mutex<Option<HashSet<u32>>>,
    /// Who owns each AppChain on the ledger right now. A chain not listed is not registered.
    owners: Mutex<HashMap<u32, String>>,
    /// L1 transfers by hash, as the ledger would show them.
    transfers: Mutex<HashMap<String, TransferView>>,
}

impl FakeChain {
    fn add_anchor(&self, chain_id: u32, sequence: u64, root: [u8; 32], block_timestamp: u64) {
        self.anchors.lock().unwrap().push((
            chain_id,
            AnchorPoint {
                sequence,
                root,
                block_timestamp,
                tx_hash: format!("{sequence:064x}"),
            },
        ));
    }
}

#[async_trait]
impl GamesChain for FakeChain {
    async fn transfer(&self, hash: &str) -> Result<Option<TransferView>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        Ok(self.transfers.lock().unwrap().get(hash).cloned())
    }

    async fn chain_owner(&self, chain_id: u32) -> Result<Option<String>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        Ok(self.owners.lock().unwrap().get(&chain_id).cloned())
    }

    async fn appchain_registered(&self, chain_id: u32) -> Result<bool, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        Ok(self
            .registered
            .lock()
            .unwrap()
            .as_ref()
            .map_or(true, |chains| chains.contains(&chain_id)))
    }

    async fn owner_appchain_activity(
        &self,
        _owner: &str,
        since: u64,
    ) -> Result<OwnerActivity, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        let hide = self.hide_pending.load(Ordering::SeqCst);
        let all = self.owner_txs.lock().unwrap().clone();
        let confirmed = all.iter().filter(|tx| !tx.pending).count() as u64;
        Ok(OwnerActivity {
            transactions: all
                .into_iter()
                .filter(|tx| tx.timestamp >= since && !(hide && tx.pending))
                .collect(),
            complete: !hide && !self.history_incomplete.load(Ordering::SeqCst),
            confirmed_complete: !self.history_incomplete.load(Ordering::SeqCst),
            confirmed_next_nonce: (confirmed > 0).then_some(confirmed),
        })
    }

    async fn account_nonce(&self, _address: &str) -> Result<u64, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        if let Some(forced) = *self.nonce_override.lock().unwrap() {
            return Ok(forced);
        }
        // Like the real node, the account nonce counts pending transactions too.
        Ok(self.owner_txs.lock().unwrap().len() as u64)
    }

    async fn transaction(&self, hash: &str) -> Result<Option<TxView>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        if let Some(view) = self.other_txs.lock().unwrap().get(hash) {
            return Ok(Some(view.clone()));
        }
        Ok(self.owner_txs.lock().unwrap().iter().find(|tx| tx.hash == hash).map(|tx| TxView {
            sender: A.to_string(),
            recipient: "appchain".to_string(),
            timestamp: tx.timestamp,
            pending: tx.pending,
        }))
    }

    async fn anchors_since(
        &self,
        chain_id: u32,
        since: u64,
    ) -> Result<Vec<AnchorPoint>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        if self.index_lag.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let mut points: Vec<AnchorPoint> = self
            .anchors
            .lock()
            .unwrap()
            .iter()
            .filter(|(id, point)| *id == chain_id && point.block_timestamp >= since)
            .map(|(_, point)| point.clone())
            .collect();
        points.sort_by_key(|p| p.sequence);
        Ok(points)
    }

    async fn address_active(&self, address: &str) -> Result<bool, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        Ok(self.everyone_active.load(Ordering::SeqCst)
            || self.active.lock().unwrap().contains(address))
    }

    async fn latest_anchor_before(
        &self,
        chain_id: u32,
        before: u64,
    ) -> Result<Option<AnchorPoint>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        Ok(self
            .anchors
            .lock()
            .unwrap()
            .iter()
            .filter(|(id, point)| *id == chain_id && point.block_timestamp < before)
            .map(|(_, point)| point.clone())
            .max_by_key(|point| point.sequence))
    }

    async fn find_anchor(
        &self,
        chain_id: u32,
        root: &[u8; 32],
    ) -> Result<Option<AnchorPoint>, ChainError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ChainError::Unavailable);
        }
        if self.index_lag.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(self
            .anchors
            .lock()
            .unwrap()
            .iter()
            .find(|(id, point)| *id == chain_id && &point.root == root)
            .map(|(_, point)| point.clone()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubmitMode {
    Accept,
    Unavailable,
    Reject,
}

struct FakeNode {
    balance: AtomicU64,
    nonce: AtomicU64,
    submit_mode: Mutex<SubmitMode>,
    /// Envelopes as the node received them, in order.
    submissions: Mutex<Vec<String>>,
    statuses: Mutex<HashMap<String, String>>,
    wrong_hash: AtomicBool,
    unavailable: AtomicBool,
}

impl FakeNode {
    fn new(balance: u64) -> FakeNode {
        FakeNode {
            balance: AtomicU64::new(balance),
            nonce: AtomicU64::new(7),
            submit_mode: Mutex::new(SubmitMode::Accept),
            submissions: Mutex::new(vec![]),
            statuses: Mutex::new(HashMap::new()),
            wrong_hash: AtomicBool::new(false),
            unavailable: AtomicBool::new(false),
        }
    }

    fn set_mode(&self, mode: SubmitMode) {
        *self.submit_mode.lock().unwrap() = mode;
    }

    fn set_status(&self, hash: &str, status: &str) {
        self.statuses
            .lock()
            .unwrap()
            .insert(hash.to_string(), status.to_string());
    }
}

fn envelope_hash(e: &SignedTransferRequest) -> String {
    let transaction = Transaction {
        sender: e.sender.clone(),
        sender_public_key: e.sender_public_key.clone(),
        recipient: e.recipient.clone(),
        amount: e.amount,
        signature: e.signature.clone(),
        zk_proof: vec![],
        nullifier: e.nullifier_array().unwrap(),
        timestamp: e.timestamp,
        fee: e.fee,
        nonce: e.nonce,
        gas_limit: e.gas_limit,
        gas_price: e.gas_price,
        proof_type: ProofType::Transaction,
        payload: TransactionPayload::StandardTransfer,
        chain_id: e.chain_id,
        version: e.version,
    };
    hex::encode(transaction.get_hash())
}

#[async_trait]
impl NodeApi for FakeNode {
    async fn account(&self, address: &str) -> Result<NodeAccountData, NodeClientError> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(NodeClientError::Unavailable);
        }
        Ok(NodeAccountData {
            address: address.to_string(),
            balance_base_units: Some(self.balance.load(Ordering::SeqCst)),
            balance: None,
            nonce: self.nonce.load(Ordering::SeqCst),
            decimals: 6,
        })
    }

    async fn estimate(
        &self,
        recipient: &str,
        amount: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError> {
        let fee = (amount / 100).max(1);
        Ok(NodeFeeEstimateData {
            recipient: recipient.to_string(),
            amount,
            fee,
            gas_limit: 500_000,
            gas_price: 1,
            total: amount + fee,
        })
    }

    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError> {
        self.submissions
            .lock()
            .unwrap()
            .push(serde_json::to_string(envelope).unwrap());
        match *self.submit_mode.lock().unwrap() {
            SubmitMode::Unavailable => Err(NodeClientError::Unavailable),
            SubmitMode::Reject => Err(NodeClientError::Rejected),
            SubmitMode::Accept => {
                let hash = if self.wrong_hash.load(Ordering::SeqCst) {
                    "ff".repeat(32)
                } else {
                    envelope_hash(envelope)
                };
                // A real node reports what it accepted as pending until a block holds it.
                self.statuses
                    .lock()
                    .unwrap()
                    .entry(hash.clone())
                    .or_insert_with(|| "pending".to_string());
                Ok(NodeTransactionData {
                    hash,
                    status: "pending".into(),
                })
            }
        }
    }

    async fn transaction_status(&self, hash: &str) -> Result<NodeTransactionData, NodeClientError> {
        match self.statuses.lock().unwrap().get(hash) {
            Some(status) => Ok(NodeTransactionData {
                hash: hash.to_string(),
                status: status.clone(),
            }),
            None => Err(NodeClientError::NotFound),
        }
    }
}

#[derive(Default)]
struct FakeAnchorer {
    calls: Mutex<Vec<(u32, [u8; 32])>>,
    fail: AtomicBool,
}

#[async_trait]
impl Anchorer for FakeAnchorer {
    async fn anchor(&self, chain_id: u32, root: &[u8; 32]) -> Result<String, AnchorError> {
        self.calls.lock().unwrap().push((chain_id, *root));
        if self.fail.load(Ordering::SeqCst) {
            return Err(AnchorError::TimedOut);
        }
        Ok(format!("{:064x}", self.calls.lock().unwrap().len()))
    }
}

// --- harness ---------------------------------------------------------------------------------

struct Harness {
    service: Arc<GamesService>,
    /// The Records Service, with its own database, over the same fake chain and clock.
    records: Arc<RecordsService>,
    /// What the Records Service anchored (the Games Reward Service's own anchorer is `anchorer`).
    records_anchorer: Arc<FakeAnchorer>,
    chain: Arc<FakeChain>,
    node: Arc<FakeNode>,
    anchorer: Arc<FakeAnchorer>,
    clock: Arc<AtomicU64>,
    payout_address: String,
}

struct Options {
    policy: Policy,
    pool_balance: u64,
    anchor_games: Vec<String>,
    with_signer: bool,
    registry_chain: Option<u32>,
    caps: RegistryCaps,
    /// Replace the Games Reward Service's anchorer (the default is the in-memory fake).
    games_anchorer: Option<Arc<dyn Anchorer>>,
    /// Give the Records Service a peer secret, which opens its `/internal` route.
    peer_secret: Option<Vec<u8>>,
    /// Use this clock for both services instead of a fresh one.
    clock: Option<Arc<AtomicU64>>,
    /// Require a wallet session on the routes that act for a player.
    require_player_auth: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // The shipped default is chess record-only (P1). Most tests here are about the payout
            // machinery, which only runs when something is credited, so the harness turns chess on
            // explicitly — `chess_is_record_only_by_default_and_still_records_and_anchors` is the
            // test that guards the default itself.
            policy: Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID).with_chess_paying(),
            pool_balance: 100_000,
            anchor_games: vec![GAME_GRID.to_string()],
            with_signer: true,
            registry_chain: Some(REGISTRY_CHAIN_ID),
            caps: RegistryCaps::standard(),
            games_anchorer: None,
            peer_secret: None,
            clock: None,
            require_player_auth: false,
        }
    }
}

fn harness(options: Options) -> Harness {
    let chain = Arc::new(FakeChain::default());
    chain.everyone_active.store(true, Ordering::SeqCst);
    let node = Arc::new(FakeNode::new(options.pool_balance));
    let anchorer = Arc::new(FakeAnchorer::default());
    let clock = options
        .clock
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicU64::new(date_start(DAY).unwrap() + 3_600)));
    let signer = FaucetSigner::from_keypair_for_tests(QuantumKeyPair::generate()).unwrap();
    let payout_address = signer.address().to_string();
    let mut config = ServiceConfig::new(payout_address.clone());
    config.anchor_games = options.anchor_games;
    config.require_player_auth = options.require_player_auth;
    let tick = clock.clone();
    let records_anchorer = Arc::new(FakeAnchorer::default());
    let mut records_config = RecordsConfig::new();
    // Stated, not inherited: the production default is an empty list, because the service's key owns only
    // its registry chain. These tests exercise the peer path, so they name the chains they allow.
    records_config.peer_anchor_chains = vec![GRID_CHAIN, CHESS_CHAIN];
    records_config.registry_chain_id = options.registry_chain;
    // A collision between these chains is a fixture error that would make tests pass for the wrong
    // reason, so every harness refuses it up front.
    if let Some(registry) = options.registry_chain {
        assert!(
            !records_config.peer_anchor_chains.contains(&registry),
            "the Registry's chain must not be one a peer may anchor"
        );
        assert_ne!(options.policy.chess.chain_id, registry, "chess must not use the Registry's chain");
        assert_ne!(options.policy.grid.chain_id, registry, "grid must not use the Registry's chain");
    }
    assert_eq!(options.policy.grid.chain_id, GRID_CHAIN);
    assert_eq!(options.policy.chess.chain_id, CHESS_CHAIN);
    records_config.owner_address = Some(A.to_string());
    let records_tick = clock.clone();
    let records = Arc::new(RecordsService::new(
        GamesStore::open(Path::new(":memory:")).unwrap(),
        options.caps,
        records_config,
        chain.clone(),
        Some(records_anchorer.clone()),
        options
            .peer_secret
            .map(|secret| PeerVerifier::new(secret, "games").unwrap()),
        Arc::new(move || records_tick.load(Ordering::SeqCst)),
    ));
    let service = Arc::new(GamesService::new(
        GamesStore::open(Path::new(":memory:")).unwrap(),
        options.policy,
        config,
        chain.clone(),
        node.clone(),
        options.with_signer.then(|| Arc::new(signer)),
        Some(options.games_anchorer.unwrap_or_else(|| anchorer.clone())),
        b"test-abuse-key".to_vec(),
        Arc::new(move || tick.load(Ordering::SeqCst)),
    ));
    Harness {
        service,
        records,
        records_anchorer,
        chain,
        node,
        anchorer,
        clock,
        payout_address,
    }
}

impl Harness {
    fn advance(&self, seconds: u64) {
        self.clock.fetch_add(seconds, Ordering::SeqCst);
    }

    fn next_day(&self) {
        let start = date_start(&shift_date(&self.service.today(), 1).unwrap()).unwrap();
        self.clock.store(start + 3_600, Ordering::SeqCst);
    }

    async fn grid_answer(&self, address: &str, level: u8) -> GridAnswerRequest {
        let day = self.service.grid_day(&self.service.today()).await.unwrap();
        let reference = ultragrid::generate(&day.seed, level).reference;
        GridAnswerRequest {
            address: address.to_string(),
            date: self.service.today(),
            level,
            answer: reference
                .iter()
                .map(|t| TurnInput {
                    cell: t.cell,
                    turns: t.turns,
                })
                .collect(),
        }
    }

    async fn solve(&self, address: &str, level: u8) -> super::service::WinResponse {
        let request = self.grid_answer(address, level).await;
        self.service
            .submit_grid_answer(request, None)
            .await
            .unwrap()
    }

    fn chess(&self, address: &str, game_id: &str) -> ChessResultRequest {
        ChessResultRequest {
            address: address.to_string(),
            game_id: game_id.to_string(),
            mode: "classic".into(),
            result: "player_win".into(),
            plies: 40,
            app: Some("0.1.0".into()),
        }
    }

    /// Run payout steps until nothing is left to do or the step budget runs out.
    async fn drain_payouts(&self, steps: usize) {
        for _ in 0..steps {
            if !self.service.process_payout_once().await.unwrap() {
                break;
            }
        }
    }
}

const GAME_ID_1: &str = "game-aaaaaaaa-0001";
const GAME_ID_2: &str = "game-aaaaaaaa-0002";

// --- UltraGrid -------------------------------------------------------------------------------

#[tokio::test]
async fn a_solved_grid_level_is_verified_recorded_and_rewarded() {
    let h = harness(Options::default());
    let response = h.solve(A, 0).await;
    assert!(response.win && response.verified);
    assert_eq!(response.outcome, Some("rewarded"));
    assert_eq!(response.reward_units, 10);
    assert_eq!(response.balance_units, 10);
    let grid = response.grid.unwrap();
    assert!(grid.connected && grid.moves <= grid.limit);

    // The stored receipt names the exact puzzle the engine builds for that day.
    let day = h.service.grid_day(DAY).await.unwrap();
    let wins = h.service.store().wins_for_day("grid", DAY).unwrap();
    assert_eq!(wins.len(), 1);
    assert!(wins[0].detail.contains(&hex::encode(day.puzzles[0].id())));
    assert!(wins[0].verified);
}

#[tokio::test]
async fn an_answer_that_does_not_solve_the_puzzle_is_not_a_win_and_is_not_recorded() {
    let h = harness(Options::default());
    let request = GridAnswerRequest {
        address: A.into(),
        date: DAY.into(),
        level: 0,
        answer: vec![],
    };
    let response = h.service.submit_grid_answer(request, None).await.unwrap();
    assert!(!response.win);
    assert_eq!(response.outcome, None);
    assert_eq!(response.reward_units, 0);
    assert!(!response.grid.unwrap().connected);
    assert!(h
        .service
        .store()
        .wins_for_day("grid", DAY)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn an_answer_that_is_not_a_move_is_refused() {
    let h = harness(Options::default());
    let request = GridAnswerRequest {
        address: A.into(),
        date: DAY.into(),
        level: 0,
        answer: vec![TurnInput {
            cell: 9_999,
            turns: 1,
        }],
    };
    let error = h
        .service
        .submit_grid_answer(request, None)
        .await
        .unwrap_err();
    assert!(matches!(error, GamesError::AnswerRefused(_)));
    assert_eq!(error.status(), 400);
}

#[tokio::test]
async fn the_wrong_day_is_refused_with_the_servers_date() {
    let h = harness(Options::default());
    let mut request = h.grid_answer(A, 0).await;
    request.date = "2026-11-15".into();
    match h
        .service
        .submit_grid_answer(request, None)
        .await
        .unwrap_err()
    {
        GamesError::DateMismatch { server_date } => assert_eq!(server_date, DAY),
        other => panic!("expected DateMismatch, got {other}"),
    }
}

#[tokio::test]
async fn an_invalid_address_or_level_is_refused() {
    let h = harness(Options::default());
    let mut request = h.grid_answer(A, 0).await;
    request.address = "not-an-address".into();
    assert!(matches!(
        h.service.submit_grid_answer(request, None).await,
        Err(GamesError::Invalid(_))
    ));
    let mut request = h.grid_answer(A, 0).await;
    request.level = 10;
    assert!(matches!(
        h.service.submit_grid_answer(request, None).await,
        Err(GamesError::Invalid(_))
    ));
}

#[tokio::test]
async fn the_same_answer_twice_is_replayed_not_paid_twice() {
    let h = harness(Options::default());
    let first = h.solve(A, 0).await;
    let second = h.solve(A, 0).await;
    assert!(!first.duplicate && second.duplicate);
    assert_eq!(second.outcome, Some("rewarded"));
    assert_eq!(h.service.wallet(A).unwrap().credited_units, 10);
}

#[tokio::test]
async fn the_fourth_level_of_a_day_is_recorded_and_says_why() {
    let h = harness(Options::default());
    for level in 0..3 {
        assert_eq!(h.solve(A, level).await.outcome, Some("rewarded"));
    }
    let fourth = h.solve(A, 3).await;
    assert_eq!(fourth.outcome, Some("recorded"));
    assert_eq!(fourth.reward_units, 0);
    assert_eq!(fourth.reason, Some("address_daily_limit"));
    assert!(fourth.message.unwrap().contains("recorded"));
    assert_eq!(fourth.balance_units, 30);
}

#[tokio::test]
async fn the_days_puzzles_are_bound_to_the_ledgers_last_anchor_before_midnight() {
    let h = harness(Options::default());
    let midnight = date_start(DAY).unwrap();
    h.chain.add_anchor(1, 1, [0x11; 32], midnight - 7_200);
    h.chain.add_anchor(1, 2, [0x22; 32], midnight - 60); // the latest before the day began
    h.chain.add_anchor(1, 3, [0x33; 32], midnight + 600); // after midnight: must not seed today
    let day = h.service.grid_day(DAY).await.unwrap();
    assert_eq!(day.prev_anchor_sequence, Some(2));
    assert_eq!(day.prev_root, [0x22; 32]);
    assert_eq!(
        day.seed,
        ultragrid::daily_seed(1, DAY, &[0x22; 32]).unwrap()
    );
    assert_eq!(
        day.commitment,
        ultragrid::set_commitment(&ultragrid::daily_set(&day.seed))
    );
}

#[tokio::test]
async fn a_day_with_no_earlier_anchor_uses_the_zero_root() {
    let h = harness(Options::default());
    let day = h.service.grid_day(DAY).await.unwrap();
    assert_eq!(day.prev_anchor_sequence, None);
    assert_eq!(day.prev_root, [0u8; 32]);
}

#[tokio::test]
async fn once_a_day_is_published_a_later_anchor_cannot_change_its_puzzles() {
    let h = harness(Options::default());
    let midnight = date_start(DAY).unwrap();
    h.chain.add_anchor(1, 1, [0x11; 32], midnight - 60);
    let before = h.service.grid_day(DAY).await.unwrap();
    // A different anchor appears that would now be "latest before midnight" if it could be backdated.
    h.chain.add_anchor(1, 2, [0x99; 32], midnight - 30);
    let after = h.service.grid_day(DAY).await.unwrap();
    assert_eq!(before.seed, after.seed);
    assert_eq!(
        h.service
            .store()
            .day_row(DAY, "grid")
            .unwrap()
            .unwrap()
            .prev_anchor_sequence,
        Some(1)
    );
}

#[tokio::test]
async fn when_the_chain_is_down_a_win_is_not_recorded_and_can_be_retried() {
    let h = harness(Options::default());
    let request = h.grid_answer(A, 0).await; // primes the day while the chain is up
    h.chain.down.store(true, Ordering::SeqCst);
    let error = h
        .service
        .submit_grid_answer(request, None)
        .await
        .unwrap_err();
    assert!(matches!(error, GamesError::ChainUnavailable));
    assert_eq!(error.status(), 503);
    assert!(h
        .service
        .store()
        .wins_for_day("grid", DAY)
        .unwrap()
        .is_empty());

    h.chain.down.store(false, Ordering::SeqCst);
    assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn an_address_that_is_not_on_the_chain_is_recorded_without_money() {
    let h = harness(Options::default());
    h.chain.everyone_active.store(false, Ordering::SeqCst);
    let response = h.solve(A, 0).await;
    assert_eq!(response.outcome, Some("recorded"));
    assert_eq!(response.reason, Some("not_on_chain"));
    assert_eq!(response.balance_units, 0);
    h.chain.active.lock().unwrap().insert(B.to_string());
    assert_eq!(h.solve(B, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn a_service_that_is_not_accepting_refuses_new_results() {
    let h = harness(Options::default());
    h.service
        .store()
        .set_state(Some(false), None, Some("maintenance"), 1)
        .unwrap();
    let request = h.grid_answer(A, 0).await;
    assert!(matches!(
        h.service.submit_grid_answer(request, None).await,
        Err(GamesError::Disabled)
    ));
    assert!(matches!(
        h.service
            .submit_chess_result(h.chess(A, GAME_ID_1), None)
            .await,
        Err(GamesError::Disabled)
    ));
}

// --- Ultra Chess -----------------------------------------------------------------------------

#[tokio::test]
async fn a_chess_win_is_rewarded_but_marked_unverified() {
    let h = harness(Options::default());
    let response = h
        .service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    assert_eq!(response.outcome, Some("rewarded"));
    assert_eq!(response.reward_units, 100);
    assert!(
        !response.verified,
        "the server cannot replay a game whose opponent ran on the phone"
    );
    assert!(!h.service.store().wins_for_day("chess", DAY).unwrap()[0].verified);
    let wallet = h.service.wallet(A).unwrap();
    assert!(wallet
        .recent
        .iter()
        .all(|win| !win.verified || win.game == "grid"));
}

#[tokio::test]
async fn a_chess_report_must_be_plausible() {
    let h = harness(Options::default());
    let invalid = |change: &dyn Fn(&mut ChessResultRequest)| {
        let mut request = h.chess(A, GAME_ID_1);
        change(&mut request);
        request
    };
    let cases = [
        invalid(&|r| r.result = "player_loss".into()),
        invalid(&|r| r.mode = "blitz".into()),
        invalid(&|r| r.plies = 9),
        invalid(&|r| r.plies = 601),
        invalid(&|r| r.game_id = "short".into()),
        invalid(&|r| r.game_id = "has spaces in the game id".into()),
        invalid(&|r| r.app = Some("x".repeat(33))),
        invalid(&|r| r.address = "0x0".into()),
    ];
    for request in cases {
        let error = h
            .service
            .submit_chess_result(request, None)
            .await
            .unwrap_err();
        assert!(matches!(error, GamesError::Invalid(_)), "{error}");
    }
    assert!(h
        .service
        .store()
        .wins_for_day("chess", DAY)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_chess_game_id_pays_once_and_cannot_be_claimed_by_two_addresses() {
    let h = harness(Options::default());
    h.service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    let again = h
        .service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    assert!(again.duplicate);
    assert_eq!(h.service.wallet(A).unwrap().credited_units, 100);
    let error = h
        .service
        .submit_chess_result(h.chess(B, GAME_ID_1), None)
        .await
        .unwrap_err();
    assert!(matches!(error, GamesError::Conflict));
    assert_eq!(error.status(), 409);
}

#[tokio::test]
async fn the_fourth_chess_win_is_recorded_and_the_games_limits_are_independent() {
    let h = harness(Options::default());
    let ids = [
        "game-aaaaaaaa-0001",
        "game-aaaaaaaa-0002",
        "game-aaaaaaaa-0003",
        "game-aaaaaaaa-0004",
    ];
    let outcomes: Vec<_> = {
        let mut out = vec![];
        for id in ids {
            out.push(
                h.service
                    .submit_chess_result(h.chess(A, id), None)
                    .await
                    .unwrap()
                    .outcome,
            );
        }
        out
    };
    assert_eq!(
        outcomes,
        vec![
            Some("rewarded"),
            Some("rewarded"),
            Some("rewarded"),
            Some("recorded")
        ]
    );
    // Chess being spent does not touch Grid.
    assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn chess_is_record_only_by_default_and_still_records_and_anchors() {
    // No environment line, no opt-in: exactly what an installation gets today. The win must still be
    // recorded and still reach the day's anchor, and the player must be told why nothing was paid.
    let h = harness(Options {
        policy: Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID),
        anchor_games: vec![GAME_CHESS.to_string()],
        ..Options::default()
    });
    let response = h
        .service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    assert_eq!(response.outcome, Some("recorded"));
    assert_eq!(response.reason, Some("not_paying"));
    assert_eq!(
        h.service.store().wins_for_day("chess", DAY).unwrap().len(),
        1
    );
    assert_eq!(
        h.service.wallet(A).unwrap().credited_units,
        0,
        "a record-only win credits nothing at all"
    );

    // The record still matters: it is what the day's anchor commits to. A game that does not pay yet
    // still has to be checkable later.
    h.next_day();
    h.service.run_daily().await.unwrap();
    let calls = h.anchorer.calls.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "the recorded win still reaches the day's anchor"
    );
    assert_eq!(
        calls[0].0,
        crate::games::rules::TEST_CHESS_CHAIN_ID,
        "on chess's own chain, never the Registry's"
    );
}

// --- withdrawals and payouts -----------------------------------------------------------------

async fn earn_chess_hundred(h: &Harness, address: &str, game_id: &str) {
    h.service
        .submit_chess_result(h.chess(address, game_id), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_withdrawal_below_the_minimum_is_refused_with_the_numbers() {
    let h = harness(Options::default());
    h.solve(A, 0).await; // 10 units
    match h.service.withdraw(A).unwrap_err() {
        GamesError::BelowMinimum { available, minimum } => {
            assert_eq!((available, minimum), (10, 100))
        }
        other => panic!("expected BelowMinimum, got {other}"),
    }
    assert!(!h.service.wallet(A).unwrap().can_withdraw);
}

#[tokio::test]
async fn a_full_withdrawal_signs_once_submits_and_settles_on_confirmation() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let wallet = h.service.wallet(A).unwrap();
    assert!(wallet.can_withdraw);

    let queued = h.service.withdraw(A).unwrap();
    assert_eq!((queued.amount_units, queued.state), (100, "queued"));
    assert!(
        !h.service.wallet(A).unwrap().can_withdraw,
        "the balance is reserved at once"
    );

    h.drain_payouts(1).await;
    let row = h.service.store().payout(&queued.id).unwrap().unwrap();
    assert_eq!(row.state, PayoutState::Submitted);
    assert_eq!(h.node.submissions.lock().unwrap().len(), 1);

    // The transfer is exactly what was owed: the whole balance, to the player, the node's nonce, and
    // the fee on top, paid by the wallet rather than taken from the player.
    let envelope: SignedTransferRequest =
        serde_json::from_str(&h.node.submissions.lock().unwrap()[0]).unwrap();
    assert_eq!(
        (
            envelope.recipient.as_str(),
            envelope.amount,
            envelope.fee,
            envelope.nonce
        ),
        (A, 100, 1, 7)
    );
    assert_eq!(
        row.tx_hash.as_deref(),
        Some(envelope_hash(&envelope).as_str())
    );
    assert_eq!(row.fee_units, Some(1));

    // Not paid until the chain says so.
    h.drain_payouts(1).await;
    assert_eq!(
        h.service.store().payout(&queued.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
    assert_eq!(h.service.wallet(A).unwrap().paid_units, 0);

    h.node
        .set_status(row.tx_hash.as_deref().unwrap(), "confirmed");
    h.drain_payouts(1).await;
    let wallet = h.service.wallet(A).unwrap();
    assert_eq!(
        (
            wallet.credited_units,
            wallet.paid_units,
            wallet.reserved_units,
            wallet.available_units
        ),
        (100, 100, 0, 0)
    );
    assert!(wallet.open_payout.is_none());
    assert!(h.service.store().audit().unwrap().ok());
    assert_eq!(
        h.node.submissions.lock().unwrap().len(),
        1,
        "a confirmed payout is never submitted again"
    );
}

#[tokio::test]
async fn asking_to_withdraw_twice_returns_the_same_payout() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let first = h.service.withdraw(A).unwrap();
    let second = h.service.withdraw(A).unwrap();
    assert_eq!(first.id, second.id);
    h.drain_payouts(2).await;
    assert_eq!(h.node.submissions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_unclear_submission_is_retried_with_identical_bytes() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let payout = h.service.withdraw(A).unwrap();

    h.node.set_mode(SubmitMode::Unavailable);
    h.drain_payouts(1).await;
    let row = h.service.store().payout(&payout.id).unwrap().unwrap();
    assert_eq!(
        row.state,
        PayoutState::Signed,
        "signed and kept, not forgotten"
    );
    let first_envelope = row.envelope.clone().unwrap();

    // The node answers again but still has no record of it: the same bytes go out a second time.
    h.node.set_mode(SubmitMode::Accept);
    h.drain_payouts(1).await;
    let submissions = h.node.submissions.lock().unwrap().clone();
    assert_eq!(submissions.len(), 2);
    assert_eq!(submissions[0], submissions[1], "a retry never re-signs");
    assert_eq!(submissions[1], first_envelope);
    assert_eq!(
        h.service.store().payout(&payout.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
    assert!(
        h.service
            .store()
            .payout(&payout.id)
            .unwrap()
            .unwrap()
            .attempts
            >= 1
    );
}

#[tokio::test]
async fn a_hash_the_node_disagrees_about_pauses_payments_and_keeps_the_payout_open() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let payout = h.service.withdraw(A).unwrap();
    h.node.wrong_hash.store(true, Ordering::SeqCst);
    h.drain_payouts(1).await;
    let state = h.service.store().state().unwrap();
    assert!(!state.paying);
    assert!(state.reason.unwrap().contains("different hash"));
    let row = h.service.store().payout(&payout.id).unwrap().unwrap();
    assert!(
        row.state.is_open(),
        "an unexplained payout is never settled either way"
    );
    // Nothing further is attempted while paused.
    assert!(!h.service.process_payout_once().await.unwrap());
}

#[tokio::test]
async fn a_payout_wallet_that_has_shrunk_pauses_payments_and_keeps_the_balance_whole() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await; // credited while the pool was healthy
    let payout = h.service.withdraw(A).unwrap();
    h.node.balance.store(500, Ordering::SeqCst); // then the wallet is drained by something else
    h.drain_payouts(1).await;

    // amount 100 + fee 1 + reserve 1,000 is more than the 500 the wallet now holds.
    assert!(!h.service.store().state().unwrap().paying);
    assert!(
        h.node.submissions.lock().unwrap().is_empty(),
        "nothing was signed or sent"
    );
    let row = h.service.store().payout(&payout.id).unwrap().unwrap();
    assert_eq!(row.state, PayoutState::Queued);
    assert_eq!(
        h.service.wallet(A).unwrap().reserved_units,
        100,
        "the player's credit is still reserved for them"
    );

    // After a top-up and an explicit enable, the same payout goes out.
    h.node.balance.store(100_000, Ordering::SeqCst);
    h.service
        .store()
        .set_state(None, Some(true), None, 1)
        .unwrap();
    h.drain_payouts(1).await;
    assert_eq!(
        h.service.store().payout(&payout.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
}

#[tokio::test]
async fn a_payout_that_never_confirms_pauses_for_a_human_who_can_resolve_it_either_way() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let payout = h.service.withdraw(A).unwrap();
    h.drain_payouts(1).await;
    let hash = h
        .service
        .store()
        .payout(&payout.id)
        .unwrap()
        .unwrap()
        .tx_hash
        .unwrap();
    h.node.set_status(&hash, "pending");

    h.advance(60);
    h.drain_payouts(1).await;
    assert!(
        h.service.store().state().unwrap().paying,
        "still within the confirmation window"
    );

    h.advance(900);
    h.drain_payouts(1).await;
    assert!(!h.service.store().state().unwrap().paying);
    let row = h.service.store().payout(&payout.id).unwrap().unwrap();
    assert!(row.state.is_open(), "not settled by guessing");

    // The operator checks the chain. It never landed, so the player's balance is released.
    h.service
        .store()
        .settle_payout(
            &payout.id,
            false,
            Some("operator: not on chain"),
            h.service.now(),
        )
        .unwrap();
    let wallet = h.service.wallet(A).unwrap();
    assert_eq!((wallet.reserved_units, wallet.available_units), (0, 100));
    assert!(h.service.store().audit().unwrap().ok());
}

#[tokio::test]
async fn a_node_that_rejects_a_signed_payout_pauses_payments() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    let payout = h.service.withdraw(A).unwrap();
    h.node.set_mode(SubmitMode::Reject);
    h.drain_payouts(1).await;
    assert!(!h.service.store().state().unwrap().paying);
    assert!(h
        .service
        .store()
        .state()
        .unwrap()
        .reason
        .unwrap()
        .contains("rejected"));
    assert!(h
        .service
        .store()
        .payout(&payout.id)
        .unwrap()
        .unwrap()
        .state
        .is_open());
}

#[tokio::test]
async fn withdrawals_are_refused_while_payments_are_paused_and_balances_are_untouched() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    h.service
        .store()
        .set_state(None, Some(false), Some("operator pause"), 1)
        .unwrap();
    let error = h.service.withdraw(A).unwrap_err();
    assert!(matches!(error, GamesError::PayoutsPaused));
    assert_eq!(error.status(), 503);
    let wallet = h.service.wallet(A).unwrap();
    assert!(wallet.payouts_paused && !wallet.can_withdraw);
    assert_eq!(wallet.available_units, 100);
    // Results are still accepted and credited while payments are paused.
    assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn without_a_signer_payments_pause_instead_of_failing_quietly() {
    let h = harness(Options {
        with_signer: false,
        ..Options::default()
    });
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    h.service.withdraw(A).unwrap();
    h.drain_payouts(1).await;
    assert!(!h.service.store().state().unwrap().paying);
    assert!(h
        .service
        .store()
        .state()
        .unwrap()
        .reason
        .unwrap()
        .contains("signer"));
}

#[tokio::test]
async fn payouts_are_processed_one_at_a_time_in_order() {
    let h = harness(Options::default());
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    earn_chess_hundred(&h, B, GAME_ID_2).await;
    let first = h.service.withdraw(A).unwrap();
    h.advance(1);
    let second = h.service.withdraw(B).unwrap();
    h.drain_payouts(1).await;
    // Only the first has been touched.
    assert_eq!(
        h.service.store().payout(&first.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
    assert_eq!(
        h.service.store().payout(&second.id).unwrap().unwrap().state,
        PayoutState::Queued
    );
    h.node.set_status(
        h.service
            .store()
            .payout(&first.id)
            .unwrap()
            .unwrap()
            .tx_hash
            .as_deref()
            .unwrap(),
        "confirmed",
    );
    h.drain_payouts(2).await;
    assert_eq!(
        h.service.store().payout(&first.id).unwrap().unwrap().state,
        PayoutState::Confirmed
    );
    assert_eq!(
        h.service.store().payout(&second.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
    // The two transfers used the same wallet, so they must not share a nonce... the node advances it on acceptance.
    assert_eq!(h.node.submissions.lock().unwrap().len(), 2);
}

// --- closing and anchoring a day -------------------------------------------------------------

async fn expected_grid_root(
    h: &Harness,
    date: &str,
    wins: &[(&str, u8, u16, [u8; 32])],
) -> [u8; 32] {
    let day = h.service.grid_day(date).await.unwrap();
    let results = roots::grid_results_root(
        wins.iter()
            .map(|(address, level, moves, puzzle_id)| roots::GridResult {
                address: roots::address_bytes(address).unwrap(),
                level: *level,
                moves: *moves,
                puzzle_id: *puzzle_id,
            })
            .collect(),
    );
    roots::grid_day_root(date, &day.commitment, &results)
}

#[tokio::test]
async fn the_daily_run_closes_yesterday_and_anchors_the_recomputable_root_exactly_once() {
    let h = harness(Options::default());
    let response = h.solve(A, 0).await;
    let day = h.service.grid_day(DAY).await.unwrap();
    let moves = response.grid.unwrap().moves as u16;
    let expected = expected_grid_root(&h, DAY, &[(A, 0, moves, day.puzzles[0].id())]).await;

    // Today is never closed early. The only thing the run does now is anchor the empty day before it,
    // the heartbeat that keeps a listed chain "live".
    let report = h.service.run_daily().await.unwrap();
    assert!(
        report.anchored.iter().all(|(_, date)| date != DAY),
        "today must stay open"
    );
    let heartbeat = h.anchorer.calls.lock().unwrap().clone();
    assert_eq!(heartbeat.len(), 1);
    assert_ne!(heartbeat[0].1, expected);
    // It lands on the ledger, so that earlier day is settled and not submitted again.
    h.chain.add_anchor(1, 1, heartbeat[0].1, h.service.now());

    h.next_day();
    let report = h.service.run_daily().await.unwrap();
    assert!(report
        .anchored
        .contains(&("grid".to_string(), DAY.to_string())));
    let calls = h.anchorer.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "one heartbeat, one real day");
    assert_eq!(
        calls[1],
        (1, expected),
        "the anchored root is the one anyone can recompute"
    );
    let row = h.service.store().day_row(DAY, "grid").unwrap().unwrap();
    assert_eq!(row.state, "anchored");
    assert_eq!(row.day_root, Some(expected.to_vec()));

    // Run again straight away: the anchor is pending, so nothing is submitted a second time.
    h.service.run_daily().await.unwrap();
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 2);

    // Once it is on the ledger the day is confirmed, still without another submission.
    h.chain.add_anchor(1, 2, expected, h.service.now());
    let report = h.service.run_daily().await.unwrap();
    assert!(report
        .confirmed
        .contains(&("grid".to_string(), DAY.to_string())));
    assert_eq!(
        h.service
            .store()
            .day_row(DAY, "grid")
            .unwrap()
            .unwrap()
            .state,
        "confirmed"
    );
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn an_anchor_of_unclear_outcome_is_checked_on_the_ledger_before_it_is_repeated() {
    let h = harness(Options::default());
    h.solve(A, 0).await;
    h.next_day();

    // The anchor command times out, but the transaction had in fact reached the node and landed.
    h.anchorer.fail.store(true, Ordering::SeqCst);
    h.service.run_daily().await.unwrap();
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 1);
    let row = h.service.store().day_row(DAY, "grid").unwrap().unwrap();
    assert!(row.last_error.is_some());
    h.chain.add_anchor(
        1,
        1,
        row.day_root.clone().unwrap().try_into().unwrap(),
        h.service.now(),
    );

    // After the retry interval the day is found on the ledger and confirmed: no second anchor is paid for.
    h.advance(601);
    h.service.run_daily().await.unwrap();
    assert_eq!(
        h.service
            .store()
            .day_row(DAY, "grid")
            .unwrap()
            .unwrap()
            .state,
        "confirmed"
    );
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_failed_anchor_is_retried_but_not_before_the_retry_interval() {
    let h = harness(Options::default());
    h.solve(A, 0).await;
    h.next_day();
    h.anchorer.fail.store(true, Ordering::SeqCst);
    h.service.run_daily().await.unwrap();
    h.advance(60);
    h.service.run_daily().await.unwrap();
    assert_eq!(
        h.anchorer.calls.lock().unwrap().len(),
        1,
        "too soon to try again"
    );
    h.anchorer.fail.store(false, Ordering::SeqCst);
    h.advance(600);
    h.service.run_daily().await.unwrap();
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 2);
    assert_eq!(
        h.service
            .store()
            .day_row(DAY, "grid")
            .unwrap()
            .unwrap()
            .state,
        "anchored"
    );
}

#[tokio::test]
async fn an_empty_yesterday_is_anchored_as_a_heartbeat_but_older_empty_days_are_not_backfilled() {
    let h = harness(Options::default());
    // Three quiet days pass with the service down; on the fourth it starts.
    for _ in 0..3 {
        h.next_day();
    }
    let report = h.service.run_daily().await.unwrap();
    assert_eq!(
        report.anchored.len(),
        1,
        "{report_anchored:?}",
        report_anchored = report.anchored
    );
    assert_eq!(h.anchorer.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn heartbeats_can_be_switched_off() {
    let mut h = harness(Options::default());
    // Rebuild with heartbeats off via a fresh service sharing nothing: configure directly.
    let chain = h.chain.clone();
    let node = h.node.clone();
    let anchorer = h.anchorer.clone();
    let clock = h.clock.clone();
    let mut config = ServiceConfig::new("00".repeat(32));
    config.anchor_empty_days = false;
    let tick = clock.clone();
    h.service = Arc::new(GamesService::new(
        GamesStore::open(Path::new(":memory:")).unwrap(),
        Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID),
        config,
        chain,
        node,
        None,
        Some(anchorer),
        b"k".to_vec(),
        Arc::new(move || tick.load(Ordering::SeqCst)),
    ));
    h.next_day();
    h.service.run_daily().await.unwrap();
    assert!(h.anchorer.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn only_the_configured_games_are_anchored_and_chess_uses_its_own_chain() {
    let h = harness(Options::default());
    h.service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    h.next_day();
    h.service.run_daily().await.unwrap();
    let calls = h.anchorer.calls.lock().unwrap().clone();
    assert!(
        calls.iter().all(|(chain, _)| *chain == 1),
        "chess is not anchored until its chain is configured"
    );

    let both = harness(Options {
        anchor_games: vec!["grid".into(), "chess".into()],
        ..Options::default()
    });
    both.service
        .submit_chess_result(both.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    both.next_day();
    both.service.run_daily().await.unwrap();
    let calls = both.anchorer.calls.lock().unwrap().clone();
    let chess_call = calls
        .iter()
        .find(|(chain, _)| *chain == crate::games::rules::TEST_CHESS_CHAIN_ID)
        .expect("chess anchored on its own chain");
    let result = roots::ChessResult {
        address: roots::address_bytes(A).unwrap(),
        game_id_hash: roots::chess_game_id_hash(GAME_ID_1),
        mode: 0,
        plies: 40,
    };
    let expected = roots::chess_day_root(DAY, &roots::chess_results_root(vec![result]));
    assert_eq!(chess_call.1, expected);
}

#[tokio::test]
async fn a_finished_days_root_is_fixed_even_if_a_late_win_somehow_appears() {
    let h = harness(Options::default());
    h.solve(A, 0).await;
    h.next_day();
    h.service.run_daily().await.unwrap();
    let anchored = h
        .service
        .store()
        .day_row(DAY, "grid")
        .unwrap()
        .unwrap()
        .day_root;

    // A write for a past day (which intake refuses, but the store would accept) must not move the root.
    h.service
        .store()
        .record_win_unlimited(
            &super::store::WinInput {
                game: "grid",
                address: B.to_string(),
                day: DAY.to_string(),
                item: "5".to_string(),
                moves: 4,
                detail: "{\"puzzle_id\":\"00\"}".to_string(),
                verified: true,
                client_digest: None,
            },
            &Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID),
            true,
            h.service.now(),
        )
        .unwrap();
    h.service.close_and_anchor("grid", DAY).await.unwrap();
    assert_eq!(
        h.service
            .store()
            .day_row(DAY, "grid")
            .unwrap()
            .unwrap()
            .day_root,
        anchored
    );
}

#[tokio::test]
async fn today_cannot_be_closed() {
    let h = harness(Options::default());
    assert!(matches!(
        h.service.close_and_anchor("grid", DAY).await,
        Err(GamesError::Invalid(_))
    ));
    assert!(matches!(
        h.service.close_and_anchor("grid", "2026-12-01").await,
        Err(GamesError::Invalid(_))
    ));
}

#[tokio::test]
async fn the_public_status_shows_the_rules_and_nothing_secret() {
    let h = harness(Options::default());
    let status = h.service.status().unwrap();
    let json = serde_json::to_string(&status).unwrap();
    assert!(status.accepting && status.paying);
    assert_eq!(status.withdraw_min_units, 100);
    assert_eq!(status.games.len(), 2);
    let grid = status.games.iter().find(|g| g.game == "grid").unwrap();
    let chess = status.games.iter().find(|g| g.game == "chess").unwrap();
    assert!(
        grid.verified && !chess.verified,
        "the status says which game the server can prove"
    );
    assert_eq!((grid.reward_units, chess.reward_units), (10, 100));
    assert!(
        grid.pays,
        "the harness turns chess on; Grid pays in every configuration"
    );
    assert!(
        chess.pays,
        "the harness enabled chess, so the status must say so"
    );
    for secret in ["key", "password", "secret", "token", "envelope"] {
        assert!(
            !json.to_lowercase().contains(secret),
            "status leaked {secret}: {json}"
        );
    }
}

#[test]
fn the_dates_helpers_agree_with_each_other() {
    assert_eq!(super::service::date_of(date_start(DAY).unwrap()), DAY);
    assert_eq!(
        super::service::date_of(date_start(DAY).unwrap() - 1),
        "2026-11-15"
    );
    assert_eq!(shift_date(DAY, 1).unwrap(), "2026-11-17");
    assert_eq!(shift_date("2026-12-31", 1).unwrap(), "2027-01-01");
    assert_eq!(shift_date("2024-02-28", 1).unwrap(), "2024-02-29");
    assert_eq!(date_start("garbage"), None);
    assert_eq!(date_start("2026-13-40"), None);
    let _ = GAME_CHESS;
}

// --- solvency: never promise what the wallet cannot pay -----------------------------------------

#[tokio::test]
async fn a_reward_is_never_promised_beyond_what_the_wallet_holds() {
    // 1,030 in the wallet, 1,000 reserved for fees: room for exactly 30 units of promises, three grid wins.
    let h = harness(Options {
        pool_balance: 1_030,
        ..Options::default()
    });
    let addresses = [
        A,
        B,
        "1111111111111111111111111111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222222222222222222222222222",
    ];
    let mut outcomes = vec![];
    for address in addresses {
        let response = h.solve(address, 0).await;
        outcomes.push((response.outcome, response.reason));
    }
    assert_eq!(
        outcomes,
        vec![
            (Some("rewarded"), None),
            (Some("rewarded"), None),
            (Some("rewarded"), None),
            (Some("recorded"), Some("pool_low")),
        ]
    );
    let (credited, _, _) = h.service.store().totals().unwrap();
    assert_eq!(
        credited, 30,
        "what is owed never exceeds what the wallet can cover above its reserve"
    );
    // The fourth win still exists and still anchors.
    assert_eq!(
        h.service.store().wins_for_day("grid", DAY).unwrap().len(),
        4
    );
    assert!(h.service.store().audit().unwrap().ok());
}

#[tokio::test]
async fn an_empty_pool_records_wins_without_promising_anything() {
    let h = harness(Options {
        pool_balance: 0,
        ..Options::default()
    });
    let response = h.solve(A, 0).await;
    assert_eq!(
        (response.outcome, response.reason),
        (Some("recorded"), Some("pool_low"))
    );
    assert!(response.message.unwrap().contains("pool is low"));
    assert_eq!(h.service.wallet(A).unwrap().credited_units, 0);
}

#[tokio::test]
async fn rewards_resume_once_the_pool_is_topped_up() {
    let h = harness(Options {
        pool_balance: 1_005,
        ..Options::default()
    });
    assert_eq!(
        h.solve(A, 0).await.reason,
        Some("pool_low"),
        "5 units of room cannot promise 10"
    );
    h.node.balance.store(100_000, Ordering::SeqCst);
    assert_eq!(h.solve(A, 1).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn what_is_already_owed_counts_against_the_pool_across_games() {
    // Room for 150: one chess win (100) fits; a second does not; a grid win (10) still does.
    let h = harness(Options {
        pool_balance: 1_150,
        ..Options::default()
    });
    assert_eq!(
        h.service
            .submit_chess_result(h.chess(A, GAME_ID_1), None)
            .await
            .unwrap()
            .outcome,
        Some("rewarded")
    );
    let second = h
        .service
        .submit_chess_result(h.chess(B, GAME_ID_2), None)
        .await
        .unwrap();
    assert_eq!(
        (second.outcome, second.reason),
        (Some("recorded"), Some("pool_low"))
    );
    assert_eq!(h.solve(B, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn when_the_pool_cannot_be_read_a_win_is_not_recorded_and_can_be_sent_again() {
    let h = harness(Options::default());
    let request = h.grid_answer(A, 0).await;
    h.node.unavailable.store(true, Ordering::SeqCst);
    assert!(matches!(
        h.service.submit_grid_answer(request, None).await,
        Err(GamesError::ChainUnavailable)
    ));
    assert!(h
        .service
        .store()
        .wins_for_day("grid", DAY)
        .unwrap()
        .is_empty());
    h.node.unavailable.store(false, Ordering::SeqCst);
    assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
}

#[tokio::test]
async fn a_game_that_does_not_pay_does_not_need_the_pool() {
    let h = harness(Options {
        policy: Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID),
        pool_balance: 0,
        ..Options::default()
    });
    h.node.unavailable.store(true, Ordering::SeqCst);
    // Record-only chess neither reads the wallet nor depends on it being reachable.
    let response = h
        .service
        .submit_chess_result(h.chess(A, GAME_ID_1), None)
        .await
        .unwrap();
    assert_eq!(response.reason, Some("not_paying"));
}

// --- the rewards wallet cannot take part in its own game -----------------------------------------

#[tokio::test]
async fn the_rewards_wallets_own_address_cannot_win_look_up_or_withdraw() {
    let h = harness(Options::default());
    let wallet = h.payout_address.clone();
    let mut request = h.grid_answer(A, 0).await;
    request.address = wallet.clone();
    assert!(matches!(
        h.service.submit_grid_answer(request, None).await,
        Err(GamesError::Invalid(_))
    ));
    assert!(matches!(
        h.service
            .submit_chess_result(h.chess(&wallet, GAME_ID_1), None)
            .await,
        Err(GamesError::Invalid(_))
    ));
    assert!(matches!(
        h.service.wallet(&wallet),
        Err(GamesError::Invalid(_))
    ));
    assert!(matches!(
        h.service.withdraw(&wallet),
        Err(GamesError::Invalid(_))
    ));
    assert!(
        h.service.store().state().unwrap().paying,
        "a stranger naming the wallet pauses nothing"
    );
}

#[tokio::test]
async fn a_payout_somehow_addressed_to_the_wallet_is_released_without_pausing_anyone_else() {
    let h = harness(Options::default());
    // Bypass intake (which refuses this) to model a bug or a hand-edited database.
    let wallet = h.payout_address.clone();
    let input = super::store::WinInput {
        game: "chess",
        address: wallet.clone(),
        day: DAY.to_string(),
        item: "game-self-payout-0001".to_string(),
        moves: 40,
        detail: "{\"mode\":\"classic\",\"plies\":40}".to_string(),
        verified: false,
        client_digest: None,
    };
    // Chess paying, so the forged row really does carry a balance the payout can be made against.
    let paying = Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID).with_chess_paying();
    h.service
        .store()
        .record_win_unlimited(&input, &paying, true, h.service.now())
        .unwrap();
    let now = h.service.now();
    h.service
        .store()
        .create_payout(&wallet, &paying, "bad-row", now - now % 86_400, now)
        .unwrap();
    earn_chess_hundred(&h, A, GAME_ID_1).await;
    h.advance(1);
    let good = h.service.withdraw(A).unwrap();

    h.drain_payouts(1).await;
    let bad = h.service.store().payout("bad-row").unwrap().unwrap();
    assert_eq!(bad.state, PayoutState::Failed);
    assert!(
        h.service.store().state().unwrap().paying,
        "one bad row must not stop everyone's payments"
    );
    // The honest payout behind it goes out on the very next step.
    h.drain_payouts(1).await;
    assert_eq!(
        h.service.store().payout(&good.id).unwrap().unwrap().state,
        PayoutState::Submitted
    );
    assert!(h.service.store().audit().unwrap().ok());
}

// --- the registry: third-party games ------------------------------------------------------------

mod registry_service {
    use super::*;
    use crate::games::{
        registry::{self, GameStatus},
        records::RegistryResultRequest,
    };

    const OWNER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";

    /// A game in the given status, with its API key.
    fn game(h: &Harness, id: &str, status: GameStatus) -> String {
        let (_, key) = h
            .records
            .store()
            .register_game(id, id, OWNER, h.service.now())
            .unwrap();
        h.records
            .store()
            .set_game_status(id, status, h.service.now())
            .unwrap();
        key
    }

    fn result(player: &str, id: &str, score: u64) -> RegistryResultRequest {
        RegistryResultRequest {
            player: player.into(),
            result_id: id.into(),
            score,
            data_hash: None,
        }
    }

    fn registry_chain_calls(h: &Harness) -> Vec<[u8; 32]> {
        h.records_anchorer
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(chain, _)| *chain == REGISTRY_CHAIN_ID)
            .map(|(_, root)| *root)
            .collect()
    }

    #[test]
    fn a_registered_game_records_a_result_with_its_key_and_a_replay_adds_nothing() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        let first = h
            .records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 900))
            .unwrap();
        assert_eq!(
            (
                first.duplicate,
                first.anchored,
                first.status,
                first.day.as_str()
            ),
            (false, true, "registered", DAY)
        );
        let again = h
            .records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 900))
            .unwrap();
        assert!(again.duplicate);
        assert_eq!(
            h.records
                .store()
                .results_for_day("tetris", DAY)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_sandbox_game_records_but_says_plainly_it_is_not_anchored() {
        let h = harness(Options::default());
        let key = game(&h, "scratch", GameStatus::Sandbox);
        let response = h
            .records
            .submit_registry_result("scratch", &key, result(A, "match-0001", 1))
            .unwrap();
        assert_eq!((response.anchored, response.status), (false, "sandbox"));
    }

    #[test]
    fn a_wrong_missing_or_foreign_key_and_an_unknown_game_are_one_and_the_same_refusal() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        let other = game(&h, "snake", GameStatus::Registered);
        for (id, presented) in [
            ("tetris", "ugk_wrong"),
            ("tetris", ""),
            ("tetris", other.as_str()),
            ("no-such-game", key.as_str()),
        ] {
            let error = h
                .records
                .submit_registry_result(id, presented, result(A, "match-0001", 1))
                .unwrap_err();
            assert!(
                matches!(error, GamesError::InvalidKey),
                "{id} / {presented}"
            );
            assert_eq!(error.status(), 401);
        }
        assert!(h
            .records
            .store()
            .results_for_day("tetris", DAY)
            .unwrap()
            .is_empty());
        assert!(h
            .records
            .store()
            .results_for_day("snake", DAY)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_suspended_game_is_refused_with_its_own_valid_key() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Suspended);
        let error = h
            .records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 1))
            .unwrap_err();
        assert!(matches!(error, GamesError::GameSuspended));
        assert_eq!(error.status(), 403);
    }

    #[test]
    fn the_same_result_id_with_different_contents_is_a_conflict() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 900))
            .unwrap();
        let error = h
            .records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 901))
            .unwrap_err();
        assert!(matches!(error, GamesError::ResultConflict));
        assert_eq!(error.status(), 409);
        assert_eq!(
            h.records.store().results_for_day("tetris", DAY).unwrap()[0].score,
            900
        );
    }

    #[test]
    fn a_games_daily_limit_is_a_429_with_a_code_a_developer_can_act_on() {
        let h = harness(Options {
            caps: RegistryCaps {
                results_per_game_per_day: 2,
                results_per_player_per_game_per_day: 1,
                ..RegistryCaps::standard()
            },
            ..Options::default()
        });
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 1))
            .unwrap();
        let player = h
            .records
            .submit_registry_result("tetris", &key, result(A, "match-0002", 1))
            .unwrap_err();
        assert!(matches!(player, GamesError::PlayerLimit));
        h.records
            .submit_registry_result("tetris", &key, result(B, "match-0003", 1))
            .unwrap();
        let full = h
            .records
            .submit_registry_result(
                "tetris",
                &key,
                result("1".repeat(64).as_str(), "match-0004", 1),
            )
            .unwrap_err();
        assert!(matches!(full, GamesError::GameLimit));
        assert_eq!((player.status(), full.status()), (429, 429));
    }

    #[test]
    fn a_malformed_result_is_refused_before_anything_is_stored() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        let bad_hash = RegistryResultRequest {
            data_hash: Some("not-hex".into()),
            ..result(A, "match-0001", 1)
        };
        for request in [result("nope", "match-0001", 1), result(A, "x", 1), bad_hash] {
            assert!(matches!(
                h.records.submit_registry_result("tetris", &key, request),
                Err(GamesError::Invalid(_))
            ));
        }
        let good_hash = RegistryResultRequest {
            data_hash: Some("ab".repeat(32)),
            ..result(A, "match-0002", 1)
        };
        h.records
            .submit_registry_result("tetris", &key, good_hash)
            .unwrap();
        assert_eq!(
            h.records.store().results_for_day("tetris", DAY).unwrap()[0].data_hash,
            [0xAB; 32]
        );
        assert_eq!(
            h.records
                .store()
                .results_for_day("tetris", DAY)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_player_address_is_accepted_in_any_case_and_stored_lowercase() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .submit_registry_result("tetris", &key, result(&A.to_uppercase(), "match-0001", 1))
            .unwrap();
        assert_eq!(
            h.records.store().results_for_day("tetris", DAY).unwrap()[0].player,
            A
        );
    }

    #[test]
    fn the_platform_switch_stops_third_party_results_too() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .store()
            .set_state(Some(false), None, Some("maintenance"), 1)
            .unwrap();
        assert!(matches!(
            h.records
                .submit_registry_result("tetris", &key, result(A, "match-0001", 1)),
            Err(GamesError::Disabled)
        ));
    }

    #[test]
    fn only_registered_and_listed_games_have_a_public_page_and_only_listed_ones_are_in_the_directory(
    ) {
        let h = harness(Options::default());
        game(&h, "scratch", GameStatus::Sandbox);
        game(&h, "tetris", GameStatus::Registered);
        game(&h, "snake", GameStatus::Listed);
        game(&h, "banned", GameStatus::Suspended);
        assert_eq!(
            h.records.registry_game("tetris").unwrap().status,
            "registered"
        );
        assert_eq!(h.records.registry_game("snake").unwrap().status, "listed");
        for hidden in ["scratch", "banned", "never-existed"] {
            assert!(
                matches!(
                    h.records.registry_game(hidden),
                    Err(GamesError::NotFound(_))
                ),
                "{hidden}"
            );
        }
        let listed: Vec<String> = h
            .records
            .registry_listed()
            .unwrap()
            .into_iter()
            .map(|g| g.id)
            .collect();
        assert_eq!(listed, vec!["snake".to_string()]);
    }

    #[tokio::test]
    async fn the_daily_run_closes_the_registry_day_and_anchors_its_root_exactly_once() {
        let h = harness(Options::default());
        let tetris = game(&h, "tetris", GameStatus::Registered);
        let snake = game(&h, "snake", GameStatus::Listed);
        let scratch = game(&h, "scratch", GameStatus::Sandbox);
        h.records
            .submit_registry_result("tetris", &tetris, result(A, "match-0001", 10))
            .unwrap();
        h.records
            .submit_registry_result("snake", &snake, result(B, "match-0001", 20))
            .unwrap();
        h.records
            .submit_registry_result("scratch", &scratch, result(A, "match-0001", 30))
            .unwrap();

        // Today is never closed early.
        h.records.run_daily().await.unwrap();
        assert!(registry_chain_calls(&h).is_empty());
        assert!(h.records.registry_day_view(DAY).is_err());

        h.next_day();
        let report = h.records.run_daily().await.unwrap();
        assert!(report
            .anchored
            .contains(&("registry".to_string(), DAY.to_string())));
        let view = h.records.registry_day_view(DAY).unwrap();
        assert_eq!(
            view.games.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            vec!["snake", "tetris"],
            "the sandbox is not anchored"
        );
        let calls = registry_chain_calls(&h);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            hex::encode(calls[0]),
            view.day_root,
            "the anchored root is the one the day publishes"
        );
        assert_eq!(view.state, "anchored");
        assert_eq!(view.chain_id, Some(REGISTRY_CHAIN_ID));

        // Straight away again: pending, so no second anchor. Then it lands, and the day is confirmed.
        h.records.run_daily().await.unwrap();
        assert_eq!(registry_chain_calls(&h).len(), 1);
        h.chain.add_anchor(REGISTRY_CHAIN_ID, 1, calls[0], h.service.now());
        let report = h.records.run_daily().await.unwrap();
        assert!(report
            .confirmed
            .contains(&("registry".to_string(), DAY.to_string())));
        assert_eq!(h.records.registry_day_view(DAY).unwrap().state, "confirmed");
        assert_eq!(registry_chain_calls(&h).len(), 1);
    }

    #[tokio::test]
    async fn a_registry_anchor_of_unclear_outcome_is_checked_on_the_ledger_before_it_is_repeated() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 10))
            .unwrap();
        h.next_day();
        h.records_anchorer.fail.store(true, Ordering::SeqCst);
        h.records.run_daily().await.unwrap();
        assert_eq!(registry_chain_calls(&h).len(), 1);
        let day = h.records.store().registry_day(DAY).unwrap().unwrap();
        assert!(day.last_error.is_some());
        // Too soon to retry; and when it is time, the root is found on the ledger and nothing is resubmitted.
        h.advance(60);
        h.records.run_daily().await.unwrap();
        assert_eq!(
            registry_chain_calls(&h).len(),
            1,
            "no retry before the interval"
        );
        h.chain.add_anchor(REGISTRY_CHAIN_ID, 1, day.day_root, h.service.now());
        h.advance(601);
        h.records.run_daily().await.unwrap();
        assert_eq!(h.records.registry_day_view(DAY).unwrap().state, "confirmed");
        assert_eq!(registry_chain_calls(&h).len(), 1);
    }

    #[tokio::test]
    async fn without_a_registry_chain_the_day_is_closed_for_proofs_but_never_anchored() {
        let h = harness(Options {
            registry_chain: None,
            ..Options::default()
        });
        let key = game(&h, "tetris", GameStatus::Registered);
        h.records
            .submit_registry_result("tetris", &key, result(A, "match-0001", 10))
            .unwrap();
        h.next_day();
        h.records.run_daily().await.unwrap();
        assert!(h
            .anchorer
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(chain, _)| *chain != 3));
        let view = h.records.registry_day_view(DAY).unwrap();
        assert_eq!((view.state.as_str(), view.chain_id), ("closed", None));
        assert!(
            h.records.registry_proof("tetris", DAY).is_ok(),
            "a developer can still be given a proof"
        );
    }

    #[tokio::test]
    async fn a_day_with_no_registered_results_is_never_anchored() {
        let h = harness(Options::default());
        let key = game(&h, "scratch", GameStatus::Sandbox);
        h.records
            .submit_registry_result("scratch", &key, result(A, "match-0001", 1))
            .unwrap();
        h.next_day();
        h.records.run_daily().await.unwrap();
        assert!(
            registry_chain_calls(&h).is_empty(),
            "nothing registered happened, so there is nothing to anchor"
        );
    }

    #[tokio::test]
    async fn a_developer_can_verify_their_proof_against_the_published_root() {
        let h = harness(Options::default());
        for id in ["tetris", "snake", "puzzle-x"] {
            let key = game(&h, id, GameStatus::Registered);
            h.records
                .submit_registry_result(id, &key, result(A, "match-0001", 5))
                .unwrap();
        }
        h.next_day();
        h.records.close_registry_day(DAY).await.unwrap();
        for id in ["tetris", "snake", "puzzle-x"] {
            let proof = h.records.registry_proof(id, DAY).unwrap();
            let leaf: [u8; 32] = hex::decode(&proof.leaf).unwrap().try_into().unwrap();
            let root: [u8; 32] = hex::decode(&proof.merkle_root).unwrap().try_into().unwrap();
            let steps: Vec<registry::ProofStep> = proof
                .steps
                .iter()
                .map(|s| registry::ProofStep {
                    sibling: hex::decode(&s.sibling).unwrap().try_into().unwrap(),
                    sibling_on_left: s.side == "left",
                })
                .collect();
            assert!(registry::verify_inclusion(&leaf, &steps, &root), "{id}");
            // And the developer recomputes their own game root from their own records.
            let mine = h.records.store().results_for_day(id, DAY).unwrap();
            assert_eq!(
                hex::encode(registry::game_results_root(id, &mine)),
                proof.game_root
            );
            assert_eq!(
                hex::encode(registry::registry_day_root(DAY, &root)),
                proof.day_root
            );
        }
        assert!(matches!(
            h.records.registry_proof("scratch-none", DAY),
            Err(GamesError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn only_a_finished_day_can_be_closed_and_a_bad_date_is_refused() {
        let h = harness(Options::default());
        assert!(matches!(
            h.records.close_registry_day(DAY).await,
            Err(GamesError::Invalid(_))
        ));
        assert!(matches!(
            h.records.close_registry_day("garbage").await,
            Err(GamesError::Invalid(_))
        ));
        assert!(matches!(
            h.records.registry_day_view("garbage"),
            Err(GamesError::Invalid(_))
        ));
    }
}

// --- HTTP ------------------------------------------------------------------------------------

mod http {
    use super::*;
    use crate::games::api;
    use actix_web::{
        http::StatusCode,
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::{json, Value};

    macro_rules! app {
        ($harness:expr) => {
            init_service(
                App::new()
                    .app_data(web::Data::from($harness.service.clone()))
                    .configure(api::configure_routes)
                    .app_data(web::Data::from($harness.records.clone()))
                    .configure(crate::games::records_api::configure_records_routes),
            )
            .await
        };
    }

    async fn json_of(response: actix_web::dev::ServiceResponse) -> (StatusCode, Value) {
        let status = response.status();
        assert_eq!(
            response
                .headers()
                .get("Cache-Control")
                .and_then(|v| v.to_str().ok()),
            Some("no-store"),
            "every response must be uncacheable"
        );
        (status, read_body_json(response).await)
    }

    #[actix_web::test]
    async fn the_status_route_publishes_the_rules() {
        let h = harness(Options::default());
        let app = app!(h);
        let (status, body) = json_of(
            call_service(
                &app,
                TestRequest::get().uri("/api/games/status").to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["success"], true);
        assert_eq!(body["withdraw_min_units"], 100);
        assert_eq!(body["games"][0]["game"], "grid");
        assert_eq!(body["games"][0]["verified"], true);
        assert_eq!(body["games"][1]["verified"], false);
    }

    #[actix_web::test]
    async fn the_day_route_lets_a_phone_check_its_own_copy_of_the_seed() {
        let h = harness(Options::default());
        let midnight = date_start(DAY).unwrap();
        h.chain.add_anchor(1, 4, [0x44; 32], midnight - 100);
        let app = app!(h);
        let (status, body) = json_of(
            call_service(
                &app,
                TestRequest::get().uri("/api/games/grid/day").to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["date"], DAY);
        assert_eq!(body["previous_anchor"]["sequence"], 4);
        assert_eq!(body["previous_anchor"]["root"], "44".repeat(32));
        let seed = ultragrid::daily_seed(1, DAY, &[0x44; 32]).unwrap();
        assert_eq!(body["seed"], hex::encode(seed));
        assert_eq!(
            body["commitment"],
            hex::encode(ultragrid::set_commitment(&ultragrid::daily_set(&seed)))
        );
        assert_eq!(body["closes_at"], midnight + 86_400);
        // Yesterday is published too; anything else is refused, and writes nothing.
        let (status, _) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri("/api/games/grid/day?date=2026-11-15")
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200);
        let rows_before = h
            .service
            .store()
            .day_row("2020-01-01", "grid")
            .unwrap()
            .is_none();
        for refused in ["2026-11-17", "2020-01-01", "2026-11-14", "garbage"] {
            let (status, body) = json_of(
                call_service(
                    &app,
                    TestRequest::get()
                        .uri(&format!("/api/games/grid/day?date={refused}"))
                        .to_request(),
                )
                .await,
            )
            .await;
            assert_eq!(status, 400, "{refused}");
            assert_eq!(body["code"], "invalid_request");
        }
        assert!(
            rows_before
                && h.service
                    .store()
                    .day_row("2020-01-01", "grid")
                    .unwrap()
                    .is_none(),
            "a refused date leaves no row behind"
        );
    }

    #[actix_web::test]
    async fn a_whole_game_over_http_answer_reward_withdraw_paid() {
        let h = harness(Options::default());
        let app = app!(h);

        // Solve a Grid level exactly as the phone would: JSON in, reward out.
        let answer = h.grid_answer(A, 0).await;
        let body = json!({
            "address": A, "date": DAY, "level": 0,
            "answer": answer.answer.iter().map(|t| json!({"cell": t.cell, "turns": t.turns})).collect::<Vec<_>>(),
        });
        let (status, win) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/grid/answers")
                    .set_json(&body)
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{win}");
        assert_eq!(
            (
                win["outcome"].as_str(),
                win["reward_units"].as_u64(),
                win["verified"].as_bool()
            ),
            (Some("rewarded"), Some(10), Some(true))
        );

        // A chess win takes the balance over the withdrawal minimum: 10 + 100.
        let chess = json!({"address": A, "game_id": GAME_ID_1, "mode": "anti", "result": "player_win", "plies": 31});
        let (status, win) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/chess/results")
                    .set_json(&chess)
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{win}");
        assert_eq!(win["verified"], false);
        assert_eq!(win["balance_units"], 110);

        let (_, wallet) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/wallet/{A}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(
            (
                wallet["available_units"].as_u64(),
                wallet["can_withdraw"].as_bool()
            ),
            (Some(110), Some(true))
        );
        assert_eq!(wallet["available_ultra"], "0.000110");
        assert_eq!(wallet["today"].as_array().unwrap().len(), 2);

        let (status, payout) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/withdraw")
                    .set_json(json!({"address": A}))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{payout}");
        assert_eq!(
            (payout["amount_units"].as_u64(), payout["state"].as_str()),
            (Some(110), Some("queued"))
        );
        let id = payout["id"].as_str().unwrap().to_string();

        h.drain_payouts(1).await;
        let hash = h
            .service
            .store()
            .payout(&id)
            .unwrap()
            .unwrap()
            .tx_hash
            .unwrap();
        h.node.set_status(&hash, "confirmed");
        h.drain_payouts(1).await;
        let (status, done) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/payouts/{id}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(
            (done["state"].as_str(), done["transaction_hash"].as_str()),
            (Some("confirmed"), Some(hash.as_str()))
        );
    }

    #[actix_web::test]
    async fn errors_are_json_with_a_code_and_the_numbers_a_client_needs() {
        let h = harness(Options::default());
        let app = app!(h);

        let mut wrong_day = h.grid_answer(A, 0).await;
        wrong_day.date = "2026-11-15".into();
        let body = json!({"address": A, "date": wrong_day.date, "level": 0, "answer": []});
        let (status, error) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/grid/answers")
                    .set_json(&body)
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(
            (
                error["code"].as_str(),
                error["server_date"].as_str(),
                error["success"].as_bool()
            ),
            (Some("date_mismatch"), Some(DAY), Some(false))
        );

        let (status, error) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/withdraw")
                    .set_json(json!({"address": A}))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 429);
        assert_eq!(
            (
                error["code"].as_str(),
                error["available_units"].as_u64(),
                error["min_withdraw_units"].as_u64()
            ),
            (Some("below_minimum"), Some(0), Some(100))
        );
    }

    #[actix_web::test]
    async fn unknown_fields_and_malformed_bodies_are_refused_as_json() {
        let h = harness(Options::default());
        let app = app!(h);
        // A client cannot slip an extra field (an amount, a reward) past the parser.
        let sneaky = json!({"address": A, "game_id": GAME_ID_1, "mode": "classic", "result": "player_win", "plies": 40, "reward_units": 1000});
        let (status, error) = json_of(
            call_service(
                &app,
                TestRequest::post()
                    .uri("/api/games/chess/results")
                    .set_json(&sneaky)
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(error["code"], "invalid_request");
        assert!(h
            .service
            .store()
            .wins_for_day("chess", DAY)
            .unwrap()
            .is_empty());

        let request = TestRequest::post()
            .uri("/api/games/grid/answers")
            .insert_header(("Content-Type", "application/json"))
            .set_payload("{not json")
            .to_request();
        let (status, error) = json_of(call_service(&app, request).await).await;
        assert_eq!(
            (status.as_u16(), error["code"].as_str()),
            (400, Some("invalid_request"))
        );

        let huge = "x".repeat(20 * 1024);
        let request = TestRequest::post()
            .uri("/api/games/withdraw")
            .insert_header(("Content-Type", "application/json"))
            .set_payload(format!("{{\"address\":\"{huge}\"}}"))
            .to_request();
        let (status, _) = json_of(call_service(&app, request).await).await;
        assert_eq!(status, 400, "an oversized body is refused");
    }

    #[actix_web::test]
    async fn a_payout_id_must_be_well_formed_and_known() {
        let h = harness(Options::default());
        let app = app!(h);
        let (status, _) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri("/api/games/payouts/not-an-id")
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 400);
        let (status, body) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/payouts/{}", "ab".repeat(16)))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(
            (status.as_u16(), body["code"].as_str()),
            (404, Some("not_found"))
        );
    }

    #[actix_web::test]
    async fn the_wallet_route_rejects_a_malformed_address() {
        let h = harness(Options::default());
        let app = app!(h);
        let (status, body) = json_of(
            call_service(
                &app,
                TestRequest::get().uri("/api/games/wallet/zzz").to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(
            (status.as_u16(), body["code"].as_str()),
            (400, Some("invalid_request"))
        );
    }

    // --- third-party games over HTTP --------------------------------------------------------------

    fn register(h: &Harness, id: &str, status: crate::games::registry::GameStatus) -> String {
        let (_, key) = h
            .records
            .store()
            .register_game(id, id, A, h.service.now())
            .unwrap();
        h.records
            .store()
            .set_game_status(id, status, h.service.now())
            .unwrap();
        key
    }

    #[actix_web::test]
    async fn a_game_reports_a_result_with_a_bearer_key() {
        use crate::games::registry::GameStatus;
        let h = harness(Options::default());
        let key = register(&h, "tetris", GameStatus::Registered);
        let app = app!(h);
        let body = json!({"player": A, "result_id": "match-0001", "score": 1200});
        let request = |auth: Option<String>| {
            let mut r = TestRequest::post()
                .uri("/api/games/g/tetris/results")
                .set_json(&body);
            if let Some(value) = auth {
                r = r.insert_header(("Authorization", value));
            }
            r.to_request()
        };
        let (status, ok) =
            json_of(call_service(&app, request(Some(format!("Bearer {key}")))).await).await;
        assert_eq!(status, 200, "{ok}");
        assert_eq!(
            (
                ok["game_id"].as_str(),
                ok["duplicate"].as_bool(),
                ok["anchored"].as_bool(),
                ok["status"].as_str()
            ),
            (Some("tetris"), Some(false), Some(true), Some("registered"))
        );

        // The scheme is case-insensitive, and a replay is a 200 that says so.
        let (status, again) =
            json_of(call_service(&app, request(Some(format!("bearer {key}")))).await).await;
        assert_eq!(
            (status.as_u16(), again["duplicate"].as_bool()),
            (200, Some(true))
        );

        // No header, a wrong scheme and a wrong key are one refusal, 401, with no hint which.
        for bad in [
            None,
            Some(format!("Basic {key}")),
            Some("Bearer ugk_wrong".to_string()),
            Some(key.clone()),
        ] {
            let (status, error) = json_of(call_service(&app, request(bad)).await).await;
            assert_eq!(
                (status.as_u16(), error["code"].as_str()),
                (401, Some("invalid_key"))
            );
        }
        assert_eq!(
            h.records
                .store()
                .results_for_day("tetris", DAY)
                .unwrap()
                .len(),
            1
        );
    }

    #[actix_web::test]
    async fn the_key_of_one_game_opens_no_other_game_over_http() {
        use crate::games::registry::GameStatus;
        let h = harness(Options::default());
        let tetris = register(&h, "tetris", GameStatus::Registered);
        register(&h, "snake", GameStatus::Registered);
        let app = app!(h);
        let body = json!({"player": A, "result_id": "match-0001"});
        let request = TestRequest::post()
            .uri("/api/games/g/snake/results")
            .insert_header(("Authorization", format!("Bearer {tetris}")))
            .set_json(&body)
            .to_request();
        let (status, error) = json_of(call_service(&app, request).await).await;
        assert_eq!(
            (status.as_u16(), error["code"].as_str()),
            (401, Some("invalid_key"))
        );
        assert!(h
            .records
            .store()
            .results_for_day("snake", DAY)
            .unwrap()
            .is_empty());
    }

    #[actix_web::test]
    async fn a_result_cannot_smuggle_extra_fields_past_the_parser() {
        use crate::games::registry::GameStatus;
        let h = harness(Options::default());
        let key = register(&h, "tetris", GameStatus::Registered);
        let app = app!(h);
        let sneaky = json!({"player": A, "result_id": "match-0001", "reward_units": 1000, "game_id": "snake"});
        let request = TestRequest::post()
            .uri("/api/games/g/tetris/results")
            .insert_header(("Authorization", format!("Bearer {key}")))
            .set_json(&sneaky)
            .to_request();
        let (status, error) = json_of(call_service(&app, request).await).await;
        assert_eq!(
            (status.as_u16(), error["code"].as_str()),
            (400, Some("invalid_request"))
        );
        assert!(h
            .records
            .store()
            .results_for_day("tetris", DAY)
            .unwrap()
            .is_empty());
    }

    #[actix_web::test]
    async fn the_public_pages_show_only_what_a_stranger_should_see() {
        use crate::games::registry::GameStatus;
        let h = harness(Options::default());
        register(&h, "scratch", GameStatus::Sandbox);
        register(&h, "tetris", GameStatus::Registered);
        register(&h, "snake", GameStatus::Listed);
        let app = app!(h);
        let get = |uri: &str| TestRequest::get().uri(uri).to_request();

        let (status, page) = json_of(call_service(&app, get("/api/games/g/tetris")).await).await;
        assert_eq!(
            (status.as_u16(), page["status"].as_str()),
            (200, Some("registered"))
        );
        assert!(
            page.get("key_hash").is_none() && !page.to_string().contains("ugk_"),
            "no key material is ever shown"
        );
        for hidden in ["scratch", "never-existed"] {
            let (status, error) =
                json_of(call_service(&app, get(&format!("/api/games/g/{hidden}"))).await).await;
            assert_eq!(
                (status.as_u16(), error["code"].as_str()),
                (404, Some("not_found")),
                "{hidden}"
            );
        }
        let (_, directory) = json_of(call_service(&app, get("/api/games/g")).await).await;
        let ids: Vec<&str> = directory["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["snake"], "only listed games are in the directory");
    }

    #[actix_web::test]
    async fn the_registry_day_and_a_proof_are_public_once_the_day_is_closed() {
        use crate::games::registry::GameStatus;
        let h = harness(Options::default());
        let key = register(&h, "tetris", GameStatus::Registered);
        let app = app!(h);
        let body = json!({"player": A, "result_id": "match-0001", "score": 5});
        let submit = TestRequest::post()
            .uri("/api/games/g/tetris/results")
            .insert_header(("Authorization", format!("Bearer {key}")))
            .set_json(&body)
            .to_request();
        assert_eq!(call_service(&app, submit).await.status(), 200);

        // Before the day is closed there is nothing to show, and that is a 404, not an empty promise.
        let (status, _) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/registry/days/{DAY}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 404);

        h.next_day();
        h.records.close_registry_day(DAY).await.unwrap();
        let (status, day) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/registry/days/{DAY}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{day}");
        assert_eq!(day["games"][0]["id"], "tetris");
        assert_eq!(day["chain_id"], REGISTRY_CHAIN_ID);
        let (status, proof) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/g/tetris/proof/{DAY}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{proof}");
        assert_eq!(proof["merkle_root"], day["merkle_root"]);
        assert_eq!(
            proof["steps"].as_array().unwrap().len(),
            0,
            "a one-game day is its own root"
        );
        let (status, missing) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri(&format!("/api/games/g/snake/proof/{DAY}"))
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(
            (status.as_u16(), missing["code"].as_str()),
            (404, Some("not_found"))
        );
        let (status, bad) = json_of(
            call_service(
                &app,
                TestRequest::get()
                    .uri("/api/games/registry/days/garbage")
                    .to_request(),
            )
            .await,
        )
        .await;
        assert_eq!(
            (status.as_u16(), bad["code"].as_str()),
            (400, Some("invalid_request"))
        );
    }
}

mod self_service {
    use super::*;
    use crate::games::{
        ownership::{self, ACTION_REGISTER, ACTION_ROTATE_KEY},
        registry::GameStatus,
        records::{RegistryRegisterRequest, RegistryResultRequest, RegistryRotateRequest},
    };

    fn register_request(
        pair: &QuantumKeyPair,
        id: &str,
        name: &str,
        timestamp: u64,
    ) -> RegistryRegisterRequest {
        let digest = ownership::proof_digest(ACTION_REGISTER, id, name, timestamp);
        RegistryRegisterRequest {
            id: id.into(),
            name: name.into(),
            public_key: hex::encode(&pair.public_key),
            timestamp,
            signature: hex::encode(pair.sign(&digest)),
        }
    }

    fn rotate_request(pair: &QuantumKeyPair, id: &str, timestamp: u64) -> RegistryRotateRequest {
        let digest = ownership::proof_digest(ACTION_ROTATE_KEY, id, "", timestamp);
        RegistryRotateRequest {
            public_key: hex::encode(&pair.public_key),
            timestamp,
            signature: hex::encode(pair.sign(&digest)),
        }
    }

    fn a_result(id: &str) -> RegistryResultRequest {
        RegistryResultRequest {
            player: A.into(),
            result_id: id.into(),
            score: 1,
            data_hash: None,
        }
    }

    #[actix_web::test]
    async fn an_owner_registers_a_sandbox_game_and_the_key_it_gets_works() {
        let h = harness(Options::default());
        let pair = QuantumKeyPair::generate();
        let created = h
            .records
            .self_register(register_request(&pair, "tetris", "Tetris", h.service.now()))
            .await
            .unwrap();
        assert_eq!(created.game.owner, pair.address(), "the owner is derived, not claimed");
        assert_eq!(created.game.status, GameStatus::Sandbox.code());
        let reply = h
            .records
            .submit_registry_result("tetris", &created.api_key, a_result("match-0001"))
            .unwrap();
        assert_eq!((reply.status, reply.anchored), ("sandbox", false));
    }

    #[actix_web::test]
    async fn a_proof_for_other_terms_or_an_unknown_address_creates_nothing() {
        let h = harness(Options::default());
        let pair = QuantumKeyPair::generate();
        let now = h.service.now();

        let mut wrong_name = register_request(&pair, "tetris", "Tetris", now);
        wrong_name.name = "Tetris Pro".into();
        let mut wrong_id = register_request(&pair, "tetris", "Tetris", now);
        wrong_id.id = "snake".into();
        for request in [
            wrong_name,
            wrong_id,
            register_request(&pair, "tetris", "Tetris", now - 301),
            register_request(&pair, "tetris", "Tetris", now + 301),
        ] {
            let error = h.records.self_register(request).await.unwrap_err();
            assert_eq!(error.code(), "invalid_key");
        }

        // A key whose address the chain has never seen cannot register, however valid the proof.
        h.chain.everyone_active.store(false, Ordering::SeqCst);
        let error = h
            .records
            .self_register(register_request(&pair, "tetris", "Tetris", now))
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request");
        h.chain.active.lock().unwrap().insert(pair.address());
        h.records
            .self_register(register_request(&pair, "tetris", "Tetris", now))
            .await
            .unwrap();
        assert_eq!(h.records.store().games().unwrap().len(), 1);
    }

    #[actix_web::test]
    async fn taken_and_reserved_ids_are_refused() {
        let h = harness(Options::default());
        let (one, two) = (QuantumKeyPair::generate(), QuantumKeyPair::generate());
        let now = h.service.now();
        h.records
            .self_register(register_request(&one, "tetris", "Tetris", now))
            .await
            .unwrap();
        for (pair, id) in [(&two, "tetris"), (&two, "grid"), (&two, "Bad_Id")] {
            let error = h
                .records
                .self_register(register_request(pair, id, "Mine", now))
                .await
                .unwrap_err();
            assert_eq!(error.code(), "invalid_request", "{id}");
        }
        assert_eq!(h.records.store().games().unwrap().len(), 1);
    }

    #[actix_web::test]
    async fn one_owner_and_one_day_are_both_bounded() {
        let h = harness(Options {
            caps: RegistryCaps {
                games_per_owner: 2,
                self_registrations_per_day: 3,
                ..RegistryCaps::standard()
            },
            ..Options::default()
        });
        let now = h.service.now();
        let owner = QuantumKeyPair::generate();
        for id in ["game-one", "game-two"] {
            h.records
                .self_register(register_request(&owner, id, id, now))
                .await
                .unwrap();
        }
        let error = h
            .records
            .self_register(register_request(&owner, "game-three", "Three", now))
            .await
            .unwrap_err();
        assert_eq!((error.code(), error.status()), ("owner_game_limit", 429));

        // Someone else may still register the third of the day; the fourth is turned away.
        let other = QuantumKeyPair::generate();
        h.records
            .self_register(register_request(&other, "game-four", "Four", now))
            .await
            .unwrap();
        let error = h
            .records
            .self_register(register_request(&QuantumKeyPair::generate(), "game-five", "Five", now))
            .await
            .unwrap_err();
        assert_eq!((error.code(), error.status()), ("registration_limit", 429));

        // The next day the allowance is back.
        h.advance(86_400);
        h.records
            .self_register(register_request(
                &QuantumKeyPair::generate(),
                "game-five",
                "Five",
                h.service.now(),
            ))
            .await
            .unwrap();
    }

    #[actix_web::test]
    async fn the_owner_rotates_a_lost_key_and_nobody_else_can() {
        let h = harness(Options::default());
        let owner = QuantumKeyPair::generate();
        let created = h
            .records
            .self_register(register_request(&owner, "tetris", "Tetris", h.service.now()))
            .await
            .unwrap();

        // A stranger's valid signature, an unknown game, and a wrong-action proof all read the same.
        let stranger = QuantumKeyPair::generate();
        for (id, request) in [
            ("tetris", rotate_request(&stranger, "tetris", h.service.now())),
            ("nothing", rotate_request(&owner, "nothing", h.service.now())),
        ] {
            let error = h.records.self_rotate_key(id, request).unwrap_err();
            assert_eq!(error.code(), "invalid_key", "{id}");
        }
        let mut wrong_action = rotate_request(&owner, "tetris", h.service.now());
        wrong_action.signature = register_request(&owner, "tetris", "", h.service.now()).signature;
        assert_eq!(
            h.records.self_rotate_key("tetris", wrong_action).unwrap_err().code(),
            "invalid_key"
        );

        h.advance(10);
        let request = rotate_request(&owner, "tetris", h.service.now());
        let replay = rotate_request(&owner, "tetris", h.service.now());
        let rotated = h.records.self_rotate_key("tetris", request).unwrap();
        assert_ne!(rotated.api_key, created.api_key);
        assert_eq!(
            h.records
                .submit_registry_result("tetris", &created.api_key, a_result("match-0001"))
                .unwrap_err()
                .code(),
            "invalid_key",
            "the old key is dead"
        );
        h.records
            .submit_registry_result("tetris", &rotated.api_key, a_result("match-0001"))
            .unwrap();

        // The same signed request, captured and sent again, must not replace the new key.
        assert_eq!(
            h.records.self_rotate_key("tetris", replay).unwrap_err().code(),
            "invalid_key"
        );
        h.records
            .submit_registry_result("tetris", &rotated.api_key, a_result("match-0002"))
            .unwrap();
    }

    #[actix_web::test]
    async fn the_http_routes_carry_the_proof_and_return_the_key_once() {
        use actix_web::{
            http::StatusCode,
            test::{call_service, init_service, read_body_json, TestRequest},
            web, App,
        };
        use serde_json::{json, Value};

        let h = harness(Options::default());
        let app = init_service(
            App::new()
                .app_data(web::Data::from(h.service.clone()))
                .configure(crate::games::api::configure_routes)
                .app_data(web::Data::from(h.records.clone()))
                .configure(crate::games::records_api::configure_records_routes),
        )
        .await;
        let owner = QuantumKeyPair::generate();
        let request = register_request(&owner, "tetris", "Tetris", h.service.now());
        let body = json!({
            "id": request.id, "name": request.name, "public_key": request.public_key,
            "timestamp": request.timestamp, "signature": request.signature,
        });
        assert!(body.to_string().len() > 14_000, "a real proof is large");
        let response = call_service(
            &app,
            TestRequest::post().uri("/api/games/g").set_json(&body).to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let created: Value = read_body_json(response).await;
        assert_eq!(created["game"]["owner"], json!(owner.address()));
        assert_eq!(created["game"]["status"], json!("sandbox"));
        assert!(created["api_key"].as_str().unwrap().len() >= 32);

        // Unknown fields are refused: nobody can slip an `owner` in beside the proof.
        let mut with_owner = body.clone();
        with_owner["owner"] = json!(A);
        with_owner["id"] = json!("other-game");
        let response = call_service(
            &app,
            TestRequest::post().uri("/api/games/g").set_json(&with_owner).to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // A forged proof is a 401.
        let mut forged = body.clone();
        forged["id"] = json!("forged-game");
        let response = call_service(
            &app,
            TestRequest::post().uri("/api/games/g").set_json(&forged).to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        h.advance(5);
        let rotate = rotate_request(&owner, "tetris", h.service.now());
        let response = call_service(
            &app,
            TestRequest::post()
                .uri("/api/games/g/tetris/rotate-key")
                .set_json(json!({
                    "public_key": rotate.public_key, "timestamp": rotate.timestamp,
                    "signature": rotate.signature,
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let rotated: Value = read_body_json(response).await;
        assert_ne!(rotated["api_key"], created["api_key"]);
    }
}

/// The generic `/api/records` and `/api/apps` routes: adapters over the registry the `/api/games/g`
/// routes already serve. These tests pin that they are the same registry under other names.
mod generic_api {
    use super::*;
    use crate::games::{
        ownership::{self, ACTION_REGISTER, ACTION_ROTATE_KEY},
        registry::GameStatus,
    };
    use actix_web::{
        dev::ServiceResponse,
        http::StatusCode,
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::{json, Value};

    macro_rules! app {
        ($harness:expr) => {
            init_service(
                App::new()
                    .app_data(web::Data::from($harness.service.clone()))
                    .configure(crate::games::api::configure_routes)
                .app_data(web::Data::from($harness.records.clone()))
                .configure(crate::games::records_api::configure_records_routes),
            )
            .await
        };
    }

    async fn json_of(response: ServiceResponse) -> (StatusCode, Value) {
        let status = response.status();
        (status, read_body_json(response).await)
    }

    macro_rules! post {
        ($app:expr, $uri:expr, $key:expr, $body:expr $(,)?) => {
            async {
                let mut request = TestRequest::post().uri($uri).set_json($body);
                let key: Option<&str> = $key;
                if let Some(key) = key {
                    request = request.insert_header(("Authorization", format!("Bearer {key}")));
                }
                json_of(call_service(&$app, request.to_request()).await).await
            }
        };
    }

    macro_rules! get {
        ($app:expr, $uri:expr $(,)?) => {
            async { json_of(call_service(&$app, TestRequest::get().uri($uri).to_request()).await).await }
        };
    }

    fn register_body(pair: &QuantumKeyPair, id: &str, name: &str, ts: u64, generic: bool) -> Value {
        let digest = ownership::proof_digest(ACTION_REGISTER, id, name, ts);
        let id_field = if generic { "app_id" } else { "id" };
        json!({
            id_field: id, "name": name, "public_key": hex::encode(&pair.public_key),
            "timestamp": ts, "signature": hex::encode(pair.sign(&digest)),
        })
    }

    fn rotate_body(pair: &QuantumKeyPair, id: &str, ts: u64) -> Value {
        let digest = ownership::proof_digest(ACTION_ROTATE_KEY, id, "", ts);
        json!({
            "public_key": hex::encode(&pair.public_key), "timestamp": ts,
            "signature": hex::encode(pair.sign(&digest)),
        })
    }

    fn game(h: &Harness, id: &str) -> String {
        let (_, key) = h
            .records
            .store()
            .register_game(id, id, A, h.service.now())
            .unwrap();
        h.records
            .store()
            .set_game_status(id, GameStatus::Registered, h.service.now())
            .unwrap();
        key
    }

    fn without(mut value: Value, key: &str) -> Value {
        if let Value::Object(map) = &mut value {
            map.remove(key);
        }
        value
    }

    #[actix_web::test]
    async fn every_generic_route_works_and_answers_in_both_vocabularies() {
        let h = harness(Options::default());
        let app = app!(h);
        let owner = QuantumKeyPair::generate();

        // POST /api/apps: a signed registration, with the generic field name.
        let (status, created) = post!(app,
            "/api/apps",
            None,
            &register_body(&owner, "tetris", "Tetris", h.service.now(), true),
        )
        .await;
        assert_eq!(status, 200, "{created}");
        assert_eq!(created["game"]["id"], "tetris");
        assert_eq!(created["game"]["app_id"], "tetris");
        assert_eq!(created["game"]["owner"], json!(owner.address()));
        let key = created["api_key"].as_str().unwrap().to_string();
        h.records
            .store()
            .set_game_status("tetris", GameStatus::Registered, h.service.now())
            .unwrap();

        // POST /api/records
        let (status, record) = post!(app,
            "/api/records",
            Some(&key),
            &json!({"app_id": "tetris", "record_id": "match-0001", "subject": A, "value": 5,
                    "data_hash": "01".repeat(32)}),
        )
        .await;
        assert_eq!(status, 200, "{record}");
        assert_eq!(
            (record["app_id"].clone(), record["record_id"].clone()),
            (json!("tetris"), json!("match-0001"))
        );
        assert_eq!(record["game_id"], "tetris", "the old names are still there");
        assert_eq!(record["result_id"], "match-0001");
        assert_eq!((record["duplicate"].clone(), record["anchored"].clone()), (json!(false), json!(true)));

        // GET /api/apps/{id}
        let (status, page) = get!(app, "/api/apps/tetris").await;
        assert_eq!(status, 200, "{page}");
        assert_eq!((page["id"].clone(), page["app_id"].clone()), (json!("tetris"), json!("tetris")));
        assert_eq!(get!(app, "/api/apps/nothing").await.0, 404);

        // POST /api/apps/{id}/rotate-key (a later second, as the replay rule requires)
        h.advance(10);
        let (status, rotated) = post!(app,
            "/api/apps/tetris/rotate-key",
            None,
            &rotate_body(&owner, "tetris", h.service.now()),
        )
        .await;
        assert_eq!(status, 200, "{rotated}");
        assert_eq!(rotated["game"]["app_id"], "tetris");
        assert_ne!(rotated["api_key"].as_str().unwrap(), key);

        // GET /api/records/days/{date} and GET /api/apps/{id}/proof/{date}, once the day is closed.
        h.next_day();
        h.records.close_registry_day(DAY).await.unwrap();
        let (status, day) = get!(app, &format!("/api/records/days/{DAY}")).await;
        assert_eq!(status, 200, "{day}");
        assert_eq!(day["games"][0]["id"], "tetris");
        assert_eq!(day["games"][0]["app_id"], "tetris");
        let (status, proof) = get!(app, &format!("/api/apps/tetris/proof/{DAY}")).await;
        assert_eq!(status, 200, "{proof}");
        assert_eq!((proof["game_id"].clone(), proof["app_id"].clone()), (json!("tetris"), json!("tetris")));
        assert_eq!(proof["merkle_root"], day["merkle_root"]);
        assert_eq!(get!(app, &format!("/api/apps/snake/proof/{DAY}")).await.0, 404);
    }

    #[actix_web::test]
    async fn the_old_routes_are_unchanged_and_never_grow_the_generic_names() {
        let h = harness(Options::default());
        let app = app!(h);
        let key = game(&h, "tetris");
        let (status, record) = post!(app,
            "/api/games/g/tetris/results",
            Some(&key),
            &json!({"player": A, "result_id": "match-0001", "score": 5}),
        )
        .await;
        assert_eq!(status, 200);
        for generic in ["app_id", "record_id"] {
            assert!(record.get(generic).is_none(), "{generic} leaked into the old reply: {record}");
        }
        let (_, page) = get!(app, "/api/games/g/tetris").await;
        assert!(page.get("app_id").is_none() && page["id"] == "tetris", "{page}");
        // The old routes refuse the new vocabulary, exactly as they refuse any unknown field.
        let (status, error) = post!(app,
            "/api/games/g/tetris/results",
            Some(&key),
            &json!({"subject": A, "record_id": "match-0002", "value": 5}),
        )
        .await;
        assert_eq!((status, error["code"].clone()), (StatusCode::BAD_REQUEST, json!("invalid_request")));
    }

    #[actix_web::test]
    async fn a_record_is_the_same_record_whichever_route_wrote_it_and_proofs_are_equivalent() {
        let h = harness(Options::default());
        let app = app!(h);
        let key = game(&h, "tetris");
        let new = |id: &str, subject: &str, value: u64| {
            json!({"app_id": "tetris", "record_id": id, "subject": subject, "value": value})
        };
        let old = |id: &str, subject: &str, value: u64| {
            json!({"player": subject, "result_id": id, "score": value})
        };

        // Written by the new route, replayed by the old one; and the other way round.
        assert_eq!(post!(app, "/api/records", Some(&key), &new("match-0001", A, 5)).await.1["duplicate"], false);
        let (status, replay) =
            post!(app, "/api/games/g/tetris/results", Some(&key), &old("match-0001", A, 5)).await;
        assert_eq!((status, replay["duplicate"].clone()), (StatusCode::OK, json!(true)));
        assert_eq!(
            post!(app, "/api/games/g/tetris/results", Some(&key), &old("match-0002", B, 9)).await.1["duplicate"],
            false
        );
        let (status, replay) = post!(app, "/api/records", Some(&key), &new("match-0002", B, 9)).await;
        assert_eq!((status, replay["duplicate"].clone()), (StatusCode::OK, json!(true)));

        // The underlying store holds exactly two records, with the mapped fields.
        let rows = h.records.store().results_for_day("tetris", DAY).unwrap();
        assert_eq!(rows.len(), 2);
        let mut seen: Vec<(String, String, u64)> = rows
            .iter()
            .map(|r| (r.player.clone(), r.result_id.clone(), r.score))
            .collect();
        seen.sort();
        let mut expected = vec![
            (A.to_string(), "match-0001".to_string(), 5),
            (B.to_string(), "match-0002".to_string(), 9),
        ];
        expected.sort();
        assert_eq!(seen, expected);

        // Both routes serve the same day and the same proof; the generic reply only adds names.
        h.next_day();
        h.records.close_registry_day(DAY).await.unwrap();
        let (_, old_proof) = get!(app, &format!("/api/games/g/tetris/proof/{DAY}")).await;
        let (_, new_proof) = get!(app, &format!("/api/apps/tetris/proof/{DAY}")).await;
        assert_eq!(old_proof, without(new_proof, "app_id"));
        let (_, old_day) = get!(app, &format!("/api/games/registry/days/{DAY}")).await;
        let (_, mut new_day) = get!(app, &format!("/api/records/days/{DAY}")).await;
        for game in new_day["games"].as_array_mut().unwrap() {
            *game = without(game.clone(), "app_id");
        }
        assert_eq!(old_day, new_day);
        // And the proof verifies: the leaf climbs to the merkle root under the frozen tags.
        assert_eq!(old_proof["steps"].as_array().unwrap().len(), 0);
    }

    #[actix_web::test]
    async fn identical_records_stay_idempotent_and_conflicting_ones_stay_409_across_routes() {
        let h = harness(Options::default());
        let app = app!(h);
        let key = game(&h, "tetris");
        let body = json!({"app_id": "tetris", "record_id": "match-0001", "subject": A, "value": 5});
        let (s1, r1) = post!(app, "/api/records", Some(&key), &body).await;
        let (s2, r2) = post!(app, "/api/records", Some(&key), &body).await;
        assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
        assert_eq!((r1["duplicate"].clone(), r2["duplicate"].clone()), (json!(false), json!(true)));
        assert_eq!(h.records.store().results_for_day("tetris", DAY).unwrap().len(), 1);

        // Different contents under the same record id: refused, on the new route and on the old.
        let different = json!({"app_id": "tetris", "record_id": "match-0001", "subject": A, "value": 6});
        let (status, error) = post!(app, "/api/records", Some(&key), &different).await;
        assert_eq!((status, error["code"].clone()), (StatusCode::CONFLICT, json!("result_conflict")));
        let (status, error) = post!(app,
            "/api/games/g/tetris/results",
            Some(&key),
            &json!({"player": B, "result_id": "match-0001", "score": 5}),
        )
        .await;
        assert_eq!((status, error["code"].clone()), (StatusCode::CONFLICT, json!("result_conflict")));
        let rows = h.records.store().results_for_day("tetris", DAY).unwrap();
        assert_eq!((rows.len(), rows[0].score), (1, 5), "nothing was overwritten");
    }

    #[actix_web::test]
    async fn bad_keys_bad_signatures_and_unknown_fields_are_refused_on_every_generic_route() {
        let h = harness(Options::default());
        let app = app!(h);
        let key = game(&h, "tetris");
        let record = json!({"app_id": "tetris", "record_id": "match-0001", "subject": A});

        // Authentication: no key, a wrong key, a key for another app, and an unknown app read the same.
        let other = game(&h, "snake");
        for presented in [None, Some("ugk_wrong"), Some(other.as_str())] {
            let (status, error) = post!(app, "/api/records", presented, &record).await;
            assert_eq!((status, error["code"].clone()), (StatusCode::UNAUTHORIZED, json!("invalid_key")));
        }
        let unknown_app = json!({"app_id": "nothing", "record_id": "match-0001", "subject": A});
        assert_eq!(post!(app, "/api/records", Some(&key), &unknown_app).await.0, 401);
        assert_eq!(h.records.store().results_for_day("tetris", DAY).unwrap().len(), 0);

        // Validation is the registry's: a malformed subject or record id is a 400, nothing is stored.
        for body in [
            json!({"app_id": "tetris", "record_id": "match-0001", "subject": "not-hex"}),
            json!({"app_id": "tetris", "record_id": "x", "subject": A}),
        ] {
            let (status, _) = post!(app, "/api/records", Some(&key), &body).await;
            assert_eq!(status, 400, "{body}");
        }

        // The registry trims and lowercases a subject, exactly as it does a player on the old route, so
        // the same address in capitals is the same record, not a second one.
        let capitals = json!({"app_id": "tetris", "record_id": "match-0009", "subject": A.to_uppercase()});
        assert_eq!(post!(app, "/api/records", Some(&key), &capitals).await.0, 200);
        let (_, replay) = post!(
            app,
            "/api/games/g/tetris/results",
            Some(&key),
            &json!({"player": A, "result_id": "match-0009"}),
        )
        .await;
        assert_eq!(replay["duplicate"], true);
        let stored = h.records.store().results_for_day("tetris", DAY).unwrap();
        assert_eq!((stored.len(), stored[0].player.as_str()), (1, A));
        let rows_before = stored.len();

        // Unknown fields, including the old vocabulary and an `owner` slipped beside a proof.
        for body in [
            json!({"app_id": "tetris", "record_id": "match-0001", "subject": A, "score": 5}),
            json!({"app_id": "tetris", "result_id": "match-0001", "player": A}),
            json!({"app_id": "tetris", "record_id": "match-0001", "subject": A, "extra": 1}),
        ] {
            let (status, error) = post!(app, "/api/records", Some(&key), &body).await;
            assert_eq!((status, error["code"].clone()), (StatusCode::BAD_REQUEST, json!("invalid_request")), "{body}");
        }
        assert_eq!(h.records.store().results_for_day("tetris", DAY).unwrap().len(), rows_before);

        // Owner signatures on /api/apps: forged, for other terms, stale, and with unknown or old fields.
        let owner = QuantumKeyPair::generate();
        let now = h.service.now();
        let mut forged = register_body(&owner, "newgame", "New Game", now, true);
        forged["name"] = json!("Another Name");
        let stale = register_body(&owner, "newgame", "New Game", now - 400, true);
        for body in [forged, stale] {
            let (status, error) = post!(app, "/api/apps", None, &body).await;
            assert_eq!((status, error["code"].clone()), (StatusCode::UNAUTHORIZED, json!("invalid_key")));
        }
        let mut with_owner = register_body(&owner, "newgame", "New Game", now, true);
        with_owner["owner"] = json!(A);
        assert_eq!(post!(app, "/api/apps", None, &with_owner).await.0, 400);
        let old_vocabulary = register_body(&owner, "newgame", "New Game", now, false);
        assert_eq!(post!(app, "/api/apps", None, &old_vocabulary).await.0, 400, "`id` is not an /api/apps field");
        assert!(h.records.store().game("newgame").unwrap().is_none(), "nothing was registered");

        // Rotation: a stranger's signature is refused, and a replayed one cannot replace the new key.
        let created = post!(app, "/api/apps", None, &register_body(&owner, "newgame", "New Game", now, true)).await.1;
        let first_key = created["api_key"].as_str().unwrap().to_string();
        h.advance(10);
        let stranger = QuantumKeyPair::generate();
        let (status, error) = post!(app,
            "/api/apps/newgame/rotate-key",
            None,
            &rotate_body(&stranger, "newgame", h.service.now()),
        )
        .await;
        assert_eq!((status, error["code"].clone()), (StatusCode::UNAUTHORIZED, json!("invalid_key")));
        let rotation = rotate_body(&owner, "newgame", h.service.now());
        let (status, rotated) = post!(app, "/api/apps/newgame/rotate-key", None, &rotation).await;
        assert_eq!(status, 200);
        assert_ne!(rotated["api_key"].as_str().unwrap(), first_key);
        let (status, _) = post!(app, "/api/apps/newgame/rotate-key", None, &rotation).await;
        assert_eq!(status, 401, "a captured rotation cannot be replayed, on either route");
        let (status, _) = post!(app, "/api/games/g/newgame/rotate-key", None, &rotation).await;
        assert_eq!(status, 401);
    }

    #[actix_web::test]
    async fn one_signature_registers_on_either_route_with_the_same_result() {
        // The digest names the id and the name only, so the same signed terms are valid on both
        // routes: they differ only in the id's field name.
        let owner = QuantumKeyPair::generate();
        let mut outcomes = Vec::new();
        for (uri, generic) in [("/api/games/g", false), ("/api/apps", true)] {
            let h = harness(Options::default());
            let app = app!(h);
            let body = register_body(&owner, "tetris", "Tetris", h.service.now(), generic);
            let (status, reply) = post!(app, uri, None, &body).await;
            assert_eq!(status, 200, "{uri}: {reply}");
            outcomes.push((reply["game"]["owner"].clone(), reply["game"]["status"].clone(), reply["game"]["name"].clone()));
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(outcomes[0].0, json!(owner.address()));
    }
}

/// The split into the Records Service and the Games Reward Service: what each may do, how they
/// authenticate to each other, and what happens when one of them fails or restarts.
mod split {
    use super::*;
    use crate::games::{
        anchor::RemoteAnchorer,
        peer::{self, sign_request},
        records_api,
        records_config::RecordsServiceConfig,
    };
    use actix_web::{
        dev::ServerHandle,
        http::StatusCode,
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App, HttpServer,
    };
    use serde_json::{json, Value};
    use std::time::Duration;

    const SECRET: &[u8] = b"a-shared-peer-secret-longer-than-32-bytes";

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// Start the Records Service's real HTTP API on a port, as a separate server from the Games one.
    fn start_records(records: Arc<RecordsService>, port: u16) -> ServerHandle {
        let data = web::Data::from(records);
        let server = HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .configure(records_api::configure_records_routes)
        })
        .workers(1)
        .bind(("127.0.0.1", port))
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        handle
    }

    fn remote(port: u16, secret: &[u8], clock: Arc<AtomicU64>) -> Arc<dyn Anchorer> {
        Arc::new(
            RemoteAnchorer::new(
                &format!("http://127.0.0.1:{port}"),
                secret.to_vec(),
                Duration::from_secs(5),
            )
            .unwrap()
            .with_clock(Arc::new(move || clock.load(Ordering::SeqCst))),
        )
    }

    /// A Games Reward Service whose anchorer is the real remote one, talking to `port`; its Records
    /// Service is the harness's, with a peer secret.
    fn wired(port: u16, games_secret: &[u8]) -> Harness {
        let clock = Arc::new(AtomicU64::new(date_start(DAY).unwrap() + 3_600));
        harness(Options {
            games_anchorer: Some(remote(port, games_secret, clock.clone())),
            peer_secret: Some(SECRET.to_vec()),
            clock: Some(clock),
            ..Options::default()
        })
    }

    fn day_row_state(h: &Harness, date: &str) -> (String, Option<String>, Option<String>) {
        let row = h.service.store().day_row(date, GAME_GRID).unwrap().unwrap();
        (row.state, row.anchor_tx, row.last_error)
    }

    // --- boundaries: each service serves only its own routes --------------------------------------

    #[actix_web::test]
    async fn each_service_serves_only_its_own_routes() {
        let h = harness(Options::default());
        let games = init_service(
            App::new()
                .app_data(web::Data::from(h.service.clone()))
                .configure(crate::games::api::configure_routes),
        )
        .await;
        let records = init_service(
            App::new()
                .app_data(web::Data::from(h.records.clone()))
                .configure(records_api::configure_records_routes),
        )
        .await;
        for uri in [
            "/api/games/g",
            "/api/games/g/tetris",
            "/api/games/registry/days/2026-11-16",
            "/api/records/days/2026-11-16",
            "/api/apps/tetris",
            "/api/records/status",
        ] {
            let in_games = call_service(&games, TestRequest::get().uri(uri).to_request()).await;
            assert_eq!(in_games.status(), 404, "the Games Reward Service must not serve {uri}");
        }
        for uri in [
            "/api/games/status",
            "/api/games/grid/day?date=2026-11-16",
            "/api/games/wallet/7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e",
            "/api/games/payouts/abc",
        ] {
            let in_records = call_service(&records, TestRequest::get().uri(uri).to_request()).await;
            assert_eq!(in_records.status(), 404, "the Records Service must not serve {uri}");
        }
        for uri in ["/api/games/withdraw", "/api/games/grid/answers", "/api/games/chess/results"] {
            let response = call_service(&records, TestRequest::post().uri(uri).set_json(json!({})).to_request()).await;
            assert_eq!(response.status(), 404, "the Records Service must not accept {uri}");
        }
        for uri in ["/api/records", "/api/apps", "/internal/v1/anchor", "/api/games/g"] {
            let response = call_service(&games, TestRequest::post().uri(uri).set_json(json!({})).to_request()).await;
            assert_eq!(response.status(), 404, "the Games Reward Service must not accept {uri}");
        }
    }

    #[test]
    fn neither_service_can_be_given_the_others_key() {
        let dir = std::env::temp_dir().join(format!("split-creds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(crate::games::config::GamesConfig::foreign_credentials_in(&dir).is_empty());
        assert!(RecordsServiceConfig::foreign_credentials_in(&dir).is_empty());
        // The owner key in the Games unit's directory, under either name, is refused...
        for name in ["games-anchor-owner.key.enc", "games-anchor.pass", "records-anchor-owner.key.enc", "records-anchor.pass"] {
            std::fs::write(dir.join(name), b"x").unwrap();
            assert_eq!(crate::games::config::GamesConfig::foreign_credentials_in(&dir).len(), 1, "{name}");
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        // ...and the payout key, or the abuse key, in the Records unit's.
        for name in ["games-payout-signer.json", "games-abuse.key"] {
            std::fs::write(dir.join(name), b"x").unwrap();
            assert_eq!(RecordsServiceConfig::foreign_credentials_in(&dir).len(), 1, "{name}");
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- the Records Service's scope -------------------------------------------------------------

    #[actix_web::test]
    async fn a_peer_may_have_a_game_day_anchored_but_never_the_registry_or_the_l1() {
        let h = harness(Options::default());
        let root = [7u8; 32];
        for chain in [0u32, REGISTRY_CHAIN_ID, 9] {
            let error = h.records.anchor_for_peer(chain, root).await.unwrap_err();
            assert_eq!(error.code(), "forbidden", "chain {chain}");
            assert_eq!(error.status(), 403);
        }
        assert!(h.records_anchorer.calls.lock().unwrap().is_empty(), "nothing was signed");
        let reply = h.records.anchor_for_peer(1, root).await.unwrap();
        assert_eq!((reply.chain_id, reply.state), (1, "submitted"));
        assert_eq!(h.records_anchorer.calls.lock().unwrap().clone(), vec![(1, root)]);
    }

    #[actix_web::test]
    async fn the_same_root_is_anchored_once_and_a_root_already_on_the_ledger_never() {
        let h = harness(Options::default());
        let root = [8u8; 32];
        let first = h.records.anchor_for_peer(1, root).await.unwrap();
        let again = h.records.anchor_for_peer(1, root).await.unwrap();
        assert_eq!(first.tx_hash, again.tx_hash, "a repeated ask returns the same transaction");
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);
        // A different root is a different anchor.
        h.records.anchor_for_peer(1, [9u8; 32]).await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 2);
        // On the ledger already (an earlier life of this service): answered from the ledger, not signed.
        h.chain.add_anchor(CHESS_CHAIN, 1, [5u8; 32], h.records.now());
        let reply = h.records.anchor_for_peer(CHESS_CHAIN, [5u8; 32]).await.unwrap();
        assert_eq!(reply.state, "confirmed");
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 2, "nothing new was signed");
    }

    #[actix_web::test]
    async fn a_peer_cannot_drain_the_owner_wallet_by_asking_for_many_different_anchors() {
        let h = harness(Options::default());
        let cap = 8u8;
        for i in 0..cap {
            h.records.anchor_for_peer(1, [i + 1; 32]).await.unwrap();
        }
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), cap as usize);
        // The ninth distinct root is refused and signs nothing.
        let error = h.records.anchor_for_peer(1, [99u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("forbidden", 403));
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), cap as usize);
        // Asking again for one it already anchored is free: it is not a new anchor.
        h.records.anchor_for_peer(1, [1u8; 32]).await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), cap as usize);
        // The next UTC day the allowance is back.
        h.next_day();
        h.records.anchor_for_peer(1, [99u8; 32]).await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), cap as usize + 1);
    }

    #[actix_web::test]
    async fn an_unreadable_ledger_or_a_failing_signer_or_a_closed_switch_submits_nothing_wrong() {
        let h = harness(Options::default());
        h.chain.down.store(true, Ordering::SeqCst);
        let error = h.records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
        assert_eq!(error.code(), "chain_unavailable");
        assert!(h.records_anchorer.calls.lock().unwrap().is_empty(), "no look, no submit");
        h.chain.down.store(false, Ordering::SeqCst);

        h.records_anchorer.fail.store(true, Ordering::SeqCst);
        let error = h.records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("anchor_failed", 502));
        h.records_anchorer.fail.store(false, Ordering::SeqCst);

        h.records.store().set_state(Some(false), None, Some("maintenance"), 1).unwrap();
        assert_eq!(h.records.anchor_for_peer(1, [2u8; 32]).await.unwrap_err().code(), "service_disabled");
    }

    // --- the internal route's authentication ------------------------------------------------------

    fn internal(body: &Value, secret: &[u8], at: u64) -> actix_web::test::TestRequest {
        let signed = sign_request(secret, "games", "POST", "/internal/v1/anchor", body.to_string().as_bytes(), at);
        internal_with(&signed, body)
    }

    fn internal_with(signed: &peer::SignedHeaders, body: &Value) -> actix_web::test::TestRequest {
        let body = body.to_string();
        let signed = peer::SignedHeaders {
            peer: signed.peer.clone(),
            timestamp: signed.timestamp,
            nonce: signed.nonce.clone(),
            signature: signed.signature.clone(),
        };
        TestRequest::post()
            .uri("/internal/v1/anchor")
            .insert_header((peer::HEADER_PEER, signed.peer))
            .insert_header((peer::HEADER_TIMESTAMP, signed.timestamp.to_string()))
            .insert_header((peer::HEADER_NONCE, signed.nonce))
            .insert_header((peer::HEADER_SIGNATURE, signed.signature))
            .insert_header(("Content-Type", "application/json"))
            .set_payload(body)
    }

    #[actix_web::test]
    async fn the_internal_route_refuses_everything_that_is_not_a_fresh_signed_request() {
        let h = harness(Options {
            peer_secret: Some(SECRET.to_vec()),
            ..Options::default()
        });
        let app = init_service(
            App::new()
                .app_data(web::Data::from(h.records.clone()))
                .configure(records_api::configure_records_routes),
        )
        .await;
        let now = h.records.now();
        let body = json!({"chain_id": 1, "root": "ab".repeat(32)});
        let status = |response: actix_web::dev::ServiceResponse| async move {
            let status = response.status();
            let value: Value = read_body_json(response).await;
            (status, value["code"].as_str().unwrap_or("").to_string())
        };

        // No headers, a wrong secret, a stale time, a tampered body: all 401, nothing signed.
        let unsigned = TestRequest::post().uri("/internal/v1/anchor").set_json(&body).to_request();
        assert_eq!(status(call_service(&app, unsigned).await).await, (StatusCode::UNAUTHORIZED, "peer_unauthenticated".into()));
        let wrong = internal(&body, b"another-secret-also-longer-than-32-bytes!!", now).to_request();
        assert_eq!(status(call_service(&app, wrong).await).await.0, 401);
        let stale = internal(&body, SECRET, now - 120).to_request();
        assert_eq!(status(call_service(&app, stale).await).await.0, 401);
        let mut tampered = internal(&body, SECRET, now);
        tampered = tampered.set_payload(json!({"chain_id": CHESS_CHAIN, "root": "ab".repeat(32)}).to_string());
        assert_eq!(status(call_service(&app, tampered.to_request()).await).await.0, 401);
        assert!(h.records_anchorer.calls.lock().unwrap().is_empty());

        // A genuine one works once; the identical request replayed is refused.
        let signed = sign_request(SECRET, "games", "POST", "/internal/v1/anchor", body.to_string().as_bytes(), now);
        assert_eq!(call_service(&app, internal_with(&signed, &body).to_request()).await.status(), 200);
        assert_eq!(
            status(call_service(&app, internal_with(&signed, &body).to_request()).await).await.0,
            401,
            "a captured request cannot be replayed"
        );
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);

        // Authenticated but out of scope, or malformed: a 403 or a 400, still nothing signed.
        let scope = json!({"chain_id": REGISTRY_CHAIN_ID, "root": "ab".repeat(32)});
        assert_eq!(status(call_service(&app, internal(&scope, SECRET, now).to_request()).await).await, (StatusCode::FORBIDDEN, "forbidden".into()));
        for bad in [json!({"chain_id": 1, "root": "zz"}), json!({"chain_id": 1}), json!({"chain_id": 1, "root": "ab".repeat(32), "extra": 1})] {
            assert_eq!(status(call_service(&app, internal(&bad, SECRET, now).to_request()).await).await.0, 400, "{bad}");
        }
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);
    }

    #[actix_web::test]
    async fn without_a_peer_secret_the_internal_route_does_not_exist() {
        let h = harness(Options::default());
        let app = init_service(
            App::new()
                .app_data(web::Data::from(h.records.clone()))
                .configure(records_api::configure_records_routes),
        )
        .await;
        let response = call_service(&app, internal(&json!({"chain_id": 1, "root": "ab".repeat(32)}), SECRET, h.records.now()).to_request()).await;
        assert_eq!(response.status(), 400);
        assert!(h.records_anchorer.calls.lock().unwrap().is_empty());
    }

    // --- both services, over real HTTP ------------------------------------------------------------

    #[actix_web::test]
    async fn a_day_is_anchored_by_the_records_service_on_the_games_services_request() {
        let port = free_port();
        let h = wired(port, SECRET);
        let server = start_records(h.records.clone(), port);

        let report = h.service.run_daily().await.unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let calls = h.records_anchorer.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "the Records Service signed exactly one anchor");
        assert_eq!(calls[0].0, 1, "on chain #1");
        assert!(h.anchorer.calls.lock().unwrap().is_empty(), "the Games service signed nothing itself");
        let yesterday = shift_date(DAY, -1).unwrap();
        let (state, tx, error) = day_row_state(&h, &yesterday);
        assert_eq!((state.as_str(), error), ("anchored", None));
        assert_eq!(tx.unwrap().len(), 64);
        // The root the Records Service anchored is the one the Games service stored.
        let stored = h.service.store().day_row(&yesterday, GAME_GRID).unwrap().unwrap().day_root.unwrap();
        assert_eq!(calls[0].1.to_vec(), stored);

        // Run again at once: still one anchor, nothing duplicated.
        h.service.run_daily().await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);
        server.stop(true).await;
    }

    #[actix_web::test]
    async fn the_wrong_secret_gets_nothing_anchored_and_the_error_is_recorded() {
        let port = free_port();
        let h = wired(port, b"a-different-secret-that-is-long-enough-xyz");
        let server = start_records(h.records.clone(), port);
        h.service.run_daily().await.unwrap();
        assert!(h.records_anchorer.calls.lock().unwrap().is_empty(), "an unauthenticated caller anchors nothing");
        let (state, tx, error) = day_row_state(&h, &shift_date(DAY, -1).unwrap());
        assert_ne!(state, "anchored");
        assert!(tx.is_none());
        assert!(error.unwrap().contains("peer_unauthenticated"));
        server.stop(true).await;
    }

    #[actix_web::test]
    async fn a_records_outage_and_restart_neither_loses_nor_doubles_an_anchor_and_never_stops_payouts() {
        let port = free_port();
        let h = wired(port, SECRET);
        let yesterday = shift_date(DAY, -1).unwrap();

        // 1. The Records Service is down. The day is closed locally, the failure is recorded, and the
        //    Games service goes on: a win is credited and a withdrawal is paid.
        let report = h.service.run_daily().await.unwrap();
        assert!(!report.errors.is_empty());
        let (state, tx, error) = day_row_state(&h, &yesterday);
        assert_ne!(state, "anchored", "nothing was anchored while the Records Service was down");
        assert!(tx.is_none() && error.is_some(), "the failure is recorded: {error:?}");
        let answer = h.grid_answer(A, 0).await;
        h.service.submit_grid_answer(answer, None).await.unwrap();
        let state_now = h.service.status().unwrap();
        assert!(state_now.accepting && state_now.paying, "an outage elsewhere paused nothing here");

        // 2. It starts. After the retry interval the same day is anchored, once.
        let server = start_records(h.records.clone(), port);
        h.advance(601);
        h.service.run_daily().await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);
        assert_eq!(day_row_state(&h, &yesterday).0, "anchored");

        // 3. It restarts with no memory of what it did (a new service over the same ledger). The anchor
        //    is already on the chain, so the next pass settles it from the ledger and signs nothing more.
        server.stop(true).await;
        let (_, tx, _) = day_row_state(&h, &yesterday);
        let root: [u8; 32] = h.records_anchorer.calls.lock().unwrap()[0].1;
        h.chain.add_anchor(1, 1, root, h.records.now());
        let mut restarted_config = RecordsConfig::new();
        restarted_config.peer_anchor_chains = vec![GRID_CHAIN, CHESS_CHAIN];
        let restarted = Arc::new(RecordsService::new(
            GamesStore::open(Path::new(":memory:")).unwrap(),
            RegistryCaps::standard(),
            restarted_config,
            h.chain.clone(),
            Some(h.records_anchorer.clone()),
            Some(PeerVerifier::new(SECRET.to_vec(), "games").unwrap()),
            {
                let clock = h.clock.clone();
                Arc::new(move || clock.load(Ordering::SeqCst))
            },
        ));
        let server = start_records(restarted, port);
        h.advance(4_000);
        h.service.run_daily().await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1, "no second anchor after a restart");
        assert_eq!(day_row_state(&h, &yesterday).0, "confirmed");
        assert!(tx.is_some());
        server.stop(true).await;
    }

    #[actix_web::test]
    async fn payouts_and_anchoring_run_side_by_side_and_the_books_still_balance() {
        let port = free_port();
        let h = wired(port, SECRET);
        let server = start_records(h.records.clone(), port);

        // A player earns 100 units and withdraws; the daily pass anchors yesterday through the Records
        // Service while that payout is in flight.
        earn_chess_hundred(&h, A, GAME_ID_1).await;
        let queued = h.service.withdraw(A).unwrap();
        h.drain_payouts(1).await;
        h.service.run_daily().await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1, "one anchor, signed by the Records Service");
        let row = h.service.store().payout(&queued.id).unwrap().unwrap();
        assert_eq!(row.state, PayoutState::Submitted);
        h.node.set_status(row.tx_hash.as_deref().unwrap(), "confirmed");
        h.drain_payouts(1).await;
        let wallet = h.service.wallet(A).unwrap();
        assert_eq!(
            (wallet.credited_units, wallet.paid_units, wallet.reserved_units, wallet.available_units),
            (100, 100, 0, 0)
        );
        assert_eq!(h.node.submissions.lock().unwrap().len(), 1, "paid exactly once");

        // A second player's payout is rejected by the node: payments pause, the 100 units stay owed and
        // reserved, and the anchoring does not notice or care.
        earn_chess_hundred(&h, B, GAME_ID_2).await;
        let second = h.service.withdraw(B).unwrap();
        h.node.set_mode(SubmitMode::Reject);
        h.drain_payouts(1).await;
        assert!(!h.service.store().state().unwrap().paying, "a rejected payout pauses payments");
        let stuck = h.service.store().payout(&second.id).unwrap().unwrap();
        assert!(stuck.state.is_open(), "the signed payout stays open for an operator, never silently dropped");
        assert_eq!(h.service.wallet(B).unwrap().reserved_units, 100, "the 100 units stay owed and reserved");
        h.next_day();
        h.service.run_daily().await.unwrap();
        assert!(
            h.records_anchorer.calls.lock().unwrap().len() >= 2,
            "the next day was still anchored while payments were paused"
        );

        // Every balance is explained by wins and payouts, in the Games database; the Records database
        // holds no wins, balances or payouts at all.
        let report = h.service.store().audit().unwrap();
        assert!(report.ok(), "{:?}", report.issues);
        assert_eq!(h.records.store().wins_for_day(GAME_CHESS, DAY).unwrap().len(), 0);
        assert!(h.records.store().payout(&queued.id).unwrap().is_none());
        assert!(h.records.store().audit().unwrap().ok(), "an empty ledger audits clean");
        server.stop(true).await;
    }

    #[actix_web::test]
    async fn the_registry_keeps_working_while_the_games_service_is_down_or_never_started() {
        // The Records Service has no reference to the Games Reward Service: it needs nothing from it.
        let h = harness(Options::default());
        let key = {
            let (_, key) = h.records.store().register_game("tetris", "Tetris", A, h.records.now()).unwrap();
            h.records.store().set_game_status("tetris", crate::games::registry::GameStatus::Registered, h.records.now()).unwrap();
            key
        };
        let reply = h
            .records
            .submit_registry_result("tetris", &key, crate::games::records::RegistryResultRequest {
                player: A.into(),
                result_id: "match-0001".into(),
                score: 1,
                data_hash: None,
            })
            .unwrap();
        assert!(reply.anchored);
        h.next_day();
        h.records.run_daily().await.unwrap();
        assert_eq!(h.records_anchorer.calls.lock().unwrap().len(), 1);
        // Nothing the Records Service did touched the Games database.
        let games_state = h.service.store().state().unwrap();
        assert!(games_state.accepting && games_state.paying);
        assert!(h.service.store().audit().unwrap().ok());
    }

    #[actix_web::test]
    async fn the_two_databases_are_independent_and_the_games_one_holds_no_registry_records() {
        let h = harness(Options::default());
        h.records.store().register_game("tetris", "Tetris", A, h.records.now()).unwrap();
        assert!(h.service.store().games().unwrap().is_empty(), "a registry game is not in the Games database");
        assert_eq!(h.records.store().games().unwrap().len(), 1);
        let answer = h.grid_answer(A, 0).await;
        h.service.submit_grid_answer(answer, None).await.unwrap();
        assert_eq!(h.records.store().wins_for_day(GAME_GRID, DAY).unwrap().len(), 0, "a win is not in the Records database");
        assert_eq!(h.service.store().wins_for_day(GAME_GRID, DAY).unwrap().len(), 1);
    }

    // --- the peer-anchor allowance is durable ------------------------------------------------------

    #[derive(Clone, Copy)]
    enum Mode {
        Accept,
        /// The signer could not even be run: nothing was submitted.
        Definite,
        /// A timeout: the transaction may or may not have been accepted.
        Ambiguous,
        /// The CLI ran and reported an error (for example it could not reach the node): it may have been
        /// accepted before the connection failed.
        CliFailed,
    }

    struct ScriptedAnchorer {
        calls: Mutex<Vec<(u32, [u8; 32])>>,
        mode: Mutex<Mode>,
    }

    impl ScriptedAnchorer {
        fn new(mode: Mode) -> Arc<ScriptedAnchorer> {
            Arc::new(ScriptedAnchorer {
                calls: Mutex::new(Vec::new()),
                mode: Mutex::new(mode),
            })
        }

        fn calls(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn set(&self, mode: Mode) {
            *self.mode.lock().unwrap() = mode;
        }
    }

    #[async_trait]
    impl Anchorer for ScriptedAnchorer {
        async fn anchor(&self, chain_id: u32, root: &[u8; 32]) -> Result<String, AnchorError> {
            self.calls.lock().unwrap().push((chain_id, *root));
            let mode = *self.mode.lock().unwrap();
            match mode {
                Mode::Accept => Ok(format!("{:064x}", self.calls.lock().unwrap().len())),
                Mode::Definite => Err(AnchorError::Spawn("no such file".into())),
                Mode::Ambiguous => Err(AnchorError::TimedOut),
                Mode::CliFailed => Err(AnchorError::Failed("cannot reach the node".into())),
            }
        }
    }

    /// Never returns: the process is killed while the signer is running.
    struct HangingAnchorer;

    #[async_trait]
    impl Anchorer for HangingAnchorer {
        async fn anchor(&self, _chain_id: u32, _root: &[u8; 32]) -> Result<String, AnchorError> {
            std::future::pending().await
        }
    }

    fn db_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("records-durable-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("records.db")
    }

    /// A Records Service over a database *file*: dropping it and calling this again is a restart.
    fn records_on(
        path: &std::path::Path,
        clock: &Arc<AtomicU64>,
        chain: &Arc<FakeChain>,
        anchorer: Arc<dyn Anchorer>,
    ) -> Arc<RecordsService> {
        records_with(path, clock, chain, anchorer, None)
    }

    fn records_with(
        path: &std::path::Path,
        clock: &Arc<AtomicU64>,
        chain: &Arc<FakeChain>,
        anchorer: Arc<dyn Anchorer>,
        registry_chain: Option<u32>,
    ) -> Arc<RecordsService> {
        let clock = clock.clone();
        Arc::new(RecordsService::new(
            GamesStore::open(path).unwrap(),
            RegistryCaps::standard(),
            {
                let mut config = RecordsConfig::new();
                config.peer_anchor_chains = vec![GRID_CHAIN, CHESS_CHAIN];
                config.owner_address = Some(A.to_string());
                config.registry_chain_id = registry_chain;
                config
            },
            chain.clone(),
            Some(anchorer),
            None,
            Arc::new(move || clock.load(Ordering::SeqCst)),
        ))
    }

    fn fresh_clock() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(date_start(DAY).unwrap() + 3_600))
    }

    fn open_chain() -> Arc<FakeChain> {
        let chain = Arc::new(FakeChain::default());
        chain.everyone_active.store(true, Ordering::SeqCst);
        chain
    }

    #[actix_web::test]
    async fn eight_distinct_roots_are_anchored_the_ninth_is_refused_and_a_restart_changes_nothing() {
        let path = db_file("restart");
        let (clock, chain) = (fresh_clock(), open_chain());
        let first = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, first.clone());
        let mut txs = Vec::new();
        for n in 1..=8u8 {
            txs.push(records.anchor_for_peer(1, [n; 32]).await.unwrap().tx_hash);
        }
        let error = records.anchor_for_peer(1, [9u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("forbidden", 403));
        assert_eq!(first.calls(), 8);

        // The process dies and restarts, three times. The allowance does not come back.
        drop(records);
        for _ in 0..3 {
            let again = ScriptedAnchorer::new(Mode::Accept);
            let records = records_on(&path, &clock, &chain, again.clone());
            let error = records.anchor_for_peer(1, [9u8; 32]).await.unwrap_err();
            assert_eq!(error.code(), "forbidden", "a restart must not reset the limit");
            assert_eq!(again.calls(), 0, "and nothing was signed");
            // A root it already anchored is answered from the record, with its transaction, for free.
            let repeat = records.anchor_for_peer(1, [3u8; 32]).await.unwrap();
            assert_eq!(repeat.tx_hash, txs[2]);
            assert_eq!(again.calls(), 0);
        }
    }

    #[actix_web::test]
    async fn a_crash_during_submission_cannot_bypass_the_limit_or_double_submit() {
        let path = db_file("crash");
        let (clock, chain) = (fresh_clock(), open_chain());
        // The signer is running when the process is killed: the request future is dropped mid-call.
        let records = records_on(&path, &clock, &chain, Arc::new(HangingAnchorer));
        let killed = tokio::time::timeout(Duration::from_millis(100), records.anchor_for_peer(1, [1u8; 32])).await;
        assert!(killed.is_err(), "the call never finished");
        drop(records);

        let healthy = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, healthy.clone());
        // The slot and the attempt survived. Asking again inside the window does not submit a second time.
        let error = records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("anchor_failed", 502));
        assert!(error.to_string().contains("in doubt"), "{error}");
        assert_eq!(healthy.calls(), 0);
        // The crashed root still holds its slot: seven more, not eight, fit in the day.
        for n in 2..=8u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
        }
        assert_eq!(records.anchor_for_peer(1, [9u8; 32]).await.unwrap_err().code(), "forbidden");
        assert_eq!(healthy.calls(), 7);
        // After the window the doubtful root may be tried again, still inside the same slot.
        clock.fetch_add(3_601, Ordering::SeqCst);
        assert!(records.anchor_for_peer(1, [1u8; 32]).await.is_ok());
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 8, "no extra slot");
    }

    #[actix_web::test]
    async fn a_failed_submission_is_retried_safely_without_exceeding_the_limit() {
        let path = db_file("retry");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Definite);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let today = records.today();

        // The signer cannot be run (a definite failure: nothing was submitted), twice, then works.
        for _ in 0..2 {
            let error = records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
            assert_eq!(error.code(), "anchor_failed");
        }
        assert_eq!(records.store().peer_anchor_usage(&today).unwrap(), 1, "two failures, one slot");
        signer.set(Mode::Accept);
        assert_eq!(records.anchor_for_peer(1, [1u8; 32]).await.unwrap().state, "submitted");
        assert_eq!(signer.calls(), 3);
        assert_eq!(records.store().peer_anchor_usage(&today).unwrap(), 1);

        // A root that never works is capped: three attempts a day, then it stops costing fees.
        signer.set(Mode::Definite);
        for _ in 0..3 {
            assert_eq!(records.anchor_for_peer(1, [2u8; 32]).await.unwrap_err().code(), "anchor_failed");
        }
        signer.set(Mode::Accept);
        let before = signer.calls();
        let error = records.anchor_for_peer(1, [2u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("forbidden", 403));
        assert_eq!(signer.calls(), before, "an exhausted root is not submitted again today");
        assert_eq!(records.store().peer_anchor_usage(&today).unwrap(), 2);
    }

    #[actix_web::test]
    async fn an_error_from_the_cli_or_a_timeout_is_ambiguous_and_is_never_retried_on_a_timer() {
        for mode in [Mode::Ambiguous, Mode::CliFailed] {
            let path = db_file("ambiguous");
            let (clock, chain) = (fresh_clock(), open_chain());
            let signer = ScriptedAnchorer::new(mode);
            let records = records_on(&path, &clock, &chain, signer.clone());
            assert_eq!(records.anchor_for_peer(1, [3u8; 32]).await.unwrap_err().code(), "anchor_failed");
            assert_eq!(signer.calls(), 1);
            signer.set(Mode::Accept);
            // An hour, a day, a month later: still no second signer call, because the first may have landed.
            // (An owner transaction in the attempt's window is what keeps it unprovable.)
            let tx_time = records.now() + 5;
            chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "aa".repeat(32), timestamp: tx_time, pending: false });
            for later in [3_601u64, 86_400, 30 * 86_400] {
                clock.store(date_start(DAY).unwrap() + 3_600 + later, Ordering::SeqCst);
                let error = records.anchor_for_peer(1, [3u8; 32]).await.unwrap_err();
                assert!(error.to_string().contains("in doubt"), "{error}");
                assert_eq!(signer.calls(), 1, "no second signer call after {later}s");
            }
            // If it did land, the ledger says so, and that is the end of it.
            chain.add_anchor(1, 1, [3u8; 32], tx_time);
            assert_eq!(records.anchor_for_peer(1, [3u8; 32]).await.unwrap().state, "confirmed");
            assert_eq!(signer.calls(), 1);
        }
    }

    /// The scenario of the review: the signer submitted, the node accepted, the process died before the
    /// transaction hash was saved. Returns the service after the restart and the signer behind it.
    async fn crashed_after_acceptance(
        name: &str,
    ) -> (std::path::PathBuf, Arc<AtomicU64>, Arc<FakeChain>, u64) {
        let path = db_file(name);
        let (clock, chain) = (fresh_clock(), open_chain());
        let records = records_on(&path, &clock, &chain, Arc::new(HangingAnchorer));
        let begun_at = records.now();
        let killed = tokio::time::timeout(Duration::from_millis(100), records.anchor_for_peer(1, [5u8; 32])).await;
        assert!(killed.is_err(), "the process died while the signer ran");
        drop(records);
        (path, clock, chain, begun_at)
    }

    #[actix_web::test]
    async fn an_accepted_anchor_that_is_not_yet_visible_is_never_submitted_a_second_time() {
        let (path, clock, chain, begun_at) = crashed_after_acceptance("invisible").await;
        // The node did accept it: the owner has a pending AppChain transaction signed just after the attempt
        // began. The ledger index shows nothing yet.
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "bb".repeat(32), timestamp: begun_at + 5, pending: true });
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());

        // Immediately after the restart, after the old resubmit window, after a day: always in doubt.
        for later in [10u64, 3_601, 7_200, 86_400 - 4_000] {
            clock.store(begun_at + later, Ordering::SeqCst);
            let error = records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err();
            assert_eq!(error.code(), "anchor_failed", "after {later}s");
            assert!(error.to_string().contains("in doubt"), "{error}");
        }
        assert_eq!(signer.calls(), 0, "the signer was not invoked a second time");

        // It is mined, but the anchors index is delayed: the owner's confirmed history shows it, the ledger
        // lookup does not. Still no second submission.
        chain.owner_txs.lock().unwrap()[0].pending = false;
        chain.index_lag.store(true, Ordering::SeqCst);
        chain.add_anchor(1, 1, [5u8; 32], begun_at + 30);
        let error = records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("in doubt"), "{error}");
        assert_eq!(signer.calls(), 0, "a delayed index is not evidence of rejection");

        // The index catches up: the anchor is found and confirmed, and still nothing was signed.
        chain.index_lag.store(false, Ordering::SeqCst);
        let reply = records.anchor_for_peer(1, [5u8; 32]).await.unwrap();
        assert_eq!(reply.state, "confirmed");
        assert_eq!(signer.calls(), 0);
    }

    #[actix_web::test]
    async fn only_proof_that_the_first_attempt_never_reached_the_node_allows_one_retry() {
        let (path, clock, chain, begun_at) = crashed_after_acceptance("proof").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let after_window = begun_at + 60 + 120 + 120 + 1;

        // Too early: the first signer could still be running, so even an empty history is not proof.
        clock.store(begun_at + 30, Ordering::SeqCst);
        assert!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err().to_string().contains("in doubt"));
        // The history cannot be shown completely: absence proves nothing.
        clock.store(after_window, Ordering::SeqCst);
        chain.history_incomplete.store(true, Ordering::SeqCst);
        // (It is stopped even earlier now, by the owner-history check: a history that cannot be read back far
        // enough cannot be compared with the recorded attempts, so nothing is signed.)
        assert_eq!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err().code(), "reconciliation_required");
        // The node cannot be read: nothing is submitted, and nothing is concluded.
        chain.history_incomplete.store(false, Ordering::SeqCst);
        chain.down.store(true, Ordering::SeqCst);
        assert_eq!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err().code(), "chain_unavailable");
        chain.down.store(false, Ordering::SeqCst);
        // A transaction in the window, even a long-confirmed one, might be this anchor.
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "cc".repeat(32), timestamp: begun_at + 20, pending: false });
        assert!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err().to_string().contains("in doubt"));
        assert_eq!(signer.calls(), 0, "no proof, no signer");

        // Transactions outside the window are not this anchor; with a complete, readable history and the
        // window closed, the first attempt provably never reached the node: exactly one retry.
        chain.owner_txs.lock().unwrap()[0].timestamp = begun_at + 5_000;
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "dd".repeat(32), timestamp: begun_at - 5_000, pending: false });
        clock.store(begun_at + 6_000, Ordering::SeqCst);
        // Those two are the owner's, but not this service's: the operator has reconciled and acknowledged them.
        records.store().set_owner_ack(begun_at + 5_500, records.store().recorded_attempts(&[&records.today()]).unwrap()).unwrap();
        assert_eq!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap().state, "submitted");
        assert_eq!(signer.calls(), 1);
        // And now it is a recorded submission: asking again never signs again.
        assert_eq!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap().state, "submitted");
        assert_eq!(signer.calls(), 1);
    }

    #[actix_web::test]
    async fn a_pending_owner_transaction_blocks_a_new_submission_until_it_is_mined() {
        let path = db_file("pending");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        // One anchor is accepted and is still pending in the node.
        records.anchor_for_peer(1, [6u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "ee".repeat(32), timestamp: records.now(), pending: true });
        let error = records.anchor_for_peer(1, [7u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("still pending"), "{error}");
        assert_eq!(signer.calls(), 1, "nothing was signed on top of the pending transaction");
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 2, "the slot is held, the attempt is not counted");
        chain.owner_txs.lock().unwrap()[0].pending = false;
        assert!(records.anchor_for_peer(1, [7u8; 32]).await.is_ok());
        assert_eq!(signer.calls(), 2);
    }

    #[actix_web::test]
    async fn an_unresolved_root_from_yesterday_is_not_a_fresh_root_today() {
        let path = db_file("yesterday");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let tx = records.anchor_for_peer(1, [7u8; 32]).await.unwrap().tx_hash;
        // The next day the Games service asks again for the same root, which the ledger still does not show.
        clock.store(date_start(&shift_date(DAY, 1).unwrap()).unwrap() + 3_600, Ordering::SeqCst);
        let again = records.anchor_for_peer(1, [7u8; 32]).await.unwrap();
        assert_eq!((again.tx_hash, signer.calls()), (tx, 1), "answered from yesterday's record, not signed again");
    }

    #[actix_web::test]
    async fn concurrent_requests_cannot_exceed_the_limit() {
        let path = db_file("concurrent");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let tasks: Vec<_> = (1..=24u8)
            .map(|n| {
                let records = records.clone();
                actix_web::rt::spawn(async move { records.anchor_for_peer(1, [n; 32]).await })
            })
            .collect();
        let mut accepted = 0;
        let mut refused = 0;
        for task in tasks {
            match task.await.unwrap() {
                Ok(_) => accepted += 1,
                Err(error) => {
                    assert_eq!(error.code(), "forbidden");
                    refused += 1;
                }
            }
        }
        assert_eq!((accepted, refused), (8, 16));
        assert_eq!(signer.calls(), 8, "exactly the allowance was signed");
        // The same root asked for 20 times at once is one anchor.
        let duplicate: Vec<_> = (0..20)
            .map(|_| {
                let records = records.clone();
                actix_web::rt::spawn(async move { records.anchor_for_peer(1, [1u8; 32]).await })
            })
            .collect();
        for task in duplicate {
            assert!(task.await.unwrap().is_ok());
        }
        assert_eq!(signer.calls(), 8, "duplicates signed nothing");
    }

    #[actix_web::test]
    async fn utc_midnight_starts_a_new_allowance_at_the_service_level() {
        let path = db_file("midnight");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        for n in 1..=8u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
        }
        // One second before midnight: still the old day.
        clock.store(date_start(&shift_date(DAY, 1).unwrap()).unwrap() - 1, Ordering::SeqCst);
        assert_eq!(records.anchor_for_peer(1, [9u8; 32]).await.unwrap_err().code(), "forbidden");
        // At midnight: a new allowance, for new roots.
        clock.fetch_add(1, Ordering::SeqCst);
        // A root already anchored yesterday is the same anchor: answered from its record, no fee, no slot.
        for n in 1..=8u8 {
            assert_eq!(records.anchor_for_peer(1, [n; 32]).await.unwrap().state, "submitted");
        }
        assert_eq!(signer.calls(), 8, "yesterday's roots signed nothing today");
        for n in 11..=18u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
        }
        assert_eq!(records.anchor_for_peer(1, [19u8; 32]).await.unwrap_err().code(), "forbidden");
        assert_eq!(signer.calls(), 16);
    }

    #[actix_web::test]
    async fn corrupted_usage_state_stops_anchoring_and_is_never_silently_reset() {
        let path = db_file("corrupt");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        for n in 1..=8u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
        }
        drop(records);
        // Someone (or something) zeroes the counter to get a fresh allowance.
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("UPDATE peer_anchor_days SET used = 0;")
            .unwrap();
        let records = records_on(&path, &clock, &chain, signer.clone());
        let before = signer.calls();
        let error = records.anchor_for_peer(1, [9u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("internal_error", 500));
        assert_eq!(signer.calls(), before, "nothing is signed on unusable state");
        let again = records.anchor_for_peer(1, [10u8; 32]).await.unwrap_err();
        assert_eq!(again.code(), "internal_error", "it stays refused; it did not heal itself");
        let used: i64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("SELECT used FROM peer_anchor_days", [], |r| r.get(0))
            .unwrap();
        assert_eq!(used, 0, "the tampered counter was left exactly as found");
        // The client is told nothing but that something went wrong.
        assert!(!error.to_string().is_empty());
    }

    // --- restoring an older database must not regress the allowance -------------------------------

    #[actix_web::test]
    async fn a_database_older_than_the_ledger_refuses_to_anchor_until_reconciled() {
        // The live service anchored eight roots today. The backup it is "restored" from knew only three.
        let (clock, chain) = (fresh_clock(), open_chain());
        let live_path = db_file("restore-live");
        let live = ScriptedAnchorer::new(Mode::Accept);
        let live_records = records_on(&live_path, &clock, &chain, live.clone());
        let mut txs = Vec::new();
        for n in 1..=8u8 {
            let reply = live_records.anchor_for_peer(1, [n; 32]).await.unwrap();
            chain.add_anchor(1, n as u64, [n; 32], live_records.now());
            txs.push(reply.tx_hash);
        }
        drop(live_records);

        let restored_path = db_file("restore-old");
        {
            let old = records_on(&restored_path, &clock, &chain, ScriptedAnchorer::new(Mode::Accept));
            let store = old.store();
            for n in 1..=3u8 {
                store.reserve_peer_anchor(&old.today(), 1, &[n; 32], 8, old.now()).unwrap();
                store.begin_peer_submission(&old.today(), 1, &[n; 32], 3, old.now()).unwrap();
                store.finish_peer_submission(&old.today(), 1, &[n; 32], Ok(&txs[(n - 1) as usize]), old.now()).unwrap();
            }
        }
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&restored_path, &clock, &chain, signer.clone());
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 3, "the backup has regressed");

        // It does not silently carry on with a 5-slot allowance it has forgotten it spent.
        let error = records.anchor_for_peer(1, [20u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("reconciliation_required", 503));
        assert!(error.to_string().contains("peer-reconcile"), "{error}");
        // Not even for a root it believes it only reserved.
        assert_eq!(records.anchor_for_peer(1, [4u8; 32]).await.unwrap().state, "confirmed", "the ledger answers for what it holds");
        assert_eq!(signer.calls(), 0);

        // Reconciliation adopts what the ledger holds (what `peer-reconcile --apply` does).
        for n in 4..=8u8 {
            let day = records.today();
            assert!(records.store().adopt_ledger_anchor(&day, 1, &[n; 32], &txs[(n - 1) as usize], records.now()).unwrap());
        }
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 8, "the allowance is what the ledger says");
        let error = records.anchor_for_peer(1, [20u8; 32]).await.unwrap_err();
        assert_eq!(error.code(), "forbidden", "the day is full, exactly as it was before the restore");
        assert_eq!(signer.calls(), 0, "and nothing was signed on a regressed allowance");
    }

    #[actix_web::test]
    async fn the_restore_hold_stops_all_anchoring_until_it_is_cleared() {
        let path = db_file("hold");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = {
            let clock = clock.clone();
            let mut config = RecordsConfig::new();
            config.peer_anchor_chains = vec![GRID_CHAIN, CHESS_CHAIN];
            config.owner_address = Some(A.to_string());
            config.registry_chain_id = Some(REGISTRY_CHAIN_ID);
            Arc::new(RecordsService::new(
                GamesStore::open(&path).unwrap(),
                RegistryCaps::standard(),
                config,
                chain.clone(),
                Some(signer.clone() as Arc<dyn Anchorer>),
                None,
                Arc::new(move || clock.load(Ordering::SeqCst)),
            ))
        };
        records.store().set_peer_hold("restored from backup 2026-11-10", records.now()).unwrap();
        for n in [1u8, 2] {
            let error = records.anchor_for_peer(1, [n; 32]).await.unwrap_err();
            assert_eq!((error.code(), error.status()), ("reconciliation_required", 503));
        }
        assert_eq!(signer.calls(), 0);
        // The registry's own day is held too: closed and recorded as held, never anchored.
        let (_, key) = records.store().register_game("tetris", "Tetris", A, records.now()).unwrap();
        records.store().set_game_status("tetris", crate::games::registry::GameStatus::Registered, records.now()).unwrap();
        records
            .submit_registry_result("tetris", &key, crate::games::records::RegistryResultRequest {
                player: A.into(), result_id: "match-0001".into(), score: 1, data_hash: None,
            })
            .unwrap();
        clock.store(date_start(&shift_date(DAY, 1).unwrap()).unwrap() + 3_600, Ordering::SeqCst);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_ne!(day.state, "anchored");
        assert!(day.last_error.unwrap().contains("held after a restore"));
        assert_eq!(signer.calls(), 0);
        // Cleared (what a successful `peer-reconcile --clear-hold` does): anchoring resumes.
        records.store().clear_peer_hold().unwrap();
        assert!(records.anchor_for_peer(1, [1u8; 32]).await.is_ok());
        assert_eq!(signer.calls(), 1);
    }

    // --- the registry's own daily anchor, under the same fail-closed rules ---------------------------

    const REGISTRY_CHAIN: u32 = super::REGISTRY_CHAIN_ID;

    /// A registered game with one result on DAY, with the clock moved to the next day so DAY can be closed.
    fn registry_day_ready(records: &RecordsService, clock: &Arc<AtomicU64>) {
        let (_, key) = records.store().register_game("tetris", "Tetris", A, records.now()).unwrap();
        records
            .store()
            .set_game_status("tetris", crate::games::registry::GameStatus::Registered, records.now())
            .unwrap();
        records
            .submit_registry_result("tetris", &key, crate::games::records::RegistryResultRequest {
                player: A.into(), result_id: "match-0001".into(), score: 1, data_hash: None,
            })
            .unwrap();
        clock.store(date_start(&shift_date(DAY, 1).unwrap()).unwrap() + 3_600, Ordering::SeqCst);
    }

    fn registry_root(records: &RecordsService) -> [u8; 32] {
        records.store().registry_day(DAY).unwrap().unwrap().day_root
    }

    /// The review's scenario for the registry: the signer submitted, the node accepted, the process died before
    /// the hash was saved. Returns what a restart needs.
    async fn registry_crashed_after_acceptance(
        name: &str,
    ) -> (std::path::PathBuf, Arc<AtomicU64>, Arc<FakeChain>, u64) {
        let path = db_file(name);
        let (clock, chain) = (fresh_clock(), open_chain());
        let records = records_with(&path, &clock, &chain, Arc::new(HangingAnchorer), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        let begun_at = records.now();
        let killed = tokio::time::timeout(Duration::from_millis(100), records.close_registry_day(DAY)).await;
        assert!(killed.is_err(), "the process died while the signer ran");
        drop(records);
        (path, clock, chain, begun_at)
    }

    #[actix_web::test]
    async fn an_accepted_registry_anchor_with_an_unsaved_hash_and_a_late_index_is_never_submitted_twice() {
        let (path, clock, chain, begun_at) = registry_crashed_after_acceptance("registry-invisible").await;
        // The node accepted it: a pending owner transaction in the attempt's window. The index shows nothing.
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "bb".repeat(32), timestamp: begun_at + 5, pending: true });
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        let root = registry_root(&records);

        for later in [10u64, 3_601, 7_200, 80_000] {
            clock.store(begun_at + later, Ordering::SeqCst);
            // The restart's daily pass tries, as it does every ten minutes.
            let day = records.close_registry_day(DAY).await.unwrap();
            assert_ne!(day.state, "anchored", "after {later}s");
            assert!(day.last_error.as_deref().unwrap_or("").contains("in doubt"), "after {later}s: {:?}", day.last_error);
        }
        assert_eq!(signer.calls(), 0, "the signer was not invoked a second time");

        // Mined, but the anchors index is behind: the owner's confirmed history shows it, the lookup does not.
        chain.owner_txs.lock().unwrap()[0].pending = false;
        chain.index_lag.store(true, Ordering::SeqCst);
        chain.add_anchor(REGISTRY_CHAIN, 1, root, begun_at + 30);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert!(day.last_error.unwrap().contains("in doubt"));
        assert_eq!(signer.calls(), 0, "a late index is not evidence of rejection");

        // The index catches up: the anchor is confirmed, still without a second signer call.
        chain.index_lag.store(false, Ordering::SeqCst);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_eq!(day.state, "confirmed");
        assert_eq!(signer.calls(), 0);
    }

    #[actix_web::test]
    async fn a_submitted_registry_anchor_is_never_resubmitted_whatever_the_time() {
        let path = db_file("registry-submitted");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_eq!((day.state.as_str(), signer.calls()), ("anchored", 1));
        // It is not on the ledger yet. An hour, a day, a month: the old rule would have signed again.
        for later in [3_601u64, 86_400, 30 * 86_400] {
            clock.fetch_add(later, Ordering::SeqCst);
            let day = records.close_registry_day(DAY).await.unwrap();
            assert_eq!(day.state, "anchored");
            assert_eq!(signer.calls(), 1, "no second submission after {later}s");
        }
    }

    #[actix_web::test]
    async fn a_registry_retry_needs_proof_and_every_unprovable_state_stays_in_doubt() {
        let (path, clock, chain, begun_at) = registry_crashed_after_acceptance("registry-proof").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        let after_window = begun_at + 60 + 120 + 120 + 1;
        let try_close = || async { records.close_registry_day(DAY).await.unwrap() };

        clock.store(begun_at + 30, Ordering::SeqCst);
        assert!(try_close().await.last_error.unwrap().contains("in doubt"), "too early");
        clock.store(after_window, Ordering::SeqCst);
        chain.history_incomplete.store(true, Ordering::SeqCst);
        assert!(try_close().await.last_error.unwrap().contains("reconcil"), "history incomplete");
        chain.history_incomplete.store(false, Ordering::SeqCst);
        chain.down.store(true, Ordering::SeqCst);
        assert_ne!(try_close().await.state, "anchored", "node unavailable");
        chain.down.store(false, Ordering::SeqCst);
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "cc".repeat(32), timestamp: begun_at + 20, pending: false });
        assert!(try_close().await.last_error.unwrap().contains("in doubt"), "a transaction in the window");
        assert_eq!(signer.calls(), 0, "no proof, no signer");

        // Outside the window, acknowledged, complete, readable: exactly one retry.
        chain.owner_txs.lock().unwrap()[0].timestamp = begun_at + 5_000;
        clock.store(begun_at + 6_000, Ordering::SeqCst);
        records.store().set_owner_ack(begun_at + 5_500, records.store().recorded_attempts(&[DAY, &shift_date(DAY, 1).unwrap()]).unwrap()).unwrap();
        let day = try_close().await;
        assert_eq!((day.state.as_str(), signer.calls()), ("anchored", 1));
        let day = try_close().await;
        assert_eq!((day.state.as_str(), signer.calls()), ("anchored", 1), "and never again");
    }

    #[actix_web::test]
    async fn a_pending_owner_transaction_blocks_a_registry_submission() {
        let path = db_file("registry-pending");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        // An earlier anchor of the owner's (accounted for by an attempt) is still pending in the node.
        records.anchor_for_peer(1, [8u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "ee".repeat(32), timestamp: records.now(), pending: true });
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_ne!(day.state, "anchored");
        assert!(day.last_error.unwrap().contains("still pending"));
        assert_eq!(signer.calls(), 1, "only the peer anchor was signed");
        chain.owner_txs.lock().unwrap()[0].pending = false;
        assert_eq!(records.close_registry_day(DAY).await.unwrap().state, "anchored");
        assert_eq!(signer.calls(), 2);
    }

    #[actix_web::test]
    async fn the_restore_hold_blocks_the_registry_and_resumes_only_after_it_is_cleared() {
        let path = db_file("registry-hold");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        records.store().set_peer_hold("restored from backup", records.now()).unwrap();
        for _ in 0..3 {
            let day = records.close_registry_day(DAY).await.unwrap();
            assert_ne!(day.state, "anchored");
            assert!(day.last_error.unwrap().contains("held after a restore"));
        }
        assert_eq!(signer.calls(), 0);
        records.store().clear_peer_hold().unwrap();
        assert_eq!(records.close_registry_day(DAY).await.unwrap().state, "anchored");
        assert_eq!(signer.calls(), 1);
    }

    #[actix_web::test]
    async fn an_operator_resolves_a_registry_day_without_resubmission_after_the_claim_is_checked() {
        use crate::games::{peer_usage::PeerResolution, records::validate_landed_transaction};
        let (path, clock, chain, begun_at) = registry_crashed_after_acceptance("registry-resolve").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        let root = registry_root(&records);
        let tx = "ab".repeat(32);
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: tx.clone(), timestamp: begun_at + 5, pending: true });
        let validate = |hash: &str, chain_id: u32, root: [u8; 32], pending_ok: bool| {
            let hash = hash.to_string();
            let chain = chain.clone();
            async move { validate_landed_transaction(chain.as_ref(), A, chain_id, &root, &hash, pending_ok).await }
        };
        // A transaction the node does not have, one signed by someone else, and a pending one without consent.
        assert!(validate(&"00".repeat(32), REGISTRY_CHAIN, root, true).await.unwrap_err().contains("no such transaction"));
        chain.other_txs.lock().unwrap().insert("dd".repeat(32), TxView { sender: B.to_string(), recipient: "appchain".into(), timestamp: begun_at, pending: false });
        assert!(validate(&"dd".repeat(32), REGISTRY_CHAIN, root, true).await.unwrap_err().contains("not by the owner"));
        chain.other_txs.lock().unwrap().insert("ee".repeat(32), TxView { sender: A.to_string(), recipient: "deadbeef".into(), timestamp: begun_at, pending: false });
        assert!(validate(&"ee".repeat(32), REGISTRY_CHAIN, root, true).await.unwrap_err().contains("not an AppChain transaction"));
        assert!(validate(&tx, REGISTRY_CHAIN, root, false).await.unwrap_err().contains("still pending"));
        // With consent, a pending transaction is recorded as a CLAIM, never as verified.
        assert!(matches!(
            validate(&tx, REGISTRY_CHAIN, root, true).await,
            Ok(crate::games::records::LandedCheck::PendingClaim(note)) if note.contains("NOT verified")
        ));
        // Once mined, chain and root are verified against the anchors index: wrong chain, wrong root, wrong tx.
        chain.owner_txs.lock().unwrap()[0].pending = false;
        assert!(validate(&tx, REGISTRY_CHAIN, root, false).await.unwrap_err().contains("does not show"), "mined but not indexed");
        chain.add_anchor(REGISTRY_CHAIN, 1, root, begun_at + 30);
        assert!(validate(&tx, REGISTRY_CHAIN, root, false).await.unwrap_err().contains("different transaction"), "indexed under another tx hash");
        chain.anchors.lock().unwrap().clear();
        chain.anchors.lock().unwrap().push((REGISTRY_CHAIN, AnchorPoint { sequence: 1, root, block_timestamp: begun_at + 30, tx_hash: tx.clone() }));
        assert!(validate(&tx, 1, root, false).await.unwrap_err().contains("does not show"), "wrong chain");
        assert!(validate(&tx, REGISTRY_CHAIN, [9u8; 32], false).await.unwrap_err().contains("does not show"), "wrong root");
        assert!(matches!(
            validate(&tx, REGISTRY_CHAIN, root, false).await,
            Ok(crate::games::records::LandedCheck::Verified(note)) if note.contains("verified: the owner's confirmed anchor")
        ));
        // The consent flag changes nothing for a MINED transaction: it is still verified in full.
        assert!(validate(&tx, REGISTRY_CHAIN, root, true).await.is_ok(), "mined and indexed: verified");
        assert!(validate(&tx, 1, root, true).await.unwrap_err().contains("does not show"), "flag cannot bypass the chain check");
        assert!(validate(&tx, REGISTRY_CHAIN, [9u8; 32], true).await.unwrap_err().contains("does not show"), "flag cannot bypass the root check");
        chain.index_lag.store(true, Ordering::SeqCst);
        assert!(validate(&tx, REGISTRY_CHAIN, root, true).await.unwrap_err().contains("does not show"), "flag cannot bypass the index check");
        chain.index_lag.store(false, Ordering::SeqCst);
        chain.anchors.lock().unwrap().clear();
        chain.anchors.lock().unwrap().push((REGISTRY_CHAIN, AnchorPoint { sequence: 1, root, block_timestamp: begun_at + 30, tx_hash: "ab".repeat(31) + "cd" }));
        assert!(validate(&tx, REGISTRY_CHAIN, root, true).await.unwrap_err().contains("different transaction"), "flag cannot bypass the hash check");
        chain.anchors.lock().unwrap().clear();
        chain.anchors.lock().unwrap().push((REGISTRY_CHAIN, AnchorPoint { sequence: 1, root, block_timestamp: begun_at + 30, tx_hash: tx.clone() }));
        chain.other_txs.lock().unwrap().insert("dd".repeat(32), TxView { sender: B.to_string(), recipient: "appchain".into(), timestamp: begun_at, pending: true });
        assert!(validate(&"dd".repeat(32), REGISTRY_CHAIN, root, true).await.unwrap_err().contains("not by the owner"), "flag cannot bypass the owner check");

        // The verified claim is recorded: the day is answered from it, and nothing is ever signed.
        chain.index_lag.store(true, Ordering::SeqCst);
        records
            .store()
            .resolve_registry_anchor(DAY, &PeerResolution::Landed { tx_hash: tx.clone() }, "found pending, then mined", records.now())
            .unwrap();
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_eq!(day.state, "anchored");
        assert_eq!(day.anchor_tx.as_deref(), Some(tx.as_str()));
        assert_eq!(signer.calls(), 0, "resolved without resubmission");
        // And the other resolution: certain nothing was submitted, so the day may be anchored.
        let path2 = db_file("registry-resolve-not");
        let (clock2, chain2) = (fresh_clock(), open_chain());
        let hang = records_with(&path2, &clock2, &chain2, Arc::new(HangingAnchorer), Some(REGISTRY_CHAIN));
        registry_day_ready(&hang, &clock2);
        let _ = tokio::time::timeout(Duration::from_millis(100), hang.close_registry_day(DAY)).await;
        drop(hang);
        let signer2 = ScriptedAnchorer::new(Mode::Accept);
        let again = records_with(&path2, &clock2, &chain2, signer2.clone(), Some(REGISTRY_CHAIN));
        assert!(again.close_registry_day(DAY).await.unwrap().last_error.unwrap().contains("in doubt"));
        again.store().resolve_registry_anchor(DAY, &PeerResolution::NotSubmitted, "the node never saw it", again.now()).unwrap();
        assert_eq!(again.close_registry_day(DAY).await.unwrap().state, "anchored");
        assert_eq!(signer2.calls(), 1);
    }

    // --- restoring: what is and is not detected ------------------------------------------------------

    /// Take a backup of a live file database and its fence file, the way an operator's backup of the state would.
    fn snapshot(records: &RecordsService, path: &std::path::Path, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let backup = db_file(name);
        records.store().backup_to(&backup).unwrap();
        let fence = backup.with_file_name(format!("{name}.fence"));
        std::fs::copy(format!("{}.fence", path.display()), &fence).unwrap();
        (backup, fence)
    }

    fn restore_db(path: &std::path::Path, backup: &std::path::Path) {
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        std::fs::copy(backup, path).unwrap();
    }

    #[actix_web::test]
    async fn a_restored_database_file_is_detected_by_the_fence_without_anyone_saying_so() {
        let path = db_file("fence-restore");
        let (clock, chain) = (fresh_clock(), open_chain());
        let live = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, live.clone());
        for n in 1..=3u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
            chain.add_anchor(1, n as u64, [n; 32], records.now());
        }
        let (backup, _) = snapshot(&records, &path, "fence-restore-backup");
        for n in 4..=6u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
            // The anchors are mined, but the index is behind: the ledger lookups cannot see them.
            chain.owner_txs.lock().unwrap().push(OwnerTx { hash: format!("{n:02x}").repeat(32), timestamp: records.now(), pending: false });
        }
        for n in 1..=3u8 {
            chain.owner_txs.lock().unwrap().push(OwnerTx { hash: format!("{:02x}", n + 10).repeat(32), timestamp: records.now() - 100, pending: false });
        }
        chain.index_lag.store(true, Ordering::SeqCst);
        drop(records);

        // Only the database file is replaced, as `cp backup records.db` would. The fence file stays.
        restore_db(&path, &backup);
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 3, "the backup has regressed");
        assert!(records.store().peer_hold().unwrap().is_none(), "nobody ran restore-mark");
        let error = records.anchor_for_peer(1, [20u8; 32]).await.unwrap_err();
        assert_eq!((error.code(), error.status()), ("reconciliation_required", 503));
        assert!(error.to_string().contains("restored from a backup"), "{error}");
        assert_eq!(signer.calls(), 0);
        // The hold was set durably by the detection itself: a restart does not forget it.
        drop(records);
        let records = records_on(&path, &clock, &chain, signer.clone());
        assert!(records.store().peer_hold().unwrap().unwrap().starts_with(&format!("{}: automatic", records.now())) || records.store().peer_hold().unwrap().unwrap().contains("automatic"));
        assert_eq!(records.anchor_for_peer(1, [21u8; 32]).await.unwrap_err().code(), "reconciliation_required");
        // The registry day is held too, though nothing told it about the restore.
        assert_eq!(signer.calls(), 0);
    }

    #[actix_web::test]
    async fn restoring_the_whole_state_directory_is_caught_by_the_owner_history_while_the_index_lags() {
        // The reported case: an older database restored WITH its fence file, a lost attempt record, an anchor
        // that is already mined, and the anchors index still behind. Neither the fence nor the ledger lookup can
        // see it. The owner's confirmed history, read from the chain itself, can.
        let path = db_file("whole-dir");
        let (clock, chain) = (fresh_clock(), open_chain());
        let live = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, live.clone());
        for n in 1..=3u8 {
            records.anchor_for_peer(1, [n; 32]).await.unwrap();
            chain.owner_txs.lock().unwrap().push(OwnerTx { hash: format!("{n:02x}").repeat(32), timestamp: records.now(), pending: false });
            chain.add_anchor(1, n as u64, [n; 32], records.now());
        }
        let (backup, backup_fence) = snapshot(&records, &path, "whole-dir-backup");
        // After the backup: one more anchor, mined, its attempt record then lost with the restore.
        records.anchor_for_peer(1, [4u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "44".repeat(32), timestamp: records.now(), pending: false });
        chain.index_lag.store(true, Ordering::SeqCst);
        drop(records);

        // The state directory is restored whole: the database AND its fence file, consistent with each other.
        restore_db(&path, &backup);
        std::fs::copy(&backup_fence, format!("{}.fence", path.display())).unwrap();
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        assert_eq!(records.store().check_fence().unwrap(), crate::games::fence::FenceCheck::Intact, "the fence cannot see this");
        let error = records.anchor_for_peer(1, [30u8; 32]).await.unwrap_err();
        assert_eq!(error.code(), "reconciliation_required", "{error}");
        assert!(error.to_string().contains("signed 4 AppChain transaction(s)"), "{error}");
        assert!(error.to_string().contains("recorded only 3"), "{error}");
        assert_eq!(signer.calls(), 0);
        // Acknowledged by an operator who has reconciled, it resumes: the baseline is the new starting line.
        records.store().set_owner_ack(records.now(), records.store().recorded_attempts(&[&records.today()]).unwrap()).unwrap();
        assert!(records.anchor_for_peer(1, [30u8; 32]).await.is_ok());
        assert_eq!(signer.calls(), 1);
    }

    #[actix_web::test]
    async fn the_owner_history_check_has_a_horizon_and_the_acknowledged_baseline_survives_day_changes() {
        let path = db_file("horizon");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let tx = |label: &str, at: u64| OwnerTx { hash: label.repeat(32), timestamp: at, pending: false };
        // An owner transaction nobody recorded, three days old: beyond the horizon (yesterday 00:00), so it is
        // NOT detected. This is the documented limit of the automatic check.
        chain.owner_txs.lock().unwrap().push(tx("99", records.now() - 3 * 86_400));
        records.anchor_for_peer(1, [1u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(tx("11", records.now())); // the node holds the anchor just signed
        // Yesterday's, unrecorded, is inside the horizon: one transaction more than attempts.
        chain.owner_txs.lock().unwrap().push(tx("98", records.now() - 86_400));
        assert_eq!(records.anchor_for_peer(1, [2u8; 32]).await.unwrap_err().code(), "reconciliation_required");
        assert_eq!(signer.calls(), 1);
        // An operator reconciles and acknowledges. Later attempts are then accounted against the baseline,
        // across a change of day (the window slides: this must not produce a false alarm).
        records.store().set_owner_ack(records.now(), records.store().recorded_attempts(&[&records.today()]).unwrap()).unwrap();
        records.anchor_for_peer(1, [2u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(tx("22", records.now() + 1));
        clock.store(date_start(&shift_date(DAY, 1).unwrap()).unwrap() + 3_600, Ordering::SeqCst);
        records.anchor_for_peer(1, [3u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(tx("33", records.now() + 1));
        assert!(records.anchor_for_peer(1, [3u8; 32]).await.is_ok(), "the same root again is an answer, not a new attempt");
        assert_eq!(signer.calls(), 3);
    }

    #[actix_web::test]
    async fn an_incomplete_confirmed_history_fails_closed_for_peers_and_for_the_registry() {
        let path = db_file("incomplete");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        chain.history_incomplete.store(true, Ordering::SeqCst);
        assert_eq!(records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err().code(), "reconciliation_required");
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_ne!(day.state, "anchored");
        assert_eq!(signer.calls(), 0);
    }

    #[actix_web::test]
    async fn a_registry_day_anchored_days_late_raises_no_false_alarm_for_later_anchors() {
        let path = db_file("registry-late");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        // The registry day for DAY is only anchored three days later (a long outage, or an unset chain).
        clock.fetch_add(3 * 86_400, Ordering::SeqCst);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_eq!((day.state.as_str(), signer.calls()), ("anchored", 1));
        // The node holds it, and the ledger shows it: both are accounted for by the registry's record.
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "12".repeat(32), timestamp: records.now(), pending: false });
        chain.add_anchor(REGISTRY_CHAIN, 1, registry_root(&records), records.now());
        // A peer anchor right after is not refused by the ledger check or the owner-history check.
        assert!(records.anchor_for_peer(1, [40u8; 32]).await.is_ok(), "no false alarm");
        assert_eq!(signer.calls(), 2);
    }

    #[actix_web::test]
    async fn a_registry_anchor_that_fails_ambiguously_is_not_retried_on_a_timer() {
        for mode in [Mode::Ambiguous, Mode::CliFailed] {
            let path = db_file("registry-ambiguous");
            let (clock, chain) = (fresh_clock(), open_chain());
            let signer = ScriptedAnchorer::new(mode);
            let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
            registry_day_ready(&records, &clock);
            let day = records.close_registry_day(DAY).await.unwrap();
            assert_ne!(day.state, "anchored");
            assert_eq!(signer.calls(), 1);
            signer.set(Mode::Accept);
            // The CLI ran and the node may have taken the transaction: an hour, a day, a month later, still no retry.
            // (An owner transaction in the attempt's window keeps it unprovable.)
            chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "af".repeat(32), timestamp: records.now() + 5, pending: false });
            for later in [3_601u64, 86_400, 30 * 86_400] {
                clock.fetch_add(later, Ordering::SeqCst);
                let day = records.close_registry_day(DAY).await.unwrap();
                assert!(day.last_error.unwrap().contains("in doubt"), "after {later}s");
                assert_eq!(signer.calls(), 1, "no second signer call after {later}s");
            }
        }
        // Only "the signer could not be run at all" is definite: that one may be retried at once.
        let path = db_file("registry-definite");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Definite);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        registry_day_ready(&records, &clock);
        assert_ne!(records.close_registry_day(DAY).await.unwrap().state, "anchored");
        signer.set(Mode::Accept);
        assert_eq!(records.close_registry_day(DAY).await.unwrap().state, "anchored");
        assert_eq!(signer.calls(), 2);
    }

    // --- an operator's claim of a PENDING transaction is only a claim ---------------------------------

    use crate::games::peer_usage::{PeerResolution, PeerState};

    async fn crashed_registry_with_pending_owner_tx(
        name: &str,
    ) -> (std::path::PathBuf, Arc<AtomicU64>, Arc<FakeChain>, u64, String) {
        let (path, clock, chain, begun_at) = registry_crashed_after_acceptance(name).await;
        let tx = "ab".repeat(32);
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: tx.clone(), timestamp: begun_at + 5, pending: true });
        (path, clock, chain, begun_at, tx)
    }

    #[actix_web::test]
    async fn a_pending_claim_never_marks_a_registry_day_landed_and_survives_a_restart() {
        let (path, clock, chain, begun_at, tx) = crashed_registry_with_pending_owner_tx("claim-registry").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        records.store().resolve_registry_anchor(DAY, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "pending on the node", records.now()).unwrap();
        let record = records.store().registry_anchor(DAY).unwrap().unwrap();
        assert_eq!((record.state, record.tx_hash.as_deref()), (PeerState::Submitting, Some(tx.as_str())), "a claim is not a landing");
        // Over time, and across restarts, it stays unresolved: not anchored, never resubmitted, reason given.
        drop(records);
        for later in [10u64, 3_601, 86_400] {
            clock.store(begun_at + later, Ordering::SeqCst);
            let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
            let day = records.close_registry_day(DAY).await.unwrap();
            assert_eq!(day.state, "closed", "the day is NOT marked anchored by a claim (after {later}s)");
            let error = day.last_error.unwrap();
            assert!(error.contains("not verified") && error.contains("still pending"), "{error}");
            let record = records.store().registry_anchor(DAY).unwrap().unwrap();
            assert_eq!((record.state, record.tx_hash.as_deref()), (PeerState::Submitting, Some(tx.as_str())), "the unresolved state persists");
        }
        assert_eq!(signer.calls(), 0, "no signer call while the claim is unresolved");
    }

    #[actix_web::test]
    async fn a_claimed_transaction_that_is_not_this_anchor_cannot_suppress_it_forever_and_the_operator_can_clear_it() {
        let (path, clock, chain, begun_at, tx) = crashed_registry_with_pending_owner_tx("claim-other").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        records.store().resolve_registry_anchor(DAY, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "claimed", records.now()).unwrap();
        // The claimed transaction is mined, but it is some other owner transaction: the index never shows this root.
        chain.owner_txs.lock().unwrap()[0].pending = false;
        clock.store(begun_at + 5_000, Ordering::SeqCst);
        let day = records.close_registry_day(DAY).await.unwrap();
        assert_eq!(day.state, "closed");
        assert!(day.last_error.unwrap().contains("does not show this root"), "visible, with the reason");
        assert_eq!(signer.calls(), 0);
        // The operator, having looked, clears it: the stale hash must go, or a later ambiguity would read it as a claim.
        records.store().resolve_registry_anchor(DAY, &PeerResolution::NotSubmitted, "it was a registration", records.now()).unwrap();
        let record = records.store().registry_anchor(DAY).unwrap().unwrap();
        assert_eq!((record.state, record.tx_hash), (PeerState::Reserved, None));
        // Acknowledge the owner's unrelated transaction, as the procedure says, and the day can be anchored.
        records.store().set_owner_ack(begun_at + 5_500, records.store().recorded_attempts(&[DAY, &shift_date(DAY, 1).unwrap()]).unwrap()).unwrap();
        clock.store(begun_at + 6_000, Ordering::SeqCst);
        assert_eq!(records.close_registry_day(DAY).await.unwrap().state, "anchored");
        assert_eq!(signer.calls(), 1);
    }

    #[actix_web::test]
    async fn a_claim_for_a_transaction_the_node_lost_or_that_is_not_the_owners_stays_in_doubt() {
        let (path, clock, chain, begun_at, tx) = crashed_registry_with_pending_owner_tx("claim-lost").await;
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_with(&path, &clock, &chain, signer.clone(), Some(REGISTRY_CHAIN));
        records.store().resolve_registry_anchor(DAY, &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "claimed", records.now()).unwrap();
        clock.store(begun_at + 5_000, Ordering::SeqCst);
        // The pending transaction vanished from the node (dropped): in doubt, never a retry on its own.
        chain.owner_txs.lock().unwrap().clear();
        let day = records.close_registry_day(DAY).await.unwrap();
        assert!(day.last_error.unwrap().contains("does not know that transaction"));
        // It turns out to be somebody else's transaction.
        chain.other_txs.lock().unwrap().insert(tx.clone(), TxView { sender: B.to_string(), recipient: "appchain".into(), timestamp: begun_at, pending: false });
        let day = records.close_registry_day(DAY).await.unwrap();
        assert!(day.last_error.unwrap().contains("not an AppChain transaction signed by the owner"));
        assert_eq!(signer.calls(), 0);
    }

    #[actix_web::test]
    async fn a_peer_root_claim_is_also_only_a_claim_and_one_transaction_cannot_anchor_two_roots() {
        let path = db_file("claim-peer");
        let (clock, chain) = (fresh_clock(), open_chain());
        let records = records_on(&path, &clock, &chain, Arc::new(HangingAnchorer));
        let begun_at = records.now();
        let _ = tokio::time::timeout(Duration::from_millis(100), records.anchor_for_peer(1, [5u8; 32])).await;
        let _ = tokio::time::timeout(Duration::from_millis(100), records.anchor_for_peer(1, [6u8; 32])).await;
        drop(records);
        let tx = "ab".repeat(32);
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: tx.clone(), timestamp: begun_at + 5, pending: true });
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "cd".repeat(32), timestamp: begun_at + 6, pending: true });
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        let day = records.today();
        records.store().resolve_peer_root(&day, 1, &[5u8; 32], &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "claimed", records.now()).unwrap();
        // The same hash cannot be named for a second root, nor for the registry.
        assert!(records.store().resolve_peer_root(&day, 1, &[6u8; 32], &PeerResolution::PendingClaim { tx_hash: tx.clone() }, "again", records.now()).is_err());
        assert!(records.store().resolve_peer_root(&day, 1, &[6u8; 32], &PeerResolution::Landed { tx_hash: tx.clone() }, "again", records.now()).is_err());
        let row = records.store().peer_anchor_roots(&day).unwrap().into_iter().find(|r| r.root == [5u8; 32]).unwrap();
        assert_eq!((row.state, row.tx_hash.as_deref()), (PeerState::Submitting, Some(tx.as_str())), "still only a claim");
        // Asked again, the claimed root stays in doubt with the reason (and nothing is signed), at any later time.
        for later in [10u64, 7_200, 86_400 - 4_000] {
            clock.store(begun_at + later, Ordering::SeqCst);
            let error = records.anchor_for_peer(1, [5u8; 32]).await.unwrap_err();
            assert!(error.to_string().contains("not verified") && error.to_string().contains("still pending"), "{error}");
        }
        assert_eq!(signer.calls(), 0);
        // Mined and shown by the index under the claimed hash: the ledger itself confirms it.
        chain.owner_txs.lock().unwrap()[0].pending = false;
        chain.add_anchor(1, 1, [5u8; 32], begun_at + 30);
        chain.anchors.lock().unwrap()[0].1.tx_hash = tx.clone();
        assert_eq!(records.anchor_for_peer(1, [5u8; 32]).await.unwrap().state, "confirmed");
        assert_eq!(signer.calls(), 0);
    }

    // --- the restore documentation says exactly what the implementation does -----------------------------

    #[test]
    fn the_documentation_does_not_claim_more_restore_protection_than_exists() {
        let runbook = include_str!("../../docs/runbooks/records-service.md");
        let checklist = include_str!("../../docs/runbooks/games-go-live-checklist.md");
        let architecture = include_str!("../../docs/APPCHAIN_SERVICE_ARCHITECTURE.md");
        // The limits are stated.
        for needed in [
            "Restoring the whole state directory",
            "invisible",
            "since 00:00 UTC yesterday",
            "not the plan",
            "hard operational prerequisite",
        ] {
            assert!(runbook.contains(needed), "the runbook must say: {needed}");
        }
        // The mandatory procedure is in the deployment checklist, command by command.
        for command in ["restore-mark", "peer-reconcile --apply", "--clear-hold", "records.db-wal"] {
            assert!(checklist.contains(command), "the checklist must contain the restore step: {command}");
        }
        assert!(checklist.contains("mandatory"), "the checklist marks the restore procedure as mandatory");
        // And no document promises complete detection.
        for (name, text) in [("runbook", runbook), ("checklist", checklist), ("architecture", architecture)] {
            let lower = text.to_lowercase();
            for forbidden in [
                "complete automatic detection",
                "detects every restore",
                "all restores are detected",
                "cannot be restored without",
                "restore is always detected",
                "fully protected against restore",
            ] {
                assert!(!lower.contains(forbidden), "{name} must not claim: {forbidden}");
            }
        }
        assert!(architecture.contains("Neither is claimed to be complete"));
    }

    // --- pending transactions the node hides are counted by the nonce, because the node accepts duplicates ----

    #[actix_web::test]
    async fn a_pending_transaction_the_nodes_list_hides_is_still_detected_and_blocks_a_submission() {
        let path = db_file("hidden-pending");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        // One anchor is made and mined; the owner's history is now "full", so pending ones are left out of it.
        records.anchor_for_peer(1, [1u8; 32]).await.unwrap();
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "a1".repeat(32), timestamp: records.now(), pending: false });
        chain.hide_pending.store(true, Ordering::SeqCst);
        // Nothing is pending (account nonce == the next confirmed nonce): a submission goes ahead.
        records.anchor_for_peer(1, [2u8; 32]).await.unwrap();
        // That second anchor is accepted and pending, and the node's list does not show it.
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "a2".repeat(32), timestamp: records.now(), pending: true });
        let error = records.anchor_for_peer(1, [3u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("still pending"), "{error}");
        assert_eq!(signer.calls(), 2, "nothing was signed on top of a pending transaction the list hid");
        assert_eq!(records.store().peer_anchor_usage(&records.today()).unwrap(), 3, "the slot is held; no attempt is counted");
        // Mined: the nonce and the newest confirmed transaction agree again, and the submission goes ahead.
        chain.owner_txs.lock().unwrap()[1].pending = false;
        assert!(records.anchor_for_peer(1, [3u8; 32]).await.is_ok());
        assert_eq!(signer.calls(), 3);
    }

    #[actix_web::test]
    async fn when_pending_transactions_cannot_be_counted_at_all_nothing_is_submitted() {
        let path = db_file("hidden-unknown");
        let (clock, chain) = (fresh_clock(), open_chain());
        let signer = ScriptedAnchorer::new(Mode::Accept);
        let records = records_on(&path, &clock, &chain, signer.clone());
        // The list hides pending ones and shows no confirmed transaction of the owner's to measure against.
        chain.hide_pending.store(true, Ordering::SeqCst);
        let error = records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("cannot show whether the owner has a pending transaction"), "{error}");
        // The node's answers disagree (the account nonce is behind its newest confirmed transaction).
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "b1".repeat(32), timestamp: records.now(), pending: false });
        chain.owner_txs.lock().unwrap().push(OwnerTx { hash: "b2".repeat(32), timestamp: records.now(), pending: false });
        *chain.nonce_override.lock().unwrap() = Some(1);
        records.store().set_owner_ack(records.now(), 0).unwrap();
        let error = records.anchor_for_peer(1, [1u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("disagree"), "{error}");
        // The account cannot be read at all.
        *chain.nonce_override.lock().unwrap() = None;
        chain.down.store(true, Ordering::SeqCst);
        assert!(records.anchor_for_peer(1, [1u8; 32]).await.is_err());
        assert_eq!(signer.calls(), 0, "no submission on any unknown");
    }
}

/// `/api/records/v1`: the public read contract. These pin the envelope and its headers, the 404 for
/// every method that is not GET or HEAD, the day-state counters (by state alone, over one window) and
/// the day list's date cursor.
mod v1_api {
    use super::*;
    use actix_web::{
        dev::ServiceResponse,
        http::{Method, StatusCode},
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::{json, Value};

    macro_rules! app {
        ($harness:expr) => {
            init_service(
                App::new()
                    .app_data(web::Data::from($harness.records.clone()))
                    .configure(crate::games::records_api::configure_records_routes),
            )
            .await
        };
    }

    fn header(response: &ServiceResponse, name: &str) -> Option<String> {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    }

    fn close(harness: &Harness, days: &[&str]) {
        for day in days {
            harness
                .records
                .store()
                .close_registry_day(day, harness.records.now())
                .unwrap();
        }
    }

    #[actix_web::test]
    async fn v1_responses_carry_the_envelope_headers_and_an_id() {
        let h = harness(Options::default());
        let app = app!(h);
        let response = call_service(
            &app,
            TestRequest::get()
                .uri("/api/records/v1/status")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header(&response, "cache-control").as_deref(), Some("no-store"));
        assert_eq!(header(&response, "etag"), None, "this phase never emits a validator");
        let id = header(&response, "x-request-id").expect("an id on every response");
        assert_eq!(id.len(), 32, "a generated id is 32 hex characters: {id}");
        let body: Value = read_body_json(response).await;
        assert_eq!(body["success"], json!(true));
        assert_eq!(body["schema_version"], json!(1));
        assert_eq!(body["served_chain_ids"], json!([REGISTRY_CHAIN_ID]));
        assert_eq!(body["request_id"], json!(id));
        assert!(body["as_of"].as_str().unwrap().ends_with('Z'));
        assert_eq!(body["service"]["accepting"], json!(true));
    }

    #[actix_web::test]
    async fn a_client_request_id_is_echoed_only_when_it_is_well_formed() {
        let h = harness(Options::default());
        let app = app!(h);
        let echoed = "client-supplied-1234";
        let response = call_service(
            &app,
            TestRequest::get()
                .uri("/api/records/v1/status")
                .insert_header(("X-Request-Id", echoed))
                .to_request(),
        )
        .await;
        assert_eq!(header(&response, "x-request-id").as_deref(), Some(echoed));

        let response = call_service(
            &app,
            TestRequest::get()
                .uri("/api/records/v1/status")
                .insert_header(("X-Request-Id", "not an id"))
                .to_request(),
        )
        .await;
        let replaced = header(&response, "x-request-id").unwrap();
        assert_ne!(replaced, "not an id");
        assert_eq!(replaced.len(), 32);
    }

    #[actix_web::test]
    async fn an_unserved_chain_is_404_and_never_a_fallback() {
        let h = harness(Options::default());
        let app = app!(h);
        let response = call_service(
            &app,
            TestRequest::get()
                .uri("/api/records/v1/chains/9/status")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body: Value = read_body_json(response).await;
        assert_eq!(body["error"]["code"], json!("unknown_chain"));
        assert_eq!(body["error"]["retryable"], json!(false));

        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/status"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = read_body_json(response).await;
        assert_eq!(body["chain_id"], json!(REGISTRY_CHAIN_ID));
        assert_eq!(body["window"]["days_total"], json!(0));
        assert_eq!(body["commitment"]["days_anchored"], json!(0));
        assert_eq!(
            body["commitment"]["confirmation"]["finality_checked"],
            json!(false)
        );
        assert_eq!(
            body["commitment"]["confirmation"]["scanned_anchors"],
            json!(150)
        );
    }

    #[actix_web::test]
    async fn no_method_other_than_get_or_head_is_served() {
        let h = harness(Options::default());
        let app = app!(h);
        for method in [
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ] {
            let response = call_service(
                &app,
                TestRequest::default()
                    .method(method.clone())
                    .uri("/api/records/v1/status")
                    .to_request(),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{method} must be 404, not actix's 405"
            );
            assert_eq!(
                header(&response, "cache-control").as_deref(),
                Some("no-store"),
                "{method}"
            );
            let body: Value = read_body_json(response).await;
            assert_eq!(body["error"]["code"], json!("not_found"), "{method}");
        }
        let response = call_service(
            &app,
            TestRequest::default()
                .method(Method::HEAD)
                .uri("/api/records/v1/status")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(header(&response, "x-request-id").is_some());
    }

    #[actix_web::test]
    async fn the_day_list_is_newest_first_and_cursor_bounded() {
        let h = harness(Options::default());
        close(&h, &["2026-11-13", "2026-11-14", "2026-11-15"]);
        let app = app!(h);
        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days?limit=2"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = read_body_json(response).await;
        assert_eq!(body["days"].as_array().unwrap().len(), 2);
        assert_eq!(body["days"][0]["date"], json!("2026-11-15"));
        assert_eq!(body["days"][1]["date"], json!("2026-11-14"));
        assert_eq!(body["has_more"], json!(true));
        assert_eq!(body["next_before"], json!("2026-11-14"));

        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days?limit=2&before=2026-11-14"))
                .to_request(),
        )
        .await;
        let body: Value = read_body_json(response).await;
        assert_eq!(body["days"].as_array().unwrap().len(), 1);
        assert_eq!(body["days"][0]["date"], json!("2026-11-13"));
        assert_eq!(body["has_more"], json!(false));
        assert_eq!(body["next_before"], Value::Null);

        for query in ["limit=0", "limit=101", "before=16-11-2026"] {
            let uri = format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days?{query}");
            let response = call_service(&app, TestRequest::get().uri(&uri).to_request()).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
            let body: Value = read_body_json(response).await;
            assert_eq!(body["error"]["code"], json!("invalid_request"), "{query}");
        }
    }

    #[actix_web::test]
    async fn counters_count_by_state_alone_and_keep_the_invariant() {
        let h = harness(Options::default());
        close(&h, &["2026-11-13", "2026-11-14", "2026-11-15"]);
        let tx = "a".repeat(64);
        h.records
            .store()
            .set_registry_anchor("2026-11-13", Some(&tx), "anchored", h.records.now())
            .unwrap();
        h.records
            .store()
            .set_registry_anchor("2026-11-14", Some(&tx), "confirmed", h.records.now())
            .unwrap();
        let app = app!(h);
        let uri = &format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/status");
        let response = call_service(&app, TestRequest::get().uri(uri).to_request()).await;
        let before: Value = read_body_json(response).await;
        let commitment = before["commitment"].clone();
        assert_eq!(commitment["days_closed"], json!(3));
        assert_eq!(commitment["days_with_root"], json!(3));
        assert_eq!(commitment["days_anchored"], json!(2));
        assert_eq!(commitment["days_pending_anchor"], json!(1));
        assert_eq!(commitment["days_confirmed"], json!(1));
        assert_eq!(commitment["latest_anchored_day"], json!("2026-11-14"));
        assert_eq!(
            commitment["days_anchored"].as_u64().unwrap(),
            commitment["days_pending_anchor"].as_u64().unwrap()
                + commitment["days_confirmed"].as_u64().unwrap(),
        );
        assert_eq!(before["window"]["from_day"], json!("2026-11-13"));
        assert_eq!(before["window"]["to_day"], json!("2026-11-15"));
        assert_eq!(before["window"]["days_total"], json!(3));

        // A transaction hash on a day that is still `closed` must move no counter: the state is the
        // only test, never the presence of `anchor_tx`.
        h.records
            .store()
            .set_registry_anchor("2026-11-15", Some(&tx), "closed", h.records.now())
            .unwrap();
        let response = call_service(&app, TestRequest::get().uri(uri).to_request()).await;
        let after: Value = read_body_json(response).await;
        assert_eq!(after["commitment"], commitment);
    }

    #[actix_web::test]
    async fn day_reads_use_the_apps_vocabulary_and_never_expose_games() {
        let h = harness(Options::default());
        close(&h, &["2026-11-15"]);
        let app = app!(h);
        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days/2026-11-15"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = read_body_json(response).await;
        assert_eq!(body["date"], json!("2026-11-15"));
        assert_eq!(body["state"], json!("closed"));
        assert_eq!(body["chain_id"], json!(REGISTRY_CHAIN_ID));
        assert_eq!(body["apps"], json!([]));
        assert!(
            body["games"].is_null(),
            "the v1 vocabulary has no `games`: {}",
            body
        );

        // An unclosed day and an app with no results that day are both 404, never an empty root.
        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days/2026-11-16"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = call_service(
            &app,
            TestRequest::get()
                .uri(&format!("/api/records/v1/chains/{REGISTRY_CHAIN_ID}/days/2026-11-15/apps/chess/proof"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body: Value = read_body_json(response).await;
        assert_eq!(body["error"]["code"], json!("not_found"));
    }
}

// --- inspecting a persisted day, and refusing to start on the wrong chains ----------------------

mod closed_day {
    use super::*;
    use crate::games::service::DayView;
    use crate::games::store::DayRow;

    const PREV: [u8; 32] = [0x44; 32];

    /// A real Grid day: a win is recorded on DAY, the day passes, and it is closed through the service.
    async fn closed_grid_day(h: &Harness) {
        let midnight = date_start(DAY).unwrap();
        h.chain.add_anchor(1, 4, PREV, midnight - 100);
        assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
        h.next_day();
        h.service.close_and_anchor("grid", DAY).await.unwrap();
    }

    fn row(state: &str) -> DayRow {
        DayRow {
            day: DAY.into(),
            game: "grid".into(),
            prev_root: Some(vec![1; 32]),
            prev_anchor_sequence: Some(7),
            seed: Some(vec![2; 32]),
            commitment: Some(vec![3; 32]),
            results_root: Some(vec![4; 32]),
            day_root: Some(vec![5; 32]),
            state: state.into(),
            ..DayRow::default()
        }
    }

    #[tokio::test]
    async fn a_closed_day_is_shown_exactly_as_it_was_persisted_and_chained_to_the_earlier_anchor() {
        let h = harness(Options::default());
        closed_grid_day(&h).await;

        let view = h.service.day_view("grid", DAY).unwrap();
        let stored = h.service.store().day_row(DAY, "grid").unwrap().unwrap();
        assert_eq!((view.day.as_str(), view.game.as_str(), view.chain_id), (DAY, "grid", 1));
        assert_eq!(view.day_root, stored.day_root.as_ref().map(hex::encode));
        assert_eq!(view.results_root, stored.results_root.as_ref().map(hex::encode));
        assert_eq!(view.commitment, stored.commitment.as_ref().map(hex::encode));
        assert_ne!(view.state, "open");
        // The root really is the documented function of the persisted parts, not a stored guess.
        let commitment: [u8; 32] = stored.commitment.clone().unwrap().try_into().unwrap();
        let results_root: [u8; 32] = stored.results_root.clone().unwrap().try_into().unwrap();
        assert_eq!(
            view.day_root.as_deref(),
            Some(hex::encode(roots::grid_day_root(DAY, &commitment, &results_root)).as_str())
        );
        // Previous root and sequence are the anchor that was on the ledger before the day began.
        assert_eq!(view.prev_root.as_deref(), Some("44".repeat(32).as_str()));
        assert_eq!(view.prev_anchor_seq, Some(4));
    }

    #[tokio::test]
    async fn a_day_with_no_earlier_anchor_shows_the_zero_root_and_no_sequence() {
        let h = harness(Options::default());
        assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
        h.next_day();
        h.service.close_and_anchor("grid", DAY).await.unwrap();
        let view = h.service.day_view("grid", DAY).unwrap();
        assert_eq!(view.prev_root.as_deref(), Some("00".repeat(32).as_str()));
        assert_eq!(view.prev_anchor_seq, None);
    }

    #[tokio::test]
    async fn an_open_day_is_open_and_carries_no_root() {
        let h = harness(Options::default());
        // Reading today's puzzles is what opens the day.
        h.service.grid_day(&h.service.today()).await.unwrap();
        let view = h.service.day_view("grid", DAY).unwrap();
        assert_eq!(view.state, "open");
        assert_eq!((view.day_root, view.results_root), (None, None));
        assert!(!view.anchor_failed);
        assert!(view.commitment.is_some(), "the published puzzle commitment is persisted from the start");
    }

    #[tokio::test]
    async fn an_unknown_day_or_game_is_not_found_a_bad_date_is_invalid_and_reading_writes_nothing() {
        let h = harness(Options::default());
        assert!(matches!(h.service.day_view("grid", "2026-01-01"), Err(GamesError::NotFound(_))));
        assert!(matches!(h.service.day_view("poker", DAY), Err(GamesError::NotFound(_))));
        assert!(matches!(h.service.day_view("grid", "not-a-date"), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.day_view("grid", "2026-13-40"), Err(GamesError::Invalid(_))));
        assert!(h.service.store().day_row("2026-01-01", "grid").unwrap().is_none(), "a read must not open a day");
    }

    #[test]
    fn a_failed_anchor_is_visible_without_its_message_and_a_confirmed_day_is_not_failed() {
        let mut failed = row("closing");
        failed.last_error = Some("secret operator detail".into());
        let view = DayView::from_row(1, failed).unwrap();
        assert_eq!(view.state, "closed");
        assert!(view.anchor_failed);
        assert!(!serde_json::to_string(&view).unwrap().contains("secret"));

        let mut confirmed = row("confirmed");
        confirmed.last_error = Some("old".into());
        confirmed.anchor_tx = Some("ab".repeat(32));
        let view = DayView::from_row(1, confirmed).unwrap();
        assert_eq!(view.state, "confirmed");
        assert!(!view.anchor_failed);
        assert_eq!(view.anchor_tx.as_deref(), Some("ab".repeat(32).as_str()));
        assert_eq!(DayView::from_row(1, row("anchored")).unwrap().state, "anchored");
    }

    #[test]
    fn a_damaged_row_is_an_internal_error_and_never_a_partial_answer() {
        let internal = |row: DayRow| matches!(DayView::from_row(1, row), Err(GamesError::Internal(_)));
        // A root of the wrong length, in each place one can occur.
        for field in ["day_root", "results_root", "prev_root", "commitment"] {
            let mut damaged = row("closing");
            let short = Some(vec![9; 31]);
            match field {
                "day_root" => damaged.day_root = short,
                "results_root" => damaged.results_root = short,
                "prev_root" => damaged.prev_root = short,
                _ => damaged.commitment = short,
            }
            assert!(internal(damaged), "{field} of 31 bytes must be refused");
        }
        // Closed but a root is missing.
        let mut no_day_root = row("closing");
        no_day_root.day_root = None;
        assert!(internal(no_day_root));
        let mut no_results_root = row("anchored");
        no_results_root.results_root = None;
        assert!(internal(no_results_root));
        // Open but already carrying a root.
        assert!(internal(row("open")));
        // A closed Grid day with no commitment, and a state this code does not know.
        let mut no_commitment = row("closing");
        no_commitment.commitment = None;
        assert!(internal(no_commitment));
        assert!(internal(row("sealed")));
        // The internal detail is for the log: the client-facing message hides it.
        let response = crate::games::api::error_response(&GamesError::Internal("stored day root is malformed".into()));
        assert_eq!(response.status().as_u16(), 500);
    }

    #[test]
    fn a_chess_day_has_no_previous_anchor_fields_and_still_shows_its_roots() {
        let mut chess = row("closing");
        chess.game = "chess".into();
        chess.prev_root = None;
        chess.prev_anchor_sequence = None;
        chess.commitment = None;
        let view = DayView::from_row(3, chess).unwrap();
        assert_eq!((view.chain_id, view.prev_root, view.prev_anchor_seq, view.commitment), (3, None, None, None));
        assert!(view.day_root.is_some() && view.results_root.is_some());
    }
}

mod chain_identity {
    use super::*;

    #[tokio::test]
    async fn every_configured_chain_registered_lets_the_service_start() {
        let h = harness(Options::default());
        h.service.verify_game_chains().await.unwrap();
        *h.chain.registered.lock().unwrap() = Some([1, 3].into_iter().collect());
        h.service.verify_game_chains().await.unwrap();
    }

    #[tokio::test]
    async fn a_chess_chain_the_ledger_does_not_know_stops_the_start_and_is_never_replaced() {
        let h = harness(Options::default());
        // The ledger today: UltraGrid #1 and the Registry #2, and chess is configured as #3 but is not there.
        *h.chain.registered.lock().unwrap() = Some([1, 2].into_iter().collect());
        let error = h.service.verify_game_chains().await.unwrap_err();
        match error {
            GamesError::Invalid(message) => {
                assert!(message.contains("chess") && message.contains("#3"), "{message}");
            }
            other => panic!("expected a definite refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unreachable_ledger_is_reported_as_unavailable_not_as_a_missing_chain() {
        let h = harness(Options::default());
        h.chain.down.store(true, Ordering::SeqCst);
        assert!(matches!(
            h.service.verify_game_chains().await,
            Err(GamesError::ChainUnavailable)
        ));
    }
}

mod closed_day_http {
    use super::*;
    use crate::games::api;
    use actix_web::{
        http::StatusCode,
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::Value;

    async fn get(h: &Harness, uri: &str) -> (StatusCode, Value, Option<String>) {
        let app = init_service(
            App::new()
                .app_data(web::Data::from(h.service.clone()))
                .configure(api::configure_routes),
        )
        .await;
        let response = call_service(&app, TestRequest::get().uri(uri).to_request()).await;
        let status = response.status();
        let cache = response
            .headers()
            .get("Cache-Control")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        (status, read_body_json(response).await, cache)
    }

    #[actix_web::test]
    async fn the_route_returns_a_closed_day_and_only_its_public_fields() {
        let h = harness(Options::default());
        let midnight = date_start(DAY).unwrap();
        h.chain.add_anchor(1, 4, [0x44; 32], midnight - 100);
        assert_eq!(h.solve(A, 0).await.outcome, Some("rewarded"));
        h.next_day();
        h.service.close_and_anchor("grid", DAY).await.unwrap();

        let (status, body, cache) = get(&h, &format!("/api/games/days/grid/{DAY}")).await;
        assert_eq!(status, 200);
        assert_eq!(cache.as_deref(), Some("no-store"));
        assert_eq!(body["success"], true);
        assert_eq!(body["day"], DAY);
        assert_eq!(body["game"], "grid");
        assert_eq!(body["chain_id"], 1);
        assert_eq!(body["prev_root"], "44".repeat(32));
        assert_eq!(body["prev_anchor_seq"], 4);
        let stored = h.service.store().day_row(DAY, "grid").unwrap().unwrap();
        assert_eq!(body["day_root"], hex::encode(stored.day_root.unwrap()));
        assert_eq!(body["results_root"], hex::encode(stored.results_root.unwrap()));
        assert_eq!(body["commitment"], hex::encode(stored.commitment.unwrap()));
        // Nothing beyond the day itself: no seed, no error text, no operator data.
        let mut keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "anchor_failed", "anchor_tx", "chain_id", "commitment", "day", "day_root", "game",
                "prev_anchor_seq", "prev_root", "results_root", "state", "success"
            ]
        );
    }

    #[actix_web::test]
    async fn an_unknown_day_is_a_404_a_bad_date_a_400_and_neither_writes_a_day() {
        let h = harness(Options::default());
        let (status, body, _) = get(&h, "/api/games/days/grid/2026-01-01").await;
        assert_eq!((status.as_u16(), body["code"].as_str()), (404, Some("not_found")));
        let (status, body, _) = get(&h, "/api/games/days/poker/2026-11-16").await;
        assert_eq!((status.as_u16(), body["code"].as_str()), (404, Some("not_found")));
        let (status, body, _) = get(&h, "/api/games/days/grid/yesterday").await;
        assert_eq!((status.as_u16(), body["code"].as_str()), (400, Some("invalid_request")));
        assert!(h.service.store().day_row("2026-01-01", "grid").unwrap().is_none());
    }

    #[actix_web::test]
    async fn an_open_day_is_reported_open_over_http() {
        let h = harness(Options::default());
        h.service.grid_day(&h.service.today()).await.unwrap();
        let (status, body, _) = get(&h, &format!("/api/games/days/grid/{DAY}")).await;
        assert_eq!(status, 200);
        assert_eq!(body["state"], "open");
        assert!(body["day_root"].is_null() && body["results_root"].is_null());
    }
}

mod chain_collisions {
    use super::*;

    #[test]
    fn the_fixture_chains_are_three_different_chains() {
        let chains = [GRID_CHAIN, REGISTRY_CHAIN_ID, CHESS_CHAIN];
        assert_eq!(chains.iter().collect::<HashSet<_>>().len(), 3);
        assert_eq!(chains, [1, 2, 3], "the arrangement these fixtures stand for: Grid, Registry, Chess");
    }

    #[tokio::test]
    async fn a_peer_can_have_grid_and_chess_anchored_but_never_the_registry() {
        let h = harness(Options::default());
        for chain in [GRID_CHAIN, CHESS_CHAIN] {
            h.records.anchor_for_peer(chain, [0x11; 32]).await.unwrap_or_else(|e| panic!("chain {chain}: {e}"));
        }
        let error = h.records.anchor_for_peer(REGISTRY_CHAIN_ID, [0x22; 32]).await.unwrap_err();
        assert_eq!(error.code(), "forbidden");
        assert!(
            h.records_anchorer.calls.lock().unwrap().iter().all(|(chain, _)| *chain != REGISTRY_CHAIN_ID),
            "nothing was signed for the Registry's chain on a peer's behalf"
        );
    }

    #[tokio::test]
    async fn even_a_peer_list_that_wrongly_names_the_registry_cannot_reach_it() {
        // A configuration slip (the Registry id added to the peer list) must not become a path to
        // anchor the Registry: the service checks the registry id itself, whatever the list says.
        let mut config = RecordsConfig::new();
        config.peer_anchor_chains = vec![GRID_CHAIN, REGISTRY_CHAIN_ID, CHESS_CHAIN];
        config.registry_chain_id = Some(REGISTRY_CHAIN_ID);
        config.owner_address = Some(A.to_string());
        let chain = Arc::new(FakeChain::default());
        let anchorer = Arc::new(FakeAnchorer::default());
        let records = RecordsService::new(
            GamesStore::open(Path::new(":memory:")).unwrap(),
            RegistryCaps::standard(),
            config,
            chain,
            Some(anchorer.clone()),
            None,
            Arc::new(|| date_start(DAY).unwrap() + 3_600),
        );
        let error = records.anchor_for_peer(REGISTRY_CHAIN_ID, [0x33; 32]).await.unwrap_err();
        assert_eq!(error.code(), "forbidden");
        assert!(anchorer.calls.lock().unwrap().is_empty());
    }
}

// --- wallet sign-in ----------------------------------------------------------------------------

mod wallet_auth {
    use super::*;
    use crate::games::auth::{self, AuthConfig, AuthError, PURPOSE_PLAY};
    use crate::games::service::SessionView;

    fn wallet() -> (QuantumKeyPair, String) {
        let keys = QuantumKeyPair::generate();
        let address = QuantumKeyPair::address_from_public_key(&keys.public_key);
        (keys, address)
    }

    /// What a real wallet does: recompute the digest from the challenge's own fields (never trusting the
    /// server's `digest`), sign it locally, and send the public key and signature.
    fn signed(h: &Harness, keys: &QuantumKeyPair, address: &str) -> (String, String, String) {
        let challenge = h.service.auth_challenge(address, PURPOSE_PLAY).unwrap();
        let mine = auth::challenge_digest(
            &challenge.purpose,
            &challenge.address,
            &challenge.challenge_id,
            challenge.issued_at,
            challenge.expires_at,
        )
        .unwrap();
        assert_eq!(challenge.digest, hex::encode(mine), "the digest the server shows is the documented one");
        (
            challenge.challenge_id,
            hex::encode(&keys.public_key),
            hex::encode(keys.sign(&mine)),
        )
    }

    fn sign_in(h: &Harness, keys: &QuantumKeyPair, address: &str) -> SessionView {
        let (id, public_key, signature) = signed(h, keys, address);
        h.service.auth_verify(&id, &public_key, &signature).unwrap()
    }

    fn refused(result: Result<SessionView, GamesError>) -> bool {
        matches!(result, Err(GamesError::AuthFailed))
    }

    #[test]
    fn a_wallet_that_signs_its_challenge_gets_a_session_for_its_own_address_and_no_other() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let session = sign_in(&h, &keys, &address);
        assert_eq!(session.address, address);
        assert_eq!(session.token.len(), 64);
        assert_eq!(h.service.session_address(&session.token).unwrap(), Some(address.clone()));
        // Only a hash of the token is kept: the token itself is not in the database.
        let stored = h.service.store().session_for(&auth::token_hash(&session.token), h.service.now()).unwrap();
        assert_eq!(stored.map(|s| s.address), Some(address));
        // A token nobody was given is nothing.
        assert_eq!(h.service.session_address(&"00".repeat(32)).unwrap(), None);
    }

    #[test]
    fn a_challenge_is_spent_by_its_first_use_and_cannot_be_replayed() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, signature) = signed(&h, &keys, &address);
        h.service.auth_verify(&id, &public_key, &signature).unwrap();
        assert!(refused(h.service.auth_verify(&id, &public_key, &signature)), "the identical request, again");
        h.advance(5);
        assert!(refused(h.service.auth_verify(&id, &public_key, &signature)), "and a little later");
    }

    #[test]
    fn a_signature_from_a_different_wallet_is_refused_and_does_not_burn_the_real_one() {
        let h = harness(Options::default());
        let (alice, alice_address) = wallet();
        let (mallory, mallory_address) = wallet();
        assert_ne!(alice_address, mallory_address);
        let challenge = h.service.auth_challenge(&alice_address, PURPOSE_PLAY).unwrap();
        let digest = hex::decode(&challenge.digest).unwrap();
        // Mallory signs the *correct* digest of Alice's challenge and presents her own key. The signature is
        // perfectly valid for her key; it is refused only because her key is not Alice's address.
        let her_signature = hex::encode(mallory.sign(&digest));
        let her_key = hex::encode(&mallory.public_key);
        assert!(
            crate::QuantumKeyPair::verify(&mallory.public_key, &digest, &hex::decode(&her_signature).unwrap()),
            "the forged attempt is a genuinely valid signature by the wrong wallet"
        );
        assert!(refused(h.service.auth_verify(&challenge.challenge_id, &her_key, &her_signature)));
        // Alice's key with Mallory's signature: the signature does not hold.
        assert!(refused(h.service.auth_verify(&challenge.challenge_id, &hex::encode(&alice.public_key), &her_signature)));
        // Alice's own proof still works: two failed tries are within the allowance.
        let session = h
            .service
            .auth_verify(
                &challenge.challenge_id,
                &hex::encode(&alice.public_key),
                &hex::encode(alice.sign(&digest)),
            )
            .unwrap();
        assert_eq!(session.address, alice_address, "the session is Alice's, never Mallory's");
    }

    #[test]
    fn a_signature_over_anything_but_this_challenge_is_refused() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, _) = signed(&h, &keys, &address);
        // The digest of a different challenge, a transaction-looking blob, and an owner proof.
        let other = h.service.auth_challenge(&address, PURPOSE_PLAY).unwrap();
        for message in [
            hex::decode(other.digest).unwrap(),
            b"UltraNet/appchain-envelope/v5".to_vec(),
            crate::games::ownership::proof_digest("register", "game-one", "Game One", 1_800_000_000).to_vec(),
        ] {
            assert!(refused(h.service.auth_verify(&id, &public_key, &hex::encode(keys.sign(&message)))));
        }
    }

    #[test]
    fn an_expired_challenge_is_refused_even_with_a_valid_signature() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, signature) = signed(&h, &keys, &address);
        h.advance(AuthConfig::default().challenge_ttl_seconds + 1);
        assert!(refused(h.service.auth_verify(&id, &public_key, &signature)));
    }

    #[test]
    fn malformed_input_is_refused_in_the_same_words_as_a_wrong_signature() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, signature) = signed(&h, &keys, &address);
        let huge = "ab".repeat(20_000);
        for (challenge, key, sig) in [
            ("not-hex".to_string(), public_key.clone(), signature.clone()),
            ("00".repeat(32), public_key.clone(), signature.clone()), // unknown id
            (id.clone(), "zz".to_string(), signature.clone()),
            (id.clone(), public_key.clone(), "".to_string()),
            (id.clone(), huge.clone(), signature.clone()),
            (id.clone(), public_key.clone(), huge),
            (id.to_uppercase(), public_key.clone(), signature.clone()),
        ] {
            assert!(refused(h.service.auth_verify(&challenge, &key, &sig)), "{challenge:.8} {key:.8} {sig:.8}");
        }
        // Issuing is checked for shape too, and says what is wrong with the request.
        assert!(matches!(h.service.auth_challenge("abc", PURPOSE_PLAY), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.auth_challenge(&address, "withdraw"), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.auth_challenge(&address, ""), Err(GamesError::Invalid(_))));
    }

    #[test]
    fn a_challenge_dies_after_too_many_failed_tries() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, signature) = signed(&h, &keys, &address);
        let wrong = hex::encode(keys.sign(b"not the digest"));
        for _ in 0..AuthConfig::default().max_attempts {
            assert!(refused(h.service.auth_verify(&id, &public_key, &wrong)));
        }
        assert!(refused(h.service.auth_verify(&id, &public_key, &signature)), "even the right signature is too late");
    }

    #[test]
    fn concurrent_verifications_of_one_challenge_make_exactly_one_session() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let (id, public_key, signature) = signed(&h, &keys, &address);
        let results: Vec<Result<SessionView, GamesError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| h.service.auth_verify(&id, &public_key, &signature)))
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });
        let winners: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
        assert_eq!(winners.len(), 1, "one request wins the challenge: {:?}", results.iter().map(|r| r.is_ok()).collect::<Vec<_>>());
        assert!(results.iter().filter(|r| r.is_err()).all(|r| matches!(r, Err(GamesError::AuthFailed))));
    }

    #[test]
    fn issuing_challenges_is_limited_per_address_and_for_the_whole_service() {
        let h = harness(Options::default());
        let (_, busy) = wallet();
        let limit = AuthConfig::default().challenges_per_address_per_window;
        for _ in 0..limit {
            h.service.auth_challenge(&busy, PURPOSE_PLAY).unwrap();
        }
        assert!(matches!(h.service.auth_challenge(&busy, PURPOSE_PLAY), Err(GamesError::AuthRateLimited)));
        let (_, other) = wallet();
        h.service.auth_challenge(&other, PURPOSE_PLAY).expect("another wallet is not held up by the first");
        // After the window the address may ask again.
        h.advance(AuthConfig::default().window_seconds + 1);
        h.service.auth_challenge(&busy, PURPOSE_PLAY).unwrap();

        // The service-wide limit, with a small ceiling so the test stays small.
        let store = h.service.store();
        let tight = AuthConfig { challenges_per_minute: 3, ..AuthConfig::default() };
        let now = h.service.now();
        let mut refused_overall = false;
        for n in 0..6u8 {
            let id = hex::encode([n.wrapping_add(100); 32]);
            let address = hex::encode([n.wrapping_add(1); 32]);
            if matches!(store.issue_challenge(&id, &address, PURPOSE_PLAY, now, &tight), Err(AuthError::RateLimited)) {
                refused_overall = true;
            }
        }
        assert!(refused_overall, "the overall ceiling holds whoever asks");
    }

    #[test]
    fn sessions_expire_are_revoked_on_logout_and_are_capped_per_wallet() {
        let h = harness(Options::default());
        let (keys, address) = wallet();
        let first = sign_in(&h, &keys, &address);
        h.service.auth_logout(&first.token).unwrap();
        assert_eq!(h.service.session_address(&first.token).unwrap(), None, "logout ends it");
        h.service.auth_logout(&first.token).unwrap(); // saying it twice is harmless
        h.service.auth_logout("garbage").unwrap(); // and so is nonsense, which is not told apart

        let expiring = sign_in(&h, &keys, &address);
        h.advance(AuthConfig::default().session_ttl_seconds + 1);
        assert_eq!(h.service.session_address(&expiring.token).unwrap(), None, "expiry ends it");

        // Beyond the cap the oldest go first.
        let cap = AuthConfig::default().max_sessions_per_address as usize;
        let tokens: Vec<String> = (0..cap + 1)
            .map(|_| {
                h.advance(1);
                sign_in(&h, &keys, &address).token
            })
            .collect();
        assert_eq!(h.service.session_address(&tokens[0]).unwrap(), None, "the oldest was revoked");
        for token in &tokens[1..] {
            assert_eq!(h.service.session_address(token).unwrap(), Some(address.clone()));
        }
        // An operator can end every session of a wallet at once.
        assert_eq!(h.service.store().revoke_sessions_for(&address, h.service.now()).unwrap() as usize, cap);
        assert_eq!(h.service.session_address(&tokens[cap]).unwrap(), None);
    }

    #[test]
    fn a_session_proves_one_wallet_and_nothing_about_owning_a_chain() {
        // Wallet B signs in. It can act for B, and a request for A is refused, whoever owns what on the ledger.
        let h = harness(Options::default());
        let (b_keys, b) = wallet();
        let (_, a) = wallet();
        let session = sign_in(&h, &b_keys, &b);
        h.service.require_player(Some(&session.token), &b).unwrap();
        assert!(matches!(h.service.require_player(Some(&session.token), &a), Err(GamesError::SessionMismatch)));
        // The session carries an address and a purpose and nothing else: there is no owner field to forge.
        let carried = h.service.store().session_for(&auth::token_hash(&session.token), h.service.now()).unwrap().unwrap();
        assert_eq!((carried.address, carried.purpose), (b, PURPOSE_PLAY.to_string()));
    }

    #[test]
    fn sign_in_state_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("games-auth-restart-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (keys, address) = wallet();
        let config = AuthConfig::default();
        let now = 1_800_000_000;
        let token_hash;
        let spent_id = "ab".repeat(32);
        {
            let store = GamesStore::open(&path).unwrap();
            let challenge = store.issue_challenge(&spent_id, &address, PURPOSE_PLAY, now, &config).unwrap();
            let digest = challenge.digest().unwrap();
            let signature = hex::encode(keys.sign(&digest));
            assert!(auth::verify_signature(&challenge, &hex::encode(&keys.public_key), &signature).is_ok());
            token_hash = auth::token_hash("a-token");
            store.finish_verification(&challenge, &token_hash, now, &config).unwrap();
        }
        let reopened = GamesStore::open(&path).unwrap();
        assert!(reopened.session_for(&token_hash, now + 1).unwrap().is_some(), "the session is still there");
        assert!(
            matches!(reopened.begin_verification(&spent_id, now + 1, &config), Err(AuthError::Rejected)),
            "and a spent challenge is still spent"
        );
        let _ = std::fs::remove_file(&path);
    }
}

mod wallet_auth_http {
    use super::*;
    use crate::games::api;
    use crate::games::auth::PURPOSE_PLAY;
    use actix_web::{
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::{json, Value};

    async fn call(h: &Harness, request: actix_web::test::TestRequest) -> (u16, Value) {
        let app = init_service(
            App::new()
                .app_data(web::Data::from(h.service.clone()))
                .configure(api::configure_routes),
        )
        .await;
        let response = call_service(&app, request.to_request()).await;
        let status = response.status().as_u16();
        (status, read_body_json(response).await)
    }

    async fn sign_in_over_http(h: &Harness, keys: &QuantumKeyPair) -> (String, String) {
        let address = QuantumKeyPair::address_from_public_key(&keys.public_key);
        let (status, challenge) = call(
            h,
            TestRequest::post()
                .uri("/api/games/auth/challenge")
                .set_json(json!({"address": address, "purpose": PURPOSE_PLAY})),
        )
        .await;
        assert_eq!(status, 200, "{challenge}");
        let digest = hex::decode(challenge["digest"].as_str().unwrap()).unwrap();
        let (status, session) = call(
            h,
            TestRequest::post().uri("/api/games/auth/verify").set_json(json!({
                "challenge_id": challenge["challenge_id"],
                "public_key": hex::encode(&keys.public_key),
                "signature": hex::encode(keys.sign(&digest)),
            })),
        )
        .await;
        assert_eq!(status, 200, "{session}");
        (address, session["token"].as_str().unwrap().to_string())
    }

    fn answer(address: &str, today: &str, level: u8) -> Value {
        json!({"address": address, "date": today, "level": level, "answer": []})
    }

    #[actix_web::test]
    async fn with_sign_in_required_a_bare_address_proves_nothing() {
        let h = harness(Options { require_player_auth: true, ..Options::default() });
        let today = h.service.today();
        let someone = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
        for (uri, body) in [
            ("/api/games/grid/answers", answer(someone, &today, 0)),
            ("/api/games/chess/results", json!({"address": someone, "game_id": "g-0123456789abcdef", "mode": "classic", "result": "player_win", "plies": 40})),
            ("/api/games/withdraw", json!({"address": someone})),
        ] {
            let (status, body) = call(&h, TestRequest::post().uri(uri).set_json(body)).await;
            assert_eq!((status, body["code"].as_str()), (401, Some("auth_required")), "{uri}");
        }
        // A token that is not a live session is the same refusal.
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/withdraw")
                .insert_header(("X-Wallet-Session", "00".repeat(32)))
                .set_json(json!({"address": someone})),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (401, Some("auth_required")));
    }

    #[actix_web::test]
    async fn a_signed_in_player_plays_and_withdraws_only_as_themselves_and_owns_nothing() {
        let h = harness(Options { require_player_auth: true, ..Options::default() });
        let b_keys = QuantumKeyPair::generate();
        let (b, token) = sign_in_over_http(&h, &b_keys).await;
        let today = h.service.today();

        // B's session cannot act for A.
        let a = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/grid/answers")
                .insert_header(("X-Wallet-Session", token.clone()))
                .set_json(answer(a, &today, 0)),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (403, Some("session_mismatch")));
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/withdraw")
                .insert_header(("X-Wallet-Session", token.clone()))
                .set_json(json!({"address": a})),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (403, Some("session_mismatch")));

        // For B it passes the gate and reaches the game, which judges the (empty) answer on its own merits.
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/grid/answers")
                .insert_header(("X-Wallet-Session", token.clone()))
                .set_json(answer(&b, &today, 0)),
        )
        .await;
        assert_ne!(status, 401, "{body}");
        assert_ne!(status, 403, "{body}");

        // After sign-out the same token is nothing.
        let (status, body) = call(
            &h,
            TestRequest::post().uri("/api/games/auth/logout").insert_header(("X-Wallet-Session", token.clone())),
        )
        .await;
        assert_eq!((status, body["signed_out"].as_bool()), (200, Some(true)));
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/withdraw")
                .insert_header(("X-Wallet-Session", token))
                .set_json(json!({"address": b})),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (401, Some("auth_required")));
    }

    #[actix_web::test]
    async fn a_wrong_session_is_refused_even_where_sign_in_is_optional() {
        // The library default is open, so an old client with no session still works...
        let h = harness(Options::default());
        let today = h.service.today();
        let someone = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
        let (status, body) = call(&h, TestRequest::post().uri("/api/games/withdraw").set_json(json!({"address": someone}))).await;
        assert_ne!(status, 401, "{body}");
        // ...but a session that is for somebody else is never silently ignored.
        let keys = QuantumKeyPair::generate();
        let (_, token) = sign_in_over_http(&h, &keys).await;
        let (status, body) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/grid/answers")
                .insert_header(("X-Wallet-Session", token))
                .set_json(answer(someone, &today, 0)),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (403, Some("session_mismatch")));
    }

    #[actix_web::test]
    async fn the_sign_in_routes_answer_a_bad_request_without_saying_what_was_wrong() {
        let h = harness(Options::default());
        let (status, body) = call(
            &h,
            TestRequest::post().uri("/api/games/auth/verify").set_json(json!({
                "challenge_id": "00".repeat(32), "public_key": "ab", "signature": "cd"
            })),
        )
        .await;
        assert_eq!((status, body["code"].as_str()), (401, Some("auth_failed")));
        let (status, body) = call(&h, TestRequest::post().uri("/api/games/auth/logout")).await;
        assert_eq!((status, body["code"].as_str()), (401, Some("auth_required")));
    }
}

// --- the game catalog --------------------------------------------------------------------------

mod platform_catalog {
    use super::*;
    use crate::games::catalog::{self, AuditRecord, CatalogError, CatalogGame};
    use crate::games::service::AdminProof;

    fn wallet() -> (QuantumKeyPair, String) {
        let keys = QuantumKeyPair::generate();
        let address = QuantumKeyPair::address_from_public_key(&keys.public_key);
        (keys, address)
    }

    /// A wallet that owns `chain` on the (fake) ledger.
    fn owner_of(h: &Harness, chain: u32) -> (QuantumKeyPair, String) {
        let (keys, address) = wallet();
        h.chain.owners.lock().unwrap().insert(chain, address.clone());
        (keys, address)
    }

    #[allow(clippy::too_many_arguments)]
    fn signed(
        keys: &QuantumKeyPair,
        action: &str,
        chain: u32,
        game_id: &str,
        name: &str,
        description: &str,
        status: &str,
        timestamp: u64,
    ) -> AdminProof {
        let mut fresh = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
        let nonce = hex::encode(fresh);
        let digest = catalog::admin_digest(action, chain, game_id, name, description, status, &nonce, timestamp);
        AdminProof {
            nonce,
            timestamp,
            public_key: hex::encode(&keys.public_key),
            signature: hex::encode(keys.sign(&digest)),
        }
    }

    async fn register(h: &Harness, keys: &QuantumKeyPair, chain: u32, name: &str) -> Result<CatalogGame, GamesError> {
        let proof = signed(keys, "register", chain, "", name, "", "", h.service.now());
        h.service.catalog_register(chain, name, "", proof).await
    }

    async fn rename(h: &Harness, keys: &QuantumKeyPair, game: &CatalogGame, name: &str) -> Result<CatalogGame, GamesError> {
        let proof = signed(keys, "update", game.chain_id, &game.game_id, name, "", "", h.service.now());
        h.service.catalog_update(&game.game_id, name, "", proof).await
    }

    async fn set_status(h: &Harness, keys: &QuantumKeyPair, game: &CatalogGame, status: &str) -> Result<CatalogGame, GamesError> {
        let proof = signed(keys, "status", game.chain_id, &game.game_id, "", "", status, h.service.now());
        h.service.catalog_set_status(&game.game_id, status, proof).await
    }

    #[tokio::test]
    async fn the_owner_of_a_chain_registers_a_game_on_it() {
        let h = harness(Options::default());
        let (keys, owner) = owner_of(&h, 7);
        let game = register(&h, &keys, 7, "Sky Racers").await.unwrap();
        assert_eq!(game.game_id.len(), 32);
        assert!(game.game_id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!((game.chain_id, game.name.as_str(), game.status.as_str()), (7, "Sky Racers", "active"));
        assert_eq!(game.validation_scheme, "record_only", "nothing that moves money until a verified scheme exists");
        assert_eq!((game.reward_policy_ref, game.adapter_ref), (None, None));
        assert_eq!(game.created_by, owner);
        let history = h.service.catalog_history(&game.game_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!((history[0].action.as_str(), history[0].actor.as_str(), history[0].chain_owner.as_str()), ("register", owner.as_str(), owner.as_str()));
    }

    #[tokio::test]
    async fn nobody_but_the_current_owner_registers_updates_or_disables_and_nothing_is_created() {
        let h = harness(Options::default());
        let (_owner_keys, _) = owner_of(&h, 7);
        let (stranger, _) = wallet();
        // A perfectly valid signature by a wallet that does not own the chain.
        assert!(matches!(register(&h, &stranger, 7, "Mine now").await, Err(GamesError::Forbidden(_))));
        // A chain the ledger does not know, and the L1 itself.
        assert!(matches!(register(&h, &stranger, 99, "Ghost").await, Err(GamesError::NotFound(_))));
        let proof = signed(&stranger, "register", 0, "", "L1", "", "", h.service.now());
        assert!(matches!(h.service.catalog_register(0, "L1", "", proof).await, Err(GamesError::Invalid(_))));
        // An unreachable ledger is not a guess about the owner.
        h.chain.down.store(true, Ordering::SeqCst);
        assert!(matches!(register(&h, &stranger, 7, "Offline").await, Err(GamesError::ChainUnavailable)));
        h.chain.down.store(false, Ordering::SeqCst);
        assert!(h.service.catalog_games(None).unwrap().is_empty(), "no refused request created a game");
    }

    #[tokio::test]
    async fn names_are_labels_so_many_games_can_share_one_and_one_chain_can_hold_many() {
        let h = harness(Options::default());
        let mut ids = HashSet::new();
        for chain in 1..=5u32 {
            let (keys, _) = owner_of(&h, chain);
            for _ in 0..8 {
                let game = register(&h, &keys, chain, "Chess").await.unwrap(); // the same label, every time
                assert!(ids.insert(game.game_id), "an id is never reused");
            }
        }
        assert_eq!(ids.len(), 40);
        assert_eq!(h.service.catalog_games(Some(3)).unwrap().len(), 8, "one chain, eight games");
        assert_eq!(h.service.catalog_games(None).unwrap().len(), 40, "five owners, five chains");
        // A label is bounded and clean, nothing more.
        let (keys, _) = owner_of(&h, 6);
        assert!(matches!(register(&h, &keys, 6, "").await, Err(GamesError::Invalid(_))));
        assert!(matches!(register(&h, &keys, 6, &"x".repeat(65)).await, Err(GamesError::Invalid(_))));
        assert!(matches!(register(&h, &keys, 6, "bad\nname").await, Err(GamesError::Invalid(_))));
    }

    #[tokio::test]
    async fn changing_the_label_or_the_status_never_changes_the_identity() {
        let h = harness(Options::default());
        let (keys, _) = owner_of(&h, 7);
        let game = register(&h, &keys, 7, "First Name").await.unwrap();
        let renamed = rename(&h, &keys, &game, "Second Name").await.unwrap();
        assert_eq!((renamed.game_id.as_str(), renamed.name.as_str()), (game.game_id.as_str(), "Second Name"));
        assert_eq!(set_status(&h, &keys, &game, "disabled").await.unwrap().status, "disabled");
        assert_eq!(set_status(&h, &keys, &game, "active").await.unwrap().status, "active");
        assert!(matches!(set_status(&h, &keys, &game, "paused").await, Err(GamesError::Invalid(_))));
        let (stranger, _) = wallet();
        assert!(matches!(rename(&h, &stranger, &game, "Hijacked").await, Err(GamesError::Forbidden(_))));
        assert!(matches!(set_status(&h, &stranger, &game, "disabled").await, Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.catalog_game(&game.game_id).unwrap().name, "Second Name");
        let actions: Vec<_> = h.service.catalog_history(&game.game_id).unwrap().into_iter().map(|e| e.action).collect();
        assert_eq!(actions, ["register", "update", "status", "status"]);
        assert!(matches!(h.service.catalog_game(&"00".repeat(16)), Err(GamesError::NotFound(_))));
    }

    #[tokio::test]
    async fn administration_follows_the_chain_when_it_changes_hands_and_the_game_stays_the_same() {
        let h = harness(Options::default());
        let (alice, alice_address) = owner_of(&h, 7);
        let game = register(&h, &alice, 7, "Heirloom").await.unwrap();
        // Alice signs a rename now, but does not send it yet.
        let early = signed(&alice, "update", 7, &game.game_id, "Alice's late edit", "", "", h.service.now());
        // The chain is handed to Bob on the ledger.
        let (bob, bob_address) = wallet();
        h.chain.owners.lock().unwrap().insert(7, bob_address.clone());
        assert!(matches!(rename(&h, &alice, &game, "Still mine").await, Err(GamesError::Forbidden(_))), "the old owner is out");
        assert!(
            matches!(h.service.catalog_update(&game.game_id, "Alice's late edit", "", early).await, Err(GamesError::Forbidden(_))),
            "a request signed while she owned it is not honoured after"
        );
        let renamed = rename(&h, &bob, &game, "Bob's Heirloom").await.unwrap();
        assert_eq!(renamed.game_id, game.game_id, "same identity");
        assert_eq!(renamed.created_by, alice_address, "history is kept: who registered it");
        let history = h.service.catalog_history(&game.game_id).unwrap();
        assert_eq!((history[0].actor.as_str(), history[1].actor.as_str(), history[1].chain_owner.as_str()), (alice_address.as_str(), bob_address.as_str(), bob_address.as_str()));
    }

    #[tokio::test]
    async fn a_signed_request_works_once_and_only_as_signed() {
        let h = harness(Options::default());
        let (keys, _) = owner_of(&h, 7);
        let now = h.service.now();
        let proof = signed(&keys, "register", 7, "", "Once", "", "", now);
        let first = h.service.catalog_register(7, "Once", "", proof.clone()).await.unwrap();
        assert!(matches!(h.service.catalog_register(7, "Once", "", proof.clone()).await, Err(GamesError::AdminProofInvalid)), "the identical request, again");
        assert_eq!(h.service.catalog_games(Some(7)).unwrap().len(), 1, "a replay created no second game");
        // A signature for one name is not a signature for another.
        assert!(matches!(h.service.catalog_register(7, "Twice", "", proof).await, Err(GamesError::AdminProofInvalid)));
        // Stale, future, malformed.
        let stale = signed(&keys, "register", 7, "", "Stale", "", "", now - 400);
        assert!(matches!(h.service.catalog_register(7, "Stale", "", stale).await, Err(GamesError::AdminProofInvalid)));
        let future = signed(&keys, "register", 7, "", "Future", "", "", now + 400);
        assert!(matches!(h.service.catalog_register(7, "Future", "", future).await, Err(GamesError::AdminProofInvalid)));
        let junk = AdminProof { nonce: "ab".repeat(8), timestamp: now, public_key: "zz".into(), signature: "".into() };
        let no_nonce = AdminProof { nonce: String::new(), ..signed(&keys, "register", 7, "", "No nonce", "", "", now) };
        assert!(matches!(h.service.catalog_register(7, "No nonce", "", no_nonce).await, Err(GamesError::AdminProofInvalid)), "a request without a nonce is refused");
        assert!(matches!(h.service.catalog_register(7, "Junk", "", junk).await, Err(GamesError::AdminProofInvalid)));
        // A registration proof cannot be used as an update, nor a status proof as a registration.
        let as_update = signed(&keys, "register", 7, "", "Reuse", "", "", now);
        assert!(matches!(h.service.catalog_update(&first.game_id, "Reuse", "", as_update).await, Err(GamesError::AdminProofInvalid)));
        // A player's sign-in is not an owner proof: the owner's own session proves nothing here.
        let session = {
            let address = QuantumKeyPair::address_from_public_key(&keys.public_key);
            let challenge = h.service.auth_challenge(&address, "play").unwrap();
            let digest = hex::decode(&challenge.digest).unwrap();
            h.service.auth_verify(&challenge.challenge_id, &hex::encode(&keys.public_key), &hex::encode(keys.sign(&digest))).unwrap()
        };
        assert!(!session.token.is_empty());
        let nothing = AdminProof { nonce: "cd".repeat(8), timestamp: now, public_key: hex::encode(&keys.public_key), signature: session.token };
        assert!(matches!(h.service.catalog_register(7, "Session", "", nothing).await, Err(GamesError::AdminProofInvalid)));
    }

    #[tokio::test]
    async fn a_chain_holds_a_bounded_number_of_games() {
        let h = harness(Options::default());
        let (keys, _) = owner_of(&h, 7);
        let limit = crate::games::service::ServiceConfig::new(String::new()).catalog_max_games_per_chain;
        for n in 0..limit {
            register(&h, &keys, 7, &format!("Game {n}")).await.unwrap();
        }
        assert!(matches!(register(&h, &keys, 7, "One too many").await, Err(GamesError::CatalogFull)));
        let (other, _) = owner_of(&h, 8);
        register(&h, &other, 8, "Another chain is not affected").await.unwrap();
    }

    #[test]
    fn the_catalog_survives_a_restart_and_a_used_request_stays_used() {
        let path = std::env::temp_dir().join(format!("games-catalog-restart-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let owner = "cd".repeat(32);
        let id = "ab".repeat(16);
        let record = |digest: [u8; 32]| AuditRecord { actor: &owner, action: "register", chain_owner: &owner, detail: "{}", digest };
        {
            let store = GamesStore::open(&path).unwrap();
            store.catalog_register(&id, 7, "Persistent", "", 1_000, 20, &record([1; 32])).unwrap();
        }
        let reopened = GamesStore::open(&path).unwrap();
        let game = reopened.catalog_game(&id).unwrap().expect("the game is still there");
        assert_eq!((game.chain_id, game.name.as_str(), game.created_by.as_str()), (7, "Persistent", owner.as_str()));
        assert_eq!(reopened.catalog_history(&id).unwrap().len(), 1);
        // The same signed request, after the restart, still cannot be used again (and creates no game).
        let other = "ef".repeat(16);
        assert!(matches!(reopened.catalog_register(&other, 7, "Replay", "", 1_001, 20, &record([1; 32])), Err(CatalogError::ProofInvalid)));
        assert!(reopened.catalog_game(&other).unwrap().is_none(), "the refused replay rolled back completely");
        let _ = std::fs::remove_file(&path);
    }
}

mod platform_catalog_http {
    use super::*;
    use crate::games::{api, catalog};
    use actix_web::{
        test::{call_service, init_service, read_body_json, TestRequest},
        web, App,
    };
    use serde_json::{json, Value};

    async fn call(h: &Harness, request: actix_web::test::TestRequest) -> (u16, Value) {
        let app = init_service(
            App::new().app_data(web::Data::from(h.service.clone())).configure(api::configure_routes),
        )
        .await;
        let response = call_service(&app, request.to_request()).await;
        let status = response.status().as_u16();
        (status, read_body_json(response).await)
    }

    fn body(keys: &QuantumKeyPair, action: &str, chain: u32, game_id: &str, name: &str, status: &str, now: u64) -> Value {
        let mut fresh = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
        let nonce = hex::encode(fresh);
        let digest = catalog::admin_digest(action, chain, game_id, name, "", status, &nonce, now);
        json!({"nonce": nonce, "timestamp": now, "public_key": hex::encode(&keys.public_key), "signature": hex::encode(keys.sign(&digest))})
    }

    #[actix_web::test]
    async fn a_game_is_registered_listed_renamed_disabled_and_audited_over_http() {
        let h = harness(Options::default());
        let keys = QuantumKeyPair::generate();
        let owner = QuantumKeyPair::address_from_public_key(&keys.public_key);
        h.chain.owners.lock().unwrap().insert(4, owner.clone());
        let now = h.service.now();

        let mut register = body(&keys, "register", 4, "", "Skyline", "", now);
        register["chain_id"] = json!(4);
        register["name"] = json!("Skyline");
        let (status, created) = call(&h, TestRequest::post().uri("/api/games/catalog").set_json(register)).await;
        assert_eq!(status, 200, "{created}");
        let id = created["game_id"].as_str().unwrap().to_string();
        assert_eq!((created["chain_id"].as_u64(), created["status"].as_str(), created["created_by"].as_str()), (Some(4), Some("active"), Some(owner.as_str())));

        let (status, listed) = call(&h, TestRequest::get().uri("/api/games/catalog?chain_id=4")).await;
        assert_eq!((status, listed["games"].as_array().map(Vec::len)), (200, Some(1)));
        let (status, other_chain) = call(&h, TestRequest::get().uri("/api/games/catalog?chain_id=5")).await;
        assert_eq!((status, other_chain["games"].as_array().map(Vec::len)), (200, Some(0)));
        let (status, one) = call(&h, TestRequest::get().uri(&format!("/api/games/catalog/{id}"))).await;
        assert_eq!((status, one["name"].as_str()), (200, Some("Skyline")));

        let mut update = body(&keys, "update", 4, &id, "Skyline II", "", now);
        update["name"] = json!("Skyline II");
        let (status, renamed) = call(&h, TestRequest::post().uri(&format!("/api/games/catalog/{id}/update")).set_json(update)).await;
        assert_eq!((status, renamed["name"].as_str(), renamed["game_id"].as_str()), (200, Some("Skyline II"), Some(id.as_str())));
        let mut disable = body(&keys, "status", 4, &id, "", "disabled", now);
        disable["status"] = json!("disabled");
        let (status, disabled) = call(&h, TestRequest::post().uri(&format!("/api/games/catalog/{id}/status")).set_json(disable)).await;
        assert_eq!((status, disabled["status"].as_str()), (200, Some("disabled")));

        let (status, history) = call(&h, TestRequest::get().uri(&format!("/api/games/catalog/{id}/history"))).await;
        assert_eq!(status, 200);
        let actions: Vec<_> = history["history"].as_array().unwrap().iter().map(|e| e["action"].as_str().unwrap()).collect();
        assert_eq!(actions, ["register", "update", "status"]);
    }

    #[actix_web::test]
    async fn the_routes_refuse_a_stranger_a_session_in_place_of_a_proof_and_bad_ids() {
        let h = harness(Options::default());
        let owner_keys = QuantumKeyPair::generate();
        h.chain.owners.lock().unwrap().insert(4, QuantumKeyPair::address_from_public_key(&owner_keys.public_key));
        let stranger = QuantumKeyPair::generate();
        let now = h.service.now();

        let mut forged = body(&stranger, "register", 4, "", "Hostile", "", now);
        forged["chain_id"] = json!(4);
        forged["name"] = json!("Hostile");
        let (status, refused) = call(&h, TestRequest::post().uri("/api/games/catalog").set_json(forged)).await;
        assert_eq!((status, refused["code"].as_str()), (403, Some("forbidden")));

        // No proof at all, with or without a session header: a malformed request, never an accepted one.
        let (status, _) = call(
            &h,
            TestRequest::post()
                .uri("/api/games/catalog")
                .insert_header(("X-Wallet-Session", "ab".repeat(32)))
                .set_json(json!({"chain_id": 4, "name": "No proof", "nonce": "ab".repeat(8)})),
        )
        .await;
        assert_eq!(status, 400);
        let mut unknown = body(&owner_keys, "register", 99, "", "Nowhere", "", now);
        unknown["chain_id"] = json!(99);
        unknown["name"] = json!("Nowhere");
        let (status, missing) = call(&h, TestRequest::post().uri("/api/games/catalog").set_json(unknown)).await;
        assert_eq!((status, missing["code"].as_str()), (404, Some("not_found")));
        for uri in ["/api/games/catalog/not-an-id", &format!("/api/games/catalog/{}", "00".repeat(16))] {
            let (status, _) = call(&h, TestRequest::get().uri(uri)).await;
            assert_eq!(status, 404, "{uri}");
        }
        assert!(h.service.catalog_games(None).unwrap().is_empty());
    }
}

// --- per-game reward pools and reward policies ---------------------------------------------------

mod reward_pools {
    use super::*;
    use crate::games::adapter::{AdapterModule, AdapterOutput, Outcome, RuntimeLimits, ValidatedResult};
    use crate::games::catalog::{self, CatalogGame};
    use crate::games::pools::{self, PolicyTerms, PoolError};
    use crate::games::service::AdminProof;

    const ADAPTER: [u8; 32] = [0xAD; 32];
    const OTHER_ADAPTER: [u8; 32] = [0xEE; 32];

    fn wallet() -> (QuantumKeyPair, String) {
        let keys = QuantumKeyPair::generate();
        let address = QuantumKeyPair::address_from_public_key(&keys.public_key);
        (keys, address)
    }

    fn proof(h: &Harness, keys: &QuantumKeyPair, action: &str, chain: u32, game_id: &str, name: &str) -> AdminProof {
        let mut fresh = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
        let nonce = hex::encode(fresh);
        let timestamp = h.service.now();
        let digest = catalog::admin_digest(action, chain, game_id, name, "", "", &nonce, timestamp);
        AdminProof { nonce, timestamp, public_key: hex::encode(&keys.public_key), signature: hex::encode(keys.sign(&digest)) }
    }

    /// A game on `chain`, owned by a fresh wallet, with the adapter approved by the platform.
    async fn game_on(h: &Harness, chain: u32) -> (QuantumKeyPair, String, CatalogGame) {
        let (keys, owner) = wallet();
        h.chain.owners.lock().unwrap().insert(chain, owner.clone());
        let registered = {
            let mut fresh = [0u8; 16];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
            let nonce = hex::encode(fresh);
            let ts = h.service.now();
            let digest = catalog::admin_digest("register", chain, "", "Pool Game", "", "", &nonce, ts);
            let p = AdminProof { nonce, timestamp: ts, public_key: hex::encode(&keys.public_key), signature: hex::encode(keys.sign(&digest)) };
            h.service.catalog_register(chain, "Pool Game", "", p).await.unwrap()
        };
        h.service.platform_approve_adapter(&registered.game_id, ADAPTER, 1, "ops").unwrap();
        (keys, owner, h.service.catalog_game(&registered.game_id).unwrap())
    }

    fn terms(reward: u64) -> PolicyTerms {
        PolicyTerms { reward_units: reward, eligible_outcomes: "win".into(), per_player_per_day: 3, per_game_per_day: 100, effective_from: 0 }
    }

    /// The owner sends `amount` to the payout address, then claims it for the game.
    async fn fund(h: &Harness, keys: &QuantumKeyPair, owner: &str, game: &CatalogGame, amount: u64, tag: u8) -> Result<pools::PoolView, GamesError> {
        let hash = hex::encode([tag; 32]);
        h.chain.transfers.lock().unwrap().insert(
            hash.clone(),
            crate::games::chain::TransferView { sender: owner.to_string(), recipient: h.payout_address.clone(), amount, pending: false },
        );
        let p = proof(h, keys, "pool-fund", game.chain_id, &game.game_id, &hash);
        h.service.pool_fund(&game.game_id, &hash, p).await
    }

    async fn propose(h: &Harness, keys: &QuantumKeyPair, game: &CatalogGame, t: PolicyTerms) -> Result<pools::PolicyRow, GamesError> {
        let summary = format!("{}|{}|{}|{}|{}", t.reward_units, t.eligible_outcomes, t.per_player_per_day, t.per_game_per_day, t.effective_from);
        let p = proof(h, keys, "policy-propose", game.chain_id, &game.game_id, &summary);
        h.service.policy_propose(&game.game_id, t, p).await
    }

    /// Fund, propose and approve a policy, ready to reserve rewards.
    async fn ready(h: &Harness, chain: u32, funds: u64, reward: u64) -> (QuantumKeyPair, String, CatalogGame) {
        let (keys, owner, game) = game_on(h, chain).await;
        fund(h, &keys, &owner, &game, funds, chain as u8).await.unwrap();
        let row = propose(h, &keys, &game, terms(reward)).await.unwrap();
        h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", "within the ceilings").unwrap();
        (keys, owner, game)
    }

    fn verdict(outcome: Outcome, module: [u8; 32]) -> ValidatedResult {
        ValidatedResult { output: AdapterOutput { outcome, score: 1, result_digest: [0; 32] }, fuel_used: 10, module_hash: module }
    }

    fn player(n: u8) -> String {
        hex::encode([n; 32])
    }

    // --- funding -------------------------------------------------------------------------------

    #[tokio::test]
    async fn the_owner_funds_a_pool_with_a_confirmed_transfer_and_it_counts_once() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        let view = fund(&h, &keys, &owner, &game, 5_000, 1).await.unwrap();
        assert_eq!((view.funded, view.available, view.reserved), (5_000, 5_000, 0));
        // The same transaction cannot be claimed again: not by this game, not by another.
        let again = proof(&h, &keys, "pool-fund", 7, &game.game_id, &hex::encode([1u8; 32]));
        assert!(matches!(h.service.pool_fund(&game.game_id, &hex::encode([1u8; 32]), again).await, Err(GamesError::Conflict)));
        assert_eq!(h.service.pool(&game.game_id).unwrap().funded, 5_000, "the replay added nothing");
        // A second, separate transfer adds to it.
        assert_eq!(fund(&h, &keys, &owner, &game, 700, 2).await.unwrap().funded, 5_700);
    }

    #[tokio::test]
    async fn a_deposit_the_ledger_does_not_show_properly_is_never_credited() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        let (stranger, _) = wallet();
        let hash = hex::encode([9u8; 32]);
        let transfer = |sender: &str, recipient: &str, amount: u64, pending: bool| {
            h.chain.transfers.lock().unwrap().insert(hash.clone(), crate::games::chain::TransferView { sender: sender.into(), recipient: recipient.into(), amount, pending });
        };
        let claim = |signer: &QuantumKeyPair| proof(&h, signer, "pool-fund", 7, &game.game_id, &hash);
        // Unknown transaction.
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await, Err(GamesError::NotFound(_))));
        // Pending, wrong recipient, zero amount, someone else's transfer.
        transfer(&owner, &h.payout_address, 100, true);
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await, Err(GamesError::Invalid(_))), "pending");
        transfer(&owner, &player(1), 100, false);
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await, Err(GamesError::Forbidden(_))), "wrong recipient");
        transfer(&owner, &h.payout_address, 0, false);
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await, Err(GamesError::Forbidden(_))), "zero amount");
        transfer(&player(2), &h.payout_address, 100, false);
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await, Err(GamesError::Forbidden(_))), "sent by someone else");
        // A stranger cannot claim even a perfect deposit for this game.
        transfer(&owner, &h.payout_address, 100, false);
        assert!(matches!(h.service.pool_fund(&game.game_id, &hash, claim(&stranger)).await, Err(GamesError::Forbidden(_))));
        // Malformed hash.
        assert!(matches!(h.service.pool_fund(&game.game_id, "abc", claim(&keys)).await, Err(GamesError::Invalid(_))));
        assert_eq!(h.service.pool(&game.game_id).unwrap().funded, 0, "none of that created money");
        // Only the real thing does.
        assert_eq!(h.service.pool_fund(&game.game_id, &hash, claim(&keys)).await.unwrap().funded, 100);
    }

    // --- isolation -----------------------------------------------------------------------------

    #[tokio::test]
    async fn one_games_funds_can_never_pay_another_games_reward() {
        let h = harness(Options::default());
        let (_, _, rich) = ready(&h, 7, 10_000, 100).await;
        let (_, _, poor) = ready_unfunded(&h, 8).await;
        let win = verdict(Outcome::Win, ADAPTER);
        // The unfunded game cannot reserve anything, however rich the other one is.
        assert!(matches!(h.service.reserve_reward(&poor.game_id, "r1", &player(1), &win), Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.pool(&poor.game_id).unwrap().available, 0, "its own pool is empty, whatever the other holds");
        // And the rich game pays from its own pool only.
        h.service.reserve_reward(&rich.game_id, "r1", &player(1), &win).unwrap();
        assert_eq!(h.service.pool(&rich.game_id).unwrap().reserved, 100);
        assert_eq!(h.service.pool(&poor.game_id).unwrap().reserved, 0);
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    /// A game with an approved adapter and an approved policy but an empty pool, which needs a pool to approve:
    /// so fund it, approve, then take the funds back.
    async fn ready_unfunded(h: &Harness, chain: u32) -> (QuantumKeyPair, String, CatalogGame) {
        let (keys, owner, game) = ready(h, chain, 1_000, 100).await;
        let p = proof(h, &keys, "pool-withdraw", chain, &game.game_id, "1000");
        h.service.pool_withdraw(&game.game_id, 1_000, p).await.unwrap();
        assert_eq!(h.service.pool(&game.game_id).unwrap().available, 0);
        (keys, owner, game)
    }

    // --- what may earn money -------------------------------------------------------------------

    #[tokio::test]
    async fn only_a_result_verified_by_the_approved_adapter_earns_anything() {
        let h = harness(Options::default());
        let (keys, owner, record_only) = {
            // A game the platform has NOT approved an adapter for stays record-only.
            let (keys, owner) = wallet();
            h.chain.owners.lock().unwrap().insert(9, owner.clone());
            let mut fresh = [0u8; 16];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut fresh);
            let nonce = hex::encode(fresh);
            let ts = h.service.now();
            let digest = catalog::admin_digest("register", 9, "", "Unverified", "", "", &nonce, ts);
            let p = AdminProof { nonce, timestamp: ts, public_key: hex::encode(&keys.public_key), signature: hex::encode(keys.sign(&digest)) };
            let game = h.service.catalog_register(9, "Unverified", "", p).await.unwrap();
            (keys, owner, game)
        };
        assert_eq!(record_only.validation_scheme, "record_only");
        fund(&h, &keys, &owner, &record_only, 5_000, 9).await.unwrap();
        let win = verdict(Outcome::Win, ADAPTER);
        assert!(matches!(h.service.reserve_reward(&record_only.game_id, "r1", &player(1), &win), Err(GamesError::Forbidden(_))), "record_only games pay nothing, funded or not");
        assert_eq!(h.service.pool(&record_only.game_id).unwrap().reserved, 0);
        assert_eq!(h.service.promised_reward(&record_only.game_id).unwrap(), None, "and promise nothing");

        // A verified game: the verdict must come from the approved module, and be a paying outcome.
        let (_, _, game) = ready(&h, 7, 5_000, 100).await;
        let from_other_module = verdict(Outcome::Win, OTHER_ADAPTER);
        assert!(matches!(h.service.reserve_reward(&game.game_id, "r1", &player(1), &from_other_module), Err(GamesError::Forbidden(_))), "a different module");
        for outcome in [Outcome::Loss, Outcome::Cancelled, Outcome::Invalid, Outcome::Tie] {
            assert!(matches!(h.service.reserve_reward(&game.game_id, "r2", &player(1), &verdict(outcome, ADAPTER)), Err(GamesError::Forbidden(_))), "{outcome:?} does not pay under a win-only policy");
        }
        assert_eq!(h.service.pool(&game.game_id).unwrap().reserved, 0, "no refusal promised anything");
        assert_eq!(h.service.reserve_reward(&game.game_id, "r3", &player(1), &win).unwrap().amount_units, 100);
    }

    #[tokio::test]
    async fn the_validation_scheme_is_a_gate_of_its_own_whatever_the_adapter_field_says() {
        // An adapter is on record but the scheme says record-only (a half-reverted or damaged row): nothing pays,
        // and nothing is promised, until the scheme itself says verified.
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 5_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        assert!(h.service.reserve_reward(&game.game_id, "before", &player(1), &win).is_ok());
        h.service
            .store()
            .lock()
            .unwrap()
            .execute("UPDATE platform_games SET validation_scheme = 'record_only' WHERE game_id = ?1", [&game.game_id])
            .unwrap();
        assert!(matches!(h.service.reserve_reward(&game.game_id, "after", &player(2), &win), Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.promised_reward(&game.game_id).unwrap(), None);
    }

    #[tokio::test]
    async fn a_real_adapter_verdict_flows_from_the_wasm_runtime_into_a_credit() {
        let h = harness(Options::default());
        // A real module: outcome comes from the first payload byte, as in the runtime's own tests.
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
        let module = AdapterModule::compile(&::wat::parse_str(wat).unwrap(), RuntimeLimits::default()).unwrap();
        let (keys, owner, game) = game_on(&h, 7).await;
        h.service.platform_approve_adapter(&game.game_id, module.hash(), 1, "ops").unwrap();
        fund(&h, &keys, &owner, &game, 1_000, 1).await.unwrap();
        let row = propose(&h, &keys, &game, terms(100)).await.unwrap();
        h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", "ok").unwrap();
        let validated = module.validate_result(&[5u8; 32], &[1]).unwrap(); // payload[0] = 1: a win
        let credit = h.service.reserve_reward(&game.game_id, "match-1", &player(5), &validated).unwrap();
        assert_eq!((credit.amount_units, credit.state.as_str(), credit.module_hash.as_str()), (100, "reserved", hex::encode(module.hash()).as_str()));
        // A loss from the same module earns nothing.
        let loss = module.validate_result(&[5u8; 32], &[2]).unwrap();
        assert!(matches!(h.service.reserve_reward(&game.game_id, "match-2", &player(5), &loss), Err(GamesError::Forbidden(_))));
    }

    // --- reward policy -------------------------------------------------------------------------

    #[tokio::test]
    async fn a_policy_is_proposed_by_the_owner_approved_by_the_platform_and_inert_until_then() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 5_000, 1).await.unwrap();
        let v1 = propose(&h, &keys, &game, terms(100)).await.unwrap();
        assert_eq!((v1.version, v1.status.as_str(), v1.proposed_by.as_str()), (1, "proposed", owner.as_str()));
        assert_eq!(v1.policy_hash.len(), 64);
        // Proposed is not approved: no reward can be reserved and none is promised.
        let win = verdict(Outcome::Win, ADAPTER);
        assert!(matches!(h.service.reserve_reward(&game.game_id, "r1", &player(1), &win), Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.promised_reward(&game.game_id).unwrap(), None);
        // The platform approves; the approver and time are recorded and the catalog points at the hash.
        let approved = h.service.platform_decide_policy(&game.game_id, 1, true, "ops-alice", "within the ceilings").unwrap();
        assert_eq!((approved.status.as_str(), approved.decided_at.is_some()), ("approved", true));
        assert_ne!(approved.decided_by.as_deref(), Some(owner.as_str()), "the owner did not approve their own policy");
        assert_eq!(h.service.catalog_game(&game.game_id).unwrap().reward_policy_ref.as_deref(), Some(approved.policy_hash.as_str()));
        assert_eq!(h.service.promised_reward(&game.game_id).unwrap(), Some(100));
        // A decision is final.
        assert!(matches!(h.service.platform_decide_policy(&game.game_id, 1, false, "ops", "changed my mind"), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.platform_decide_policy(&game.game_id, 9, true, "ops", ""), Err(GamesError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_policy_beyond_the_platform_ceilings_or_from_a_stranger_is_refused() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 5_000, 1).await.unwrap();
        let mut too_rich = terms(100);
        too_rich.reward_units = crate::games::config::MAX_REWARD_UNITS + 1;
        assert!(matches!(propose(&h, &keys, &game, too_rich).await, Err(GamesError::Invalid(_))));
        let mut too_many = terms(100);
        too_many.per_player_per_day = crate::games::config::MAX_REWARDED_PER_ADDRESS_PER_DAY + 1;
        assert!(matches!(propose(&h, &keys, &game, too_many).await, Err(GamesError::Invalid(_))));
        let mut too_broad = terms(100);
        too_broad.per_game_per_day = crate::games::config::MAX_REWARDED_PER_GAME_PER_DAY + 1;
        assert!(matches!(propose(&h, &keys, &game, too_broad).await, Err(GamesError::Invalid(_))));
        for bad in ["loss", "win,loss", "", "cancelled"] {
            let mut t = terms(100);
            t.eligible_outcomes = bad.into();
            assert!(matches!(propose(&h, &keys, &game, t).await, Err(GamesError::Invalid(_))), "{bad:?}");
        }
        let mut zero = terms(100);
        zero.reward_units = 0;
        assert!(matches!(propose(&h, &keys, &game, zero).await, Err(GamesError::Invalid(_))));
        let (stranger, _) = wallet();
        assert!(matches!(propose(&h, &stranger, &game, terms(100)).await, Err(GamesError::Forbidden(_))));
        assert!(h.service.store().policies(&game.game_id).unwrap().is_empty(), "nothing that was refused was stored");
        // The boundary values themselves are allowed.
        let ceiling = PolicyTerms { reward_units: crate::games::config::MAX_REWARD_UNITS, eligible_outcomes: "win,tie".into(), per_player_per_day: crate::games::config::MAX_REWARDED_PER_ADDRESS_PER_DAY, per_game_per_day: crate::games::config::MAX_REWARDED_PER_GAME_PER_DAY, effective_from: 0 };
        assert!(propose(&h, &keys, &game, ceiling).await.is_ok());
    }

    #[tokio::test]
    async fn the_platform_will_not_approve_a_policy_the_pool_cannot_pay_even_once() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 50, 1).await.unwrap(); // 50 units, against a reward of 100
        let row = propose(&h, &keys, &game, terms(100)).await.unwrap();
        assert!(matches!(h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", ""), Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.store().policy(&game.game_id, 1).unwrap().unwrap().status, "proposed", "still undecided");
        // The owner tops the pool up and it can be approved.
        fund(&h, &keys, &owner, &game, 60, 2).await.unwrap();
        assert!(h.service.platform_decide_policy(&game.game_id, 1, true, "ops", "").is_ok());
        // A rejection is recorded and changes nothing else.
        let second = propose(&h, &keys, &game, terms(200)).await.unwrap();
        let rejected = h.service.platform_decide_policy(&game.game_id, second.version, false, "ops", "too generous for the funding").unwrap();
        assert_eq!(rejected.status, "rejected");
        assert_eq!(h.service.store().policy_active(&game.game_id, h.service.now()).unwrap().unwrap().version, 1);
    }

    #[tokio::test]
    async fn a_new_version_supersedes_the_old_and_never_reprices_a_completed_credit() {
        let h = harness(Options::default());
        let (keys, _, game) = ready(&h, 7, 10_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        let early = h.service.reserve_reward(&game.game_id, "early", &player(1), &win).unwrap();
        let v2 = propose(&h, &keys, &game, terms(250)).await.unwrap();
        let v2 = h.service.platform_decide_policy(&game.game_id, v2.version, true, "ops", "").unwrap();
        let rows = h.service.store().policies(&game.game_id).unwrap();
        assert_eq!(rows.iter().map(|r| r.status.as_str()).collect::<Vec<_>>(), ["superseded", "approved"]);
        // History is kept: version 1's terms and hash are exactly what they were.
        assert_eq!((rows[0].reward_units, rows[0].version), (100, 1));
        let later = h.service.reserve_reward(&game.game_id, "later", &player(2), &win).unwrap();
        assert_eq!((later.amount_units, later.policy_version, later.policy_hash.as_str()), (250, 2, v2.policy_hash.as_str()));
        let early_again = h.service.store().pool_credit(&early.credit_id).unwrap().unwrap();
        assert_eq!((early_again.amount_units, early_again.policy_version), (100, 1), "the earlier credit keeps the price it was made at");
    }

    #[tokio::test]
    async fn a_policy_that_is_not_yet_effective_does_not_apply() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 10_000, 1).await.unwrap();
        let mut later = terms(100);
        later.effective_from = h.service.now() + 3_600;
        let row = propose(&h, &keys, &game, later).await.unwrap();
        h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", "").unwrap();
        let win = verdict(Outcome::Win, ADAPTER);
        assert!(matches!(h.service.reserve_reward(&game.game_id, "r1", &player(1), &win), Err(GamesError::Forbidden(_))), "approved, but not yet in force");
        h.advance(3_601);
        assert!(h.service.reserve_reward(&game.game_id, "r1", &player(1), &win).is_ok());
    }

    // --- limits and funds ----------------------------------------------------------------------

    #[tokio::test]
    async fn the_policys_daily_limits_hold_and_reset_at_midnight_utc() {
        let h = harness(Options::default());
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 100_000, 1).await.unwrap();
        let limits = PolicyTerms { reward_units: 10, eligible_outcomes: "win".into(), per_player_per_day: 2, per_game_per_day: 3, effective_from: 0 };
        let row = propose(&h, &keys, &game, limits).await.unwrap();
        h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", "").unwrap();
        let win = verdict(Outcome::Win, ADAPTER);
        h.service.reserve_reward(&game.game_id, "a", &player(1), &win).unwrap();
        h.service.reserve_reward(&game.game_id, "b", &player(1), &win).unwrap();
        assert!(matches!(h.service.reserve_reward(&game.game_id, "c", &player(1), &win), Err(GamesError::GameLimit)), "a third for one player");
        h.service.reserve_reward(&game.game_id, "d", &player(2), &win).unwrap();
        assert!(matches!(h.service.reserve_reward(&game.game_id, "e", &player(3), &win), Err(GamesError::GameLimit)), "a fourth for the game");
        h.next_day();
        assert!(h.service.reserve_reward(&game.game_id, "f", &player(1), &win).is_ok(), "a new UTC day starts the counts again");
    }

    #[tokio::test]
    async fn without_enough_funds_no_reward_is_promised_and_nothing_is_reserved() {
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 150, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        assert_eq!(h.service.promised_reward(&game.game_id).unwrap(), Some(100));
        h.service.reserve_reward(&game.game_id, "r1", &player(1), &win).unwrap();
        // 50 left against a reward of 100: the second win is refused, creates no credit, and the promise is withdrawn.
        assert!(matches!(h.service.reserve_reward(&game.game_id, "r2", &player(2), &win), Err(GamesError::Forbidden(_))));
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.funded, pool.reserved, pool.available), (150, 100, 50));
        assert_eq!(h.service.promised_reward(&game.game_id).unwrap(), None, "no promise the pool cannot keep");
        // The refused result left no trace in the books.
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    // --- double spending -----------------------------------------------------------------------

    #[tokio::test]
    async fn a_result_is_credited_once_however_many_times_it_is_submitted() {
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 1_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        let first = h.service.reserve_reward(&game.game_id, "match-1", &player(1), &win).unwrap();
        let replay = h.service.reserve_reward(&game.game_id, "match-1", &player(1), &win).unwrap();
        assert_eq!(first.credit_id, replay.credit_id, "the same credit comes back");
        assert_eq!(h.service.pool(&game.game_id).unwrap().reserved, 100, "reserved once");
        // The same result id claimed for a different player is refused, not paid twice.
        assert!(matches!(h.service.reserve_reward(&game.game_id, "match-1", &player(2), &win), Err(GamesError::ResultConflict)));
        assert_eq!(h.service.pool(&game.game_id).unwrap().reserved, 100);
        // The same id in another game is another result.
        let (_, _, other) = ready(&h, 8, 1_000, 100).await;
        assert!(h.service.reserve_reward(&other.game_id, "match-1", &player(1), &win).is_ok());
    }

    #[tokio::test]
    async fn a_credit_settles_once_and_the_accounting_follows() {
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 1_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        let a = h.service.reserve_reward(&game.game_id, "a", &player(1), &win).unwrap();
        let b = h.service.reserve_reward(&game.game_id, "b", &player(2), &win).unwrap();
        assert_eq!(h.service.pool(&game.game_id).unwrap().available, 800);
        let paid = h.service.settle_reward(&a.credit_id, true).unwrap();
        assert_eq!((paid.reserved, paid.paid, paid.available), (100, 100, 800));
        let released = h.service.settle_reward(&b.credit_id, false).unwrap();
        assert_eq!((released.reserved, released.paid, released.available), (0, 100, 900), "a released credit gives its reservation back");
        // Settling again moves nothing, whichever way it is asked.
        assert!(matches!(h.service.settle_reward(&a.credit_id, true), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.settle_reward(&a.credit_id, false), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.settle_reward(&b.credit_id, true), Err(GamesError::Invalid(_))));
        assert!(matches!(h.service.settle_reward(&"00".repeat(16), true), Err(GamesError::NotFound(_))));
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.funded, pool.reserved, pool.paid, pool.withdrawn, pool.available), (1_000, 0, 100, 0, 900));
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn concurrent_wins_cannot_spend_more_than_the_pool_holds() {
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 1_000, 100).await; // enough for exactly ten rewards
        let outcomes: Vec<Result<crate::games::pools::Credit, GamesError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..40u32)
                .map(|n| {
                    let service = h.service.clone();
                    let game_id = game.game_id.clone();
                    scope.spawn(move || service.reserve_reward(&game_id, &format!("match-{n}"), &hex::encode([(n % 250) as u8 + 1; 32]), &verdict(Outcome::Win, ADAPTER)))
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });
        // The per-player limit also applies (3 a day), so count what the funds alone allow with distinct players.
        let granted = outcomes.iter().filter(|r| r.is_ok()).count();
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!(pool.reserved, 100 * granted as u64, "what was reserved is exactly what was granted");
        assert!(pool.reserved <= pool.funded, "never more than the pool holds");
        assert_eq!(granted, 10, "ten rewards of 100 against 1,000 funded");
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_same_result_raced_from_many_threads_makes_one_credit() {
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 1_000, 100).await;
        let results: Vec<Result<crate::games::pools::Credit, GamesError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| {
                    let service = h.service.clone();
                    let game_id = game.game_id.clone();
                    scope.spawn(move || service.reserve_reward(&game_id, "one-match", &player(1), &verdict(Outcome::Win, ADAPTER)))
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });
        let ids: HashSet<String> = results.iter().filter_map(|r| r.as_ref().ok().map(|c| c.credit_id.clone())).collect();
        assert_eq!(ids.len(), 1, "every thread was told about the same single credit");
        assert_eq!(h.service.pool(&game.game_id).unwrap().reserved, 100);
    }

    // --- withdrawal ----------------------------------------------------------------------------

    #[tokio::test]
    async fn the_owner_takes_back_only_unreserved_funds_and_only_to_the_current_owner() {
        let h = harness(Options::default());
        let (keys, owner, game) = ready(&h, 7, 1_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        h.service.reserve_reward(&game.game_id, "a", &player(1), &win).unwrap(); // 100 reserved
        assert!(matches!(withdraw(&h, &keys, &game, 901).await, Err(GamesError::Forbidden(_))), "reserved funds cannot be taken back");
        assert!(matches!(withdraw(&h, &keys, &game, 0).await, Err(GamesError::Forbidden(_))));
        let after = withdraw(&h, &keys, &game, 900).await.unwrap();
        assert_eq!((after.withdrawn, after.available, after.reserved), (900, 0, 100));
        // A stranger cannot withdraw, and the amount is bound into what the owner signed.
        let (stranger, _) = wallet();
        let p = proof(&h, &stranger, "pool-withdraw", 7, &game.game_id, "1");
        assert!(matches!(h.service.pool_withdraw(&game.game_id, 1, p).await, Err(GamesError::Forbidden(_))));
        let signed_for_one = proof(&h, &keys, "pool-withdraw", 7, &game.game_id, "1");
        assert!(matches!(h.service.pool_withdraw(&game.game_id, 500, signed_for_one).await, Err(GamesError::AdminProofInvalid)));
        // The destination is whoever owns the chain now: after a handover, the new owner.
        let (heir, heir_address) = wallet();
        h.chain.owners.lock().unwrap().insert(7, heir_address.clone());
        fund(&h, &heir, &heir_address, &game, 300, 77).await.unwrap();
        let p = proof(&h, &heir, "pool-withdraw", 7, &game.game_id, "300");
        h.service.pool_withdraw(&game.game_id, 300, p).await.unwrap();
        let history = h.service.catalog_history(&game.game_id).unwrap();
        let last = history.last().unwrap();
        assert_eq!((last.action.as_str(), last.actor.as_str()), ("pool-withdraw", heir_address.as_str()));
        assert!(last.detail.contains(&heir_address), "the queued return goes to the new owner");
        let _ = owner;
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    async fn withdraw(h: &Harness, keys: &QuantumKeyPair, game: &CatalogGame, amount: u64) -> Result<pools::PoolView, GamesError> {
        let p = proof(h, keys, "pool-withdraw", game.chain_id, &game.game_id, &amount.to_string());
        h.service.pool_withdraw(&game.game_id, amount, p).await
    }

    // --- restart, reconciliation, solvency -----------------------------------------------------

    #[test]
    fn the_pools_survive_a_restart_and_the_books_can_be_re_derived() {
        use crate::games::catalog::AuditRecord;
        let path = std::env::temp_dir().join(format!("games-pools-restart-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let game_id = "ab".repeat(16);
        let owner = "cd".repeat(32);
        let record = |n: u8| AuditRecord { actor: &owner, action: "x", chain_owner: &owner, detail: "{}", digest: [n; 32] };
        {
            let store = GamesStore::open(&path).unwrap();
            store.catalog_register(&game_id, 7, "Persistent", "", 1_000, 20, &record(1)).unwrap();
            store.pool_record_deposit(&game_id, &"11".repeat(32), &owner, 1_000, 1_001, &record(2)).unwrap();
            store.platform_approve_adapter(&game_id, &ADAPTER, 1, 1_002, &record(3)).unwrap();
            let t = terms(100);
            store.policy_propose(&game_id, &t, &owner, 1_003, &record(4)).unwrap();
            store.policy_decide(&game_id, 1, true, &"ee".repeat(32), "ok", 1_004, &record(5)).unwrap();
            let credit = store.pool_reserve(&game_id, "m1", &"33".repeat(32), &"44".repeat(16), &verdict(Outcome::Win, ADAPTER), 2_000).unwrap();
            store.pool_settle(&credit.credit_id, true, 2_001).unwrap();
            store.pool_reserve(&game_id, "m2", &"33".repeat(32), &"55".repeat(16), &verdict(Outcome::Win, ADAPTER), 2_002).unwrap();
        }
        let reopened = GamesStore::open(&path).unwrap();
        let pool = reopened.pool_view(&game_id).unwrap();
        assert_eq!((pool.funded, pool.reserved, pool.paid, pool.available), (1_000, 100, 100, 800));
        assert!(reopened.pool_audit().unwrap().is_empty(), "the books re-derive from the records");
        assert_eq!(reopened.policy_active(&game_id, 3_000).unwrap().unwrap().version, 1);
        // A claimed deposit stays claimed, a credited result stays credited, a settled credit stays settled.
        assert!(matches!(reopened.pool_record_deposit(&game_id, &"11".repeat(32), &owner, 1_000, 3_000, &record(9)), Err(PoolError::AlreadyClaimed)));
        let again = reopened.pool_reserve(&game_id, "m1", &"33".repeat(32), &"66".repeat(16), &verdict(Outcome::Win, ADAPTER), 3_000).unwrap();
        assert_eq!(again.state, "paid", "the same result is the same credit, already paid");
        assert_eq!(reopened.pool_view(&game_id).unwrap().reserved, 100);
        // Damage the stored figure by hand: the audit says so, naming the pool and the difference.
        drop(reopened);
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute("UPDATE game_pools SET funded = funded + 500", []).unwrap();
        drop(raw);
        let issues = GamesStore::open(&path).unwrap().pool_audit().unwrap();
        assert!(issues.iter().any(|issue| issue.contains("funded is 1500")), "{issues:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_database_itself_refuses_a_pool_that_spends_more_than_it_holds() {
        let store = GamesStore::open(std::path::Path::new(":memory:")).unwrap();
        let guard = store.lock().unwrap();
        guard.execute("INSERT INTO game_pools (game_id, funded) VALUES (?1, 100)", [&"ab".repeat(16)]).unwrap();
        assert!(guard.execute("UPDATE game_pools SET reserved = 101", []).is_err(), "reserved beyond funded");
        assert!(guard.execute("UPDATE game_pools SET paid = 60, withdrawn = 41", []).is_err(), "paid and withdrawn beyond funded");
        assert!(guard.execute("UPDATE game_pools SET reserved = -1", []).is_err(), "a negative balance");
    }

    #[tokio::test]
    async fn a_payout_wallet_that_holds_less_than_is_owed_freezes_rewards_until_it_is_checked() {
        // The wallet holds 500; the pool is funded with 1,000. The service owes more than it holds.
        let h = harness(Options { pool_balance: 500, ..Options::default() });
        let (_, _, game) = ready(&h, 7, 1_000, 100).await;
        let win = verdict(Outcome::Win, ADAPTER);
        assert!(h.service.reserve_reward(&game.game_id, "ok-before", &player(1), &win).is_ok(), "nothing has noticed yet");
        assert!(matches!(h.service.pool_check_solvency().await, Err(GamesError::PayoutsPaused)));
        assert!(h.service.store().pools_frozen().unwrap().is_some());
        assert!(matches!(h.service.reserve_reward(&game.game_id, "after", &player(2), &win), Err(GamesError::PayoutsPaused)), "frozen for everyone");
        // Once the wallet is topped up and an operator clears the freeze, rewards resume.
        h.node.balance.store(5_000, Ordering::SeqCst);
        h.service.pool_check_solvency().await.unwrap();
        h.service.store().pools_set_frozen(false, None).unwrap();
        assert!(h.service.reserve_reward(&game.game_id, "after", &player(2), &win).is_ok());
    }

    // --- paying from a pool: the worker against the fake node's failure modes ---------------------

    async fn queued_reward(h: &Harness, chain: u32, funds: u64) -> (CatalogGame, crate::games::pools::Credit, crate::games::pool_payouts::PoolPayout) {
        let (_, _, game) = ready(h, chain, funds, 100).await;
        let credit = h.service.reserve_reward_for_payout(&game.game_id, "match-1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        let payout = h.service.store().pool_payouts_for(&game.game_id).unwrap().remove(0);
        (game, credit, payout)
    }

    fn confirm_on_node(h: &Harness, payout: &crate::games::pool_payouts::PoolPayout) {
        let hash = payout.tx_hash.clone().expect("signed");
        h.node.statuses.lock().unwrap().insert(hash, "confirmed".into());
    }

    #[tokio::test]
    async fn a_payout_is_stored_before_it_is_sent_and_settles_only_when_the_ledger_confirms_it() {
        let h = harness(Options::default());
        let (game, credit, payout) = queued_reward(&h, 7, 1_000).await;
        assert_eq!(payout.state, "queued");
        assert_eq!((h.service.pool(&game.game_id).unwrap().reserved, h.service.pool(&game.game_id).unwrap().fees_reserved), (100, 1));
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let sent = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!(sent.state, "submitted");
        assert_eq!(h.node.submissions.lock().unwrap().len(), 1);
        assert_eq!(h.node.submissions.lock().unwrap()[0], sent.envelope.clone().unwrap(), "what was sent is what was stored");
        // Pending changes nothing in the books.
        let before = h.service.pool(&game.game_id).unwrap();
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!(h.service.pool(&game.game_id).unwrap(), before);
        // Confirmed: reserved becomes paid, the fee is spent, the credit is paid, and nothing more happens.
        confirm_on_node(&h, &sent);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.reserved, pool.paid, pool.fees_reserved, pool.fees_paid, pool.available), (0, 100, 0, 1, 899));
        assert_eq!(h.service.store().pool_credit(&credit.credit_id).unwrap().unwrap().state, "paid");
        assert!(!h.service.process_pool_payout_once().await.unwrap(), "nothing is left to do");
        assert_eq!(h.node.submissions.lock().unwrap().len(), 1, "never sent again");
        assert!(h.service.store().pool_payout_confirm(&payout.payout_id, h.service.now()).is_err(), "a payout settles once");
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unreachable_node_gets_the_same_signed_bytes_again_never_a_second_signature() {
        let h = harness(Options::default());
        let (game, _, payout) = queued_reward(&h, 7, 1_000).await;
        *h.node.submit_mode.lock().unwrap() = SubmitMode::Unavailable;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let signed = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!((signed.state.as_str(), signed.attempts), ("signed", 1));
        let bytes = signed.envelope.clone().unwrap();
        // Still down, then up: every try is those bytes.
        assert!(h.service.process_pool_payout_once().await.unwrap());
        *h.node.submit_mode.lock().unwrap() = SubmitMode::Accept;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let after = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!((after.state.as_str(), after.envelope.as_deref(), after.nonce), ("submitted", Some(bytes.as_str()), signed.nonce));
        assert!(h.node.submissions.lock().unwrap().iter().all(|sent| *sent == bytes), "{} sends, one set of bytes", h.node.submissions.lock().unwrap().len());
        assert_eq!(h.service.pool(&game.game_id).unwrap().paid, 0, "nothing is paid until it is confirmed");
    }

    #[tokio::test]
    async fn a_node_that_rejects_a_signed_payout_freezes_the_pools_and_only_the_pools() {
        let h = harness(Options::default());
        let (game, _, payout) = queued_reward(&h, 7, 1_000).await;
        *h.node.submit_mode.lock().unwrap() = SubmitMode::Reject;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pools_frozen().unwrap().is_some());
        assert!(h.service.store().state().unwrap().paying, "the older Grid and Chess payouts are not paused by a pool problem");
        let signed = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!(signed.state, "signed", "kept for an operator, never silently re-signed");
        // Frozen: the worker idles and no new reservation is promised.
        assert!(!h.service.process_pool_payout_once().await.unwrap());
        assert!(matches!(h.service.reserve_reward_for_payout(&game.game_id, "m2", &player(2), &verdict(Outcome::Win, ADAPTER)), Err(GamesError::PayoutsPaused)));
        // An operator clears it; the same bytes go out again.
        h.service.store().pools_set_frozen(false, None).unwrap();
        *h.node.submit_mode.lock().unwrap() = SubmitMode::Accept;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let sent = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!((sent.state.as_str(), sent.envelope), ("submitted", signed.envelope));
    }

    #[tokio::test]
    async fn a_node_answering_about_a_different_transaction_freezes_the_pools() {
        let h = harness(Options::default());
        let (_, _, _) = queued_reward(&h, 7, 1_000).await;
        h.node.wrong_hash.store(true, Ordering::SeqCst);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pools_frozen().unwrap().unwrap().contains("different hash"));
    }

    #[tokio::test]
    async fn a_wallet_that_holds_too_little_freezes_the_pools_before_anything_is_signed() {
        // The wallet holds 50 against a pool funded with 1,000: caught at the start of the cycle, before any work.
        let h = harness(Options { pool_balance: 50, ..Options::default() });
        let (_, _, payout) = queued_reward(&h, 7, 1_000).await;
        assert!(!h.service.process_pool_payout_once().await.unwrap(), "the cycle stops at the solvency check");
        let after = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!((after.state.as_str(), after.envelope.is_none()), ("queued", true), "nothing was signed or sent");
        assert!(h.service.store().pools_frozen().unwrap().unwrap().contains("is owed"));
        assert!(h.node.submissions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_reward_that_no_longer_matches_its_policy_is_never_signed() {
        let h = harness(Options::default());
        let (game, _, payout) = queued_reward(&h, 7, 1_000).await;
        h.service
            .store()
            .lock()
            .unwrap()
            .execute("UPDATE game_reward_policies SET policy_hash = zeroblob(32) WHERE game_id = ?1", [&game.game_id])
            .unwrap();
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pools_frozen().unwrap().unwrap().contains("not backed"));
        assert!(h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap().envelope.is_none());
        assert!(h.node.submissions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_reward_that_can_never_be_paid_gives_its_reservation_and_fee_back() {
        let h = harness(Options::default());
        let (game, credit, payout) = queued_reward(&h, 7, 1_000).await;
        // The winner is the payout wallet itself (the service refuses that at intake; this is a damaged row).
        h.service.store().lock().unwrap().execute("UPDATE game_pool_payouts SET to_address = ?1", [&h.payout_address]).unwrap();
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.reserved, pool.fees_reserved, pool.paid, pool.available), (0, 0, 0, 1_000), "the pool is whole again");
        assert_eq!(h.service.store().pool_credit(&credit.credit_id).unwrap().unwrap().state, "released");
        assert_eq!(h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap().state, "failed");
        // And a payout that has used up its attempts before ever being signed does the same.
        let credit2 = h.service.reserve_reward_for_payout(&game.game_id, "match-2", &player(2), &verdict(Outcome::Win, ADAPTER)).unwrap();
        let second = h.service.store().pool_payouts_for(&game.game_id).unwrap().into_iter().find(|p| p.ref_id == credit2.credit_id).unwrap();
        for _ in 0..crate::games::service::ServiceConfig::new(String::new()).max_submission_attempts {
            h.service.store().pool_payout_record_attempt(&second.payout_id, Some("tried"), h.service.now()).unwrap();
        }
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!(h.service.store().pool_payout(&second.payout_id).unwrap().unwrap().state, "failed");
        assert_eq!(h.service.pool(&game.game_id).unwrap().available, 1_000);
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_credit_has_one_payout_however_often_the_queue_is_swept() {
        let h = harness(Options::default());
        let (game, _, _) = queued_reward(&h, 7, 1_000).await;
        // The crash case: a credit with no payout is found and queued, once.
        h.service.store().pool_reserve(&game.game_id, "orphan", &player(3), &"ee".repeat(16), &verdict(Outcome::Win, ADAPTER), h.service.now()).unwrap();
        for _ in 0..5 {
            h.service.store().pool_enqueue_pending(h.service.now()).unwrap();
        }
        assert_eq!(h.service.store().pool_payouts_for(&game.game_id).unwrap().len(), 2, "one for each credit, never more");
        // Replays of a result already credited do not queue another.
        for _ in 0..3 {
            h.service.reserve_reward_for_payout(&game.game_id, "match-1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        }
        assert_eq!(h.service.store().pool_payouts_for(&game.game_id).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_pool_that_cannot_hold_the_network_fee_creates_neither_credit_nor_payout() {
        let h = harness(Options::default());
        // Exactly one reward's worth, with nothing left over for the fee.
        let (keys, owner, game) = game_on(&h, 7).await;
        fund(&h, &keys, &owner, &game, 100, 1).await.unwrap();
        let row = propose(&h, &keys, &game, terms(100)).await.unwrap();
        h.service.platform_decide_policy(&game.game_id, row.version, true, "ops", "").unwrap();
        let refused = h.service.reserve_reward_for_payout(&game.game_id, "m1", &player(1), &verdict(Outcome::Win, ADAPTER));
        assert!(matches!(refused, Err(GamesError::Forbidden(_))), "{refused:?}");
        assert_eq!(h.service.store().pool_payouts_for(&game.game_id).unwrap().len(), 0);
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.reserved, pool.fees_reserved, pool.available), (0, 0, 100), "the whole commitment rolled back");
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_owner_withdrawal_pays_its_own_fee_and_goes_to_the_owner_the_ledger_names_at_signing() {
        let h = harness(Options::default());
        let (keys, owner, game) = ready(&h, 7, 1_000, 100).await;
        let p = proof(&h, &keys, "pool-withdraw", 7, &game.game_id, "500");
        h.service.pool_withdraw(&game.game_id, 500, p).await.unwrap();
        // The chain changes hands before the transfer is signed.
        let (heir, heir_address) = wallet();
        let _ = heir;
        h.chain.owners.lock().unwrap().insert(7, heir_address.clone());
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let payout = h.service.store().pool_payouts_for(&game.game_id).unwrap().remove(0);
        assert_eq!((payout.kind.as_str(), payout.to_address.as_str()), ("withdrawal", heir_address.as_str()));
        assert_ne!(payout.to_address, owner);
        assert_eq!((payout.fee_units, payout.sent_units), (Some(5), Some(495)), "the fee comes out of the 500");
        confirm_on_node(&h, &payout);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let pool = h.service.pool(&game.game_id).unwrap();
        assert_eq!((pool.withdrawn, pool.available, pool.paid), (500, 500, 0), "the pool paid 500 out and keeps its other 500");
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_withdrawal_too_small_to_cover_its_own_fee_stops_for_an_operator() {
        let h = harness(Options::default());
        let (keys, _, game) = ready(&h, 7, 1_000, 100).await;
        let p = proof(&h, &keys, "pool-withdraw", 7, &game.game_id, "1");
        h.service.pool_withdraw(&game.game_id, 1, p).await.unwrap();
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pools_frozen().unwrap().unwrap().contains("too small"));
        assert!(h.node.submissions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn two_games_paying_at_once_never_touch_each_others_books() {
        let h = harness(Options::default());
        let (_, _, a) = ready(&h, 7, 1_000, 100).await;
        let (_, _, b) = ready(&h, 8, 500, 100).await;
        h.service.reserve_reward_for_payout(&a.game_id, "a1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        h.service.reserve_reward_for_payout(&b.game_id, "b1", &player(2), &verdict(Outcome::Win, ADAPTER)).unwrap();
        for _ in 0..5 {
            h.service.process_pool_payout_once().await.unwrap();
            for payout in h.service.store().pool_payouts_for(&a.game_id).unwrap().into_iter().chain(h.service.store().pool_payouts_for(&b.game_id).unwrap()) {
                if payout.state == "submitted" {
                    confirm_on_node(&h, &payout);
                }
            }
        }
        let (pa, pb) = (h.service.pool(&a.game_id).unwrap(), h.service.pool(&b.game_id).unwrap());
        assert_eq!((pa.paid, pa.fees_paid, pa.available), (100, 1, 899));
        assert_eq!((pb.paid, pb.fees_paid, pb.available), (100, 1, 399));
        assert!(h.service.store().pool_audit().unwrap().is_empty());
    }

    // --- review fixes: settlement, replay, wrong state, shared-wallet solvency ----------------------

    #[tokio::test]
    async fn a_credit_with_a_payout_cannot_be_settled_or_released_directly() {
        let h = harness(Options::default());
        let (game, credit, payout) = queued_reward(&h, 7, 1_000).await;
        let snapshot = |h: &Harness| {
            (
                h.service.pool(&game.game_id).unwrap(),
                h.service.store().pool_credit(&credit.credit_id).unwrap().unwrap().state,
                h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap(),
            )
        };
        // Queued, then signed and submitted: in both, neither outcome is allowed, and nothing moves.
        for stage in 0..2 {
            let before = snapshot(&h);
            assert!(matches!(h.service.settle_reward(&credit.credit_id, true), Err(GamesError::Invalid(_))), "paid, stage {stage}");
            assert!(matches!(h.service.settle_reward(&credit.credit_id, false), Err(GamesError::Invalid(_))), "released, stage {stage}");
            assert!(h.service.store().pool_settle(&credit.credit_id, true, h.service.now()).is_err());
            assert!(h.service.store().pool_settle(&credit.credit_id, false, h.service.now()).is_err());
            assert_eq!(snapshot(&h), before, "credit, pool and payout are exactly as they were (stage {stage})");
            if stage == 0 {
                assert!(h.service.process_pool_payout_once().await.unwrap());
                assert_eq!(h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap().state, "submitted");
            }
        }
        // The worker's own settlement still works, once the node confirms.
        let sent = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        confirm_on_node(&h, &sent);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let (pool, state, payout_after) = snapshot(&h);
        assert_eq!((state.as_str(), payout_after.state.as_str(), pool.paid, pool.reserved), ("paid", "confirmed", 100, 0));
        assert!(matches!(h.service.settle_reward(&credit.credit_id, false), Err(GamesError::Invalid(_))), "and a paid credit stays paid");
    }

    #[tokio::test]
    async fn a_credit_without_a_payout_can_still_be_settled_by_the_accounting_api() {
        // The direct API remains for credits that no payout covers (the Stage 4 accounting tests rely on it).
        let h = harness(Options::default());
        let (_, _, game) = ready(&h, 7, 1_000, 100).await;
        let credit = h.service.reserve_reward(&game.game_id, "plain", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        assert!(h.service.settle_reward(&credit.credit_id, true).is_ok());
    }

    #[tokio::test]
    async fn the_replay_of_a_released_credit_is_an_explicit_refusal_not_a_success() {
        let h = harness(Options::default());
        let (game, credit, _) = queued_reward(&h, 7, 1_000).await;
        h.service.store().lock().unwrap().execute("UPDATE game_pool_payouts SET to_address = ?1", [&h.payout_address]).unwrap();
        assert!(h.service.process_pool_payout_once().await.unwrap()); // released
        assert_eq!(h.service.store().pool_credit(&credit.credit_id).unwrap().unwrap().state, "released");
        let before = h.service.pool(&game.game_id).unwrap();
        let replay = h.service.reserve_reward_for_payout(&game.game_id, "match-1", &player(1), &verdict(Outcome::Win, ADAPTER));
        assert!(matches!(replay, Err(GamesError::Forbidden(_))), "{replay:?}");
        assert!(matches!(h.service.reserve_reward(&game.game_id, "match-1", &player(1), &verdict(Outcome::Win, ADAPTER)), Err(GamesError::Forbidden(_))));
        assert_eq!(h.service.pool(&game.game_id).unwrap(), before, "and nothing was reserved by asking");
        // A paid credit's replay is still the idempotent success it was.
        let (game2, credit2, payout2) = queued_reward(&h, 8, 1_000).await;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        confirm_on_node(&h, &h.service.store().pool_payout(&payout2.payout_id).unwrap().unwrap());
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let again = h.service.reserve_reward_for_payout(&game2.game_id, "match-1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        assert_eq!((again.credit_id, again.state), (credit2.credit_id, "paid".to_string()));
    }

    #[tokio::test]
    async fn a_payout_can_only_be_marked_submitted_from_a_signed_state() {
        let h = harness(Options::default());
        let (_, _, payout) = queued_reward(&h, 7, 1_000).await;
        let now = h.service.now();
        assert!(h.service.store().pool_payout_mark_submitted(&payout.payout_id, now).is_err(), "queued: never signed");
        assert!(h.service.store().pool_payout_mark_submitted(&"00".repeat(16), now).is_err(), "no such payout");
        assert!(h.service.process_pool_payout_once().await.unwrap()); // signed and submitted by the worker
        assert!(h.service.store().pool_payout_mark_submitted(&payout.payout_id, now).is_ok(), "already submitted: the same fact again");
        confirm_on_node(&h, &h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap());
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pool_payout_mark_submitted(&payout.payout_id, now).is_err(), "confirmed: finished");
        assert_eq!(h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap().state, "confirmed", "and the failed call changed nothing");
    }

    #[tokio::test]
    async fn the_older_grid_path_cannot_promise_funds_the_pools_are_owed() {
        // With nothing owed to a pool, this Grid win is rewarded...
        let free = harness(Options::default());
        assert_eq!(free.solve(A, 0).await.outcome, Some("rewarded"));
        // ...with the whole wallet owed to a game's pool, the same win is recorded but not promised.
        let h = harness(Options { pool_balance: 1_000, ..Options::default() });
        let _ = ready(&h, 7, 1_000, 100).await;
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 1_000);
        assert_eq!(h.solve(A, 0).await.outcome, Some("recorded"), "the pool's money is not the older path's to promise");
    }

    #[tokio::test]
    async fn the_older_path_will_not_sign_a_payout_out_of_what_the_pools_are_owed() {
        let h = harness(Options::default()); // the wallet holds 100,000
        // A chess win (100 units) earns a withdrawable balance on the older path.
        h.service.submit_chess_result(h.chess(A, GAME_ID_1), None).await.unwrap();
        h.service.withdraw(A).unwrap();
        // A pool is then funded with nearly the whole wallet: what is left for the older path is 50.
        let _ = ready(&h, 7, 99_950, 100).await;
        h.service.process_payout_once().await.unwrap();
        assert!(h.node.submissions.lock().unwrap().is_empty(), "nothing was signed out of the pool's money");
        assert!(!h.service.store().state().unwrap().paying, "the older path pauses itself, as it always has when the wallet is short");
        // Topped up, it pays.
        h.node.balance.store(200_000, Ordering::SeqCst);
        h.service.store().set_state(None, Some(true), None, h.service.now()).unwrap();
        h.service.process_payout_once().await.unwrap();
        assert_eq!(h.node.submissions.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_queued_withdrawal_and_a_fee_in_flight_are_still_owed_until_they_leave_the_wallet() {
        let h = harness(Options::default());
        let (keys, _, game) = ready(&h, 7, 1_000, 100).await;
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 1_000);
        let credit = h.service.reserve_reward_for_payout(&game.game_id, "r1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        let p = proof(&h, &keys, "pool-withdraw", 7, &game.game_id, "400");
        h.service.pool_withdraw(&game.game_id, 400, p).await.unwrap();
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 1_000, "a reserved reward, its fee and a queued withdrawal have not left the wallet");
        // Reward first (oldest): sent, then confirmed.
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!(h.service.store().pool_in_flight_total().unwrap(), 101, "100 to the winner and 1 to the node may be out already");
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 1_000, "still owed until the ledger says it left");
        confirm_on_node(&h, &h.service.store().pool_payouts_for(&game.game_id).unwrap().into_iter().find(|p| p.ref_id == credit.credit_id).unwrap());
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 899, "the reward and its fee have left");
        // Then the withdrawal.
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let withdrawal = h.service.store().pool_payouts_for(&game.game_id).unwrap().into_iter().find(|p| p.kind == "withdrawal").unwrap();
        assert_eq!(h.service.store().pool_unspent_total().unwrap(), 899);
        confirm_on_node(&h, &withdrawal);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!((h.service.store().pool_unspent_total().unwrap(), h.service.store().pool_in_flight_total().unwrap()), (499, 0));
    }

    #[tokio::test]
    async fn a_shortfall_across_games_freezes_every_pool_and_nothing_is_signed() {
        // Game A is owed 600 and game B 400, but the wallet holds 900.
        let h = harness(Options { pool_balance: 900, ..Options::default() });
        let (_, _, a) = ready(&h, 7, 600, 100).await;
        let (_, _, b) = ready(&h, 8, 400, 100).await;
        h.service.reserve_reward_for_payout(&a.game_id, "a1", &player(1), &verdict(Outcome::Win, ADAPTER)).unwrap();
        h.service.reserve_reward_for_payout(&b.game_id, "b1", &player(2), &verdict(Outcome::Win, ADAPTER)).unwrap();
        assert!(!h.service.process_pool_payout_once().await.unwrap());
        assert!(h.service.store().pools_frozen().unwrap().is_some(), "both games are held, because the wallet cannot cover both");
        assert!(h.node.submissions.lock().unwrap().is_empty());
        // Covered again and cleared, both are paid.
        h.node.balance.store(2_300, Ordering::SeqCst);
        h.service.store().pools_set_frozen(false, None).unwrap();
        for _ in 0..5 {
            h.service.process_pool_payout_once().await.unwrap();
            for payout in h.service.store().pool_payouts_for(&a.game_id).unwrap().into_iter().chain(h.service.store().pool_payouts_for(&b.game_id).unwrap()) {
                if payout.state == "submitted" {
                    confirm_on_node(&h, &payout);
                }
            }
        }
        assert_eq!((h.service.pool(&a.game_id).unwrap().paid, h.service.pool(&b.game_id).unwrap().paid), (100, 100));
    }

    #[tokio::test]
    async fn a_payout_the_node_already_confirmed_does_not_look_like_a_shortfall_before_it_is_settled() {
        // The wallet holds the pool's 1,000, the 1,000 reserve and 50 more.
        let h = harness(Options { pool_balance: 2_050, ..Options::default() });
        let (_, _, payout) = queued_reward(&h, 7, 1_000).await;
        assert!(h.service.process_pool_payout_once().await.unwrap()); // signed and sent
        // The block lands: the wallet drops by the 100 and the fee of 1 before this service has looked.
        h.node.balance.store(1_949, Ordering::SeqCst);
        confirm_on_node(&h, &h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap());
        assert!(h.service.process_pool_payout_once().await.unwrap(), "1,949 plus 101 in flight covers the 1,000 still on the books");
        assert!(h.service.store().pools_frozen().unwrap().is_none(), "no false alarm");
        assert_eq!(h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap().state, "confirmed");
        h.service.pool_check_solvency().await.expect("1,949 held against 899 still owed");
    }

    #[tokio::test]
    async fn money_in_flight_is_what_keeps_a_landed_but_unsettled_payout_from_looking_like_a_shortfall() {
        let h = harness(Options { pool_balance: 2_050, ..Options::default() });
        let (game, _, payout) = queued_reward(&h, 7, 1_000).await;
        assert!(h.service.process_pool_payout_once().await.unwrap());
        let sent = h.service.store().pool_payout(&payout.payout_id).unwrap().unwrap();
        assert_eq!((sent.state.as_str(), sent.sent_units, sent.fee_units), ("submitted", Some(100), Some(1)));
        // The node includes it before this service looks: the wallet has already dropped by 100 + 1 and the pool's
        // 1,000 is still on the books, because nothing has settled it.
        confirm_on_node(&h, &sent);
        h.node.balance.store(950, Ordering::SeqCst);
        let store = h.service.store();
        let (balance, in_flight, owed) = (h.service.payout_balance().await.unwrap(), store.pool_in_flight_total().unwrap(), store.pool_unspent_total().unwrap());
        assert_eq!((balance, in_flight, owed, store.legacy_owed_total().unwrap()), (950, 101, 1_000, 0));
        // 1. Without the in-flight amount the wallet is short; 2. the in-flight amount covers the gap exactly (50 short, 101 in flight).
        assert!(balance < owed, "excluding in-flight, 950 < 1,000 would freeze every pool");
        assert!(balance + in_flight >= owed && owed - balance == 50);
        // 3. With it included, the real check passes and freezes nothing.
        h.service.pool_check_solvency().await.expect("950 + 101 covers 1,000");
        assert!(store.pools_frozen().unwrap().is_none());
        // The boundary is exact: 899 + 101 = 1,000 passes, 898 + 101 = 999 does not and names the real balance.
        h.node.balance.store(899, Ordering::SeqCst);
        h.service.pool_check_solvency().await.expect("899 + 101 is exactly 1,000");
        h.node.balance.store(898, Ordering::SeqCst);
        assert!(matches!(h.service.pool_check_solvency().await, Err(GamesError::PayoutsPaused)));
        assert_eq!(store.pools_frozen().unwrap().unwrap(), "the payout wallet holds 898 but 1000 is owed across the pools and the older balances");
        // Once the payout settles, nothing is in flight and the books drop by what left the wallet.
        store.pools_set_frozen(false, None).unwrap();
        h.node.balance.store(899, Ordering::SeqCst);
        assert!(h.service.process_pool_payout_once().await.unwrap());
        assert_eq!((store.pool_in_flight_total().unwrap(), store.pool_unspent_total().unwrap(), h.service.pool(&game.game_id).unwrap().paid), (0, 899, 100));
    }
}

// --- the shipped UltraGrid verifier, judged by the real runtime --------------------------------
//
// This test travels with the verifier so the verifier can gate itself on its own. The behavioural
// Grid tests — puzzle regeneration, day and chain binding, malformed payloads — stay with the play
// pipeline that gives them meaning, because they need `service.play`, which this branch does not
// carry. Splitting them this way keeps the byte-identity claim provable here and the behaviour
// claim where the behaviour lives.
mod shipped_grid_verifier {
    use crate::games::adapter::{AdapterModule, RuntimeLimits};

    #[test]
    fn the_shipped_grid_verifier_is_a_module_the_runtime_accepts() {
        let module = AdapterModule::compile(crate::games::adapters::GRID_VERIFIER, RuntimeLimits::default()).expect("the shipped module compiles");
        // Pinned: a changed module must change this hash deliberately, in a commit that says so. The full
        // digest, not a prefix, so two modules cannot share a pin by accident.
        assert_eq!(
            hex::encode(module.hash()),
            "376470459f3ac1d0fde46ac4e81f4661fdb219b8039ca8d552f785a898556321",
            "the shipped UltraGrid verifier's bytes changed: rebuild with scripts/build-adapters.sh and update this pin, the runbook's identity table and any registered copy"
        );
        assert_eq!(
            module.hash(),
            <[u8; 32]>::from(<sha3::Sha3_256 as sha3::Digest>::digest(crate::games::adapters::GRID_VERIFIER)),
            "the pin is the SHA3-256 of the bytes in the repository, recomputed, not copied from a note"
        );
    }
}
