#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/migrate_appchain_registry.sh <db-path> [backup-dir] [--apply]

The migrator always writes a raw backup first and is a dry run unless --apply
is provided. The node service must already be stopped.
USAGE
}

if [[ $# -lt 1 || "$1" == "-h" || "$1" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
migrator="${ULTRANET_APPCHAIN_MIGRATOR_BIN:-$repo_root/target/release/ultranet-appchain-migrate}"
db_path="$1"
shift

if [[ $# -gt 0 && "$1" != --* ]]; then
  backup_dir="$1"
  shift
else
  backup_dir="/var/backups/ultranet/appchain-registry-$(date -u +%Y%m%dT%H%M%SZ)"
fi

if [[ ! -x "$migrator" ]]; then
  printf 'Missing %s. Build it with: cargo build --release --locked --bin ultranet-appchain-migrate\n' "$migrator" >&2
  exit 1
fi

exec "$migrator" --db-path "$db_path" --backup-dir "$backup_dir" "$@"
