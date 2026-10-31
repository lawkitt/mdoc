#!/usr/bin/env bash
# Usage: scripts/install-android-ndk.sh
# Installs the pinned Android NDK under $(android_home)/ndk (lib.sh): on the
# Mac for its mobile check, on Windows for the Android app (ADR 0089).
# Verified against the size and SHA-1 Google publishes in
# https://dl.google.com/android/repository/repository2-3.xml (it publishes no
# SHA-256). Prints the NDK's directory on stdout. A new ANDROID_NDK_VERSION
# needs its sizes and digests here. About 1 GB, downloaded once.
source "$(dirname "$0")/lib.sh"
load_tools

case "$(platform)/$ANDROID_NDK_VERSION" in
  macos-arm64/r29) os=darwin size=1049519838 sha1=03d29fbb57e3c05a7d53597dd011d856c1456a4f ;;
  windows-x64/r29) os=windows size=833850862 sha1=ab3bb30fbb9e6903666d60c55d11e78b04e07472 ;;
  macos-arm64/* | windows-x64/*) die "no size and SHA-1 for Android NDK $ANDROID_NDK_VERSION on $(platform) in $0" ;;
  *) die "the Android NDK install is for Apple Silicon macOS or Windows x64" ;;
esac

root=$(android_home)/ndk
dir=$root/android-ndk-$ANDROID_NDK_VERSION
if [ ! -f "$dir/source.properties" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  zip=$tmp/ndk.zip
  fetch_pinned "https://dl.google.com/android/repository/android-ndk-$ANDROID_NDK_VERSION-$os.zip" "$size" "$sha1" "$zip"
  mkdir -p "$tmp/unpacked" "$root"
  unzip -q "$zip" -d "$tmp/unpacked"
  mv "$tmp/unpacked/android-ndk-$ANDROID_NDK_VERSION" "$dir"
  echo "Android NDK $ANDROID_NDK_VERSION installed to $dir" >&2
fi
echo "$dir"
