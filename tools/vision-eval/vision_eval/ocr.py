"""PP-OCR detection and recognition on ONNX Runtime, after the app's settings.

The app (pdf-inspector `vision::oar`, PP-OCRv6 Small detector) caps the
detector's longest side at 960 px with score 0.3, box 0.6 and unclip 2.0, and
recognises 48 px high crops. This module reproduces that closely enough to
list a page's words with boxes (the text-loss measure) and to time OCR; it is
not the app's code. The seal reader reuses the same pieces with the seal
detector's own settings (polygons, unclip 0.5).
"""
from __future__ import annotations

import math
from dataclasses import dataclass

import cv2
import numpy as np
import onnxruntime as ort
import pyclipper
from shapely.geometry import Polygon

THREADS = 4  # the app's intra-op threads on a desktop


def session(path: str) -> ort.InferenceSession:
    so = ort.SessionOptions()
    so.intra_op_num_threads = THREADS
    so.log_severity_level = 3
    return ort.InferenceSession(path, so, providers=["CPUExecutionProvider"])


@dataclass
class DetSettings:
    limit: int = 960          # longest side cap (the app); 0 = resize_long below
    resize_long: int = 0      # seal detector: longest side set to this
    thresh: float = 0.3
    box_thresh: float = 0.6
    unclip: float = 2.0
    polygons: bool = False    # seal detector: curved text polygons
    max_candidates: int = 1000


APP_DET = DetSettings()
SEAL_DET = DetSettings(limit=0, resize_long=736, thresh=0.2, box_thresh=0.6, unclip=0.5, polygons=True)


def det_input(bgr: np.ndarray, s: DetSettings) -> tuple[np.ndarray, float, float]:
    h, w = bgr.shape[:2]
    if s.resize_long:
        ratio = s.resize_long / max(h, w)
    else:
        ratio = min(1.0, s.limit / max(h, w))
    nh = max(32, int(round(h * ratio / 32)) * 32)
    nw = max(32, int(round(w * ratio / 32)) * 32)
    x = cv2.resize(bgr, (nw, nh)).astype(np.float32) / 255.0
    x = (x - np.array([0.485, 0.456, 0.406], np.float32)) / np.array([0.229, 0.224, 0.225], np.float32)
    return x.transpose(2, 0, 1)[None].astype(np.float32), nh / h, nw / w


def _unclip(points: np.ndarray, ratio: float) -> list[np.ndarray]:
    poly = Polygon(points)
    if poly.length == 0:
        return []
    distance = poly.area * ratio / poly.length
    offset = pyclipper.PyclipperOffset()
    offset.AddPath([tuple(map(int, p)) for p in points], pyclipper.JT_ROUND, pyclipper.ET_CLOSEDPOLYGON)
    return [np.array(p, np.float32) for p in offset.Execute(distance)]


def _mini_box(contour) -> tuple[np.ndarray, float]:
    rect = cv2.minAreaRect(contour)
    pts = sorted(cv2.boxPoints(rect).tolist(), key=lambda p: p[0])
    left = sorted(pts[:2], key=lambda p: p[1])
    right = sorted(pts[2:], key=lambda p: p[1])
    box = np.array([left[0], right[0], right[1], left[1]], np.float32)
    return box, min(rect[1])


def _score(pred: np.ndarray, points: np.ndarray) -> float:
    h, w = pred.shape
    xmin = int(np.clip(np.floor(points[:, 0].min()), 0, w - 1))
    xmax = int(np.clip(np.ceil(points[:, 0].max()), 0, w - 1))
    ymin = int(np.clip(np.floor(points[:, 1].min()), 0, h - 1))
    ymax = int(np.clip(np.ceil(points[:, 1].max()), 0, h - 1))
    mask = np.zeros((ymax - ymin + 1, xmax - xmin + 1), np.uint8)
    shifted = points.copy()
    shifted[:, 0] -= xmin
    shifted[:, 1] -= ymin
    cv2.fillPoly(mask, [shifted.astype(np.int32)], 1)
    return float(cv2.mean(pred[ymin:ymax + 1, xmin:xmax + 1], mask)[0])


