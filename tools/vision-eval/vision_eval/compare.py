"""Raw-output digests and a tolerant diff between two machines' runs.

    python -m vision_eval.compare digests [--out DIR]
    python -m vision_eval.compare diff <out-a> <out-b> [--sets ...]

`digests` prints one SHA-256 per set and candidate over the canonical raw
outputs: boxes rounded to whole pixels, scores to two decimals, text as is,
timing left out. Equal digests mean equal outputs at that rounding. `diff`
matches each page's items between two runs (same class or label, same text,
box IoU ≥ 0.9) over the union of both runs' pages and folders, and counts
what differs, including pages or runs present on one side only, since float noise across CPUs moves
a box by a pixel or a score by a hundredth.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from . import geometry
from .run import out_root


def _items(candidate: str, raw: dict) -> list[dict]:
    """Comparable items: key (class/label/name), text, box, score."""
    if "error" in raw:
        return [{"key": "error", "text": raw["error"], "box": [0, 0, 0, 0], "score": 0}]
    if candidate == "ocr":
        return [{"key": "line", "text": l["text"], "box": l["box"], "score": l["conf"]} for l in raw["lines"]]
    if candidate == "layout":
        return [{"key": i["label"], "text": "", "box": i["box"], "score": i["score"]} for i in raw["items"]]
    if candidate == "mirage":
        return ([{"key": "object:" + o["name"], "text": "", "box": o["box"], "score": o["score"] or 0} for o in raw["objects"]]
                + [{"key": "text", "text": t["string"], "box": t["box"], "score": t["score"] or 0} for t in raw["text"]])
    if candidate == "rxing":
        return [{"key": c["format"], "text": c["text"], "box": c["box"], "score": 1} for c in raw["codes"]]
    if candidate.startswith("seal"):
        return [{"key": "seal:" + ("inline" if s["inline"] else "withheld"), "text": s["text"], "box": s["box"],
                 "score": s["conf"]} for s in raw["seals"]]
    return []


def _candidate(folder: Path) -> str:
    return json.loads((folder / "run.json").read_text(encoding="utf-8"))["candidate"]


def canonical(candidate: str, raw: dict) -> list:
    rows = [[i["key"], i["text"], [round(v) for v in i["box"]], round(float(i["score"]), 2)]
            for i in _items(candidate, raw)]
    return sorted(rows, key=lambda r: json.dumps(r, ensure_ascii=False))


def runs(out: Path, sets: list[str] | None):
    root = out / "runs"
    for set_dir in sorted(p for p in root.iterdir() if p.is_dir()):
        if sets and set_dir.name not in sets:
            continue
        for run_dir in sorted(p for p in set_dir.iterdir() if (p / "run.json").exists()):
            yield set_dir.name, run_dir


def digests(out: Path, sets: list[str] | None) -> None:
    for set_name, run_dir in runs(out, sets):
        cand = _candidate(run_dir)
        h = hashlib.sha256()
        n = 0
        for f in sorted(run_dir.glob("*.json")):
            if f.name == "run.json":
                continue
            raw = json.loads(f.read_text(encoding="utf-8"))
            h.update(f.stem.encode() + b"\n" + json.dumps(canonical(cand, raw), ensure_ascii=False).encode() + b"\n")
            n += 1
        print(f"{set_name}/{run_dir.name} {h.hexdigest()} {n} pages")


def diff(a: Path, b: Path, sets: list[str] | None) -> None:
    in_a = {(s, r.name) for s, r in runs(a, sets)}
    for set_name, run_b in runs(b, sets):
        if (set_name, run_b.name) not in in_a:
            print(f"{set_name}/{run_b.name}: missing in {a}")
    for set_name, run_a in runs(a, sets):
        run_b = b / "runs" / set_name / run_a.name
        if not (run_b / "run.json").exists():
            print(f"{set_name}/{run_a.name}: missing in {b}")
            continue
        cand = _candidate(run_a)
        same = only_a = only_b = text_diff = 0
        dscore = 0.0
        names = {f.name for f in run_a.glob("*.json")} | {f.name for f in run_b.glob("*.json")}
        names.discard("run.json")
        pages_only_a = pages_only_b = 0
        for name in sorted(names):
            fa, fb = run_a / name, run_b / name
            pages_only_a += not fb.exists()
            pages_only_b += not fa.exists()
            ia = _items(cand, json.loads(fa.read_text(encoding="utf-8"))) if fa.exists() else []
            ib = _items(cand, json.loads(fb.read_text(encoding="utf-8"))) if fb.exists() else []
            used = set()
            for x in ia:
                best, bi = 0.0, None
                for j, y in enumerate(ib):
                    if j in used or y["key"] != x["key"]:
                        continue
                    v = geometry.iou(x["box"], y["box"])
                    if v > best:
                        best, bi = v, j
                if bi is None or best < 0.9:
                    only_a += 1
                    continue
                used.add(bi)
                same += 1
                text_diff += x["text"] != ib[bi]["text"]
                dscore = max(dscore, abs(float(x["score"]) - float(ib[bi]["score"])))
            only_b += len(ib) - len(used)
        if pages_only_a or pages_only_b:
            print(f"{set_name}/{run_a.name}: {pages_only_a} pages only in A, {pages_only_b} pages only in B")
        print(f"{set_name}/{run_a.name}: {same} matched, {only_a} only in A, {only_b} only in B, "
              f"{text_diff} matched with different text, max score Δ {dscore:.3f}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("digests")
    d.add_argument("--out", default=None)
    d.add_argument("--sets", default=None)
    f = sub.add_parser("diff")
    f.add_argument("a", type=Path)
    f.add_argument("b", type=Path)
    f.add_argument("--sets", default=None)
    args = ap.parse_args()
    sets = args.sets.split(",") if args.sets else None
    if args.cmd == "digests":
        digests(out_root(args.out), sets)
    else:
        diff(args.a.expanduser().resolve(), args.b.expanduser().resolve(), sets)


if __name__ == "__main__":
    main()
