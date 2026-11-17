"""Decision 33's follow-up measurements (lawkitt/mdoc#911), fixed in its pre-registration.

    python -m vision_eval.d33 namemap [--sets ...]   # Mirage scored with the D33 name map, with and without keywords
    python -m vision_eval.d33 qr <sources.json>      # rxing, 2D symbols only, on 300 DPI crops around each QR code

Report-only. Prints aggregates only.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

from PIL import Image

from . import classes, pages, score
from .run import out_root
from .worker import exe

# Pre-registered (decision 33): a Mirage object name maps to the first class
# whose words it contains (whole words or phrases, lower case). Every other
# name (text, text block, document, table, ...) maps to nothing.
D33_MAP = [
    ("seal", ["seal", "stamp", "logo", "emblem", "circle", "badge", "crest", "coat of arms", "insignia", "medal"]),
    ("signature", ["signature", "autograph", "scribble", "doodle", "drawing", "sketch", "line", "arrow", "squiggle"]),
    ("handwriting", ["handwriting", "handwritten", "writing", "note", "script"]),
    ("qr_code", ["qr", "qr code", "matrix code", "data matrix"]),
    ("barcode", ["barcode", "bar code"]),
]
SCORED = ["seal", "signature", "handwriting", "qr_code", "barcode"]


def d33_class(name: str) -> str | None:
    words = name.lower().replace("-", " ").split()
    joined = " ".join(words)
    for cls, keys in D33_MAP:
        for key in keys:
            if (" " in key and f" {key} " in f" {joined} ") or key in words:
                return cls
    return None


def remapped(run: dict) -> dict:
    out = {"outputs": {}, "page_ms": run["page_ms"]}
    for pid, raw in run["outputs"].items():
        objs = [dict(o, **{"class": d33_class(o.get("name", ""))}) for o in raw.get("objects", [])]
        out["outputs"][pid] = dict(raw, objects=objs)
    return out


def namemap(out: Path, set_names: list[str]) -> None:
    found = pages.available(out)
    for set_name in set_names:
        sp = pages.load(found[set_name])["pages"]
        print(f"## {set_name}")
        ocr_run = score.load_run(out / "runs" / set_name / "ocr")
        for cand in ("mirage", "mirage-kw"):
            run = score.load_run(out / "runs" / set_name / cand)
            if run is None or score.missing(sp, run):
                print(f"{cand}: incomplete"); continue
            for label, r in (("own names", run), ("D33 map", remapped(run))):
                c = score.detection_scores(sp, r, "mirage")
                cells = []
                for cls in SCORED:
                    k = c.get((cls, "all"))
                    if not k:
                        continue
                    gt, pred = k["tp"] + k["fn"], k["tp"] + k["fp"]
                    rec = f"{100 * k['tp'] / gt:.0f}%" if gt else "–"
                    prec = f"{100 * k['tp'] / pred:.0f}%" if pred else "–"
                    cells.append(f"{cls} R {rec} ({k['tp']}/{gt}) P {prec} ({pred} pred)")
                loss = score.text_loss(sp, ocr_run, r, "mirage") if ocr_run else None
                lost = f"lost {loss['lost_total']} {loss['lost']}" if loss else ""
                print(f"{cand} [{label}]: " + "; ".join(cells) + f"; {lost}")
            ag = score.agnostic_recall(sp, run, "mirage")
            print(f"{cand} [any name]: " + ", ".join(f"{k} {h}/{t}" for k, (h, t) in sorted(ag.items())))


def _crop_box(box, w, h, scale):
    """Annotated box (150 DPI page pixels) grown by half its larger side, at least 32 px, per side."""
    x0, y0, x1, y1 = box
    m = max(0.5 * max(x1 - x0, y1 - y0), 32)
    return [int(max(0, (x0 - m)) * scale), int(max(0, (y0 - m)) * scale),
            int(min(w, (x1 + m)) * scale), int(min(h, (y1 + m)) * scale)]


def _render(page: dict, src: dict | None) -> tuple[Image.Image, str]:
    """The page at 300 DPI: from its source PDF when known, else the stored 150 DPI page ×2 (Lanczos)."""
    if src:
        import pypdfium2 as pdfium
        pdf = pdfium.PdfDocument(src["pdf"])
        return pdf[src["page"] - 1].render(scale=300 / 72).to_pil().convert("RGB"), "source-300dpi"
    im = Image.open(page["path"]).convert("RGB")
    return im.resize((im.width * 2, im.height * 2), Image.LANCZOS), "stored-150dpi-x2"


def qr(out: Path, sources: dict) -> None:
    found = pages.available(out)
    cdir = out / "d33" / "crops"
    cdir.mkdir(parents=True, exist_ok=True)
    rng = random.Random(911)
    jobs = []  # (crop path, kind, set, how, truth text or None)
    for set_name in ("overlays", "ru-filings", "en-recap"):
        if set_name not in found:
            continue
        sp = pages.load(found[set_name])["pages"]
        code_free = [p for p in sp if p["truth"] and not any(a["class"] in ("qr_code", "barcode") for a in p["truth"]["artifacts"])]
        for p in sp:
            for i, a in enumerate(a for a in (p["truth"] or {"artifacts": []})["artifacts"] if a["class"] == "qr_code"):
                img, how = _render(p, sources.get(p["id"]))
                t = p["truth"]
                cb = _crop_box(a["box"], t["width"], t["height"], img.width / t["width"])
                pos = cdir / f"pos-{set_name}-{len(jobs):03d}.png"
                img.crop(cb).save(pos)
                jobs.append((pos, "positive", set_name, how, a.get("text")))
                # A code-free crop of the same size, page and position from Random(911).
                n = rng.choice(code_free)
                nimg, nhow = _render(n, sources.get(n["id"]))
                cw, ch = cb[2] - cb[0], cb[3] - cb[1]
                x = rng.randint(0, max(0, nimg.width - cw)); y = rng.randint(0, max(0, nimg.height - ch))
                neg = cdir / f"neg-{set_name}-{len(jobs):03d}.png"
                nimg.crop((x, y, x + cw, y + ch)).save(neg)
                jobs.append((neg, "negative", set_name, nhow, None))
    env = dict(os.environ, RXING_FORMATS="2d")
    res = subprocess.run([exe("rxing-host")] + [str(j[0]) for j in jobs], capture_output=True, text=True, env=env, check=True)
    lines = [json.loads(l) for l in res.stdout.splitlines() if l.strip()]
    agg = defaultdict(lambda: {"crops": 0, "decoded": 0, "correct": 0, "checked": 0, "formats": defaultdict(int)})
    for (path, kind, set_name, how, truth), r in zip(jobs, lines):
        k = agg[(set_name, kind, how)]
        k["crops"] += 1
        if r.get("codes"):
            k["decoded"] += 1
            for c in r["codes"]:
                k["formats"][c["format"]] += 1
        if truth:
            k["checked"] += 1
            k["correct"] += any(c["text"] == truth for c in r.get("codes", []))
    for (set_name, kind, how), k in sorted(agg.items()):
        content = f"; content correct {k['correct']}/{k['checked']}" if k["checked"] else ("; content unverified" if kind == "positive" else "")
        print(f"{set_name} {kind} [{how}]: {k['decoded']}/{k['crops']} with a decode{content}; formats {dict(k['formats'])}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    n = sub.add_parser("namemap"); n.add_argument("--sets", default="overlays,en-recap,ru-filings"); n.add_argument("--out")
    q = sub.add_parser("qr"); q.add_argument("sources", type=Path, help="page id -> {pdf, page} for pages whose source PDF exists"); q.add_argument("--out")
    args = ap.parse_args()
    out = out_root(args.out)
    if args.cmd == "namemap":
        namemap(out, args.sets.split(","))
    else:
        qr(out, json.loads(args.sources.read_text(encoding="utf-8")))


if __name__ == "__main__":
    main()
