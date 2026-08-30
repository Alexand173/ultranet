use crate::faucet::{
    models::{ApiResponse, ClaimData, CreateClaimRequest},
    service::{FaucetError, FaucetService},
};
use actix_web::{
    http::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER},
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use serde_json::json;
use std::{io, sync::Arc};
use tokio::sync::watch;

const MAX_JSON_BODY_BYTES: usize = 16 * 1024;

pub async fn run_server(
    service: Arc<FaucetService>,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let bind = service.config.bind;
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(service.clone()))
            .app_data(web::JsonConfig::default().limit(MAX_JSON_BODY_BYTES))
            .configure(configure_routes)
    })
    .bind(bind)?
    .run();

    tokio::select! {
        result = server => result,
        _ = shutdown.changed() => Ok(()),
    }
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/faucet/claims", web::post().to(create_claim))
        .route("/api/faucet/claims/{claim_id}", web::get().to(get_claim))
        .route("/api/faucet/status", web::get().to(get_status))
        .route("/internal/health", web::get().to(internal_health))
        .route("/internal/status", web::get().to(internal_status))
        .route("/internal/metrics", web::get().to(internal_metrics))
        .route("/internal/enable", web::post().to(internal_enable))
        .route("/internal/disable", web::post().to(internal_disable))
        .route("/internal/reconcile", web::post().to(internal_reconcile));
}

async fn create_claim(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
    body: Result<web::Json<CreateClaimRequest>, actix_web::Error>,
) -> HttpResponse {
    if !request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim() == "application/json")
        })
    {
        return error_response(FaucetError::InvalidRequest);
    }
    let body = match body {
        Ok(body) => body,
        Err(_) => return error_response(FaucetError::InvalidRequest),
    };
    let Some(idempotency_key) = request
        .headers()
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
    else {
        return error_response(FaucetError::InvalidRequest);
    };
    let client_identity = trusted_client_identity(&request);
    let now = crate::auth::now_seconds();
    match service
        .admit_claim(
            &body.address,
            &body.captcha_token,
            idempotency_key,
            client_identity.as_deref(),
            now,
        )
        .await
    {
        Ok((_, bundle)) => {
            let claim = ClaimData {
                claim_id: bundle.claim.claim_id.clone(),
                status: bundle.claim.status,
                address: bundle.claim.address.clone(),
                amount_base_units: bundle.claim.amount_base_units,
                amount_ultra: crate::UltraBlockchain::format_base_units(
                    bundle.claim.amount_base_units,
                ),
                decimals: crate::UltraBlockchain::ULTRA_DECIMALS,
                retry_after_seconds: bundle.claim.cooldown_until.saturating_sub(now),
            };
            HttpResponse::Accepted().json(ApiResponse {
                success: true,
                message: "Faucet claim queued".into(),
                data: Some(claim),
            })
        }
        Err(error) => error_response(error),
    }
}

async fn get_claim(
    service: web::Data<Arc<FaucetService>>,
    claim_id: web::Path<String>,
) -> HttpResponse {
    let claim_id = claim_id.into_inner();
    if !is_claim_id(&claim_id) {
        return error_response(FaucetError::InvalidRequest);
    }
    match service.store.claim(&claim_id) {
        Ok(Some(bundle)) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Faucet claim status".into(),
            data: Some(service.claim_data(&bundle)),
        }),
        Ok(None) => HttpResponse::NotFound().json(ApiResponse::<()> {
            success: false,
            message: "Faucet claim not found".into(),
            data: None,
        }),
        Err(_) => error_response(FaucetError::Store(
            crate::faucet::store::StoreError::Malformed("claim lookup failed".into()),
        )),
    }
}

async fn get_status(service: web::Data<Arc<FaucetService>>) -> HttpResponse {
    match service.public_status() {
        Ok(status) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Faucet status".into(),
            data: Some(status),
        }),
        Err(error) => error_response(error),
    }
}

async fn internal_health(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let state = match service.store.service_state() {
        Ok(state) => state,
        Err(error) => return error_response(FaucetError::Store(error)),
    };
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "enabled": state.enabled,
            "signer_loaded": true,
            "schema_version": state.schema_version,
        }
    }))
}

