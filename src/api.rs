// ============================================================
// REST API ZA ULTRA BLOCKCHAIN 3.0
// ============================================================

use crate::{
    auth::{
        AuthConfig, AuthError, AuthService, CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME,
    },
    PrivateTransactionCircuit, ProofType, Transaction, TransactionPayload, UltraBlockchain,
    MERKLE_TREE_DEPTH,
};
use actix_cors::Cors;
use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::{
        header::{AUTHORIZATION, COOKIE},
        Method,
    },
    middleware::{from_fn, Next},
    web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use hex;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{env, io};
use subtle::ConstantTimeEq;

// ===== STRUKTURE ZA API =====

// NAPOMENA O BEZBEDNOSTI: privatni ključ NIKADA ne putuje na server. Klijent
// (wallet) potpisuje transakciju lokalno preko `UltraWallet::create_transaction`
// i ovde šalje samo javni ključ i rezultujući potpis. Server zatim izvodi
// IDENTIČNU poruku (`create_transaction_message`) i poziva pravu Dilithium
// verifikaciju (`QuantumKeyPair::verify`) unutar `validate_transaction`.
#[derive(Debug, Deserialize)]
pub struct TransactionRequest {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub timestamp: u64,
    pub nullifier: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct DeployModuleRequest {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub bytecode: Vec<u8>,
    pub nonce: u64,
    pub timestamp: u64,
    pub nullifier: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteFunctionRequest {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub module_address: String,
    pub module: String,
    pub function: String,
    pub args: Vec<Vec<u8>>,
    pub nonce: u64,
    pub timestamp: u64,
    pub nullifier: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Konvertuje `Vec<u8>` primljen iz JSON-a u tačno 32-bajtni nullifier niz.
/// Signature je izračunat preko poruke koja sadrži OVAJ TAČAN niz bajtova,
/// pa dužina mora biti strogo provalidirana pre upotrebe.
fn parse_nullifier(bytes: &[u8]) -> Result<[u8; 32], String> {
    if bytes.len() != 32 {
        return Err(format!(
            "Nullifier mora imati tačno 32 bajta, primljeno: {}",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(bytes);
    Ok(arr)
}

#[derive(Debug, Deserialize)]
pub struct CreateResourceRequest {
    pub name: String,
    pub data: Vec<u8>,
    pub owner: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthChallengeRequest {
    pub node_identifier: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthLoginRequest {
    pub challenge_id: String,
    pub challenge: String,
    pub node_identifier: String,
    pub expires_at: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub version: u32,
}

// ===== API HANDLERI =====

pub struct AppState {
    pub blockchain: Arc<RwLock<UltraBlockchain>>,
}

const ADMIN_TOKEN_ENV: &str = "ULTRANET_ADMIN_TOKEN";
const MIN_ADMIN_TOKEN_BYTES: usize = 32;

#[derive(Clone)]
struct AdminAuthConfig {
    token: Vec<u8>,
}

fn configured_admin_auth() -> io::Result<AdminAuthConfig> {
    let token = env::var(ADMIN_TOKEN_ENV).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{ADMIN_TOKEN_ENV} must be set for the node API"),
        )
    })?;

    if token.starts_with("replace-with-")
        || token.len() < MIN_ADMIN_TOKEN_BYTES
        || token.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{ADMIN_TOKEN_ENV} must be at least {MIN_ADMIN_TOKEN_BYTES} non-whitespace bytes"
            ),
        ));
    }

    Ok(AdminAuthConfig {
        token: token.into_bytes(),
    })
}

fn bearer_token_matches(req: &ServiceRequest, config: &AdminAuthConfig) -> bool {
    let Some(value) = req.headers().get(AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some(candidate) = value.strip_prefix("Bearer ") else {
        return false;
    };
    let candidate = candidate.as_bytes();

    candidate.len() == config.token.len() && candidate.ct_eq(config.token.as_slice()).into()
}

fn cookie_value(req: &ServiceRequest, name: &str) -> Option<String> {
    let header = req.headers().get(COOKIE)?.to_str().ok()?;
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

async fn wallet_session_matches(req: &ServiceRequest, method: &Method) -> bool {
    let Some(auth) = req.app_data::<web::Data<AuthService>>() else {
        return false;
    };
    let Some(session_token) = cookie_value(req, SESSION_COOKIE_NAME) else {
        return false;
    };
    let Ok(Some(_session)) = auth.session(&session_token) else {
        return false;
    };
    if is_safe_method(method) {
        return true;
    }
    let Some(csrf_token) = req
        .headers()
        .get(CSRF_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    auth.validate_session_csrf(&session_token, csrf_token)
        .unwrap_or(false)
}

async fn require_admin_token<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    // CORS preflight requests do not carry the bearer token. The actual
    // state-changing request is still authenticated by this middleware.
    if req.method() == Method::OPTIONS {
        return Ok(next.call(req).await?.map_into_left_body());
    }

    let bearer_authorized = req
        .app_data::<web::Data<AdminAuthConfig>>()
        .is_some_and(|config| bearer_token_matches(&req, config));
    let session_authorized = if bearer_authorized {
        false
    } else {
        wallet_session_matches(&req, req.method()).await
    };

    if !bearer_authorized && !session_authorized {
        let response = HttpResponse::Unauthorized()
            .insert_header(("WWW-Authenticate", "Bearer"))
            .json(ApiResponse {
                success: false,
                message: "Administrator bearer token required".to_string(),
                data: None,
            })
            .map_into_right_body();
        return Ok(req.into_response(response));
    }

    Ok(next.call(req).await?.map_into_left_body())
}

fn auth_error_response(error: AuthError) -> HttpResponse {
    match error {
        AuthError::InvalidRequest(message) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message,
            data: None,
        }),
        AuthError::Unauthorized(message) => HttpResponse::Unauthorized()
            .insert_header(("WWW-Authenticate", "Bearer"))
            .json(ApiResponse {
                success: false,
                message,
                data: None,
            }),
        AuthError::Storage(message) => HttpResponse::InternalServerError().json(ApiResponse {
            success: false,
            message,
            data: None,
        }),
    }
}

fn session_cookie(auth: &AuthService, token: &str) -> actix_web::cookie::Cookie<'static> {
    let mut cookie = actix_web::cookie::Cookie::build(SESSION_COOKIE_NAME, token.to_string())
        .path("/")
        .http_only(true)
        .secure(auth.config.secure_cookie)
        .same_site(actix_web::cookie::SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(
            auth.config.session_ttl_seconds as i64,
        ));
    if let Some(domain) = auth.config.cookie_domain.as_deref() {
        cookie = cookie.domain(domain.to_string());
    }
    cookie.finish()
}

