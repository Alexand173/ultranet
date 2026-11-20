//! UltraGrid — the daily "route the signal" puzzle, as a pure, deterministic engine.
//!
//! A grid of network tiles. A signal leaves the **source** (left edge) and must reach the **target**
//! (right edge). The only move is to turn a tile a quarter turn clockwise. The fewer turns, the better.
//!
//! Why this is a *verifiable* game, which is the whole point of building it on an AppChain:
//!
//! * Everything is derived from a 32-byte seed. The same seed gives the same ten puzzles on every
//!   machine, so the phone, the server and any auditor see identical grids.
//! * An answer is a list of `(tile, quarter turns)`. The server replays it and floods the grid; the
//!   result cannot be claimed, only demonstrated. A patched client cannot invent a win.
//! * Nothing here touches the network, the clock or a random number generator of the OS.
//!
//! What this does **not** do: it does not stop a *program* from solving puzzles. A route search is easy
//! to write. Verifiability stops forged results; it does not stop automation. The caps and the small
//! prize in the game spec are what bound that, and the spec says so.
//!
//! Portability: the random stream, the tile tables and the byte layout below are specified exactly so
//! the Android client can reproduce them. The pinned vectors in the tests are the contract.

use sha3::{Digest, Sha3_256};

/// Bump when any rule below changes: the version is hashed into every puzzle id.
pub const FORMAT_VERSION: u8 = 1;
/// Puzzles in one daily set; level 0 is the easiest, level 9 the largest.
pub const LEVELS_PER_DAY: usize = 10;
/// A par below this is not a puzzle, it is a coin flip; such draws are discarded.
const MIN_PAR: u32 = 3;
/// Bound on regenerations for one level. Hitting it means a bug, not bad luck.
const MAX_ATTEMPTS: usize = 2_000;

// Directions, clockwise from north. The bit for direction `d` is `1 << d`.
const N: usize = 0;
const E: usize = 1;
const S: usize = 2;
const W: usize = 3;
const DX: [i32; 4] = [0, 1, 0, -1];
const DY: [i32; 4] = [-1, 0, 1, 0];

fn bit(direction: usize) -> u8 {
    1 << direction
}

fn opposite(direction: usize) -> usize {
    (direction + 2) % 4
}

// ---------------------------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TileKind {
    /// No connections, cannot be turned.
    Wall = 0,
    /// The source and the target: all four sides open, cannot be turned.
    Terminal = 1,
    /// Two opposite sides.
    Straight = 2,
    /// Two adjacent sides.
    Bend = 3,
    /// Three sides.
    Tee = 4,
    /// All four sides; turning it changes nothing, so it cannot be turned.
    Cross = 5,
}

impl TileKind {
    fn from_code(code: u8) -> Option<TileKind> {
        Some(match code {
            0 => TileKind::Wall,
            1 => TileKind::Terminal,
            2 => TileKind::Straight,
            3 => TileKind::Bend,
            4 => TileKind::Tee,
            5 => TileKind::Cross,
            _ => return None,
        })
    }

    /// How many quarter turns bring the tile back to where it started.
    pub fn period(self) -> u8 {
        match self {
            TileKind::Straight => 2,
            TileKind::Bend | TileKind::Tee => 4,
            TileKind::Wall | TileKind::Terminal | TileKind::Cross => 1,
        }
    }

    /// Whether a player may turn this tile at all.
    pub fn rotatable(self) -> bool {
        self.period() > 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub kind: TileKind,
    /// `0..kind.period()`.
    pub orientation: u8,
}

impl Tile {
    /// Open sides as a bitmask (N=1, E=2, S=4, W=8).
    ///
    /// ```text
    /// Straight  o0 N+S   o1 E+W
    /// Bend      o0 N+E   o1 E+S   o2 S+W   o3 W+N
    /// Tee       o0 N+E+S o1 E+S+W o2 S+W+N o3 W+N+E   (the missing side moves clockwise)
    /// ```
    pub fn arms(self) -> u8 {
        arms(self.kind, self.orientation)
    }

