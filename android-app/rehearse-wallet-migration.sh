#!/usr/bin/env bash
#
# rehearse-wallet-migration.sh — controlled uninstall → reinstall → wallet-restore rehearsal.
#
# One question, before the release APK reaches anybody: after the debug-signed install is destroyed
# and the production-signed build is installed in its place, can an existing user get their exact
# wallet back from their recovery phrase?
#
# This script never touches a recovery phrase. The phrase is typed by a human, on the device, in the
# app's Restore screen. The script refuses to run if anything phrase-like is in the environment, and
# it prints only the wallet *address* — never the encrypted seed, salt or IV.
#
# Usage
#   ./rehearse-wallet-migration.sh baseline   [--device SERIAL]
#   ./rehearse-wallet-migration.sh reinstall  [--device SERIAL] [--execute --confirm SERIAL --phrase-held]
#   ./rehearse-wallet-migration.sh verify     --expected-address <64 hex> [--device SERIAL]
#   ./rehearse-wallet-migration.sh report
#
# Phases
#   baseline    read-only: device, installed version, installed signer, stored wallet address
#   reinstall   DESTRUCTIVE, dry-run unless --execute: uninstall, install the release APK, re-verify
#   verify      read-only: compare the address the app shows after restore with the baseline
#   report      assemble every artefact for this serial into one evidence file
#
# Exit status is 0 only when the phase's checks pass.

set -Eeuo pipefail
cd "$(dirname "$0")"

PACKAGE=cc.ultranet.ultraverse
RELEASE_APK=${RELEASE_APK:-app/build/outputs/apk/release/app-release.apk}
EXPECTED_VERSION_CODE=2
EXPECTED_VERSION_NAME=0.2.0
EXPECTED_CERT_SHA256=cca42165fb74fac25fd3fc838d5a40cdfe0471bdb7cff6ef161835136b287ac0
WALLET_REL=no_backup/ultranet-wallet-v1.json
EVIDENCE_DIR=../docs/runbooks/evidence

die() { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }
step() { printf '\n── %s\n' "$*"; }

COMMAND=${1:-}
[ -n "$COMMAND" ] || die "no phase given (baseline | reinstall | verify | report)"
shift || true

DEVICE=""
EXECUTE=0
CONFIRM=""
PHRASE_HELD=0
EXPECTED_ADDRESS=""
while [ $# -gt 0 ]; do
    case "$1" in
        --device) DEVICE=${2:?--device needs a serial}; shift 2 ;;
        --execute) EXECUTE=1; shift ;;
        --confirm) CONFIRM=${2:?--confirm needs the serial you are destroying}; shift 2 ;;
        --phrase-held) PHRASE_HELD=1; shift ;;
        --expected-address) EXPECTED_ADDRESS=${2:?--expected-address needs 64 hex}; shift 2 ;;
        *) die "unknown argument: $1" ;;
    esac
done

