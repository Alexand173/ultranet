//! The games service: checks answers, records receipts, pays rewards, and anchors each game's day.
//!
//! Two games share it — UltraGrid (answers are replayed and verified) and Ultra Chess (results are
//! reported by the phone and cannot be verified; the receipts say so). See
//! `docs/GAMES_RULES_AND_PAYOUTS.md` for the rules and why each exists.

pub mod anchor;
pub mod api;
pub mod chain;
pub mod fence;
pub mod config;
pub mod ownership;
pub mod peer;
pub mod peer_usage;
pub mod records;
pub mod records_api;
pub mod records_config;
pub mod registry;
pub mod registry_anchor;
pub mod roots;
pub mod rules;
pub mod service;
pub mod store;
pub mod usage;

#[cfg(test)]
mod tests;
