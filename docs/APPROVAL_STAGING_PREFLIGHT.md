# Validator approval staging preflight

This is the staging gate for the validator-only web approval path. It validates transport, identity mapping, permissions, and the protected API boundary before a controlled two-owner canary. It does not authorize a production approval by itself.

## Security boundary

```text
browser ── HTTPS ──> node API (ultranet)
                         │
                         ├── owner-0 socket group ──> owner-0 signer
                         ├── owner-1 socket group ──> owner-1 signer
                         └── owner-2 socket group ──> owner-2 signer

node user: reaches socket descriptors only
signer user: reads exactly one private owner key
public website: receives no key, nonce, nullifier, digest, or signature array
```

The checked-in file signer is a staging transport fixture. It requires local `APPROVE` presence and must not be made unattended for production. Production approval requires the reviewed HSM or separately administered local-presence adapter.

## 1. Review the release

Build the node and signer from the same source revision. Review the static deployment contract before copying anything to the host:

```bash
cargo fmt --all -- --check
cargo test --locked --lib -- approval_signer
bash scripts/check_approval_staging.sh --static
```

Confirm that the following files are present in the release review:

- `deploy/ultranet-approval-signer@.service`
- `deploy/ultranet-approval-signer@.socket`
- `deploy/ultranet-approval-signer.tmpfiles`
- `deploy/ultranet.service.d/approval-sockets.conf`
- `deploy/sovereign-owner-auth.example.json`
- `scripts/check_approval_staging.sh`

The production signer unit must not contain `--unattended`. The node environment must not contain private-key fields or private key material.

## 2. Provision staging identities

Create three unique groups and three unique non-login signer accounts:

```text
ultranet-approval-owner-0 / ultranet-approver-0
ultranet-approval-owner-1 / ultranet-approver-1
ultranet-approval-owner-2 / ultranet-approver-2
```

Install one private key record per signer at:

```text
/var/lib/ultranet-approval-signer/owner-0/key.json
/var/lib/ultranet-approval-signer/owner-1/key.json
/var/lib/ultranet-approval-signer/owner-2/key.json
```

Each key file must be mode `0600`, owned by its matching signer account, and contain exactly one valid Dilithium-5 keypair. Never copy the all-owner offline key file to the node.

Install the public-only owner mapping at `/etc/ultranet/sovereign-owner-auth.json` as `root:ultranet` mode `0640` (or root-owned mode `0600`). The mapping must contain exactly three entries and the session identifiers must exactly match both node allowlists.

## 3. Install and check the systemd boundary

Install the service, socket, tmpfiles, and node drop-in from the same release. Then reload and enable only the socket units:

```bash
sudo systemd-tmpfiles --create /etc/tmpfiles.d/ultranet-approval-signer.conf
sudo systemctl daemon-reload
for owner in 0 1 2; do
  sudo systemctl enable --now "ultranet-approval-signer@${owner}.socket"
done
sudo systemctl restart ultranet.service
```

The expected runtime properties are:

```text
/run/ultranet-approval-signer/owner-N/       0710 signer-N:owner-group-N
/run/ultranet-approval-signer/owner-N/approval.sock 0660 signer-N:owner-group-N
```

`ultranet` must be a member of all three socket groups through `SupplementaryGroups`. It must be able to connect to every socket but must not be able to read any signer key. A signer account must be able to read only its own key directory.

## 4. Run the automated preflight

Run the host check as root on staging:

```bash
sudo bash scripts/check_approval_staging.sh \
  --api-base-url https://api-staging.example.com
```

The check fails closed if any of these conditions is wrong:

- web approval is not explicitly enabled for staging;
- session cookies are not secure or CORS contains a wildcard/non-HTTPS origin;
- the node environment or owner mapping has unsafe ownership/mode;
- the two session allowlists do not exactly match the three owner sessions;
- a mapping has duplicate/invalid owner indexes, sessions, signer IDs, or socket paths;
- a signer account/group/key is missing or a key has unsafe permissions;
- a socket is not systemd-owned, `0660`, per-owner, or reachable by `ultranet`;
- `ultranet` can read a signer private key;
- a signer unit enables unattended file signing;
- the node is unhealthy or the protected review endpoint does not reject an anonymous request with `401`.

The preflight prints only pass/fail labels and never prints keys, tokens, mapping contents, or response bodies.

## 5. Execute the controlled canary

After the preflight passes, use two different authorized owner sessions:

1. Open one pending proposal in Join Swarm and confirm the complete hash.
2. Verify owner 0's local presence prompt and record the intent as `1/2`.
3. Repeat the exact hash review with owner 1.
4. Verify the server returns the activated state and the node journal contains one final approval activation.
5. Confirm the audit record contains public owner identities only.
6. Confirm browser/network logs contain no private key, admin token, nonce, nullifier, digest, or signature array.

Also exercise negative cases: anonymous review, validator without owner capability, duplicate owner, stale proposal, expired intent, signer outage, nonce contention, and a changed confirmation hash. None may submit an approval transaction.

## 6. Rollback and production gate

If any check or canary step fails:

```bash
sudo systemctl disable --now 'ultranet-approval-signer@*.socket'
sudoedit /etc/ultranet/ultranet.env
sudo systemctl restart ultranet.service
```

Restore the previous node environment and release if required. Keep the offline `ultranet-approve` ceremony available as the break-glass path.

Do not set `ULTRANET_WEB_APPROVAL_ENABLED=true` on production until all of the following are separately signed off:

- the HSM/local-presence signer adapter replaces the staging file adapter;
- socket and key custody have been reviewed by the operators responsible for each owner;
- backup/restore and crash reconciliation have been exercised;
- the two-owner canary and negative cases pass;
- origin proxy/firewall review confirms signer sockets are not exposed through Caddy, Cloudflare, TCP, or public DNS.
