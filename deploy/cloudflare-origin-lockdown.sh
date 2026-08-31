#!/usr/bin/env bash
set -euo pipefail

# Stage or apply the VPS web-origin allowlist for Cloudflare-proxied hosts.
# This script never changes Cloudflare DNS/WAF configuration. Proxy both
# api.ultranetwork.cc and faucet.ultranetwork.cc in Cloudflare first.

readonly CLOUDFLARE_IPV4_URL='https://www.cloudflare.com/ips-v4'
readonly CLOUDFLARE_IPV6_URL='https://www.cloudflare.com/ips-v6'
readonly API_HOST='api.ultranetwork.cc'
readonly FAUCET_HOST='faucet.ultranetwork.cc'
readonly BACKUP_ROOT='/var/backups/ultranet/cloudflare-origin-lockdown'

usage() {
    cat <<'USAGE'
Usage:
  cloudflare-origin-lockdown.sh --check
  cloudflare-origin-lockdown.sh --apply
  CLOUDFLARE_LOCKDOWN_CONFIRM=I_UNDERSTAND \
    cloudflare-origin-lockdown.sh --apply --remove-broad

Modes:
  --check          Fetch and validate Cloudflare ranges and verify both DNS
                   names resolve to Cloudflare addresses. No firewall changes.
  --apply          Add Cloudflare-only TCP 80/443 rules, preserving existing
                   broad rules unless --remove-broad is also supplied.
  --remove-broad  With --apply, remove the broad Anywhere 80/443 rules only
                   after Cloudflare rules and public HTTPS checks pass.

The script requires an active SSH session for --apply. It does not modify DNS,
Cloudflare WAF rules, Caddy, SSH, or P2P rules. Review the saved UFW snapshot
before removing any broad rule.
USAGE
}

die() {
    printf 'cloudflare-origin-lockdown: %s\n' "$*" >&2
    exit 1
}

mode='check'
remove_broad='false'
while (($# > 0)); do
    case "$1" in
        --check)
            mode='check'
            ;;
        --apply)
            mode='apply'
            ;;
        --remove-broad)
            remove_broad='true'
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            die "unknown argument: $1"
            ;;
    esac
    shift
done

if [[ "$remove_broad" == 'true' && "$mode" != 'apply' ]]; then
    die '--remove-broad requires --apply'
fi
if [[ "$mode" == 'apply' && "$EUID" -ne 0 ]]; then
    die '--apply must run as root'
fi
if [[ "$mode" == 'apply' && -z "${SSH_CONNECTION:-}" ]]; then
    die '--apply must run from an active SSH session with a separate recovery path'
fi
if [[ "$remove_broad" == 'true' && "${CLOUDFLARE_LOCKDOWN_CONFIRM:-}" != 'I_UNDERSTAND' ]]; then
    die 'set CLOUDFLARE_LOCKDOWN_CONFIRM=I_UNDERSTAND before --remove-broad'
fi

for command in curl python3 getent; do
    command -v "$command" >/dev/null 2>&1 || die "required command is missing: $command"
done
if [[ "$mode" == 'apply' ]]; then
    command -v ufw >/dev/null 2>&1 || die 'required command is missing: ufw'
    ufw status | grep -Fq 'Status: active' || die 'ufw must already be active before applying web rules'
fi

workdir=$(mktemp -d)
cleanup() {
    chmod -R u+rwX,go-rwx "$workdir"
    find "$workdir" -type f -exec rm -f -- {} +
    rmdir "$workdir" 2>/dev/null || true
}
trap cleanup EXIT
ipv4_file="$workdir/ips-v4"
ipv6_file="$workdir/ips-v6"

curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --connect-timeout 10 --max-time 30 "$CLOUDFLARE_IPV4_URL" -o "$ipv4_file"
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --connect-timeout 10 --max-time 30 "$CLOUDFLARE_IPV6_URL" -o "$ipv6_file"

validate_ranges() {
    local file=$1
    local expected_version=$2
    python3 - "$file" "$expected_version" <<'PY'
import ipaddress
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
expected_version = int(sys.argv[2])
lines = [line.strip() for line in path.read_text(encoding="ascii").splitlines() if line.strip()]
if not lines:
    raise SystemExit(f"{path} is empty")
networks = []
for line in lines:
    try:
        network = ipaddress.ip_network(line, strict=False)
    except ValueError as error:
        raise SystemExit(f"invalid CIDR in {path}: {line}: {error}")
    if network.version != expected_version:
        raise SystemExit(f"wrong address family in {path}: {line}")
    networks.append(network)
if len(networks) < (10 if expected_version == 4 else 5):
    raise SystemExit(f"unexpectedly small Cloudflare {expected_version} list: {len(networks)}")
print(len(networks))
PY
}

ipv4_count=$(validate_ranges "$ipv4_file" 4)
ipv6_count=$(validate_ranges "$ipv6_file" 6)
printf 'validated Cloudflare ranges: ipv4=%s ipv6=%s\n' "$ipv4_count" "$ipv6_count"

