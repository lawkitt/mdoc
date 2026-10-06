#!/usr/bin/env bash
# Regenerate the icon asset set from build/appicon.png. Keep this source
# untouched; replace it with a square PNG (ideally at least 1024 x 1024).
# Requires macOS's sips and iconutil, plus Python 3 (standard library only).
set -euo pipefail
cd "$(dirname "$0")/.."          # repo root
SOURCE="build/appicon.png"

for tool in sips iconutil python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing required tool: $tool (run this script on macOS with Python 3 installed)." >&2
    exit 1
  fi
done

# Check the source before generating or replacing any assets.
python3 - "$SOURCE" <<'PY'
import struct
import sys
from pathlib import Path

header = Path(sys.argv[1]).read_bytes()[:24]
if len(header) != 24 or header[:8] != b'\x89PNG\r\n\x1a\n' or header[12:16] != b'IHDR':
    sys.exit('build/appicon.png must be a PNG image.')
width, height = struct.unpack('>II', header[16:24])
if width != height or width == 0:
    sys.exit('build/appicon.png must be square.')
PY

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/icon.iconset"
mkdir -p "$ICONSET"

# Resize each size directly from the source, preserving transparency.
for s in 16 32 48 64 128 256 512 1024; do
  sips -z "$s" "$s" "$SOURCE" --out "$TMP/$s.png" >/dev/null
done

# macOS .icns — listed in cargo-packager's icons.
for s in 16 32 128 256 512; do
  cp "$TMP/$s.png" "$ICONSET/icon_${s}x${s}.png"
  cp "$TMP/$((s*2)).png" "$ICONSET/icon_${s}x${s}@2x.png"
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
