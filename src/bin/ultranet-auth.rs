use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use UltraNet::{
    api::AuthLoginRequest,
    auth::{canonical_login_message, validate_node_identifier, AuthChallenge},
    quantum_crypto::{PKTrait, SKTrait},
    QuantumKeyPair,
};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8081";
const LOCAL_KEYPAIR_PROBE: &[u8] = b"ULTRANET_AUTH_LOCAL_KEYPAIR_CHECK_V1";

#[derive(Debug, Parser)]
#[command(
    name = "ultranet-auth",
    version,
    about = "Create a Dilithium-5 UltraNet authentication signature locally"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Request a fresh challenge and write the signed login JSON payload.
    SignChallenge(SignChallengeArgs),
}

#[derive(Debug, Args)]
struct SignChallengeArgs {
    /// Public API origin. Can also be set with ULTRANET_API_BASE_URL.
    #[arg(long)]
    api_base_url: Option<String>,

    /// Local sovereign key file; never copy this file to the node host.
    #[arg(long, default_value = "sovereign_keys.json")]
    keys: PathBuf,

    /// Zero-based key record index inside the key file.
    #[arg(long, default_value_t = 0)]
    key_index: usize,

    /// Write the public signed request to a new file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,
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
                hex::decode(&value).map_err(|error| format!("{field} is not valid hex: {error}"))
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

#[derive(Debug, Serialize)]
struct AuthChallengeRequest<'a> {
    node_identifier: &'a str,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

fn api_base_url(explicit: Option<String>) -> String {
    explicit
        .or_else(|| env::var("ULTRANET_API_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn load_keypair(path: &Path, key_index: usize) -> Result<QuantumKeyPair, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read key file {}: {error}", path.display()))?;
    let records = serde_json::from_str::<KeyFile>(&raw)
        .map_err(|error| format!("invalid key file {}: {error}", path.display()))?
        .records();
    let record = records
        .into_iter()
        .nth(key_index)
        .ok_or_else(|| format!("key index {key_index} does not exist in {}", path.display()))?;

    let public_key = record.public_key.into_bytes("public_key")?;
    let secret_key = record.secret_key.into_bytes("secret_key/private_key")?;

    UltraNet::quantum_crypto::PublicKey::from_bytes(&public_key)
        .map_err(|_| "public_key is not a valid Dilithium-5 public key".to_string())?;
    UltraNet::quantum_crypto::SecretKey::from_bytes(&secret_key)
        .map_err(|_| "secret_key is not a valid Dilithium-5 secret key".to_string())?;

    let derived_address = QuantumKeyPair::address_from_public_key(&public_key);
    if let Some(address) = record.address {
        let declared_address = validate_node_identifier(&address)
            .map_err(|error| format!("key record address is invalid: {error}"))?;
        if declared_address != derived_address {
            return Err("key record address does not match its public_key".into());
        }
    }

    let keypair = QuantumKeyPair {
        public_key,
        secret_key,
        key_id: [0; 32],
        created_at: 0,
        version: 1,
    };
    let probe_signature = keypair.sign(LOCAL_KEYPAIR_PROBE);
    if !QuantumKeyPair::verify(&keypair.public_key, LOCAL_KEYPAIR_PROBE, &probe_signature) {
        return Err(
            "public_key and secret_key do not belong to the same Dilithium-5 keypair".into(),
        );
    }

    Ok(keypair)
}

async fn request_challenge(
    client: &reqwest::Client,
    base_url: &str,
    node_identifier: &str,
) -> Result<AuthChallenge, String> {
    let response = client
        .post(format!("{base_url}/api/auth/challenge"))
        .json(&AuthChallengeRequest { node_identifier })
        .send()
        .await
        .map_err(|error| format!("cannot reach UltraNet API: {error}"))?;
    let status = response.status();
    let payload = response
        .json::<ApiEnvelope<AuthChallenge>>()
        .await
        .map_err(|error| format!("UltraNet API returned invalid JSON ({status}): {error}"))?;

    if !status.is_success() || !payload.success {
        return Err(payload
            .message
            .unwrap_or_else(|| format!("UltraNet API returned HTTP {status}")));
    }

    payload
        .data
        .ok_or_else(|| "UltraNet API response did not contain a challenge".into())
}

fn build_signed_payload(
    challenge: &AuthChallenge,
    keypair: &QuantumKeyPair,
) -> Result<AuthLoginRequest, String> {
    let message = canonical_login_message(
        &challenge.challenge_id,
        &challenge.challenge,
        &challenge.node_identifier,
        challenge.expires_at,
        challenge.version,
    );
    let signature = keypair.sign(&message);
    if !QuantumKeyPair::verify(&keypair.public_key, &message, &signature) {
        return Err("local signature self-check failed".into());
    }

    Ok(AuthLoginRequest {
        challenge_id: challenge.challenge_id.clone(),
        challenge: challenge.challenge.clone(),
        node_identifier: challenge.node_identifier.clone(),
        expires_at: challenge.expires_at,
        public_key: keypair.public_key.clone(),
        signature,
        version: challenge.version,
    })
}

fn write_output(output: Option<&Path>, json: &str) -> Result<(), String> {
    let Some(path) = output else {
        println!("{json}");
        return Ok(());
    };

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create output {}: {error}", path.display()))?;
    file.write_all(json.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("cannot write output {}: {error}", path.display()))?;
    set_private_permissions(path)?;
    eprintln!("Signed public login payload written to {}", path.display());
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot restrict output permissions: {error}"))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

async fn run_sign_challenge(args: SignChallengeArgs) -> Result<(), String> {
    let keypair = load_keypair(&args.keys, args.key_index)?;
    let node_identifier = keypair.address();
    let base_url = api_base_url(args.api_base_url);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("cannot configure HTTP client: {error}"))?;
    let challenge = request_challenge(&client, &base_url, &node_identifier).await?;
    let payload = build_signed_payload(&challenge, &keypair)?;
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("cannot encode signed login payload: {error}"))?;
    write_output(args.output.as_deref(), &json)
}

