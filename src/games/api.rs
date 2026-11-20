//! The Games Reward Service's HTTP API, all under `/api/games`. (Third-party games and the generic
//! `/api/records` and `/api/apps` routes belong to the Records Service: see `records_api.rs`.)
//!
//! ```text
//! GET  /api/games/status                  the rules in force, and whether results and payouts are open
//! GET  /api/games/grid/day?date=          the seed inputs of a Grid day, so a phone can check its own copy
//! POST /api/games/grid/answers            {address, date, level, answer:[{cell,turns}]}
//! POST /api/games/chess/results           {address, game_id, mode, result, plies, app?}
//! GET  /api/games/wallet/{address}        the balance, today's counts and recent wins
//! POST /api/games/withdraw                {address}   queue the whole balance for withdrawal
//! GET  /api/games/payouts/{id}            where a withdrawal stands
//! ```
//!
//! There is no login. Money only ever flows **to the address that was credited**, so submitting
//! someone else's answer credits *them*, and a withdrawal pays the address it names and no other. That
//! removes the one piece of state (a session) that could be stolen. What it cannot do is stop abuse,
//! which is the job of the caps, the chain-presence rule and the reverse proxy's rate limits.

use super::service::{
    date_start, shift_date, ChessResultRequest, GamesError, GamesService, GridAnswerRequest,
};
use crate::ultragrid;
use actix_web::{http::StatusCode, web, App, HttpRequest, HttpResponse, HttpServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::watch;

// A post-quantum public key and signature are about 7 KB each as hex; ownership proofs carry both.
pub(super) const MAX_JSON_BODY_BYTES: usize = 32 * 1024;

/// The JSON body rules shared by both services: a size limit, and a malformed body is our own 400.
pub(super) fn json_config() -> web::JsonConfig {
    web::JsonConfig::default()
        .limit(MAX_JSON_BODY_BYTES)
        .error_handler(|error, _| {
            let response = error_body(
                "invalid_request",
                &format!("the request body is not valid: {error}"),
                None,
            );
            actix_web::error::InternalError::from_response(error, response).into()
        })
}

pub fn configure_routes(config: &mut web::ServiceConfig) {
    config
        .app_data(json_config())
        .route("/api/games/status", web::get().to(status))
        .route("/api/games/grid/day", web::get().to(grid_day))
        .route("/api/games/grid/answers", web::post().to(grid_answer))
        .route("/api/games/chess/results", web::post().to(chess_result))
        .route("/api/games/wallet/{address}", web::get().to(wallet))
        .route("/api/games/withdraw", web::post().to(withdraw))
        .route("/api/games/payouts/{id}", web::get().to(payout));
}

pub async fn run_server(
    service: Arc<GamesService>,
    bind: std::net::SocketAddr,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let data = web::Data::from(service);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .configure(configure_routes)
    })
    .bind(bind)?
    .shutdown_timeout(10)
    .run();
    let handle = server.handle();
    tokio::spawn(async move {
        let _ = shutdown.changed().await;
        handle.stop(true).await;
    });
    server.await
}

pub(super) fn error_body(code: &str, message: &str, extra: Option<Value>) -> HttpResponse {
    let mut body = json!({ "success": false, "code": code, "message": message });
    if let (Some(Value::Object(extra)), Value::Object(map)) = (extra, &mut body) {
        map.extend(extra);
    }
    HttpResponse::build(StatusCode::BAD_REQUEST)
        .insert_header(("Cache-Control", "no-store"))
        .json(body)
}

