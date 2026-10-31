#!/usr/bin/env bash
# Usage: scripts/android-check.sh [--paths]
# The Android job (ADR 0089), run by the Windows PC's local CI after lint and
# test when the commit changes crates/, Cargo.lock, apps/android/ or the
# Android scripts (on a merge ref, against its main): the Android core, the
# app's JVM parity and unit tests, the app and its instrumented tests built,
# then those tests on CI's own emulator (emulator-5560), which is stopped
# afterwards if this run booted it. MDOC_ANDROID_JOB=always or never
# overrides the path check; --paths prints the paths it watches. Windows x64.
source "$(dirname "$0")/lib.sh"

# What the Android job checks: the core, the app and every script it runs.
ANDROID_JOB_PATHS='^(crates/|Cargo\.lock$|apps/android/|scripts/(lib\.sh|build-android-core\.sh|install-android-(sdk|ndk)\.sh|android-[a-z-]+\.(sh|py))$)'
[ "${1:-}" = --paths ] && {
  echo "$ANDROID_JOB_PATHS"
  exit
}

[ "$(platform)" = windows-x64 ] || die "the Android job runs on Windows x64"

android_changed() {
  case "${MDOC_ANDROID_JOB:-}" in
    always) return 0 ;;
    never) return 1 ;;
  esac
  git rev-parse --verify --quiet HEAD^1 >/dev/null || return 0
  # Not piped into grep -q: an early exit would fail the pipeline.
  local changed
  changed=$(git diff --name-only HEAD^1 HEAD)
  grep -qE "$ANDROID_JOB_PATHS" <<<"$changed"
}

if ! android_changed; then
  echo "android-check: no core or Android change; the Android job is skipped"
  exit 0
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

echo "android-check: instrumented tests on $serial"
(cd apps/android && ANDROID_SERIAL=$serial ./gradlew --no-daemon --console=plain connectedDebugAndroidTest)
