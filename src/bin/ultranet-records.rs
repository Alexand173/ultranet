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
//! ultranet-records peer-usage [--date YYYY-MM-DD]    # the Games service's anchor allowance for a day (database only)
//! ultranet-records usage-report [--date D | --from D --to D] [--days N] [--json]
//!                                                    # accepted records, anchor cost and each app's share of it,
//!                                                    # from the database read-only: no key, no network (P3)
//! ultranet-records peer-resolve --date D --chain C --root <64 hex> (--landed <tx hash> [--pending-unverified-ok] | --not-submitted) --note "..."
//! ultranet-records registry-resolve --date D (--landed <tx hash> [--pending-unverified-ok] | --not-submitted) --note "..."
//!                                                    # an operator's decision about an anchor whose outcome is unknown;
//!                                                    # a landed transaction is checked against the node first
//! ultranet-records restore-mark --note "restored from ..."   # after putting a backup in place: hold anchoring
//! ultranet-records peer-reconcile [--apply] [--acknowledge-owner-transactions] [--clear-hold]
//!                                                    # compare the ledger and the owner's history with the database (needs the node)
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
        "peer-usage" => peer_usage(&args[1..]),
        "usage-report" => usage_report(&args[1..]),
        "peer-resolve" => peer_resolve(&args[1..]).await,
        "registry-resolve" => registry_resolve(&args[1..]).await,
        "restore-mark" => restore_mark(&args[1..]),
        "peer-reconcile" => peer_reconcile(&args[1..]).await,
        "game-register" => game_register(&args[1..]),
        "game-list" => game_list(),
        "game-status" => game_status(&args[1..]),
        "game-rotate-key" => game_rotate_key(&args[1..]),
        _ => Err("use serve, check-config, status, disable, enable, backup, close-day, peer-usage, peer-resolve, registry-resolve, restore-mark, peer-reconcile, game-register, game-list, game-status or game-rotate-key".into()),
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
            if let Some(owner) = &config.service.owner_address {
                // `appchain-cli address` does not honour `--password-file` (only the signing commands do), so the
                // passphrase reaches that one child through its environment, for the length of the call.
                let passphrase = read_secret(&config.credential_path(&anchor.password_credential), 1)?;
                let passphrase = String::from_utf8_lossy(&passphrase).lines().next().unwrap_or_default().trim().to_string();
                let output = std::process::Command::new(&anchor.cli_path)
                    .arg("address")
                    .arg("--key-file")
                    .arg(config.credential_path(&anchor.key_credential))
                    .env("ULTRANET_APPCHAIN_KEY_PASSWORD", passphrase)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .map_err(|e| format!("cannot run {}: {e}", anchor.cli_path.display()))?;
                let address = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !output.status.success() || &address != owner {
                    return Err(format!(
                        "RECORDS_OWNER_ADDRESS ({owner}) is not the address of the owner key ({})",
                        if address.is_empty() { "unreadable" } else { &address }
                    ));
                }
                println!("Owner address:     {owner} (matches the owner key)");
            }
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

/// What the Games service has asked this one to anchor on a UTC day, and how much of the allowance is
/// used. Opens the database only. If the stored state is inconsistent it says so and changes nothing:
/// see `docs/runbooks/records-service.md` §5.1.
fn peer_usage(args: &[String]) -> Result<(), String> {
    let store = open_store()?;
    let day = flag(args, "--date").unwrap_or_else(|| UltraNet::games::service::date_of(UltraNet::auth::now_seconds()));
    let limit = env::var("RECORDS_PEER_ANCHORS_PER_DAY").ok().unwrap_or_else(|| "8 (default)".into());
    let roots = store.peer_anchor_roots(&day).map_err(|e| e.to_string())?;
    match store.check_fence().map_err(|e| e.to_string())? {
        UltraNet::games::fence::FenceCheck::Intact => {}
        UltraNet::games::fence::FenceCheck::Regressed(why) => println!("FENCE REGRESSED, this database looks restored: {why}"),
    }
    if let Some(hold) = store.peer_hold().map_err(|e| e.to_string())? {
        println!("HELD after a restore, no anchor is signed until reconciled: {hold}");
    }
    println!("{day}: {} of {limit} slots used", roots.len());
    for root in roots {
        println!(
            "  chain {} {} {:<10} attempts {} {}",
            root.chain_id,
            hex::encode(root.root),
            root.state.code(),
            root.attempts,
            root.tx_hash.as_deref().map(|tx| format!("tx {tx}")).or(root.last_error.map(|e| format!("last error: {e}"))).unwrap_or_default()
        );
    }
    Ok(())
}

