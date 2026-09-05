# Staging UFW activation and SSH preservation runbook

This document records the initial firewall/network state of the isolated staging VPS and defines the exact UFW sequence for a later approved maintenance window. It is a procedure and evidence record; it does not authorize or perform UFW activation.

## Scope and safety boundary

- Host: `ubuntu-2gb-nbg1-2`
- Staging IPv4: `2.28.61.71`
- Staging IPv6: `2a01:4f8:1c16:797e::1`
- Production IPv4 that must remain absent: `167.233.161.115`
- Snapshot time: `2026-09-04T19:59:30Z`
- Snapshot path: `/var/backups/ultranet/faucet-staging-origin-lockdown/initial-20260904T195930Z`
- Snapshot ownership/mode: `root:root`, `0700`; files are `0600`
- Snapshot contents: 29 files, including UFW status/raw output, UFW source rule files, iptables-save, nftables, routes, addresses, listeners, service state, and `SHA256SUMS`
- SSH access documentation reviewed: [`faucet-staging-origin-lockdown-review.md`](./faucet-staging-origin-lockdown-review.md)
- Staging firewall procedure: [`faucet-staging-origin-lockdown.sh`](./faucet-staging-origin-lockdown.sh)
- Approved operator SSH IPv4 source: `109.245.57.92/32`
- Approved operator SSH IPv6 source: none; provider-console recovery only
- Operator-provided Cloudflare web ranges are recorded below for review; the apply procedure must still fetch and validate the official live lists

The engineer has confirmed that an independent recovery path is prepared. The provider console/recovery path was not exercised by this read-only audit. Do not activate UFW until that path is available in the same maintenance window.

Do not paste SSH keys, passwords, tokens, provider credentials, or recovery secrets into this repository or an evidence record.

## SSH identity review

The host reported these public SSH host-key fingerprints. Compare the trusted provider/known-host record before making a firewall change; a fingerprint mismatch is an immediate stop condition:

```text
ED25519 SHA256:XQpLaYEuOEEPC1nhntvP2MAGVtWzUJPvY2vixkzN0pc
ECDSA   SHA256:e8BX7ivk9IchromL0/2GT2I7MynTPlALpYTudWoiWp
RSA     SHA256:3JhuCCw+tMHzM/r/JDmtbITxA4EQfqcGpHvmHDE6eRo
```

Effective SSH settings at snapshot time:

```text
listenaddress 0.0.0.0:22
listenaddress [::]:22
port 22
permitrootlogin without-password
pubkeyauthentication yes
passwordauthentication yes
kbdinteractiveauthentication no
```

`passwordauthentication yes` is a separate SSH-hardening issue. It was not changed during this UFW task. Review and disable password authentication only as a separate, key-verified change with its own recovery test.

## Initial network/firewall snapshot

### UFW

```text
Status: inactive
Added user rules: (None)
IPV6=yes
DEFAULT_INPUT_POLICY="DROP"
DEFAULT_OUTPUT_POLICY="ACCEPT"
DEFAULT_FORWARD_POLICY="DROP"
```

### Netfilter backends

The live IPv4 and IPv6 filter tables were empty and currently had accept policies because UFW was inactive:

```text
IPv4 INPUT   ACCEPT [0:0]
IPv4 FORWARD ACCEPT [0:0]
IPv4 OUTPUT  ACCEPT [0:0]
IPv6 INPUT   ACCEPT [0:0]
IPv6 FORWARD ACCEPT [0:0]
IPv6 OUTPUT  ACCEPT [0:0]
```

The nftables listing contained only empty `ip filter` and `ip6 filter` tables. No UFW user rule was active.

### Addresses and routes

```text
IPv4: 2.28.61.71/32 on eth0
IPv6: 2a01:4f8:1c16:797e::1/64 on eth0
IPv4 default: via 172.31.1.1 dev eth0
IPv6 default: via fe80::1 dev eth0
IPv4 resolver routes: 185.12.64.1 and 185.12.64.2 via 172.31.1.1
```

### Listening sockets

