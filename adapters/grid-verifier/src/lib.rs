//! UltraGrid's universal verifier, as a Games adapter module.
//!
//! The engine below is **the same `src/ultragrid/mod.rs` the node and the phone already use** — included
//! by path, not copied — so there is one puzzle generator and one replay rule in the world. This crate
//! only adds the adapter ABI (`src/games/adapter.rs`): a fixed input buffer, a 38-byte verdict, and no
//! imports at all, because the runtime refuses a module that can reach the clock, the network or a
//! random number generator.
//!
//! Payload (little-endian, 44 bytes + 3 per turn, at most 4,096 bytes):
//!
//! ```text
//!  0      u8    version = 1
//!  1      u8    game    = 1  (UltraGrid)
//!  2..6   u32   chain_id
//!  6..10  u32   day      days since 1970-01-01, UTC
//! 10..42  [32]  previous_anchor_root — the chain's last anchor before that UTC day began
//! 42      u8    level    0..=9
//! 43      u8    count    number of turns
//! 44..    count × { u16 cell, u8 turns }
//! ```
//!
//! The adapter derives the day's seed from the chain, the date and that anchor root, regenerates the
//! puzzle from the seed, and replays the answer on it. A fabricated puzzle is impossible: the puzzle is
//! never in the payload. Whether the declared day is *current* is the host's rule, not this module's —
//! a module has no clock, which is exactly why it can be deterministic.
//!
//! What this does not do: it does not stop a program from solving a puzzle whose seed is public. That is
//! why the service still caps rewards per player and per day. See `docs/games-grid-chess-migration.md`.

#[path = "../../../src/ultragrid/mod.rs"]
mod engine;

use engine::{daily_seed, verify, Turn, LEVELS_PER_DAY};
use sha3::{Digest, Sha3_256};

const ABI_VERSION: i32 = 1;
const INPUT_BYTES: usize = 4_200;
const OUTPUT_BYTES: usize = 38;
const PAYLOAD_VERSION: u8 = 1;
const GAME_GRID: u8 = 1;
const HEADER_BYTES: usize = 44;

const OUTCOME_INVALID: u8 = 0;
const OUTCOME_WIN: u8 = 1;
const OUTCOME_LOSS: u8 = 2;

/// The host writes the request here. The address is exported, so the runtime never guesses it.
static mut INPUT: [u8; INPUT_BYTES] = [0; INPUT_BYTES];
/// The host reads the verdict from here.
static mut OUTPUT: [u8; OUTPUT_BYTES] = [0; OUTPUT_BYTES];

#[no_mangle]
pub extern "C" fn adapter_abi_version() -> i32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn input_buffer() -> i32 {
    core::ptr::addr_of!(INPUT) as i32
}

#[no_mangle]
pub extern "C" fn input_capacity() -> i32 {
    INPUT_BYTES as i32
}

#[no_mangle]
pub extern "C" fn output_buffer() -> i32 {
    core::ptr::addr_of!(OUTPUT) as i32
}

/// The host passes the exact byte length it wrote: `1 + 32 + 4 + payload`.
#[no_mangle]
pub extern "C" fn adapter_run(length: i32) -> i32 {
    let length = length.max(0) as usize;
    if length > INPUT_BYTES {
        return -1;
    }
    // SAFETY: the host wrote exactly `length` bytes at this address, and this module is single-run
    // (the runtime builds a fresh instance per run), so nothing else aliases the buffer.
    let input = unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(INPUT) as *const u8, length) };
    let (outcome, score, digest) = judge(input);
    let out = unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(OUTPUT) as *mut u8, OUTPUT_BYTES) };
    out[0] = ABI_VERSION as u8;
    out[1] = outcome;
    out[2..6].copy_from_slice(&score.to_le_bytes());
    out[6..38].copy_from_slice(&digest);
    OUTPUT_BYTES as i32
}

/// `abi(1) || player(32) || payload_len u32le || payload`.
fn judge(input: &[u8]) -> (u8, u32, [u8; 32]) {
    const PREFIX: usize = 1 + 32 + 4;
    if input.len() < PREFIX || input[0] != ABI_VERSION as u8 {
        return invalid(input);
    }
    let declared = u32::from_le_bytes([input[33], input[34], input[35], input[36]]) as usize;
    let payload = &input[PREFIX..];
    if declared != payload.len() {
        return invalid(input);
    }
    match accept(payload, &input[1..33]) {
        Some(result) => result,
        None => invalid(input),
    }
}

/// The verdict for a payload the adapter could read, or `None` when it could not.
fn accept(payload: &[u8], player: &[u8]) -> Option<(u8, u32, [u8; 32])> {
    if payload.len() < HEADER_BYTES || payload[0] != PAYLOAD_VERSION || payload[1] != GAME_GRID {
        return None;
    }
    let chain_id = u32::from_le_bytes(payload[2..6].try_into().ok()?);
    let day = u32::from_le_bytes(payload[6..10].try_into().ok()?);
    let root: [u8; 32] = payload[10..42].try_into().ok()?;
    let level = payload[42];
    let count = payload[43] as usize;
    if level as usize >= LEVELS_PER_DAY || payload.len() != HEADER_BYTES + count * 3 {
        return None;
    }

    let mut answer = Vec::with_capacity(count);
    for entry in payload[HEADER_BYTES..].chunks_exact(3) {
        answer.push(Turn {
            cell: u16::from_le_bytes([entry[0], entry[1]]),
            turns: entry[2],
        });
    }

    // The puzzle is regenerated, never accepted: the same chain, day and anchor root always give the
    // same ten puzzles, so there is nothing for a client to choose.
    let date = date_of_day(day)?;
    let seed = daily_seed(chain_id, &date, &root)?;
    let puzzle = engine::puzzle(&seed, level);

    let digest = result_digest(&seed, level, count, player, &answer);
    let (outcome, score) = match verify(&puzzle, &answer) {
        Ok(outcome) if outcome.win => (OUTCOME_WIN, outcome.moves),
        // A well-formed answer that does not solve the puzzle is a loss, not a refusal: the player
        // sent a real attempt and the verdict is about the puzzle, not the message.
        Ok(outcome) => (OUTCOME_LOSS, outcome.moves),
        Err(_) => (OUTCOME_INVALID, 0),
    };
    Some((outcome, score, digest))
}

fn invalid(input: &[u8]) -> (u8, u32, [u8; 32]) {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraGrid/refused/v1");
    hasher.update(input);
    (OUTCOME_INVALID, 0, hasher.finalize().into())
}

/// A digest over exactly what was judged, so two different accepted plays never share one.
fn result_digest(seed: &[u8; 32], level: u8, count: usize, player: &[u8], answer: &[Turn]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraGrid/result/v1");
    hasher.update(seed);
    hasher.update([level]);
    hasher.update((count as u32).to_le_bytes());
    hasher.update(player);
    for turn in answer {
        hasher.update(turn.cell.to_le_bytes());
        hasher.update([turn.turns]);
    }
    hasher.finalize().into()
}

/// `YYYY-MM-DD` for a count of days since 1970-01-01, by the civil-from-days algorithm.
fn date_of_day(day: u32) -> Option<String> {
    // The service's own date check is the ceiling; this bound only keeps the arithmetic finite.
    if day > 3_650_000 {
        return None;
    }
    let z = i64::from(day) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}
