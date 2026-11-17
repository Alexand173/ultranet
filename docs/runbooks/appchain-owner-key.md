# The AppChain owner key — who holds it, where, and what to do when it moves

An AppChain is an L1 object with **one owner address**. That address is the only one that may anchor
the chain or hand it over. There is no admin override, no recovery key, and no operator who can help:
if the key is lost, the chain can still be read by everyone, but nobody can ever anchor it again.

So the question "which key" is not bookkeeping. It decides who controls the chain, and it has exactly
one answer per chain at any moment.

## 1. The two keys people confuse

| | Phone key | Server key |
|---|---|---|
| What it is | The wallet in the UltraVerse app: a 32-byte seed, encrypted with the wallet password, held in the Android Keystore | A Dilithium key file created by `appchain-cli`, encrypted with a passphrase (Argon2id + ChaCha20-Poly1305) |
| Who uses it | A person, by tapping | A process: the Records Service, once a day, for years |
| Where it lives | The phone's secure storage | Staged at `/root/records-staging/records-anchor-owner.key.enc` (`0600`), installed encrypted as `/etc/ultranet-records/secrets/records-anchor-owner.key.enc.cred`, and seen by the service as the credential `records-anchor-owner.key.enc` |
| How it signs | In the app, one transaction at a time | `appchain-cli anchor …`, run by `ultranet-records` (the Games Reward Service asks it to anchor Grid's and Chess's days; it never holds this key) |
| Can it anchor daily? | Only if someone opens the app | Yes — this is what it is for |

Both are just addresses on L1. The chain does not know or care which one is "yours"; it knows which one
signed the registration, and it will keep asking that one.

**The path above is the Records Service's, not a convention.** `ultranet-records.service` loads the owner key
and its passphrase by name, `RecordsServiceConfig` resolves them inside `$CREDENTIALS_DIRECTORY` (defaults
`records-anchor-owner.key.enc` and `records-anchor.pass`, overridable with
`RECORDS_ANCHOR_KEY_CREDENTIAL` / `RECORDS_ANCHOR_PASSWORD_CREDENTIAL`), and hands the key to
`appchain-cli anchor` as `--key-file` with the passphrase as `--password-file`. **The Games Reward Service
never has this key**, and refuses to start if it finds it in its own credentials directory. A key left at some other
path is a key the service never finds, however good its permissions are.

## 2. Handing chain #1 to the server key

**Who signs decides the tool.** Chain #1 `UltraGrid` is currently owned by `7d2cd1cc…f9d9388e` — the
wallet in the UltraVerse app. So the transfer in step 3 is signed **on the phone**. `appchain-cli
transfer` works too, but only for someone who holds the *current* owner's key file, and today that is a
phone seed, not a file. The CLI's job here is the key that comes *after* the handover.

### 2.1 Create the server key

