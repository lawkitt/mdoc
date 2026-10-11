#!/usr/bin/env bash
# Regenerate the icon asset set from build/appicon.svg, a square SVG whose
# rounded square fills the canvas. Every size is rendered from the vector:
#   - macOS .icns: the shape at 824 of 1024 px, centred on a transparent
#     canvas, as Apple's icon grid expects (a full-bleed icon looks oversized
#     in the Dock and Finder);
#   - Windows .ico and resources/icons/icon.png: full-bleed.
# Requires macOS's sips and iconutil, plus Python 3 (standard library only).
set -euo pipefail
cd "$(dirname "$0")/.."          # repo root
SOURCE="build/appicon.svg"

for tool in sips iconutil python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing required tool: $tool (run this script on macOS with Python 3 installed)." >&2
    exit 1
  fi
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/icon.iconset"
mkdir -p "$ICONSET" "$TMP/mac"

# Check the source and write the padded macOS variant: the same drawing with
# a viewBox grown so the original fills 824/1024 of it.
python3 - "$SOURCE" "$TMP/mac.svg" <<'PY'
import re
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding='utf-8')
root = re.search(r'<svg\b[^>]*>', source)
box = root and re.search(r'viewBox="([^"]+)"', root.group(0))
if not box:
    sys.exit('build/appicon.svg needs a viewBox on its <svg> element.')
x, y, w, h = (float(v) for v in box.group(1).replace(',', ' ').split())
if w != h or w <= 0:
    sys.exit('build/appicon.svg must be square.')
pad = w * (1024 / 824 - 1) / 2
tag = root.group(0).replace(box.group(0), f'viewBox="{x - pad:g} {y - pad:g} {w + 2 * pad:g} {h + 2 * pad:g}"')
Path(sys.argv[2]).write_text(source.replace(root.group(0), tag, 1), encoding='utf-8')
PY

render() { # svg, size, out
  sips -s format png -z "$2" "$2" "$1" --out "$3" >/dev/null
}
for s in 16 32 48 64 128 256 512 1024; do
  render "$SOURCE" "$s" "$TMP/$s.png"
  render "$TMP/mac.svg" "$s" "$TMP/mac/$s.png"
done

# macOS .icns — listed in cargo-packager's icons.
for s in 16 32 128 256 512; do
  cp "$TMP/mac/$s.png" "$ICONSET/icon_${s}x${s}.png"
  cp "$TMP/mac/$((s*2)).png" "$ICONSET/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$TMP/icon.icns"

# Windows multi-resolution .ico, containing PNG images — embedded into
# the .exe via resources/icon.rc and listed in cargo-packager's icons.
python3 - "$TMP" <<'PY'
import struct
import sys
from pathlib import Path

root = Path(sys.argv[1])
sizes = (256, 128, 64, 48, 32, 16)
images = [(root / f'{size}.png').read_bytes() for size in sizes]
offset = 6 + 16 * len(sizes)
entries = []
for size, data in zip(sizes, images):
    dimension = size if size < 256 else 0
    entries.append(struct.pack('<BBBBHHII', dimension, dimension, 0, 0,
                               1, 32, len(data), offset))
    offset += len(data)
(root / 'icon.ico').write_bytes(struct.pack('<HHH', 0, 1, len(sizes))
                               + b''.join(entries) + b''.join(images))
PY

# Publish only after every format has been generated successfully.
mkdir -p build/windows resources/icons
cp "$TMP/1024.png" resources/icons/icon.png
cp "$TMP/icon.ico" build/windows/icon.ico
cp "$TMP/icon.ico" resources/icons/icon.ico
cp "$TMP/icon.icns" resources/icons/icon.icns

echo "Regenerated: build/windows/icon.ico, resources/icons/icon.{png,ico,icns}"
