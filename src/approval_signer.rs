use crate::governance::approval::{
    canonical_approval_message, owner_address, verify_partial_signature, ApprovalDraft,
    COMBINED_SIGNATURE_BYTES, PARTIAL_SIGNATURE_BYTES,
};
use crate::quantum_crypto::QuantumKeyPair;
use async_trait::async_trait;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use zeroize::Zeroize;

pub const DEFAULT_SIGNER_TIMEOUT_SECONDS: u64 = 20;
pub const DEFAULT_APPROVAL_INTENT_TTL_SECONDS: u64 = 10 * 60;
const MIN_SIGNER_RESPONSE_BYTES: usize = PARTIAL_SIGNATURE_BYTES;
const MAX_SIGNER_FRAME_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnerAuthBinding {
    pub owner_index: usize,
    pub session_node_identifier: String,
    pub signer_id: String,
    pub signer_socket: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ApprovalGatewayConfig {
    pub enabled: bool,
    pub validator_review_identifiers: HashSet<String>,
    pub owner_auth_bindings: Vec<OwnerAuthBinding>,
    pub signer_timeout: Duration,
    pub intent_ttl_seconds: u64,
}

impl ApprovalGatewayConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            validator_review_identifiers: HashSet::new(),
            owner_auth_bindings: Vec::new(),
            signer_timeout: Duration::from_secs(DEFAULT_SIGNER_TIMEOUT_SECONDS),
            intent_ttl_seconds: DEFAULT_APPROVAL_INTENT_TTL_SECONDS,
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let enabled = parse_bool_env("ULTRANET_WEB_APPROVAL_ENABLED", false)?;
        let timeout_seconds = parse_bounded_u64(
            "ULTRANET_APPROVAL_SIGNER_TIMEOUT_SECONDS",
            DEFAULT_SIGNER_TIMEOUT_SECONDS,
            1,
            120,
        )?;
        let intent_ttl_seconds = parse_bounded_u64(
            "ULTRANET_APPROVAL_INTENT_TTL_SECONDS",
            DEFAULT_APPROVAL_INTENT_TTL_SECONDS,
            60,
            3_600,
        )?;

        if !enabled {
            return Ok(Self {
                signer_timeout: Duration::from_secs(timeout_seconds),
                intent_ttl_seconds,
                ..Self::disabled()
            });
        }

        let validator_review_identifiers =
            parse_identifier_list("ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS", true)?;
        if validator_review_identifiers.is_empty() {
            return Err(
                "ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS must contain at least one identifier when web approval is enabled".into(),
            );
        }

        let owner_auth_path = required_path_env("ULTRANET_SOVEREIGN_OWNER_AUTH_FILE")?;
        let owner_auth_bindings = load_owner_auth_bindings(&owner_auth_path)?;

        Ok(Self {
            enabled,
            validator_review_identifiers,
            owner_auth_bindings,
            signer_timeout: Duration::from_secs(timeout_seconds),
            intent_ttl_seconds,
        })
    }

    pub fn validate_against_owners(&self, owners: &[Vec<u8>]) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if owners.len() != 3 {
            return Err(format!(
                "web Sovereign approval requires exactly 3 configured owner keys; received {}",
                owners.len()
            ));
        }
        let mut owner_indexes = HashSet::new();
        let mut session_identifiers = HashSet::new();
        let mut signer_ids = HashSet::new();
        let mut signer_sockets = HashSet::new();
        let mut public_keys = HashSet::new();
        for (index, public_key) in owners.iter().enumerate() {
            if public_key.len() != 2_592 || !public_keys.insert(public_key) {
                return Err("configured Sovereign owner keys are invalid or duplicated".into());
            }
            let binding = self
                .owner_auth_bindings
                .iter()
                .find(|binding| binding.owner_index == index)
                .ok_or_else(|| {
                    format!("missing web approval binding for Sovereign owner {index}")
                })?;
            let session_identifier =
                crate::auth::validate_node_identifier(&binding.session_node_identifier)?;
            if !self
                .validator_review_identifiers
                .contains(&session_identifier)
            {
                return Err(format!(
                    "Sovereign owner session {session_identifier} is not in ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS"
                ));
            }
            if !owner_indexes.insert(binding.owner_index)
                || !session_identifiers.insert(session_identifier)
                || binding.signer_id.trim().is_empty()
                || !signer_ids.insert(binding.signer_id.clone())
                || binding.signer_socket.as_os_str().is_empty()
                || !signer_sockets.insert(binding.signer_socket.clone())
            {
                return Err(
                    "web approval owner bindings contain duplicate or empty identities".into(),
                );
            }
        }
        if self.owner_auth_bindings.len() != owners.len() {
            return Err(
                "web approval owner bindings must cover exactly the configured owner set".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignerRequest {
    pub intent_id: String,
    pub owner_index: usize,
    pub signer_id: String,
    pub draft: ApprovalDraft,
    pub digest: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignerResponse {
    pub intent_id: String,
    pub owner_address: String,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

#[async_trait]
pub trait ApprovalSigner: Send + Sync {
    async fn sign(&self, request: SignerRequest) -> Result<SignerResponse, String>;
}

#[derive(Debug, Default)]
pub struct DisabledApprovalSigner;

#[async_trait]
impl ApprovalSigner for DisabledApprovalSigner {
    async fn sign(&self, _request: SignerRequest) -> Result<SignerResponse, String> {
        Err("isolated Sovereign approval signer is unavailable".into())
    }
}

#[derive(Clone)]
pub struct ApprovalGateway {
    pub config: ApprovalGatewayConfig,
    signers: Arc<HashMap<usize, Arc<dyn ApprovalSigner>>>,
}

impl ApprovalGateway {
    pub fn from_env() -> Result<Self, String> {
        let config = ApprovalGatewayConfig::from_env()?;
        if !config.enabled {
            return Ok(Self {
                config,
                signers: Arc::new(HashMap::new()),
            });
        }

        #[cfg(unix)]
        {
            let mut signers = HashMap::new();
            for binding in &config.owner_auth_bindings {
                let signer_socket = binding.signer_socket.clone();
                signers.insert(
                    binding.owner_index,
                    Arc::new(UnixSocketApprovalSigner {
                        socket: signer_socket,
                        timeout: config.signer_timeout,
                    }) as Arc<dyn ApprovalSigner>,
                );
            }
            return Ok(Self {
                config,
                signers: Arc::new(signers),
            });
        }

        #[cfg(not(unix))]
        {
            let _ = config;
            Err("web Sovereign approval requires Unix-domain signer sockets on this host".into())
        }
    }

    pub fn for_tests(
        validator_review_identifiers: HashSet<String>,
        owner_auth_bindings: Vec<OwnerAuthBinding>,
        signer: Arc<dyn ApprovalSigner>,
    ) -> Self {
        let signers = owner_auth_bindings
            .iter()
            .map(|binding| (binding.owner_index, signer.clone()))
            .collect();
        Self {
            config: ApprovalGatewayConfig {
                enabled: true,
                validator_review_identifiers,
                owner_auth_bindings,
                signer_timeout: Duration::from_secs(DEFAULT_SIGNER_TIMEOUT_SECONDS),
                intent_ttl_seconds: DEFAULT_APPROVAL_INTENT_TTL_SECONDS,
            },
            signers: Arc::new(signers),
        }
    }

    pub fn is_validator_reviewer(&self, session_node_identifier: &str) -> bool {
        self.config
            .validator_review_identifiers
            .contains(session_node_identifier)
    }

    pub fn owner_binding(&self, session_node_identifier: &str) -> Option<OwnerAuthBinding> {
        self.config
            .owner_auth_bindings
            .iter()
            .find(|binding| binding.session_node_identifier == session_node_identifier)
            .cloned()
    }

    pub fn capabilities(&self, session_node_identifier: &str) -> Vec<&'static str> {
        if !self.config.enabled || !self.is_validator_reviewer(session_node_identifier) {
            return Vec::new();
        }
        let mut capabilities = vec!["validator_review"];
        if self.owner_binding(session_node_identifier).is_some() {
            capabilities.push("sovereign_approve");
        }
        capabilities
    }

    pub fn signer(&self, owner_index: usize) -> Option<Arc<dyn ApprovalSigner>> {
        self.signers.get(&owner_index).cloned()
    }
}

#[cfg(unix)]
struct UnixSocketApprovalSigner {
    socket: PathBuf,
    timeout: Duration,
}

#[cfg(unix)]
#[async_trait]
impl ApprovalSigner for UnixSocketApprovalSigner {
    async fn sign(&self, request: SignerRequest) -> Result<SignerResponse, String> {
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixStream,
            time::timeout,
        };

        let operation = async {
            let mut stream = UnixStream::connect(&self.socket)
                .await
                .map_err(|error| format!("cannot connect to isolated approval signer: {error}"))?;
            let mut encoded = serde_json::to_vec(&request)
                .map_err(|error| format!("cannot encode signer request: {error}"))?;
            encoded.push(b'\n');
            stream
                .write_all(&encoded)
                .await
                .map_err(|error| format!("cannot send signer request: {error}"))?;
            stream
                .shutdown()
                .await
                .map_err(|error| format!("cannot finish signer request: {error}"))?;

            let mut reader = BufReader::new(stream);
            let mut response = Vec::new();
            reader
                .read_until(b'\n', &mut response)
                .await
                .map_err(|error| format!("cannot read isolated signer response: {error}"))?;
            if response.len() > MAX_SIGNER_FRAME_BYTES {
                return Err("isolated signer response is too large".into());
            }
            serde_json::from_slice::<SignerResponse>(&response)
                .map_err(|error| format!("isolated signer returned invalid response: {error}"))
        };

        timeout(self.timeout, operation)
            .await
            .map_err(|_| "isolated approval signer timed out".to_string())?
    }
}

#[derive(Clone)]
struct CachedSigningOperation {
    draft: ApprovalDraft,
    digest: Vec<u8>,
    response: SignerResponse,
}

pub struct FileApprovalSigner {
    keypair: QuantumKeyPair,
    owner_index: usize,
    signer_id: String,
    require_local_confirmation: bool,
    signing_lock: parking_lot::Mutex<()>,
    cached_operations: parking_lot::Mutex<HashMap<String, CachedSigningOperation>>,
}

impl FileApprovalSigner {
    pub fn from_key_file(
        path: &Path,
        key_record_index: usize,
        owner_index: usize,
        signer_id: String,
        require_local_confirmation: bool,
    ) -> Result<Self, String> {
        ensure_private_file_permissions(path)?;
        let raw = fs::read_to_string(path)
            .map_err(|error| format!("cannot read signer key file {}: {error}", path.display()))?;
        let records = parse_key_records(&raw)?;
        let record = records.into_iter().nth(key_record_index).ok_or_else(|| {
            format!("key record index {key_record_index} does not exist in signer key file")
        })?;
        let (declared_address, public_key, secret_key) = record.into_parts()?;
        let derived_address = owner_address(&public_key);
        if let Some(address) = declared_address {
            let normalized = crate::auth::validate_node_identifier(&address)?;
            if normalized != derived_address {
                return Err("signer key address does not match its public key".into());
            }
        }
        let keypair = QuantumKeyPair {
            public_key,
            secret_key,
            key_id: [0; 32],
            created_at: 0,
            version: 1,
        };
        let probe = keypair.sign(b"ULTRANET_APPROVAL_SIGNER_KEYPAIR_CHECK_V1");
        if !QuantumKeyPair::verify(
            &keypair.public_key,
            b"ULTRANET_APPROVAL_SIGNER_KEYPAIR_CHECK_V1",
            &probe,
        ) {
            return Err("signer public/private keypair self-check failed".into());
        }
        Ok(Self {
            keypair,
            owner_index,
            signer_id,
            require_local_confirmation,
            signing_lock: parking_lot::Mutex::new(()),
            cached_operations: parking_lot::Mutex::new(HashMap::new()),
        })
    }

    fn confirm_locally(&self, request: &SignerRequest) -> Result<(), String> {
        if !self.require_local_confirmation {
            return Ok(());
        }
        eprintln!(
            "Sovereign approval request for proposal {} (owner index {}). Type APPROVE to sign.",
            request.draft.proposal_hash, request.owner_index
        );
        eprint!("approval-signer> ");
        io::stderr()
            .flush()
            .map_err(|error| format!("cannot flush signer prompt: {error}"))?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|error| format!("cannot read local signer confirmation: {error}"))?;
        if answer.trim() != "APPROVE" {
            return Err("local Sovereign owner confirmation was not provided".into());
        }
        Ok(())
    }
}

impl FileApprovalSigner {
    fn sign_request(&self, request: SignerRequest) -> Result<SignerResponse, String> {
        if request.owner_index != self.owner_index || request.signer_id != self.signer_id {
            return Err("signer request does not match this isolated owner process".into());
        }
        let expected_digest = canonical_approval_message(&request.draft)?;
        if request.digest != expected_digest {
            return Err("signer request digest does not match the canonical approval draft".into());
        }
        if request.intent_id.trim().is_empty() {
            return Err("signer request intent_id is required".into());
        }

        // The cache lookup, local-presence check, signature creation, and cache
        // insertion form one idempotent operation. Without this guard, two
        // concurrent retries could both sign the same intent before either
        // response is cached.
        let _signing_guard = self.signing_lock.lock();
        if let Some(cached) = self.cached_operations.lock().get(&request.intent_id) {
            if cached.draft != request.draft || cached.digest != request.digest {
                return Err("signer intent_id was reused with different approval fields".into());
            }
            return Ok(cached.response.clone());
        }

        self.confirm_locally(&request)?;
        let signature = self.keypair.sign(&expected_digest);
        verify_partial_signature(&self.keypair.public_key, &signature, &request.draft)?;
        if signature.len() != MIN_SIGNER_RESPONSE_BYTES {
            return Err(format!(
                "isolated signer produced {} bytes; expected {PARTIAL_SIGNATURE_BYTES}",
                signature.len()
            ));
        }
        let response = SignerResponse {
            intent_id: request.intent_id.clone(),
            owner_address: owner_address(&self.keypair.public_key),
            public_key: self.keypair.public_key.clone(),
            signature,
        };
        self.cached_operations.lock().insert(
            request.intent_id,
            CachedSigningOperation {
                draft: request.draft,
                digest: expected_digest,
                response: response.clone(),
            },
        );
        Ok(response)
    }
}

#[async_trait]
impl ApprovalSigner for FileApprovalSigner {
    async fn sign(&self, request: SignerRequest) -> Result<SignerResponse, String> {
        self.sign_request(request)
    }
}

impl Drop for FileApprovalSigner {
    fn drop(&mut self) {
        self.keypair.zeroize();
    }
}

#[derive(Debug, Deserialize)]
struct KeyRecord {
    #[serde(default)]
    address: Option<String>,
    public_key: EncodedKeyBytes,
    #[serde(alias = "private_key")]
    secret_key: EncodedKeyBytes,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EncodedKeyBytes {
    Hex(String),
    Bytes(Vec<u8>),
}

impl EncodedKeyBytes {
    fn into_bytes(self, field: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Hex(value) => {
                let mut value = value;
                let decoded = hex::decode(value.trim_start_matches("0x").trim_start_matches("0X"))
                    .map_err(|error| format!("{field} is not valid hex: {error}"));
                value.zeroize();
                decoded
            }
            Self::Bytes(value) => Ok(value),
        }
    }
}

impl Zeroize for EncodedKeyBytes {
    fn zeroize(&mut self) {
        match self {
            Self::Hex(value) => value.zeroize(),
            Self::Bytes(value) => value.zeroize(),
        }
    }
}

struct OwnedKeyRecord {
    address: Option<String>,
    public_key: EncodedKeyBytes,
    secret_key: EncodedKeyBytes,
}

impl OwnedKeyRecord {
    fn into_parts(mut self) -> Result<(Option<String>, Vec<u8>, Vec<u8>), String> {
        let address = self.address.take();
        let public_key =
            std::mem::replace(&mut self.public_key, EncodedKeyBytes::Bytes(Vec::new()))
                .into_bytes("public_key")?;
        let secret_key =
            std::mem::replace(&mut self.secret_key, EncodedKeyBytes::Bytes(Vec::new()))
                .into_bytes("secret_key/private_key")?;
        Ok((address, public_key, secret_key))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KeyFile {
    Records(Vec<KeyRecord>),
    Owners { owners: Vec<KeyRecord> },
}

fn parse_key_records(raw: &str) -> Result<Vec<OwnedKeyRecord>, String> {
    let file = serde_json::from_str::<KeyFile>(raw)
        .map_err(|error| format!("invalid signer key JSON: {error}"))?;
    let records = match file {
        KeyFile::Records(records) => records,
        KeyFile::Owners { owners } => owners,
    };
    records
        .into_iter()
        .map(|record| {
            Ok(OwnedKeyRecord {
                address: record.address,
                public_key: record.public_key,
                secret_key: record.secret_key,
            })
        })
        .collect()
}

fn ensure_private_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("cannot inspect signer key file: {error}"))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "signer key file {} is group/other accessible; run chmod 600",
                path.display()
            ));
        }
    }
    Ok(())
}

