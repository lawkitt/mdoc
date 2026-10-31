#!/usr/bin/env bash
# Usage: scripts/android-spike-device.sh --adb <serial>
#        scripts/android-spike-device.sh --test-lab <model>:<api> [<model>:<api> ...]
# Runs the Android feasibility spike (ADR 0089) and copies each report to
# target/android-spike/reports/. Builds the core (scripts/build-android-core.sh),
# stages the data once (scripts/android-spike-data.sh) and builds the harness
# (tools/android-spike) first.
#   --adb       a connected phone or a booted emulator, by its adb serial
#   --test-lab  Firebase Test Lab phones, e.g. shiba:36 (Pixel 8); needs gcloud
#               signed in with the Test Lab project set (see the Android grill's
#               Maintainer actions). `gcloud firebase test android models list
#               --filter=form=PHYSICAL` lists the models.
# Windows x64.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android spike runs from Windows x64"
mode=${1:-}
shift || true
case "$mode" in
  --adb) [ $# = 1 ] || die "usage: $0 --adb <serial>" ;;
  --test-lab) [ $# -ge 1 ] || die "usage: $0 --test-lab <model>:<api> ..." ;;
  *) die "usage: $0 --adb <serial> | --test-lab <model>:<api> ..." ;;
esac

home=$(android_home)
sdk=$home/Sdk
export JAVA_HOME ANDROID_HOME
JAVA_HOME=$(native_path "$home/jdk-21")
ANDROID_HOME=$(native_path "$sdk")
package=io.github.lawkitt.mdoc.spike
files=/sdcard/Android/data/$package/files
reports=target/android-spike/reports
mkdir -p "$reports"
stamp=$(date +%Y%m%d-%H%M%S)

scripts/build-android-core.sh >/dev/null
[ -d target/android-spike/assets/data/pseudonymization ] || scripts/android-spike-data.sh >/dev/null
(cd tools/android-spike && ./gradlew --quiet assembleDebug assembleDebugAndroidTest)
apk=tools/android-spike/app/build/outputs/apk/debug/app-debug.apk
test_apk=tools/android-spike/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk

if [ "$mode" = --adb ]; then
  adb=("$sdk/platform-tools/adb" -s "$1")
  "${adb[@]}" install -r -t "$apk" >/dev/null
  "${adb[@]}" install -r -t "$test_apk" >/dev/null
  "${adb[@]}" shell rm -f "$files/spike-report.json"
  echo "android-spike: running on $1 (a few minutes)" >&2
  "${adb[@]}" shell am instrument -w -e class "$package.SpikeTest" \
    "$package.test/androidx.test.runner.AndroidJUnitRunner" | tee "$reports/$1-$stamp.log"
  "${adb[@]}" pull "$files/spike-report.json" "$reports/$1-$stamp.json" >/dev/null
  echo "$reports/$1-$stamp.json"
  exit
fi

gcloud=${GCLOUD:-$(command -v gcloud || echo "$(cygpath -u "$LOCALAPPDATA")/Google/Cloud SDK/google-cloud-sdk/bin/gcloud")}
devices=()
for device in "$@"; do
  devices+=(--device "model=${device%%:*},version=${device##*:},locale=en,orientation=portrait")
done
log=$reports/test-lab-$stamp.log
echo "android-spike: running on Test Lab: $* (several minutes)" >&2
# A failed test still leaves its report to fetch; the log says which.
"$gcloud" firebase test android run --type instrumentation \
  --app "$apk" --test "$test_apk" "${devices[@]}" \
  --timeout 30m --no-record-video --no-performance-metrics \
  --directories-to-pull "$files" --results-dir "mdoc-spike-$stamp" 2>&1 | tee "$log" || true
bucket=$(grep -o 'test-lab-[a-z0-9-]*' "$log" | head -n 1)
[ -n "$bucket" ] || die "no results bucket in $log"
"$gcloud" storage ls "gs://$bucket/mdoc-spike-$stamp/**/spike-report.json" | while read -r url; do
  # gs://<bucket>/<dir>/<model>-<api>-<locale>-<orientation>/artifacts/...
  name=$(cut -d/ -f5 <<<"$url")
  "$gcloud" storage cp "$url" "$reports/$name-$stamp.json" --quiet
  echo "$reports/$name-$stamp.json"
done
