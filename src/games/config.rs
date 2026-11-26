//! Environment configuration for the games service, with hard ceilings.
//!
//! An operator may tighten any rule from the environment. They may not loosen one past the ceilings
//! below, because those ceilings are what the rules review (`docs/GAMES_RULES_AND_PAYOUTS.md`) rests
//! on: with them, the worst a single address can earn in a day and the worst the whole service can pay
//! in a day are both small, known numbers. A value over a ceiling stops the service starting, with a
//! message saying which, instead of being quietly clamped.
//!
//! Secrets never come from here. The payout signer, the abuse key and the chain owner's key and
//! passphrase arrive as systemd credentials (files), exactly as the faucet's do.

use super::{
    rules::{GamePolicy, Policy},
    service::ServiceConfig,
};
use std::{env, net::SocketAddr, path::PathBuf};

// --- ceilings ---------------------------------------------------------------------------------

/// 0.001 `$ULTRA`: a prize is a token of a win, not income.
pub const MAX_REWARD_UNITS: u64 = 1_000;
pub const MAX_REWARDED_PER_ADDRESS_PER_DAY: u32 = 10;
pub const MAX_REWARDED_PER_GAME_PER_DAY: u64 = 10_000;
pub const MAX_REWARDED_GLOBAL_PER_DAY: u64 = 100_000;
pub const MAX_REWARDED_PER_CLIENT_PER_DAY: u32 = 1_000;
/// The network fee floor is one unit per 100, so a smaller minimum would make a payout mostly fee.
pub const MIN_WITHDRAW_UNITS: u64 = 100;
pub const MAX_WITHDRAW_MIN_UNITS: u64 = 10_000_000;
/// 5 `$ULTRA` a day, however large the wallet.
pub const MAX_PAYOUT_DAILY_CAP_UNITS: u64 = 5_000_000;
/// The reserve covers the network fee on each payout (1 unit in 100, at least 1). Below this it could not
/// cover a burst of withdrawals, and the solvency rule would promise money the fees then eat.
pub const MIN_PAYOUT_RESERVE_UNITS: u64 = 100;
pub const MAX_PAYOUT_RESERVE_UNITS: u64 = 1_000_000_000;
pub const MIN_CHESS_PLIES: u32 = 10;
pub const MAX_CHESS_PLIES: u32 = 1_000;

/// Refuse a policy that loosens a rule past its ceiling.
pub fn validate_policy(policy: &Policy) -> Result<(), String> {
    for game in [&policy.grid, &policy.chess] {
        let name = game.game;
        if game.reward_units == 0 || game.reward_units > MAX_REWARD_UNITS {
            return Err(format!(
                "{name}: the reward must be 1 to {MAX_REWARD_UNITS} base units"
            ));
        }
        if game.rewarded_per_address_per_day == 0
            || game.rewarded_per_address_per_day > MAX_REWARDED_PER_ADDRESS_PER_DAY
        {
            return Err(format!(
                "{name}: rewarded wins per address per day must be 1 to {MAX_REWARDED_PER_ADDRESS_PER_DAY}"
            ));
        }
        if game.rewarded_per_day == 0 || game.rewarded_per_day > MAX_REWARDED_PER_GAME_PER_DAY {
            return Err(format!(
                "{name}: rewarded wins per day must be 1 to {MAX_REWARDED_PER_GAME_PER_DAY}"
            ));
        }
        if game.chain_id == 0 {
            return Err(format!("{name}: chain id 0 is the L1, not an AppChain"));
        }
    }
    if policy.grid.chain_id == policy.chess.chain_id {
        return Err("the two games must be anchored to different AppChains".into());
    }
    if policy.rewarded_global_per_day == 0
        || policy.rewarded_global_per_day > MAX_REWARDED_GLOBAL_PER_DAY
    {
        return Err(format!(
            "the global daily ceiling must be 1 to {MAX_REWARDED_GLOBAL_PER_DAY}"
        ));
    }
    if policy.rewarded_per_client_per_day == 0
        || policy.rewarded_per_client_per_day > MAX_REWARDED_PER_CLIENT_PER_DAY
    {
        return Err(format!(
            "the per-client daily limit must be 1 to {MAX_REWARDED_PER_CLIENT_PER_DAY}"
        ));
    }
    if policy.withdraw_min_units < MIN_WITHDRAW_UNITS
        || policy.withdraw_min_units > MAX_WITHDRAW_MIN_UNITS
    {
        return Err(format!(
            "the withdrawal minimum must be {MIN_WITHDRAW_UNITS} to {MAX_WITHDRAW_MIN_UNITS} base units"
        ));
    }
    if policy.payout_daily_cap_units < policy.withdraw_min_units
        || policy.payout_daily_cap_units > MAX_PAYOUT_DAILY_CAP_UNITS
    {
        return Err(format!(
            "the daily payout cap must be the withdrawal minimum to {MAX_PAYOUT_DAILY_CAP_UNITS} base units"
        ));
    }
    if policy.payout_reserve_units < MIN_PAYOUT_RESERVE_UNITS
        || policy.payout_reserve_units > MAX_PAYOUT_RESERVE_UNITS
    {
        return Err(format!(
            "the payout reserve must be {MIN_PAYOUT_RESERVE_UNITS} to {MAX_PAYOUT_RESERVE_UNITS} base units"
        ));
    }
    if policy.chess_min_plies < MIN_CHESS_PLIES
        || policy.chess_max_plies > MAX_CHESS_PLIES
        || policy.chess_min_plies >= policy.chess_max_plies
    {
        return Err(format!(
            "chess plies must be within {MIN_CHESS_PLIES} to {MAX_CHESS_PLIES}"
        ));
    }
    Ok(())
}

