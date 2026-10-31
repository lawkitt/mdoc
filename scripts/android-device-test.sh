#!/usr/bin/env bash
# Usage: scripts/android-device-test.sh [<model>:<api> ...]
# The app's instrumented tests on Firebase Test Lab phones (ADR 0089), by
# default one per RAM class: Pixel 6a (6 GB, bluejay:32), Pixel 8 (8 GB,
# shiba:36) and Pixel 10 Pro (16 GB, blazer:36). Builds the core
# (scripts/build-android-core.sh), the debug app and its tests (they carry
# arm64), runs them with gcloud, copies each phone's JUnit results to
# target/android-device-test/<time>/ and fails when a phone fails or leaves
# no results. CellularTest skips on a phone with no SIM, as Test Lab's are.
# Needs gcloud signed in with the Test Lab project set (the Android grill's
# Maintainer actions); `gcloud firebase test android models list
# --filter=form=PHYSICAL` lists the phones. Windows x64.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android device tests run from Windows x64"
[ $# -gt 0 ] || set -- bluejay:32 shiba:36 blazer:36
home=$(android_home)
export JAVA_HOME ANDROID_HOME
JAVA_HOME=$(native_path "$home/jdk-21")
ANDROID_HOME=$(native_path "$home/Sdk")

scripts/build-android-core.sh >/dev/null
(cd apps/android && ./gradlew --console=plain --quiet assembleDebug assembleDebugAndroidTest)
apk=apps/android/app/build/outputs/apk/debug/app-debug.apk
test_apk=apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk

stamp=$(date +%Y%m%d-%H%M%S)
out=target/android-device-test/$stamp
mkdir -p "$out"
log=$out/test-lab.log
gcloud=${GCLOUD:-$(command -v gcloud || echo "$(cygpath -u "$LOCALAPPDATA")/Google/Cloud SDK/google-cloud-sdk/bin/gcloud")}
devices=()
for device in "$@"; do
  devices+=(--device "model=${device%%:*},version=${device##*:},locale=en,orientation=portrait")
done
echo "android-device-test: running on Test Lab: $* (about 15 minutes)" >&2
failed=0
"$gcloud" firebase test android run --type instrumentation \
  --app "$apk" --test "$test_apk" "${devices[@]}" \
  --timeout 30m --no-record-video --no-performance-metrics \
  --results-dir "mdoc-device-test-$stamp" 2>&1 | tee "$log" || failed=1

bucket=$(grep -o 'test-lab-[a-z0-9-]*' "$log" | head -n 1)
[ -n "$bucket" ] || die "no results bucket in $log"
# gs://<bucket>/<dir>/<model>-<api>-<locale>-<orientation>/test_result_1.xml
"$gcloud" storage ls "gs://$bucket/mdoc-device-test-$stamp/*/test_result_*.xml" 2>/dev/null | while read -r url; do
  "$gcloud" storage cp "$url" "$out/$(cut -d/ -f5 <<<"$url").xml" --quiet
done
missing=0
for device in "$@"; do
  name=${device%%:*}-${device##*:}-en-portrait
  if [ ! -f "$out/$name.xml" ]; then
    echo "android-device-test: $name left no results" >&2
    missing=$((missing + 1))
  fi
done
echo "android-device-test: results in $out"
[ "$failed" = 0 ] || die "a phone failed or was inconclusive; see $log"
[ "$missing" = 0 ] || die "$missing of $# phones left no results; see $log"
