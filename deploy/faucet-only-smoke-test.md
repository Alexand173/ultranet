# Faucet-only loopback smoke test

This checklist is for a disposable faucet-test VPS only. It exercises the faucet service without connecting to production UltraNet, production DNS, production Cloudflare, production credentials, production state, or production funds.

The initial test target is the separately provisioned VPS whose identity must be confirmed by the operator before use. The first phase uses the repository's built-in `ultranet-faucet preview` harness. It is the preferred smoke test for a small host because it uses an in-process mock node, a disposable signer, temporary SQLite state, and a static test CAPTCHA verifier.

This is not a production deployment guide and does not authorize a real-value claim.

## Scope

```text
included:  faucet binary, loopback preview, API contract, idempotency,
           claim-state transitions, operator boundary, disablement, cleanup
excluded: production node, production faucet, dashboard, approval signer,
          public DNS, Cloudflare, Caddy, public claims, production funds,
          sovereign keys, validator database, production admin credentials
```

The preview process binds only to a loopback address such as `127.0.0.1:18090`. Do not expose that port through a firewall, reverse proxy, DNS record, or SSH reverse tunnel.

## Safety gates

Before copying an artifact or starting a process, confirm all of the following in the private maintenance record:

- The VPS is the intended disposable test host and its public IP is not the documented production origin `167.233.161.115`.
- The host is not being used for the production validator, production faucet, dashboard, approval signer, or any production credential.
- No command below references `api.ultranetwork.cc`, `faucet.ultranetwork.cc`, a production faucet address, `sovereign_keys.json`, `send_ultra.py`, `ULTRANET_ADMIN_TOKEN`, `/var/lib/ultranet`, `/etc/ultranet`, or production encrypted credentials.
- The approved source revision is known and the artifact was built from a clean checkout of that revision.
- The test port is unused locally and remotely.
- The operator understands that `preview` is a mock-node test. It does not prove production-node integration, Turnstile behavior, proxy routing, or real-funds reconciliation.
- No private key, wallet password, recovery phrase, Turnstile secret, abuse key, operator token, or signed envelope will be pasted into chat, shell history, ordinary logs, or test artifacts.

The runtime's normal `serve` command requires a real loopback node API and real Turnstile credentials. Do not switch from `preview` to `serve` as a shortcut. A real disposable-node test is a separate gated procedure described below.

## Recorded baseline for the new VPS

The read-only baseline recorded on 2026-09-02 was:

| Check | Result | Gate |
| --- | --- | --- |
| Public IPv4 | `2.28.61.71` | Must remain distinct from production |
| Hostname | `ubuntu-2gb-nbg1-2` | Confirm before use |
| OS | Ubuntu `24.04.4 LTS` | Record with evidence |
| Kernel | Linux `6.8.0-138-generic`, x86_64 | Record with evidence |
| CPU | 1 vCPU | Preview only is the safe default |
| Memory | 1.9 GiB total, 1.5 GiB available at check time | Do not assume full-node capacity |
| Swap | 0 B | Do not add without a separate approval |
| Root disk | 38 GiB total, 33 GiB available | Preserve headroom |
| Root inodes | 6% used | No pressure observed |
| Listening TCP | SSH on port 22; no UltraNet listener | Preview must use a loopback-only test port |
| Firewall | UFW inactive | Do not expose a test listener |
| UltraNet services | Production/staging node and faucet units inactive | Clean starting point |

Re-run the non-mutating identity/capacity inventory immediately before the test if the host has changed. Do not interpret the baseline as proof that the host can build or sustain a validator.

## Post-resize baseline for the same disposable VPS

A second non-mutating inventory was run on 2026-09-02 after the operator's
provider-console change. The host identity and public IPv4 remained stable:

| Check | Result | Interpretation |
| --- | --- | --- |
| Public IPv4 | `2.28.61.71` | Preserved; still distinct from production |
| Hostname | `ubuntu-2gb-nbg1-2` | Preserved |
| OS/kernel | Ubuntu `24.04.4 LTS`; Linux `6.8.0-138-generic`, x86_64 | Same guest baseline |
| CPU | 4 vCPU, AMD EPYC-Genoa | Improved from the original 1 vCPU |
| Memory | 7.6 GiB total, 7.1 GiB available at check time | Matches the documented 8-GiB-class minimum reference, not the 32-GiB sustained recommendation |
| Swap | 0 B | No swap was added or changed |
| Root disk | 38 GiB total, 3.2 GiB used, 33 GiB available | Disk headroom remains adequate for a disposable artifact/state test |
| Root inodes | 6% used | No inode pressure observed |
| Load | `0.00, 0.00, 0.00` | Host was idle during the check |
| Listening TCP/UDP | SSH on `22`; local DNS/DHCP only | No UltraNet listener or faucet listener |
| Firewall | UFW inactive; no nftables ruleset was reported | Do not expose a node listener until a separate network policy is approved |
| UltraNet services | All four expected units inactive/not found | Clean starting point |

