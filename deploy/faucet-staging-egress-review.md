# Staging faucet egress installation review

This document prepares the **review-only** installation block for the staging faucet destination policy. It has not been executed against any VPS. Do not execute it from this task.

The block must remain deferred until all of these are independently recorded:

- the SSH session is confirmed to be the separately provisioned staging VPS, not `167.233.161.115`;
- `faucet-staging.ultranetwork.cc` has an approved proxied DNS record and resolves through the intended public edge;
- the separate `caddy-staging.service` and `/etc/caddy/Caddyfile.staging` have been provisioned and reviewed;
- the source revision and non-secret deployment artifacts have an approved checksum record;
- a private change record authorizes the egress policy installation and identifies the range/resolver approval type.

The block does not change DNS, Cloudflare, UFW, production Caddy, production systemd units, credentials, faucet intake, or claim state. It intentionally performs no faucet start/restart. A later controlled restart is required only after `--check --effective` passes and `FAUCET_ENABLED=false` is re-verified.

## Deferred review-only command block

The following is a prepared command block, not an instruction to run now. It requires explicit private values and fails closed if the DNS/Caddy/source gates are not satisfied. Run it only from a root-authorized maintenance shell on the identified staging VPS after the prerequisites above are approved.

```bash
#!/usr/bin/env bash
set -euo pipefail

# Required private maintenance-shell values. Do not place secrets here.
: "${STAGING_SOURCE_DIR:?Set the approved clean staging checkout path}"
: "${STAGING_REVISION:?Set the approved 40-character source revision}"
: "${STAGING_VPS_IP:?Set the approved staging VPS IPv4 address}"
: "${STAGING_HOST_IDENTITY:?Set the approved staging hostname/host identity}"
: "${EGRESS_REFRESH_APPROVALS:?Set initial, range, resolver, or both from the private change record}"

readonly PRODUCTION_IP="167.233.161.115"
readonly STAGING_HOSTNAME="faucet-staging.ultranetwork.cc"
readonly STAGING_CADDY_SERVICE="caddy-staging.service"
readonly FAUCET_SERVICE="ultranet-faucet-staging.service"
readonly EGRESS_HELPER="/usr/local/libexec/ultranet-faucet-staging-egress.py"

# Host identity and origin-separation guards.
test "$(hostnamectl --static)" = "$STAGING_HOST_IDENTITY"
global_ipv4s="$(ip -4 -o addr show scope global | awk '{sub(/\/.*/, "", $4); print $4}')"
grep -Fxq "$STAGING_VPS_IP" <<<"$global_ipv4s"
! grep -Fxq "$PRODUCTION_IP" <<<"$global_ipv4s"

# DNS must be provisioned before this block is considered applicable. This is
# only a guard; the full public DNS/TLS matrix remains a separate read-only gate.
staging_addresses="$(getent ahosts "$STAGING_HOSTNAME" 2>/dev/null | awk '{print $1}' | sort -u)"
test -n "$staging_addresses"
! grep -Fxq "$PRODUCTION_IP" <<<"$staging_addresses"
! grep -Fxq "$STAGING_VPS_IP" <<<"$staging_addresses"

# Caddy must be separate and reviewable. Validate, but never reload it here.
test -f /etc/caddy/Caddyfile.staging
sudo systemctl cat "$STAGING_CADDY_SERVICE" >/dev/null
! test "$(sudo systemctl show "$STAGING_CADDY_SERVICE" -p FragmentPath --value)" = "/etc/systemd/system/caddy.service"
sudo systemctl show "$STAGING_CADDY_SERVICE" -p FragmentPath -p ExecStart
sudo caddy validate --config /etc/caddy/Caddyfile.staging --adapter caddyfile

# Source and artifact identity. No uncommitted or untracked deployment files.
test "$(git -C "$STAGING_SOURCE_DIR" rev-parse HEAD)" = "$STAGING_REVISION"
test -z "$(git -C "$STAGING_SOURCE_DIR" status --porcelain --untracked-files=all)"
for artifact in \
  deploy/ultranet-faucet-staging.service \
  deploy/ultranet-faucet-staging-egress.py \
  deploy/ultranet-faucet-staging-egress-expiry.service \
  deploy/ultranet-faucet-staging-egress-expiry.timer; do
  test -f "$STAGING_SOURCE_DIR/$artifact"
done
sha256sum \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging.service" \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress.py" \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.service" \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.timer"

# Install only the reviewed non-secret helper, units, and base faucet unit.
sudo install -d -o root -g root -m 0755 /usr/local/libexec
sudo install -o root -g root -m 0755 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress.py" \
  "$EGRESS_HELPER"
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging.service" \
  /etc/systemd/system/ultranet-faucet-staging.service
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.service" \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.service
sudo install -o root -g root -m 0644 \
  "$STAGING_SOURCE_DIR/deploy/ultranet-faucet-staging-egress-expiry.timer" \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.timer
sudo install -d -o root -g root -m 0755 \
  /etc/ultranet-faucet-staging/egress

# The helper refuses to replace policy while the faucet is active. Keep the
# environment disabled and select only the approval type recorded privately.
sudo systemctl stop "$FAUCET_SERVICE"
sudo grep -Eq '^FAUCET_ENABLED=false$' \
  /etc/ultranet-faucet-staging/faucet.env
case "$EGRESS_REFRESH_APPROVALS" in
  initial)
    refresh_flags=(--approve-range-change --approve-resolver-change)
    ;;
  range)
    refresh_flags=(--approve-range-change)
    ;;
  resolver)
    refresh_flags=(--approve-resolver-change)
    ;;
  both)
    refresh_flags=(--approve-range-change --approve-resolver-change)
    ;;
  *)
    printf '%s\n' 'EGRESS_REFRESH_APPROVALS must be initial, range, resolver, or both' >&2
    exit 1
    ;;
esac
sudo "$EGRESS_HELPER" --refresh "${refresh_flags[@]}"

# Verify all units before loading the drop-in. Do not start or restart the
# faucet in this block.
sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.service \
  /etc/systemd/system/ultranet-faucet-staging-egress-expiry.timer
sudo systemctl daemon-reload
sudo "$EGRESS_HELPER" --check --effective
sudo systemctl show "$FAUCET_SERVICE" \
  -p FragmentPath -p IPAddressDeny -p IPAddressAllow \
  -p RestrictAddressFamilies -p User -p Group
sudo stat -c '%n %U:%G %a' \
  /usr/local/libexec/ultranet-faucet-staging-egress.py \
  /etc/ultranet-faucet-staging/egress/resolvers.allow \
  /etc/ultranet-faucet-staging/egress/manifest.json \
  /etc/systemd/system/ultranet-faucet-staging.service.d/50-egress-allowlist.conf

# Start only the expiry monitor. The faucet remains stopped until the separate
# disabled-only preflight authorizes a controlled restart.
sudo systemctl enable --now ultranet-faucet-staging-egress-expiry.timer
if sudo systemctl is-active --quiet "$FAUCET_SERVICE"; then
  printf '%s\n' 'faucet unexpectedly active after policy installation' >&2
  exit 1
fi
```