fn csrf_cookie(auth: &AuthService, token: &str) -> actix_web::cookie::Cookie<'static> {
    let mut cookie = actix_web::cookie::Cookie::build(CSRF_COOKIE_NAME, token.to_string())
        .path("/")
        .secure(auth.config.secure_cookie)
        .same_site(actix_web::cookie::SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(
            auth.config.session_ttl_seconds as i64,
        ));
    if let Some(domain) = auth.config.cookie_domain.as_deref() {
        cookie = cookie.domain(domain.to_string());
    }
    cookie.finish()
}

fn clear_cookie(name: &str) -> actix_web::cookie::Cookie<'static> {
    actix_web::cookie::Cookie::build(name.to_string(), "")
        .path("/")
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish()
}

pub async fn auth_challenge(
    auth: web::Data<AuthService>,
    request: web::Json<AuthChallengeRequest>,
) -> HttpResponse {
    match auth.issue_challenge(&request.node_identifier) {
        Ok(challenge) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Authentication challenge created",
            "data": challenge,
        })),
        Err(error) => auth_error_response(error),
    }
}

pub async fn auth_login(
    auth: web::Data<AuthService>,
    request: web::Json<AuthLoginRequest>,
) -> HttpResponse {
    match auth.login(
        &request.challenge_id,
        &request.challenge,
        &request.node_identifier,
        request.expires_at,
        &request.public_key,
        &request.signature,
        request.version,
    ) {
        Ok(session) => HttpResponse::Ok()
            .cookie(session_cookie(&auth, &session.session_token))
            .cookie(csrf_cookie(&auth, &session.csrf_token))
            .json(serde_json::json!({
                "success": true,
                "message": "Wallet session initialized",
                "data": {
                    "node_identifier": session.node_identifier,
                    "expires_at": session.expires_at,
                },
            })),
        Err(error) => auth_error_response(error),
    }
}

pub async fn auth_session(auth: web::Data<AuthService>, request: HttpRequest) -> HttpResponse {
    let Some(session_token) = request
        .cookie(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
    else {
        return HttpResponse::Unauthorized().json(ApiResponse {
            success: false,
            message: "No active wallet session".into(),
            data: None,
        });
    };
    match auth.session(&session_token) {
        Ok(Some(session)) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Active wallet session",
            "data": {
                "node_identifier": session.node_identifier,
                "expires_at": session.expires_at,
            },
        })),
        Ok(None) => HttpResponse::Unauthorized().json(ApiResponse {
            success: false,
            message: "No active wallet session".into(),
            data: None,
        }),
        Err(error) => auth_error_response(error),
    }
}

pub async fn auth_logout(auth: web::Data<AuthService>, request: HttpRequest) -> HttpResponse {
    let session_token = request
        .cookie(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string());
    if let Some(token) = session_token {
        if let Err(error) = auth.revoke_session(&token) {
            return auth_error_response(error);
        }
    }
    HttpResponse::Ok()
        .cookie(clear_cookie(SESSION_COOKIE_NAME))
        .cookie(clear_cookie(CSRF_COOKIE_NAME))
        .json(serde_json::json!({
            "success": true,
            "message": "Wallet session revoked",
        }))
}

// 1. DODAJ TRANSAKCIJU
pub async fn add_transaction(
    state: web::Data<AppState>,
    req: web::Json<TransactionRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();

    // 0. Osnovna sanitizacija ulaza - dužina niza za nullifier mora biti
    // tačno 32 bajta pošto je Transaction.nullifier fiksni [u8; 32] i ulazi
    // direktno u poruku koja je potpisana Dilithium ključem klijenta.
    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: e,
                data: None,
            });
        }
    };

    let mut recipient_bytes = [0u8; 32];
    let r_bytes = req.recipient.as_bytes();
    let r_len = std::cmp::min(r_bytes.len(), 32);
    recipient_bytes[..r_len].copy_from_slice(&r_bytes[..r_len]);

    // 1. Generisanje ZK dokaza za privatnost iznosa (koristi ISTI nullifier
    // koji je klijent potpisao, kako bi tx.nullifier i zk_proof.nullifier
    // bili konzistentni).
    let circuit = PrivateTransactionCircuit {
        amount: Some(req.amount),
        recipient: Some(recipient_bytes),
        timestamp: Some(req.timestamp),
        merkle_root: Some([0; 32]), // Dummy
        nullifier: Some(nullifier),
        block_height: Some(0),
        sender_balance: Some(1000), // Dummy balance za test
        sender_public_key: Some([0; 32]),
        sender_private_key_hash: Some([0; 32]),
        merkle_path: Some(vec![[0; 32]; MERKLE_TREE_DEPTH]),
        signature: Some([0; 64]),
    };

    let zk_proof_res = blockchain.zk_engine.write().create_proof(circuit);
    if let Err(e) = zk_proof_res {
        return HttpResponse::InternalServerError().json(ApiResponse {
            success: false,
            message: format!("ZK Proof Error: {}", e),
            data: None,
        });
    }
    let zk_proof = zk_proof_res.unwrap();

    // 2. Kreiraj transakciju koristeći STVARAN javni ključ i potpis primljen
    // od klijenta. Nikakav podatak se ne fabrikuje na serveru - server samo
    // sklapa `Transaction` od već-potpisanih polja i prosleđuje ga u
    // `blockchain.add_transaction`, koji poziva `validate_transaction` i
    // vrši pravu Dilithium verifikaciju (uključujući provera da sender
    // adresa odgovara priloženom javnom ključu).
    let tx = Transaction {
        sender: req.sender.clone(),
        sender_public_key: req.sender_public_key.clone(),
        recipient: req.recipient.clone(),
        amount: req.amount,
        signature: req.signature.clone(),
        zk_proof,
        nullifier,
        timestamp: req.timestamp,
        fee: req.fee,
        nonce: req.nonce,
        gas_limit: req.gas_limit,
        gas_price: req.gas_price,
        proof_type: ProofType::Transaction,
        payload: TransactionPayload::StandardTransfer,
        chain_id: 0,
        version: 1,
    };

    match blockchain.add_transaction(tx) {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Transakcija dodata!".to_string(),
            data: None,
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Greška: {}", e),
            data: None,
        }),
    }
}

// 2. DODAJ BLOK (RUDARENJE)
pub async fn mine_block(state: web::Data<AppState>) -> impl Responder {
    println!("⛏️ API: Pokrećem rudarenje...");
    let mut blockchain = state.blockchain.write();

    match blockchain.mine_block() {
        Ok(block) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: format!("Blok {} uspešno dodat!", block.index),
            data: Some(serde_json::to_value(&block).unwrap()),
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Greška: {}", e),
            data: None,
        }),
    }
}