    fn turned(self) -> Tile {
        Tile {
            kind: self.kind,
            orientation: (self.orientation + 1) % self.kind.period(),
        }
    }
}

fn arms(kind: TileKind, orientation: u8) -> u8 {
    match kind {
        TileKind::Wall => 0,
        TileKind::Terminal | TileKind::Cross => 15,
        TileKind::Straight => {
            if orientation % 2 == 0 {
                bit(N) | bit(S)
            } else {
                bit(E) | bit(W)
            }
        }
        TileKind::Bend => {
            let o = (orientation % 4) as usize;
            bit(o) | bit((o + 1) % 4)
        }
        TileKind::Tee => {
            // Orientation 0 is missing the west side; each step moves the gap clockwise.
            let o = (orientation % 4) as usize;
            15 & !bit((W + o) % 4)
        }
    }
}

/// Quarter turns (clockwise only) from `from` to the nearest orientation that opens every side in
/// `needed`, and that orientation. `None` when no orientation of the tile can.
fn cheapest_orientation(tile: Tile, needed: u8) -> Option<(u8, u8)> {
    let period = tile.kind.period();
    (0..period)
        .filter(|&o| arms(tile.kind, o) & needed == needed)
        .map(|o| ((o + period - tile.orientation) % period, o))
        .min()
}

// ---------------------------------------------------------------------------------------------
// The random stream
// ---------------------------------------------------------------------------------------------

/// SHA3-256 in counter mode. Specified so any language can reproduce it:
///
/// ```text
/// block(i) = SHA3-256("UltraGrid/stream/v1" || seed(32) || u32le(len(label)) || label || u64le(i))
/// bytes    = block(0) || block(1) || ...        words = u32le over those bytes
/// below(n) = take words until one is < floor(2^32 / n) * n, return it mod n   (no modulo bias)
/// ```
struct Stream {
    seed: [u8; 32],
    label: Vec<u8>,
    counter: u64,
    block: [u8; 32],
    position: usize,
}

impl Stream {
    fn new(seed: &[u8; 32], label: &[u8]) -> Stream {
        Stream {
            seed: *seed,
            label: label.to_vec(),
            counter: 0,
            block: [0; 32],
            position: 32,
        }
    }

    fn refill(&mut self) {
        let mut hasher = Sha3_256::new();
        hasher.update(b"UltraGrid/stream/v1");
        hasher.update(self.seed);
        hasher.update((self.label.len() as u32).to_le_bytes());
        hasher.update(&self.label);
        hasher.update(self.counter.to_le_bytes());
        self.block = hasher.finalize().into();
        self.counter += 1;
        self.position = 0;
    }

    fn next_u32(&mut self) -> u32 {
        if self.position + 4 > 32 {
            self.refill();
        }
        let word = u32::from_le_bytes(
            self.block[self.position..self.position + 4]
                .try_into()
                .expect("four bytes"),
        );
        self.position += 4;
        word
    }

    fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "below(0) has no answer");
        let limit = (u32::MAX / n) * n;
        loop {
            let word = self.next_u32();
            if word < limit {
                return word % n;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Puzzles
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Puzzle {
    pub level: u8,
    pub width: u8,
    pub height: u8,
    /// Row-major, `index = y * width + x`, `y` growing downwards.
    pub cells: Vec<Tile>,
    pub source: u16,
    pub target: u16,
    /// Turns the reference route costs. Always achievable; a better route may exist.
    pub par: u16,
}

/// One entry of an answer: turn tile `cell` clockwise `turns` times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turn {
    pub cell: u16,
    pub turns: u8,
}

/// A generated puzzle together with the route it was built around.
#[derive(Clone, Debug)]
pub struct Generated {
    pub puzzle: Puzzle,
    pub reference: Vec<Turn>,
}

impl Puzzle {
    /// Canonical bytes: `version, level, width, height, source u16le, target u16le, par u16le`, then one
    /// byte per cell, `kind << 3 | orientation`.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![FORMAT_VERSION, self.level, self.width, self.height];
        bytes.extend(self.source.to_le_bytes());
        bytes.extend(self.target.to_le_bytes());
        bytes.extend(self.par.to_le_bytes());
        bytes.extend(
            self.cells
                .iter()
                .map(|tile| ((tile.kind as u8) << 3) | tile.orientation),
        );
        bytes
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Option<Puzzle> {
        if bytes.len() < 10 || bytes[0] != FORMAT_VERSION {
            return None;
        }
        let (level, width, height) = (bytes[1], bytes[2], bytes[3]);
        let source = u16::from_le_bytes([bytes[4], bytes[5]]);
        let target = u16::from_le_bytes([bytes[6], bytes[7]]);
        let par = u16::from_le_bytes([bytes[8], bytes[9]]);
        let count = width as usize * height as usize;
        if bytes.len() != 10 + count || source as usize >= count || target as usize >= count {
            return None;
        }
        let mut cells = Vec::with_capacity(count);
        for byte in &bytes[10..] {
            let kind = TileKind::from_code(byte >> 3)?;
            let orientation = byte & 7;
            if orientation >= kind.period() {
                return None;
            }
            cells.push(Tile { kind, orientation });
        }
        Some(Puzzle {
            level,
            width,
            height,
            cells,
            source,
            target,
            par,
        })
    }

    /// Identity of this exact puzzle; anchors and receipts name a puzzle by this.
    pub fn id(&self) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(b"UltraGrid/puzzle-id/v1");
        hasher.update(self.canonical_bytes());
        hasher.finalize().into()
    }

    /// Turns allowed for a win: the reference cost plus a margin, so a sound route that is a little
    /// longer than the reference still counts. A policy constant, not a law of the puzzle.
    pub fn move_limit(&self) -> u32 {
        let par = self.par as u32;
        par + (par / 4).max(2)
    }

    /// One text row per grid row, using box-drawing arms; `S` is the source, `T` the target.
    pub fn render(&self) -> String {
        render_cells(
            &self.cells,
            self.width as usize,
            self.source as usize,
            self.target as usize,
        )
    }
}

fn render_cells(cells: &[Tile], width: usize, source: usize, target: usize) -> String {
    let mut out = String::new();
    for (index, tile) in cells.iter().enumerate() {
        if index > 0 && index % width == 0 {
            out.push('\n');
        }
        out.push(if index == source {
            'S'
        } else if index == target {
            'T'
        } else {
            match (tile.kind, tile.orientation) {
                (TileKind::Wall, _) => '█',
                (TileKind::Terminal, _) => '■',
                (TileKind::Cross, _) => '┼',
                (TileKind::Straight, 0) => '│',
                (TileKind::Straight, _) => '─',
                (TileKind::Bend, 0) => '└',
                (TileKind::Bend, 1) => '┌',
                (TileKind::Bend, 2) => '┐',
                (TileKind::Bend, _) => '┘',
                (TileKind::Tee, 0) => '├',
                (TileKind::Tee, 1) => '┬',
                (TileKind::Tee, 2) => '┤',
                (TileKind::Tee, _) => '┴',
            }
        });
    }
    out
}

/// Whether the signal reaches the target. A side between two neighbours carries signal when *both*
/// tiles open onto it.
fn connected(cells: &[Tile], width: usize, height: usize, source: usize, target: usize) -> bool {
    let mut seen = vec![false; cells.len()];
    let mut stack = vec![source];
    seen[source] = true;
    while let Some(here) = stack.pop() {
        if here == target {
            return true;
        }
        let (x, y) = ((here % width) as i32, (here / width) as i32);
        for direction in 0..4 {
            let (nx, ny) = (x + DX[direction], y + DY[direction]);
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let there = ny as usize * width + nx as usize;
            if seen[there] {
                continue;
            }
            if cells[here].arms() & bit(direction) != 0
                && cells[there].arms() & bit(opposite(direction)) != 0
            {
                seen[there] = true;
                stack.push(there);
            }
        }
    }
    false
}

/// Side length for a level: 4, 4, 5, 5, 6, 6, 7, 7, 8, 8.
fn side(level: u8) -> usize {
    4 + level as usize / 2
}

/// Direction from cell `a` to the adjacent cell `b`.
fn direction_between(a: usize, b: usize, width: usize) -> usize {
    let (ax, ay) = ((a % width) as i32, (a / width) as i32);
    let (bx, by) = ((b % width) as i32, (b / width) as i32);
    (0..4)
        .find(|&d| ax + DX[d] == bx && ay + DY[d] == by)
        .expect("path cells are adjacent")
}

/// Cheapest route by random per-cell weights. Scans for the lowest unfinished distance in index order
/// and only improves on a strictly smaller distance, so ties resolve the same way everywhere.
fn route(
    width: usize,
    height: usize,
    blocked: &[bool],
    weights: &[u32],
    from: usize,
    to: usize,
) -> Option<Vec<usize>> {
    let count = width * height;
    let mut distance = vec![u32::MAX; count];
    let mut previous = vec![usize::MAX; count];
    let mut done = vec![false; count];
    distance[from] = 0;
    loop {
        let mut best = usize::MAX;
        let mut best_distance = u32::MAX;
        for index in 0..count {
            if !done[index] && distance[index] < best_distance {
                best_distance = distance[index];
                best = index;
            }
        }
        if best == usize::MAX {
            break;
        }
        done[best] = true;
        if best == to {
            break;
        }
        let (x, y) = ((best % width) as i32, (best / width) as i32);
        for direction in 0..4 {
            let (nx, ny) = (x + DX[direction], y + DY[direction]);
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let next = ny as usize * width + nx as usize;
            if blocked[next] || done[next] {
                continue;
            }
            let candidate = best_distance + weights[next];
            if candidate < distance[next] {
                distance[next] = candidate;
                previous[next] = best;
            }
        }
    }
    if distance[to] == u32::MAX {
        return None;
    }
    let mut path = vec![to];
    while *path.last().expect("non-empty") != from {
        path.push(previous[*path.last().expect("non-empty")]);
    }
    path.reverse();
    Some(path)
}

/// Build the puzzle for `level` from `seed`. Deterministic: same inputs, same bytes.
///
/// The puzzle is built *around a route*, so a solution is known to exist: lay a route from source to
/// target, give its tiles the right arms, then scramble each by a random number of turns. Decoys fill
/// the rest. `par` is the cost of turning the route's tiles back, which is achievable by construction;
/// it is not claimed to be the true minimum.
pub fn generate(seed: &[u8; 32], level: u8) -> Generated {
    assert!(
        (level as usize) < LEVELS_PER_DAY,
        "level {level} is outside 0..{LEVELS_PER_DAY}"
    );
    let mut label = b"puzzle/".to_vec();
    label.push(level);
    let mut stream = Stream::new(seed, &label);
    for _ in 0..MAX_ATTEMPTS {
        if let Some(generated) = attempt(&mut stream, level) {
            return generated;
        }
    }
    panic!("no acceptable puzzle for level {level} after {MAX_ATTEMPTS} attempts");
}

pub fn puzzle(seed: &[u8; 32], level: u8) -> Puzzle {
    generate(seed, level).puzzle
}

fn attempt(stream: &mut Stream, level: u8) -> Option<Generated> {
    let width = side(level);
    let height = width;
    let count = width * height;

    // Source on the left edge, target on the right edge.
    let source = stream.below(height as u32) as usize * width;
    let target = stream.below(height as u32) as usize * width + (width - 1);

    // Walls: count/8 of them, never on a terminal.
    let mut blocked = vec![false; count];
    for _ in 0..count / 8 {
        let cell = stream.below(count as u32) as usize;
        if cell != source && cell != target {
            blocked[cell] = true;
        }
    }

    let weights: Vec<u32> = (0..count).map(|_| 1 + stream.below(4)).collect();
    let path = route(width, height, &blocked, &weights, source, target)?;

    let mut cells = vec![
        Tile {
            kind: TileKind::Wall,
            orientation: 0
        };
        count
    ];
    let mut on_path = vec![false; count];
    for &cell in &path {
        on_path[cell] = true;
    }

    // Decoys and walls first, so the route's tiles overwrite whatever they land on.
    for cell in 0..count {
        if blocked[cell] {
            continue;
        }
        let roll = stream.below(100);
        let kind = if roll < 25 {
            TileKind::Straight
        } else if roll < 65 {
            TileKind::Bend
        } else if roll < 90 {
            TileKind::Tee
        } else {
            TileKind::Cross
        };
        let orientation = stream.below(kind.period() as u32) as u8;
        cells[cell] = Tile { kind, orientation };
    }
    cells[source] = Tile {
        kind: TileKind::Terminal,
        orientation: 0,
    };
    cells[target] = Tile {
        kind: TileKind::Terminal,
        orientation: 0,
    };

    // The route's interior: right arms, then a random scramble.
    let mut reference = Vec::new();
    let mut par: u32 = 0;
    for window in 1..path.len() - 1 {
        let (before, here, after) = (path[window - 1], path[window], path[window + 1]);
        let needed = bit(direction_between(here, before, width))
            | bit(direction_between(here, after, width));
        let opposite_arms = needed == bit(N) | bit(S) || needed == bit(E) | bit(W);
        let roll = stream.below(100);
        let kind = if opposite_arms {
            if roll < 60 {
                TileKind::Straight
            } else if roll < 75 {
                TileKind::Cross
            } else {
                TileKind::Tee
            }
        } else if roll < 65 {
            TileKind::Bend
        } else {
            TileKind::Tee
        };
        let period = kind.period();
        let valid: Vec<u8> = (0..period)
            .filter(|&o| arms(kind, o) & needed == needed)
            .collect();
        let planted = valid[stream.below(valid.len() as u32) as usize];
        let scramble = stream.below(period as u32) as u8;
        let tile = Tile {
            kind,
            orientation: (planted + scramble) % period,
        };
        cells[here] = tile;
        if let Some((turns, _)) = cheapest_orientation(tile, needed) {
            if turns > 0 {
                reference.push(Turn {
                    cell: here as u16,
                    turns,
                });
                par += turns as u32;
            }
        }
    }

    // Reject a puzzle that is trivial, or already solved by the draw.
    if par < MIN_PAR || connected(&cells, width, height, source, target) {
        return None;
    }

    Some(Generated {
        puzzle: Puzzle {
            level,
            width: width as u8,
            height: height as u8,
            cells,
            source: source as u16,
            target: target as u16,
            par: par as u16,
        },
        reference,
    })
}

// ---------------------------------------------------------------------------------------------
// Verifying an answer
// ---------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// More entries than the grid has cells.
    TooManyEntries,
    CellOutOfRange(u16),
    /// A wall, a terminal or a cross: turning it is not a move.
    NotRotatable(u16),
    /// `turns` must be `1..period`; a full period is no move at all.
    BadTurnCount {
        cell: u16,
        turns: u8,
    },
    DuplicateCell(u16),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Total quarter turns in the answer.
    pub moves: u32,
    pub connected: bool,
    pub par: u16,
    pub limit: u32,
    /// Connected within the move limit: the only thing a prize may depend on.
    pub win: bool,
}

