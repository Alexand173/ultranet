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
use sha3::Digest;
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
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub chain_id: u32,
    #[serde(default = "default_legacy_transaction_version")]
    pub version: u32,
}

fn default_legacy_transaction_version() -> u32 {
    UltraBlockchain::LEGACY_TRANSACTION_VERSION
}

#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub address: String,
    /// Compatibility field: integer base units, not whole $ULTRA.
    pub balance: u64,
    /// Canonical explicit name for the integer account balance.
    pub balance_base_units: u64,
    /// Fixed-decimal human-readable representation derived from base units.
    pub balance_ultra: String,
    pub nonce: u64,
    pub decimals: u8,
    pub updated_at: u64,
}

#[derive(Debug, Serialize)]
pub struct FeeEstimateResponse {
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct TransactionView {
    pub id: String,
    pub hash: String,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub timestamp: u64,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeEstimateQuery {
    pub recipient: String,
    pub amount: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressTransactionsQuery {
    pub limit: Option<u64>,
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
            "Nullifier must contain exactly 32 bytes; received {}",
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

#[derive(Clone, Debug)]
struct AdminAuthConfig {
    token: Vec<u8>,
}

fn configured_admin_auth() -> io::Result<AdminAuthConfig> {
    let token = env::var(ADMIN_TOKEN_ENV).map_err(|_| missing_admin_token_error())?;
    validate_admin_token(&token)
}

fn missing_admin_token_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{ADMIN_TOKEN_ENV} is required for the node API. This private administrator bearer token protects state-changing node operations; it is not a wallet key or public node identifier. Create a 64-hex-character token with `openssl rand -hex 32`, put it only in UltraNetNode.env or the service environment, and start the node again. Never share it or place it in browser code."
        ),
    )
}

fn validate_admin_token(token: &str) -> io::Result<AdminAuthConfig> {
    if token.starts_with("replace-with-") {
        return Err(invalid_admin_token_error(
            "still contains the template placeholder; edit UltraNetNode.env beside UltraNetNode.exe",
        ));
    }
    if token.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(invalid_admin_token_error(
            "must not contain spaces, tabs, or other whitespace",
        ));
    }
    if token.len() < MIN_ADMIN_TOKEN_BYTES {
        return Err(invalid_admin_token_error(
            "must be at least 32 non-whitespace bytes; use 64 hexadecimal characters for 32 random bytes",
        ));
    }

    Ok(AdminAuthConfig {
        token: token.as_bytes().to_vec(),
    })
}

fn invalid_admin_token_error(reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{ADMIN_TOKEN_ENV} {reason}. This private administrator bearer token protects state-changing node operations; it is not a wallet key or public node identifier. Generate it with `openssl rand -hex 32` or the PowerShell command in README-WINDOWS.txt. Never share it or place it in browser code."
        ),
    )
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

fn transaction_view(tx: &Transaction, status: &str) -> TransactionView {
    let hash = tx.get_hash();
    TransactionView {
        id: hex::encode(&hash[..8]),
        hash: hex::encode(hash),
        sender: tx.sender.clone(),
        recipient: tx.recipient.clone(),
        amount: tx.amount,
        fee: tx.fee,
        nonce: tx.nonce,
        timestamp: tx.timestamp,
        status: status.to_string(),
    }
}

fn transaction_response(tx: &Transaction, status: &str) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Transaction accepted".to_string(),
        data: Some(serde_json::to_value(transaction_view(tx, status)).unwrap()),
    })
}

