//! Autonomous block production.
//!
//! The node seals a block only when `POST /api/mine` is called, so a funded faucet queues claims
//! that never confirm and the mempool never clears. This module runs the same `mine_block()` on an
//! interval when `ULTRANET_AUTO_MINE_INTERVAL_SECONDS` is set, with a floor on the interval and a
//! ceiling on blocks per minute so a misconfiguration cannot outrun validation.
//!
//! Off by default: with the interval at `0` the node behaves exactly as before.

use crate::UltraBlockchain;
use parking_lot::RwLock;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// A shorter interval than this would starve block assembly of any settling time.
pub const MIN_INTERVAL_SECONDS: u64 = 2;
/// Rolling ceiling that applies whatever the interval says.
pub const DEFAULT_MAX_BLOCKS_PER_MINUTE: u32 = 6;
pub const AUTO_MINE_INTERVAL_ENV: &str = "ULTRANET_AUTO_MINE_INTERVAL_SECONDS";
pub const AUTO_MINE_MAX_BLOCKS_ENV: &str = "ULTRANET_AUTO_MINE_MAX_BLOCKS_PER_MINUTE";

static MINED_BLOCKS: AtomicU64 = AtomicU64::new(0);
static LAST_MINE_UNIX: AtomicU64 = AtomicU64::new(0);

/// Blocks this process sealed on its own. Exposed through `/api/stats`.
pub fn mined_block_count() -> u64 {
    MINED_BLOCKS.load(Ordering::Relaxed)
}

/// Unix seconds of the last autonomous block, `0` if none yet.
pub fn last_mine_unix() -> u64 {
    LAST_MINE_UNIX.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoMineConfig {
    pub interval_seconds: u64,
    pub max_blocks_per_minute: u32,
}

impl AutoMineConfig {
    pub fn from_env() -> Result<Self, String> {
        let interval = std::env::var(AUTO_MINE_INTERVAL_ENV).ok();
        let max_blocks = std::env::var(AUTO_MINE_MAX_BLOCKS_ENV).ok();
        Self::parse(interval.as_deref(), max_blocks.as_deref())
    }

    /// Pure parser so the rules can be tested without touching the process environment.
    pub fn parse(interval: Option<&str>, max_blocks: Option<&str>) -> Result<Self, String> {
        let interval_seconds = parse_whole(AUTO_MINE_INTERVAL_ENV, interval, 0)?;
        if interval_seconds != 0 && interval_seconds < MIN_INTERVAL_SECONDS {
            return Err(format!(
                "{AUTO_MINE_INTERVAL_ENV} must be 0 (disabled) or at least {MIN_INTERVAL_SECONDS} seconds"
            ));
        }

        let max_blocks_per_minute = parse_whole(
            AUTO_MINE_MAX_BLOCKS_ENV,
            max_blocks,
            u64::from(DEFAULT_MAX_BLOCKS_PER_MINUTE),
        )?;
        if max_blocks_per_minute == 0 {
            return Err(format!("{AUTO_MINE_MAX_BLOCKS_ENV} must be at least 1"));
        }
        let max_blocks_per_minute = u32::try_from(max_blocks_per_minute)
            .map_err(|_| format!("{AUTO_MINE_MAX_BLOCKS_ENV} is out of range"))?;

        Ok(Self {
            interval_seconds,
            max_blocks_per_minute,
        })
    }

    pub fn enabled(&self) -> bool {
        self.interval_seconds > 0
    }
}

fn parse_whole(name: &str, raw: Option<&str>, default: u64) -> Result<u64, String> {
    match raw {
        None => Ok(default),
        Some(value) if value.trim().is_empty() => Ok(default),
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("{name} must be a whole number")),
    }
}

/// Rolling per-minute allowance. A deque is enough: at most `max` timestamps are kept.
pub struct MineRate {
    window: Duration,
    maximum: u32,
    recent: VecDeque<Instant>,
}

impl MineRate {
    pub fn new(max_blocks_per_minute: u32) -> Self {
        Self {
            window: Duration::from_secs(60),
            maximum: max_blocks_per_minute,
            recent: VecDeque::new(),
        }
    }

