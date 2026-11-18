"""Write comments.pdf: a synthetic two-page agreement with comment
annotations, for the Source rail (originals spec, Comments in Source).

Standard library only, and no compression, so every platform writes the same
bytes. Run from anywhere: python -I tests/fixtures/pdf-source/generate.py

Page 1 is Letter with an offset CropBox [36 36 576 756]. It holds, by
pdf-inspector's list index:
  0  a sticky note, with an icon appearance and a popup
  1  a reply to the note
  -  a review state, Completed, that resolves the note (not an entry)
  2  a highlight with a comment, and a popup
  3  a reply to the highlight
  4  a FreeText box with text drawn by its appearance
  5  an underline, 6 a strikeout, 7 a squiggly with only /RC
  -  a highlight without a comment, a stamp, ink, and a FreeText that is only
     an appearance: part of the page, so they stay drawn (not entries)
Page 2 has /Rotate 90 and text drawn to read upright once turned:
  8  a highlight with a comment
  9  a sticky note

The numbers in the comments are pdf-inspector's list indexes; the tests pin
where each lands.
"""

from pathlib import Path

# Helvetica advance widths (per 1000 em) for ASCII 32..126.
WIDTHS = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
]

SIZE = 12
LEFT = 72


def width(text, size=SIZE):
    return sum(WIDTHS[ord(c) - 32] for c in text) * size / 1000


def esc(text):
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def num(value):
    text = f"{value:.2f}".rstrip("0").rstrip(".")
    return "0" if text == "-0" else text


def arr(values):
    return "[" + " ".join(num(v) for v in values) + "]"


class Pdf:
    def __init__(self):
        self.objects = []

    def reserve(self):
        self.objects.append(None)
        return len(self.objects)

    def put(self, ref, body):
        self.objects[ref - 1] = body.encode("latin-1") if isinstance(body, str) else body

    def add(self, body):
        ref = self.reserve()
        self.put(ref, body)
        return ref

    def stream(self, dictionary, data):
        data = data.encode("latin-1")
        return self.add(
            f"<< {dictionary} /Length {len(data)} >>\nstream\n".encode("latin-1")
            + data
            + b"\nendstream"
        )

    def write(self, path, root):
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for i, body in enumerate(self.objects, start=1):
            offsets.append(len(out))
            out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
        xref = len(out)
        out += f"xref\n0 {len(self.objects) + 1}\n0000000000 65535 f \n".encode()
        for offset in offsets:
            out += f"{offset:010d} 00000 n \n".encode()
        out += (
            f"trailer\n<< /Size {len(self.objects) + 1} /Root {root} 0 R >>\n"
            f"startxref\n{xref}\n%%EOF\n"
        ).encode()
        Path(path).write_bytes(bytes(out))


pdf = Pdf()
font = pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
bold = pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
multiply = pdf.add("<< /Type /ExtGState /BM /Multiply >>")
resources = f"<< /Font << /F1 {font} 0 R /F2 {bold} 0 R >> /ExtGState << /GM {multiply} 0 R >> >>"
pages = pdf.reserve()


def form(bbox, data):
    return pdf.stream(f"/Type /XObject /Subtype /Form /BBox {arr(bbox)} /Resources {resources}", data)


def annot(fields):
    return pdf.add("<< /Type /Annot " + fields + " >>")


# ---- Page 1: the agreement, CropBox offset by 36 pt on every side --------

TITLE = "Supply Agreement"
LINES = [
    "1. The Supplier delivers the goods by 1 March 2027.",
    "2. Liability is capped at the fees paid in the prior year.",
    "3. Either party may end this agreement on 30 days notice.",
    "4. Payment is due within 45 days of each invoice.",
    "5. This agreement is governed by the laws of England.",
    "6. Notices go to the addresses on the first page.",
]
TOP = 660
STEP = 24


def baseline(line):
    return TOP - STEP * line


def words(line, phrase):
    """The box of `phrase` on `line`: x0, y0, x1, y1 (descent to ascent)."""
    text = LINES[line]
    start = text.index(phrase)
    x0 = LEFT + width(text[:start])
    y = baseline(line)
    return (x0, y - 3, x0 + width(phrase), y + 10)


def quad(box):
    x0, y0, x1, y1 = box
    # Upper-left, upper-right, lower-left, lower-right, as Acrobat writes.
    return arr([x0, y1, x1, y1, x0, y0, x1, y0])


def markup(subtype, box, fields, color, drawing):
    x0, y0, x1, y1 = box
    ap = form(box, drawing)
    return annot(
        f"/Subtype /{subtype} /Rect {arr(box)} /QuadPoints {quad(box)} /C {color} /F 4 "
        f"/AP << /N {ap} 0 R >> {fields}"
    )


