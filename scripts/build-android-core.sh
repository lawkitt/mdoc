#!/usr/bin/env bash
# Usage: scripts/build-android-core.sh
# Builds the mobile core (crates/mdoc-ffi) for Android and assembles what the
# app packages (ADR 0089):
#   target/android/jniLibs/<abi>/  libmdoc_ffi.so, libonnxruntime.so, libpdfium.so
#   target/android/kotlin/         the core's generated Kotlin bindings
#   target/android/licenses/       ONNX Runtime's and PDFium's licences
#   target/android/host/           the core built for this computer, which the
#                                  app's JVM tests load
# for arm64-v8a (phones) and x86_64 (the emulator), with the NDK's clang for
# API 31, the apps' minimum. ONNX Runtime 1.27.0 (Microsoft's Maven AAR) and
# PDFium 7988 (bblanchon) are pinned, checksum-verified downloads; nothing is
# downloaded at run time. Fails if any library is not aligned for 16 KB pages
# or needs a shared library Android does not provide.
# Windows x64 or Apple Silicon macOS.
source "$(dirname "$0")/lib.sh"

case "$(platform)" in
  windows-x64) host=windows-x86_64 exe=.exe host_lib=mdoc_ffi.dll ;;
  macos-arm64) host=darwin-x86_64 exe='' host_lib=libmdoc_ffi.dylib ;;
  *) die "the Android core builds on Windows x64 or Apple Silicon macOS" ;;
esac

api=31
ort_url=https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/1.27.0/onnxruntime-android-1.27.0.aar
ort_sha=077dec5e2d821234c7dc0aba584bec8f999854b546c754cab93a90741c56fbeb
ort_license_url=https://raw.githubusercontent.com/microsoft/onnxruntime/v1.27.0/LICENSE
ort_license_sha=2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c
pdfium_base=https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7988
# <rust target> <android abi> <pdfium archive> <pdfium sha256>
slices=(
  "aarch64-linux-android arm64-v8a pdfium-android-arm64.tgz 0e36e095e592845c38267a0b2f0e73b13d60aa831b2959605e6a5b012edc330a"
  "x86_64-linux-android x86_64 pdfium-android-x64.tgz 922bb43dcc74d73c0fdf016a34128d9e042a09b604174ada2efa276a457ff30d"
)

ndk=$(scripts/install-android-ndk.sh)
bin=$ndk/toolchains/llvm/prebuilt/$host/bin
[ -x "$bin/clang$exe" ] || die "no clang in $bin"
readelf=$bin/llvm-readelf$exe

out=target/android
# Cargo's own output, which CARGO_TARGET_DIR may move.
cargo_target=${CARGO_TARGET_DIR:-target}
deps=$out/deps
rm -rf "$out/jniLibs" "$out/kotlin" "$out/licenses" "$out/host"
mkdir -p "$deps" "$out/kotlin" "$out/licenses/pdfium" "$out/host"

fetch_sha256 "$ort_url" "$ort_sha" "$deps/onnxruntime-android.aar"
fetch_sha256 "$ort_license_url" "$ort_license_sha" "$out/licenses/onnxruntime-LICENSE"
rm -rf "$deps/onnxruntime" && mkdir "$deps/onnxruntime"
unzip -q "$deps/onnxruntime-android.aar" 'jni/*/libonnxruntime.so' -d "$deps/onnxruntime"

for slice in "${slices[@]}"; do
  read -r target abi pdfium pdfium_sha <<<"$slice"
  rustup target add "$target" >/dev/null 2>&1 || rustup target add "$target"
  upper=$(tr 'a-z-' 'A-Z_' <<<"$target")
  under=${target//-/_}
  # clang itself, told the target and API level, rather than the NDK's .cmd
  # wrappers: a linker behind cmd.exe hits Windows' command-line limit.
  clang=$(native_path "$bin/clang$exe")
  export "CC_$under=$clang" "CFLAGS_$under=--target=$target$api"
  export "AR_$under=$(native_path "$bin/llvm-ar$exe")"
  export "CARGO_TARGET_${upper}_LINKER=$clang"
  export "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=--target=$target$api -C link-arg=-Wl,-z,max-page-size=16384"
  echo "build-android-core: mdoc-ffi for $target" >&2
  # cdylib only here: desktop and iOS keep the crate's lib and staticlib.
  cargo rustc --locked --profile mobile -p mdoc-ffi --lib --target "$target" --crate-type cdylib

  dir=$out/jniLibs/$abi
  mkdir -p "$dir"
  cp "$cargo_target/$target/mobile/libmdoc_ffi.so" "$dir/"
  cp "$deps/onnxruntime/jni/$abi/libonnxruntime.so" "$dir/"
  fetch_sha256 "$pdfium_base/$pdfium" "$pdfium_sha" "$deps/$pdfium"
  rm -rf "$deps/pdfium-$abi" && mkdir "$deps/pdfium-$abi"
  tar -xzf "$deps/$pdfium" -C "$deps/pdfium-$abi"
  cp "$deps/pdfium-$abi/lib/libpdfium.so" "$dir/"
done
cp "$deps/pdfium-arm64-v8a/LICENSE" "$out/licenses/pdfium/LICENSE"
cp "$deps/pdfium-arm64-v8a/licenses/"* "$out/licenses/pdfium/"

# Every library: 16 KB-aligned LOAD segments (Play requires it for Android
# 15+), and no shared library outside Android's public NDK ones and our own.
allowed=" libc.so libm.so libdl.so liblog.so libandroid.so libmdoc_ffi.so libonnxruntime.so libpdfium.so "
for so in "$out"/jniLibs/*/*.so; do
  while read -r align; do
    [ $((align)) -ge 16384 ] || die "$so: a LOAD segment is aligned to $align, not 16 KB"
  done < <("$readelf" -lW "$so" | awk '$1 == "LOAD" { print $NF }')
  while read -r needed; do
    [[ $allowed == *" $needed "* ]] || die "$so needs $needed, which the APK does not ship"
  done < <("$readelf" -dW "$so" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p')
done

# The same core for this computer: the app's JVM parity tests call it
# through the same Kotlin bindings (ADR 0089).
echo "build-android-core: mdoc-ffi for this computer" >&2
cargo rustc --locked --profile mobile -p mdoc-ffi --lib --crate-type cdylib
cp "$cargo_target/mobile/$host_lib" "$out/host/"

# Kotlin bindings from the library's UniFFI metadata.
cargo run --locked -p mdoc-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$cargo_target/aarch64-linux-android/mobile/libmdoc_ffi.so" \
  --language kotlin --no-format --out-dir "$out/kotlin"
echo "$out"