// 1. DODAJ TRANSAKCIJU
pub async fn add_transaction(
    state: web::Data<AppState>,
    req: web::Json<TransactionRequest>,
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

    if req.chain_id != UltraBlockchain::L1_CHAIN_ID
        || req.version != UltraBlockchain::LEGACY_TRANSACTION_VERSION
    {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Standard transfers require L1 chain_id 0 and transaction version 1"
                .to_string(),
            data: None,
        });
    }

    if let Some(existing) = blockchain.storage.get_transaction_by_nullifier(&nullifier) {
        let existing_hash = existing.get_hash();
        if existing.sender != req.sender
            || existing.sender_public_key != req.sender_public_key
            || existing.recipient != req.recipient
            || existing.amount != req.amount
            || existing.fee != req.fee
            || existing.nonce != req.nonce
            || existing.signature != req.signature
        {
            return HttpResponse::Conflict().json(ApiResponse {
                success: false,
                message: "Transaction nullifier is already bound to different fields".to_string(),
                data: None,
            });
        }
        let status = if blockchain.storage.is_pending_transaction(&existing_hash) {
            "pending"
        } else {
            "confirmed"
        };
        return transaction_response(&existing, status);
    }

    let mut recipient_bytes = [0u8; 32];
    let recipient_bytes_source = req.recipient.as_bytes();
    let recipient_length = std::cmp::min(recipient_bytes_source.len(), recipient_bytes.len());
    recipient_bytes[..recipient_length]
        .copy_from_slice(&recipient_bytes_source[..recipient_length]);

    let current_balance = blockchain.get_balance(&req.sender);
    let merkle_root = blockchain.merkle_tree.read().get_root();
    let mut merkle_root_array = [0u8; 32];
    merkle_root_array.copy_from_slice(&merkle_root[..32]);
    let public_key_digest = sha3::Sha3_256::digest(&req.sender_public_key);
    let mut public_key_digest_array = [0u8; 32];
    public_key_digest_array.copy_from_slice(&public_key_digest);

    // Generate the proof from the current account and state-root context. The
    // private circuit currently exposes only the nullifier as a public input;
    // using live values here prevents the public endpoint from retaining the
    // old demo balance/key/merkle placeholders.
    let circuit = PrivateTransactionCircuit {
        amount: Some(req.amount),
        recipient: Some(recipient_bytes),
        timestamp: Some(req.timestamp),
        merkle_root: Some(merkle_root_array),
        nullifier: Some(nullifier),
        block_height: Some(blockchain.chain.len() as u64),
        sender_balance: Some(current_balance),
        sender_public_key: Some(public_key_digest_array),
        sender_private_key_hash: Some([0; 32]),
        merkle_path: Some(vec![[0; 32]; MERKLE_TREE_DEPTH]),
        signature: Some([0; 64]),
    };

    let zk_proof = match blockchain.zk_engine.write().create_proof(circuit) {
        Ok(proof) => proof,
        Err(error) => {
            return HttpResponse::InternalServerError().json(ApiResponse {
                success: false,
                message: format!("ZK Proof Error: {error}"),
                data: None,
            });
        }
    };

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
        chain_id: req.chain_id,
        version: req.version,
    };
    let tx_hash = tx.get_hash();

    match blockchain.add_transaction(tx.clone()) {
        Ok(_) => transaction_response(
            &tx,
            if blockchain.storage.is_pending_transaction(&tx_hash) {
                "pending"
            } else {
                "confirmed"
            },
        ),
        Err(error) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Error: {error}"),
            data: None,
        }),
    }
}

// 2. DODAJ BLOK (RUDARENJE)
pub async fn mine_block(state: web::Data<AppState>) -> impl Responder {
    println!("⛏️ API: Starting mining...");
    let mut blockchain = state.blockchain.write();

    match blockchain.mine_block() {
        Ok(block) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: format!("Block {} added successfully!", block.index),
            data: Some(serde_json::to_value(&block).unwrap()),
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Error: {}", e),
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
        message: "Chain state".to_string(),
        data: Some(serde_json::to_value(&stats).unwrap()),
    })
}

// 4. PROVERI BALANS
pub async fn get_balance(state: web::Data<AppState>, address: web::Path<String>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let balance = blockchain.get_balance(&address);

    let address = address.into_inner();
    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Balance".to_string(),
        data: Some(serde_json::json!({
            "address": address,
            "balance": balance,
            "balance_base_units": balance,
            "balance_ultra": UltraBlockchain::format_base_units(balance),
            "decimals": UltraBlockchain::ULTRA_DECIMALS,
        })),
    })
}

pub async fn get_account(state: web::Data<AppState>, address: web::Path<String>) -> impl Responder {
    let address = address.into_inner();
    if !UltraBlockchain::is_valid_address(&address) {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Address must be a 64-character lowercase hexadecimal value".to_string(),
            data: None,
        });
    }

    let blockchain = state.blockchain.read();
    let updated_at = blockchain
        .chain
        .last()
        .map(|block| block.timestamp)
        .unwrap_or_else(|| chrono::Utc::now().timestamp().max(0) as u64);
    let balance = blockchain.get_balance(&address);
    let account = AccountResponse {
        address: address.clone(),
        balance,
        balance_base_units: balance,
        balance_ultra: UltraBlockchain::format_base_units(balance),
        nonce: blockchain.get_next_nonce(&address),
        decimals: UltraBlockchain::ULTRA_DECIMALS,
        updated_at,
    };

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Account".to_string(),
        data: Some(serde_json::to_value(account).unwrap()),
    })
}

pub async fn estimate_transaction_fee(query: web::Query<FeeEstimateQuery>) -> impl Responder {
    let recipient = query.recipient.trim().to_string();
    if !UltraBlockchain::is_valid_address(&recipient) {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Recipient must be a 64-character lowercase hexadecimal address".to_string(),
            data: None,
        });
    }
    if query.amount == 0 || query.amount > UltraBlockchain::MAX_TRANSFER_AMOUNT {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Amount must be greater than zero and within the transfer limit".to_string(),
            data: None,
        });
    }

    let fee = UltraBlockchain::minimum_transfer_fee(query.amount);
    let Some(total) = query.amount.checked_add(fee) else {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Transaction total exceeds the maximum integer value".to_string(),
            data: None,
        });
    };
    let estimate = FeeEstimateResponse {
        recipient,
        amount: query.amount,
        fee,
        gas_limit: 500_000,
        gas_price: 1,
        total,
    };

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Transaction fee estimate".to_string(),
        data: Some(serde_json::to_value(estimate).unwrap()),
    })
}

