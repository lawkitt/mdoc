#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1"]
# ///
"""Rebuild resources/fonts-ios/NotoSansCJKsc-Regular-Subset.otf, the iOS
app's Chinese, Japanese and Korean face for DOCX Originals (#1036).

Usage:
  scripts/dev/subset-cjk-font.py <NotoSansCJKsc-Regular.otf> <output folder>

The source is noto-cjk's Sans2.004 NotoSansCJKsc-Regular.otf; its SHA-256 is
checked first, so the same input always gives the same bytes. The subset keeps:

- ASCII, Latin-1 and the general punctuation CJK text uses;
- every character of GB2312, JIS X 0208 and KS X 1001, the national standard
  sets for Simplified Chinese, Japanese and Korean, taken from Python's
  gb2312, euc_jp and euc_kr codecs: 10,450 Han characters among them;
- a short list of common characters GB2312 (1980) lacks (EXTRAS);
- all 11,172 Hangul syllables and the Hangul compatibility jamo;
- kana with its extensions and half-width forms, CJK punctuation, enclosed
  and compatibility CJK (㈜, ㎡), vertical and full-width forms.

Then it adds two family names, "Noto Sans CJK JP" and "Noto Sans CJK KR", in
Japanese and Korean name records beside "Noto Sans CJK SC". docxide-pdf finds
a fallback face by family name from a fixed list per script, and only its
Chinese and Han-only lists name the SC face: a missing kana or Hangul
character is looked up under the JP or KR name instead (`cjk_rescue_font`).

It drops the OpenType layout tables (GSUB, GPOS, GDEF) and with them the
alternate glyphs they reach (vertical, Japanese, Korean and Traditional
forms): docxide-pdf draws CJK text by cmap alone and reads GSUB only in math
fonts. Hinting and the other tables stay as pyftsubset leaves them; the
timestamp is not touched.
"""

import hashlib
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

SOURCE_SHA256 = "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b"
OUTPUT = "NotoSansCJKsc-Regular-Subset.otf"
FAMILY = "Noto Sans CJK SC"
# Windows language IDs for the extra family names (ja-JP, ko-KR).
ALIASES = {0x0411: "Noto Sans CJK JP", 0x0412: "Noto Sans CJK KR"}

# Common in modern Simplified Chinese text and names, absent from GB2312:
# 啰 (Table of General Standard Chinese Characters, level 1), and name
# characters such as 镕 (朱镕基), 喆, 堃, 赟, 昇, 珺, 玥, 甦, 犇, 祎, 曌, 彧, 頔.
EXTRAS = "啰镕喆堃赟昇珺玥甦犇祎曌彧頔"

RANGES = [
    (0x0020, 0x007E),  # ASCII
    (0x00A0, 0x00FF),  # Latin-1
    (0x2000, 0x206F),  # General Punctuation
    (0x3000, 0x303F),  # CJK Symbols and Punctuation
    (0x3040, 0x30FF),  # Hiragana, Katakana
    (0x3130, 0x318F),  # Hangul Compatibility Jamo
    (0x31F0, 0x31FF),  # Katakana Phonetic Extensions
    (0x3200, 0x33FF),  # Enclosed CJK Letters and Months, CJK Compatibility
    (0xAC00, 0xD7A3),  # Hangul Syllables, all 11,172
    (0xFE10, 0xFE1F),  # Vertical Forms
    (0xFE30, 0xFE4F),  # CJK Compatibility Forms
    (0xFF00, 0xFFEF),  # Halfwidth and Fullwidth Forms
]


def double_byte(codec: str) -> set[int]:
    """Every character a two-byte EUC code (rows and cells 0xA1-0xFE) names."""
    chars = set()
    for lead in range(0xA1, 0xFF):
        for trail in range(0xA1, 0xFF):
            try:
                text = bytes([lead, trail]).decode(codec)
            except UnicodeDecodeError:
                continue
            if len(text) == 1:
                chars.add(ord(text))
    return chars


def unicodes() -> set[int]:
    gb2312 = double_byte("gb2312")
    extras = {ord(c) for c in EXTRAS}
    if extras & gb2312:
        sys.exit(f"EXTRAS already in GB2312: {''.join(map(chr, sorted(extras & gb2312)))}")
    chars = gb2312 | double_byte("euc_jp") | double_byte("euc_kr") | extras
    for first, last in RANGES:
        chars.update(range(first, last + 1))
    return chars


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__.split("\n\n")[1])
    source, folder = Path(sys.argv[1]), Path(sys.argv[2])
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        sys.exit(f"{source}: SHA-256 {digest}, expected {SOURCE_SHA256} (noto-cjk Sans2.004)")

    font = TTFont(source, recalcTimestamp=False)
    wanted = unicodes()
    cmap = font.getBestCmap()
    absent = {c for c in (ord(c) for c in EXTRAS) if c not in cmap}
    if absent:
        sys.exit(f"the source lacks {''.join(map(chr, sorted(absent)))}")
    options = subset.Options()
    options.recalc_timestamp = False
    options.layout_features = []
    options.drop_tables += ["GSUB", "GPOS", "GDEF"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=wanted)
    subsetter.subset(font)

    name = font["name"]
    if name.getDebugName(1) != FAMILY:
        sys.exit(f"family is {name.getDebugName(1)!r}, expected {FAMILY!r}")
    for lang, alias in ALIASES.items():
        name.setName(alias, 1, 3, 1, lang)

    out = folder / OUTPUT
    font.save(out)
    kept = len(set(font.getBestCmap()))
    print(f"{out}: {out.stat().st_size} bytes, {kept} characters, {len(font.getGlyphOrder())} glyphs")
    print(f"{hashlib.sha256(out.read_bytes()).hexdigest()}  {OUTPUT}")


if __name__ == "__main__":
    main()
