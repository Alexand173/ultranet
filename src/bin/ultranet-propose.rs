//! Apply to become a validator, signed with the node's own validator identity.
//!
//! `POST /api/governance/propose` takes a version-2 envelope signed by the applicant's Dilithium-5
//! key. The Android app and the browser wallet can build one, but neither can sign with a key held on
//! a server — and the server's validator identity is exactly the key whose address should be paid. A
//! node with that identity had no way to apply. This is that way.
//!
//! The envelope is signed through `UltraBlockchain::create_transaction_message_for`, the same function
//! the node verifies with, so the digest cannot drift from what is accepted.
//!
//! Dry run by default. It prints what it would submit, and with `--check-file` writes the digest,
//! signature and public key for independent verification. `--send` submits.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use UltraNet::{
    validator_identity, ProofType, QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain,
};

const DEFAULT_NODE: &str = "http://127.0.0.1:8081";
const DEFAULT_METADATA: &str = "UltraNet validator";
/// The node rejects a timestamp more than an hour old, and its clock is the one that decides.
const SIGNING_MARGIN_SECONDS: u64 = 5;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("ultranet-propose: {error}");
        std::process::exit(1);
    }
}

struct Options {
    db_path: PathBuf,
    node: String,
    metadata: String,
    check_file: Option<PathBuf>,
    send: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut db_path: Option<PathBuf> = None;
    let mut node = DEFAULT_NODE.to_string();
    let mut metadata = DEFAULT_METADATA.to_string();
    let mut check_file = None;
    let mut send = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => {
                db_path = Some(PathBuf::from(value(args, &mut index)?));
            }
            "--node" => node = value(args, &mut index)?,
            "--metadata" => metadata = value(args, &mut index)?,
            "--check-file" => check_file = Some(PathBuf::from(value(args, &mut index)?)),
            "--send" => send = true,
            other => {
                return Err(format!(
                    "unknown argument {other}; use --db <path> --node <url> --metadata <text> \
                     --check-file <path> --send"
                ))
            }
        }
        index += 1;
    }

    let db_path = match db_path {
        Some(path) => path,
        None => env::var("ULTRANET_DB_PATH")
            .map(PathBuf::from)
            .map_err(|_| {
                "no database path: pass --db <path> or set ULTRANET_DB_PATH".to_string()
            })?,
    };
    if metadata.trim().is_empty() || metadata.len() > 128 {
        return Err("metadata must be 1 to 128 characters".to_string());
    }

    Ok(Options {
        db_path,
        node,
        metadata,
        check_file,
        send,
    })
}

fn value(args: &[String], index: &mut usize) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{} needs a value", args[*index - 1]))
}

