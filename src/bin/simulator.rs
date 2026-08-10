// src/bin/simulator.rs - Market Load Simulator for UltraNet v7.1
// Generates sharded load to stress test Block-STM and the 100-Year Longevity Engine.

use rand::Rng;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;

    let target_url = "http://127.0.0.1:8081/api/transaction";
    let mut rng = rand::thread_rng();

    println!("🚀 ULTRANET MARKET SIMULATOR: Genesis Phase");
    println!("📈 Target: Simulating 1,000,000 active user behavior patterns...");

    loop {
        // Generate random sharded transaction
        let sender = format!("user_{}", rng.gen_range(1..1000000));
        let recipient = format!("user_{}", rng.gen_range(1..1000000));
        let amount = rng.gen_range(1..500);

        let payload = json!({
            "sender": sender,
            "recipient": recipient,
            "amount": amount,
            "fee": (amount / 100) + 1,
            "_private_key": vec![0u8; 32]
        });

        // Fire and forget (don't wait for ZK generation to maximize load)
        let _ = client.post(target_url).json(&payload).send().await;

        if rng.gen_bool(0.01) {
            println!("🔥 MARKET LOAD: Submitted 100 sharded transfers...");
        }

        // Burst interval
        sleep(Duration::from_millis(50)).await;
    }
}
