"""Two more HEIC fixtures for #1024 item 9's review, written with
pillow-heif (libheif with x265):

- transparent.heic: 64 x 64, an alpha plane. Left half fully transparent
  over black samples, the next quarter half transparent black, the last
  quarter opaque red.
- two-images.heic: two 64 x 64 images, blue first and red second, the
  second marked primary.

Usage: python3 make-heif-extra.py OUT_DIR
"""

import sys

import pillow_heif
from PIL import Image

out = sys.argv[1]

alpha = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
for x in range(64):
    for y in range(64):
        if x < 32:
            alpha.putpixel((x, y), (0, 0, 0, 0))
        elif x < 48:
            alpha.putpixel((x, y), (0, 0, 0, 128))
        else:
            alpha.putpixel((x, y), (220, 30, 30, 255))
heif = pillow_heif.from_pillow(alpha)
heif.save(f"{out}/transparent.heic", quality=-1)

blue = Image.new("RGB", (64, 64), (30, 30, 220))
red = Image.new("RGB", (64, 64), (220, 30, 30))
heif = pillow_heif.from_pillow(blue)
heif.add_from_pillow(red)
heif.save(f"{out}/two-images.heic", quality=90, primary_index=1)

for name in ["transparent.heic", "two-images.heic"]:
    check = pillow_heif.open_heif(f"{out}/{name}", convert_hdr_to_8bit=True)
    print(name, len(check), "primary", check.primary_index,
          [(image.mode, image.size, image.info.get("primary")) for image in check])
    image = check[check.primary_index]
    print("  primary pixel (40,32):", Image.frombytes(image.mode, image.size, bytes(image.data), "raw", image.mode, image.stride).getpixel((40, 32)))