def db_boxes(pred: np.ndarray, s: DetSettings, sy: float, sx: float) -> list[tuple[np.ndarray, float]]:
    """DB post-processing; returns (polygon in source pixels, score)."""
    bitmap = (pred > s.thresh).astype(np.uint8)
    contours, _ = cv2.findContours(bitmap * 255, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
    out = []
    for contour in contours[: s.max_candidates]:
        if s.polygons:
            eps = 0.002 * cv2.arcLength(contour, True)
            pts = cv2.approxPolyDP(contour, eps, True).reshape(-1, 2).astype(np.float32)
            if len(pts) < 4:
                continue
            score = _score(pred, pts)
            if score < s.box_thresh:
                continue
            grown = _unclip(pts, s.unclip)
            if len(grown) != 1:
                continue
            poly = grown[0]
            if _mini_box(poly.reshape(-1, 1, 2))[1] < 5:
                continue
        else:
            box, side = _mini_box(contour)
            if side < 3:
                continue
            score = _score(pred, box)
            if score < s.box_thresh:
                continue
            grown = _unclip(box, s.unclip)
            if len(grown) != 1:
                continue
            poly, side = _mini_box(grown[0].reshape(-1, 1, 2))
            if side < 5:
                continue
        poly = poly.astype(np.float32)
        poly[:, 0] /= sx
        poly[:, 1] /= sy
        out.append((poly, score))
    return out


def detect(sess: ort.InferenceSession, bgr: np.ndarray, s: DetSettings) -> list[tuple[np.ndarray, float]]:
    x, sy, sx = det_input(bgr, s)
    pred = sess.run(None, {sess.get_inputs()[0].name: x})[0][0, 0]
    return db_boxes(pred, s, sy, sx)


def quad_crop(bgr: np.ndarray, quad: np.ndarray) -> np.ndarray:
    """PaddleOCR's get_rotate_crop_image: a perspective crop, tall crops turned."""
    quad = quad.astype(np.float32)
    w = int(max(np.linalg.norm(quad[0] - quad[1]), np.linalg.norm(quad[2] - quad[3])))
    h = int(max(np.linalg.norm(quad[0] - quad[3]), np.linalg.norm(quad[1] - quad[2])))
    w, h = max(w, 1), max(h, 1)
    m = cv2.getPerspectiveTransform(quad, np.float32([[0, 0], [w, 0], [w, h], [0, h]]))
    crop = cv2.warpPerspective(bgr, m, (w, h), borderMode=cv2.BORDER_REPLICATE, flags=cv2.INTER_CUBIC)
    if h / w >= 1.5:
        crop = np.rot90(crop)
    return crop


class Recognizer:
    def __init__(self, model: str, dictionary: str):
        self.sess = session(model)
        chars = [l.rstrip("\r\n") for l in open(dictionary, encoding="utf-8")]
        self.chars = ["<blank>"] + chars + [" "]

    def __call__(self, crop_bgr: np.ndarray) -> tuple[str, float]:
        h, w = crop_bgr.shape[:2]
        width = int(min(3200, max(16, math.ceil(48 * w / max(h, 1)))))
        x = cv2.resize(crop_bgr, (width, 48)).astype(np.float32) / 255.0
        x = ((x - 0.5) / 0.5).transpose(2, 0, 1)[None].astype(np.float32)
        probs = self.sess.run(None, {self.sess.get_inputs()[0].name: x})[0][0]
        idx = probs.argmax(-1)
        text, confs, prev = [], [], -1
        for t, i in enumerate(idx):
            if i != prev and i != 0 and i < len(self.chars):
                text.append(self.chars[i])
                confs.append(float(probs[t, i]))
            prev = i
        return "".join(text), (float(np.mean(confs)) if confs else 0.0)


def read_lines(det: ort.InferenceSession, rec: Recognizer, bgr: np.ndarray, min_conf: float = 0.5) -> list[dict]:
    """The page's text lines: box (axis-aligned bounds), quad, text, confidence."""
    lines = []
    for quad, score in detect(det, bgr, APP_DET):
        text, conf = rec(quad_crop(bgr, quad))
        if not text.strip() or conf < min_conf:
            continue
        xs, ys = quad[:, 0], quad[:, 1]
        lines.append({
            "box": [float(xs.min()), float(ys.min()), float(xs.max()), float(ys.max())],
            "text": text, "conf": round(conf, 4), "det": round(score, 4),
        })
    lines.sort(key=lambda l: (round(l["box"][1] / 10), l["box"][0]))
    return lines


def words(lines: list[dict]) -> list[dict]:
    """Splits each line into words, each with a slice of the line's box in
    proportion to its characters (OCR gives no word boxes)."""
    out = []
    for line in lines:
        text = line["text"]
        x0, y0, x1, y1 = line["box"]
        n = max(len(text), 1)
        pos = 0
        for token in text.split(" "):
            if token:
                a = x0 + (x1 - x0) * pos / n
                b = x0 + (x1 - x0) * (pos + len(token)) / n
                out.append({"text": token, "box": [a, y0, b, y1]})
            pos += len(token) + 1
    return out
