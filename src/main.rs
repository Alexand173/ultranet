// src/main.rs - UltraNet Entry Point
use std::{env, io, path::PathBuf};
use UltraNet::{run_node, runtime_config, validator_identity, FheEngine, SharedStorage};

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();

    if arguments
        .first()
        .map(|argument| argument == "--export-validator-public-key")
        .unwrap_or(false)
    {
        if let Err(error) = export_validator_public_key(&arguments) {
            eprintln!("UltraNet validator public-key export failed: {error}");
            if runtime_config::pause_on_error() {
                pause_before_exit();
            }
            std::process::exit(1);
        }
        return;
    }

    if arguments
        .iter()
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

    if arguments.iter().any(|argument| argument == "--check-fhe") {
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

fn export_validator_public_key(arguments: &[String]) -> Result<(), String> {
    if arguments.len() > 2 {
        return Err("Usage: UltraNetNode --export-validator-public-key [output-path]".to_string());
    }

    let runtime_config = runtime_config::prepare()?;
    let output_path = arguments.get(1).map(PathBuf::from);
    let exported =
        validator_identity::export_public_key(&runtime_config.db_path, output_path.as_deref())?;

    println!("UltraNet validator public key exported.");
    println!("Public key file: {}", exported.path.display());
    println!("Validator address: {}", exported.address);
    println!("The file contains only the public key. Keep the node data directory private because it stores the corresponding secret key.");
    Ok(())
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
