# Wallet migration rehearsal — debug install → production-signed 0.2.0

**Status: procedure prepared; packaging half proven on the emulator; restore not yet rehearsed.**
The rehearsal is the last thing standing between the signed APK (A3) and distribution. Nothing here has
been run against a real wallet — the unaudited half is exactly steps 2 and 4, which need the operator's
phrase and therefore cannot be rehearsed by anyone else. Everything up to and including "the release
build is installed, production-signed, with the old wallet destroyed" has been executed and recorded at
the end of this document.

| | |
|---|---|
| Artefact under test | `android-app/app/build/outputs/apk/release/app-release.apk` |
| sha256 | `ed8363ed121884529edf6ba4f90f52ff7a0fb40ac0a3b29408e18c14fbd11d27` |
| versionCode / versionName | `2` / `0.2.0` |
| Signer | `CN=UltraNet Release, OU=Android, O=UltraNet` |
| Certificate SHA-256 | `cc a42165fb74fac25fd3fc838d5a40cdfe0471bdb7cff6ef161835136b287ac0` |
| Harness | [`android-app/rehearse-wallet-migration.sh`](../../android-app/rehearse-wallet-migration.sh) |
| Evidence | [`./evidence/`](./evidence/) — one file per phase, per device |

## Why this rehearsal exists

The stored wallet lives in `no_backup/ultranet-wallet-v1.json` — written through `AtomicFile` into
`context.noBackupFilesDir` (`EncryptedWalletStore.kt:10`). Two consequences, both of which this
rehearsal has to survive:

1. **Android's auto-backup cannot save it.** The `no_backup` directory is excluded by construction, so
   there is no cloud copy to fall back on.
2. **Uninstall destroys it.** Nothing outside the device holds a copy.

The address is stored in clear text in that file; the seed is encrypted under the user's password. So a
user who reinstalls without their recovery phrase does not lose an app — they lose the wallet, and the
node has no way to recover it (`CONTRACT.md` §11.5).

The debug-signed build cannot be upgraded in place to the production-signed one: same `applicationId`,
different certificate. Verified on 2026-11-25 by installing the debug APK over the release install:

```
Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Existing package cc.ultranet.ultraverse signatures
do not match newer version; ignoring!]
```

So **uninstall is mandatory**, and the phrase is the only bridge.

## Safety rules — non-negotiable

- **The recovery phrase never touches this machine.** Not in a shell argument, not in a file, not in an
  environment variable, not in a screenshot. The harness refuses to run if `MNEMONIC`, `RECOVERY_PHRASE`,
  `PHRASE`, `SEED`, `SEED_PHRASE` or `WALLET_PHRASE` is set, and it prints only the wallet address.
  The phrase is typed on the device, by the operator, into the app.
- **Use a dedicated test device**, never the only device holding the real wallet.
- **Two phrases are available, use the right one.** For the mechanism itself, a throwaway rehearsal
  phrase (never funded, never used in production) is sufficient and safest. To also check balances and
  history against a baseline, the device must carry a funded wallet — then the phrase is the real one
  and the risk is real. Decide before starting, and write down which was used.
- **No production transaction is signed or broadcast.** Any submission test points at an unreachable
  node URL so signing is exercised and transport fails — never at `api.ultranetwork.cc`.
- **No service, node or deployment action.** This rehearsal touches one Android device and nothing else.
- **The APK is not distributed** until the acceptance table below is signed.

## Preconditions

1. Device attached, `adb devices` shows **exactly one** entry (the harness refuses to guess otherwise).
2. Recovery phrase in the operator's hands, offline, verified word-for-word, before step 3.
3. `ANDROID_HOME` / `JAVA_HOME` available, or the repo's `.toolchains/` used (the harness defaults to them).
4. Baseline balances and history captured for the wallet in question (step 1).

## The rehearsal

### Step 1 — pre-migration baseline

```bash
cd android-app
./rehearse-wallet-migration.sh baseline
```

Read-only. Records device, Android version, installed versionName/versionCode, install times, the
installed signer (read from the on-device `base.apk`), whether the wallet file exists, and its address.
Writes `docs/runbooks/evidence/baseline-<serial>-<utc>.md`.

Also record by hand, into that file's table or next to it: the wallet's $ULTRA balance and its recent
statement entries — the app shows both once unlocked. This is the comparison baseline for step 4.

### Step 2 — verify the phrase can reproduce the address, before destroying anything

The phrase's derivation is already pinned by device tests (`WalletCryptoParityTest`,
`WalletStoreTest`, `AppChainSignerTest` — 39 tests across nine classes, green on 2026-11-25), which is
what makes this step a cross-check rather than a discovery: the same phrase plus the same password is
deterministic and must yield the same address.

Cheapest safe form, on a second device or a wiped test emulator: install the release APK, choose
**Restore**, type the phrase, and compare the address the app shows with the baseline address. If it
differs, **stop** — do not uninstall anything on the wallet's device.

If no second device is available, fold this into step 4 and treat a mismatch there as a FAIL that costs
you the wallet. That is why a second device is preferred.

### Step 3 — destructive reinstall

```bash
./rehearse-wallet-migration.sh reinstall                      # dry run, prints exactly what will happen
./rehearse-wallet-migration.sh reinstall --execute --confirm <serial> --phrase-held
```

The harness refuses to proceed unless: exactly one device is attached, `--confirm` repeats its serial,
`--phrase-held` is given, and the release APK's package, versionCode, versionName **and** certificate
all match the table at the top. Then it uninstalls `cc.ultranet.ultraverse`, installs the release APK,
and re-reads the device: version must be `2`/`0.2.0`, signer must be the production certificate, and no
wallet file may survive.

