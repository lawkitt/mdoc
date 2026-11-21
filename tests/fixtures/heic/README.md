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
