use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;
use zeroize::Zeroizing;
use UltraNet::faucet::{
    api, captcha::TurnstileVerifier, config::FaucetConfig, node_client::NodeClient, preview,
    service::FaucetService, signer::FaucetSigner, store::FaucetStore,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    let result = match command {
        "serve" if args.len() == 1 => serve().await,
        "check-config" if args.len() == 1 => check_config(),
        "preview" if args.len() == 1 => preview::run(preview_bind()).await,
        "keygen" => keygen(&args[1..]),
        _ => Err(format!(
            "invalid command; use serve, check-config, preview, or keygen --output <path>"
        )),
    };
    if let Err(error) = result {
        eprintln!("ultranet-faucet: {error}");
        std::process::exit(1);
    }
}

fn preview_bind() -> SocketAddr {
    env::var("FAUCET_PREVIEW_BIND")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .filter(|address| address.ip().is_loopback())
        .unwrap_or_else(|| "127.0.0.1:8090".parse().unwrap())
}

fn keygen(args: &[String]) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--output" {
        return Err("keygen requires exactly --output <path>".into());
    }
    let output = PathBuf::from(&args[1]);
    if output
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "sovereign_keys.json")
    {
        return Err("refusing to create a credential named sovereign_keys.json".into());
    }
    if output.exists() {
        return Err(format!("refusing to overwrite {}", output.display()));
    }
    let keypair = UltraNet::QuantumKeyPair::generate();
    let public_key = hex::encode(&keypair.public_key);
    let secret_key = Zeroizing::new(hex::encode(&keypair.secret_key));
    let contents = Zeroizing::new(format!(
        "{{\"public_key\":\"{public_key}\",\"secret_key\":\"{}\"}}\n",
        secret_key.as_str()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&output).map_err(|error| {
        format!(
            "cannot create signer credential {}: {error}",
            output.display()
        )
    })?;
    file.write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            format!(
                "cannot write signer credential {}: {error}",
                output.display()
            )
        })?;
    println!("Generated dedicated Dilithium-5 signer credential.");
    println!("Address: {}", keypair.address());
    println!("Credential path: {}", output.display());
    Ok(())
}

fn check_config() -> Result<(), String> {
    let config = FaucetConfig::from_env()?;
    println!("UltraNet faucet configuration is valid.");
    println!("Bind address: {}", config.bind);
    println!("Node API: {}", config.node_api_base_url);
    println!("Faucet address: {}", config.faucet_address);
    println!(
        "Claim amount: {} base units",
        config.claim_amount_base_units
    );
    println!("Intake enabled: {}", config.enabled);
    Ok(())
}

async fn serve() -> Result<(), String> {
    let config = FaucetConfig::from_env()?;
    let store = FaucetStore::open(&config.state_path).map_err(|error| error.to_string())?;
    let signer_path = config.credential_path(&config.signer_credential);
    let signer = Arc::new(
        FaucetSigner::load(&signer_path, &config.faucet_address)
            .map_err(|error| error.to_string())?,
    );
    let turnstile_secret =
        read_secret(&config.credential_path(&config.turnstile_secret_credential))?;
    let captcha = Arc::new(
        TurnstileVerifier::new(secret_string(turnstile_secret)?)
            .map_err(|error| error.to_string())?,
    );
    let abuse_key = read_secret(&config.credential_path(&config.abuse_key_credential))?;
    let operator_token = read_secret(&config.credential_path(&config.operator_token_credential))?;
    let node = Arc::new(
        NodeClient::new(config.node_api_base_url.clone()).map_err(|error| error.to_string())?,
    );
    let service = Arc::new(
        FaucetService::new(
            config,
            store,
            signer,
            node,
            captcha,
            abuse_key,
            operator_token,
        )
        .map_err(|error| error.to_string())?,
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(service.clone().run_worker(shutdown_rx.clone()));
    let server_result = tokio::select! {
        result = api::run_server(service, shutdown_rx) => result.map_err(|error| error.to_string()),
        result = tokio::signal::ctrl_c() => {
            result.map_err(|error| error.to_string())?;
            shutdown_tx.send(true).map_err(|_| "faucet worker already stopped".to_string())?;
            Ok(())
        }
    };
    let _ = shutdown_tx.send(true);
    worker.abort();
    server_result
}

fn read_secret(path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut bytes = fs::read(path)
        .map_err(|error| format!("cannot read faucet credential {}: {error}", path.display()))?;
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if bytes.len() < 16 {
        return Err(format!("faucet credential {} is too short", path.display()));
    }
    Ok(Zeroizing::new(bytes))
}

fn secret_string(secret: Zeroizing<Vec<u8>>) -> Result<String, String> {
    String::from_utf8(secret.to_vec()).map_err(|_| "Turnstile credential must be UTF-8".into())
}