The later controlled preflight must additionally inspect the parent slice's effective network policy, perform a denied-destination negative test, confirm loopback node access, and record the active manifest/drop-in checksums. It must not enable intake or submit a claim.

## Egress architecture decision

### Systemd IP-only policy — current staging implementation

**Advantages**

- Uses the existing service boundary and requires no new network daemon or application protocol.
- Keeps the faucet process unable to reach arbitrary public destinations by default through `IPAddressDeny=any`.
- Preserves the existing HTTPS client and certificate validation path.
- Has a small review surface: generated `IPAddressAllow` entries, recorded file checksums, freshness guard, and expiry monitor.
- Fails closed when the manifest, resolver approval, drop-in, or effective systemd properties are stale or missing.

**Limitations**

- Filtering is destination-IP based. It cannot enforce `challenges.cloudflare.com` as a hostname or TCP port `443`.
- Published Cloudflare ranges are broad, unsigned text fetched over HTTPS; any service on an allowed Cloudflare address and any allowed port remains reachable at the network layer.
- A compromised faucet process could attempt another destination inside the allowed ranges.
- Resolver changes and Cloudflare range changes require an operator-approved refresh and controlled restart; no automatic refresh/restart is allowed while intake is enabled.
- The policy depends on the effective parent-slice systemd policy and correct resolver configuration.

This is acceptable only as an explicitly recorded residual risk for a disabled-only staging preflight or canary. It must not be described as exact hostname/port containment.

### Dedicated local proxy — stronger alternative

A dedicated proxy would expose only a loopback listener or Unix socket to the faucet. The faucet would have no direct external network access; the proxy would make the outbound HTTPS request after enforcing the exact upstream hostname, path, and port (`challenges.cloudflare.com:443`), with strict TLS/SNI validation and a separate egress policy.

**Advantages**

- Provides an application-layer destination contract instead of relying only on IP ranges.
- Can reject arbitrary CONNECT targets, non-HTTPS ports, unexpected SNI, and non-approved paths before any external connection.
- Creates a single place for bounded timeouts, DNS provenance, redacted audit events, and upstream failure handling.
- Reduces the impact of a compromised faucet process because the process only reaches the local proxy.

**Costs and risks**

- Adds a new privileged or separately isolated daemon, unit, socket/port, configuration, health checks, logs, restart policy, and rollback path.
- The proxy itself still needs a reviewed Cloudflare-range or firewall policy and must be prevented from becoming a generic open proxy.
- The current faucet `reqwest` client uses the fixed Siteverify URL but does not explicitly configure a local proxy; adopting this design requires explicit, tested proxy wiring and a proof that direct connections are impossible.
- Proxy configuration, DNS resolution, TLS verification, buffering, body limits, and secret-bearing form handling create additional security-sensitive code paths.
- A proxy outage becomes a new availability dependency and must fail closed without queue or retry amplification.

### Recommendation

Keep the checked-in systemd policy as the reviewable staging baseline, but do not accept it as exact egress containment. Before a real-value canary, choose one of these explicitly:

1. record acceptance of the systemd residual risk and complete the denied-destination, freshness, resolver, and effective-policy evidence; or
2. stop and design the dedicated local proxy if exact `challenges.cloudflare.com:443` containment is a hard requirement.

DNS and separate Caddy provisioning are prerequisites for the next operational step. No VPS installation, policy refresh, systemd reload, Caddy validation/reload, or public request was performed while preparing this document.
