"""Grid views for annotating pages by eye (decision 27), and annotation checks.

    python -m vision_eval.annotate grid <page.png> <out.png> [--crop x0,y0,x1,y1]
    python -m vision_eval.annotate check <set-dir>
    python -m vision_eval.annotate sample <set-dir> --share 0.2 --seed <n>

`grid` draws a labelled pixel grid over a page (every 50 px, labels every
100) or over a crop of it (every 10 px, labels every 50), so an annotator can
read box coordinates off the picture. It shows the page only, never a
candidate's output. `check` validates every annotation against the format in
pages.py. `sample` picks the random share of pages for the maintainer's
agreement review.
"""
from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from . import classes, pages


def grid(src: Path, out: Path, crop: list[int] | None) -> None:
    im = Image.open(src).convert("RGB")
    x0, y0 = 0, 0
    if crop:
        x0, y0, x1, y1 = crop
        im = im.crop((x0, y0, x1, y1))
    step, label = (10, 50) if crop else (50, 100)
    scale = 4 if crop and max(im.size) < 400 else (2 if crop else 1)
    im = im.resize((im.width * scale, im.height * scale), Image.LANCZOS)
    d = ImageDraw.Draw(im, "RGBA")
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 14)
    except OSError:
        font = ImageFont.load_default()
    w, h = im.size
    for gx in range((x0 // step) * step, x0 + w // scale + 1, step):
        X = (gx - x0) * scale
        major = gx % label == 0
        d.line([(X, 0), (X, h)], fill=(255, 0, 160, 140 if major else 50), width=1)
        if major:
            d.text((X + 2, 2), str(gx), fill=(200, 0, 120, 255), font=font)
    for gy in range((y0 // step) * step, y0 + h // scale + 1, step):
        Y = (gy - y0) * scale
        major = gy % label == 0
        d.line([(0, Y), (w, Y)], fill=(0, 140, 255, 140 if major else 50), width=1)
        if major:
            d.text((2, Y + 2), str(gy), fill=(0, 90, 200, 255), font=font)
    im.save(out)


def check(set_dir: Path) -> None:
    meta = pages.load(set_dir)
    problems = 0
    for p in meta["pages"]:
        t = p["truth"]
        if t is None:
            print(f"{p['id']}: no annotation")
            problems += 1
            continue
        with Image.open(p["path"]) as im:
            size = im.size
        if (t["width"], t["height"]) != size:
            print(f"{p['id']}: size {t['width']}x{t['height']} but the page is {size[0]}x{size[1]}")
            problems += 1
        for a in t["artifacts"]:
            x0, y0, x1, y1 = a["box"]
            if a["class"] not in classes.CLASSES:
                print(f"{p['id']}: unknown class {a['class']}")
                problems += 1
            if not (0 <= x0 < x1 <= size[0] and 0 <= y0 < y1 <= size[1]):
                print(f"{p['id']}: box {a['box']} outside the page")
                problems += 1
            if a["class"] == "seal" and "segments" not in a:
                print(f"{p['id']}: seal without segments (use [] for an unreadable seal)")
                problems += 1
    print(f"{len(meta['pages'])} pages, {problems} problems")


def sample(set_dir: Path, share: float, seed: int) -> None:
    meta = pages.load(set_dir)
    ids = sorted(p["id"] for p in meta["pages"])
    k = max(1, round(share * len(ids)))
    for pid in sorted(random.Random(seed).sample(ids, k)):
        print(pid)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("grid")
    g.add_argument("src", type=Path)
    g.add_argument("out", type=Path)
    g.add_argument("--crop", default=None)
    c = sub.add_parser("check")
    c.add_argument("set_dir", type=Path)
    s = sub.add_parser("sample")
    s.add_argument("set_dir", type=Path)
    s.add_argument("--share", type=float, default=0.2)
    s.add_argument("--seed", type=int, required=True)
    args = ap.parse_args()
    if args.cmd == "grid":
        grid(args.src, args.out, [int(v) for v in args.crop.split(",")] if args.crop else None)
    elif args.cmd == "check":
        check(args.set_dir)
    else:
        sample(args.set_dir, args.share, args.seed)


if __name__ == "__main__":
    main()
