//! The Records Service's HTTP API.
//!
//! ```text
//! the registry, under its original names (unchanged):
//!   GET  /api/games/g                        listed games          POST /api/games/g              register (signed)
//!   GET  /api/games/g/{id}                   a registered game     POST /api/games/g/{id}/results a result (Bearer key)
//!   GET  /api/games/g/{id}/proof/{date}      an inclusion proof    POST /api/games/g/{id}/rotate-key
//!   GET  /api/games/registry/days/{date}     a closed day
//! the same registry, under the generic names:
//!   POST /api/records   POST /api/apps   POST /api/apps/{id}/rotate-key
//!   GET  /api/apps/{id} GET  /api/apps/{id}/proof/{date}   GET /api/records/days/{date}
//! operations:
//!   GET  /api/records/status                 whether it is accepting and anchoring
//! between services (never routed by the reverse proxy; signed, see `peer.rs`):
//!   POST /internal/v1/anchor                 {chain_id, root}  anchor a first-party game's day
//! ```
//!
//! The `/api/games/g…` and `/api/games/registry/…` paths are served by this process, not by the Games
//! Reward Service, so the reverse proxy sends them here. Their behaviour is exactly what it was when one
//! process served everything.

use super::{
    api::{error_body, json_config, respond},
    peer::{self, PeerError},
    records::{
        RecordsService, RegistryRegisterRequest, RegistryResultRequest, RegistryRotateRequest,
    },
    service::GamesError,
};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::watch;

const MAX_INTERNAL_BODY_BYTES: usize = 4 * 1024;

pub fn configure_records_routes(config: &mut web::ServiceConfig) {
    config
        .app_data(json_config())
        .app_data(web::PayloadConfig::new(MAX_INTERNAL_BODY_BYTES))
        .route("/api/records/status", web::get().to(records_status))
        .route("/internal/v1/anchor", web::post().to(internal_anchor))
        // Third-party games, under their original names.
        .route("/api/games/g", web::get().to(registry_listed))
        .route("/api/games/g", web::post().to(registry_register))
        .route("/api/games/g/{id}/rotate-key", web::post().to(registry_rotate))
        .route("/api/games/g/{id}", web::get().to(registry_game))
        .route("/api/games/g/{id}/results", web::post().to(registry_result))
        .route(
            "/api/games/g/{id}/proof/{date}",
            web::get().to(registry_proof),
        )
        .route(
            "/api/games/registry/days/{date}",
            web::get().to(registry_day),
        )
        // The generic names for the same registry. These are adapters: they rename fields (see the
        // table above `RecordRequest`) and call the very same service methods as the routes above.
        .route("/api/records", web::post().to(records_submit))
        .route("/api/records/days/{date}", web::get().to(records_day))
        .route("/api/apps", web::post().to(apps_register))
        .route("/api/apps/{id}", web::get().to(apps_get))
        .route("/api/apps/{id}/rotate-key", web::post().to(registry_rotate_generic))
        .route("/api/apps/{id}/proof/{date}", web::get().to(apps_proof));
    // The versioned public read API. Additive: it renames nothing and changes no route above.
    config.configure(super::records_v1::configure_v1_routes);
}

pub async fn run_records_server(
    service: Arc<RecordsService>,
    bind: std::net::SocketAddr,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let data = web::Data::from(service);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .configure(configure_records_routes)
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

async fn records_status(service: web::Data<RecordsService>) -> HttpResponse {
    respond(service.status())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerAnchorRequest {
    chain_id: u32,
    root: String,
}

/// The one route a peer service may call. Authentication comes first and answers every failure the same
/// way; only then is the body read as a request.
async fn internal_anchor(
    service: web::Data<RecordsService>,
    request: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    let Some(verifier) = service.peer() else {
        // No peer is configured: this route does not exist as far as anyone can tell.
        return error_body("not_found", "no such route", None);
    };
    let authenticated = verifier.verify(
        &header(peer::HEADER_PEER),
        &header(peer::HEADER_TIMESTAMP),
        &header(peer::HEADER_NONCE),
        &header(peer::HEADER_SIGNATURE),
        request.method().as_str(),
        request.path(),
        &body,
        service.now(),
    );
    if authenticated == Err(PeerError::Unauthenticated) {
        return respond::<Value>(Err(GamesError::PeerUnauthenticated));
    }
    let parsed: Result<PeerAnchorRequest, _> = serde_json::from_slice(&body);
    let Ok(parsed) = parsed else {
        return respond::<Value>(Err(GamesError::Invalid(
            "the request body is not valid".into(),
        )));
    };
    let root: Option<[u8; 32]> = hex::decode(&parsed.root)
        .ok()
        .and_then(|bytes| bytes.try_into().ok());
    let Some(root) = root else {
        return respond::<Value>(Err(GamesError::Invalid(
            "root must be 64 hexadecimal characters".into(),
        )));
    };
    respond(service.anchor_for_peer(parsed.chain_id, root).await)
}

/// The API key from `Authorization: Bearer <key>`. The scheme is case-insensitive; anything else, or no
/// header, is simply "no key", which the service answers exactly as it answers a wrong one.
fn bearer_key(request: &HttpRequest) -> String {
    request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, key) = value.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("bearer")
                .then(|| key.trim().to_string())
        })
        .filter(|key| key.len() <= 128)
        .unwrap_or_default()
}

async fn registry_result(
    service: web::Data<RecordsService>,
    request: HttpRequest,
    id: web::Path<String>,
    body: web::Json<RegistryResultRequest>,
) -> HttpResponse {
    respond(service.submit_registry_result(&id, &bearer_key(&request), body.into_inner()))
}

