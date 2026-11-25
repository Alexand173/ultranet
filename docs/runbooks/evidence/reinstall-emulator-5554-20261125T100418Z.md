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
