#!/usr/bin/env bash
# Usage: scripts/mobile-check.sh
# The mobile check (ADR 0085), run by the Mac's local CI after lint and test:
# `cargo check` of mdoc-ffi for iOS (device and Simulator) and Android, so
# desktop-only code cannot creep into the core; then, when the commit
# changes crates/, Cargo.lock or apps/ (on a merge ref, against its main),
# the iOS job: the core, the app and its tests on a Simulator of the pinned
# Xcode and runtime (scripts/tools.env, ADR 0091), shut down afterwards if
# this run booted it. macOS only.
# MDOC_IOS_JOB=always or never overrides the path check.
source "$(dirname "$0")/lib.sh"

[ "$(uname -s)" = Darwin ] || die "the mobile check runs on macOS"

targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android)
rustup target add "${targets[@]}"

# Android's C dependencies (ring, onig) build with the NDK's clang, for
# API 31: Android 12, the app's minSdk (ADR 0089).
ndk=$(scripts/install-android-ndk.sh)
bin=$ndk/toolchains/llvm/prebuilt/darwin-x86_64/bin
export CC_aarch64_linux_android=$bin/aarch64-linux-android31-clang
export AR_aarch64_linux_android=$bin/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$CC_aarch64_linux_android
for target in "${targets[@]}"; do
  echo "mobile-check: mdoc-ffi for $target"
  cargo check --locked -p mdoc-ffi --target "$target"
done

# --- the iOS job ------------------------------------------------------------

ios_changed() {
  case "${MDOC_IOS_JOB:-}" in
    always) return 0 ;;
    never) return 1 ;;
  esac
  git rev-parse --verify --quiet HEAD^1 >/dev/null || return 0
  # Not piped into grep -q: an early exit would fail the pipeline.
  local changed
  changed=$(git diff --name-only HEAD^1 HEAD)
  grep -qE '^(crates/|Cargo\.lock$|apps/)' <<<"$changed"
}

if ! ios_changed; then
  echo "mobile-check: no core or app change; the iOS job is skipped"
  exit 0
fi

load_tools
# The pinned Xcode, compared exactly: a newer one changes the SDK and the
# Simulator runtimes under the tests.
xcode=$(xcodebuild -version 2>/dev/null | sed -n 's/^Xcode //p')
[ "$xcode" = "$XCODE_VERSION" ] ||
  die "the iOS job needs Xcode $XCODE_VERSION (scripts/tools.env); this Mac has ${xcode:-none}. Install it, or pin the new one in its own PR"

device="iPhone 17"
runtime="iOS-${IOS_SIM_RUNTIME//./-}"
# The Simulator the tests use, and whether it was running before.
udid=$(xcrun simctl list devices available -j | python3 -c '
import json, sys
devices = json.load(sys.stdin)["devices"]
for runtime, entries in devices.items():
    if runtime.endswith(sys.argv[2]):
        for device in entries:
            if device["name"] == sys.argv[1]:
                print(device["udid"], device["state"])
                sys.exit(0)
' "$device" "$runtime")
[ -n "$udid" ] || die "no $device Simulator on iOS $IOS_SIM_RUNTIME (scripts/tools.env); add one in Xcode › Settings › Components"
read -r udid was <<<"$udid"
shutdown() {
  if [ "$was" != Booted ]; then
    echo "mobile-check: shutting down the $device Simulator"
    xcrun simctl shutdown "$udid" 2>/dev/null || true
  fi
}
trap shutdown EXIT

echo "mobile-check: the iOS core, app and tests"
just ios-app ios-test "$device"
