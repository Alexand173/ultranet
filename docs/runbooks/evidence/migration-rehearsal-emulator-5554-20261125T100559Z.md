# Wallet migration rehearsal — consolidated record

Assembled 2026-11-25T10:05:59Z (UTC) from the phase files for `emulator-5554`.

Release APK under test: `app/build/outputs/apk/release/app-release.apk` (versionCode 2, 0.2.0, signer `cca42165fb74fac25fd3fc838d5a40cdfe0471bdb7cff6ef161835136b287ac0`).

| Phase | File | Present |
|---|---|---|
| baseline | `baseline-emulator-5554-20261125T100416Z.md` | yes |
| reinstall | `reinstall-emulator-5554-20261125T100418Z.md` | yes |
| verify | — | **no** |

## Phase records

<!-- baseline-emulator-5554-20261125T100416Z.md -->

# Wallet migration rehearsal — baseline

Captured 2026-11-25T10:04:17Z (UTC) by rehearse-wallet-migration.sh baseline.

| Field | Value |
|---|---|
| Device serial | `emulator-5554` |
| Model | sdk_gphone64_x86_64 |
| Android | 16 (sdk 36) |
| Installed version | versionCode=2 minSdk=23 targetSdk=36 versionName=0.2.0 |
| Install times | lastUpdateTime=2026-11-25 11:04:15 firstInstallTime=2026-11-25 11:04:15 |
| Installed signer | `sha256=eabdf35bf6728f960840f7386def1e8aa5651c9db90b54f6d3dc5cea99fd18ba` |
| Wallet file | no wallet file |
| Wallet address | `absent` |

Recovery phrase: **not recorded here and must never be.** It is typed on the device, by the operator, into the app Restore screen.

<!-- reinstall-emulator-5554-20261125T100418Z.md -->

# Wallet migration rehearsal — destructive reinstall

Executed 2026-11-25T10:04:22Z (UTC) on `emulator-5554`.

| Field | Before | After |
|---|---|---|
| Version | versionCode=2 minSdk=23 targetSdk=36 versionName=0.2.0 | versionCode=2 minSdk=23 targetSdk=36 versionName=0.2.0 |
| Signer | `sha256=eabdf35bf6728f960840f7386def1e8aa5651c9db90b54f6d3dc5cea99fd18ba` | `sha256=cca42165fb74fac25fd3fc838d5a40cdfe0471bdb7cff6ef161835136b287ac0` |
| Wallet address | `absent` | unreadable (release install, not debuggable — expected) |
| Data dir has wallet file | — | no |

| Step | Result |
|---|---|
| uninstall | Success |
| install -r release APK | Success |
| installed signer == production cert | yes |
| wallet destroyed by uninstall | yes |

Next: open the app, choose Restore, type the recovery phrase **on the device**, then run `verify`.

