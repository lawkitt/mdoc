#!/usr/bin/env bash
# Usage: scripts/android-spike-data.sh [mdoc-profile]
# Stages the Android feasibility spike's data as the harness's assets, under
# target/android-spike/assets/data (ADR 0089): copies of the fixtures; both
# OCR bundles (English PP-OCRv6 Small and Cyrillic), installed fresh through
# the desktop's pinned, verified downloads; and a copy of the GLiNER2 FP16
# model from an mdoc profile (default: this machine's), which is only read.
# tools/android-spike packs the folder into its APK.
source "$(dirname "$0")/lib.sh"

case "$(platform)" in
  windows-x64) default=$(cygpath -u "$LOCALAPPDATA")/mdoc ;;
  macos-arm64) default="$HOME/Library/Application Support/mdoc" ;;
  *) die "the spike data is staged on Windows x64 or Apple Silicon macOS" ;;
esac
profile=${1:-$default}
fp16=$(find "$profile/pseudonymization" -maxdepth 1 -type d -name 'gliner2-pii-fp16-*' 2>/dev/null | head -n 1)
[ -n "$fp16" ] || die "no GLiNER2 FP16 model in $profile: set up pseudonymization in mdoc first"

work=target/android-spike
out=$work/assets/data
# The OCR install keeps its verified downloads between runs.
cargo run --locked --release -p mdoc-ffi --example spike_ocr_models -- "$work/ocr-install"
rm -rf "$out"
mkdir -p "$out/fixtures" "$out/ocr" "$out/pseudonymization"
cp tests/fixtures/docx-preview/legal-opinion.docx "$out/fixtures/"
for language in english russian; do
  cp "tests/fixtures/ocr-qualification/$language.pdf" "tests/fixtures/ocr-qualification/$language.txt" "$out/fixtures/"
done
cp -R "$work/ocr-install/models" "$out/ocr/"
cp -R "$fp16" "$out/pseudonymization/"
echo "$out"