Writes `docs/runbooks/evidence/reinstall-<serial>-<utc>.md`.

### Step 4 — wallet restoration

On the device: open UltraVerse → wallet → **Restore** → type the recovery phrase → set the password →
unlock. Then:

```bash
./rehearse-wallet-migration.sh verify --expected-address <the address the app shows>
```

The address is read off the app screen and typed into the command — it is public, unlike the phrase.
The harness compares it with the stored baseline address and writes
`evidence/verify-<serial>-<utc>.md` with a PASS/FAIL verdict and exit code 1 on a mismatch.

Then compare balance and statement against the step 1 baseline by hand.

### Step 5 — functional verification, without broadcasting

On the release build, in this order:

| Check | How | Pass |
|---|---|---|
| Unlock | password opens the wallet | balance and statement render |
| Session / idle lock | leave the app idle until the lock fires, then reopen | wallet re-locks and unlocks again |
| Statement | portfolio / statement view | matches the step 1 baseline |
| Signing | AppChain review screen → sign, with the node URL pointed at an unreachable address | signature produced, submission fails as a transport error — not a rejected transaction, not a broadcast |
| Transport failure vs rejection | read the error the app shows | says the node could not be reached, not that the chain refused |
| Locked-wallet submission | lock the wallet, then try to submit | refused before signing |

Set the unreachable node URL *before* signing, and revert it afterwards. Under no circumstances point
this step at the production node.

### Step 6 — acceptance

```bash
./rehearse-wallet-migration.sh report
```

Assembles every phase file for the serial into `evidence/migration-rehearsal-<serial>-<utc>.md`.
Distribute the APK only when all of the following hold.

| # | Criterion | Verdict |
|---|---|---|
| 1 | Uninstall destroyed the wallet file (no residue in the data dir) | ☐ |
| 2 | Installed version is `2` / `0.2.0` | ☐ |
| 3 | Installed signer equals the production certificate `cca42165…` | ☐ |
| 4 | Restore from the phrase produced the **same** address as the baseline | ☐ |
| 5 | Balance and statement match the baseline | ☐ |
| 6 | Unlock, idle lock, statement, signing, transport-failure handling all behave as in step 5 | ☐ |
| 7 | No production transaction was signed or broadcast during the rehearsal | ☐ |
| 8 | The phrase was never written to this machine, a ticket or a screenshot | ☐ |

## Abort and rollback

Abort immediately if: the address after restore differs from the baseline; the installed signer is not
the production certificate; a wallet file survived the uninstall; or the phrase turns out to be
unavailable or wrong.

Rollback is **restore from the phrase** — there is no other path. Reinstalling the debug APK
(`app/build/outputs/apk/debug/app-debug.apk`) restores the *app*, not the wallet: it is a different
certificate, so it too needs an uninstall first, and the data directory is already gone.

If restore fails on the only device holding a funded wallet, that wallet is lost. This is the reason
step 2 exists and the reason the rehearsal is not run against the real wallet first.

## Running this on the emulator instead of a phone

The emulator is enough for the packaging half of the rehearsal and for validating the harness itself.
It needs one extra environment variable, because the AVD lives under the repo, not under `$HOME`:

```bash
export ANDROID_AVD_HOME="$PWD/.toolchains/android-user/avd"
"$ANDROID_HOME/emulator/emulator" -avd UltraVerse_API_36 -no-window -no-audio -no-boot-anim \
  -gpu swiftshader_indirect -memory 2048 -cores 2 &
```

Without it the emulator answers `Unknown AVD name [UltraVerse_API_36]`. What the emulator cannot do is
step 2 and step 4 — those need the real phrase, which stays on the operator's device.

## What is already proven, and what is not

| Claim | Status | Evidence |
|---|---|---|
| Same phrase → same address (derivation determinism) | **proven on device** | `WalletCryptoParityTest`, `WalletStoreTest`, `AppChainSignerTest`; 39/39 across nine classes, 2026-11-25 |
| The release APK carries the production signer and versionCode 2 | **proven** | `apksigner verify --print-certs`, `aapt2 dump badging` |
| Uninstall destroys the wallet; the release APK then installs and is production-signed **on the device** | **proven on the emulator** | `evidence/reinstall-emulator-5554-20261125T100418Z.md` — before: `eabdf35b…` (Android Debug), after: `cca42165…` (production), wallet gone |
| A debug install cannot be upgraded in place over the release install | **proven on the emulator** | `INSTALL_FAILED_UPDATE_INCOMPATIBLE`, quoted above |
| The harness refuses to run without the phrase, a confirmed serial, or a matching signer/version | **proven** | refusals below |
| A real wallet survives the uninstall via its phrase, with balances and history intact | **not proven** | needs a device that holds a wallet, and the operator's phrase |
| Existing users have a safe migration path | **not proven** | follows from the row above |

### Harness refusals observed on 2026-11-25

```
MNEMONIC=… ./rehearse-wallet-migration.sh baseline
  REFUSED: MNEMONIC is set in this shell. This script must never be able to read a recovery phrase …

./rehearse-wallet-migration.sh baseline           # with no device attached
  REFUSED: no device attached (start the test device, then 'adb devices')

./rehearse-wallet-migration.sh reinstall --execute --confirm emulator-5554
  REFUSED: --phrase-held is required: the phrase must be in the operator's hands, offline,
           before the uninstall

./rehearse-wallet-migration.sh verify --expected-address 1111…1111   # no baseline exists
  verdict: UNPROVEN — no baseline address to compare against
```

The `verify` self-test's synthetic record was deleted afterwards; it was a check of the harness, not
evidence about a wallet.

Until the last two rows of the table are proven on a real device, the release APK stays undistributed.
