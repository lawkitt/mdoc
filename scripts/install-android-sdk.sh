#!/usr/bin/env bash
# Usage: scripts/install-android-sdk.sh
# Installs what the Android app builds and runs with (ADR 0089, 0091) on
# Windows x64, Apple Silicon macOS or Linux x86_64, under $(android_home)
# (lib.sh: E:\Android on the PC, ~/Library/Android on the Mac, ~/Android on
# Linux), the SDK in its Sdk folder (mdoc-sdk on macOS and Linux, apart from
# Android Studio's; android_sdk):
#   jdk-21/                     Temurin 21, the JDK Gradle runs on
#   Sdk/cmdline-tools/latest    sdkmanager and avdmanager
#   Sdk/platform-tools          adb
#   Sdk/build-tools/<version>   the APK tools
#   Sdk/platforms/android-36    the API the app targets
#   Sdk/platforms/android-37.0  the API it compiles against (AndroidX needs 37)
#   Sdk/emulator                the emulator
#   Sdk/system-images/...       an API 36 image with 16 KB pages, so the
#                               emulator rejects a misaligned library:
#                               x86_64, or arm64-v8a on the Mac
#                               (android_emulator_abi)
# Each archive is pinned: Google's by size and SHA-1 from repository2-3.xml
# and sys-img2-3.xml (no SHA-256 published), the JDK by Adoptium's SHA-256.
# Each package gets the package.xml sdkmanager would write, copied from
# Google's manifest entry for the pinned archive
# (scripts/android-package-xml.py), so AGP and avdmanager know it.
# Command-line tools 23 and later take no licence acceptance. Prints the SDK
# directory on stdout. About 3 GB, downloaded once; a new pin replaces only
# its own directory. A directory this script did not install (no .mdoc-pin)
# is never replaced; MDOC_ANDROID_HOME points elsewhere.
source "$(dirname "$0")/lib.sh"

home=$(android_home)
sdk=$(android_sdk)
cache=$home/downloads

repo=https://dl.google.com/android/repository
sysimg=$repo/sys-img/google_apis
jdk_base=https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9
# <directory under Sdk> <url> <size> <sha1>, the same versions everywhere.
platforms=(
  "platforms/android-36 $repo/platform-36_r02.zip 65878410 2c1a80dd4d9f7d0e6dd336ec603d9b5c55a6f576"
  "platforms/android-37.0 $repo/platform-37.0_r02.zip 67281901 ed8ebf7f8822a4de5686d427f237d2fa30ff7410"
)
case "$(platform)" in
  windows-x64)
    packages=(
      "cmdline-tools/latest $repo/commandlinetools-win-16111833_latest.zip 154957218 57d04f2d75eb8e8fffc5000a987e5de4b5a63e9d"
      "platform-tools $repo/platform-tools_r37.0.1-win.zip 8044989 e03e78b1d80b396f1c3358e31251cb31740e1110"
      "build-tools/36.1.0 $repo/build-tools_r36.1_windows.zip 59366211 95f048ae758dad90617975fe3f787f038b03338c"
      "emulator $repo/emulator-windows_x64-16740729.zip 455344678 4d7268aa7787c640ad892844e6c9a5f159e91ad3"
      "system-images/android-36/google_apis_ps16k/x86_64 $sysimg/x86_64-ps16k-36_r07.zip 1849428145 dd783282e84bf475a02eba6777c79fc5695e1583"
    )
    jdk_url=$jdk_base/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip
    jdk_sha=a4dbaa05fca4bd89d84584e58c459bd732efebb1722af80ab13c6516832004f9
    ;;
  macos-arm64)
    packages=(
      "cmdline-tools/latest $repo/commandlinetools-mac_arm64-16111833_latest.zip 155384151 ad03dc49bfacfd52c110b14104ea548b8a07e830"
      "platform-tools $repo/platform-tools_r37.0.1-darwin.zip 16110554 6ae73f4de6452dc57e62ec02b68eed92a4c21661"
      "build-tools/36.1.0 $repo/build-tools_r36.1_macosx.zip 80527175 d365b05ca1810ca92b428b92717865ad4188b5e1"
      "emulator $repo/emulator-darwin_aarch64-16740729.zip 416113219 6c5e61b873e11c7d1d08683bea089f762baee070"
      "system-images/android-36/google_apis_ps16k/arm64-v8a $sysimg/arm64-v8a-ps16k-36_r07.zip 1875087037 dda632745023567113d9954101709c6526a3c250"
    )
    jdk_url=$jdk_base/OpenJDK21U-jdk_aarch64_mac_hotspot_21.0.13_9.tar.gz
    jdk_sha=bf036b35b071bfb2f54f2c0af8ee8d88afa95ee6eb3dce02c0f94cfc2524d04a
    ;;
  Linux-x86_64)
    packages=(
      "cmdline-tools/latest $repo/commandlinetools-linux-16111833_latest.zip 181052239 e025545c62a8e64c7559119566a569fb1dec5f60"
      "platform-tools $repo/platform-tools_r37.0.1-linux.zip 9054187 477254aa5f903c15cf51001717bdf347fb6b53e0"
      "build-tools/36.1.0 $repo/build-tools_r36.1_linux.zip 64195204 936a0d6bd5ae3e2118a7567dddbc95ff67ed46e9"
      "emulator $repo/emulator-linux_x64-16740729.zip 349658551 f41f9e31eb5ca64926603c03265c50bbc0ff4172"
      "system-images/android-36/google_apis_ps16k/x86_64 $sysimg/x86_64-ps16k-36_r07.zip 1849428145 dd783282e84bf475a02eba6777c79fc5695e1583"
    )
    jdk_url=$jdk_base/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13_9.tar.gz
    jdk_sha=d7f5d5cf865a2902b70ff549b97586a7d3f234e016f041ab447ea740c7d66d1c
    ;;
  *) die "the Android SDK install is for Windows x64, Apple Silicon macOS or Linux x86_64 (this is $(platform))" ;;