pub async fn get_address_transactions(
    state: web::Data<AppState>,
    address: web::Path<String>,
    query: web::Query<AddressTransactionsQuery>,
) -> impl Responder {
    let address = address.into_inner();
    if !UltraBlockchain::is_valid_address(&address) {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "Address must be a 64-character lowercase hexadecimal value".to_string(),
            data: None,
        });
    }
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: "limit must be between 1 and 100".to_string(),
            data: None,
        });
    }
    let limit = limit as usize;
    let blockchain = state.blockchain.read();
    let mut transactions = Vec::new();

    for block in blockchain.chain.iter().rev() {
        for tx in block.transactions.iter().rev() {
            if tx.sender == address || tx.recipient == address {
                transactions.push(transaction_view(tx, "confirmed"));
                if transactions.len() >= limit {
                    break;
                }
            }
        }
        if transactions.len() >= limit {
            break;
        }
    }

    if transactions.len() < limit {
        let pending = match blockchain
            .storage
            .get_pending_transactions_for_address(&address)
        {
            Ok(pending) => pending,
            Err(error) => {
                return HttpResponse::InternalServerError().json(ApiResponse {
                    success: false,
                    message: format!("Failed to load pending transactions: {error}"),
                    data: None,
                });
            }
        };
        for tx in pending {
            transactions.push(transaction_view(&tx, "pending"));
            if transactions.len() >= limit {
                break;
            }
        }
    }

    transactions.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    transactions.truncate(limit);
    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Address transactions",
        "transactions": transactions,
    }))
}

// 5. PROVERI VALIDNOST
pub async fn validate_chain(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let valid = blockchain.is_chain_valid();

    HttpResponse::Ok().json(ApiResponse {
        success: valid,
        message: if valid {
            "Chain is valid!".to_string()
        } else {
            "Chain is invalid!".to_string()
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
            message: "Block found".to_string(),
            data: Some(serde_json::to_value(block).unwrap()),
        })
    } else {
        HttpResponse::NotFound().json(ApiResponse {
            success: false,
            message: "Block not found".to_string(),
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
        message: "Statistics".to_string(),
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
            message: "Recursive proof found".to_string(),
            data: Some(serde_json::json!({
                "proof": hex::encode(&proof),
                "size": proof.len()
            })),
        }),
        None => HttpResponse::NotFound().json(ApiResponse {
            success: false,
            message: "No recursive proof found".to_string(),
            data: None,
        }),
    }
}

pub async fn verify_recursive_chain(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();

    match blockchain.verify_recursive_chain() {
        Ok(true) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            message: "Recursive chain is valid!".to_string(),
            data: Some(serde_json::json!({ "valid": true })),
        }),
        Ok(false) => HttpResponse::Ok().json(ApiResponse {
            success: false,
            message: "Recursive chain is invalid!".to_string(),
            data: Some(serde_json::json!({ "valid": false })),
        }),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Error: {}", e),
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
#[serde(deny_unknown_fields)]
pub struct CreateAppChainRequest {
    pub name: String,
    /// A display owner address or alias. The node derives a separate treasury
    /// address and never accepts a caller-selected treasury account.
    pub owner: String,
}

#[derive(Debug, Serialize)]
pub struct AppChainView {
    pub id: u32,
    pub name: String,
    pub owner: String,
    /// Dedicated real L1 treasury address. Fund it using a standard transfer.
    pub account_address: String,
    pub genesis_root: String,
    pub anchor_fee: String,
    pub balance: String,
    pub anchor_spend: String,
    pub anchor_count: u64,
    pub latest_anchor_at: Option<u64>,
    pub latest_state_root: Option<String>,
    pub anchor_availability: &'static str,
    pub proof_scheme: &'static str,
}

#[derive(Debug, Serialize)]
pub struct AppChainOverviewTotals {
    pub anchor_count: u64,
    pub anchor_spend: String,
}

#[derive(Debug, Serialize)]
pub struct AppChainOverviewResponse {
    pub success: bool,
    pub chains: Vec<AppChainView>,
    pub totals: AppChainOverviewTotals,
    pub anchor_availability: &'static str,
    pub proof_scheme: &'static str,
    pub updated_at: u64,
}

#[derive(Debug, Serialize)]
pub struct AppChainListResponse {
    pub success: bool,
    pub chains: Vec<AppChainView>,
}

#[derive(Debug, Serialize)]
pub struct CreateAppChainResponse {
    pub success: bool,
    pub message: String,
    pub chain_id: u32,
    pub chain: AppChainView,
}

