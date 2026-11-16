//! `ultranet-games` — the games service and the operator's commands for it.
//!
//! ```text
//! ultranet-games serve            run the API, the payout worker and the daily anchor
//! ultranet-games check-config     validate the environment and credentials; no network, no listener
//! ultranet-games status           the state, the totals, the open payouts and the recent days
//! ultranet-games audit            re-derive every balance; exits 1 on any disagreement
//! ultranet-games disable [--accepting] [--paying] --reason "..."     (no flag = both)
//! ultranet-games enable  [--accepting] [--paying]                    (no flag = both)
//! ultranet-games payouts          list unfinished payouts
//! ultranet-games payout-resolve --id <id> (--confirmed | --failed) --note "..."
//! ultranet-games close-day --game grid|chess --date YYYY-MM-DD
//! ultranet-games backup --to /var/backups/ultranet-games/games-YYYYMMDD.db
//!
//! ultranet-games game-register --id tetris --name "Tetris" --owner <64 hex>    # prints the API key ONCE
//! ultranet-games game-list
//! ultranet-games game-status --id tetris --status sandbox|registered|listed|suspended
//! ultranet-games game-rotate-key --id tetris                                   # prints the new key ONCE
//! ultranet-games registry-close-day --date YYYY-MM-DD                          # close (and anchor) a registry day
//! ```
//!
//! Only `serve`, `check-config` and `close-day` read credentials or touch the network. The rest open
//! the database and nothing else, so an operator can inspect or pause a service that is misbehaving
//! without giving the tool any key.

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::watch;
use zeroize::Zeroizing;
use UltraNet::{
    faucet::{node_client::NodeClient, signer::FaucetSigner},
    games::{
        anchor::{Anchorer, CliAnchorer},
        api,
        chain::HttpChain,
        config::GamesConfig,
        rules::format_units,
        service::{system_clock, GamesService},
        store::GamesStore,
    },
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
        "audit" => audit(),
        "disable" => set_switches(&args[1..], false),
        "enable" => set_switches(&args[1..], true),
        "payouts" => payouts(),
        "payout-resolve" => payout_resolve(&args[1..]),
        "close-day" => close_day(&args[1..]).await,
        "backup" => backup(&args[1..]),
        "game-register" => game_register(&args[1..]),
        "game-list" => game_list(),
        "game-status" => game_status(&args[1..]),
        "game-rotate-key" => game_rotate_key(&args[1..]),
        "registry-close-day" => registry_close_day(&args[1..]).await,
        _ => Err("use serve, check-config, status, audit, disable, enable, payouts, payout-resolve, close-day, backup, game-register, game-list, game-status, game-rotate-key or registry-close-day".into()),
    };
    if let Err(error) = result {
        eprintln!("ultranet-games: {error}");
        std::process::exit(1);
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// The database path without the full configuration, so inspecting a service never needs its secrets.
fn db_path() -> PathBuf {
    PathBuf::from(
        env::var("GAMES_DB_PATH").unwrap_or_else(|_| "/var/lib/ultranet-games/games.db".into()),
    )
}

fn open_store() -> Result<GamesStore, String> {
    let path = db_path();
    if !path.exists() {
        return Err(format!(
            "no database at {}; set GAMES_DB_PATH",
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

/// The payout signer reuses the faucet's key type, whose errors say "faucet signer". An operator
/// reading the journal needs the name of *this* service's credential and the address it must match.
fn payout_signer_error(config: &GamesConfig, error: &str) -> String {
    format!(
        "the payout signer credential {} does not load for {}: {error}",
        config
            .credential_path(&config.payout_signer_credential)
            .display(),
        config.service.payout_address
    )
}

fn build_service(config: &GamesConfig) -> Result<Arc<GamesService>, String> {
    let store = GamesStore::open(&config.db_path).map_err(|e| e.to_string())?;
    let signer = FaucetSigner::load(
        &config.credential_path(&config.payout_signer_credential),
        &config.service.payout_address,
    )
    .map_err(|e| payout_signer_error(config, &e.to_string()))?;
    let abuse_key = read_secret(&config.credential_path(&config.abuse_key_credential), 32)?;
    let node =
        Arc::new(NodeClient::new(config.node_api_base_url.clone()).map_err(|e| e.to_string())?);
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
    Ok(Arc::new(GamesService::new(
        store,
        config.policy.clone(),
        config.service.clone(),
        chain,
        node,
        Some(Arc::new(signer)),
        anchorer,
        abuse_key.to_vec(),
        system_clock(),
    )))
}

async fn serve() -> Result<(), String> {
    let config = GamesConfig::from_env()?;
    let service = build_service(&config)?;
    if !config.policy.chess.pays {
        tracing::warn!(
            "chess is configured as record-only: wins are recorded and anchored but not paid"
        );
    }
    if config.anchor.is_none() {
        tracing::warn!("GAMES_ANCHOR_CLI is not set: days will not be anchored automatically");
    }
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(service.clone().run_worker(shutdown_rx.clone()));
    tracing::info!(bind = %config.bind, "ultranet-games listening");

    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| e.to_string())?;
    let result = tokio::select! {
        result = api::run_server(service, config.bind, shutdown_rx) => result.map_err(|e| e.to_string()),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = terminate.recv() => Ok(()),
    };
    let _ = shutdown_tx.send(true);
    // Let an in-flight payout step finish rather than cutting it off between signing and saving.
    let _ = tokio::time::timeout(Duration::from_secs(20), worker).await;
    result
}

fn check_config() -> Result<(), String> {
    let config = GamesConfig::from_env()?;
    println!("UltraNet games configuration is valid.");
    println!("Bind address:      {}", config.bind);
    println!("Database:          {}", config.db_path.display());
    println!("Node API:          {}", config.node_api_base_url);
    println!("Payout address:    {}", config.service.payout_address);
    let game = |g: &UltraNet::games::rules::GamePolicy| {
        format!(
            "{} units per win, {} per address per day, {} per day, {}, chain #{}",
            g.reward_units,
            g.rewarded_per_address_per_day,
            g.rewarded_per_day,
            if g.pays { "paying" } else { "RECORD ONLY" },
            g.chain_id
        )
    };
    println!("UltraGrid:         {}", game(&config.policy.grid));
    println!("Ultra Chess:       {}", game(&config.policy.chess));
    println!(
        "Withdraw minimum:  {} units ({} $ULTRA), daily payout cap {} units",
        config.policy.withdraw_min_units,
        format_units(config.policy.withdraw_min_units),
        config.policy.payout_daily_cap_units
    );
    println!(
        "Chain presence:    {}",
        if config.policy.require_chain_presence {
            "required"
        } else {
            "NOT required"
        }
    );
    println!(
        "Anchored games:    {}",
        config.service.anchor_games.join(", ")
    );

    // Credentials are checked for presence and shape, never printed.
    FaucetSigner::load(
        &config.credential_path(&config.payout_signer_credential),
        &config.service.payout_address,
    )
    .map_err(|e| payout_signer_error(&config, &e.to_string()))?;
    println!("Payout signer:     loaded, address matches");
    read_secret(&config.credential_path(&config.abuse_key_credential), 32)?;
    println!("Abuse key:         present");
    match &config.anchor {
        None => println!("Anchoring:         DISABLED (GAMES_ANCHOR_CLI not set)"),
        Some(anchor) => {
            if !anchor.cli_path.exists() {
                return Err(format!(
                    "GAMES_ANCHOR_CLI {} does not exist",
                    anchor.cli_path.display()
                ));
            }
            for name in [&anchor.key_credential, &anchor.password_credential] {
                let path = config.credential_path(name);
                if !path.exists() {
                    return Err(format!("anchor credential {} is missing", path.display()));
                }
            }
            println!(
                "Anchoring:         enabled via {}",
                anchor.cli_path.display()
            );
        }
    }
    Ok(())
}

fn status() -> Result<(), String> {
    let store = open_store()?;
    let state = store.state().map_err(|e| e.to_string())?;
    println!("accepting results: {}", state.accepting);
    println!("paying out:        {}", state.paying);
    if let Some(reason) = &state.reason {
        println!("last reason:       {reason}");
    }
    let (credited, paid, reserved) = store.totals().map_err(|e| e.to_string())?;
    println!(
        "credited {} · paid {} · reserved {} · still owed {} (base units)",
        credited,
        paid,
        reserved,
        credited.saturating_sub(paid)
    );
    let open = store.open_payouts().map_err(|e| e.to_string())?;
    println!("open payouts:      {}", open.len());
    Ok(())
}

fn audit() -> Result<(), String> {
    let report = open_store()?.audit().map_err(|e| e.to_string())?;
    if report.ok() {
        println!("audit ok: every balance is explained by wins and payouts");
        return Ok(());
    }
    for issue in &report.issues {
        println!("PROBLEM: {issue}");
    }
    std::process::exit(1);
}

fn set_switches(args: &[String], value: bool) -> Result<(), String> {
    let store = open_store()?;
    let both = !has(args, "--accepting") && !has(args, "--paying");
    let accepting = (both || has(args, "--accepting")).then_some(value);
    let paying = (both || has(args, "--paying")).then_some(value);
    let reason = if value {
        None
    } else {
        Some(
            flag(args, "--reason")
                .ok_or("disabling needs --reason \"why\" so the next person knows")?,
        )
    };
    let now = UltraNet::auth::now_seconds();
    store
        .set_state(accepting, paying, reason.as_deref(), now)
        .map_err(|e| e.to_string())?;
    let state = store.state().map_err(|e| e.to_string())?;
    println!(
        "accepting results: {} · paying out: {}",
        state.accepting, state.paying
    );
    Ok(())
}

/// A consistent snapshot of the database, readable only by its owner. Works while the service runs.
fn backup(args: &[String]) -> Result<(), String> {
    let destination = PathBuf::from(flag(args, "--to").ok_or("--to <new file> is required")?);
    let store = open_store()?;
    store.backup_to(&destination).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    // A backup nobody has checked is a hope, not a backup: open the copy and audit it.
    let copy = GamesStore::open(&destination).map_err(|e| e.to_string())?;
    let report = copy.audit().map_err(|e| e.to_string())?;
    println!("backup written to {} (mode 0600)", destination.display());
    if report.ok() {
        println!("the copy opens and passes the audit");
        Ok(())
    } else {
        for issue in &report.issues {
            println!("PROBLEM in the copy: {issue}");
        }
        Err("the backup was written but it does not pass the audit; the live database has the same problem".into())
    }
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

/// Close a finished registry day and anchor it if the configuration has a registry chain. Like
/// `close-day` it loads the full configuration and the unit's credentials, so it is not a plain-shell command.
async fn registry_close_day(args: &[String]) -> Result<(), String> {
    let date = flag(args, "--date").ok_or("--date YYYY-MM-DD is required")?;
    let config = GamesConfig::from_env()?;
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

fn payouts() -> Result<(), String> {
    let store = open_store()?;
    let open = store.open_payouts().map_err(|e| e.to_string())?;
    if open.is_empty() {
        println!("no unfinished payouts");
    }
    for payout in open {
        println!(
            "{}  {}  {} units  {}  attempts {}  hash {}  {}",
            payout.id,
            payout.address,
            payout.amount_units,
            payout.state.code(),
            payout.attempts,
            payout.tx_hash.as_deref().unwrap_or("-"),
            payout.last_error.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

fn payout_resolve(args: &[String]) -> Result<(), String> {
    let id = flag(args, "--id").ok_or("--id is required")?;
    let note =
        flag(args, "--note").ok_or("--note is required: say what you checked on the chain")?;
    let confirmed = match (has(args, "--confirmed"), has(args, "--failed")) {
        (true, false) => true,
        (false, true) => false,
        _ => return Err("choose exactly one of --confirmed or --failed".into()),
    };
    let store = open_store()?;
    let payout = store
        .payout(&id)
        .map_err(|e| e.to_string())?
        .ok_or("no such payout")?;
    println!(
        "payout {} · {} units to {} · state {} · hash {}",
        payout.id,
        payout.amount_units,
        payout.address,
        payout.state.code(),
        payout.tx_hash.as_deref().unwrap_or("-")
    );
    println!(
        "marking it {}. Only do this after looking the hash up on the chain: {} means the player's balance {}.",
        if confirmed { "CONFIRMED" } else { "FAILED" },
        if confirmed { "confirmed" } else { "failed" },
        if confirmed { "is paid and cannot be paid again" } else { "is released and can be withdrawn again" }
    );
    store
        .settle_payout(
            &id,
            confirmed,
            Some(&format!("operator: {note}")),
            UltraNet::auth::now_seconds(),
        )
        .map_err(|e| e.to_string())?;
    println!("done");
    Ok(())
}

async fn close_day(args: &[String]) -> Result<(), String> {
    let game = flag(args, "--game").ok_or("--game grid|chess is required")?;
    let date = flag(args, "--date").ok_or("--date YYYY-MM-DD is required")?;
    let config = GamesConfig::from_env()?;
    let service = build_service(&config)?;
    let row = service
        .close_and_anchor(&game, &date)
        .await
        .map_err(|e| e.to_string())?;
    println!("{} {} -> {}", row.game, row.day, row.state);
    if let Some(root) = &row.day_root {
        println!("day root:    {}", hex::encode(root));
    }
    if let Some(tx) = &row.anchor_tx {
        println!("anchor tx:   {tx}");
    }
    if let Some(error) = &row.last_error {
        println!("last error:  {error}");
    }
    Ok(())
}
