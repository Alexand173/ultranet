# Staging Turnstile secret rotation preflight

**Status:** historical 2026-09-13 rotation/disabled validation recorded; post-canary procedure reviewed on 2026-09-18 but not executed. The incorrect approval record has been replaced by an operator development test record; operator readiness, fresh technical evidence, and test results remain unrecorded. No production changes are authorized.
**Scope:** isolated staging faucet only
**Host:** `ubuntu-2gb-nbg1-2`
**Public hostname:** `faucet-staging.ultranetwork.cc`
**Staging unit:** `ultranet-faucet-staging.service`

This runbook prepares a fail-closed rotation of the **server-side** Cloudflare Turnstile secret. It does not rotate the public sitekey, enable intake, submit a claim, restart Caddy, change DNS, alter production services, or authorize the canary.

For `ULTRANET-STG-TEST-20260918-01`, any reference to an approved canary means the operator-scoped development test after technical checks pass, not the withdrawn approval/signature. Historical risk-acceptance statements below are not fresh evidence: the operator must verify a valid bounded risk disposition or remediation without relying on the withdrawn record. No independent reviewer is assigned for this test.

**Latest execution note — 2026-09-12:** the initial read-only preflight stopped because the effective staging egress policy was expired. The policy was subsequently refreshed with no resolver or Cloudflare-range changes, effective coverage passed, and the staging expiry timer was enabled. The encrypted Turnstile rollback copy was preserved while waiting for approved protected input.

**Rotation completion note — 2026-09-13:** the provider-issued staging secret was encrypted with the existing host key and unchanged alias `faucet-staging-turnstile.secret`; the temporary encrypted artifact was atomically renamed over the live credential. The new encrypted checksum is `f819078d9f2e08f8c771d78f305832ba7f9878f7a9655daa9a3e9f313b35a922`; the root-only rollback checksum remains `4034af98c4e4d16a518e34f1a206ed1fa6c810abf58a45cfbcc5722b980b9d53`. Plaintext and the temporary artifact are absent. Staging-only restart, egress condition, and `check-config` passed with `FAUCET_ENABLED=false`; the service was stopped afterward, leaving port `8090` closed. Cloudflare provider acceptance was not tested and no claim or canary is authorized. The host credential store emitted a warning that its backing media is not encrypted; treat host-level credential-at-rest protection as a separate residual-risk review. The initial read-only pass observed an active `99-canary-override.conf` drop-in containing only `ExecCondition=/bin/true`; the main egress condition still ran and passed. The drop-in was later removed under separate approval and the effective unit was reverified. A strict final disabled-only check returned HTTP `200` from `/api/faucet/status` with `enabled=false`, `availability=disabled`, fixed amount `1.000000`, and six decimals; `/internal/status` returned `401`, and the unit was stopped immediately afterward.

**Read-only reconciliation note — 2026-09-13:** the staging host, production separation, disabled boundary, credential metadata/checksums, rollback copy, temporary-artifact absence, effective egress policy, expiry timer, and all staging unit verification checks passed. The live drop-in was `root:root 0644` with SHA-256 `3488f825f851fb805fb9922b6711d5ebc6d7486bcc8663799c3de3c8a599d453` and added only `ExecCondition=/bin/true` after the reviewed egress condition; this was the initial read-only evidence and no mutation was performed during that pass. The host credential-store file is `root:root 0400` on the unencrypted ext4 root filesystem; `systemd-creds has-tpm2` returned exit `19` with partial capability. No approved at-rest protection was evidenced, so host credential-store remediation or explicit risk acceptance remains required before a canary.

**Override removal note — 2026-09-13:** after a portable host-identity and fail-closed recheck passed, the recorded `99-canary-override.conf` path, `root:root` mode `0644`, SHA-256, and exact directives (`[Service]` plus `ExecCondition=/bin/true`) matched. The file was removed under the separately approved disposition, `systemctl daemon-reload` completed, and effective inspection showed only the reviewed egress condition and `50-egress-allowlist.conf`; `systemd-analyze verify` passed. The faucet remained inactive with `FAUCET_ENABLED=false`, port `8090` closed, egress effective, and the expiry timer active. This resolved the observed override drift but does not authorize a canary. The historical note asserted a one-canary credential-store risk acceptance; that assertion is not a current disposition and must not rely on the withdrawn approval. Verify remediation or a valid bounded private operator risk disposition before any test. TPM2/LUKS-backed remediation remains required for production or long-lived staging.

