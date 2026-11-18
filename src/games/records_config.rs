//! Configuration of the Records Service, read from `RECORDS_*` variables and systemd credentials.
//!
//! This service holds the chain owner's key (as `appchain-cli`'s encrypted key file and its passphrase)
//! and nothing that pays anyone. It refuses to start if the Games Reward Service's payout credential is
//! present in its credentials directory: two services that hold each other's secrets are one service.

use super::{
    records::RecordsConfig,
    registry::RegistryCaps,
};
use std::{env, net::SocketAddr, path::PathBuf};

pub const MAX_RESULTS_PER_GAME_PER_DAY: u64 = 1_000_000;
pub const MAX_RESULTS_PER_PLAYER_PER_DAY: u64 = 100_000;
pub const MAX_SANDBOX_RESULTS_PER_DAY: u64 = 10_000;
pub const MAX_GAMES_PER_OWNER: u64 = 20;
pub const MAX_SELF_REGISTRATIONS_PER_DAY: u64 = 5_000;
pub const MAX_PEER_ANCHORS_PER_DAY: u64 = 100;
pub const MAX_PEER_SUBMISSIONS_PER_ROOT: u64 = 10;
pub const DEFAULT_NODE_API_BASE_URL: &str = "http://127.0.0.1:8081";

/// Credentials this service must never hold: the Games Reward Service's payout key and its abuse key.
pub const FOREIGN_CREDENTIALS: &[&str] = &["games-payout-signer.json", "games-abuse.key"];

#[derive(Clone, Debug)]
pub struct AnchorConfig {
    pub cli_path: PathBuf,
    pub key_credential: String,
    pub password_credential: String,
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug)]
pub struct RecordsServiceConfig {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    pub node_api_base_url: String,
    pub caps: RegistryCaps,
    pub service: RecordsConfig,
    /// None: days are closed (proofs can be given) but never anchored.
    pub anchor: Option<AnchorConfig>,
    /// The secret shared with the Games Reward Service. Unset: the `/internal` route does not exist.
    pub peer_credential: Option<String>,
}

impl RecordsServiceConfig {
    pub fn from_env() -> Result<RecordsServiceConfig, String> {
        Self::from_lookup(&|name| env::var(name).ok())
    }

