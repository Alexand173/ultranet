# The AppChain owner key — who holds it, where, and what to do when it moves

An AppChain is an L1 object with **one owner address**. That address is the only one that may anchor
the chain or hand it over. There is no admin override, no recovery key, and no operator who can help:
if the key is lost, the chain can still be read by everyone, but nobody can ever anchor it again.

So the question "which key" is not bookkeeping. It decides who controls the chain, and it has exactly
one answer per chain at any moment.

## 1. The two keys people confuse

| | Phone key | Server key |
|---|---|---|
| What it is | The wallet in the UltraVerse app: a 32-byte seed, encrypted with the wallet password, held in the Android Keystore | A Dilithium key file created by `appchain-cli`, encrypted with a passphrase |
| Who uses it | A person, by tapping | A process: the game server, twice a day, for years |
| Where it lives | The phone's secure storage | `/opt/ultranet/owner-key.enc` on the server, `0600` |
| How it signs | In the app, one transaction at a time | `appchain-cli anchor …` from cron or a systemd timer |
| Can it anchor daily? | Only if someone opens the app | Yes — this is what it is for |

Both are just addresses on L1. The chain does not know or care which one is "yours"; it knows which one
signed the registration, and it will keep asking that one.

## 2. The order that cannot be reversed

1. **Create the server key.** On the server:

   ```bash
   appchain-cli generate-key --out /opt/ultranet/owner-key.enc
   # prints the address, e.g. 2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed
   ```

   Nothing owns anything yet. An address with no history is harmless until a transfer names it.

2. **Fund it.** It needs `0.001` per transfer, `0.001` per anchor, `0.01` to register. Send `$ULTRA`
   from the wallet that will perform the transfer.

3. **Transfer the chain to it**, from the wallet that currently owns the chain — in the app
   (*AppChains → the chain → Transfer ownership*) or:

   ```bash
   appchain-cli transfer --key-file owner-key.enc --chain-id 1 --new-owner <address>
   ```

4. **Verify before trusting it:**

   ```bash
   curl -s https://api.ultranetwork.cc/api/appchain/ledger/1 | python3 -c \
     'import json,sys; print(json.load(sys.stdin)["chain"]["owner"])'
   ```

   This must print the server key's address. Until it does, the phone still owns the chain and the
   server cannot anchor.

The order matters: do not delete or wipe the phone wallet until step 4 has confirmed. A handover that
has not landed in a block has not happened.

## 3. Registering "the simple way" — what a normal user actually does

The CLI is **not** part of a normal user's flow. For someone who just wants an AppChain of their own:

1. Open the app, unlock the wallet.
2. AppChains → Register AppChain → type a name → sign.

That is the whole thing. They become the owner with the phone key, and they can anchor by hand from the
chain's screen whenever they want. No CLI, no server, no terminal.

The CLI appears at exactly one moment: when the owner wants something *automated* to sign for them —
a game anchoring its results every day. That is an operator decision, not a user step, and it is
reversible only by another transfer.

## 4. Everyday anchoring from the server

```bash
appchain-cli anchor --key-file /opt/ultranet/owner-key.enc \
  --chain-id 1 --hash "$(cat /var/lib/ultranet-grid/root.hex)"
```

Passphrase sources, in order: `--password-file`, `ULTRANET_APPCHAIN_KEY_PASSWORD`, then an interactive
prompt. For a systemd timer, use a `0600` password file or a credential file — an environment variable
is visible to anything that can read `/proc`, so treat it as the weakest of the three.

The tool refuses to do anything when the node reports no activation height, because every transaction
it could send would be rejected; it reads the fee from the node's own terms rather than assuming it.

## 5. When something goes wrong

| Situation | Reality | Action |
|---|---|---|
| Server key leaked | Whoever holds it can anchor the chain, and can hand it to themselves | Transfer the chain to a newly generated key **from the leaked key** while you still control it, then destroy the old one |
| Server key lost, phone key intact | The phone still owns the chain | Generate a new key and transfer to it from the phone |
| Both keys lost | Nobody can anchor the chain again; this is final | The chain's history stays readable and its anchors stay verifiable. There is no recovery, which is why the transfer exists |
| Transferred by mistake | The recipient owns it now | Only that recipient can transfer it back — ask them |
| `wrong passphrase for this key file` | The file is intact, the passphrase is not | Find the passphrase. Do not regenerate over the file |

The one habit that prevents the worst row: **keeping the phone key is the backup for the server key.**
Transfer to the server, but keep the phone wallet and its recovery phrase until the server has anchored
successfully at least once.

## 6. What is not built yet

- **No delegation.** An owner cannot grant "anchor only" rights to a second key and keep ownership.
  Owning and anchoring are the same right, so an automated signer has to *be* the owner.
- **No timer unit.** The CLI is cron-ready; a `deploy/ultranet-appchain-anchor.{service,timer}` pair is
  not written yet.
- **No key import from the phone.** The app derives its key from a 32-byte seed through PQClean's
  seeded keygen; the CLI uses the crate's random keygen and stores the secret key itself. Both produce
  ordinary Dilithium-5 keys and share the address rule (`hex(SHA3-256(public_key))`), so addresses are
  comparable, but a phone wallet cannot be pasted into the CLI.
