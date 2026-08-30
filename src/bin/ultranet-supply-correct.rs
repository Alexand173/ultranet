use clap::{Args, Parser, Subcommand};
use rand::{rngs::OsRng, RngCore};
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    supply_correction, QuantumKeyPair, UltraBlockchain,
};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8081";
const OWNER_COUNT: usize = 3;
const PUBLIC_KEY_BYTES: usize = 2_592;
const SIGNATURE_BYTES: usize = 4_627;
const NULLIFIER_BYTES: usize = 32;
const LOCAL_KEYPAIR_PROBE: &[u8] = b"ULTRANET_SUPPLY_CORRECTION_LOCAL_KEYPAIR_CHECK_V1";

#[derive(Debug, Parser)]
#[command(
    name = "ultranet-supply-correct",
    version,
    about = "Prepare, sign, combine, and submit the one-time UltraNet genesis supply correction"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Emit the three authorized public owner records without private material.
    Owners(OwnersArgs),
    /// Read the live sovereign account and emit a public correction draft.
    Prepare(PrepareArgs),
    /// Sign one public draft with one local owner secret key.
    Sign(SignArgs),
    /// Verify two distinct owner artifacts and emit the final public request.
    Combine(CombineArgs),
    /// Re-read the live precondition and submit an already-reviewed public request.
    Submit(SubmitArgs),
}