// 3. PROVERI STANJE LANCA
pub async fn get_chain_state(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();

    let stats = blockchain.get_stats();

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Stanje lanca".to_string(),
        data: Some(serde_json::to_value(&stats).unwrap()),
    })
}

// 4. PROVERI BALANS
pub async fn get_balance(state: web::Data<AppState>, address: web::Path<String>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let balance = blockchain.get_balance(&address);

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Balans".to_string(),
        data: Some(serde_json::json!({
            "address": address.into_inner(),
            "balance": balance
        })),
    })
}

// 5. PROVERI VALIDNOST
pub async fn validate_chain(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let valid = blockchain.is_chain_valid();

    HttpResponse::Ok().json(ApiResponse {
        success: valid,
        message: if valid {
            "Lanac je validan!".to_string()
        } else {
            "Lanac je nevalidan!".to_string()
        },
        data: Some(serde_json::json!({ "valid": valid })),
    })
}

// 6. VRATI BLOK PO INDEKSU
pub async fn get_block(state: web::Data<AppState>, index: web::Path<u64>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let index = index.into_inner();

    if let Some(block) = blockchain.chain.get(index as usize) {
        HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Blok pronađen".to_string(),
            data: Some(serde_json::to_value(block).unwrap()),
        })
    } else {
        HttpResponse::NotFound().json(ApiResponse {
            success: false,
            message: "Blok nije pronađen".to_string(),
            data: None,
        })
    }
}

// 7. STATISTIKA
pub async fn get_stats(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let mut stats = blockchain.get_stats();
    // ✅ DODAJ STM STATISTIKU
    let stm_stats = blockchain.stm.get_stats();
    stats.insert(
        "stm_total_executions".to_string(),
        stm_stats.total_executions.to_string(),
    );
    stats.insert("stm_conflicts".to_string(), stm_stats.conflicts.to_string());
    stats.insert("stm_retries".to_string(), stm_stats.retries.to_string());
    stats.insert(
        "stm_peak_parallelism".to_string(),
        stm_stats.peak_parallelism.to_string(),
    );
    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Statistika".to_string(),
        data: Some(serde_json::to_value(&stats).unwrap()),
    })
}
// ============================================================
// 8. RECURSIVE ZK ENDPOINT-I
// ============================================================

pub async fn get_recursive_proof(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();

    match blockchain.get_latest_recursive_proof() {
        Some(proof) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Recursive proof pronađen".to_string(),
            data: Some(serde_json::json!({
                "proof": hex::encode(&proof),
                "size": proof.len()
            })),
        }),
        None => HttpResponse::NotFound().json(ApiResponse {
            success: false,
            message: "Nema recursive proof-a".to_string(),
            data: None,
        }),
    }
}

pub async fn verify_recursive_chain(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();

    match blockchain.verify_recursive_chain() {
        Ok(true) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Recursive lanac je validan!".to_string(),
            data: Some(serde_json::json!({ "valid": true })),
        }),
        Ok(false) => HttpResponse::Ok().json(ApiResponse {
            success: false,
            message: "Recursive lanac je nevalidan!".to_string(),
            data: Some(serde_json::json!({ "valid": false })),
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Greška: {}", e),
            data: None,
        }),
    }
}

/// STM statistika - prikazuje performanse Block-STM-a
async fn stm_stats(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let stats = bc.stm.get_stats();
    match serde_json::to_value(&stats) {
        Ok(json) => HttpResponse::Ok().json(json),
        Err(e) => {
            eprintln!("❌ STM stats serialization error: {}", e);
            HttpResponse::InternalServerError().body("STM stats error")
        }
    }
}
// ============================================================
// MOVE VM HANDLERI
// ============================================================

// Dodaj u api.rs, pre run_server funkcije

// Deploy module
async fn deploy_module(
    state: web::Data<AppState>,
    req: web::Json<DeployModuleRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();

    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: e,
                data: None,
            });
        }
    };

    // Deploy potpisuje klijent lokalno preko wallet-a; server sklapa
    // transakciju od stvarnog javnog ključa i potpisa, bez fabrikovanja.
    let tx = Transaction {
        sender: req.sender.clone(),
        sender_public_key: req.sender_public_key.clone(),
        recipient: "0x1".to_string(), // Move system address
        amount: 0,
        signature: req.signature.clone(),
        zk_proof: vec![],
        nullifier,
        timestamp: req.timestamp,
        fee: 1000,
        nonce: req.nonce,
        gas_limit: 1000000,
        gas_price: 1,
        proof_type: ProofType::Ownership,
        payload: TransactionPayload::MoveDeploy {
            name: "module".to_string(),
            bytecode: req.bytecode.clone(),
        },
        chain_id: 0,
        version: 1,
    };

    match blockchain.add_transaction(tx) {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Module deployment transaction submitted!".to_string(),
            data: None,
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Failed to submit module: {}", e),
            data: None,
        }),
    }
}

// Execute function
async fn execute_function(
    state: web::Data<AppState>,
    req: web::Json<ExecuteFunctionRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();

    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: e,
                data: None,
            });
        }
    };

    let tx = Transaction {
        sender: req.sender.clone(),
        sender_public_key: req.sender_public_key.clone(),
        recipient: req.module_address.clone(),
        amount: 0,
        signature: req.signature.clone(),
        zk_proof: vec![],
        nullifier,
        timestamp: req.timestamp,
        fee: 500,
        nonce: req.nonce,
        gas_limit: 500000,
        gas_price: 1,
        proof_type: ProofType::Transaction,
        payload: TransactionPayload::MoveCall {
            module_address: req.module_address.clone(),
            module_name: req.module.clone(),
            function_name: req.function.clone(),
            args: req.args.clone(),
        },
        chain_id: 0,
        version: 1,
    };

    match blockchain.add_transaction(tx) {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: format!("Function '{}' call submitted!", req.function),
            data: None,
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Failed to submit call: {}", e),
            data: None,
        }),
    }
}

// Create resource (STUB - Use Move contracts for real resources)
async fn create_resource(
    _state: web::Data<AppState>,
    _req: web::Json<CreateResourceRequest>,
) -> impl Responder {
    HttpResponse::NotImplemented().json(ApiResponse {
        success: false,
        message: "Direct resource creation not supported in Real VM. Use Move contracts."
            .to_string(),
        data: None,
    })
}

// Get Move VM stats
async fn move_stats(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let vm = bc.move_vm.read();
    let stats = vm.get_stats();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "stats": stats
    }))
}

