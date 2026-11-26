//! Which day a play is about, and the host's rule that it is the current one.
//!
//! A Games adapter is pure: no imports, no clock, no chain. That is what makes its verdict reproducible.
//! It also means the *day* a payload claims to judge cannot be checked inside the module — a module that
//! could read a clock could not be deterministic. So the day is declared by the payload in a form the
//! host can read, and the host refuses a play whose declared day is not the chain's current one.
//!
//! Without this, a player could hand Grid an anchor root of their choosing and get a puzzle of their
//! choosing. The module would still judge that puzzle honestly — it would just be the wrong puzzle.
//!
//! A payload that declares no day (any game that has no daily context) is passed through untouched.

use super::service::{GamesError, GamesService};

/// One UTC day in seconds.
pub const DAY_SECONDS: u64 = 86_400;

/// UltraGrid's payload kind, as its adapter reads byte 1.
pub const GRID_KIND: u8 = 1;
/// `version u8, kind u8, chain u32, day u32, root 32, level u8, count u8`.
pub const GRID_HEADER_BYTES: usize = 44;
/// Turns are `cell u16, turns u8`.
pub const GRID_TURN_BYTES: usize = 3;

/// A payload's own statement of the day it should be judged for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayContext {
    pub chain_id: u32,
    pub day: u32,
    pub previous_root: [u8; 32],
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` date. `None` for anything that is not a real date.
pub fn day_index(date: &str) -> Option<u32> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let number = |range: core::ops::Range<usize>| -> Option<i64> {
        let text = date.get(range)?;
        if !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse().ok()
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // days-from-civil, the inverse of `date_of`
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u32::try_from(days).ok()
}

/// `YYYY-MM-DD` for a count of days since 1970-01-01.
pub fn date_of(day: u32) -> String {
    let z = i64::from(day) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day_of_month = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// The UTC day a moment falls in.
pub fn day_of(seconds: u64) -> u32 {
    u32::try_from(seconds / DAY_SECONDS).unwrap_or(u32::MAX)
}

/// The first second of a UTC day.
pub fn day_start(day: u32) -> u64 {
    u64::from(day) * DAY_SECONDS
}

/// Read the day a payload declares, when it declares one. Only a payload this service knows the layout
/// of is understood; everything else returns `None` and is judged as an opaque input.
pub fn declared_day_context(payload: &[u8]) -> Option<DayContext> {
    if payload.len() < 2 || payload[0] != 1 || payload[1] != GRID_KIND {
        return None;
    }
    if payload.len() < GRID_HEADER_BYTES {
        return None;
    }
    let chain_id = u32::from_le_bytes(payload.get(2..6)?.try_into().ok()?);
    let day = u32::from_le_bytes(payload.get(6..10)?.try_into().ok()?);
    let previous_root: [u8; 32] = payload.get(10..42)?.try_into().ok()?;
    Some(DayContext {
        chain_id,
        day,
        previous_root,
    })
}

/// One UltraGrid play, in the exact bytes its verifier reads.
pub fn grid_payload(chain_id: u32, day: u32, previous_root: &[u8; 32], level: u8, turns: &[(u16, u8)]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(GRID_HEADER_BYTES + turns.len() * GRID_TURN_BYTES);
    payload.push(1);
    payload.push(GRID_KIND);
    payload.extend_from_slice(&chain_id.to_le_bytes());
    payload.extend_from_slice(&day.to_le_bytes());
    payload.extend_from_slice(previous_root);
    payload.push(level);
    payload.push(u8::try_from(turns.len()).unwrap_or(u8::MAX));
    for (cell, turns) in turns {
        payload.extend_from_slice(&cell.to_le_bytes());
        payload.push(*turns);
    }
    payload
}

/// The level and turns a Grid payload carries, for tests and for the host's own diagnostics.
pub fn grid_answer(payload: &[u8]) -> Option<(u8, Vec<(u16, u8)>)> {
    if payload.len() < GRID_HEADER_BYTES {
        return None;
    }
    let level = payload[42];
    let count = payload[43] as usize;
    if payload.len() != GRID_HEADER_BYTES + count * GRID_TURN_BYTES {
        return None;
    }
    let turns = payload[GRID_HEADER_BYTES..]
        .chunks_exact(GRID_TURN_BYTES)
        .map(|entry| (u16::from_le_bytes([entry[0], entry[1]]), entry[2]))
        .collect();
    Some((level, turns))
}

impl GamesService {
    /// The host's freshness rule: a play that declares a day must declare *this* chain's current day, and
    /// the anchor root that day's puzzles were derived from. Nothing about this is game-specific — it is a
    /// rule about the claim a payload makes.
    pub(super) async fn require_current_day(&self, chain_id: u32, declared: &DayContext, now: u64) -> Result<(), GamesError> {
        if declared.chain_id != chain_id {
            return Err(GamesError::Forbidden("that play is for a different AppChain".into()));
        }
        let today = day_of(now);
        if declared.day != today {
            return Err(GamesError::Invalid(format!(
                "that play is for {}, and only today's play ({}) can be judged",
                date_of(declared.day),
                date_of(today)
            )));
        }
        let expected = match self.chain().latest_anchor_before(chain_id, day_start(today)).await? {
            Some(point) => point.root,
            None => [0u8; 32],
        };
        if declared.previous_root != expected {
            return Err(GamesError::Forbidden(
                "that play does not use this chain's anchor root for the day".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_day_numbers_are_inverse() {
        for (date, day) in [("1970-01-01", 0u32), ("2026-11-26", 20_783), ("2000-02-29", 11_016)] {
            assert_eq!(day_index(date), Some(day), "{date}");
            assert_eq!(date_of(day), date, "{day}");
        }
        for bad in ["2026-13-01", "2026-00-10", "2026-11-32", "2026-1-1", "not-a-date", "2026/11/26"] {
            assert_eq!(day_index(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_payload_declares_its_day_and_nothing_else_does() {
        let mut turns = vec![(3u16, 2u8), (17, 1)];
        let payload = grid_payload(1, 20_798, &[9u8; 32], 4, &turns);
        let declared = declared_day_context(&payload).expect("a grid payload declares its day");
        assert_eq!(declared.chain_id, 1);
        assert_eq!(declared.day, 20_798);
        assert_eq!(declared.previous_root, [9u8; 32]);
        assert_eq!(grid_answer(&payload), Some((4u8, turns.clone())));
        // An opaque input is not a day claim, and a truncated one is not either.
        assert_eq!(declared_day_context(&[0x01, 0x09, 0x00]), None);
        assert_eq!(declared_day_context(&payload[..40]), None);
        turns.clear();
        assert_eq!(grid_answer(&grid_payload(1, 1, &[0u8; 32], 0, &turns)), Some((0u8, vec![])));
    }

    #[test]
    fn the_day_of_a_moment_is_utc_and_midnight_belongs_to_the_new_day() {
        assert_eq!(day_start(20_798), 20_798 * 86_400);
        assert_eq!(day_of(20_798 * 86_400), 20_798);
        assert_eq!(day_of(20_798 * 86_400 + 86_399), 20_798);
        assert_eq!(day_of(20_798 * 86_400 + 86_400), 20_799);
    }
}
