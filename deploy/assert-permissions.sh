#!/usr/bin/env bash
# Assert that static web assets are readable by the server that serves them.
#
# A web server runs as its own unprivileged user, so a file installed root-owned and mode 0640
# is invisible to it and every request for that path fails. That happened to the faucet page on
# 2026-11-15: `install -m 640` made https://faucet.ultranetwork.cc/ return
# ERR_HTTP_RESPONSE_CODE_FAILURE until the mode was corrected to 0644.
#
# Usage: assert-permissions.sh <file-or-directory>...
# Exits non-zero (listing every offender) when anything under the given paths would not be served.

set -Eeuo pipefail

if [ "$#" -eq 0 ]; then
  printf 'usage: %s <file-or-directory>...\n' "$0" >&2
  exit 2
fi

failures=0
checked=0

report() {
  printf '  %s: %s\n' "$1" "$2" >&2
  failures=$((failures + 1))
}

for target in "$@"; do
  if [ ! -e "$target" ]; then
    report "$target" "missing"
    continue
  fi

  while IFS= read -r -d '' path; do
    checked=$((checked + 1))
    mode=$(stat -c '%a' "$path")

    if [ -d "$path" ]; then
      # Directories need the execute bit for other, or nothing inside them can be reached.
      if (( (8#$mode & 8#001) == 0 )); then
        report "$path" "directory not traversable by other ($mode)"
      fi
      if (( (8#$mode & 8#002) != 0 )); then
        report "$path" "directory is world-writable ($mode)"
      fi
      continue
    fi

    if [ ! -f "$path" ]; then
      report "$path" "not a regular file"
      continue
    fi
    if (( (8#$mode & 8#004) == 0 )); then
      report "$path" "not world-readable ($mode) — the web server cannot read this"
    fi
    if (( (8#$mode & 8#002) != 0 )); then
      report "$path" "world-writable ($mode)"
    fi
  done < <(find "$target" -print0)
done

if [ "$failures" -ne 0 ]; then
  printf '\nFAILED: %d permission problem(s) across %d path(s).\n' "$failures" "$checked" >&2
  exit 1
fi

printf 'OK: %d path(s) checked, every file is readable by the web server.\n' "$checked"