#[derive(Debug, Serialize)]
pub struct AppChainAnchorResponse {
    pub success: bool,
    pub message: String,
    pub chain_id: u32,
    pub anchor_number: u64,
    pub state_root: String,
    pub timestamp: u64,
    pub anchor_count: u64,
    pub charged_base_units: String,
    pub balance: String,
    pub account_address: String,
    pub proof_scheme: &'static str,
    pub is_test: bool,
}

/// JSON body retained only for the legacy client-supplied anchor route.
/// The route rejects this body; production anchoring is server-generated.
#[derive(Debug, Deserialize)]
pub struct AppChainAnchorRequest {
    pub chain_id: u32,
    pub state_root: String,
    pub proof: String,
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
    /// Compatibility field in whole $ULTRA units.
    pub genesis_allocation: u64,
    pub genesis_allocation_ultra: u64,
    pub genesis_allocation_base_units: u64,
    pub genesis_allocation_display: String,
    pub decimals: u8,
    pub sovereign_address: String,
    pub multi_sig_threshold: String,
    pub signature_scheme: String,
    pub signature_size: usize,
    pub halving_interval: u64,
    /// Compatibility field: base block reward in protocol base units.
    pub base_reward: u64,
    pub base_reward_base_units: u64,
    pub base_reward_ultra: String,
    pub consensus_protocol: String,
    pub verified_latency: String,
}

// Handler for /api/manifest
async fn get_manifest() -> impl Responder {
    let manifest = ManifestResponse {
        version: "7.1 Sovereign".to_string(),
        ticker: "$ULTRA".to_string(),
        genesis_allocation: UltraBlockchain::GENESIS_ALLOCATION_ULTRA,
        genesis_allocation_ultra: UltraBlockchain::GENESIS_ALLOCATION_ULTRA,
        genesis_allocation_base_units: UltraBlockchain::GENESIS_ALLOCATION_BASE_UNITS,
        genesis_allocation_display: format!(
            "{} $ULTRA",
            UltraBlockchain::format_base_units(UltraBlockchain::GENESIS_ALLOCATION_BASE_UNITS)
        ),
        decimals: UltraBlockchain::ULTRA_DECIMALS,
        sovereign_address: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        multi_sig_threshold: format!("{}-of-3", UltraBlockchain::SOVEREIGN_THRESHOLD),
        signature_scheme: "Dilithium-5 (Lattice-based)".to_string(),
        signature_size: 4627,
        halving_interval: 31_557_600,
        base_reward: UltraBlockchain::GENESIS_REWARD,
        base_reward_base_units: UltraBlockchain::GENESIS_REWARD,
        base_reward_ultra: UltraBlockchain::format_base_units(UltraBlockchain::GENESIS_REWARD),
        consensus_protocol: "Bullshark / Mysticeti DAG".to_string(),
        verified_latency: "27.79µs / vertex".to_string(),
    };

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        message: "Protocol Manifest".to_string(),
        data: Some(serde_json::to_value(&manifest).unwrap()),
    })
}

fn validate_appchain_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("AppChain name is required".to_string());
    }
    if name.len() > 80 {
        return Err("AppChain name must be 80 characters or fewer".to_string());
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || " -_".contains(character))
    {
        return Err(
            "AppChain name may contain only letters, numbers, spaces, hyphens, and underscores"
                .to_string(),
        );
    }
    Ok(name.to_string())
}

// Create AppChain
async fn create_appchain(
    state: web::Data<AppState>,
    req: web::Json<CreateAppChainRequest>,
) -> impl Responder {
    let name = match validate_appchain_name(&req.name) {
        Ok(name) => name,
        Err(message) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": message,
            }))
        }
    };
    let owner = req.owner.trim();
    if owner.is_empty() || owner.len() > 120 {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "AppChain owner is required and must be 120 characters or fewer",
        }));
    }
    let blockchain = state.blockchain.read();
    let mut registry = blockchain.appchain_registry.write();
    let chain_id = match registry.next_chain_id() {
        Ok(chain_id) => chain_id,
        Err(message) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": message,
            }))
        }
    };
    let config = crate::appchain::AppChainConfig {
        id: chain_id,
        name,
        owner: owner.to_string(),
        account_address: crate::appchain::derive_appchain_treasury_address(chain_id),
        genesis_root: [0u8; 32],
        anchor_fee: crate::appchain::DEFAULT_APPCHAIN_ANCHOR_FEE,
        anchor_spend: 0,
        anchor_count: 0,
        latest_anchor_at: None,
        latest_state_root: None,
    };

    if let Err(message) = blockchain.storage.save_appchain_config(&config) {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Unable to persist AppChain config: {message}"),
        }));
    }
    if let Err(message) = registry.register_chain(config.clone()) {
        let _ = blockchain.storage.delete_appchain_config(chain_id);
        return HttpResponse::Conflict().json(serde_json::json!({
            "success": false,
            "message": message,
        }));
    }
    drop(registry);

    // Keep AppChain state under the same durable root as the node. If the
    // isolated runtime cannot open, roll back the registry record instead of
    // panicking after the config has been persisted.
    let db_path = std::env::var("ULTRANET_DB_PATH").unwrap_or_else(|_| "ultranet_db".to_string());
    if let Err(error) = crate::appchain::AppChainRuntime::try_new(chain_id, &db_path) {
        let _ = blockchain.storage.delete_appchain_config(chain_id);
        blockchain
            .appchain_registry
            .write()
            .active_chains
            .remove(&chain_id);
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Unable to open AppChain runtime: {error}"),
        }));
    }

    HttpResponse::Ok().json(CreateAppChainResponse {
        success: true,
        message: format!(
            "AppChain #{} ('{}') created successfully!",
            config.id, config.name
        ),
        chain_id,
        chain: appchain_view(&blockchain, &config),
    })
}

