#!/usr/bin/env bash
# Usage: scripts/install-android-sdk.sh
# Installs what the Android app builds and runs with (ADR 0089), Windows x64
# only for now, under $(android_home) (lib.sh, E:\Android on the PC):
#   jdk-21/                     Temurin 21, the JDK Gradle runs on
#   Sdk/cmdline-tools/latest    sdkmanager and avdmanager
#   Sdk/platform-tools          adb
#   Sdk/build-tools/<version>   the APK tools
#   Sdk/platforms/android-36    the API the app compiles against
#   Sdk/emulator                the emulator
#   Sdk/system-images/...       an API 36 x86_64 image with 16 KB pages, so
#                               the emulator rejects a misaligned library
# Each archive is pinned: Google's by size and SHA-1 from repository2-3.xml
# and sys-img2-3.xml (no SHA-256 published), the JDK by Adoptium's SHA-256.
# Command-line tools 23 and later take no licence acceptance. Prints the SDK
# directory on stdout. About 3 GB, downloaded once; a new pin replaces only
# its own directory.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android SDK install is for Windows x64"

home=$(android_home)
sdk=$home/Sdk
cache=$home/downloads
mkdir -p "$sdk" "$cache"

repo=https://dl.google.com/android/repository
# <directory under Sdk> <url> <size> <sha1>
packages=(
  "cmdline-tools/latest $repo/commandlinetools-win-16111833_latest.zip 154957218 57d04f2d75eb8e8fffc5000a987e5de4b5a63e9d"
  "platform-tools $repo/platform-tools_r37.0.1-win.zip 8044989 e03e78b1d80b396f1c3358e31251cb31740e1110"
  "build-tools/36.1.0 $repo/build-tools_r36.1_windows.zip 59366211 95f048ae758dad90617975fe3f787f038b03338c"
  "platforms/android-36 $repo/platform-36_r02.zip 65878410 2c1a80dd4d9f7d0e6dd336ec603d9b5c55a6f576"
  "emulator $repo/emulator-windows_x64-16740729.zip 455344678 4d7268aa7787c640ad892844e6c9a5f159e91ad3"
  "system-images/android-36/google_apis_ps16k/x86_64 $repo/sys-img/google_apis/x86_64-ps16k-36_r07.zip 1849428145 dd783282e84bf475a02eba6777c79fc5695e1583"
)

# install <directory> <archive> <pin>: unpacks the archive's one top-level
# folder to <directory> and records the pin, so a matching pin skips it.
install() {
  local dir=$1 archive=$2 pin=$3 tmp
  [ "$(cat "$dir/.mdoc-pin" 2>/dev/null)" = "$pin" ] && return
  tmp=$(mktemp -d "$cache/unpack.XXXXXX")
  case "$archive" in
    *.zip) unzip -q "$archive" -d "$tmp" ;;
    *) tar -xzf "$archive" -C "$tmp" ;;
  esac
  local top=("$tmp"/*)
  [ "${#top[@]}" = 1 ] && [ -d "${top[0]}" ] || die "$archive: expected one top-level folder"
  rm -rf "${dir:?}"
  mkdir -p "$(dirname "$dir")"
  mv "${top[0]}" "$dir"
  rm -rf "${tmp:?}"
  echo "$pin" >"$dir/.mdoc-pin"
  echo "installed $dir" >&2
}

for entry in "${packages[@]}"; do
  read -r dir url size sha1 <<<"$entry"
  file=$cache/$(basename "$url")
  if [ "$(cat "$sdk/$dir/.mdoc-pin" 2>/dev/null)" != "$sha1" ]; then
    fetch_pinned "$url" "$size" "$sha1" "$file"
    install "$sdk/$dir" "$file" "$sha1"
  fi
done

jdk_url=https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip
jdk_sha=a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9
if [ "$(cat "$home/jdk-21/.mdoc-pin" 2>/dev/null)" != "$jdk_sha" ]; then
  fetch_sha256 "$jdk_url" "$jdk_sha" "$cache/temurin-21.zip"
  install "$home/jdk-21" "$cache/temurin-21.zip" "$jdk_sha"
fi

echo "$sdk"
