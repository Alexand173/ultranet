//! The versioned public read API: `/api/records/v1/…`.
//!
//! Reads only. Nothing here writes, anchors, rotates a key or touches a credential, and every route
//! ignores `Authorization` exactly as it ignores the absence of one.
//!
//! Two rules shape this module:
//!
//! * **Methods.** Any method other than GET and HEAD answers 404, not 405: there is no method
//!   enumeration and no accidental write surface. That is why one dispatcher serves the whole prefix
//!   instead of one actix resource per path, which would answer 405 for a path it knows.
//! * **Nothing is claimed beyond what is stored.** `closed` means closed and not anchored;
//!   `anchored` means the transaction was submitted and is pending inclusion, not mined; `confirmed`
//!   means the L1 anchor index matched the day's root, with no finality depth and no signer check.
//!   That index is searched only over its newest window, which the payload declares as
//!   `scanned_anchors`.
//!
//! Every response — including a 404 — carries `success`, `schema_version`, `as_of`, `request_id`,
//! `X-Request-Id`, `Cache-Control: no-store` and no `ETag`. Caching stays off until a closed day is
//! proven immutable; it is not, because `state` and `anchor_tx` are rewritten after the close.

use super::{records::RecordsService, service::GamesError};
use actix_web::{
    http::{Method, StatusCode},
    web, HttpRequest, HttpResponse,
};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Bumped when a field is added. A client ignores fields it does not know.
const SCHEMA_VERSION: u32 = 1;
const DEFAULT_LIMIT: usize = 30;
const MAX_LIMIT: usize = 100;
/// How many of the newest anchors the L1 anchor index is searched for a confirmation.
const ANCHOR_INDEX_WINDOW: u32 = 150;
const PREFIX: &str = "/api/records/v1";

/// The public read routes. Registered from `records_api::configure_records_routes`: additive, and it
/// changes no legacy route.
pub fn configure_v1_routes(config: &mut web::ServiceConfig) {
    config
        .route(PREFIX, web::to(dispatch))
        .route("/api/records/v1/{tail:.*}", web::to(dispatch));
}

#[derive(Debug, Deserialize)]
struct DaysQuery {
    limit: Option<usize>,
    before: Option<String>,
}

async fn dispatch(service: web::Data<RecordsService>, request: HttpRequest) -> HttpResponse {
    let id = request_id(&request);
    let as_of = as_of(service.now());
    let method = request.method().clone();
    let head_only = method == Method::HEAD;
    if method != Method::GET && !head_only {
        return respond(
            &id,
            StatusCode::NOT_FOUND,
            failure(&id, &as_of, "not_found", "no such route", false),
            head_only,
        );
    }
    let tail = request
        .path()
        .strip_prefix(PREFIX)
        .unwrap_or("")
        .trim_matches('/')
        .to_string();
    let segments: Vec<&str> = if tail.is_empty() {
        Vec::new()
    } else {
        tail.split('/').collect()
    };
    match segments.as_slice() {
        ["status"] => service_status(&service, &id, &as_of, head_only),
        ["chains", chain, "status"] => chain_status(&service, &id, &as_of, chain, head_only),
        ["chains", chain, "days"] => days(&service, &request, &id, &as_of, chain, head_only),
        ["chains", chain, "days", date] => day(&service, &id, &as_of, chain, date, head_only),
        ["chains", chain, "days", date, "apps", app, "proof"] => {
            proof(&service, &id, &as_of, chain, date, app, head_only)
        }
        _ => respond(
            &id,
            StatusCode::NOT_FOUND,
            failure(&id, &as_of, "not_found", "no such route", false),
            head_only,
        ),
    }
}

