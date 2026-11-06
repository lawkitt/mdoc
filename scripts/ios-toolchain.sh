#!/usr/bin/env bash
# Usage: scripts/ios-toolchain.sh [--no-simulator | --start | --finish]
# Readies the iOS job's pinned toolchain (scripts/tools.env, ADR 0091) and
# prints `DEVELOPER_DIR=<its Developer folder>` on stdout, which ci-ios.yml
# appends to $GITHUB_ENV (ADR 0095). Finds the Xcode whose `xcodebuild
# -version` is XCODE_VERSION (the selected one, or /Applications/Xcode_<v>.app
# as the hosted images name them) and fails when none is; uses the installed
# iOS Simulator runtime of build IOS_SIM_RUNTIME_BUILD (IOS_SIM_RUNTIME), and
# downloads that build only when it is missing (about 8 GB, 4 minutes on a
# hosted runner, whose image ships it); creates the iPhone 17 the job tests
# on when the runtime has none. On a Mac already set up it changes nothing.
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

# runtime: the identifier and version of the pinned runtime, or nothing.
runtime() { ios_sim_runtime; }
pin="iOS $IOS_SIM_RUNTIME ($IOS_SIM_RUNTIME_BUILD)"

# download: the runtime from Apple, one retry, as for any download (ADR 0095).
download() {
  # By build: a version alone is ambiguous (26.2 resolves to the 23C52
  # release candidate; 26.1 to 23B80, which left Xcode 26.1.1 without an iOS
  # Simulator destination), and no version takes the newest runtime this
  # Xcode supports (26.3.1 for Xcode 26.2; #654). Arm64 only, like the core.
  local start=$SECONDS
  echo "ios-toolchain: no $pin Simulator runtime installed; downloading it" >&2
  fetch() { xcodebuild -downloadPlatform iOS -buildVersion "$IOS_SIM_RUNTIME_BUILD" -architectureVariant arm64 >&2; }
  fetch || {
    echo "ios-toolchain: the download failed; retrying once" >&2
    fetch
  }
  # Runtimes of one version share an identifier, so with another build of
  # it installed Xcode may prefer that one.
  [ -n "$(runtime)" ] || die "$pin is still not an available Simulator runtime after its download; if another iOS $IOS_SIM_RUNTIME build is installed, prefer this one with: xcrun simctl runtime match set iphoneos$IOS_SIM_RUNTIME $IOS_SIM_RUNTIME_BUILD"
  echo "ios-toolchain: $pin installed in $((SECONDS - start)) s" >&2
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
      echo "ios-toolchain: downloading the $pin Simulator runtime in the background ($state/log)" >&2
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
      [ "$(cat "$state/status" 2>/dev/null)" = 0 ] || die "the background download of $pin failed (above)"
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
