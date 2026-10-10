#!/usr/bin/env bash
# Usage: scripts/smoke-models.sh [--download]
# Runs the default OCR and pseudonymization models through the app's own
# setup and offline paths. Uses the models already installed for this user;
# --download first installs them (verified, pinned downloads). Reports land
# in target/smoke-models/.
source "$(dirname "$0")/lib.sh"
require_shipped_target "smoke-models"

models=(ocr-v6 pii-fp16)
test_one() {
  cargo test -p mdoc --bin mdoc "$1" -- --ignored --nocapture --test-threads=1
}

if [ "${1:-}" = "--download" ]; then
  for model in "${models[@]}"; do
    echo "=== setup $model"
    MDOC_MODEL=$model test_one settings_model_setup_probe
  done
fi

mkdir -p target/smoke-models
for model in "${models[@]}"; do
  echo "=== offline $model"
  MDOC_MODEL=$model MDOC_REPORT="target/smoke-models/$model.json" test_one settings_model_offline_probe
done

echo "=== pseudonymization UI with the installed model"
test_one installed_model_offline_smoke
