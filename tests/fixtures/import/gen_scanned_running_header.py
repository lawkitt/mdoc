"""Generate handmade-scanned-running-header.pdf: an image-only scan of four
pages under one running header and one running footer, each page shifted
by up to 2 pt as a scanner would (#925 M14). The phones recognize it one
page at a time; the header and footer must stay on page 1 only, as in a
whole-document import.

Each page is drawn as a native PDF at 150 dpi, rasterized by macOS `sips`,
turned into a bilevel (1-bit) image and wrapped as an image-only US Letter
page. The offsets
are seeded, so only `sips` can change the bytes. macOS only; no
third-party packages. Run: python3 gen_scanned_running_header.py
"""
import random
import subprocess
import tempfile
import zlib
from pathlib import Path

SCALE = 150 / 72
WIDTH, HEIGHT = 612, 792
PAGES = 4
HEADER = "Annual Report of the Board 2026 Confidential"
FOOTER = "Prepared for the shareholders of Acme Holdings Ltd."
BODY = [
    "The Board reviewed the audited accounts for the financial year.",
    "Revenue increased by eleven percent compared with the prior period.",
    "The Committee approved the remuneration policy without amendment.",
    "Directors confirmed that the going concern basis remains appropriate.",
    "Risk management procedures were reviewed by the internal audit team.",
]


def pdf(objects):
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        if isinstance(body, str):
            body = body.encode()
        out += f"{number} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        out += f"{offset:010} 00000 n \n".encode()
    out += (
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF"
    ).encode()
    return bytes(out)


def page_text(page):
    """The page's text operators in PDF points: header, body, footer."""
    ops = [f"BT /F1 10 Tf 72 750 Td ({HEADER}) Tj ET"]
    for line in range(12):
        text = f"Item {page}.{line + 1}. {BODY[(line + page) % len(BODY)]}"
        ops.append(f"BT /F1 12 Tf 72 {700 - line * 26} Td ({text}) Tj ET")
    ops.append(f"BT /F1 10 Tf 72 40 Td ({FOOTER}) Tj ET")
    return ops


def native_page(page, text):
    shift = random.Random(page)
    dx, dy = shift.uniform(-2, 2), shift.uniform(-2, 2)
    ops = [f"{SCALE:.4f} 0 0 {SCALE:.4f} 0 0 cm 1 0 0 1 {dx:.2f} {dy:.2f} cm"]
    ops += text
    content = "\n".join(ops)
    return pdf([
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        f"<< /Type /Page /Parent 2 0 R "
        f"/MediaBox [0 0 {WIDTH * SCALE:.0f} {HEIGHT * SCALE:.0f}] "
        "/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>",
        f"<< /Length {len(content)} >>\nstream\n{content}\nendstream",
    ])


def bilevel(bmp):
    """A 32-bit BMP as 1-bit rows, white 1, black 0: a bilevel office scan,
    which keeps the fixture small."""
    offset = int.from_bytes(bmp[10:14], "little")
    width = int.from_bytes(bmp[18:22], "little", signed=True)
    height = int.from_bytes(bmp[22:26], "little", signed=True)
    assert int.from_bytes(bmp[28:30], "little") == 32
    rows = range(abs(height)) if height < 0 else range(height - 1, -1, -1)
    out = bytearray()
    for row in rows:
        line = bytearray((width + 7) // 8)
        start = offset + row * width * 4
        for x in range(width):
            blue, green, red, alpha = bmp[start + 4 * x:start + 4 * x + 4]
            grey = (0.299 * red + 0.587 * green + 0.114 * blue) * alpha / 255
            if grey + 255 - alpha >= 128:
                line[x // 8] |= 0x80 >> (x % 8)
        out += line
    return width, abs(height), bytes(out)


def rasterize(page, text, work):
    source = work / f"page-{page}.pdf"
    bmp = work / f"page-{page}.bmp"
    source.write_bytes(native_page(page, text))
    subprocess.run(
        ["sips", "-s", "format", "bmp", str(source), "--out", str(bmp)],
        check=True, capture_output=True,
    )
    return bilevel(bmp.read_bytes())


def scan(pages):
    """An image-only PDF of `pages`, each a list of text operators."""
    with tempfile.TemporaryDirectory() as work:
        scans = [
            rasterize(page, text, Path(work))
            for page, text in enumerate(pages, start=1)
        ]
    objects = ["<< /Type /Catalog /Pages 2 0 R >>", ""]
    kids = []
    for width, height, bits in scans:
        data = zlib.compress(bits, 9)
        image = len(objects) + 1
        objects.append(
            f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} "
            f"/ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /FlateDecode "
            f"/Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"
        )
        content = f"q {WIDTH} 0 0 {HEIGHT} 0 0 cm /Im0 Do Q"
        objects.append(f"<< /Length {len(content)} >>\nstream\n{content}\nendstream")
        kids.append(len(objects) + 1)
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] "
            f"/Resources << /XObject << /Im0 {image} 0 R >> >> "
            f"/Contents {image + 1} 0 R >>"
        )
    refs = " ".join(f"{kid} 0 R" for kid in kids)
    objects[1] = f"<< /Type /Pages /Kids [{refs}] /Count {len(kids)} >>"
    return pdf(objects)


def main():
    pages = [page_text(page) for page in range(1, PAGES + 1)]
    target = Path(__file__).with_name("handmade-scanned-running-header.pdf")
    target.write_bytes(scan(pages))


if __name__ == "__main__":
    main()
