"""Boxes, matching and text distances shared by runners and scoring."""
from __future__ import annotations

import unicodedata


def mirage_box(box: list[float], width: int, height: int) -> list[float]:
    """Mirage reports [x0, y0, x1, y1] on 0–1000 per image axis; to pixels."""
    x0, y0, x1, y1 = box
    return [x0 * width / 1000.0, y0 * height / 1000.0, x1 * width / 1000.0, y1 * height / 1000.0]


def area(b: list[float]) -> float:
    return max(0.0, b[2] - b[0]) * max(0.0, b[3] - b[1])


def intersection(a: list[float], b: list[float]) -> float:
    return area([max(a[0], b[0]), max(a[1], b[1]), min(a[2], b[2]), min(a[3], b[3])])


def iou(a: list[float], b: list[float]) -> float:
    i = intersection(a, b)
    u = area(a) + area(b) - i
    return i / u if u > 0 else 0.0


def poly_box(points) -> list[float]:
    xs = [float(p[0]) for p in points]
    ys = [float(p[1]) for p in points]
    return [min(xs), min(ys), max(xs), max(ys)]


def match(gts: list[list[float]], preds: list[list[float]], threshold: float) -> list[tuple[int, int]]:
    """Greedy one-to-one matching by IoU, best pairs first."""
    pairs = sorted(
        ((iou(g, p), gi, pi) for gi, g in enumerate(gts) for pi, p in enumerate(preds)),
        reverse=True,
    )
    used_g, used_p, out = set(), set(), []
    for value, gi, pi in pairs:
        if value < threshold:
            break
        if gi in used_g or pi in used_p:
            continue
        used_g.add(gi)
        used_p.add(pi)
        out.append((gi, pi))
    return out


def normalize_text(s: str) -> str:
    """Case-folded NFKC with quotes and dashes unified and whitespace collapsed."""
    s = unicodedata.normalize("NFKC", s).casefold()
    for a, b in (("«", '"'), ("»", '"'), ("“", '"'), ("”", '"'), ("„", '"'), ("’", "'"),
                 ("–", "-"), ("—", "-"), ("ё", "е")):
        s = s.replace(a, b)
    return " ".join(s.split())


def edit_distance(a: str, b: str) -> int:
    if len(a) < len(b):
        a, b = b, a
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def seal_cer(truth: list[str], predicted: str) -> tuple[int, int]:
    """Character errors of a seal reading against its truth.

    Compares what a stand-in would inline: the reader's joined text against
    the truth segments joined in reading order (ring, then centre lines), both
    normalised, without whitespace or decorative bullets (a ring's spacing and
    separators are not content). Returns (errors, truth characters).
    """
    t = _seal_chars(" ".join(truth))
    p = _seal_chars(predicted)
    return edit_distance(t, p), len(t)


def _seal_chars(s: str) -> str:
    s = normalize_text(s)
    for ch in "•·∙●*★☆":
        s = s.replace(ch, "")
    return "".join(s.split())
