//! `ultranet-games` — the Games Reward Service: checks answers, records wins and pays rewards. It holds
//! the payout key and **no chain owner's key**; days are anchored by asking the Records Service
//! (`ultranet-records`). Third-party games and the generic records API are served by that other process.
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
//! ultranet-games close-day --game grid|chess --date YYYY-MM-DD    (asks the Records Service to anchor)
//! ultranet-games backup --to /var/backups/ultranet-games/games-YYYYMMDD.db
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
        anchor::{Anchorer, RemoteAnchorer},
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
        _ => Err("use serve, check-config, status, audit, disable, enable, payouts, payout-resolve, close-day or backup".into()),
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

/// The Games Reward Service must not hold the chain owner's key. Refusing to start when one is in its
/// credentials directory turns a separation of duties from a convention into a check.
fn refuse_foreign_credentials(config: &GamesConfig) -> Result<(), String> {
    let present = config.foreign_credentials_present();
    if present.is_empty() {
        return Ok(());
    }
    Err(format!(
        "this service must not hold the chain owner's key, but found: {}. Those belong to ultranet-records; remove them from this unit's credentials",
        present
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn build_service(config: &GamesConfig) -> Result<Arc<GamesService>, String> {
    refuse_foreign_credentials(config)?;
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
    let anchorer: Option<Arc<dyn Anchorer>> = match config.anchor.as_ref() {
        None => None,
        Some(anchor) => {
            let secret = read_secret(
                &config.credential_path(&anchor.credential),
                UltraNet::games::peer::MIN_SECRET_BYTES,
            )?;
            Some(Arc::new(RemoteAnchorer::new(
                &anchor.records_url,
                secret.to_vec(),
                Duration::from_secs(anchor.timeout_seconds),
            )?) as Arc<dyn Anchorer>)
        }
    };
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

/// Refuse to start on a chain the ledger does not know. A definite "not registered" stops at once; a node
/// that cannot be read yet (it may still be starting) is retried for a short, bounded time, then refused:
/// serving with chains nobody could confirm is how a game ends up anchored to someone else's chain.
async fn verify_chains_at_start(service: &GamesService) -> Result<(), String> {
    use UltraNet::games::service::GamesError;
    const ATTEMPTS: u32 = 6;
    for attempt in 1..=ATTEMPTS {
        match service.verify_game_chains().await {
            Ok(()) => return Ok(()),
            Err(GamesError::ChainUnavailable) if attempt < ATTEMPTS => {
                tracing::warn!(attempt, "the node's ledger could not be read to check the game chains; retrying");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(GamesError::ChainUnavailable) => {
                return Err("the ledger could not be read, so the configured game chains were not confirmed; not starting".into())
            }
            Err(other) => return Err(other.to_string()),
        }
    }
    Ok(())
}

async fn serve() -> Result<(), String> {
    let config = GamesConfig::from_env()?;
    let service = build_service(&config)?;
    verify_chains_at_start(&service).await?;
    if !config.policy.chess.pays {
        tracing::warn!(
            "chess is configured as record-only: wins are recorded and anchored but not paid"
        );
    }
    if config.anchor.is_none() {
        tracing::warn!("GAMES_RECORDS_URL is not set: days will not be anchored automatically");
    }
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(service.clone().run_worker(shutdown_rx.clone()));
    tracing::info!(bind = %config.bind, "ultranet-games listening");

    let result = tokio::select! {
        result = api::run_server(service, config.bind, shutdown_rx) => result.map_err(|e| e.to_string()),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = UltraNet::platform::terminate() => Ok(()),
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
    refuse_foreign_credentials(&config)?;
    println!("Owner key:         not held here (correct)");
    match &config.anchor {
        None => println!("Anchoring:         DISABLED (GAMES_RECORDS_URL not set)"),
        Some(anchor) => {
            read_secret(
                &config.credential_path(&anchor.credential),
                UltraNet::games::peer::MIN_SECRET_BYTES,
            )?;
            println!(
                "Anchoring:         through the Records Service at {} (peer secret present)",
                anchor.records_url
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
    let owner_only = UltraNet::platform::restrict_to_owner(&destination).map_err(|e| e.to_string())?;
    // A backup nobody has checked is a hope, not a backup: open the copy and audit it.
    let copy = GamesStore::open(&destination).map_err(|e| e.to_string())?;
    let report = copy.audit().map_err(|e| e.to_string())?;
    println!(
        "backup written to {} ({})",
        destination.display(),
        UltraNet::platform::describe_owner_only(owner_only)
    );
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