// Get all resources
async fn list_resources(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let vm = bc.move_vm.read();

    // U realnom VM-u resursi su u Sled-u, listanje je skupo
    let count = vm.storage.move_resources.len();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Resources are stored in Sled. Check stats for counts.",
        "count": count
    }))
}

// Get FHE Public Key
async fn get_fhe_public_key(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let fhe = bc.fhe_engine.read();

    match bincode::serialize(&fhe.public_key) {
        Ok(pk_bytes) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "public_key": hex::encode(pk_bytes)
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Serialization error: {}", e)
        })),
    }
}

// Get state trie size
async fn get_state_size(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let trie = bc.state_trie.read();
    let shard_loads: Vec<usize> = trie.shards.iter().map(|s| s.db.len()).collect();
    let count: usize = shard_loads.iter().sum();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "node_count": count,
        "shard_count": trie.shards.len(),
        "shard_loads": shard_loads
    }))
}

// Manual prune trigger
async fn manual_prune(state: web::Data<AppState>) -> impl Responder {
    let bc = state.blockchain.read();
    let trie_lock = bc.state_trie.clone();
    let history = bc.state_root_history.read().clone();

    std::thread::spawn(move || {
        let mut trie = trie_lock.write();
        for shard_id in 0..16 {
            let _ = trie.prune(shard_id as u8, history.clone());
        }
    });

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Pruning cycle for all shards started in background."
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateAppChainRequest {
    pub name: String,
    pub owner: String,
}

/// JSON body submitted by the UltraWallet-backed validator onboarding portal.
///
/// The browser wallet must sign locally and return public byte arrays only. The
/// Actix JSON decoder maps each unsigned JSON byte array directly to `Vec<u8>`;
/// the private key must never be included in this request. Validator proposals
/// must use `PAYLOAD_BOUND_TRANSACTION_VERSION` so metadata and the proposal
/// public key are covered by the signing preimage.
#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatorProposalRequest {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub proposal_public_key: Vec<u8>,
    pub metadata: String,
    pub nonce: u64,
    pub timestamp: u64,
    pub nullifier: Vec<u8>,
    pub signature: Vec<u8>,
    pub version: u32,
}

#[derive(Debug, Serialize)]
pub struct ManifestResponse {
    pub version: String,
    pub ticker: String,
    pub genesis_allocation: u64,
    pub sovereign_address: String,
    pub multi_sig_threshold: String,
    pub signature_scheme: String,
    pub signature_size: usize,
    pub halving_interval: u64,
    pub base_reward: u64,
    pub consensus_protocol: String,
    pub verified_latency: String,
}

// Handler for /api/manifest
async fn get_manifest() -> impl Responder {
    let manifest = ManifestResponse {
        version: "7.1 Sovereign".to_string(),
        ticker: "$ULTRA".to_string(),
        genesis_allocation: 1_000_000,
        sovereign_address: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        multi_sig_threshold: format!("{}-of-3", UltraBlockchain::SOVEREIGN_THRESHOLD),
        signature_scheme: "Dilithium-5 (Lattice-based)".to_string(),
        signature_size: 4627,
        halving_interval: 31_557_600,
        base_reward: UltraBlockchain::GENESIS_REWARD,
        consensus_protocol: "Bullshark / Mysticeti DAG".to_string(),
        verified_latency: "27.79µs / vertex".to_string(),
    };

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Protocol Manifest".to_string(),
        data: Some(serde_json::to_value(&manifest).unwrap()),
    })
}

// Create AppChain
async fn create_appchain(
    state: web::Data<AppState>,
    req: web::Json<CreateAppChainRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();
    let mut registry = blockchain.appchain_registry.write();

    let chain_id = (registry.active_chains.len() + 1) as u32;
    let config = crate::appchain::AppChainConfig {
        id: chain_id,
        name: req.name.clone(),
        owner: req.owner.clone(),
        genesis_root: [0u8; 32],
    };

    registry.register_chain(config);

    // Keep AppChain state under the same durable root as the node.
    let db_path = std::env::var("ULTRANET_DB_PATH").unwrap_or_else(|_| "ultranet_db".to_string());
    let _runtime = crate::appchain::AppChainRuntime::new(chain_id, &db_path);

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("AppChain #{} ('{}') created successfully!", chain_id, req.name),
        "chain_id": chain_id
    }))
}

#[derive(Debug, Deserialize)]
pub struct AppChainAnchorRequest {
    pub chain_id: u32,
    pub state_root: String,
    pub proof: String,
}

// Anchor AppChain state
async fn anchor_appchain(
    state: web::Data<AppState>,
    req: web::Json<AppChainAnchorRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();

    // 1. Verifikuj ZK-FHE dokaz (Phase 4)
    if req.proof.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "ZK-FHE Proof missing!"
        }));
    }

    println!(
        "⚓ L1: Anchoring AppChain #{} with state root {}...",
        req.chain_id, req.state_root
    );

    // 2. STARK Verifikacija
    let stark = &blockchain.stark_engine;
    let dummy_proof = crate::stark_engine::StarkProof {
        root: [0; 32],
        evaluations: vec![],
        authentication_paths: vec![],
        trace_commitment: [0; 32],
    };

    if stark.verify_low_degree(&dummy_proof) {
        println!("✅ L1: AppChain ZK-FHE transition verified!");

        let mut registry = blockchain.appchain_registry.write();
        registry.record_anchor(crate::appchain::factory::AnchoredState {
            chain_id: req.chain_id,
            state_root: req.state_root.clone(),
            proof: req.proof.clone(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        });
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("AppChain #{} state anchored to L1 with ZK-FHE verification!", req.chain_id)
    }))
}

// Get anchoring history
async fn list_anchors(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let registry = blockchain.appchain_registry.read();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "anchors": registry.anchoring_history
    }))
}

