#!/usr/bin/env bash
#
# resolve-changed-files.sh — establish, validate and diff the comparison range for a CI run.
#
# Writes changed-files.txt and nothing else. This is the only place that decides "what changed", which
# makes it the one place that must never guess: every failure below exits non-zero, because an
# infrastructure or git error read as "no files changed" would let a Windows run report green having
# tested nothing at all.
#
# Contract
#   env   : EVENT           github.event_name
#           FULL_INPUT      workflow_dispatch input full_suite ("true" / "false")
#           EVENT_BASE_SHA  push: github.event.before        pull_request: pull_request.base.sha
#                           (may be empty, or all-zero for a new ref)
#           EVENT_HEAD_SHA  push: github.sha                 pull_request: pull_request.head.sha
#           HEAD_SHA        the commit that was checked out
#           DEFAULT_BRANCH  the repository's default branch (used in diagnostics only)
#
#   out   : changed-files.txt  one path per line; legitimately empty when a *successful* diff shows the
#                              range introduces no changes
#           stdout             one line naming the range that was used
#
# Per-event rules, each explicit:
#   pull_request      base = pull_request.base.sha, head = pull_request.head.sha. Both must resolve to
#                     commits. The PR's own change set, not the synthetic merge result.
#   push              head = github.sha. `before` is the previous tip, or all-zero for a brand-new ref —
#                     which is a fact about the ref, not an error. An all-zero `before` has no earlier
#                     commit, so the honest base is the empty tree: every path is new. That can only
#                     select more tests, never fewer.
#   workflow_dispatch, full_suite=true
#                     The complete suite runs by definition, so change detection is not consulted at all
#                     and cannot fail the run.
#   workflow_dispatch, full_suite=false
#                     head = the checked-out commit, base = its first parent: the change someone asked to
#                     tier-test. No parent means the request has no base, which is reported rather than
#                     assumed; the answer to that is full_suite=true.
#   anything else     No defined comparison base, so the run fails instead of inventing one.
set -euo pipefail

# The well-known empty tree. `git diff` against it lists every path in the head commit as added.
EMPTY_TREE=4b825dc642cb6eb9a060e54bf8d69288fbee4904

EVENT=${EVENT:?EVENT is required}
FULL_INPUT=${FULL_INPUT:-false}

die() {
    echo "::error title=Windows change detection::$*" >&2
    exit 1
}

# Never leave a previous run's file in place for a later step to read.
rm -f changed-files.txt

# A full-suite run proves everything by definition. It must not be possible for change detection to
# fail — or to be believed — on the way to it.
if [ "$EVENT" = "workflow_dispatch" ] && [ "$FULL_INPUT" = "true" ]; then
    : > changed-files.txt
    echo "full: the complete serial suite runs; change detection is not used"
    exit 0
fi

is_zero_or_empty() {
    [ -z "$1" ] || [[ "$1" =~ ^0+$ ]]
}

# resolve_commit <ref> <role> — echoes the commit id, or fails naming what could not be resolved.
# `^{commit}` is deliberate: it rejects a tree, a blob, a tag pointing at a blob, and any abbreviated
# reference that does not name a commit, which a bare `rev-parse --verify` would accept.
resolve_commit() {
    local ref=$1 role=$2 sha
    [ -n "$ref" ] || die "no $role to compare against"
    if ! sha=$(git rev-parse --verify --quiet "${ref}^{commit}"); then
        die "$role ${ref} is not a commit in this clone (missing, or not a commit object)"
    fi
    [ -n "$sha" ] || die "$role ${ref} did not resolve to a commit"
    printf '%s' "$sha"
}

case "$EVENT" in
    pull_request)
        base=$(resolve_commit "${EVENT_BASE_SHA:-}" "pull-request base")
        head=$(resolve_commit "${EVENT_HEAD_SHA:-}" "pull-request head")
        ;;
    push)
        head=$(resolve_commit "${EVENT_HEAD_SHA:-}" "pushed commit")
        if is_zero_or_empty "${EVENT_BASE_SHA:-}"; then
            # A new ref reports an all-zero `before`. There is no earlier commit to diff against, and
            # saying so is a fact about the ref, not a swallowed error. The empty tree gives every path
            # in the pushed commit, so the tier can only grow.
            base=$EMPTY_TREE
            # git needs the empty tree to exist as an object before it can diff against it.
            git cat-file -e "$EMPTY_TREE" 2>/dev/null || printf '' | git mktree >/dev/null
            printf 'note: %s reported an all-zero before; treating every path as new\n' \
                "${DEFAULT_BRANCH:-the branch}" >&2
        else
            base=$(resolve_commit "$EVENT_BASE_SHA" "previous pushed commit")
        fi
        ;;
    workflow_dispatch)
        head=$(resolve_commit "${HEAD_SHA:-}" "checked-out commit")
        if ! base=$(git rev-parse --verify --quiet "${head}~1^{commit}"); then
            die "no comparison base for ${head:0:7}: it has no parent commit, so there is no change to tier-test; dispatch with full_suite=true"
        fi
        ;;
    *)
        die "unsupported event '${EVENT}': this workflow defines no comparison base for it"
        ;;
esac

# The diff itself. No fallback: if this fails, the run fails. A failure here means git could not read
# the range, which is not the same thing as the range being empty.
#
# Staged through a temporary file, because the redirection would otherwise create an empty
# changed-files.txt the moment git fails — the exact "looks like no changes" artefact this script exists
# to prevent. The plan file appears only once the diff has completed successfully.
staged=changed-files.txt.tmp
rm -f "$staged"
if ! git diff --name-only "$base" "$head" > "$staged"; then
    rm -f "$staged"
    die "git diff --name-only ${base:0:7} ${head:0:7} failed; refusing to treat it as an empty change set"
fi
mv -f "$staged" changed-files.txt

files=$(grep -c . changed-files.txt || true)
printf 'base=%s head=%s files=%s\n' "${base:0:7}" "${head:0:7}" "${files:-0}"
