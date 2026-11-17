"""Evaluation classes and how each candidate's output maps onto them.

The class list follows decision 30's page artifacts plus decision 8's visual
PII classes. Mappings are fixed from the public fixtures and synthetic
overlays before any private-set score is read (decision 13: bars and rules
come before numbers).
"""
from __future__ import annotations

# Classes annotated on every page (absent classes are simply not drawn).
CLASSES = [
    "seal", "signature", "handwriting", "qr_code", "barcode", "watermark",
    "header", "footer", "punch_hole", "face", "picture",
]

# Classes whose OCR text a stand-in replaces (decision 9), so a predicted box
# of these classes over real text loses that text. Headers and footers are
# dropped only as exact repeats (decision 10) and pictures are not OCR'd
# text, so neither replaces text here.
REPLACING = ["seal", "signature", "handwriting", "qr_code", "barcode", "watermark", "punch_hole", "face"]

# Decision 13's recall bar applies to these; precision to every class scored
# except pictures, which are not page artifacts (a diagnostic only).
RECALL_BAR_CLASSES = ["seal", "signature"]
NO_BAR = ["picture"]

# PP-DocLayout_plus-L label → class. `image` is a picture region, which on
# documents also catches signatures and seals; the report shows that overlap
# as a diagnostic, never as the class itself.
LAYOUT = {"seal": "seal", "header": "header", "footer": "footer", "image": "picture"}
LAYOUT_THRESHOLD = 0.5  # the model's draw_threshold in inference.yml

# Mirage names come from its language head (no fixed list). A name maps to
# the first class whose words it contains, matched on whole words.
MIRAGE_WORDS = [
    ("signature", ["signature", "signatures", "autograph"]),
    ("seal", ["seal", "stamp", "stamps", "emblem"]),
    ("handwriting", ["handwriting", "handwritten", "scribble", "scrawl", "doodle"]),
    ("qr_code", ["qr", "qrcode", "qr code"]),
    ("barcode", ["barcode", "bar code", "barcodes"]),
    ("watermark", ["watermark"]),
    ("face", ["face", "person", "man", "woman", "boy", "girl", "portrait", "head"]),
    ("punch_hole", ["hole", "holes", "punch"]),
    ("picture", ["photo", "photograph", "picture", "image", "logo"]),
]


# The classes each detector can output at all; the report scores only these.
CAPABLE = {
    "mirage": [c for c in CLASSES],
    "mirage-kw": [c for c in CLASSES],
    "layout": sorted(set(LAYOUT.values())),
    "rxing": ["qr_code", "barcode"],
}


def mirage_class(name: str) -> str | None:
    words = name.lower().replace("-", " ").split()
    joined = " ".join(words)
    for cls, keys in MIRAGE_WORDS:
        for key in keys:
            if (" " in key and key in joined) or key in words:
                return cls
    return None


# rxing barcode formats: 2D symbols are "qr_code" (their stand-in is the QR
# code line, decision 9), linear ones "barcode".
RXING_2D = {"QR_CODE", "MICRO_QR_CODE", "RECTANGULAR_MICRO_QR_CODE", "AZTEC", "DATA_MATRIX",
            "PDF_417", "MAXICODE"}


def rxing_class(fmt: str) -> str:
    return "qr_code" if fmt.upper().replace(" ", "_") in RXING_2D else "barcode"
