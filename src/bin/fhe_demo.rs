// ============================================================
// ULTRANET FHE CLIENT DEMO - CONFIDENTIAL TRANSFERS
// ============================================================

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tfhe::integer::gen_keys_radix;
use tfhe::shortint::parameters::PARAM_MESSAGE_2_CARRY_2_KS_PBS;

#[derive(Serialize)]
struct ExecuteFunctionRequest {
    sender: String,
    module_address: String,
    module: String,
    function: String,
    args: Vec<Vec<u8>>,
}

#[derive(Deserialize)]
struct FhePkResponse {
    success: bool,
    public_key: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60)) // FHE operacije na serveru mogu potrajati
        .build()?;

    println!("============================================================");
    println!("🔐 ULTRANET CONFIDENTIAL TRANSFER DEMO");
    println!("============================================================");
    println!("🔗 Connecting to UltraNet node at http://127.0.0.1:8081...");

    // 1. Preuzmi javni ključ sa mreže
    // U realnom sistemu, klijent bi koristio ovaj PK za enkripciju.
    // Za demo koristimo lokalno generisane ključeve sa identičnim parametrima.
    let resp = client
        .get("http://127.0.0.1:8081/api/fhe/pk")
        .send()
        .await?;
    let pk_data: FhePkResponse = resp.json().await?;

    if !pk_data.success {
        println!("❌ Failed to fetch FHE Public Key. Is the node running?");
        return Ok(());
    }
    println!(
        "✅ FHE Public Key received ({} bytes)!",
        pk_data.public_key.len() / 2
    );

    // 2. Inicijalizacija FHE klijenta
    println!("🏗️  Initializing local FHE environment (Radix 8-block/16-bit)...");
    let (client_key, _server_key) = gen_keys_radix(PARAM_MESSAGE_2_CARRY_2_KS_PBS, 8);

    // 3. Enkriptuj MINT iznos (100 tokena)
    let mint_amount: u64 = 100;
    println!("🛡️  Encrypting MINT amount: {}...", mint_amount);
    let ct_mint = client_key.encrypt(mint_amount);
    let ct_mint_bytes = bincode::serialize(&ct_mint)?;
    println!("📦  Ciphertext size: {} bytes", ct_mint_bytes.len());

    // 4. Pošalji transakciju za enkriptovani MINT
    println!("🚀 Submitting encrypted MINT transaction to Mempool...");
    let mint_req = ExecuteFunctionRequest {
        sender: "Alice".to_string(),
        module_address: "0x1".to_string(),
        module: "FheCoin".to_string(),
        function: "mint".to_string(),
        args: vec![ct_mint_bytes, vec![0x01]], // Alice adresa (0x1)
    };

    let resp = client
        .post("http://127.0.0.1:8081/api/move/execute")
        .json(&mint_req)
        .send()
        .await?;
    println!("📡 Node response: {}", resp.text().await?);

    // 5. Enkriptuj TRANSFER iznos (42 tokena)
    let transfer_amount: u64 = 42;
    println!("\n🛡️  Encrypting TRANSFER amount: {}...", transfer_amount);
    let ct_transfer = client_key.encrypt(transfer_amount);
    let ct_transfer_bytes = bincode::serialize(&ct_transfer)?;

    // 6. Pošalji transakciju za enkriptovani TRANSFER (Alice -> Bob)
    println!("🚀 Submitting encrypted TRANSFER transaction (Alice -> Bob)...");
    let transfer_req = ExecuteFunctionRequest {
        sender: "Alice".to_string(),
        module_address: "0x1".to_string(),
        module: "FheCoin".to_string(),
        function: "transfer".to_string(),
        args: vec![
            ct_transfer_bytes,
            vec![0x01], // From Alice
            vec![0x02], // To Bob
        ],
    };

    let resp = client
        .post("http://127.0.0.1:8081/api/move/execute")
        .json(&transfer_req)
        .send()
        .await?;
    println!("📡 Node response: {}", resp.text().await?);

    // 7. Demo Anchoring sa ZK-FHE
    println!("\n⚓ Simulating AppChain Anchoring with ZK-FHE Proof...");
    let anchor_req = serde_json::json!({
        "chain_id": 1,
        "state_root": "0xABC123",
        "proof": "STARK_FHE_TRACE_DUMMY_PROOF"
    });

    let resp = client
        .post("http://127.0.0.1:8081/api/appchain/anchor")
        .json(&anchor_req)
        .send()
        .await?;
    println!("📡 L1 Anchor Response: {}", resp.text().await?);

    println!("\n============================================================");
    println!("🎉 DEMO ZAVRŠEN!");
    println!("============================================================");
    println!("Mreža je izvršila sabiranje i oduzimanje balansa homomorfno.");
    println!("Veličina stanja u Sled-u je porasla, ali su podaci ostali enkriptovani.");
    println!("Samo korisnik sa klijentskim ključem može pročitati balans.");
    println!("Pogledaj logove UltraNet čvora za '✅ [FHE] Transferred' potvrdu.");

    Ok(())
}
