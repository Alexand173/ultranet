//! Service-level tests: the whole pipeline against a fake chain, a fake node and a fake anchorer.
//!
//! Each test names a rule a player or an operator would care about. The fakes record what they were
//! asked, so a test can assert not only the answer but also what the service did to get it, which is
//! how "signed once", "retried with identical bytes" and "anchored once" are proved.

use super::{
    anchor::{AnchorError, Anchorer},
    chain::{AnchorPoint, ChainError, GamesChain},
    roots,
    rules::{Policy, GAME_CHESS, GAME_GRID},
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

// --- fakes -----------------------------------------------------------------------------------

#[derive(Default)]
struct FakeChain {
    active: Mutex<HashSet<String>>,
    everyone_active: AtomicBool,
    anchors: Mutex<Vec<(u32, AnchorPoint)>>,
    down: AtomicBool,
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
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // The shipped default is chess record-only (P1). Most tests here are about the payout
            // machinery, which only runs when something is credited, so the harness turns chess on
            // explicitly — `chess_is_record_only_by_default_and_still_records_and_anchors` is the
            // test that guards the default itself.
            policy: Policy::standard().with_chess_paying(),
            pool_balance: 100_000,
            anchor_games: vec![GAME_GRID.to_string()],
            with_signer: true,
            registry_chain: Some(3),
        }
    }
}

