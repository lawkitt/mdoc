"""Rewrite a Group 4 TIFF in 800-row strips, with Pillow built with libtiff.

ImageMagick writes Group 4 only as one strip per page, so
fax-g4-strips.tif comes from fax-g4.tif through this script:
`python3 g4-strips.py fax-g4.tif fax-g4-strips.tif`.
"""

import sys

from PIL import Image, TiffImagePlugin

ROWS_PER_STRIP = 800


def main(source, target):
    with Image.open(source) as tiff:
        pages = []
        for index in range(tiff.n_frames):
            tiff.seek(index)
            pages.append(tiff.copy())
    TiffImagePlugin.WRITE_LIBTIFF = True
    info = TiffImagePlugin.ImageFileDirectory_v2()
    info[278] = ROWS_PER_STRIP  # RowsPerStrip
    info[262] = 0  # PhotometricInterpretation: min-is-white
    pages[0].save(
        target,
        save_all=True,
        append_images=pages[1:],
        compression="group4",
        dpi=(300, 300),
        tiffinfo=info,
    )


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