On the games host, as root (`sudo -i`, or an administrator's shell — the blocks carry their own `sudo`),
in a directory only root can read. `generate-key` refuses to overwrite an existing file, which is the
behaviour you want: a second run must never replace the chain's owner.

```bash
sudo install -d -o root -g root -m 0700 /root/records-staging
cd /root/records-staging
sudo bash -c 'head -c 48 /dev/urandom | base64 > records-anchor.pass && chmod 600 records-anchor.pass'
sudo ULTRANET_APPCHAIN_KEY_PASSWORD="$(sudo cat /root/records-staging/records-anchor.pass)" \
  /opt/ultranet-records/appchain-cli generate-key --out /root/records-staging/records-anchor-owner.key.enc
```

It prints the address on stdout (`wrote …; this key owns nothing until a transfer names it` goes to
stderr). Note that address; confirm it with
`appchain-cli address --key-file …/records-anchor-owner.key.enc --password-file …/records-anchor.pass`, which
prints the address and public key in clear and needs the passphrase.

Nothing owns anything yet. An address with no history is harmless until a transfer names it.

> The passphrase is passed through the environment for that one command, which is the weakest of the
> three sources (anything that can read `/proc` sees it). That is acceptable for a key that owns nothing
> yet on a host you control, and it is why the passphrase is generated from `/dev/urandom` into a file
> rather than typed twice. Do not carry the habit to `anchor` or `transfer`, which read
> `--password-file`.

### 2.2 Fund it for anchoring — not for the transfer

**The transfer fee is paid by the wallet that sends the transfer, not by the one that receives it.** So
the new address needs no balance to accept the chain. It needs a balance to *anchor* afterwards:
0.001 `$ULTRA` per anchor, one anchor a day per chain, so **1 `$ULTRA` is about 1,000 days**. Confirm it
before going further, because an unfunded owner key accepts the chain and then fails every anchor:

```bash
curl -s https://api.ultranetwork.cc/api/account/<server-address> | python3 -m json.tool
```

Send that 1 `$ULTRA` from **your own wallet** (Send in the app). Do **not** reach for
`send_ultra.py`: it is hard-wired sovereign → faucet, has no `--recipient`, and is deliberately not a
general sender.

### 2.3 Transfer the chain, from the phone

*AppChains → UltraGrid → Transfer ownership →* paste the server address → *Sign & transfer*. Fee 0.001
`$ULTRA`, charged to the signing wallet (which held 0.089 at the time of writing). Or, from a machine
that holds the current owner's key file:

```bash
appchain-cli transfer --key-file <current-owner-key> --chain-id 1 --new-owner <server-address>
```

### 2.3.1 Verify it on the ledger — a handover not in a block has not happened

```bash
curl -s https://api.ultranetwork.cc/api/appchain/ledger/1 | python3 -c \
  'import json,sys; print(json.load(sys.stdin)["chain"]["owner"])'
```

This must print the server key's address, character for character. Until it does, the phone still owns
the chain and the service cannot anchor.

### 2.3.2 Prove the old owner is locked out, while nothing is at stake

Try one anchor from the phone. It must be **refused**, with a message saying only the owner of the chain
may anchor it. A rejected transaction costs no fee, so this check is free — and it is the difference
between a transfer that landed and one that merely looked as if it did. Skipping it is how a half-done
handover survives until the first day that must be anchored.

### 2.4 Install the key as the service credential, and only then destroy the plaintext

```bash
sudo systemd-creds setup
sudo install -d -o root -g ultranet-records -m 0750 /etc/ultranet-records/secrets
for name in records-anchor-owner.key.enc records-anchor.pass; do
  sudo systemd-creds encrypt --with-key=host --name="$name" \
    "/root/records-staging/$name" "/etc/ultranet-records/secrets/$name.cred"
done
sudo shred --remove /root/records-staging/records-anchor-owner.key.enc /root/records-staging/records-anchor.pass
```

**Before running the shred, take the offline copies** — the encrypted key file and its passphrase, in two
different places, apart from each other. Once the chain is handed over, that file *is* the chain: lose
both and AppChain #1 can never be anchored again, by anyone, ever. Credential encryption protects a
stolen disk or backup; it does not protect against a compromised running host, because the service has to
be able to decrypt it unattended. That is the cost of the arrangement, and it is why this key must be
dedicated to anchoring and to nothing else.

### 2.5 Start the service and watch the first day close

```bash
# /etc/ultranet-records/records.env   (the Records Service holds the owner key)
RECORDS_ANCHOR_CLI=/opt/ultranet-records/appchain-cli
RECORDS_PEER_CREDENTIAL=records-peer-games.key
# /etc/ultranet-games/games.env       (the Games Reward Service asks it; it holds no owner key)
GAMES_RECORDS_URL=http://127.0.0.1:8096
GAMES_ANCHOR_GAMES=grid
```

Start the Records Service first (`sudo systemctl start ultranet-records`), then the Games Reward Service
(`sudo systemctl start ultranet-games`), read the rules each prints, and watch the first day close. The
Games service closes yesterday by itself, asks the Records Service to anchor it, and retries every ten
minutes until the anchor lands, so no command is needed — only attention:

```bash
journalctl -u ultranet-games -u ultranet-records -f | grep -i -e anchor -e "day "
curl -s https://api.ultranetwork.cc/api/appchain/ledger/1/anchors | python3 -m json.tool
```

The ledger must show the new root with the next sequence number. An owner-key problem shows up here and
nowhere earlier — common causes are a wrong owner (2.3.1), an empty owner wallet (2.2), and a key the unit
cannot find (2.4). Note that `close-day` by hand needs the same configuration and credentials the unit
has, so it is not a plain-shell command; only `status`, `audit`, `payouts` and `backup` are.

### 2.6 The order, on one screen

| # | Do | If you skip it |
|---|---|---|
| 1 | Create the key, note the address | Nothing to transfer to |
| 2 | Fund it for anchors | The chain moves, then every anchor fails |
| 3 | Transfer, signed by the current owner | — |
| 4 | Verify the owner on the ledger | You do not know who owns the chain |
| 5 | Prove the old key is refused | A failed transfer looks like a successful one |
| 6 | Install as credentials, copy offline, shred staging | The service cannot find or decrypt it, or an offline copy is lost |
| 7 | Watch the first day land | The first automatic anchor is the test |

Keep the phone wallet and its recovery phrase until step 5 passes. After that the phone is just a viewer:
it can read the chain, and it can no longer anchor it.

## 3. Registering "the simple way" — what a normal user actually does

The CLI is **not** part of a normal user's flow. For someone who just wants an AppChain of their own:

1. Open the app, unlock the wallet.
2. AppChains → Register AppChain → type a name → sign.

That is the whole thing. They become the owner with the phone key, and they can anchor by hand from the
chain's screen whenever they want. No CLI, no server, no terminal.

The CLI appears at exactly one moment: when the owner wants something *automated* to sign for them —
a game anchoring its results every day. That is an operator decision, not a user step, and it is
reversible only by another transfer.

## 4. Everyday anchoring

For UltraGrid the service does it: one anchor a day per configured game, at 0.001 `$ULTRA` each, taken
from the **owner wallet** — the same wallet the key derives from, which must hold a balance. Manual
anchoring is the exception, for a day that failed or a chain with no service behind it.

The root is the one the service stored for that day — `close-day` prints it, and it is what
`GET /api/appchain/ledger/1/anchors` shows once the anchor lands. Anchor with that hex, never with a root
you computed elsewhere, because the ledger must agree with the record the service kept.

```bash
# with the key file still in hand (before step 2.4's shred):
appchain-cli anchor --key-file /root/records-staging/records-anchor-owner.key.enc \
  --chain-id 1 --hash <the day root> \
  --password-file /root/records-staging/records-anchor.pass

# once both are credentials, decrypt each to a root-only file, anchor, and shred the two files:
sudo systemd-creds decrypt --with-key=host --name=records-anchor-owner.key.enc \
  /etc/ultranet-records/secrets/records-anchor-owner.key.enc.cred /root/owner-key.tmp
sudo systemd-creds decrypt --with-key=host --name=records-anchor.pass \
  /etc/ultranet-records/secrets/records-anchor.pass.cred /root/owner-key.pass.tmp
sudo chmod 600 /root/owner-key.tmp /root/owner-key.pass.tmp
appchain-cli anchor --key-file /root/owner-key.tmp --chain-id 1 --hash <the day root> \
  --password-file /root/owner-key.pass.tmp
sudo shred --remove /root/owner-key.tmp /root/owner-key.pass.tmp
```

Passphrase sources, in order: `--password-file`, `ULTRANET_APPCHAIN_KEY_PASSWORD`, then stdin. For
anything unattended use a `0600` password file or a credential file — an environment variable is visible
to anything that can read `/proc`, so treat it as the weakest of the three.

The tool refuses to do anything when the node reports no activation height, because every transaction it
could send would be rejected; it reads the fee from the node's own terms rather than assuming it.

## 5. When something goes wrong

| Situation | Reality | Action |
|---|---|---|
| Server key leaked | Whoever holds it can anchor the chain, and can hand it to themselves | Transfer the chain to a newly generated key **from the leaked key** while you still control it, then destroy the old one |
| Server key lost, phone key intact | The phone still owns the chain **only if the transfer never landed** — check the ledger | If the ledger still names the phone, generate a new key and transfer to it from the phone. If it names the lost server key, this is final |
| Both keys lost | Nobody can anchor the chain again; this is final | The chain's history stays readable and its anchors stay verifiable. There is no recovery, which is why the transfer exists |
| Transferred by mistake | The recipient owns it now | Only that recipient can transfer it back — ask them |
| `wrong passphrase for this key file` | The file is intact, the passphrase is not | Find the passphrase. Do not regenerate over the file — `generate-key` refuses to overwrite, so a second attempt is a second identity |
| Anchors fail but the transfer landed | Usually the owner wallet is empty, or the service cannot read the key | Check the balance (2.2), then `journalctl -u ultranet-records` for the credential error |

The one habit that prevents the worst row: **keeping the phone key is the backup for the server key.**
Transfer to the server, but keep the phone wallet and its recovery phrase until the server has anchored
successfully at least once.

## 6. What is not built yet

- **No delegation.** An owner cannot grant "anchor only" rights to a second key and keep ownership.
  Owning and anchoring are the same right, so an automated signer has to *be* the owner. This is why a
  leaked server key is also the ability to hand the chain away.
- **No generic anchor timer.** The Records Service anchors the chains, UltraGrid and Chess on the Games service's request; a
  standalone `deploy/ultranet-appchain-anchor.{service,timer}` for chains with no service still does not
  exist.
- **No key import from the phone.** The app derives its key from a 32-byte seed through PQClean's seeded
  keygen; the CLI uses the crate's random keygen and stores the secret key itself. Both produce ordinary
  Dilithium-5 keys and share the address rule (`hex(SHA3-256(public_key))`), so addresses are
  comparable, but a phone wallet cannot be pasted into the CLI.
