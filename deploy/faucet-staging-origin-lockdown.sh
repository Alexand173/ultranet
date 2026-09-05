#!/usr/bin/env bash
set -uo pipefail

# Staging-only Cloudflare origin firewall procedure.
# This script never changes DNS, Cloudflare WAF rules, Caddy, SSH/P2P rules,
# production services, credentials, faucet intake, or claim state.
# Run only on the separately provisioned staging VPS.

readonly STAGING_HOSTNAME='faucet-staging.ultranetwork.cc'
readonly STAGING_HOST_IDENTITY='ubuntu-2gb-nbg1-2'
readonly STAGING_IPV4='2.28.61.71'
readonly STAGING_IPV6='2a01:4f8:1c16:797e::1'
readonly PRODUCTION_IPV4='167.233.161.115'
readonly CLOUDFLARE_IPV4_URL='https://www.cloudflare.com/ips-v4'
readonly CLOUDFLARE_IPV6_URL='https://www.cloudflare.com/ips-v6'
readonly CADDY_SERVICE='caddy-staging.service'
readonly PRODUCTION_CADDY_SERVICE='caddy.service'
readonly FAUCET_ENV='/etc/ultranet-faucet-staging/faucet.env'
readonly CADDYFILE='/etc/caddy/Caddyfile.staging'
readonly BACKUP_ROOT='/var/backups/ultranet/faucet-staging-origin-lockdown'
readonly MIN_IPV4_RANGES=10
readonly MIN_IPV6_RANGES=5
readonly MAX_RANGES_PER_FAMILY=128

MODE=''
REMOVE_BROAD='false'
WORKDIR=''
V4_FILE=''
V6_FILE=''
V4_COUNT=0
V6_COUNT=0
DNS_V4=''
DNS_V6=''

# Keep collecting failures during --check so the operator sees every local
# blocker in one dry run. No function below mutates the firewall in check mode.
declare -a ERRORS=()
declare -a NOTES=()

usage() {
    cat <<'USAGE'
Usage:
  sudo bash deploy/faucet-staging-origin-lockdown.sh --check
  sudo bash deploy/faucet-staging-origin-lockdown.sh --apply
  sudo env STAGING_LOCKDOWN_CONFIRM=I_UNDERSTAND \
    bash deploy/faucet-staging-origin-lockdown.sh --apply --remove-broad

Modes:
  --check          Read-only staging identity/range/UFW/Caddy check. It also
                   prints the Cloudflare rules that an apply would add.
  --apply          Snapshot UFW, add Cloudflare IPv4/IPv6 TCP 80/443 rules,
                   retain broad web rules, and verify public staging HTTPS.
  --remove-broad  With --apply, remove broad TCP 80/443 rules only after the
                   additive rules and public checks pass.

The script is staging-specific and refuses the production origin identity.
It does not activate UFW automatically. Review the existing SSH/P2P policy and
confirm an independent recovery path before any --apply operation.
USAGE
}

fail_check() {
    ERRORS+=("$*")
    printf 'FAIL: %s\n' "$*" >&2
}

note() {
    NOTES+=("$*")
    printf 'NOTE: %s\n' "$*"
}

has_command() {
    command -v "$1" >/dev/null 2>&1
}

cleanup() {
    if [[ -n "$WORKDIR" && -d "$WORKDIR" ]]; then
        chmod -R u+rwX,go-rwx "$WORKDIR" 2>/dev/null || true
        find "$WORKDIR" -type f -exec rm -f -- {} + 2>/dev/null || true
        rmdir "$WORKDIR" 2>/dev/null || true
    fi
}
trap cleanup EXIT

