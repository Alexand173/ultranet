//! The rules of the games and of paying for them, as pure functions.
//!
//! Nothing here touches a database, a clock or the network, so every rule can be read, argued with
//! and tested on its own. The service applies [`decide`] inside one database transaction, so the
//! counters it reads and the reward it writes can never disagree.
//!
//! The numbers are `docs/ECONOMIC_ARCHITECTURE_V2.md` §5 (R4, R4b) applied to two games, and
//! `docs/GAMES_RULES_AND_PAYOUTS.md` explains each one. They are defaults: the operator may tighten
//! them from the environment, never silently loosen what the code enforces.

use crate::UltraBlockchain;

pub const GAME_GRID: &str = "grid";
pub const GAME_CHESS: &str = "chess";

/// One game's reward terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamePolicy {
    pub game: &'static str,
    /// The AppChain this game's days are anchored to.
    pub chain_id: u32,
    /// Base units credited for one rewarded win.
    pub reward_units: u64,
    /// Rewarded wins one address may collect per game per UTC day. Wins beyond it are still recorded.
    pub rewarded_per_address_per_day: u32,
    /// Rewarded wins this game may pay per UTC day, across all addresses.
    pub rewarded_per_day: u64,
    /// A game that does not pay still records every win and still anchors its day.
    pub pays: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub grid: GamePolicy,
    pub chess: GamePolicy,
    /// Rewarded wins across every game per UTC day: the backstop whatever the number of games.
    pub rewarded_global_per_day: u64,
    /// Rewarded wins per client per UTC day. A flood limit, not an anti-farm rule: many phones share
    /// one carrier address, so this is deliberately far above the per-address limit.
    pub rewarded_per_client_per_day: u32,
    /// The smallest balance that may be withdrawn. The whole balance is paid, never a part.
    pub withdraw_min_units: u64,
    /// The most that may be queued for withdrawal per UTC day: defence in depth behind the wallet's
    /// own balance, so a bug cannot empty it in a day.
    pub payout_daily_cap_units: u64,
    /// Reward only addresses that exist on the chain (a balance or a transaction). Without it an
    /// attacker could mint unlimited fresh addresses and drain the day's budget.
    pub require_chain_presence: bool,
    pub chess_min_plies: u32,
    pub chess_max_plies: u32,
    /// Held back inside the payout wallet for network fees, which it pays on top of every payout. It is
    /// part of the solvency rule: rewards are promised only against what is left after it.
    pub payout_reserve_units: u64,
}

impl Policy {
    /// V2's standard parameters, with Grid's 10-unit test reward (R4b).
    ///
    /// **Chess is record-only here on purpose.** Its results are reported by the phone and the server
    /// cannot verify them (§2.2 of `GAMES_RULES_AND_PAYOUTS.md`), so paying for them is an opt-in that
    /// has to be written down, never a default: a configuration file missing one line must not be able
    /// to start paying for wins nobody proved. `GAMES_CHESS_PAYS=true` is the only thing that turns it
    /// on, and the rules review (§6, P1) records why it is off.
    ///
    /// **Chess has no default chain.** Which AppChain anchors chess is a deployment fact (Ultra Chess is
    /// #3; #2 is the Registry), so the caller must name it. A default here once pointed chess at the
    /// Registry's chain id; `GamesConfig` now refuses to start without `GAMES_CHESS_CHAIN_ID`.
    pub fn standard(chess_chain_id: u32) -> Policy {
        Policy {
            grid: GamePolicy {
                game: GAME_GRID,
                chain_id: 1,
                reward_units: 10,
                rewarded_per_address_per_day: 3,
                rewarded_per_day: 2_000,
                pays: true,
            },
            chess: GamePolicy {
                game: GAME_CHESS,
                chain_id: chess_chain_id,
                reward_units: 100,
                rewarded_per_address_per_day: 3,
                // The review's recommended ceiling for the game nobody can verify (§6, P1), not V2's
                // 2,000. Turning chess on should give the exposure that was argued for, not ten times it
                // because a second line was left out.
                rewarded_per_day: 200,
                pays: false,
            },
            rewarded_global_per_day: 20_000,
            rewarded_per_client_per_day: 30,
            withdraw_min_units: 100,
            payout_daily_cap_units: 200_000,
            require_chain_presence: true,
            chess_min_plies: 10,
            chess_max_plies: 600,
            payout_reserve_units: 1_000,
        }
    }

