"""Runs one candidate over one page set in a process of its own.

    python -m vision_eval.worker <candidate> <set-dir> <out-dir> [--regions <dir>|gt]

Writes `<out-dir>/<page-id>.json` (the raw output: boxes in pixels, classes
or names, scores, text) and `<out-dir>/run.json` (per-page milliseconds
without model loading, load time, this process's peak memory and its peak
before any model loaded). `run.py` starts it; it is not meant to be run by
hand except to debug one candidate.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import cv2
import numpy as np
import onnxruntime as ort

from . import classes, geometry, memory, ocr, pages
from .fetch import host_platform, models_dir

CANDIDATES = ["ocr", "mirage", "layout", "seal", "seal-mobile", "rxing"]
SEAL_GATE = 0.90  # inline a seal's text only at this reader confidence or above (fixed before any scores)
MIRAGE_OPTIONS = {}  # the engine's defaults: sections objects and text


def bin_dir() -> Path:
    return Path(os.environ.get("MDOC_VISION_BIN", "~/.cache/mdoc-vision-eval/bin")).expanduser()


def exe(name: str) -> str:
    return str(bin_dir() / (name + (".exe" if sys.platform == "win32" else "")))


def timed(pages_: list[dict], fn) -> tuple[dict, dict]:
    """Runs fn(page) per page; the first page runs twice and its first,
    warm-up call is not counted."""
    outputs, ms = {}, {}
    for i, page in enumerate(pages_):
        if i == 0:
            fn(page)
        t = time.perf_counter()
        outputs[page["id"]] = fn(page)
        ms[page["id"]] = round((time.perf_counter() - t) * 1000, 2)
    return outputs, ms


def run_ocr(set_pages: list[dict]) -> tuple[dict, dict, float]:
    m = models_dir() / "ocr"
    t = time.perf_counter()
    det = ocr.session(str(m / "pp-ocrv6_small_det.onnx"))
    recs = {
        "en": ocr.Recognizer(str(m / "pp-ocrv6_small_rec.onnx"), str(m / "ppocrv6_dict.txt")),
        "ru": ocr.Recognizer(str(m / "cyrillic_pp-ocrv5_mobile_rec.onnx"), str(m / "ppocrv5_cyrillic_dict.txt")),
    }
    load = (time.perf_counter() - t) * 1000

    def one(page):
        lines = ocr.read_lines(det, recs[page["lang"]], cv2.imread(str(page["path"])))
        return {"lines": lines, "words": ocr.words(lines)}

    out, ms = timed(set_pages, one)
    return out, ms, load


def run_layout(set_pages: list[dict]) -> tuple[dict, dict, float]:
    from .pins import LAYOUT_LABELS
    t = time.perf_counter()
    sess = ocr.session(str(models_dir() / "paddle/PP-DocLayout_plus-L_onnx/inference.onnx"))
    load = (time.perf_counter() - t) * 1000

    def one(page):
        rgb = cv2.cvtColor(cv2.imread(str(page["path"])), cv2.COLOR_BGR2RGB)
        h, w = rgb.shape[:2]
        x = cv2.resize(rgb, (800, 800), interpolation=cv2.INTER_CUBIC).astype(np.float32) / 255.0
        feed = {"image": x.transpose(2, 0, 1)[None],
                "im_shape": np.array([[800, 800]], np.float32),
                "scale_factor": np.array([[800 / h, 800 / w]], np.float32)}
        boxes = sess.run(None, feed)[0]
        items = []
        for cls_id, score, x0, y0, x1, y1 in boxes.tolist():
            if score < 0.3:  # raw output keeps more than the scoring cut (0.5)
                continue
            label = LAYOUT_LABELS[int(cls_id)]
            items.append({"label": label, "class": classes.LAYOUT.get(label), "score": round(score, 4),
                          "box": [round(v, 1) for v in (max(0, x0), max(0, y0), min(w, x1), min(h, y1))]})
        return {"items": items}

    out, ms = timed(set_pages, one)
    return out, ms, load


def seal_regions(set_pages: list[dict], regions: str) -> dict[str, list[list[float]]]:
    out = {}
    for page in set_pages:
        if regions == "gt":
            truth = page["truth"] or {"artifacts": []}
            out[page["id"]] = [a["box"] for a in truth["artifacts"] if a["class"] == "seal"]
        else:
            raw = json.loads((Path(regions) / f"{page['id']}.json").read_text(encoding="utf-8"))
            out[page["id"]] = [i["box"] for i in raw["items"]
                               if i["class"] == "seal" and i["score"] >= classes.LAYOUT_THRESHOLD]
    return out


def run_seal(set_pages: list[dict], variant: str, regions: str) -> tuple[dict, dict, float]:
    from . import seal
    m = models_dir()
    det_name = "PP-OCRv4_mobile_seal_det" if variant == "seal-mobile" else "PP-OCRv4_server_seal_det"
    t = time.perf_counter()
    det = ocr.session(str(m / "paddle" / f"{det_name}_onnx/inference.onnx"))
    rec = ocr.Recognizer(str(m / "ocr/cyrillic_pp-ocrv5_mobile_rec.onnx"), str(m / "ocr/ppocrv5_cyrillic_dict.txt"))
    load = (time.perf_counter() - t) * 1000
    boxes = seal_regions(set_pages, regions)

    def one(page):
        if not boxes[page["id"]]:
            return {"seals": []}
        img = cv2.imread(str(page["path"]))
        seals = []
        for box in boxes[page["id"]]:
            r = seal.read_seal(det, rec, img, box)
            r["box"] = [round(v, 1) for v in box]
            r["inline"] = r["conf"] >= SEAL_GATE and bool(r["text"])
            seals.append(r)
        return {"seals": seals}

    out, ms = timed(set_pages, one)
    return out, ms, load


def write_ppm(path: Path, page: dict) -> tuple[int, int]:
    bgr = cv2.imread(str(page["path"]))
    rgb = cv2.cvtColor(bgr, cv2.COLOR_BGR2RGB)
    h, w = rgb.shape[:2]
    with open(path, "wb") as f:
        f.write(f"P6\n{w} {h}\n255\n".encode())
        f.write(np.ascontiguousarray(rgb).tobytes())
    return w, h


def run_mirage(set_pages: list[dict]) -> tuple[dict, dict, float, int]:
    env = dict(os.environ, CACTUS_TELEMETRY="0", DO_NOT_TRACK="1", HF_HUB_OFFLINE="1")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "options.json").write_text(json.dumps(MIRAGE_OPTIONS), encoding="utf-8")
        ppms = []
        for i, page in enumerate(set_pages):
            p = tmp / f"{i:05d}.ppm"
            write_ppm(p, page)
            ppms.append(p)
        # The first image twice: its first call is the warm-up.
        args = [exe("mirage_host"), str(models_dir() / "mirage/mirage.cact"), str(tmp / "options.json"),
                str(ocr.THREADS)] + [str(ppms[0])] + [str(p) for p in ppms]
        t = time.perf_counter()
        proc = subprocess.run(args, env=env, capture_output=True, text=True, encoding="utf-8", check=True)
        lines = [json.loads(l) for l in proc.stdout.splitlines() if l.strip()]
        load = next((float(l.split()[2]) for l in proc.stderr.splitlines() if l.startswith("loaded in")), 0.0)
    peak = lines[-1]["peak_bytes"]
    results = lines[1:-1]
    out, ms = {}, {}
    for page, r in zip(set_pages, results):
        if "error" in r:
            out[page["id"]] = {"error": r["error"]}
            continue
        w, h = r["width"], r["height"]
        res = r["result"]
        objects = [{"name": o.get("name", ""), "class": classes.mirage_class(o.get("name", "")),
                    "score": o.get("score"), "bbox_1000": o.get("bbox"),
                    "box": [round(v, 1) for v in geometry.mirage_box(o["bbox"], w, h)]}
                   for o in res.get("objects", []) if o.get("bbox")]
        text = [{"string": t.get("string", ""), "score": t.get("score"), "bbox_1000": t.get("bbox"),
                 "box": [round(v, 1) for v in geometry.mirage_box(t["bbox"], w, h)]}
                for t in res.get("text", []) if t.get("bbox")]
        out[page["id"]] = {"objects": objects, "text": text, "scene": res.get("scene"),
                           "regions": res.get("regions", [])}
        ms[page["id"]] = r["ms"]
    return out, ms, load, peak


def _extend_linear(gray: np.ndarray, box: list[float]) -> list[float]:
    """rxing gives a linear barcode's scan line; grow it up and down while the
    rows still look like the scan row's bars."""
    x0, x1 = int(max(0, box[0])), int(min(gray.shape[1], box[2] + 1))
    y = int(round((box[1] + box[3]) / 2))
    if x1 - x0 < 8:
        return box
    ref = gray[y, x0:x1].astype(np.float32)
    ref -= ref.mean()
    def same(row):
        r = gray[row, x0:x1].astype(np.float32)
        r -= r.mean()
        d = float(np.linalg.norm(ref) * np.linalg.norm(r))
        return d > 0 and float(ref @ r) / d > 0.8
    top = y
    while top > 0 and same(top - 1):
        top -= 1
    bottom = y
    while bottom < gray.shape[0] - 1 and same(bottom + 1):
        bottom += 1
    return [box[0], float(top), box[2], float(bottom + 1)]