# ── toolchain ────────────────────────────────────────────────────────────────────────────────────
JAVA_HOME=${JAVA_HOME:-$PWD/.toolchains/jdk}
ANDROID_HOME=${ANDROID_HOME:-$PWD/.toolchains/android-sdk}
export JAVA_HOME ANDROID_HOME
ADB=$ANDROID_HOME/platform-tools/adb
[ -x "$ADB" ] || die "adb not found at $ADB (set ANDROID_HOME)"
BUILD_TOOLS=$(ls -d "$ANDROID_HOME"/build-tools/* 2>/dev/null | sort -V | tail -1)
[ -n "$BUILD_TOOLS" ] || die "no build-tools under $ANDROID_HOME"
APKSIGNER=$BUILD_TOOLS/apksigner
AAPT2=$BUILD_TOOLS/aapt2

# ── guards ───────────────────────────────────────────────────────────────────────────────────────
refuse_phrase_in_environment() {
    local name
    for name in MNEMONIC RECOVERY_PHRASE PHRASE SEED SEED_PHRASE WALLET_PHRASE; do
        if [ -n "${!name:-}" ]; then
            die "$name is set in this shell. This script must never be able to read a recovery phrase — unset it and type the phrase on the device instead."
        fi
    done
}

# Exactly one device, and it must be the one the operator named. A rehearsal that silently picks a
# device is how the wrong wallet gets destroyed.
resolve_device() {
    local attached
    attached=$("$ADB" devices | awk 'NR>1 && $2=="device" {print $1}')
    [ -n "$attached" ] || die "no device attached (start the test device, then 'adb devices')"
    if [ "$(printf '%s\n' "$attached" | wc -l)" -ne 1 ]; then
        die "more than one device attached:
$(printf '%s\n' "$attached" | sed 's/^/  /')
Disconnect everything except the rehearsal device; this phase must not guess."
    fi
    if [ -n "$DEVICE" ] && [ "$DEVICE" != "$attached" ]; then
        die "--device $DEVICE does not match the attached device $attached"
    fi
    SERIAL=$attached
    ADB="$ADB -s $SERIAL"
}

shell_() { $ADB shell "$@" 2>/dev/null; }

# apksigner writes its signer report to stderr and exits 1 when it merely has a warning to give —
# an unprotected META-INF entry in a Gradle-built APK is one. Read the report, ignore the exit code,
# and treat "no certificate line" as the only failure.
apk_cert() { # apk path → comma-separated lowercase SHA-256 digests, or empty
    "$APKSIGNER" verify --print-certs "$1" 2>&1 \
        | sed -n 's/.*certificate SHA-256 digest: //p' \
        | tr 'A-F' 'a-f' | tr -d ':' | sort -u | paste -sd, - || true
}

apk_facts() { # path → "versionCode versionName package certsha"
    local apk=$1
    local pkg ver name cert
    pkg=$("$AAPT2" dump badging "$apk" 2>/dev/null | sed -n "s/^package: name='\([^']*\)'.*/\1/p" || true)
    ver=$("$AAPT2" dump badging "$apk" 2>/dev/null | sed -n "s/.*versionCode='\([0-9]*\)'.*/\1/p" | head -1 || true)
    name=$("$AAPT2" dump badging "$apk" 2>/dev/null | sed -n "s/.*versionName='\([^']*\)'.*/\1/p" | head -1 || true)
    cert=$(apk_cert "$apk")
    printf '%s %s %s %s' "$ver" "$name" "$pkg" "$cert"
}

guard_release_apk() {
    [ -f "$RELEASE_APK" ] || die "release APK not found at $RELEASE_APK"
    local ver name pkg cert
    read -r ver name pkg cert <<<"$(apk_facts "$RELEASE_APK")"
    [ "$pkg" = "$PACKAGE" ] || die "release APK package is '$pkg', expected $PACKAGE"
    [ "$ver" = "$EXPECTED_VERSION_CODE" ] || die "release APK versionCode is '$ver', expected $EXPECTED_VERSION_CODE"
    [ "$name" = "$EXPECTED_VERSION_NAME" ] || die "release APK versionName is '$name', expected $EXPECTED_VERSION_NAME"
    [ "$cert" = "$EXPECTED_CERT_SHA256" ] || die "release APK signer is '${cert:-(none)}', expected the production certificate $EXPECTED_CERT_SHA256"
    RELEASE_APK_SHA256=$(sha256sum "$RELEASE_APK" | cut -d' ' -f1)
    say "release APK      : $RELEASE_APK"
    say "  sha256         : $RELEASE_APK_SHA256"
    say "  package/version: $pkg $ver ($name)"
    say "  signer (prod)  : $cert"
}

installed_version() {
    shell_ dumpsys package "$PACKAGE" | grep -m2 -E "versionCode=|versionName=" | tr -s ' ' | sed 's/^ *//' | paste -sd' ' - || true
}

installed_times() {
    shell_ dumpsys package "$PACKAGE" | grep -m2 -E "firstInstallTime=|lastUpdateTime=" | tr -s ' ' | sed 's/^ *//' | paste -sd' ' - || true
}

# The installed signer, read from the APK that is actually on the device. A non-debuggable release
# install is still readable here — base.apk is world-readable — so this works on both variants, which
# is what makes "the device is running the production certificate" a device fact and not an assumption.
installed_signer() {
    local path tmp cert
    path=$(shell_ pm path "$PACKAGE" | sed -n 's/^package://p' | head -1 || true)
    [ -n "$path" ] || { printf 'not installed'; return 0; }
    tmp=$(mktemp -d)
    if $ADB pull "$path" "$tmp/base.apk" >/dev/null 2>&1; then
        cert=$(apk_cert "$tmp/base.apk")
        rm -rf "$tmp"
        if [ -n "$cert" ]; then printf 'sha256=%s' "$cert"; return 0; fi
    else
        rm -rf "$tmp"
    fi
    printf 'unreadable (%s)' "$path"
}