fn service_status(
    service: &RecordsService,
    id: &str,
    as_of: &str,
    head_only: bool,
) -> HttpResponse {
    let status = match service.status() {
        Ok(status) => status,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let served: Vec<u32> = status.registry_chain_id.into_iter().collect();
    let body = success(
        id,
        as_of,
        json!({
            "service": {
                "accepting": status.accepting,
                "anchoring": status.anchoring,
                "held": status.held,
                "version": env!("CARGO_PKG_VERSION"),
            },
            "served_chain_ids": served,
        }),
    );
    respond(id, StatusCode::OK, body, head_only)
}

/// The chain a caller asked about, but only if this service actually serves it. There is no
/// fallback to "the" registry: an arbitrary AppChain id that this service does not serve is a 404.
///
/// `Ok(None)` is that 404. `Err` is a service that cannot answer at all, which is a 500 the client
/// may retry — not a 404 that tells it to stop asking. Collapsing the two would report a database
/// failure as "this chain is not served here", which is the confusion the contract exists to avoid.
fn served_chain(service: &RecordsService, raw: &str) -> Result<Option<u32>, GamesError> {
    let status = service.status()?;
    let Ok(chain) = raw.parse::<u32>() else {
        return Ok(None);
    };
    Ok(status
        .registry_chain_id
        .filter(|configured| *configured == chain))
}

fn chain_status(
    service: &RecordsService,
    id: &str,
    as_of: &str,
    raw_chain: &str,
    head_only: bool,
) -> HttpResponse {
    let chain = match served_chain(service, raw_chain) {
        Ok(Some(chain)) => chain,
        Ok(None) => return unknown_chain(id, as_of, head_only),
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let status = match service.status() {
        Ok(status) => status,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let totals = match service.registry_day_totals() {
        Ok(totals) => totals,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let from_day = totals.from_day;
    let to_day = totals.to_day;
    let days_behind = to_day
        .as_deref()
        .and_then(|earlier| days_between(&status.date, earlier));
    let stale = days_behind.is_some_and(|behind| behind > 1);
    let body = success(
        id,
        as_of,
        json!({
            "chain_id": chain,
            "accepting": status.accepting,
            "anchoring": status.anchoring,
            "held": status.held,
            "today": status.date,
            "last_closed_day": to_day.clone(),
            "window": {
                "from_day": from_day,
                "to_day": to_day,
                "days_total": totals.days_closed,
            },
            "commitment": {
                "days_closed": totals.days_closed,
                "days_with_root": totals.days_with_root,
                "days_confirmed": totals.days_confirmed,
                "days_anchored": totals.days_anchored,
                "days_pending_anchor": totals.days_pending_anchor,
                "latest_anchored_day": totals.latest_anchored_day,
                "confirmation": {
                    "source": "l1_anchor_index",
                    "finality_checked": false,
                    "scanned_anchors": ANCHOR_INDEX_WINDOW,
                },
            },
            "stale": stale,
            "days_behind": days_behind,
        }),
    );
    respond(id, StatusCode::OK, body, head_only)
}

fn days(
    service: &RecordsService,
    request: &HttpRequest,
    id: &str,
    as_of: &str,
    raw_chain: &str,
    head_only: bool,
) -> HttpResponse {
    let chain = match served_chain(service, raw_chain) {
        Ok(Some(chain)) => chain,
        Ok(None) => return unknown_chain(id, as_of, head_only),
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let query = match web::Query::<DaysQuery>::from_query(request.query_string()) {
        Ok(query) => query.into_inner(),
        Err(_) => {
            return invalid(
                id,
                as_of,
                "limit must be a number and before must be a date",
                head_only,
            )
        }
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return invalid(id, as_of, "limit must be between 1 and 100", head_only);
    }
    if let Some(before) = query.before.as_deref() {
        if !crate::ultragrid::valid_date(before) {
            return invalid(id, as_of, "date must look like 2026-11-16", head_only);
        }
    }
    let mut rows = match service.registry_days_page(query.before.as_deref(), limit + 1) {
        Ok(rows) => rows,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_before = if has_more {
        rows.last().map(|day| day.date.clone())
    } else {
        None
    };
    let listed: Vec<Value> = rows
        .iter()
        .map(|day| {
            json!({
                "date": day.date.clone(),
                "day_root": hex::encode(day.day_root),
                "merkle_root": hex::encode(day.merkle_root),
                "state": day.state.clone(),
                "anchor_tx": day.anchor_tx.clone(),
                "app_count": day.app_count,
            })
        })
        .collect();
    let body = success(
        id,
        as_of,
        json!({
            "chain_id": chain,
            "limit": limit,
            "days": listed,
            "next_before": next_before,
            "has_more": has_more,
        }),
    );
    respond(id, StatusCode::OK, body, head_only)
}

fn day(
    service: &RecordsService,
    id: &str,
    as_of: &str,
    raw_chain: &str,
    date: &str,
    head_only: bool,
) -> HttpResponse {
    let chain = match served_chain(service, raw_chain) {
        Ok(Some(chain)) => chain,
        Ok(None) => return unknown_chain(id, as_of, head_only),
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let view = match service.registry_day_view(date) {
        Ok(view) => view,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let apps: Vec<Value> = view
        .games
        .iter()
        .map(|game| {
            json!({
                "app_id": game.id.clone(),
                "root": game.root.clone(),
            })
        })
        .collect();
    let body = success(
        id,
        as_of,
        json!({
            "chain_id": chain,
            "date": view.date,
            "state": view.state,
            "merkle_root": view.merkle_root,
            "day_root": view.day_root,
            "anchor_tx": view.anchor_tx,
            "apps": apps,
        }),
    );
    respond(id, StatusCode::OK, body, head_only)
}

fn proof(
    service: &RecordsService,
    id: &str,
    as_of: &str,
    raw_chain: &str,
    date: &str,
    app: &str,
    head_only: bool,
) -> HttpResponse {
    let chain = match served_chain(service, raw_chain) {
        Ok(Some(chain)) => chain,
        Ok(None) => return unknown_chain(id, as_of, head_only),
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let view = match service.registry_proof(app, date) {
        Ok(view) => view,
        Err(error) => return service_error(id, as_of, error, head_only),
    };
    let steps: Vec<Value> = view
        .steps
        .iter()
        .map(|step| {
            json!({
                "sibling": step.sibling.clone(),
                "side": step.side,
            })
        })
        .collect();
    let body = success(
        id,
        as_of,
        json!({
            "chain_id": chain,
            "app_id": view.game_id,
            "date": view.date,
            "game_root": view.game_root,
            "leaf": view.leaf,
            "steps": steps,
            "merkle_root": view.merkle_root,
            "day_root": view.day_root,
            "state": view.state,
            "anchor_tx": view.anchor_tx,
        }),
    );
    respond(id, StatusCode::OK, body, head_only)
}

// --- Responses -----------------------------------------------------------------------------------

fn respond(id: &str, status: StatusCode, body: Value, head_only: bool) -> HttpResponse {
    let mut builder = HttpResponse::build(status);
    builder.insert_header(("Cache-Control", "no-store"));
    builder.insert_header(("X-Request-Id", id.to_string()));
    builder.insert_header(("X-Content-Type-Options", "nosniff"));
    builder.insert_header(("Referrer-Policy", "no-referrer"));
    if head_only {
        // Same status and headers, no body: a HEAD client learns nothing it could parse.
        builder.content_type("application/json").finish()
    } else {
        builder.json(body)
    }
}

fn success(id: &str, as_of: &str, fields: Value) -> Value {
    let mut map = match fields {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("data".to_string(), other);
            map
        }
    };
    map.insert("success".to_string(), json!(true));
    map.insert("schema_version".to_string(), json!(SCHEMA_VERSION));
    map.insert("as_of".to_string(), json!(as_of));
    map.insert("request_id".to_string(), json!(id));
    Value::Object(map)
}

fn failure(id: &str, as_of: &str, code: &str, message: &str, retryable: bool) -> Value {
    json!({
        "success": false,
        "schema_version": SCHEMA_VERSION,
        "as_of": as_of,
        "request_id": id,
        "error": { "code": code, "message": message, "retryable": retryable },
    })
}

fn unknown_chain(id: &str, as_of: &str, head_only: bool) -> HttpResponse {
    respond(
        id,
        StatusCode::NOT_FOUND,
        failure(
            id,
            as_of,
            "unknown_chain",
            "no records are served for that chain",
            false,
        ),
        head_only,
    )
}

fn invalid(id: &str, as_of: &str, message: &str, head_only: bool) -> HttpResponse {
    respond(
        id,
        StatusCode::BAD_REQUEST,
        failure(id, as_of, "invalid_request", message, false),
        head_only,
    )
}

fn service_error(id: &str, as_of: &str, error: GamesError, head_only: bool) -> HttpResponse {
    let (status, code, retryable, message): (StatusCode, &str, bool, String) = match &error {
        GamesError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            false,
            message.clone(),
        ),
        GamesError::NotFound(message) => {
            (StatusCode::NOT_FOUND, "not_found", false, message.clone())
        }
        GamesError::Disabled => (
            StatusCode::SERVICE_UNAVAILABLE,
            "service_disabled",
            true,
            "the service is not accepting requests".to_string(),
        ),
        GamesError::Internal(detail) => {
            // The detail belongs in the operator's log, not in a public response.
            tracing::error!(detail = %detail, "records v1 request failed internally");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                true,
                "something went wrong on the server".to_string(),
            )
        }
        other => (
            StatusCode::from_u16(other.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            other.code(),
            true,
            other.to_string(),
        ),
    };
    respond(
        id,
        status,
        failure(id, as_of, code, &message, retryable),
        head_only,
    )
}

// --- Identity and time ---------------------------------------------------------------------------

/// One id per request. A well-formed client value is echoed so a report can be matched to the edge
/// and origin logs; anything else is replaced rather than trusted into a log line.
fn request_id(request: &HttpRequest) -> String {
    if let Some(value) = request
        .headers()
        .get("X-Request-Id")
        .and_then(|value| value.to_str().ok())
    {
        if valid_request_id(value) {
            return value.to_string();
        }
    }
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn valid_request_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn as_of(now: u64) -> String {
    Utc.timestamp_opt(now as i64, 0)
        .single()
        .map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn days_between(today: &str, earlier: &str) -> Option<i64> {
    let today = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let earlier = chrono::NaiveDate::parse_from_str(earlier, "%Y-%m-%d").ok()?;
    Some((today - earlier).num_days())
}