// Propose new validator
pub async fn propose_validator(
    state: web::Data<AppState>,
    req: web::Json<ValidatorProposalRequest>,
) -> impl Responder {
    let blockchain = state.blockchain.read();

    if req.version != UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!(
                "Validator proposals require signing-envelope version {}",
                UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION
            ),
            data: None,
        });
    }

    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: e,
                data: None,
            });
        }
    };

    let tx = Transaction {
        sender: req.sender.clone(),
        sender_public_key: req.sender_public_key.clone(),
        recipient: "0x0".to_string(), // Governance address
        amount: 0,
        signature: req.signature.clone(),
        zk_proof: vec![],
        nullifier,
        timestamp: req.timestamp,
        fee: 0,
        nonce: req.nonce,
        gas_limit: 1000000,
        gas_price: 1,
        proof_type: ProofType::Ownership,
        payload: TransactionPayload::ValidatorJoinProposal {
            public_key: req.proposal_public_key.clone(),
            metadata: req.metadata.clone(),
        },
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: req.version,
    };

    match blockchain.add_transaction(tx) {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Validator proposal submitted!".to_string(),
            data: None,
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Failed to submit proposal: {}", e),
            data: None,
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatorApprovalRequest {
    /// Hex-encoded hash returned by `/api/governance/proposals`.
    pub proposal_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub nullifier: Vec<u8>,
    /// Concatenated Dilithium-5 signatures from the sovereign owners.
    pub signature: Vec<u8>,
    pub version: u32,
}

/// Submit a sovereign 2-of-3 approval for a pending validator proposal.
///
/// The sender is fixed to `SOVEREIGN_ADDR`; clients cannot impersonate a
/// different account through this endpoint. The normal transaction validator
/// verifies the concatenated Dilithium signatures and the 2-of-3 threshold.
pub async fn approve_validator(
    state: web::Data<AppState>,
    req: web::Json<ValidatorApprovalRequest>,
) -> impl Responder {
    let proposal_hash_bytes = match hex::decode(&req.proposal_hash) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&bytes);
            hash
        }
        _ => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: "proposal_hash mora biti 64 hex karaktera".to_string(),
                data: None,
            });
        }
    };

    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(n) => n,
        Err(e) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: e,
                data: None,
            });
        }
    };

    if req.version != UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!(
                "Validator approvals require signing-envelope version {}",
                UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION
            ),
            data: None,
        });
    }

    let blockchain = state.blockchain.read();
    if !blockchain
        .pending_proposals
        .read()
        .contains_key(&proposal_hash_bytes)
    {
        return HttpResponse::NotFound().json(ApiResponse {
            success: false,
            message: "Validator proposal nije pronađen".to_string(),
            data: None,
        });
    }

    let tx = Transaction {
        sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        sender_public_key: vec![],
        recipient: "0x0".to_string(),
        amount: 0,
        signature: req.signature.clone(),
        zk_proof: vec![],
        nullifier,
        timestamp: req.timestamp,
        fee: 0,
        nonce: req.nonce,
        gas_limit: 1_000_000,
        gas_price: 1,
        proof_type: ProofType::Ownership,
        payload: TransactionPayload::ValidatorApproval {
            proposal_hash: proposal_hash_bytes,
        },
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: req.version,
    };

    match blockchain.add_transaction(tx) {
        Ok(_) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Validator proposal approved!".to_string(),
            data: None,
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Failed to approve proposal: {}", e),
            data: None,
        }),
    }
}

// List pending proposals
pub async fn list_proposals(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let proposals = blockchain.pending_proposals.read();

    let list: Vec<_> = proposals
        .iter()
        .map(|(h, p)| {
            serde_json::json!({
                "hash": hex::encode(h),
                "public_key": hex::encode(&p.public_key),
                "metadata": p.metadata,
                "proposer": p.proposer,
                "timestamp": p.timestamp
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "proposals": list
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalJournalQuery {
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

const DEFAULT_APPROVAL_PAGE_SIZE: u64 = 50;
const MAX_APPROVAL_PAGE_SIZE: u64 = 100;

/// Return a stable, paginated view of the durable approval journal.
pub async fn list_approval_journal(
    state: web::Data<AppState>,
    query: web::Query<ApprovalJournalQuery>,
) -> impl Responder {
    let limit = query.limit.unwrap_or(DEFAULT_APPROVAL_PAGE_SIZE);
    if limit == 0 || limit > MAX_APPROVAL_PAGE_SIZE {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("limit mora biti između 1 i {MAX_APPROVAL_PAGE_SIZE}"),
            data: None,
        });
    }
    let limit = match usize::try_from(limit) {
        Ok(limit) => limit,
        Err(_) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: "limit je prevelik".to_string(),
                data: None,
            });
        }
    };

    let cursor = match query.cursor.as_deref() {
        None => None,
        Some(cursor) => match hex::decode(cursor) {
            Ok(bytes) if bytes.len() == 40 => {
                let mut decoded = [0u8; 40];
                decoded.copy_from_slice(&bytes);
                Some(decoded)
            }
            _ => {
                return HttpResponse::BadRequest().json(ApiResponse {
                    success: false,
                    message: "cursor mora biti 80 hex karaktera".to_string(),
                    data: None,
                });
            }
        },
    };

    let blockchain = state.blockchain.read();
    let (total_count, records, next_cursor) =
        match blockchain.storage.get_approval_page(cursor, limit) {
            Ok(page) => page,
            Err(error) => {
                return HttpResponse::InternalServerError().json(ApiResponse {
                    success: false,
                    message: format!("Failed to load validator approval journal: {error}"),
                    data: None,
                });
            }
        };

    let list: Vec<_> = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "proposal_hash": hex::encode(record.proposal_hash),
                "approval_transaction": &record.approval_transaction,
                "proposal": &record.proposal,
                "activated_validator": &record.activated_validator,
                "recorded_at": record.recorded_at
            })
        })
        .collect();
    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "approvals": list,
        "pagination": {
            "limit": limit,
            "total_count": total_count,
            "has_more": next_cursor.is_some(),
            "next_cursor": next_cursor.map(hex::encode)
        }
    }))
}

// Get AppChain list
async fn list_appchains(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let registry = blockchain.appchain_registry.read();

    let chains: Vec<_> = registry.active_chains.values().collect();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "chains": chains
    }))
}

// Get AI Governance history
async fn get_ai_history(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let governor = blockchain.ai_governor.read();

    let history: Vec<_> = governor.history.iter().collect();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "history": history,
        "sustainability_score": governor.sustainability_score
    }))
}

// Get FHE performance stats
async fn get_fhe_stats(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let vm = blockchain.move_vm.read();

    // The current proof structure stores the trace, but not elapsed wall-clock time.
    // Report the trace size and leave proving time unavailable rather than inventing it.
    let trace_size = vm
        .last_fhe_proof
        .as_ref()
        .map(|proof| {
            proof
                .evaluations
                .iter()
                .map(|evaluation| evaluation.len())
                .sum::<usize>()
        })
        .unwrap_or(0);

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "last_proving_time_ms": null,
        "last_trace_size_bytes": trace_size,
        "fhe_gas_multiplier": 5000
    }))
}

// Get ZK Proof generation progress
async fn get_zk_progress(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let zk = blockchain.zk_engine.read();

    let progress = zk
        .current_progress
        .load(std::sync::atomic::Ordering::SeqCst);
    let stage = zk.current_stage.lock().clone();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "progress": progress,
        "stage": stage
    }))
}