// --- configuration ----------------------------------------------------------------------------

/// How this service reaches the Records Service to have a day anchored.
#[derive(Clone, Debug)]
pub struct PeerAnchorConfig {
    pub records_url: String,
    pub credential: String,
    pub timeout_seconds: u64,
}

/// Credentials this service must never hold: the chain owner's key and its passphrase, under every name
/// the Records Service might know them by. `check-config` and `serve` refuse to start if one is present.
pub const FOREIGN_CREDENTIALS: &[&str] = &[
    "games-anchor-owner.key.enc",
    "games-anchor.pass",
    "records-anchor-owner.key.enc",
    "records-anchor.pass",
];

#[derive(Clone, Debug)]
pub struct GamesConfig {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    pub node_api_base_url: String,
    pub policy: Policy,
    pub service: ServiceConfig,
    pub payout_signer_credential: String,
    pub abuse_key_credential: String,
    /// None means days are closed by an operator command but never anchored automatically.
    pub anchor: Option<PeerAnchorConfig>,
}

pub const DEFAULT_NODE_API_BASE_URL: &str = "http://127.0.0.1:8081";

impl GamesConfig {
    pub fn from_env() -> Result<GamesConfig, String> {
        Self::from_lookup(&|name| env::var(name).ok())
    }

    /// Read configuration through `lookup`, so tests can supply a map instead of the process environment.
    pub fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Result<GamesConfig, String> {
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
        let small = |name: &str, default: u32| -> Result<u32, String> {
            u32::try_from(number(name, default as u64)?).map_err(|_| format!("{name} is too large"))
        };

        // Which AppChain anchors chess is a deployment fact, so there is no default: a missing line must
        // never fall back to a chain id that belongs to something else (the Registry is #2, Ultra Chess #3).
        let chess_chain_id = match lookup("GAMES_CHESS_CHAIN_ID") {
            None => {
                return Err(
                    "GAMES_CHESS_CHAIN_ID is required and has no default: set it to the AppChain id of Ultra Chess (the Registry is a different chain)"
                        .into(),
                )
            }
            Some(value) => value
                .trim()
                .parse::<u32>()
                .map_err(|_| "GAMES_CHESS_CHAIN_ID must be an unsigned integer".to_string())?,
        };
        let standard = Policy::standard(chess_chain_id);
        let game = |prefix: &str, base: &GamePolicy| -> Result<GamePolicy, String> {
            Ok(GamePolicy {
                game: base.game,
                chain_id: small(&format!("GAMES_{prefix}_CHAIN_ID"), base.chain_id)?,
                reward_units: number(&format!("GAMES_{prefix}_REWARD_UNITS"), base.reward_units)?,
                rewarded_per_address_per_day: small(
                    &format!("GAMES_{prefix}_REWARDED_PER_ADDRESS_PER_DAY"),
                    base.rewarded_per_address_per_day,
                )?,
                rewarded_per_day: number(
                    &format!("GAMES_{prefix}_REWARDED_PER_DAY"),
                    base.rewarded_per_day,
                )?,
                pays: boolean(&format!("GAMES_{prefix}_PAYS"), base.pays)?,
            })
        };
        let policy = Policy {
            grid: game("GRID", &standard.grid)?,
            chess: game("CHESS", &standard.chess)?,
            rewarded_global_per_day: number(
                "GAMES_REWARDED_GLOBAL_PER_DAY",
                standard.rewarded_global_per_day,
            )?,
            rewarded_per_client_per_day: small(
                "GAMES_REWARDED_PER_CLIENT_PER_DAY",
                standard.rewarded_per_client_per_day,
            )?,
            withdraw_min_units: number("GAMES_WITHDRAW_MIN_UNITS", standard.withdraw_min_units)?,
            payout_daily_cap_units: number(
                "GAMES_PAYOUT_DAILY_CAP_UNITS",
                standard.payout_daily_cap_units,
            )?,
            require_chain_presence: boolean(
                "GAMES_REQUIRE_CHAIN_PRESENCE",
                standard.require_chain_presence,
            )?,
            chess_min_plies: small("GAMES_CHESS_MIN_PLIES", standard.chess_min_plies)?,
            chess_max_plies: small("GAMES_CHESS_MAX_PLIES", standard.chess_max_plies)?,
            payout_reserve_units: number(
                "GAMES_PAYOUT_RESERVE_UNITS",
                standard.payout_reserve_units,
            )?,
        };
        validate_policy(&policy)?;

        let payout_address = lookup("GAMES_PAYOUT_ADDRESS")
            .map(|value| value.trim().to_ascii_lowercase())
            .ok_or("GAMES_PAYOUT_ADDRESS is required")?;
        if !crate::UltraBlockchain::is_valid_address(&payout_address) {
            return Err("GAMES_PAYOUT_ADDRESS must be 64 lowercase hexadecimal characters".into());
        }

        let mut service = ServiceConfig::new(payout_address);
        service.confirmation_timeout_seconds = number(
            "GAMES_CONFIRMATION_TIMEOUT_SECONDS",
            service.confirmation_timeout_seconds,
        )?;
        // A signed transfer older than the node's one-hour freshness window can never be accepted, so a
        // longer wait would only delay the pause that tells an operator something is wrong.
        if service.confirmation_timeout_seconds == 0 || service.confirmation_timeout_seconds > 3_000
        {
            return Err("GAMES_CONFIRMATION_TIMEOUT_SECONDS must be 1 to 3000".into());
        }
        service.max_submission_attempts = small(
            "GAMES_MAX_SUBMISSION_ATTEMPTS",
            service.max_submission_attempts,
        )?;
        // Fail closed: a player's address in a request is not proof that they control the wallet, so the
        // routes that act for a player need a wallet session unless an operator explicitly turns that off.
        service.require_player_auth = boolean("GAMES_REQUIRE_PLAYER_AUTH", true)?;
        // Fail closed: pool rewards and withdrawals move real ULTRA, so they stay off until an operator says so.
        service.pool_payouts_enabled = boolean("GAMES_POOL_PAYOUTS_ENABLED", false)?;
        service.play_per_minute = number("GAMES_PLAY_PER_MINUTE", service.play_per_minute)?;
        service.anchor_empty_days = boolean("GAMES_ANCHOR_EMPTY_DAYS", service.anchor_empty_days)?;
        service.anchor_retry_seconds =
            number("GAMES_ANCHOR_RETRY_SECONDS", service.anchor_retry_seconds)?;
        service.anchor_games = text("GAMES_ANCHOR_GAMES", "grid")
            .split(',')
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        if service
            .anchor_games
            .iter()
            .any(|name| name != "grid" && name != "chess")
        {
            return Err("GAMES_ANCHOR_GAMES may list only grid and chess".into());
        }

        // Anchoring is delegated: this service holds no owner key and runs no signing tool. It asks the
        // Records Service, over loopback, with a signed request (see `peer.rs`).
        let anchor = match lookup("GAMES_RECORDS_URL") {
            None => None,
            Some(url) => {
                let url = url.trim().trim_end_matches('/').to_string();
                let loopback = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"]
                    .iter()
                    .any(|prefix| url.starts_with(prefix));
                if !loopback {
                    return Err(
                        "GAMES_RECORDS_URL must be a loopback http URL such as http://127.0.0.1:8096: the request is authenticated but not encrypted".into(),
                    );
                }
                Some(PeerAnchorConfig {
                    records_url: url,
                    credential: text("GAMES_RECORDS_PEER_CREDENTIAL", "games-records-peer.key"),
                    timeout_seconds: number("GAMES_ANCHOR_TIMEOUT_SECONDS", 60)?,
                })
            }
        };

        Ok(GamesConfig {
            bind: text("GAMES_BIND", "127.0.0.1:8095")
                .parse()
                .map_err(|_| "GAMES_BIND must be a socket address".to_string())?,
            db_path: PathBuf::from(text("GAMES_DB_PATH", "/var/lib/ultranet-games/games.db")),
            node_api_base_url: text("GAMES_NODE_API_BASE_URL", DEFAULT_NODE_API_BASE_URL),
            policy,
            service,
            payout_signer_credential: text(
                "GAMES_PAYOUT_SIGNER_CREDENTIAL",
                "games-payout-signer.json",
            ),
            abuse_key_credential: text("GAMES_ABUSE_KEY_CREDENTIAL", "games-abuse.key"),
            anchor,
        })
    }

