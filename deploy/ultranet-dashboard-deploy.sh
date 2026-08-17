#!/usr/bin/env bash
set -Eeuo pipefail

readonly DEPLOY_USER="ultranet-deploy"
readonly SERVICE_USER="ultranet"
readonly STAGING_ROOT="/var/lib/ultranet-deploy/staging"
readonly RELEASES_ROOT="/opt/ultranet/releases"
readonly ACTIVE_WEBSITE="/opt/ultranet/website"
readonly SHARED_SOURCE="/opt/ultranet/ULTRA_NET_TECHNICAL_GUIDE.md"
readonly DASHBOARD_SERVICE="ultranet-dashboard.service"
readonly VALIDATOR_SERVICE="ultranet.service"

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

if [[ "${EUID}" -ne 0 ]]; then
  fail "this helper must run as root"
fi

if [[ "$#" -ne 1 || ! "$1" =~ ^[0-9a-f]{40}$ ]]; then
  fail "usage: $0 <40-character commit SHA>"
fi

readonly COMMIT_SHA="$1"
readonly ARCHIVE_PATH="$STAGING_ROOT/${COMMIT_SHA}.tar.gz"
readonly WORK_ROOT="$RELEASES_ROOT/.staging-${COMMIT_SHA}-$$"
readonly RELEASE_ROOT="$RELEASES_ROOT/${COMMIT_SHA}"
readonly PREVIOUS_WEBSITE="$RELEASES_ROOT/previous"
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
  rollback "validator state changed from ${validator_before:-unknown} to ${validator_after:-unknown}"
fi

mv "$WORK_ROOT" "$RELEASE_ROOT"
rm -f "$ARCHIVE_PATH"
trap - EXIT

log "deployment succeeded: $COMMIT_SHA"
log "dashboard state: $(systemctl is-active "$DASHBOARD_SERVICE")"
log "validator state: ${validator_after:-unknown}"
log "active build id: $(tr -d '\n' < "$ACTIVE_WEBSITE/.next/BUILD_ID")"
