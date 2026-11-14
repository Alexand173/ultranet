//! `appchain-cli` — sign and submit AppChain transactions with a key that lives on a server.
//!
//! Why this exists: registration and anchoring are owner-signed. A phone cannot anchor twice a day
//! for years, so the owner key ends up on the machine that does. This tool is that machine's hands:
//! it holds one key, encrypted at rest, and does exactly three things with it.
//!
//! ```text
//! appchain-cli generate-key --out owner-key.enc      # create the server key, print its address
//! appchain-cli address      --key-file owner-key.enc
//! appchain-cli register     --key-file owner-key.enc --name "UltraNet Grid"
//! appchain-cli anchor       --key-file owner-key.enc --chain-id 1 --hash <64 hex>
//! appchain-cli transfer     --key-file owner-key.enc --chain-id 1 --new-owner <64 hex>
//! ```
//!
//! The envelope is not re-implemented here. A request is turned into a `Transaction` by
//! `appchain_request_to_transaction` and signed over `UltraBlockchain::create_transaction_message_for`,
//! the two functions the node itself uses, so a CLI transaction cannot drift from what the node
//! verifies. `cli_digests_match_the_node_vectors` pins that against the shared vectors anyway.
//!
//! Passphrase sources, in order: `--password-file`, `ULTRANET_APPCHAIN_KEY_PASSWORD`, then stdin.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use pqcrypto_dilithium::dilithium5;
use pqcrypto_traits::sign::{DetachedSignature as _, SecretKey as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use UltraNet::api::{appchain_request_to_transaction, AppChainOperation, AppChainTxRequest};
use UltraNet::appchain::ledger::{
    APPCHAIN_ANCHOR_FEE, APPCHAIN_REGISTER_FEE, APPCHAIN_TRANSFER_FEE, APPCHAIN_TX_VERSION,
};
use UltraNet::{QuantumKeyPair, UltraBlockchain};

const DEFAULT_NODE: &str = "https://api.ultranetwork.cc";
const KDF_LABEL: &str = "argon2id";
const CIPHER_LABEL: &str = "chacha20poly1305";
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const KEY_VERSION: u32 = 1;

/// The on-disk form of a server key: the Dilithium secret key, encrypted, plus what a reader needs
/// to identify it without decrypting. No passphrase material is stored.
#[derive(Debug, Serialize, Deserialize)]
struct KeyFile {
    version: u32,
    address: String,
    public_key: String,
    kdf: String,
    cipher: String,
    salt: String,
    nonce: String,
    secret_key: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        std::process::exit(2);
    }
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|argument| argument == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let required = |name: &str| -> String {
        flag(name).unwrap_or_else(|| {
            eprintln!("missing required {name}");
            usage();
            std::process::exit(2);
        })
    };

    let result = match args[0].as_str() {
        "generate-key" => generate_key(&required("--out")).map(|address| {
            println!("{address}");
        }),
        "address" => with_key(&required("--key-file"), |key, _secret| {
            println!("{}", key.address);
            Ok(())
        }),
        "register" => {
            let key_file = required("--key-file");
            let name = required("--name");
            submit(
                &key_file,
                &flag,
                AppChainOperation::Register { name },
                APPCHAIN_REGISTER_FEE,
            )
        }
        "anchor" => {
            let key_file = required("--key-file");
            let chain_id = parse_u32(&required("--chain-id"));
            let hash = required("--hash");
            submit(
                &key_file,
                &flag,
                AppChainOperation::Anchor { chain_id, state_root: hash },
                APPCHAIN_ANCHOR_FEE,
            )
        }
        "transfer" => {
            let key_file = required("--key-file");
            let chain_id = parse_u32(&required("--chain-id"));
            let new_owner = required("--new-owner");
            submit(
                &key_file,
                &flag,
                AppChainOperation::TransferOwner { chain_id, new_owner },
                APPCHAIN_TRANSFER_FEE,
            )
        }
        other => {
            eprintln!("unknown command: {other}");
            usage();
            std::process::exit(2);
        }
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn usage() {
    eprintln!(
        "appchain-cli — sign AppChain transactions from a server-side key\n\
         \n\
         commands:\n\
         \x20 generate-key --out <path>                 create an encrypted key file, print its address\n\
         \x20 address      --key-file <path>             print the address and public key\n\
         \x20 register     --key-file <path> --name <name>\n\
         \x20 anchor       --key-file <path> --chain-id <id> --hash <64 hex>\n\
         \x20 transfer     --key-file <path> --chain-id <id> --new-owner <64 hex>\n\
         \n\
         options for register/anchor/transfer:\n\
         \x20 --node <url>          default {DEFAULT_NODE}\n\
         \x20 --nonce <n>           default: read from the node\n\
         \x20 --fee-base-units <n>  default: the fee published by the node's terms\n\
         \x20 --password-file <p>   first line is the passphrase\n\
         \n\
         A higher fee than the minimum is allowed; a lower one is refused by the node."
    );
}

fn parse_u32(value: &str) -> u32 {
    value.parse().unwrap_or_else(|_| {
        eprintln!("expected a number, got {value}");
        std::process::exit(2);
    })
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the epoch")
        .as_secs()
}

fn passphrase(password_file: Option<String>) -> Result<String, String> {
    if let Some(path) = password_file {
        let text = fs::read_to_string(&path).map_err(|error| format!("cannot read {path}: {error}"))?;
        return Ok(text.lines().next().unwrap_or_default().trim().to_string());
    }
    if let Ok(value) = std::env::var("ULTRANET_APPCHAIN_KEY_PASSWORD") {
        return Ok(value);
    }
    eprint!("key passphrase: ");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| format!("cannot read the passphrase: {error}"))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|error| format!("key derivation failed: {error}"))?;
    Ok(key)
}