**Final local unit health check — 2026-09-13:** `ultranet-staging.service` was active while `ultranet-faucet-staging.service` remained inactive and disabled; `FAUCET_ENABLED=false` and port `8090` remained closed. Only the expected loopback validator listeners (`127.0.0.1:8081` and `127.0.0.1:19000`) were present. Staging Caddy was active, production Caddy inactive, staging Caddy validation passed, egress was effective, the expiry timer was active, and `systemd-analyze verify` passed for validator, faucet, egress-expiry, and staging Caddy units. All four encrypted faucet credentials remained `root:root` mode `0600`; the host credential-store finding remains unresolved.

The current safety boundary must remain true throughout:

```text
FAUCET_ENABLED=false
ultranet-faucet-staging.service: inactive unless a separately approved disabled validation starts it
127.0.0.1:8090: closed unless a separately approved disabled validation starts it
production Caddy/service/DNS/firewall/state/credentials: untouched
```

## 1. What the rotation does and does not prove

The rotation changes only the encrypted artifact projected by the existing systemd credential alias:

```text
faucet-staging-turnstile.secret
```

The alias is mapped by [deploy/ultranet-faucet-staging.service](../../deploy/ultranet-faucet-staging.service) to:

```text
/etc/ultranet-faucet-staging/secrets/faucet-staging-turnstile.secret.cred
```

A successful disabled restart proves that the encrypted credential, alias, permissions, and service wiring are usable. It does **not** prove that Cloudflare Siteverify accepts the secret. Provider acceptance requires a later, separately approved one-claim canary after every other hard gate passes.

## 2. Preconditions and approval boundary

Do not begin the mutating steps until all of these are recorded in the private change record:

- [ ] The SSH host identity and approved staging operator session are verified.
- [ ] The Cloudflare widget is staging-only and belongs to `faucet-staging.ultranetwork.cc`.
- [ ] The widget action is exactly `faucet_claim`.
- [ ] The provider-issued secret was obtained through an approved private channel.
- [ ] The current encrypted credential has a root-only rollback copy.
- [ ] The operator has approval to rotate the staging credential while intake remains disabled.
- [ ] The operator has a tested recovery path if the disabled validation fails.
- [ ] No production credential, production widget, production service, or production host is part of the procedure.

This runbook intentionally does not contain a secret, token, wallet credential, recovery phrase, CAPTCHA token, idempotency key, signed envelope, or claim recipient material.

## 3. Provider-side verification

In the Cloudflare dashboard or an approved read-only provider view, verify the replacement credential's provenance before placing it on the host:

1. The Turnstile widget is active.
2. The exact allowed hostname is `faucet-staging.ultranetwork.cc`.
3. Separately inspect the helper render configuration and server-side expected action: both must use `faucet_claim`. This is an application/Siteverify contract, not a presumed dashboard widget setting.
4. The widget is separate from the production widget.
5. The replacement is not the production Turnstile secret.
6. The public sitekey remains the reviewed staging key:
   `0x4AAAAAAEqF5I0TO-lamwb7`.

Record only redacted metadata:

```text
widget identity: non-secret identifier, if available
hostname: faucet-staging.ultranetwork.cc
action: faucet_claim
verification timestamp: UTC
operator/change-record reference: approved reference
```

Never record the secret in chat, screenshots, browser storage, shell history, command arguments, environment variables, repository files, ordinary logs, or backups beside the database.

## 4. Read-only host preflight

Run this section before creating or replacing any credential. It must not restart a service or change the public route.

```bash
set -euo pipefail

EXPECTED_HOST='ubuntu-2gb-nbg1-2'
FAUCET_UNIT='ultranet-faucet-staging.service'
FAUCET_ENV='/etc/ultranet-faucet-staging/faucet.env'
EGRESS_CHECK='/usr/local/libexec/ultranet-faucet-staging-egress.py'

[ "$(hostnamectl --static)" = "$EXPECTED_HOST" ]
sudo grep -Eq '^FAUCET_ENABLED=false$' "$FAUCET_ENV"
sudo grep -Eq '^FAUCET_TURNSTILE_HOSTNAME=faucet-staging\.ultranetwork\.cc$' "$FAUCET_ENV"
sudo grep -Eq '^FAUCET_TURNSTILE_CREDENTIAL=faucet-staging-turnstile\.secret$' "$FAUCET_ENV"

sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service

sudo "$EGRESS_CHECK" --check --effective
sudo systemctl is-active --quiet ultranet-faucet-staging-egress-expiry.timer
test "$(sudo systemctl show "$FAUCET_UNIT" -p ActiveState --value)" = inactive
listeners="$(sudo ss -ltnH 'sport = :8090')"
test -z "$listeners"
sudo systemctl is-active --quiet caddy-staging.service
```

