//! The verifier modules the platform ships, as bytes.
//!
//! Each is built from source in `adapters/` by `scripts/build-adapters.sh` and committed here so the
//! service can install it with `ultranet-games adapter-register --module`. The service never trusts these
//! bytes because they are in the repository: it recomputes the SHA3-256 of whatever it is given, stores
//! it under that hash, and plays only against the hash an operator approved. A test pins each hash, so a
//! changed module cannot slip in unnoticed.

/// UltraGrid's verifier: regenerates the day's puzzle from the chain, the date and the anchor root, then
/// replays the answer on it. Source: `adapters/grid-verifier`, which includes `src/ultragrid/mod.rs`.
pub const GRID_VERIFIER: &[u8] = include_bytes!("adapters/grid-verifier.wasm");

/// The ABI version each module declares, passed to `--adapter-version`.
pub const GRID_ADAPTER_VERSION: u32 = 1;
