#!/usr/bin/env bash
set -Eeuo pipefail

readonly DEPLOY_USER="ultranet-deploy"
readonly SERVICE_USER="ultranet"
readonly STAGING_ROOT="/var/lib/ultranet-deploy/staging"
readonly RELEASES_ROOT="/opt/ultranet/releases"
readonly ACTIVE_WEBSITE="/opt/ultranet/website"
# Defined with the other paths so the prune-only modes can protect it before
# a commit SHA exists.
readonly PREVIOUS_WEBSITE="$RELEASES_ROOT/previous"
readonly SHARED_SOURCE="/opt/ultranet/ULTRA_NET_TECHNICAL_GUIDE.md"
readonly DASHBOARD_SERVICE="ultranet-dashboard.service"
readonly VALIDATOR_SERVICE="ultranet.service"

# Retention. Every deploy writes a ~918 MB directory under $RELEASES_ROOT and
# nothing used to delete them: 66 of them reached 31 GB and took the host to 95%
# disk on 2026-10-31. Defaults keep a usable rollback and a failure trail.
readonly KEEP_PREVIOUS="${ULTRANET_DEPLOY_KEEP_PREVIOUS:-2}"
readonly KEEP_FAILED="${ULTRANET_DEPLOY_KEEP_FAILED:-3}"
readonly KEEP_COMMIT_RELEASES="${ULTRANET_DEPLOY_KEEP_COMMIT_RELEASES:-3}"
readonly STAGE_ARCHIVE_DAYS="${ULTRANET_DEPLOY_STAGE_ARCHIVE_DAYS:-7}"
readonly PRUNE_REPORT="report"
readonly PRUNE_DELETE="delete"

log() {
  printf '[ultranet-dashboard-deploy] %s\n' "$*"
}

fail() {
  printf '[ultranet-dashboard-deploy] ERROR: %s\n' "$*" >&2
  exit 1
}

print_dashboard_logs() {
  journalctl -u "$DASHBOARD_SERVICE" -n 80 --no-pager >&2 || true
}

# Newest-first list of directories under $RELEASES_ROOT matching the predicates.
list_prune_candidates() {
  find "$RELEASES_ROOT" -mindepth 1 -maxdepth 1 -type d "$@" -printf '%T@ %p\n' 2>/dev/null \
    | sort -rn \
    | cut -d' ' -f2-
}

# Keeps the newest $1 candidates matching the remaining find predicates, then
# reports or removes the rest. Every victim is re-validated against the three
# name shapes this script creates, so a changed glob can never widen into a
# delete of something the deploy did not put there.
prune_selected() {
  local keep="$1" mode="$2"
  shift 2
  local -a victims=()
  mapfile -t victims < <(list_prune_candidates "$@" | awk -v keep="$keep" 'NR > keep')

  local victim
  for victim in "${victims[@]:-}"; do
    [[ -n "$victim" ]] || continue
    case "$victim" in
      "$RELEASES_ROOT"/previous-[0-9]*|"$RELEASES_ROOT"/failed-*|"$RELEASES_ROOT"/[0-9a-f][0-9a-f]*) ;;
      *) log "refusing to prune an unexpected path: $victim"; continue ;;
    esac
    [[ "$victim" != "$PREVIOUS_WEBSITE" ]] || continue
    [[ "$victim" != "$ACTIVE_WEBSITE" ]] || continue
    if [[ "$mode" == "$PRUNE_REPORT" ]]; then
      log "would prune: ${victim#"$RELEASES_ROOT"/}"
    else
      log "pruning: ${victim#"$RELEASES_ROOT"/}"
      rm -rf --one-file-system -- "$victim"
    fi
  done
}

# Crashed runs leave .staging-* work roots behind, and a failed deploy never
# reaches the success path that removes its staged archive.
prune_staging_leftovers() {
  local mode="$1"
  local -a leftovers=()
  mapfile -t leftovers < <(
    {
      find "$RELEASES_ROOT" -mindepth 1 -maxdepth 1 -type d -name '.staging-*' -mtime +0 -print 2>/dev/null
      find "$STAGING_ROOT" -mindepth 1 -maxdepth 1 -type f -name '*.tar.gz' -mtime +"$STAGE_ARCHIVE_DAYS" -print 2>/dev/null
    }
  )

  local item
  for item in "${leftovers[@]:-}"; do
    [[ -n "$item" ]] || continue
    if [[ "$mode" == "$PRUNE_REPORT" ]]; then
      log "would prune: $item"
    else
      log "pruning: $item"
      rm -rf --one-file-system -- "$item"
    fi
  done
}

