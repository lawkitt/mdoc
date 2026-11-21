# TIFF fixtures

Synthetic multi-page TIFFs for ROADMAP "Unlocked files and images" item 8
(multi-page and fax TIFF, #1024): every page must import, and CCITT G3 and
G4 strips pass through to PDFium as `CCITTFaxDecode`. Every name, company,
amount and date on the pages is fictional. Each file mixes English and
Russian pages, so OCR's language choice is exercised too.

| File | Pages | Compression | Size, resolution | Photometric | Fill order | Notes |
|---|---|---|---|---|---|---|
| `fax-g3-1d.tif` | 3 | CCITT Group 3 1D (Modified Huffman), T4Options 4 | 1728 × 2200, 204 × 196 dpi (fax fine) | min-is-white | 1 (MSB first) | one strip per page |
| `fax-g3-2d-normal-lsb.tif` | 2 | CCITT Group 3 2D, T4Options 1 | 1728 × 1100, 204 × 98 dpi (fax normal) | min-is-black | 2 (LSB first) | 4 strips per page; half-height rows, so the page keeps its shape only if the vertical resolution is honoured |
| `fax-g4.tif` | 2 | CCITT Group 4 (T.6) | 2550 × 3300, 300 dpi | min-is-white | 1 | one strip per page |
| `scan-lzw-gray.tif` | 2 | LZW, horizontal predictor | 1275 × 1650, 150 dpi, 8-bit gray | min-is-black | 1 | several strips |
| `scan-lzw-rgb.tif` | 1 | LZW, horizontal predictor | 1275 × 1650, 150 dpi, 8-bit RGB | RGB | 1 | pale yellow paper |

The pages, in order:

- `fax-g3-1d.tif`: a supply agreement (English), the same agreement
  (Russian), a signature page (English, serif).
- `fax-g3-2d-normal-lsb.tif`: the first two pages of `fax-g3-1d.tif`.
- `fax-g4.tif` and `scan-lzw-gray.tif`: a notice of termination (English,
  serif), the same notice (Russian).
- `scan-lzw-rgb.tif`: the English notice.

## Regenerating

1. `tests/fixtures/tiff/generate.sh` (Git Bash on Windows, with
   ImageMagick 7 built with libtiff; it uses Windows' Arial and Times New
   Roman, or `TIFF_FIXTURE_SANS` and `TIFF_FIXTURE_SERIF`).
2. ImageMagick writes Group 3 only as 1D, so rewrite the 2D file with
   Pillow built with libtiff (here, in WSL): `python3 g3-2d.py
   fax-g3-2d-normal-lsb.tif out.tif`, then move `out.tif` over it.

Another ImageMagick, libtiff, Pillow or font build may write other bytes.
The tests rely on the pages' text and on the tags in the table, not on the
bytes.
