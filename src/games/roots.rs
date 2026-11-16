//! What a day anchors. Each game's day closes into one 32-byte root that the chain's owner signs.
//!
//! The root commits to **which puzzles were asked** and **which wins were accepted**, in a canonical
//! order, so anyone holding the same list of results can recompute it and compare it with the anchor.
//! It proves nothing about whether a player was human, and the documentation says so.
//!
//! ```text
//! grid record    = address(32) || level(1) || moves(u16le) || puzzle_id(32)          sorted by (address, level)
//! grid results   = SHA3-256("UltraGrid/results/v1"  || u32le(count) || records)
//! grid day root  = SHA3-256("UltraGrid/day-root/v1" || date || commitment(32) || results)
//!
//! chess record   = address(32) || SHA3-256("UltraChess/game-id/v1" || game_id) || mode(1) || plies(u16le)
//!                                                                                   sorted by (address, game id hash)
//! chess results  = SHA3-256("UltraChess/results/v1"  || u32le(count) || records)
//! chess day root = SHA3-256("UltraChess/day-root/v1" || date || results)
//! ```
//!
//! `date` is `YYYY-MM-DD` in ASCII. `commitment` is the Grid set commitment for that day.

use sha3::{Digest, Sha3_256};

fn sha3(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// A 64-character hex address as 32 bytes.
pub fn address_bytes(address: &str) -> Option<[u8; 32]> {
    if !crate::UltraBlockchain::is_valid_address(address) {
        return None;
    }
    hex::decode(address).ok()?.try_into().ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridResult {
    pub address: [u8; 32],
    pub level: u8,
    pub moves: u16,
    pub puzzle_id: [u8; 32],
}

pub fn grid_results_root(mut results: Vec<GridResult>) -> [u8; 32] {
    results.sort_by(|a, b| (a.address, a.level).cmp(&(b.address, b.level)));
    let mut body = Vec::with_capacity(results.len() * 67);
    for result in &results {
        body.extend_from_slice(&result.address);
        body.push(result.level);
        body.extend_from_slice(&result.moves.to_le_bytes());
        body.extend_from_slice(&result.puzzle_id);
    }
    sha3(&[
        b"UltraGrid/results/v1",
        &(results.len() as u32).to_le_bytes(),
        &body,
    ])
}

pub fn grid_day_root(date: &str, commitment: &[u8; 32], results_root: &[u8; 32]) -> [u8; 32] {
    sha3(&[
        b"UltraGrid/day-root/v1",
        date.as_bytes(),
        commitment,
        results_root,
    ])
}

pub fn chess_mode_code(mode: &str) -> Option<u8> {
    match mode {
        "classic" => Some(0),
        "random" => Some(1),
        "anti" => Some(2),
        _ => None,
    }
}

pub fn chess_game_id_hash(game_id: &str) -> [u8; 32] {
    sha3(&[b"UltraChess/game-id/v1", game_id.as_bytes()])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChessResult {
    pub address: [u8; 32],
    pub game_id_hash: [u8; 32],
    pub mode: u8,
    pub plies: u16,
}

pub fn chess_results_root(mut results: Vec<ChessResult>) -> [u8; 32] {
    results.sort_by(|a, b| (a.address, a.game_id_hash).cmp(&(b.address, b.game_id_hash)));
    let mut body = Vec::with_capacity(results.len() * 67);
    for result in &results {
        body.extend_from_slice(&result.address);
        body.extend_from_slice(&result.game_id_hash);
        body.push(result.mode);
        body.extend_from_slice(&result.plies.to_le_bytes());
    }
    sha3(&[
        b"UltraChess/results/v1",
        &(results.len() as u32).to_le_bytes(),
        &body,
    ])
}

pub fn chess_day_root(date: &str, results_root: &[u8; 32]) -> [u8; 32] {
    sha3(&[b"UltraChess/day-root/v1", date.as_bytes(), results_root])
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
    const B: &str = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";

    fn addr(hex_address: &str) -> [u8; 32] {
        address_bytes(hex_address).unwrap()
    }

    fn grid(address: &str, level: u8, moves: u16, id_byte: u8) -> GridResult {
        GridResult {
            address: addr(address),
            level,
            moves,
            puzzle_id: [id_byte; 32],
        }
    }

    #[test]
    fn an_address_is_exactly_sixty_four_lowercase_hex_characters() {
        assert!(address_bytes(A).is_some());
        assert!(address_bytes(&A.to_uppercase()).is_none());
        assert!(address_bytes(&A[..63]).is_none());
        assert!(address_bytes("0x0").is_none());
    }

    #[test]
    fn the_results_root_does_not_depend_on_the_order_results_arrived_in() {
        let one = vec![grid(A, 0, 5, 1), grid(B, 3, 9, 2), grid(A, 2, 7, 3)];
        let mut other = one.clone();
        other.reverse();
        assert_eq!(grid_results_root(one), grid_results_root(other));
    }

    #[test]
    fn every_field_of_a_result_changes_the_root() {
        let base = vec![grid(A, 0, 5, 1)];
        let root = grid_results_root(base.clone());
        assert_ne!(root, grid_results_root(vec![grid(B, 0, 5, 1)]), "address");
        assert_ne!(root, grid_results_root(vec![grid(A, 1, 5, 1)]), "level");
        assert_ne!(root, grid_results_root(vec![grid(A, 0, 6, 1)]), "moves");
        assert_ne!(root, grid_results_root(vec![grid(A, 0, 5, 2)]), "puzzle id");
        assert_ne!(
            root,
            grid_results_root(vec![]),
            "an empty day is not this day"
        );
        assert_ne!(
            grid_results_root(base.clone()),
            grid_results_root(vec![base[0].clone(), base[0].clone()]),
            "a repeated result is counted"
        );
    }

    #[test]
    fn the_day_root_binds_date_commitment_and_results() {
        let results = grid_results_root(vec![grid(A, 0, 5, 1)]);
        let commitment = [7u8; 32];
        let root = grid_day_root("2026-11-16", &commitment, &results);
        assert_ne!(root, grid_day_root("2026-11-17", &commitment, &results));
        assert_ne!(root, grid_day_root("2026-11-16", &[8u8; 32], &results));
        assert_ne!(root, grid_day_root("2026-11-16", &commitment, &[0u8; 32]));
    }

    #[test]
    fn chess_results_are_ordered_and_field_sensitive() {
        let result = |address: &str, game_id: &str, mode: u8, plies: u16| ChessResult {
            address: addr(address),
            game_id_hash: chess_game_id_hash(game_id),
            mode,
            plies,
        };
        let base = vec![result(A, "game-one", 0, 40), result(B, "game-two", 2, 31)];
        let mut reversed = base.clone();
        reversed.reverse();
        let root = chess_results_root(base.clone());
        assert_eq!(root, chess_results_root(reversed));
        assert_ne!(
            root,
            chess_results_root(vec![
                result(A, "game-one", 1, 40),
                result(B, "game-two", 2, 31)
            ])
        );
        assert_ne!(
            root,
            chess_results_root(vec![
                result(A, "game-one", 0, 41),
                result(B, "game-two", 2, 31)
            ])
        );
        assert_ne!(
            root,
            chess_results_root(vec![
                result(A, "game-ONE", 0, 40),
                result(B, "game-two", 2, 31)
            ])
        );
        assert_ne!(
            chess_day_root("2026-11-16", &root),
            chess_day_root("2026-11-17", &root)
        );
        assert_eq!(chess_mode_code("classic"), Some(0));
        assert_eq!(chess_mode_code("random"), Some(1));
        assert_eq!(chess_mode_code("anti"), Some(2));
        assert_eq!(chess_mode_code("blitz"), None);
    }

    #[test]
    fn the_two_games_cannot_produce_each_others_roots() {
        // Same date and an empty day: the domains keep a Grid root and a Chess root apart.
        let grid_empty = grid_results_root(vec![]);
        let chess_empty = chess_results_root(vec![]);
        assert_ne!(grid_empty, chess_empty);
        assert_ne!(
            grid_day_root("2026-11-16", &[0u8; 32], &grid_empty),
            chess_day_root("2026-11-16", &chess_empty)
        );
    }

    #[test]
    fn the_roots_are_pinned() {
        // Reproduced independently in Python from the layouts in the module documentation.
        let results = grid_results_root(vec![grid(B, 3, 9, 2), grid(A, 0, 5, 1)]);
        let day = grid_day_root("2026-11-16", &[7u8; 32], &results);
        let chess = chess_results_root(vec![ChessResult {
            address: addr(A),
            game_id_hash: chess_game_id_hash("game-one"),
            mode: 2,
            plies: 31,
        }]);
        let chess_day = chess_day_root("2026-11-16", &chess);
        println!("GRID_RESULTS={}", hex::encode(results));
        println!("GRID_DAY={}", hex::encode(day));
        println!("CHESS_RESULTS={}", hex::encode(chess));
        println!("CHESS_DAY={}", hex::encode(chess_day));
        assert_eq!(hex::encode(results), PINNED_GRID_RESULTS);
        assert_eq!(hex::encode(day), PINNED_GRID_DAY);
        assert_eq!(hex::encode(chess), PINNED_CHESS_RESULTS);
        assert_eq!(hex::encode(chess_day), PINNED_CHESS_DAY);
    }

    const PINNED_GRID_RESULTS: &str =
        "354b5eaa238a39f300644e263d581cce9240224c2dab9a52f470b22d5446983c";
    const PINNED_GRID_DAY: &str =
        "3b3e62df3913c6b603174da7a4602a8d1fb83d0c55c967b5ba93346a9c40fadb";
    const PINNED_CHESS_RESULTS: &str =
        "d7ff3ba470ef00769cd2ae78e378c985035aabc0dda6e3def4e686390cefda72";
    const PINNED_CHESS_DAY: &str =
        "28403c4d86b95e2d6f02c2a0f827472c040b3dc82d76ef874fc593d32525025d";
}