/// Replay an answer on the puzzle's starting grid. Order does not matter: only the net turn per tile.
pub fn verify(puzzle: &Puzzle, answer: &[Turn]) -> Result<Outcome, VerifyError> {
    if answer.len() > puzzle.cells.len() {
        return Err(VerifyError::TooManyEntries);
    }
    let mut cells = puzzle.cells.clone();
    let mut touched = vec![false; cells.len()];
    let mut moves: u32 = 0;
    for entry in answer {
        let index = entry.cell as usize;
        if index >= cells.len() {
            return Err(VerifyError::CellOutOfRange(entry.cell));
        }
        let kind = cells[index].kind;
        if !kind.rotatable() {
            return Err(VerifyError::NotRotatable(entry.cell));
        }
        if entry.turns == 0 || entry.turns >= kind.period() {
            return Err(VerifyError::BadTurnCount {
                cell: entry.cell,
                turns: entry.turns,
            });
        }
        if touched[index] {
            return Err(VerifyError::DuplicateCell(entry.cell));
        }
        touched[index] = true;
        for _ in 0..entry.turns {
            cells[index] = cells[index].turned();
        }
        moves += entry.turns as u32;
    }
    let connected = connected(
        &cells,
        puzzle.width as usize,
        puzzle.height as usize,
        puzzle.source as usize,
        puzzle.target as usize,
    );
    let limit = puzzle.move_limit();
    Ok(Outcome {
        moves,
        connected,
        par: puzzle.par,
        limit,
        win: connected && moves <= limit,
    })
}

