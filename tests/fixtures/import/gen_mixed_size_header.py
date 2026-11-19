"""Generate handmade-mixed-size-header.pdf (native text) and
handmade-scanned-mixed-size-header.pdf (an image-only scan of the same
pages) for #925 M17, adopting lawkitt/pdf-inspector#23.

Pages 1-3 (US Letter) and page 4 (US Legal) carry the running header and
footer of gen_scanned_running_header.py at the same heights, y=750 and y=40:
42 pt below Letter's top but 258 pt below Legal's, so the pages keep the
header a steady distance from the bottom edge, not the top. Page 5 is a
sparse Letter page: two body lines, the second the header's sentence 96 pt
below the top, the full pages' average distance from the top, which no page
uses. That sentence is body text: it stays on page 5 and is not counted as
a repeat.

The native file is deterministic; the scan is rasterized by macOS `sips`
with the offsets of gen_scanned_running_header.py. macOS only for the scan;
no third-party packages. Run: python3 gen_mixed_size_header.py
"""
from pathlib import Path

import gen_scanned_running_header as base

LEGAL = 1008
PAGES = 5


def page_text(page):
    if page < PAGES:
        return base.page_text(page)
    return [
        f"BT /F1 12 Tf 72 400 Td (Item {page}.1. {base.BODY[0]}) Tj ET",
        f"BT /F1 12 Tf 72 {base.HEIGHT - 96} Td ({base.HEADER}) Tj ET",
    ]


def height(page):
    return LEGAL if page == 4 else base.HEIGHT


def native(pages):
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>",
    ]
    kids = []
    for page, text in enumerate(pages, start=1):
        content = "\n".join(text)
        objects.append(f"<< /Length {len(content)} >>\nstream\n{content}\nendstream")
        kids.append(len(objects) + 1)
        objects.append(
            f"<< /Type /Page /Parent 2 0 R "
            f"/MediaBox [0 0 {base.WIDTH} {height(page)}] "
            f"/Resources << /Font << /F1 3 0 R >> >> /Contents {len(objects)} 0 R >>"
        )
    refs = " ".join(f"{kid} 0 R" for kid in kids)
    objects[1] = f"<< /Type /Pages /Kids [{refs}] /Count {len(kids)} >>"
    return base.pdf(objects)


def main():
    pages = [page_text(page) for page in range(1, PAGES + 1)]
    heights = [height(page) for page in range(1, PAGES + 1)]
    here = Path(__file__).parent
    (here / "handmade-mixed-size-header.pdf").write_bytes(native(pages))
    (here / "handmade-scanned-mixed-size-header.pdf").write_bytes(
        base.scan(pages, heights)
    )


if __name__ == "__main__":
    main()
