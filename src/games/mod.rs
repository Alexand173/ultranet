//! The games service: checks answers, records receipts, pays rewards, and anchors each game's day.
//!
//! Two games share it — UltraGrid (answers are replayed and verified) and Ultra Chess (results are
//! reported by the phone and cannot be verified; the receipts say so). See
//! `docs/GAMES_RULES_AND_PAYOUTS.md` for the rules and why each exists.

pub mod anchor;
pub mod api;
pub mod chain;
pub mod config;
pub mod ownership;
pub mod registry;
pub mod roots;
pub mod rules;
pub mod service;
pub mod store;

#[cfg(test)]
mod tests;