wallet_address() {
    # run-as only works on a debuggable install. Read just the address out of the wallet file; the
    # encrypted seed, salt and IV are dropped on the floor and never printed.
    local raw
    raw=$(shell_ run-as "$PACKAGE" cat "$WALLET_REL" 2>/dev/null || true)
    printf '%s' "$raw" | python3 -c '
import json, sys
raw = sys.stdin.read().strip()
if not raw:
    print("absent")
else:
    try:
        print(json.loads(raw).get("address", "present-but-unparsable"))
    except Exception:
        print("present-but-unparsable")
' 2>/dev/null || printf 'unreadable'
}

wallet_present() {
    if shell_ run-as "$PACKAGE" ls "$WALLET_REL" >/dev/null 2>&1; then
        printf 'file present'
    elif shell_ dumpsys package "$PACKAGE" | grep -q "flags=\[.*DEBUGGABLE.*\]"; then
        printf 'no wallet file'
    else
        printf 'unreadable (release install, not debuggable — expected)'
    fi
}

data_dir_present() {
    if shell_ ls "/data/data/$PACKAGE/$WALLET_REL" >/dev/null 2>&1; then printf 'yes'; else printf 'no'; fi
}

device_facts() {
    say "serial           : $SERIAL"
    say "model            : $(shell_ getprop ro.product.model | tr -d '\r')"
    say "android          : $(shell_ getprop ro.build.version.release | tr -d '\r') (sdk $(shell_ getprop ro.build.version.sdk | tr -d '\r'))"
    say "abi              : $(shell_ getprop ro.product.cpu.abi | tr -d '\r')"
}

latest_for() { ls -t "$EVIDENCE_DIR"/"$1"-"$2"-*.md 2>/dev/null | head -1 || true; }

# ── phases ──────────────────────────────────────────────────────────────────────────────────────
phase_baseline() {
    refuse_phrase_in_environment
    resolve_device
    mkdir -p "$EVIDENCE_DIR"
    local stamped out
    stamped=$(date -u +%Y%m%dT%H%M%SZ)
    out=$EVIDENCE_DIR/baseline-$SERIAL-$stamped.md

    step "pre-migration baseline (read-only)"
    local version times signer address present
    version=$(installed_version); times=$(installed_times)
    signer=$(installed_signer); address=$(wallet_address); present=$(wallet_present)

    device_facts
    say "installed        : ${version:-not installed}"
    say "  install times  : ${times:-n/a}"
    say "  signer         : $signer"
    say "  wallet         : $present"
    say "  wallet address : $address"

    {
        printf '# Wallet migration rehearsal — baseline\n\n'
        printf 'Captured %s (UTC) by rehearse-wallet-migration.sh baseline.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf '| Field | Value |\n|---|---|\n'
        printf '| Device serial | `%s` |\n' "$SERIAL"
        printf '| Model | %s |\n' "$(shell_ getprop ro.product.model | tr -d '\r')"
        printf '| Android | %s (sdk %s) |\n' "$(shell_ getprop ro.build.version.release | tr -d '\r')" "$(shell_ getprop ro.build.version.sdk | tr -d '\r')"
        printf '| Installed version | %s |\n' "${version:-not installed}"
        printf '| Install times | %s |\n' "${times:-n/a}"
        printf '| Installed signer | `%s` |\n' "$signer"
        printf '| Wallet file | %s |\n' "$present"
        printf '| Wallet address | `%s` |\n' "$address"
        printf '\nRecovery phrase: **not recorded here and must never be.** It is typed on the device, by the operator, into the app Restore screen.\n'
    } >"$out"

    say ""
    say "baseline written : $out"
    if [ "$address" = "absent" ]; then
        say "NOTE             : this install has no wallet, so it cannot serve as the baseline half of the rehearsal."
    fi
}