The credential alias check must match the installed unit. The canonical staging template uses `faucet-staging-turnstile.secret`; if the live environment differs, stop and reconcile the reviewed unit and environment before rotating anything. Do not “fix” an alias by changing only one side.

Capture metadata only:

```bash
sudo stat -c '%n %U:%G %a' \
  /etc/ultranet-faucet-staging/faucet.env \
  /etc/ultranet-faucet-staging/secrets/*.cred

sudo systemctl show "$FAUCET_UNIT" \
  -p FragmentPath \
  -p User \
  -p Group \
  -p SupplementaryGroups \
  -p ExecStart \
  -p ActiveState \
  -p SubState \
  -p ExecMainStartTimestamp \
  -p ExecMainExitTimestamp
```

Do not use `cat`, `journalctl` without redaction review, shell tracing, environment dumps, or decryption commands on credential files.

## 5. Preserve the encrypted rollback artifact

Use a root-only backup directory. The rollback copy must be made before the new credential is staged.

```bash
set -euo pipefail

SECRETS_DIR='/etc/ultranet-faucet-staging/secrets'
CURRENT="$SECRETS_DIR/faucet-staging-turnstile.secret.cred"
ROTATION_ID="$(date -u +%Y%m%dT%H%M%SZ)"
BACKUP_DIR="/var/backups/ultranet/faucet-staging-turnstile/$ROTATION_ID"

# Section 13 parent/path/checksum guards and serialized operator session required.
sudo test -f "$CURRENT"
sudo test ! -L "$CURRENT"
sudo test "$(sudo stat -c '%U:%G %a %h' "$CURRENT")" = 'root:root 600 1'
sudo test ! -e "$BACKUP_DIR"
sudo test ! -L "$BACKUP_DIR"
# The existing backup parent must already be verified root-controlled.
sudo mkdir -m 0700 -- "$BACKUP_DIR"
sudo chown root:root "$BACKUP_DIR"
sudo install -o root -g root -m 0600 \
  "$CURRENT" \
  "$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"
sudo cmp --silent "$CURRENT" "$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"

sudo stat -c '%n %U:%G %a' \
  "$CURRENT" \
  "$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"

sudo sha256sum \
  "$CURRENT" \
  "$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"
```

Record the backup path, encrypted-file checksum, owner, group, and mode. Do not decrypt the rollback copy.

If the backup cannot be created and verified, stop. Do not continue with staging or replacement.

## 6. Securely stage and encrypt the replacement

Use the approved secret-management process or a root-only temporary directory. A protected plaintext file may exist only for the minimum encryption window.

Required properties:

- directory owned by `root:root`, mode `0700`;
- plaintext file owned by `root:root`, mode `0600`;
- no secret in a command argument, environment variable, shell history, editor swap file, screenshot, chat message, or ordinary log;
- no shell tracing while handling the secret;
- no production secret or production widget credential;
- no plaintext backup beside the faucet database;
- cleanup performed through the host-approved procedure after encryption.

The replacement encrypted file must be created on the same filesystem as the final target, using a temporary name:

```text
/etc/ultranet-faucet-staging/secrets/.faucet-staging-turnstile.secret.cred.new
```

The operator supplies the protected plaintext path privately. It must not be pasted into this runbook or transcript.

```bash
set -euo pipefail

SECRETS_DIR='/etc/ultranet-faucet-staging/secrets'
NEW_CRED="$SECRETS_DIR/.faucet-staging-turnstile.secret.cred.new"
PROTECTED_PLAINTEXT='<approved-protected-plaintext-path>'

# Mandatory: Section 13 file guards and private cleanup handlers are in place.
# A stale output (including a dangling symlink) must never be overwritten.
sudo test -f "$PROTECTED_PLAINTEXT"
sudo test ! -L "$PROTECTED_PLAINTEXT"
sudo test "$(sudo stat -c '%U:%G %a' "$PROTECTED_PLAINTEXT")" = 'root:root 600'
sudo test ! -e "$NEW_CRED"
sudo test ! -L "$NEW_CRED"
sudo bash -c '
  set -euo pipefail
  set +x
  umask 077
  set -o noclobber
  exec 3> "$2"
  systemd-creds encrypt \
    --with-key=host \
    --name=faucet-staging-turnstile.secret \
    "$1" - >&3
' _ "$PROTECTED_PLAINTEXT" "$NEW_CRED"

sudo test -s "$NEW_CRED"
sudo test ! -L "$NEW_CRED"
sudo test "$(sudo stat -c '%U:%G %a' "$NEW_CRED")" = 'root:root 600'
sudo stat -c '%n %U:%G %a' "$NEW_CRED"
sudo sha256sum "$NEW_CRED"
```