    pub fn from_lookup(
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<RecordsServiceConfig, String> {
        let text = |name: &str, default: &str| lookup(name).unwrap_or_else(|| default.to_string());
        let number = |name: &str, default: u64| -> Result<u64, String> {
            match lookup(name) {
                None => Ok(default),
                Some(value) => value
                    .trim()
                    .parse()
                    .map_err(|_| format!("{name} must be an unsigned integer")),
            }
        };
        let boolean = |name: &str, default: bool| -> Result<bool, String> {
            match lookup(name).as_deref().map(str::trim) {
                None => Ok(default),
                Some("true") | Some("1") | Some("yes") => Ok(true),
                Some("false") | Some("0") | Some("no") => Ok(false),
                Some(_) => Err(format!("{name} must be true or false")),
            }
        };
        let within = |name: &str, default: u64, ceiling: u64| -> Result<u64, String> {
            let value = number(name, default)?;
            if value == 0 || value > ceiling {
                return Err(format!("{name} must be 1 to {ceiling}"));
            }
            Ok(value)
        };

        let caps = RegistryCaps {
            results_per_game_per_day: within(
                "RECORDS_RESULTS_PER_GAME_PER_DAY",
                RegistryCaps::standard().results_per_game_per_day,
                MAX_RESULTS_PER_GAME_PER_DAY,
            )?,
            results_per_player_per_game_per_day: within(
                "RECORDS_RESULTS_PER_PLAYER_PER_DAY",
                RegistryCaps::standard().results_per_player_per_game_per_day,
                MAX_RESULTS_PER_PLAYER_PER_DAY,
            )?,
            sandbox_results_per_game_per_day: within(
                "RECORDS_SANDBOX_RESULTS_PER_DAY",
                RegistryCaps::standard().sandbox_results_per_game_per_day,
                MAX_SANDBOX_RESULTS_PER_DAY,
            )?,
            games_per_owner: within(
                "RECORDS_GAMES_PER_OWNER",
                RegistryCaps::standard().games_per_owner,
                MAX_GAMES_PER_OWNER,
            )?,
            self_registrations_per_day: within(
                "RECORDS_SELF_REGISTRATIONS_PER_DAY",
                RegistryCaps::standard().self_registrations_per_day,
                MAX_SELF_REGISTRATIONS_PER_DAY,
            )?,
        };

        let mut service = RecordsConfig::new();
        service.require_chain_presence =
            boolean("RECORDS_REQUIRE_CHAIN_PRESENCE", service.require_chain_presence)?;
        service.anchor_retry_seconds =
            number("RECORDS_ANCHOR_RETRY_SECONDS", service.anchor_retry_seconds)?;
        if let Some(list) = lookup("RECORDS_PEER_ANCHOR_CHAINS") {
            service.peer_anchor_chains = list
                .split(',')
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .map(|item| {
                    item.parse::<u32>()
                        .map_err(|_| "RECORDS_PEER_ANCHOR_CHAINS must be comma-separated chain ids".to_string())
                })
                .collect::<Result<Vec<u32>, String>>()?;
            if service.peer_anchor_chains.contains(&0) {
                return Err("RECORDS_PEER_ANCHOR_CHAINS must not include chain 0 (the L1)".into());
            }
        }
        service.peer_anchors_per_day = within(
            "RECORDS_PEER_ANCHORS_PER_DAY",
            service.peer_anchors_per_day as u64,
            MAX_PEER_ANCHORS_PER_DAY,
        )? as u32;
        service.peer_submissions_per_root = within(
            "RECORDS_PEER_SUBMISSIONS_PER_ROOT",
            service.peer_submissions_per_root as u64,
            MAX_PEER_SUBMISSIONS_PER_ROOT,
        )? as u32;
        service.owner_address = match lookup("RECORDS_OWNER_ADDRESS") {
            None => None,
            Some(value) => {
                let value = value.trim().to_ascii_lowercase();
                if !crate::UltraBlockchain::is_valid_address(&value) {
                    return Err("RECORDS_OWNER_ADDRESS must be 64 lowercase hexadecimal characters".into());
                }
                Some(value)
            }
        };
        if lookup("RECORDS_ANCHOR_CLI").is_some() && service.owner_address.is_none() {
            return Err(
                "RECORDS_OWNER_ADDRESS is required when RECORDS_ANCHOR_CLI is set: an anchor whose outcome is unknown, for a peer or for the registry's own day, can only be reconciled by reading the owner's transaction history".into(),
            );
        }
        if lookup("RECORDS_PEER_CREDENTIAL").is_some() && service.owner_address.is_none() {
            return Err(
                "RECORDS_OWNER_ADDRESS is required when RECORDS_PEER_CREDENTIAL is set: an anchor whose outcome is unknown can only be reconciled by reading the owner's transaction history".into(),
            );
        }
        service.registry_chain_id = match lookup("RECORDS_REGISTRY_CHAIN_ID") {
            None => None,
            Some(value) => {
                let id: u32 = value.trim().parse().map_err(|_| {
                    "RECORDS_REGISTRY_CHAIN_ID must be an unsigned integer".to_string()
                })?;
                if id == 0 || service.peer_anchor_chains.contains(&id) {
                    return Err(
                        "RECORDS_REGISTRY_CHAIN_ID must be an AppChain of its own: not 0 (the L1) and not a chain a peer may anchor".into(),
                    );
                }
                Some(id)
            }
        };

        let anchor = match lookup("RECORDS_ANCHOR_CLI") {
            None => None,
            Some(cli) => {
                let cli_path = PathBuf::from(cli.trim());
                if !cli_path.is_absolute() {
                    return Err("RECORDS_ANCHOR_CLI must be an absolute path".into());
                }
                service.anchor_timeout_seconds = number("RECORDS_ANCHOR_TIMEOUT_SECONDS", 60)?;
                Some(AnchorConfig {
                    cli_path,
                    key_credential: text(
                        "RECORDS_ANCHOR_KEY_CREDENTIAL",
                        "records-anchor-owner.key.enc",
                    ),
                    password_credential: text(
                        "RECORDS_ANCHOR_PASSWORD_CREDENTIAL",
                        "records-anchor.pass",
                    ),
                    timeout_seconds: number("RECORDS_ANCHOR_TIMEOUT_SECONDS", 60)?,
                })
            }
        };

        Ok(RecordsServiceConfig {
            bind: text("RECORDS_BIND", "127.0.0.1:8096")
                .parse()
                .map_err(|_| "RECORDS_BIND must be a socket address".to_string())?,
            db_path: PathBuf::from(text(
                "RECORDS_DB_PATH",
                "/var/lib/ultranet-records/records.db",
            )),
            node_api_base_url: text("RECORDS_NODE_API_BASE_URL", DEFAULT_NODE_API_BASE_URL),
            caps,
            service,
            anchor,
            peer_credential: lookup("RECORDS_PEER_CREDENTIAL"),
        })
    }

    pub fn credential_path(&self, name: &str) -> PathBuf {
        match env::var("CREDENTIALS_DIRECTORY") {
            Ok(directory) => PathBuf::from(directory).join(name),
            Err(_) => PathBuf::from(name),
        }
    }

    /// The same check against an explicit directory (so a test need not touch the environment).
    pub fn foreign_credentials_in(directory: &std::path::Path) -> Vec<PathBuf> {
        FOREIGN_CREDENTIALS
            .iter()
            .map(|name| directory.join(name))
            .filter(|path| path.exists())
            .collect()
    }

    /// Any credential this service must not hold that is present in its credentials directory.
    pub fn foreign_credentials_present(&self) -> Vec<PathBuf> {
        FOREIGN_CREDENTIALS
            .iter()
            .map(|name| self.credential_path(name))
            .filter(|path| path.exists())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(pairs: &[(&str, &str)]) -> Result<RecordsServiceConfig, String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        RecordsServiceConfig::from_lookup(&|name| map.get(name).cloned())
    }

    #[test]
    fn the_defaults_anchor_nothing_and_open_no_peer_route() {
        let c = config(&[]).unwrap();
        assert_eq!(c.bind, "127.0.0.1:8096".parse().unwrap());
        assert_eq!(c.caps, RegistryCaps::standard());
        assert!(c.anchor.is_none() && c.peer_credential.is_none());
        assert_eq!(c.service.registry_chain_id, None);
        assert_eq!(c.service.peer_anchor_chains, vec![1, 2]);
        assert!(c.service.require_chain_presence);
    }

    #[test]
    fn the_registry_needs_a_chain_of_its_own_that_a_peer_cannot_anchor() {
        assert_eq!(
            config(&[("RECORDS_REGISTRY_CHAIN_ID", "3")]).unwrap().service.registry_chain_id,
            Some(3)
        );
        for bad in ["0", "1", "2", "-1", "three", ""] {
            assert!(config(&[("RECORDS_REGISTRY_CHAIN_ID", bad)]).is_err(), "{bad:?}");
        }
        // If the peer is allowed another chain, the registry may not use it.
        assert!(config(&[("RECORDS_PEER_ANCHOR_CHAINS", "1,2,7"), ("RECORDS_REGISTRY_CHAIN_ID", "7")]).is_err());
        assert!(config(&[("RECORDS_PEER_ANCHOR_CHAINS", "1"), ("RECORDS_REGISTRY_CHAIN_ID", "2")]).is_ok());
        assert!(config(&[("RECORDS_PEER_ANCHOR_CHAINS", "0,1")]).is_err());
        assert!(config(&[("RECORDS_PEER_ANCHOR_CHAINS", "a,b")]).is_err());
    }

    #[test]
    fn limits_cannot_be_loosened_past_their_ceilings_or_set_to_zero() {
        let ceilings = [
            ("RECORDS_RESULTS_PER_GAME_PER_DAY", MAX_RESULTS_PER_GAME_PER_DAY),
            ("RECORDS_RESULTS_PER_PLAYER_PER_DAY", MAX_RESULTS_PER_PLAYER_PER_DAY),
            ("RECORDS_SANDBOX_RESULTS_PER_DAY", MAX_SANDBOX_RESULTS_PER_DAY),
            ("RECORDS_GAMES_PER_OWNER", MAX_GAMES_PER_OWNER),
            ("RECORDS_SELF_REGISTRATIONS_PER_DAY", MAX_SELF_REGISTRATIONS_PER_DAY),
            ("RECORDS_PEER_ANCHORS_PER_DAY", MAX_PEER_ANCHORS_PER_DAY),
            ("RECORDS_PEER_SUBMISSIONS_PER_ROOT", MAX_PEER_SUBMISSIONS_PER_ROOT),
        ];
        for (name, ceiling) in ceilings {
            assert!(config(&[(name, &ceiling.to_string())]).is_ok(), "{name} at its ceiling");
            assert!(config(&[(name, &(ceiling + 1).to_string())]).is_err(), "{name} over it");
            assert!(config(&[(name, "0")]).is_err(), "{name} at zero");
        }
    }

    #[test]
    fn a_peer_route_needs_the_owner_address_so_ambiguous_anchors_can_be_reconciled() {
        let owner = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
        let error = config(&[("RECORDS_PEER_CREDENTIAL", "records-peer-games.key")]).unwrap_err();
        assert!(error.contains("RECORDS_OWNER_ADDRESS"), "{error}");
        let ok = config(&[
            ("RECORDS_PEER_CREDENTIAL", "records-peer-games.key"),
            ("RECORDS_OWNER_ADDRESS", owner),
        ])
        .unwrap();
        assert_eq!(ok.service.owner_address.as_deref(), Some(owner));
        for bad in ["abc", &owner.to_uppercase()[..63], "zz"] {
            assert!(config(&[("RECORDS_OWNER_ADDRESS", bad)]).is_err(), "{bad}");
        }
        // Without a peer route the address is optional.
        assert!(config(&[]).unwrap().service.owner_address.is_none());
    }

    #[test]
    fn anchoring_needs_an_absolute_cli_path_and_this_service_reads_only_its_own_variables() {
        let owner = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
        assert!(config(&[("RECORDS_ANCHOR_CLI", "appchain-cli"), ("RECORDS_OWNER_ADDRESS", owner)]).unwrap_err().contains("absolute"));
        // Any anchoring, the registry's included, needs the owner address.
        assert!(config(&[("RECORDS_ANCHOR_CLI", "/opt/ultranet-records/appchain-cli")]).unwrap_err().contains("RECORDS_OWNER_ADDRESS"));
        let ok = config(&[("RECORDS_ANCHOR_CLI", "/opt/ultranet-records/appchain-cli"), ("RECORDS_OWNER_ADDRESS", owner)]).unwrap();
        let anchor = ok.anchor.unwrap();
        assert_eq!(anchor.key_credential, "records-anchor-owner.key.enc");
        assert_eq!(anchor.timeout_seconds, 60);
        // The Games Reward Service's variables mean nothing here.
        let ignored = config(&[("GAMES_PAYOUT_ADDRESS", "x"), ("GAMES_ANCHOR_CLI", "/x/y"), ("GAMES_REGISTRY_CHAIN_ID", "3")]).unwrap();
        assert!(ignored.anchor.is_none() && ignored.service.registry_chain_id.is_none());
        assert!(FOREIGN_CREDENTIALS.contains(&"games-payout-signer.json"));
    }
}
