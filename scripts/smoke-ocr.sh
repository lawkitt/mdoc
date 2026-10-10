#!/usr/bin/env bash
# Usage: scripts/smoke-ocr.sh
# The release gate: downloads the pinned OCR runtime and models into throwaway
# storage and recognizes the English and Russian qualification fixtures.
source "$(dirname "$0")/lib.sh"
require_shipped_target "smoke-ocr"

log=$(mktemp)
trap 'rm -f "$log"' EXIT
cargo test -p mdoc --bin mdoc ocr::tests::setup_and_offline_english_russian_smoke \
  -- --ignored --exact --nocapture 2>&1 | tee "$log"
# A renamed or cfg'd-out test would otherwise "pass" by running nothing.
grep -q "test result: ok. 1 passed" "$log" || die "the OCR smoke test did not run"