async fn internal_status(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let now = crate::auth::now_seconds();
    let state = match service.store.service_state() {
        Ok(state) => state,
        Err(error) => return error_response(FaucetError::Store(error)),
    };
    let budget = match service.store.budget_snapshot(now) {
        Ok(budget) => budget,
        Err(error) => return error_response(FaucetError::Store(error)),
    };
    let queue_depth = match service.store.queue_depth() {
        Ok(depth) => depth,
        Err(error) => return error_response(FaucetError::Store(error)),
    };
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "enabled": state.enabled,
            "kill_switch_reason": state.kill_switch_reason,
            "signer_key_id": state.signer_key_id,
            "faucet_address": state.faucet_address,
            "last_observed_nonce": state.last_observed_nonce,
            "last_node_health_at": state.last_node_health_at,
            "queue_depth": queue_depth,
            "budget": {
                "window_start": budget.window_start,
                "window_end": budget.window_end,
                "reserved_base_units": budget.reserved_base_units,
                "confirmed_base_units": budget.confirmed_base_units,
                "claim_count": budget.claim_count,
            }
        }
    }))
}

async fn internal_metrics(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let queue_depth = service.store.queue_depth().unwrap_or_default();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(format!("ultranet_faucet_queue_depth {queue_depth}\n"))
}

async fn internal_enable(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let _operator_guard = service.operator_lock.lock().await;
    match service.enable() {
        Ok(()) => {
            HttpResponse::Ok().json(json!({ "success": true, "message": "Faucet intake enabled" }))
        }
        Err(error) => error_response(error),
    }
}

async fn internal_disable(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let _operator_guard = service.operator_lock.lock().await;
    match service.disable("operator kill switch") {
        Ok(()) => {
            HttpResponse::Ok().json(json!({ "success": true, "message": "Faucet intake disabled" }))
        }
        Err(error) => error_response(error),
    }
}

async fn internal_reconcile(
    service: web::Data<Arc<FaucetService>>,
    request: HttpRequest,
) -> HttpResponse {
    if !operator_authorized(&request, &service) {
        return unauthorized();
    }
    let _operator_guard = service.operator_lock.lock().await;
    match service.process_next().await {
        Ok(processed) => {
            HttpResponse::Ok().json(json!({ "success": true, "processed": processed }))
        }
        Err(error) => error_response(error),
    }
}

fn error_response(error: FaucetError) -> HttpResponse {
    let (status, retry_after, message) = match error {
        FaucetError::InvalidRequest => (
            actix_web::http::StatusCode::BAD_REQUEST,
            None,
            "Invalid faucet request",
        ),
        FaucetError::CaptchaRejected => (
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
            None,
            "Anti-bot verification failed",
        ),
        FaucetError::CaptchaUnavailable => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(30),
            "Faucet anti-bot verification is temporarily unavailable",
        ),
        FaucetError::Disabled => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(60),
            "Faucet is temporarily unavailable",
        ),
        FaucetError::QueueFull => (
            actix_web::http::StatusCode::TOO_MANY_REQUESTS,
            Some(60),
            "Faucet queue is temporarily full",
        ),
        FaucetError::BudgetExhausted => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(60),
            "Faucet is temporarily unavailable",
        ),
        FaucetError::AddressCooldown(seconds) => (
            actix_web::http::StatusCode::CONFLICT,
            Some(seconds.min(86_400)),
            "This address has an active faucet cooldown",
        ),
        FaucetError::IdempotencyConflict => (
            actix_web::http::StatusCode::CONFLICT,
            None,
            "Idempotency key is already bound to another request",
        ),
        FaucetError::RateLimited(seconds) => (
            actix_web::http::StatusCode::TOO_MANY_REQUESTS,
            Some(seconds.min(86_400)),
            "Faucet request rate limit reached",
        ),
        FaucetError::NodeUnavailable | FaucetError::SignerUnavailable => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(30),
            "Faucet is temporarily unavailable",
        ),
        FaucetError::NodeRejected
        | FaucetError::SignerInvalid
        | FaucetError::ConfirmationTimeout => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(30),
            "Faucet payout could not be submitted",
        ),
        FaucetError::Store(_) => (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            Some(30),
            "Faucet state is temporarily unavailable",
        ),
    };
    let response = HttpResponse::build(status).json(ApiResponse::<()> {
        success: false,
        message: message.into(),
        data: None,
    });
    if let Some(seconds) = retry_after {
        let mut response = response;
        response.headers_mut().insert(
            RETRY_AFTER,
            actix_web::http::header::HeaderValue::from_str(&seconds.to_string()).unwrap(),
        );
        response
    } else {
        response
    }
}

fn operator_authorized(request: &HttpRequest, service: &FaucetService) -> bool {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| service.verify_operator_token(candidate))
}

fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header(("WWW-Authenticate", "Bearer"))
        .json(ApiResponse::<()> {
            success: false,
            message: "Operator authentication required".into(),
            data: None,
        })
}

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

fn is_claim_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_ids_are_lowercase_hex_only() {
        assert!(is_claim_id(&"a1".repeat(16)));
        assert!(!is_claim_id(&"A1".repeat(16)));
        assert!(!is_claim_id("too-short"));
    }
}