    /// Any credential this service must not hold that is present in its credentials directory.
    pub fn foreign_credentials_present(&self) -> Vec<PathBuf> {
        FOREIGN_CREDENTIALS
            .iter()
            .map(|name| self.credential_path(name))
            .filter(|path| path.exists())
            .collect()
    }

    /// The same check against an explicit directory (so a test need not touch the environment).
    pub fn foreign_credentials_in(directory: &std::path::Path) -> Vec<PathBuf> {
        FOREIGN_CREDENTIALS
            .iter()
            .map(|name| directory.join(name))
            .filter(|path| path.exists())
            .collect()
    }

    /// Where a named credential lives: inside `$CREDENTIALS_DIRECTORY` under systemd.
    pub fn credential_path(&self, name: &str) -> PathBuf {
        match env::var("CREDENTIALS_DIRECTORY") {
            Ok(directory) => PathBuf::from(directory).join(name),
            Err(_) => PathBuf::from(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const PAYOUT: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";

    fn config(pairs: &[(&str, &str)]) -> Result<GamesConfig, String> {
        let mut map: HashMap<String, String> = HashMap::new();
        map.insert("GAMES_PAYOUT_ADDRESS".into(), PAYOUT.into());
        map.insert(
            "GAMES_CHESS_CHAIN_ID".into(),
            crate::games::rules::TEST_CHESS_CHAIN_ID.to_string(),
        );
        for (key, value) in pairs {
            map.insert((*key).into(), (*value).into());
        }
        GamesConfig::from_lookup(&|name| map.get(name).cloned())
    }

    #[test]
    fn the_defaults_are_the_reviewed_standard_policy() {
        let config = config(&[]).unwrap();
        assert_eq!(config.policy, Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID));
        assert_eq!(config.bind, "127.0.0.1:8095".parse().unwrap());
        assert_eq!(config.service.anchor_games, vec!["grid".to_string()]);
        assert!(
            config.anchor.is_none(),
            "anchoring is opt-in: it needs a key"
        );
        assert_eq!(config.node_api_base_url, "http://127.0.0.1:8081");
    }

    #[test]
    fn the_payout_address_is_required_and_must_be_an_address() {
        assert!(GamesConfig::from_lookup(&|name| {
            (name == "GAMES_CHESS_CHAIN_ID").then(|| "3".to_string())
        })
        .unwrap_err()
        .contains("GAMES_PAYOUT_ADDRESS"));
        assert!(config(&[("GAMES_PAYOUT_ADDRESS", "abc")])
            .unwrap_err()
            .contains("64"));
    }

    #[test]
    fn a_rule_can_be_tightened() {
        let config = config(&[
            ("GAMES_GRID_REWARD_UNITS", "5"),
            ("GAMES_CHESS_REWARDED_PER_DAY", "50"),
            ("GAMES_CHESS_REWARDED_PER_ADDRESS_PER_DAY", "1"),
            ("GAMES_WITHDRAW_MIN_UNITS", "500"),
        ])
        .unwrap();
        assert_eq!(config.policy.grid.reward_units, 5);
        assert_eq!(config.policy.chess.rewarded_per_day, 50);
        assert_eq!(config.policy.chess.rewarded_per_address_per_day, 1);
        assert_eq!(config.policy.withdraw_min_units, 500);
        assert!(
            !config.policy.chess.pays,
            "tightening a rule must not quietly turn chess's payouts on"
        );
    }

    #[test]
    fn a_configuration_without_games_chess_pays_does_not_pay_chess() {
        // P1 in code, and the whole reason it is in code: a configuration file missing this line is
        // record-only. Chess results come from the phone and nothing proves them, so "on" has to be
        // written down by someone, not inherited from a default.
        let unconfigured = config(&[]).unwrap();
        assert!(!unconfigured.policy.chess.pays);
        assert!(
            unconfigured.policy.grid.pays,
            "Grid is verified by replay, so it pays unless switched off"
        );

        let enabled = config(&[("GAMES_CHESS_PAYS", "true")]).unwrap();
        assert!(enabled.policy.chess.pays);

        for off in ["false", "0", "no"] {
            assert!(
                !config(&[("GAMES_CHESS_PAYS", off)])
                    .unwrap()
                    .policy
                    .chess
                    .pays
            );
        }
        // A malformed value is refused rather than read as "on" — the failure mode that would matter.
        assert!(config(&[("GAMES_CHESS_PAYS", "yes please")]).is_err());
    }

    #[test]
    fn a_rule_cannot_be_loosened_past_its_ceiling() {
        let refused = [
            ("GAMES_GRID_REWARD_UNITS", "1001"),
            ("GAMES_CHESS_REWARD_UNITS", "0"),
            ("GAMES_GRID_REWARDED_PER_ADDRESS_PER_DAY", "11"),
            ("GAMES_CHESS_REWARDED_PER_DAY", "10001"),
            ("GAMES_REWARDED_GLOBAL_PER_DAY", "100001"),
            ("GAMES_REWARDED_PER_CLIENT_PER_DAY", "1001"),
            ("GAMES_WITHDRAW_MIN_UNITS", "99"),
            ("GAMES_PAYOUT_DAILY_CAP_UNITS", "5000001"),
            ("GAMES_CHESS_MIN_PLIES", "9"),
            ("GAMES_CHESS_MAX_PLIES", "1001"),
            ("GAMES_PAYOUT_RESERVE_UNITS", "99"),
        ];
        for (name, value) in refused {
            assert!(
                config(&[(name, value)]).is_err(),
                "{name}={value} must be refused"
            );
        }
    }

    #[test]
    fn the_ceilings_themselves_are_accepted() {
        let config = config(&[
            ("GAMES_GRID_REWARD_UNITS", "1000"),
            ("GAMES_GRID_REWARDED_PER_ADDRESS_PER_DAY", "10"),
            ("GAMES_REWARDED_GLOBAL_PER_DAY", "100000"),
            ("GAMES_PAYOUT_DAILY_CAP_UNITS", "5000000"),
        ]);
        assert!(config.is_ok(), "{:?}", config.err());
    }

    #[test]
    fn the_worst_case_a_single_address_can_earn_stays_small_even_at_the_ceilings() {
        // 10 rewarded wins x 1,000 units, per game, two games: 0.02 $ULTRA a day, however it is configured.
        let worst = (MAX_REWARD_UNITS * MAX_REWARDED_PER_ADDRESS_PER_DAY as u64) * 2;
        assert_eq!(worst, 20_000);
        assert!(worst <= MAX_PAYOUT_DAILY_CAP_UNITS);
    }

    #[test]
    fn the_chess_chain_has_no_default_and_a_missing_one_stops_the_start() {
        // The map holds a valid payout address and nothing about chess: the service must refuse,
        // and must name the setting, rather than fall back to some chain id.
        let lookup = |name: &str| (name == "GAMES_PAYOUT_ADDRESS").then(|| PAYOUT.to_string());
        let error = GamesConfig::from_lookup(&lookup).unwrap_err();
        assert!(error.contains("GAMES_CHESS_CHAIN_ID is required"), "{error}");
        assert!(error.contains("no default"), "{error}");
    }

    #[test]
    fn an_unusable_chess_chain_is_refused_and_never_replaced_by_another() {
        for bad in ["", "abc", "-1", "3.0", "4294967296"] {
            let error = config(&[("GAMES_CHESS_CHAIN_ID", bad)]).unwrap_err();
            assert!(error.contains("GAMES_CHESS_CHAIN_ID"), "{bad:?}: {error}");
        }
        // Chain 0 is the L1; chain 1 is Grid's.
        assert!(config(&[("GAMES_CHESS_CHAIN_ID", "0")]).unwrap_err().contains("L1"));
        assert!(config(&[("GAMES_CHESS_CHAIN_ID", "1")])
            .unwrap_err()
            .contains("different AppChains"));
    }

    #[test]
    fn the_chess_chain_is_exactly_the_one_that_was_configured() {
        // Not the Registry's #2 unless somebody writes 2: nothing here picks a number for the operator.
        assert_eq!(config(&[("GAMES_CHESS_CHAIN_ID", "3")]).unwrap().policy.chess.chain_id, 3);
        assert_eq!(config(&[("GAMES_CHESS_CHAIN_ID", "7")]).unwrap().policy.chess.chain_id, 7);
    }

    #[test]
    fn player_sign_in_is_required_unless_an_operator_turns_it_off_explicitly() {
        assert!(config(&[]).unwrap().service.require_player_auth, "fail closed by default");
        assert!(!config(&[("GAMES_REQUIRE_PLAYER_AUTH", "false")]).unwrap().service.require_player_auth);
        assert!(!config(&[]).unwrap().service.pool_payouts_enabled, "pool payouts are off by default");
        assert!(config(&[("GAMES_POOL_PAYOUTS_ENABLED", "true")]).unwrap().service.pool_payouts_enabled);
        assert!(config(&[("GAMES_POOL_PAYOUTS_ENABLED", "maybe")]).is_err());
        assert!(config(&[("GAMES_REQUIRE_PLAYER_AUTH", "true")]).unwrap().service.require_player_auth);
        // A misspelt value is refused, never read as "off".
        assert!(config(&[("GAMES_REQUIRE_PLAYER_AUTH", "maybe")]).unwrap_err().contains("GAMES_REQUIRE_PLAYER_AUTH"));
    }

    #[test]
    fn the_two_games_must_have_their_own_chains() {
        assert!(config(&[("GAMES_CHESS_CHAIN_ID", "1")])
            .unwrap_err()
            .contains("different AppChains"));
        assert!(config(&[("GAMES_GRID_CHAIN_ID", "0")])
            .unwrap_err()
            .contains("L1"));
    }

    #[test]
    fn a_confirmation_wait_longer_than_a_signed_transfer_can_live_is_refused() {
        assert!(config(&[("GAMES_CONFIRMATION_TIMEOUT_SECONDS", "3601")]).is_err());
        assert!(config(&[("GAMES_CONFIRMATION_TIMEOUT_SECONDS", "0")]).is_err());
        assert!(config(&[("GAMES_CONFIRMATION_TIMEOUT_SECONDS", "900")]).is_ok());
    }

    #[test]
    fn anchoring_goes_through_a_loopback_records_service_and_names_known_games() {
        for bad in [
            "https://records.example.com",
            "http://10.0.0.5:8096",
            "http://records.internal:8096",
            "127.0.0.1:8096",
            "http://127.0.0.1.evil.example:8096",
        ] {
            assert!(
                config(&[("GAMES_RECORDS_URL", bad)]).is_err(),
                "{bad} must be refused"
            );
        }
        let ok = config(&[
            ("GAMES_RECORDS_URL", "http://127.0.0.1:8096/"),
            ("GAMES_ANCHOR_GAMES", "grid, chess"),
        ])
        .unwrap();
        assert_eq!(
            ok.service.anchor_games,
            vec!["grid".to_string(), "chess".to_string()]
        );
        let anchor = ok.anchor.unwrap();
        assert_eq!(anchor.records_url, "http://127.0.0.1:8096");
        assert_eq!(anchor.credential, "games-records-peer.key");
        assert_eq!(anchor.timeout_seconds, 60);
        assert!(config(&[("GAMES_ANCHOR_GAMES", "grid,poker")]).is_err());
    }

    #[test]
    fn this_service_has_no_setting_for_an_owner_key_or_the_registry() {
        // The old variables are not silently honoured: setting them changes nothing, and the
        // service holds no anchor tool or key because there is nothing here that reads one.
        let with_old = config(&[
            ("GAMES_ANCHOR_CLI", "/opt/ultranet-games/appchain-cli"),
            ("GAMES_REGISTRY_CHAIN_ID", "3"),
            ("GAMES_REGISTRY_GAMES_PER_OWNER", "9"),
        ])
        .unwrap();
        assert!(with_old.anchor.is_none(), "GAMES_ANCHOR_CLI is ignored here");
        assert_eq!(with_old.policy, Policy::standard(crate::games::rules::TEST_CHESS_CHAIN_ID));
        assert!(FOREIGN_CREDENTIALS.contains(&"games-anchor-owner.key.enc"));
        assert!(FOREIGN_CREDENTIALS.contains(&"records-anchor.pass"));
    }

    #[test]
    fn malformed_numbers_and_booleans_are_refused_not_defaulted() {
        assert!(config(&[("GAMES_GRID_REWARD_UNITS", "ten")]).is_err());
        assert!(config(&[("GAMES_CHESS_PAYS", "maybe")]).is_err());
        assert!(config(&[("GAMES_BIND", "not-an-address")]).is_err());
    }
}
