#!/usr/bin/env bash
# Usage: scripts/ios-spike-data.sh [mdoc-profile]
# Stages the feasibility spike's data under target/ios-spike/data (ADR 0085):
# copies of the fixtures, and copies of the OCR models and the GLiNER2 FP16
# model from an mdoc profile (default: this Mac's, where mdoc's setup put
# them). The profile is only read. tools/ios-spike bundles the folder, and
# the Mac stand-in (crates/mdoc-ffi/examples/spike.rs) reads it in place.
source "$(dirname "$0")/lib.sh"

profile=${1:-"$HOME/Library/Application Support/mdoc"}
out=target/ios-spike/data
[ -d "$profile/ocr/v1/models" ] || die "no OCR models in $profile: set up both OCR bundles in mdoc first"
fp16=$(find "$profile/pseudonymization" -maxdepth 1 -type d -name 'gliner2-pii-fp16-*' | head -n 1)
[ -n "$fp16" ] || die "no GLiNER2 FP16 model in $profile: set up pseudonymization in mdoc first"

rm -rf "$out"
mkdir -p "$out/fixtures" "$out/ocr" "$out/pseudonymization"
cp tests/fixtures/docx-preview/legal-opinion.docx "$out/fixtures/"
for language in english russian; do
  cp "tests/fixtures/ocr-qualification/$language.pdf" "tests/fixtures/ocr-qualification/$language.txt" "$out/fixtures/"
done
cp -R "$profile/ocr/v1/models" "$out/ocr/"
cp -R "$fp16" "$out/pseudonymization/"
echo "$out"