content = [f"BT /F2 18 Tf {LEFT} 700 Td ({esc(TITLE)}) Tj ET"]
for i, line in enumerate(LINES):
    content.append(f"BT /F1 {SIZE} Tf {LEFT} {baseline(i)} Td ({esc(line)}) Tj ET")
body = [
    "The parties sign this agreement on the date written above.",
    "Each party keeps one signed copy.",
]
for i, line in enumerate(body):
    content.append(f"BT /F1 {SIZE} Tf {LEFT} {420 - STEP * i} Td ({esc(line)}) Tj ET")
page1_content = pdf.stream("", "\n".join(content))

page1 = pdf.reserve()
annots1 = []

# 0: a sticky note with an icon appearance and a popup.
note_rect = (500, 652, 520, 672)
note_ap = form(note_rect, "1 0.85 0.2 rg 0.5 0.4 0 RG 500.5 652.5 19 19 re B 0.5 0.4 0 rg 504 664 12 2 re f 504 659 12 2 re f")
note = pdf.reserve()
note_popup = annot(f"/Subtype /Popup /Rect {arr((520, 600, 700, 680))} /Parent {note} 0 R /Open false")
pdf.put(
    note,
    "<< /Type /Annot /Subtype /Text /Name /Comment /F 28 "
    f"/Rect {arr(note_rect)} /AP << /N {note_ap} 0 R >> /Popup {note_popup} 0 R "
    "/T (Ann Lee) /M (D:20260919100000Z) /C [1 0.85 0.2] "
    "/Contents (Check the delivery date with logistics.) >>",
)
annots1 += [note, note_popup]
# 1: a reply to the note.
annots1.append(annot(
    f"/Subtype /Text /F 28 /Rect {arr(note_rect)} /IRT {note} 0 R "
    "/T (Boris Ivanov) /M (D:20260920091500Z) /Contents (Logistics confirmed 1 March.)"
))
# The note resolved, as a review state.
annots1.append(annot(
    f"/Subtype /Text /F 30 /Rect {arr(note_rect)} /IRT {note} 0 R "
    "/T (Ann Lee) /M (D:20260921080000Z) /State (Completed) /StateModel (Review) "
    "/Contents (Completed set by Ann Lee)"
))

YELLOW = "[1 0.9 0.1]"


def fill(box, color):
    x0, y0, x1, y1 = box
    return f"/GM gs {color} rg {num(x0)} {num(y0)} {num(x1 - x0)} {num(y1 - y0)} re f"


def rule(box, color, at):
    x0, y0, x1, _ = box
    return f"{color} RG 1 w {num(x0)} {num(at)} m {num(x1)} {num(at)} l S"


# 2: a highlight with a comment and a popup.
cap = words(1, "capped at the fees paid")
highlight = markup(
    "Highlight", cap,
    "/T (Ann Lee) /M (D:20260919101000Z) /Contents (Is this cap right? It looks low.)",
    YELLOW, fill(cap, "1 0.9 0.1"),
)
highlight_popup = annot(f"/Subtype /Popup /Rect {arr((400, 560, 580, 640))} /Parent {highlight} 0 R /Open true")
annots1 += [highlight, highlight_popup]
# 3: a reply to the highlight.
annots1.append(annot(
    f"/Subtype /Text /F 28 /Rect {arr((cap[2], cap[1], cap[2] + 20, cap[1] + 20))} /IRT {highlight} 0 R "
    "/T (Boris Ivanov) /M (D:20260919120000Z) /Contents (Yes, it matches the 2025 schedule.)"
))
# 4: a FreeText box with its text drawn by its appearance.
box = (330, 470, 560, 510)
box_ap = form(box, "0.8 0 0 RG 1 w 330.5 470.5 229 39 re S BT 0.8 0 0 rg /F1 12 Tf 340 485 Td (Signed copy to follow.) Tj ET")
annots1.append(annot(
    f"/Subtype /FreeText /F 4 /Rect {arr(box)} /AP << /N {box_ap} 0 R >> /DA (/F1 12 Tf 0.8 0 0 rg) "
    "/T (Carol Diaz) /M (D:20260922140000Z) /Contents (Signed copy to follow.)"
))
# 5, 6, 7: an underline, a strikeout and a squiggly (only /RC).
days = words(2, "30 days notice")
annots1.append(markup(
    "Underline", days, "/T (Carol Diaz) /M (D:20260922141000Z) /Contents (Should this be 60 days?)",
    "[0 0.4 1]", rule(days, "0 0.4 1", days[1] + 1),
))
due = words(3, "45")
annots1.append(markup(
    "StrikeOut", due, "/T (Carol Diaz) /M (D:20260922141500Z) /Contents (Change to 30.)",
    "[0.9 0 0]", rule(due, "0.9 0 0", (due[1] + due[3]) / 2),
))
england = words(4, "England")
annots1.append(markup(
    "Squiggly", england, "/T (Boris Ivanov) /M (D:20260923100000Z) /RC (<body><p>Or <b>Wales</b>?</p></body>)",
    "[0 0.6 0]", rule(england, "0 0.6 0", england[1] + 1),
))
# A highlight without a comment: the reader's own mark, drawn as is.
first = words(5, "first page")
annots1.append(markup("Highlight", first, "/T (Ann Lee)", YELLOW, fill(first, "1 0.9 0.1")))
# A stamp, ink and a FreeText that is only an appearance: part of the page.
stamp = (420, 712, 560, 744)
stamp_ap = form(stamp, "0.8 0.1 0.1 RG 2 w 421 713 138 30 re S BT 0.8 0.1 0.1 rg /F2 16 Tf 440 722 Td (APPROVED) Tj ET")
annots1.append(annot(f"/Subtype /Stamp /F 4 /Rect {arr(stamp)} /AP << /N {stamp_ap} 0 R >> /Name /Approved /Contents (Approved)"))
ink = (80, 330, 240, 380)
ink_ap = form(ink, "0 0 0.5 RG 1.5 w 85 345 m 110 380 130 330 155 360 c 175 380 200 335 235 355 c S")
annots1.append(annot(
    f"/Subtype /Ink /F 4 /Rect {arr(ink)} /InkList [[85 345 155 360 235 355]] /AP << /N {ink_ap} 0 R >> /Contents (Signature)"
))
draft = (72, 470, 300, 510)
draft_ap = form(draft, "0.5 g BT /F2 28 Tf 80 480 Td (DRAFT) Tj ET")
annots1.append(annot(f"/Subtype /FreeText /F 4 /Rect {arr(draft)} /AP << /N {draft_ap} 0 R >> /DA (/F2 28 Tf 0.5 g)"))

