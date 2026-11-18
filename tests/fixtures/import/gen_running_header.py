"""Generate handmade-running-header.pdf: four pages under one running header,
the first also drawing a passage in white on the bare page (#925 M4, M5).
Deterministic; standard library only. Run: python3 gen_running_header.py
"""
from pathlib import Path

PAGES = 4
HEADER = "Case No. 2-123/2026 Confidential"


def page_content(page):
    lines = [f"BT /F1 10 Tf 72 760 Td ({HEADER}) Tj ET"]
    for line in range(12):
        y = 700 - 24 * line
        lines.append(
            f"BT /F1 12 Tf 72 {y} Td "
            f"(Clause {page}.{line}: the parties agree to the terms below.) Tj ET"
        )
    if page == 1:
        lines.append("1 g BT /F1 12 Tf 72 300 Td (SECRET-WHITE) Tj ET 0 g")
    return "\n".join(lines)


def stream(content):
    return f"<< /Length {len(content)} >>\nstream\n{content}\nendstream"


def main():
    kids = " ".join(f"{4 + 2 * page} 0 R" for page in range(PAGES))
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        f"<< /Type /Pages /Kids [{kids}] /Count {PAGES} >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    for page in range(1, PAGES + 1):
        objects.append(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
            f"/Resources << /Font << /F1 3 0 R >> >> /Contents {5 + 2 * (page - 1)} 0 R >>"
        )
        objects.append(stream(page_content(page)))
    pdf = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(pdf))
        pdf += f"{number} 0 obj\n{body}\nendobj\n".encode()
    xref = len(pdf)
    pdf += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        pdf += f"{offset:010} 00000 n \n".encode()
    pdf += (
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF"
    ).encode()
    Path(__file__).with_name("handmade-running-header.pdf").write_bytes(pdf)


if __name__ == "__main__":
    main()
