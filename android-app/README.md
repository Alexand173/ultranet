# UltraVerse Android

A native Kotlin + Jetpack Compose client for the UltraVerse service catalog and the UltraNet local wallet.

## Current scope

- The home screen keeps all 15 services in four groups. Green dots indicate the three supported paths; red X marks unavailable concepts. Concept tiles remain inactive.
- Portfolio, wallet create, wallet restore, and wallet service entry open native screens. AppChains and validator onboarding still open their verified website flows. Swap and Profile remain unavailable.
- Wallet phrases are validated/generated as 12-word English BIP39. The website-compatible key seed is BIP39 PBKDF2-HMAC-SHA512 (2048 rounds) followed by SHA3-256 over `UltraNet/Dilithium5/keygen/v1 || bip39Seed`.
- Dilithium-5 key generation and signing compile the same checked-in PQClean clean C sources as the browser WASM and Rust node. The native secret key exists only inside a JNI operation, is never stored, and is wiped after signing/key derivation.
- A wallet seed is encrypted locally with PBKDF2-HMAC-SHA256 (310,000 iterations), AES-256-GCM, a random 16-byte salt, a random 12-byte IV, and an Android-wallet version/address AAD. The record is atomically stored under `noBackupFilesDir`; Android backup is disabled. Android storage is intentionally distinct from the browser IndexedDB record format; restoring the same recovery phrase derives the same wallet identity.
- The app reads the real account, balance, next nonce, fee estimate, transaction history, and transaction status from `https://api.ultranetwork.cc`. The private seed/key/password/recovery phrase are never sent to the node. No account/profile is created.
- Transfers use integer base units (six decimals), a local 32-byte nullifier, the canonical version-1 SHA3-256 digest and local Dilithium-5 signing, then POST the established byte-array JSON envelope over HTTPS. Pending outgoing amount + fee is deducted from displayed available funds. Sending is disabled when history is unavailable or reaches the 100-record API limit, so pending spend cannot be silently ignored.
- Signing the transaction is local, but transfer proving/validation is still performed by the existing node endpoint. A pending transaction is not a confirmed transaction; confirmation depends on block production.
- The signed request is retained in memory for identical retry after an ambiguous response. It is not persisted across app process death; after restart, check address history before preparing another transaction.
- The UI follows the saved appearance choice (Follow system / Light / Dark), not only the OS mode. `UltraVersePalette` carries one role set with a dark and a light instance, exposed through `UltraVerseColors`; screens read roles and never branch on the mode themselves. Light mode maps the cyan accent to an accessible teal (`#0B6F79`), because `#52E9EC` fails contrast on white.
- Every light-mode role that is used as text clears WCAG AA (4.5:1) on the white, tinted-card, and gradient-end surfaces: `ink` `#0B2233`, `muted` `#3F5C6C`, `dim` `#557180`, `cyan` `#0B6F79`, `gold` `#8A6516`, `supported` `#0E7047`, `unavailable` `#C13342`. The earlier `dim` (`#7D97A6`, 2.9:1) and `gold` (`#9A7420`, 3.7:1) tones failed AA for small labels.
- The `×` availability glyph uses its own role, `unavailableStrong` (`#A32A36` light, `#FF8894` dark), because `unavailable` on the 18% badge tint measured 4.17:1 in light and 3.97:1 in dark. Both now clear 5.2:1 on the badge, in the legend, and in the bottom-navigation badge, which also needs an explicit `contentColor` — `Badge` cannot resolve `contentColorFor` for a colour outside the Material scheme.
- Window and launch colours live once, in `res/values/colors.xml`, and `styles.xml` references them. `UltraVerseResourceColors` exposes the same values out of `UltraVersePalette` for Compose drawing, and `WindowPaletteConsistencyTest` fails if the resources and the palette ever drift apart.
- The hero ring and the header emblem draw `ultranet_coin_disc.png`, a square, transparent-cornered crop of the official render. Regenerate it with `python3 tools/make_coin_disc.py` after replacing `ultranet_official_coin.png` (the launcher icon still uses the full render).

## Build

This repository keeps the Android toolchain project-local under ignored `android-app/.toolchains/`. The Gradle wrapper is pinned to Gradle 9.6.0, project JDK 17, Android SDK 37, NDK 28.2.13676358, and CMake 3.31.6. The environment script selects local tools without changing global shell configuration.

```sh
source ./env.sh
./gradlew assembleDebug
```

Run local JVM and device tests with:

```sh
./gradlew testDebugUnitTest connectedDebugAndroidTest
```

The connected tests include the website BIP39 seed vector, shared WASM/native key/address/signature vectors, the Rust v1 digest fixture, AES-GCM tamper/wrong-password checks, API envelope and signed-byte-array contract checks, and Compose catalog/native-route regressions.

`AppearanceModeTest` covers the header appearance menu: all three modes are offered, choosing Dark or Light repaints and updates the button's accessible name, `AppearanceStore` reloads exactly what it saved, and a cleared store resolves to the system-following default. Prefer this test over injected taps when validating the switch, because a headless software-rendered emulator can silently drop injected touches.