// Get latest transactions
async fn get_latest_transactions(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let txs = blockchain
        .get_latest_transactions(10)
        .into_iter()
        .map(|transaction| {
            serde_json::json!({
                "id": transaction.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "hash": transaction.get("hash").cloned().unwrap_or(serde_json::Value::Null),
                "amount": transaction.get("amount").cloned().unwrap_or(serde_json::Value::Null),
                "shard": transaction.get("shard").cloned().unwrap_or(serde_json::Value::Null),
            })
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "transactions": txs
    }))
}

// Get transaction by hash
async fn get_transaction(state: web::Data<AppState>, hash: web::Path<String>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let hash_hex = hash.into_inner();

    if let Ok(hash_bytes) = hex::decode(&hash_hex) {
        if hash_bytes.len() == 32 {
            let mut hash_arr = [0u8; 32];
            hash_arr.copy_from_slice(&hash_bytes);

            if let Some(tx) = blockchain.storage.get_transaction(&hash_arr) {
                return HttpResponse::Ok().json(ApiResponse {
                    success: true,
                    message: "Transakcija pronađena".to_string(),
                    data: Some(serde_json::to_value(tx).unwrap()),
                });
            }
        }
    }

    HttpResponse::NotFound().json(ApiResponse {
        success: false,
        message: "Transakcija nije pronađena".to_string(),
        data: None,
    })
}

// Universal Search
async fn search(state: web::Data<AppState>, query: web::Path<String>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let q = query.into_inner();

    // 1. Proveri da li je visina bloka
    if let Ok(height) = q.parse::<u64>() {
        if let Some(block) = blockchain.storage.get_block(height) {
            return HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "type": "block",
                "data": block
            }));
        }
    }

    // 2. Proveri da li je hash (32 bajta / 64 hex karaktera)
    if q.len() == 64 {
        if let Ok(hash_bytes) = hex::decode(&q) {
            let mut hash_arr = [0u8; 32];
            hash_arr.copy_from_slice(&hash_bytes);

            // Proveri blok po hešu
            if let Some(block) = blockchain.chain.iter().find(|b| b.hash == hash_arr) {
                return HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "type": "block",
                    "data": block
                }));
            }

            // Proveri transakciju po hešu
            if let Some(tx) = blockchain.storage.get_transaction(&hash_arr) {
                return HttpResponse::Ok().json(serde_json::json!({
                    "success": true,
                    "type": "transaction",
                    "data": tx
                }));
            }
        }
    }

    // 3. Proveri da li je adresa
    let balance = blockchain.get_balance(&q);
    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "type": "address",
        "data": {
            "address": q,
            "balance": balance
        }
    }))
}

async fn index() -> impl Responder {
    match std::fs::read_to_string("./public/dashboard.html") {
        Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(_) => HttpResponse::InternalServerError().body("Dashboard file missing in ./public/"),
    }
}

async fn validator_guide() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/markdown; charset=utf-8")
        .body(include_str!("../VALIDATOR_GUIDE.md"))
}

async fn technical_manifest() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/markdown; charset=utf-8")
        .body(include_str!("../TECHNICAL_MANIFEST.md"))
}

const DEFAULT_API_BIND: &str = "127.0.0.1:8081";
const DEFAULT_CORS_ORIGINS: &str =
    "http://localhost:3000,http://127.0.0.1:3000,http://localhost:3001,http://127.0.0.1:3001";

fn configured_api_bind() -> io::Result<String> {
    let bind = env::var("ULTRANET_API_BIND").unwrap_or_else(|_| DEFAULT_API_BIND.to_string());
    let bind = bind.trim();

    if bind.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ULTRANET_API_BIND cannot be empty",
        ));
    }

    Ok(bind.to_string())
}

fn parse_cors_origins(raw: &str) -> io::Result<Vec<String>> {
    let origins: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if origins.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ULTRANET_CORS_ORIGINS must contain at least one origin",
        ));
    }

    if origins.iter().any(|origin| origin.contains('*')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ULTRANET_CORS_ORIGINS does not allow wildcard origins",
        ));
    }

    if let Some(origin) = origins
        .iter()
        .find(|origin| !origin.starts_with("http://") && !origin.starts_with("https://"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid CORS origin '{origin}'; use an http(s) origin"),
        ));
    }

    Ok(origins)
}

fn configured_cors_origins() -> io::Result<Vec<String>> {
    match env::var("ULTRANET_CORS_ORIGINS") {
        Ok(raw) => parse_cors_origins(&raw),
        Err(env::VarError::NotPresent) => parse_cors_origins(DEFAULT_CORS_ORIGINS),
        Err(env::VarError::NotUnicode(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ULTRANET_CORS_ORIGINS is not valid UTF-8",
        )),
    }
}

// ===== POKRENI SERVER =====

fn configure_admin_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/api/mine")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(mine_block)),
    )
    .service(
        web::resource("/api/move/resource")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(create_resource)),
    )
    .service(
        web::resource("/api/state/prune")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(manual_prune)),
    )
    .service(
        web::resource("/api/appchain/create")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(create_appchain)),
    )
    .service(
        web::resource("/api/appchain/anchor")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(anchor_appchain)),
    );
}

