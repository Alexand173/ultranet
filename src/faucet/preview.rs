use crate::{
    faucet::{
        api,
        captcha::StaticCaptchaVerifier,
        config::FaucetConfig,
        models::{
            NodeAccountData, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest,
        },
        node_client::NodeApi,
        service::FaucetService,
        signer::FaucetSigner,
        store::FaucetStore,
    },
    ProofType, QuantumKeyPair, Transaction, TransactionPayload, UltraBlockchain,
};
use async_trait::async_trait;
use rand::{rngs::OsRng, RngCore};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use zeroize::Zeroizing;

const PREVIEW_OPERATOR_TOKEN: &[u8] = b"local-preview-operator-token-v1";

#[derive(Debug)]
struct PreviewState {
    balance: u64,
    nonce: u64,
    transactions: HashMap<String, NodeTransactionData>,
}

struct PreviewNode {
    address: String,
    state: Mutex<PreviewState>,
}

impl PreviewNode {
    fn new(address: String) -> Self {
        Self {
            address,
            state: Mutex::new(PreviewState {
                balance: 1_000_000_000,
                nonce: 0,
                transactions: HashMap::new(),
            }),
        }
    }

    fn transaction_hash(envelope: &SignedTransferRequest) -> Result<[u8; 32], String> {
        let nullifier: [u8; 32] = envelope
            .nullifier
            .as_slice()
            .try_into()
            .map_err(|_| "preview nullifier must contain 32 bytes".to_string())?;
        let transaction = Transaction {
            sender: envelope.sender.clone(),
            sender_public_key: envelope.sender_public_key.clone(),
            recipient: envelope.recipient.clone(),
            amount: envelope.amount,
            signature: envelope.signature.clone(),
            zk_proof: vec![],
            nullifier,
            timestamp: envelope.timestamp,
            fee: envelope.fee,
            nonce: envelope.nonce,
            gas_limit: envelope.gas_limit,
            gas_price: envelope.gas_price,
            proof_type: ProofType::Transaction,
            payload: TransactionPayload::StandardTransfer,
            chain_id: envelope.chain_id,
            version: envelope.version,
        };
        Ok(transaction.get_hash())
    }
}

#[async_trait]
impl NodeApi for PreviewNode {
    async fn account(
        &self,
        address: &str,
    ) -> Result<NodeAccountData, crate::faucet::node_client::NodeClientError> {
        let state = self
            .state
            .lock()
            .map_err(|_| crate::faucet::node_client::NodeClientError::Unavailable)?;
        Ok(NodeAccountData {
            address: address.to_string(),
            balance_base_units: Some(state.balance),
            balance: Some(state.balance),
            nonce: state.nonce,
            decimals: UltraBlockchain::ULTRA_DECIMALS,
        })
    }

    async fn estimate(
        &self,
        recipient: &str,
        amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, crate::faucet::node_client::NodeClientError> {
        let fee = UltraBlockchain::minimum_transfer_fee(amount_base_units);
        Ok(NodeFeeEstimateData {
            recipient: recipient.to_string(),
            amount: amount_base_units,
            fee,
            gas_limit: 500_000,
            gas_price: 1,
            total: amount_base_units
                .checked_add(fee)
                .ok_or(crate::faucet::node_client::NodeClientError::InvalidResponse)?,
        })
    }

    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, crate::faucet::node_client::NodeClientError> {
        let hash = Self::transaction_hash(envelope)
            .map_err(|_| crate::faucet::node_client::NodeClientError::InvalidResponse)?;
        let hash_hex = hex::encode(hash);
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::faucet::node_client::NodeClientError::Unavailable)?;
        if let Some(existing) = state.transactions.get(&hash_hex) {
            return Ok(existing.clone());
        }
        if envelope.sender != self.address
            || envelope.chain_id != UltraBlockchain::L1_CHAIN_ID
            || envelope.version != UltraBlockchain::LEGACY_TRANSACTION_VERSION
            || envelope.nonce != state.nonce
            || envelope.recipient == self.address
            || !UltraBlockchain::is_valid_address(&envelope.recipient)
            || envelope.fee < UltraBlockchain::minimum_transfer_fee(envelope.amount)
        {
            return Err(crate::faucet::node_client::NodeClientError::Rejected);
        }
        let message = {
            let nullifier: [u8; 32] = envelope
                .nullifier
                .as_slice()
                .try_into()
                .map_err(|_| crate::faucet::node_client::NodeClientError::InvalidResponse)?;
            let transaction = Transaction {
                sender: envelope.sender.clone(),
                sender_public_key: envelope.sender_public_key.clone(),
                recipient: envelope.recipient.clone(),
                amount: envelope.amount,
                signature: envelope.signature.clone(),
                zk_proof: vec![],
                nullifier,
                timestamp: envelope.timestamp,
                fee: envelope.fee,
                nonce: envelope.nonce,
                gas_limit: envelope.gas_limit,
                gas_price: envelope.gas_price,
                proof_type: ProofType::Transaction,
                payload: TransactionPayload::StandardTransfer,
                chain_id: envelope.chain_id,
                version: envelope.version,
            };
            UltraBlockchain::create_transaction_message_for(&transaction)
        };
        if QuantumKeyPair::address_from_public_key(&envelope.sender_public_key) != envelope.sender
            || !QuantumKeyPair::verify(&envelope.sender_public_key, &message, &envelope.signature)
        {
            return Err(crate::faucet::node_client::NodeClientError::Rejected);
        }
        let total = envelope
            .amount
            .checked_add(envelope.fee)
            .ok_or(crate::faucet::node_client::NodeClientError::InvalidResponse)?;
        if state.balance < total {
            return Err(crate::faucet::node_client::NodeClientError::Rejected);
        }
        state.balance -= total;
        state.nonce = state.nonce.saturating_add(1);
        let result = NodeTransactionData {
            hash: hash_hex.clone(),
            status: "confirmed".into(),
        };
        state.transactions.insert(hash_hex, result.clone());
        Ok(result)
    }

