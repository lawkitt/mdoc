"""Rewrites a bilevel TIFF as CCITT Group 3 2D (T4Options bit 0) with
FillOrder 2, keeping its resolution: ImageMagick writes only Group 3 1D.
Needs Pillow built with libtiff. Usage: python3 g3-2d.py IN.tif OUT.tif"""
import sys
from PIL import Image, ImageSequence, TiffImagePlugin

TiffImagePlugin.WRITE_LIBTIFF = True
src, dst = sys.argv[1], sys.argv[2]
pages = [page.convert("1") for page in ImageSequence.Iterator(Image.open(src))]
xres, yres = Image.open(src).info["dpi"]
info = TiffImagePlugin.ImageFileDirectory_v2()
info[292] = 1  # T4Options: 2D coding
info[266] = 2  # FillOrder: least significant bit first
pages[0].save(dst, compression="group3", dpi=(xres, yres), tiffinfo=info,
              save_all=True, append_images=pages[1:])
