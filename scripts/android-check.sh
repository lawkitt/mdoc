#!/usr/bin/env bash
# Usage: scripts/android-check.sh
# The Android job (ADR 0089, 0091), ci/android on the PC's queue runner (ADR
# 0095), when the commit touches the android area of scripts/ci-areas.txt (on
# a merge ref, against its main): the Android core, the app's JVM parity and
# unit tests, the app, its instrumented tests and the minified release-like
# app (R8) built; on CI's own emulator (emulator-5560), which is stopped
# afterwards if this run booted it, the minified app opens its library (a
# keep rule R8 needs and lacks fails here), then the instrumented tests run.
# MDOC_ANDROID_JOB=always or never overrides the area check (the queue runner
# sets always: it has already decided).
# Windows x64, Apple Silicon macOS (an arm64-v8a emulator) or Linux x86_64
# with KVM.
source "$(dirname "$0")/lib.sh"

case "$(platform)" in
  windows-x64 | macos-arm64 | Linux-x86_64) ;;
  *) die "the Android job runs on Windows x64, Apple Silicon macOS or Linux x86_64" ;;
esac

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

# The queue runner's job (and local CI) builds the core only for its
# emulator's ABI, x86_64 (arm64-v8a on the Mac): the other one only phones
# load (#614: three Rust builds were most of a 15-25 minute job).
# `just android-core` and device runs keep both.
if [ "${MDOC_ANDROID_JOB:-}" = always ]; then
  export MDOC_ANDROID_ABIS=${MDOC_ANDROID_ABIS:-$(android_emulator_abi)}
  echo "android-check: the Android core for ${MDOC_ANDROID_ABIS} only (CI)"
fi

sdk=$(scripts/install-android-sdk.sh)
scripts/build-android-core.sh >/dev/null
# ANDROID_SDK_ROOT too: AGP refuses two different SDKs, and hosted runners
# set it to the image's own.
export JAVA_HOME ANDROID_HOME ANDROID_SDK_ROOT
JAVA_HOME=$(native_path "$(android_jdk)")
ANDROID_HOME=$(native_path "$sdk")
ANDROID_SDK_ROOT=$ANDROID_HOME

echo "android-check: JVM tests and builds"
(cd apps/android && ./gradlew --no-daemon --console=plain testDebugUnitTest assembleDebug assembleDebugAndroidTest assembleMinified)

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

# The minified app (ADR 0091), opened as a person would: Home lists the
# library only once the core opened through JNA and read a snapshot back,
# which is what R8 breaks when it strips or renames what the bindings reach
# by name. A crash or Home's "could not open" fails the job.
minified_opens() {
  local pkg=io.github.lawkitt.mdoc.minified screen=target/android/minified-screen.xml started=false
  local apk=apps/android/app/build/outputs/apk/minified/app-minified.apk
  echo "android-check: the minified app opens its library on $serial"
  # Git Bash would turn /sdcard/... into a Windows path for native adb.
  export MSYS2_ARG_CONV_EXCL=/sdcard
  "${adb[@]}" uninstall "$pkg" >/dev/null 2>&1 || true
  # -g: the notification permission, whose dialog would cover Home.
  "${adb[@]}" install -g "$apk" >/dev/null
  "${adb[@]}" logcat -c >/dev/null 2>&1 || true
  "${adb[@]}" shell am start -W -n "$pkg/io.github.lawkitt.mdoc.MainActivity" >/dev/null
  local verdict=timeout
  for _ in $(seq 1 30); do
    rm -f "$screen"
    if "${adb[@]}" shell uiautomator dump /sdcard/mdoc-minified.xml >/dev/null 2>&1; then
      "${adb[@]}" exec-out cat /sdcard/mdoc-minified.xml >"$screen" 2>/dev/null || true
    fi
    if grep -q 'text="No documents yet"' "$screen" 2>/dev/null; then
      verdict=ok
      break
    fi
    if grep -q 'text="mdoc could not open its documents"' "$screen" 2>/dev/null; then
      verdict=refused
      break
    fi
    if [ -n "$("${adb[@]}" shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ]; then
      started=true
    elif $started; then
      verdict=crashed
      break
    fi
    sleep 2
  done
  if [ "$verdict" != ok ]; then
    echo "android-check: the minified app's screen:" >&2
    grep -oE 'text="[^"]+"' "$screen" 2>/dev/null | sed 's/^/  /' >&2 || true
    echo "android-check: its crash log:" >&2
    "${adb[@]}" logcat -d -s AndroidRuntime:E 2>/dev/null | tail -n 40 >&2 || true
  fi
  "${adb[@]}" uninstall "$pkg" >/dev/null 2>&1 || true
  case "$verdict" in
    ok) ;;
    refused) die "the minified app could not open its library: an R8 keep rule is missing (apps/android/app/proguard-rules.pro)" ;;
    crashed) die "the minified app crashed: an R8 keep rule may be missing (apps/android/app/proguard-rules.pro)" ;;
    *) die "the minified app showed neither Home nor its error in a minute" ;;
  esac
}
minified_opens

# Quarantined tests (.config/quarantine.txt, ADR 0095) are left out.
skip=$(bash scripts/quarantine.sh android) || die "scripts/quarantine.sh: .config/quarantine.txt does not parse"
gradle=(--no-daemon --console=plain :app:connectedDebugAndroidTest)
[ -z "$skip" ] || gradle+=("-Pandroid.testInstrumentationRunnerArguments.notClass=$skip")
echo "android-check: instrumented tests on $serial${skip:+ (quarantined: $skip)}"
(cd apps/android && ANDROID_SERIAL=$serial ./gradlew "${gradle[@]}")
