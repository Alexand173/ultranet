# UltraVerse Android

A native Kotlin + Jetpack Compose client for the UltraVerse service catalog and the UltraNet local wallet.

## Current scope

- The home screen keeps all 16 services in their four logical groups. Green dots indicate the six supported paths; the red X marks services that are not implemented yet. Unimplemented services stay in their own group rather than being moved into a separate concepts section, and their tiles remain inactive.
- **UltraNet Pay** is the fifth supported path, in Payments & addresses. It is a payment-first screen over the same wallet infrastructure as Portfolio → Send, not a second transfer implementation: it drives the same `WalletController` sequence (`updateRecipient` → `estimateTransfer` → `prepareTransfer` → `submitTransfer`), so the unresolved-transfer guard, the pending-spend subtraction, the history-completeness requirement and the local-only Dilithium signing all still apply. The recipient field takes a pasted address, an in-app QR scan, or typing; the node's fee quote is shown as Amount / Network fee / Total before the confirmation sheet; a receipt shows the hash (selectable, copyable) with **Done** returning to a cleared form. It withholds the form entirely in three cases, each a real blocker rather than a placeholder: no wallet stored, wallet locked (signing needs the unlocked session, and Pay never asks for the password itself), and an unresolved signed transfer (paying again could spend twice). The QR reader is CameraX plus the ML Kit barcode model bundled in the APK, so frames never leave the device and no Play Services module is downloaded; `CAMERA` is declared optional and requested only when Scan is tapped, which also means the AVD cannot exercise it.
- **Contacts** (Payments & addresses, replacing the earlier *Name Service* concept) is a local address book so a payment never needs 64 hex characters typed. It is deliberately not an on-chain name service: no node route, no transaction type, no fee, and nothing leaves the phone. A contact is a name plus the canonical address (`0x` stripped, lowercase), stored as JSON in the private `ultraverse.contacts` SharedPreferences, written with `commit()`. Android backup is disabled, so contacts are lost on reinstall, like the wallet record. Names are unique ignoring case, each address can be saved once, and the book holds at most 500. Pay gains a **Contacts** button and name suggestions in the recipient field: picking a contact writes the hex address into the same `recipientInput` the controller signs, so the transfer path is unchanged. A chip under the field ("To: Mom · 3b8e…") and the confirmation sheet name the recipient next to the full address. Wallet → Send has the same picker, and Portfolio rows show contact names. A contact is created only by an explicit save (Contacts tile, *Save as contact* on a Pay receipt, *Save contact* on an expanded statement row), never inferred from incoming transfers, because look-alike "address poisoning" transfers exist to be copied. Changing a saved contact's address asks for confirmation. `ContactStoreTest`, `ContactsScreenTest` and `UltraNetPayScreenTest.typingAContactNameFillsTheExactSavedAddress` cover the rules, the screen and the Pay hand-off.
- **Profile** (bottom bar) is the user's own side: the receive code, the address as a printed serial, and the three places that belong to this wallet. The QR encodes the bare 64-hex address — the same string `ContactRules.normalizeAddress` and the hosted faucet page accept — so any wallet in the ecosystem, including this app's own scanner, reads it without a URI scheme. It is rendered by `QRCodeWriter` from `com.google.zxing:core` (pure Java, added for encoding only; decoding stays with ML Kit) at error-correction M, drawn with `FilterQuality.None` so module edges stay square, and always ink-on-paper rather than theme colours, because a scanner needs that contrast in both appearances. The address is set in sixteen groups of four, four to a row, with the first group cyan and the last gold so both ends of the comparison are marked. Copy and Share are the only actions; nothing on the page is editable. It works with the wallet locked — the address is public and being paid does not need a session — and offers **Create wallet** when there is none. The role line (Holder / Validator) comes from `hasValidatorRecord` and is simply absent when the node cannot answer, rather than guessed. `ProfileScreenTest` decodes the rendered bitmap back to the exact address and pins the locked and no-wallet states; `ProfileNavigationTest` walks the tab.
- **Ultra Chess** (Applications, replacing the *Games* concept) is one screen holding three rulesets: **Classic**, **Random** (shuffled back ranks, bishops on opposite colours, no castling rights) and **Anti-Chess** (no check or checkmate, a capture must be played whenever one exists, and the side that loses every piece wins; two bare kings are a draw rather than an endless game). The player is White and moves first. `ChessRules` is a pure function over an immutable position — pseudo-legal generation, then legality filtering, with castling, en passant and four-way promotion — and it is pinned by perft against the published counts (20 / 400 / 8 902 at depths 1–3). `ChessAi` is negamax with alpha-beta, captures first, iterative deepening under a 700 ms and 120 000-node budget; Anti-Chess inverts its evaluation, so the same search tries to give material away. The opponent's contract is legality, not strength: the tests assert it never returns a move outside `ChessRules.legalMoves`, and it takes a hanging queen when one is there. The screen shows the selected square ringed and each legal destination as a disc (a ring when occupied), the last move as a gold wash, a checked king in red, long-algebraic last-move text (`e2–e4`, `O-O`, `e7–e8=Q`), Undo (the move and the reply), New game, and a confirmation before a mode switch abandons a live position. Board colours are their own theme roles (`boardLight`/`boardDark`) because a chess plate needs two tones in both appearances, where `surface` and `backgroundDeep` are the same white in light mode. No chain code is involved: `docs/CHESS_APPCHAIN_PLAN.md` records the receipt design as a later phase.
- Every supported tile is a native destination; no tile opens the website. Wallet, AppChains, validator applications and the operator tools each have in-app screens. Swap remains unavailable.
- AppChains are L1 objects, not a node-local registry. The screen reads `GET /api/appchain/ledger` and `GET /api/appchain/ledger/{id}/anchors` (both public) and writes exactly one thing: a version-5 transaction signed by the wallet's Dilithium key and submitted to `POST /api/appchain/tx`. Registration costs 0.01 `$ULTRA` and each anchor 0.001, both debited from the signing address; the signer of a registration becomes the owner, and only that address may anchor the chain. There is no admin token, no operator step and no derived treasury. `AppChainSignerTest` pins the version-5 digest against the node's own fixture, and the terms strip shows dashes rather than the client's constants when the ledger cannot be read — an unknown price is stated as unknown.
- Validator: an application is signed locally as a version-2 proposal envelope (SHA3-256 over sender, zeroed recipient/amount/fee, nonce, timestamp, nullifier, gas limits, domain string, 2592-byte public key and metadata) and posted to `/api/governance/propose`. It carries no value. `ValidatorProposalDigestTest` pins the digest against an independent reference vector.
- Validator approval status comes from `GET /api/governance/validator-review`, which the node only answers for an allowlisted operator session. The app states that limitation literally instead of pretending the list is empty; a normal wallet can still submit an application. A build that does not mount the route answers 404, which is reported as "this node build does not serve the review route" rather than as a rejected request.
- The mining gate accepts either the applicant address recorded in a proposal's `proposer`, or the validator's own address derived as `hex(SHA3-256(activated_validator.public_key))` — the rule the node applies to a key. Searching the serialised approval journal for the address as text can never find a 2592-byte key array, which is how an approved validator key came to be refused while only its applicant was accepted. `ValidatorRecordMatchTest` pins both paths against a journal captured from production.
- Operator tools: block production calls `POST /api/mine` with the admin token. The token is imported once and stored as ciphertext in SharedPreferences under an AES-256-GCM key that never leaves the Android Keystore; replacing or removing it is a separate action. A saved token proves local storage only, not that the node accepts it. `POST /api/mine` returns the produced block with byte-array hashes (converted to hex) and answers 400 when the mempool has nothing valid, which the screen reports as "No block was produced" rather than an authorization failure.
- The only deliberate external hand-off left is the node release page in Validator → Node setup, behind a confirmation dialog; the app itself never navigates away.
- **Wallet session:** once unlocked, the wallet stays unlocked for as long as the app process is in the foreground. Switching service screens, opening Portfolio, or moving between tools never locks it; the session ends on the explicit **Lock wallet** action or when the activity stops (the app is backgrounded). Earlier builds called `lock()` from the bottom-bar Apps handler, which ended the session on every navigation.
- **Home masthead has three states.** No wallet: create/restore/open. Wallet stored but locked: a single **Open wallet** action — offering create/restore here would be wrong. Unlocked: the real `$ULTRA` account balance, the amount available after pending sends, the observed-balance graphic and the **Portfolio** destination; the setup buttons are gone.
- **Portfolio reads as a statement:** account balance → pending sends including their fees → available after pending sends, then *Money in* / *Money out* for confirmed transfers, a separate pending section, day-grouped rows that expand in place to fee, nonce, counterparty and hash, and an *Observed balance history* section. Coverage is labelled (`Complete history` vs `Latest 100 records`) rather than implied, and the reconciliation never claims to explain the whole on-chain balance.
- Balance observations are written only when a refresh actually read a balance from the node, capped at 60 samples in `ultraverse.balance-history`. The graphic connects those points directly and a single sample draws a dot: gaps mean the app did not read the node, not that the balance was flat.
- While unlocked, every tool top bar (AppChains, Validator, Validate transactions) shows the balance and spendable amount. It is provided once from the app shell through `LocalWalletBalance`, and tapping it opens Portfolio with Back returning to that tool.
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

