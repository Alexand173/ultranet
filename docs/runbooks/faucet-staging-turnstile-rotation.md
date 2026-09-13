# Staging Turnstile secret rotation preflight

**Status:** staging-only credential rotation and disabled validation completed; egress policy effective; provider acceptance/canary and production changes are not authorized
**Scope:** isolated staging faucet only
**Host:** `ubuntu-2gb-nbg1-2`
**Public hostname:** `faucet-staging.ultranetwork.cc`
**Staging unit:** `ultranet-faucet-staging.service`

This runbook prepares a fail-closed rotation of the **server-side** Cloudflare Turnstile secret. It does not rotate the public sitekey, enable intake, submit a claim, restart Caddy, change DNS, alter production services, or authorize the canary.

**Latest execution note — 2026-09-12:** the initial read-only preflight stopped because the effective staging egress policy was expired. The policy was subsequently refreshed with no resolver or Cloudflare-range changes, effective coverage passed, and the staging expiry timer was enabled. The encrypted Turnstile rollback copy was preserved while waiting for approved protected input.

**Rotation completion note — 2026-09-13:** the provider-issued staging secret was encrypted with the existing host key and unchanged alias `faucet-staging-turnstile.secret`; the temporary encrypted artifact was atomically renamed over the live credential. The new encrypted checksum is `f819078d9f2e08f8c771d78f305832ba7f9878f7a9655daa9a3e9f313b35a922`; the root-only rollback checksum remains `4034af98c4e4d16a518e34f1a206ed1fa6c810abf58a45cfbcc5722b980b9d53`. Plaintext and the temporary artifact are absent. Staging-only restart, egress condition, and `check-config` passed with `FAUCET_ENABLED=false`; the service was stopped afterward, leaving port `8090` closed. Cloudflare provider acceptance was not tested and no claim or canary is authorized. The host credential store emitted a warning that its backing media is not encrypted; treat host-level credential-at-rest protection as a separate residual-risk review. The initial read-only pass observed an active `99-canary-override.conf` drop-in containing only `ExecCondition=/bin/true`; the main egress condition still ran and passed. The drop-in was later removed under separate approval and the effective unit was reverified. A strict final disabled-only check returned HTTP `200` from `/api/faucet/status` with `enabled=false`, `availability=disabled`, fixed amount `1.000000`, and six decimals; `/internal/status` returned `401`, and the unit was stopped immediately afterward.

**Read-only reconciliation note — 2026-09-13:** the staging host, production separation, disabled boundary, credential metadata/checksums, rollback copy, temporary-artifact absence, effective egress policy, expiry timer, and all staging unit verification checks passed. The live drop-in was `root:root 0644` with SHA-256 `3488f825f851fb805fb9922b6711d5ebc6d7486bcc8663799c3de3c8a599d453` and added only `ExecCondition=/bin/true` after the reviewed egress condition; this was the initial read-only evidence and no mutation was performed during that pass. The host credential-store file is `root:root 0400` on the unencrypted ext4 root filesystem; `systemd-creds has-tpm2` returned exit `19` with partial capability. No approved at-rest protection was evidenced, so host credential-store remediation or explicit risk acceptance remains required before a canary.

**Override removal note — 2026-09-13:** after a portable host-identity and fail-closed recheck passed, the recorded `99-canary-override.conf` path, `root:root` mode `0644`, SHA-256, and exact directives (`[Service]` plus `ExecCondition=/bin/true`) matched. The file was removed under the separately approved disposition, `systemctl daemon-reload` completed, and effective inspection showed only the reviewed egress condition and `50-egress-allowlist.conf`; `systemd-analyze verify` passed. The faucet remained inactive with `FAUCET_ENABLED=false`, port `8090` closed, egress effective, and the expiry timer active. This resolves the live override drift but does not authorize a canary. The host credential-store risk is formally accepted for exactly one separately approved disposable staging canary only; TPM2/LUKS-backed remediation remains required for production or long-lived staging.

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
3. The configured action is `faucet_claim`.
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

! sudo ss -ltnH | grep -Fq ':8090'
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

sudo test -f "$CURRENT"
sudo install -d -o root -g root -m 0700 "$BACKUP_DIR"
sudo install -o root -g root -m 0600 \
  "$CURRENT" \
  "$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"

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

sudo test -r "$PROTECTED_PLAINTEXT"
sudo systemd-creds encrypt \
  --with-key=host \
  --name=faucet-staging-turnstile.secret \
  "$PROTECTED_PLAINTEXT" \
  "$NEW_CRED"

sudo chown root:root "$NEW_CRED"
sudo chmod 0600 "$NEW_CRED"
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

sudo test -f "$NEW_CRED"
sudo test "$(sudo stat -c '%U:%G %a' "$NEW_CRED")" = 'root:root 600'
sudo mv -T "$NEW_CRED" "$CURRENT"
sudo test "$(sudo stat -c '%U:%G %a' "$CURRENT")" = 'root:root 600'
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