fn generate_key(path: &str) -> Result<String, String> {
    if Path::new(path).exists() {
        return Err(format!(
            "refusing to overwrite {path}: move the existing key first, or the chain's owner is lost"
        ));
    }
    let pair = QuantumKeyPair::generate();
    let address = pair.address();
    let secret = &pair.secret_key;

    let mut salt = [0u8; SALT_BYTES];
    let mut nonce = [0u8; NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce);

    let pass = passphrase(None)?;
    if pass.is_empty() {
        return Err("an empty passphrase is not a passphrase".to_string());
    }
    let key = derive_key(&pass, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let secret_encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), secret.as_slice())
        .map_err(|_| "encryption failed".to_string())?;

    let file = KeyFile {
        version: KEY_VERSION,
        address: address.clone(),
        public_key: hex::encode(&pair.public_key),
        kdf: KDF_LABEL.to_string(),
        cipher: CIPHER_LABEL.to_string(),
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce),
        secret_key: BASE64.encode(secret_encrypted),
    };
    let body = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    fs::write(path, body).map_err(|error| format!("cannot write {path}: {error}"))?;
    // 0600: the file is encrypted, but it should not be world-readable either.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    eprintln!("wrote {path}; this key owns nothing until a transfer names it");
    Ok(address)
}

fn read_key(path: &str, password_file: Option<String>) -> Result<(KeyFile, Vec<u8>), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let file: KeyFile = serde_json::from_str(&text).map_err(|error| format!("{path} is not a key file: {error}"))?;
    if file.version != KEY_VERSION {
        return Err(format!("{path} uses key-file version {}", file.version));
    }
    if file.kdf != KDF_LABEL || file.cipher != CIPHER_LABEL {
        return Err(format!("{path} uses an unsupported {}/{} scheme", file.kdf, file.cipher));
    }
    let salt = BASE64.decode(&file.salt).map_err(|error| error.to_string())?;
    let nonce = BASE64.decode(&file.nonce).map_err(|error| error.to_string())?;
    let ciphertext = BASE64.decode(&file.secret_key).map_err(|error| error.to_string())?;
    let pass = passphrase(password_file)?;
    let key = derive_key(&pass, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let secret = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "wrong passphrase for this key file".to_string())?;
    Ok((file, secret))
}

fn with_key<F>(path: &str, body: F) -> Result<(), String>
where
    F: FnOnce(&KeyFile, &[u8]) -> Result<(), String>,
{
    let (file, secret) = read_key(path, None)?;
    body(&file, &secret)
}