async fn registry_register(
    service: web::Data<RecordsService>,
    body: web::Json<RegistryRegisterRequest>,
) -> HttpResponse {
    respond(service.self_register(body.into_inner()).await)
}

async fn registry_rotate(
    service: web::Data<RecordsService>,
    id: web::Path<String>,
    body: web::Json<RegistryRotateRequest>,
) -> HttpResponse {
    respond(service.self_rotate_key(&id, body.into_inner()))
}

// --- The generic API: /api/records and /api/apps -----------------------------------------------------
//
// One registry, two vocabularies. These routes exist so that applications that are not games (quizzes,
// certificates, votes) do not have to speak "game" and "player". They hold no logic of their own:
// each handler renames fields, calls the service method the `/api/games/g` route calls, and adds the
// generic names to the reply. Nothing is removed from a reply, so a client can read either vocabulary.
//
//   request (generic)   internal field        meaning
//   app_id              game id               3-32 of a-z 0-9 -
//   record_id           result_id             3-64 of letters, digits, - _ .  (the idempotency key)
//   subject             player                32 bytes as 64 lowercase hex: an address, or the SHA3-256
//                                             of any identifier the application chooses
//   value               score                 optional unsigned integer, default 0
//   data_hash           data_hash             unchanged
//
//   reply (added)       copied from
//   app_id              game_id, or id, or game.id
//   record_id           result_id
//
// Authentication, the API-key hash, owner signatures (the signed digest still names the id and the
// name, so a signature is valid on either route), replay protection, idempotency and every limit are
// the service's and are unchanged. A generic route refuses the old field names, and an old route refuses
// the new ones: `deny_unknown_fields` on both sides.

/// What `POST /api/records` accepts. The key travels as `Authorization: Bearer`, as on the old route.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordRequest {
    app_id: String,
    record_id: String,
    subject: String,
    #[serde(default)]
    value: u64,
    #[serde(default)]
    data_hash: Option<String>,
}

/// What `POST /api/apps` accepts. The signature is over the same digest as `POST /api/games/g`
/// (action, id, name, time), with `app_id` as the id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppRegisterRequest {
    app_id: String,
    name: String,
    public_key: String,
    timestamp: u64,
    signature: String,
}

/// Copy `from` to `to` inside one JSON object, leaving `from` in place.
fn copy_field(object: &mut Value, from: &str, to: &str) {
    if let Some(value) = object.get(from).cloned() {
        if let Value::Object(map) = object {
            map.insert(to.to_string(), value);
        }
    }
}

fn to_value<T: Serialize>(result: Result<T, GamesError>) -> Result<Value, GamesError> {
    result.map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
}

async fn records_submit(
    service: web::Data<RecordsService>,
    request: HttpRequest,
    body: web::Json<RecordRequest>,
) -> HttpResponse {
    let record = body.into_inner();
    let internal = RegistryResultRequest {
        player: record.subject,
        result_id: record.record_id,
        score: record.value,
        data_hash: record.data_hash,
    };
    let reply = to_value(service.submit_registry_result(
        &record.app_id,
        &bearer_key(&request),
        internal,
    ))
    .map(|mut value| {
        copy_field(&mut value, "game_id", "app_id");
        copy_field(&mut value, "result_id", "record_id");
        value
    });
    respond(reply)
}

async fn apps_register(
    service: web::Data<RecordsService>,
    body: web::Json<AppRegisterRequest>,
) -> HttpResponse {
    let app = body.into_inner();
    let internal = RegistryRegisterRequest {
        id: app.app_id,
        name: app.name,
        public_key: app.public_key,
        timestamp: app.timestamp,
        signature: app.signature,
    };
    let reply = to_value(service.self_register(internal).await).map(|mut value| {
        if let Some(game) = value.get_mut("game") {
            copy_field(game, "id", "app_id");
        }
        value
    });
    respond(reply)
}

async fn registry_rotate_generic(
    service: web::Data<RecordsService>,
    id: web::Path<String>,
    body: web::Json<RegistryRotateRequest>,
) -> HttpResponse {
    let reply = to_value(service.self_rotate_key(&id, body.into_inner())).map(|mut value| {
        if let Some(game) = value.get_mut("game") {
            copy_field(game, "id", "app_id");
        }
        value
    });
    respond(reply)
}

async fn apps_get(service: web::Data<RecordsService>, id: web::Path<String>) -> HttpResponse {
    respond(to_value(service.registry_game(&id)).map(|mut value| {
        copy_field(&mut value, "id", "app_id");
        value
    }))
}

async fn apps_proof(
    service: web::Data<RecordsService>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (id, date) = path.into_inner();
    respond(to_value(service.registry_proof(&id, &date)).map(|mut value| {
        copy_field(&mut value, "game_id", "app_id");
        value
    }))
}

async fn records_day(service: web::Data<RecordsService>, date: web::Path<String>) -> HttpResponse {
    respond(to_value(service.registry_day_view(&date)).map(|mut value| {
        if let Some(Value::Array(games)) = value.get_mut("games") {
            for game in games {
                copy_field(game, "id", "app_id");
            }
        }
        value
    }))
}

async fn registry_listed(service: web::Data<RecordsService>) -> HttpResponse {
    respond(
        service
            .registry_listed()
            .map(|games| json!({ "games": games })),
    )
}

async fn registry_game(service: web::Data<RecordsService>, id: web::Path<String>) -> HttpResponse {
    respond(service.registry_game(&id))
}

async fn registry_proof(
    service: web::Data<RecordsService>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (id, date) = path.into_inner();
    respond(service.registry_proof(&id, &date))
}

async fn registry_day(service: web::Data<RecordsService>, date: web::Path<String>) -> HttpResponse {
    respond(service.registry_day_view(&date))
}
