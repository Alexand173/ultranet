use std::{env, net::SocketAddr, path::PathBuf};

use crate::UltraBlockchain;

pub const DEFAULT_BIND: &str = "127.0.0.1:8090";
pub const DEFAULT_NODE_API_BASE_URL: &str = "http://127.0.0.1:8081";
pub const DEFAULT_FAUCET_ADDRESS: &str =
    "787e68b2c5ac93d3eaaa5db72ab4bb0404e1ef3f4315e0c83d557a09d800a358";
pub const DEFAULT_CLAIM_AMOUNT_BASE_UNITS: u64 = 1_000_000;
pub const DEFAULT_DAILY_DEBIT_CAP_BASE_UNITS: u64 = 100_000_000;
pub const DEFAULT_MIN_BALANCE_RESERVE_BASE_UNITS: u64 = 200_000_000;
pub const DEFAULT_ADDRESS_COOLDOWN_SECONDS: u64 = 86_400;
pub const DEFAULT_MAX_QUEUE_LENGTH: usize = 100;
pub const DEFAULT_MAX_SUBMISSION_ATTEMPTS: u32 = 5;
pub const DEFAULT_CONFIRMATION_TIMEOUT_SECONDS: u64 = 900;
pub const POLICY_VERSION: &str = "mainnet-beta-v1";

#[derive(Debug, Clone)]
pub struct FaucetConfig {
    pub bind: SocketAddr,
    pub node_api_base_url: String,
    pub faucet_address: String,
    pub claim_amount_base_units: u64,
    pub daily_debit_cap_base_units: u64,
    pub min_balance_reserve_base_units: u64,
    pub address_cooldown_seconds: u64,
    pub max_queue_length: usize,
    pub max_submission_attempts: u32,
    pub confirmation_timeout_seconds: u64,
    pub enabled: bool,
    pub captcha_provider: String,
    pub state_path: PathBuf,
    pub signer_credential: String,
    pub turnstile_secret_credential: String,
    pub abuse_key_credential: String,
    pub operator_token_credential: String,
}

impl FaucetConfig {
    pub fn from_env() -> Result<Self, String> {
        let bind = parse_bind(&env_string("FAUCET_BIND", DEFAULT_BIND.to_string())?)?;
        let node_api_base_url = env_string(
            "FAUCET_NODE_API_BASE_URL",
            DEFAULT_NODE_API_BASE_URL.to_string(),
        )?
        .trim_end_matches('/')
        .to_string();
        validate_node_api_url(&node_api_base_url)?;

        let faucet_address = env_string("FAUCET_ADDRESS", DEFAULT_FAUCET_ADDRESS.to_string())?;
        if !UltraBlockchain::is_valid_address(&faucet_address) {
            return Err(
                "FAUCET_ADDRESS must be a 64-character lowercase hexadecimal address".into(),
            );
        }

        let config = Self {
            bind,
            node_api_base_url,
            faucet_address,
            claim_amount_base_units: parse_u64(
                "FAUCET_CLAIM_AMOUNT_BASE_UNITS",
                DEFAULT_CLAIM_AMOUNT_BASE_UNITS,
            )?,
            daily_debit_cap_base_units: parse_u64(
                "FAUCET_DAILY_DEBIT_CAP_BASE_UNITS",
                DEFAULT_DAILY_DEBIT_CAP_BASE_UNITS,
            )?,
            min_balance_reserve_base_units: parse_u64(
                "FAUCET_MIN_BALANCE_RESERVE_BASE_UNITS",
                DEFAULT_MIN_BALANCE_RESERVE_BASE_UNITS,
            )?,
            address_cooldown_seconds: parse_u64(
                "FAUCET_ADDRESS_COOLDOWN_SECONDS",
                DEFAULT_ADDRESS_COOLDOWN_SECONDS,
            )?,
            max_queue_length: parse_usize("FAUCET_MAX_QUEUE_LENGTH", DEFAULT_MAX_QUEUE_LENGTH)?,
            max_submission_attempts: parse_u32(
                "FAUCET_MAX_SUBMISSION_ATTEMPTS",
                DEFAULT_MAX_SUBMISSION_ATTEMPTS,
            )?,
            confirmation_timeout_seconds: parse_u64(
                "FAUCET_CONFIRMATION_TIMEOUT_SECONDS",
                DEFAULT_CONFIRMATION_TIMEOUT_SECONDS,
            )?,
            enabled: parse_bool("FAUCET_ENABLED", false)?,
            captcha_provider: env_string("FAUCET_CAPTCHA_PROVIDER", "turnstile".into())?,
            state_path: PathBuf::from(env_string(
                "FAUCET_STATE_PATH",
                "/var/lib/ultranet-faucet/faucet.db".into(),
            )?),
            signer_credential: credential_name("FAUCET_SIGNER_CREDENTIAL", "faucet-signer.json")?,
            turnstile_secret_credential: credential_name(
                "FAUCET_TURNSTILE_CREDENTIAL",
                "faucet-turnstile.secret",
            )?,
            abuse_key_credential: credential_name(
                "FAUCET_ABUSE_KEY_CREDENTIAL",
                "faucet-abuse.key",
            )?,
            operator_token_credential: credential_name(
                "FAUCET_OPERATOR_TOKEN_CREDENTIAL",
                "faucet-operator.token",
            )?,
        };
        config.validate()
    }