def run_rxing(set_pages: list[dict]) -> tuple[dict, dict, float, int]:
    paths = [str(p["path"]) for p in set_pages]
    proc = subprocess.run([exe("rxing-host")] + paths[:1] + paths, capture_output=True, text=True,
                          encoding="utf-8", check=True)
    lines = [json.loads(l) for l in proc.stdout.splitlines() if l.strip()]
    out, ms = {}, {}
    for page, r in zip(set_pages, lines[1:]):
        if "error" in r:
            out[page["id"]] = {"error": r["error"]}
            continue
        codes = []
        gray = None
        for c in r["codes"]:
            box = geometry.poly_box(c["points"]) if c["points"] else [0, 0, 0, 0]
            cls = classes.rxing_class(c["format"])
            if cls == "barcode":
                if gray is None:
                    gray = cv2.imread(str(page["path"]), cv2.IMREAD_GRAYSCALE)
                box = _extend_linear(gray, box)
            codes.append({"format": c["format"], "class": cls, "text": c["text"],
                          "box": [round(v, 1) for v in box], "points": c["points"]})
        out[page["id"]] = {"codes": codes}
        ms[page["id"]] = round(r["ms"], 2)
    return out, ms, 0.0, 0


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("candidate", choices=CANDIDATES)
    ap.add_argument("set_dir", type=Path)
    ap.add_argument("out_dir", type=Path)
    ap.add_argument("--regions", default=None, help="seal regions: a layout run folder or 'gt'")
    args = ap.parse_args()
    meta = pages.load(args.set_dir)
    idle_peak = memory.peak_bytes()
    child_peak = 0
    if args.candidate == "ocr":
        out, ms, load = run_ocr(meta["pages"])
    elif args.candidate == "layout":
        out, ms, load = run_layout(meta["pages"])
    elif args.candidate in ("seal", "seal-mobile"):
        if not args.regions:
            raise SystemExit("seal needs --regions <layout run folder> or --regions gt")
        out, ms, load = run_seal(meta["pages"], args.candidate, args.regions)
    elif args.candidate == "mirage":
        out, ms, load, child_peak = run_mirage(meta["pages"])
    else:
        out, ms, load, child_peak = run_rxing(meta["pages"])
    args.out_dir.mkdir(parents=True, exist_ok=True)
    for pid, data in out.items():
        (args.out_dir / f"{pid}.json").write_text(
            json.dumps(data, ensure_ascii=False, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    native = args.candidate in ("mirage", "rxing")
    run = {
        "candidate": args.candidate, "set": meta["name"], "pages": len(meta["pages"]),
        "regions": args.regions if args.regions in (None, "gt") else "layout",
        "page_ms": ms, "load_ms": round(load, 1),
        # Native hosts report their own peak; Python runners report this
        # process's peak and the peak before any model loaded.
        "peak_bytes": child_peak if native else memory.peak_bytes(),
        "idle_peak_bytes": 0 if native else idle_peak,
        "platform": host_platform(), "machine": platform.processor() or platform.machine(),
        "onnxruntime": ort.__version__, "threads": ocr.THREADS,
    }
    (args.out_dir / "run.json").write_text(json.dumps(run, indent=1, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