Do not run `systemd-creds decrypt` for evidence. Do not print the protected plaintext path if it reveals a secret-management location that is itself sensitive.

After successful encryption, remove the plaintext using the approved host cleanup procedure. Secure deletion on SSD-backed storage is best-effort; do not claim physical erasure as proof of security.

## 7. Atomic replacement

Do not truncate or edit the live `.cred` file in place. Verify the temporary encrypted file first, then atomically rename it over the existing credential on the same filesystem.

```bash
set -euo pipefail

SECRETS_DIR='/etc/ultranet-faucet-staging/secrets'
NEW_CRED="$SECRETS_DIR/.faucet-staging-turnstile.secret.cred.new"
CURRENT="$SECRETS_DIR/faucet-staging-turnstile.secret.cred"

# Recheck Section 13 guards and the approved current hash before this block.
sudo test -f "$NEW_CRED"
sudo test -s "$NEW_CRED"
sudo test ! -L "$NEW_CRED"
sudo test -f "$CURRENT"
sudo test ! -L "$CURRENT"
sudo test "$(sudo stat -c '%U:%G %a %h' "$NEW_CRED")" = 'root:root 600 1'
sudo test "$(sudo stat -c '%d' "$NEW_CRED")" = "$(sudo stat -c '%d' "$CURRENT")"
NEW_HASH="$(sudo sha256sum "$NEW_CRED" | cut -d ' ' -f 1)"
sudo mv -T "$NEW_CRED" "$CURRENT"
sudo test "$(sudo stat -c '%U:%G %a' "$CURRENT")" = 'root:root 600'
test "$(sudo sha256sum "$CURRENT" | cut -d ' ' -f 1)" = "$NEW_HASH"
sudo test ! -e "$NEW_CRED"
sudo test ! -L "$NEW_CRED"
sudo sha256sum "$CURRENT"
```

Do not change these names or aliases:

```text
faucet-staging-turnstile.secret
FAUCET_TURNSTILE_CREDENTIAL
LoadCredentialEncrypted=faucet-staging-turnstile.secret:...
```

If atomic replacement fails, leave the old credential in place, remove only the temporary encrypted artifact if safe, and stop.

## 8. Disabled validation and projection check

Only run this operator-private procedure after explicit disabled-validation authorization and the identity, file, backup, runtime-disablement, and baseline gates in Section 13 pass. No command here is permission to start the service. Do not restart validator, Caddy, or production units; credential-content replacement alone needs no `daemon-reload`.

Use one bounded Bash session with an EXIT cleanup, rather than separate start/check commands that can leave a service running after failure. Require non-interactive sudo access to the approved commands before starting. Review effective systemd start/stop timeouts and keep an independent recovery session available: a shell trap cannot survive SIGKILL, connection loss, or host failure. A timeout kills the client, not necessarily the systemd job; stop/verify and escalate any failure. Output stays operator-private pending redaction.

```bash
(
  set -euo pipefail
  set +x
  UNIT='ultranet-faucet-staging.service'
  ENV_FILE='/etc/ultranet-faucet-staging/faucet.env'
  EGRESS='/usr/local/libexec/ultranet-faucet-staging-egress.py'

  test "$(hostnamectl --static)" = 'ubuntu-2gb-nbg1-2'
  sudo -n true
  test "$(sudo -n systemctl show "$UNIT" -p ActiveState --value)" = inactive
  listeners="$(sudo -n ss -ltnH 'sport = :8090')"
  test -z "$listeners"

  cleanup() {
    result=$?
    trap - EXIT HUP INT TERM
    set +e
    timeout --kill-after=5s 45s sudo -n systemctl stop "$UNIT"
    stop_result=$?
    state="$(timeout --kill-after=5s 10s sudo -n systemctl show "$UNIT" -p ActiveState --value)"
    state_result=$?
    listeners="$(timeout --kill-after=5s 10s sudo -n ss -ltnH 'sport = :8090')"
    port_result=$?
    if [ "$stop_result" -ne 0 ] || [ "$state_result" -ne 0 ] ||
       [ "$state" != inactive ] || [ "$port_result" -ne 0 ] || [ -n "$listeners" ]; then
      printf '%s\n' 'STOP VERIFICATION FAILED: use the independent recovery path.' >&2
      result=1
    fi
    exit "$result"
  }
  trap cleanup EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM

  sudo -n grep -Eq '^FAUCET_ENABLED=false$' "$ENV_FILE"
  timeout --kill-after=5s 30s sudo -n systemd-analyze verify \
    /etc/systemd/system/ultranet-staging.service \
    /etc/systemd/system/ultranet-faucet-staging.service
  timeout --kill-after=5s 30s sudo -n "$EGRESS" --check --effective
  sudo -n systemctl is-active --quiet ultranet-faucet-staging-egress-expiry.timer
  timeout --kill-after=5s 45s sudo -n systemctl start "$UNIT"
  sudo -n systemctl is-active --quiet "$UNIT"
  timeout --kill-after=5s 30s sudo -n -u ultranet-faucet-staging bash -c '
    set -euo pipefail
    set +x
    set -a
    . /etc/ultranet-faucet-staging/faucet.env
    set +a
    /opt/ultranet-faucet-staging/ultranet-faucet check-config
  '
  curl --fail --silent --show-error --connect-timeout 5 --max-time 10 \
    http://127.0.0.1:8090/api/faucet/status | python3 -c '
import json, sys
value = json.load(sys.stdin)
data = value["data"]
assert value["success"] is True
assert data["enabled"] is False and data["availability"] == "disabled"
assert data["claim_amount_base_units"] == 1_000_000
assert data["claim_amount_ultra"] == "1.000000" and data["decimals"] == 6
print("disabled_status=passed")
'
  timeout --kill-after=5s 30s sudo -n "$EGRESS" --check --effective
  sudo -n grep -Eq '^FAUCET_ENABLED=false$' "$ENV_FILE"
)
```

