use crate::{quantum_crypto::QuantumKeyPair, Storage};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::{
    collections::HashSet,
    env,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

pub const AUTH_PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_CHALLENGE_TTL_SECONDS: u64 = 300;
pub const DEFAULT_SESSION_TTL_SECONDS: u64 = 28_800;
pub const MAX_CHALLENGE_TTL_SECONDS: u64 = 900;
pub const MAX_SESSION_TTL_SECONDS: u64 = 86_400;
pub const SESSION_COOKIE_NAME: &str = "ultranet_session";
pub const CSRF_COOKIE_NAME: &str = "ultranet_csrf";
pub const CSRF_HEADER_NAME: &str = "X-UltraNet-CSRF";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChallengeRecord {
    challenge: String,
    node_identifier: String,
    expires_at: u64,
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRecord {
    node_identifier: String,
    csrf_hash: [u8; 32],
    created_at: u64,
    expires_at: u64,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub authorized_node_identifiers: HashSet<String>,
    pub challenge_ttl_seconds: u64,
    pub session_ttl_seconds: u64,
    pub secure_cookie: bool,
    pub cookie_domain: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self, String> {
        let authorized_node_identifiers = match env::var("ULTRANET_AUTHORIZED_NODE_IDENTIFIERS") {
            Ok(raw) => raw
                .split(',')
                .map(str::trim)
                .filter(|identifier| !identifier.is_empty())
                .map(validate_node_identifier)
                .collect::<Result<HashSet<_>, _>>()?,
            Err(env::VarError::NotPresent) => HashSet::new(),
            Err(env::VarError::NotUnicode(_)) => {
                return Err("ULTRANET_AUTHORIZED_NODE_IDENTIFIERS must be valid UTF-8".into())
            }
        };

        let challenge_ttl_seconds = parse_bounded_seconds(
            "ULTRANET_AUTH_CHALLENGE_TTL_SECONDS",
            DEFAULT_CHALLENGE_TTL_SECONDS,
            1,
            MAX_CHALLENGE_TTL_SECONDS,
        )?;
        let session_ttl_seconds = parse_bounded_seconds(
            "ULTRANET_AUTH_SESSION_TTL_SECONDS",
            DEFAULT_SESSION_TTL_SECONDS,
            1,
            MAX_SESSION_TTL_SECONDS,
        )?;
        let secure_cookie = match env::var("ULTRANET_SESSION_COOKIE_SECURE") {
            Ok(value) => match value.trim() {
                "true" => true,
                "false" => false,
                _ => return Err("ULTRANET_SESSION_COOKIE_SECURE must be true or false".into()),
            },
            Err(env::VarError::NotPresent) => true,
            Err(env::VarError::NotUnicode(_)) => {
                return Err("ULTRANET_SESSION_COOKIE_SECURE must be valid UTF-8".into())
            }
        };
        let cookie_domain = match env::var("ULTRANET_AUTH_COOKIE_DOMAIN") {
            Ok(value) => Some(validate_cookie_domain(&value)?),
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                return Err("ULTRANET_AUTH_COOKIE_DOMAIN must be valid UTF-8".into())
            }
        };

        Ok(Self {
            authorized_node_identifiers,
            challenge_ttl_seconds,
            session_ttl_seconds,
            secure_cookie,
            cookie_domain,
        })
    }

    pub fn for_tests(node_identifier: String) -> Self {
        Self {
            authorized_node_identifiers: [node_identifier].into_iter().collect(),
            challenge_ttl_seconds: DEFAULT_CHALLENGE_TTL_SECONDS,
            session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
            secure_cookie: false,
            cookie_domain: None,
        }
    }
}

fn validate_cookie_domain(raw: &str) -> Result<String, String> {
    let domain = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    let valid = !domain.is_empty()
        && domain.len() <= 253
        && !domain.contains("..")
        && domain
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        && !domain.starts_with('-')
        && !domain.ends_with('-')
        && !domain.starts_with('.')
        && !domain.ends_with('.');

    if !valid {
        return Err(
            "ULTRANET_AUTH_COOKIE_DOMAIN must be a hostname without a scheme, path, whitespace, or empty labels"
                .into(),
        );
    }
    Ok(domain)
}

fn parse_bounded_seconds(
    name: &str,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, String> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .parse::<u64>()
            .map_err(|_| format!("{name} must be an integer"))?,
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => return Err(format!("{name} must be valid UTF-8")),
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(format!(
            "{name} must be between {minimum} and {maximum} seconds"
        ));
    }
    Ok(value)
}