The provider screenshot displayed the label `CPX32 | x86 | 40 GB`, but no provider
control-plane/API credentials are connected to this workspace. Treat the plan
label as operator-supplied context and verify the provider-side server ID,
RAM/CPU/disk profile, resize completion, and rollback capability out of band.
The guest measurements above are the authoritative capacity evidence for the
next gate. They do not authorize a build, service start, firewall change, or
real claim.

## Required non-secret variables

Set these only in a private maintenance shell. They are identifiers, not credentials:

```bash
export TEST_VPS_IP="<approved-test-vps-ip>"
export APPROVED_REVISION="<approved-40-character-commit-sha>"
export PREVIEW_REMOTE_BIND="127.0.0.1:18090"
export PREVIEW_LOCAL_BIND="127.0.0.1:28090"
```

Use a local SSH configuration with strict host-key checking. Do not use `StrictHostKeyChecking=no`.

## Phase 0 — verify the artifact without building on the small VPS

Prefer building on a stronger controlled machine and copying only the release binary to a disposable `/tmp` path on the test host. The one-CPU/1.9-GiB host should not be treated as a safe Rust build worker for the full UltraNet workspace.

From the approved clean checkout:

```bash
set -euo pipefail
test "$(git rev-parse HEAD)" = "$APPROVED_REVISION"
test -z "$(git status --porcelain --untracked-files=all)"
cargo build --release --locked --bin ultranet-faucet
export ARTIFACT_SHA256="$(sha256sum target/release/ultranet-faucet | awk '{print $1}')"
test "${#ARTIFACT_SHA256}" -eq 64
printf '%s\n' "$ARTIFACT_SHA256"
```

Record the full revision and checksum. Keep `ARTIFACT_SHA256` in the same controlled client shell for the transfer comparison. Do not copy the repository, `target/` tree, environment files, key bundles, database, or deployment secrets to the VPS.

After copying the binary to a disposable remote path, compare its checksum programmatically and verify metadata only:

```bash
set -euo pipefail
: "${ARTIFACT_SHA256:?Keep the locally calculated artifact checksum in this shell}"
scp -o BatchMode=yes -o StrictHostKeyChecking=yes \
  target/release/ultranet-faucet \
  "root@$TEST_VPS_IP:/tmp/ultranet-faucet-preview"
ssh -o BatchMode=yes -o StrictHostKeyChecking=yes root@$TEST_VPS_IP \
  "set -euo pipefail; \
   test -f /tmp/ultranet-faucet-preview; \
   chmod 0755 /tmp/ultranet-faucet-preview; \
   actual=\$(sha256sum /tmp/ultranet-faucet-preview | awk '{print \$1}'); \
   test \"\$actual\" = '$ARTIFACT_SHA256'; \
   printf '%s\\n' \"remote_artifact_sha256=\$actual\"; \
   file /tmp/ultranet-faucet-preview"
```

The binary copy is disposable test state. Remove it during cleanup. Never install it under `/opt/ultranet`, `/opt/ultranet-faucet`, or a production systemd path for this smoke test.

## Phase 1 — built-in preview smoke test

### Start and keep the service private

Start the preview with a loopback bind only. Keep the SSH session alive while testing; do not daemonize it into a persistent service:

```bash
ssh -T root@$TEST_VPS_IP \
  env FAUCET_PREVIEW_BIND="$PREVIEW_REMOTE_BIND" \
  /tmp/ultranet-faucet-preview preview
```

The process should print only a preview address and temporary state directory, followed by:

```text
Preview only: no live node, Turnstile, or production credentials are used.
```

Do not treat the generated preview signer as a deployable credential. The preview removes its temporary directory after clean shutdown.

In a second private terminal, create an SSH local forward only for test traffic. This does not publish the remote loopback listener:

```bash
ssh -T -N \
  -L "$PREVIEW_LOCAL_BIND:$PREVIEW_REMOTE_BIND" \
  root@$TEST_VPS_IP
```

