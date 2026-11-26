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

step "grid-verifier"
(
    cd "$ROOT/adapters/grid-verifier"
    cargo build --release --locked --target "$TARGET"
)
BUILT=$ROOT/adapters/grid-verifier/target/$TARGET/release/grid_verifier.wasm
[ -f "$BUILT" ] || { echo "build produced no module at $BUILT" >&2; exit 1; }

mkdir -p "$OUT"
cp "$BUILT" "$OUT/grid-verifier.wasm"
chmod 0644 "$OUT/grid-verifier.wasm"

say "module      : src/games/adapters/grid-verifier.wasm"
say "bytes       : $(wc -c <"$OUT/grid-verifier.wasm")"
say "sha3-256    : $(python3 - "$OUT/grid-verifier.wasm" <<'PY'
import hashlib, sys
print(hashlib.sha3_256(open(sys.argv[1], 'rb').read()).hexdigest())
PY
)"

if [ "$VERIFY" -eq 1 ]; then
    step "the service must accept the module it ships"
    cargo test --locked -j 1 --lib the_shipped_grid_verifier -- --test-threads=1
fi

say ""
say "Pin the sha3-256 above in src/games/tests.rs (the_shipped_grid_verifier_is_a_module_the_runtime_accepts)"
say "if it changed — a module whose bytes changed for an unchanged reason is a bug, not a rebuild."