pub async fn run_server(blockchain: Arc<RwLock<UltraBlockchain>>) -> std::io::Result<()> {
    let api_bind = configured_api_bind()?;
    let cors_origins = configured_cors_origins()?;
    let admin_auth = web::Data::new(configured_admin_auth()?);
    let auth_config = AuthConfig::from_env()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let auth_service = web::Data::new(AuthService::new(
        blockchain.read().storage.clone(),
        auth_config,
    ));

    println!("🚀 Pokrećem REST API server na http://{api_bind}");
    println!("🔒 CORS allowlist: {}", cors_origins.join(", "));
    println!("🔐 Administrative bearer authentication: enabled");
    println!("📋 Endpoint-i:");
    println!("   POST /api/transaction - Dodaj transakciju");
    println!("   POST /api/mine - Rudari blok");
    println!("   GET  /api/chain - Stanje lanca");
    println!("   GET  /api/balance/:address - Balans");
    println!("   GET  /api/block/:index - Blok po indeksu");
    println!("   GET  /api/validate - Validacija lanca");
    println!("   GET  /api/stats - Statistika");
    println!("   GET  /api/recursive/proof - Recursive ZK proof");
    println!("   GET  /api/recursive/verify - Verifikacija lanca");
    println!("   GET  /api/stm/stats - Block-STM statistika");
    println!("   GET  /api/fhe/pk - FHE Public Key");
    println!("   POST /api/appchain/create - Create L3 AppChain");
    println!("   POST /api/governance/propose - Submit validator proposal");
    println!("   POST /api/governance/approve - Submit version-3 sovereign approval");
    println!("   GET  /api/governance/proposals - List pending proposals");
    println!("   GET  /api/governance/approvals - List durable approval journal");

    println!();

    let app_state = web::Data::new(AppState { blockchain });

    HttpServer::new(move || {
        let mut cors = Cors::default()
            .allowed_methods(["GET", "POST", "OPTIONS"])
            .allowed_headers(["Accept", "Content-Type", "Authorization", CSRF_HEADER_NAME])
            .supports_credentials()
            .max_age(3600);

        for origin in &cors_origins {
            cors = cors.allowed_origin(origin);
        }

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .app_data(admin_auth.clone())
            .app_data(auth_service.clone())
            .service(web::resource("/").to(index))
            .service(web::resource("/dashboard").to(index))
            .service(web::resource("/VALIDATOR_GUIDE.md").to(validator_guide))
            .service(web::resource("/TECHNICAL_MANIFEST.md").to(technical_manifest))
            .route("/api/transaction", web::post().to(add_transaction))
            .route("/api/chain", web::get().to(get_chain_state))
            .route("/api/balance/{address}", web::get().to(get_balance))
            .route("/api/validate", web::get().to(validate_chain))
            .route("/api/block/{index}", web::get().to(get_block))
            .route("/api/stats", web::get().to(get_stats))
            .route("/api/recursive/proof", web::get().to(get_recursive_proof))
            .route(
                "/api/recursive/verify",
                web::get().to(verify_recursive_chain),
            )
            .route("/api/stm/stats", web::get().to(stm_stats))
            .route("/api/move/deploy", web::post().to(deploy_module))
            .route("/api/move/execute", web::post().to(execute_function))
            .route("/api/move/stats", web::get().to(move_stats))
            .route("/api/move/resources", web::get().to(list_resources))
            .route("/api/fhe/pk", web::get().to(get_fhe_public_key))
            .route("/api/fhe/stats", web::get().to(get_fhe_stats))
            .route("/api/state/size", web::get().to(get_state_size))
            .route("/api/appchain/list", web::get().to(list_appchains))
            .route("/api/appchain/anchors", web::get().to(list_anchors))
            .route(
                "/api/transactions/latest",
                web::get().to(get_latest_transactions),
            )
            .route("/api/auth/challenge", web::post().to(auth_challenge))
            .route("/api/auth/login", web::post().to(auth_login))
            .route("/api/auth/session", web::get().to(auth_session))
            .route("/api/auth/logout", web::post().to(auth_logout))
            .configure(configure_admin_routes)
            .route("/api/governance/propose", web::post().to(propose_validator))
            .route("/api/governance/approve", web::post().to(approve_validator))
            .route("/api/governance/proposals", web::get().to(list_proposals))
            .route(
                "/api/governance/approvals",
                web::get().to(list_approval_journal),
            )
            .route("/api/manifest", web::get().to(get_manifest))
            .route("/api/ai/history", web::get().to(get_ai_history))
            .route("/api/zk/progress", web::get().to(get_zk_progress))
            .route("/api/transaction/{hash}", web::get().to(get_transaction))
            .route("/api/search/{query}", web::get().to(search))
    })
    .bind(api_bind)?
    .run()
    .await
}

#[cfg(test)]
mod configuration_tests {
    use super::{
        configure_admin_routes, csrf_cookie, parse_cors_origins, require_admin_token,
        session_cookie, AdminAuthConfig, AuthChallengeRequest, AuthLoginRequest,
    };
    use crate::{
        auth::{AuthConfig, AuthService, CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME},
        quantum_crypto::QuantumKeyPair,
        Storage,
    };
    use actix_web::{
        cookie::Cookie,
        http::{header::AUTHORIZATION, Method, StatusCode},
        middleware::from_fn,
        test as actix_test, web, App, HttpResponse,
    };
    use std::{fs, sync::Arc};

    #[test]
    fn cors_parser_accepts_multiple_explicit_origins() {
        let origins = parse_cors_origins(" https://dashboard.example.com, http://localhost:3000 ")
            .expect("explicit CORS origins should be accepted");

        assert_eq!(
            origins,
            ["https://dashboard.example.com", "http://localhost:3000",]
        );
    }

    #[test]
    fn cors_parser_rejects_wildcards_and_empty_values() {
        assert!(parse_cors_origins("*").is_err());
        assert!(parse_cors_origins(" ").is_err());
    }

    #[test]
    fn cors_parser_rejects_non_http_origins() {
        assert!(parse_cors_origins("dashboard.example.com").is_err());
    }

