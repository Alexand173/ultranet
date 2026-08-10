// ============================================================
// INTEGRACIONI TESTOVI ZA ULTRA BLOCKCHAIN 3.0
// ============================================================

use actix_web::{test as actix_test, web, App};
use chrono::Utc;
use parking_lot::RwLock;
use std::fs;
use std::sync::Arc;
use UltraNet::api::{
    approve_validator, list_approval_journal, list_proposals, propose_validator, AppState,
    ValidatorApprovalRequest, ValidatorProposalRequest,
};
use UltraNet::*;

mod fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup(path: &str) {
        let _ = fs::remove_dir_all(path);
    }

    fn init_test_bc(name: &str) -> UltraBlockchain {
        let path = format!("test_db_int_{}", name);
        cleanup(&path);
        UltraBlockchain::new(&path)
    }

    // ============================================================
    // TEST 1: KREIRANJE BLOCKCHAIN-A
    // ============================================================
    #[test]
    fn test_blockchain_creation() {
        let name = "creation";
        let blockchain = init_test_bc(name);

        assert_eq!(blockchain.chain.len(), 1, "Treba da ima genesis blok");
        assert_eq!(
            blockchain.validator.read().get_validator_count(),
            5,
            "Treba da ima 5 validatora"
        );
        assert_eq!(
            blockchain
                .difficulty
                .load(std::sync::atomic::Ordering::SeqCst),
            4,
            "Početna težina treba da bude 4"
        );
        assert_eq!(blockchain.version, 1, "Verzija treba da bude 1");

        println!("✅ Test blockchain_creation: PROŠAO!");
        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 2: KREIRANJE TRANSAKCIJE
    // ============================================================
    #[tokio::test]
    async fn test_transaction_creation() {
        let name = "tx_creation";
        let blockchain = init_test_bc(name);
        let mut alice = UltraWallet::new();
        let bob = UltraWallet::new();

        let merkle_root = blockchain.merkle_tree.read().get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root[0..32]);

        let mut zk_engine = blockchain.zk_engine.write();
        // Updated call to match new create_transaction signature
        let tx = alice.create_transaction(
            bob.get_address(),
            100,
            1,
            500000,
            1,
            &mut zk_engine,
            &merkle_root_array,
            ProofType::Transaction,
        );
        drop(zk_engine);

        assert!(tx.is_ok(), "Transakcija treba da bude uspešna");
        let tx = tx.unwrap();
        assert_eq!(tx.amount, 100, "Iznos treba da bude 100");
        assert_eq!(tx.fee, 1, "Fee treba da bude 1");
        assert_eq!(tx.sender, alice.get_address(), "Sender treba da bude Alice");
        assert_eq!(
            tx.recipient,
            bob.get_address(),
            "Recipient treba da bude Bob"
        );

        println!("✅ Test transaction_creation: PROŠAO!");
        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 3: DODAVANJE TRANSAKCIJE
    // ============================================================
    #[tokio::test]
    async fn test_add_transaction() {
        let name = "add_tx";
        let blockchain = init_test_bc(name);
        let mut alice = UltraWallet::new();
        let bob = UltraWallet::new();

        // FUND ALICE
        {
            let mut state = blockchain.state.write();
            state.insert(alice.get_address(), 100000);
        }

        let merkle_root = blockchain.merkle_tree.read().get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root[0..32]);

        let mut zk_engine = blockchain.zk_engine.write();
        let tx = alice
            .create_transaction(
                bob.get_address(),
                100,
                1,
                500000,
                1,
                &mut zk_engine,
                &merkle_root_array,
                ProofType::Transaction,
            )
            .unwrap();
        drop(zk_engine);

        let result = blockchain.add_transaction(tx);
        assert!(result.is_ok(), "Transakcija treba da bude dodata");

        let mempool_size = blockchain.mempool.read().get_pending_count();
        println!("📊 Mempool veličina: {}", mempool_size);

        assert_eq!(mempool_size, 1, "Mempool treba da ima 1 transakciju");

        println!("✅ Test add_transaction: PROŠAO!");
        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 4: RUDARENJE BLOKA
    // ============================================================
    #[tokio::test]
    async fn test_mine_block() {
        let name = "mine";
        let mut blockchain = init_test_bc(name);
        let mut alice = UltraWallet::new();
        let bob = UltraWallet::new();

        // FUND ALICE (Required after hardening)
        {
            let mut state = blockchain.state.write();
            state.insert(alice.get_address(), 100000);
        }

        let merkle_root = blockchain.merkle_tree.read().get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root[0..32]);

        let mut zk_engine = blockchain.zk_engine.write();
        let tx = alice
            .create_transaction(
                bob.get_address(),
                100,
                1,
                500000,
                1,
                &mut zk_engine,
                &merkle_root_array,
                ProofType::Transaction,
            )
            .unwrap();
        drop(zk_engine);

        let _ = blockchain.add_transaction(tx);

        let result = blockchain.mine_block();
        assert!(result.is_ok(), "Rudarenje treba da bude uspešno");

        let block = result.unwrap();
        assert_eq!(block.index, 1, "Index bloka treba da bude 1");
        assert!(
            !block.transactions.is_empty(),
            "Blok treba da ima transakcije"
        );

        assert_eq!(blockchain.chain.len(), 2, "Lanac treba da ima 2 bloka");

        println!("✅ Test mine_block: PROŠAO!");
        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 5: VALIDACIJA LANCA
    // ============================================================
    #[tokio::test]
    async fn test_chain_validation() {
        let name = "validation";
        let mut blockchain = init_test_bc(name);
        let mut alice = UltraWallet::new();
        let bob = UltraWallet::new();

        // FUND ALICE
        {
            let mut state = blockchain.state.write();
            state.insert(alice.get_address(), 100000);
        }

        let merkle_root = blockchain.merkle_tree.read().get_root();
        let mut merkle_root_array = [0u8; 32];
        merkle_root_array.copy_from_slice(&merkle_root[0..32]);

        let mut zk_engine = blockchain.zk_engine.write();
        let tx = alice
            .create_transaction(
                bob.get_address(),
                100,
                1,
                500000,
                1,
                &mut zk_engine,
                &merkle_root_array,
                ProofType::Transaction,
            )
            .unwrap();
        drop(zk_engine);

        let _ = blockchain.add_transaction(tx);
        let _ = blockchain.mine_block().unwrap();

        let is_valid = blockchain.is_chain_valid();
        assert!(is_valid, "Lanac treba da bude validan");

        println!("✅ Test chain_validation: PROŠAO!");
        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 6: LIVE ACTIX VALIDATOR PROPOSAL (VERSION 2)
    // ============================================================
    #[actix_web::test]
    async fn test_live_actix_validator_proposal_v2() {
        let name = "live_validator_proposal_v2";
        let blockchain = Arc::new(RwLock::new(init_test_bc(name)));
        let wallet = UltraWallet::new();
        let timestamp = Utc::now().timestamp() as u64;
        let payload = TransactionPayload::ValidatorJoinProposal {
            public_key: wallet.keypair.public_key.clone(),
            metadata: "Live-Actix-Validator".to_string(),
        };

        let mut signed_transaction = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [42u8; 32],
            timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
        };

        let message = blockchain
            .read()
            .create_transaction_message(&signed_transaction);
        signed_transaction.signature = wallet.keypair.sign(&message);
        let expected_hash = signed_transaction.get_hash();

        let request = ValidatorProposalRequest {
            sender: signed_transaction.sender.clone(),
            sender_public_key: signed_transaction.sender_public_key.clone(),
            proposal_public_key: match &signed_transaction.payload {
                TransactionPayload::ValidatorJoinProposal { public_key, .. } => public_key.clone(),
                _ => unreachable!("The live test must use a validator proposal payload"),
            },
            metadata: match &signed_transaction.payload {
                TransactionPayload::ValidatorJoinProposal { metadata, .. } => metadata.clone(),
                _ => unreachable!("The live test must use a validator proposal payload"),
            },
            nonce: signed_transaction.nonce,
            timestamp: signed_transaction.timestamp,
            nullifier: signed_transaction.nullifier.to_vec(),
            signature: signed_transaction.signature.clone(),
            version: signed_transaction.version,
        };

        let app_state = web::Data::new(AppState {
            blockchain: blockchain.clone(),
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(app_state)
                .route("/api/governance/propose", web::post().to(propose_validator)),
        )
        .await;

        let response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/propose")
                .set_json(&request)
                .to_request(),
        )
        .await;

        assert!(
            response.status().is_success(),
            "Live proposal endpoint returned {}",
            response.status()
        );
        let body: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(body["success"].as_bool(), Some(true));
        assert_eq!(
            body["message"].as_str(),
            Some("Validator proposal submitted!")
        );

        let pending = blockchain
            .read()
            .pending_proposals
            .read()
            .get(&expected_hash)
            .cloned();
        let pending = pending.expect("The live endpoint must register the proposal");
        assert_eq!(pending.metadata, "Live-Actix-Validator");
        assert_eq!(pending.public_key, wallet.keypair.public_key);

        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 7: LIVE ACTIX VALIDATOR APPROVAL (2-OF-3)
    // ============================================================
    #[actix_web::test]
    async fn test_live_actix_validator_approval_success() {
        let name = "live_validator_approval_success";
        let fixture = fixtures::sovereign_keys::SovereignKeyFixture::generate();
        let blockchain = Arc::new(RwLock::new(init_test_bc(name)));
        blockchain.write().sovereign_owners = fixture.public_keys();

        let wallet = UltraWallet::new();
        let proposal_timestamp = Utc::now().timestamp() as u64;
        let proposal_payload = TransactionPayload::ValidatorJoinProposal {
            public_key: wallet.keypair.public_key.clone(),
            metadata: "Live-Actix-Approval-Validator".to_string(),
        };
        let mut proposal_tx = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [43u8; 32],
            timestamp: proposal_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: proposal_payload,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
        };
        let proposal_message = blockchain.read().create_transaction_message(&proposal_tx);
        proposal_tx.signature = wallet.keypair.sign(&proposal_message);
        let proposal_hash = proposal_tx.get_hash();

        let proposal_request = ValidatorProposalRequest {
            sender: proposal_tx.sender.clone(),
            sender_public_key: proposal_tx.sender_public_key.clone(),
            proposal_public_key: wallet.keypair.public_key.clone(),
            metadata: "Live-Actix-Approval-Validator".to_string(),
            nonce: proposal_tx.nonce,
            timestamp: proposal_tx.timestamp,
            nullifier: proposal_tx.nullifier.to_vec(),
            signature: proposal_tx.signature.clone(),
            version: proposal_tx.version,
        };

        let app_state = web::Data::new(AppState {
            blockchain: blockchain.clone(),
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(app_state)
                .route("/api/governance/propose", web::post().to(propose_validator))
                .route("/api/governance/approve", web::post().to(approve_validator)),
        )
        .await;

        let proposal_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/propose")
                .set_json(&proposal_request)
                .to_request(),
        )
        .await;
        assert!(proposal_response.status().is_success());

        let approval_timestamp = Utc::now().timestamp() as u64;
        let approval_nullifier = [44u8; 32];
        let approval_tx = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: approval_nullifier,
            timestamp: approval_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval { proposal_hash },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let approval_message = blockchain.read().create_transaction_message(&approval_tx);
        let approval_request = ValidatorApprovalRequest {
            proposal_hash: hex::encode(proposal_hash),
            timestamp: approval_timestamp,
            nonce: 0,
            nullifier: approval_nullifier.to_vec(),
            signature: fixture.sign_with_threshold(&approval_message),
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };

        let approval_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/approve")
                .set_json(&approval_request)
                .to_request(),
        )
        .await;
        assert!(approval_response.status().is_success());
        let body: serde_json::Value = actix_test::read_body_json(approval_response).await;
        assert_eq!(body["success"].as_bool(), Some(true));
        assert_eq!(
            body["message"].as_str(),
            Some("Validator proposal approved!")
        );
        assert!(!blockchain
            .read()
            .pending_proposals
            .read()
            .contains_key(&proposal_hash));
        assert_eq!(blockchain.read().validator.read().get_validator_count(), 6);

        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 8: LIVE ACTIX APPROVAL REJECTS SINGLE SIGNATURE
    // ============================================================
    #[actix_web::test]
    async fn test_live_actix_validator_approval_fails_with_single_signature() {
        let name = "live_validator_approval_insufficient_sig";
        let fixture = fixtures::sovereign_keys::SovereignKeyFixture::generate();
        let blockchain = Arc::new(RwLock::new(init_test_bc(name)));
        blockchain.write().sovereign_owners = fixture.public_keys();

        let wallet = UltraWallet::new();
        let proposal_timestamp = Utc::now().timestamp() as u64;
        let mut proposal_tx = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [45u8; 32],
            timestamp: proposal_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorJoinProposal {
                public_key: wallet.keypair.public_key.clone(),
                metadata: "Live-Actix-Insufficient-Validator".to_string(),
            },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
        };
        let proposal_message = blockchain.read().create_transaction_message(&proposal_tx);
        proposal_tx.signature = wallet.keypair.sign(&proposal_message);
        let proposal_hash = proposal_tx.get_hash();

        let proposal_request = ValidatorProposalRequest {
            sender: proposal_tx.sender.clone(),
            sender_public_key: proposal_tx.sender_public_key.clone(),
            proposal_public_key: wallet.keypair.public_key.clone(),
            metadata: "Live-Actix-Insufficient-Validator".to_string(),
            nonce: proposal_tx.nonce,
            timestamp: proposal_tx.timestamp,
            nullifier: proposal_tx.nullifier.to_vec(),
            signature: proposal_tx.signature.clone(),
            version: proposal_tx.version,
        };
        let app_state = web::Data::new(AppState {
            blockchain: blockchain.clone(),
        });
        let app = actix_test::init_service(
            App::new()
                .app_data(app_state)
                .route("/api/governance/propose", web::post().to(propose_validator))
                .route("/api/governance/approve", web::post().to(approve_validator)),
        )
        .await;

        let proposal_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/propose")
                .set_json(&proposal_request)
                .to_request(),
        )
        .await;
        assert!(proposal_response.status().is_success());

        let approval_timestamp = Utc::now().timestamp() as u64;
        let approval_nullifier = [46u8; 32];
        let approval_tx = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: approval_nullifier,
            timestamp: approval_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval { proposal_hash },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let approval_message = blockchain.read().create_transaction_message(&approval_tx);
        let approval_request = ValidatorApprovalRequest {
            proposal_hash: hex::encode(proposal_hash),
            timestamp: approval_timestamp,
            nonce: 0,
            nullifier: approval_nullifier.to_vec(),
            signature: fixture.sign_with_owner(0, &approval_message),
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };

        let approval_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/approve")
                .set_json(&approval_request)
                .to_request(),
        )
        .await;
        assert_eq!(
            approval_response.status(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
        let body: serde_json::Value = actix_test::read_body_json(approval_response).await;
        assert_eq!(body["success"].as_bool(), Some(false));
        assert!(body["message"]
            .as_str()
            .expect("approval error should include a message")
            .contains("Insufficient signatures"));
        assert!(blockchain
            .read()
            .pending_proposals
            .read()
            .contains_key(&proposal_hash));
        assert_eq!(blockchain.read().validator.read().get_validator_count(), 5);

        cleanup(&format!("test_db_int_{}", name));
    }

    #[actix_web::test]
    async fn test_live_actix_validator_approval_rejects_unknown_proposal() {
        let name = "live_validator_approval_unknown";
        let blockchain = Arc::new(RwLock::new(init_test_bc(name)));
        let app_state = web::Data::new(AppState { blockchain });
        let app = actix_test::init_service(
            App::new()
                .app_data(app_state)
                .route("/api/governance/approve", web::post().to(approve_validator)),
        )
        .await;

        let request = ValidatorApprovalRequest {
            proposal_hash: "00".repeat(32),
            timestamp: Utc::now().timestamp() as u64,
            nonce: 0,
            nullifier: vec![7u8; 32],
            signature: vec![],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/governance/approve")
                .set_json(&request)
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::NOT_FOUND);
        let body: serde_json::Value = actix_test::read_body_json(response).await;
        assert_eq!(body["success"].as_bool(), Some(false));
        assert_eq!(
            body["message"].as_str(),
            Some("Validator proposal nije pronađen")
        );

        cleanup(&format!("test_db_int_{}", name));
    }

    // ============================================================
    // TEST 10: PENDING PROPOSALS SURVIVE RESTARTS
    // ============================================================
    #[actix_web::test]
    async fn test_pending_validator_proposal_persists_across_restart() {
        let name = "pending_proposal_persistence";
        let path = format!("test_db_int_{}", name);
        cleanup(&path);
        let fixture = fixtures::sovereign_keys::SovereignKeyFixture::generate();
        let wallet = UltraWallet::new();
        let proposal_timestamp = Utc::now().timestamp() as u64;
        let mut proposal_tx = Transaction {
            sender: wallet.address.clone(),
            sender_public_key: wallet.keypair.public_key.clone(),
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [51u8; 32],
            timestamp: proposal_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorJoinProposal {
                public_key: wallet.keypair.public_key.clone(),
                metadata: "Persistent-Validator".to_string(),
            },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
        };

        let blockchain = Arc::new(RwLock::new(UltraBlockchain::new(&path)));
        let proposal_message = blockchain.read().create_transaction_message(&proposal_tx);
        proposal_tx.signature = wallet.keypair.sign(&proposal_message);
        let proposal_hash = proposal_tx.get_hash();
        let proposal_request = ValidatorProposalRequest {
            sender: proposal_tx.sender.clone(),
            sender_public_key: proposal_tx.sender_public_key.clone(),
            proposal_public_key: wallet.keypair.public_key.clone(),
            metadata: "Persistent-Validator".to_string(),
            nonce: proposal_tx.nonce,
            timestamp: proposal_tx.timestamp,
            nullifier: proposal_tx.nullifier.to_vec(),
            signature: proposal_tx.signature.clone(),
            version: proposal_tx.version,
        };

        {
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(AppState {
                        blockchain: blockchain.clone(),
                    }))
                    .route("/api/governance/propose", web::post().to(propose_validator)),
            )
            .await;
            let response = actix_test::call_service(
                &app,
                actix_test::TestRequest::post()
                    .uri("/api/governance/propose")
                    .set_json(&proposal_request)
                    .to_request(),
            )
            .await;
            assert!(response.status().is_success());
        }
        drop(blockchain);

        let reopened = Arc::new(RwLock::new(UltraBlockchain::new(&path)));
        let restored = reopened
            .read()
            .pending_proposals
            .read()
            .get(&proposal_hash)
            .cloned()
            .expect("pending proposal must survive restart");
        assert_eq!(restored.metadata, "Persistent-Validator");
        assert_eq!(restored.public_key, wallet.keypair.public_key);

        {
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(AppState {
                        blockchain: reopened.clone(),
                    }))
                    .route("/api/governance/proposals", web::get().to(list_proposals)),
            )
            .await;
            let response = actix_test::call_service(
                &app,
                actix_test::TestRequest::get()
                    .uri("/api/governance/proposals")
                    .to_request(),
            )
            .await;
            let body: serde_json::Value = actix_test::read_body_json(response).await;
            let proposals = body["proposals"].as_array().expect("proposal list");
            assert!(proposals.iter().any(|proposal| {
                proposal["hash"] == serde_json::Value::String(hex::encode(proposal_hash))
            }));
        }

        reopened.write().sovereign_owners = fixture.public_keys();
        let approval_timestamp = Utc::now().timestamp() as u64;
        let approval_nullifier = [52u8; 32];
        let approval_tx = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: approval_nullifier,
            timestamp: approval_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval { proposal_hash },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let approval_message = reopened.read().create_transaction_message(&approval_tx);
        let failed_request = ValidatorApprovalRequest {
            proposal_hash: hex::encode(proposal_hash),
            timestamp: approval_timestamp,
            nonce: 0,
            nullifier: approval_nullifier.to_vec(),
            signature: fixture.sign_with_owner(0, &approval_message),
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        {
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(AppState {
                        blockchain: reopened.clone(),
                    }))
                    .route("/api/governance/approve", web::post().to(approve_validator)),
            )
            .await;
            let response = actix_test::call_service(
                &app,
                actix_test::TestRequest::post()
                    .uri("/api/governance/approve")
                    .set_json(&failed_request)
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), actix_web::http::StatusCode::BAD_REQUEST);
        }
        drop(reopened);

        let reopened_after_rejection = UltraBlockchain::new(&path);
        assert!(reopened_after_rejection
            .pending_proposals
            .read()
            .contains_key(&proposal_hash));
        drop(reopened_after_rejection);

        let approved = Arc::new(RwLock::new(UltraBlockchain::new(&path)));
        approved.write().sovereign_owners = fixture.public_keys();
        let approval_timestamp = Utc::now().timestamp() as u64;
        let approval_nullifier = [53u8; 32];
        let mut approval_tx = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: "0x0".to_string(),
            amount: 0,
            signature: vec![],
            zk_proof: vec![],
            nullifier: approval_nullifier,
            timestamp: approval_timestamp,
            fee: 0,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval { proposal_hash },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let approval_message = approved.read().create_transaction_message(&approval_tx);
        approval_tx.signature = fixture.sign_with_threshold(&approval_message);
        approved
            .write()
            .add_transaction(approval_tx)
            .expect("threshold approval should remove the proposal");
        drop(approved);

        let reopened_after_approval = UltraBlockchain::new(&path);
        assert!(!reopened_after_approval
            .pending_proposals
            .read()
            .contains_key(&proposal_hash));
        assert_eq!(
            reopened_after_approval
                .validator
                .read()
                .get_validator_count(),
            6,
            "approved validator must remain active after restart"
        );
        let mut extra_record = reopened_after_approval
            .storage
            .get_all_approval_records()
            .expect("approval journal should load")
            .into_iter()
            .next()
            .expect("approval journal should contain the approval")
            .clone();
        extra_record.proposal_hash = [0xAA; 32];
        extra_record.recorded_at = extra_record.recorded_at.saturating_add(1);
        reopened_after_approval
            .storage
            .save_approval_record(&extra_record)
            .expect("second approval record should persist");

        {
            let reopened = Arc::new(RwLock::new(reopened_after_approval));
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(AppState {
                        blockchain: reopened.clone(),
                    }))
                    .route(
                        "/api/governance/approvals",
                        web::get().to(list_approval_journal),
                    ),
            )
            .await;
            let response = actix_test::call_service(
                &app,
                actix_test::TestRequest::get()
                    .uri("/api/governance/approvals?limit=1")
                    .to_request(),
            )
            .await;
            let body: serde_json::Value = actix_test::read_body_json(response).await;
            let approvals = body["approvals"].as_array().expect("approval journal list");
            assert_eq!(approvals.len(), 1);
            assert_eq!(approvals[0]["proposal_hash"], hex::encode(proposal_hash));
            assert_eq!(body["pagination"]["limit"], 1);
            assert_eq!(body["pagination"]["total_count"], 2);
            assert_eq!(body["pagination"]["has_more"], true);
            let next_cursor = body["pagination"]["next_cursor"]
                .as_str()
                .expect("next cursor");
            assert_eq!(next_cursor.len(), 80);

            let next_page_uri = format!("/api/governance/approvals?limit=1&cursor={next_cursor}");
            let next_page = actix_test::call_service(
                &app,
                actix_test::TestRequest::get()
                    .uri(&next_page_uri)
                    .to_request(),
            )
            .await;
            let next_body: serde_json::Value = actix_test::read_body_json(next_page).await;
            assert_eq!(next_body["approvals"].as_array().unwrap().len(), 1);
            assert_eq!(
                next_body["approvals"][0]["proposal_hash"],
                hex::encode([0xAA; 32])
            );
            assert_eq!(next_body["pagination"]["total_count"], 2);
            assert_eq!(next_body["pagination"]["has_more"], false);
            assert!(next_body["pagination"]["next_cursor"].is_null());

            for uri in [
                "/api/governance/approvals?limit=0",
                "/api/governance/approvals?limit=101",
                "/api/governance/approvals?limit=not-a-number",
                "/api/governance/approvals?cursor=not-hex",
                "/api/governance/approvals?cursor=00",
                "/api/governance/approvals?offset=1",
            ] {
                let invalid = actix_test::call_service(
                    &app,
                    actix_test::TestRequest::get().uri(uri).to_request(),
                )
                .await;
                assert_eq!(
                    invalid.status(),
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "{uri}"
                );
            }
        }
        cleanup(&path);
    }
}

fn main() {
    println!("🧪 Pokrećem sve testove...");
}
