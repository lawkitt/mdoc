#!/usr/bin/env bash
# Usage: scripts/android-spike-data.sh
# Stages the Android feasibility spike's fixtures as the harness's assets,
# under target/android-spike/assets/data/fixtures (ADR 0089). The models are
# not bundled: the harness downloads them on the phone through the core's
# Setup, as the app does.
source "$(dirname "$0")/lib.sh"

out=target/android-spike/assets/data
rm -rf "$out"
mkdir -p "$out/fixtures"
cp tests/fixtures/docx-preview/legal-opinion.docx "$out/fixtures/"
for language in english russian; do
  cp "tests/fixtures/ocr-qualification/$language.pdf" "tests/fixtures/ocr-qualification/$language.txt" "$out/fixtures/"
done
echo "$out"