fn test_anchoring_enabled() -> bool {
    cfg!(debug_assertions)
        && matches!(
            env::var("ULTRANET_ENABLE_TEST_ANCHORING").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        )
}

const APPCHAIN_PROOF_SCHEME: &str = "SHA3-256 server state commitment v1";

fn appchain_anchor_availability() -> &'static str {
    "production"
}

fn appchain_view(
    blockchain: &UltraBlockchain,
    config: &crate::appchain::AppChainConfig,
) -> AppChainView {
    AppChainView {
        id: config.id,
        name: config.name.clone(),
        owner: config.owner.clone(),
        account_address: config.account_address.clone(),
        genesis_root: hex::encode(config.genesis_root),
        anchor_fee: config.anchor_fee.to_string(),
        balance: blockchain.get_appchain_treasury_balance(config).to_string(),
        anchor_spend: config.anchor_spend.to_string(),
        anchor_count: config.anchor_count,
        latest_anchor_at: config.latest_anchor_at,
        latest_state_root: config.latest_state_root.clone(),
        anchor_availability: appchain_anchor_availability(),
        proof_scheme: APPCHAIN_PROOF_SCHEME,
    }
}

fn appchain_views(
    blockchain: &UltraBlockchain,
    registry: &crate::appchain::AppChainRegistry,
) -> Vec<AppChainView> {
    let mut chains = registry.active_chains.values().collect::<Vec<_>>();
    chains.sort_by_key(|config| config.id);
    chains
        .into_iter()
        .map(|config| appchain_view(blockchain, config))
        .collect()
}

fn now_seconds() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

async fn anchor_appchain(state: web::Data<AppState>, path: web::Path<u32>) -> impl Responder {
    run_appchain_anchor(state, path.into_inner(), false).await
}

async fn legacy_anchor_appchain(
    _state: web::Data<AppState>,
    _req: web::Json<AppChainAnchorRequest>,
) -> impl Responder {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "success": false,
        "message": "Use POST /api/appchain/{chain_id}/anchor; client-supplied roots and proofs are not accepted."
    }))
}

// Development-only compatibility path. It uses the same server-side state and
// proof pipeline, but is labelled as a test result for local UI QA.
async fn anchor_appchain_test(state: web::Data<AppState>, path: web::Path<u32>) -> impl Responder {
    if !test_anchoring_enabled() {
        return HttpResponse::NotImplemented().json(serde_json::json!({
            "success": false,
            "message": "Test-only AppChain anchoring is disabled. Set ULTRANET_ENABLE_TEST_ANCHORING=true for development only."
        }));
    }
    run_appchain_anchor(state, path.into_inner(), true).await
}