    /// The same rules with chess paying. Used by tests of the paid path, and it is the shape an
    /// operator takes on the day P1 is revisited — one function call, so the fact that it was a
    /// deliberate choice is in the code that chose it.
    pub fn with_chess_paying(mut self) -> Policy {
        self.chess.pays = true;
        self
    }

    pub fn game(&self, game: &str) -> Option<&GamePolicy> {
        match game {
            GAME_GRID => Some(&self.grid),
            GAME_CHESS => Some(&self.chess),
            _ => None,
        }
    }
}

/// What has already been rewarded today, read inside the same transaction that writes the new win.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub address_in_game: u32,
    pub game: u64,
    pub global: u64,
    pub client: u32,
}

/// What the payout wallet can cover, read at the moment a reward is decided.
///
/// A balance is a promise. The service never credits one it could not pay: a reward is granted only if
/// everything already owed, plus this reward, plus a reserve for fees, still fits inside the payout
/// wallet. Without this the daily ceilings (which are rates) could outrun a small pool, and players
/// would see money they could not withdraw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Solvency {
    /// The payout wallet's balance on the chain, in base units.
    pub pool_units: u64,
    /// Credited and not yet paid, across every address (reserved payouts included).
    pub outstanding_units: u64,
    /// Held back for network fees, which the wallet pays on top of every payout.
    pub reserve_units: u64,
}

impl Solvency {
    /// Whether `reward` more can be promised without promising more than the wallet holds.
    pub fn covers(&self, reward: u64) -> bool {
        self.outstanding_units
            .checked_add(reward)
            .and_then(|owed| owed.checked_add(self.reserve_units))
            .is_some_and(|needed| needed <= self.pool_units)
    }

    /// A pool large enough that it never limits anything. For tests of the other rules only.
    #[cfg(test)]
    pub fn unlimited() -> Solvency {
        Solvency {
            pool_units: u64::MAX / 4,
            outstanding_units: 0,
            reserve_units: 0,
        }
    }
}

/// Why a win was recorded without a reward. Every one of these still counts as a win and is anchored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordedReason {
    /// The game is not paying (switched off, or not yet).
    NotPaying,
    /// The address has no balance and no transaction history on the chain.
    NotOnChain,
    AddressLimit,
    ClientLimit,
    GameLimit,
    GlobalLimit,
    /// The payout wallet could not cover this reward on top of everything already owed.
    PoolLow,
}