The environment must already be reviewed as non-secret, root-controlled, and canonical (no duplicate/overriding intake flag). The grep is not a runtime-disablement proof; the status assertion checks the running service. Startup checks credential projection; standalone `check-config` checks non-secret configuration, not Siteverify acceptance.

After the wrapper exits, independently verify inactive state, closed port, no pending start/restart job, unchanged disabled configuration, active expiry monitoring, and the private stopped-state baseline comparison. Review logs privately and share only redacted results. A failed service state is not a successful closeout; record it and reconcile without another start.

Expected state: unchanged signer/chain/fee policy, zero active queue and reservations, and no new claims or transactions during rotation. For the unused preflight, claims and confirmed budget remain zero. After a successful canary, preserve its one reconciled claim and expected confirmed budget instead; compare balances, nonce, fees, SQLite records, and hashes to the post-canary baseline. Never clear state to satisfy a zero-count assertion.

If startup, egress, status, cleanup, or baseline comparison fails, stop and follow the validity-aware failure path in Sections 10 and 13. Do not bypass the egress guard or submit a diagnostic claim.

## 9. Redacted post-rotation checks

Capture only allowlisted status and metadata. Raw `systemctl status` includes journal output; do not send it or unreviewed `journalctl` output to a shared transcript. The operator reviews logs privately and shares only approved redacted summaries, never raw bodies or credentials.

```bash
sudo systemctl show ultranet-faucet-staging.service \
  -p ActiveState -p SubState -p Result -p ExecMainStatus \
  -p ExecMainStartTimestamp -p ExecMainExitTimestamp

sudo stat -c '%n %U:%G %a' \
  /etc/ultranet-faucet-staging/secrets/*.cred

sudo ss -ltnH 'sport = :8090'
```

Review logs for absence of:

- Turnstile secret or credential contents;
- CAPTCHA tokens;
- operator tokens;
- request bodies;
- authorization headers;
- idempotency keys;
- signed envelopes;
- wallet credentials or recovery material.

During an authorized bounded disabled validation, verify `enabled=false`, zero active queue/reservations, and unchanged claims/confirmed budget relative to the appropriate baseline. After the canary that includes its reconciled claim, not zero claims. Use only the approved private operator procedure for protected status; never put the operator token in command arguments or evidence. Compare the stopped-state records after the validation and do not restart merely to collect extra evidence.

A public HTTP `502` is not a secret-validation or disabled-contract result. It is compatible with an intentionally stopped upstream only when fresh inactive-service and closed-port evidence corroborates it; do not assume every `502` has that cause. Do not start the service merely to change that response.

## 10. Rollback procedure

Rollback immediately if any of the following occurs:

- host identity or unit identity does not match staging;
- the exact hostname/action/widget cannot be verified;
- the encrypted artifact has incorrect owner, group, mode, or alias;
- `systemd-analyze verify`, egress validation, service projection, or `check-config` fails;
- the staging signer/address or chain policy changes unexpectedly;
- any secret or token appears in logs, shell history, process arguments, `/proc`, screenshots, or evidence;
- any production service, route, credential, DNS record, firewall, or state is touched;
- the service attempts to start with `FAUCET_ENABLED=true`;
- any claim request or claim processing occurs.

