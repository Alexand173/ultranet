#!/usr/bin/env bash
#
# build-adapters.sh — rebuild the verifier modules the Games service ships, from source.
#
# The service validates a module against a narrow instruction set before it runs it and refuses
# anything outside it (src/games/adapter.rs). The wasm feature flags that satisfy that validator are
# pinned in adapters/grid-verifier/.cargo/config.toml, not in this script, so there is one answer to
# "how is the module built".
#
# The bytes committed at src/games/adapters/<name>.wasm are what the service installs, and their
# SHA3-256 is pinned by a test in src/games/tests.rs. So this script copies the build output and
# prints the hash the test must carry; it does not edit that pin for you.
#
# Usage
#   ./scripts/build-adapters.sh            # build and copy the modules
#   ./scripts/build-adapters.sh --verify   # ... then prove the service accepts them (slow: builds the node)
#
# Requirements: the pinned toolchain (rust-toolchain.toml) and the wasm32 target:
#   rustup target add wasm32-unknown-unknown

set -Eeuo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

VERIFY=0
for arg in "$@"; do
    case "$arg" in
        --verify) VERIFY=1 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

say() { printf '%s\n' "$*"; }
step() { printf '\n── %s\n' "$*"; }

TARGET=wasm32-unknown-unknown
OUT=$ROOT/src/games/adapters

rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
    || { echo "the $TARGET target is not installed for this toolchain; run: rustup target add $TARGET" >&2; exit 1; }

# ── path normalisation ──────────────────────────────────────────────────────────────────────────
#
# A panic location embeds the absolute path of the file it came from, and three such strings in this
# module come from crates.io sources under $CARGO_HOME. Without remapping, the same source built by
# two people produces two different modules — which is exactly what `Adapter reproducibility` caught
# on 2026-11-26: 49de66d6… built here, 4436023d… built on the runner, from identical source.
#
# Each builder's own paths are mapped to fixed synthetic ones, so the bytes stop depending on where
# the build happened. Nothing here names a user or a directory: CARGO_HOME and the workspace root are
# discovered on the machine doing the building, and the destinations are constants.
#
# Order matters, and not in the obvious direction: rustc applies the **last** matching prefix, which
# was established by building both orders and reading the strings out of the module (2026-11-26). So
# the broad cargo-home mapping goes first as the catch-all, and the per-registry mapping last, so a
# crate source is reported as /cargo/registry/src/<crate>/… with Cargo's index directory name
# (index.crates.io-<hash>) gone — that name is a Cargo internal and is not something to depend on.
CANONICAL_CARGO_HOME=/cargo
CANONICAL_REGISTRY_SRC=/cargo/registry/src
CANONICAL_WORKSPACE=/workspace
CARGO_HOME_DIR=${CARGO_HOME:-$HOME/.cargo}

# Populate the registry before discovering anything. The per-registry mapping below can only be read
# off the disk once the sources are unpacked, and a fresh CI runner starts with an empty cargo home —
# so without this the mapping set would depend on whether the machine had built before, and the same
# source would produce different bytes on a cold runner than on a warm workstation. Fetching first
# makes the mapping set identical on both; it also fails early, and honestly, if the lockfile cannot
# be satisfied.
( cd "$ROOT/adapters/grid-verifier" && cargo fetch --locked --target "$TARGET" )
# A cargo home or checkout that is reached through a symlink has two spellings, and rustc records
# whichever one it was given. Both are mapped, so a symlinked cache directory (common on CI images,
# Nix, and anywhere CARGO_HOME is a link) cannot smuggle the real path into the module.
CARGO_HOME_REAL=$(cd "$CARGO_HOME_DIR" 2>/dev/null && pwd -P) || CARGO_HOME_REAL=$CARGO_HOME_DIR
ROOT_REAL=$(cd "$ROOT" && pwd -P)

# Written as `if` rather than `[ … ] && …` on purpose: under `set -e` those read as an error one day
# away, and this script must not depend on the subtle rule about which command in an && list is
# allowed to fail.
BUILDER_PREFIXES=()
if [ "$CARGO_HOME_REAL" != "$CARGO_HOME_DIR" ]; then
    BUILDER_PREFIXES+=("$CARGO_HOME_REAL")