```text
Public TCP 22:   0.0.0.0:22 and [::]:22, sshd
Public TCP 80:   *:80, staging caddy
Public TCP 443:  *:443, staging caddy
Loopback 19000:  127.0.0.1:19000, UltraNet
Loopback 8081:   127.0.0.1:8081, UltraNet API
Loopback 8090:   127.0.0.1:8090, staging faucet
Loopback 2019:   127.0.0.1:2019, Caddy admin
Loopback DNS:   127.0.0.53:53 / 127.0.0.54:53, systemd-resolved
```

There is no public P2P listener in this snapshot. The live node is loopback-only on `127.0.0.1:19000`. Do not add TCP 9000 or TCP 19000 as part of the web-origin firewall task. A future public P2P topology requires a separate listener, peer, and firewall review.

## Reviewed Cloudflare web ranges

The operator-provided lists match the expected Cloudflare source material shown for the staging web policy. They are applicable only to TCP 80/443, never to SSH. The checked-in procedure remains authoritative: it fetches the official endpoints over HTTPS, validates family/count/global-CIDR/overlap constraints, and refuses stale or malformed input.

IPv4 ranges reviewed (15):

```text
173.245.48.0/20
103.21.244.0/22
103.22.200.0/22
103.31.4.0/22
141.101.64.0/18
108.162.192.0/18
190.93.240.0/20
188.114.96.0/20
197.234.240.0/22
198.41.128.0/17
162.158.0.0/15
104.16.0.0/13
104.24.0.0/14
172.64.0.0/13
131.0.72.0/22
```

IPv6 ranges reviewed (7):

```text
2400:cb00::/32
2606:4700::/32
2803:f800::/32
2405:b500::/32
2405:8100::/32
2a06:98c0::/29
2c0f:f248::/32
```

Do not manually install these copied lists with ad hoc UFW commands. Run `faucet-staging-origin-lockdown.sh --check` and let the reviewed script fetch the current lists and print the exact planned rule set.

## Approved UFW configuration sequence

Run the following only from the active root SSH session after confirming the host identity and the independent recovery path. The approved operator SSH source is the non-secret IPv4 CIDR `109.245.57.92/32`; no operator IPv6 source was supplied, so IPv6 SSH remains intentionally closed and the provider console is the IPv6 recovery path. Keep shell-only connection values private; the approved CIDR itself is non-secret documentation.

### 0. Set and validate the approved SSH source

Do not infer an allowlist from a random current address and do not use an unrestricted `ufw allow 22/tcp` rule. The only approved operator SSH source currently supplied is the exact IPv4 `/32` below.

```bash
set -euo pipefail

SSH_V4_CIDR='109.245.57.92/32'
SSH_V6_CIDR=''

python3 - "$SSH_V4_CIDR" "$SSH_V6_CIDR" <<'PY'
import ipaddress
import sys

v4 = ipaddress.ip_network(sys.argv[1], strict=True)
if v4.version != 4 or v4.prefixlen == 0:
    raise SystemExit('SSH_V4_CIDR must be a non-default IPv4 CIDR')
print(f'approved_ssh_ipv4={v4}')

if sys.argv[2]:
    v6 = ipaddress.ip_network(sys.argv[2], strict=True)
    if v6.version != 6 or v6.prefixlen == 0:
        raise SystemExit('SSH_V6_CIDR must be a non-default IPv6 CIDR')
    print(f'approved_ssh_ipv6={v6}')
else:
    print('approved_ssh_ipv6=none; provider-console-recovery-only')
PY
```

Do not substitute `::/0`, the Cloudflare IPv6 ranges, or the staging origin IPv6 address for an operator SSH source. No IPv6 SSH allow rule is added in this runbook.

### 1. Reconfirm identity and capture the current SSH session

```bash
hostnamectl --static
ip -4 -o addr show scope global
ip -6 -o addr show scope global
printf 'ssh_connection=%s\n' "$SSH_CONNECTION"
ufw status verbose
ufw status numbered
ufw show added
ss -ltnp
```

