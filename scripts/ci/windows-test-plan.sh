#!/usr/bin/env bash
#
# windows-test-plan.sh — decide what a Windows CI run must prove, without running anything.
#
# Contract
#   stdin : changed file paths, one per line (from `git diff --name-only <base> <head>`)
#   env   : EVENT (github.event_name), FULL_INPUT (workflow_dispatch full_suite input)
#   stdout: key=value header, then one directive per line, in the order they must run
#
#     MODE=none|runtime|full      none  = nothing to prove on Windows
#     ADAPTER=0|1                 the wasm module or its build changed
#     COMPILE_ALL=0|1             compile every Windows test target before running anything
#     COMPILE                     directive: `cargo test --all-targets --no-run`
#     TEST LIB <filter>           directive: `cargo test --lib -- <filter> --test-threads=1`
#     TEST BIN <name>             directive: `cargo test --test <name> -- --test-threads=1`
#
# It is pure: no git, no cargo, no network. That is the point — it can be run against any change set,
# including a historical commit or a hypothetical one, to see exactly what CI would run.
#
# Every filter below was chosen by reading the repository, not by guessing from file paths: the lib
# filters were taken from `cargo test --lib -- --list`, and each one is asserted by the caller to match
# at least one test, so a rename that empties a filter turns the job red instead of green.
#
# What is deliberately *not* here is documented in .github/workflows/windows-tests.yml: the suites that
# build a blockchain per test (games::tests, appchain_authorization_test, integration_test, the two
# real-node payout binaries) cost ~120 s per test on this runner and belong to the full gate.
set -euo pipefail

event=${EVENT:-}
full=${FULL_INPUT:-false}

rust=0 adapter=0 appchain=0 games=0 payouts=0 api=0 faucet=0 dag=0 core=0

while IFS= read -r f; do
    [ -n "$f" ] || continue
    case "$f" in
        src/*|tests/*|adapters/*|Cargo.toml|Cargo.lock|rust-toolchain.toml|.cargo/*) rust=1 ;;
    esac
    case "$f" in
        # src/games/adapter.rs is the validator itself; src/games/adapters* is the module and its loader.
        adapters/*|scripts/*|src/ultragrid/*|src/games/adapter.rs|src/games/adapters*) adapter=1 ;;
    esac
    case "$f" in
        src/appchain/*|tests/appchain_*|src/bin/appchain-cli.rs) appchain=1 ;;
    esac
    case "$f" in
        src/games/*|tests/games_*|src/bin/ultranet-games.rs) games=1 ;;
    esac
    case "$f" in
        src/games/pools.rs|src/games/pool_payouts.rs|src/games/rules.rs|src/games/service.rs|src/games/play.rs|src/games/api.rs|tests/games_*) payouts=1 ;;
    esac
    case "$f" in
        src/api.rs|tests/api_json_errors_test.rs) api=1 ;;
    esac
    case "$f" in
        src/faucet/*|tests/faucet_*) faucet=1 ;;
    esac
    case "$f" in
        src/dag_mysticeti.rs|tests/dag_*) dag=1 ;;
    esac
    # Shared infrastructure: everything downstream can break, so every family runs. src/api.rs is on
    # this list because the AppChain, faucet and games routes all live in it — a change there is not an
    # "api change", it is a change to every route. A src/bin/* tool is deliberately *not* here: a binary
    # depends on the library, never the other way round, so its own tests belong to its own family.
    case "$f" in
        src/lib.rs|src/api.rs|src/storage.rs|src/shared_storage.rs|src/block_stm.rs|src/async_validator.rs|src/p2p.rs|src/main.rs|src/platform.rs|src/runtime_config.rs|tests/integration_test.rs|Cargo.toml|Cargo.lock|rust-toolchain.toml) core=1 ;;
    esac
done

if [ "$event" = "workflow_dispatch" ] && [ "$full" = "true" ]; then
    printf 'MODE=full\nADAPTER=%s\nCOMPILE_ALL=0\n' "$adapter"
    exit 0
fi

if [ "$rust" -eq 0 ]; then
    printf 'MODE=none\nADAPTER=%s\nCOMPILE_ALL=0\n' "$adapter"
    exit 0
fi

# A change to shared infrastructure is a change to every family's behaviour.
if [ "$core" -eq 1 ]; then
    appchain=1; games=1; payouts=1; api=1; faucet=1; dag=1
fi

printf 'MODE=runtime\nADAPTER=%s\nCOMPILE_ALL=1\n' "$adapter"
echo "COMPILE"

# The module the platform ships, judged by the real runtime. Cheap, and it is the one test that
# proves the committed bytes are loadable — it runs whether or not the adapter changed.
echo "TEST LIB games::tests::shipped_grid_verifier"

if [ "$adapter" -eq 1 ] || [ "$games" -eq 1 ]; then
    # Adapter validator, bulk memory and the per-byte fuel metering (24 tests, sub-second).
    echo "TEST LIB games::adapter"
    # The puzzle generator the verifier and the node share (15 tests).
    echo "TEST LIB ultragrid::tests"
fi

if [ "$games" -eq 1 ] || [ "$payouts" -eq 1 ]; then
    # Reward policy, limits and who may be paid (16 tests). The pool accounting itself lives in
    # games::tests::reward_pools — 43 tests that each build a chain — and stays in the full gate.
    echo "TEST LIB games::rules"
    # The routes the app and the website read, as far as they can be exercised without a chain.
    echo "TEST LIB games::api"
fi

if [ "$appchain" -eq 1 ]; then
    # Wire format is pure and fast; the API and lifecycle binaries build one chain each (~120 s each),
    # which is the affordable end-to-end proof. All 11 authorization tests stay in the full gate.
    echo "TEST BIN appchain_wire_format_test"
    echo "TEST LIB appchain::ledger"
    echo "TEST BIN appchain_api_test"
    echo "TEST BIN appchain_e2e_lifecycle_test"
fi

if [ "$api" -eq 1 ]; then
    # The JSON error envelope every route must answer with.
    echo "TEST BIN api_json_errors_test"
fi

if [ "$faucet" -eq 1 ]; then
    echo "TEST LIB faucet::"
    echo "TEST BIN faucet_api_test"
    echo "TEST BIN faucet_service_test"
fi

if [ "$dag" -eq 1 ]; then
    echo "TEST LIB dag_mysticeti::tests"
    echo "TEST BIN dag_stress_test"
fi