Failure response must keep intake disabled. Do not automatically restore a credential for every failure above: a host mismatch forbids host mutation, and exposure/revocation forbids reactivation of the old secret.

1. On the verified staging host, disable runtime intake through the protected operator path, assert persisted `FAUCET_ENABLED=false`, and stop the faucet with the bounded stop/verification pattern in Section 8. Do not suppress stop errors with `|| true`; use the tested recovery path if shutdown cannot be verified. Preserve failed-unit diagnostics rather than clearing them automatically.
2. Record provider rotation and old-secret validity/revocation state privately. Restoring ciphertext cannot undo provider revocation. If validity is unknown, expired, revoked, or compromised, keep the service stopped and provision a fresh staging-only secret through the approved private recovery process.
3. Only for an explicitly approved local-artifact restore, verify the exact recorded backup path, SHA-256, regular-file/non-symlink status, root-only parents, and `root:root 0600` ownership/mode. Apply Section 13 file guards to the live and rollback paths. Preserve the failed replacement's permitted metadata before restoring.
4. Use exclusive creation and restrictive permissions for the same-filesystem `.faucet-staging-turnstile.secret.cred.rollback` artifact. A preexisting file or symlink is a stop condition, not something to overwrite. Copy only the verified encrypted backup, compare its checksum, then atomically rename over the reviewed live credential. Recheck metadata, hash, temporary absence, inactive service, and closed port. No credential decryption is allowed.
5. A restore does not authorize another start. Any disabled-only validation needs its own explicit scope and the Section 8 cleanup wrapper. Never restore SQLite, balances, or nonce state to erase a completed or uncertain claim, and never resubmit on a timeout.
6. Any suspected or confirmed secret exposure requires provider-side incident rotation/invalidation while stopped; never restore or reactivate the exposed credential as a remedy. Containment must not wait for transaction reconciliation. The rotation owner follows the approved private incident procedure, accounts for any provider overlap, and retains redacted incident evidence only.

Preserve the rollback path, old/new encrypted checksums, redacted logs, service state, and failure reason. Do not delete the old encrypted rollback artifact until the private rollback window and any approved canary decision are complete.

## 11. Provider acceptance and canary boundary

The rotation preflight is complete only when the disabled validation and rollback path pass. It does not authorize:

- `FAUCET_ENABLED=true`;
- `/internal/enable`;
- a claim request;
- a canary recipient;
- an idempotency key;
- production credential use;
- production service or Caddy changes.

The development canary requires the [operator test record](./faucet-staging-one-claim-test-record.md), a current test window, operator readiness, and all technical gates from [deploy/faucet-staging-claims-enablement.md](../../deploy/faucet-staging-claims-enablement.md). It is limited to exactly one disposable claim, with no independent-review or signed-approval requirement for this test. Use a fresh single-use Turnstile token, reconcile the exact payout and node state, and disable intake immediately afterward.

## 12. Evidence record

Record only:

- UTC timestamps;
- staging host/unit identity;
- provider hostname/action/widget metadata;
- old and new encrypted-file checksums;
- rollback backup path;
- owner/group/mode metadata;
- service/check-config/egress status;
- queue, budget, and claim counts;
- redacted error/status lines;
- operator test reference and technical evidence references (no invented reviewer or signature).

Never record:

- the Turnstile secret;
- plaintext credential paths if sensitive;
- operator tokens;
- wallet credentials or recovery phrases;
- CAPTCHA tokens;
- idempotency keys;
- signed request envelopes;
- private claim recipient material.

## 13. Post-canary rotation and closeout

**Procedure reviewed for documentation on 2026-09-18; not an execution record.** Test reference: `ULTRANET-STG-TEST-20260918-01`; owner/operator: Vladan Jotov. The [development test record](./faucet-staging-one-claim-test-record.md) replaces the withdrawn, incorrect approval record. No independent reviewer or signature is required for this operator-run test. Operator readiness, fresh technical evidence, and results remain unrecorded. No live action is performed merely by updating these documents.

### A. Disable and preserve the post-canary baseline

`202 Accepted` is admission only. On the normal path follow the handoff's bounded confirmation/reconciliation procedure before closeout; disabling solely on admission can strand queued work. Disablement blocks queued selection/new payout start, but cannot guarantee cancellation of an already-in-flight transaction. A running disabled service may poll pending transactions. `/internal/reconcile` runs worker logic and is not a read-only probe; it may submit while enabled. Never restart or re-enable after cutoff merely to drain unresolved work. The prohibition on additional public claim POSTs does not change the backend's existing bounded same-envelope behavior or authorize extra worker runs.

