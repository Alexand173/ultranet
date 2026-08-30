use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;
use UltraNet::faucet::{
    api,
    captcha::StaticCaptchaVerifier,
    config::FaucetConfig,
    models::{NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest},
    node_client::{NodeApi, NodeClientError},
    service::FaucetService,
    signer::FaucetSigner,
    store::FaucetStore,
};
use UltraNet::QuantumKeyPair;

struct UnusedNode {
    calls: Mutex<u32>,
}

#[async_trait]
impl NodeApi for UnusedNode {
    async fn account(&self, _address: &str) -> Result<NodeAccountData, NodeClientError> {
        *self.calls.lock().unwrap() += 1;
        Err(NodeClientError::Unavailable)
    }

    async fn estimate(
        &self,
        _recipient: &str,
        _amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError> {
        Err(NodeClientError::Unavailable)
    }

    async fn submit(
        &self,
        _envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError> {
        Err(NodeClientError::Unavailable)
    }

    async fn transaction_status(
        &self,
        _hash: &str,
    ) -> Result<NodeTransactionData, NodeClientError> {
        Err(NodeClientError::Unavailable)
    }
}

fn service() -> Arc<FaucetService> {
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
        state_path: "unused-api-test.db".into(),
        signer_credential: "unused-signer".into(),
        turnstile_secret_credential: "unused-turnstile".into(),
        abuse_key_credential: "unused-abuse".into(),
        operator_token_credential: "unused-operator".into(),
    };
    Arc::new(
        FaucetService::new(
            config,
            FaucetStore::open_in_memory().unwrap(),
            signer,
            Arc::new(UnusedNode {
                calls: Mutex::new(0),
            }),
            StaticCaptchaVerifier::new(true),
            Zeroizing::new(vec![1; 32]),
            Zeroizing::new(vec![2; 32]),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn public_claim_contract_is_strict_and_idempotent() {
    let service = service();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(actix_web::web::Data::new(service.clone()))
            .app_data(actix_web::web::JsonConfig::default().limit(16 * 1024))
            .configure(api::configure_routes),
    )
    .await;

    let destination = "a".repeat(64);
    let missing_key = actix_web::test::TestRequest::post()
        .uri("/api/faucet/claims")
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "address": destination.clone(),
            "captcha_token": "test-token"
        }))
        .to_request();
    let response = actix_web::test::call_service(&app, missing_key).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::BAD_REQUEST);

    let valid = actix_web::test::TestRequest::post()
        .uri("/api/faucet/claims")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Idempotency-Key", "i".repeat(16)))
        .set_json(serde_json::json!({
            "address": destination,
            "captcha_token": "test-token"
        }))
        .to_request();
    let response = actix_web::test::call_service(&app, valid).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::ACCEPTED);
    let body: serde_json::Value = actix_web::test::read_body_json(response).await;
    assert!(body["data"]["claim_id"].is_string());
    assert_eq!(body["data"]["amount_base_units"], 1_000_000);
    assert_eq!(body["data"]["amount_ultra"], "1.000000");
    assert!(body.get("signature").is_none());
    assert!(body.to_string().find("private_key").is_none());
    assert!(body.to_string().find("captcha_token").is_none());

    let duplicate = actix_web::test::TestRequest::post()
        .uri("/api/faucet/claims")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Idempotency-Key", "i".repeat(16)))
        .set_json(serde_json::json!({
            "address": "a".repeat(64),
            "captcha_token": "expired-or-replayed-token"
        }))
        .to_request();
    let response = actix_web::test::call_service(&app, duplicate).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::ACCEPTED);

    let changed = actix_web::test::TestRequest::post()
        .uri("/api/faucet/claims")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Idempotency-Key", "i".repeat(16)))
        .set_json(serde_json::json!({
            "address": "b".repeat(64),
            "captcha_token": "test-token"
        }))
        .to_request();
    let response = actix_web::test::call_service(&app, changed).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn operator_routes_require_the_separate_faucet_token() {
    let service = service();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(actix_web::web::Data::new(service))
            .configure(api::configure_routes),
    )
    .await;
    let missing = actix_web::test::call_service(
        &app,
        actix_web::test::TestRequest::get()
            .uri("/internal/status")
            .to_request(),
    )
    .await;
    assert_eq!(missing.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}