impl RecordedReason {
    pub fn code(self) -> &'static str {
        match self {
            RecordedReason::NotPaying => "not_paying",
            RecordedReason::NotOnChain => "not_on_chain",
            RecordedReason::AddressLimit => "address_daily_limit",
            RecordedReason::ClientLimit => "client_daily_limit",
            RecordedReason::GameLimit => "game_daily_limit",
            RecordedReason::GlobalLimit => "global_daily_limit",
            RecordedReason::PoolLow => "pool_low",
        }
    }

    pub fn from_code(code: &str) -> Option<RecordedReason> {
        Some(match code {
            "not_paying" => RecordedReason::NotPaying,
            "not_on_chain" => RecordedReason::NotOnChain,
            "address_daily_limit" => RecordedReason::AddressLimit,
            "client_daily_limit" => RecordedReason::ClientLimit,
            "game_daily_limit" => RecordedReason::GameLimit,
            "global_daily_limit" => RecordedReason::GlobalLimit,
            "pool_low" => RecordedReason::PoolLow,
            _ => return None,
        })
    }

    /// Words for the screen. They say what happened and do not apologise or promise.
    pub fn message(self) -> &'static str {
        match self {
            RecordedReason::NotPaying => "This game is not paying rewards right now. Your win is recorded.",
            RecordedReason::NotOnChain => {
                "Rewards go to addresses that exist on the chain. This address has no balance or history yet. Your win is recorded."
            }
            RecordedReason::AddressLimit => {
                "You have collected today's rewarded wins for this game. Your win is recorded."
            }
            RecordedReason::ClientLimit => "Too many rewarded wins came from this connection today. Your win is recorded.",
            RecordedReason::GameLimit => "This game has paid its rewards for today. Your win is recorded.",
            RecordedReason::GlobalLimit => "All games have paid their rewards for today. Your win is recorded.",
            RecordedReason::PoolLow => {
                "The rewards pool is low, so rewards are paused until it is topped up. Your win is recorded."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Rewarded { units: u64 },
    Recorded(RecordedReason),
}

/// Decide the reward for one verified win. The order is part of the rule: it fixes which reason a
/// player is told when several apply.
pub fn decide(
    policy: &Policy,
    game: &GamePolicy,
    counters: &Counters,
    on_chain: bool,
    solvency: &Solvency,
) -> Verdict {
    if !game.pays {
        return Verdict::Recorded(RecordedReason::NotPaying);
    }
    if policy.require_chain_presence && !on_chain {
        return Verdict::Recorded(RecordedReason::NotOnChain);
    }
    if counters.address_in_game >= game.rewarded_per_address_per_day {
        return Verdict::Recorded(RecordedReason::AddressLimit);
    }
    if counters.client >= policy.rewarded_per_client_per_day {
        return Verdict::Recorded(RecordedReason::ClientLimit);
    }
    if counters.game >= game.rewarded_per_day {
        return Verdict::Recorded(RecordedReason::GameLimit);
    }
    if counters.global >= policy.rewarded_global_per_day {
        return Verdict::Recorded(RecordedReason::GlobalLimit);
    }
    if !solvency.covers(game.reward_units) {
        return Verdict::Recorded(RecordedReason::PoolLow);
    }
    Verdict::Rewarded {
        units: game.reward_units,
    }
}

/// The network fee for a payout of `amount` base units: the node's own rule, so the service never
/// disagrees with it. It is paid by the payout wallet, never by the player.
pub fn payout_fee(amount_units: u64) -> u64 {
    UltraBlockchain::minimum_transfer_fee(amount_units)
}

/// `0.000010` for 10 base units: six decimals, no rounding, for logs and receipts.
pub fn format_units(units: u64) -> String {
    format!("{}.{:06}", units / 1_000_000, units % 1_000_000)
}

/// The chess chain the tests use: Ultra Chess's real identity, never the Registry's (#2).
#[cfg(test)]
pub const TEST_CHESS_CHAIN_ID: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy::standard(TEST_CHESS_CHAIN_ID)
    }

    fn grid(policy: &Policy) -> &GamePolicy {
        &policy.grid
    }

    #[test]
    fn a_fresh_on_chain_address_is_rewarded_the_games_amount() {
        let policy = policy();
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Rewarded { units: 10 }
        );
        let paying = policy.with_chess_paying();
        assert_eq!(
            decide(
                &paying,
                &paying.chess,
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Rewarded { units: 100 }
        );
    }

    #[test]
    fn chess_is_record_only_until_something_says_otherwise() {
        // The point of the default: an unconfigured installation records chess and pays nothing.
        let policy = policy();
        assert!(policy.grid.pays);
        assert!(!policy.chess.pays, "chess must be opt-in, not opt-out");
        assert_eq!(
            decide(
                &policy,
                &policy.chess,
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::NotPaying)
        );
        // Turning it on is one named act, and the reward is then exactly R4's number.
        let paying = policy.with_chess_paying();
        assert!(paying.chess.pays);
        assert_eq!(
            decide(
                &paying,
                &paying.chess,
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Rewarded { units: 100 }
        );
    }

    #[test]
    fn the_standard_numbers_are_the_ones_the_architecture_ratifies() {
        let policy = policy();
        assert_eq!(policy.grid.reward_units, 10, "R4b: Grid's test reward");
        assert_eq!(policy.chess.reward_units, 100, "R4: the standard reward");
        assert_eq!(policy.grid.rewarded_per_address_per_day, 3);
        assert_eq!(policy.chess.rewarded_per_address_per_day, 3);
        assert_eq!(policy.grid.rewarded_per_day, 2_000);
        assert_eq!(
            policy.chess.rewarded_per_day, 200,
            "chess's ceiling is the reviewed one, so switching it on cannot multiply the exposure"
        );
        assert_eq!(policy.rewarded_global_per_day, 20_000);
        assert_eq!(policy.withdraw_min_units, 100);
    }

    #[test]
    fn the_third_win_is_rewarded_and_the_fourth_is_only_recorded() {
        let policy = policy();
        let counters = |n| Counters {
            address_in_game: n,
            ..Counters::default()
        };
        for already in 0..3 {
            assert!(matches!(
                decide(
                    &policy,
                    grid(&policy),
                    &counters(already),
                    true,
                    &Solvency::unlimited()
                ),
                Verdict::Rewarded { .. }
            ));
        }
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &counters(3),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::AddressLimit)
        );
    }

    #[test]
    fn each_cap_stops_payment_but_never_recording() {
        let policy = policy();
        let cases = [
            (
                Counters {
                    client: 30,
                    ..Counters::default()
                },
                RecordedReason::ClientLimit,
            ),
            (
                Counters {
                    game: 2_000,
                    ..Counters::default()
                },
                RecordedReason::GameLimit,
            ),
            (
                Counters {
                    global: 20_000,
                    ..Counters::default()
                },
                RecordedReason::GlobalLimit,
            ),
        ];
        for (counters, reason) in cases {
            assert_eq!(
                decide(
                    &policy,
                    grid(&policy),
                    &counters,
                    true,
                    &Solvency::unlimited()
                ),
                Verdict::Recorded(reason)
            );
        }
    }

    #[test]
    fn an_address_that_is_not_on_the_chain_is_recorded_not_paid() {
        let policy = policy();
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &Counters::default(),
                false,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::NotOnChain)
        );
        // Switching the gate off is an explicit operator choice, and then presence does not matter.
        let mut open = policy.clone();
        open.require_chain_presence = false;
        assert!(matches!(
            decide(
                &open,
                grid(&open),
                &Counters::default(),
                false,
                &Solvency::unlimited()
            ),
            Verdict::Rewarded { .. }
        ));
    }

    #[test]
    fn a_game_that_does_not_pay_still_records() {
        let mut policy = policy();
        policy.chess.pays = false;
        assert_eq!(
            decide(
                &policy,
                &policy.chess,
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::NotPaying)
        );
        assert!(matches!(
            decide(
                &policy,
                &policy.grid,
                &Counters::default(),
                true,
                &Solvency::unlimited()
            ),
            Verdict::Rewarded { .. }
        ));
    }

    #[test]
    fn when_several_limits_apply_the_player_is_told_the_first_in_a_fixed_order() {
        let policy = policy();
        let everything = Counters {
            address_in_game: 3,
            game: 2_000,
            global: 20_000,
            client: 30,
        };
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &everything,
                true,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::AddressLimit)
        );
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &everything,
                false,
                &Solvency::unlimited()
            ),
            Verdict::Recorded(RecordedReason::NotOnChain)
        );
    }

    #[test]
    fn reason_codes_round_trip() {
        for reason in [
            RecordedReason::NotPaying,
            RecordedReason::NotOnChain,
            RecordedReason::AddressLimit,
            RecordedReason::ClientLimit,
            RecordedReason::GameLimit,
            RecordedReason::GlobalLimit,
            RecordedReason::PoolLow,
        ] {
            assert_eq!(RecordedReason::from_code(reason.code()), Some(reason));
            assert!(!reason.message().is_empty());
        }
        assert_eq!(RecordedReason::from_code("nonsense"), None);
    }

    #[test]
    fn a_reward_is_promised_only_if_the_wallet_can_cover_everything_already_owed() {
        let policy = policy();
        let pool = |pool_units, outstanding_units| Solvency {
            pool_units,
            outstanding_units,
            reserve_units: 1_000,
        };
        // 1,100 in the wallet, 1,000 held for fees: exactly 100 units of promises fit.
        assert!(
            matches!(
                decide(
                    &policy,
                    grid(&policy),
                    &Counters::default(),
                    true,
                    &pool(1_100, 90)
                ),
                Verdict::Rewarded { units: 10 }
            ),
            "90 owed + 10 new = 100 fits"
        );
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &Counters::default(),
                true,
                &pool(1_100, 91)
            ),
            Verdict::Recorded(RecordedReason::PoolLow),
            "91 owed + 10 new = 101 does not"
        );
        // An empty wallet promises nothing, even to the first winner.
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &Counters::default(),
                true,
                &pool(0, 0)
            ),
            Verdict::Recorded(RecordedReason::PoolLow)
        );
        // A wallet smaller than the reserve alone promises nothing either.
        assert_eq!(
            decide(
                &policy,
                grid(&policy),
                &Counters::default(),
                true,
                &pool(999, 0)
            ),
            Verdict::Recorded(RecordedReason::PoolLow)
        );
    }

    #[test]
    fn a_rule_the_player_can_act_on_is_reported_before_the_pool() {
        let policy = policy();
        let broke = Solvency {
            pool_units: 0,
            outstanding_units: 0,
            reserve_units: 1_000,
        };
        let at_limit = Counters {
            address_in_game: 3,
            ..Counters::default()
        };
        assert_eq!(
            decide(&policy, grid(&policy), &at_limit, true, &broke),
            Verdict::Recorded(RecordedReason::AddressLimit)
        );
        assert_eq!(
            decide(&policy, grid(&policy), &Counters::default(), false, &broke),
            Verdict::Recorded(RecordedReason::NotOnChain)
        );
    }

    #[test]
    fn solvency_arithmetic_cannot_overflow_into_a_yes() {
        let near_max = Solvency {
            pool_units: u64::MAX,
            outstanding_units: u64::MAX,
            reserve_units: 1_000,
        };
        assert!(!near_max.covers(10));
    }

    #[test]
    fn the_payout_fee_is_the_nodes_rule() {
        // 1 % with a floor of one unit: the arithmetic in V2 §5.4.
        assert_eq!(payout_fee(100), 1);
        assert_eq!(payout_fee(99), 1);
        assert_eq!(payout_fee(1_000), 10);
        assert_eq!(payout_fee(10), 1);
    }

    #[test]
    fn units_format_with_six_decimals_and_no_rounding() {
        assert_eq!(format_units(10), "0.000010");
        assert_eq!(format_units(100), "0.000100");
        assert_eq!(format_units(1_000_000), "1.000000");
        assert_eq!(format_units(1_209_981_001), "1209.981001");
    }

    #[test]
    fn the_worst_a_single_address_can_earn_in_a_day_is_small_and_known() {
        let policy = policy();
        let grid_max = policy.grid.reward_units * policy.grid.rewarded_per_address_per_day as u64;
        assert_eq!(grid_max, 30);
        // As shipped an address can be credited 30 base units a day, because chess pays nothing until
        // it is turned on. Reporting a prize it would not pay would be the lie here.
        assert!(!policy.chess.pays);
        assert_eq!(format_units(grid_max), "0.000030");
        // If P1 is revisited, the ceiling on one address is 30 + 300 = 330 units and no more.
        let paying = policy.with_chess_paying();
        let chess_max =
            paying.chess.reward_units * paying.chess.rewarded_per_address_per_day as u64;
        assert_eq!(chess_max, 300);
        assert_eq!(format_units(grid_max + chess_max), "0.000330");
    }

    #[test]
    fn the_games_daily_ceilings_fit_inside_the_global_one() {
        let policy = policy();
        assert!(
            policy.grid.rewarded_per_day + policy.chess.rewarded_per_day
                <= policy.rewarded_global_per_day
        );
    }
}