/// Per-application, per-day usage and anchor cost: the figures D3 needs to decide who pays.
///
/// This is the only report that can be trusted about cost, because it reads what the service actually
/// did. It opens the database read-only and does nothing else: no key, no network, no child process. It
/// reads `RECORDS_DB_PATH` and `RECORDS_REGISTRY_CHAIN_ID` straight from the environment for the same
/// reason `peer-usage` does — building the full service configuration is how a report would end up
/// holding a credential it never needs.
fn usage_report(args: &[String]) -> Result<(), String> {
    let path = db_path();
    // The operator reads this line, so a plain reason beats an internal prefix.
    let store = GamesStore::open_read_only(&path).map_err(|error| match error {
        UltraNet::games::store::StoreError::Malformed(reason) => reason,
        other => other.to_string(),
    })?;
    let today = UltraNet::games::service::date_of(UltraNet::auth::now_seconds());

    let (from, to) = match flag(args, "--date") {
        Some(date) => (date.clone(), date),
        None => {
            let days = match flag(args, "--days") {
                Some(value) => value
                    .parse::<u64>()
                    .ok()
                    .filter(|days| *days > 0)
                    .ok_or_else(|| format!("--days needs a positive whole number, got {value}"))?,
                None => 7,
            };
            let to = flag(args, "--to").unwrap_or_else(|| today.clone());
            let from = flag(args, "--from").unwrap_or_else(|| {
                days_before(&to, days.saturating_sub(1)).unwrap_or_else(|| to.clone())
            });
            (from, to)
        }
    };

    let registry_chain_id = env::var("RECORDS_REGISTRY_CHAIN_ID")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());

    let report = UltraNet::games::usage::build(&store, &from, &to, &today, registry_chain_id)
        .map_err(|e| e.to_string())?;
    if args.iter().any(|argument| argument == "--json") {
        let json = serde_json::to_string_pretty(&report.to_json()).map_err(|e| e.to_string())?;
        println!("{json}");
    } else {
        print!("{}", report.render_text(&path.display().to_string()));
    }
    Ok(())
}

/// The UTC date `days` days before `date`, for the report's default window.
fn days_before(date: &str, days: u64) -> Option<String> {
    let start = UltraNet::games::service::date_start(date)?;
    Some(UltraNet::games::service::date_of(
        start.saturating_sub(days.saturating_mul(86_400)),
    ))
}

/// What an operator decided about an anchor whose outcome the service could not settle, with a claimed
/// landed transaction checked against the node first (owner, AppChain, and, once mined, chain and root).
async fn resolution_from(
    args: &[String],
    chain_id: u32,
    root: &[u8; 32],
) -> Result<UltraNet::games::peer_usage::PeerResolution, String> {
    use UltraNet::games::peer_usage::PeerResolution;
    match (flag(args, "--landed"), args.iter().any(|a| a == "--not-submitted")) {
        (Some(tx_hash), false) => {
            use UltraNet::games::records::LandedCheck;
            let config = RecordsServiceConfig::from_env()?;
            let owner = config.service.owner_address.clone().ok_or("RECORDS_OWNER_ADDRESS is required")?;
            let chain = HttpChain::new(&config.node_api_base_url).map_err(|e| e.to_string())?;
            let checked = UltraNet::games::records::validate_landed_transaction(
                &chain,
                &owner,
                chain_id,
                root,
                &tx_hash,
                args.iter().any(|a| a == "--pending-unverified-ok"),
            )
            .await
            .map_err(|reason| format!("not recording it: {reason}"))?;
            match checked {
                LandedCheck::Verified(note) => {
                    println!("{note}");
                    Ok(PeerResolution::Landed { tx_hash })
                }
                // A pending transaction is only a claim: it never marks the anchor landed.
                LandedCheck::PendingClaim(note) => {
                    println!("{note}");
                    Ok(PeerResolution::PendingClaim { tx_hash })
                }
            }
        }
        (None, true) => Ok(PeerResolution::NotSubmitted),
        _ => Err("give exactly one of --landed <tx hash> or --not-submitted".into()),
    }
}

