#!/usr/bin/env bash
#
# test-windows-change-detection.sh — exercise every way the Windows change detection can be asked to
# run, including the ways it must refuse.
#
# The property under test is narrow and worth restating, because it is what the fail-closed fix is for:
# a run whose diff could not be established must FAIL. It must never continue as though the change set
# were empty, because an empty change set legitimately selects MODE=none — a green job that ran nothing.
#
# So each failing case asserts two things: the resolver exits non-zero, and it left no changed-files.txt
# behind for a later step to read. Each passing case asserts the mode the planner then produces, since a
# correct range with a wrong plan is just as broken.
#
#   usage: bash scripts/ci/test-windows-change-detection.sh
set -uo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
RESOLVER="$ROOT/scripts/ci/resolve-changed-files.sh"
PLANNER="$ROOT/scripts/ci/windows-test-plan.sh"
REAL_GIT=$(command -v git)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
repo="$work/repo"
mkdir -p "$repo"

pass=0
fail=0

ok()   { printf '  \033[32mPASS\033[0m  %s\n' "$1"; pass=$((pass + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }

# --- fixture: a repository with a docs-only commit and a Rust commit on top ------------------------
init_repo() {
    cd "$repo" || exit 1
    git init -q -b main .
    git config user.email ci@example.invalid
    git config user.name "CI test"
    git config commit.gpgsign false

    mkdir -p src docs tests
    printf '# base\n' > README.md
    printf 'fn base() {}\n' > src/lib.rs
    git add -A && git commit -qm "base"
    C_BASE=$(git rev-parse HEAD)

    printf 'note\n' > docs/note.md
    git add -A && git commit -qm "docs only"
    C_DOCS=$(git rev-parse HEAD)

    printf 'fn api() {}\n' > src/api.rs
    git add -A && git commit -qm "rust change"
    C_RUST=$(git rev-parse HEAD)

    C_TREE=$(git rev-parse 'HEAD^{tree}')
    export C_BASE C_DOCS C_RUST C_TREE
}

# --- helpers --------------------------------------------------------------------------------------

# mode_for <env assignments...> — run the planner on the file the resolver just wrote.
mode_for() {
    local m
    m=$(env "$@" bash "$PLANNER" < changed-files.txt 2>/dev/null | sed -n 's/^MODE=//p')
    printf '%s' "$m"
}

# expect_success <name> <expected mode> <env assignments...>
expect_success() {
    local name=$1 want=$2
    shift 2
    rm -f changed-files.txt
    local out status mode
    out=$(env "$@" bash "$RESOLVER" 2>&1)
    status=$?
    if [ "$status" -ne 0 ]; then
        bad "$name: expected exit 0, got $status — $out"
        return
    fi
    mode=$(mode_for "$@")
    if [ "$mode" != "$want" ]; then
        bad "$name: expected MODE=$want, got '${mode}' (range: $(printf '%s' "$out" | tail -1))"
        return
    fi
    ok "$name → MODE=$want ($(printf '%s' "$out" | tail -1))"
}

# expect_failure <name> <env assignments...>
expect_failure() {
    local name=$1
    shift
    rm -f changed-files.txt
    local out status
    out=$(env "$@" bash "$RESOLVER" 2>&1)
    status=$?
    if [ "$status" -eq 0 ]; then
        bad "$name: expected a non-zero exit, got 0 — it reported success: $(printf '%s' "$out" | tail -1)"
        return
    fi
    if [ -e changed-files.txt ]; then
        bad "$name: failed as expected but left changed-files.txt behind for the planner to read"
        return
    fi
    ok "$name → exit $status, no plan file ($(printf '%s' "$out" | grep -o 'Windows change detection::.*' | head -1))"
}

# --- run ------------------------------------------------------------------------------------------

init_repo || exit 1
echo
echo "Windows change detection — fail-closed behaviour"
echo "  repo fixture: $C_BASE (base) → $C_DOCS (docs) → $C_RUST (rust)"
echo

echo "Legitimate change sets:"
expect_success "pull_request, Rust change"          runtime \
    EVENT=pull_request EVENT_BASE_SHA="$C_DOCS" EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"
expect_success "pull_request, documentation only"   none \
    EVENT=pull_request EVENT_BASE_SHA="$C_BASE" EVENT_HEAD_SHA="$C_DOCS" HEAD_SHA="$C_DOCS"
expect_success "genuinely empty diff"               none \
    EVENT=pull_request EVENT_BASE_SHA="$C_RUST" EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"
expect_success "push, all-zero before (new ref)"    runtime \
    EVENT=push EVENT_BASE_SHA="0000000000000000000000000000000000000000" \
    EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST" DEFAULT_BRANCH=main
expect_success "dispatch, full_suite=true"          full \
    EVENT=workflow_dispatch FULL_INPUT=true EVENT_BASE_SHA="$C_BASE" EVENT_HEAD_SHA="$C_RUST"
expect_success "dispatch, full_suite=false"         runtime \
    EVENT=workflow_dispatch FULL_INPUT=false HEAD_SHA="$C_RUST"

echo
echo "Ranges that cannot be established (must fail, never MODE=none):"
expect_failure "invalid base SHA" \
    EVENT=pull_request EVENT_BASE_SHA="deadbeefdeadbeefdeadbeefdeadbeefdeadbeef" \
    EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"
expect_failure "invalid HEAD SHA" \
    EVENT=pull_request EVENT_BASE_SHA="$C_BASE" \
    EVENT_HEAD_SHA="deadbeefdeadbeefdeadbeefdeadbeefdeadbeef" HEAD_SHA="$C_RUST"
expect_failure "missing base commit (a tree, not a commit)" \
    EVENT=pull_request EVENT_BASE_SHA="$C_TREE" EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"
expect_failure "missing base commit (no ref given)" \
    EVENT=pull_request EVENT_BASE_SHA="" EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"
expect_failure "dispatch, full_suite=false, no parent commit" \
    EVENT=workflow_dispatch FULL_INPUT=false HEAD_SHA="$C_BASE"
expect_failure "unsupported event" \
    EVENT=schedule HEAD_SHA="$C_RUST"

# A real `git diff` failure, injected through PATH: every git command delegates to the real binary
# except `diff`, which exits 128 the way git does when it cannot read the range.
mkdir -p "$work/stub"
cat > "$work/stub/git" <<STUB
#!/usr/bin/env bash
if [ "\${1:-}" = "diff" ]; then
    echo "fatal: unable to read tree (injected failure)" >&2
    exit 128
fi
exec "$REAL_GIT" "\$@"
STUB
chmod +x "$work/stub/git"
expect_failure "git diff failure" \
    PATH="$work/stub:$PATH" EVENT=pull_request EVENT_BASE_SHA="$C_BASE" \
    EVENT_HEAD_SHA="$C_RUST" HEAD_SHA="$C_RUST"

# The full-suite dispatch must reach MODE=full even when every git input is nonsense, because it is not
# allowed to depend on change detection at all.
expect_success "dispatch, full_suite=true, with unusable refs" full \
    PATH="$work/stub:$PATH" EVENT=workflow_dispatch FULL_INPUT=true \
    EVENT_BASE_SHA="notasha" EVENT_HEAD_SHA="notasha"

echo
printf '  %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