If local port forwarding is unavailable, run the same assertions through `ssh root@$TEST_VPS_IP curl ...` against `127.0.0.1:18090`. Never bind the preview to `0.0.0.0` or the VPS public address.

### Endpoint assertions

Use a deterministic valid destination that is not the generated preview faucet address. The preview prints its source address; compare before submitting. Use a fresh high-entropy idempotency key for the first request.

```bash
set -euo pipefail
PREVIEW_URL="http://127.0.0.1:28090"
DESTINATION="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
IDEMPOTENCY_KEY="preview-$(openssl rand -hex 16)"

curl --fail --silent --show-error \
  "$PREVIEW_URL/api/faucet/status"

CLAIM_RESPONSE="$(curl --fail --silent --show-error \
  -X POST "$PREVIEW_URL/api/faucet/claims" \
  -H 'Content-Type: application/json' \
  -H "Idempotency-Key: $IDEMPOTENCY_KEY" \
  -d "{\"address\":\"$DESTINATION\",\"captcha_token\":\"preview-test-token\"}")"
CLAIM_ID="$(printf '%s' "$CLAIM_RESPONSE" | \
  python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["claim_id"])')"
test "${#CLAIM_ID}" -eq 32
```

Capture and inspect only the public response fields in memory. Assert:

- `GET /api/faucet/status` returns `200` with six decimals and the fixed preview policy.
- The valid claim returns `202 Accepted`.
- The response contains a claim ID, fixed amount `1,000,000` base units, and display amount `1.000000`.
- The response does not contain `private_key`, `secret_key`, `signature`, `captcha_token`, signed envelope bytes, or operator credentials.
- No request body or secret is written to a persistent artifact. `CLAIM_RESPONSE` is a short-lived shell variable containing only the public JSON response.

Poll the claim endpoint until the preview reports `confirmed`:

```bash
curl --fail --silent --show-error \
  "$PREVIEW_URL/api/faucet/claims/<claim-id>"
```

Assert that the final public state contains a transaction hash, `confirmed` status, the fixed amount, and six decimals.

### Idempotency and policy assertions

1. Repeat the exact destination, CAPTCHA value, and idempotency key. Require the original claim ID and no second payout.
2. Reuse that idempotency key with a different valid destination. Require `409 Conflict` and no new payout.
3. Submit a second fresh-key claim for the original destination during its cooldown. Require the cooldown conflict.
4. Send an unknown JSON field. Require request rejection.
5. Send a malformed/uppercase/incorrect-length address. Require request rejection.
6. Send a request without `Idempotency-Key` or with the wrong content type. Require request rejection.
7. Request `/internal/status` without authorization. Require `401`.
8. Use the preview-only static operator token `local-preview-operator-token-v1` from `src/faucet/preview.rs` to inspect `/internal/status`; this literal is test-only and must never be used for a real service.
9. Call the preview-only `/internal/disable`, then confirm public status reports disabled and a new claim is rejected. Do not call `/internal/enable` in any real environment as part of this checklist.
10. Confirm the preview process remains bound to loopback and no public port was opened.

The preview's static CAPTCHA verifier is test-only. Passing these checks does not validate Cloudflare Turnstile, a real node, a production wallet, or a public reverse proxy.

### Stop and cleanup

Stop the preview process and forwarding session deliberately with an interrupt, wait for the SSH command to exit, and confirm the exact temporary preview directory printed at startup is gone. Remove only the exact disposable binary path used in Phase 0:

```bash
PREVIEW_BINARY=/tmp/ultranet-faucet-preview
PREVIEW_PID="<pid-from-preview-output>"
PREVIEW_STATE_DIR="/tmp/ultranet-faucet-preview-$PREVIEW_PID"
ssh -T root@$TEST_VPS_IP \
  "set -euo pipefail; \
   test ! -e '$PREVIEW_STATE_DIR'; \
   rm -f '$PREVIEW_BINARY'; \
   test ! -e '$PREVIEW_BINARY'"
```

If the preview was terminated forcefully and its directory remains, inspect the exact printed path, confirm no preview process still exists, remove only that directory, and rerun the verification block. Do not use a wildcard or recursive deletion against broad system directories. Record only redacted status codes, public claim lifecycle fields, checksum, process exit, and cleanup evidence.

## Phase 2 — optional disposable-node integration test

