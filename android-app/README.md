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
- The UI follows the system theme. `UltraVersePalette` carries one role set with a dark and a light instance, exposed through `UltraVerseColors`; screens read roles and never branch on the mode themselves. Light mode maps the cyan accent to an accessible teal (`#0F7F8A`), because `#52E9EC` fails contrast on white.

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

`WalletCryptoParityTest` runs four 310 000-iteration PBKDF2 derivations, so it takes minutes on a software-rendered emulator and seconds on hardware. Use `-Pandroid.testInstrumentationRunnerArguments.class=<class>` to run one class at a time when iterating.

The portfolio shows a balance and nonce only when account data *and* transaction history both load. Without history, pending outgoing spend cannot be subtracted, so the screen reports unavailable data instead of an overstated balance.

A local API-36 Google APIs x86_64 AVD may be started with:

```sh
emulator -avd UltraVerse_API_36 -no-window -no-audio -no-boot-anim -accel off -gpu swiftshader_indirect -no-snapshot
```

The development host is WSL2 without KVM, so software-rendered emulator performance is limited and system-wide ANR warnings may occur. Prefer a hardware-accelerated emulator or physical device for UI iteration. The debug APK is at `app/build/outputs/apk/debug/app-debug.apk` after a successful build.

## Crypto source attribution

`app/src/main/cpp/CMakeLists.txt` compiles `website/crypto/ultra-dilithium5/pqclean` directly. PQClean licensing is documented in `app/src/main/cpp/LICENSE-PQCLEAN` and the source project README. The BIP-0039 English wordlist and MIT notice are in `app/src/main/assets/`.
