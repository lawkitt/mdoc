# HEIC fixtures

Synthetic HEIC photos of one fictional contract page, for #1024 items 9
(HEIC on desktop) and 10 (phones use the shared image path). Every name,
place and number on the page is made up. `make-heic.sh` rebuilds them on
macOS: `page.swift` draws the page with Core Text and writes the variants
with ImageIO, and `sips` converts the plain upright page. The output isn't
byte-stable across macOS versions.

A red arrow in the page's top-left corner shows which way is up.

| File | Stored pixels | Metadata | Expected import |
|---|---|---|---|
| `page-upright.heic` | 1200 × 1600, upright | orientation 1, no camera data | the page as stored |
| `page-orientation-6.heic` | 1600 × 1200, page turned 90° counter-clockwise | EXIF orientation 6 | upright 1200 × 1600 page, orientation applied |
| `page-gps.heic` | 1200 × 1600, upright | orientation 1, GPS 12.3456 N 65.4321 W, Make `Fixture Camera Co`, Model `Model F1`, DateTimeOriginal `2031:03:14 09:26:53` | the page, with no GPS, camera or date left in the Markdown or the built PDF |

All three are HEVC-coded HEIC, encoded by ImageIO on macOS 27.2.

Three more, for #1054's review:

| File | What it is | Expected import |
|---|---|---|
| `transparent.heic` | 64 × 64 with an alpha plane: left half fully transparent over black samples, the next quarter half transparent black, the last quarter opaque red (220, 30, 30) | white, mid gray and red: transparency on white, nothing under it showing |
| `two-images.heic` | two 64 × 64 images, blue first, red second; the second is primary (`pitm`) | red, the primary image |
| `page-orientation-6-decoded.jpg` | the upright JPEG a Windows import stores for `page-orientation-6.heic` (WIC with HEIF 1.2.65.0 and HEVC 2.5.48.0, quality 92, no metadata) | the Windows render's stand-in for the HEIC, since CI's Windows runners have no HEIC codec |

`make-heif-extra.py` writes the first two with pillow-heif 1.8.1 (libheif
1.23.6, x265): `python3 make-heif-extra.py tests/fixtures/heic`. The JPEG
is the builder's output on a PC with the extensions (the JPEG stream of
`image_pdf::build` on `page-orientation-6.heic`).
