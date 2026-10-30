#!/usr/bin/env bash
# Usage: scripts/build-ios-core.sh
# Builds the mobile core (crates/mdoc-ffi) for iOS devices and the Simulator
# and assembles what an iOS app links (ADR 0085):
#   target/ios/MdocCore.xcframework  the core with ONNX Runtime linked in
#   target/ios/swift/                its generated Swift bindings
#   target/ios/pdfium.xcframework    PDFium as an embedded framework
# ONNX Runtime and PDFium are pinned, checksum-verified downloads; nothing
# is downloaded at run time. The app must export _OrtGetApiBase and keep it
# through stripping (tools/ios-spike shows the build settings).
source "$(dirname "$0")/lib.sh"
[ "$(platform)" = macos-arm64 ] || die "the iOS core builds only on Apple Silicon macOS"

# Microsoft's iOS build of the ONNX Runtime desktop ships (src/ocr.rs).
ort_url="https://download.onnxruntime.ai/pod-archive-onnxruntime-c-1.27.0.zip"
ort_sha="8c74edd600eafc3055de9e8f7a9602afee44ed516913cb5e132bca02cc34622c"
# PDFium 7988, the version desktop ships, from bblanchon/pdfium-binaries: no
# static iOS build exists there, so it embeds as a framework.
pdfium_base="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7988"
pdfium_device_sha="cda317b85a6dd06fa8c2965f7108149f21296b3622352f0f29c78f0047ea380b"
pdfium_simulator_sha="eaa56dacce4f0aa819d646199cfb84bf9e6eebd22a1d185394be5a3415d93455"

export IPHONEOS_DEPLOYMENT_TARGET=17.0
out=target/ios
deps=$out/deps
rm -rf "$out/MdocCore.xcframework" "$out/pdfium.xcframework" "$out/swift" "$out/stage"
mkdir -p "$deps" "$out/swift" "$out/stage"

# fetch <url> <sha256> <file>: downloads unless a verified copy is there.
fetch() {
  if ! [ -f "$3" ] || ! echo "$2  $3" | shasum -a 256 -c --status; then
    curl --fail --silent --show-error --location --retry 3 --output "$3.part" "$1"
    echo "$2  $3.part" | shasum -a 256 -c --status || die "$1 failed its checksum"
    mv "$3.part" "$3"
  fi
}

fetch "$ort_url" "$ort_sha" "$deps/onnxruntime.zip"
rm -rf "$deps/onnxruntime" && mkdir "$deps/onnxruntime"
unzip -q "$deps/onnxruntime.zip" -d "$deps/onnxruntime"
ort=$deps/onnxruntime/onnxruntime.xcframework

# slice <rust-target> <ort-slice> <pdfium-archive> <pdfium-sha>
slice() {
  local target=$1 stage=$out/stage/$1
  cargo build --locked --profile mobile -p mdoc-ffi --lib --target "$target"
  mkdir -p "$stage/lib" "$stage/pdfium.framework"
  # One arm64 static library: the core, then ONNX Runtime for the core's
  # reference to OrtGetApiBase to pull in.
  lipo "$ort/$2/onnxruntime.framework/onnxruntime" -thin arm64 -output "$stage/onnxruntime.a"
  libtool -static -no_warning_for_no_symbols -o "$stage/lib/libMdocCore.a" \
    "target/$target/mobile/libmdoc_ffi.a" "$stage/onnxruntime.a"

  fetch "$pdfium_base/$3" "$4" "$deps/$3"
  rm -rf "$stage/pdfium" && mkdir "$stage/pdfium"
  tar -xzf "$deps/$3" -C "$stage/pdfium"
  cp "$stage/pdfium/lib/libpdfium.dylib" "$stage/pdfium.framework/pdfium"
  install_name_tool -id @rpath/pdfium.framework/pdfium "$stage/pdfium.framework/pdfium"
  cat >"$stage/pdfium.framework/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>pdfium</string>
  <key>CFBundleIdentifier</key><string>io.github.lawkitt.mdoc.pdfium</string>
  <key>CFBundleName</key><string>pdfium</string>
  <key>CFBundlePackageType</key><string>FMWK</string>
  <key>CFBundleShortVersionString</key><string>153.0.7988</string>
  <key>CFBundleVersion</key><string>7988</string>
  <key>MinimumOSVersion</key><string>26.0</string>
</dict>
</plist>
PLIST
}

slice aarch64-apple-ios ios-arm64 pdfium-ios-device-arm64.tgz "$pdfium_device_sha"
slice aarch64-apple-ios-sim ios-arm64_x86_64-simulator pdfium-ios-simulator-arm64.tgz "$pdfium_simulator_sha"

# Swift bindings from the library's UniFFI metadata.
cargo run --locked -p mdoc-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/aarch64-apple-ios/mobile/libmdoc_ffi.a \
  --language swift --no-format --out-dir "$out/swift"
headers=$out/stage/headers
mkdir -p "$headers"
cp "$out/swift/mdoc_ffiFFI.h" "$headers/"
cp "$out/swift/mdoc_ffiFFI.modulemap" "$headers/module.modulemap"

xcodebuild -create-xcframework \
  -library "$out/stage/aarch64-apple-ios/lib/libMdocCore.a" -headers "$headers" \
  -library "$out/stage/aarch64-apple-ios-sim/lib/libMdocCore.a" -headers "$headers" \
  -output "$out/MdocCore.xcframework" >/dev/null
xcodebuild -create-xcframework \
  -framework "$out/stage/aarch64-apple-ios/pdfium.framework" \
  -framework "$out/stage/aarch64-apple-ios-sim/pdfium.framework" \
  -output "$out/pdfium.xcframework" >/dev/null
mkdir -p "$out/licenses"
cp "$deps/onnxruntime/LICENSE" "$out/licenses/onnxruntime-LICENSE"
cp -R "$out/stage/aarch64-apple-ios/pdfium/licenses" "$out/licenses/pdfium"
echo "$out/MdocCore.xcframework"