`assembleDebug` produces an APK that ART always interprets: the platform caps the compiler filter for debuggable apps, so `cmd package compile -m speed -f` reports `Success` and changes nothing. On a software-emulated host that made first composition cost ~30 s of main-thread CPU plus ~30 s of JIT — enough for `am start -W` to time out and for SystemUI to ANR. `./gradlew assembleRelease` builds a second variant signed with the debug key: it keeps the same `applicationId`, so it installs over the debug build and preserves the stored wallet, but because it is not debuggable the platform AOT-compiles it. Use that one to walk UI flows by hand; keep the debug build for instrumentation and debugging.

Run local JVM and device tests with:

```sh
./gradlew testDebugUnitTest connectedDebugAndroidTest
```

The connected tests include the website BIP39 seed vector, shared WASM/native key/address/signature vectors, the Rust v1 digest fixture, AES-GCM tamper/wrong-password checks, API envelope and signed-byte-array contract checks, Compose catalog/native-route regressions, `ValidatorRecordMatchTest` (the mining gate against a production journal payload), and `WalletSessionStatementTest`. That last one seeds the encrypted store, unlocks through the real UI (real 310 000-iteration PBKDF2 and real Dilithium key derivation), then walks the three masthead states, the balance carried into AppChains' top bar, and the portfolio statement — and asserts that returning to Portfolio never asks for the password again. Note that the Apps tab deliberately scrolls past the masthead to the first service group, so a test that wants the hero has to scroll back to index 1.