- Immediately after reconciliation, on failure, uncertainty, timeout, or at test-window end—whichever comes first—privately disable runtime intake, assert persisted `FAUCET_ENABLED=false`, stop `ultranet-faucet-staging.service`, and verify inactive state and closed port `8090`. An environment-file check alone does not establish runtime disablement. No second request, diagnostic claim, or replay POST is included.
- Preserve consistent SQLite/WAL records and the claim/node/hash/nonce/fee/source-debit/recipient-credit/budget reconciliation privately. For a successful run, expect one reconciled claim and its confirmed budget, zero active queue/reservations, and no later changes during rotation. Do not reset or restore transaction state to force zero counters. An unresolved outcome remains a failure requiring reconciliation, not retry authorization.
- Refresh SSH host identity, staging/production isolation, unit/environment/alias identity, effective egress and expiry timer, disabled flag, and root-only backup evidence. Historical September 13 hashes are comparison evidence only. Stale egress or drift means stop; do not silently refresh policy or bypass `ExecCondition`.
- The claim must occur within the fixed operator test window. If it expires unused, record a new operator-selected window before testing; do not perform a canary or an unnecessary post-canary rotation under an expired window. Following the test claim, mandatory disablement and secret-containment closeout must still be completed, even if provider overlap ends after the window. This does not extend claim scope or allow another service start outside its explicitly recorded validation scope.

### B. Provider-side rotation and invalidation