    pub fn validate(mut self) -> Result<Self, String> {
        if !self.bind.ip().is_loopback() {
            return Err("FAUCET_BIND must bind to a loopback address".into());
        }
        validate_node_api_url(&self.node_api_base_url)?;
        if !UltraBlockchain::is_valid_address(&self.faucet_address) {
            return Err("faucet address is not a canonical UltraNet address".into());
        }
        if self.claim_amount_base_units == 0
            || self.claim_amount_base_units > UltraBlockchain::MAX_TRANSFER_AMOUNT
        {
            return Err("FAUCET_CLAIM_AMOUNT_BASE_UNITS is outside the transfer range".into());
        }
        let minimum_fee = UltraBlockchain::minimum_transfer_fee(self.claim_amount_base_units);
        let claim_debit = self
            .claim_amount_base_units
            .checked_add(minimum_fee)
            .ok_or_else(|| "claim amount plus fee overflows u64".to_string())?;
        if self.daily_debit_cap_base_units < claim_debit {
            return Err("daily debit cap must cover at least one complete claim".into());
        }
        if self.min_balance_reserve_base_units == 0 {
            return Err("FAUCET_MIN_BALANCE_RESERVE_BASE_UNITS must be greater than zero".into());
        }
        if self.max_queue_length == 0 || self.max_submission_attempts == 0 {
            return Err("queue length and submission attempts must be greater than zero".into());
        }
        if self.confirmation_timeout_seconds == 0 || self.address_cooldown_seconds == 0 {
            return Err("cooldown and confirmation timeout must be greater than zero".into());
        }
        if self.captcha_provider != "turnstile" {
            return Err("FAUCET_CAPTCHA_PROVIDER must be turnstile".into());
        }
        if self.state_path.as_os_str().is_empty() {
            return Err("FAUCET_STATE_PATH cannot be empty".into());
        }
        for (name, value) in [
            ("FAUCET_SIGNER_CREDENTIAL", &self.signer_credential),
            (
                "FAUCET_TURNSTILE_CREDENTIAL",
                &self.turnstile_secret_credential,
            ),
            ("FAUCET_ABUSE_KEY_CREDENTIAL", &self.abuse_key_credential),
            (
                "FAUCET_OPERATOR_TOKEN_CREDENTIAL",
                &self.operator_token_credential,
            ),
        ] {
            validate_credential_name(name, value)?;
        }
        self.node_api_base_url = self.node_api_base_url.trim_end_matches('/').to_string();
        Ok(self)
    }

    pub fn credential_path(&self, credential_name: &str) -> PathBuf {
        if let Ok(directory) = env::var("CREDENTIALS_DIRECTORY") {
            return PathBuf::from(directory).join(credential_name);
        }
        PathBuf::from(credential_name)
    }