assert_proxied_dns() {
    local host=$1
    local addresses
    addresses=$(getent ahosts "$host" | awk '{print $1}' | sort -u || true)
    [[ -n "$addresses" ]] || die "$host does not resolve through the local resolver"
    HOST_ADDRESSES="$addresses" python3 - "$ipv4_file" "$ipv6_file" "$host" <<'PY'
import ipaddress
import os
import pathlib
import sys

v4 = [ipaddress.ip_network(line.strip()) for line in pathlib.Path(sys.argv[1]).read_text().splitlines() if line.strip()]
v6 = [ipaddress.ip_network(line.strip()) for line in pathlib.Path(sys.argv[2]).read_text().splitlines() if line.strip()]
addresses = [ipaddress.ip_address(value) for value in os.environ["HOST_ADDRESSES"].splitlines()]
for address in addresses:
    ranges = v4 if address.version == 4 else v6
    if not any(address in network for network in ranges):
        raise SystemExit(f"{sys.argv[3]} resolves to non-Cloudflare address {address}")
print(f"{sys.argv[3]}: {', '.join(map(str, addresses))}")
PY
}

assert_proxied_dns "$API_HOST"
assert_proxied_dns "$FAUCET_HOST"

if [[ "$mode" == 'check' ]]; then
    printf 'check complete: both public origins resolve only to Cloudflare ranges\n'
    exit 0
fi

stamp=$(date -u +%Y%m%dT%H%M%SZ)
snapshot_dir="$BACKUP_ROOT/$stamp"
install -d -o root -g root -m 0700 "$snapshot_dir"
install -o root -g root -m 0644 "$ipv4_file" "$snapshot_dir/ips-v4"
install -o root -g root -m 0644 "$ipv6_file" "$snapshot_dir/ips-v6"
ufw status verbose >"$snapshot_dir/ufw-before.txt"
ufw show raw >"$snapshot_dir/ufw-before.raw"

allow_cloudflare_rule() {
    local cidr=$1
    local port=$2
    if ! ufw status | grep -F "$cidr" | grep -Fq "${port}/tcp"; then
        ufw allow from "$cidr" to any port "$port" proto tcp comment 'Cloudflare web origin'
    fi
}

while IFS= read -r cidr || [[ -n "$cidr" ]]; do
    allow_cloudflare_rule "$cidr" 80
done <"$ipv4_file"
while IFS= read -r cidr || [[ -n "$cidr" ]]; do
    allow_cloudflare_rule "$cidr" 80
done <"$ipv6_file"
while IFS= read -r cidr || [[ -n "$cidr" ]]; do
    allow_cloudflare_rule "$cidr" 443
done <"$ipv4_file"
while IFS= read -r cidr || [[ -n "$cidr" ]]; do
    allow_cloudflare_rule "$cidr" 443
done <"$ipv6_file"

# Verify the public proxies before any destructive removal. These are
# read-only endpoints and must not submit claims or touch operator routes.
curl --fail --silent --show-error --connect-timeout 10 --max-time 20 \
    "https://$API_HOST/api/validate" >/dev/null
curl --fail --silent --show-error --connect-timeout 10 --max-time 20 \
    "https://$FAUCET_HOST/api/faucet/status" >/dev/null

if [[ "$remove_broad" == 'true' ]]; then
    ufw delete allow 80/tcp || true
    ufw delete allow 443/tcp || true
    if ufw status | awk '$1 == "80/tcp" && $2 == "ALLOW" && $3 == "IN" && $4 == "Anywhere" { found = 1 } END { exit found ? 0 : 1 }'; then
        die 'broad TCP 80 IPv4 rule is still present after removal'
    fi
    if ufw status | awk '$1 == "443/tcp" && $2 == "ALLOW" && $3 == "IN" && $4 == "Anywhere" { found = 1 } END { exit found ? 0 : 1 }'; then
        die 'broad TCP 443 IPv4 rule is still present after removal'
    fi
    if ufw status | awk '$1 == "80/tcp" && $2 == "(v6)" && $3 == "ALLOW" && $4 == "IN" && $5 == "Anywhere" && $6 == "(v6)" { found = 1 } END { exit found ? 0 : 1 }'; then
        die 'broad TCP 80 IPv6 rule is still present after removal'
    fi
    if ufw status | awk '$1 == "443/tcp" && $2 == "(v6)" && $3 == "ALLOW" && $4 == "IN" && $5 == "Anywhere" && $6 == "(v6)" { found = 1 } END { exit found ? 0 : 1 }'; then
        die 'broad TCP 443 IPv6 rule is still present after removal'
    fi
fi

ufw status verbose >"$snapshot_dir/ufw-after.txt"
printf 'applied Cloudflare web allowlist; snapshot=%s\n' "$snapshot_dir"
if [[ "$remove_broad" == 'true' ]]; then
    printf 'removed broad TCP 80/443 rules after public HTTPS checks\n'
else
    printf 'broad TCP 80/443 rules retained; remove only after final review\n'
fi
