#!/usr/bin/env bash
# Usage: scripts/ios-toolchain.sh [--no-simulator]
# Readies the iOS job's pinned toolchain (scripts/tools.env, ADR 0091) and
# prints `DEVELOPER_DIR=<its Developer folder>` on stdout, which ci-ios.yml
# appends to $GITHUB_ENV (ADR 0095). Finds the Xcode whose `xcodebuild
# -version` is XCODE_VERSION (the selected one, or /Applications/Xcode_<v>.app
# as the hosted images name them) and fails when none is; downloads the
# IOS_SIM_RUNTIME Simulator runtime when that Xcode lacks it; creates the
# iPhone 17 the job tests on when the runtime has none. On a Mac already set
# up it changes nothing. --no-simulator stops after Xcode: a build of the
# core (main-caches.yml) needs no Simulator.
source "$(dirname "$0")/lib.sh"
[ "$(uname -s)" = Darwin ] || die "the iOS toolchain is macOS only"
case "${1:-}" in
  '' | --no-simulator) ;;
  *) die "usage: scripts/ios-toolchain.sh [--no-simulator]" ;;
esac
load_tools

device="iPhone 17"
runtime_id="com.apple.CoreSimulator.SimRuntime.iOS-${IOS_SIM_RUNTIME//./-}"
device_type="com.apple.CoreSimulator.SimDeviceType.${device// /-}"

# Xcode: the pinned version exactly, as mobile-check.sh requires.
xcode_version() { { DEVELOPER_DIR=$1 xcodebuild -version 2>/dev/null || true; } | sed -n 's/^Xcode //p'; }
developer=
found=()
for app in "$(xcode-select -p 2>/dev/null)" /Applications/Xcode_"$XCODE_VERSION".app/Contents/Developer \
  /Applications/Xcode-"$XCODE_VERSION".app/Contents/Developer /Applications/Xcode*.app/Contents/Developer; do
  [ -d "$app" ] || continue
  v=$(xcode_version "$app")
  found+=("${v:-?} (${app%/Contents/Developer})")
  if [ "$v" = "$XCODE_VERSION" ]; then
    developer=$app
    break
  fi
done
[ -n "$developer" ] ||
  die "the iOS job needs Xcode $XCODE_VERSION (scripts/tools.env); found: ${found[*]:-none}. Install it, or pin another in its own PR"
export DEVELOPER_DIR=$developer
echo "ios-toolchain: Xcode $XCODE_VERSION at ${developer%/Contents/Developer}" >&2
if [ "${1:-}" = --no-simulator ]; then
  echo "DEVELOPER_DIR=$developer"
  exit 0
fi

# runtime_ready: the pinned runtime is installed and usable.
runtime_ready() {
  xcrun simctl list runtimes -j | python3 -c '
import json, sys
sys.exit(not any(r["identifier"] == sys.argv[1] and r.get("isAvailable")
                 for r in json.load(sys.stdin)["runtimes"]))
' "$runtime_id"
}

if ! runtime_ready; then
  # The build of the runtime that this Xcode pairs with its SDK when the
  # versions agree: Xcode 26.1.1 takes no other iOS 26.1 build (23B80 leaves
  # it without an iOS Simulator destination). Arm64 only, like the core.
  # About 8 GB from Apple; one retry, as for any download (ADR 0095).
  version=()
  [ "$(xcrun --sdk iphonesimulator --show-sdk-version)" = "$IOS_SIM_RUNTIME" ] ||
    version=(-buildVersion "$IOS_SIM_RUNTIME")
  echo "ios-toolchain: downloading the iOS $IOS_SIM_RUNTIME Simulator runtime" >&2
  start=$SECONDS
  download() { xcodebuild -downloadPlatform iOS ${version[@]+"${version[@]}"} -architectureVariant arm64 >&2; }
  download || {
    echo "ios-toolchain: the download failed; retrying once" >&2
    download
  }
  runtime_ready || die "iOS $IOS_SIM_RUNTIME is still not an available Simulator runtime after its download"
  echo "ios-toolchain: iOS $IOS_SIM_RUNTIME installed in $((SECONDS - start)) s" >&2
fi

# The device: the runtime's own, else one created on it.
has_device() {
  xcrun simctl list devices available -j | python3 -c '
import json, sys
sys.exit(not any(d["name"] == sys.argv[2]
                 for d in json.load(sys.stdin)["devices"].get(sys.argv[1], [])))
' "$runtime_id" "$device"
}
if ! has_device; then
  echo "ios-toolchain: creating the $device Simulator on iOS $IOS_SIM_RUNTIME" >&2
  xcrun simctl create "$device" "$device_type" "$runtime_id" >&2
fi

echo "DEVELOPER_DIR=$developer"