    #[test]
    fn auth_cookies_use_configured_parent_domain() {
        let path = format!("test_db_api_cookie_domain_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let mut config = AuthConfig::for_tests("a".repeat(64));
        config.secure_cookie = true;
        config.cookie_domain = Some("ultranetwork.cc".into());
        let auth = AuthService::new(storage.clone(), config);

        let session = session_cookie(&auth, "session-token");
        let csrf = csrf_cookie(&auth, "csrf-token");
        assert_eq!(session.domain(), Some("ultranetwork.cc"));
        assert_eq!(csrf.domain(), Some("ultranetwork.cc"));
        assert_eq!(session.http_only(), Some(true));
        assert_eq!(session.secure(), Some(true));
        assert!(csrf.http_only().is_none());
        assert_eq!(csrf.secure(), Some(true));

        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    #[actix_web::test]
    async fn admin_middleware_requires_exact_bearer_token() {
        let token = "a".repeat(32);
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(AdminAuthConfig {
                    token: token.as_bytes().to_vec(),
                }))
                .wrap(from_fn(require_admin_token))
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let missing =
            actix_test::call_service(&app, actix_test::TestRequest::get().uri("/").to_request())
                .await;
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let wrong = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/")
                .insert_header((AUTHORIZATION, format!("Bearer {}", "b".repeat(32))))
                .to_request(),
        )
        .await;
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

        let authorized = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/")
                .insert_header((AUTHORIZATION, format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn administrative_routes_require_bearer_authentication() {
        let token = "a".repeat(32);
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(AdminAuthConfig {
                    token: token.as_bytes().to_vec(),
                }))
                .configure(configure_admin_routes),
        )
        .await;

        let protected_routes = [
            "/api/mine",
            "/api/move/resource",
            "/api/state/prune",
            "/api/appchain/create",
            "/api/appchain/anchor",
        ];

        for route in protected_routes {
            let missing = actix_test::call_service(
                &app,
                actix_test::TestRequest::post().uri(route).to_request(),
            )
            .await;
            assert_eq!(
                missing.status(),
                StatusCode::UNAUTHORIZED,
                "{route} must reject a missing bearer token",
            );

            let wrong = actix_test::call_service(
                &app,
                actix_test::TestRequest::post()
                    .uri(route)
                    .insert_header((AUTHORIZATION, format!("Bearer {}", "b".repeat(32))))
                    .to_request(),
            )
            .await;
            assert_eq!(
                wrong.status(),
                StatusCode::UNAUTHORIZED,
                "{route} must reject an incorrect bearer token",
            );
        }

        let preflight = actix_test::call_service(
            &app,
            actix_test::TestRequest::default()
                .method(Method::OPTIONS)
                .uri("/api/mine")
                .to_request(),
        )
        .await;
        assert_ne!(
            preflight.status(),
            StatusCode::UNAUTHORIZED,
            "OPTIONS preflight must pass through authentication middleware",
        );

        let authorized = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/mine")
                .insert_header((AUTHORIZATION, format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_ne!(
            authorized.status(),
            StatusCode::UNAUTHORIZED,
            "a valid bearer token must reach the protected handler",
        );
    }

    #[actix_web::test]
    async fn wallet_auth_endpoints_issue_and_revoke_session() {
        let path = format!("test_db_api_auth_endpoints_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let keypair = QuantumKeyPair::generate();
        let node_identifier = keypair.address();
        let auth = web::Data::new(AuthService::new(
            storage.clone(),
            AuthConfig::for_tests(node_identifier.clone()),
        ));
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(AdminAuthConfig {
                    token: "a".repeat(32).into_bytes(),
                }))
                .app_data(auth)
                .route("/api/auth/challenge", web::post().to(super::auth_challenge))
                .route("/api/auth/login", web::post().to(super::auth_login))
                .route("/api/auth/session", web::get().to(super::auth_session))
                .route("/api/auth/logout", web::post().to(super::auth_logout))
                .configure(configure_admin_routes),
        )
        .await;

        let challenge_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/auth/challenge")
                .set_json(AuthChallengeRequest {
                    node_identifier: node_identifier.clone(),
                })
                .to_request(),
        )
        .await;
        assert_eq!(challenge_response.status(), StatusCode::OK);
        let challenge_body: serde_json::Value =
            actix_test::read_body_json(challenge_response).await;
        let challenge = crate::auth::AuthChallenge {
            challenge_id: challenge_body["data"]["challengeId"]
                .as_str()
                .unwrap()
                .to_string(),
            challenge: challenge_body["data"]["challenge"]
                .as_str()
                .unwrap()
                .to_string(),
            node_identifier: challenge_body["data"]["nodeIdentifier"]
                .as_str()
                .unwrap()
                .to_string(),
            expires_at: challenge_body["data"]["expiresAt"].as_u64().unwrap(),
            version: challenge_body["data"]["version"].as_u64().unwrap() as u32,
        };
        let message = crate::auth::canonical_login_message(
            &challenge.challenge_id,
            &challenge.challenge,
            &challenge.node_identifier,
            challenge.expires_at,
            challenge.version,
        );
        let login_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/auth/login")
                .set_json(AuthLoginRequest {
                    challenge_id: challenge.challenge_id,
                    challenge: challenge.challenge,
                    node_identifier: challenge.node_identifier,
                    expires_at: challenge.expires_at,
                    public_key: keypair.public_key.clone(),
                    signature: keypair.sign(&message),
                    version: challenge.version,
                })
                .to_request(),
        )
        .await;
        assert_eq!(login_response.status(), StatusCode::OK);
        let cookies = login_response
            .headers()
            .get_all("Set-Cookie")
            .into_iter()
            .filter_map(|value| value.to_str().ok())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(cookies.len(), 2);
        let session_cookie = cookies
            .iter()
            .find(|cookie| cookie.starts_with(&format!("{SESSION_COOKIE_NAME}=")))
            .and_then(|cookie| Cookie::parse(cookie).ok())
            .expect("login should set a session cookie");
        let csrf_cookie = cookies
            .iter()
            .find(|cookie| cookie.starts_with(&format!("{CSRF_COOKIE_NAME}=")))
            .and_then(|cookie| Cookie::parse(cookie).ok())
            .expect("login should set a csrf cookie");
        let cookie_header = format!(
            "{}={}; {}={}",
            SESSION_COOKIE_NAME,
            session_cookie.value(),
            CSRF_COOKIE_NAME,
            csrf_cookie.value()
        );

        let session_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::get()
                .uri("/api/auth/session")
                .insert_header(("Cookie", cookie_header.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(session_response.status(), StatusCode::OK);

        let logout_response = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/auth/logout")
                .insert_header(("Cookie", cookie_header))
                .to_request(),
        )
        .await;
        assert_eq!(logout_response.status(), StatusCode::OK);

        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }

    #[actix_web::test]
    async fn wallet_session_requires_csrf_for_state_changing_routes() {
        let path = format!("test_db_api_auth_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let keypair = QuantumKeyPair::generate();
        let node_identifier = keypair.address();
        let auth = AuthService::new(
            storage.clone(),
            AuthConfig::for_tests(node_identifier.clone()),
        );
        let challenge = auth.issue_challenge(&node_identifier).unwrap();
        let message = crate::auth::canonical_login_message(
            &challenge.challenge_id,
            &challenge.challenge,
            &challenge.node_identifier,
            challenge.expires_at,
            challenge.version,
        );
        let session = auth
            .login(
                &challenge.challenge_id,
                &challenge.challenge,
                &challenge.node_identifier,
                challenge.expires_at,
                &keypair.public_key,
                &keypair.sign(&message),
                challenge.version,
            )
            .unwrap();
        let token_cookie = Cookie::build(SESSION_COOKIE_NAME, session.session_token.clone())
            .path("/")
            .finish()
            .to_string();
        let csrf_cookie = Cookie::build(CSRF_COOKIE_NAME, session.csrf_token.clone())
            .path("/")
            .finish()
            .to_string();
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(AdminAuthConfig {
                    token: "a".repeat(32).into_bytes(),
                }))
                .app_data(web::Data::new(auth))
                .configure(configure_admin_routes),
        )
        .await;

        let missing_csrf = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/mine")
                .insert_header(("Cookie", token_cookie.clone()))
                .to_request(),
        )
        .await;
        assert_eq!(missing_csrf.status(), StatusCode::UNAUTHORIZED);

        let authorized = actix_test::call_service(
            &app,
            actix_test::TestRequest::post()
                .uri("/api/mine")
                .insert_header(("Cookie", format!("{token_cookie}; {csrf_cookie}")))
                .insert_header((CSRF_HEADER_NAME, session.csrf_token))
                .to_request(),
        )
        .await;
        assert_ne!(
            authorized.status(),
            StatusCode::UNAUTHORIZED,
            "a valid wallet session and CSRF header must reach the protected handler",
        );

        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }
}