phase_reinstall() {
    refuse_phrase_in_environment
    resolve_device
    guard_release_apk
    mkdir -p "$EVIDENCE_DIR"

    step "destructive reinstall on $SERIAL"
    say "This will:"
    say "  1. uninstall $PACKAGE, destroying the stored wallet on this device"
    say "  2. install the production-signed $EXPECTED_VERSION_NAME (versionCode $EXPECTED_VERSION_CODE)"
    say "  3. confirm the package, version and signer on the device afterwards"
    say ""
    say "Before continuing, the operator must hold the recovery phrase for the wallet currently on"
    say "this device, offline, outside this machine. If it is not held, stop here."

    if [ "$EXECUTE" -ne 1 ]; then
        say ""
        say "DRY RUN — nothing was changed. Re-run with:"
        say "  ./rehearse-wallet-migration.sh reinstall --device $SERIAL --execute --confirm $SERIAL --phrase-held"
        return 0
    fi
    [ "$CONFIRM" = "$SERIAL" ] || die "--confirm must repeat the serial ($SERIAL); refusing to destroy a wallet on an unconfirmed device"
    [ "$PHRASE_HELD" -eq 1 ] || die "--phrase-held is required: the phrase must be in the operator's hands, offline, before the uninstall"

    local before_address before_signer before_version stamped out
    before_address=$(wallet_address)
    before_signer=$(installed_signer)
    before_version=$(installed_version)
    stamped=$(date -u +%Y%m%dT%H%M%SZ)
    out=$EVIDENCE_DIR/reinstall-$SERIAL-$stamped.md
    say ""
    say "version before   : ${before_version:-not installed}"
    say "signer before    : $before_signer"
    say "wallet address   : $before_address"

    local result
    if ! result=$($ADB uninstall "$PACKAGE" 2>&1); then
        die "uninstall failed: $(printf '%s' "$result" | tr -d '\r' | tail -1)"
    fi
    result=$(printf '%s' "$result" | tr -d '\r' | tail -1)
    say "uninstall        : $result"
    [ "$result" = "Success" ] || die "uninstall did not report Success: $result"
    [ "$(shell_ pm path "$PACKAGE" | wc -l)" -eq 0 ] || die "package is still present after uninstall"

    # Newer adb prints "Install command complete in NNN ms" rather than "Success"; the exit status is
    # the real verdict, so do not pattern-match it.
    if ! install_log=$($ADB install -r "$RELEASE_APK" 2>&1); then
        die "install failed: $(printf '%s' "$install_log" | tr -d '\r' | tail -1)"
    fi
    say "install          : $(printf '%s' "$install_log" | tr -d '\r' | tail -1)"

    local version signer present data_dir
    sleep 2
    version=$(installed_version); signer=$(installed_signer); present=$(wallet_present); data_dir=$(data_dir_present)
    say "installed        : $version"
    say "  signer         : $signer"
    say "  wallet         : $present"
    say "  data dir       : $data_dir"

    [ -n "$(printf '%s' "$version" | grep "versionCode=$EXPECTED_VERSION_CODE")" ] \
        || die "installed versionCode is not $EXPECTED_VERSION_CODE: $version"
    [ "$signer" = "sha256=$EXPECTED_CERT_SHA256" ] \
        || die "installed signer is $signer, expected sha256=$EXPECTED_CERT_SHA256"
    [ "$present" = "unreadable (release install, not debuggable — expected)" ] || [ "$present" = "no wallet file" ] \
        || die "a wallet file survived the uninstall: $present"

    {
        printf '# Wallet migration rehearsal — destructive reinstall\n\n'
        printf 'Executed %s (UTC) on `%s`.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SERIAL"
        printf '| Field | Before | After |\n|---|---|---|\n'
        printf '| Version | %s | %s |\n' "${before_version:-not installed}" "$version"
        printf '| Signer | `%s` | `%s` |\n' "$before_signer" "$signer"
        printf '| Wallet address | `%s` | %s |\n' "$before_address" "$present"
        printf '| Data dir has wallet file | — | %s |\n' "$data_dir"
        printf '\n| Step | Result |\n|---|---|\n'
        printf '| uninstall | Success |\n'
        printf '| install -r release APK | Success |\n'
        printf '| installed signer == production cert | yes |\n'
        printf '| wallet destroyed by uninstall | yes |\n'
        printf '\nNext: open the app, choose Restore, type the recovery phrase **on the device**, then run `verify`.\n'
    } >"$out"
    say ""
    say "reinstall record : $out"
    if printf '%s' "$before_address" | grep -qE '^[0-9a-f]{64}$'; then
        say "NEXT             : open the app → Restore → type the phrase on the device → set the password,"
        say "                   then: ./rehearse-wallet-migration.sh verify --expected-address $before_address"
    else
        say "NEXT             : this install held no wallet, so only the packaging half was rehearsed —"
        say "                   repeat on a device that holds one to rehearse the restore itself."
    fi
}

