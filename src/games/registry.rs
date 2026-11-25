//! The game registry: how a developer's game exists on the platform, how it reports results, and what a
//! day of all of them anchors.
//!
//! **What the platform promises a third-party game, and what it does not.** It promises that *this
//! developer's key said X on day D* and that the saying was fixed in a root anchored on the chain: tamper
//! evidence. It does **not** promise X was true. Only a game whose rules the server can replay (UltraGrid)
//! can be verified; for anything else a result is the developer's claim, and the registry records it as
//! one. It also moves **no money**: a registered game's results are recorded and anchored, never paid from
//! a pool the platform holds.
//!
//! **Anchoring that does not grow with the number of games.** Each game gets one root per day over its
//! results. The platform anchors one Merkle root over *all* games' roots, so one game or a thousand cost
//! the same single anchor, and any developer can prove their day is inside it with a short proof.
//!
//! ```text
//! result record    = player(32) || SHA3-256("UltraRecord/record-id/v1" || result_id)(32) || score(u64le) || data_hash(32)
//!                                                                        sorted by (player, result id hash)
//! game day root    = SHA3-256("UltraRecord/records/v1" || u32le(len(game_id)) || game_id || u32le(count) || records)
//! leaf             = SHA3-256("UltraRecord/leaf/v1"    || u32le(len(game_id)) || game_id || game day root)
//! node             = SHA3-256("UltraRecord/node/v1"    || left || right)         an unpaired node is carried up unchanged
//! merkle root      = the single node left; SHA3-256("UltraRecord/empty/v1") when no game had results
//! registry day root= SHA3-256("UltraRecord/day-root/v1" || date || merkle root)   the 32 bytes the owner key anchors
//! ```
//!
//! Leaves are sorted by game id. An unpaired node is carried up rather than duplicated, because
//! duplicating the last node makes `[a, b, c]` and `[a, b, c, c]` share a root.

use super::{
    roots::{address_bytes, sha3},
    store::{to_i64, to_u64, GamesStore, StoreError},
};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use subtle::ConstantTimeEq;
use thiserror::Error;

pub(super) const REGISTRY_SCHEMA: &str = "
CREATE TABLE registry_games (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('sandbox','registered','listed','suspended')),
    key_hash BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX registry_games_by_owner ON registry_games (owner);

CREATE TABLE registry_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id TEXT NOT NULL REFERENCES registry_games (id),
    player TEXT NOT NULL,
    result_id TEXT NOT NULL,
    result_id_hash BLOB NOT NULL,
    score INTEGER NOT NULL CHECK (score >= 0),
    data_hash BLOB NOT NULL,
    day TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (game_id, result_id)
);
CREATE INDEX registry_results_by_day ON registry_results (game_id, day);
CREATE INDEX registry_results_by_player_day ON registry_results (game_id, player, day);

CREATE TABLE registry_days (
    day TEXT PRIMARY KEY,
    leaves TEXT NOT NULL,
    merkle_root BLOB NOT NULL,
    day_root BLOB NOT NULL,
    state TEXT NOT NULL,
    anchor_tx TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at INTEGER NOT NULL
);
";

pub const KEY_PREFIX: &str = "ugk_";
pub const MIN_ID_LEN: usize = 3;
pub const MAX_ID_LEN: usize = 32;
pub const MAX_NAME_LEN: usize = 48;
pub const MIN_RESULT_ID_LEN: usize = 8;
pub const MAX_RESULT_ID_LEN: usize = 64;
/// Larger than any score a game could mean, and safely inside SQLite's integer range.
pub const MAX_SCORE: u64 = 1 << 62;

/// Ids a developer cannot take: the first-party games, and every word the API's own routes use, so a
/// future `/api/games/{id}/…` can never collide with a fixed route.
const RESERVED_IDS: &[&str] = &[
    "grid",
    "chess",
    "ultragrid",
    "ultrachess",
    "ultra",
    "ultranet",
    "ultraverse",
    "platform",
    "registry",
    "admin",
    "games",
    "game",
    "api",
    "status",
    "wallet",
    "withdraw",
    "payouts",
    "system",
    "root",
    "register",
    "results",
    "days",
    "keys",
    "sandbox",
    "listed",
    "test",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStatus {
    /// Free and private: results are stored for a few days and **never anchored**. Where a developer
    /// starts, and what "add a game in five minutes" means.
    Sandbox,
    /// Results are stored and included in the day's anchored root.
    Registered,
    /// Registered, and curated into the app's Games directory. Listing is the operator's call.
    Listed,
    /// Results are refused. Past days stay as anchored.
    Suspended,
}

impl GameStatus {
    pub fn code(self) -> &'static str {
        match self {
            GameStatus::Sandbox => "sandbox",
            GameStatus::Registered => "registered",
            GameStatus::Listed => "listed",
            GameStatus::Suspended => "suspended",
        }
    }

    pub fn parse(value: &str) -> Option<GameStatus> {
        Some(match value {
            "sandbox" => GameStatus::Sandbox,
            "registered" => GameStatus::Registered,
            "listed" => GameStatus::Listed,
            "suspended" => GameStatus::Suspended,
            _ => return None,
        })
    }

    pub fn accepts_results(self) -> bool {
        self != GameStatus::Suspended
    }

    /// Whether this game's days are part of the platform's anchored root.
    pub fn is_anchored(self) -> bool {
        matches!(self, GameStatus::Registered | GameStatus::Listed)
    }
}

/// How much one game may write per day. Storage is the platform's cost, so it is bounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryCaps {
    pub results_per_game_per_day: u64,
    pub results_per_player_per_game_per_day: u64,
    /// A sandbox is free and unanchored, so it is held to a much smaller day.
    pub sandbox_results_per_game_per_day: u64,
    /// Games one owner address may hold. Self-service registration is free, so it is bounded.
    pub games_per_owner: u64,
    /// Games anyone may register themselves in one UTC day, across all owners.
    pub self_registrations_per_day: u64,
}

