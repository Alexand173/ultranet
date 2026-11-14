#!/usr/bin/env bash
# Run the instrumentation suite in a way that cannot take the workstation down.
#
# The old way — one `connectedAndroidTest` call — held a 4 GB Gradle daemon and a Gradle-managed
# emulator at the same time, ran 18 classes in one process, and died with the whole machine when
# memory ran out, losing every result with it. This script:
#
#   * builds the APKs once with --no-daemon and a 3 GB heap, so no build JVM outlives the build;
#   * installs them with adb directly, so Gradle is not involved while the emulator runs;
#   * boots one headless emulator with a fixed, modest footprint and software rendering;
#   * runs ONE class per `am instrument` call, clearing app data first, so classes cannot leak
#     wallet state into each other and a failure is one line of the report, not a lost run;
#   * checks the emulator is alive between classes, restarts it and retries that class once if it
#     died, then moves on — the suite always reaches the end;
#   * writes each class's raw output to build/instrumented-tests/<Class>.log and one summary table,
#     so a crash cannot erase what already passed;
#   * refuses to start without KVM access or with too little free memory.
#
# Usage: ./run-instrumented-tests.sh [Class ...]      (no arguments runs every class)
#   AVD_NAME, EMULATOR_MEMORY_MB, EMULATOR_CORES, MIN_FREE_MB, CLASS_TIMEOUT, BOOT_TIMEOUT override.

set -uo pipefail

cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)" || exit 1
# shellcheck disable=SC1091
. ./env.sh

PACKAGE=cc.ultranet.ultraverse
RUNNER="$PACKAGE.test/androidx.test.runner.AndroidJUnitRunner"
APK=app/build/outputs/apk/debug/app-debug.apk
TEST_APK=app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
OUT=build/instrumented-tests
SUMMARY="$OUT/summary.txt"
SERIAL="emulator-5554"

BOOT_TIMEOUT=${BOOT_TIMEOUT:-300}
CLASS_TIMEOUT=${CLASS_TIMEOUT:-900}
MEMORY_MB=${EMULATOR_MEMORY_MB:-2048}
CORES=${EMULATOR_CORES:-2}
MIN_FREE_MB=${MIN_FREE_MB:-3500}

mkdir -p "$OUT"
: > "$SUMMARY"