fn ensure_owner_auth_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("cannot inspect owner auth file: {error}"))?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o600 && mode != 0o640 {
            return Err(format!(
                "owner auth file {} must have mode 0600 or 0640",
                path.display()
            ));
        }
    }
    Ok(())
}

fn parse_bool_env(name: &str, default: bool) -> Result<bool, String> {
    match env::var(name) {
        Ok(value) => match value.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("{name} must be true or false")),
        },
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn parse_bounded_u64(name: &str, default: u64, minimum: u64, maximum: u64) -> Result<u64, String> {
    let value = match env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| format!("{name} must be an integer"))?,
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => return Err(format!("{name} must be valid UTF-8")),
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{name} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn required_path_env(name: &str) -> Result<PathBuf, String> {
    let value =
        env::var(name).map_err(|_| format!("{name} is required when web approval is enabled"))?;
    let path = PathBuf::from(value.trim());
    if path.as_os_str().is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    Ok(path)
}

fn parse_identifier_list(name: &str, allow_missing: bool) -> Result<HashSet<String>, String> {
    match env::var(name) {
        Ok(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(crate::auth::validate_node_identifier)
            .collect(),
        Err(env::VarError::NotPresent) if allow_missing => Ok(HashSet::new()),
        Err(env::VarError::NotPresent) => Err(format!("{name} is required")),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn load_owner_auth_bindings(path: &Path) -> Result<Vec<OwnerAuthBinding>, String> {
    ensure_owner_auth_permissions(path)?;
    let raw = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read Sovereign owner auth file {}: {error}",
            path.display()
        )
    })?;
    let mut bindings = serde_json::from_str::<Vec<OwnerAuthBinding>>(&raw)
        .map_err(|error| format!("invalid Sovereign owner auth file: {error}"))?;
    if bindings.is_empty() {
        return Err("Sovereign owner auth file must contain owner bindings".into());
    }
    for binding in &mut bindings {
        binding.session_node_identifier =
            crate::auth::validate_node_identifier(&binding.session_node_identifier)?;
        binding.signer_id = binding.signer_id.trim().to_string();
        if binding.signer_id.is_empty() {
            return Err("Sovereign owner signer_id must not be empty".into());
        }
        if !binding.signer_socket.is_absolute() {
            return Err("Sovereign owner signer_socket must be an absolute path".into());
        }
    }
    Ok(bindings)
}