pub(super) fn error_response(error: &GamesError) -> HttpResponse {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let message = match error {
        // The detail of an internal failure is for the operator's log, not for a client.
        GamesError::Internal(detail) => {
            tracing::error!(detail = %detail, "games request failed internally");
            "something went wrong on the server".to_string()
        }
        other => other.to_string(),
    };
    let mut body = json!({ "success": false, "code": error.code(), "message": message });
    if let Value::Object(map) = &mut body {
        match error {
            GamesError::DateMismatch { server_date } => {
                map.insert("server_date".into(), json!(server_date));
            }
            GamesError::BelowMinimum { available, minimum } => {
                map.insert("available_units".into(), json!(available));
                map.insert("min_withdraw_units".into(), json!(minimum));
            }
            _ => {}
        }
    }
    HttpResponse::build(status)
        .insert_header(("Cache-Control", "no-store"))
        .json(body)
}

pub(super) fn respond<T: Serialize>(result: Result<T, GamesError>) -> HttpResponse {
    match result {
        Ok(value) => {
            let mut body = serde_json::to_value(value).unwrap_or(Value::Null);
            if let Value::Object(map) = &mut body {
                map.insert("success".into(), json!(true));
            }
            HttpResponse::Ok()
                .insert_header(("Cache-Control", "no-store"))
                .json(body)
        }
        Err(error) => error_response(&error),
    }
}