esac
packages+=("${platforms[@]}")
mkdir -p "$sdk" "$cache"

# The manifest that lists a package's archive: package.xml is copied from it.
manifest_for() {
  local url=$1 manifest
  case "$url" in
    "$sysimg"/*) manifest=$cache/sys-img2-3.xml url=$sysimg/sys-img2-3.xml ;;
    *) manifest=$cache/repository2-3.xml url=$repo/repository2-3.xml ;;
  esac
  [ -f "$manifest" ] || curl "${CURL_PATIENT[@]}" --output "$manifest" "$url"
  echo "$manifest"
}
python=$(command -v python3 || command -v python) || die "Python is needed for the SDK's package.xml files"

# install <directory> <archive> <pin>: unpacks the archive's one top-level
# folder to <directory> and records the pin, so a matching pin skips it.
install() {
  local dir=$1 archive=$2 pin=$3 tmp
  [ "$(cat "$dir/.mdoc-pin" 2>/dev/null)" = "$pin" ] && return
  [ ! -e "$dir" ] || [ -f "$dir/.mdoc-pin" ] ||
    die "$dir was not installed by $0 (no .mdoc-pin): move it, or set MDOC_ANDROID_HOME to another folder"
  tmp=$(mktemp -d "$cache/unpack.XXXXXX")
  case "$archive" in
    *.zip) unzip -q "$archive" -d "$tmp" ;;
    *) tar -xzf "$archive" -C "$tmp" ;;
  esac
  local top=("$tmp"/*)
  if [ "${#top[@]}" != 1 ] || [ ! -d "${top[0]}" ]; then
    die "$archive: expected one top-level folder"
  fi
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
  if [ ! -f "$sdk/$dir/package.xml" ]; then
    "$python" scripts/android-package-xml.py "$(manifest_for "$url")" "$sha1" "$sdk/$dir/package.xml"
  fi
done
rm -f "$cache/repository2-3.xml" "$cache/sys-img2-3.xml"

if [ "$(cat "$home/jdk-21/.mdoc-pin" 2>/dev/null)" != "$jdk_sha" ]; then
  jdk=$cache/$(basename "$jdk_url")
  fetch_sha256 "$jdk_url" "$jdk_sha" "$jdk"
  install "$home/jdk-21" "$jdk" "$jdk_sha"
fi

echo "$sdk"
