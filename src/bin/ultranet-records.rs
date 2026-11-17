//! `ultranet-records` — the Records Service: third-party games and every other application that anchors
//! records. It holds the chain owner's key (through `appchain-cli`) and **no payout key**; the Games Reward
//! Service (`ultranet-games`) holds that, and asks this service to anchor first-party game days.
//!
//! ```text
//! ultranet-records serve            run the API and the daily close-and-anchor
//! ultranet-records check-config     validate the environment and credentials; no network, no listener
//! ultranet-records status           the switch and the registered games
//! ultranet-records disable --reason "..."      stop accepting records and peer anchors
//! ultranet-records enable
//! ultranet-records backup --to /var/backups/ultranet-records/records-YYYYMMDD.db
//! ultranet-records close-day --date YYYY-MM-DD                          # close (and anchor) a registry day
//!
//! ultranet-records game-register --id tetris --name "Tetris" --owner <64 hex>    # prints the API key ONCE
//! ultranet-records game-list
//! ultranet-records game-status --id tetris --status sandbox|registered|listed|suspended
//! ultranet-records game-rotate-key --id tetris                                   # prints the new key ONCE
//! ```
//!
//! Only `serve`, `check-config` and `close-day` read credentials or touch the network. The rest open the
//! database and nothing else.

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::watch;
use zeroize::Zeroizing;
use UltraNet::games::{
    anchor::{Anchorer, CliAnchorer},
    chain::HttpChain,
    peer::{PeerVerifier, MIN_SECRET_BYTES},
    records::RecordsService,
    records_api,
    records_config::RecordsServiceConfig,
    service::system_clock,
    store::GamesStore,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    let result = match command {
        "serve" => serve().await,
        "check-config" => check_config(),
        "status" => status(),
        "disable" => set_accepting(&args[1..], false),
        "enable" => set_accepting(&args[1..], true),
        "backup" => backup(&args[1..]),
        "close-day" => close_day(&args[1..]).await,
        "game-register" => game_register(&args[1..]),
        "game-list" => game_list(),
        "game-status" => game_status(&args[1..]),
        "game-rotate-key" => game_rotate_key(&args[1..]),
        _ => Err("use serve, check-config, status, disable, enable, backup, close-day, game-register, game-list, game-status or game-rotate-key".into()),
    };
    if let Err(error) = result {
        eprintln!("ultranet-records: {error}");
        std::process::exit(1);
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The database path without the full configuration, so inspecting a service never needs its secrets.
fn db_path() -> PathBuf {
    PathBuf::from(
        env::var("RECORDS_DB_PATH")
            .unwrap_or_else(|_| "/var/lib/ultranet-records/records.db".into()),
    )
}

fn open_store() -> Result<GamesStore, String> {
    let path = db_path();
    if !path.exists() {
        return Err(format!(
            "no database at {}; set RECORDS_DB_PATH",
            path.display()
        ));
    }
    GamesStore::open(&path).map_err(|e| e.to_string())
}

fn read_secret(path: &Path, minimum: usize) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut bytes =
        fs::read(path).map_err(|e| format!("cannot read credential {}: {e}", path.display()))?;
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if bytes.len() < minimum {
        return Err(format!(
            "credential {} is too short (needs {minimum} bytes)",
            path.display()
        ));
    }
    Ok(Zeroizing::new(bytes))
}

/// The Records Service must not hold the payout key. Refusing to start when one is in its credentials
/// directory turns a separation of duties from a convention into a check.
fn refuse_foreign_credentials(config: &RecordsServiceConfig) -> Result<(), String> {
    let present = config.foreign_credentials_present();
    if present.is_empty() {
        return Ok(());
    }
    Err(format!(
        "this service must not hold the games payout credentials, but found: {}. Those belong to ultranet-games; remove them from this unit's credentials",
        present.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    ))
}

fn build_service(config: &RecordsServiceConfig) -> Result<Arc<RecordsService>, String> {
    refuse_foreign_credentials(config)?;
    let store = GamesStore::open(&config.db_path).map_err(|e| e.to_string())?;
    let chain = Arc::new(HttpChain::new(&config.node_api_base_url).map_err(|e| e.to_string())?);
    let anchorer: Option<Arc<dyn Anchorer>> = config.anchor.as_ref().map(|anchor| {
        Arc::new(CliAnchorer {
            cli_path: anchor.cli_path.clone(),
            key_file: config.credential_path(&anchor.key_credential),
            password_file: config.credential_path(&anchor.password_credential),
            node_url: config.node_api_base_url.clone(),
            timeout: Duration::from_secs(anchor.timeout_seconds),
        }) as Arc<dyn Anchorer>
    });
    let peer = match &config.peer_credential {
        None => None,
        Some(name) => {
            let secret = read_secret(&config.credential_path(name), MIN_SECRET_BYTES)?;
            Some(PeerVerifier::new(secret.to_vec(), "games")?)
        }
    };
    Ok(Arc::new(RecordsService::new(
        store,
        config.caps,
        config.service.clone(),
        chain,
        anchorer,
        peer,
        system_clock(),
    )))
}

async fn serve() -> Result<(), String> {
    let config = RecordsServiceConfig::from_env()?;
    let service = build_service(&config)?;
    if config.anchor.is_none() {
        tracing::warn!("RECORDS_ANCHOR_CLI is not set: nothing will be anchored");
    }
    if config.service.registry_chain_id.is_none() {
        tracing::warn!("RECORDS_REGISTRY_CHAIN_ID is not set: registry days are closed but not anchored");
    }
    if config.peer_credential.is_none() {
        tracing::warn!("RECORDS_PEER_CREDENTIAL is not set: the games service cannot have its days anchored");
    }
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(service.clone().run_worker(shutdown_rx.clone()));
    tracing::info!(bind = %config.bind, "ultranet-records listening");
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| e.to_string())?;
    let result = tokio::select! {
        result = records_api::run_records_server(service, config.bind, shutdown_rx) => result.map_err(|e| e.to_string()),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = terminate.recv() => Ok(()),
    };
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(20), worker).await;
    result
}

