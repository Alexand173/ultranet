// src/main.rs - UltraNet Entry Point
use UltraNet::run_node;

#[tokio::main]
async fn main() -> Result<(), String> {
    run_node().await
}