impl RegistryCaps {
    pub fn standard() -> RegistryCaps {
        RegistryCaps {
            results_per_game_per_day: 50_000,
            results_per_player_per_game_per_day: 1_000,
            sandbox_results_per_game_per_day: 500,
            games_per_owner: 3,
            self_registrations_per_day: 100,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("{0}")]
    Invalid(String),
    #[error("that game id is reserved")]
    ReservedId,
    #[error("that game id is already taken")]
    IdTaken,
    /// An unknown game and a wrong key are indistinguishable on purpose.
    #[error("the game id or key is not valid")]
    BadKey,
    #[error("this game is suspended and is not accepting results")]
    Suspended,
    #[error("a result with that id was already recorded with different contents")]
    Conflict,
    #[error("this game has reached its limit of results for today")]
    DailyCap,
    #[error("this player has reached the limit of results for this game today")]
    PlayerDailyCap,
    #[error("no such game")]
    UnknownGame,
    #[error("this address already holds the most games one owner may have")]
    OwnerLimit,
    #[error("too many games were registered today; try again tomorrow")]
    RegistrationLimit,
    #[error("storage error: {0}")]
    Store(String),
}

impl From<StoreError> for RegistryError {
    fn from(error: StoreError) -> Self {
        RegistryError::Store(error.to_string())
    }
}

impl From<rusqlite::Error> for RegistryError {
    fn from(error: rusqlite::Error) -> Self {
        RegistryError::Store(error.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRecord {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub status: GameStatus,
    pub created_at: u64,
    pub updated_at: u64,
}

/// What a game reports for one result. The platform stores it; it does not interpret it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultInput {
    pub player: String,
    /// Unique within the game. The developer's own id for this play, round or match.
    pub result_id: String,
    pub score: u64,
    /// A hash of whatever the game wants to be able to prove later (a replay, a move list). Zeros if none.
    pub data_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRow {
    pub game_id: String,
    pub player: String,
    pub result_id: String,
    pub score: u64,
    pub data_hash: [u8; 32],
    pub day: String,
    pub created_at: u64,
}

// --- validation -------------------------------------------------------------------------------

/// `a-z`, `0-9` and single hyphens, 3 to 32 characters, not starting or ending with a hyphen, and not
/// reserved. An id is an identity, so it is strict; a name is only a label.
pub fn validate_game_id(id: &str) -> Result<(), RegistryError> {
    if !(MIN_ID_LEN..=MAX_ID_LEN).contains(&id.len()) {
        return Err(RegistryError::Invalid(format!(
            "a game id is {MIN_ID_LEN} to {MAX_ID_LEN} characters"
        )));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(RegistryError::Invalid(
            "a game id may use only a-z, 0-9 and hyphens".into(),
        ));
    }
    if id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return Err(RegistryError::Invalid(
            "a game id may not start or end with a hyphen, or contain two in a row".into(),
        ));
    }
    if RESERVED_IDS.contains(&id) {
        return Err(RegistryError::ReservedId);
    }
    Ok(())
}

pub fn validate_game_name(name: &str) -> Result<String, RegistryError> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Err(RegistryError::Invalid(format!(
            "a game name is 1 to {MAX_NAME_LEN} characters"
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " -_.:'".contains(c))
    {
        return Err(RegistryError::Invalid(
            "a game name may use letters, digits, spaces and - _ . : '".into(),
        ));
    }
    Ok(name.to_string())
}

fn validate_result(input: &ResultInput) -> Result<(), RegistryError> {
    if address_bytes(&input.player).is_none() {
        return Err(RegistryError::Invalid(
            "player must be a 64-character lowercase hex address".into(),
        ));
    }
    let id = &input.result_id;
    if !(MIN_RESULT_ID_LEN..=MAX_RESULT_ID_LEN).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    {
        return Err(RegistryError::Invalid(format!(
            "result_id must be {MIN_RESULT_ID_LEN} to {MAX_RESULT_ID_LEN} letters, digits, - _ or ."
        )));
    }
    if input.score > MAX_SCORE {
        return Err(RegistryError::Invalid("score is out of range".into()));
    }
    Ok(())
}

// --- API keys ---------------------------------------------------------------------------------

/// `ugk_` and 64 hex characters of operating-system randomness. Shown once, at registration or rotation.
fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("{KEY_PREFIX}{}", hex::encode(bytes))
}

/// Only this is stored. A leaked database does not leak a usable key.
fn hash_key(key: &str) -> [u8; 32] {
    sha3(&[b"UltraGame/api-key/v1", key.as_bytes()])
}

fn result_id_hash(result_id: &str) -> [u8; 32] {
    sha3(&[b"UltraRecord/record-id/v1", result_id.as_bytes()])
}

// --- the roots --------------------------------------------------------------------------------

pub fn game_results_root(game_id: &str, results: &[ResultRow]) -> [u8; 32] {
    let mut records: Vec<([u8; 32], [u8; 32], u64, [u8; 32])> = results
        .iter()
        .filter_map(|row| {
            Some((
                address_bytes(&row.player)?,
                result_id_hash(&row.result_id),
                row.score,
                row.data_hash,
            ))
        })
        .collect();
    records.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let mut body = Vec::with_capacity(records.len() * 104);
    for (player, id_hash, score, data_hash) in &records {
        body.extend_from_slice(player);
        body.extend_from_slice(id_hash);
        body.extend_from_slice(&score.to_le_bytes());
        body.extend_from_slice(data_hash);
    }
    sha3(&[
        b"UltraRecord/records/v1",
        &(game_id.len() as u32).to_le_bytes(),
        game_id.as_bytes(),
        &(records.len() as u32).to_le_bytes(),
        &body,
    ])
}

pub fn leaf_hash(game_id: &str, game_root: &[u8; 32]) -> [u8; 32] {
    sha3(&[
        b"UltraRecord/leaf/v1",
        &(game_id.len() as u32).to_le_bytes(),
        game_id.as_bytes(),
        game_root,
    ])
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    sha3(&[b"UltraRecord/node/v1", left, right])
}

pub fn empty_root() -> [u8; 32] {
    sha3(&[b"UltraRecord/empty/v1"])
}

pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return empty_root();
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| {
                if pair.len() == 2 {
                    node_hash(&pair[0], &pair[1])
                } else {
                    pair[0]
                }
            })
            .collect();
    }
    level[0]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofStep {
    pub sibling: [u8; 32],
    pub sibling_on_left: bool,
}

/// The siblings needed to climb from `leaves[index]` to the root. An unpaired node has no sibling at
/// its level, so contributes no step there.
pub fn inclusion_proof(leaves: &[[u8; 32]], index: usize) -> Option<Vec<ProofStep>> {
    if index >= leaves.len() {
        return None;
    }
    let mut proof = Vec::new();
    let mut level = leaves.to_vec();
    let mut position = index;
    while level.len() > 1 {
        if position % 2 == 1 {
            proof.push(ProofStep {
                sibling: level[position - 1],
                sibling_on_left: true,
            });
        } else if position + 1 < level.len() {
            proof.push(ProofStep {
                sibling: level[position + 1],
                sibling_on_left: false,
            });
        }
        level = level
            .chunks(2)
            .map(|pair| {
                if pair.len() == 2 {
                    node_hash(&pair[0], &pair[1])
                } else {
                    pair[0]
                }
            })
            .collect();
        position /= 2;
    }
    Some(proof)
}

pub fn verify_inclusion(leaf: &[u8; 32], proof: &[ProofStep], root: &[u8; 32]) -> bool {
    let mut current = *leaf;
    for step in proof {
        current = if step.sibling_on_left {
            node_hash(&step.sibling, &current)
        } else {
            node_hash(&current, &step.sibling)
        };
    }
    &current == root
}

/// The 32 bytes the owner key anchors for a registry day.
pub fn registry_day_root(date: &str, merkle_root: &[u8; 32]) -> [u8; 32] {
    sha3(&[b"UltraRecord/day-root/v1", date.as_bytes(), merkle_root])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryDay {
    pub date: String,
    /// `(game id, that game's day root)`, sorted by game id.
    pub games: Vec<(String, [u8; 32])>,
    pub merkle_root: [u8; 32],
    pub day_root: [u8; 32],
    /// `closed` (fixed, not yet anchored), `anchored` (submitted) or `confirmed` (seen on the ledger).
    pub state: String,
    pub anchor_tx: Option<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub updated_at: u64,
}

/// One stored day, as the public day list needs it: roots, state, and how many applications
/// contributed. It never carries results, players, scores or data hashes.
#[derive(Debug, Clone)]
pub struct RegistryDaySummary {
    pub date: String,
    pub merkle_root: [u8; 32],
    pub day_root: [u8; 32],
    pub state: String,
    pub anchor_tx: Option<String>,
    pub app_count: usize,
}

/// Every stored day, counted. This is the one window the public counters describe: all closed days,
/// from `from_day` to `to_day`. `days_anchored` counts by **state** (`anchored` or `confirmed`);
/// the presence of `anchor_tx` is never used as a state test here.
#[derive(Debug, Clone, Default)]
pub struct RegistryDayTotals {
    pub from_day: Option<String>,
    pub to_day: Option<String>,
    pub days_closed: u64,
    pub days_with_root: u64,
    pub days_confirmed: u64,
    pub days_anchored: u64,
    pub days_pending_anchor: u64,
    pub latest_anchored_day: Option<String>,
}

impl RegistryDay {
    fn leaves(&self) -> Vec<[u8; 32]> {
        self.games
            .iter()
            .map(|(id, root)| leaf_hash(id, root))
            .collect()
    }

    /// The proof that `game_id`'s day is inside this day's root, with the leaf it starts from.
    pub fn proof_for(&self, game_id: &str) -> Option<([u8; 32], Vec<ProofStep>)> {
        let index = self.games.iter().position(|(id, _)| id == game_id)?;
        let leaves = self.leaves();
        Some((leaves[index], inclusion_proof(&leaves, index)?))
    }
}

// --- the store --------------------------------------------------------------------------------

fn game_from_row(row: &rusqlite::Row<'_>) -> Result<GameRecord, StoreError> {
    let status: String = row.get(3)?;
    Ok(GameRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        owner: row.get(2)?,
        status: GameStatus::parse(&status)
            .ok_or_else(|| StoreError::Malformed(format!("unknown game status {status}")))?,
        created_at: to_u64(row.get(4)?)?,
        updated_at: to_u64(row.get(5)?)?,
    })
}

const GAME_COLUMNS: &str = "id, name, owner, status, created_at, updated_at";

impl GamesStore {
    /// Create a game in the **sandbox** and return it with its API key. The key is shown here and never
    /// again: only its hash is kept.
    pub fn register_game(
        &self,
        id: &str,
        name: &str,
        owner: &str,
        now: u64,
    ) -> Result<(GameRecord, String), RegistryError> {
        validate_game_id(id)?;
        let name = validate_game_name(name)?;
        if address_bytes(owner).is_none() {
            return Err(RegistryError::Invalid(
                "the owner must be a 64-character lowercase hex address".into(),
            ));
        }
        let key = generate_key();
        let connection = self.lock()?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO registry_games (id, name, owner, status, key_hash, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'sandbox', ?4, ?5, ?5)",
            params![id, name, owner, hash_key(&key).as_slice(), to_i64(now)?],
        )?;
        if inserted == 0 {
            return Err(RegistryError::IdTaken);
        }
        drop(connection);
        let record = self.game(id)?.ok_or(RegistryError::UnknownGame)?;
        Ok((record, key))
    }