fn submit(
    key_file: &str,
    flag: &impl Fn(&str) -> Option<String>,
    operation: AppChainOperation,
    minimum_fee: u64,
) -> Result<(), String> {
    let node = flag("--node").unwrap_or_else(|| DEFAULT_NODE.to_string()).trim_end_matches('/').to_string();
    let password_file = flag("--password-file");
    let (file, secret_bytes) = read_key(key_file, password_file)?;
    let secret = dilithium5::SecretKey::from_bytes(&secret_bytes)
        .map_err(|_| "the stored key is not a Dilithium-5 secret key".to_string())?;

    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let fee = match flag("--fee-base-units") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| format!("--fee-base-units expects a number, got {value}"))?,
        None => runtime.block_on(published_fee(&node, &operation, minimum_fee))?,
    };
    let nonce = match flag("--nonce") {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| format!("--nonce expects a number, got {value}"))?,
        None => runtime.block_on(account_nonce(&node, &file.address))?,
    };

    let mut nullifier = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut nullifier);
    let mut request = AppChainTxRequest {
        sender: file.address.clone(),
        sender_public_key: hex::decode(&file.public_key).map_err(|error| error.to_string())?,
        nonce,
        timestamp: now_epoch(),
        nullifier,
        fee,
        signature: vec![],
        version: APPCHAIN_TX_VERSION,
        operation,
    };
    let transaction =
        appchain_request_to_transaction(&request).map_err(|error| format!("request rejected: {error}"))?;
    let message = UltraBlockchain::create_transaction_message_for(&transaction);
    let signature = dilithium5::detached_sign(&message, &secret);
    request.signature = signature.as_bytes().to_vec();

    let body = serde_json::to_value(&request).map_err(|error| error.to_string())?;
    let response: serde_json::Value = runtime.block_on(async {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{node}/api/appchain/tx"))
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("cannot reach {node}: {error}"))?;
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|error| format!("the node returned something that is not JSON: {error}"))
    })?;

    if response["success"] != json!(true) {
        return Err(response["message"]
            .as_str()
            .unwrap_or("the node rejected the transaction")
            .to_string());
    }
    let hash = response["data"]["hash"].as_str().unwrap_or("");
    println!("accepted, pending a block: {hash}");
    println!("nonce {nonce}, fee {fee} base units, signed by {}", file.address);
    Ok(())
}

async fn published_fee(node: &str, operation: &AppChainOperation, fallback: u64) -> Result<u64, String> {
    let terms = terms(node).await?;
    let field = match operation {
        AppChainOperation::Register { .. } => "register_fee_base_units",
        AppChainOperation::Anchor { .. } => "anchor_fee_base_units",
        AppChainOperation::TransferOwner { .. } => "transfer_fee_base_units",
    };
    Ok(terms[field].as_u64().unwrap_or(fallback))
}

async fn terms(node: &str) -> Result<serde_json::Value, String> {
    let ledger: serde_json::Value = reqwest::get(format!("{node}/api/appchain/ledger"))
        .await
        .map_err(|error| format!("cannot read {node}/api/appchain/ledger: {error}"))?
        .json()
        .await
        .map_err(|error| format!("the node returned something that is not JSON: {error}"))?;
    let terms = ledger["terms"].clone();
    if terms["activation_height"].is_null() {
        return Err(
            "this node refuses AppChain transactions: no activation height is set, so nothing \
             would be accepted whatever is sent"
                .to_string(),
        );
    }
    Ok(terms)
}

async fn account_nonce(node: &str, address: &str) -> Result<u64, String> {
    let account: serde_json::Value = reqwest::get(format!("{node}/api/account/{address}"))
        .await
        .map_err(|error| format!("cannot read the account: {error}"))?
        .json()
        .await
        .map_err(|error| format!("the node returned something that is not JSON: {error}"))?;
    account["data"]["nonce"]
        .as_u64()
        .ok_or_else(|| format!("the node did not report a nonce for {address}"))
}

/// The shared vectors from `src/appchain/ledger.rs`, reproduced through this tool's own path.
#[cfg(test)]
mod tests {
    use super::*;

    const SENDER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    const NEW_OWNER: &str = "2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed";

