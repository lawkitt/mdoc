"""The seal reader: PP-OCRv4 seal text detection, then PP-OCRv5 recognition.

PaddleOCR's seal pipeline finds curved text inside a seal region and
straightens each polygon before recognition. Here a polygon that fills its
minimum-area rectangle is straight text (a perspective crop); any other is a
ring arc, unwrapped with a polar transform around the seal's centre and read
in both directions (letter tops outward reading clockwise, or inward reading
anticlockwise), keeping the more confident reading. A whole ring starts at
its widest blank run.
"""
from __future__ import annotations

import math

import cv2
import numpy as np

from . import ocr

STRAIGHT_FILL = 0.80  # polygon area / min-area-rect area at or above: straight text
MIN_CONF = 0.5         # segments below this mean character probability are dropped
FULL_RING = 300.0      # an arc spanning more degrees is a whole ring


def _ink_columns(strip: np.ndarray) -> np.ndarray:
    gray = cv2.cvtColor(strip, cv2.COLOR_BGR2GRAY)
    return (gray < 160).sum(axis=0)


def _polar_strip(bgr: np.ndarray, center: tuple[float, float], poly: np.ndarray) -> list[np.ndarray]:
    cx, cy = center
    dx, dy = poly[:, 0] - cx, poly[:, 1] - cy
    r = np.hypot(dx, dy)
    theta = np.mod(np.degrees(np.arctan2(dy, dx)), 360.0)  # image coordinates: clockwise
    # The arc's angular span: the complement of the largest gap between angles.
    order = np.sort(theta)
    gaps = np.diff(np.concatenate([order, order[:1] + 360.0]))
    k = int(np.argmax(gaps))
    start = order[(k + 1) % len(order)]
    span = 360.0 - gaps[k]
    r0, r1 = max(1.0, float(r.min())), float(r.max())
    if r1 - r0 < 4 or span < 5:
        return []
    max_r = r1 * 1.02
    n_angle = max(360, int(2 * math.pi * max_r))
    polar = cv2.warpPolar(bgr, (int(max_r), n_angle), (cx, cy), max_r,
                          cv2.WARP_POLAR_LINEAR | cv2.INTER_CUBIC)
    first = int(start / 360.0 * n_angle)
    count = int(span / 360.0 * n_angle) + 1
    if span > FULL_RING:
        # A whole ring has no polygon gap: start at its widest blank run.
        # Only the middle of the text band: the seal's border circles ink
        # every angle.
        q = (r1 - r0) / 4
        whole = polar[:, int(r0 + q):int(math.ceil(r1 - q))]
        ink = _ink_columns(np.ascontiguousarray(whole.transpose(1, 0, 2))) > 0
        blank = np.concatenate([~ink, ~ink])
        best, best_len, run_start, run = 0, 0, 0, 0
        for i, b in enumerate(blank):
            run = run + 1 if b else 0
            if run == 1:
                run_start = i
            if run > best_len:
                best, best_len = run_start, run
        first = (best + best_len // 2) % n_angle
        count = n_angle
    rows = (np.arange(count) + first) % n_angle
    band = polar[rows][:, int(r0):int(math.ceil(r1))]
    # band[angle, radius] → image[row, col] with columns along the arc and the
    # outer edge on top.
    outward = np.ascontiguousarray(np.flipud(band.transpose(1, 0, 2)))
    return [outward, np.ascontiguousarray(np.rot90(outward, 2))]


def read_seal(det, rec: ocr.Recognizer, page_bgr: np.ndarray, box: list[float]) -> dict:
    """Reads one seal region. Returns segments with text, confidence and kind."""
    h, w = page_bgr.shape[:2]
    x0, y0, x1, y1 = box
    pad = 0.06 * max(x1 - x0, y1 - y0)
    x0, y0 = int(max(0, x0 - pad)), int(max(0, y0 - pad))
    x1, y1 = int(min(w, x1 + pad)), int(min(h, y1 + pad))
    crop = page_bgr[y0:y1, x0:x1]
    if crop.size == 0:
        return {"segments": [], "text": "", "conf": 0.0}
    center = (crop.shape[1] / 2.0, crop.shape[0] / 2.0)
    segments = []
    for poly, score in ocr.detect(det, crop, ocr.SEAL_DET):
        rect = cv2.minAreaRect(poly.reshape(-1, 1, 2))
        rect_area = rect[1][0] * rect[1][1]
        fill = cv2.contourArea(poly.reshape(-1, 1, 2)) / rect_area if rect_area > 0 else 0.0
        if fill >= STRAIGHT_FILL:
            quad, _ = ocr._mini_box(poly.reshape(-1, 1, 2))
            candidates = [ocr.quad_crop(crop, quad)]
            candidates.append(np.ascontiguousarray(np.rot90(candidates[0], 2)))
            kind = "straight"
        else:
            candidates = _polar_strip(crop, center, poly)
            kind = "arc"
        best = ("", 0.0)
        for image in candidates:
            if image.size == 0 or min(image.shape[:2]) < 4:
                continue
            text, conf = rec(image)
            if conf > best[1]:
                best = (text, conf)
        text, conf = best
        if not text.strip() or conf < MIN_CONF:
            continue
        pts = poly.copy()
        pts[:, 0] += x0
        pts[:, 1] += y0
        cy_ = float(pts[:, 1].mean())
        segments.append({
            "text": text.strip(), "conf": round(conf, 4), "kind": kind, "det": round(score, 4),
            "polygon": [[round(float(a), 1), round(float(b), 1)] for a, b in pts], "_y": cy_,
        })
    # Arcs first (outer ring), then straight lines top to bottom.
    segments.sort(key=lambda s: (s["kind"] != "arc", s["_y"]))
    for s in segments:
        s.pop("_y")
    conf = min((s["conf"] for s in segments), default=0.0)
    return {"segments": segments, "text": " ".join(s["text"] for s in segments), "conf": round(conf, 4)}