#[tokio::main]
async fn main() {
    let result = match Cli::parse().command {
        Command::SignChallenge(args) => run_sign_challenge(args).await,
    };
    if let Err(error) = result {
        eprintln!("ultranet-auth: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn signs_a_canonical_authentication_payload() {
        let keypair = QuantumKeyPair::generate();
        let challenge = AuthChallenge {
            challenge_id: "challenge-id".into(),
            challenge: "challenge".into(),
            node_identifier: keypair.address(),
            expires_at: 1_900_000_000,
            version: 1,
        };

        let payload = build_signed_payload(&challenge, &keypair).unwrap();
        let message = canonical_login_message(
            &payload.challenge_id,
            &payload.challenge,
            &payload.node_identifier,
            payload.expires_at,
            payload.version,
        );
        assert!(QuantumKeyPair::verify(
            &payload.public_key,
            &message,
            &payload.signature
        ));
    }

    #[test]
    fn loads_array_key_file_and_rejects_address_mismatch() {
        let keypair = QuantumKeyPair::generate();
        let path = env::temp_dir().join(format!(
            "ultranet-auth-test-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let record = serde_json::json!([{
            "address": keypair.address(),
            "public_key": hex::encode(&keypair.public_key),
            "secret_key": hex::encode(&keypair.secret_key)
        }]);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let loaded = load_keypair(&path, 0).unwrap();
        assert_eq!(loaded.address(), keypair.address());
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!([{
                "address": "0".repeat(64),
                "public_key": hex::encode(&keypair.public_key),
                "secret_key": hex::encode(&keypair.secret_key)
            }]))
            .unwrap(),
        )
        .unwrap();
        assert!(load_keypair(&path, 0).is_err());

        let other_keypair = QuantumKeyPair::generate();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!([{
                "address": keypair.address(),
                "public_key": hex::encode(&keypair.public_key),
                "secret_key": hex::encode(&other_keypair.secret_key)
            }]))
            .unwrap(),
        )
        .unwrap();
        assert!(load_keypair(&path, 0).is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn loads_owner_style_byte_array_key_file() {
        let keypair = QuantumKeyPair::generate();
        let path = env::temp_dir().join(format!(
            "ultranet-auth-owner-test-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let record = serde_json::json!({
            "owners": [{
                "address": keypair.address(),
                "public_key": keypair.public_key.clone(),
                "private_key": keypair.secret_key.clone()
            }]
        });
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();

        let loaded = load_keypair(&path, 0).unwrap();
        assert_eq!(loaded.address(), keypair.address());
        let _ = fs::remove_file(path);
    }
}