phase_verify() {
    [ -n "$EXPECTED_ADDRESS" ] || die "verify needs --expected-address <the address the app now shows>"
    printf '%s' "$EXPECTED_ADDRESS" | grep -qE '^[0-9a-f]{64}$' || die "--expected-address must be 64 lowercase hex"
    refuse_phrase_in_environment
    resolve_device
    mkdir -p "$EVIDENCE_DIR"

    step "post-restore verification"
    local version signer present baseline_file baseline_address verdict
    version=$(installed_version); signer=$(installed_signer); present=$(wallet_present)
    baseline_file=$(latest_for baseline "$SERIAL")
    baseline_address=""
    [ -n "$baseline_file" ] && baseline_address=$(sed -n 's/^| Wallet address | `\([^`]*\)` |$/\1/p' "$baseline_file")

    device_facts
    say "installed        : $version"
    say "  signer         : $signer"
    say "  wallet         : $present"
    say "  address shown  : $EXPECTED_ADDRESS"
    say "  baseline       : ${baseline_address:-not recorded}"

    if [ -z "$baseline_address" ] || [ "$baseline_address" = "absent" ]; then
        verdict="UNPROVEN — no baseline address to compare against"
    elif [ "$baseline_address" = "$EXPECTED_ADDRESS" ]; then
        verdict="PASS — the restored wallet is the same wallet"
    else
        verdict="FAIL — the restored address differs from the baseline; the phrase was wrong or the derivation changed"
    fi
    say "verdict          : $verdict"

    local out
    out=$EVIDENCE_DIR/verify-$SERIAL-$(date -u +%Y%m%dT%H%M%SZ).md
    {
        printf '# Wallet migration rehearsal — post-restore verification\n\n'
        printf 'Captured %s (UTC) on `%s`.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SERIAL"
        printf '| Field | Value |\n|---|---|\n'
        printf '| Installed version | %s |\n' "$version"
        printf '| Installed signer | `%s` |\n' "$signer"
        printf '| Wallet file | %s |\n' "$present"
        printf '| Baseline address | `%s` |\n' "$baseline_address"
        printf '| Address after restore | `%s` |\n' "$EXPECTED_ADDRESS"
        printf '\n**%s**\n' "$verdict"
    } >"$out"
    say "record written   : $out"
    case "$verdict" in FAIL*) return 1 ;; esac
    return 0
}

phase_report() {
    resolve_device 2>/dev/null || true
    local serial=${SERIAL:-${DEVICE:-}}
    [ -n "$serial" ] || die "report needs --device or exactly one attached device"
    local stamped out
    stamped=$(date -u +%Y%m%dT%H%M%SZ)
    out=$EVIDENCE_DIR/migration-rehearsal-$serial-$stamped.md
    mkdir -p "$EVIDENCE_DIR"
    {
        printf '# Wallet migration rehearsal — consolidated record\n\n'
        printf 'Assembled %s (UTC) from the phase files for `%s`.\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$serial"
        printf 'Release APK under test: `%s` (versionCode %s, %s, signer `%s`).\n\n' \
            "$RELEASE_APK" "$EXPECTED_VERSION_CODE" "$EXPECTED_VERSION_NAME" "$EXPECTED_CERT_SHA256"
        printf '| Phase | File | Present |\n|---|---|---|\n'
        local phase f
        for phase in baseline reinstall verify; do
            f=$(latest_for "$phase" "$serial")
            if [ -n "$f" ]; then
                printf '| %s | `%s` | yes |\n' "$phase" "$(basename "$f")"
            else
                printf '| %s | — | **no** |\n' "$phase"
            fi
        done
        printf '\n## Phase records\n\n'
        local file
        for phase in baseline reinstall verify; do
            for file in $(ls -t "$EVIDENCE_DIR"/"$phase"-"$serial"-*.md 2>/dev/null | head -1); do
                printf '<!-- %s -->\n\n' "$(basename "$file")"
                cat "$file"
                printf '\n'
            done
        done
    } >"$out"
    say "consolidated record written : $out"
}

case "$COMMAND" in
    baseline) phase_baseline ;;
    reinstall) phase_reinstall ;;
    verify) phase_verify ;;
    report) phase_report ;;
    *) die "unknown phase '$COMMAND' (baseline | reinstall | verify | report)" ;;
esac