parse_args() {
    while (($# > 0)); do
        case "$1" in
            --check)
                [[ -z "$MODE" ]] || { printf '%s\n' 'choose exactly one mode' >&2; exit 2; }
                MODE='check'
                ;;
            --apply)
                [[ -z "$MODE" ]] || { printf '%s\n' 'choose exactly one mode' >&2; exit 2; }
                MODE='apply'
                ;;
            --remove-broad)
                REMOVE_BROAD='true'
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            *)
                usage >&2
                printf 'unknown argument: %s\n' "$1" >&2
                exit 2
                ;;
        esac
        shift
    done
    [[ -n "$MODE" ]] || { usage >&2; exit 2; }
    if [[ "$REMOVE_BROAD" == 'true' && "$MODE" != 'apply' ]]; then
        printf '%s\n' '--remove-broad requires --apply' >&2
        exit 2
    fi
}

require_commands() {
    local command
    for command in curl getent hostnamectl ip install mktemp python3 sha256sum ss systemctl ufw; do
        if ! has_command "$command"; then
            fail_check "required command is missing: $command"
        fi
    done
}

actual_hostname() {
    local value=''
    if has_command hostnamectl; then
        value="$(hostnamectl --static 2>/dev/null || true)"
    fi
    if [[ -z "$value" ]] && has_command hostname; then
        value="$(hostname 2>/dev/null || true)"
    fi
    printf '%s' "$value"
}

check_host_identity() {
    local hostname_value
    hostname_value="$(actual_hostname)"
    if [[ "$hostname_value" != "$STAGING_HOST_IDENTITY" ]]; then
        fail_check "host identity is '$hostname_value', expected '$STAGING_HOST_IDENTITY'"
    else
        printf 'host_identity=%s\n' "$hostname_value"
    fi

    local ipv4s ipv6s
    ipv4s="$(ip -4 -o addr show scope global 2>/dev/null | awk '{sub(/\/.*/, "", $4); print $4}' || true)"
    ipv6s="$(ip -6 -o addr show scope global 2>/dev/null | awk '{sub(/\/.*/, "", $4); print $4}' || true)"
    printf 'global_ipv4s=%s\n' "${ipv4s//$'\n'/ }"
    printf 'global_ipv6s=%s\n' "${ipv6s//$'\n'/ }"

    if ! grep -Fxq "$STAGING_IPV4" <<<"$ipv4s"; then
        fail_check "staging IPv4 $STAGING_IPV4 is not assigned to this host"
    fi
    if grep -Fxq "$PRODUCTION_IPV4" <<<"$ipv4s"; then
        fail_check "production IPv4 $PRODUCTION_IPV4 is assigned to this host"
    fi
    if ! grep -Fxq "$STAGING_IPV6" <<<"$ipv6s"; then
        fail_check "staging IPv6 $STAGING_IPV6 is not assigned to this host"
    fi
}

validate_range_file() {
    local file=$1
    local version=$2
    python3 - "$file" "$version" "$MIN_IPV4_RANGES" "$MIN_IPV6_RANGES" "$MAX_RANGES_PER_FAMILY" <<'PY'
import ipaddress
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
version = int(sys.argv[2])
minimum = int(sys.argv[3] if version == 4 else sys.argv[4])
maximum = int(sys.argv[5])
try:
    raw = path.read_bytes()
    text = raw.decode("ascii")
except (OSError, UnicodeDecodeError) as error:
    raise SystemExit(f"cannot read ASCII range list: {error}")

lines = []
for raw_line in text.splitlines():
    value = raw_line.split("#", 1)[0].strip()
    if value:
        lines.append(value)
if len(lines) < minimum:
    raise SystemExit(f"Cloudflare IPv{version} list is unexpectedly small: {len(lines)}")
if len(lines) > maximum:
    raise SystemExit(f"Cloudflare IPv{version} list is unexpectedly large: {len(lines)}")

networks = []
for value in lines:
    try:
        network = ipaddress.ip_network(value, strict=True)
    except ValueError as error:
        raise SystemExit(f"invalid Cloudflare IPv{version} CIDR {value!r}: {error}")
    if network.version != version:
        raise SystemExit(f"Cloudflare IPv{version} list contains another family: {value}")
    if network.prefixlen == 0 or not network.is_global:
        raise SystemExit(f"Cloudflare IPv{version} list contains a non-global/default range: {value}")
    networks.append(network)

ordered = sorted(networks, key=lambda item: (int(item.network_address), item.prefixlen))
if len(set(ordered)) != len(ordered):
    raise SystemExit(f"Cloudflare IPv{version} list contains duplicate ranges")
for previous, current in zip(ordered, ordered[1:]):
    if previous.overlaps(current):
        raise SystemExit(f"Cloudflare IPv{version} list contains overlapping ranges")
print(len(ordered))
PY
}

