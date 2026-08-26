use clap::{Args, Parser, Subcommand};
use rand::{rngs::OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};
use UltraNet::{
    auth::validate_node_identifier,
    quantum_crypto::{PKTrait, SKTrait},
    QuantumKeyPair, UltraBlockchain,
};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8081";
const PROPOSAL_RECIPIENT: &str = "0x0";
const APPROVAL_AMOUNT: u64 = 0;
const APPROVAL_FEE: u64 = 0;
const APPROVAL_GAS_LIMIT: u64 = 1_000_000;
const APPROVAL_GAS_PRICE: u64 = 1;
const SIGNATURE_BYTES: usize = 4_627;
const COMBINED_SIGNATURE_BYTES: usize = SIGNATURE_BYTES * 2;
const NULLIFIER_BYTES: usize = 32;
const SOVEREIGN_OWNER_COUNT: usize = 3;
const LOCAL_KEYPAIR_PROBE: &[u8] = b"ULTRANET_APPROVAL_LOCAL_KEYPAIR_CHECK_V1";

#[derive(Debug, Parser)]
#[command(
    name = "ultranet-approve",
    version,
    about = "Prepare, sign, combine, and submit an UltraNet validator approval offline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List public owner addresses and public keys without printing private keys.
    Owners(OwnersArgs),
    /// Verify a pending proposal, fetch the current nonce, and create a signing draft.
    Prepare(PrepareArgs),
    /// Sign one prepared draft with one local Sovereign owner key.
    Sign(SignArgs),
    /// Verify two owner signatures and create the final approval JSON payload.
    Combine(CombineArgs),
    /// Submit a combined public approval payload to the node.
    Submit(SubmitArgs),
}

#[derive(Debug, Args)]
struct KeyFileArgs {
    /// Local Sovereign key file. Never copy this file to the node host.
    #[arg(long, default_value = "sovereign_keys.json")]
    keys: PathBuf,
}

#[derive(Debug, Args)]
struct OutputArgs {
    /// Create a new output file instead of writing JSON to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Pretty-print JSON output.
    #[arg(long)]
    pretty: bool,
}

