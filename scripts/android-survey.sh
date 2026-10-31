#!/usr/bin/env bash
# Usage: scripts/android-survey.sh <output folder> [phone|tablet]
# The Android UI survey (ADR 0087, 0089): SurveyTest walks every screen on
# the emulator, light, dark and at the largest font, and the screenshots are
# copied into the folder (tablet ones prefixed "tablet-"). Boots the emulator
# if needed (scripts/android-emulator.sh) and needs the core built
# (just android-core). Not part of local CI. Windows x64.
source "$(dirname "$0")/lib.sh"

out=${1:?usage: scripts/android-survey.sh <output folder> [phone|tablet]}
kind=${2:-phone}
[ "$(platform)" = windows-x64 ] || die "the Android survey runs on Windows x64"
home=$(android_home)
export JAVA_HOME ANDROID_HOME MSYS2_ARG_CONV_EXCL=/data
JAVA_HOME=$(native_path "$home/jdk-21")
ANDROID_HOME=$(native_path "$home/Sdk")
serial=$(scripts/android-emulator.sh "$kind")
adb=("$home/Sdk/platform-tools/adb" -s "$serial")
# SurveyTest copies its screenshots here: the test run uninstalls mdoc.
files=/data/local/tmp/mdoc-survey

"${adb[@]}" shell rm -rf "$files" >/dev/null 2>&1 || true
(cd apps/android && ANDROID_SERIAL=$serial ./gradlew --console=plain :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=io.github.lawkitt.mdoc.SurveyTest \
  -Pandroid.testInstrumentationRunnerArguments.mdoc.survey=1)
mkdir -p "$out"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"${adb[@]}" pull "$files" "$tmp" >/dev/null
prefix=''
[ "$kind" = tablet ] && prefix=tablet-
count=0
for f in "$tmp"/mdoc-survey/*.png; do
  cp "$f" "$out/$prefix$(basename "$f")"
  count=$((count + 1))
done
echo "$count screenshots in $out"
