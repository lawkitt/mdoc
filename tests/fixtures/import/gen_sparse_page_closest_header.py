"""Generate handmade-sparse-page-closest-header.pdf (native text) for #925
M18, adopting lawkitt/pdf-inspector#24.

Pages 1-3 carry the running header and footer of
gen_scanned_running_header.py. Page 4 is sparse: the header where the full
pages have it (y=750), and the header's sentence again as a body line 24 pt
lower, inside the 5% position tolerance (35.5 pt here). Only the match
closest to the header's place is the header: it goes, and the sentence stays
on page 4 and is not counted as a repeat.

Native text only: an image-only scan needs macOS `sips` (see
gen_sparse_page_header.py). The phones' page-at-a-time OCR reads this file
too, as resume.rs's test does. Deterministic; standard library only.
Run: python3 gen_sparse_page_closest_header.py
"""
from pathlib import Path

import gen_scanned_running_header as base
import gen_sparse_page_header as sparse


def page_text(page):
    if page < base.PAGES:
        return base.page_text(page)
    return [
        f"BT /F1 10 Tf 72 750 Td ({base.HEADER}) Tj ET",
        f"BT /F1 12 Tf 72 726 Td ({base.HEADER}) Tj ET",
    ]


def main():
    pages = [page_text(page) for page in range(1, base.PAGES + 1)]
    target = Path(__file__).with_name("handmade-sparse-page-closest-header.pdf")
    target.write_bytes(sparse.native(pages))


if __name__ == "__main__":
    main()