prune_releases() {
  local mode="$1"
  [[ -d "$RELEASES_ROOT" ]] || return 0
  prune_selected "$KEEP_PREVIOUS" "$mode" -name 'previous-*'
  prune_selected "$KEEP_FAILED" "$mode" -name 'failed-*'
  prune_selected "$KEEP_COMMIT_RELEASES" "$mode" \
    -regextype posix-extended -regex '.*/[0-9a-f]{40}'
  prune_staging_leftovers "$mode"
}

if [[ "${EUID}" -ne 0 ]]; then
  fail "this helper must run as root"
fi

if [[ "$#" -ne 1 ]]; then
  fail "usage: $0 <40-character commit SHA> | --prune-only | --prune-dry-run"
fi

# Retention modes run without a candidate, so a host can reclaim space between
# deploys and an operator can review the exact deletions first.
case "$1" in
  --prune-only)
    prune_releases "$PRUNE_DELETE"
    exit 0
    ;;
  --prune-dry-run)
    prune_releases "$PRUNE_REPORT"
    exit 0
    ;;
esac

if [[ ! "$1" =~ ^[0-9a-f]{40}$ ]]; then
  fail "usage: $0 <40-character commit SHA> | --prune-only | --prune-dry-run"
fi

readonly COMMIT_SHA="$1"
readonly ARCHIVE_PATH="$STAGING_ROOT/${COMMIT_SHA}.tar.gz"
readonly WORK_ROOT="$RELEASES_ROOT/.staging-${COMMIT_SHA}-$$"
readonly RELEASE_ROOT="$RELEASES_ROOT/${COMMIT_SHA}"
readonly TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
readonly ARCHIVE_LOG="$WORK_ROOT/archive-members.txt"

[[ -f "$ARCHIVE_PATH" ]] || fail "staged archive is missing: $ARCHIVE_PATH"
[[ ! -e "$WORK_ROOT" ]] || fail "staging directory already exists: $WORK_ROOT"
[[ ! -e "$RELEASE_ROOT" ]] || fail "commit release already exists: $RELEASE_ROOT"
[[ -d "$ACTIVE_WEBSITE" ]] || fail "active website directory is missing: $ACTIVE_WEBSITE"

install -d -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0750 "$RELEASES_ROOT"
install -d -o root -g root -m 0750 "$WORK_ROOT"

cleanup_on_error() {
  local status=$?
  if [[ "$status" -ne 0 ]]; then
    printf '[ultranet-dashboard-deploy] deployment failed; active service was not intentionally left on the candidate\n' >&2
    print_dashboard_logs
  fi
  exit "$status"
}
trap cleanup_on_error EXIT