Do not begin Phase 2 merely because Phase 1 passed. This VPS has only 1.9 GiB RAM, one vCPU, and no swap; it may not be able to build or sustain a validator.

Phase 2 requires a separate written capacity decision and all of these prerequisites:

- A release `UltraNet` node artifact and faucet artifact built from the same clean locked revision.
- A separately reviewed disposable validator unit/configuration. The repository's production `deploy/ultranet.service` must not be copied unchanged.
- A disposable node database/state directory, chain/genesis/network identity, P2P identity, service account, API bind, and peer policy. The node API must be `127.0.0.1:8081` and must not use production peers, storage, genesis, or admin credentials.
- A fresh faucet signer-derived address that is not the production faucet address.
- A separate faucet SQLite path and separate encrypted signer, abuse, operator, and non-production Turnstile credentials.
- Outbound Turnstile access and an exact non-production widget/hostname/action if testing the normal `serve` path. If strict no-external-connectivity is required, stop after Phase 1; the current runtime does not provide a production-like local CAPTCHA mode for `serve`.
- Explicit approval to create a disposable claim. A real-node claim is not implied by preview approval.

The current node startup also creates a libp2p listener with a hard-coded `0.0.0.0:9000` bind. An empty `ULTRANET_PERSISTENT_PEERS` value does not prevent that listener or all peer-discovery behavior. A disposable-node deployment therefore needs a separately reviewed host firewall/network policy that blocks unneeded external TCP/UDP `9000` traffic and prevents production peer connectivity. The node's ZK and recursive-ZK initialization also makes the current 1.9-GiB host unsuitable as an assumed full-node build/runtime target.

If any prerequisite is missing, record Phase 2 as blocked. Do not point the faucet at a remote node to bypass the loopback validation; `FAUCET_NODE_API_BASE_URL` is required to target loopback.

### Phase 2 status after the post-resize baseline

The resize clears the original 1.9-GiB/1-vCPU capacity blocker to a conditional
8-GiB-class capacity gate, but Phase 2 remains blocked until the following
steps are completed in order:

1. Verify the provider-side CPX32 server identity and resize record out of band.
2. Build the node and faucet from one clean locked revision on a stronger
   controlled machine; record both checksums and copy only the binaries.
3. Install the reviewed `deploy/ultranet-staging.service` and
   `deploy/ultranet.env.staging-test.example` only after the runtime supports
   `ULTRANET_ADMIN_TOKEN_FILE`, `ULTRANET_P2P_BIND`, empty bootnodes, and the
   disposable network profile. Keep the node admin token out of the faucet.
4. Apply a separately reviewed host network policy. The node must not reach the
   production bootnode or expose P2P publicly; UFW is currently inactive and
   the node's historical default was a `0.0.0.0:9000` listener.
5. Run the node-only preflight: configuration, file-backed admin auth, fresh
   state/identity, loopback API, loopback P2P bind, empty peer/bootstrap set,
   `/api/validate`, six-decimal accounting, no production log references, and
   service-user filesystem isolation.
6. Keep the faucet disabled and stop here unless every node-only assertion
   passes. A separate written approval is required before loading non-production
   faucet credentials or enabling intake for one claim.
7. If the claim gate is approved, perform exactly one disposable claim and the
   existing reconciliation/idempotency/disablement/cleanup matrix. Any failure
   stops the test and leaves intake disabled.

The resized host still has no swap and only 7.6 GiB total RAM; do not call it a
sustained validator environment or compile the full workspace there by default.

### Node-only preflight attempt status

A non-mutating node-only preflight was run on 2026-09-03. It stopped before any
installation or service start because the required provider confirmation and
staging artifacts were not present in the execution environment.

Observed gates:

- Provider CLI/API access and provider credentials were not available locally;
  the guest measurements confirm the enlarged profile, but the provider-side
  CPX32 server/resize record remains an out-of-band verification item.
- The VPS still had no `/opt/ultranet-staging/target/release/UltraNet`, staging
  environment file, encrypted admin credential, staging state directory, or
  installed `ultranet-staging.service`.
- UFW was inactive and the nftables ruleset was empty. Only SSH and local
  resolver/DHCP listeners were present.
- Production and staging UltraNet/faucet units were inactive or not found.

No binary was copied, source was built on the VPS, state or credentials were
created, systemd was changed, firewall rules were changed, a node was started,
or faucet/production connectivity was enabled. Phase 2 remains blocked until
provider identity is confirmed, artifacts are built from one locked revision,
and the reviewed node network policy and credential-install procedure are
explicitly approved.