fetch_and_validate_ranges() {
    WORKDIR="$(mktemp -d /tmp/ultranet-faucet-staging-lockdown.XXXXXX)"
    V4_FILE="$WORKDIR/ips-v4"
    V6_FILE="$WORKDIR/ips-v6"
    if ! curl --fail --silent --show-error --proto '=https' --tlsv1.2 \
        --connect-timeout 10 --max-time 30 "$CLOUDFLARE_IPV4_URL" -o "$V4_FILE"; then
        fail_check "unable to fetch Cloudflare IPv4 ranges"
    else
        if V4_COUNT="$(validate_range_file "$V4_FILE" 4 2>/dev/null)"; then
            printf 'cloudflare_ipv4_ranges=%s sha256=%s\n' "$V4_COUNT" "$(sha256sum "$V4_FILE" | awk '{print $1}')"
        else
            fail_check "Cloudflare IPv4 range validation failed"
            V4_COUNT=0
        fi
    fi

    if ! curl --fail --silent --show-error --proto '=https' --tlsv1.2 \
        --connect-timeout 10 --max-time 30 "$CLOUDFLARE_IPV6_URL" -o "$V6_FILE"; then
        fail_check "unable to fetch Cloudflare IPv6 ranges"
    else
        if V6_COUNT="$(validate_range_file "$V6_FILE" 6 2>/dev/null)"; then
            printf 'cloudflare_ipv6_ranges=%s sha256=%s\n' "$V6_COUNT" "$(sha256sum "$V6_FILE" | awk '{print $1}')"
        else
            fail_check "Cloudflare IPv6 range validation failed"
            V6_COUNT=0
        fi
    fi
}

validate_dns_family() {
    local family=$1
    local addresses=$2
    local file=$3
    local label=$4
    if [[ -z "$addresses" ]]; then
        fail_check "$label has no DNS answers"
        return
    fi
    if ! HOST_ADDRESSES="$addresses" python3 - "$file" "$family" "$STAGING_IPV4" "$STAGING_IPV6" "$PRODUCTION_IPV4" <<'PY'
import ipaddress
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
version = int(sys.argv[2])
expected_v4 = ipaddress.ip_address(sys.argv[3])
expected_v6 = ipaddress.ip_address(sys.argv[4])
production = ipaddress.ip_address(sys.argv[5])
ranges = [
    ipaddress.ip_network(line.strip())
    for line in path.read_text(encoding="ascii").splitlines()
    if line.strip()
]
for raw in os.environ["HOST_ADDRESSES"].splitlines():
    address = ipaddress.ip_address(raw.strip())
    if address.version != version:
        raise SystemExit(f"unexpected address family in {raw}")
    if address in {expected_v4, expected_v6, production}:
        raise SystemExit(f"DNS exposes an origin address: {address}")
    if not any(address in network for network in ranges):
        raise SystemExit(f"DNS answer is outside Cloudflare IPv{version} ranges: {address}")
PY
    then
        fail_check "$label contains a non-Cloudflare or origin address"
    else
        printf '%s=%s\n' "$label" "${addresses//$'\n'/ }"
    fi
}

check_dns() {
    if [[ "$V4_COUNT" -gt 0 ]]; then
        DNS_V4="$(getent ahostsv4 "$STAGING_HOSTNAME" 2>/dev/null | awk '{print $1}' | sort -u || true)"
        validate_dns_family 4 "$DNS_V4" "$V4_FILE" 'staging_dns_ipv4'
    fi
    if [[ "$V6_COUNT" -gt 0 ]]; then
        DNS_V6="$(getent ahostsv6 "$STAGING_HOSTNAME" 2>/dev/null | awk '{print $1}' | sort -u || true)"
        validate_dns_family 6 "$DNS_V6" "$V6_FILE" 'staging_dns_ipv6'
    fi
}