fn check_config() -> Result<(), String> {
    let config = RecordsServiceConfig::from_env()?;
    println!("UltraNet records configuration is valid.");
    println!("Bind address:      {}", config.bind);
    println!("Database:          {}", config.db_path.display());
    println!("Node API:          {}", config.node_api_base_url);
    println!(
        "Registry chain:    {}",
        config.service.registry_chain_id.map_or("not set (days closed, not anchored)".to_string(), |id| format!("#{id}"))
    );
    println!("Peer may anchor:   chains {:?}", config.service.peer_anchor_chains);
    println!(
        "Limits per day:    {} per game, {} per player, {} per sandbox; {} games per owner, {} self-registrations",
        config.caps.results_per_game_per_day,
        config.caps.results_per_player_per_game_per_day,
        config.caps.sandbox_results_per_game_per_day,
        config.caps.games_per_owner,
        config.caps.self_registrations_per_day
    );
    refuse_foreign_credentials(&config)?;
    println!("Payout key:        not held here (correct)");
    match &config.anchor {
        None => println!("Anchoring:         DISABLED (RECORDS_ANCHOR_CLI not set)"),
        Some(anchor) => {
            if !anchor.cli_path.exists() {
                return Err(format!("RECORDS_ANCHOR_CLI {} does not exist", anchor.cli_path.display()));
            }
            for name in [&anchor.key_credential, &anchor.password_credential] {
                let path = config.credential_path(name);
                if !path.exists() {
                    return Err(format!("anchor credential {} is missing", path.display()));
                }
            }
            println!("Anchoring:         enabled via {}", anchor.cli_path.display());
        }
    }
    match &config.peer_credential {
        None => println!("Peer route:        DISABLED (RECORDS_PEER_CREDENTIAL not set)"),
        Some(name) => {
            read_secret(&config.credential_path(name), MIN_SECRET_BYTES)?;
            println!("Peer route:        enabled, secret present");
        }
    }
    Ok(())
}

fn status() -> Result<(), String> {
    let store = open_store()?;
    let state = store.state().map_err(|e| e.to_string())?;
    println!("accepting records: {}", state.accepting);
    if let Some(reason) = &state.reason {
        println!("reason: {reason}");
    }
    game_list()
}

