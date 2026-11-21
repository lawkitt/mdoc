#!/usr/bin/env bash
# Usage: tests/fixtures/tiff/generate.sh
# Regenerates the synthetic TIFF fixtures in this folder with ImageMagick 7
# (`magick`, built with libtiff) and the Arial and Times New Roman fonts
# Windows ships. Every name and number on the pages is fictional.
# A different ImageMagick, libtiff or font build may write other bytes; the
# pages' text and the TIFF tags (compression, resolution, fill order, page
# count) are what the tests rely on.
set -euo pipefail
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -r "${work:?}"' EXIT

sans=${TIFF_FIXTURE_SANS:-C:/Windows/Fonts/arial.ttf}
serif=${TIFF_FIXTURE_SERIF:-C:/Windows/Fonts/times.ttf}

# page NAME WIDTH HEIGHT FONT SIZE TEXT: black text on white, one line per \n.
page() {
  magick -size "$2x$3" xc:white -fill black -font "$4" -pointsize "$5" \
    -interline-spacing 18 -annotate +120+200 "$6" "$work/$1.png"
}

# Fax-width pages (1728 px across an A4/Letter line at 204 dpi), fine mode.
page en1 1728 2200 "$sans" 46 $'SUPPLY AGREEMENT No. 17/26\n\nDate: 3 March 2026\n\nSeller: Northwind Paper Ltd,\nrepresented by Martin Hale, Director.\n\nBuyer: Brightwater Legal LLP,\nrepresented by Clara Voss, Partner.\n\n1. The Seller delivers 400 reams of\n   archival paper by 30 April 2026.\n2. The Buyer pays EUR 6,250 within\n   14 days of delivery.'
page ru2 1728 2200 "$sans" 46 $'ДОГОВОР ПОСТАВКИ № 17/26\n\nг. Казань, 3 марта 2026 г.\n\nПоставщик: ООО «Северный ветер»,\nв лице директора Петрова Ивана\nСергеевича.\n\nПокупатель: АО «Светлая вода»,\nв лице Орловой Анны Викторовны.\n\n1. Поставщик передает 400 пачек\n   бумаги до 30 апреля 2026 г.\n2. Покупатель оплачивает 412 000 руб.'
page en3 1728 2200 "$serif" 50 $'SIGNATURES\n\nFor the Seller: Martin Hale\n\n______________________\n\nFor the Buyer: Clara Voss\n\n______________________\n\nPage 3 of 3'

# 300 dpi Letter-width scan pages for G4 and LZW.
page scan1 2550 3300 "$serif" 58 $'NOTICE OF TERMINATION\n\nTo: Brightwater Legal LLP\nAttn: Clara Voss\n\nNorthwind Paper Ltd terminates the Supply\nAgreement No. 17/26 with effect from\n1 June 2026 under clause 9.2.\n\nMartin Hale, Director'
page scan2 2550 3300 "$sans" 54 $'УВЕДОМЛЕНИЕ О РАСТОРЖЕНИИ\n\nКому: АО «Светлая вода»\nВниманию: Орловой А. В.\n\nООО «Северный ветер» расторгает договор\nпоставки № 17/26 с 1 июня 2026 г.\nпо пункту 9.2.\n\nДиректор И. С. Петров'

bilevel=(-colorspace Gray -threshold 55% -type bilevel)

# G3 1D (Modified Huffman, T4Options 0), fine mode 204x196 dpi, 3 pages.
magick "$work/en1.png" "$work/ru2.png" "$work/en3.png" "${bilevel[@]}" \
  -units PixelsPerInch -density 204x196 -compress Fax \
  -define tiff:group3-options=0 -define tiff:photometric=min-is-white fax-g3-1d.tif

# G3 2D (T4Options bit 0), normal mode 204x98 dpi (half-height rows: the page
# keeps its shape only if the vertical resolution is honoured), and FillOrder
# 2 (least significant bit first), as many fax modems write. 2 pages.
magick \( "$work/en1.png" -resize '100%x50%!' \) \( "$work/ru2.png" -resize '100%x50%!' \) \
  "${bilevel[@]}" -units PixelsPerInch -density 204x98 -compress Fax \
  -define tiff:group3-options=1 -define tiff:fill-order=lsb -define tiff:photometric=min-is-white fax-g3-2d-normal-lsb.tif

# G4 (T.6), 300 dpi, 2 pages.
magick "$work/scan1.png" "$work/scan2.png" "${bilevel[@]}" \
  -units PixelsPerInch -density 300 -compress Group4 -define tiff:photometric=min-is-white fax-g4.tif

# LZW, 8-bit grayscale with horizontal predictor, 150 dpi, 2 pages.
magick "$work/scan1.png" "$work/scan2.png" -alpha off -colorspace Gray -resize 50% \
  -depth 8 -units PixelsPerInch -density 150 -compress LZW \
  -define tiff:predictor=2 scan-lzw-gray.tif

# LZW, 8-bit RGB, 150 dpi, 1 page with a pale yellow background.
magick "$work/scan1.png" -resize 50% -fill '#fff6d5' -opaque white \
  -type TrueColor -depth 8 -units PixelsPerInch -density 150 -compress LZW \
  scan-lzw-rgb.tif
