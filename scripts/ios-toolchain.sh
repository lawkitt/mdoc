#!/usr/bin/env bash
# Usage: scripts/ios-toolchain.sh [--no-simulator | --start | --finish]
# Readies the iOS job's pinned toolchain (scripts/tools.env, ADR 0091) and
# prints `DEVELOPER_DIR=<its Developer folder>` on stdout, which ci-ios.yml
# appends to $GITHUB_ENV (ADR 0095). Finds the Xcode whose `xcodebuild
# -version` is XCODE_VERSION (the selected one, or /Applications/Xcode_<v>.app
# as the hosted images name them) and fails when none is; uses an installed
# iOS Simulator runtime whose major.minor is IOS_SIM_RUNTIME, and downloads
# that runtime only when none is (about 8 GB, 4 minutes on a hosted runner);
# creates the iPhone 17 the job tests on when the runtime has none. On a Mac
# already set up it changes nothing.
#
#   --no-simulator  Xcode only: a build of the core (main-caches.yml) needs
#                   no Simulator runtime. An app build does, even for a
#                   generic destination.
#   --start         Xcode, then any download in the background, logged in
#                   target/ios-toolchain/, so it overlaps the core's build.
#   --finish        Waits for --start's download, then readies the runtime
#                   and the device as with no option.
source "$(dirname "$0")/lib.sh"
[ "$(uname -s)" = Darwin ] || die "the iOS toolchain is macOS only"
mode=${1:-}
case "$mode" in
  '' | --no-simulator | --start | --finish) ;;
  *) die "usage: scripts/ios-toolchain.sh [--no-simulator | --start | --finish]" ;;
esac
load_tools

device="iPhone 17"
device_type="com.apple.CoreSimulator.SimDeviceType.${device// /-}"
# --start's background download: its log and, once it ends, its exit code.
state=target/ios-toolchain

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
if [ "$mode" = --no-simulator ]; then
  echo "DEVELOPER_DIR=$developer"
  exit 0
fi

# runtime: the identifier and version of an available iOS runtime whose
# major.minor is the pin's (26.2 takes 26.2 or 26.2.x), or nothing.
runtime() {
  xcrun simctl list runtimes -j | python3 -c '
import json, sys
want = sys.argv[1].split(".")[:2]
for r in json.load(sys.stdin)["runtimes"]:
    if (r.get("platform") == "iOS" and r.get("isAvailable")
            and r["version"].split(".")[:2] == want):
        print(r["identifier"], r["version"])
        break
' "$IOS_SIM_RUNTIME"
}

# download: the runtime from Apple, one retry, as for any download (ADR 0095).
download() {
  # The build of the runtime that this Xcode pairs with its SDK when the
  # versions agree: Xcode 26.1.1 took no other iOS 26.1 build (23B80 left
  # it without an iOS Simulator destination). Arm64 only, like the core.
  local version=() start=$SECONDS
  [ "$(xcrun --sdk iphonesimulator --show-sdk-version)" = "$IOS_SIM_RUNTIME" ] ||
    version=(-buildVersion "$IOS_SIM_RUNTIME")
  echo "ios-toolchain: no iOS $IOS_SIM_RUNTIME Simulator runtime installed; downloading it" >&2
  fetch() { xcodebuild -downloadPlatform iOS ${version[@]+"${version[@]}"} -architectureVariant arm64 >&2; }
  fetch || {
    echo "ios-toolchain: the download failed; retrying once" >&2
    fetch
  }
  [ -n "$(runtime)" ] || die "iOS $IOS_SIM_RUNTIME is still not an available Simulator runtime after its download"
  echo "ios-toolchain: iOS $IOS_SIM_RUNTIME installed in $((SECONDS - start)) s" >&2
}

case "$mode" in
  --start)
    if [ -z "$(runtime)" ]; then
      rm -rf "$state" && mkdir -p "$state"
      # Detached, so the step ends now; the job's later steps keep it alive.
      # shellcheck disable=SC2016 # the inner shell expands them
      nohup bash -c 'scripts/ios-toolchain.sh; echo $? >"$0/status"' "$state" \
        </dev/null >"$state/log" 2>&1 &
      echo $! >"$state/pid"
      echo "ios-toolchain: downloading the iOS $IOS_SIM_RUNTIME Simulator runtime in the background ($state/log)" >&2
    fi
    echo "DEVELOPER_DIR=$developer"
    exit 0
    ;;
  --finish)
    if [ -d "$state" ]; then
      start=$SECONDS
      while ! [ -s "$state/status" ] && kill -0 "$(cat "$state/pid")" 2>/dev/null; do sleep 5; done
      # Its own lines and the end of Xcode's; the progress lines are noise.
      grep -v '^Downloading ' "$state/log" | tail -n 40 >&2 || true
      echo "ios-toolchain: waited $((SECONDS - start)) s for the background download" >&2
      [ "$(cat "$state/status" 2>/dev/null)" = 0 ] || die "the background download of iOS $IOS_SIM_RUNTIME failed (above)"
    fi
    ;;
esac

[ -n "$(runtime)" ] || download
read -r runtime_id runtime_version <<<"$(runtime)"
echo "ios-toolchain: iOS $runtime_version Simulator runtime ($runtime_id)" >&2

# The device: the runtime's own, else one created on it.
has_device() {
  xcrun simctl list devices available -j | python3 -c '
import json, sys
sys.exit(not any(d["name"] == sys.argv[2]
                 for d in json.load(sys.stdin)["devices"].get(sys.argv[1], [])))
' "$runtime_id" "$device"
}
if ! has_device; then
  echo "ios-toolchain: creating the $device Simulator on iOS $runtime_version" >&2
  xcrun simctl create "$device" "$device_type" "$runtime_id" >&2
fi

echo "DEVELOPER_DIR=$developer"