    /// Reserves a slot for `now`, or reports that the ceiling is reached.
    pub fn allow(&mut self, now: Instant) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|stamp| now.duration_since(*stamp) >= self.window)
        {
            self.recent.pop_front();
        }
        if self.recent.len() as u32 >= self.maximum {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

fn pending_count(blockchain: &RwLock<UltraBlockchain>) -> usize {
    blockchain.read().mempool.read().get_pending_count()
}

/// One mining pass. `Ok(None)` means there was nothing to mine — the common, quiet case.
pub fn auto_mine_tick(blockchain: &RwLock<UltraBlockchain>) -> Result<Option<u64>, String> {
    if pending_count(blockchain) == 0 {
        return Ok(None);
    }

    let mut chain = blockchain.write();
    match chain.mine_block() {
        Ok(block) => {
            MINED_BLOCKS.fetch_add(1, Ordering::Relaxed);
            LAST_MINE_UNIX.store(crate::auth::now_seconds(), Ordering::Relaxed);
            Ok(Some(block.index))
        }
        // Dead records make `mine_block` refuse; the caller backs off and the operator prunes.
        Err(error) => Err(error),
    }
}

pub async fn run_auto_miner(
    blockchain: std::sync::Arc<RwLock<UltraBlockchain>>,
    config: AutoMineConfig,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(config.interval_seconds));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut rate = MineRate::new(config.max_blocks_per_minute);
    let mut backoff_until: Option<Instant> = None;

    loop {
        ticker.tick().await;
        let now = Instant::now();
        if backoff_until.is_some_and(|until| now < until) {
            continue;
        }
        if pending_count(blockchain.as_ref()) == 0 || !rate.allow(now) {
            continue;
        }

        match auto_mine_tick(blockchain.as_ref()) {
            Ok(Some(index)) => {
                println!(
                    "⛏️  Auto-mined block {index} ({} by this process)",
                    mined_block_count()
                );
                backoff_until = None;
            }
            Ok(None) => {}
            Err(error) => {
                // Usually unmineable leftovers: warn, back off, and keep serving.
                println!("⚠️  Auto-mine skipped: {error}");
                backoff_until = Some(now + Duration::from_secs(config.interval_seconds.max(30)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temporary_db(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ultranet-auto-mine-{label}-{}-{unique}",
            std::process::id()
        ));
        path
    }

    #[test]
    fn auto_mining_is_disabled_unless_configured() {
        let config = AutoMineConfig::parse(None, None).expect("defaults must parse");
        assert_eq!(config.interval_seconds, 0);
        assert_eq!(config.max_blocks_per_minute, DEFAULT_MAX_BLOCKS_PER_MINUTE);
        assert!(!config.enabled());
    }

    #[test]
    fn auto_mine_interval_has_a_floor() {
        assert!(AutoMineConfig::parse(Some("1"), None).is_err());
        assert!(AutoMineConfig::parse(Some("0"), None).is_ok());
        let config = AutoMineConfig::parse(Some("10"), Some("6")).expect("10s must parse");
        assert!(config.enabled());
        assert_eq!(config.interval_seconds, 10);
    }

    #[test]
    fn auto_mine_rejects_a_zero_block_ceiling() {
        assert!(AutoMineConfig::parse(Some("10"), Some("0")).is_err());
        assert!(AutoMineConfig::parse(Some("10"), Some("many")).is_err());
    }

    #[test]
    fn mine_rate_enforces_the_rolling_minute() {
        let mut rate = MineRate::new(2);
        let start = Instant::now();
        assert!(rate.allow(start));
        assert!(rate.allow(start + Duration::from_secs(10)));
        assert!(
            !rate.allow(start + Duration::from_secs(20)),
            "a third block inside the same minute must be refused"
        );
        assert!(
            rate.allow(start + Duration::from_secs(61)),
            "the window must slide"
        );
    }

    #[test]
    fn empty_mempool_is_a_quiet_no_op() {
        let path = temporary_db("empty");
        let blockchain = RwLock::new(UltraBlockchain::new(
            path.to_str()
                .expect("temporary database path must be UTF-8"),
        ));

        for _ in 0..3 {
            assert_eq!(auto_mine_tick(&blockchain).expect("empty tick"), None);
        }
        assert_eq!(blockchain.read().chain.len(), 1, "only genesis exists");
        let _ = std::fs::remove_dir_all(&path);
    }
}
