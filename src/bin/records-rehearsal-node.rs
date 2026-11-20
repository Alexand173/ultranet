//! A disposable single-node chain for rehearsing the Records Service against the real node code.
//!
//! It runs the repository's real blockchain and real API server in this process, on a scratch database, with one
//! pre-funded address. There is no peer networking, no validator key and no real value. Blocks are mined only when
//! the rehearsal asks (`touch <control>/mine-now`), so a script can observe a transaction while it is still pending.
//!
//! ```text
//! records-rehearsal-node --disposable --db /tmp/rehearsal/db --control /tmp/rehearsal/control \
//!                        --port 18081 --fund <64 hex address>:<base units>
//! ```
//!
//! Safety gates (it refuses to start unless all hold):
//! * `--disposable` is given: the operator states this is not production;
//! * the database directory is new or empty, and lies under the system temp directory;
//! * the API binds only to 127.0.0.1, and the port is not the production node's default (8081);
//! * no peer is configured: only the API is started, never the p2p stack.
//!
//! It is a test harness. It proves how the *node* behaves (pending transactions, nonces, ledger indexing); it does
//! not prove anything about consensus, peers or production.

#[cfg(unix)]
use parking_lot::RwLock;
#[cfg(unix)]
use std::{env, fs, path::PathBuf, sync::Arc, time::Duration};
#[cfg(unix)]
use UltraNet::{api, storage::Storage, UltraBlockchain};

#[cfg(unix)]
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[cfg(unix)]
fn refuse(reason: &str) -> ! {
    eprintln!("records-rehearsal-node: refusing to start: {reason}");
    std::process::exit(2);
}

#[cfg(unix)]
#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--disposable") {
        refuse("pass --disposable to state that this is a throwaway chain, never production");
    }
    let db = PathBuf::from(flag(&args, "--db").unwrap_or_else(|| refuse("--db <directory> is required")));
    let control = PathBuf::from(flag(&args, "--control").unwrap_or_else(|| refuse("--control <directory> is required")));
    let port: u16 = flag(&args, "--port")
        .unwrap_or_else(|| refuse("--port <number> is required"))
        .parse()
        .unwrap_or_else(|_| refuse("--port must be a number"));
    if port == 8081 {
        refuse("port 8081 is the production node's default; choose another");
    }
    let temp = env::temp_dir();
    let db_text = db.to_string_lossy().to_string();
    if !db.starts_with(&temp) || db_text.contains("ultranet_db") || db_text.starts_with("/var/lib") {
        refuse("the database must lie under the system temp directory and must not look like a real data path");
    }
    if db.exists() && fs::read_dir(&db).map(|mut d| d.next().is_some()).unwrap_or(true) {
        refuse("the database directory must be new or empty");
    }
    fs::create_dir_all(&control).unwrap_or_else(|e| refuse(&format!("cannot create the control directory: {e}")));

    env::set_var("ULTRANET_API_BIND", format!("127.0.0.1:{port}"));
    env::set_var("ULTRANET_CORS_ORIGINS", format!("http://127.0.0.1:{port}"));
    // A throwaway admin credential for this process only: random, never printed, never written.
    let mut token = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token);
    env::set_var("ULTRANET_ADMIN_TOKEN", hex::encode(token));
    env::set_var("ULTRANET_DB_PATH", &db);
    // AppChain transactions are accepted only from an agreed height on; a scratch chain agrees on its own.
    env::set_var(
        "ULTRANET_APPCHAIN_ACTIVATION_HEIGHT",
        flag(&args, "--appchain-activation-height").unwrap_or_else(|| "1".to_string()),
    );

    let storage = Arc::new(Storage::new(&db_text).unwrap_or_else(|e| refuse(&format!("cannot open the scratch database: {e}"))));
    for funding in args.windows(2).filter(|w| w[0] == "--fund").map(|w| w[1].clone()) {
        let (address, units) = funding.split_once(':').unwrap_or_else(|| refuse("--fund takes <address>:<base units>"));
        if !UltraNet::UltraBlockchain::is_valid_address(address) {
            refuse("--fund names an address that is not 64 lowercase hex characters");
        }
        let units: u64 = units.parse().unwrap_or_else(|_| refuse("--fund amount must be a number"));
        storage
            .save_state(address, units)
            .unwrap_or_else(|e| refuse(&format!("cannot fund {address}: {e}")));
        println!("FUNDED {address} {units}");
    }

    let blockchain = Arc::new(RwLock::new(UltraBlockchain::with_storage(storage)));

    // Mine only on request, so a script can look at the node while a transaction is pending.
    let miner = blockchain.clone();
    let control_for_miner = control.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let trigger = control_for_miner.join("mine-now");
        if trigger.exists() {
            let _ = fs::remove_file(&trigger);
            let result = miner.write().mine_block();
            match result {
                Ok(block) => println!("MINED block {} ({} transaction(s))", block.index, block.transactions.len()),
                Err(error) => println!("MINE_FAILED {error}"),
            }
            let _ = fs::write(control_for_miner.join("mined"), b"1");
        }
    });

    println!("REHEARSAL_NODE_READY http://127.0.0.1:{port}");
    let server = tokio::select! {
        result = api::run_server(blockchain) => result.map_err(|e| e.to_string()),
        _ = tokio::signal::ctrl_c() => Ok(()),
    };
    if let Err(error) = server {
        eprintln!("records-rehearsal-node: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("the rehearsal node requires a Unix host");
    std::process::exit(1);
}
