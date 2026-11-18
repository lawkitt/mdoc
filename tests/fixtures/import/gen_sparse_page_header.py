"""Generate handmade-sparse-page-header.pdf (native text) and
handmade-scanned-sparse-page-header.pdf (an image-only scan of the same
pages) for #925 M16, adopting lawkitt/pdf-inspector#22 (M15).

Pages 1-3 carry the running header and footer of
gen_scanned_running_header.py. Page 4 is sparse: two body lines, the second
the header's sentence in the body, well below the header's position. That
sentence is body text: it stays on page 4 and is not counted as a repeat.

The native file is deterministic; the scan is rasterized by macOS `sips`
with the offsets of gen_scanned_running_header.py. macOS only for the scan;
no third-party packages. Run: python3 gen_sparse_page_header.py
"""
from pathlib import Path

import gen_scanned_running_header as base


def page_text(page):
    if page < base.PAGES:
        return base.page_text(page)
    return [
        f"BT /F1 12 Tf 72 700 Td (Item {page}.1. {base.BODY[0]}) Tj ET",
        f"BT /F1 12 Tf 72 620 Td ({base.HEADER}) Tj ET",
    ]


def native(pages):
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>",
    ]
    kids = []
    for text in pages:
        content = "\n".join(text)
        objects.append(f"<< /Length {len(content)} >>\nstream\n{content}\nendstream")
        kids.append(len(objects) + 1)
        objects.append(
            f"<< /Type /Page /Parent 2 0 R "
            f"/MediaBox [0 0 {base.WIDTH} {base.HEIGHT}] "
            f"/Resources << /Font << /F1 3 0 R >> >> /Contents {len(objects)} 0 R >>"
        )
    refs = " ".join(f"{kid} 0 R" for kid in kids)
    objects[1] = f"<< /Type /Pages /Kids [{refs}] /Count {len(kids)} >>"
    return base.pdf(objects)


def main():
    pages = [page_text(page) for page in range(1, base.PAGES + 1)]
    here = Path(__file__).parent
    (here / "handmade-sparse-page-header.pdf").write_bytes(native(pages))
    (here / "handmade-scanned-sparse-page-header.pdf").write_bytes(base.scan(pages))


if __name__ == "__main__":
    main()
