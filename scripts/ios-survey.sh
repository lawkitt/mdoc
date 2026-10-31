#!/usr/bin/env bash
# The iOS UI survey (ADR 0087): run SurveyUITests on a Simulator and export
# every screen, light and dark, downscaled to 1000 px high, into a folder.
#   scripts/ios-survey.sh "iPhone 17" docs/evidence/ios-app/<date>-survey
# Needs the core built (`just ios-core`). Shuts the Simulators down after,
# since a booted one can push the Mac into swap and stall local CI.
set -euo pipefail
cd "$(dirname "$0")/.." # repo root
DEVICE="${1:-iPhone 17}"
OUT="${2:?usage: scripts/ios-survey.sh <device> <output folder>}"
RESULT="target/ios/survey-$(date +%s).xcresult"
EXPORT="$(mktemp -d)"
trap 'rm -rf "$EXPORT"; xcrun simctl shutdown all >/dev/null 2>&1 || true' EXIT

status=0
TEST_RUNNER_MDOC_SURVEY=1 xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc \
  -destination "platform=iOS Simulator,OS=26.1,name=$DEVICE" \
  -derivedDataPath target/ios/DerivedData -resultBundlePath "$RESULT" \
  -only-testing:mdocUITests/SurveyUITests -quiet test || status=$?

xcrun xcresulttool export attachments --path "$RESULT" --output-path "$EXPORT" >/dev/null
mkdir -p "$OUT"
# The manifest maps each exported file to the attachment's name.
python3 - "$EXPORT" "$OUT" "$DEVICE" <<'PY'
import json
import re
import shutil
import sys
from pathlib import Path

export, out, device = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
prefix = 'ipad-' if 'iPad' in device else ''
count = 0
for test in json.loads((export / 'manifest.json').read_text()):
    for item in test.get('attachments', []):
        name = item.get('suggestedHumanReadableName', '')
        match = re.match(r'(\d\d-[a-z0-9-]+-(?:light|dark))', name)
        if match:
            shutil.copy(export / item['exportedFileName'], out / f'{prefix}{match.group(1)}.png')
            count += 1
print(f'{count} screenshots in {out}')
PY
for f in "$OUT"/*.png; do sips -Z 1000 "$f" >/dev/null; done
exit "$status"