/// An operator's decision about a peer root whose anchor the service could not settle by itself.
async fn peer_resolve(args: &[String]) -> Result<(), String> {
    use UltraNet::games::peer_usage::PeerResolution;
    let day = flag(args, "--date").ok_or("--date YYYY-MM-DD is required")?;
    let chain: u32 = flag(args, "--chain")
        .ok_or("--chain <id> is required")?
        .parse()
        .map_err(|_| "--chain must be a number")?;
    let root: [u8; 32] = hex::decode(flag(args, "--root").ok_or("--root <64 hex> is required")?)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("--root must be 64 hexadecimal characters")?;
    let note = flag(args, "--note").ok_or("--note \"what you checked\" is required")?;
    let resolution = resolution_from(args, chain, &root).await?;
    open_store()?
        .resolve_peer_root(&day, chain, &root, &resolution, &note, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    match resolution {
        PeerResolution::Landed { .. } => println!("recorded as landed; it will not be submitted again"),
        PeerResolution::PendingClaim { .. } => println!("recorded as a CLAIM; the root stays in doubt (never resubmitted) and is verified automatically once the transaction is mined and indexed"),
        PeerResolution::NotSubmitted => println!("returned to reserved; it may be submitted again (the attempt stays counted)"),
    }
    Ok(())
}

/// An operator's decision about a registry day whose anchor the service could not settle by itself.
async fn registry_resolve(args: &[String]) -> Result<(), String> {
    use UltraNet::games::peer_usage::PeerResolution;
    let day = flag(args, "--date").ok_or("--date YYYY-MM-DD is required")?;
    let note = flag(args, "--note").ok_or("--note \"what you checked\" is required")?;
    let store = open_store()?;
    let config = RecordsServiceConfig::from_env()?;
    let record = store
        .registry_anchor(&day)
        .map_err(|e| e.to_string())?
        .ok_or("that registry day has no anchor record")?;
    let resolution = resolution_from(args, record.chain_id, &record.root).await?;
    let _ = config;
    store
        .resolve_registry_anchor(&day, &resolution, &note, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    match resolution {
        PeerResolution::Landed { tx_hash } => {
            store
                .set_registry_anchor(&day, Some(&tx_hash), "anchored", UltraNet::auth::now_seconds())
                .map_err(|e| e.to_string())?;
            println!("recorded as landed; it will not be submitted again")
        }
        PeerResolution::PendingClaim { .. } => println!("recorded as a CLAIM; the day stays in doubt (never resubmitted) and is verified automatically once the transaction is mined and indexed"),
        PeerResolution::NotSubmitted => println!("returned to reserved; it may be submitted again"),
    }
    Ok(())
}

/// After a backup has been put in place by hand, hold all anchoring until it has been reconciled with the
/// ledger. Database only.
fn restore_mark(args: &[String]) -> Result<(), String> {
    let note = flag(args, "--note").ok_or("--note \"restored from <which backup>\" is required")?;
    open_store()?
        .set_peer_hold(&note, UltraNet::auth::now_seconds())
        .map_err(|e| e.to_string())?;
    println!("anchoring is held. Start the service, then run `ultranet-records peer-reconcile`.");
    Ok(())
}

/// Compare what the ledger and the owner's history hold with what this database remembers, adopt what it has
/// forgotten (`--apply`), baseline the owner's other transactions (`--acknowledge-owner-transactions`), and
/// release the hold (`--clear-hold`) only when nothing is unaccounted for.
async fn peer_reconcile(args: &[String]) -> Result<(), String> {
    use UltraNet::games::chain::GamesChain;
    use UltraNet::games::service::{date_of, date_start};
    let has_flag = |name: &str| args.iter().any(|a| a == name);
    let (apply, clear) = (has_flag("--apply"), has_flag("--clear-hold"));
    let acknowledge = has_flag("--acknowledge-owner-transactions");
    let acknowledged_pending = has_flag("--i-have-checked-for-pending-transactions");
    let config = RecordsServiceConfig::from_env()?;
    let owner = config.service.owner_address.clone().ok_or("RECORDS_OWNER_ADDRESS is required")?;
    let store = GamesStore::open(&config.db_path).map_err(|e| e.to_string())?;
    let chain = HttpChain::new(&config.node_api_base_url).map_err(|e| e.to_string())?;
    let now = UltraNet::auth::now_seconds();
    let today = date_of(now);
    let since = date_start(&today).ok_or("today is not a date")?.saturating_sub(86_400);
    let yesterday = date_of(since);

    match store.check_fence().map_err(|e| e.to_string())? {
        UltraNet::games::fence::FenceCheck::Intact => println!("fence: intact"),
        UltraNet::games::fence::FenceCheck::Regressed(why) => println!("fence: REGRESSED, {why}"),
    }

    // 1. Anchors the ledger shows that this database has no record of.
    let known = store.peer_known_roots(&[&today, &yesterday]).map_err(|e| e.to_string())?;
    let registry_known = store.registry_known_roots().map_err(|e| e.to_string())?;
    let mut unresolved = 0usize;
    for chain_id in &config.service.peer_anchor_chains {
        for point in chain.anchors_since(*chain_id, since).await.map_err(|e| format!("cannot read the ledger: {e}"))? {
            if known.contains(&(*chain_id, point.root)) {
                continue;
            }
            println!("UNKNOWN to this database: chain {chain_id} root {} tx {} (block time {})", hex::encode(point.root), point.tx_hash, point.block_timestamp);
            if apply {
                let day = date_of(point.block_timestamp);
                let added = store.adopt_ledger_anchor(&day, *chain_id, &point.root, &point.tx_hash, now).map_err(|e| e.to_string())?;
                println!("  adopted into {day} as a used slot: {added}");
            } else {
                unresolved += 1;
            }
        }
    }
    if let Some(registry_chain) = config.service.registry_chain_id {
        for point in chain.anchors_since(registry_chain, since).await.map_err(|e| format!("cannot read the ledger: {e}"))? {
            if registry_known.contains(&(registry_chain, point.root)) {
                continue;
            }
            println!("UNKNOWN registry anchor: chain {registry_chain} root {} tx {}", hex::encode(point.root), point.tx_hash);
            // A registry anchor can only be adopted against the closed day whose root it carries.
            let mut matched = false;
            // A registry day can be anchored days after it closes: look back across the catch-up window.
            for back in 0..=9u64 {
                let date = &date_of(date_start(&today).unwrap_or(now).saturating_sub(back * 86_400));
                if let Some(day) = store.registry_day(date).map_err(|e| e.to_string())? {
                    if day.day_root == point.root {
                        matched = true;
                        if apply {
                            store.ensure_registry_anchor(date, registry_chain, &point.root, now).map_err(|e| e.to_string())?;
                            store
                                .resolve_registry_anchor(date, &UltraNet::games::peer_usage::PeerResolution::Landed { tx_hash: point.tx_hash.clone() }, "adopted from the ledger", now)
                                .map_err(|e| e.to_string())?;
                            store.set_registry_anchor(date, Some(&point.tx_hash), "confirmed", now).map_err(|e| e.to_string())?;
                            println!("  adopted as registry day {date}");
                        }
                    }
                }
            }
            if !matched {
                println!("  it matches no closed registry day in this database: it cannot be adopted");
            }
            if !matched || !apply {
                unresolved += 1;
            }
        }
    }

    // 2. The owner's own history against the attempts this database recorded.
    let activity = chain.owner_appchain_activity(&owner, since).await.map_err(|e| format!("cannot read the owner's history: {e}"))?;
    let pending: Vec<_> = activity.transactions.iter().filter(|t| t.pending).collect();
    for tx in &pending {
        println!("PENDING owner AppChain transaction {} (signed at {})", tx.hash, tx.timestamp);
    }
    if !activity.complete {
        println!("note: the node could not show the owner's full history, so pending transactions may be hidden");
    }
    let (ack_ts, ack_attempts) = store.owner_ack().map_err(|e| e.to_string())?;
    let recorded_now = store.recorded_attempts(&[&yesterday, &today]).map_err(|e| e.to_string())?;
    let (counted, recorded) = if ack_ts >= since {
        let from = date_of(ack_ts);
        let days: Vec<&str> = if from == today { vec![&today] } else { vec![&yesterday, &today] };
        (
            activity.transactions.iter().filter(|t| t.timestamp > ack_ts).count() as u64,
            store.recorded_attempts(&days).map_err(|e| e.to_string())?.saturating_sub(ack_attempts),
        )
    } else {
        (activity.transactions.len() as u64, recorded_now)
    };
    println!("owner AppChain transactions since the baseline: {counted}; attempts this database recorded: {recorded}");
    let mut accounted = counted <= recorded;
    if !accounted {
        println!("UNACCOUNTED: the owner has signed {} more AppChain transaction(s) than this database recorded attempts for", counted - recorded);
        if acknowledge {
            store.set_owner_ack(now, store.recorded_attempts(&[&today]).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            println!("  acknowledged: everything up to now is treated as accounted for");
            accounted = true;
        }
    }
    println!("today's allowance: {} slot(s) used", store.peer_anchor_usage(&today).map_err(|e| e.to_string())?);

    if clear {
        if unresolved > 0 {
            return Err("not releasing the hold: anchors the ledger shows are still unaccounted for; run with --apply (and resolve any that cannot be adopted)".into());
        }
        if !accounted {
            return Err("not releasing the hold: the owner's history shows transactions this database cannot account for; if you know why (for example you registered a chain), run with --acknowledge-owner-transactions".into());
        }
        if !pending.is_empty() {
            return Err("not releasing the hold: the owner has pending transactions; wait for them to be mined, then run again".into());
        }
        if !activity.complete && !acknowledged_pending {
            return Err("not releasing the hold: pending transactions may be hidden from the node's list. Check the node yourself, then run again with --i-have-checked-for-pending-transactions".into());
        }
        store.clear_peer_hold().map_err(|e| e.to_string())?;
        store.accept_fence().map_err(|e| e.to_string())?;
        println!("the hold is released and the fence now describes this database; anchoring may resume");
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