fn set_accepting(args: &[String], value: bool) -> Result<(), String> {
    let store = open_store()?;
    let reason = if value {
        None
    } else {
        Some(flag(args, "--reason").ok_or("disabling needs --reason \"why\" so the next person knows")?)
    };
    store
        .set_state(Some(value), None, reason.as_deref(), UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    println!("accepting records: {}", store.state().map_err(|e| e.to_string())?.accepting);
    Ok(())
}

/// A consistent snapshot of the database, readable only by its owner. Works while the service runs.
fn backup(args: &[String]) -> Result<(), String> {
    let destination = PathBuf::from(flag(args, "--to").ok_or("--to <new file> is required")?);
    let store = open_store()?;
    store.backup_to(&destination).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    // Open the copy: a backup nobody has opened is a hope, not a backup.
    let copy = GamesStore::open(&destination).map_err(|e| e.to_string())?;
    let games = copy.games().map_err(|e| e.to_string())?.len();
    println!("backup written to {} (mode 0600); the copy opens and lists {games} game(s)", destination.display());
    Ok(())
}

/// Create a third-party game in the sandbox. The key is printed here and never stored: lose it and the
/// only remedy is `game-rotate-key`. Only the database is opened.
fn game_register(args: &[String]) -> Result<(), String> {
    let id = flag(args, "--id").ok_or("--id is required")?;
    let name = flag(args, "--name").ok_or("--name is required")?;
    let owner = flag(args, "--owner").ok_or("--owner <64 hex address> is required")?;
    let store = open_store()?;
    let (game, key) = store
        .register_game(&id, &name, &owner, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    println!(
        "registered {} ({}) for {} in the {} state",
        game.id,
        game.name,
        game.owner,
        game.status.code()
    );
    println!();
    println!("API KEY (shown once, store it now): {key}");
    println!();
    println!(
        "Results go to POST /api/games/g/{}/results with `Authorization: Bearer <key>`.",
        game.id
    );
    println!("A sandbox game is private and never anchored: promote it with `game-status --status registered`.");
    Ok(())
}

fn game_list() -> Result<(), String> {
    let games = open_store()?.games().map_err(|e| e.to_string())?;
    if games.is_empty() {
        println!("no registered games");
    }
    for game in games {
        println!(
            "{:<32} {:<11} {}  {}",
            game.id,
            game.status.code(),
            game.owner,
            game.name
        );
    }
    Ok(())
}

fn game_status(args: &[String]) -> Result<(), String> {
    use UltraNet::games::registry::GameStatus;
    let id = flag(args, "--id").ok_or("--id is required")?;
    let status = flag(args, "--status")
        .and_then(|value| GameStatus::parse(&value))
        .ok_or("--status must be sandbox, registered, listed or suspended")?;
    open_store()?
        .set_game_status(&id, status, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    println!("{id} is now {}", status.code());
    if status == GameStatus::Suspended {
        println!("It takes no new results. Days already closed and anchored are unchanged.");
    }
    Ok(())
}

fn game_rotate_key(args: &[String]) -> Result<(), String> {
    let id = flag(args, "--id").ok_or("--id is required")?;
    let key = open_store()?
        .rotate_key(&id, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    println!("the old key for {id} no longer works");
    println!();
    println!("NEW API KEY (shown once, store it now): {key}");
    Ok(())
}

/// Close a finished day and anchor it if the configuration has a registry chain. It loads the full
/// configuration and the unit's credentials, so it is not a plain-shell command.
async fn close_day(args: &[String]) -> Result<(), String> {
    let date = flag(args, "--date").ok_or("--date YYYY-MM-DD is required")?;
    let config = RecordsServiceConfig::from_env()?;
    let service = build_service(&config)?;
    let day = service
        .close_registry_day(&date)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "registry {} -> {} ({} game(s))",
        day.date,
        day.state,
        day.games.len()
    );
    println!("merkle root: {}", hex::encode(day.merkle_root));
    println!("day root:    {}", hex::encode(day.day_root));
    if let Some(tx) = &day.anchor_tx {
        println!("anchor tx:   {tx}");
    }
    if let Some(error) = &day.last_error {
        println!("last error:  {error}");
    }
    Ok(())
}

