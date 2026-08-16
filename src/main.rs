// src/main.rs - UltraNet Entry Point
use std::{env, io};
use UltraNet::{run_node, runtime_config, FheEngine, SharedStorage};

#[tokio::main]
async fn main() {
    if env::args()
        .skip(1)
        .any(|argument| argument == "--check-config")
    {
        if let Err(error) = runtime_config::check_config() {
            eprintln!("UltraNet configuration check failed: {error}");
            if runtime_config::pause_on_error() {
                pause_before_exit();
            }
            std::process::exit(1);
        }
        println!("UltraNet configuration is valid.");
        return;
    }

    if env::args()
        .skip(1)
        .any(|argument| argument == "--check-fhe")
    {
        if let Err(error) = check_fhe_initialization() {
            eprintln!("UltraNet FHE initialization check failed: {error}");
            if runtime_config::pause_on_error() {
                pause_before_exit();
            }
            std::process::exit(1);
        }
        println!("UltraNet FHE initialization is valid.");
        return;
    }

    if let Err(error) = run_node().await {
        eprintln!("UltraNet startup failed: {error}");
        if runtime_config::pause_on_error() {
            pause_before_exit();
        }
        std::process::exit(1);
    }
}

fn check_fhe_initialization() -> Result<(), String> {
    let runtime_config = runtime_config::prepare()?;
    let shared_storage =
        SharedStorage::new(&runtime_config.db_path.to_string_lossy()).map_err(|error| {
            format!(
                "Cannot open shared storage at {}: {error}",
                runtime_config.db_path.display()
            )
        })?;
    let _fhe_engine = FheEngine::new(shared_storage.fhe_keys.clone());
    Ok(())
}

fn pause_before_exit() {
    eprintln!("Press Enter to close this window.");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}