check_ufw() {
    if ! has_command ufw; then
        return
    fi
    local status
    status="$(ufw status 2>/dev/null || true)"
    if grep -Fq 'Status: active' <<<"$status"; then
        printf '%s\n' 'ufw_status=active'
    else
        fail_check 'UFW is inactive; the script will not enable it automatically'
    fi

    if [[ ! -r /etc/default/ufw ]] || ! grep -Eq '^IPV6=yes$' /etc/default/ufw; then
        fail_check 'UFW IPv6 filtering is not explicitly enabled in /etc/default/ufw'
    else
        printf '%s\n' 'ufw_ipv6=yes'
    fi

    if [[ -n "$status" ]]; then
        if grep -Eq '(^|[[:space:]])(80|443)/tcp[[:space:]]+ALLOW[[:space:]]+IN[[:space:]]+Anywhere([[:space:]]|$)' <<<"$status"; then
            note 'broad IPv4 web rules are present and would be retained during additive apply'
        fi
        if grep -Eq '(^|[[:space:]])(80|443)/tcp[[:space:]]+\(v6\)[[:space:]]+ALLOW[[:space:]]+IN[[:space:]]+Anywhere[[:space:]]+\(v6\)' <<<"$status"; then
            note 'broad IPv6 web rules are present and would be retained during additive apply'
        fi
    fi
}

check_caddy_and_services() {
    if ! has_command systemctl; then
        return
    fi
    local fragment exec_start
    fragment="$(systemctl show "$CADDY_SERVICE" -p FragmentPath --value 2>/dev/null || true)"
    exec_start="$(systemctl show "$CADDY_SERVICE" -p ExecStart --value 2>/dev/null || true)"
    if [[ "$fragment" != '/etc/systemd/system/caddy-staging.service' ]]; then
        fail_check "staging Caddy FragmentPath is '$fragment', expected /etc/systemd/system/caddy-staging.service"
    fi
    if [[ "$exec_start" != *'/etc/caddy/Caddyfile.staging'* ]]; then
        fail_check 'staging Caddy ExecStart does not load /etc/caddy/Caddyfile.staging'
    fi
    if ! systemctl is-active --quiet "$CADDY_SERVICE"; then
        fail_check "$CADDY_SERVICE is not active"
    else
        printf '%s\n' 'caddy_staging_active=yes'
    fi
    if systemctl is-active --quiet "$PRODUCTION_CADDY_SERVICE"; then
        fail_check "$PRODUCTION_CADDY_SERVICE is active on the staging host"
    else
        printf '%s\n' 'caddy_production_active=no'
    fi

    if [[ ! -f "$CADDYFILE" ]]; then
        fail_check "missing $CADDYFILE"
    elif has_command caddy; then
        if ! caddy validate --config "$CADDYFILE" --adapter caddyfile; then
            fail_check 'staging Caddyfile validation failed'
        fi
    else
        fail_check 'caddy command is missing'
    fi

    if [[ ! -r "$FAUCET_ENV" ]]; then
        fail_check "missing $FAUCET_ENV"
    elif ! grep -Eq '^FAUCET_ENABLED=false$' "$FAUCET_ENV"; then
        fail_check 'staging faucet environment does not explicitly set FAUCET_ENABLED=false'
    else
        printf '%s\n' 'faucet_enabled=false'
    fi

    if has_command ss; then
        local listeners
        listeners="$(ss -ltnp 2>/dev/null || true)"
        if ! grep -Eq ':80[[:space:]]|:443[[:space:]]' <<<"$listeners"; then
            fail_check 'Caddy is not listening on both TCP 80 and 443'
        fi
        if ! grep -Eq '127\.0\.0\.1:8081|127\.0\.0\.1:8090' <<<"$listeners"; then
            fail_check 'expected loopback node/faucet listeners were not observed'
        fi
        if grep -Eq '0\.0\.0\.0:8081|0\.0\.0\.0:8090|\*:8081|\*:8090' <<<"$listeners"; then
            fail_check 'node or faucet listener is not loopback-only'
        fi
    fi
}

