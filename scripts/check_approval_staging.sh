#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
MODE="host"
API_BASE_URL=""

usage() {
    printf '%s\n' \
        "Usage: $0 [--static] [--api-base-url URL]" \
        "" \
        "  --static              Validate checked-in deployment contracts only." \
        "  --api-base-url URL    Local staging API URL (default: derived from env)." \
        "  --help                Show this help."
}

fail() {
    printf 'APPROVAL_PREFLIGHT_FAIL: %s\n' "$1" >&2
    exit 1
}

pass() {
    printf 'APPROVAL_PREFLIGHT_OK: %s\n' "$1"
}

require_file() {
    [[ -f "$1" ]] || fail "required file is missing: $2"
}

contains() {
    grep -Fq -- "$1" "$2" || fail "$3"
}

while (($# > 0)); do
    case "$1" in
        --static)
            MODE="static"
            ;;
        --api-base-url)
            (($# >= 2)) || fail "--api-base-url requires a value"
            API_BASE_URL="$2"
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
    shift
done

UNIT="$REPO_ROOT/deploy/ultranet-approval-signer@.service"
TMPFILES="$REPO_ROOT/deploy/ultranet-approval-signer.tmpfiles"
NODE_DROPIN="$REPO_ROOT/deploy/ultranet.service.d/approval-sockets.conf"
OWNER_AUTH_EXAMPLE="$REPO_ROOT/deploy/sovereign-owner-auth.example.json"
GITIGNORE="$REPO_ROOT/.gitignore"

run_static_checks() {
    SOCKET_UNIT="$REPO_ROOT/deploy/ultranet-approval-signer@.socket"
    require_file "$UNIT" "signer systemd unit"
    require_file "$SOCKET_UNIT" "signer socket activation unit"
    require_file "$TMPFILES" "signer tmpfiles contract"
    require_file "$NODE_DROPIN" "node signer-group drop-in"
    require_file "$OWNER_AUTH_EXAMPLE" "owner auth example"
    require_file "$GITIGNORE" ".gitignore"

    contains "Group=ultranet-approval-owner-%i" "$UNIT" \
        "signer unit does not use a per-owner group"
    contains "--socket /run/ultranet-approval-signer/owner-%i/approval.sock" "$UNIT" \
        "signer unit socket path is not the per-owner socket"
    contains "Requires=ultranet-approval-signer@%i.socket" "$UNIT" \
        "signer service is not bound to its socket activation unit"
    contains "ListenStream=/run/ultranet-approval-signer/owner-%i/approval.sock" "$SOCKET_UNIT" \
        "socket unit does not use the per-owner socket"
    contains "SocketUser=ultranet-approver-%i" "$SOCKET_UNIT" \
        "socket unit does not set the signer owner"
    contains "SocketGroup=ultranet-approval-owner-%i" "$SOCKET_UNIT" \
        "socket unit does not set the per-owner group"
    contains "SocketMode=0660" "$SOCKET_UNIT" \
        "socket unit does not grant the node group access"
    contains "SupplementaryGroups=ultranet-approval-owner-0 ultranet-approval-owner-1 ultranet-approval-owner-2" "$NODE_DROPIN" \
        "node drop-in does not grant all three socket groups"
    contains "/run/ultranet-approval-signer/owner-0/approval.sock" "$OWNER_AUTH_EXAMPLE" \
        "owner-0 mapping does not use the per-owner socket"
    contains "/run/ultranet-approval-signer/owner-1/approval.sock" "$OWNER_AUTH_EXAMPLE" \
        "owner-1 mapping does not use the per-owner socket"
    contains "/run/ultranet-approval-signer/owner-2/approval.sock" "$OWNER_AUTH_EXAMPLE" \
        "owner-2 mapping does not use the per-owner socket"
    contains "/owner-0" "$TMPFILES" "tmpfiles contract is missing owner-0"
    contains "/owner-1" "$TMPFILES" "tmpfiles contract is missing owner-1"
    contains "/owner-2" "$TMPFILES" "tmpfiles contract is missing owner-2"
    contains "/owner-identities.json" "$GITIGNORE" "public approval artifacts are not ignored"
    contains "/transfer.json" "$GITIGNORE" "legacy signed transfer artifacts are not ignored"
    contains "!scripts/check_approval_staging.sh" "$GITIGNORE" "preflight script is still ignored"

    if grep -Eq '^[[:space:]]*--unattended([[:space:]]|$)' "$UNIT"; then
        fail "production signer unit enables unattended file signing"
    fi

    python3 - "$OWNER_AUTH_EXAMPLE" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
entries = json.loads(path.read_text())
if not isinstance(entries, list) or len(entries) != 3:
    raise SystemExit("owner auth example must contain exactly three bindings")
expected_fields = {"owner_index", "session_node_identifier", "signer_id", "signer_socket"}
seen_sessions = set()
seen_signers = set()
seen_indexes = set()
for entry in entries:
    if not isinstance(entry, dict) or set(entry) != expected_fields:
        raise SystemExit("owner auth example contains unsupported or missing fields")
    index = entry["owner_index"]
    if index not in (0, 1, 2) or index in seen_indexes:
        raise SystemExit("owner auth example has invalid or duplicate owner indexes")
    seen_indexes.add(index)
    session = entry["session_node_identifier"]
    if not isinstance(session, str) or not re.fullmatch(r"[0-9a-f]{64}", session):
        if not session.startswith("replace-with-authorized-owner-session-identifier-"):
            raise SystemExit("owner auth example has an invalid session identifier placeholder")
    if entry["signer_id"] != f"owner-{index}" or entry["signer_id"] in seen_signers:
        raise SystemExit("owner auth example has invalid or duplicate signer IDs")
    seen_signers.add(entry["signer_id"])
    expected_socket = f"/run/ultranet-approval-signer/owner-{index}/approval.sock"
    if entry["signer_socket"] != expected_socket:
        raise SystemExit("owner auth example has an unexpected signer socket")
PY

    pass "checked-in signer ACL, socket, mapping, and artifact-ignore contracts"
}

if [[ "$MODE" == "static" ]]; then
    run_static_checks
    exit 0
fi

[[ "$EUID" -eq 0 ]] || fail "host preflight must run as root"
command -v systemctl >/dev/null 2>&1 || fail "systemctl is required"
command -v systemd-tmpfiles >/dev/null 2>&1 || fail "systemd-tmpfiles is required"
command -v python3 >/dev/null 2>&1 || fail "python3 is required"
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v runuser >/dev/null 2>&1 || fail "runuser is required"

ENV_FILE="/etc/ultranet/ultranet.env"
require_file "$ENV_FILE" "/etc/ultranet/ultranet.env"

mapfile -t CONFIG < <(python3 - "$ENV_FILE" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
values = {}
for raw in path.read_text().splitlines():
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    if line.startswith("export "):
        line = line[7:].lstrip()
    key, separator, value = line.partition("=")
    if not separator or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
        raise SystemExit("environment file contains an invalid assignment")
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        value = value[1:-1]
    values[key] = value

if values.get("ULTRANET_WEB_APPROVAL_ENABLED") != "true":
    raise SystemExit("ULTRANET_WEB_APPROVAL_ENABLED must be true for staging preflight")
if values.get("ULTRANET_SESSION_COOKIE_SECURE") != "true":
    raise SystemExit("ULTRANET_SESSION_COOKIE_SECURE must be true on the staging HTTPS path")
for key, value in values.items():
    upper = key.upper()
    if any(token in upper for token in ("PRIVATE_KEY", "SECRET_KEY", "SOVEREIGN_KEYS", "SIGNER_KEY")):
        raise SystemExit(f"private key material must not be present in node environment ({key})")
admin_token = values.get("ULTRANET_ADMIN_TOKEN", "")
if len(admin_token.encode()) < 32:
    raise SystemExit("ULTRANET_ADMIN_TOKEN is missing or shorter than 32 bytes")
for key in ("ULTRANET_AUTHORIZED_NODE_IDENTIFIERS", "ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS"):
    items = [item.strip() for item in values.get(key, "").split(",") if item.strip()]
    if not items or any(not re.fullmatch(r"[0-9a-f]{64}", item) for item in items):
        raise SystemExit(f"{key} must contain one or more lowercase 64-character identifiers")
cors = [item.strip() for item in values.get("ULTRANET_CORS_ORIGINS", "").split(",") if item.strip()]
if not cors or any(item == "*" or not item.startswith("https://") for item in cors):
    raise SystemExit("ULTRANET_CORS_ORIGINS must contain explicit HTTPS origins only")
auth_path = values.get("ULTRANET_SOVEREIGN_OWNER_AUTH_FILE", "")
if not auth_path.startswith("/etc/ultranet/"):
    raise SystemExit("ULTRANET_SOVEREIGN_OWNER_AUTH_FILE must be under /etc/ultranet")
api_bind = values.get("ULTRANET_API_BIND", "")
if not re.fullmatch(r"127\.0\.0\.1:[0-9]+", api_bind):
    raise SystemExit("ULTRANET_API_BIND must bind to loopback for staging")
print(auth_path)
print(api_bind)
PY
)
(( ${#CONFIG[@]} == 2 )) || fail "could not read the non-secret staging configuration"
AUTH_FILE="${CONFIG[0]}"
API_BIND="${CONFIG[1]}"

python3 - "$ENV_FILE" "$AUTH_FILE" <<'PY'
import json
import pwd
import grp
import re
import stat
import sys
from pathlib import Path

env_path = Path(sys.argv[1])
auth_path = Path(sys.argv[2])

def exact_mode(path: Path, expected: int, label: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode != expected:
        raise SystemExit(f"{label} must have mode {expected:04o}")

def one_of_modes(path: Path, expected: set[int], label: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode not in expected:
        formatted = ", ".join(f"{value:04o}" for value in sorted(expected))
        raise SystemExit(f"{label} must have mode one of {formatted}")

def require_root_group(path: Path, expected_group: str, label: str) -> None:
    metadata = path.stat()
    if pwd.getpwuid(metadata.st_uid).pw_name != "root":
        raise SystemExit(f"{label} must be owned by root")
    if grp.getgrgid(metadata.st_gid).gr_name != expected_group:
        raise SystemExit(f"{label} must be owned by group {expected_group}")

def read_env(path: Path) -> dict[str, str]:
    values = {}
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].lstrip()
        key, separator, value = line.partition("=")
        if separator:
            values[key] = value.strip().strip("\"'")
    return values

exact_mode(env_path, 0o640, "node environment")
require_root_group(env_path, "ultranet", "node environment")
auth_mode = stat.S_IMODE(auth_path.stat().st_mode)
one_of_modes(auth_path, {0o600, 0o640}, "owner auth mapping")
if auth_mode == 0o640:
    require_root_group(auth_path, "ultranet", "owner auth mapping")
elif auth_path.stat().st_uid != 0:
    raise SystemExit("0600 owner auth mapping must be owned by root")

entries = json.loads(auth_path.read_text())
if not isinstance(entries, list) or len(entries) != 3:
    raise SystemExit("owner auth mapping must contain exactly three bindings")
expected_fields = {"owner_index", "session_node_identifier", "signer_id", "signer_socket"}
seen_sessions = set()
seen_signers = set()
for entry in entries:
    if not isinstance(entry, dict) or set(entry) != expected_fields:
        raise SystemExit("owner auth mapping contains unsupported or secret fields")
    index = entry["owner_index"]
    if index not in (0, 1, 2):
        raise SystemExit("owner auth mapping contains an invalid owner index")
    session = entry["session_node_identifier"]
    if not isinstance(session, str) or not re.fullmatch(r"[0-9a-f]{64}", session):
        raise SystemExit("owner auth mapping contains an invalid session identifier")
    if session in seen_sessions:
        raise SystemExit("owner auth mapping contains duplicate session identifiers")
    seen_sessions.add(session)
    if entry["signer_id"] != f"owner-{index}" or entry["signer_id"] in seen_signers:
        raise SystemExit("owner auth mapping contains invalid or duplicate signer IDs")
    seen_signers.add(entry["signer_id"])
    if entry["signer_socket"] != f"/run/ultranet-approval-signer/owner-{index}/approval.sock":
        raise SystemExit("owner auth mapping contains an unexpected socket path")

env = read_env(env_path)
for key in ("ULTRANET_AUTHORIZED_NODE_IDENTIFIERS", "ULTRANET_VALIDATOR_REVIEW_IDENTIFIERS"):
    configured = {item.strip() for item in env[key].split(",") if item.strip()}
    if configured != seen_sessions:
        raise SystemExit(f"{key} must exactly match the owner session mapping")

for index in range(3):
    group_name = f"ultranet-approval-owner-{index}"
    user_name = f"ultranet-approver-{index}"
    try:
        group.getgrnam(group_name)
        account = pwd.getpwnam(user_name)
    except KeyError as error:
        raise SystemExit(f"missing signer account or group for owner {index}") from error
    if grp.getgrgid(account.pw_gid).gr_name != group_name:
        raise SystemExit(f"{user_name} must use {group_name} as its primary group")

    key_path = Path(f"/var/lib/ultranet-approval-signer/owner-{index}/key.json")
    if not key_path.is_file():
        raise SystemExit(f"missing private signer key file for owner {index}")
    exact_mode(key_path, 0o600, f"owner-{index} signer key")
    key_stat = key_path.stat()
    if pwd.getpwuid(key_stat.st_uid).pw_name != user_name:
        raise SystemExit(f"owner-{index} signer key has the wrong owner")
    records = json.loads(key_path.read_text())
    if isinstance(records, dict) and "owners" in records:
        records = records["owners"]
    elif isinstance(records, dict):
        records = [records]
    if not isinstance(records, list) or len(records) != 1:
        raise SystemExit(f"owner-{index} signer must contain exactly one private key record")
    record = records[0]
    if not isinstance(record, dict) or "public_key" not in record or not ("secret_key" in record or "private_key" in record):
        raise SystemExit(f"owner-{index} signer key record is incomplete")
PY

for group in ultranet-approval-owner-0 ultranet-approval-owner-1 ultranet-approval-owner-2; do
    getent group "$group" >/dev/null || fail "missing required group: $group"
done

NODE_GROUPS="$(systemctl show ultranet.service -p SupplementaryGroups --value)"
for group in ultranet-approval-owner-0 ultranet-approval-owner-1 ultranet-approval-owner-2; do
    [[ " $NODE_GROUPS " == *" $group "* ]] || fail "ultranet.service is missing supplementary group $group"
done

systemctl is-active --quiet ultranet.service || fail "ultranet.service is not active"
for index in 0 1 2; do
    socket_unit="ultranet-approval-signer@${index}.socket"
    service="ultranet-approval-signer@${index}.service"
    systemctl is-active --quiet "$socket_unit" || fail "$socket_unit is not active"
    [[ "$(systemctl show "$service" -p User --value)" == "ultranet-approver-${index}" ]] || fail "$service has the wrong user"
    [[ "$(systemctl show "$service" -p Group --value)" == "ultranet-approval-owner-${index}" ]] || fail "$service has the wrong group"
    unit_text="$(systemctl cat "$service")"
    if grep -Eq '^[[:space:]]*--unattended([[:space:]]|$)' <<<"$unit_text"; then
        fail "$service enables unattended file signing"
    fi

done

TMPFILES_INSTALLED="/etc/tmpfiles.d/ultranet-approval-signer.conf"
require_file "$TMPFILES_INSTALLED" "installed signer tmpfiles configuration"

for index in 0 1 2; do
    signer_user="ultranet-approver-${index}"
    signer_group="ultranet-approval-owner-${index}"
    socket_dir="/run/ultranet-approval-signer/owner-${index}"
    socket_path="$socket_dir/approval.sock"
    [[ -d "$socket_dir" ]] || fail "missing runtime directory for owner ${index}"
    [[ "$(stat -c '%a' "$socket_dir")" == "710" ]] || fail "runtime directory for owner ${index} must be mode 0710"
    [[ "$(stat -c '%U' "$socket_dir")" == "$signer_user" ]] || fail "runtime directory for owner ${index} has the wrong owner"
    [[ "$(stat -c '%G' "$socket_dir")" == "$signer_group" ]] || fail "runtime directory for owner ${index} has the wrong group"
    [[ -S "$socket_path" ]] || fail "missing signer socket for owner ${index}"
    [[ "$(stat -c '%a' "$socket_path")" == "660" ]] || fail "signer socket for owner ${index} must be mode 0660"
    [[ "$(stat -c '%U' "$socket_path")" == "$signer_user" ]] || fail "signer socket for owner ${index} has the wrong owner"
    [[ "$(stat -c '%G' "$socket_path")" == "$signer_group" ]] || fail "signer socket for owner ${index} has the wrong group"

    runuser -u ultranet -- python3 - "$socket_path" <<'PY' || fail "ultranet cannot connect to a signer socket"
import socket
import sys

connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
connection.settimeout(2)
connection.connect(sys.argv[1])
connection.close()
PY
    systemctl is-active --quiet "ultranet-approval-signer@${index}.service" \
        || fail "ultranet-approval-signer@${index}.service did not activate from its socket"

    key_path="/var/lib/ultranet-approval-signer/owner-${index}/key.json"
    runuser -u ultranet -- test ! -r "$key_path" || fail "node user can read owner-${index} private key"
    runuser -u "$signer_user" -- test -r "$key_path" || fail "$signer_user cannot read its private key"
done

if [[ -z "$API_BASE_URL" ]]; then
    API_BASE_URL="http://${API_BIND}"
fi
[[ "$API_BASE_URL" =~ ^https?:// ]] || fail "API base URL must include an HTTP(S) scheme"
curl --fail --silent --show-error --max-time 5 "$API_BASE_URL/api/stats" >/dev/null \
    || fail "staging API health request failed"
review_status="$(curl --silent --show-error --max-time 5 -o /dev/null -w '%{http_code}' "$API_BASE_URL/api/governance/validator-review")"
[[ "$review_status" == "401" ]] || fail "validator review route did not reject an unauthenticated request with 401"

pass "staging signer groups, socket ACLs, key isolation, service state, and protected API boundary are ready"
