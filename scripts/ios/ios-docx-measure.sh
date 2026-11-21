#!/usr/bin/env bash
# Usage: scripts/ios/ios-docx-measure.sh <hardware UDID | Simulator name> OUTPUT
# Time through the DOCX Original's first PDFKit draw and peak memory:
# DocxOriginalMeasureTests renders legal-opinion.docx, coverage.docx and a
# synthetic 100-page contract (tests/fixtures/docx-preview/long.py) in
# process, MDOC_DOCX_MEASURE_RUNS times each (3 by default), in one test
# process per document, so each peak is its own; then prints one table.
# Measurement only. A phone needs what scripts/ios/ios-device-test.sh needs
# (unlocked, trusted, Developer Mode, the maintainer's team signing); the
# documents go to and the tables come from the app's data container.
# Simulator numbers are not phone numbers. Needs the core built
# (`just ios-core`).
source "$(dirname "$0")/../lib.sh"
[ "$(platform)" = macos-arm64 ] || die "the iOS tests build only on Apple Silicon macOS"
usage="usage: scripts/ios/ios-docx-measure.sh <hardware UDID | Simulator name> OUTPUT"
target=${1:?$usage}
output=${2:?$usage}
runs=${MDOC_DOCX_MEASURE_RUNS:-3}
bundle=io.github.lawkitt.mdoc
team=F33DLM8K53
documents=(coverage.docx legal-opinion.docx long-100.docx)
# A fresh OUTPUT, so only this run's tables are read.
[ ! -e "$output" ] || [ -z "$(ls -A "$output")" ] || die "$output is not empty"
mkdir -p "$output/corpus"
output=$(cd "$output" && pwd)
tag=$(date +%Y%m%d-%H%M%S)
cp tests/fixtures/docx-preview/coverage.docx tests/fixtures/docx-preview/legal-opinion.docx "$output/corpus/"
python3 -I tests/fixtures/docx-preview/long.py "$output/corpus/long-100.docx"

extra=(TEST_RUNNER_MDOC_DOCX_MEASURE_RUNS="$runs" TEST_RUNNER_MDOC_DOCX_MEASURE_TAG="$tag")
if [[ $target =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{16}$ ]]; then
  device=yes
  destination="id=$target"
  derived=target/ios/DeviceTests
  xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
    DEVELOPMENT_TEAM="$team" -quiet build-for-testing
  # The container exists once mdoc is installed; the test run reinstalls it
  # over this copy and keeps its data.
  xcrun devicectl device install app --device "$target" "$derived/Build/Products/Debug-iphoneos/mdoc.app"
  xcrun devicectl device copy to --device "$target" --domain-type appDataContainer \
    --domain-identifier "$bundle" --source "$output/corpus" --destination Library/mdoc-docx-measure/corpus
else
  device=no
  os=$(sed -n 's/^IOS_SIM_RUNTIME=//p' scripts/tools.env)
  destination="platform=iOS Simulator,OS=$os,name=$target"
  derived=target/ios/DerivedData
  xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -quiet build-for-testing
  extra+=(TEST_RUNNER_MDOC_DOCX_MEASURE_CORPUS="$output/corpus" TEST_RUNNER_MDOC_DOCX_MEASURE_OUT="$output/tables")
fi

for document in "${documents[@]}"; do
  echo "rendering $document, $runs runs"
  env TEST_RUNNER_MDOC_DOCX_MEASURE="$document" "${extra[@]}" \
    xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -only-testing:mdocTests/DocxOriginalMeasureTests/testRender \
    -quiet test-without-building
done

if [ "$device" = yes ]; then
  xcrun devicectl device copy from --device "$target" --domain-type appDataContainer \
    --domain-identifier "$bundle" --source "Library/mdoc-docx-measure/out/$tag" --destination "$output/tables"
fi
first=yes
for document in "${documents[@]}"; do
  table=$output/tables/$document.tsv
  [ -f "$table" ] || die "no table for $document under $output/tables"
  if [ "$first" = yes ]; then cat "$table"; else tail -n +2 "$table"; fi
  first=no
done | tee "$output/runs.tsv"
