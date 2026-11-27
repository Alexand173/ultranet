#!/usr/bin/env bash
#
# rustc-remap-paths.sh — a rustc wrapper that makes the adapter build path-independent.
#
# Why a wrapper and not `.cargo/config.toml`: the prefixes that need remapping are the *builder's*
# paths — `$CARGO_HOME` and the workspace root — and Cargo cannot interpolate environment variables
# into `rustflags`. `RUSTFLAGS` cannot be used either, because Cargo treats it as a *replacement* for
# the target's `rustflags`, not an addition: setting it would silently drop the target features and
# link arguments in `.cargo/config.toml`, which are what make the module load at all. `RUSTC_WRAPPER`
# sits beside those flags instead of replacing them, so this wrapper adds the remapping and touches
# nothing else.
#
# The pairs come from the caller in ADAPTER_REMAP_PAIRS, one `FROM=TO` per line, most specific last —
# rustc applies the last matching prefix, which scripts/build-adapters.sh establishes by construction.
# That script discovers the prefixes on the machine it runs on; nothing here names a user, a home
# directory or a checkout path.
#
# Cargo invokes a wrapper as `wrapper <rustc> <args...>`.

set -euo pipefail

rustc_binary=$1
shift

remaps=()
if [ -n "${ADAPTER_REMAP_PAIRS:-}" ]; then
    while IFS= read -r pair; do
        [ -n "$pair" ] || continue
        remaps+=("--remap-path-prefix=$pair")
    done <<<"$ADAPTER_REMAP_PAIRS"
fi

if [ "${#remaps[@]}" -eq 0 ]; then
    exec "$rustc_binary" "$@"
fi
exec "$rustc_binary" "$@" "${remaps[@]}"
