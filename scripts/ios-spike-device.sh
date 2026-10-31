#!/usr/bin/env bash
# Usage: DEVELOPMENT_TEAM=<team-id> scripts/ios-spike-device.sh [device-id] [report.json]
#        scripts/ios-spike-device.sh --simulator <simulator-udid> [report.json]
# Runs the feasibility spike's harness (tools/ios-spike, ADR 0085) on a
# connected, paired iPhone or iPad, or in a booted Simulator, and copies its
# JSON report back (default: target/ios-spike/reports/<device>-<time>.json).
# Builds the iOS core (scripts/build-ios-core.sh) and stages the fixtures and
# models from this Mac's mdoc profile (scripts/ios-spike-data.sh) first;
# MDOC_SPIKE_PROFILE names another profile, MDOC_SPIKE_SKIP_BUILD=1 reuses
# both. A device needs Developer Mode on and the Mac's Apple ID team signed
# in to Xcode: xcodebuild -allowProvisioningUpdates makes the profile and
# registers the phone with the team. [device-id] is the hardware UDID
# (00008150-…, as xcodebuild lists it), not devicectl's CoreDevice identifier.
source "$(dirname "$0")/lib.sh"
[ "$(platform)" = macos-arm64 ] || die "the harness builds only on Apple Silicon macOS"

bundle=io.github.lawkitt.mdoc.spike
project=tools/ios-spike/MdocSpike.xcodeproj
derived=target/ios-spike/build
simulator=0
if [ "${1:-}" = --simulator ]; then
  simulator=1
  device=${2:?"--simulator needs a booted simulator's UDID (xcrun simctl list devices booted)"}
  report=${3:-}
else
  device=${1:-}
  report=${2:-}
  [ -n "${DEVELOPMENT_TEAM:-}" ] || die "set DEVELOPMENT_TEAM to the signing team's ID (Xcode > Settings > Accounts)"
fi

if [ "${MDOC_SPIKE_SKIP_BUILD:-0}" != 1 ]; then
  scripts/build-ios-core.sh
  scripts/ios-spike-data.sh ${MDOC_SPIKE_PROFILE:+"$MDOC_SPIKE_PROFILE"}
fi

if [ "$simulator" = 1 ]; then
  xcodebuild -project "$project" -scheme MdocSpike -configuration Release \
    -destination "id=$device" -derivedDataPath "$derived" build -quiet
  app=$derived/Build/Products/Release-iphonesimulator/MdocSpike.app
  xcrun simctl install "$device" "$app"
  # Waits for the harness to exit, with its output on this terminal.
  xcrun simctl launch --console --terminate-running-process "$device" "$bundle" --exit-when-done
  source_report=$(xcrun simctl get_app_container "$device" "$bundle" data)/Documents/spike-report.json
  name=simulator-$device
else
  if [ -z "$device" ]; then
    json=target/ios-spike/devices.json
    mkdir -p "$(dirname "$json")"
    xcrun devicectl list devices --json-output "$json" >/dev/null
    device=$(plutil -extract result.devices json -o - "$json" | /usr/bin/python3 -c '
import json, sys
for d in json.load(sys.stdin):
    p = d.get("connectionProperties", {})
    h = d.get("hardwareProperties", {})
    if p.get("pairingState") == "paired" and h.get("platform") == "iOS":
        print(h["udid"]); break')
    [ -n "$device" ] || die "no paired iPhone or iPad is connected (xcrun devicectl list devices)"
  fi
  xcodebuild -project "$project" -scheme MdocSpike -configuration Release \
    -destination "id=$device" -derivedDataPath "$derived" -allowProvisioningUpdates \
    -allowProvisioningDeviceRegistration DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" build -quiet
  app=$derived/Build/Products/Release-iphoneos/MdocSpike.app
  xcrun devicectl device install app --device "$device" "$app"
  # --console waits for the harness to exit and shows its output.
  xcrun devicectl device process launch --device "$device" --terminate-existing --console \
    "$bundle" --exit-when-done
  source_report=
  name=device-$device
fi

report=${report:-target/ios-spike/reports/$name-$(date +%Y%m%d-%H%M%S).json}
mkdir -p "$(dirname "$report")"
if [ "$simulator" = 1 ]; then
  cp "$source_report" "$report"
else
  xcrun devicectl device copy from --device "$device" --domain-type appDataContainer \
    --domain-identifier "$bundle" --source Documents/spike-report.json --destination "$report"
fi
echo "$report"