pub fn validate_node_identifier(raw: &str) -> Result<String, String> {
    let identifier = raw.trim().to_ascii_lowercase();
    if identifier.len() != 64 || !identifier.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("node identifier must be a 64-character hexadecimal value".into());
    }
    Ok(identifier)
}

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn random_hex(byte_count: usize) -> String {
    let mut bytes = vec![0u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash_secret(secret: &str) -> [u8; 32] {
    Sha3_256::digest(secret.as_bytes()).into()
}

fn canonical_challenge_message(
    challenge_id: &str,
    challenge: &str,
    node_identifier: &str,
    expires_at: u64,
    version: u32,
) -> Vec<u8> {
    format!(
        "ULTRANET_AUTH_V{version}\n{challenge_id}\n{challenge}\n{node_identifier}\n{expires_at}"
    )
    .into_bytes()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthChallenge {
    #[serde(rename = "challengeId")]
    pub challenge_id: String,
    pub challenge: String,
    #[serde(rename = "nodeIdentifier")]
    pub node_identifier: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: u64,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub session_token: String,
    pub node_identifier: String,
    pub csrf_token: String,
    pub expires_at: u64,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidRequest(String),
    Unauthorized(String),
    Storage(String),
}

impl AuthError {
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRequest(message)
            | Self::Unauthorized(message)
            | Self::Storage(message) => message,
        }
    }
}

pub struct AuthService {
    pub storage: Arc<Storage>,
    pub config: AuthConfig,
}

impl AuthService {
    pub fn new(storage: Arc<Storage>, config: AuthConfig) -> Self {
        Self { storage, config }
    }

    pub fn issue_challenge(&self, node_identifier: &str) -> Result<AuthChallenge, AuthError> {
        let node_identifier =
            validate_node_identifier(node_identifier).map_err(AuthError::InvalidRequest)?;
        let challenge_id = random_hex(24);
        let challenge = random_hex(32);
        let expires_at = now_seconds().saturating_add(self.config.challenge_ttl_seconds);
        let record = ChallengeRecord {
            challenge: challenge.clone(),
            node_identifier: node_identifier.clone(),
            expires_at,
            version: AUTH_PROTOCOL_VERSION,
        };
        let encoded = bincode::serialize(&record).map_err(|error| {
            AuthError::Storage(format!("failed to encode auth challenge: {error}"))
        })?;
        self.storage
            .auth_challenges
            .insert(challenge_id.as_bytes(), encoded)
            .map_err(|error| {
                AuthError::Storage(format!("failed to persist auth challenge: {error}"))
            })?;
        self.storage.db.flush().map_err(|error| {
            AuthError::Storage(format!("failed to flush auth challenge: {error}"))
        })?;
        Ok(AuthChallenge {
            challenge_id,
            challenge,
            node_identifier,
            expires_at,
            version: AUTH_PROTOCOL_VERSION,
        })
    }

    pub fn login(
        &self,
        challenge_id: &str,
        challenge: &str,
        node_identifier: &str,
        expires_at: u64,
        public_key: &[u8],
        signature: &[u8],
        version: u32,
    ) -> Result<AuthSession, AuthError> {
        if version != AUTH_PROTOCOL_VERSION {
            return Err(AuthError::InvalidRequest(
                "unsupported authentication protocol version".into(),
            ));
        }
        let node_identifier =
            validate_node_identifier(node_identifier).map_err(AuthError::InvalidRequest)?;
        if challenge_id.trim().is_empty() || challenge.trim().is_empty() {
            return Err(AuthError::InvalidRequest(
                "challenge credentials are incomplete".into(),
            ));
        }
        if expires_at < now_seconds() {
            return Err(AuthError::Unauthorized(
                "authentication challenge expired".into(),
            ));
        }
        if !self
            .config
            .authorized_node_identifiers
            .contains(&node_identifier)
        {
            return Err(AuthError::Unauthorized("authentication failed".into()));
        }

        let stored = self
            .storage
            .auth_challenges
            .get(challenge_id.as_bytes())
            .map_err(|error| AuthError::Storage(format!("failed to read auth challenge: {error}")))?
            .ok_or_else(|| {
                AuthError::Unauthorized(
                    "authentication challenge is invalid or already used".into(),
                )
            })?;
        let record: ChallengeRecord = bincode::deserialize(&stored).map_err(|error| {
            AuthError::Storage(format!("failed to decode auth challenge: {error}"))
        })?;
        if record.version != version
            || record.challenge != challenge
            || record.node_identifier != node_identifier
            || record.expires_at != expires_at
            || record.expires_at < now_seconds()
        {
            return Err(AuthError::Unauthorized(
                "authentication challenge is invalid or expired".into(),
            ));
        }

        let derived_identifier = QuantumKeyPair::address_from_public_key(public_key);
        if derived_identifier != node_identifier {
            return Err(AuthError::Unauthorized("authentication failed".into()));
        }
        let message = canonical_challenge_message(
            challenge_id,
            challenge,
            &node_identifier,
            expires_at,
            version,
        );
        if !QuantumKeyPair::verify(public_key, &message, signature) {
            return Err(AuthError::Unauthorized("authentication failed".into()));
        }

        match self
            .storage
            .auth_challenges
            .compare_and_swap(challenge_id.as_bytes(), Some(stored), None::<Vec<u8>>)
            .map_err(|error| {
                AuthError::Storage(format!("failed to consume auth challenge: {error}"))
            })? {
            Ok(()) => {}
            Err(_) => {
                return Err(AuthError::Unauthorized(
                    "authentication challenge is invalid or already used".into(),
                ))
            }
        }

        let session_token = random_hex(32);
        let csrf_token = random_hex(32);
        let created_at = now_seconds();
        let session = SessionRecord {
            node_identifier: node_identifier.clone(),
            csrf_hash: hash_secret(&csrf_token),
            created_at,
            expires_at: created_at.saturating_add(self.config.session_ttl_seconds),
        };
        let encoded = bincode::serialize(&session).map_err(|error| {
            AuthError::Storage(format!("failed to encode auth session: {error}"))
        })?;
        self.storage
            .auth_sessions
            .insert(hash_secret(&session_token), encoded)
            .map_err(|error| {
                AuthError::Storage(format!("failed to persist auth session: {error}"))
            })?;
        self.storage.db.flush().map_err(|error| {
            AuthError::Storage(format!("failed to flush auth session: {error}"))
        })?;
        Ok(AuthSession {
            session_token,
            node_identifier,
            csrf_token,
            expires_at: session.expires_at,
        })
    }

    pub fn session(&self, session_token: &str) -> Result<Option<AuthSession>, AuthError> {
        if session_token.trim().is_empty() {
            return Ok(None);
        }
        let key = hash_secret(session_token);
        let Some(raw) =
            self.storage.auth_sessions.get(key).map_err(|error| {
                AuthError::Storage(format!("failed to read auth session: {error}"))
            })?
        else {
            return Ok(None);
        };
        let record: SessionRecord = bincode::deserialize(&raw).map_err(|error| {
            AuthError::Storage(format!("failed to decode auth session: {error}"))
        })?;
        if record.expires_at <= now_seconds() {
            self.storage.auth_sessions.remove(key).map_err(|error| {
                AuthError::Storage(format!("failed to remove expired auth session: {error}"))
            })?;
            return Ok(None);
        }
        Ok(Some(AuthSession {
            session_token: String::new(),
            node_identifier: record.node_identifier,
            csrf_token: String::new(),
            expires_at: record.expires_at,
        }))
    }

    pub fn validate_session_csrf(
        &self,
        session_token: &str,
        csrf_token: &str,
    ) -> Result<bool, AuthError> {
        let key = hash_secret(session_token);
        let Some(raw) =
            self.storage.auth_sessions.get(key).map_err(|error| {
                AuthError::Storage(format!("failed to read auth session: {error}"))
            })?
        else {
            return Ok(false);
        };
        let record: SessionRecord = bincode::deserialize(&raw).map_err(|error| {
            AuthError::Storage(format!("failed to decode auth session: {error}"))
        })?;
        if record.expires_at <= now_seconds() {
            return Ok(false);
        }
        Ok(hash_secret(csrf_token).ct_eq(&record.csrf_hash).into())
    }

    pub fn revoke_session(&self, session_token: &str) -> Result<(), AuthError> {
        if session_token.trim().is_empty() {
            return Ok(());
        }
        self.storage
            .auth_sessions
            .remove(hash_secret(session_token))
            .map_err(|error| {
                AuthError::Storage(format!("failed to revoke auth session: {error}"))
            })?;
        self.storage.db.flush().map_err(|error| {
            AuthError::Storage(format!("failed to flush auth session revocation: {error}"))
        })?;
        Ok(())
    }
}

pub fn canonical_login_message(
    challenge_id: &str,
    challenge: &str,
    node_identifier: &str,
    expires_at: u64,
    version: u32,
) -> Vec<u8> {
    canonical_challenge_message(
        challenge_id,
        challenge,
        node_identifier,
        expires_at,
        version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn node_identifiers_are_canonical_hex() {
        let identifier = "A".repeat(64);
        assert_eq!(
            validate_node_identifier(&identifier).unwrap(),
            "a".repeat(64)
        );
        assert!(validate_node_identifier("a".repeat(63).as_str()).is_err());
        assert!(validate_node_identifier(&format!("{}z", "a".repeat(63))).is_err());
    }

    #[test]
    fn cookie_domains_are_safe_hostnames() {
        assert_eq!(
            validate_cookie_domain(".UltraNetwork.cc").unwrap(),
            "ultranetwork.cc"
        );
        assert!(validate_cookie_domain("https://ultranetwork.cc").is_err());
        assert!(validate_cookie_domain("ultranetwork.cc/api").is_err());
        assert!(validate_cookie_domain("ultranet..cc").is_err());
        assert!(validate_cookie_domain("ultranet cc").is_err());
    }

    #[test]
    fn signed_login_creates_session_and_consumes_challenge() {
        let path = format!("test_db_auth_{}", std::process::id());
        let _ = fs::remove_dir_all(&path);
        let storage = Arc::new(Storage::new(&path).expect("storage should open"));
        let keypair = QuantumKeyPair::generate();
        let node_identifier = keypair.address();
        let service = AuthService::new(
            storage.clone(),
            AuthConfig::for_tests(node_identifier.clone()),
        );
        let challenge = service.issue_challenge(&node_identifier).unwrap();
        let message = canonical_login_message(
            &challenge.challenge_id,
            &challenge.challenge,
            &challenge.node_identifier,
            challenge.expires_at,
            challenge.version,
        );
        let signature = keypair.sign(&message);
        let session = service
            .login(
                &challenge.challenge_id,
                &challenge.challenge,
                &challenge.node_identifier,
                challenge.expires_at,
                &keypair.public_key,
                &signature,
                challenge.version,
            )
            .expect("valid wallet signature should authenticate");
        assert_eq!(session.node_identifier, node_identifier);
        assert!(service.session(&session.session_token).unwrap().is_some());
        assert!(service
            .validate_session_csrf(&session.session_token, &session.csrf_token)
            .unwrap());
        assert!(matches!(
            service.login(
                &challenge.challenge_id,
                &challenge.challenge,
                &challenge.node_identifier,
                challenge.expires_at,
                &keypair.public_key,
                &signature,
                challenge.version,
            ),
            Err(AuthError::Unauthorized(_))
        ));
        service.revoke_session(&session.session_token).unwrap();
        assert!(service.session(&session.session_token).unwrap().is_none());
        drop(storage);
        let _ = fs::remove_dir_all(&path);
    }
}