async fn run() -> Result<(), String> {
    let options = parse_options(&env::args().skip(1).collect::<Vec<_>>())?;
    let client = reqwest::Client::new();

    let keypair = validator_identity::load_keypair(&options.db_path)?;
    let address = keypair.address();
    let payout = UltraNet::validator_payout_address(&keypair.public_key);

    let nonce = account_nonce(&client, &options.node, &address).await?;
    let timestamp = node_timestamp(&client, &options.node).await?;

    let mut nullifier = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nullifier);

    let mut tx = Transaction {
        sender: address.clone(),
        sender_public_key: keypair.public_key.clone(),
        recipient: "0x0".to_string(),
        amount: 0,
        signature: Vec::new(),
        zk_proof: Vec::new(),
        nullifier,
        timestamp,
        fee: 0,
        nonce,
        gas_limit: 1_000_000,
        gas_price: 1,
        proof_type: ProofType::Ownership,
        payload: TransactionPayload::ValidatorJoinProposal {
            public_key: keypair.public_key.clone(),
            metadata: options.metadata.clone(),
        },
        chain_id: UltraBlockchain::L1_CHAIN_ID,
        version: UltraBlockchain::PAYLOAD_BOUND_TRANSACTION_VERSION,
    };

    let message = UltraBlockchain::create_transaction_message_for(&tx);
    let signature = keypair.sign(&message);
    if !QuantumKeyPair::verify(&keypair.public_key, &message, &signature) {
        return Err(
            "the signature does not verify against this key; refusing to submit".to_string(),
        );
    }
    tx.signature = signature;

    let body = serde_json::json!({
        "sender": tx.sender,
        "sender_public_key": tx.sender_public_key,
        "proposal_public_key": tx.sender_public_key,
        "metadata": options.metadata,
        "nonce": tx.nonce,
        "timestamp": tx.timestamp,
        "nullifier": tx.nullifier.to_vec(),
        "signature": tx.signature,
        "version": tx.version,
    });

    println!("applicant address : {address}");
    println!("payout address    : {payout}");
    println!("nonce             : {nonce}");
    println!("timestamp         : {timestamp}");
    println!(
        "envelope digest   : {}",
        hex::encode(UltraBlockchain::create_transaction_message_for(&tx))
    );

    if let Some(path) = &options.check_file {
        let artifact = serde_json::json!({
            "address": address,
            "payout_address": payout,
            "public_key": hex::encode(&keypair.public_key),
            "digest": hex::encode(UltraBlockchain::create_transaction_message_for(&tx)),
            "signature": hex::encode(&tx.signature),
            "nonce": tx.nonce,
            "timestamp": tx.timestamp,
            "version": tx.version,
        });
        write_artifact(path, &artifact)?;
        println!("artefact          : {}", path.display());
    }

    if !options.send {
        println!("\ndry run — nothing submitted. Request body:\n{body}");
        println!("\nre-run with --send to submit to {}", options.node);
        return Ok(());
    }

    let response = client
        .post(format!("{}/api/governance/propose", options.node))
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("unable to reach the node: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("unreadable node response: {error}"))?;
    if !status.is_success() {
        return Err(format!("the node refused the proposal ({status}): {text}"));
    }
    println!("\nproposal submitted:\n{text}");
    println!(
        "\nnext: the proposal must be approved by 2 of the 3 sovereign owners before the validator is \
         active or paid."
    );
    Ok(())
}

async fn account_nonce(client: &reqwest::Client, node: &str, address: &str) -> Result<u64, String> {
    let response = client
        .get(format!("{node}/api/account/{address}"))
        .send()
        .await
        .map_err(|error| format!("unable to read the account: {error}"))?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("unreadable account response: {error}"))?;
    body["data"]["nonce"]
        .as_u64()
        .ok_or_else(|| format!("the node returned no nonce for {address}"))
}

/// The node's clock, not this machine's: a signature timestamp from a skewed host is rejected.
async fn node_timestamp(client: &reqwest::Client, node: &str) -> Result<u64, String> {
    let response = client
        .get(format!("{node}/api/appchain/overview"))
        .send()
        .await;
    if let Ok(response) = response {
        if let Ok(body) = response.json::<serde_json::Value>().await {
            if let Some(updated_at) = body["data"]["updated_at"].as_u64() {
                return Ok(updated_at.saturating_sub(SIGNING_MARGIN_SECONDS));
            }
        }
    }

    let local = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("the local clock is unreadable: {error}"))?
        .as_secs();
    eprintln!(
        "warning: the node's clock could not be read; signing with the local clock ({local}). \
         If the node is behind, the proposal will be rejected as too old."
    );
    Ok(local.saturating_sub(SIGNING_MARGIN_SECONDS))
}

fn write_artifact(path: &Path, artifact: &serde_json::Value) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "{} already exists; remove it rather than overwriting a signed artefact",
            path.display()
        ));
    }
    let serialized = serde_json::to_string_pretty(artifact)
        .map_err(|error| format!("unable to encode the artefact: {error}"))?;
    fs::write(path, &serialized).map_err(|error| {
        format!(
            "unable to write {} ({error}); a signing artefact contains a signature, never a secret",
            path.display()
        )
    })
}
