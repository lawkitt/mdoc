#!/bin/bash
# Usage: tests/fixtures/heic/make-heic.sh
# Rebuilds the HEIC fixtures on macOS: Swift draws the fictional page and
# writes the orientation and metadata variants with ImageIO; sips converts the
# plain upright page. Output is not byte-stable across macOS versions.
set -euo pipefail
cd "$(dirname "$0")"
swift page.swift .
sips -s format heic -s formatOptions 60 page-source.png --out page-upright.heic >/dev/null
mv -f page-source.png "${TMPDIR:-/tmp}/mdoc-heic-page-source.png"
