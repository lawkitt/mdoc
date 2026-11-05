#!/usr/bin/env bash
# Usage: scripts/android-check.sh
# The Android job (ADR 0089), ci/android on the PC's queue runner (ADR 0095), when
# the commit touches the android area of scripts/ci-areas.txt (on a merge
# ref, against its main): the Android core, the app's JVM parity and unit
# tests, the app and its instrumented tests built, then those tests on CI's
# own emulator (emulator-5560), which is stopped afterwards if this run
# booted it. MDOC_ANDROID_JOB=always or never
# overrides the area check (the queue runner sets always: it has already
# decided).
# Windows x64.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android job runs on Windows x64"

android_changed() {
  case "${MDOC_ANDROID_JOB:-}" in
    always) return 0 ;;
    never) return 1 ;;
  esac
  git rev-parse --verify --quiet HEAD^1 >/dev/null || return 0
  affects android "$(diff_areas HEAD^1 HEAD)"
}

if ! android_changed; then
  echo "android-check: no core or android change (scripts/ci-areas.txt); the Android job is skipped"
  exit 0
fi

# The queue runner's job (and local CI) tests on the x86_64 emulator only:
# no arm64-v8a core, which only phones load (#614: three Rust builds were
# most of a 15-25 minute job). `just android-core` and device runs keep it.
if [ "${MDOC_ANDROID_JOB:-}" = always ]; then
  export MDOC_ANDROID_ABIS=${MDOC_ANDROID_ABIS:-x86_64}
  echo "android-check: the Android core for ${MDOC_ANDROID_ABIS} only (CI)"
fi

home=$(android_home)
sdk=$(scripts/install-android-sdk.sh)
scripts/build-android-core.sh >/dev/null
export JAVA_HOME ANDROID_HOME
JAVA_HOME=$(native_path "$home/jdk-21")
ANDROID_HOME=$(native_path "$sdk")

echo "android-check: JVM tests and builds"
(cd apps/android && ./gradlew --no-daemon --console=plain testDebugUnitTest assembleDebug assembleDebugAndroidTest)

serial=emulator-5560
adb=("$sdk/platform-tools/adb" -s "$serial")
was=$("${adb[@]}" get-state 2>/dev/null || true)
stop() {
  if [ "$was" != device ]; then
    echo "android-check: stopping the CI emulator"
    "${adb[@]}" emu kill >/dev/null 2>&1 || true
  fi
}
trap stop EXIT
scripts/android-emulator.sh ci >/dev/null

# Quarantined tests (.config/quarantine.txt, ADR 0095) are left out.
skip=$(bash scripts/quarantine.sh android) || die "scripts/quarantine.sh: .config/quarantine.txt does not parse"
gradle=(--no-daemon --console=plain :app:connectedDebugAndroidTest)
[ -z "$skip" ] || gradle+=("-Pandroid.testInstrumentationRunnerArguments.notClass=$skip")
echo "android-check: instrumented tests on $serial${skip:+ (quarantined: $skip)}"
(cd apps/android && ANDROID_SERIAL=$serial ./gradlew "${gradle[@]}")