async fn run_appchain_anchor(
    state: web::Data<AppState>,
    chain_id: u32,
    is_test: bool,
) -> HttpResponse {
    let blockchain = state.blockchain.read();
    let mut registry = blockchain.appchain_registry.write();
    let Some(previous_config) = registry.get_chain(chain_id).cloned() else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "success": false,
            "message": format!("AppChain #{chain_id} was not found"),
        }));
    };
    let db_path = env::var("ULTRANET_DB_PATH").unwrap_or_else(|_| "ultranet_db".to_string());
    let runtime = match crate::appchain::AppChainRuntime::try_new(chain_id, &db_path) {
        Ok(runtime) => runtime,
        Err(error) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Unable to open AppChain runtime: {error}"),
            }))
        }
    };
    let snapshot = match runtime.snapshot_state() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Unable to snapshot AppChain state: {error}"),
            }))
        }
    };
    let timestamp = now_seconds();
    let anchor_number = match previous_config.anchor_count.checked_add(1) {
        Some(anchor_number) => anchor_number,
        None => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("AppChain #{chain_id} anchor count overflowed"),
            }))
        }
    };
    let proof = runtime.create_anchor_proof(
        &snapshot,
        anchor_number,
        &previous_config.account_address,
        previous_config.anchor_fee,
        timestamp,
    );
    if let Err(error) = runtime.verify_anchor_proof(&snapshot, &proof) {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Server-generated AppChain proof failed verification: {error}"),
        }));
    }
    let proof_json = match serde_json::to_string(&proof) {
        Ok(proof) => proof,
        Err(error) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Unable to encode AppChain proof: {error}"),
            }))
        }
    };
    let treasury_balance = blockchain.get_appchain_treasury_balance(&previous_config);
    let (updated_config, anchor) = match registry.preview_anchor(
        chain_id,
        treasury_balance,
        hex::encode(snapshot.state_root),
        proof_json,
        timestamp,
        is_test,
    ) {
        Ok(result) => result,
        Err(message) => {
            return HttpResponse::PaymentRequired().json(serde_json::json!({
                "success": false,
                "message": message,
            }))
        }
    };

    if let Err(error) = blockchain.storage.save_appchain_config(&updated_config) {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Unable to persist AppChain treasury debit: {error}"),
        }));
    }
    if let Err(error) = blockchain.storage.save_appchain_anchor(&anchor) {
        let _ = blockchain.storage.save_appchain_config(&previous_config);
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Unable to persist AppChain anchor: {error}"),
        }));
    }
    if let Err(message) = registry.apply_anchor(updated_config.clone(), anchor.clone()) {
        let _ = blockchain.storage.save_appchain_config(&previous_config);
        let _ = blockchain.storage.delete_appchain_anchor(&anchor);
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("Unable to apply AppChain anchor: {message}"),
        }));
    }

    let balance = blockchain.get_appchain_treasury_balance(&updated_config);
    HttpResponse::Ok().json(AppChainAnchorResponse {
        success: true,
        message: if is_test {
            format!("AppChain #{chain_id} test anchor completed.")
        } else {
            format!("AppChain #{chain_id} anchored with server-verified state proof.")
        },
        chain_id,
        anchor_number: anchor.anchor_number,
        state_root: anchor.state_root,
        timestamp: anchor.timestamp,
        anchor_count: updated_config.anchor_count,
        charged_base_units: anchor.fee_charged.to_string(),
        balance: balance.to_string(),
        account_address: updated_config.account_address,
        proof_scheme: APPCHAIN_PROOF_SCHEME,
        is_test,
    })
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyCorrectionRequest {
    /// Fixed protocol correction identifier, encoded as 64 hexadecimal chars.
    pub correction_id: String,
    pub target_address: String,
    pub expected_balance_base_units: u64,
    pub target_balance_base_units: u64,
    pub timestamp: u64,
    pub nonce: u64,
    pub nullifier: Vec<u8>,
    /// Concatenated Dilithium-5 signatures from two distinct sovereign owners.
    pub signature: Vec<u8>,
    pub version: u32,
}

