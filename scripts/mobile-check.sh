#!/usr/bin/env bash
# Usage: scripts/mobile-check.sh
# The mobile core's build check (ADR 0085), run by the Mac's local CI after
# lint and test: `cargo check` of mdoc-ffi for iOS (device and Simulator) and
# Android, so desktop-only code cannot creep into the core. macOS only.
source "$(dirname "$0")/lib.sh"

[ "$(uname -s)" = Darwin ] || die "the mobile check runs on macOS"

targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android)
rustup target add "${targets[@]}"
for target in "${targets[@]}"; do
  echo "mobile-check: mdoc-ffi for $target"
  cargo check --locked -p mdoc-ffi --target "$target"
done