/// The caller's address as the reverse proxy reports it, trusted only from a loopback peer: anything
/// else could be forged, and a forged identity would let one client dodge, or pin, another's limit.
fn trusted_client_identity(request: &HttpRequest) -> Option<String> {
    let peer_is_loopback = request
        .peer_addr()
        .is_some_and(|peer| peer.ip().is_loopback());
    if !peer_is_loopback {
        return None;
    }
    request
        .headers()
        .get("X-Forwarded-For")
        .or_else(|| request.headers().get("X-Real-IP"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64 && value.is_ascii())
        .map(ToOwned::to_owned)
}

async fn status(service: web::Data<GamesService>) -> HttpResponse {
    respond(service.status())
}

#[derive(Deserialize)]
struct DayQuery {
    date: Option<String>,
}

async fn grid_day(service: web::Data<GamesService>, query: web::Query<DayQuery>) -> HttpResponse {
    let today = service.today();
    let date = query.date.clone().unwrap_or_else(|| today.clone());
    // Only today and yesterday: every other date would make the service read the ledger and write a
    // row for a day nobody is playing, which a client could repeat without end.
    let yesterday = shift_date(&today, -1).unwrap_or_default();
    if date != today && date != yesterday {
        return error_response(&GamesError::Invalid(
            "puzzles are published for today and yesterday only".into(),
        ));
    }
    respond(service.grid_day(&date).await.map(|day| {
        let closes_at = date_start(&date).map(|start| start + 86_400);
        json!({
            "date": day.date,
            "server_date": today,
            "chain_id": service.policy().grid.chain_id,
            "previous_anchor": day.prev_anchor_sequence.map(|sequence| json!({
                "sequence": sequence,
                "root": hex::encode(day.prev_root),
            })),
            "seed": hex::encode(day.seed),
            "commitment": hex::encode(day.commitment),
            "levels": ultragrid::LEVELS_PER_DAY,
            "closes_at": closes_at,
        })
    }))
}

async fn grid_answer(
    service: web::Data<GamesService>,
    request: HttpRequest,
    body: web::Json<GridAnswerRequest>,
) -> HttpResponse {
    let client = trusted_client_identity(&request);
    respond(
        service
            .submit_grid_answer(body.into_inner(), client.as_deref())
            .await,
    )
}

async fn chess_result(
    service: web::Data<GamesService>,
    request: HttpRequest,
    body: web::Json<ChessResultRequest>,
) -> HttpResponse {
    let client = trusted_client_identity(&request);
    respond(
        service
            .submit_chess_result(body.into_inner(), client.as_deref())
            .await,
    )
}

async fn wallet(service: web::Data<GamesService>, address: web::Path<String>) -> HttpResponse {
    respond(service.wallet(&address))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WithdrawRequest {
    address: String,
}

async fn withdraw(
    service: web::Data<GamesService>,
    body: web::Json<WithdrawRequest>,
) -> HttpResponse {
    respond(service.withdraw(&body.address))
}

async fn payout(service: web::Data<GamesService>, id: web::Path<String>) -> HttpResponse {
    let id = id.into_inner();
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return error_response(&GamesError::Invalid(
            "a payout id is 32 lowercase hexadecimal characters".into(),
        ));
    }
    match service.payout_status(&id) {
        Ok(Some(view)) => respond(Ok(view)),
        Ok(None) => HttpResponse::NotFound()
            .insert_header(("Cache-Control", "no-store"))
            .json(json!({ "success": false, "code": "not_found", "message": "no such payout" })),
        Err(error) => error_response(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, test::TestRequest};

    fn request_from(peer: &str, forwarded: Option<&str>) -> HttpRequest {
        let mut request = TestRequest::default().peer_addr(peer.parse().unwrap());
        if let Some(value) = forwarded {
            request = request.insert_header(("X-Forwarded-For", value));
        }
        request.to_http_request()
    }

    #[test]
    fn the_forwarded_address_is_trusted_only_from_the_local_proxy() {
        assert_eq!(
            trusted_client_identity(&request_from(
                "127.0.0.1:5000",
                Some("198.51.100.7, 10.0.0.1")
            ))
            .as_deref(),
            Some("198.51.100.7")
        );
        // A client talking to the service directly cannot choose its own identity.
        assert_eq!(
            trusted_client_identity(&request_from("203.0.113.5:5000", Some("198.51.100.7"))),
            None
        );
        assert_eq!(
            trusted_client_identity(&request_from("127.0.0.1:5000", None)),
            None
        );
        assert_eq!(
            trusted_client_identity(&request_from("127.0.0.1:5000", Some(&"9".repeat(65)))),
            None
        );
        assert_eq!(
            trusted_client_identity(&request_from("127.0.0.1:5000", Some(""))),
            None
        );
    }

    #[actix_web::test]
    async fn an_internal_failure_never_shows_its_detail_to_the_client() {
        let response = error_response(&GamesError::Internal(
            "database at /var/lib/ultranet-games exploded".into(),
        ));
        assert_eq!(response.status(), 500);
        let body =
            String::from_utf8(to_bytes(response.into_body()).await.unwrap().to_vec()).unwrap();
        assert!(!body.contains("/var/lib"), "{body}");
        assert!(body.contains("internal_error"));
    }

    #[actix_web::test]
    async fn the_error_codes_and_statuses_are_stable() {
        let cases = [
            (GamesError::Disabled, 503, "service_disabled"),
            (GamesError::PayoutsPaused, 503, "payouts_paused"),
            (GamesError::Invalid("x".into()), 400, "invalid_request"),
            (
                GamesError::DateMismatch {
                    server_date: "2026-11-16".into(),
                },
                400,
                "date_mismatch",
            ),
            (GamesError::AnswerRefused("x".into()), 400, "answer_refused"),
            (GamesError::ChainUnavailable, 503, "chain_unavailable"),
            (GamesError::Conflict, 409, "game_id_conflict"),
            (
                GamesError::BelowMinimum {
                    available: 1,
                    minimum: 100,
                },
                429,
                "below_minimum",
            ),
            (GamesError::PayoutDailyCap, 429, "payout_daily_cap"),
            (GamesError::InvalidKey, 401, "invalid_key"),
            (GamesError::GameSuspended, 403, "game_suspended"),
            (GamesError::ResultConflict, 409, "result_conflict"),
            (GamesError::GameLimit, 429, "game_daily_limit"),
            (GamesError::OwnerLimit, 429, "owner_game_limit"),
            (GamesError::RegistrationLimit, 429, "registration_limit"),
            (GamesError::PlayerLimit, 429, "player_daily_limit"),
            (GamesError::NotFound("x".into()), 404, "not_found"),
        ];
        for (error, status, code) in cases {
            assert_eq!(error.status(), status);
            assert_eq!(error.code(), code);
        }
    }
}
