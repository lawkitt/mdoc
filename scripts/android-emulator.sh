#!/usr/bin/env bash
# Usage: scripts/android-emulator.sh [--window] [phone|tablet|ci]
# Boots an Android emulator from the pinned API 36 image with 16 KB pages
# (scripts/install-android-sdk.sh), x86_64, or arm64-v8a on the Mac
# (android_emulator_abi), headless unless --window, waits until Android has
# booted and prints its adb serial (ADR 0089, 0091). The virtual
# devices live in $(android_home)/avd and are written here on first use: a
# Pixel 8-sized phone (emulator-5554), a Pixel Tablet-sized tablet
# (emulator-5556) and CI's own phone (emulator-5560), so a CI run
# never tests on an emulator someone is using. Already running, it checks
# the device is that AVD with 16 KB pages and prints the serial.
# `adb -s <serial> emu kill` stops it. Windows x64, Apple Silicon macOS, or
# Linux x86_64 with KVM (read and write access to /dev/kvm).
source "$(dirname "$0")/lib.sh"

case "$(platform)" in
  windows-x64) exe=.exe ;;
  macos-arm64) exe='' ;;
  Linux-x86_64)
    exe=''
    if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
      die "the emulator needs KVM: no read and write access to /dev/kvm (join the kvm group, or on a hosted runner add a udev rule for it)"
    fi
    ;;
  *) die "the Android emulator runs on Windows x64, Apple Silicon macOS or Linux x86_64" ;;
esac
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
sdk=$(android_sdk)
abi=$(android_emulator_abi)
case "$abi" in
  x86_64) arch=x86_64 ;;
  arm64-v8a) arch=arm64 ;;
esac
image=system-images/android-36/google_apis_ps16k/$abi
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
# A device on this port counts only if it is this AVD with 16 KB pages:
# another one (Android Studio's, say, with 4 KB pages) would quietly skip
# the 16 KB check.
ours() {
  local running pages
  running=$("${adb[@]}" emu avd name 2>/dev/null | tr -d '\r' | head -n 1)
  pages=$("${adb[@]}" shell getconf PAGESIZE 2>/dev/null | tr -d '\r')
  [ "$running" = "$name" ] ||
    die "$serial is ${running:-another device}, not $name: stop it (adb -s $serial emu kill) and run this again"
  [ "$pages" = 16384 ] || die "$serial has ${pages:-unknown}-byte pages, not 16 KB"
}
if booted; then
  ours
  echo "$serial"
  exit
fi

# The device, written here rather than by avdmanager, which needs the JDK
# and asks questions. The emulator itself needs only this. Windows paths
# take backslashes.
if [ ! -f "$avd/config.ini" ]; then
  mkdir -p "$avd"
  if [ "$(platform)" = windows-x64 ]; then
    avd_path=$(cygpath -w "$avd") avd_rel="avd\\$name.avd" sysdir="${image//\//\\}\\"
  else
    avd_path=$avd avd_rel=avd/$name.avd sysdir=$image/
  fi
  cat >"$home/avd/$name.ini" <<INI
avd.ini.encoding=UTF-8
path=$avd_path
path.rel=$avd_rel
target=android-36
INI
  cat >"$avd/config.ini" <<INI
AvdId=$name
avd.ini.displayname=mdoc $kind (API 36, 16 KB pages)
abi.type=$abi
hw.cpu.arch=$arch
hw.cpu.ncore=4
hw.ramSize=$ram
vm.heapSize=512
disk.dataPartition.size=12G
image.sysdir.1=$sysdir
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
"$sdk/emulator/emulator$exe" "${flags[@]}" >"$log" 2>&1 &
"$sdk/platform-tools/adb" start-server >/dev/null 2>&1
for _ in $(seq 1 180); do
  if booted; then
    ours
    "${adb[@]}" shell settings put global window_animation_scale 0 >/dev/null
    "${adb[@]}" shell settings put global transition_animation_scale 0 >/dev/null
    "${adb[@]}" shell settings put global animator_duration_scale 0 >/dev/null
    # CI's tests need the app's window to keep input focus (the clipboard
    # reads null without it, #545): the screen stays on, no ANR or crash
    # dialog takes focus, and the keyguard and any system dialog close.
    if [ "$kind" = ci ]; then
      "${adb[@]}" shell svc power stayon true >/dev/null
      "${adb[@]}" shell settings put global hide_error_dialogs 1 >/dev/null
      "${adb[@]}" shell wm dismiss-keyguard >/dev/null || true
      "${adb[@]}" shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null || true
    fi
    echo "$serial"
    exit
  fi
  sleep 2
done
die "$name did not boot in 6 minutes; see $log"
