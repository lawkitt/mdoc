"""Mirage options and the native host (`host/mirage_host.c`).

libcactus 4.1.1 silently ignores option keys it doesn't know, so a typo in a
pre-registered setting would pass unnoticed (lawkitt/mdoc#918, decision 12).
`check_options` accepts only the keys and values `cactus_describe` parses,
read from the pinned library's option parser, and `check_applied` asserts on
the output that the options took effect.
"""
from __future__ import annotations

import json
import os
import subprocess
import tempfile
from pathlib import Path

import cv2
import numpy as np

from . import ocr
from .fetch import models_dir

# cactus_describe's option keys in libcactus 4.1.1 (the strings next to its
# "invalid options" messages). `form` belongs to extract(), which this
# harness never runs, so it is rejected too.
SECTIONS = ["objects", "text", "relations", "reasoning", "scene", "regions", "events"]
FIELDS = ["bbox", "score", "point", "position", "mask"]
MASK_FORMATS = ["largest", "merged", "polygons", "bitmap"]
NUMBER_0_1 = ["conf", "text_conf", "mask_threshold"]
WHOLE = ["max_objects", "max_text_lines", "max_tokens"]
KNOWN = set(["sections", "fields", "mask_format", "agnostic_nms", "keyword_bias", "mask_tolerance"]
            + NUMBER_0_1 + WHOLE)


def check_options(options: dict) -> dict:
    """Raises ValueError on any key or value libcactus 4.1.1 would not apply."""
    if not isinstance(options, dict):
        raise ValueError("Mirage options must be a JSON object")
    unknown = sorted(set(options) - KNOWN)
    if unknown:
        raise ValueError(f"unknown Mirage option keys {unknown}: libcactus would silently ignore them")
    for key, value in options.items():
        if key in ("sections", "fields"):
            names = SECTIONS if key == "sections" else FIELDS
            if not isinstance(value, list) or not value or any(v not in names for v in value):
                raise ValueError(f"{key} must be a non-empty list of {names}")
        elif key == "mask_format":
            if value not in MASK_FORMATS:
                raise ValueError(f"mask_format must be one of {MASK_FORMATS}")
        elif key == "agnostic_nms":
            if not isinstance(value, bool):
                raise ValueError("agnostic_nms must be true or false")
        elif key in NUMBER_0_1:
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 <= value <= 1:
                raise ValueError(f"{key} must be a number from 0 to 1")
        elif key in WHOLE:
            if isinstance(value, bool) or not isinstance(value, int) or value < 1:
                raise ValueError(f"{key} must be a whole number of at least 1")
        elif key == "keyword_bias":
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 <= value <= 100:
                raise ValueError("keyword_bias must be a number from 0 to 100")
        elif key == "mask_tolerance":
            if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
                raise ValueError("mask_tolerance must be a number of at least 0")
    return options


def check_applied(options: dict, result: dict) -> None:
    """Asserts on one describe result that its options took effect."""
    sections = options.get("sections")
    if sections is not None:
        for name in ("text", "relations", "reasoning", "regions", "events"):
            if name not in sections and result.get(name):
                raise AssertionError(f"sections {sections} ignored: the result has {name}")
    conf = options.get("conf")
    if conf is not None:
        low = [o.get("score") for o in result.get("objects", [])
               if isinstance(o.get("score"), (int, float)) and o["score"] < conf - 0.001]
        if low:
            raise AssertionError(f"conf {conf} ignored: object scores down to {min(low)}")
    cap = options.get("max_objects")
    if cap is not None and len(result.get("objects", [])) > cap:
        raise AssertionError(f"max_objects {cap} ignored")


def write_ppm(path: Path, rgb: np.ndarray) -> None:
    h, w = rgb.shape[:2]
    with open(path, "wb") as f:
        f.write(f"P6\n{w} {h}\n255\n".encode())
        f.write(np.ascontiguousarray(rgb).tobytes())


def page_rgb(path: Path) -> np.ndarray:
    return cv2.cvtColor(cv2.imread(str(path)), cv2.COLOR_BGR2RGB)


def describe(images: list[np.ndarray], options: dict, keywords: list[str] | None, exe: str) -> tuple[list[dict], float, int]:
    """Runs Mirage once over RGB images in one host process.

    The first image runs twice; its first call is a warm-up and is dropped.
    Returns (one host line per image, load ms, peak bytes). Raises on an
    option libcactus 4.1.1 doesn't know, and on output showing an option
    was ignored.
    """
    from .worker import parse_host_line
    check_options(options)
    env = dict(os.environ, CACTUS_TELEMETRY="0", DO_NOT_TRACK="1", HF_HUB_OFFLINE="1")
    env.pop("MIRAGE_KEYWORDS", None)
    if keywords:
        env["MIRAGE_KEYWORDS"] = "\n".join(keywords)
    if not images:
        return [], 0.0, 0
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "options.json").write_text(json.dumps(options), encoding="utf-8")
        ppms = []
        for i, rgb in enumerate(images):
            p = tmp / f"{i:05d}.ppm"
            write_ppm(p, rgb)
            ppms.append(str(p))
        args = [exe, str(models_dir() / "mirage/mirage.cact"), str(tmp / "options.json"),
                str(ocr.THREADS), ppms[0]] + ppms
        proc = subprocess.run(args, env=env, capture_output=True, text=True, encoding="utf-8", check=True)
        parsed = [parse_host_line(l) for l in proc.stdout.splitlines() if l.strip()]
        load = next((float(l.split()[2]) for l in proc.stderr.splitlines() if l.startswith("loaded in")), 0.0)
    lines = [dict(p, nan_values=n) for p, n in parsed]
    peak = lines[-1]["peak_bytes"]
    results = lines[1:-1]
    if len(results) != len(images):
        raise RuntimeError(f"mirage_host returned {len(results)} results for {len(images)} images")
    for r in results:
        if "error" not in r:
            check_applied(options, r["result"])
    return results, load, peak