    /// How many games an owner holds (every status: a suspended game still counts).
    pub fn games_owned_by(&self, owner: &str) -> Result<u64, RegistryError> {
        let connection = self.lock()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM registry_games WHERE owner = ?1",
            params![owner],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// How many games were created at or after `since` (seconds).
    pub fn games_created_since(&self, since: u64) -> Result<u64, RegistryError> {
        let connection = self.lock()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM registry_games WHERE created_at >= ?1",
            params![to_i64(since)?],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Register a game for an owner who proved control of the address, within the self-service limits.
    /// The two counts and the insert happen under one lock, so concurrent requests cannot pass the same
    /// limit together.
    pub fn register_game_self_service(
        &self,
        id: &str,
        name: &str,
        owner: &str,
        day_start: u64,
        caps: &RegistryCaps,
        now: u64,
    ) -> Result<(GameRecord, String), RegistryError> {
        validate_game_id(id)?;
        let name = validate_game_name(name)?;
        let key = generate_key();
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owned: i64 = tx.query_row(
            "SELECT COUNT(*) FROM registry_games WHERE owner = ?1",
            params![owner],
            |row| row.get(0),
        )?;
        if owned as u64 >= caps.games_per_owner {
            return Err(RegistryError::OwnerLimit);
        }
        let today: i64 = tx.query_row(
            "SELECT COUNT(*) FROM registry_games WHERE created_at >= ?1",
            params![to_i64(day_start)?],
            |row| row.get(0),
        )?;
        if today as u64 >= caps.self_registrations_per_day {
            return Err(RegistryError::RegistrationLimit);
        }
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO registry_games (id, name, owner, status, key_hash, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'sandbox', ?4, ?5, ?5)",
            params![id, name, owner, hash_key(&key).as_slice(), to_i64(now)?],
        )?;
        if inserted == 0 {
            return Err(RegistryError::IdTaken);
        }
        tx.commit()?;
        drop(connection);
        let record = self.game(id)?.ok_or(RegistryError::UnknownGame)?;
        Ok((record, key))
    }

    /// Rotate a key on the owner's signed request. `proof_time` must be later than the game's last
    /// change, so a captured request cannot be replayed after it has been used.
    pub fn rotate_key_signed(
        &self,
        id: &str,
        owner: &str,
        proof_time: u64,
        now: u64,
    ) -> Result<String, RegistryError> {
        let key = generate_key();
        let connection = self.lock()?;
        let changed = connection.execute(
            "UPDATE registry_games SET key_hash = ?3, updated_at = ?4
             WHERE id = ?1 AND owner = ?2 AND updated_at < ?5",
            params![
                id,
                owner,
                hash_key(&key).as_slice(),
                to_i64(now.max(proof_time))?,
                to_i64(proof_time)?
            ],
        )?;
        if changed == 0 {
            // Unknown game, someone else's game, or a replayed request: all the same answer.
            return Err(RegistryError::BadKey);
        }
        Ok(key)
    }

    pub fn game(&self, id: &str) -> Result<Option<GameRecord>, RegistryError> {
        let connection = self.lock()?;
        let found = connection
            .query_row(
                &format!("SELECT {GAME_COLUMNS} FROM registry_games WHERE id = ?1"),
                params![id],
                |row| Ok(game_from_row(row)),
            )
            .optional()?;
        Ok(found.transpose()?)
    }

    pub fn games(&self) -> Result<Vec<GameRecord>, RegistryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM registry_games ORDER BY id"
        ))?;
        let rows = statement.query_map([], |row| Ok(game_from_row(row)))?;
        let mut games = Vec::new();
        for row in rows {
            games.push(row??);
        }
        Ok(games)
    }

    /// Check a presented key. An unknown game and a wrong key give the same answer, and an unknown game
    /// still does the same hashing and comparison, so neither the message nor the timing says which.
    pub fn authenticate(&self, id: &str, presented: &str) -> Result<GameRecord, RegistryError> {
        let stored: Option<Vec<u8>> = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT key_hash FROM registry_games WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?
        };
        let presented_hash = hash_key(presented);
        let expected = stored.clone().unwrap_or_else(|| vec![0u8; 32]);
        let matches: bool = presented_hash.as_slice().ct_eq(expected.as_slice()).into();
        if stored.is_none() || !matches {
            return Err(RegistryError::BadKey);
        }
        self.game(id)?.ok_or(RegistryError::BadKey)
    }

    /// Replace a game's key. The old one stops working at once. Returns the new key, shown once.
    pub fn rotate_key(&self, id: &str, now: u64) -> Result<String, RegistryError> {
        let key = generate_key();
        let connection = self.lock()?;
        let changed = connection.execute(
            "UPDATE registry_games SET key_hash = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, hash_key(&key).as_slice(), to_i64(now)?],
        )?;
        if changed == 0 {
            return Err(RegistryError::UnknownGame);
        }
        Ok(key)
    }

    pub fn set_game_status(
        &self,
        id: &str,
        status: GameStatus,
        now: u64,
    ) -> Result<(), RegistryError> {
        let connection = self.lock()?;
        let changed = connection.execute(
            "UPDATE registry_games SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, status.code(), to_i64(now)?],
        )?;
        if changed == 0 {
            return Err(RegistryError::UnknownGame);
        }
        Ok(())
    }

    /// Record one result for a game, once. The same result id with the same contents is replayed (and
    /// not added again); the same id with *different* contents is a conflict, never a quiet overwrite.
    /// Returns the stored row and whether it was a replay.
    pub fn record_result(
        &self,
        game_id: &str,
        input: &ResultInput,
        day: &str,
        caps: &RegistryCaps,
        now: u64,
    ) -> Result<(ResultRow, bool), RegistryError> {
        validate_result(input)?;
        let mut guard = self.lock()?;
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let status: Option<String> = tx
            .query_row(
                "SELECT status FROM registry_games WHERE id = ?1",
                params![game_id],
                |row| row.get(0),
            )
            .optional()?;
        let status = status
            .as_deref()
            .and_then(GameStatus::parse)
            .ok_or(RegistryError::UnknownGame)?;
        if !status.accepts_results() {
            return Err(RegistryError::Suspended);
        }

        let existing = tx
            .query_row(
                "SELECT player, score, data_hash, day, created_at FROM registry_results WHERE game_id = ?1 AND result_id = ?2",
                params![game_id, input.result_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        if let Some((player, score, data_hash, stored_day, created_at)) = existing {
            let same = player == input.player
                && to_u64(score)? == input.score
                && data_hash.as_slice() == input.data_hash.as_slice();
            if !same {
                return Err(RegistryError::Conflict);
            }
            return Ok((
                ResultRow {
                    game_id: game_id.to_string(),
                    player,
                    result_id: input.result_id.clone(),
                    score: input.score,
                    data_hash: input.data_hash,
                    day: stored_day,
                    created_at: to_u64(created_at)?,
                },
                true,
            ));
        }

        let count = |sql: &str, args: &[&dyn rusqlite::ToSql]| -> Result<u64, RegistryError> {
            let value: i64 = tx.query_row(sql, args, |row| row.get(0))?;
            Ok(to_u64(value)?)
        };
        let game_today = count(
            "SELECT COUNT(*) FROM registry_results WHERE game_id = ?1 AND day = ?2",
            &[&game_id, &day],
        )?;
        let game_cap = if status == GameStatus::Sandbox {
            caps.sandbox_results_per_game_per_day
        } else {
            caps.results_per_game_per_day
        };
        if game_today >= game_cap {
            return Err(RegistryError::DailyCap);
        }
        let player_today = count(
            "SELECT COUNT(*) FROM registry_results WHERE game_id = ?1 AND player = ?2 AND day = ?3",
            &[&game_id, &input.player, &day],
        )?;
        if player_today >= caps.results_per_player_per_game_per_day {
            return Err(RegistryError::PlayerDailyCap);
        }

        tx.execute(
            "INSERT INTO registry_results (game_id, player, result_id, result_id_hash, score, data_hash, day, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                game_id,
                input.player,
                input.result_id,
                result_id_hash(&input.result_id).as_slice(),
                to_i64(input.score)?,
                input.data_hash.as_slice(),
                day,
                to_i64(now)?
            ],
        )?;
        tx.commit()?;
        Ok((
            ResultRow {
                game_id: game_id.to_string(),
                player: input.player.clone(),
                result_id: input.result_id.clone(),
                score: input.score,
                data_hash: input.data_hash,
                day: day.to_string(),
                created_at: now,
            },
            false,
        ))
    }

    pub fn results_for_day(
        &self,
        game_id: &str,
        day: &str,
    ) -> Result<Vec<ResultRow>, RegistryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT player, result_id, score, data_hash, created_at FROM registry_results
             WHERE game_id = ?1 AND day = ?2 ORDER BY id",
        )?;
        let rows = statement.query_map(params![game_id, day], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            let (player, result_id, score, data_hash, created_at) = row?;
            results.push(ResultRow {
                game_id: game_id.to_string(),
                player,
                result_id,
                score: to_u64(score)?,
                data_hash: data_hash.try_into().map_err(|_| {
                    RegistryError::Store("a stored data hash is not 32 bytes".into())
                })?,
                day: day.to_string(),
                created_at: to_u64(created_at)?,
            });
        }
        Ok(results)
    }

    /// Fix a finished day's registry root, once. Only games that are **registered or listed** and had at
    /// least one result that day are included; a sandbox is never anchored. The first computation wins:
    /// a day closed once is never recomputed, so suspending a game later cannot change what was anchored.
    pub fn close_registry_day(&self, date: &str, now: u64) -> Result<RegistryDay, RegistryError> {
        if let Some(day) = self.registry_day(date)? {
            return Ok(day);
        }
        let mut games: Vec<(String, [u8; 32])> = Vec::new();
        for game in self.games()? {
            if !game.status.is_anchored() {
                continue;
            }
            let results = self.results_for_day(&game.id, date)?;
            if !results.is_empty() {
                games.push((game.id.clone(), game_results_root(&game.id, &results)));
            }
        }
        games.sort_by(|a, b| a.0.cmp(&b.0));
        let leaves: Vec<[u8; 32]> = games.iter().map(|(id, root)| leaf_hash(id, root)).collect();
        let merkle = merkle_root(&leaves);
        let day_root = registry_day_root(date, &merkle);
        let encoded: Vec<(String, String)> = games
            .iter()
            .map(|(id, root)| (id.clone(), hex::encode(root)))
            .collect();
        let connection = self.lock()?;
        connection.execute(
            "INSERT OR IGNORE INTO registry_days (day, leaves, merkle_root, day_root, state, attempts, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'closed', 0, ?5)",
            params![
                date,
                serde_json::to_string(&encoded).map_err(|e| RegistryError::Store(e.to_string()))?,
                merkle.as_slice(),
                day_root.as_slice(),
                to_i64(now)?
            ],
        )?;
        drop(connection);
        self.registry_day(date)?
            .ok_or(RegistryError::Store("registry day vanished".into()))
    }

    pub fn registry_day(&self, date: &str) -> Result<Option<RegistryDay>, RegistryError> {
        let connection = self.lock()?;
        let row = connection
            .query_row(
                "SELECT leaves, merkle_root, day_root, state, anchor_tx, attempts, last_error, updated_at
                 FROM registry_days WHERE day = ?1",
                params![date],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((leaves, merkle, day_root, state, anchor_tx, attempts, last_error, updated_at)) =
            row
        else {
            return Ok(None);
        };
        let encoded: Vec<(String, String)> =
            serde_json::from_str(&leaves).map_err(|e| RegistryError::Store(e.to_string()))?;
        let games = encoded
            .into_iter()
            .map(|(id, root)| {
                let bytes: [u8; 32] = hex::decode(&root)
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| {
                        RegistryError::Store("a stored game root is malformed".into())
                    })?;
                Ok((id, bytes))
            })
            .collect::<Result<Vec<_>, RegistryError>>()?;
        let to32 = |v: Vec<u8>| -> Result<[u8; 32], RegistryError> {
            v.try_into()
                .map_err(|_| RegistryError::Store("a stored root is not 32 bytes".into()))
        };
        Ok(Some(RegistryDay {
            date: date.to_string(),
            games,
            merkle_root: to32(merkle)?,
            day_root: to32(day_root)?,
            state,
            anchor_tx,
            attempts: u32::try_from(attempts)
                .map_err(|_| RegistryError::Store("attempts out of range".into()))?,
            last_error,
            updated_at: to_u64(updated_at)?,
        }))
    }

    /// The newest days, newest first, older than `before` if given. The caller asks for one row more
    /// than it will show, so it learns whether more exist without a second query.
    pub fn registry_day_page(
        &self,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RegistryDaySummary>, RegistryError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT day, merkle_root, day_root, state, anchor_tx, leaves
             FROM registry_days
             WHERE (?1 IS NULL OR day < ?1)
             ORDER BY day DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![before, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;
        let mut days = Vec::new();
        for row in rows {
            let (date, merkle, day_root, state, anchor_tx, leaves) = row?;
            let to32 = |value: Option<Vec<u8>>| -> Result<[u8; 32], RegistryError> {
                value
                    .ok_or_else(|| RegistryError::Store("a stored root is missing".into()))?
                    .try_into()
                    .map_err(|_| RegistryError::Store("a stored root is not 32 bytes".into()))
            };
            let encoded: Vec<(String, String)> =
                serde_json::from_str(&leaves).map_err(|e| RegistryError::Store(e.to_string()))?;
            days.push(RegistryDaySummary {
                date,
                merkle_root: to32(merkle)?,
                day_root: to32(day_root)?,
                state,
                anchor_tx,
                app_count: encoded.len(),
            });
        }
        Ok(days)
    }

    /// Counts over every stored day. States are counted as strings, exactly as they are stored:
    /// `anchored` and `confirmed` together are "anchored"; `anchored` alone is "pending".
    pub fn registry_day_totals(&self) -> Result<RegistryDayTotals, RegistryError> {
        let connection = self.lock()?;
        let (from_day, to_day, closed, with_root, confirmed, anchored, pending) = connection
            .query_row(
                "SELECT MIN(day), MAX(day), COUNT(*),
                        SUM(CASE WHEN day_root IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN state = 'confirmed' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN state IN ('anchored', 'confirmed') THEN 1 ELSE 0 END),
                        SUM(CASE WHEN state = 'anchored' THEN 1 ELSE 0 END)
                 FROM registry_days",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )?;
        let latest_anchored_day = connection
            .query_row(
                "SELECT MAX(day) FROM registry_days WHERE state IN ('anchored', 'confirmed')",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        let count = |value: Option<i64>| -> Result<u64, RegistryError> {
            u64::try_from(value.unwrap_or(0))
                .map_err(|_| RegistryError::Store("a day count is negative".into()))
        };
        Ok(RegistryDayTotals {
            from_day,
            to_day,
            days_closed: count(Some(closed))?,
            days_with_root: count(with_root)?,
            days_confirmed: count(confirmed)?,
            days_anchored: count(anchored)?,
            days_pending_anchor: count(pending)?,
            latest_anchored_day,
        })
    }

    /// Record where a registry day's anchor stands. Never touches the roots.
    pub fn set_registry_anchor(
        &self,
        date: &str,
        tx_hash: Option<&str>,
        state: &str,
        now: u64,
    ) -> Result<(), RegistryError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE registry_days SET state = ?2, anchor_tx = COALESCE(?3, anchor_tx), updated_at = ?4 WHERE day = ?1",
            params![date, state, tx_hash, to_i64(now)?],
        )?;
        Ok(())
    }

    pub fn record_registry_anchor_error(
        &self,
        date: &str,
        error: &str,
        now: u64,
    ) -> Result<(), RegistryError> {
        let connection = self.lock()?;
        connection.execute(
            "UPDATE registry_days SET attempts = attempts + 1, last_error = ?2, updated_at = ?3 WHERE day = ?1",
            params![date, error, to_i64(now)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const OWNER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    const PLAYER_A: &str = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
    const PLAYER_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const DAY: &str = "2026-11-16";
    const NOW: u64 = 1_794_800_000;

    fn store() -> GamesStore {
        GamesStore::open(Path::new(":memory:")).unwrap()
    }

    fn result(player: &str, id: &str, score: u64) -> ResultInput {
        ResultInput {
            player: player.to_string(),
            result_id: id.to_string(),
            score,
            data_hash: [0u8; 32],
        }
    }

    fn row(player: &str, id: &str, score: u64, fill: u8) -> ResultRow {
        ResultRow {
            game_id: "ignored".into(),
            player: player.into(),
            result_id: id.into(),
            score,
            data_hash: [fill; 32],
            day: DAY.into(),
            created_at: NOW,
        }
    }

    // --- ids, names, keys -------------------------------------------------------------------

    #[test]
    fn a_game_id_is_strict_because_it_is_an_identity() {
        for good in [
            "tetris",
            "ab-1",
            "my-game-2",
            "a1b",
            "x".repeat(32).as_str(),
        ] {
            assert!(validate_game_id(good).is_ok(), "{good}");
        }
        for bad in [
            "ab",
            "Tetris",
            "my game",
            "-lead",
            "trail-",
            "do--uble",
            "under_score",
            "é-game",
            "",
        ] {
            assert!(
                matches!(validate_game_id(bad), Err(RegistryError::Invalid(_))),
                "{bad}"
            );
        }
        assert!(matches!(
            validate_game_id(&"x".repeat(33)),
            Err(RegistryError::Invalid(_))
        ));
    }

    #[test]
    fn the_first_party_games_and_the_apis_own_words_are_reserved() {
        for reserved in [
            "grid",
            "chess",
            "ultragrid",
            "platform",
            "status",
            "wallet",
            "withdraw",
            "register",
            "admin",
        ] {
            assert_eq!(
                validate_game_id(reserved),
                Err(RegistryError::ReservedId),
                "{reserved}"
            );
        }
    }

    #[test]
    fn a_name_is_only_a_label() {
        assert_eq!(
            validate_game_name("  Space Tetris: Redux  ").unwrap(),
            "Space Tetris: Redux"
        );
        assert!(validate_game_name("").is_err());
        assert!(validate_game_name(&"x".repeat(49)).is_err());
        assert!(validate_game_name("bad<script>").is_err());
        assert!(validate_game_name("line\nbreak").is_err());
    }

    #[test]
    fn registering_returns_a_key_once_and_stores_only_its_hash() {
        let store = store();
        let (game, key) = store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        assert_eq!(
            (game.status, game.owner.as_str()),
            (GameStatus::Sandbox, OWNER)
        );
        assert!(key.starts_with("ugk_") && key.len() == 4 + 64);

        // Nothing in the database equals, or contains, the key.
        let dump: String = {
            let connection = store.lock().unwrap();
            let blob: Vec<u8> = connection
                .query_row(
                    "SELECT key_hash FROM registry_games WHERE id = 'tetris'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            hex::encode(blob)
        };
        assert!(!dump.contains(&key[4..]) && dump != key);
        assert_eq!(dump, hex::encode(hash_key(&key)));
    }

    #[test]
    fn a_game_id_can_be_taken_only_once_and_two_keys_never_coincide() {
        let store = store();
        let (_, first) = store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        assert_eq!(
            store
                .register_game("tetris", "Other", PLAYER_A, NOW)
                .unwrap_err(),
            RegistryError::IdTaken
        );
        let (_, second) = store.register_game("snake", "Snake", OWNER, NOW).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn the_right_key_authenticates_and_every_other_case_looks_the_same() {
        let store = store();
        let (_, key) = store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        assert_eq!(store.authenticate("tetris", &key).unwrap().id, "tetris");
        let wrong = store.authenticate("tetris", "ugk_nope").unwrap_err();
        let unknown = store.authenticate("no-such-game", &key).unwrap_err();
        let (_, other_key) = store.register_game("snake", "Snake", OWNER, NOW).unwrap();
        let crossed = store.authenticate("tetris", &other_key).unwrap_err();
        assert_eq!(wrong, RegistryError::BadKey);
        assert_eq!(
            unknown,
            RegistryError::BadKey,
            "a stranger cannot learn which ids exist"
        );
        assert_eq!(
            crossed,
            RegistryError::BadKey,
            "one game's key opens no other game"
        );
        assert_eq!(wrong.to_string(), unknown.to_string());
    }

    #[test]
    fn rotating_a_key_retires_the_old_one_at_once() {
        let store = store();
        let (_, old) = store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        let new = store.rotate_key("tetris", NOW + 1).unwrap();
        assert_ne!(old, new);
        assert_eq!(
            store.authenticate("tetris", &old).unwrap_err(),
            RegistryError::BadKey
        );
        assert!(store.authenticate("tetris", &new).is_ok());
        assert_eq!(
            store.rotate_key("ghost", NOW).unwrap_err(),
            RegistryError::UnknownGame
        );
    }

    // --- results ----------------------------------------------------------------------------

    #[test]
    fn a_result_is_recorded_once_and_a_replay_adds_nothing() {
        let store = store();
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        let caps = RegistryCaps::standard();
        let (stored, replay) = store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 900),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        assert!(!replay);
        assert_eq!((stored.score, stored.day.as_str()), (900, DAY));
        let (again, replay) = store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 900),
                "2026-11-17",
                &caps,
                NOW + 9,
            )
            .unwrap();
        assert!(replay);
        assert_eq!(
            again.day, DAY,
            "a replay returns what was stored, not a new day"
        );
        assert_eq!(store.results_for_day("tetris", DAY).unwrap().len(), 1);
        assert!(store
            .results_for_day("tetris", "2026-11-17")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_same_result_id_with_different_contents_is_a_conflict_never_an_overwrite() {
        let store = store();
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        let caps = RegistryCaps::standard();
        store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 900),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        for tampered in [
            result(PLAYER_A, "match-0001", 901),
            result(PLAYER_B, "match-0001", 900),
            ResultInput {
                data_hash: [9u8; 32],
                ..result(PLAYER_A, "match-0001", 900)
            },
        ] {
            assert_eq!(
                store
                    .record_result("tetris", &tampered, DAY, &caps, NOW)
                    .unwrap_err(),
                RegistryError::Conflict
            );
        }
        assert_eq!(store.results_for_day("tetris", DAY).unwrap()[0].score, 900);
    }

    #[test]
    fn two_games_may_reuse_the_same_result_id() {
        let store = store();
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        store.register_game("snake", "Snake", OWNER, NOW).unwrap();
        let caps = RegistryCaps::standard();
        assert!(store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 1),
                DAY,
                &caps,
                NOW
            )
            .is_ok());
        assert!(store
            .record_result("snake", &result(PLAYER_A, "match-0001", 2), DAY, &caps, NOW)
            .is_ok());
    }

    #[test]
    fn a_result_must_be_well_formed() {
        let store = store();
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        let caps = RegistryCaps::standard();
        for bad in [
            result("not-an-address", "match-0001", 1),
            result(PLAYER_A, "short", 1),
            result(PLAYER_A, "has spaces in it", 1),
            result(PLAYER_A, &"x".repeat(65), 1),
            result(PLAYER_A, "match-0001", MAX_SCORE + 1),
        ] {
            assert!(
                matches!(
                    store.record_result("tetris", &bad, DAY, &caps, NOW),
                    Err(RegistryError::Invalid(_))
                ),
                "{bad:?}"
            );
        }
        assert!(store.results_for_day("tetris", DAY).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_or_suspended_game_takes_no_results() {
        let store = store();
        let caps = RegistryCaps::standard();
        assert_eq!(
            store
                .record_result(
                    "ghost-game",
                    &result(PLAYER_A, "match-0001", 1),
                    DAY,
                    &caps,
                    NOW
                )
                .unwrap_err(),
            RegistryError::UnknownGame
        );
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        store
            .set_game_status("tetris", GameStatus::Suspended, NOW)
            .unwrap();
        assert_eq!(
            store
                .record_result(
                    "tetris",
                    &result(PLAYER_A, "match-0001", 1),
                    DAY,
                    &caps,
                    NOW
                )
                .unwrap_err(),
            RegistryError::Suspended
        );
    }

    #[test]
    fn the_daily_limits_hold_per_game_and_per_player_and_a_sandbox_gets_the_smallest_day() {
        let store = store();
        store.register_game("tetris", "Tetris", OWNER, NOW).unwrap();
        let caps = RegistryCaps {
            results_per_game_per_day: 4,
            results_per_player_per_game_per_day: 2,
            sandbox_results_per_game_per_day: 3,
            ..RegistryCaps::standard()
        };
        // Per player: the third result by one player is refused, another player is not.
        store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 1),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0002", 1),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        assert_eq!(
            store
                .record_result(
                    "tetris",
                    &result(PLAYER_A, "match-0003", 1),
                    DAY,
                    &caps,
                    NOW
                )
                .unwrap_err(),
            RegistryError::PlayerDailyCap
        );
        store
            .record_result(
                "tetris",
                &result(PLAYER_B, "match-0004", 1),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        // A sandbox stops at 3 even though a registered game would allow 4.
        assert_eq!(
            store
                .record_result(
                    "tetris",
                    &result(PLAYER_B, "match-0005", 1),
                    DAY,
                    &caps,
                    NOW
                )
                .unwrap_err(),
            RegistryError::DailyCap
        );
        // Registered, the same game may take one more; the next day starts fresh.
        store
            .set_game_status("tetris", GameStatus::Registered, NOW)
            .unwrap();
        assert!(
            store
                .record_result(
                    "tetris",
                    &result(PLAYER_B, "match-0005", 1),
                    DAY,
                    &caps,
                    NOW
                )
                .is_err_and(|e| e == RegistryError::DailyCap)
                == false
        );
        assert!(store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0006", 1),
                "2026-11-17",
                &caps,
                NOW
            )
            .is_ok());
    }

    // --- roots and Merkle proofs --------------------------------------------------------------

    #[test]
    fn a_games_results_root_ignores_arrival_order_and_names_its_game() {
        let one = vec![
            row(PLAYER_A, "match-0001", 5, 1),
            row(PLAYER_B, "match-0002", 9, 2),
        ];
        let mut reversed = one.clone();
        reversed.reverse();
        assert_eq!(
            game_results_root("tetris", &one),
            game_results_root("tetris", &reversed)
        );
        assert_ne!(
            game_results_root("tetris", &one),
            game_results_root("snake", &one),
            "the same results under another game id are a different root"
        );
    }

    #[test]
    fn every_field_of_a_result_changes_the_root() {
        let base = vec![row(PLAYER_A, "match-0001", 5, 1)];
        let root = game_results_root("tetris", &base);
        for changed in [
            vec![row(PLAYER_B, "match-0001", 5, 1)],
            vec![row(PLAYER_A, "match-0002", 5, 1)],
            vec![row(PLAYER_A, "match-0001", 6, 1)],
            vec![row(PLAYER_A, "match-0001", 5, 2)],
            vec![],
        ] {
            assert_ne!(root, game_results_root("tetris", &changed));
        }
    }

    fn leaves(count: usize) -> Vec<[u8; 32]> {
        (0..count)
            .map(|i| leaf_hash(&format!("game-{i:02}"), &[i as u8; 32]))
            .collect()
    }

    #[test]
    fn every_leaf_of_every_tree_up_to_seventeen_has_a_proof_that_verifies() {
        for count in 1..=17 {
            let tree = leaves(count);
            let root = merkle_root(&tree);
            for (index, leaf) in tree.iter().enumerate() {
                let proof = inclusion_proof(&tree, index).unwrap();
                assert!(
                    verify_inclusion(leaf, &proof, &root),
                    "tree of {count}, leaf {index}"
                );
                assert!(proof.len() <= 5, "a proof is logarithmic, not linear");
            }
            assert!(inclusion_proof(&tree, count).is_none());
        }
    }

    #[test]
    fn a_wrong_leaf_a_wrong_proof_or_a_wrong_root_does_not_verify() {
        let tree = leaves(6);
        let root = merkle_root(&tree);
        let proof = inclusion_proof(&tree, 2).unwrap();
        assert!(verify_inclusion(&tree[2], &proof, &root));
        assert!(!verify_inclusion(&tree[3], &proof, &root), "another leaf");
        assert!(
            !verify_inclusion(&tree[2], &proof, &[7u8; 32]),
            "another root"
        );
        let mut tampered = proof.clone();
        tampered[0].sibling[0] ^= 1;
        assert!(
            !verify_inclusion(&tree[2], &tampered, &root),
            "a bent sibling"
        );
        let mut flipped = proof.clone();
        flipped[0].sibling_on_left = !flipped[0].sibling_on_left;
        assert!(
            !verify_inclusion(&tree[2], &flipped, &root),
            "a flipped side"
        );
        assert!(
            !verify_inclusion(&tree[2], &proof[1..], &root),
            "a truncated proof"
        );
    }

    #[test]
    fn carrying_an_unpaired_node_up_means_no_two_trees_share_a_root() {
        let [a, b, c, ..] = leaves(3)
            .try_into()
            .unwrap_or_else(|v: Vec<[u8; 32]>| [v[0], v[1], v[2]]);
        assert_ne!(
            merkle_root(&[a, b, c]),
            merkle_root(&[a, b, c, c]),
            "no duplicate-last-node ambiguity"
        );
        assert_ne!(merkle_root(&[a, b]), merkle_root(&[a, b, c]));
        assert_ne!(merkle_root(&[]), merkle_root(&[a]));
        assert_eq!(merkle_root(&[a]), a, "a single leaf is its own root");
    }

    #[test]
    fn the_order_games_were_registered_in_does_not_change_the_days_root() {
        let store = store();
        for id in ["zeta", "alpha", "middle"] {
            store.register_game(id, id, OWNER, NOW).unwrap();
            store
                .set_game_status(id, GameStatus::Registered, NOW)
                .unwrap();
            store
                .record_result(
                    id,
                    &result(PLAYER_A, "match-0001", 7),
                    DAY,
                    &RegistryCaps::standard(),
                    NOW,
                )
                .unwrap();
        }
        let day = store.close_registry_day(DAY, NOW).unwrap();
        assert_eq!(
            day.games
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "middle", "zeta"]
        );
    }

    // --- the day ----------------------------------------------------------------------------

    fn two_games_and_a_sandbox(store: &GamesStore) {
        let caps = RegistryCaps::standard();
        for (id, status) in [
            ("tetris", GameStatus::Registered),
            ("snake", GameStatus::Listed),
            ("scratch", GameStatus::Sandbox),
            ("quiet", GameStatus::Registered),
        ] {
            store.register_game(id, id, OWNER, NOW).unwrap();
            store.set_game_status(id, status, NOW).unwrap();
        }
        store
            .record_result(
                "tetris",
                &result(PLAYER_A, "match-0001", 10),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        store
            .record_result(
                "snake",
                &result(PLAYER_B, "match-0001", 20),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
        store
            .record_result(
                "scratch",
                &result(PLAYER_A, "match-0001", 30),
                DAY,
                &caps,
                NOW,
            )
            .unwrap();
    }

    #[test]
    fn only_registered_and_listed_games_with_results_are_in_the_days_root() {
        let store = store();
        two_games_and_a_sandbox(&store);
        let day = store.close_registry_day(DAY, NOW).unwrap();
        let ids: Vec<&str> = day.games.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["snake", "tetris"],
            "the sandbox and the silent game are not anchored"
        );
        assert_eq!(day.merkle_root, merkle_root(&day.leaves()));
        assert_eq!(day.day_root, registry_day_root(DAY, &day.merkle_root));
    }

    #[test]
    fn a_day_with_no_anchored_results_has_a_defined_empty_root() {
        let store = store();
        let day = store.close_registry_day(DAY, NOW).unwrap();
        assert!(day.games.is_empty());
        assert_eq!(day.merkle_root, empty_root());
    }

    #[test]
    fn a_closed_day_never_changes_even_if_a_game_is_suspended_or_a_result_arrives_later() {
        let store = store();
        two_games_and_a_sandbox(&store);
        let first = store.close_registry_day(DAY, NOW).unwrap();
        store
            .set_game_status("tetris", GameStatus::Suspended, NOW + 5)
            .unwrap();
        store
            .record_result(
                "snake",
                &result(PLAYER_A, "match-0002", 99),
                DAY,
                &RegistryCaps::standard(),
                NOW + 6,
            )
            .unwrap();
        let again = store.close_registry_day(DAY, NOW + 7).unwrap();
        assert_eq!(first, again, "what was fixed is what stays");
    }

    #[test]
    fn a_developer_can_prove_their_game_is_inside_the_anchored_root() {
        let store = store();
        two_games_and_a_sandbox(&store);
        let day = store.close_registry_day(DAY, NOW).unwrap();
        for (id, game_root) in &day.games {
            let (leaf, proof) = day.proof_for(id).unwrap();
            assert_eq!(leaf, leaf_hash(id, game_root));
            assert!(verify_inclusion(&leaf, &proof, &day.merkle_root));
        }
        assert!(
            day.proof_for("scratch").is_none(),
            "a sandbox game has nothing to prove"
        );
        // The developer recomputes their own root from their own records, so the proof is checkable end to end.
        let mine = store.results_for_day("tetris", DAY).unwrap();
        let recomputed = game_results_root("tetris", &mine);
        assert_eq!(
            recomputed,
            day.games.iter().find(|(id, _)| id == "tetris").unwrap().1
        );
    }

    // --- the schema -------------------------------------------------------------------------

    #[test]
    fn the_database_upgrades_in_place_from_the_first_schema_without_touching_existing_data() {
        let dir = std::env::temp_dir().join(format!("games-upgrade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("games.db");
        {
            let store = GamesStore::open(&path).unwrap();
            store
                .record_win_unlimited(
                    &super::super::store::WinInput {
                        game: "chess",
                        address: PLAYER_A.to_string(),
                        day: DAY.to_string(),
                        item: "game-upgrade-0001".to_string(),
                        moves: 40,
                        detail: "{}".to_string(),
                        verified: false,
                        client_digest: None,
                    },
                    &super::super::rules::Policy::standard().with_chess_paying(),
                    true,
                    NOW,
                )
                .unwrap();
            // Put the file back the way a build from before the registry left it.
            let connection = store.lock().unwrap();
            connection
                .execute_batch(
                    "DROP TABLE platform_game_audit; DROP TABLE platform_games; DROP TABLE auth_sessions; DROP TABLE auth_challenges; DROP TABLE registry_anchor_attempts; DROP TABLE peer_anchor_roots; DROP TABLE peer_anchor_days;
                     DROP TABLE registry_days; DROP TABLE registry_results; DROP TABLE registry_games;
                     UPDATE meta SET value = '1' WHERE key = 'schema_version';",
                )
                .unwrap();
        }
        let upgraded = GamesStore::open(&path).unwrap();
        assert_eq!(
            upgraded.wallet(PLAYER_A, DAY).unwrap().credited,
            100,
            "existing balances are untouched"
        );
        assert!(upgraded.audit().unwrap().ok());
        // Schema 1 went through 2 and 3 and arrived at 4: the later tables exist and are empty.
        assert_eq!(upgraded.peer_anchor_usage(DAY).unwrap(), 0);
        assert!(upgraded.peer_hold().unwrap().is_none());
        assert!(upgraded.registry_anchor(DAY).unwrap().is_none());
        assert!(
            upgraded
                .register_game("tetris", "Tetris", OWNER, NOW)
                .is_ok(),
            "the registry works after the upgrade"
        );
        drop(upgraded);
        // And opening it again is a no-op, not a second upgrade.
        assert!(GamesStore::open(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- the pinned vectors -------------------------------------------------------------------

    #[test]
    fn the_roots_are_pinned() {
        // Reproduced independently in Python from the layouts in the module documentation.
        let results = vec![
            row(PLAYER_B, "match-0002", 9, 2),
            row(PLAYER_A, "match-0001", 5, 1),
        ];
        let tetris = game_results_root("tetris", &results);
        let snake = game_results_root("snake", &[row(PLAYER_A, "round-7", 1_000, 0)]);
        let zeta = game_results_root("zeta", &[]);
        let tree = [
            leaf_hash("snake", &snake),
            leaf_hash("tetris", &tetris),
            leaf_hash("zeta", &zeta),
        ];
        let root = merkle_root(&tree);
        let day = registry_day_root(DAY, &root);
        let proof_for_zeta = inclusion_proof(&tree, 2).unwrap();
        println!("TETRIS={}", hex::encode(tetris));
        println!("SNAKE={}", hex::encode(snake));
        println!("MERKLE={}", hex::encode(root));
        println!("DAY={}", hex::encode(day));
        println!(
            "ZETA_PROOF={:?}",
            proof_for_zeta
                .iter()
                .map(|s| (hex::encode(s.sibling), s.sibling_on_left))
                .collect::<Vec<_>>()
        );
        println!("EMPTY={}", hex::encode(empty_root()));
        assert_eq!(hex::encode(tetris), PINNED_TETRIS);
        assert_eq!(hex::encode(snake), PINNED_SNAKE);
        assert_eq!(hex::encode(root), PINNED_MERKLE);
        assert_eq!(hex::encode(day), PINNED_DAY);
        assert_eq!(hex::encode(empty_root()), PINNED_EMPTY);
        assert_eq!(proof_for_zeta.len(), 1);
        assert!(proof_for_zeta[0].sibling_on_left);
        assert_eq!(hex::encode(proof_for_zeta[0].sibling), PINNED_ZETA_SIBLING);
    }

    const PINNED_TETRIS: &str = "a9548ebbb42d3fed507a94995fae1b5066f84b169bc83221b6811bb60ee6b744";
    const PINNED_SNAKE: &str = "d64fdd3860a91abc4ced3dd26c56788ab94e8b996887d87eeac1fc1c4bbb1859";
    const PINNED_MERKLE: &str = "8aee448bb31e3c53e33e27239a3e313478d8d29fff7dcf179b0b8b5907d2ccc8";
    const PINNED_DAY: &str = "c003c41c297e990fb0264d4cbdb9c8958622d3b3fcb6cbee50eade9246ad2823";
    const PINNED_EMPTY: &str = "fb474aef10d6c2a3314304c9669047c2fb1d5886549932e41e37440aed9a3fd8";
    const PINNED_ZETA_SIBLING: &str =
        "9fafc8c72f9953e313109dea0990ed2595c4d47812cc1018cbc58f662dd6f96c";
}
