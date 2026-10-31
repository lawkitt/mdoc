#!/usr/bin/env bash
# Usage: scripts/android-device-test.sh [<serial> | <model>:<api> ...]
# The Android device tests (ADR 0089): the app's instrumented tests and the
# long-job test (apps/android/longjobs: the model download, text recognition
# and Find names off screen and after a process kill, then no crash or ANR
# in mdoc's exit info).
#   <serial>       a USB phone or an emulator, by its adb serial: both suites.
#   <model>:<api>  Firebase Test Lab phones, e.g. shiba:36: both suites on
#                  each. With none, the app's tests on one phone per RAM
#                  class (Pixel 6a bluejay:32, Pixel 8 shiba:36, Pixel 10 Pro
#                  blazer:36) and the long jobs on a Pixel and a Samsung
#                  (shiba:36, Galaxy A55 a55xzh:36): five phones, the Spark
#                  plan's daily quota.
# Builds the core (scripts/build-android-core.sh), the debug app and both
# test APKs (they carry arm64), copies each phone's JUnit results to
# target/android-device-test/<time>/ and fails when a phone fails or leaves
# no results. CellularTest skips on a phone with no SIM, as Test Lab's are.
# Test Lab needs gcloud signed in with the project set (the Android grill's
# Maintainer actions); `gcloud firebase test android models list
# --filter=form=PHYSICAL` lists the phones. A USB phone with a release mdoc
# keeps it: the debug build has another signature and does not install over
# it, so uninstall mdoc first on a test phone. Windows x64.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android device tests run from Windows x64"
home=$(android_home)
export JAVA_HOME ANDROID_HOME
JAVA_HOME=$(native_path "$home/jdk-21")
ANDROID_HOME=$(native_path "$home/Sdk")
stamp=$(date +%Y%m%d-%H%M%S)
out=target/android-device-test/$stamp
mkdir -p "$out"

# A Test Lab phone is model:api (API two digits); anything else is a serial.
test_lab() { [[ $1 =~ ^[A-Za-z0-9_]+:[0-9]{2}$ ]]; }

scripts/build-android-core.sh >/dev/null

if [ $# = 1 ] && ! test_lab "$1"; then
  serial=$1
  "$home/Sdk/platform-tools/adb" -s "$serial" get-state >/dev/null || die "no device $serial on adb"
  echo "android-device-test: both suites on $serial (about 30 minutes, longer while models download)" >&2
  failed=0
  (cd apps/android && ANDROID_SERIAL=$serial ./gradlew --console=plain :app:connectedDebugAndroidTest \
    :longjobs:connectedDebugAndroidTest --continue) | tee "$out/gradle.log" || failed=1
  for suite in app longjobs; do
    results=apps/android/$suite/build/outputs/androidTest-results/connected
    [ -d "$results" ] && cp -r "$results" "$out/$suite"
  done
  echo "android-device-test: results in $out"
  [ "$failed" = 0 ] || die "a test failed on $serial; see $out/gradle.log"
  exit
fi

for device in "$@"; do
  test_lab "$device" || die "$device is not a Test Lab phone (model:api) and only one serial may be given"
done
if [ $# -gt 0 ]; then
  app_phones=("$@")
  long_phones=("$@")
else
  app_phones=(bluejay:32 shiba:36 blazer:36)
  long_phones=(shiba:36 a55xzh:36)
fi

(cd apps/android && ./gradlew --console=plain --quiet assembleDebug assembleDebugAndroidTest :longjobs:assembleDebug)
apk=apps/android/app/build/outputs/apk/debug/app-debug.apk
gcloud=${GCLOUD:-$(command -v gcloud || echo "$(cygpath -u "$LOCALAPPDATA")/Google/Cloud SDK/google-cloud-sdk/bin/gcloud")}
failed=0
missing=0

# One matrix: the suite's test APK on each phone, its JUnit XML copied to
# $out/<suite>/<model>-<api>.xml.
matrix() {
  local suite=$1 test_apk=$2 timeout=$3
  shift 3
  local devices=() device log=$out/$suite.log dir=mdoc-device-test-$stamp-$suite bucket name
  for device in "$@"; do
    devices+=(--device "model=${device%%:*},version=${device##*:},locale=en,orientation=portrait")
  done
  echo "android-device-test: $suite on Test Lab: $*" >&2
  "$gcloud" firebase test android run --type instrumentation \
    --app "$apk" --test "$test_apk" "${devices[@]}" \
    --timeout "$timeout" --no-record-video --no-performance-metrics \
    --results-dir "$dir" 2>&1 | tee "$log" || failed=1
  mkdir -p "$out/$suite"
  bucket=$(grep -o 'test-lab-[a-z0-9-]*' "$log" | head -n 1)
  if [ -n "$bucket" ]; then
    # gs://<bucket>/<dir>/<model>-<api>-<locale>-<orientation>/test_result_1.xml
    "$gcloud" storage ls "gs://$bucket/$dir/*/test_result_*.xml" 2>/dev/null | while read -r url; do
      "$gcloud" storage cp "$url" "$out/$suite/$(cut -d/ -f5 <<<"$url" | sed 's/-en-portrait$//').xml" --quiet
    done
  fi
  for device in "$@"; do
    name=${device%%:*}-${device##*:}
    if [ ! -f "$out/$suite/$name.xml" ]; then
      echo "android-device-test: $suite left no results on $name" >&2
      missing=$((missing + 1))
    fi
  done
}

matrix app apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk 30m "${app_phones[@]}"
matrix longjobs apps/android/longjobs/build/outputs/apk/debug/longjobs-debug.apk 60m "${long_phones[@]}"
echo "android-device-test: results in $out"
[ "$failed" = 0 ] || die "a phone failed or was inconclusive; see the logs in $out"
[ "$missing" = 0 ] || die "$missing phone runs left no results; see the logs in $out"