log() { printf '[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
die() { printf '[%s] cannot start: %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; exit 2; }
free_mb() { awk '/MemAvailable/ {print int($2 / 1024)}' /proc/meminfo; }

AVD_NAME=${AVD_NAME:-$(timeout 30 emulator -list-avds 2>/dev/null | head -1)}
# The emulator launcher replaces itself with the qemu process, so the PID stays valid; matching on
# the process command line does not, because the AVD name is not in it.
EMULATOR_PID=""

emulator_running() {
    [ -n "$EMULATOR_PID" ] && kill -0 "$EMULATOR_PID" 2>/dev/null
}

stop_emulator() {
    timeout 30 adb -s "$SERIAL" emu kill >/dev/null 2>&1
    for _ in $(seq 1 25); do
        emulator_running || { EMULATOR_PID=""; return 0; }
        sleep 1
    done
    if emulator_running; then
        kill -TERM "$EMULATOR_PID" 2>/dev/null
        sleep 3
        emulator_running && kill -KILL "$EMULATOR_PID" 2>/dev/null
    fi
    wait "$EMULATOR_PID" 2>/dev/null
    EMULATOR_PID=""
}

start_emulator() {
    stop_emulator
    log "booting ${AVD_NAME} headless (${MEMORY_MB} MB, ${CORES} cores, software GPU)"
    nohup emulator -avd "$AVD_NAME" -no-window -no-audio -no-boot-anim -no-snapshot \
        -gpu swiftshader_indirect -memory "$MEMORY_MB" -cores "$CORES" \
        -netdelay none -netspeed full > "$OUT/emulator.log" 2>&1 &
    EMULATOR_PID=$!
    local waited=0
    while ((waited < BOOT_TIMEOUT)); do
        if ! emulator_running; then
            log "the emulator process exited during boot"
            tail -n 25 "$OUT/emulator.log" >&2
            return 1
        fi
        if [ "$(timeout 20 adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then
            # Animations are the main source of Compose test flakiness and CPU burn on a software GPU.
            for setting in window_animation_scale transition_animation_scale animator_duration_scale; do
                timeout 20 adb -s "$SERIAL" shell settings put global "$setting" 0 >/dev/null 2>&1
            done
            log "booted after ${waited}s"
            return 0
        fi
        sleep 5
        waited=$((waited + 5))
    done
    log "the emulator did not finish booting within ${BOOT_TIMEOUT}s"
    return 1
}

emulator_alive() {
    if [ -n "$EMULATOR_PID" ] && ! kill -0 "$EMULATOR_PID" 2>/dev/null; then
        return 1
    fi
    [ "$(timeout 20 adb -s "$SERIAL" get-state 2>/dev/null | tr -d '\r')" = "device" ] || return 1
    [ "$(timeout 20 adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]
}

install_apks() {
    [ -f "$APK" ] || die "missing $APK"
    [ -f "$TEST_APK" ] || die "missing $TEST_APK"
    timeout 300 adb -s "$SERIAL" install -r -t "$APK" > "$OUT/install.log" 2>&1 || {
        tail -n 5 "$OUT/install.log" >&2
        return 1
    }
    timeout 300 adb -s "$SERIAL" install -r -t "$TEST_APK" >> "$OUT/install.log" 2>&1 || {
        tail -n 5 "$OUT/install.log" >&2
        return 1
    }
}

# Echoes "PASS n", "FAIL n", "CRASH" or "INCOMPLETE" from a raw `am instrument` log.
verdict() {
    local log_file="$1"
    if ! grep -q '^INSTRUMENTATION_CODE: -1' "$log_file"; then
        # The run never finished: either the emulator died or the process was killed.
        if emulator_alive; then echo "INCOMPLETE"; else echo "CRASH"; fi
        return
    fi
    local passed failed
    passed=$(grep -c '^INSTRUMENTATION_STATUS_CODE: 0$' "$log_file")
    failed=$(grep -c '^INSTRUMENTATION_STATUS_CODE: -2$' "$log_file")
    if ((failed > 0)); then echo "FAIL $failed"; else echo "PASS $passed"; fi
}

run_class() {
    local class="$1" log_file="$OUT/$1.log" started=$SECONDS
    # App data is cleared so a class sees a first-install device, whatever ran before it.
    timeout 30 adb -s "$SERIAL" shell pm clear "$PACKAGE" >/dev/null 2>&1
    timeout "$CLASS_TIMEOUT" adb -s "$SERIAL" shell am instrument -w -r \
        -e class "$PACKAGE.$class" "$RUNNER" > "$log_file" 2>&1
    local elapsed=$((SECONDS - started))
    local result
    result=$(verdict "$log_file")
    if [ "$result" = "CRASH" ] || [ "$result" = "INCOMPLETE" ]; then
        log "$class $result after ${elapsed}s — restarting the emulator and retrying once"
        if start_emulator && install_apks; then
            started=$SECONDS
            timeout "$CLASS_TIMEOUT" adb -s "$SERIAL" shell pm clear "$PACKAGE" >/dev/null 2>&1
            timeout "$CLASS_TIMEOUT" adb -s "$SERIAL" shell am instrument -w -r \
                -e class "$PACKAGE.$class" "$RUNNER" > "$log_file.retry" 2>&1
            elapsed=$((SECONDS - started))
            result=$(verdict "$log_file.retry")
            result="$result (retried)"
        fi
    fi
    printf '%-38s %-10s %5ss\n' "$class" "$result" "$elapsed" | tee -a "$SUMMARY"
    case "$result" in PASS*) return 0 ;; *) return 1 ;; esac
}

# ---------------------------------------------------------------- preflight
[ -n "$AVD_NAME" ] || die "no AVD found; create one with avdmanager"
[ -r /dev/kvm ] && [ -w /dev/kvm ] || die "no KVM access (/dev/kvm). Without it the emulator runs
  in software, the app starves SystemUI, and the run ends in ANRs. Check 'id -nG | grep kvm'."
available=$(free_mb)
((available >= MIN_FREE_MB)) || die "only ${available} MB of memory is available, ${MIN_FREE_MB} MB is
  the floor. Stop the node build or another emulator first."

timeout 60 adb start-server >/dev/null 2>&1
log "free memory: ${available} MB; AVD: ${AVD_NAME}"

# ---------------------------------------------------------------- build once, without a daemon
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    log "building app and test APKs (no daemon, one worker, 3 GB heap)"
    if ! ./gradlew --no-daemon --max-workers=1 \
        "-Dorg.gradle.jvmargs=-Xmx3g -XX:MaxMetaspaceSize=768m" \
        :app:assembleDebug :app:assembleDebugAndroidTest > "$OUT/build.log" 2>&1; then
        log "the build failed; last lines:"
        tail -n 30 "$OUT/build.log" >&2
        exit 3
    fi
fi

# ---------------------------------------------------------------- classes
if (($# > 0)); then
    CLASSES=("$@")
else
    mapfile -t CLASSES < <(find app/src/androidTest -name '*Test.kt' -printf '%f\n' |
        sed 's/\.kt$//' | sort)
fi
log "running ${#CLASSES[@]} class(es), one at a time"

trap 'stop_emulator' EXIT INT TERM

start_emulator || die "the emulator could not be started; see $OUT/emulator.log"
install_apks || die "the APKs could not be installed; see $OUT/install.log"

failures=0
for class in "${CLASSES[@]}"; do
    if ! emulator_alive; then
        log "the emulator is gone before $class — restarting"
        start_emulator && install_apks || { log "giving up: the emulator cannot be restarted"; break; }
    fi
    run_class "$class" || failures=$((failures + 1))
    # A software-GPU emulator that has been up for a while leaks memory; keep the host level.
    if (($(free_mb) < MIN_FREE_MB)); then
        log "free memory fell to $(free_mb) MB — recycling the emulator"
        start_emulator && install_apks || true
    fi
done

printf '\n' >> "$SUMMARY"
if ((failures == 0)); then
    echo "all ${#CLASSES[@]} classes passed" | tee -a "$SUMMARY"
    exit 0
fi
echo "$failures of ${#CLASSES[@]} classes did not pass (see $OUT/<Class>.log)" | tee -a "$SUMMARY"
exit 1
