#!/usr/bin/env bash
# Publish the public faucet page to the reverse proxy's document root.
#
# The page is plain static files: index.html, app.js and styles.css. They are installed 0644 and
# root-owned, then verified with assert-permissions.sh. Caddy serves them as its own user, so any
# tighter mode takes the claim page down (ERR_HTTP_RESPONSE_CODE_FAILURE, 2026-11-15).
#
# Usage: deploy-faucet-site.sh [destination]
#   destination defaults to $FAUCET_SITE_ROOT, then /var/www/faucet.
#
# Run it on the host that serves faucet.ultranetwork.cc. It is idempotent and needs root only to
# write into the document root.

set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SOURCE_DIR="$SCRIPT_DIR/faucet-site"
destination="${1:-${FAUCET_SITE_ROOT:-/var/www/faucet}}"
readonly FILES=(index.html app.js styles.css)

log() { printf '[deploy-faucet-site] %s\n' "$*"; }

[ -d "$destination" ] || { printf 'destination does not exist: %s\n' "$destination" >&2; exit 1; }

for file in "${FILES[@]}"; do
  [ -f "$SOURCE_DIR/$file" ] || { printf 'missing source file: %s\n' "$SOURCE_DIR/$file" >&2; exit 1; }
done

log "installing ${#FILES[@]} files into $destination as root:root 0644"
for file in "${FILES[@]}"; do
  install -o root -g root -m 0644 "$SOURCE_DIR/$file" "$destination/$file"
  log "  $file  $(stat -c '%a %U:%G' "$destination/$file")"
done

log "verifying every file is readable by the web server"
"$SCRIPT_DIR/assert-permissions.sh" "$destination"

log "done; reload is not required for static files"