refs = " ".join(f"{r} 0 R" for r in annots1)
pdf.put(
    page1,
    f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 612 792] /CropBox [36 36 576 756] "
    f"/Resources {resources} /Contents {page1_content} 0 R /Annots [{refs}] >>",
)

# ---- Page 2: a landscape schedule on a page turned a quarter clockwise ----

SCHEDULE = [
    "Schedule A: delivery sites",
    "Manchester, unit 4, by 1 March 2027",
    "Bristol, unit 9, by 1 April 2027",
    "Glasgow, unit 2, by 1 May 2027",
]
# Text runs up the unturned page so it reads left to right once turned:
# baseline x grows down the turned page, y runs along the line.
COLUMN = 100
START = 72


def column(line):
    return COLUMN + 30 * line


def turned_words(line, phrase):
    text = SCHEDULE[line]
    start = text.index(phrase)
    y0 = START + width(text[:start])
    x = column(line)
    # Ascent is toward smaller x, descent toward larger.
    return (x - 10, y0, x + 3, y0 + width(phrase))


content = []
for i, line in enumerate(SCHEDULE):
    face = "/F2" if i == 0 else "/F1"
    content.append(f"BT {face} {SIZE} Tf 0 1 -1 0 {column(i)} {START} Tm ({esc(line)}) Tj ET")
page2_content = pdf.stream("", "\n".join(content))
page2 = pdf.reserve()
annots2 = []
# 8: a highlight with a comment on the turned page.
bristol = turned_words(2, "Bristol, unit 9")
x0, y0, x1, y1 = bristol
annots2.append(annot(
    f"/Subtype /Highlight /F 4 /Rect {arr(bristol)} "
    f"/QuadPoints {arr([x0, y1, x1, y1, x0, y0, x1, y0])} /C {YELLOW} "
    f"/AP << /N {form(bristol, fill(bristol, '1 0.9 0.1'))} 0 R >> "
    "/T (Ann Lee) /M (D:20260924090000Z) /Contents (Add the Leeds site.)"
))
# 9: a sticky note beside the title.
title_note = (column(0) - 14, 300, column(0) + 6, 320)
annots2.append(annot(
    f"/Subtype /Text /Name /Note /F 28 /Rect {arr(title_note)} "
    f"/AP << /N {form(title_note, '1 0.85 0.2 rg 0.5 0.4 0 RG ' + num(title_note[0] + 0.5) + ' 300.5 19 19 re B')} 0 R >> "
    "/T (Boris Ivanov) /M (D:20260924093000Z) /Contents (Landscape table: check the dates.)"
))
refs = " ".join(f"{r} 0 R" for r in annots2)
pdf.put(
    page2,
    f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 612 792] /Rotate 90 "
    f"/Resources {resources} /Contents {page2_content} 0 R /Annots [{refs}] >>",
)

pdf.put(pages, f"<< /Type /Pages /Kids [{page1} 0 R {page2} 0 R] /Count 2 >>")
catalog = pdf.add(f"<< /Type /Catalog /Pages {pages} 0 R >>")
pdf.write(Path(__file__).with_name("comments.pdf"), catalog)