// ---------------------------------------------------------------------------------------------
// The daily set
// ---------------------------------------------------------------------------------------------

/// `YYYY-MM-DD`, digits only; the engine does not know calendars, the caller owns that.
pub fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}

/// The seed of one day's set. It binds the chain, the date and the **previous anchored root**, so a
/// day's puzzles are fixed by what was already on L1 before the day began. Day zero uses 32 zero bytes.
pub fn daily_seed(chain_id: u32, date: &str, previous_root: &[u8; 32]) -> Option<[u8; 32]> {
    if !valid_date(date) {
        return None;
    }
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraGrid/day-seed/v1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(date.as_bytes());
    hasher.update(previous_root);
    Some(hasher.finalize().into())
}

/// All ten puzzles of a day, easiest first.
pub fn daily_set(seed: &[u8; 32]) -> Vec<Puzzle> {
    (0..LEVELS_PER_DAY as u8)
        .map(|level| puzzle(seed, level))
        .collect()
}

/// One hash over a set, in level order, for an anchor to commit to.
pub fn set_commitment(puzzles: &[Puzzle]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(b"UltraGrid/set/v1");
    hasher.update((puzzles.len() as u32).to_le_bytes());
    for puzzle in puzzles {
        hasher.update(puzzle.id());
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn reference_answer(generated: &Generated) -> Vec<Turn> {
        generated.reference.clone()
    }

    #[test]
    fn tile_tables_match_the_documented_arms() {
        let t = |kind, orientation| arms(kind, orientation);
        assert_eq!(t(TileKind::Straight, 0), bit(N) | bit(S));
        assert_eq!(t(TileKind::Straight, 1), bit(E) | bit(W));
        assert_eq!(t(TileKind::Bend, 0), bit(N) | bit(E));
        assert_eq!(t(TileKind::Bend, 1), bit(E) | bit(S));
        assert_eq!(t(TileKind::Bend, 2), bit(S) | bit(W));
        assert_eq!(t(TileKind::Bend, 3), bit(W) | bit(N));
        assert_eq!(t(TileKind::Tee, 0), bit(N) | bit(E) | bit(S));
        assert_eq!(t(TileKind::Tee, 1), bit(E) | bit(S) | bit(W));
        assert_eq!(t(TileKind::Tee, 2), bit(S) | bit(W) | bit(N));
        assert_eq!(t(TileKind::Tee, 3), bit(W) | bit(N) | bit(E));
        assert_eq!(t(TileKind::Cross, 0), 15);
        assert_eq!(t(TileKind::Wall, 0), 0);
    }

    #[test]
    fn a_full_period_of_turns_returns_to_the_start() {
        for kind in [TileKind::Straight, TileKind::Bend, TileKind::Tee] {
            let start = Tile {
                kind,
                orientation: 0,
            };
            let mut tile = start;
            for _ in 0..kind.period() {
                tile = tile.turned();
            }
            assert_eq!(tile, start);
        }
    }

    #[test]
    fn the_stream_is_pinned() {
        // The contract the Android port must reproduce, word for word.
        let mut stream = Stream::new(&seed(0x42), b"puzzle/\x00");
        let words: Vec<u32> = (0..4).map(|_| stream.next_u32()).collect();
        println!("STREAM_WORDS={words:?}");
        assert_eq!(words, STREAM_WORDS);
    }

    #[test]
    fn below_never_leaves_its_range_and_is_deterministic() {
        let mut a = Stream::new(&seed(7), b"x");
        let mut b = Stream::new(&seed(7), b"x");
        for n in [1u32, 2, 3, 7, 100, 4_000_000_000] {
            for _ in 0..200 {
                let value = a.below(n);
                assert!(value < n);
                assert_eq!(value, b.below(n));
            }
        }
    }

    #[test]
    fn generation_is_deterministic_and_seed_sensitive() {
        for level in 0..LEVELS_PER_DAY as u8 {
            let first = puzzle(&seed(1), level);
            assert_eq!(first, puzzle(&seed(1), level));
            assert_ne!(first.id(), puzzle(&seed(2), level).id());
        }
    }

    #[test]
    fn every_puzzle_is_solvable_by_its_reference_within_the_limit() {
        for byte in 0..40u8 {
            for level in 0..LEVELS_PER_DAY as u8 {
                let generated = generate(&seed(byte), level);
                let puzzle = &generated.puzzle;

                // Not solved at the start, and not trivial.
                let start = verify(puzzle, &[]).unwrap();
                assert!(!start.connected, "seed {byte} level {level} starts solved");
                assert!(puzzle.par as u32 >= MIN_PAR);

                // The reference route works and costs exactly par.
                let outcome = verify(puzzle, &reference_answer(&generated)).unwrap();
                assert!(
                    outcome.connected,
                    "seed {byte} level {level}: reference fails"
                );
                assert_eq!(outcome.moves, puzzle.par as u32);
                assert!(outcome.win);
                assert!(outcome.limit >= puzzle.par as u32 + 2);
            }
        }
    }

    #[test]
    fn terminals_sit_on_opposite_edges_and_are_never_walled() {
        for byte in 0..20u8 {
            for level in 0..LEVELS_PER_DAY as u8 {
                let p = puzzle(&seed(byte), level);
                let w = p.width as u16;
                assert_eq!(p.source % w, 0, "source must be on the left edge");
                assert_eq!(p.target % w, w - 1, "target must be on the right edge");
                assert_eq!(p.cells[p.source as usize].kind, TileKind::Terminal);
                assert_eq!(p.cells[p.target as usize].kind, TileKind::Terminal);
                assert_eq!(p.width as usize, side(level));
            }
        }
    }

    #[test]
    fn canonical_bytes_round_trip_and_reject_damage() {
        let p = puzzle(&seed(9), 5);
        let bytes = p.canonical_bytes();
        assert_eq!(Puzzle::from_canonical_bytes(&bytes), Some(p.clone()));

        let mut short = bytes.clone();
        short.pop();
        assert!(Puzzle::from_canonical_bytes(&short).is_none());

        let mut wrong_version = bytes.clone();
        wrong_version[0] = FORMAT_VERSION + 1;
        assert!(Puzzle::from_canonical_bytes(&wrong_version).is_none());

        // An orientation outside the tile's period is not a tile.
        let mut bad_tile = bytes.clone();
        bad_tile[10] = ((TileKind::Straight as u8) << 3) | 3;
        assert!(Puzzle::from_canonical_bytes(&bad_tile).is_none());
    }

    #[test]
    fn verify_refuses_moves_that_are_not_moves() {
        let generated = generate(&seed(3), 4);
        let p = &generated.puzzle;
        let wall = p.cells.iter().position(|t| t.kind == TileKind::Wall);
        if let Some(wall) = wall {
            assert_eq!(
                verify(
                    p,
                    &[Turn {
                        cell: wall as u16,
                        turns: 1
                    }]
                ),
                Err(VerifyError::NotRotatable(wall as u16))
            );
        }
        assert_eq!(
            verify(
                p,
                &[Turn {
                    cell: p.source,
                    turns: 1
                }]
            ),
            Err(VerifyError::NotRotatable(p.source))
        );
        assert_eq!(
            verify(
                p,
                &[Turn {
                    cell: 9_999,
                    turns: 1
                }]
            ),
            Err(VerifyError::CellOutOfRange(9_999))
        );
        let bend = p
            .cells
            .iter()
            .position(|t| t.kind == TileKind::Bend)
            .unwrap() as u16;
        assert_eq!(
            verify(
                p,
                &[Turn {
                    cell: bend,
                    turns: 0
                }]
            ),
            Err(VerifyError::BadTurnCount {
                cell: bend,
                turns: 0
            })
        );
        assert_eq!(
            verify(
                p,
                &[Turn {
                    cell: bend,
                    turns: 4
                }]
            ),
            Err(VerifyError::BadTurnCount {
                cell: bend,
                turns: 4
            })
        );
        assert_eq!(
            verify(
                p,
                &[
                    Turn {
                        cell: bend,
                        turns: 1
                    },
                    Turn {
                        cell: bend,
                        turns: 1
                    }
                ]
            ),
            Err(VerifyError::DuplicateCell(bend))
        );
        let flood = vec![
            Turn {
                cell: bend,
                turns: 1
            };
            p.cells.len() + 1
        ];
        assert_eq!(verify(p, &flood), Err(VerifyError::TooManyEntries));
    }

    #[test]
    fn a_connected_answer_over_the_limit_is_not_a_win() {
        let generated = generate(&seed(5), 6);
        let p = &generated.puzzle;
        // The reference plus spare turns on tiles the route does not use: still connected (the route's
        // edges stay live), but the cost climbs past the limit.
        let used: std::collections::HashSet<u16> =
            generated.reference.iter().map(|t| t.cell).collect();
        let mut answer = generated.reference.clone();
        for (index, tile) in p.cells.iter().enumerate() {
            if answer.iter().map(|t| t.turns as u32).sum::<u32>() > p.move_limit() {
                break;
            }
            if tile.kind.rotatable() && !used.contains(&(index as u16)) {
                answer.push(Turn {
                    cell: index as u16,
                    turns: 1,
                });
            }
        }
        let outcome = verify(p, &answer).unwrap();
        assert!(
            outcome.connected,
            "turning unrelated tiles cannot cut the route"
        );
        assert!(outcome.moves > outcome.limit);
        assert!(!outcome.win);
    }

    #[test]
    fn the_order_of_entries_does_not_change_the_result() {
        let generated = generate(&seed(8), 7);
        let forward = verify(&generated.puzzle, &generated.reference).unwrap();
        let mut reversed = generated.reference.clone();
        reversed.reverse();
        assert_eq!(forward, verify(&generated.puzzle, &reversed).unwrap());
    }

    #[test]
    fn move_limit_gives_a_margin_that_grows_with_par() {
        let mut p = puzzle(&seed(1), 0);
        p.par = 4;
        assert_eq!(p.move_limit(), 6);
        p.par = 40;
        assert_eq!(p.move_limit(), 50);
    }

    #[test]
    fn daily_seed_binds_chain_date_and_previous_root() {
        let zero = [0u8; 32];
        let base = daily_seed(1, "2026-11-16", &zero).unwrap();
        assert_ne!(base, daily_seed(2, "2026-11-16", &zero).unwrap());
        assert_ne!(base, daily_seed(1, "2026-11-17", &zero).unwrap());
        assert_ne!(base, daily_seed(1, "2026-11-16", &[1u8; 32]).unwrap());
        assert_eq!(base, daily_seed(1, "2026-11-16", &zero).unwrap());
        assert!(daily_seed(1, "16-11-2026", &zero).is_none());
        assert!(daily_seed(1, "2026-1x-16", &zero).is_none());
    }

    #[test]
    fn a_day_has_ten_growing_puzzles_and_one_commitment() {
        let day = daily_set(&seed(0x42));
        assert_eq!(day.len(), LEVELS_PER_DAY);
        let sides: Vec<u8> = day.iter().map(|p| p.width).collect();
        assert_eq!(sides, vec![4, 4, 5, 5, 6, 6, 7, 7, 8, 8]);
        let ids: std::collections::HashSet<_> = day.iter().map(|p| p.id()).collect();
        assert_eq!(
            ids.len(),
            LEVELS_PER_DAY,
            "no two puzzles of a day coincide"
        );
        assert_eq!(
            set_commitment(&day),
            set_commitment(&daily_set(&seed(0x42)))
        );
        let mut other = day.clone();
        other.swap(0, 1);
        assert_ne!(
            set_commitment(&day),
            set_commitment(&other),
            "order is committed"
        );
    }

    #[test]
    fn golden_puzzles_are_pinned() {
        // The contract for the Android port: seed 0x42*32, these ids and pars, byte for byte. They were
        // also reproduced by an independent Python port written from the module's documentation alone.
        let day = daily_set(&seed(0x42));
        let observed: Vec<(String, u16)> = day
            .iter()
            .map(|p| (hex::encode(&p.id()[..8]), p.par))
            .collect();
        println!("GOLDEN={observed:?}");
        println!("COMMITMENT={}", hex::encode(set_commitment(&day)));
        println!("LEVEL0=\n{}", day[0].render());
        let expected: Vec<(String, u16)> = GOLDEN
            .iter()
            .map(|(id, par)| (id.to_string(), *par))
            .collect();
        assert_eq!(observed, expected);
        assert_eq!(hex::encode(set_commitment(&day)), GOLDEN_COMMITMENT);
    }

    const STREAM_WORDS: [u32; 4] = [1190497459, 3033664823, 940410908, 2627798237];
    const GOLDEN: [(&str, u16); 10] = [
        ("f65ba372918e9d85", 9),
        ("6c668928d256eaae", 9),
        ("2ca87b8575c4ed80", 8),
        ("bf734a2072d011e2", 5),
        ("90d5841a4936de7b", 5),
        ("ecc8f7553ed68e99", 5),
        ("8f60a599beb109a1", 8),
        ("fe1591381a86dd19", 5),
        ("0078468086630f4f", 3),
        ("f0dea9b16887eae6", 10),
    ];
    const GOLDEN_COMMITMENT: &str =
        "3a8539a3bf36d06a85483a40d288b174e111ca42e74012e1081da54e0a91027c";
}