print_rule_plan() {
    printf '%s\n' '--- planned Cloudflare staging UFW rules ---'
    if [[ -f "$V4_FILE" ]]; then
        while IFS= read -r cidr || [[ -n "$cidr" ]]; do
            [[ -n "$cidr" ]] || continue
            printf 'ufw allow from %s to any port 80 proto tcp\n' "$cidr"
            printf 'ufw allow from %s to any port 443 proto tcp\n' "$cidr"
        done < "$V4_FILE"
    fi
    if [[ -f "$V6_FILE" ]]; then
        while IFS= read -r cidr || [[ -n "$cidr" ]]; do
            [[ -n "$cidr" ]] || continue
            printf 'ufw allow from %s to any port 80 proto tcp\n' "$cidr"
            printf 'ufw allow from %s to any port 443 proto tcp\n' "$cidr"
        done < "$V6_FILE"
    fi
    printf '%s\n' 'broad_rules=retained_during_additive_apply'
    if [[ "$REMOVE_BROAD" == 'true' ]]; then
        printf '%s\n' 'broad_rules=removed_only_after_public_checks_and_private_confirmation'
    fi
}

public_readonly_checks() {
    local status_code status_file headers_file
    status_file="$WORKDIR/public-status.json"
    headers_file="$WORKDIR/public-status.headers"
    status_code="$(curl --silent --show-error --fail --connect-timeout 10 --max-time 20 \
        --output "$status_file" --dump-header "$headers_file" \
        --write-out '%{http_code}' "https://$STAGING_HOSTNAME/api/faucet/status" 2>/dev/null || true)"
    if [[ "$status_code" != '200' ]]; then
        fail_check "public staging status returned HTTP ${status_code:-request-failed}"
        return
    fi
    if ! python3 - "$status_file" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
data = value["data"]
assert value["success"] is True
assert data["enabled"] is False
assert data["availability"] == "disabled"
assert data["claim_amount_base_units"] == 1_000_000
assert data["claim_amount_ultra"] == "1.000000"
assert data["decimals"] == 6
PY
    then
        fail_check 'public staging status does not match the disabled fixed-amount contract'
    else
        printf '%s\n' 'public_status=200 disabled amount=1.000000 decimals=6'
    fi
    if ! grep -Eiq '^cache-control:.*no-store' "$headers_file"; then
        fail_check 'public staging status is missing Cache-Control: no-store'
    fi
}

snapshot_ufw() {
    local stamp snapshot_dir
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    snapshot_dir="$BACKUP_ROOT/$stamp"
    install -d -o root -g root -m 0700 "$snapshot_dir" || return 1
    ufw status verbose > "$snapshot_dir/ufw-before.txt" || return 1
    ufw show raw > "$snapshot_dir/ufw-before.raw" || return 1
    ip -4 -o addr show scope global > "$snapshot_dir/ipv4-before.txt" || return 1
    ip -6 -o addr show scope global > "$snapshot_dir/ipv6-before.txt" || return 1
    ss -ltnp > "$snapshot_dir/listeners-before.txt" || return 1
    if [[ -f "$V4_FILE" ]]; then install -o root -g root -m 0644 "$V4_FILE" "$snapshot_dir/ips-v4" || return 1; fi
    if [[ -f "$V6_FILE" ]]; then install -o root -g root -m 0644 "$V6_FILE" "$snapshot_dir/ips-v6" || return 1; fi
    printf 'ufw_snapshot=%s\n' "$snapshot_dir"
}

