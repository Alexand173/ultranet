#!/usr/bin/python3
"""Create and validate the staging faucet's systemd IP destination policy.

This helper has three deliberately separate modes:

* --refresh: root-only, operator-controlled fetch of the published Cloudflare
  ranges and Turnstile endpoint addresses. It writes only non-secret policy
  material and requires explicit approval when ranges or resolver addresses
  change.
* --check: read-only validation used by the faucet unit's ExecCondition.
* --enforce: root-only expiry/effective-policy check that stops the staging
  faucet when the policy is missing, stale, malformed, or not loaded.

The policy is destination-IP based. It is not a hostname or port firewall.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import socket
import ssl
import stat
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

SERVICE = "ultranet-faucet-staging.service"
TURNSTILE_HOSTNAME = "challenges.cloudflare.com"
CLOUDFLARE_V4_URL = "https://www.cloudflare.com/ips-v4"
CLOUDFLARE_V6_URL = "https://www.cloudflare.com/ips-v6"
MAX_RESPONSE_BYTES = 128 * 1024
MIN_V4_RANGES = 10
MIN_V6_RANGES = 5
MAX_RANGES_PER_FAMILY = 128
POLICY_MAX_AGE_SECONDS = 26 * 60 * 60

EGRESS_DIR = Path("/etc/ultranet-faucet-staging/egress")
ROLLBACK_DIR = EGRESS_DIR / "rollback"
FAUCET_ENV_PATH = Path("/etc/ultranet-faucet-staging/faucet.env")
RESOLVER_ALLOWLIST = EGRESS_DIR / "resolvers.allow"
MANIFEST_PATH = EGRESS_DIR / "manifest.json"
DROPIN_DIR = Path("/etc/systemd/system/ultranet-faucet-staging.service.d")
DROPIN_PATH = DROPIN_DIR / "50-egress-allowlist.conf"


class PolicyError(RuntimeError):
    """A safe-to-display policy validation failure."""


def fail(message: str) -> None:
    raise PolicyError(message)


def utc_now() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


def isoformat(value: dt.datetime) -> str:
    return (
        value.astimezone(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def parse_timestamp(value: object, field: str) -> dt.datetime:
    if not isinstance(value, str):
        fail(f"manifest {field} is not a timestamp")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        fail(f"manifest {field} is invalid: {error}")
    if parsed.tzinfo is None:
        fail(f"manifest {field} has no timezone")
    return parsed.astimezone(dt.timezone.utc)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    try:
        return sha256_bytes(path.read_bytes())
    except OSError as error:
        fail(f"cannot hash {path}: {error}")


def require_sha256(value: object, field: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        fail(f"manifest {field} is not a SHA-256 digest")
    try:
        int(value, 16)
    except ValueError:
        fail(f"manifest {field} is not a SHA-256 digest")
    return value.lower()


def read_text(path: Path, label: str, max_bytes: int = MAX_RESPONSE_BYTES) -> str:
    try:
        value = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {label}: {error}")
    if len(value) > max_bytes:
        fail(f"{label} is too large")
    try:
        return value.decode("ascii")
    except UnicodeDecodeError:
        fail(f"{label} is not ASCII")


def non_comment_lines(value: str) -> list[str]:
    lines: list[str] = []
    for raw_line in value.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if line:
            lines.append(line)
    return lines


def validate_ranges(value: str, version: int) -> list[ipaddress._BaseNetwork]:
    lines = non_comment_lines(value)
    minimum = MIN_V4_RANGES if version == 4 else MIN_V6_RANGES
    if len(lines) < minimum:
        fail(f"Cloudflare IPv{version} list is unexpectedly small: {len(lines)}")
    if len(lines) > MAX_RANGES_PER_FAMILY:
        fail(f"Cloudflare IPv{version} list is unexpectedly large: {len(lines)}")

    networks: list[ipaddress._BaseNetwork] = []
    for line in lines:
        try:
            network = ipaddress.ip_network(line, strict=True)
        except ValueError as error:
            fail(f"invalid Cloudflare IPv{version} CIDR {line!r}: {error}")
        if network.version != version:
            fail(f"Cloudflare IPv{version} list contains another address family")
        if network.prefixlen == 0 or not network.is_global:
            fail(f"Cloudflare IPv{version} list contains a non-global/default range: {line}")
        networks.append(network)

    ordered = sorted(networks, key=lambda item: (int(item.network_address), item.prefixlen))
    for previous, current in zip(ordered, ordered[1:]):
        if previous.overlaps(current):
            fail(f"Cloudflare IPv{version} list contains overlapping ranges")
    return ordered


def parse_ip_lines(value: str, label: str) -> list[ipaddress._BaseAddress]:
    addresses: list[ipaddress._BaseAddress] = []
    for line in non_comment_lines(value):
        try:
            address = ipaddress.ip_address(line)
        except ValueError as error:
            fail(f"{label} contains an invalid address {line!r}: {error}")
        if address.is_unspecified or address.is_multicast:
            fail(f"{label} contains an unusable address: {line}")
        addresses.append(address)
    if not addresses:
        fail(f"{label} is empty")
    if len(set(addresses)) != len(addresses):
        fail(f"{label} contains duplicate addresses")
    return sorted(addresses, key=lambda item: (item.version, int(item)))


def configured_resolver_addresses() -> list[ipaddress._BaseAddress]:
    value = read_text(Path("/etc/resolv.conf"), "/etc/resolv.conf", 32 * 1024)
    addresses: list[ipaddress._BaseAddress] = []
    for raw_line in value.splitlines():
        fields = raw_line.split("#", 1)[0].split()
        if len(fields) >= 2 and fields[0] == "nameserver":
            addresses.extend(parse_ip_lines(fields[1], "/etc/resolv.conf nameserver"))
    if not addresses:
        fail("/etc/resolv.conf has no nameserver")
    return sorted(set(addresses), key=lambda item: (item.version, int(item)))


def approved_resolver_addresses() -> list[ipaddress._BaseAddress]:
    secure_directory(RESOLVER_ALLOWLIST.parent, "resolver policy directory")
    secure_regular_file(RESOLVER_ALLOWLIST, "resolver allowlist")
    return parse_ip_lines(
        read_text(RESOLVER_ALLOWLIST, "resolver allowlist", 16 * 1024),
        "resolver allowlist",
    )


def resolver_policy(approve_change: bool) -> tuple[list[ipaddress._BaseAddress], bytes]:
    configured = configured_resolver_addresses()
    if RESOLVER_ALLOWLIST.exists():
        approved = approved_resolver_addresses()
    else:
        approved = []
    if set(configured) != set(approved) and not approve_change:
        fail("resolver configuration changed or has not been approved")
    resolver_bytes = (
        "# Root-approved resolver destinations used by the staging egress policy.\n"
        + "\n".join(str(address) for address in configured)
        + "\n"
    ).encode("ascii")
    return configured, resolver_bytes


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):  # type: ignore[no-untyped-def]
        raise PolicyError("Cloudflare range endpoint attempted a redirect")


def fetch_official_ranges(url: str) -> bytes:
    parsed = urllib.parse.urlsplit(url)
    if (
        parsed.scheme != "https"
        or parsed.hostname != "www.cloudflare.com"
        or parsed.path not in {"/ips-v4", "/ips-v6"}
    ):
        fail("unexpected Cloudflare range URL")
    context = ssl.create_default_context()
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    opener = urllib.request.build_opener(
        NoRedirectHandler,
        urllib.request.HTTPSHandler(context=context),
    )
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "text/plain",
            "User-Agent": "UltraNet-staging-egress-refresh/1",
        },
        method="GET",
    )
    try:
        with opener.open(request, timeout=30) as response:
            if response.status != 200 or response.geturl() != url:
                fail("Cloudflare range endpoint returned an unexpected response")
            body = response.read(MAX_RESPONSE_BYTES + 1)
    except PolicyError:
        raise
    except (OSError, urllib.error.URLError) as error:
        fail(f"Cloudflare range fetch failed: {error}")
    if len(body) > MAX_RESPONSE_BYTES:
        fail("Cloudflare range response is too large")
    return body


def resolve_turnstile(
    ranges_v4: list[ipaddress._BaseNetwork],
    ranges_v6: list[ipaddress._BaseNetwork],
) -> list[ipaddress._BaseAddress]:
    try:
        records = socket.getaddrinfo(
            TURNSTILE_HOSTNAME,
            443,
            type=socket.SOCK_STREAM,
            proto=socket.IPPROTO_TCP,
        )
    except OSError as error:
        fail(f"cannot resolve {TURNSTILE_HOSTNAME}: {error}")
    addresses = sorted(
        {ipaddress.ip_address(record[4][0]) for record in records},
        key=lambda item: (item.version, int(item)),
    )
    if not addresses:
        fail(f"{TURNSTILE_HOSTNAME} returned no addresses")
    for address in addresses:
        ranges = ranges_v4 if address.version == 4 else ranges_v6
        if not any(address in network for network in ranges):
            fail(f"{TURNSTILE_HOSTNAME} resolved outside published Cloudflare ranges")
    return addresses


def address_cidr(address: ipaddress._BaseAddress) -> str:
    return f"{address}/{address.max_prefixlen}"


def make_allowlist(
    ranges_v4: list[ipaddress._BaseNetwork],
    ranges_v6: list[ipaddress._BaseNetwork],
    resolvers: list[ipaddress._BaseAddress],
) -> list[str]:
    values = {"127.0.0.0/8", "::1/128"}
    values.update(str(network) for network in ranges_v4)
    values.update(str(network) for network in ranges_v6)
    values.update(address_cidr(address) for address in resolvers)
    return sorted(
        values,
        key=lambda value: (
            ipaddress.ip_network(value, strict=False).version,
            int(ipaddress.ip_network(value, strict=False).network_address),
            value,
        ),
    )


def make_dropin(allowlist: list[str]) -> bytes:
    lines = [
        "# Generated by ultranet-faucet-staging-egress.py; do not edit by hand.",
        "# Destination-IP policy only; the faucet still requires exact HTTPS/TLS validation.",
        "[Service]",
    ]
    lines.extend(f"IPAddressAllow={value}" for value in allowlist)
    return ("\n".join(lines) + "\n").encode("ascii")


def render_ranges(networks: list[ipaddress._BaseNetwork]) -> bytes:
    return ("\n".join(str(network) for network in networks) + "\n").encode("ascii")


def parse_generated_dropin(value: str) -> list[str]:
    allowlist: list[str] = []
    section: str | None = None
    seen_service = False
    for raw_line in value.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line
            if section != "[Service]":
                fail("egress drop-in contains an unexpected section")
            if seen_service:
                fail("egress drop-in contains multiple Service sections")
            seen_service = True
            continue
        if section != "[Service]" or not line.startswith("IPAddressAllow="):
            fail("egress drop-in contains an unexpected directive")
        value = line.removeprefix("IPAddressAllow=")
        try:
            network = ipaddress.ip_network(value, strict=True)
        except ValueError as error:
            fail(f"egress drop-in contains an invalid destination range: {error}")
        if network.prefixlen == 0 and str(network) in {"0.0.0.0/0", "::/0"}:
            fail("egress drop-in must not allow the default route")
        allowlist.append(str(network))
    if not seen_service or not allowlist:
        fail("egress drop-in is missing its Service section or allowlist")
    if len(allowlist) != len(set(allowlist)):
        fail("egress drop-in contains duplicate destinations")
    return sorted(
        allowlist,
        key=lambda value: (
            ipaddress.ip_network(value).version,
            int(ipaddress.ip_network(value).network_address),
            value,
        ),
    )


def secure_directory(path: Path, label: str) -> None:
    try:
        info = path.lstat()
    except OSError as error:
        fail(f"cannot inspect {label}: {error}")
    if (
        not stat.S_ISDIR(info.st_mode)
        or info.st_uid != 0
        or info.st_gid != 0
        or info.st_mode & 0o022
    ):
        fail(f"{label} must be a root:root-owned directory without group/other write access")


def secure_regular_file(
    path: Path,
    label: str,
    required: bool = True,
    require_root_group: bool = True,
) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        if required:
            fail(f"{label} is missing")
        return
    except OSError as error:
        fail(f"cannot inspect {label}: {error}")
    if (
        not stat.S_ISREG(info.st_mode)
        or info.st_uid != 0
        or (require_root_group and info.st_gid != 0)
        or info.st_mode & 0o022
    ):
        ownership = "root:root-owned" if require_root_group else "root-owned"
        fail(f"{label} must be a {ownership} regular file without group/other write access")


def load_manifest() -> dict[str, object]:
    secure_directory(EGRESS_DIR, "egress directory")
    secure_regular_file(MANIFEST_PATH, "egress manifest")
    try:
        value = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"egress manifest is invalid: {error}")
    if not isinstance(value, dict):
        fail("egress manifest is not an object")
    return value


def require_disabled_environment() -> None:
    secure_regular_file(
        FAUCET_ENV_PATH,
        "faucet environment",
        require_root_group=False,
    )
    values = [
        line.strip().removeprefix("FAUCET_ENABLED=")
        for line in read_text(FAUCET_ENV_PATH, "faucet environment", 32 * 1024).splitlines()
        if line.strip().startswith("FAUCET_ENABLED=")
    ]
    if values != ["false"]:
        fail("faucet environment must explicitly keep FAUCET_ENABLED=false")


def manifest_addresses(value: object, field: str) -> set[ipaddress._BaseAddress]:
    if not isinstance(value, list) or not value or not all(isinstance(item, str) for item in value):
        fail(f"manifest {field} is invalid")
    parsed = parse_ip_lines("\n".join(value), field)
    if len(parsed) != len(value):
        fail(f"manifest {field} contains duplicate addresses")
    return set(parsed)


def validate_manifest_and_dropin(resolve_current_turnstile: bool = True) -> dict[str, object]:
    manifest = load_manifest()
    if manifest.get("schema") != 1 or manifest.get("service") != SERVICE:
        fail("egress manifest schema or service does not match")
    if manifest.get("turnstile_hostname") != TURNSTILE_HOSTNAME:
        fail("egress manifest hostname does not match")
    if manifest.get("cloudflare_v4_url") != CLOUDFLARE_V4_URL or manifest.get("cloudflare_v6_url") != CLOUDFLARE_V6_URL:
        fail("egress manifest source URLs do not match the approved endpoints")
    generated_at = parse_timestamp(manifest.get("generated_at"), "generated_at")
    expires_at = parse_timestamp(manifest.get("expires_at"), "expires_at")
    now = utc_now()
    if generated_at > now + dt.timedelta(minutes=5):
        fail("egress manifest was generated in the future")
    if expires_at <= now:
        fail("egress policy is expired")
    if expires_at > generated_at + dt.timedelta(seconds=POLICY_MAX_AGE_SECONDS):
        fail("egress manifest expiry exceeds the policy maximum")

    resolver_values = manifest_addresses(manifest.get("resolver_addresses"), "resolver_addresses")
    turnstile_values = manifest_addresses(
        manifest.get("turnstile_addresses"), "turnstile_addresses"
    )
    allowlist_value = manifest.get("allowlist")
    if (
        not isinstance(allowlist_value, list)
        or not allowlist_value
        or not all(isinstance(value, str) for value in allowlist_value)
    ):
        fail("egress manifest allowlist is invalid")
    try:
        expected = sorted(
            {str(ipaddress.ip_network(value, strict=True)) for value in allowlist_value},
            key=lambda value: (
                ipaddress.ip_network(value).version,
                int(ipaddress.ip_network(value).network_address),
                value,
            ),
        )
    except ValueError as error:
        fail(f"egress manifest allowlist contains an invalid range: {error}")
    if len(expected) != len(allowlist_value):
        fail("egress manifest allowlist contains duplicate destinations")
    if "127.0.0.0/8" not in expected or "::1/128" not in expected:
        fail("egress allowlist is missing loopback destinations")
    for resolver in resolver_values:
        if not any(
            resolver in ipaddress.ip_network(value, strict=True) for value in expected
        ):
            fail("egress allowlist is missing an approved resolver destination")
    for address in turnstile_values:
        if not any(
            address in ipaddress.ip_network(value, strict=True)
            for value in expected
        ):
            fail("egress allowlist is missing a Turnstile destination")
    cloudflare_v4_value = manifest.get("cloudflare_v4_ranges")
    cloudflare_v6_value = manifest.get("cloudflare_v6_ranges")
    if (
        not isinstance(cloudflare_v4_value, list)
        or not all(isinstance(value, str) for value in cloudflare_v4_value)
        or not isinstance(cloudflare_v6_value, list)
        or not all(isinstance(value, str) for value in cloudflare_v6_value)
    ):
        fail("egress manifest Cloudflare ranges are missing")
    try:
        manifest_v4 = validate_ranges("\n".join(cloudflare_v4_value), 4)
        manifest_v6 = validate_ranges("\n".join(cloudflare_v6_value), 6)
    except PolicyError as error:
        raise PolicyError(f"egress manifest contains invalid Cloudflare range material: {error}")
    require_sha256(manifest.get("cloudflare_v4_sha256"), "cloudflare_v4_sha256")
    require_sha256(manifest.get("cloudflare_v6_sha256"), "cloudflare_v6_sha256")
    if (
        manifest.get("cloudflare_v4_count") != len(manifest_v4)
        or manifest.get("cloudflare_v6_count") != len(manifest_v6)
        or require_sha256(
            manifest.get("cloudflare_v4_ranges_sha256"), "cloudflare_v4_ranges_sha256"
        )
        != sha256_bytes(render_ranges(manifest_v4))
        or require_sha256(
            manifest.get("cloudflare_v6_ranges_sha256"), "cloudflare_v6_ranges_sha256"
        )
        != sha256_bytes(render_ranges(manifest_v6))
    ):
        fail("egress manifest Cloudflare range metadata does not match the ranges")
    for address in turnstile_values:
        if not address.is_global:
            fail("egress manifest contains a non-global Turnstile destination")
        source_ranges = manifest_v4 if address.version == 4 else manifest_v6
        if not any(address in network for network in source_ranges):
            fail("egress manifest Turnstile destination is outside Cloudflare ranges")
    if resolve_current_turnstile:
        # Cloudflare anycast addresses may rotate. Require current resolution
        # to remain inside the validated published ranges rather than pinning
        # the service to the exact address set seen during refresh.
        resolve_turnstile(manifest_v4, manifest_v6)
    expected_from_sources = set(
        make_allowlist(manifest_v4, manifest_v6, list(resolver_values))
    )
    if set(expected) != expected_from_sources:
        fail("egress manifest allowlist contains unexpected destinations")
    resolver_bytes = render_resolver_allowlist(
        sorted(resolver_values, key=lambda item: (item.version, int(item)))
    )
    if manifest.get("resolver_allowlist_sha256") != sha256_bytes(resolver_bytes):
        fail("resolver allowlist checksum does not match the manifest")
    if sha256_file(RESOLVER_ALLOWLIST) != sha256_bytes(resolver_bytes):
        fail("resolver allowlist file does not match the manifest")

    secure_directory(DROPIN_DIR, "egress drop-in directory")
    dropin_text = read_text(DROPIN_PATH, "egress drop-in")
    actual = parse_generated_dropin(dropin_text)
    if actual != expected:
        fail("egress drop-in does not match the manifest allowlist")
    expected_digest = manifest.get("dropin_sha256")
    if not isinstance(expected_digest, str) or sha256_file(DROPIN_PATH) != expected_digest:
        fail("egress drop-in checksum does not match the manifest")
    if "127.0.0.0/8" not in actual or "::1/128" not in actual:
        fail("egress allowlist is missing loopback destinations")
    for resolver in resolver_values:
        if not any(
            resolver in ipaddress.ip_network(value, strict=True) for value in actual
        ):
            fail("egress allowlist is missing an approved resolver destination")
    return manifest


def effective_systemd_policy() -> tuple[set[str], set[str]]:
    def show(property_name: str) -> set[str]:
        try:
            result = subprocess.run(
                ["systemctl", "show", SERVICE, "-p", property_name, "--value"],
                check=True,
                capture_output=True,
                text=True,
                timeout=10,
            )
        except (OSError, subprocess.SubprocessError) as error:
            fail(f"cannot inspect effective systemd {property_name}: {error}")
        return set(result.stdout.split())

    return show("IPAddressAllow"), show("IPAddressDeny")


def check_policy(
    effective: bool = False, resolve_current_turnstile: bool = True
) -> dict[str, object]:
    require_disabled_environment()
    manifest = validate_manifest_and_dropin(
        resolve_current_turnstile=resolve_current_turnstile
    )
    current_resolvers = set(configured_resolver_addresses())
    expected_resolvers = manifest_addresses(
        manifest.get("resolver_addresses"), "resolver_addresses"
    )
    if current_resolvers != expected_resolvers:
        fail("/etc/resolv.conf no longer matches the approved egress manifest")
    if set(approved_resolver_addresses()) != current_resolvers:
        fail("resolver allowlist no longer matches /etc/resolv.conf")
    if effective:
        actual_allow, actual_deny = effective_systemd_policy()
        expected_allow = {str(value) for value in manifest["allowlist"]}
        if actual_allow != expected_allow:
            fail("effective systemd IPAddressAllow does not match the manifest")
        if "any" not in actual_deny and not {"0.0.0.0/0", "::/0"}.issubset(actual_deny):
            fail("effective systemd IPAddressDeny is not deny-all")
    return manifest


def atomic_write(path: Path, data: bytes, mode: int) -> None:
    path.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    try:
        os.chmod(path.parent, 0o755)
        os.chown(path.parent, 0, 0)
    except OSError as error:
        fail(f"cannot secure policy directory {path.parent}: {error}")
    secure_directory(path.parent, f"policy directory {path.parent}")
    temporary_name: str | None = None
    try:
        descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
        os.fchown(descriptor, 0, 0)
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary_name, path)
        temporary_name = None
        directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    except OSError as error:
        fail(f"cannot atomically write {path}: {error}")
    finally:
        if temporary_name:
            try:
                os.unlink(temporary_name)
            except OSError:
                pass


def service_is_active() -> bool:
    try:
        result = subprocess.run(
            ["systemctl", "is-active", "--quiet", SERVICE],
            check=False,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as error:
        fail(f"cannot inspect {SERVICE} state: {error}")
    return result.returncode == 0


def backup_current_policy() -> None:
    existing = [
        path
        for path in (RESOLVER_ALLOWLIST, DROPIN_PATH, MANIFEST_PATH)
        if path.exists()
    ]
    if not existing:
        return
    backup_dir = ROLLBACK_DIR / utc_now().strftime("%Y%m%dT%H%M%SZ")
    backup_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.chmod(backup_dir, 0o700)
    os.chown(backup_dir, 0, 0)
    for source in existing:
        secure_regular_file(source, f"current {source.name}")
        try:
            data = source.read_bytes()
        except OSError as error:
            fail(f"cannot read current {source.name} for rollback: {error}")
        atomic_write(backup_dir / source.name, data, 0o600)


def refresh(approve_range_change: bool, approve_resolver_change: bool) -> None:
    if os.geteuid() != 0:
        fail("--refresh must run as root")
    require_disabled_environment()
    if service_is_active():
        fail("stop the staging faucet while replacing its egress policy")

    resolvers, resolver_bytes = resolver_policy(approve_resolver_change)
    body_v4 = fetch_official_ranges(CLOUDFLARE_V4_URL)
    body_v6 = fetch_official_ranges(CLOUDFLARE_V6_URL)
    ranges_v4 = validate_ranges(body_v4.decode("ascii"), 4)
    ranges_v6 = validate_ranges(body_v6.decode("ascii"), 6)
    turnstile = resolve_turnstile(ranges_v4, ranges_v6)
    allowlist = make_allowlist(ranges_v4, ranges_v6, resolvers)
    dropin = make_dropin(allowlist)
    now = utc_now()

    if MANIFEST_PATH.exists():
        previous = load_manifest()
        old_v4 = previous.get("cloudflare_v4_sha256")
        old_v6 = previous.get("cloudflare_v6_sha256")
        if (
            old_v4 != sha256_bytes(body_v4)
            or old_v6 != sha256_bytes(body_v6)
        ) and not approve_range_change:
            fail("published Cloudflare ranges changed; rerun with explicit range-change approval")
    elif not approve_range_change:
        fail("initial egress policy requires explicit range-change approval")

    manifest: dict[str, object] = {
        "schema": 1,
        "service": SERVICE,
        "turnstile_hostname": TURNSTILE_HOSTNAME,
        "generated_at": isoformat(now),
        "expires_at": isoformat(now + dt.timedelta(seconds=POLICY_MAX_AGE_SECONDS)),
        "cloudflare_v4_url": CLOUDFLARE_V4_URL,
        "cloudflare_v6_url": CLOUDFLARE_V6_URL,
        "cloudflare_v4_sha256": sha256_bytes(body_v4),
        "cloudflare_v6_sha256": sha256_bytes(body_v6),
        "cloudflare_v4_count": len(ranges_v4),
        "cloudflare_v6_count": len(ranges_v6),
        "cloudflare_v4_ranges": [str(network) for network in ranges_v4],
        "cloudflare_v6_ranges": [str(network) for network in ranges_v6],
        "cloudflare_v4_ranges_sha256": sha256_bytes(render_ranges(ranges_v4)),
        "cloudflare_v6_ranges_sha256": sha256_bytes(render_ranges(ranges_v6)),
        "resolver_source": "/etc/resolv.conf",
        "resolver_addresses": [str(address) for address in resolvers],
        "resolver_allowlist_sha256": sha256_bytes(resolver_bytes),
        "turnstile_addresses": [str(address) for address in turnstile],
        "allowlist": allowlist,
        "dropin_sha256": sha256_bytes(dropin),
    }
    manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")

    DROPIN_DIR.mkdir(mode=0o755, parents=True, exist_ok=True)
    os.chmod(DROPIN_DIR, 0o755)
    # The service is stopped before refresh. Preserve a root-only rollback copy
    # before replacing any live policy material. A partial update fails the
    # service ExecCondition and leaves the faucet stopped.
    backup_current_policy()
    atomic_write(RESOLVER_ALLOWLIST, resolver_bytes, 0o644)
    atomic_write(DROPIN_PATH, dropin, 0o644)
    atomic_write(MANIFEST_PATH, manifest_bytes, 0o644)
    print(
        "egress_policy_refreshed "
        f"expires={manifest['expires_at']} "
        f"ranges_v4={len(ranges_v4)} ranges_v6={len(ranges_v6)} "
        f"resolvers={len(resolvers)} turnstile_addresses={len(turnstile)}"
    )


def enforce() -> None:
    if os.geteuid() != 0:
        fail("--enforce must run as root")
    try:
        check_policy(effective=True, resolve_current_turnstile=False)
    except PolicyError as error:
        result = subprocess.run(
            ["systemctl", "stop", "--no-block", SERVICE],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            print("egress policy invalid; stop command did not complete", file=sys.stderr)
        raise error
    print("egress_policy_effective=passed")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--refresh", action="store_true")
    modes.add_argument("--check", action="store_true")
    modes.add_argument("--enforce", action="store_true")
    parser.add_argument("--effective", action="store_true")
    parser.add_argument("--approve-range-change", action="store_true")
    parser.add_argument("--approve-resolver-change", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.refresh and (args.approve_range_change or args.approve_resolver_change):
        print("approval flags require --refresh", file=sys.stderr)
        return 2
    if args.effective and not args.check:
        print("--effective requires --check", file=sys.stderr)
        return 2
    try:
        if args.refresh:
            refresh(args.approve_range_change, args.approve_resolver_change)
        elif args.check:
            check_policy(effective=args.effective)
            print("egress_policy=passed")
        else:
            enforce()
    except PolicyError as error:
        print(f"ultranet-faucet-staging-egress: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