pub fn random_operation_id() -> String {
    let mut bytes = [0u8; 24];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn validate_signer_response(
    request: &SignerRequest,
    response: &SignerResponse,
    expected_owner_index: usize,
    configured_owners: &[Vec<u8>],
) -> Result<(), String> {
    if response.intent_id != request.intent_id {
        return Err("isolated signer returned a mismatched intent_id".into());
    }
    let owner_index = crate::governance::approval::find_authorized_owner_index(
        &response.public_key,
        configured_owners,
    )?;
    if owner_index != expected_owner_index {
        return Err("isolated signer returned a different authorized owner".into());
    }
    if response.owner_address != owner_address(&response.public_key) {
        return Err("isolated signer owner address does not match public key".into());
    }
    verify_partial_signature(&response.public_key, &response.signature, &request.draft)
}

pub fn signer_protocol_constants() -> (usize, usize) {
    (PARTIAL_SIGNATURE_BYTES, COMBINED_SIGNATURE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{governance::approval::ApprovalDraft, UltraBlockchain};

    #[test]
    fn disabled_gateway_has_no_capabilities() {
        let gateway = ApprovalGateway {
            config: ApprovalGatewayConfig::disabled(),
            signers: Arc::new(HashMap::new()),
        };
        assert!(gateway.capabilities("a".repeat(64).as_str()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn owner_auth_permissions_allow_private_or_group_readable_mapping_only() {
        use std::os::unix::fs::PermissionsExt;

        let directory = temporary_signer_directory("owner-auth-permissions");
        std::fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("sovereign-owner-auth.json");
        std::fs::write(&path, b"[]").expect("test mapping should be written");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("private mapping mode should be set");
        assert!(ensure_owner_auth_permissions(&path).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("group-readable mapping mode should be set");
        assert!(ensure_owner_auth_permissions(&path).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("world-readable mapping mode should be set");
        assert!(ensure_owner_auth_permissions(&path).is_err());

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn signer_protocol_requires_exact_owner_binding() {
        let config = ApprovalGatewayConfig {
            enabled: true,
            validator_review_identifiers: ["a".repeat(64)].into_iter().collect(),
            owner_auth_bindings: vec![OwnerAuthBinding {
                owner_index: 0,
                session_node_identifier: "a".repeat(64),
                signer_id: "owner-0".into(),
                signer_socket: PathBuf::from("/run/owner-0.sock"),
            }],
            signer_timeout: Duration::from_secs(1),
            intent_ttl_seconds: 60,
        };
        assert_eq!(config.owner_auth_bindings[0].signer_id, "owner-0");
        assert!(config
            .validator_review_identifiers
            .contains(&"a".repeat(64)));
    }

    #[test]
    fn signer_draft_digest_has_no_signature_input() {
        let draft = ApprovalDraft {
            proposal_hash: "11".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 0,
            nullifier: vec![0x22; 32],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let digest = canonical_approval_message(&draft).unwrap();
        assert_eq!(digest.len(), 32);
    }

    fn temporary_signer_directory(label: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ultranet-approval-signer-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_test_signer_key_file(directory: &std::path::Path, owner: &QuantumKeyPair) -> PathBuf {
        let path = directory.join("signer-key.json");
        let contents = serde_json::json!([{
            "address": owner.address(),
            "public_key": hex::encode(&owner.public_key),
            "secret_key": hex::encode(&owner.secret_key),
        }]);
        std::fs::write(&path, serde_json::to_vec(&contents).unwrap())
            .expect("test signer key file should be written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("test signer key file should be private");
        }
        path
    }

    fn signer_test_request(
        intent_id: &str,
        draft: ApprovalDraft,
        digest: Vec<u8>,
    ) -> SignerRequest {
        SignerRequest {
            intent_id: intent_id.to_string(),
            owner_index: 0,
            signer_id: "test-signer".to_string(),
            draft,
            digest,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signer_same_intent_is_idempotent_under_concurrent_retries() {
        let directory = temporary_signer_directory("same-intent");
        std::fs::create_dir_all(&directory).expect("test signer directory should be created");
        let owner = QuantumKeyPair::generate();
        let key_path = write_test_signer_key_file(&directory, &owner);
        let signer = Arc::new(
            FileApprovalSigner::from_key_file(&key_path, 0, 0, "test-signer".into(), false)
                .expect("test signer should load"),
        );
        let draft = ApprovalDraft {
            proposal_hash: "11".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 7,
            nullifier: vec![0x22; 32],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let digest = canonical_approval_message(&draft).expect("test draft should hash");
        let barrier = Arc::new(tokio::sync::Barrier::new(8));
        let mut tasks = Vec::new();

        for _ in 0..8 {
            let signer = signer.clone();
            let barrier = barrier.clone();
            let request = signer_test_request("same-operation", draft.clone(), digest.clone());
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                signer.sign(request).await
            }));
        }

        let mut responses = Vec::new();
        for task in tasks {
            responses.push(task.await.expect("signer task should not panic").expect(
                "concurrent retries for one valid operation should all return the cached result",
            ));
        }
        let first = responses
            .first()
            .expect("at least one signer response")
            .clone();
        for response in &responses {
            assert_eq!(response, &first);
        }
        assert_eq!(first.intent_id, "same-operation");
        assert_eq!(first.owner_address, owner.address());
        assert_eq!(first.public_key, owner.public_key);
        verify_partial_signature(&first.public_key, &first.signature, &draft)
            .expect("cached signer response should remain cryptographically valid");

        let mut changed_draft = draft.clone();
        changed_draft.nonce += 1;
        let changed_digest = canonical_approval_message(&changed_draft).unwrap();
        let error = signer
            .sign(signer_test_request(
                "same-operation",
                changed_draft,
                changed_digest,
            ))
            .await
            .expect_err("an intent ID must not be reused for different approval fields");
        assert!(error.contains("different approval fields"));

        drop(signer);
        drop(owner);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signer_distinct_concurrent_intents_do_not_cross_contaminate() {
        let directory = temporary_signer_directory("distinct-intents");
        std::fs::create_dir_all(&directory).expect("test signer directory should be created");
        let owner = QuantumKeyPair::generate();
        let key_path = write_test_signer_key_file(&directory, &owner);
        let signer = Arc::new(
            FileApprovalSigner::from_key_file(&key_path, 0, 0, "test-signer".into(), false)
                .expect("test signer should load"),
        );
        let draft = ApprovalDraft {
            proposal_hash: "33".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 9,
            nullifier: vec![0x44; 32],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        };
        let digest = canonical_approval_message(&draft).expect("test draft should hash");
        let barrier = Arc::new(tokio::sync::Barrier::new(4));
        let mut tasks = Vec::new();

        for index in 0..4 {
            let signer = signer.clone();
            let barrier = barrier.clone();
            let intent_id = format!("distinct-operation-{index}");
            let request = signer_test_request(&intent_id, draft.clone(), digest.clone());
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                signer.sign(request).await
            }));
        }

        let mut responses = Vec::new();
        for task in tasks {
            responses.push(
                task.await
                    .expect("signer task should not panic")
                    .expect("distinct valid operations should all complete successfully"),
            );
        }
        let mut intent_ids = responses
            .iter()
            .map(|response| response.intent_id.clone())
            .collect::<Vec<_>>();
        intent_ids.sort();
        assert_eq!(
            intent_ids,
            vec![
                "distinct-operation-0",
                "distinct-operation-1",
                "distinct-operation-2",
                "distinct-operation-3",
            ]
        );
        for response in &responses {
            assert_eq!(response.owner_address, owner.address());
            assert_eq!(response.public_key, owner.public_key);
            verify_partial_signature(&response.public_key, &response.signature, &draft)
                .expect("each distinct response should verify against its own draft");
        }

        drop(signer);
        drop(owner);
        let _ = std::fs::remove_dir_all(directory);
    }
}
