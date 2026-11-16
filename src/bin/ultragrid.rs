//! `ultragrid` — look at a day's puzzles and check an answer, offline.
//!
//! ```text
//! ultragrid day    --date 2026-11-16 [--chain-id 1] [--prev-root <64 hex>]
//! ultragrid verify --date 2026-11-16 --level 3 --answer "7:1,12:2" [--chain-id 1] [--prev-root <64 hex>]
//! ultragrid reference --date 2026-11-16 --level 3 [--chain-id 1] [--prev-root <64 hex>]
//! ```
//!
//! An answer is `cell:turns` pairs, comma separated: turn tile `cell` clockwise `turns` times. Cells
//! count from 0, left to right, top to bottom. Nothing here touches the network or a key.

use std::env;
use std::process::exit;

use UltraNet::ultragrid::{
    daily_seed, daily_set, generate, set_commitment, verify, Turn, LEVELS_PER_DAY,
};

fn usage() -> ! {
    eprintln!(
        "usage:\n  ultragrid day --date YYYY-MM-DD [--chain-id N] [--prev-root HEX]\n  \
         ultragrid verify --date YYYY-MM-DD --level N --answer \"cell:turns,...\" [--chain-id N] [--prev-root HEX]\n  \
         ultragrid reference --date YYYY-MM-DD --level N [--chain-id N] [--prev-root HEX]"
    );
    exit(2)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn seed_from(args: &[String]) -> [u8; 32] {
    let date = flag(args, "--date").unwrap_or_else(|| usage());
    let chain_id: u32 = flag(args, "--chain-id")
        .map(|value| value.parse().unwrap_or_else(|_| usage()))
        .unwrap_or(1);
    let previous_root = match flag(args, "--prev-root") {
        None => [0u8; 32],
        Some(hex_root) => hex::decode(hex_root.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .unwrap_or_else(|| {
                eprintln!("--prev-root must be 64 hexadecimal characters");
                exit(2)
            }),
    };
    daily_seed(chain_id, &date, &previous_root).unwrap_or_else(|| {
        eprintln!("--date must look like 2026-11-16");
        exit(2)
    })
}

fn level_from(args: &[String]) -> u8 {
    let level: u8 = flag(args, "--level")
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| usage());
    if level as usize >= LEVELS_PER_DAY {
        eprintln!("--level must be 0 to {}", LEVELS_PER_DAY - 1);
        exit(2)
    }
    level
}

fn parse_answer(text: &str) -> Vec<Turn> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    text.split(',')
        .map(|pair| {
            let (cell, turns) = pair.trim().split_once(':').unwrap_or_else(|| {
                eprintln!("answer entries look like cell:turns, got \"{pair}\"");
                exit(2)
            });
            Turn {
                cell: cell.trim().parse().unwrap_or_else(|_| usage()),
                turns: turns.trim().parse().unwrap_or_else(|_| usage()),
            }
        })
        .collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("day") => {
            let seed = seed_from(&args);
            let set = daily_set(&seed);
            println!("seed        {}", hex::encode(seed));
            println!("commitment  {}", hex::encode(set_commitment(&set)));
            for puzzle in &set {
                println!(
                    "\nlevel {}  {}x{}  par {}  limit {}  id {}",
                    puzzle.level,
                    puzzle.width,
                    puzzle.height,
                    puzzle.par,
                    puzzle.move_limit(),
                    hex::encode(&puzzle.id()[..8])
                );
                println!("{}", puzzle.render());
            }
        }
        Some("verify") => {
            let seed = seed_from(&args);
            let level = level_from(&args);
            let answer = parse_answer(&flag(&args, "--answer").unwrap_or_default());
            let puzzle = generate(&seed, level).puzzle;
            match verify(&puzzle, &answer) {
                Ok(outcome) => {
                    println!(
                        "moves {}  connected {}  par {}  limit {}  win {}",
                        outcome.moves, outcome.connected, outcome.par, outcome.limit, outcome.win
                    );
                    exit(if outcome.win { 0 } else { 1 });
                }
                Err(error) => {
                    eprintln!("refused: {error:?}");
                    exit(2)
                }
            }
        }
        Some("reference") => {
            let seed = seed_from(&args);
            let level = level_from(&args);
            let generated = generate(&seed, level);
            let text: Vec<String> = generated
                .reference
                .iter()
                .map(|turn| format!("{}:{}", turn.cell, turn.turns))
                .collect();
            println!("{}", text.join(","));
        }
        _ => usage(),
    }
}