`WindowPaletteConsistencyTest` holds `res/values/colors.xml` to `UltraVersePalette`, and `GroupedHomeScreenTest.appsTabReturnsToTheFirstGroupAfterEveryHeaderItem` proves the Apps tab still scrolls past every header slot to the first service group. Run each class in its own Gradle invocation: a comma-separated `class=` argument silently executes only the first entry.

`WalletCryptoParityTest` runs four 310 000-iteration PBKDF2 derivations, so it takes minutes on a software-rendered emulator and seconds on hardware. Use `-Pandroid.testInstrumentationRunnerArguments.class=<class>` to run one class at a time when iterating.

The portfolio shows a balance and nonce only when account data *and* transaction history both load. Without history, pending outgoing spend cannot be subtracted, so the screen reports unavailable data instead of an overstated balance.

A local API-36 Google APIs x86_64 AVD may be started with:

```sh
emulator -avd UltraVerse_API_36 -no-window -no-audio -no-boot-anim -accel off -gpu swiftshader_indirect -no-snapshot
```

Substituting `-gpu off` moves rasterisation into the guest and boots the same way; both are software paths and both hit the capture issue described below.

The development host is WSL2 without KVM, so software-rendered emulator performance is limited and system-wide ANR warnings may occur. Prefer a hardware-accelerated emulator or physical device for UI iteration. The debug APK is at `app/build/outputs/apk/debug/app-debug.apk` after a successful build.

Headless software rendering is not stable for long sessions: `adb exec-out screencap` eventually returns an all-black frame and `adb shell input tap` stops reaching the app while uiautomator dumps still look correct. The emulator console prints `Failed to find ColorBuffer` / `Failed to bind to texture` when this happens. A freshly booted emulator captures reliably, so take the screenshots you need right after `am start`, or restart the emulator; when touch input is dead, verify interaction through the instrumentation tests instead.

## Testing on a physical device

A real phone is the better target for UI iteration and for any screenshot you intend to judge, because none of the software-rendering caveats above apply. Nothing in the build needs changing: `abiFilters` already covers `arm64-v8a` and `x86_64`, so the native Dilithium library ships for both.

| Requirement | Value |
|---|---|
| CPU ABI | `arm64-v8a` for a phone, `x86_64` for an x86 tablet |
| OS | API 23 minimum; API 34–36 recommended, because `targetSdk = 36` drives edge-to-edge behaviour |
| Developer options | enabled, with USB or wireless debugging |
| Network | HTTPS reach to `https://api.ultranetwork.cc`; cleartext is disabled in the manifest |

### Connecting from WSL2

WSL2 cannot see USB devices by default, so wireless debugging is the simpler route:

```sh
source android-app/env.sh          # project-local platform-tools on PATH
adb pair <phone-ip>:<pair-port>    # port and six-digit code from Wireless debugging
adb connect <phone-ip>:5555
adb devices                        # must read "device", not "unauthorized"
```

Accept the RSA debugging prompt on the phone the first time. For USB instead, bind and attach the device from an administrator PowerShell with `usbipd-win` (`usbipd list`, then `usbipd bind --busid <id>`, then `usbipd attach --wsl --busid <id>`); adb inside WSL usually still needs the platform-tools udev rules afterwards, which is the most common failure point.

### Running the suites

```sh
cd android-app && source ./env.sh
./gradlew connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=cc.ultranet.ultraverse.WindowPaletteConsistencyTest
```

One class per invocation: a comma-separated `class=` argument silently runs only the first entry. Each run overwrites `app/build/outputs/androidTest-results/connected/debug/*.xml`, so read that XML rather than trusting the exit code. With more than one device attached, export `ANDROID_SERIAL=<serial>` — otherwise Gradle fans out across all of them and a stale emulator fails the run with `No connected devices!`.

`WalletCryptoParityTest` is the one to plan around: its four 310 000-iteration PBKDF2 derivations cost about 80 s per case under software emulation and seconds on a phone.

### Confirm both ABIs shipped

The native library is compiled per ABI, so verify the APK carries both slices before trusting it on the device:

```sh
python3 -c "import zipfile;print([n for n in zipfile.ZipFile('app/build/outputs/apk/debug/app-debug.apk').namelist() if n.endswith('.so')])"
```

Expected output lists `lib/arm64-v8a/libultranetcrypto.so` and `lib/x86_64/libultranetcrypto.so` (plus `libandroidx.graphics.path.so` in each). The arm64 slice should be a 64-bit AArch64 ELF, `e_machine` `0xB7`; if the slice is missing, the app installs and then dies at the first Dilithium call.

### Device hygiene

- Developer options → **Stay awake**, and keep the device unlocked: `MainActivity` locks the wallet on `ON_STOP`, so a sleeping screen can leave a test on the wrong route.
- Disable window, transition and animator animations if runs flake.
- `adb uninstall cc.ultranet.ultraverse` before judging the launch frame: the appearance preference lives in app storage, so a previous *Dark* choice survives a reinstall.

## Crypto source attribution

`app/src/main/cpp/CMakeLists.txt` compiles `website/crypto/ultra-dilithium5/pqclean` directly. PQClean licensing is documented in `app/src/main/cpp/LICENSE-PQCLEAN` and the source project README. The BIP-0039 English wordlist and MIT notice are in `app/src/main/assets/`.
