use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;
use UltraNet::faucet::{
    captcha::StaticCaptchaVerifier,
    config::FaucetConfig,
    models::{NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest},
    node_client::{NodeApi, NodeClientError},
    service::{AdmissionResult, FaucetService},
    signer::FaucetSigner,
    store::FaucetStore,
};
use UltraNet::{ProofType, QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain};

struct MockNode {
    faucet_address: Mutex<String>,
    balance: u64,
    submitted: Mutex<Vec<SignedTransferRequest>>,
    confirm: bool,
    fail_first_submit: Mutex<bool>,
}

impl MockNode {
    fn transaction_hash(envelope: &SignedTransferRequest) -> [u8; 32] {
        let nullifier: [u8; 32] = envelope.nullifier.as_slice().try_into().unwrap();
        let transaction = Transaction {
            sender: envelope.sender.clone(),
            sender_public_key: envelope.sender_public_key.clone(),
            recipient: envelope.recipient.clone(),
            amount: envelope.amount,
            signature: envelope.signature.clone(),
            zk_proof: vec![],
            nullifier,
            timestamp: envelope.timestamp,
            fee: envelope.fee,
            nonce: envelope.nonce,
            gas_limit: envelope.gas_limit,
            gas_price: envelope.gas_price,
            proof_type: ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: envelope.chain_id,
            version: envelope.version,
        };
        transaction.get_hash()
    }
}

#[async_trait]
impl NodeApi for MockNode {
    async fn account(&self, address: &str) -> Result<NodeAccountData, NodeClientError> {
        Ok(NodeAccountData {
            address: address.to_string(),
            balance_base_units: Some(self.balance),
            balance: Some(self.balance),
            nonce: 0,
            decimals: UltraBlockchain::ULTRA_DECIMALS,
        })
    }

    async fn estimate(
        &self,
        recipient: &str,
        amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError> {
        let fee = UltraBlockchain::minimum_transfer_fee(amount_base_units);
        Ok(NodeFeeEstimateData {
            recipient: recipient.to_string(),
            amount: amount_base_units,
            fee,
            gas_limit: 500_000,
            gas_price: 1,
            total: amount_base_units + fee,
        })
    }

    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError> {
        if *self.fail_first_submit.lock().unwrap() {
            *self.fail_first_submit.lock().unwrap() = false;
            return Err(NodeClientError::Unavailable);
        }
        assert_eq!(
            envelope.sender,
            self.faucet_address.lock().unwrap().as_str()
        );
        self.submitted.lock().unwrap().push(envelope.clone());
        Ok(NodeTransactionData {
            hash: hex::encode(Self::transaction_hash(envelope)),
            status: if self.confirm { "confirmed" } else { "pending" }.into(),
        })
    }

    async fn transaction_status(&self, hash: &str) -> Result<NodeTransactionData, NodeClientError> {
        if self.submitted.lock().unwrap().is_empty() {
            return Err(NodeClientError::NotFound);
        }
        Ok(NodeTransactionData {
            hash: hash.to_string(),
            status: "confirmed".into(),
        })
    }
}

fn service(mock: Arc<MockNode>) -> FaucetService {
    let keypair = QuantumKeyPair::generate();
    let address = keypair.address();
    let signer = Arc::new(FaucetSigner::from_keypair_for_tests(keypair).unwrap());
    let config = FaucetConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        node_api_base_url: "http://127.0.0.1:8081".into(),
        faucet_address: address,
        claim_amount_base_units: 1_000_000,
        daily_debit_cap_base_units: 100_000_000,
        min_balance_reserve_base_units: 200_000_000,
        address_cooldown_seconds: 86_400,
        max_queue_length: 100,
        max_submission_attempts: 5,
        confirmation_timeout_seconds: 900,
        enabled: true,
        captcha_provider: "turnstile".into(),
        state_path: "unused-test.db".into(),
        signer_credential: "unused-signer".into(),
        turnstile_secret_credential: "unused-turnstile".into(),
        abuse_key_credential: "unused-abuse".into(),
        operator_token_credential: "unused-operator".into(),
    };
    FaucetService::new(
        config,
        FaucetStore::open_in_memory().unwrap(),
        signer,
        mock,
        StaticCaptchaVerifier::new(true),
        Zeroizing::new(vec![1; 32]),
        Zeroizing::new(vec![2; 32]),
    )
    .unwrap()
}

#[tokio::test]
async fn worker_persists_and_confirms_one_signed_payout() {
    let mock = Arc::new(MockNode {
        faucet_address: Mutex::new(String::new()),
        balance: 1_000_000_000,
        submitted: Mutex::new(Vec::new()),
        confirm: true,
        fail_first_submit: Mutex::new(false),
    });
    let service = service(mock.clone());
    // The helper creates its own signer address; use its configured address for the claim fixture.
    let destination = "a".repeat(64);
    let (result, bundle) = service
        .admit_claim(
            &destination,
            "test-token",
            &"i".repeat(16),
            None,
            1_785_000_000,
        )
        .await
        .unwrap();
    assert_eq!(result, AdmissionResult::Created);
    assert_eq!(bundle.claim.source_debit_base_units, 1_010_000);

    // The mock's address must match the service signer address for the worker preflight.
    let signer_address = service.signer.address().to_string();
    mock.faucet_address
        .lock()
        .unwrap()
        .clone_from(&signer_address);
    service.process_next().await.unwrap();
    let stored = service
        .store
        .claim(&bundle.claim.claim_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.claim.status.as_str(), "confirmed");
    assert_eq!(mock.submitted.lock().unwrap().len(), 1);

    service.process_next().await.unwrap();
    let confirmed = service
        .store
        .claim(&bundle.claim.claim_id)
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.claim.status.as_str(), "confirmed");
    assert_eq!(mock.submitted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn transport_failure_reuses_the_persisted_envelope() {
    let mock = Arc::new(MockNode {
        faucet_address: Mutex::new(String::new()),
        balance: 1_000_000_000,
        submitted: Mutex::new(Vec::new()),
        confirm: true,
        fail_first_submit: Mutex::new(true),
    });
    let service = service(mock.clone());
    mock.faucet_address
        .lock()
        .unwrap()
        .clone_from(&service.signer.address().to_string());
    let (_, bundle) = service
        .admit_claim(
            &"b".repeat(64),
            "test-token",
            &"j".repeat(16),
            None,
            UltraNet::auth::now_seconds(),
        )
        .await
        .unwrap();

    service.process_next().await.unwrap();
    let after_timeout = service
        .store
        .claim(&bundle.claim.claim_id)
        .unwrap()
        .unwrap();
    let persisted = after_timeout.payout.signed_envelope.clone().unwrap();
    assert_eq!(after_timeout.claim.status.as_str(), "submitting");

    service.process_next().await.unwrap();
    let after_retry = service
        .store
        .claim(&bundle.claim.claim_id)
        .unwrap()
        .unwrap();
    assert_eq!(after_retry.claim.status.as_str(), "confirmed");
    let submitted = mock.submitted.lock().unwrap();
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].nullifier, persisted.nullifier);
    assert_eq!(submitted[0].nonce, persisted.nonce);
    assert_eq!(submitted[0].signature, persisted.signature);
}
