#!/usr/bin/env bash
# Usage: scripts/android-emulator.sh [--window] [phone|tablet|ci]
# Boots an Android emulator from the pinned API 36 x86_64 image with 16 KB
# pages (scripts/install-android-sdk.sh), headless unless --window, waits
# until Android has booted and prints its adb serial (ADR 0089). The virtual
# devices live in $(android_home)/avd and are written here on first use: a
# Pixel 8-sized phone (emulator-5554), a Pixel Tablet-sized tablet
# (emulator-5556) and local CI's own phone (emulator-5560), so a CI run
# never tests on an emulator someone is using. Already running, it only
# prints the serial.
# `adb -s <serial> emu kill` stops it. Windows x64.
source "$(dirname "$0")/lib.sh"

[ "$(platform)" = windows-x64 ] || die "the Android emulator runs on Windows x64"
window=no
if [ "${1:-}" = --window ]; then
  window=yes
  shift
fi
kind=${1:-phone}
case "$kind" in
  phone) port=5554 width=1080 height=2400 density=420 ram=6144 ;;
  tablet) port=5556 width=2560 height=1600 density=320 ram=6144 ;;
  ci) port=5560 width=1080 height=2400 density=420 ram=4096 ;;
  *) die "usage: $0 [--window] [phone|tablet|ci]" ;;
esac

home=$(android_home)
sdk=$home/Sdk
image=system-images/android-36/google_apis_ps16k/x86_64
[ -f "$sdk/$image/system.img" ] || die "no emulator image: run just android-setup"
name=mdoc-$kind
avd=$home/avd/$name.avd
serial=emulator-$port
adb=("$sdk/platform-tools/adb" -s "$serial")
export ANDROID_HOME ANDROID_SDK_ROOT ANDROID_AVD_HOME
ANDROID_HOME=$(native_path "$sdk")
ANDROID_SDK_ROOT=$ANDROID_HOME
ANDROID_AVD_HOME=$(native_path "$home/avd")

booted() {
  [ "$("${adb[@]}" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]
}
if booted; then
  echo "$serial"
  exit
fi

# The device, written by hand: avdmanager cannot place an image installed
# without sdkmanager's package metadata. The emulator itself needs only this.
if [ ! -f "$avd/config.ini" ]; then
  mkdir -p "$avd"
  cat >"$home/avd/$name.ini" <<INI
avd.ini.encoding=UTF-8
path=$(cygpath -w "$avd")
path.rel=avd\\$name.avd
target=android-36
INI
  cat >"$avd/config.ini" <<INI
AvdId=$name
avd.ini.displayname=mdoc $kind (API 36, 16 KB pages)
abi.type=x86_64
hw.cpu.arch=x86_64
hw.cpu.ncore=4
hw.ramSize=$ram
vm.heapSize=512
disk.dataPartition.size=12G
image.sysdir.1=${image//\//\\}\\
tag.id=google_apis_ps16k
tag.display=Google APIs, 16 KB Page Size
target=android-36
hw.lcd.width=$width
hw.lcd.height=$height
hw.lcd.density=$density
hw.keyboard=yes
hw.mainKeys=no
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader_indirect
hw.camera.back=virtualscene
hw.camera.front=emulated
showDeviceFrame=no
PlayStore.enabled=false
INI
fi

flags=(-avd "$name" -port "$port" -no-snapshot -no-boot-anim -no-audio -gpu swiftshader_indirect)
[ $window = yes ] || flags+=(-no-window)
mkdir -p target/android
log=target/android/emulator-$kind.log
echo "android-emulator: booting $name ($serial); log in $log" >&2
"$sdk/emulator/emulator.exe" "${flags[@]}" >"$log" 2>&1 &
"$sdk/platform-tools/adb" start-server >/dev/null 2>&1
for _ in $(seq 1 180); do
  if booted; then
    "${adb[@]}" shell settings put global window_animation_scale 0 >/dev/null
    "${adb[@]}" shell settings put global transition_animation_scale 0 >/dev/null
    "${adb[@]}" shell settings put global animator_duration_scale 0 >/dev/null
    echo "$serial"
    exit
  fi
  sleep 2
done
die "$name did not boot in 6 minutes; see $log"
