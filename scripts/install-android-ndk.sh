#!/usr/bin/env bash
# Usage: scripts/install-android-ndk.sh
# Installs the pinned Android NDK (macOS only) under ~/Library/Android/ndk,
# verified against the size and SHA-1 Google publishes in
# https://dl.google.com/android/repository/repository2-3.xml (it publishes no
# SHA-256). Prints the NDK's directory on stdout. A new ANDROID_NDK_VERSION
# needs its size and digest here. About 1 GB, downloaded once.
source "$(dirname "$0")/lib.sh"
load_tools

[ "$(platform)" = macos-arm64 ] || die "the Android NDK install is for the Mac's mobile check"
case "$ANDROID_NDK_VERSION" in
  r29) size=1049519838 sha1=03d29fbb57e3c05a7d53597dd011d856c1456a4f ;;
  *) die "no size and SHA-1 for Android NDK $ANDROID_NDK_VERSION in $0" ;;
esac

root=$HOME/Library/Android/ndk
dir=$root/android-ndk-$ANDROID_NDK_VERSION
if [ ! -f "$dir/source.properties" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  zip=$tmp/ndk.zip
  echo "downloading Android NDK $ANDROID_NDK_VERSION (about 1 GB)" >&2
  curl -fsSL -o "$zip" "https://dl.google.com/android/repository/android-ndk-$ANDROID_NDK_VERSION-darwin.zip"
  [ "$(wc -c <"$zip" | tr -d ' ')" = "$size" ] || die "Android NDK size mismatch"
  [ "$(shasum -a 1 "$zip" | awk '{ print $1 }')" = "$sha1" ] || die "Android NDK checksum mismatch"
  mkdir -p "$tmp/unpacked" "$root"
  unzip -q "$zip" -d "$tmp/unpacked"
  mv "$tmp/unpacked/android-ndk-$ANDROID_NDK_VERSION" "$dir"
  echo "Android NDK $ANDROID_NDK_VERSION installed to $dir" >&2
fi
echo "$dir"