#[derive(Debug, Args)]
struct OwnersArgs {
    #[command(flatten)]
    key_file: KeyFileArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct PrepareArgs {
    /// Public API origin. Can also be set with ULTRANET_API_BASE_URL.
    #[arg(long)]
    api_base_url: Option<String>,

    /// 64-hex-character hash returned by GET /api/governance/proposals.
    #[arg(long)]
    proposal_hash: String,

    /// Optional manual nonce. If omitted, fetch the Sovereign account's next nonce.
    #[arg(long)]
    nonce: Option<u64>,

    /// Prepare without contacting the API; requires --nonce and independent proposal verification.
    #[arg(long)]
    offline: bool,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct SignArgs {
    /// Prepared public approval draft JSON.
    #[arg(long)]
    request: PathBuf,

    /// Zero-based owner record index in the local key file.
    #[arg(long)]
    owner_index: usize,

    #[command(flatten)]
    key_file: KeyFileArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct CombineArgs {
    /// Prepared public approval draft JSON.
    #[arg(long)]
    request: PathBuf,

    /// Two signed approval artifact files. Repeat this option once per owner.
    #[arg(long = "signature", value_name = "FILE", num_args = 1)]
    signatures: Vec<PathBuf>,

    /// Public owner manifest created by the owners command; contains no private keys.
    #[arg(long = "authorized-owners")]
    authorized_owners: PathBuf,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct SubmitArgs {
    /// Combined approval JSON created by the combine command.
    #[arg(long)]
    request: PathBuf,

    /// Public API origin. Can also be set with ULTRANET_API_BASE_URL.
    #[arg(long)]
    api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ApprovalDraft {
    proposal_hash: String,
    timestamp: u64,
    nonce: u64,
    nullifier: Vec<u8>,
    version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalPayload {
    #[serde(flatten)]
    draft: ApprovalDraft,
    signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedApprovalArtifact {
    #[serde(flatten)]
    draft: ApprovalDraft,
    owner_address: String,
    public_key: String,
    signature: String,
}

#[derive(Deserialize)]
struct KeyRecord {
    #[serde(default)]
    address: Option<String>,
    public_key: EncodedKeyBytes,
    #[serde(alias = "private_key")]
    secret_key: EncodedKeyBytes,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EncodedKeyBytes {
    Hex(String),
    Bytes(Vec<u8>),
}

impl Zeroize for EncodedKeyBytes {
    fn zeroize(&mut self) {
        match self {
            Self::Hex(value) => value.zeroize(),
            Self::Bytes(value) => value.zeroize(),
        }
    }
}

impl Zeroize for KeyRecord {
    fn zeroize(&mut self) {
        if let Some(address) = self.address.as_mut() {
            address.zeroize();
        }
        self.public_key.zeroize();
        self.secret_key.zeroize();
    }
}

impl Drop for KeyRecord {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl KeyRecord {
    fn into_parts(mut self) -> (Option<String>, EncodedKeyBytes, EncodedKeyBytes) {
        let address = self.address.take();
        let public_key =
            std::mem::replace(&mut self.public_key, EncodedKeyBytes::Bytes(Vec::new()));
        let secret_key =
            std::mem::replace(&mut self.secret_key, EncodedKeyBytes::Bytes(Vec::new()));
        (address, public_key, secret_key)
    }
}

impl EncodedKeyBytes {
    fn into_bytes(self, field: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Hex(mut value) => {
                let decoded = hex::decode(value.trim_start_matches("0x").trim_start_matches("0X"))
                    .map_err(|error| format!("{field} is not valid hex: {error}"));
                value.zeroize();
                decoded
            }
            Self::Bytes(value) => Ok(value),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum KeyFile {
    Records(Vec<KeyRecord>),
    Owners { owners: Vec<KeyRecord> },
}

impl KeyFile {
    fn records(self) -> Vec<KeyRecord> {
        match self {
            Self::Records(records) => records,
            Self::Owners { owners } => owners,
        }
    }
}

struct LoadedOwner {
    index: usize,
    address: String,
    keypair: QuantumKeyPair,
}

impl Drop for LoadedOwner {
    fn drop(&mut self) {
        self.keypair.zeroize();
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct OwnerSummary {
    index: usize,
    address: String,
    public_key: String,
}

#[derive(Debug, Deserialize)]
struct PendingProposal {
    hash: String,
    public_key: String,
    metadata: String,
}

#[derive(Debug, Deserialize)]
struct ProposalsResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    proposals: Vec<PendingProposal>,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct AccountView {
    nonce: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

fn api_base_url(explicit: Option<String>) -> String {
    explicit
        .or_else(|| env::var("ULTRANET_API_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn ensure_private_key_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("cannot inspect key file {}: {error}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "key file {} is accessible by group or other users; run chmod 600 before continuing",
                path.display()
            ));
        }
    }
    Ok(())
}

fn load_key_records(path: &Path) -> Result<Vec<KeyRecord>, String> {
    ensure_private_key_permissions(path)?;
    let raw = Zeroizing::new(
        fs::read_to_string(path)
            .map_err(|error| format!("cannot read key file {}: {error}", path.display()))?,
    );
    let key_file = serde_json::from_str::<KeyFile>(&raw)
        .map_err(|error| format!("invalid key file {}: {error}", path.display()))?;
    let records = key_file.records();
    if records.is_empty() {
        return Err(format!(
            "key file {} contains no owner records",
            path.display()
        ));
    }
    Ok(records)
}

fn load_owners(path: &Path) -> Result<Vec<LoadedOwner>, String> {
    load_key_records(path)?
        .into_iter()
        .enumerate()
        .map(|(index, record)| load_owner(index, record))
        .collect()
}

fn load_owner_at(path: &Path, owner_index: usize) -> Result<LoadedOwner, String> {
    let records = load_key_records(path)?;
    let record = records
        .into_iter()
        .nth(owner_index)
        .ok_or_else(|| format!("owner index {owner_index} does not exist"))?;
    load_owner(owner_index, record)
}

fn load_owner(index: usize, record: KeyRecord) -> Result<LoadedOwner, String> {
    let (declared_address, encoded_public_key, encoded_secret_key) = record.into_parts();
    let public_key = encoded_public_key.into_bytes("public_key")?;
    let secret_key = Zeroizing::new(encoded_secret_key.into_bytes("secret_key/private_key")?);

    UltraNet::quantum_crypto::PublicKey::from_bytes(&public_key)
        .map_err(|_| format!("owner {index} public_key is not a valid Dilithium-5 public key"))?;
    UltraNet::quantum_crypto::SecretKey::from_bytes(&secret_key)
        .map_err(|_| format!("owner {index} secret_key is not a valid Dilithium-5 secret key"))?;

    let derived_address = QuantumKeyPair::address_from_public_key(&public_key);
    if let Some(address) = declared_address {
        let declared_address = validate_node_identifier(&address)
            .map_err(|error| format!("owner {index} address is invalid: {error}"))?;
        if declared_address != derived_address {
            return Err(format!(
                "owner {index} address does not match its public_key"
            ));
        }
    }

    let keypair = QuantumKeyPair {
        public_key,
        secret_key: secret_key.to_vec(),
        key_id: [0; 32],
        created_at: 0,
        version: 1,
    };
    let mut probe_signature = keypair.sign(LOCAL_KEYPAIR_PROBE);
    let keypair_matches =
        QuantumKeyPair::verify(&keypair.public_key, LOCAL_KEYPAIR_PROBE, &probe_signature);
    probe_signature.zeroize();
    if !keypair_matches {
        return Err(format!(
            "owner {index} public_key and secret_key do not belong to the same Dilithium-5 keypair"
        ));
    }

    Ok(LoadedOwner {
        index,
        address: derived_address,
        keypair,
    })
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("invalid JSON file {}: {error}", path.display()))
}

fn create_private_output(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn write_json<T: Serialize>(value: &T, output: Option<&Path>, pretty: bool) -> Result<(), String> {
    let mut bytes = Vec::new();
    if pretty {
        serde_json::to_writer_pretty(&mut bytes, value)
            .map_err(|error| format!("cannot encode JSON: {error}"))?;
    } else {
        serde_json::to_writer(&mut bytes, value)
            .map_err(|error| format!("cannot encode JSON: {error}"))?;
    }
    bytes.push(b'\n');

    if let Some(path) = output {
        let mut file = create_private_output(path)
            .map_err(|error| format!("cannot create output {}: {error}", path.display()))?;
        file.write_all(&bytes)
            .and_then(|_| file.flush())
            .map_err(|error| format!("cannot write output {}: {error}", path.display()))?;
        eprintln!("Wrote JSON output to {}", path.display());
    } else {
        io::stdout()
            .write_all(&bytes)
            .map_err(|error| format!("cannot write JSON to stdout: {error}"))?;
    }
    Ok(())
}

fn parse_proposal_hash(value: &str) -> Result<[u8; 32], String> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let bytes = hex::decode(value)
        .map_err(|_| "proposal_hash must contain only hexadecimal characters".to_string())?;
    if bytes.len() != 32 {
        return Err("proposal_hash must be exactly 32 bytes (64 hexadecimal characters)".into());
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

fn normalize_proposal_hash(value: &str) -> Result<String, String> {
    Ok(hex::encode(parse_proposal_hash(value)?))
}

fn parse_nullifier(value: &[u8]) -> Result<[u8; NULLIFIER_BYTES], String> {
    if value.len() != NULLIFIER_BYTES {
        return Err(format!(
            "nullifier must contain exactly {NULLIFIER_BYTES} bytes; received {}",
            value.len()
        ));
    }
    let mut nullifier = [0u8; NULLIFIER_BYTES];
    nullifier.copy_from_slice(value);
    Ok(nullifier)
}

fn validate_draft(draft: &ApprovalDraft) -> Result<(), String> {
    parse_proposal_hash(&draft.proposal_hash)?;
    parse_nullifier(&draft.nullifier)?;
    if draft.version != UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION {
        return Err(format!(
            "validator approvals require signing-envelope version {}",
            UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION
        ));
    }
    Ok(())
}

fn canonical_approval_message(draft: &ApprovalDraft) -> Result<Vec<u8>, String> {
    validate_draft(draft)?;
    let proposal_hash = parse_proposal_hash(&draft.proposal_hash)?;
    let nullifier = parse_nullifier(&draft.nullifier)?;

    let mut hasher = Sha3_256::new();
    hasher.update(UltraBlockchain::SOVEREIGN_ADDR.as_bytes());
    hasher.update(PROPOSAL_RECIPIENT.as_bytes());
    hasher.update(&APPROVAL_AMOUNT.to_le_bytes());
    hasher.update(&APPROVAL_FEE.to_le_bytes());
    hasher.update(&draft.timestamp.to_le_bytes());
    hasher.update(&nullifier);
    hasher.update(&draft.nonce.to_le_bytes());
    hasher.update(&APPROVAL_GAS_LIMIT.to_le_bytes());
    hasher.update(&APPROVAL_GAS_PRICE.to_le_bytes());
    hasher.update(b"UltraNet/approval-signing-envelope/v3");
    hasher.update(&draft.version.to_le_bytes());
    hasher.update(&UltraBlockchain::L1_CHAIN_ID.to_le_bytes());
    hasher.update(b"ValidatorApproval");
    hasher.update(&proposal_hash);
    Ok(hasher.finalize().to_vec())
}

fn now_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

async fn fetch_pending_proposal(
    client: &reqwest::Client,
    base_url: &str,
    proposal_hash: &str,
) -> Result<PendingProposal, String> {
    let response = client
        .get(format!("{base_url}/api/governance/proposals"))
        .send()
        .await
        .map_err(|error| format!("cannot reach UltraNet API: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("cannot read proposal response ({status}): {error}"))?;
    let payload = serde_json::from_str::<ProposalsResponse>(&body).map_err(|error| {
        format!("UltraNet API returned invalid proposal JSON ({status}): {error}")
    })?;
    if !status.is_success() || !payload.success {
        return Err(payload
            .message
            .unwrap_or_else(|| format!("UltraNet API returned HTTP {status}")));
    }

    let requested_hash = normalize_proposal_hash(proposal_hash)?;
    payload
        .proposals
        .into_iter()
        .find(|proposal| {
            normalize_proposal_hash(&proposal.hash).ok().as_deref() == Some(&requested_hash)
        })
        .ok_or_else(|| format!("pending validator proposal {requested_hash} was not found"))
}

async fn fetch_next_nonce(client: &reqwest::Client, base_url: &str) -> Result<u64, String> {
    let response = client
        .get(format!(
            "{base_url}/api/account/{}",
            UltraBlockchain::SOVEREIGN_ADDR
        ))
        .send()
        .await
        .map_err(|error| format!("cannot reach UltraNet account API: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("cannot read account response ({status}): {error}"))?;
    let payload = serde_json::from_str::<ApiEnvelope<AccountView>>(&body).map_err(|error| {
        format!("UltraNet API returned invalid account JSON ({status}): {error}")
    })?;
    if !status.is_success() || !payload.success {
        return Err(payload
            .message
            .unwrap_or_else(|| format!("UltraNet API returned HTTP {status}")));
    }
    payload
        .data
        .map(|account| account.nonce)
        .ok_or_else(|| "UltraNet account response did not contain a nonce".into())
}

fn decode_fixed_hex(value: &str, field: &str, expected_bytes: usize) -> Result<Vec<u8>, String> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let bytes = hex::decode(value).map_err(|_| format!("{field} is not valid hexadecimal"))?;
    if bytes.len() != expected_bytes {
        return Err(format!(
            "{field} must decode to exactly {expected_bytes} bytes; received {}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn load_owner_manifest(path: &Path) -> Result<Vec<OwnerSummary>, String> {
    let owners = read_json::<Vec<OwnerSummary>>(path)?;
    if owners.len() != SOVEREIGN_OWNER_COUNT {
        return Err(format!(
            "authorized owner manifest must contain exactly {SOVEREIGN_OWNER_COUNT} owners; received {}",
            owners.len()
        ));
    }

    for (position, owner) in owners.iter().enumerate() {
        if owner.index != position {
            return Err(format!(
                "authorized owner manifest indexes must be contiguous starting at 0; expected {position}, received {}",
                owner.index
            ));
        }
        let public_key = decode_fixed_hex(
            &owner.public_key,
            &format!("authorized owner {} public_key", owner.index),
            2_592,
        )?;
        let derived_address = QuantumKeyPair::address_from_public_key(&public_key);
        let declared_address = validate_node_identifier(&owner.address).map_err(|error| {
            format!(
                "authorized owner {} address is invalid: {error}",
                owner.index
            )
        })?;
        if declared_address != derived_address {
            return Err(format!(
                "authorized owner {} address does not match public_key",
                owner.index
            ));
        }
    }
    Ok(owners)
}

fn find_authorized_owner_index(
    public_key: &[u8],
    owners: &[OwnerSummary],
    label: &str,
) -> Result<usize, String> {
    owners
        .iter()
        .find_map(|owner| {
            let owner_key = decode_fixed_hex(
                &owner.public_key,
                &format!("authorized owner {} public_key", owner.index),
                2_592,
            )
            .ok()?;
            (owner_key == public_key).then_some(owner.index)
        })
        .ok_or_else(|| format!("{label} public key is not in the authorized owner manifest"))
}

fn validate_signed_artifact(
    artifact: &SignedApprovalArtifact,
    draft: &ApprovalDraft,
    message: &[u8],
    label: &str,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if artifact.draft != *draft {
        return Err(format!("{label} was signed for different approval fields"));
    }
    let public_key = decode_fixed_hex(&artifact.public_key, &format!("{label} public_key"), 2_592)?;
    UltraNet::quantum_crypto::PublicKey::from_bytes(&public_key)
        .map_err(|_| format!("{label} public_key is not a valid Dilithium-5 public key"))?;
    let derived_address = QuantumKeyPair::address_from_public_key(&public_key);
    let declared_address = validate_node_identifier(&artifact.owner_address)
        .map_err(|error| format!("{label} owner_address is invalid: {error}"))?;
    if declared_address != derived_address {
        return Err(format!("{label} owner_address does not match public_key"));
    }
    let signature = decode_fixed_hex(
        &artifact.signature,
        &format!("{label} signature"),
        SIGNATURE_BYTES,
    )?;
    if !QuantumKeyPair::verify(&public_key, message, &signature) {
        return Err(format!(
            "{label} does not verify against the approval draft"
        ));
    }
    Ok((public_key, signature))
}

async fn run_owners(args: OwnersArgs) -> Result<(), String> {
    let owners = load_owners(&args.key_file.keys)?;
    let summaries = owners
        .iter()
        .map(|owner| OwnerSummary {
            index: owner.index,
            address: owner.address.clone(),
            public_key: hex::encode(&owner.keypair.public_key),
        })
        .collect::<Vec<_>>();
    write_json(
        &summaries,
        args.output.output.as_deref(),
        args.output.pretty,
    )
}

async fn run_prepare(args: PrepareArgs) -> Result<(), String> {
    let proposal_hash = normalize_proposal_hash(&args.proposal_hash)?;
    let base_url = api_base_url(args.api_base_url.clone());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| format!("cannot configure HTTP client: {error}"))?;
    let (proposal_metadata, proposal_public_key_bytes) = if args.offline {
        if args.nonce.is_none() {
            return Err("--offline requires --nonce; verify the proposal and nonce independently before signing".into());
        }
        (None, None)
    } else {
        let proposal = fetch_pending_proposal(&client, &base_url, &proposal_hash).await?;
        let public_key =
            decode_fixed_hex(&proposal.public_key, "pending proposal public_key", 2_592)?;
        (Some(proposal.metadata), Some(public_key.len()))
    };
    let nonce = match args.nonce {
        Some(nonce) => nonce,
        None => fetch_next_nonce(&client, &base_url).await?,
    };
    let timestamp = now_seconds()?;
    let mut nullifier = vec![0u8; NULLIFIER_BYTES];
    OsRng.fill_bytes(&mut nullifier);
    let draft = ApprovalDraft {
        proposal_hash,
        timestamp,
        nonce,
        nullifier,
        version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
    };
    validate_draft(&draft)?;
    write_json(&draft, args.output.output.as_deref(), args.output.pretty)?;
    eprintln!("Prepared validator approval draft");
    if let Some(metadata) = proposal_metadata {
        let escaped_metadata =
            serde_json::to_string(&metadata).unwrap_or_else(|_| "\"<invalid>\"".into());
        eprintln!("  alias: {escaped_metadata}");
    } else {
        eprintln!("  alias: not fetched (--offline; verify the proposal independently)");
    }
    eprintln!("  proposal hash: {}", draft.proposal_hash);
    if let Some(byte_count) = proposal_public_key_bytes {
        eprintln!("  node public key bytes: {byte_count}");
    }
    eprintln!("  nonce: {}", draft.nonce);
    eprintln!("  nullifier: generated with the OS CSPRNG; do not reuse this draft");
    Ok(())
}

async fn run_sign(args: SignArgs) -> Result<(), String> {
    let draft = read_json::<ApprovalDraft>(&args.request)?;
    let message = canonical_approval_message(&draft)?;
    let owner = load_owner_at(&args.key_file.keys, args.owner_index)?;

    let mut signature = owner.keypair.sign(&message);
    if signature.len() != SIGNATURE_BYTES
        || !QuantumKeyPair::verify(&owner.keypair.public_key, &message, &signature)
    {
        signature.zeroize();
        return Err(format!(
            "owner {} produced an invalid Dilithium-5 signature",
            owner.index
        ));
    }
    let artifact = SignedApprovalArtifact {
        draft,
        owner_address: owner.address.clone(),
        public_key: hex::encode(&owner.keypair.public_key),
        signature: hex::encode(&signature),
    };
    signature.zeroize();
    write_json(&artifact, args.output.output.as_deref(), args.output.pretty)
}

async fn run_combine(args: CombineArgs) -> Result<(), String> {
    if args.signatures.len() != 2 {
        return Err("combine requires exactly two --signature files".into());
    }
    let draft = read_json::<ApprovalDraft>(&args.request)?;
    let message = canonical_approval_message(&draft)?;
    let authorized_owners = load_owner_manifest(&args.authorized_owners)?;
    let first = read_json::<SignedApprovalArtifact>(&args.signatures[0])?;
    let second = read_json::<SignedApprovalArtifact>(&args.signatures[1])?;
    let first_label = format!("signature artifact {}", args.signatures[0].display());
    let second_label = format!("signature artifact {}", args.signatures[1].display());
    let (first_public_key, first_signature) =
        validate_signed_artifact(&first, &draft, &message, &first_label)?;
    let (second_public_key, second_signature) =
        validate_signed_artifact(&second, &draft, &message, &second_label)?;
    let first_owner_index =
        find_authorized_owner_index(&first_public_key, &authorized_owners, &first_label)?;
    let second_owner_index =
        find_authorized_owner_index(&second_public_key, &authorized_owners, &second_label)?;
    if first_owner_index == second_owner_index {
        return Err("the two signatures must come from different authorized owners".into());
    }

    let mut combined_signature = Vec::with_capacity(COMBINED_SIGNATURE_BYTES);
    if first_owner_index <= second_owner_index {
        combined_signature.extend_from_slice(&first_signature);
        combined_signature.extend_from_slice(&second_signature);
    } else {
        combined_signature.extend_from_slice(&second_signature);
        combined_signature.extend_from_slice(&first_signature);
    }
    if combined_signature.len() != COMBINED_SIGNATURE_BYTES {
        return Err(format!(
            "combined signature must contain exactly {COMBINED_SIGNATURE_BYTES} bytes"
        ));
    }

    let payload = ApprovalPayload {
        draft,
        signature: combined_signature,
    };
    write_json(&payload, args.output.output.as_deref(), args.output.pretty)?;
    eprintln!(
        "Combined two verified owner signatures into a {COMBINED_SIGNATURE_BYTES}-byte approval"
    );
    Ok(())
}

async fn run_submit(args: SubmitArgs) -> Result<(), String> {
    let payload = read_json::<ApprovalPayload>(&args.request)?;
    validate_draft(&payload.draft)?;
    if payload.signature.len() != COMBINED_SIGNATURE_BYTES {
        return Err(format!(
            "approval signature must contain exactly {COMBINED_SIGNATURE_BYTES} bytes; received {}",
            payload.signature.len()
        ));
    }

    let base_url = api_base_url(args.api_base_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| format!("cannot configure HTTP client: {error}"))?;
    let response = client
        .post(format!("{base_url}/api/governance/approve"))
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("cannot reach UltraNet approval API: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("cannot read approval response ({status}): {error}"))?;
    let result = serde_json::from_str::<ApiResponse>(&body).map_err(|error| {
        format!("UltraNet API returned invalid approval JSON ({status}): {error}")
    })?;
    if !status.is_success() || !result.success {
        return Err(format!("approval rejected: {}", result.message));
    }
    println!("{}", result.message);
    Ok(())
}

#[tokio::main]
async fn main() {
    let result = match Cli::parse().command {
        Command::Owners(args) => run_owners(args).await,
        Command::Prepare(args) => run_prepare(args).await,
        Command::Sign(args) => run_sign(args).await,
        Command::Combine(args) => run_combine(args).await,
        Command::Submit(args) => run_submit(args).await,
    };
    if let Err(error) = result {
        eprintln!("ultranet-approve: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UltraNet::{ProofType, Transaction, TransactionPayload};

    fn test_draft() -> ApprovalDraft {
        ApprovalDraft {
            proposal_hash: "11".repeat(32),
            timestamp: 1_785_183_488,
            nonce: 7,
            nullifier: vec![0x22; NULLIFIER_BYTES],
            version: UltraBlockchain::APPROVAL_BOUND_TRANSACTION_VERSION,
        }
    }

    #[test]
    fn canonical_message_matches_node_transaction_envelope() {
        let path = format!("test_db_approval_cli_{}", std::process::id());
        let blockchain = UltraBlockchain::new(&path);
        let draft = test_draft();
        let mut transaction = Transaction {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: PROPOSAL_RECIPIENT.to_string(),
            amount: APPROVAL_AMOUNT,
            signature: vec![],
            zk_proof: vec![],
            nullifier: [0x22; NULLIFIER_BYTES],
            timestamp: draft.timestamp,
            fee: APPROVAL_FEE,
            nonce: draft.nonce,
            gas_limit: APPROVAL_GAS_LIMIT,
            gas_price: APPROVAL_GAS_PRICE,
            proof_type: ProofType::Ownership,
            payload: TransactionPayload::ValidatorApproval {
                proposal_hash: [0x11; 32],
            },
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: draft.version,
        };
        transaction.signature.clear();
        assert_eq!(
            canonical_approval_message(&draft).unwrap(),
            blockchain.create_transaction_message(&transaction)
        );
        drop(blockchain);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn generated_signature_pair_has_the_required_flat_length() {
        let draft = test_draft();
        let message = canonical_approval_message(&draft).unwrap();
        let owner_one = QuantumKeyPair::generate();
        let owner_two = QuantumKeyPair::generate();
        let first = owner_one.sign(&message);
        let second = owner_two.sign(&message);
        let mut combined = first.clone();
        combined.extend_from_slice(&second);

        assert_eq!(first.len(), SIGNATURE_BYTES);
        assert_eq!(second.len(), SIGNATURE_BYTES);
        assert_eq!(combined.len(), COMBINED_SIGNATURE_BYTES);
        assert!(QuantumKeyPair::verify(
            &owner_one.public_key,
            &message,
            &first
        ));
        assert!(QuantumKeyPair::verify(
            &owner_two.public_key,
            &message,
            &second
        ));
    }

    #[test]
    fn nullifier_and_hash_lengths_are_strict() {
        assert!(parse_proposal_hash(&"aa".repeat(32)).is_ok());
        assert!(parse_proposal_hash(&"aa".repeat(31)).is_err());
        assert!(parse_nullifier(&[0u8; NULLIFIER_BYTES]).is_ok());
        assert!(parse_nullifier(&[0u8; NULLIFIER_BYTES - 1]).is_err());
    }
}