The expected host must be `ubuntu-2gb-nbg1-2`, with the staging IPv4/IPv6 addresses above and without the production IPv4. If `SSH_CONNECTION` is empty, stop; the checked-in staging apply procedure also requires an active SSH session.

### 2. Confirm IPv6 processing before adding rules

```bash
grep -qx 'IPV6=yes' /etc/default/ufw
```

If this assertion fails, stop and correct `/etc/default/ufw` through a separately reviewed change before adding IPv6 rules. Do not activate an IPv4-only UFW policy while the host has intentional IPv6 ingress.

### 3. Set the intended default policies

These commands prepare the UFW policy. They do not enable UFW by themselves:

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw default deny routed
```

UFW’s packaged before-rules must continue to provide loopback and established/related traffic. Do not replace `/etc/ufw/before*.rules` with an ad hoc ruleset.

### 4. Add SSH rules before activation

Add only the approved source-restricted IPv4 rule:

```bash
sudo ufw allow from "$SSH_V4_CIDR" to any port 22 proto tcp \
  comment 'approved staging SSH IPv4 maintenance'
```

Do not add an IPv6 SSH rule because no approved operator IPv6 source was supplied. Verify the IPv4 rule before activation:

```bash
sudo ufw show added
sudo ufw status numbered
sudo ufw show raw | grep -E -- 'dport 22|DROP|REJECT' || true
```

The review must confirm:

- the IPv4 SSH rule has exactly `109.245.57.92/32` as its source;
- no IPv6 SSH allow rule is present;
- neither rule is `Anywhere`/`Anywhere (v6)`;
- the provider console is available as the IPv6 recovery path;
- no rule grants public TCP 9000 or TCP 19000;
- no production origin, production Caddy, or production service is referenced.

### 5. Preserve public web availability, if required

The staging Caddy listener is currently public and the additive Cloudflare procedure intentionally retains broad web rules until external checks pass. If the maintenance window must not interrupt public HTTP/HTTPS, add temporary broad web rules before activation:

```bash
sudo ufw allow 80/tcp comment 'temporary staging web until Cloudflare ranges are applied'
sudo ufw allow 443/tcp comment 'temporary staging web until Cloudflare ranges are applied'
```

If a short public outage is explicitly accepted, omit these two rules; the later additive procedure will add Cloudflare-only rules after UFW becomes active. In either case, broad web access must be removed only through the separately approved `--remove-broad` phase after dual-stack checks.

Recheck the complete pre-activation rule list:

```bash
sudo ufw status numbered
sudo ufw show added
```

Do not continue if the list contains an unexpected SSH source, a public P2P rule, an origin-specific production rule, or an accidental default-route rule.

### 5a. Final pre-activation rule review

Before `ufw --force enable`, the expected UFW user-rule set is:

| Rule | Expected state before activation |
| --- | --- |
| TCP 22 from `109.245.57.92/32` | Required; operator-only SSH access |
| TCP 22 from any other IPv4 source | Absent |
| IPv6 TCP 22 from any source | Absent; provider console is IPv6 recovery |
| TCP 80 from Anywhere | Optional temporary rule only if public web must remain available during handoff |
| TCP 443 from Anywhere | Optional temporary rule only if public web must remain available during handoff |
| Cloudflare IPv4/IPv6 TCP 80/443 ranges | Not manually added here; added by the checked-in staging procedure after UFW is active |
| TCP 9000 or TCP 19000 | Absent; no public P2P listener exists |
| Production origin/service rules | Absent |

If temporary broad web rules are selected, the only expected broad user rules are TCP 80 and TCP 443, and they must be removed later through the reviewed staging `--remove-broad` phase. If they are not selected, public web will remain unavailable until the additive Cloudflare phase succeeds.

### 6. Verify the independent recovery path

Before enabling UFW:

1. Keep the current SSH session open.
2. Open a second SSH session from the independently approved path/source, or open the provider console.
3. Confirm the second path reaches the same host identity and can run `ufw status verbose`.
4. Keep the provider console available until post-activation SSH verification succeeds.

A second terminal on the same source network is not an independent recovery path.

### 7. Activate UFW

Only after Steps 0–6 pass:

```bash
sudo ufw --force enable
```

Immediately verify from the current session and the independent path:

```bash
sudo ufw status verbose
sudo ufw status numbered
sudo ufw show raw
ss -ltnp
```

Expected immediate state:

- UFW reports `Status: active`;
- the IPv4 SSH rule for `109.245.57.92/32` is present;
- no IPv6 SSH allow rule is present, and the provider console remains available for IPv6 recovery;
- current IPv4 SSH remains usable from the approved operator address;
- loopback node/faucet/Caddy-admin listeners are unchanged;
- public web remains available if temporary 80/443 rules were selected;
- no public P2P listener or port 9000/19000 rule exists.

Do not run the staging origin-lockdown apply step until IPv4 SSH remains usable and the independent provider recovery path is confirmed.

### 8. Hand off to the staging Cloudflare procedure

After UFW activation and SSH verification:

```bash
sudo bash deploy/faucet-staging-origin-lockdown.sh --check
```

The check must pass the staging identity, DNS, Cloudflare range, UFW-active, IPv6, Caddy, listener, disabled-intake, and public-status gates. The script explicitly refuses to enable UFW automatically.

Then, only with separate approval for additive web rules:

```bash
sudo bash deploy/faucet-staging-origin-lockdown.sh --apply
```

The additive phase snapshots UFW again, adds current Cloudflare IPv4/IPv6 TCP 80/443 rules, retains broad web rules, and verifies the public disabled status. Do not use `--remove-broad` in the same first operation.

After independent IPv4 and IPv6 public checks pass, use the separately approved removal command:

```bash
sudo env STAGING_LOCKDOWN_CONFIRM=I_UNDERSTAND \
  bash deploy/faucet-staging-origin-lockdown.sh \
  --apply --remove-broad