fn parse_fixed_32_hex(value: &str, field: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(value.trim()).map_err(|_| format!("{field} must be hexadecimal"))?;
    if bytes.len() != 32 {
        return Err(format!("{field} must contain exactly 32 bytes"));
    }
    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

/// Submit the one-time sovereign genesis supply correction.
///
/// The sender and transaction envelope are fixed by the node. Authority comes
/// from the version-4 2-of-3 sovereign signatures, not from the admin bearer
/// token or a caller-selected Move function.
pub async fn submit_supply_correction(
    state: web::Data<AppState>,
    req: web::Json<SupplyCorrectionRequest>,
) -> impl Responder {
    if req.version != UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION {
        return HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!(
                "Supply corrections require signing-envelope version {}",
                UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION
            ),
            data: None,
        });
    }

    let correction_id = match parse_fixed_32_hex(&req.correction_id, "correction_id") {
        Ok(value) => value,
        Err(message) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message,
                data: None,
            })
        }
    };
    let nullifier = match parse_nullifier(&req.nullifier) {
        Ok(value) => value,
        Err(message) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message,
                data: None,
            })
        }
    };
    let payload = TransactionPayload::SovereignSupplyCorrection {
        correction_id,
        target_address: req.target_address.trim().to_string(),
        expected_balance: req.expected_balance_base_units,
        target_balance: req.target_balance_base_units,
    };
    let tx = Transaction {
        sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
        sender_public_key: vec![],
        recipient: crate::supply_correction::SUPPLY_CORRECTION_RECIPIENT.to_string(),
        amount: 0,
        signature: req.signature.clone(),
        zk_proof: vec![],
        nullifier,
        timestamp: req.timestamp,
        fee: 0,
        nonce: req.nonce,
        gas_limit: crate::supply_correction::SUPPLY_CORRECTION_GAS_LIMIT,
        gas_price: crate::supply_correction::SUPPLY_CORRECTION_GAS_PRICE,
        proof_type: ProofType::Ownership,
        payload,
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: req.version,
    };
    let blockchain = state.blockchain.read();
    match blockchain.add_transaction(tx.clone()) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Sovereign supply correction accepted",
            "data": {
                "transaction": transaction_view(&tx, "pending"),
                "correction_id": hex::encode(correction_id),
                "target_balance_base_units": req.target_balance_base_units,
                "decimals": UltraBlockchain::ULTRA_DECIMALS,
            }
        })),
        Err(message) => HttpResponse::BadRequest().json(ApiResponse {
            success: false,
            message: format!("Supply correction rejected: {message}"),
            data: None,
        }),
    }
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
                message: "proposal_hash must be 64 hexadecimal characters".to_string(),
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
            message: "Validator proposal not found".to_string(),
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
            message: format!("limit must be between 1 and {MAX_APPROVAL_PAGE_SIZE}"),
            data: None,
        });
    }
    let limit = match usize::try_from(limit) {
        Ok(limit) => limit,
        Err(_) => {
            return HttpResponse::BadRequest().json(ApiResponse {
                success: false,
                message: "limit is too large".to_string(),
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
                    message: "cursor must be 80 hexadecimal characters".to_string(),
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

fn appchain_overview_totals(
    blockchain: &UltraBlockchain,
    registry: &crate::appchain::AppChainRegistry,
) -> AppChainOverviewTotals {
    let chains = appchain_views(blockchain, registry);
    AppChainOverviewTotals {
        anchor_count: chains
            .iter()
            .map(|chain| chain.anchor_count)
            .fold(0, u64::saturating_add),
        anchor_spend: chains
            .iter()
            .filter_map(|chain| chain.anchor_spend.parse::<u64>().ok())
            .fold(0, u64::saturating_add)
            .to_string(),
    }
}

// Get AppChain list
async fn list_appchains(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let registry = blockchain.appchain_registry.read();

    HttpResponse::Ok().json(AppChainListResponse {
        success: true,
        chains: appchain_views(&blockchain, &registry),
    })
}

// Get AppChain registry data used by the operator dashboard.
async fn appchain_overview(state: web::Data<AppState>) -> impl Responder {
    let blockchain = state.blockchain.read();
    let registry = blockchain.appchain_registry.read();

    HttpResponse::Ok().json(AppChainOverviewResponse {
        success: true,
        chains: appchain_views(&blockchain, &registry),
        totals: appchain_overview_totals(&blockchain, &registry),
        anchor_availability: appchain_anchor_availability(),
        proof_scheme: APPCHAIN_PROOF_SCHEME,
        updated_at: now_seconds(),
    })
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
                let status = if blockchain.storage.is_pending_transaction(&hash_arr) {
                    "pending"
                } else {
                    "confirmed"
                };
                return HttpResponse::Ok().json(ApiResponse {
                    success: true,
                    message: "Transaction found".to_string(),
                    data: Some(serde_json::to_value(transaction_view(&tx, status)).unwrap()),
                });
            }
        }
    }

    HttpResponse::NotFound().json(ApiResponse {
        success: false,
        message: "Transaction not found".to_string(),
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
    parse_api_bind(&bind)
}

fn parse_api_bind(raw: &str) -> io::Result<String> {
    let bind = raw.trim();

    if bind.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ULTRANET_API_BIND cannot be empty",
        ));
    }

    bind.parse::<std::net::SocketAddr>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ULTRANET_API_BIND must be a valid IP:port address: {error}"),
        )
    })?;

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