fn harness(options: Options) -> Harness {
    let chain = Arc::new(FakeChain::default());
    chain.everyone_active.store(true, Ordering::SeqCst);
    let node = Arc::new(FakeNode::new(options.pool_balance));
    let anchorer = Arc::new(FakeAnchorer::default());
    let clock = Arc::new(AtomicU64::new(date_start(DAY).unwrap() + 3_600));
    let signer = FaucetSigner::from_keypair_for_tests(QuantumKeyPair::generate()).unwrap();
    let payout_address = signer.address().to_string();
    let mut config = ServiceConfig::new(payout_address.clone());
    config.anchor_games = options.anchor_games;
    config.registry_chain_id = options.registry_chain;
    let tick = clock.clone();
    let service = Arc::new(GamesService::new(
        GamesStore::open(Path::new(":memory:")).unwrap(),
        options.policy,
        config,
        chain.clone(),
        node.clone(),
        options.with_signer.then(|| Arc::new(signer)),
        Some(anchorer.clone()),
        b"test-abuse-key".to_vec(),
        Arc::new(move || tick.load(Ordering::SeqCst)),
    ));
    Harness {
        service,
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
        policy: Policy::standard(),
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
    assert_eq!(calls[0].0, 2, "on chess's own chain");
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
        Policy::standard(),
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
        .find(|(chain, _)| *chain == 2)
        .expect("chess anchored on chain 2");
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
            &Policy::standard(),
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
        policy: Policy::standard(),
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
    let paying = Policy::standard().with_chess_paying();
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
        service::RegistryResultRequest,
    };

    const OWNER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";

    /// A game in the given status, with its API key.
    fn game(h: &Harness, id: &str, status: GameStatus) -> String {
        let (_, key) = h
            .service
            .store()
            .register_game(id, id, OWNER, h.service.now())
            .unwrap();
        h.service
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

    fn chain_three_calls(h: &Harness) -> Vec<[u8; 32]> {
        h.anchorer
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(chain, _)| *chain == 3)
            .map(|(_, root)| *root)
            .collect()
    }

    #[test]
    fn a_registered_game_records_a_result_with_its_key_and_a_replay_adds_nothing() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        let first = h
            .service
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
            .service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 900))
            .unwrap();
        assert!(again.duplicate);
        assert_eq!(
            h.service
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
            .service
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
                .service
                .submit_registry_result(id, presented, result(A, "match-0001", 1))
                .unwrap_err();
            assert!(
                matches!(error, GamesError::InvalidKey),
                "{id} / {presented}"
            );
            assert_eq!(error.status(), 401);
        }
        assert!(h
            .service
            .store()
            .results_for_day("tetris", DAY)
            .unwrap()
            .is_empty());
        assert!(h
            .service
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
            .service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 1))
            .unwrap_err();
        assert!(matches!(error, GamesError::GameSuspended));
        assert_eq!(error.status(), 403);
    }

    #[test]
    fn the_same_result_id_with_different_contents_is_a_conflict() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 900))
            .unwrap();
        let error = h
            .service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 901))
            .unwrap_err();
        assert!(matches!(error, GamesError::ResultConflict));
        assert_eq!(error.status(), 409);
        assert_eq!(
            h.service.store().results_for_day("tetris", DAY).unwrap()[0].score,
            900
        );
    }

    #[test]
    fn a_games_daily_limit_is_a_429_with_a_code_a_developer_can_act_on() {
        let mut policy = Policy::standard();
        policy.registry.results_per_game_per_day = 2;
        policy.registry.results_per_player_per_game_per_day = 1;
        let h = harness(Options {
            policy,
            ..Options::default()
        });
        let key = game(&h, "tetris", GameStatus::Registered);
        h.service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 1))
            .unwrap();
        let player = h
            .service
            .submit_registry_result("tetris", &key, result(A, "match-0002", 1))
            .unwrap_err();
        assert!(matches!(player, GamesError::PlayerLimit));
        h.service
            .submit_registry_result("tetris", &key, result(B, "match-0003", 1))
            .unwrap();
        let full = h
            .service
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
                h.service.submit_registry_result("tetris", &key, request),
                Err(GamesError::Invalid(_))
            ));
        }
        let good_hash = RegistryResultRequest {
            data_hash: Some("ab".repeat(32)),
            ..result(A, "match-0002", 1)
        };
        h.service
            .submit_registry_result("tetris", &key, good_hash)
            .unwrap();
        assert_eq!(
            h.service.store().results_for_day("tetris", DAY).unwrap()[0].data_hash,
            [0xAB; 32]
        );
        assert_eq!(
            h.service
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
        h.service
            .submit_registry_result("tetris", &key, result(&A.to_uppercase(), "match-0001", 1))
            .unwrap();
        assert_eq!(
            h.service.store().results_for_day("tetris", DAY).unwrap()[0].player,
            A
        );
    }

    #[test]
    fn the_platform_switch_stops_third_party_results_too() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.service
            .store()
            .set_state(Some(false), None, Some("maintenance"), 1)
            .unwrap();
        assert!(matches!(
            h.service
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
            h.service.registry_game("tetris").unwrap().status,
            "registered"
        );
        assert_eq!(h.service.registry_game("snake").unwrap().status, "listed");
        for hidden in ["scratch", "banned", "never-existed"] {
            assert!(
                matches!(
                    h.service.registry_game(hidden),
                    Err(GamesError::NotFound(_))
                ),
                "{hidden}"
            );
        }
        let listed: Vec<String> = h
            .service
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
        h.service
            .submit_registry_result("tetris", &tetris, result(A, "match-0001", 10))
            .unwrap();
        h.service
            .submit_registry_result("snake", &snake, result(B, "match-0001", 20))
            .unwrap();
        h.service
            .submit_registry_result("scratch", &scratch, result(A, "match-0001", 30))
            .unwrap();

        // Today is never closed early.
        h.service.run_daily().await.unwrap();
        assert!(chain_three_calls(&h).is_empty());
        assert!(h.service.registry_day_view(DAY).is_err());

        h.next_day();
        let report = h.service.run_daily().await.unwrap();
        assert!(report
            .anchored
            .contains(&("registry".to_string(), DAY.to_string())));
        let view = h.service.registry_day_view(DAY).unwrap();
        assert_eq!(
            view.games.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
            vec!["snake", "tetris"],
            "the sandbox is not anchored"
        );
        let calls = chain_three_calls(&h);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            hex::encode(calls[0]),
            view.day_root,
            "the anchored root is the one the day publishes"
        );
        assert_eq!(view.state, "anchored");
        assert_eq!(view.chain_id, Some(3));

        // Straight away again: pending, so no second anchor. Then it lands, and the day is confirmed.
        h.service.run_daily().await.unwrap();
        assert_eq!(chain_three_calls(&h).len(), 1);
        h.chain.add_anchor(3, 1, calls[0], h.service.now());
        let report = h.service.run_daily().await.unwrap();
        assert!(report
            .confirmed
            .contains(&("registry".to_string(), DAY.to_string())));
        assert_eq!(h.service.registry_day_view(DAY).unwrap().state, "confirmed");
        assert_eq!(chain_three_calls(&h).len(), 1);
    }

    #[tokio::test]
    async fn a_registry_anchor_of_unclear_outcome_is_checked_on_the_ledger_before_it_is_repeated() {
        let h = harness(Options::default());
        let key = game(&h, "tetris", GameStatus::Registered);
        h.service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 10))
            .unwrap();
        h.next_day();
        h.anchorer.fail.store(true, Ordering::SeqCst);
        h.service.run_daily().await.unwrap();
        assert_eq!(chain_three_calls(&h).len(), 1);
        let day = h.service.store().registry_day(DAY).unwrap().unwrap();
        assert!(day.last_error.is_some());
        // Too soon to retry; and when it is time, the root is found on the ledger and nothing is resubmitted.
        h.advance(60);
        h.service.run_daily().await.unwrap();
        assert_eq!(
            chain_three_calls(&h).len(),
            1,
            "no retry before the interval"
        );
        h.chain.add_anchor(3, 1, day.day_root, h.service.now());
        h.advance(601);
        h.service.run_daily().await.unwrap();
        assert_eq!(h.service.registry_day_view(DAY).unwrap().state, "confirmed");
        assert_eq!(chain_three_calls(&h).len(), 1);
    }

    #[tokio::test]
    async fn without_a_registry_chain_the_day_is_closed_for_proofs_but_never_anchored() {
        let h = harness(Options {
            registry_chain: None,
            ..Options::default()
        });
        let key = game(&h, "tetris", GameStatus::Registered);
        h.service
            .submit_registry_result("tetris", &key, result(A, "match-0001", 10))
            .unwrap();
        h.next_day();
        h.service.run_daily().await.unwrap();
        assert!(h
            .anchorer
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(chain, _)| *chain != 3));
        let view = h.service.registry_day_view(DAY).unwrap();
        assert_eq!((view.state.as_str(), view.chain_id), ("closed", None));
        assert!(
            h.service.registry_proof("tetris", DAY).is_ok(),
            "a developer can still be given a proof"
        );
    }

    #[tokio::test]
    async fn a_day_with_no_registered_results_is_never_anchored() {
        let h = harness(Options::default());
        let key = game(&h, "scratch", GameStatus::Sandbox);
        h.service
            .submit_registry_result("scratch", &key, result(A, "match-0001", 1))
            .unwrap();
        h.next_day();
        h.service.run_daily().await.unwrap();
        assert!(
            chain_three_calls(&h).is_empty(),
            "nothing registered happened, so there is nothing to anchor"
        );
    }

    #[tokio::test]
    async fn a_developer_can_verify_their_proof_against_the_published_root() {
        let h = harness(Options::default());
        for id in ["tetris", "snake", "puzzle-x"] {
            let key = game(&h, id, GameStatus::Registered);
            h.service
                .submit_registry_result(id, &key, result(A, "match-0001", 5))
                .unwrap();
        }
        h.next_day();
        h.service.close_registry_day(DAY).await.unwrap();
        for id in ["tetris", "snake", "puzzle-x"] {
            let proof = h.service.registry_proof(id, DAY).unwrap();
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
            let mine = h.service.store().results_for_day(id, DAY).unwrap();
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
            h.service.registry_proof("scratch-none", DAY),
            Err(GamesError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn only_a_finished_day_can_be_closed_and_a_bad_date_is_refused() {
        let h = harness(Options::default());
        assert!(matches!(
            h.service.close_registry_day(DAY).await,
            Err(GamesError::Invalid(_))
        ));
        assert!(matches!(
            h.service.close_registry_day("garbage").await,
            Err(GamesError::Invalid(_))
        ));
        assert!(matches!(
            h.service.registry_day_view("garbage"),
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
                    .configure(api::configure_routes),
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
            .service
            .store()
            .register_game(id, id, A, h.service.now())
            .unwrap();
        h.service
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
            h.service
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
            .service
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
            .service
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
        h.service.close_registry_day(DAY).await.unwrap();
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
        assert_eq!(day["chain_id"], 3);
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
        service::{RegistryRegisterRequest, RegistryResultRequest, RegistryRotateRequest},
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
            .service
            .self_register(register_request(&pair, "tetris", "Tetris", h.service.now()))
            .await
            .unwrap();
        assert_eq!(created.game.owner, pair.address(), "the owner is derived, not claimed");
        assert_eq!(created.game.status, GameStatus::Sandbox.code());
        let reply = h
            .service
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
            let error = h.service.self_register(request).await.unwrap_err();
            assert_eq!(error.code(), "invalid_key");
        }

        // A key whose address the chain has never seen cannot register, however valid the proof.
        h.chain.everyone_active.store(false, Ordering::SeqCst);
        let error = h
            .service
            .self_register(register_request(&pair, "tetris", "Tetris", now))
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request");
        h.chain.active.lock().unwrap().insert(pair.address());
        h.service
            .self_register(register_request(&pair, "tetris", "Tetris", now))
            .await
            .unwrap();
        assert_eq!(h.service.store().games().unwrap().len(), 1);
    }

    #[actix_web::test]
    async fn taken_and_reserved_ids_are_refused() {
        let h = harness(Options::default());
        let (one, two) = (QuantumKeyPair::generate(), QuantumKeyPair::generate());
        let now = h.service.now();
        h.service
            .self_register(register_request(&one, "tetris", "Tetris", now))
            .await
            .unwrap();
        for (pair, id) in [(&two, "tetris"), (&two, "grid"), (&two, "Bad_Id")] {
            let error = h
                .service
                .self_register(register_request(pair, id, "Mine", now))
                .await
                .unwrap_err();
            assert_eq!(error.code(), "invalid_request", "{id}");
        }
        assert_eq!(h.service.store().games().unwrap().len(), 1);
    }

    #[actix_web::test]
    async fn one_owner_and_one_day_are_both_bounded() {
        let mut policy = Policy::standard();
        policy.registry.games_per_owner = 2;
        policy.registry.self_registrations_per_day = 3;
        let h = harness(Options {
            policy,
            ..Options::default()
        });
        let now = h.service.now();
        let owner = QuantumKeyPair::generate();
        for id in ["game-one", "game-two"] {
            h.service
                .self_register(register_request(&owner, id, id, now))
                .await
                .unwrap();
        }
        let error = h
            .service
            .self_register(register_request(&owner, "game-three", "Three", now))
            .await
            .unwrap_err();
        assert_eq!((error.code(), error.status()), ("owner_game_limit", 429));

        // Someone else may still register the third of the day; the fourth is turned away.
        let other = QuantumKeyPair::generate();
        h.service
            .self_register(register_request(&other, "game-four", "Four", now))
            .await
            .unwrap();
        let error = h
            .service
            .self_register(register_request(&QuantumKeyPair::generate(), "game-five", "Five", now))
            .await
            .unwrap_err();
        assert_eq!((error.code(), error.status()), ("registration_limit", 429));

        // The next day the allowance is back.
        h.advance(86_400);
        h.service
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
            .service
            .self_register(register_request(&owner, "tetris", "Tetris", h.service.now()))
            .await
            .unwrap();

        // A stranger's valid signature, an unknown game, and a wrong-action proof all read the same.
        let stranger = QuantumKeyPair::generate();
        for (id, request) in [
            ("tetris", rotate_request(&stranger, "tetris", h.service.now())),
            ("nothing", rotate_request(&owner, "nothing", h.service.now())),
        ] {
            let error = h.service.self_rotate_key(id, request).unwrap_err();
            assert_eq!(error.code(), "invalid_key", "{id}");
        }
        let mut wrong_action = rotate_request(&owner, "tetris", h.service.now());
        wrong_action.signature = register_request(&owner, "tetris", "", h.service.now()).signature;
        assert_eq!(
            h.service.self_rotate_key("tetris", wrong_action).unwrap_err().code(),
            "invalid_key"
        );

        h.advance(10);
        let request = rotate_request(&owner, "tetris", h.service.now());
        let replay = rotate_request(&owner, "tetris", h.service.now());
        let rotated = h.service.self_rotate_key("tetris", request).unwrap();
        assert_ne!(rotated.api_key, created.api_key);
        assert_eq!(
            h.service
                .submit_registry_result("tetris", &created.api_key, a_result("match-0001"))
                .unwrap_err()
                .code(),
            "invalid_key",
            "the old key is dead"
        );
        h.service
            .submit_registry_result("tetris", &rotated.api_key, a_result("match-0001"))
            .unwrap();

        // The same signed request, captured and sent again, must not replace the new key.
        assert_eq!(
            h.service.self_rotate_key("tetris", replay).unwrap_err().code(),
            "invalid_key"
        );
        h.service
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
                .configure(crate::games::api::configure_routes),
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
                    .configure(crate::games::api::configure_routes),
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
            .service
            .store()
            .register_game(id, id, A, h.service.now())
            .unwrap();
        h.service
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
        h.service
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
        h.service.close_registry_day(DAY).await.unwrap();
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
        let rows = h.service.store().results_for_day("tetris", DAY).unwrap();
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
        h.service.close_registry_day(DAY).await.unwrap();
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
        assert_eq!(h.service.store().results_for_day("tetris", DAY).unwrap().len(), 1);

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
        let rows = h.service.store().results_for_day("tetris", DAY).unwrap();
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
        assert_eq!(h.service.store().results_for_day("tetris", DAY).unwrap().len(), 0);

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
        let stored = h.service.store().results_for_day("tetris", DAY).unwrap();
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
        assert_eq!(h.service.store().results_for_day("tetris", DAY).unwrap().len(), rows_before);

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
        assert!(h.service.store().game("newgame").unwrap().is_none(), "nothing was registered");

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