    async fn transaction_status(
        &self,
        hash: &str,
    ) -> Result<NodeTransactionData, crate::faucet::node_client::NodeClientError> {
        self.state
            .lock()
            .map_err(|_| crate::faucet::node_client::NodeClientError::Unavailable)?
            .transactions
            .get(hash)
            .cloned()
            .ok_or(crate::faucet::node_client::NodeClientError::NotFound)
    }
}

pub async fn run(bind: SocketAddr) -> Result<(), String> {
    let root = preview_root()?;
    let signer_path = root.join("faucet-signer.json");
    let keypair = QuantumKeyPair::generate();
    let faucet_address = keypair.address();
    write_signer_credential(&signer_path, &keypair)?;
    let signer = Arc::new(
        FaucetSigner::load(&signer_path, &faucet_address).map_err(|error| error.to_string())?,
    );
    let config = FaucetConfig {
        bind,
        node_api_base_url: "http://127.0.0.1:8081".into(),
        faucet_address: faucet_address.clone(),
        claim_amount_base_units: 1_000_000,
        daily_debit_cap_base_units: 100_000_000,
        min_balance_reserve_base_units: 200_000_000,
        address_cooldown_seconds: 86_400,
        max_queue_length: 100,
        max_submission_attempts: 5,
        confirmation_timeout_seconds: 900,
        enabled: true,
        captcha_provider: "turnstile".into(),
        state_path: root.join("faucet.db"),
        signer_credential: signer_path.to_string_lossy().into_owned(),
        turnstile_secret_credential: "preview-unused".into(),
        abuse_key_credential: "preview-unused".into(),
        operator_token_credential: "preview-unused".into(),
    };
    let store = FaucetStore::open(&config.state_path).map_err(|error| error.to_string())?;
    let node = Arc::new(PreviewNode::new(faucet_address.clone()));
    let service = Arc::new(
        FaucetService::new(
            config,
            store,
            signer,
            node,
            StaticCaptchaVerifier::new(true),
            Zeroizing::new(random_bytes(32)),
            Zeroizing::new(PREVIEW_OPERATOR_TOKEN.to_vec()),
        )
        .map_err(|error| error.to_string())?,
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker = tokio::spawn(service.clone().run_worker(shutdown_rx.clone()));
    println!("UltraNet faucet preview listening on http://{bind}");
    println!("Preview faucet address: {faucet_address}");
    println!("Preview state directory: {}", root.display());
    println!("Preview only: no live node, Turnstile, or production credentials are used.");
    let result = tokio::select! {
        result = api::run_server(service, shutdown_rx) => result.map_err(|error| error.to_string()),
        result = tokio::signal::ctrl_c() => {
            result.map_err(|error| error.to_string())?;
            shutdown_tx.send(true).map_err(|_| "preview worker already stopped".to_string())?;
            Ok(())
        }
    };
    let _ = shutdown_tx.send(true);
    worker.abort();
    let _ = fs::remove_dir_all(&root);
    result
}

fn preview_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!("ultranet-faucet-preview-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|error| format!("cannot reset preview directory: {error}"))?;
    }
    fs::create_dir(&root).map_err(|error| format!("cannot create preview directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("cannot restrict preview directory: {error}"))?;
    }
    Ok(root)
}

fn write_signer_credential(path: &Path, keypair: &QuantumKeyPair) -> Result<(), String> {
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
    let mut file = options
        .open(path)
        .map_err(|error| format!("cannot create preview signer credential: {error}"))?;
    file.write_all(contents.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| format!("cannot write preview signer credential: {error}"))?;
    Ok(())
}

fn random_bytes(length: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; length];
    OsRng.fill_bytes(&mut bytes);
    bytes
}
