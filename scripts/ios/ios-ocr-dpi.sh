#!/usr/bin/env bash
# Usage: scripts/ios/ios-ocr-dpi.sh <hardware UDID | Simulator name> OUTPUT [DPI…]
# OCR time and peak memory per render resolution (150 200 300 by default)
# on a phone or a Simulator: OCRResolutionTests recognizes the OCR corpus
# (tests/fixtures/ocr-upstream and ocr-qualification, MDOC_OCR_DPI_RUNS
# times each, 3 by default) in one test process per resolution, so each
# peak is its own; then tools/ocr-upstream/dpi.py scores the outputs and
# prints the table. Measurement only. A phone needs what
# scripts/ios/ios-device-test.sh needs (unlocked, trusted, Developer Mode,
# online for the model download, the maintainer's team signing); the corpus
# goes to and the outputs come from the app's data container. Simulator
# numbers are not phone numbers. Needs the core built (`just ios-core`).
source "$(dirname "$0")/../lib.sh"
[ "$(platform)" = macos-arm64 ] || die "the iOS tests build only on Apple Silicon macOS"
usage="usage: scripts/ios/ios-ocr-dpi.sh <hardware UDID | Simulator name> OUTPUT [DPI…]"
target=${1:?$usage}
output=${2:?$usage}
shift 2
resolutions=("$@")
[ ${#resolutions[@]} -gt 0 ] || resolutions=(150 200 300)
runs=${MDOC_OCR_DPI_RUNS:-3}
bundle=io.github.lawkitt.mdoc
team=F33DLM8K53
mkdir -p "$output"
output=$(cd "$output" && pwd)

if [[ $target =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{16}$ ]]; then
  device=yes
  destination="id=$target"
  derived=target/ios/DeviceTests
  xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
    DEVELOPMENT_TEAM="$team" -quiet build-for-testing
  corpus=$(mktemp -d)
  cp tests/fixtures/ocr-upstream/*.pdf tests/fixtures/ocr-qualification/*.pdf "$corpus/"
  rm -f "$corpus"/*.native.pdf
  # The container exists once mdoc is installed; the test run reinstalls it
  # over this copy and keeps its data.
  xcrun devicectl device install app --device "$target" "$derived/Build/Products/Debug-iphoneos/mdoc.app"
  xcrun devicectl device copy to --device "$target" --domain-type appDataContainer \
    --domain-identifier "$bundle" --source "$corpus" --destination Library/mdoc-ocr-dpi/corpus
  rm -rf "$corpus"
else
  device=no
  os=$(sed -n 's/^IOS_SIM_RUNTIME=//p' scripts/tools.env)
  destination="platform=iOS Simulator,OS=$os,name=$target"
  derived=target/ios/DerivedData
  xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -quiet build-for-testing
fi

# The Simulator writes straight into OUTPUT; a phone into its container.
extra=()
[ "$device" = yes ] || extra=(TEST_RUNNER_MDOC_OCR_DPI_OUT="$output")
for dpi in "${resolutions[@]}"; do
  echo "OCR at $dpi dpi, $runs runs a page"
  env TEST_RUNNER_MDOC_OCR_DPI="$dpi" TEST_RUNNER_MDOC_OCR_DPI_RUNS="$runs" ${extra[@]+"${extra[@]}"} \
    xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "$destination" \
    -derivedDataPath "$derived" -only-testing:mdocTests/OCRResolutionTests/testResolution \
    -quiet test-without-building
done

if [ "$device" = yes ]; then
  xcrun devicectl device copy from --device "$target" --domain-type appDataContainer \
    --domain-identifier "$bundle" --source Library/mdoc-ocr-dpi/out --destination "$output/device"
fi
# Each runs-<dpi>.tsv sits beside its <dpi>/ outputs, wherever the copy put them.
tables=()
while IFS= read -r table; do tables+=("$table"); done < <(find "$output" -name 'runs-*.tsv' | sort)
[ ${#tables[@]} -gt 0 ] || die "no runs-*.tsv under $output"
python3 tools/ocr-upstream/dpi.py --out "$output" --runs-from "${tables[@]}"
