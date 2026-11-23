// ============================================================
// API ERROR ENVELOPE — one shape for every failure, including the extractor's
// ============================================================
//
// A client that cannot tell "malformed" from "rejected" cannot build error handling. Before
// `json_error_config`, actix answered a body that was not valid JSON (or had no content type) with
// `text/plain`, while every handler answered the JSON envelope — so an SDK had to branch on the content
// type before it could parse anything.
//
// These tests assert three things: the shape (one envelope), the status codes (400 kept distinct from
// 413, so a client that retries on 400 does not retry on 413), and that the configuration covers routes
// other than the AppChain one. The last point is the reason it lives on the app rather than in a
// handler: a per-handler fix would leave every other JSON route inconsistent.

use actix_web::{test as actix_test, web, App, HttpResponse, Responder};
use serde_json::{json, Value};
use UltraNet::api::{json_error_config, AppChainTxRequest};

async fn accept(_request: web::Json<AppChainTxRequest>) -> impl Responder {
    HttpResponse::Ok().json(json!({ "success": true }))
}

async fn echo(_body: web::Json<Value>) -> impl Responder {
    HttpResponse::Ok().json(json!({ "success": true }))
}

/// Configured exactly as the node configures its app: the error configuration is `app_data`, so it
/// applies to every JSON route, and these tests can therefore prove it does.
macro_rules! test_app {
    () => {{
        actix_test::init_service(
            App::new()
                .app_data(json_error_config())
                .route("/api/appchain/tx", web::post().to(accept))
                .route("/api/transaction", web::post().to(echo)),
        )
        .await
    }};
}

/// Post a raw payload and return (status, is the response JSON, parsed body).
macro_rules! post_raw {
    ($app:expr, $uri:expr, $payload:expr, $content_type:expr) => {{
        let mut request = actix_test::TestRequest::post().uri($uri).set_payload($payload);
        if let Some(content_type) = $content_type {
            request = request.insert_header(("content-type", content_type));
        }
        let response = actix_test::call_service(&$app, request.to_request()).await;
        let status = response.status().as_u16();
        let answered_as_json = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.starts_with("application/json"))
            .unwrap_or(false);
        let body: Value = actix_test::read_body_json(response).await;
        (status, answered_as_json, body)
    }};
}

/// A body that is well-formed for the envelope. Field lengths are irrelevant here: nothing in this file
/// reaches admission, because every case fails or succeeds at deserialisation. Lengths belong to
/// `appchain_wire_format_test`.
fn valid_body() -> Value {
    json!({
        "sender": "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e",
        "sender_public_key": vec![1u8; 8],
        "nonce": 0,
        "timestamp": 1_800_000_000u64,
        "nullifier": vec![0u8; 32],
        "fee": 10_000u64,
        "signature": vec![2u8; 8],
        "version": 5,
        "operation": { "register": { "name": "UltraNet Grid" } }
    })
}

fn assert_contract_error(status: u16, answered_as_json: bool, body: &Value, expected_status: u16) {
    assert_eq!(status, expected_status, "wrong status: {body}");
    assert!(
        answered_as_json,
        "an API error must be JSON, not text/plain: {body}"
    );
    assert_eq!(body["success"].as_bool(), Some(false), "{body}");
    assert_eq!(body["data"], Value::Null, "{body}");
    assert!(
        body["message"].as_str().is_some_and(|message| !message.is_empty()),
        "an error must carry a message: {body}"
    );
}

#[actix_web::test]
async fn malformed_json_returns_the_contract_envelope() {
    let app = test_app!();
    let (status, answered_as_json, body) =
        post_raw!(app, "/api/appchain/tx", "{", Some("application/json"));

    assert_contract_error(status, answered_as_json, &body, 400);
    assert!(
        body["message"].as_str().unwrap_or_default().contains("Malformed request"),
        "the message must say the request was malformed: {body}"
    );
}

#[actix_web::test]
async fn an_unknown_field_returns_the_contract_envelope() {
    let app = test_app!();
    let mut body = valid_body();
    body["extension"] = json!(1);

    let (status, answered_as_json, response_body) =
        post_raw!(app, "/api/appchain/tx", body.to_string(), Some("application/json"));

    assert_contract_error(status, answered_as_json, &response_body, 400);
    assert!(
        response_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown field"),
        "the message must name the cause: {response_body}"
    );
}

#[actix_web::test]
async fn a_wrong_field_type_returns_the_contract_envelope() {
    let app = test_app!();
    let mut body = valid_body();
    body["version"] = json!("five");

    let (status, answered_as_json, response_body) =
        post_raw!(app, "/api/appchain/tx", body.to_string(), Some("application/json"));

    assert_contract_error(status, answered_as_json, &response_body, 400);
}

#[actix_web::test]
async fn a_missing_content_type_returns_the_contract_envelope() {
    let app = test_app!();
    let (status, answered_as_json, body) =
        post_raw!(app, "/api/appchain/tx", "{}", None::<&str>);

    assert_contract_error(status, answered_as_json, &body, 400);
}

/// 413 must survive as 413. A client retries a 400 with a corrected body; retrying a 413 unchanged just
/// fails again, and reporting it as 400 would teach the client the wrong lesson.
#[actix_web::test]
async fn an_oversized_body_returns_413_not_400() {
    let app = test_app!();
    let payload = format!("{{\"blob\":\"{}\"}}", "a".repeat(2_200_000));

    let (status, answered_as_json, body) =
        post_raw!(app, "/api/appchain/tx", payload, Some("application/json"));

    assert_contract_error(status, answered_as_json, &body, 413);
}

/// The configuration is on the app, not in a handler, so a second route must behave identically. This is
/// the assertion that fails if someone later "fixes" only the AppChain handler.
#[actix_web::test]
async fn the_envelope_covers_every_json_route() {
    let app = test_app!();
    let (status, answered_as_json, body) =
        post_raw!(app, "/api/transaction", "not json at all", Some("application/json"));

    assert_contract_error(status, answered_as_json, &body, 400);
}

/// And the change must not have turned every response into an error envelope.
#[actix_web::test]
async fn a_missing_route_is_still_a_404() {
    let app = test_app!();
    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::get().uri("/no/such/route").to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 404);
}
