//! The developer's side of "add a game": prove you own an address, get a game id and an API key.
//!
//! ```text
//! ultranet-game-dev address     --key owner.json
//! ultranet-game-dev register    --key owner.json --id my-game --name "My Game" [--api <base url>]
//! ultranet-game-dev rotate-key  --key owner.json --id my-game                    [--api <base url>]
//! ```
//!
//! `owner.json` is the wallet key file (`{"public_key":"<hex>","secret_key":"<hex>"}`, as written by
//! `ultranet-faucet keygen --output owner.json`). The secret key never leaves this machine: it signs a
//! short message locally, and only the public key and the signature are sent. The address that owns the
//! game is the hash of the public key, so the game can only ever be owned by the key that signed.
//! The API key the server returns is printed once; save it, because only its hash is kept.

use std::{env, fs, path::Path, time::Duration};
use zeroize::Zeroizing;
use UltraNet::games::ownership::{proof_digest, ACTION_REGISTER, ACTION_ROTATE_KEY};
use UltraNet::QuantumKeyPair;

const DEFAULT_API: &str = "https://api.ultranetwork.cc";

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("address") => address(&args[1..]),
        Some("register") => register(&args[1..]).await,
        Some("rotate-key") => rotate(&args[1..]).await,
        _ => Err("use address, register or rotate-key (see the header of this file)".into()),
    };
    if let Err(error) = result {
        eprintln!("ultranet-game-dev: {error}");
        std::process::exit(1);
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn load_key(args: &[String]) -> Result<QuantumKeyPair, String> {
    let path = flag(args, "--key").ok_or("--key <owner.json> is required")?;
    let text = Zeroizing::new(
        fs::read_to_string(Path::new(&path)).map_err(|e| format!("cannot read {path}: {e}"))?,
    );
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path} is not valid JSON: {e}"))?;
    let field = |name: &str| -> Result<Vec<u8>, String> {
        let hex_text = value[name]
            .as_str()
            .ok_or_else(|| format!("{path} has no \"{name}\""))?;
        hex::decode(hex_text).map_err(|_| format!("\"{name}\" in {path} is not hex"))
    };
    let pair = QuantumKeyPair {
        public_key: field("public_key")?,
        secret_key: field("secret_key")?,
        key_id: [0; 32],
        created_at: 0,
        version: 1,
    };
    // Prove the pair is whole before trusting it: a mismatched key would sign proofs nobody accepts.
    let probe = pair.sign(b"ultranet-game-dev self-check");
    if !QuantumKeyPair::verify(&pair.public_key, b"ultranet-game-dev self-check", &probe) {
        return Err(format!("the public and secret keys in {path} do not match"));
    }
    Ok(pair)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn address(args: &[String]) -> Result<(), String> {
    println!("{}", load_key(args)?.address());
    Ok(())
}

async fn post(url: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("cannot reach {url}: {e}"))?;
    let status = response.status();
    let reply: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let message = reply["error"]["message"]
            .as_str()
            .or_else(|| reply["message"].as_str())
            .unwrap_or("no message");
        let code = reply["error"]["code"]
            .as_str()
            .or_else(|| reply["code"].as_str())
            .unwrap_or("error");
        return Err(format!("the server refused it ({status}, {code}): {message}"));
    }
    Ok(reply)
}

fn print_result(reply: &serde_json::Value) {
    println!("game:    {}", reply["game"]["id"].as_str().unwrap_or("?"));
    println!("owner:   {}", reply["game"]["owner"].as_str().unwrap_or("?"));
    println!("status:  {}", reply["game"]["status"].as_str().unwrap_or("?"));
    println!();
    println!("API key (shown once; keep it on your server, never in an app or web page):");
    println!("{}", reply["api_key"].as_str().unwrap_or("?"));
}

async fn register(args: &[String]) -> Result<(), String> {
    let pair = load_key(args)?;
    let id = flag(args, "--id").ok_or("--id is required")?;
    let name = flag(args, "--name").ok_or("--name is required")?.trim().to_string();
    let api = flag(args, "--api").unwrap_or_else(|| DEFAULT_API.into());
    let timestamp = now();
    let signature = pair.sign(&proof_digest(ACTION_REGISTER, &id, &name, timestamp));
    let reply = post(
        &format!("{}/api/games/g", api.trim_end_matches('/')),
        serde_json::json!({
            "id": id, "name": name, "public_key": hex::encode(&pair.public_key),
            "timestamp": timestamp, "signature": hex::encode(signature),
        }),
    )
    .await?;
    print_result(&reply);
    println!("\nYour game starts in the sandbox: private, unanchored. The operator promotes it.");
    Ok(())
}

async fn rotate(args: &[String]) -> Result<(), String> {
    let pair = load_key(args)?;
    let id = flag(args, "--id").ok_or("--id is required")?;
    let api = flag(args, "--api").unwrap_or_else(|| DEFAULT_API.into());
    let timestamp = now();
    let signature = pair.sign(&proof_digest(ACTION_ROTATE_KEY, &id, "", timestamp));
    let reply = post(
        &format!("{}/api/games/g/{id}/rotate-key", api.trim_end_matches('/')),
        serde_json::json!({
            "public_key": hex::encode(&pair.public_key),
            "timestamp": timestamp, "signature": hex::encode(signature),
        }),
    )
    .await?;
    print_result(&reply);
    println!("\nThe old key no longer works.");
    Ok(())
}