`UltraNetOperatorApi` performs its blocking socket work inside `withContext(Dispatchers.IO)`. The tool screens call it straight from `LaunchedEffect` and `rememberCoroutineScope`, which both run on the main dispatcher, so without that hop every tool screen raised `NetworkOnMainThreadException` and died mid-composition instead of rendering.

`AppearanceModeTest` covers the header appearance menu: all three modes are offered, choosing Dark or Light repaints and updates the button's accessible name, `AppearanceStore` reloads exactly what it saved, and a cleared store resolves to the system-following default. Prefer this test over injected taps when validating the switch, because a headless software-rendered emulator can silently drop injected touches.

`WindowPaletteConsistencyTest` holds `res/values/colors.xml` to `UltraVersePalette`, and `GroupedHomeScreenTest.appsTabReturnsToTheFirstGroupAfterEveryHeaderItem` proves the Apps tab still scrolls past every header slot to the first service group. Run each class in its own Gradle invocation: a comma-separated `class=` argument silently executes only the first entry.

`WalletCryptoParityTest` runs four 310 000-iteration PBKDF2 derivations: well under a second per case on a KVM-accelerated emulator or a phone, but minutes per case when the emulator has no acceleration. Use `-Pandroid.testInstrumentationRunnerArguments.class=<class>` to run one class at a time when iterating.