### Phase 2 setup order when unblocked

1. Choose a larger disposable host or obtain an explicit capacity exception. Use the existing 8-GiB validator profile as the minimum practical reference for a controlled node experiment; 32 GiB remains the documented recommendation for sustained validator workloads. Do not add swap or relax limits as an unreviewed workaround.
2. Build `UltraNet` and `ultranet-faucet` on a stronger controlled machine from the same clean locked revision. Record both artifact checksums and copy only the release binaries.
3. Create a dedicated disposable node account, state directory, and unit. Set `ULTRANET_DB_PATH` to that directory, `ULTRANET_API_BIND=127.0.0.1:8081`, `ULTRANET_PAUSE_ON_ERROR=false`, and an exact non-wildcard local CORS origin if one is needed. Generate a node-only `ULTRANET_ADMIN_TOKEN` separately; never give it to the faucet.
4. Start with no `ULTRANET_PERSISTENT_PEERS`, a fresh node database, a fresh node P2P identity, no production genesis/storage/peers, and the reviewed firewall policy for the node's `0.0.0.0:9000` listener. Verify `/api/validate` before starting the faucet.
5. Create a separate faucet account/state directory and four encrypted faucet credentials. Generate a fresh signer-derived address and set it in a dedicated environment file with `FAUCET_NODE_API_BASE_URL=http://127.0.0.1:8081`, loopback bind, `FAUCET_ENABLED=false`, a small disposable daily cap, and the exact approved non-production Turnstile hostname.
6. Install the faucet under its dedicated user and unit, run `check-config`, verify service-user filesystem isolation, and confirm the local status endpoint reports disabled before any enablement.
7. Obtain explicit approval for one disposable claim, enable only the disposable faucet, perform the exact reconciliation/idempotency assertions below, disable intake immediately, preserve redacted evidence, and remove the disposable state.

If all gates are approved, follow the separately reviewed staging-node procedure and then require, in order:

1. Node `/api/validate` reports the intended disposable chain and `decimals=6`.
2. Node account, nonce, and fee-estimate responses are coherent for the fresh faucet address.
3. Faucet `check-config` passes as the dedicated faucet account with `FAUCET_ENABLED=false` and no credential contents printed.
4. Service isolation proves the faucet account cannot read node state, production paths, or unrelated credentials.
5. Exactly one disposable-wallet claim is submitted only after explicit enablement.
6. The claim reaches `confirmed` and reconciles exactly across public claim state, node hash, amount, fee, nonce, source debit, recipient balance, and SQLite state.
7. Same-key replay returns the original claim without a second debit; changed-destination replay returns `409`.
8. Intake is disabled immediately after reconciliation and both disposable services are stopped.

Never use the repository's continuous `simulator` binary for this test.

## Evidence and rollback

Preserve a short test record containing:

- host identity and capacity baseline;
- approved source revision and artifact checksum;
- loopback bind and port;
- public status code/fields;
- claim status transition and idempotency outcomes;
- operator authentication and disablement outcomes;
- process exit and temporary-state cleanup;
- explicit Phase 2 pass or block reason.

Before sharing evidence, scan it for private keys, secret values, authorization headers, CAPTCHA tokens, wallet passwords, recovery phrases, raw request bodies, signed envelopes, and full operator tokens.

Stop immediately and keep the faucet disabled if:

- the host identity is unexpected;
- any configuration references production or a non-loopback node;
- the preview binds publicly;
- the host becomes unstable or is OOM-killed;
- any secret appears in output or an unsafe file;
- a duplicate idempotency request causes another payout;
- a changed-fingerprint request is accepted;
- an unsupported route is exposed; or
- a command would modify production DNS, Cloudflare, firewall, proxy, systemd, database, credentials, or funds.

## Definition of Done

- [ ] The test host identity and capacity are rechecked immediately before execution.
- [ ] Phase 1 preview status, claim, confirmation, duplicate idempotency, changed-fingerprint conflict, cooldown, malformed-input, operator-authentication, disablement, loopback, and cleanup checks pass.
- [ ] The approved artifact revision and checksum are recorded without copying source or production secrets.
- [ ] Phase 2 is either completed against a separately proven disposable node with exact reconciliation or explicitly blocked by capacity/infrastructure/CAPTCHA prerequisites.
- [ ] No production endpoint, credential, state, proxy, DNS, firewall, service, or funds are contacted or modified.
