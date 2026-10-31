#!/usr/bin/env bash
# Usage: scripts/ios-device-test.sh <hardware UDID> [auto|detector|below-floor]
# The device UI tests (ADR 0088) on a connected iPhone: the review flow and,
# by the phone's tier, the long jobs at the detector tier
# (DeviceLongJobsUITests) or the below-floor test (DeviceBelowFloorUITests),
# through the real background task the Simulator refuses. `auto` (the
# default) reads the tier from the phone's model; `below-floor` also runs on
# a phone that meets the floor, as UI tests take the tier from a stand-in
# memory size. Builds for the device, installs mdoc, copies the fixtures into
# its data container with devicectl (the app ships none; the UI tests' seed
# looks there when the Mac path does not exist), runs the tests, and fails on
# a new mdoc crash report. The phone must be unlocked, trusted, in Developer
# Mode with UI Automation on, and on Wi-Fi; the maintainer's team signs
# (apps/ios/README.md). The UDID is the 00008150-… id xcodebuild lists, not
# devicectl's CoreDevice identifier. Needs the core built (`just ios-core`).
source "$(dirname "$0")/lib.sh"
[ "$(platform)" = macos-arm64 ] || die "the device tests build only on Apple Silicon macOS"
usage="usage: scripts/ios-device-test.sh <hardware UDID> [auto|detector|below-floor]"
device=${1:?$usage}
tier=${2:-auto}

bundle=io.github.lawkitt.mdoc
team=F33DLM8K53
derived=target/ios/DeviceTests
fixtures=target/ios/device-fixtures
# AppModel.deviceFixturesFolder: the seed's folder in the data container.
container_fixtures=Library/mdoc-ui-fixtures
result=target/ios/device-test-$(date +%Y%m%d-%H%M%S).xcresult

# The detector floor is 8 GB (ADR 0088): the iPhone 15 Pro (iPhone16,x) and
# every iPhone from the 16 (iPhone17,x) on; earlier models have 6 GB or less.
if [ "$tier" = auto ]; then
  details=$(mktemp)
  xcrun devicectl device info details --device "$device" --json-output "$details" >/dev/null 2>&1 ||
    die "could not read the phone's model"
  model=$(/usr/bin/python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["result"]["hardwareProperties"]["productType"])' "$details")
  rm -f "$details"
  case "$model" in
    iPhone*)
      major=${model#iPhone}
      major=${major%%,*}
      ;;
    *) die "not an iPhone: $model" ;;
  esac
  if [ "$major" -ge 16 ]; then tier=detector; else tier=below-floor; fi
  echo "$model: $tier tier"
fi
case "$tier" in
  detector) tier_test=DeviceLongJobsUITests ;;
  below-floor) tier_test=DeviceBelowFloorUITests ;;
  *) die "$usage" ;;
esac

# Fixtures: the demo .docx, a 37-page .docx, a text PDF and camera pages
# rendered here.
rm -rf "$fixtures"
mkdir -p "$fixtures"
cp "docs/product-media/Settlement agreement.docx" tests/fixtures/docx-preview/legal-opinion.docx \
  tests/fixtures/import/text.pdf "$fixtures/"
xcrun swiftc -o target/ios/scan-pages scripts/ios-scan-pages.swift
target/ios/scan-pages "$fixtures/Tenancy scan" 6

xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "id=$device" \
  -derivedDataPath "$derived" -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
  DEVELOPMENT_TEAM="$team" -quiet build-for-testing

# The container exists once mdoc is installed; the test run reinstalls it
# over this copy and keeps its data.
xcrun devicectl device install app --device "$device" "$derived/Build/Products/Debug-iphoneos/mdoc.app"
xcrun devicectl device copy to --device "$device" --domain-type appDataContainer \
  --domain-identifier "$bundle" --source "$fixtures" --destination "$container_fixtures"

# The phone's mdoc crash reports, one name a line. Fails when they cannot be
# listed, so a broken query never reads as "no new crash".
crash_logs() {
  local json listed=0
  json=$(mktemp)
  if xcrun devicectl device info files --device "$device" --domain-type systemCrashLogs \
    --json-output "$json" >/dev/null 2>&1; then
    /usr/bin/python3 - "$json" <<'PY' && listed=1
import json, sys
for f in json.load(open(sys.argv[1]))["result"]["files"]:
    name = f["relativePath"]
    if name.rsplit("/", 1)[-1].startswith("mdoc"):
        print(name)
PY
  fi
  rm -f "$json"
  [ "$listed" = 1 ]
}
before=$(crash_logs) || die "could not list the phone's crash reports"

status=0
xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "id=$device" \
  -derivedDataPath "$derived" -resultBundlePath "$result" \
  -only-testing:mdocUITests/ReviewFlowUITests -only-testing:"mdocUITests/$tier_test" \
  -quiet test-without-building || status=$?

if ! after=$(crash_logs); then
  echo "could not list the phone's crash reports after the tests" >&2
  status=1
elif new=$(comm -13 <(sort <<<"$before") <(sort <<<"$after")) && [ -n "$new" ]; then
  echo "new mdoc crash reports on the phone:" >&2
  echo "$new" >&2
  status=1
fi
echo "results: $result"
exit "$status"