    #[cfg(test)]
    pub fn for_tests(state_path: PathBuf, faucet_address: String) -> Self {
        Self {
            bind: "127.0.0.1:0".parse().unwrap(),
            node_api_base_url: DEFAULT_NODE_API_BASE_URL.into(),
            faucet_address,
            claim_amount_base_units: DEFAULT_CLAIM_AMOUNT_BASE_UNITS,
            daily_debit_cap_base_units: DEFAULT_DAILY_DEBIT_CAP_BASE_UNITS,
            min_balance_reserve_base_units: DEFAULT_MIN_BALANCE_RESERVE_BASE_UNITS,
            address_cooldown_seconds: DEFAULT_ADDRESS_COOLDOWN_SECONDS,
            max_queue_length: DEFAULT_MAX_QUEUE_LENGTH,
            max_submission_attempts: DEFAULT_MAX_SUBMISSION_ATTEMPTS,
            confirmation_timeout_seconds: DEFAULT_CONFIRMATION_TIMEOUT_SECONDS,
            enabled: true,
            captcha_provider: "turnstile".into(),
            state_path,
            signer_credential: "unused-signer".into(),
            turnstile_secret_credential: "unused-turnstile".into(),
            abuse_key_credential: "unused-abuse".into(),
            operator_token_credential: "unused-operator".into(),
        }
    }
}

fn env_string(name: &str, default: String) -> Result<String, String> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn credential_name(name: &str, default: &str) -> Result<String, String> {
    env_string(name, default.to_string())
}

fn parse_bind(value: &str) -> Result<SocketAddr, String> {
    value
        .trim()
        .parse::<SocketAddr>()
        .map_err(|error| format!("FAUCET_BIND must be a valid IP:port address: {error}"))
}

fn validate_node_api_url(value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| format!("FAUCET_NODE_API_BASE_URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("FAUCET_NODE_API_BASE_URL must be an http(s) URL".into());
    }
    if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")) {
        return Err("FAUCET_NODE_API_BASE_URL must target loopback".into());
    }
    Ok(())
}

fn parse_u64(name: &str, default: u64) -> Result<u64, String> {
    env_string(name, default.to_string())?
        .trim()
        .parse()
        .map_err(|_| format!("{name} must be an unsigned integer"))
}

fn parse_u32(name: &str, default: u32) -> Result<u32, String> {
    env_string(name, default.to_string())?
        .trim()
        .parse()
        .map_err(|_| format!("{name} must be an unsigned integer"))
}

fn parse_usize(name: &str, default: usize) -> Result<usize, String> {
    env_string(name, default.to_string())?
        .trim()
        .parse()
        .map_err(|_| format!("{name} must be an unsigned integer"))
}

fn parse_bool(name: &str, default: bool) -> Result<bool, String> {
    match env_string(name, default.to_string())?.trim() {
        "true" | "TRUE" | "1" | "yes" | "YES" => Ok(true),
        "false" | "FALSE" | "0" | "no" | "NO" => Ok(false),
        _ => Err(format!("{name} must be true or false")),
    }
}

fn validate_credential_name(name: &str, value: &str) -> Result<(), String> {
    let path = std::path::Path::new(value);
    if value.trim().is_empty()
        || value.contains('\0')
        || (!path.is_absolute()
            && value
                .split(std::path::MAIN_SEPARATOR)
                .any(|part| part == ".."))
    {
        return Err(format!(
            "{name} must be a safe credential name or absolute path"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_for_local_loopback() {
        let config = FaucetConfig::for_tests("faucet-test.db".into(), "a".repeat(64));
        assert!(config.clone().validate().is_ok());
    }

    #[test]
    fn http_remote_node_is_rejected() {
        assert!(validate_node_api_url("http://203.0.113.5:8081").is_err());
        assert!(validate_node_api_url("https://203.0.113.5:8081").is_err());
    }

    #[test]
    fn daily_cap_covers_the_complete_debit() {
        let mut config = FaucetConfig::for_tests("faucet-test.db".into(), "a".repeat(64));
        config.daily_debit_cap_base_units = config.claim_amount_base_units;
        assert!(config.validate().is_err());
    }
}