fi
BUILDER_PREFIXES+=("$CARGO_HOME_DIR")
if [ "$ROOT_REAL" != "$ROOT" ]; then
    BUILDER_PREFIXES+=("$ROOT_REAL")
fi
BUILDER_PREFIXES+=("$ROOT")

REMAP_PAIRS=()
for prefix in "${BUILDER_PREFIXES[@]}"; do
    case "$prefix" in
        "$ROOT"|"$ROOT_REAL") REMAP_PAIRS+=("$prefix=$CANONICAL_WORKSPACE") ;;
        *) REMAP_PAIRS+=("$prefix=$CANONICAL_CARGO_HOME") ;;
    esac
done
# Both spellings of the cargo home may hold a registry, and a glob that matches nothing must simply
# contribute nothing.
for index in "$CARGO_HOME_DIR"/registry/src/*/ "$CARGO_HOME_REAL"/registry/src/*/; do
    if [ -d "$index" ]; then
        REMAP_PAIRS+=("${index%/}=$CANONICAL_REGISTRY_SRC")
    fi
done

# `RUSTFLAGS` would replace the target's flags rather than add to them, which would drop the features
# and link arguments in adapters/grid-verifier/.cargo/config.toml. A wrapper passes them alongside.
RUSTC_WRAPPER=$ROOT/scripts/rustc-remap-paths.sh
[ -x "$RUSTC_WRAPPER" ] || { echo "the rustc wrapper is missing or not executable: $RUSTC_WRAPPER" >&2; exit 1; }
ADAPTER_REMAP_PAIRS=$(printf '%s\n' "${REMAP_PAIRS[@]}")
export RUSTC_WRAPPER ADAPTER_REMAP_PAIRS

step "path normalisation"
printf '%s\n' "${REMAP_PAIRS[@]}" | sed 's/^/  /'

step "grid-verifier"
(
    cd "$ROOT/adapters/grid-verifier"
    # Cold by construction. Cargo does not invalidate cached units when the rustc wrapper changes, so
    # an incremental build can reuse dependency artifacts compiled *before* the remapping existed and
    # silently produce a different module than a fresh checkout would. That is the same class of bug
    # this script exists to prevent, so the cache is cleared rather than trusted — the crate's nine
    # units cost a couple of seconds.
    rm -rf target
    cargo build --release --locked --target "$TARGET"
)
BUILT=$ROOT/adapters/grid-verifier/target/$TARGET/release/grid_verifier.wasm
[ -f "$BUILT" ] || { echo "build produced no module at $BUILT" >&2; exit 1; }

# Checked *before* it is copied into the repository: a build that still carries a builder path must
# not be able to overwrite the committed module on its way to being refused.
python3 - "$BUILT" "$OUT/grid-verifier.wasm" "${BUILDER_PREFIXES[@]}" <<'PY'
import hashlib
import sys

built, destination = sys.argv[1], sys.argv[2]
builder_prefixes = sys.argv[3:]
data = open(built, "rb").read()
print(f"  module      : {destination}")
print(f"  bytes       : {len(data)}")
print(f"  sha3-256    : {hashlib.sha3_256(data).hexdigest()}")

# A path that leaked into the module is a path that will differ on the next machine to build it, so
# this is checked rather than assumed: a new dependency that panics from somewhere unremapped fails
# the build here instead of failing the reproducibility job days later.
leaked = sorted({prefix for prefix in builder_prefixes if prefix.encode() in data})
if leaked:
    print("  REFUSED     : machine-specific paths survived remapping: " + ", ".join(leaked), file=sys.stderr)
    sys.exit(1)
print("  path check  : no builder paths in the module")
PY

mkdir -p "$OUT"
cp "$BUILT" "$OUT/grid-verifier.wasm"
chmod 0644 "$OUT/grid-verifier.wasm"

if [ "$VERIFY" -eq 1 ]; then
    step "the service must accept the module it ships"
    cargo test --locked -j 1 --lib the_shipped_grid_verifier -- --test-threads=1
fi

say ""
say "Pin the sha3-256 above in src/games/tests.rs (the_shipped_grid_verifier_is_a_module_the_runtime_accepts)"
say "if it changed — a module whose bytes changed for an unchanged reason is a bug, not a rebuild."