[Cloudflare's rotation documentation](https://developers.cloudflare.com/turnstile/troubleshooting/rotate-secret-key/) reviewed on 2026-09-18 states that normal rotation permits **both old and new secrets for two hours**. Therefore local replacement is not immediate revocation. Recheck current provider behavior privately at execution time and record the actual provider rotation timestamp, overlap deadline, and old-secret invalidation status separately.

1. Verify the existing staging widget, its reviewed sitekey, and exact allowed hostname. The helper's render action and the server's expected Siteverify action must be `faucet_claim`; action is not presumed to be a dashboard widget setting. Do not create a replacement widget or change the public sitekey under this procedure.
2. The rotation owner performs **Rotate Secret Key** for the verified staging widget in the approved private provider session. Never expose the returned secret or provider session/token to the assistant, screenshots, ordinary logs, command arguments, or repository.
3. Use only the provider-issued replacement through the protected input process. Re-encrypting the same secret or generating a different ciphertext checksum does not prove rotation.
4. Keep the faucet stopped through the overlap except for an explicitly authorized bounded disabled validation. Record provider-confirmed invalidation metadata, or clearly label a deadline derived from the documented two-hour behavior as an expected deadline rather than an observed revocation. If actual invalidation cannot be established, mark it pending and escalate; do not run an extra Siteverify request or claim to test it.
5. If exposure requires immediate revocation, normal two-hour overlap is not immediate containment. Use the separately approved provider incident-revocation path; do not invent an immediate-revoke control or substitute a production widget. Keep intake stopped while escalation is resolved.

### C. Protected input, file guards, encryption, and replacement

Sections 5–7 are usable only after these mandatory guards and cleanup arrangements pass in an operator-private session. Serialize the operation: no concurrent rotation or root-level credential editor may run.

- Verify the exact live path and all parent directories are non-symlink, root-controlled, and not writable by untrusted users. The credential and plaintext must be regular non-symlink files owned by `root:root` with mode `0600`; secret/backup/input directories must be root-only `0700`. Reject device/FIFO/socket paths and unexpected hard links. Verify live encrypted SHA-256 against the fresh approved baseline before backup and again before replacement.
- Use a new unique root-only backup directory. Reject a preexisting rotation directory or backup target; do not overwrite historical rollback evidence. Verify backup content hash equals the approved live hash without decryption.
- Stage plaintext only through an approved secret manager or verified protected tmpfs/root-only input location, with swap and core-dump exposure assessed. Set restrictive `umask 077`, disable shell tracing/recording, and prevent editor swap/history or accidental plaintext backups. Tmpfs alone does not prove protection from swap.
- Before encryption, verify absence of both `.faucet-staging-turnstile.secret.cred.new` and `.faucet-staging-turnstile.secret.cred.rollback`, including dangling symlinks. Any stale artifact is a stop condition requiring review, not automatic deletion or overwrite. Create output exclusively (`O_CREAT|O_EXCL`, or a reviewed root-shell noclobber file descriptor), with mode `0600` from creation. Encrypt to that open descriptor using the installed `systemd-creds` supported stdout mode, not to an unchecked preexisting path.
- Keep alias `faucet-staging-turnstile.secret` and explicit `--with-key=host` solely under the bounded exception. Do not copy `/var/lib/systemd/credential.secret` or use a null key. Verify the new ciphertext is nonempty, regular, non-symlink, `root:root 0600`, and on the same filesystem as the live target. Record only metadata and SHA-256; never decrypt for evidence.
- Install cleanup handlers before private input: remove the operation's own protected plaintext and incomplete encrypted temporary artifact on failure as well as success. Track which files were exclusively created by this operation; never remove an unknown preexisting artifact. Use the approved host cleanup procedure and verify absence independently. Unlink or SSD `shred` is not proof of physical erasure.
- Atomically rename the verified same-filesystem new artifact over the exact reviewed live credential without following a symlink. Recheck owner/mode/hash and temporary absence. If replacement fails, stop; do not assume the old artifact survived without checking its metadata/hash.

The shell fragments in this runbook are not substitutes for these guards. If the approved private input, exclusive creation, cleanup, or recovery implementation is missing, do not improvise it with secrets present: keep stopped and obtain review first.

### D. Bounded validation, recovery, and closeout

- With explicit disabled-validation authorization only, use Section 8's bounded wrapper and independently verify shutdown even after a failed check. No Caddy/validator restart, `/internal/enable`, Siteverify probe, or extra claim is allowed. Configuration/projection success does not prove provider acceptance of the new secret.
- Compare the stopped-state SQLite/ledger/budget baseline after validation. Preserve the completed claim and expected confirmed budget, zero active queue/reservations, and unchanged balances/fees/nonce. Unexpected changes fail closeout.
- Apply Section 10's validity-aware failure path: restoring encrypted bytes cannot reactivate a revoked provider secret. Unknown, revoked, or compromised credentials stay out of use. Preserve the stopped service and obtain a fresh provider-issued replacement if needed; never automatically restore ledger state.
- Record UTC start/stop, actual provider rotation time and overlap deadline, old-secret invalidation status/evidence, current/backup encrypted hashes and metadata, plaintext/temp cleanup, private log-review result, effective egress/timer result, baseline reconciliation, and final inactive/closed-port evidence. Do not assert success for missing or failed evidence.
- Any valid bounded single-canary host-key disposition does not authorize long-lived use/storage; do not infer that disposition from historical acceptance wording. Keep intake stopped, assign retention/revocation and protected-storage remediation to the owner, and retain encrypted rollback evidence only under its approved private retention decision. No automatic deletion, migration, or extension is authorized by this procedure.

### Post-canary closeout checklist — not yet executed

- [ ] Runtime intake disabled; persisted flag false; service inactive; port 8090 closed.
- [ ] One-claim reconciliation preserved, no retry/duplicate, no queue/reservations or unexpected budget/nonce changes.
- [ ] Staging provider secret rotated privately; overlap deadline and old-secret invalidation evidence recorded (pending is not passed).
- [ ] Exclusive protected encryption, alias, hashes/metadata, atomic replacement, and success/failure cleanup verified.
- [ ] Authorized disabled-only validation passed, or failure remains recorded with the service stopped; new-secret Siteverify acceptance is not claimed.
- [ ] Final egress/timer, private log review, temporary absence, retention owner, and bounded-exception closeout evidenced.

## Definition of Done for the historical preflight

- [ ] Exact staging widget/hostname verified against the provider control plane; helper action and server-side expected action verified separately as `faucet_claim`.
- [ ] Provider-issued staging secret provenance verified without recording the secret.
- [x] Existing encrypted credential preserved in a root-only rollback directory.
- [x] Replacement encrypted with the existing host key and unchanged alias.
- [x] Replacement atomically installed as `root:root` mode `0600`.
- [x] `FAUCET_ENABLED=false` asserted before and after validation.
- [x] Staging-only disabled restart and `check-config` pass; the service was stopped after validation.
- [x] Effective staging egress policy passes.
- [x] No production service, Caddy, DNS, firewall, credential, state, or funds are touched.
- [x] No secret, token, or request-private value appears in evidence.
- [x] Execution approval was recorded before the staging-only implementation; separate approval remains required for provider acceptance and any canary.
- [x] Live canary-named drop-in was captured with exact metadata/directives, removed under separate approval, and the effective unit/egress guard were reverified without starting the faucet.
- [ ] Verify credential-store remediation or a valid bounded private operator risk disposition for this test. Historical acceptance wording and the withdrawn approval do not close this gate; TPM2/LUKS-backed protection remains required for production or long-lived staging.

**Boundary:** This approved runbook covers only the staging credential rotation and disabled validation. Stop before any canary, intake enablement, production change, or use of an unapproved secret source.