# Validate archive names before extraction. Only the website tree and the
# canonical whitepaper source may cross the deploy boundary.
tar -tzf "$ARCHIVE_PATH" >"$ARCHIVE_LOG"
while IFS= read -r member; do
  [[ -n "$member" ]] || continue
  [[ "$member" != /* && "$member" != ../* && "$member" != *"/../"* && "$member" != *"/.." ]] || fail "unsafe archive member: $member"
  case "$member" in
    website|website/*|ULTRA_NET_TECHNICAL_GUIDE.md) ;;
    *) fail "unexpected archive member: $member" ;;
  esac
done <"$ARCHIVE_LOG"

while IFS= read -r listing; do
  case "${listing:0:1}" in
    l|h) fail "symbolic or hard links are not accepted in deployment archives" ;;
  esac
done < <(tar -tvzf "$ARCHIVE_PATH")

tar --no-same-owner --no-same-permissions -xzf "$ARCHIVE_PATH" -C "$WORK_ROOT"
for required in \
  "$WORK_ROOT/ULTRA_NET_TECHNICAL_GUIDE.md" \
  "$WORK_ROOT/website/package.json" \
  "$WORK_ROOT/website/package-lock.json" \
  "$WORK_ROOT/website/scripts/generate-whitepaper.mjs"; do
  [[ -f "$required" ]] || fail "required deployment input is missing: $required"
done

chown -R "$SERVICE_USER:$SERVICE_USER" "$WORK_ROOT"

log "running server-side frontend verification for $COMMIT_SHA"
runuser -u "$SERVICE_USER" -- env \
  WORK_ROOT="$WORK_ROOT" \
  bash -s <<'BUILD'
set -Eeuo pipefail
cd "$WORK_ROOT/website"
if [[ -f /etc/ultranet/website.env ]]; then
  set -a
  . /etc/ultranet/website.env
  set +a
fi
export NODE_ENV=production
npm ci --include=dev
npm run lint
npx tsc --noEmit
npm run build
BUILD

readonly GENERATED_HTML="$WORK_ROOT/website/public/docs/ultranet-whitepaper.html"
[[ -s "$GENERATED_HTML" ]] || fail "whitepaper HTML was not generated"
grep -Fq "UltraNet v7.1 Sovereign Technical Guide" "$GENERATED_HTML" || fail "generated whitepaper title is missing"
grep -Fq "34_CHAPTERS" "$GENERATED_HTML" || fail "generated whitepaper chapter marker is missing"
grep -Fq '<div class="mermaid"' "$GENERATED_HTML" || fail "generated whitepaper diagrams are missing"
if grep -Fq "THIS PAGE COULD NOT BE FOUND" "$GENERATED_HTML"; then
  fail "generated whitepaper contains a Next.js 404 page"
fi

install -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0644 \
  "$WORK_ROOT/ULTRA_NET_TECHNICAL_GUIDE.md" "$SHARED_SOURCE"

validator_before="$(systemctl is-active "$VALIDATOR_SERVICE" 2>/dev/null || true)"
log "validator state before dashboard restart: ${validator_before:-unknown}"

if [[ -e "$PREVIOUS_WEBSITE" ]]; then
  mv "$PREVIOUS_WEBSITE" "$RELEASES_ROOT/previous-${TIMESTAMP}"
fi
mv "$ACTIVE_WEBSITE" "$PREVIOUS_WEBSITE"
mv "$WORK_ROOT/website" "$ACTIVE_WEBSITE"
chown -R "$SERVICE_USER:$SERVICE_USER" "$ACTIVE_WEBSITE"

rollback() {
  local reason="$1"
  printf '[ultranet-dashboard-deploy] rolling back: %s\n' "$reason" >&2
  if [[ -d "$ACTIVE_WEBSITE" ]]; then
    mv "$ACTIVE_WEBSITE" "$RELEASES_ROOT/failed-${COMMIT_SHA}-${TIMESTAMP}"
  fi
  if [[ -d "$PREVIOUS_WEBSITE" ]]; then
    mv "$PREVIOUS_WEBSITE" "$ACTIVE_WEBSITE"
    chown -R "$SERVICE_USER:$SERVICE_USER" "$ACTIVE_WEBSITE"
  fi
  systemctl restart "$DASHBOARD_SERVICE" || true
  print_dashboard_logs
  exit 1
}

if ! systemctl restart "$DASHBOARD_SERVICE"; then
  rollback "dashboard service restart failed"
fi
sleep 5
systemctl is-active --quiet "$DASHBOARD_SERVICE" || rollback "dashboard service is not active"

if ! curl --fail --silent --show-error --max-time 20 \
  --output "$WORK_ROOT/whitepaper-route.html" \
  http://127.0.0.1:3000/docs/whitepaper; then
  rollback "local whitepaper route did not respond"
fi
grep -Fq "UltraNet Technical Whitepaper" "$WORK_ROOT/whitepaper-route.html" \
  || rollback "local whitepaper route did not render"
if ! curl --fail --silent --show-error --max-time 20 \
  --output "$WORK_ROOT/generated-whitepaper-route.html" \
  http://127.0.0.1:3000/docs/ultranet-whitepaper.html; then
  rollback "local generated HTML route did not respond"
fi
grep -Fq "UltraNet v7.1 Sovereign Technical Guide" "$WORK_ROOT/generated-whitepaper-route.html" \
  || rollback "local generated HTML route did not render"
curl --fail --silent --show-error --output /dev/null --max-time 20 \
  http://127.0.0.1:3000/docs/ultranet-whitepaper.pdf \
  || rollback "local PDF route did not render"

validator_after="$(systemctl is-active "$VALIDATOR_SERVICE" 2>/dev/null || true)"
if [[ "$validator_after" != "$validator_before" ]]; then
  # A dashboard restart can overlap validator startup. `activating` to
  # `active` is the expected healthy transition; every other change still
  # fails closed so an unrelated validator outage cannot be hidden.
  if [[ "$validator_before" != "activating" || "$validator_after" != "active" ]]; then
    rollback "validator state changed from ${validator_before:-unknown} to ${validator_after:-unknown}"
  fi
  log "validator state transitioned normally from activating to active"
fi

mv "$WORK_ROOT" "$RELEASE_ROOT"
rm -f "$ARCHIVE_PATH"
trap - EXIT

# Only after the candidate is live and verified: a failed deploy leaves its
# evidence in place. A prune problem must never fail a good deploy.
log "applying release retention (previous=${KEEP_PREVIOUS} failed=${KEEP_FAILED} releases=${KEEP_COMMIT_RELEASES})"
prune_releases "$PRUNE_DELETE" || log "warning: retention prune failed; release directories were left in place"

log "deployment succeeded: $COMMIT_SHA"
log "dashboard state: $(systemctl is-active "$DASHBOARD_SERVICE")"
log "validator state: ${validator_after:-unknown}"
log "active build id: $(tr -d '\n' < "$ACTIVE_WEBSITE/.next/BUILD_ID")"
