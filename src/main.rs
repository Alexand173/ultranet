// src/main.rs - UltraNet Entry Point
use std::{env, io};
use UltraNet::{run_node, runtime_config};

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

    if let Err(error) = run_node().await {
        eprintln!("UltraNet startup failed: {error}");
        if runtime_config::pause_on_error() {
            pause_before_exit();
        }
        std::process::exit(1);
    }
}

fn pause_before_exit() {
    eprintln!("Press Enter to close this window.");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}
