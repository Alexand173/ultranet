use parking_lot::RwLock;
use serde::Serialize;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use UltraNet::{
    api,
    auth::{canonical_login_message, AuthConfig, AuthService},
    quantum_crypto::QuantumKeyPair,
    storage::Storage,
    UltraBlockchain, ValidatorJoinProposalData,
};

#[derive(Debug, Serialize)]
struct SessionBootstrap {
    owner_index: usize,
    node_identifier: String,
    session_token: String,
    csrf_token: String,
}

#[derive(Debug, Serialize)]
struct Bootstrap {
    api_base_url: String,
    proposal_hash: String,
    owner_sessions: Vec<SessionBootstrap>,
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("cannot sync {}: {error}", path.display()))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let root = env::var_os("ULTRANET_APPROVAL_E2E_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            env::temp_dir().join(format!(
                "ultranet-approval-e2e-{}-{nonce}",
                std::process::id()
            ))
        });
    let db_path = root.join("db");
    let key_dir = root.join("keys");
    let socket_dir = root.join("sockets");
    fs::create_dir_all(&db_path).map_err(|error| format!("cannot create db dir: {error}"))?;
    fs::create_dir_all(&key_dir).map_err(|error| format!("cannot create key dir: {error}"))?;
    fs::create_dir_all(&socket_dir)
        .map_err(|error| format!("cannot create socket dir: {error}"))?;

    let owners = std::array::from_fn::<_, 3, _>(|_| QuantumKeyPair::generate());
    let owner_identifiers = owners
        .iter()
        .map(QuantumKeyPair::address)
        .collect::<Vec<_>>();
    let signer_binary = env::var_os("ULTRANET_APPROVAL_SIGNER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("/home/valerian/UltraNet_Linux/target/release/ultranet-approval-signer")
        });

    let mut signer_children: Vec<Child> = Vec::new();
    let mut signer_sockets = Vec::new();
    for (owner_index, owner) in owners.iter().enumerate() {
        let key_path = key_dir.join(format!("owner-{owner_index}.json"));
        let socket_path = socket_dir.join(format!("owner-{owner_index}.sock"));
        let key_json = serde_json::to_vec(&serde_json::json!([{
            "address": owner.address(),
            "public_key": hex::encode(&owner.public_key),
            "secret_key": hex::encode(&owner.secret_key),
        }]))
        .map_err(|error| format!("cannot encode local signer key: {error}"))?;
        private_write(&key_path, &key_json)?;

        let child = Command::new(&signer_binary)
            .arg("--socket")
            .arg(&socket_path)
            .arg("--keys")
            .arg(&key_path)
            .arg("--owner-index")
            .arg(owner_index.to_string())
            .arg("--key-index")
            .arg("0")
            .arg("--signer-id")
            .arg(format!("owner-{owner_index}"))
            .arg("--unattended")
            .env("ULTRANET_APPROVAL_SIGNER_ALLOW_UNATTENDED", "I_UNDERSTAND")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot start local signer {owner_index}: {error}"))?;
        signer_children.push(child);
        signer_sockets.push(socket_path);
    }

    let auth_file = root.join("sovereign-owner-auth.json");
    let bindings = owner_identifiers
        .iter()
        .enumerate()
        .map(|(owner_index, node_identifier)| {
            serde_json::json!({
                "owner_index": owner_index,
                "session_node_identifier": node_identifier,
                "signer_id": format!("owner-{owner_index}"),
                "signer_socket": signer_sockets[owner_index],
            })
        })
        .collect::<Vec<_>>();
    private_write(
        &auth_file,
        &serde_json::to_vec_pretty(&bindings)
            .map_err(|error| format!("cannot encode owner auth mapping: {error}"))?,
    )?;

    for _ in 0..100 {
        if signer_sockets.iter().all(|path| path.exists()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !signer_sockets.iter().all(|path| path.exists()) {
        for child in &mut signer_children {
            let _ = child.kill();
            let _ = child.wait();
        }
        return Err("local signer sockets did not become ready".into());
    }

    let storage = Arc::new(
        Storage::new(
            db_path
                .to_str()
                .ok_or_else(|| "db path is not UTF-8".to_string())?,
        )
        .map_err(|error| format!("cannot open local approval database: {error}"))?,
    );
    let proposal_hash = [0xabu8; 32];
    let candidate = QuantumKeyPair::generate();
    let proposal = ValidatorJoinProposalData {
        public_key: candidate.public_key.clone(),
        metadata: "Browser-E2E-Validator".to_string(),
        proposer: owner_identifiers[0].clone(),
        timestamp: UltraNet::auth::now_seconds(),
    };
    storage
        .save_pending_proposal(&proposal_hash, &proposal)
        .map_err(|error| format!("cannot persist local pending proposal: {error}"))?;

    let mut blockchain = UltraBlockchain::with_storage(storage.clone());
    blockchain.sovereign_owners = owners
        .iter()
        .map(|owner| owner.public_key.clone())
        .collect();
    blockchain.sovereign_threshold = 2;

    let auth = AuthService::new(
        storage,
        AuthConfig {
            authorized_node_identifiers: owner_identifiers.iter().cloned().collect(),
            challenge_ttl_seconds: 300,
            session_ttl_seconds: 28_800,
            secure_cookie: false,
            cookie_domain: None,
        },
    );
    let mut owner_sessions = Vec::new();
    for (owner_index, owner) in owners.iter().enumerate() {
        let challenge = auth
            .issue_challenge(&owner_identifiers[owner_index])
            .map_err(|error| format!("cannot issue local owner challenge: {}", error.message()))?;
        let login_message = canonical_login_message(
            &challenge.challenge_id,
            &challenge.challenge,
            &challenge.node_identifier,
            challenge.expires_at,
            challenge.version,
        );
        let signature = owner.sign(&login_message);
        let session = auth
            .login(
                &challenge.challenge_id,
                &challenge.challenge,
                &challenge.node_identifier,
                challenge.expires_at,
                &owner.public_key,
                &signature,
                challenge.version,
            )
            .map_err(|error| format!("cannot create local owner session: {}", error.message()))?;
        owner_sessions.push(SessionBootstrap {
            owner_index,
            node_identifier: session.node_identifier,
            session_token: session.session_token,
            csrf_token: session.csrf_token,
        });
    }

    let api_port = env::var("ULTRANET_APPROVAL_E2E_PORT").unwrap_or_else(|_| "18081".to_string());
    let api_bind = format!("127.0.0.1:{api_port}");
    let api_base_url = format!("http://{api_bind}");
    env::set_var("ULTRANET_API_BIND", &api_bind);
    env::set_var(
        "ULTRANET_CORS_ORIGINS",
        "http://localhost:3000,http://127.0.0.1:3000,http://localhost:3001,http://127.0.0.1:3001",
    );
    env::set_var("ULTRANET_ADMIN_TOKEN", "a".repeat(64));
    env::set_var("ULTRANET_DB_PATH", &db_path);
    env::set_var(
        "ULTRANET_AUTHORIZED_NODE_IDENTIFIERS",
        owner_identifiers.join(","),
    );
    env::set_var(
        "ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS",
        owner_identifiers.join(","),
    );
    env::set_var("ULTRANET_WEB_APPROVAL_ENABLED", "true");
    env::set_var("ULTRANET_SOVEREIGN_OWNER_AUTH_FILE", &auth_file);
    env::set_var("ULTRANET_APPROVAL_SIGNER_TIMEOUT_SECONDS", "20");
    env::set_var("ULTRANET_APPROVAL_INTENT_TTL_SECONDS", "600");
    env::set_var("ULTRANET_SESSION_COOKIE_SECURE", "false");
    env::remove_var("ULTRANET_AUTH_COOKIE_DOMAIN");

    let bootstrap_path = root.join("bootstrap.json");
    let bootstrap = Bootstrap {
        api_base_url,
        proposal_hash: hex::encode(proposal_hash),
        owner_sessions,
    };
    private_write(
        &bootstrap_path,
        &serde_json::to_vec_pretty(&bootstrap)
            .map_err(|error| format!("cannot encode bootstrap: {error}"))?,
    )?;

    println!("LOCAL_APPROVAL_E2E_READY");
    println!("BOOTSTRAP_FILE={}", bootstrap_path.display());
    println!("PROPOSAL_HASH={}", bootstrap.proposal_hash);
    println!("API_BASE_URL={}", bootstrap.api_base_url);
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;

    let blockchain = Arc::new(RwLock::new(blockchain));
    let server_result = tokio::select! {
        result = api::run_server(blockchain) => result.map_err(|error| error.to_string()),
        _ = tokio::signal::ctrl_c() => Ok(()),
    };

    for child in &mut signer_children {
        let _ = child.kill();
        let _ = child.wait();
    }
    server_result
}