    fn digest(nonce: u64, fee: u64, operation: AppChainOperation) -> String {
        let request = AppChainTxRequest {
            sender: SENDER.to_string(),
            sender_public_key: vec![],
            nonce,
            timestamp: 1_800_000_000,
            nullifier: vec![0x11; 32],
            fee,
            signature: vec![],
            version: APPCHAIN_TX_VERSION,
            operation,
        };
        let transaction = appchain_request_to_transaction(&request).expect("well-formed request");
        hex::encode(UltraBlockchain::create_transaction_message_for(&transaction))
    }

    #[test]
    fn cli_digests_match_the_node_vectors() {
        assert_eq!(
            digest(0, APPCHAIN_REGISTER_FEE, AppChainOperation::Register { name: "UltraNet Grid".into() }),
            "7dcaf29f0809d289fdf6e10223ae79c6e7e1b319d45c51de8b17550756130f90"
        );
        assert_eq!(
            digest(1, APPCHAIN_ANCHOR_FEE, AppChainOperation::Anchor { chain_id: 1, state_root: "ab".repeat(32) }),
            "f1c64d0830b020aa34c0add54ae7815f61cbe0007e2b6795e844bbbfadd43a72"
        );
        assert_eq!(
            digest(
                2,
                APPCHAIN_TRANSFER_FEE,
                AppChainOperation::TransferOwner { chain_id: 1, new_owner: NEW_OWNER.into() }
            ),
            "702b1dd8dcdf04061511aea85a9d049843b3e2e531cca690b9176d50b8bcf9ab"
        );
    }

    fn encrypted(passphrase: &str) -> (KeyFile, Vec<u8>) {
        let pair = QuantumKeyPair::generate();
        let mut salt = [0u8; SALT_BYTES];
        let mut nonce = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce);
        let key = derive_key(passphrase, &salt).expect("derived key");
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let secret = cipher
            .encrypt(Nonce::from_slice(&nonce), pair.secret_key.as_slice())
            .expect("encrypted");
        (
            KeyFile {
                version: KEY_VERSION,
                address: pair.address(),
                public_key: hex::encode(&pair.public_key),
                kdf: KDF_LABEL.to_string(),
                cipher: CIPHER_LABEL.to_string(),
                salt: BASE64.encode(salt),
                nonce: BASE64.encode(nonce),
                secret_key: BASE64.encode(secret),
            },
            pair.secret_key.clone(),
        )
    }

    #[test]
    fn the_key_file_round_trips_and_rejects_the_wrong_passphrase() {
        let (file, secret) = encrypted("correct horse battery staple");
        let path = std::env::temp_dir().join(format!("appchain-cli-{}.enc", std::process::id()));
        let path = path.to_string_lossy().to_string();
        fs::write(&path, serde_json::to_string_pretty(&file).unwrap()).expect("written");

        // The right passphrase returns the same secret key, so the address is reproducible.
        let pass_path = std::env::temp_dir().join(format!("appchain-cli-{}.pass", std::process::id()));
        let pass_path = pass_path.to_string_lossy().to_string();
        fs::write(&pass_path, "correct horse battery staple\n").expect("written");
        let (loaded, recovered) = read_key(&path, Some(pass_path.clone())).expect("decrypted");
        assert_eq!(loaded.address, file.address);
        assert_eq!(QuantumKeyPair::address_from_public_key(&decoded(&loaded.public_key)), loaded.address);
        assert_eq!(recovered, secret);

        // The wrong passphrase fails rather than returning garbage.
        fs::write(&pass_path, "wrong passphrase\n").expect("written");
        let error = read_key(&path, Some(pass_path.clone())).expect_err("must not decrypt");
        assert!(error.contains("wrong passphrase"), "{error}");

        // And the file itself never carries the secret in the clear, in either encoding a reader
        // might look for. (Substring checks on single bytes would match the base64 by chance.)
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains(&hex::encode(&secret)), "secret key appears as hex");
        assert!(!on_disk.contains(&BASE64.encode(&secret)), "secret key appears as base64");
        // The tail, not the head: a Dilithium secret key starts with the same seed the public key
        // embeds, so a prefix match is expected and proves nothing about secrecy.
        let tail = &secret[secret.len() - 64..];
        assert!(!on_disk.contains(&hex::encode(tail)), "the private sector of the key appears");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&pass_path);
    }

    fn decoded(public_key_hex: &str) -> Vec<u8> {
        hex::decode(public_key_hex).expect("hex")
    }
}