The portfolio shows a balance and nonce only when account data *and* transaction history both load. Without history, pending outgoing spend cannot be subtracted, so the screen reports unavailable data instead of an overstated balance.

A local API-36 Google APIs x86_64 AVD may be started with:

```sh
emulator -avd UltraVerse_API_36 -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -no-snapshot
```

The host is WSL2 with hardware acceleration available. `/dev/kvm` exists and `emulator -accel-check` reports "KVM (version 12) is installed and usable". With acceleration the AVD finishes booting in about ten seconds, `am start -W` reports `Status: ok` with `TotalTime` near 1.5 s, and `adb shell input tap` reaches the app, so `uiautomator dump` plus injected taps can drive a whole flow.

Screen capture is a separate problem that acceleration does not solve. `adb exec-out screencap` returns a real frame for the first one or two captures after a boot and then a black frame (~21 KB instead of ~460 KB), with either `-gpu swiftshader_indirect` or `-gpu off`, and it does not recover without restarting the emulator. Treat the semantics tree as the source of truth for behaviour and take review screenshots on a phone.

Acceleration needs the `kvm` group, which had no members until 2026-10-24. A session that started before `usermod -aG kvm` keeps the old supplementary groups, so run `emulator -accel-check` before blaming the app: restarting the session picks the group up, and `sudo chmod 666 /dev/kvm` unblocks the current one without restarting anything.

Do not fall back to `-accel off` for UI work. Under software emulation first composition costs ~30 s of main-thread CPU plus ~30 s of JIT, which starves SystemUI and `com.google.android.as` into ANRs and makes `am start -W` report `Status: timeout`. Those were symptoms of CPU starvation, not a rendering defect — they disappear with acceleration. The debug APK is at `app/build/outputs/apk/debug/app-debug.apk` after a successful build.

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

On this workstation, use the runner, not `connectedAndroidTest`:

```sh
cd android-app && ./run-instrumented-tests.sh                    # every class
cd android-app && ./run-instrumented-tests.sh ValidatorRecordMatchTest ChessRulesTest
cd android-app && SKIP_BUILD=1 ./run-instrumented-tests.sh       # APKs already current
```

One `connectedAndroidTest` invocation holds a 4 GB Gradle daemon and a Gradle-managed emulator at the same time and runs every class in one process; on a 23 GB host that ran out of memory and took the machine down with the results. The runner builds once with `--no-daemon` and a 3 GB heap, installs with adb, boots one headless emulator, then runs **one class per `am instrument` call** with `pm clear` before each so classes cannot leak wallet state into each other. It restarts the emulator and retries a class once if the emulator died, recycles the emulator when free memory drops below 3.5 GB, and always kills it on exit.

Results go to `build/instrumented-tests/summary.txt` and `<Class>.log`, so a crash cannot erase what already passed. Full suite, 2026-11-14: 18 classes, 64 tests, about two and a half minutes, 20 s boot.

On a physical device, Gradle is fine:

```sh
cd android-app && source ./env.sh
ANDROID_SERIAL=<serial> ./gradlew connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=cc.ultranet.ultraverse.WindowPaletteConsistencyTest
```

A comma-separated `class=` argument silently runs only the first entry, so one class per invocation there too. Each run overwrites `app/build/outputs/androidTest-results/connected/debug/*.xml`, so read that XML rather than trusting the exit code. With more than one device attached, export `ANDROID_SERIAL` — otherwise Gradle fans out across all of them and a stale emulator fails the run with `No connected devices!`

`WalletCryptoParityTest` used to be the one to plan around: its four 310 000-iteration PBKDF2 derivations cost about 80 s per case under software emulation. With KVM acceleration it finishes in about 3 s, so what matters now is having `/dev/kvm` access at all (`id -nG | grep kvm`); the runner refuses to start without it.

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