```

The removal phase must be followed by direct-origin tests against both staging address families. Public Cloudflare HTTPS must continue to work, while direct origin access must fail. The faucet remains `FAUCET_ENABLED=false` throughout.

## Stop conditions

Stop and leave UFW unchanged or use the provider recovery path if:

- host identity or SSH host-key fingerprint does not match;
- the approved SSH source `109.245.57.92/32` is missing or uncertain;
- an IPv6 SSH allow rule is added without a separately approved IPv6 operator source;
- the provider-console recovery path cannot be tested;
- UFW does not report `IPV6=yes`;
- `ufw show added` contains an unexpected rule;
- SSH fails from either approved path after activation;
- a public P2P rule is proposed without a separate topology decision;
- the staging script reports UFW inactive, an identity mismatch, bad DNS, invalid Cloudflare ranges, a public listener violation, or failed public status;
- public Cloudflare HTTPS or IPv6 checks fail;
- a service restart, credential change, production change, claim, or intake enablement is proposed as a workaround.

Keep `FAUCET_ENABLED=false`. Do not switch to production DNS, Caddy, credentials, database, node state, or funds as a recovery shortcut.

## Rollback posture

The exact pre-change snapshot is preserved at:

```text
/var/backups/ultranet/faucet-staging-origin-lockdown/initial-20260904T195930Z
```

It includes the pre-change UFW source files and checksums. If access becomes uncertain, use the provider console or independent recovery path first. Do not improvise a new public SSH rule. Disable the newly activated UFW policy through the recovery path only after confirming the command and host identity, then compare the restored state to the snapshot. Preserve the failure timestamp, rule output, listener output, and snapshot checksum evidence.

## Review conclusion

The existing SSH/recovery documentation is substantively correct: it requires an independent recovery path, protects ports 22/9000 from the web-lockdown procedure, requires IPv6 handling, and blocks activation when UFW is inactive. This document adds the missing operational detail: the exact operator-only IPv4 source `109.245.57.92/32` must be created and independently verified before `ufw --force enable`, no IPv6 SSH allow rule is added without separate approval, the Cloudflare lists apply only to web ports, and the live host currently has no public P2P listener or firewall rule to preserve.

This audit made no UFW, SSH, nftables, iptables, DNS, Cloudflare, Caddy, systemd, service, credential, intake, or claim changes.