Before any restart, assert the intake flag again:

```bash
sudo grep -Eq '^FAUCET_ENABLED=false$' \
  /etc/ultranet-faucet-staging/faucet.env

sudo systemd-analyze verify \
  /etc/systemd/system/ultranet-staging.service \
  /etc/systemd/system/ultranet-faucet-staging.service
```

Restart only the staging faucet unit, and only after the rotation approval is recorded:

```bash
sudo systemctl restart ultranet-faucet-staging.service
```

Do not restart `caddy.service`, `caddy-staging.service`, `ultranet.service`, or any production faucet unit for this operation. A `daemon-reload` is not required when only the encrypted credential contents changed; use it only if an independently approved unit change also occurred.

Validate the staging projection and disabled configuration:

```bash
sudo systemctl is-active --quiet ultranet-faucet-staging.service

sudo -u ultranet-faucet-staging bash -c '
  set -a
  . /etc/ultranet-faucet-staging/faucet.env
  set +a
  /opt/ultranet-faucet-staging/ultranet-faucet check-config
'

sudo /usr/local/libexec/ultranet-faucet-staging-egress.py \
  --check --effective

sudo grep -Eq '^FAUCET_ENABLED=false$' \
  /etc/ultranet-faucet-staging/faucet.env
```

Expected disabled-validation results:

- systemd service startup successfully loads the encrypted credential projection and passes the effective egress condition;
- standalone `check-config` succeeds for the non-secret staging environment and canonical credential alias; it is not, by itself, a provider-acceptance test;
- `FAUCET_ENABLED=false` remains true;
- signer/address, chain identity, six decimals, fixed amount, and fee policy remain unchanged;
- queue depth, reserved budget, confirmed budget, and claims remain zero;
- `127.0.0.1:8090` is present only if the disabled service is intentionally running for validation;
- no public claim is submitted;
- Caddy is not reloaded or restarted.

If the egress condition or service start fails, do not bypass it. Treat the validation as failed and follow rollback.

## 9. Redacted post-rotation checks

Capture only status, metadata, and redacted logs:

```bash
sudo systemctl --no-pager --full status \
  ultranet-faucet-staging.service

sudo journalctl -u ultranet-faucet-staging.service \
  --since '10 minutes ago' \
  --no-pager

sudo stat -c '%n %U:%G %a' \
  /etc/ultranet-faucet-staging/secrets/*.cred

sudo ss -ltnp
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

If the service is running in disabled mode, use the approved protected operator/status procedure and verify `enabled=false`, zero queue, zero budget reservations, and zero claims. Never paste the operator token into a command or evidence record.

A stopped-service HTTP `502` is not a secret-validation result. It only means that the intentionally stopped loopback upstream is unavailable. Do not start the service merely to change that response.

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

Rollback must keep intake disabled:

```bash
set -euo pipefail

SECRETS_DIR='/etc/ultranet-faucet-staging/secrets'
CURRENT="$SECRETS_DIR/faucet-staging-turnstile.secret.cred"
: "${BACKUP_DIR:?Set BACKUP_DIR to the exact root-only backup directory recorded in Section 5}"
ROLLBACK="$BACKUP_DIR/faucet-staging-turnstile.secret.cred.before"
ROLLBACK_TMP="$SECRETS_DIR/.faucet-staging-turnstile.secret.cred.rollback"

sudo systemctl stop ultranet-faucet-staging.service || true
sudo grep -Eq '^FAUCET_ENABLED=false$' \
  /etc/ultranet-faucet-staging/faucet.env

sudo install -o root -g root -m 0600 "$ROLLBACK" "$ROLLBACK_TMP"
sudo mv -T "$ROLLBACK_TMP" "$CURRENT"
sudo test "$(sudo stat -c '%U:%G %a' "$CURRENT")" = 'root:root 600'

sudo systemctl reset-failed ultranet-faucet-staging.service
! sudo systemctl is-active --quiet ultranet-faucet-staging.service
sudo sha256sum "$CURRENT"
```

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

A later canary requires separate written approval for exactly one disposable claim and all hard gates from [deploy/faucet-staging-claims-enablement.md](../../deploy/faucet-staging-claims-enablement.md). The canary must use a fresh single-use Turnstile token, reconcile the exact payout and node state, and disable intake immediately afterward.

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
- approval and reviewer references.

Never record:

- the Turnstile secret;
- plaintext credential paths if sensitive;
- operator tokens;
- wallet credentials or recovery phrases;
- CAPTCHA tokens;
- idempotency keys;
- signed request envelopes;
- private claim recipient material.

## Definition of Done for this preflight

- [ ] Exact staging widget hostname/action verified against the provider control plane.
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
- [x] Host credential-store residual risk is formally accepted for exactly one disposable staging canary; TPM2/LUKS-backed protection remains required for production or long-lived staging.

**Boundary:** This approved runbook covers only the staging credential rotation and disabled validation. Stop before any canary, intake enablement, production change, or use of an unapproved secret source.
