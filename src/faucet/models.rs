use crate::{Transaction, UltraBlockchain};
use serde::{Deserialize, Serialize};

pub const DECIMALS: u8 = UltraBlockchain::ULTRA_DECIMALS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    Queued,
    Submitting,
    Pending,
    Confirmed,
    Failed,
}

impl ClaimStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Submitting => "submitting",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "submitting" => Some(Self::Submitting),
            "pending" => Some(Self::Pending),
            "confirmed" => Some(Self::Confirmed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Submitting | Self::Pending | Self::Confirmed
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateClaimRequest {
    pub address: String,
    pub captcha_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimData {
    pub claim_id: String,
    pub status: ClaimStatus,
    pub address: String,
    pub amount_base_units: u64,
    pub amount_ultra: String,
    pub decimals: u8,
    pub retry_after_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimStatusData {
    pub claim_id: String,
    pub status: ClaimStatus,
    pub amount_base_units: u64,
    pub amount_ultra: String,
    pub decimals: u8,
    pub transaction_hash: Option<String>,
    pub submitted_at: Option<u64>,
    pub confirmed_at: Option<u64>,
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicStatusData {
    pub enabled: bool,
    pub availability: &'static str,
    pub claim_amount_base_units: u64,
    pub claim_amount_ultra: String,
    pub decimals: u8,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTransferRequest {
    pub sender: String,
    pub sender_public_key: Vec<u8>,
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub timestamp: u64,
    pub nullifier: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub signature: Vec<u8>,
    pub chain_id: u32,
    pub version: u32,
}

impl SignedTransferRequest {
    pub fn from_transaction(transaction: &Transaction) -> Self {
        Self {
            sender: transaction.sender.clone(),
            sender_public_key: transaction.sender_public_key.clone(),
            recipient: transaction.recipient.clone(),
            amount: transaction.amount,
            fee: transaction.fee,
            nonce: transaction.nonce,
            timestamp: transaction.timestamp,
            nullifier: transaction.nullifier.to_vec(),
            gas_limit: transaction.gas_limit,
            gas_price: transaction.gas_price,
            signature: transaction.signature.clone(),
            chain_id: transaction.chain_id,
            version: transaction.version,
        }
    }

    pub fn nullifier_array(&self) -> Result<[u8; 32], String> {
        self.nullifier
            .as_slice()
            .try_into()
            .map_err(|_| "signed transfer nullifier must contain exactly 32 bytes".to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeEnvelope<T> {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeAccountData {
    pub address: String,
    #[serde(default)]
    pub balance_base_units: Option<u64>,
    #[serde(default)]
    pub balance: Option<u64>,
    pub nonce: u64,
    pub decimals: u8,
}

impl NodeAccountData {
    pub fn balance_base_units(&self) -> Option<u64> {
        self.balance_base_units.or(self.balance)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeFeeEstimateData {
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeTransactionData {
    pub hash: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCode {
    NodeUnavailable,
    NodeRejected,
    NodeHashMismatch,
    SignerUnavailable,
    BudgetDisabled,
    ConfirmationTimeout,
    UnexpectedNonce,
}

impl FailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NodeUnavailable => "NODE_UNAVAILABLE",
            Self::NodeRejected => "NODE_REJECTED",
            Self::NodeHashMismatch => "NODE_HASH_MISMATCH",
            Self::SignerUnavailable => "SIGNER_UNAVAILABLE",
            Self::BudgetDisabled => "BUDGET_DISABLED",
            Self::ConfirmationTimeout => "CONFIRMATION_TIMEOUT",
            Self::UnexpectedNonce => "UNEXPECTED_NONCE",
        }
    }
}