#[derive(Debug, Args)]
struct KeyFileArgs {
    /// Local key file. Never copy this file to a node host.
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
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct SignArgs {
    /// Public correction draft JSON produced by prepare.
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
    /// Public correction draft JSON produced by prepare.
    #[arg(long)]
    request: PathBuf,
    /// Exactly two signed artifacts. Repeat once per owner.
    #[arg(long = "signature", value_name = "FILE", num_args = 1)]
    signatures: Vec<PathBuf>,
    /// Public owner manifest emitted by owners.
    #[arg(long = "authorized-owners")]
    authorized_owners: PathBuf,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct SubmitArgs {
    /// Final public correction request produced by combine.
    #[arg(long)]
    request: PathBuf,
    /// Public API origin. Can also be set with ULTRANET_API_BASE_URL.
    #[arg(long)]
    api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SupplyCorrectionDraft {
    sender: String,
    sender_public_key: Vec<u8>,
    recipient: String,
    amount: u64,
    fee: u64,
    nonce: u64,
    timestamp: u64,
    nullifier: Vec<u8>,
    gas_limit: u64,
    gas_price: u64,
    chain_id: u32,
    version: u32,
    correction_id: String,
    target_address: String,
    expected_balance_base_units: u64,
    target_balance_base_units: u64,
    decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedSupplyCorrectionArtifact {
    #[serde(flatten)]
    draft: SupplyCorrectionDraft,
    owner_index: usize,
    owner_address: String,
    public_key: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CombinedSupplyCorrectionRequest {
    #[serde(flatten)]
    draft: SupplyCorrectionDraft,
    signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnerSummary {
    index: usize,
    address: String,
    public_key: String,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct KeyRecord {
    #[serde(default)]
    address: Option<String>,
    public_key: EncodedKeyBytes,
    #[serde(alias = "private_key")]
    secret_key: EncodedKeyBytes,
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

#[derive(Debug, Deserialize)]
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
    address: String,
    #[serde(default)]
    balance: Option<u64>,
    #[serde(default)]
    balance_base_units: Option<u64>,
    nonce: u64,
    decimals: u8,
}

#[derive(Debug, Deserialize)]
struct SubmitResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: String,
    data: Option<SubmitData>,
}

#[derive(Debug, Deserialize)]
struct SubmitData {
    transaction: Option<SubmitTransaction>,
}

#[derive(Debug, Deserialize)]
struct SubmitTransaction {
    hash: Option<String>,
}

fn main() {
    let result = match Cli::parse().command {
        Command::Owners(args) => run_owners(args),
        Command::Prepare(args) => run_prepare(args),
        Command::Sign(args) => run_sign(args),
        Command::Combine(args) => run_combine(args),
        Command::Submit(args) => run_submit(args),
    };
    if let Err(error) = result {
        eprintln!("ultranet-supply-correct: {error}");
        std::process::exit(1);
    }
}

fn api_base_url(explicit: Option<String>) -> String {
    explicit
        .or_else(|| env::var("ULTRANET_API_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn now_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
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
    if records.len() != OWNER_COUNT {
        return Err(format!(
            "key file must contain exactly {OWNER_COUNT} sovereign owners; received {}",
            records.len()
        ));
    }
    Ok(records)
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
    let matches =
        QuantumKeyPair::verify(&keypair.public_key, LOCAL_KEYPAIR_PROBE, &probe_signature);
    probe_signature.zeroize();
    if !matches {
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

fn load_owner_at(path: &Path, index: usize) -> Result<LoadedOwner, String> {
    if index >= OWNER_COUNT {
        return Err(format!(
            "owner index must be between 0 and {}",
            OWNER_COUNT - 1
        ));
    }
    load_key_records(path)?
        .into_iter()
        .enumerate()
        .nth(index)
        .map(|(record_index, record)| load_owner(record_index, record))
        .ok_or_else(|| format!("owner index {index} does not exist"))?
}

fn load_owners(path: &Path) -> Result<Vec<LoadedOwner>, String> {
    load_key_records(path)?
        .into_iter()
        .enumerate()
        .map(|(index, record)| load_owner(index, record))
        .collect()
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
        eprintln!("Wrote public JSON output to {}", path.display());
    } else {
        io::stdout()
            .write_all(&bytes)
            .map_err(|error| format!("cannot write JSON to stdout: {error}"))?;
    }
    Ok(())
}

fn parse_fixed_hex(value: &str, field: &str, expected_bytes: usize) -> Result<Vec<u8>, String> {
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

fn parse_correction_id(value: &str) -> Result<[u8; 32], String> {
    let bytes = parse_fixed_hex(value, "correction_id", 32)?;
    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

fn validate_draft(draft: &SupplyCorrectionDraft) -> Result<Vec<u8>, String> {
    if draft.sender != UltraBlockchain::SOVEREIGN_ADDR {
        return Err("draft sender is not the fixed sovereign address".into());
    }
    if !draft.sender_public_key.is_empty() {
        return Err("supply correction sender_public_key must be empty".into());
    }
    if draft.recipient != supply_correction::SUPPLY_CORRECTION_RECIPIENT
        || draft.amount != 0
        || draft.fee != 0
        || draft.chain_id != UltraBlockchain::L1_CHAIN_ID
        || draft.gas_limit != supply_correction::SUPPLY_CORRECTION_GAS_LIMIT
        || draft.gas_price != supply_correction::SUPPLY_CORRECTION_GAS_PRICE
    {
        return Err("draft does not use the fixed zero-value L1 envelope".into());
    }
    if draft.version != UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION {
        return Err(format!(
            "supply correction requires version {}",
            UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION
        ));
    }
    if draft.decimals != UltraBlockchain::ULTRA_DECIMALS {
        return Err(format!(
            "supply correction requires {} decimal places",
            UltraBlockchain::ULTRA_DECIMALS
        ));
    }
    let correction_id = parse_correction_id(&draft.correction_id)?;
    if correction_id != supply_correction::correction_id() {
        return Err("draft correction_id is not the fixed protocol identifier".into());
    }
    if draft.target_address != supply_correction::target_address() {
        return Err("draft target_address is not the sovereign genesis address".into());
    }
    if draft.target_balance_base_units != supply_correction::target_balance_base_units() {
        return Err(format!(
            "draft target balance must be {} base units",
            supply_correction::target_balance_base_units()
        ));
    }
    if draft.expected_balance_base_units >= draft.target_balance_base_units {
        return Err("draft expected balance must be below the target balance".into());
    }
    draft
        .target_balance_base_units
        .checked_sub(draft.expected_balance_base_units)
        .ok_or_else(|| "draft balance delta overflowed".to_string())?;
    if draft.nullifier.len() != NULLIFIER_BYTES {
        return Err(format!(
            "nullifier must contain exactly {NULLIFIER_BYTES} bytes"
        ));
    }
    let nullifier: [u8; NULLIFIER_BYTES] = draft
        .nullifier
        .as_slice()
        .try_into()
        .map_err(|_| "nullifier must contain exactly 32 bytes".to_string())?;

    Ok(supply_correction::canonical_message(
        &draft.sender,
        &draft.recipient,
        draft.amount,
        draft.fee,
        draft.timestamp,
        &nullifier,
        draft.nonce,
        draft.gas_limit,
        draft.gas_price,
        draft.chain_id,
        draft.version,
        &correction_id,
        &draft.target_address,
        draft.expected_balance_base_units,
        draft.target_balance_base_units,
    ))
}

fn normalize_owner_summaries(owners: &[OwnerSummary]) -> Result<(), String> {
    if owners.len() != OWNER_COUNT {
        return Err(format!(
            "authorized owner manifest must contain exactly {OWNER_COUNT} owners; received {}",
            owners.len()
        ));
    }
    let mut addresses = std::collections::HashSet::new();
    let mut public_keys = std::collections::HashSet::new();
    for (position, owner) in owners.iter().enumerate() {
        if owner.index != position {
            return Err(format!(
                "authorized owner indexes must be contiguous starting at 0; expected {position}, received {}",
                owner.index
            ));
        }
        let public_key = parse_fixed_hex(
            &owner.public_key,
            &format!("authorized owner {} public_key", owner.index),
            PUBLIC_KEY_BYTES,
        )?;
        UltraNet::quantum_crypto::PublicKey::from_bytes(&public_key)
            .map_err(|_| format!("authorized owner {} public_key is invalid", owner.index))?;
        let address = validate_node_identifier(&owner.address).map_err(|error| {
            format!(
                "authorized owner {} address is invalid: {error}",
                owner.index
            )
        })?;
        if address != QuantumKeyPair::address_from_public_key(&public_key) {
            return Err(format!(
                "authorized owner {} address does not match public_key",
                owner.index
            ));
        }
        if !addresses.insert(address) || !public_keys.insert(public_key) {
            return Err("authorized owner manifest contains duplicate owners".into());
        }
    }
    Ok(())
}

fn find_owner_index(public_key: &[u8], owners: &[OwnerSummary]) -> Result<usize, String> {
    owners
        .iter()
        .find_map(|owner| {
            let owner_key = parse_fixed_hex(
                &owner.public_key,
                &format!("authorized owner {} public_key", owner.index),
                PUBLIC_KEY_BYTES,
            )
            .ok()?;
            (owner_key == public_key).then_some(owner.index)
        })
        .ok_or_else(|| "signature public key is not in the authorized owner manifest".into())
}

async fn fetch_account(client: &reqwest::Client, base_url: &str) -> Result<AccountView, String> {
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
    let account = payload
        .data
        .ok_or_else(|| "account response did not contain data".to_string())?;
    if account.address != UltraBlockchain::SOVEREIGN_ADDR {
        return Err("account API returned an unexpected address".into());
    }
    if account.decimals != UltraBlockchain::ULTRA_DECIMALS {
        return Err(format!(
            "account API returned decimals={}, expected {}",
            account.decimals,
            UltraBlockchain::ULTRA_DECIMALS
        ));
    }
    if let (Some(compatibility), Some(explicit)) = (account.balance, account.balance_base_units) {
        if compatibility != explicit {
            return Err("account API returned conflicting balance fields".into());
        }
    }
    if account.balance_base_units.is_none() && account.balance.is_none() {
        return Err("account API returned no base-unit balance".into());
    }
    Ok(account)
}

fn account_balance_base_units(account: &AccountView) -> Result<u64, String> {
    account
        .balance_base_units
        .or(account.balance)
        .ok_or_else(|| "account API returned no base-unit balance".into())
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| format!("cannot configure HTTP client: {error}"))
}

fn run_owners(args: OwnersArgs) -> Result<(), String> {
    let owners = load_owners(&args.key_file.keys)?;
    let summaries = owners
        .iter()
        .map(|owner| OwnerSummary {
            index: owner.index,
            address: owner.address.clone(),
            public_key: hex::encode(&owner.keypair.public_key),
        })
        .collect::<Vec<_>>();
    normalize_owner_summaries(&summaries)?;
    write_json(
        &summaries,
        args.output.output.as_deref(),
        args.output.pretty,
    )
}

fn run_prepare(args: PrepareArgs) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("cannot start async runtime: {error}"))?;
    runtime.block_on(async move {
        let base_url = api_base_url(args.api_base_url);
        let client = build_client()?;
        let account = fetch_account(&client, &base_url).await?;
        let expected_balance = account_balance_base_units(&account)?;
        let target_balance = UltraBlockchain::GENESIS_ALLOCATION_BASE_UNITS;
        if expected_balance >= target_balance {
            return Err(format!(
                "refusing to prepare: live balance is {} base units; target is {}",
                expected_balance, target_balance
            ));
        }
        let timestamp = now_seconds()?;
        let mut nullifier = vec![0u8; NULLIFIER_BYTES];
        OsRng.fill_bytes(&mut nullifier);
        let draft = SupplyCorrectionDraft {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: supply_correction::SUPPLY_CORRECTION_RECIPIENT.to_string(),
            amount: 0,
            fee: 0,
            nonce: account.nonce,
            timestamp,
            nullifier,
            gas_limit: supply_correction::SUPPLY_CORRECTION_GAS_LIMIT,
            gas_price: supply_correction::SUPPLY_CORRECTION_GAS_PRICE,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION,
            correction_id: hex::encode(supply_correction::correction_id()),
            target_address: supply_correction::target_address().to_string(),
            expected_balance_base_units: expected_balance,
            target_balance_base_units: target_balance,
            decimals: UltraBlockchain::ULTRA_DECIMALS,
        };
        validate_draft(&draft)?;
        write_json(&draft, args.output.output.as_deref(), args.output.pretty)?;
        eprintln!("Prepared public sovereign supply-correction draft");
        eprintln!("  current balance: {} base units", expected_balance);
        eprintln!(
            "  target balance: {} base units ({})",
            target_balance,
            UltraBlockchain::format_base_units(target_balance)
        );
        eprintln!("  nonce: {}", draft.nonce);
        eprintln!("  correction_id: {}", draft.correction_id);
        eprintln!("  nullifier: generated by the OS CSPRNG; do not reuse this draft");
        Ok(())
    })
}

fn run_sign(args: SignArgs) -> Result<(), String> {
    let draft = read_json::<SupplyCorrectionDraft>(&args.request)?;
    let message = validate_draft(&draft)?;
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
    let artifact = SignedSupplyCorrectionArtifact {
        draft,
        owner_index: owner.index,
        owner_address: owner.address,
        public_key: hex::encode(&owner.keypair.public_key),
        signature: hex::encode(&signature),
    };
    signature.zeroize();
    write_json(&artifact, args.output.output.as_deref(), args.output.pretty)
}

fn validate_signed_artifact(
    artifact: &SignedSupplyCorrectionArtifact,
    draft: &SupplyCorrectionDraft,
    message: &[u8],
    label: &str,
) -> Result<(usize, Vec<u8>, Vec<u8>), String> {
    if artifact.draft != *draft {
        return Err(format!(
            "{label} was signed for different correction fields"
        ));
    }
    let public_key = parse_fixed_hex(
        &artifact.public_key,
        &format!("{label} public_key"),
        PUBLIC_KEY_BYTES,
    )?;
    UltraNet::quantum_crypto::PublicKey::from_bytes(&public_key)
        .map_err(|_| format!("{label} public_key is not a valid Dilithium-5 key"))?;
    let derived_address = QuantumKeyPair::address_from_public_key(&public_key);
    let declared_address = validate_node_identifier(&artifact.owner_address)
        .map_err(|error| format!("{label} owner_address is invalid: {error}"))?;
    if declared_address != derived_address {
        return Err(format!("{label} owner_address does not match public_key"));
    }
    let signature = parse_fixed_hex(
        &artifact.signature,
        &format!("{label} signature"),
        SIGNATURE_BYTES,
    )?;
    if !QuantumKeyPair::verify(&public_key, message, &signature) {
        return Err(format!(
            "{label} does not verify against the correction draft"
        ));
    }
    Ok((artifact.owner_index, public_key, signature))
}

fn run_combine(args: CombineArgs) -> Result<(), String> {
    if args.signatures.len() != 2 {
        return Err("combine requires exactly two --signature files".into());
    }
    let draft = read_json::<SupplyCorrectionDraft>(&args.request)?;
    let message = validate_draft(&draft)?;
    let authorized_owners = read_json::<Vec<OwnerSummary>>(&args.authorized_owners)?;
    normalize_owner_summaries(&authorized_owners)?;

    let first = read_json::<SignedSupplyCorrectionArtifact>(&args.signatures[0])?;
    let second = read_json::<SignedSupplyCorrectionArtifact>(&args.signatures[1])?;
    let first_label = format!("signature artifact {}", args.signatures[0].display());
    let second_label = format!("signature artifact {}", args.signatures[1].display());
    let (first_declared_index, first_public_key, first_signature) =
        validate_signed_artifact(&first, &draft, &message, &first_label)?;
    let (second_declared_index, second_public_key, second_signature) =
        validate_signed_artifact(&second, &draft, &message, &second_label)?;
    let first_index = find_owner_index(&first_public_key, &authorized_owners)?;
    let second_index = find_owner_index(&second_public_key, &authorized_owners)?;
    if first_declared_index != first_index || second_declared_index != second_index {
        return Err(
            "signature artifact owner index does not match the public owner manifest".into(),
        );
    }
    if first_index == second_index {
        return Err("the two signatures must come from different authorized owners".into());
    }

    let mut combined_signature = Vec::with_capacity(SIGNATURE_BYTES * 2);
    if first_index <= second_index {
        combined_signature.extend_from_slice(&first_signature);
        combined_signature.extend_from_slice(&second_signature);
    } else {
        combined_signature.extend_from_slice(&second_signature);
        combined_signature.extend_from_slice(&first_signature);
    }
    if combined_signature.len() != SIGNATURE_BYTES * 2 {
        return Err("combined signature has an unexpected length".into());
    }

    let request = CombinedSupplyCorrectionRequest {
        draft,
        signature: combined_signature,
    };
    write_json(&request, args.output.output.as_deref(), args.output.pretty)?;
    eprintln!("Combined two verified sovereign signatures into a public request");
    eprintln!("Submit only after an independent review of nonce, precondition, target, and correction_id.");
    Ok(())
}

fn run_submit(args: SubmitArgs) -> Result<(), String> {
    let request = read_json::<CombinedSupplyCorrectionRequest>(&args.request)?;
    validate_draft(&request.draft)?;
    if request.signature.len() != SIGNATURE_BYTES * 2 {
        return Err(format!(
            "combined signature must contain exactly {} bytes; received {}",
            SIGNATURE_BYTES * 2,
            request.signature.len()
        ));
    }

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("cannot start async runtime: {error}"))?;
    runtime.block_on(async move {
        let base_url = api_base_url(args.api_base_url);
        let client = build_client()?;
        let account = fetch_account(&client, &base_url).await?;
        let live_balance = account_balance_base_units(&account)?;
        if live_balance != request.draft.expected_balance_base_units {
            return Err(format!(
                "refusing to submit stale correction: expected {} base units, live state is {}",
                request.draft.expected_balance_base_units, live_balance
            ));
        }
        if account.nonce != request.draft.nonce {
            return Err(format!(
                "refusing to submit stale correction: draft nonce {}, live next nonce {}",
                request.draft.nonce, account.nonce
            ));
        }

        let response = client
            .post(format!("{base_url}/api/governance/supply-correction"))
            .json(&request)
            .send()
            .await
            .map_err(|error| format!("cannot reach UltraNet supply-correction API: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("cannot read supply-correction response ({status}): {error}"))?;
        let result = serde_json::from_str::<SubmitResponse>(&body).map_err(|error| {
            format!("UltraNet API returned invalid supply-correction JSON ({status}): {error}")
        })?;
        if !status.is_success() || !result.success {
            return Err(format!("supply correction rejected: {}", result.message));
        }
        let hash = result
            .data
            .and_then(|data| data.transaction)
            .and_then(|transaction| transaction.hash)
            .unwrap_or_else(|| "not returned; query the API before retrying".to_string());
        println!("{}", result.message);
        println!("Transaction hash: {hash}");
        if status == StatusCode::OK {
            eprintln!("The request is accepted into the node's pending pool; inclusion/finality still requires operator verification.");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_draft() -> SupplyCorrectionDraft {
        SupplyCorrectionDraft {
            sender: UltraBlockchain::SOVEREIGN_ADDR.to_string(),
            sender_public_key: vec![],
            recipient: supply_correction::SUPPLY_CORRECTION_RECIPIENT.to_string(),
            amount: 0,
            fee: 0,
            nonce: 7,
            timestamp: 1_785_183_488,
            nullifier: vec![0x22; NULLIFIER_BYTES],
            gas_limit: supply_correction::SUPPLY_CORRECTION_GAS_LIMIT,
            gas_price: supply_correction::SUPPLY_CORRECTION_GAS_PRICE,
            chain_id: UltraBlockchain::L1_CHAIN_ID,
            version: UltraBlockchain::SUPPLY_CORRECTION_TRANSACTION_VERSION,
            correction_id: hex::encode(supply_correction::correction_id()),
            target_address: supply_correction::target_address().to_string(),
            expected_balance_base_units: 1_000_000,
            target_balance_base_units: UltraBlockchain::GENESIS_ALLOCATION_BASE_UNITS,
            decimals: UltraBlockchain::ULTRA_DECIMALS,
        }
    }

    #[test]
    fn draft_digest_matches_node_correction_envelope() {
        let draft = test_draft();
        let message = validate_draft(&draft).unwrap();
        let correction_id = supply_correction::correction_id();
        let nullifier: [u8; 32] = draft.nullifier.clone().try_into().unwrap();
        let expected = supply_correction::canonical_message(
            &draft.sender,
            &draft.recipient,
            draft.amount,
            draft.fee,
            draft.timestamp,
            &nullifier,
            draft.nonce,
            draft.gas_limit,
            draft.gas_price,
            draft.chain_id,
            draft.version,
            &correction_id,
            &draft.target_address,
            draft.expected_balance_base_units,
            draft.target_balance_base_units,
        );
        assert_eq!(message, expected);
    }

    #[test]
    fn stale_target_and_mutated_precondition_are_rejected() {
        let mut draft = test_draft();
        draft.target_balance_base_units -= 1;
        assert!(validate_draft(&draft).is_err());

        let mut draft = test_draft();
        draft.expected_balance_base_units = draft.target_balance_base_units;
        assert!(validate_draft(&draft).is_err());
    }

    #[test]
    fn owner_manifest_rejects_duplicate_public_records() {
        let owner = OwnerSummary {
            index: 0,
            address: "a".repeat(64),
            public_key: "00".repeat(PUBLIC_KEY_BYTES),
        };
        let owners = vec![
            owner.clone(),
            OwnerSummary {
                index: 1,
                ..owner.clone()
            },
            OwnerSummary { index: 2, ..owner },
        ];
        assert!(normalize_owner_summaries(&owners).is_err());
    }
}