allow_cloudflare_ranges() {
    local file cidr port
    for file in "$V4_FILE" "$V6_FILE"; do
        while IFS= read -r cidr || [[ -n "$cidr" ]]; do
            [[ -n "$cidr" ]] || continue
            for port in 80 443; do
                if ! ufw status | grep -F "$cidr" | grep -Fq "${port}/tcp"; then
                    if ! ufw allow from "$cidr" to any port "$port" proto tcp comment 'Cloudflare staging web origin'; then
                        fail_check "could not add Cloudflare rule $cidr:$port"
                    fi
                fi
            done
        done < "$file"
    done
}

assert_no_broad_web_rules() {
    local status
    status="$(ufw status 2>/dev/null || true)"
    if grep -Eq '(^|[[:space:]])(80|443)/tcp[[:space:]]+ALLOW[[:space:]]+IN[[:space:]]+Anywhere([[:space:]]|$)' <<<"$status"; then
        fail_check 'a broad IPv4 TCP 80/443 rule remains'
    fi
    if grep -Eq '(^|[[:space:]])(80|443)/tcp[[:space:]]+\(v6\)[[:space:]]+ALLOW[[:space:]]+IN[[:space:]]+Anywhere[[:space:]]+\(v6\)' <<<"$status"; then
        fail_check 'a broad IPv6 TCP 80/443 rule remains'
    fi
}

remove_broad_web_rules() {
    if ! ufw --force delete allow 80/tcp; then
        note 'no broad TCP 80 rule was deleted'
    fi
    if ! ufw --force delete allow 443/tcp; then
        note 'no broad TCP 443 rule was deleted'
    fi
    assert_no_broad_web_rules
}

report_and_exit_if_failed() {
    if ((${#ERRORS[@]} > 0)); then
        printf '\npreflight=failed errors=%s\n' "${#ERRORS[@]}" >&2
        local error
        for error in "${ERRORS[@]}"; do
            printf -- '- %s\n' "$error" >&2
        done
        return 1
    fi
    printf '\npreflight=passed\n'
    return 0
}

main() {
    parse_args "$@"
    if [[ "$EUID" -ne 0 ]]; then
        fail_check 'run this staging procedure as root, normally through sudo'
    fi
    if [[ "$MODE" == 'apply' && -z "${SSH_CONNECTION:-}" ]]; then
        fail_check '--apply requires an active SSH session (SSH_CONNECTION is empty)'
    fi
    if [[ "$REMOVE_BROAD" == 'true' && "${STAGING_LOCKDOWN_CONFIRM:-}" != 'I_UNDERSTAND' ]]; then
        fail_check 'set STAGING_LOCKDOWN_CONFIRM=I_UNDERSTAND for --remove-broad'
    fi

    require_commands
    check_host_identity
    fetch_and_validate_ranges
    if [[ "$V4_COUNT" -gt 0 || "$V6_COUNT" -gt 0 ]]; then
        check_dns
    fi
    check_ufw
    check_caddy_and_services
    print_rule_plan

    if ! report_and_exit_if_failed; then
        exit 1
    fi
    if [[ "$MODE" == 'check' ]]; then
        printf '%s\n' 'no firewall changes were made'
        exit 0
    fi

    if ! snapshot_ufw; then
        printf '%s\n' 'unable to create the root-only UFW rollback snapshot; no rules were changed' >&2
        exit 1
    fi
    allow_cloudflare_ranges
    if ((${#ERRORS[@]} > 0)); then
        report_and_exit_if_failed || true
        exit 1
    fi
    public_readonly_checks
    if ((${#ERRORS[@]} > 0)); then
        report_and_exit_if_failed || true
        exit 1
    fi
    if [[ "$REMOVE_BROAD" == 'true' ]]; then
        remove_broad_web_rules
        if ((${#ERRORS[@]} > 0)); then
            report_and_exit_if_failed || true
            exit 1
        fi
        public_readonly_checks
        if ((${#ERRORS[@]} > 0)); then
            report_and_exit_if_failed || true
            exit 1
        fi
        printf '%s\n' 'broad_web_rules=removed'
    else
        printf '%s\n' 'broad_web_rules=retained; run the separately approved removal phase after external dual-stack checks'
    fi
    printf '%s\n' 'staging_origin_lockdown=applied'
}

main "$@"