/// Validate startup configuration before opening storage or running expensive setup.
pub fn validate_configuration() -> io::Result<()> {
    let _ = configured_api_bind()?;
    let _ = configured_cors_origins()?;
    let _ = configured_admin_auth()?;
    AuthConfig::from_env()
        .map(|_| ())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

// ===== START SERVER =====

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
            .route(web::post().to(legacy_anchor_appchain)),
    )
    .service(
        web::resource("/api/appchain/{chain_id}/anchor")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(anchor_appchain)),
    )
    .service(
        web::resource("/api/appchain/{chain_id}/anchor/test")
            .wrap(from_fn(require_admin_token))
            .route(web::post().to(anchor_appchain_test)),
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

    println!("🚀 Starting REST API server at http://{api_bind}");
    println!("🔒 CORS allowlist: {}", cors_origins.join(", "));
    println!("🔐 Administrative bearer authentication: enabled");
    println!("📋 Endpoints:");
    println!("   POST /api/transaction - Add wallet-signed transaction");
    println!("   GET  /api/transaction/:hash - Transaction status");
    println!("   GET  /api/transaction/estimate - Fee estimate");
    println!("   GET  /api/account/:address - Account balance and nonce");
    println!("   GET  /api/address/:address/transactions - Address history");
    println!("   POST /api/mine - Mine block");
    println!("   GET  /api/chain - Chain state");
    println!("   GET  /api/balance/:address - Balance");
    println!("   GET  /api/block/:index - Block by index");
    println!("   GET  /api/validate - Validate chain");
    println!("   GET  /api/stats - Statistics");
    println!("   GET  /api/recursive/proof - Recursive ZK proof");
    println!("   GET  /api/recursive/verify - Verify chain");
    println!("   GET  /api/stm/stats - Block-STM statistics");
    println!("   GET  /api/fhe/pk - FHE Public Key");
    println!("   POST /api/appchain/create - Create L3 AppChain with dedicated treasury");
    println!("   POST /api/appchain/{{chain_id}}/anchor - Server-verified AppChain anchor");
    println!("   POST /api/appchain/{{chain_id}}/anchor/test - Development-only fixture anchor");
    println!("   POST /api/governance/propose - Submit validator proposal");
    println!("   POST /api/governance/approve - Submit version-3 sovereign approval");
    println!(
        "   POST /api/governance/supply-correction - Submit one-time version-4 supply correction"
    );
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
            .route(
                "/api/transaction/estimate",
                web::get().to(estimate_transaction_fee),
            )
            .route("/api/transaction/{hash}", web::get().to(get_transaction))
            .route("/api/account/{address}", web::get().to(get_account))
            .route(
                "/api/address/{address}/transactions",
                web::get().to(get_address_transactions),
            )
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
            .route("/api/appchain/overview", web::get().to(appchain_overview))
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
            .route(
                "/api/governance/supply-correction",
                web::post().to(submit_supply_correction),
            )
            .route("/api/governance/proposals", web::get().to(list_proposals))
            .route(
                "/api/governance/approvals",
                web::get().to(list_approval_journal),
            )
            .route("/api/manifest", web::get().to(get_manifest))
            .route("/api/ai/history", web::get().to(get_ai_history))
            .route("/api/zk/progress", web::get().to(get_zk_progress))
            .route("/api/search/{query}", web::get().to(search))
    })
    .bind(api_bind)?
    .run()
    .await
}

#[cfg(test)]
mod configuration_tests {
    use super::{
        configure_admin_routes, csrf_cookie, missing_admin_token_error, parse_api_bind,
        parse_cors_origins, require_admin_token, session_cookie, validate_admin_token,
        AdminAuthConfig, AuthChallengeRequest, AuthLoginRequest,
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
    fn api_bind_parser_accepts_ip_and_port() {
        assert_eq!(
            parse_api_bind(" 127.0.0.1:8081 ").unwrap(),
            "127.0.0.1:8081"
        );
    }

    #[test]
    fn api_bind_parser_rejects_empty_and_malformed_values() {
        assert!(parse_api_bind(" ").is_err());
        assert!(parse_api_bind("localhost:8081").is_err());
        assert!(parse_api_bind("127.0.0.1").is_err());
    }

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
    fn admin_token_validation_rejects_missing_quality_values() {
        for token in [
            "replace-with-a-token",
            "short",
            "token with whitespace and enough length",
        ] {
            let error = validate_admin_token(token).unwrap_err().to_string();
            assert!(error.contains("ULTRANET_ADMIN_TOKEN"));
            assert!(!error.contains(token));
        }
    }

    #[test]
    fn admin_token_validation_accepts_a_32_byte_token_without_echoing_it() {
        let token = "a".repeat(32);
        let config = validate_admin_token(&token).expect("32-byte token should be accepted");
        assert_eq!(config.token, token.as_bytes());
    }

    #[test]
    fn admin_token_validation_accepts_the_recommended_64_hex_format() {
        let token = "0123456789abcdef".repeat(4);
        let config =
            validate_admin_token(&token).expect("64 hexadecimal characters should be accepted");
        assert_eq!(config.token, token.as_bytes());
    }

    #[test]
    fn admin_token_validation_explains_common_desktop_mistakes() {
        let placeholder_error = validate_admin_token("replace-with-a-token")
            .unwrap_err()
            .to_string();
        assert!(placeholder_error.contains("template placeholder"));

        let whitespace_error = validate_admin_token(&format!("{} ", "a".repeat(32)))
            .unwrap_err()
            .to_string();
        assert!(whitespace_error.contains("spaces, tabs, or other whitespace"));

        let short_error = validate_admin_token(&"a".repeat(31))
            .unwrap_err()
            .to_string();
        assert!(short_error.contains("64 hexadecimal characters"));
    }

    #[test]
    fn missing_admin_token_error_contains_actionable_english_guidance() {
        let error = missing_admin_token_error().to_string();
        assert!(error.contains("private administrator bearer token"));
        assert!(error.contains("openssl rand -hex 32"));
        assert!(error.contains("not a wallet key or public node identifier"));
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
            "/api/appchain/1/anchor/test",
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
