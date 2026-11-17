"""Usage tuning of the released Mirage for seals and stamps (lawkitt/mdoc#918).

    python -m vision_eval.tune disjoint [--tune tune-seal] [--against en-recap,ru-filings,overlays]
    python -m vision_eval.tune run [--sets tune-seal] [--variants all|<id>,...]
    python -m vision_eval.tune gate [--sets tune-seal]
    python -m vision_eval.tune sweep [--sets tune-seal] [--write setting.json]
    python -m vision_eval.tune final <setting.json> [--sets en-recap,ru-filings,overlays]

Decision 15's run, in order: the disjointness check of the tuning split
against #911's sets, the variant runs, the any-name ceiling gate (decisions
10, 17 and 19), the sweep with decision 11's selection rule, the check that
the chosen setting already meets every final bar on the tuning split
(decision 21), and one final run of the pre-registered setting (decisions
12, 18 and 20).

Every variant calls Mirage with `sections: ["objects"]` (decision 5). Run
levers (decision 7, in): `conf`, `agnostic_nms`, English keywords, the bottom
half at 2× (two tiles) and re-describing at most 2 candidate crops per page.
Score levers, applied to the raw output without rerunning: the decision 33
name map, the crop names, and a rule layer (aspect, size, mask roundness,
OCR text inside, ink colour) whose thresholds come from the tuning split.

`run`, `gate` and `sweep` refuse a set whose `set.json` role isn't `tune`;
`final` refuses one whose role is, and runs only once per output folder.
Only aggregates are printed.
Details:
- Sets: `tune-seal` pages carry `tier: real` (RECAP) or `tier: synthetic`
  (Hyperbots). The gate and the sweep count real pages and report the
  synthetic tier beside them.
- Options: `mirage.check_options` rejects keys and values libcactus 4.1.1's
  parser doesn't know; `mirage.check_applied` asserts on every result that
  the options took effect (no text when only objects were asked for, no
  score under `conf`).
- Tiles: the bottom half as two tiles 60% of the page wide, so Mirage's
  512 px canvas sees each at about twice the page's scale.
- Crops: at most 2 seal-sized boxes per page (`crop_candidates`), padded
  25% or 50% of their larger side.
- Pools: boxes named a seal by the decision 33 map; those plus boxes whose
  crop has such a name; or every box. Rules (each may be off): aspect, long
  side as a share of the page's, mask roundness (4πA/P²), the share of the
  box covered by OCR words, the share of its ink that is coloured. Kept boxes
  go through non-maximum suppression at IoU 0.5.
- Time: a page's time is the sum of its calls without warm-ups; peak is the
  largest host process's.
- `sweep --write` saves the setting for pre-registration and prints its
  SHA-256; `final` checks every field of it against what it would run, then
  writes `tune-final.json` in the output folder and refuses to run again.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import shutil
import statistics
import time
from datetime import datetime, timezone
from pathlib import Path

import cv2
import numpy as np

from . import d33, geometry, mirage, pages, score
from .run import out_root
from .worker import MIRAGE_KEYWORDS, exe, mirage_objects

TUNE_SETS = ["tune-seal"]
FINAL_SETS = ["en-recap", "ru-filings", "overlays"]
IOU = 0.5
GATE = 0.67           # decision 19 (was 0.70, decision 17; lowered after the gate result)
GATE_MEDIAN_MS = 190  # decision 10: +30% of the app's OCR (0.63 s per page on the M4)
# Decisions 19 and 20: every report says the bars moved after the gate result.
BARS_NOTE = ("The gate threshold and the final recall bar are 67%, lowered after seeing the gate result "
             "(decisions 19 and 20); the earlier bars were 90% (#911 decision 13) and 70% (decisions 17 and 18).")
GUARD = "reduced-recall seal guard"
BARS = {"recall": 0.67, "precision": 0.85, "lost_words": 0, "time": 0.30, "memory_mb": 300}  # decisions 18 and 20

# Run levers (decision 7). Every variant runs objects only (decision 5).
CONFS = [0.05, 0.1, 0.2, 0.3]
TILES = [None, "bottom2x"]
# No crop pass, or (crops per page, padding of each side as a share of the
# box's larger side): at most 2 crops (decision 7), 25–50% padding.
CROPS = [None, (1, 0.25), (1, 0.5), (2, 0.25), (2, 0.5)]
MAX_CROPS = 2
# Crop candidates, fixed before any tuning-split run: seal-sized page or tile
# boxes (long side 2–30% of the page's, aspect at most 3). Boxes not already
# named a seal by the decision 33 map come first, then those least covered by
# the page's OCR words (in steps of 0.1: text blocks are mostly words), then
# the highest score.
CROP_SIDE = (0.02, 0.30)
CROP_ASPECT = 3.0


def variant_id(v: dict) -> str:
    parts = [f"c{int(round(v['conf'] * 100)):02d}"]
    if v["agnostic_nms"]:
        parts.append("nms")
    if v["keywords"]:
        parts.append("kw")
    if v["tile"]:
        parts.append("tile")
    if v["crops"]:
        parts.append(f"crop{v['crops']}p{int(round(v['crop_pad'] * 100))}")
    return "-".join(parts)


def variants() -> list[dict]:
    out = []
    for conf, nms, kw, tile, crop in itertools.product(CONFS, [False, True], [False, True], TILES, CROPS):
        out.append({"conf": conf, "agnostic_nms": nms, "keywords": kw, "tile": tile,
                    "crops": crop[0] if crop else 0, "crop_pad": crop[1] if crop else None})
    return out


def options(v: dict) -> dict:
    opts = {"sections": ["objects"], "conf": v["conf"]}
    if v["agnostic_nms"]:
        opts["agnostic_nms"] = True
    return mirage.check_options(opts)


def tiles(w: int, h: int, kind: str | None) -> list[tuple[int, int, int, int]]:
    """The bottom half as two tiles, each 60% of the page wide: on a portrait
    page each tile's long side is about half the page's, so Mirage's 512 px
    canvas shows it at about twice the page's scale."""
    if kind is None:
        return []
    if kind != "bottom2x":
        raise ValueError(f"unknown tiling {kind}")
    tw, y0 = int(round(0.6 * w)), h // 2
    return [(0, y0, tw, h), (w - tw, y0, w, h)]


def crop_box(box: list[float], pad: float, w: int, h: int) -> tuple[int, int, int, int]:
    m = pad * max(box[2] - box[0], box[3] - box[1])
    return (int(max(0, box[0] - m)), int(max(0, box[1] - m)), int(min(w, box[2] + m + 1)), int(min(h, box[3] + m + 1)))


def is_seal(name: str) -> bool:
    return d33.d33_class(name) == "seal"


def crop_candidates(objects: list[dict], w: int, h: int, n: int = MAX_CROPS,
                    words: list[dict] | None = None) -> list[int]:
    side = max(w, h)
    pool = []
    for i, o in enumerate(objects):
        b = o["box"]
        bw, bh = b[2] - b[0], b[3] - b[1]
        if bw <= 0 or bh <= 0 or max(bw / bh, bh / bw) > CROP_ASPECT:
            continue
        if not CROP_SIDE[0] <= max(bw, bh) / side <= CROP_SIDE[1]:
            continue
        pool.append(i)
    cover = {i: round(ocr_cover(words or [], objects[i]["box"]), 1) for i in pool}
    pool.sort(key=lambda i: (is_seal(objects[i]["name"]), cover[i], -(objects[i]["score"] or 0)))
    chosen = []
    for i in pool:
        if all(geometry.iou(objects[i]["box"], objects[j]["box"]) < 0.5 for j in chosen):
            chosen.append(i)
        if len(chosen) == min(n, MAX_CROPS):
            break
    return chosen


def require_role(meta: dict, tune: bool) -> None:
    role = meta.get("role")
    if tune and role != "tune":
        raise SystemExit(f"set {meta['name']} has role {role!r}: tuning runs only on role 'tune' (decision 2)")
    if not tune and role == "tune":
        raise SystemExit(f"set {meta['name']} is a tuning set: the final run is on #911's sets only")


def run_variant(meta: dict, v: dict, folder: Path, ocr_run: dict | None) -> None:
    """Runs one variant over a set: the page pass, then tiles, then crops of
    the page and tile boxes; one host process each. A page's time is the sum
    of its calls (warm-ups excluded); peak is the largest of the processes.
    Crop candidates use the set's `ocr` run (the app has its OCR words before
    vision runs)."""
    sp = meta["pages"]
    if v["crops"] and (ocr_run is None or score.missing(sp, ocr_run)):
        raise SystemExit(f"crops need a complete ocr run: `python -m vision_eval.run ocr --sets {meta['name']}`")
    opts = options(v)
    kw = MIRAGE_KEYWORDS if v["keywords"] else None
    host = exe("mirage_host")
    rgbs = [mirage.page_rgb(p["path"]) for p in sp]
    page_res, load, peak = mirage.describe(rgbs, opts, kw, host)
    out, ms = {}, {}
    for p, rgb, r in zip(sp, rgbs, page_res):
        if "error" in r:
            raise RuntimeError(f"Mirage failed on a page: {r['error']}")
        h, w = rgb.shape[:2]
        objs = [dict(o, src="page") for o in mirage_objects(r["result"], r["width"], r["height"])]
        out[p["id"]] = {"width": w, "height": h, "objects": objs, "crops": [], "nan_values": r["nan_values"]}
        ms[p["id"]] = {"page": r["ms"], "tiles": 0.0, "crops": 0.0}
    if v["tile"]:
        jobs = [(p["id"], t, rgb[t[1]:t[3], t[0]:t[2]]) for p, rgb in zip(sp, rgbs)
                for t in tiles(rgb.shape[1], rgb.shape[0], v["tile"])]
        res, load2, peak2 = mirage.describe([j[2] for j in jobs], opts, kw, host)
        peak, load = max(peak, peak2), load + load2
        for (pid, t, _), r in zip(jobs, res):
            if "error" in r:
                raise RuntimeError(f"Mirage failed on a tile: {r['error']}")
            out[pid]["objects"] += [dict(o, src="tile") for o in
                                    mirage_objects(r["result"], r["width"], r["height"], t[0], t[1])]
            ms[pid]["tiles"] += r["ms"]
    if v["crops"]:
        jobs = []
        for p, rgb in zip(sp, rgbs):
            o = out[p["id"]]
            words = ocr_run["outputs"][p["id"]].get("words", [])
            for i in crop_candidates(o["objects"], o["width"], o["height"], v["crops"], words):
                c = crop_box(o["objects"][i]["box"], v["crop_pad"], o["width"], o["height"])
                jobs.append((p["id"], i, c, rgb[c[1]:c[3], c[0]:c[2]]))
        res, load3, peak3 = mirage.describe([j[3] for j in jobs], opts, kw, host)
        peak, load = max(peak, peak3), load + load3
        for (pid, i, c, _), r in zip(jobs, res):
            if "error" in r:
                raise RuntimeError(f"Mirage failed on a crop: {r['error']}")
            out[pid]["crops"].append({"of": i, "box": list(c),
                                      "objects": mirage_objects(r["result"], r["width"], r["height"], c[0], c[1])})
            ms[pid]["crops"] += r["ms"]
    stage = folder.with_name(folder.name + ".partial")
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    for pid, data in out.items():
        (stage / f"{pid}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1, sort_keys=True) + "\n",
                                           encoding="utf-8")
    run = {"candidate": "mirage-tune", "variant": v, "id": variant_id(v), "options": opts,
           "keywords": kw, "set": meta["name"], "pages": len(sp),
           "page_ms": {pid: round(sum(m.values()), 3) for pid, m in ms.items()}, "part_ms": ms,
           "load_ms": round(load, 1), "peak_bytes": peak, "idle_peak_bytes": 0}
    (stage / "run.json").write_text(json.dumps(run, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    if folder.exists():
        shutil.rmtree(folder)
    stage.rename(folder)


# ---- features and the rule layer ----

def roundness(mask: list[float] | None) -> float | None:
    """4πA/P² of the mask polygon: 1 for a circle, 0.785 for a square."""
    if not mask or len(mask) < 6:
        return None
    pts = np.array(mask, dtype=np.float64).reshape(-1, 2)
    x, y = pts[:, 0], pts[:, 1]
    a = 0.5 * abs(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))
    per = float(np.sum(np.hypot(np.diff(np.r_[x, x[0]]), np.diff(np.r_[y, y[0]]))))
    return float(4 * np.pi * a / per ** 2) if per > 0 else None


def ink_colour(hsv: np.ndarray, box: list[float]) -> float:
    """Share of the box's ink pixels (darker than the paper) that are coloured."""
    x0, y0 = max(0, int(box[0])), max(0, int(box[1]))
    x1, y1 = min(hsv.shape[1], int(box[2]) + 1), min(hsv.shape[0], int(box[3]) + 1)
    if x1 <= x0 or y1 <= y0:
        return 0.0
    patch = hsv[y0:y1, x0:x1].reshape(-1, 3).astype(np.float32)
    ink = patch[:, 2] < 200
    if not ink.any():
        return 0.0
    coloured = (patch[:, 1] > 90) & (patch[:, 2] > 40) & ink
    return float(coloured.sum() / ink.sum())


def ocr_cover(words: list[dict], box: list[float]) -> float:
    """Share of the box covered by OCR words whose centre lies inside it."""
    a = geometry.area(box)
    if a <= 0:
        return 0.0
    covered = 0.0
    for w in words:
        b = w["box"]
        cx, cy = (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
        if box[0] <= cx <= box[2] and box[1] <= cy <= box[3]:
            covered += geometry.intersection(b, box)
    return min(1.0, covered / a)


def features(raw: dict, page: dict, words: list[dict]) -> list[dict]:
    """Per page or tile box: its candidate pools and rule features."""
    hsv = None
    side = max(raw["width"], raw["height"])
    crop_seal = {c["of"] for c in raw.get("crops", []) if any(is_seal(o["name"]) for o in c["objects"])}
    out = []
    for i, o in enumerate(raw["objects"]):
        b = o["box"]
        bw, bh = b[2] - b[0], b[3] - b[1]
        if bw <= 0 or bh <= 0:
            continue
        if hsv is None:
            hsv = cv2.cvtColor(cv2.imread(str(page["path"])), cv2.COLOR_BGR2HSV)
        named = is_seal(o["name"])
        out.append({"box": b, "score": o["score"] or 0.0,
                    "pools": {"d33": named, "d33+crop": named or i in crop_seal, "any": True},
                    "aspect": max(bw / bh, bh / bw), "side": max(bw, bh) / side,
                    "round": roundness(o.get("mask")), "ocr": ocr_cover(words, b), "colour": ink_colour(hsv, b)})
    return out


POOLS = ["d33", "d33+crop", "any"]
RULE_GRID = {
    "max_aspect": [None, 2.0, 3.0],
    "min_side": [None, 0.03],
    "max_side": [None, 0.30],
    "min_round": [None, 0.60],
    "max_ocr": [None, 0.20, 0.40],
    "min_colour": [None, 0.15],
}


def keep(f: dict, pool: str, rules: dict) -> bool:
    if not f["pools"][pool]:
        return False
    r = rules
    if r.get("max_aspect") is not None and f["aspect"] > r["max_aspect"]:
        return False
    if r.get("min_side") is not None and f["side"] < r["min_side"]:
        return False
    if r.get("max_side") is not None and f["side"] > r["max_side"]:
        return False
    if r.get("min_round") is not None and f["round"] is not None and f["round"] < r["min_round"]:
        return False
    if r.get("max_ocr") is not None and f["ocr"] > r["max_ocr"]:
        return False
    if r.get("min_colour") is not None and f["colour"] < r["min_colour"]:
        return False
    return True


def seal_boxes(feats: list[dict], pool: str, rules: dict) -> list[list[float]]:
    """The kept boxes after non-maximum suppression at IoU 0.5 (best score first)."""
    kept = sorted((f for f in feats if keep(f, pool, rules)), key=lambda f: -f["score"])
    out = []
    for f in kept:
        if all(geometry.iou(f["box"], b) < IOU for b in out):
            out.append(f["box"])
    return out


# ---- scoring ----

def load_set(out: Path, name: str) -> dict:
    found = pages.available(out)
    if name not in found:
        raise SystemExit(f"set {name} not found (MDOC_VISION_SETS or MDOC_HOLDOUT)")
    return pages.load(found[name])


def tier(page: dict) -> str:
    return page.get("tier", "real")


def prepared(meta: dict, run: dict, ocr_run: dict) -> list[dict]:
    """Per annotated page: seal truth, reference words and box features."""
    out = []
    for p in meta["pages"]:
        raw = run["outputs"].get(p["id"])
        if p["truth"] is None or raw is None:
            continue
        words = (ocr_run["outputs"].get(p["id"]) or {}).get("words", [])
        refs, _ = score.reference_words(p, ocr_run)
        anyb = [o["box"] for o in raw["objects"]] + [o["box"] for c in raw.get("crops", []) for o in c["objects"]]
        out.append({"id": p["id"], "tier": tier(p), "lang": p["lang"],
                    "seals": [a["box"] for a in p["truth"]["artifacts"] if a["class"] == "seal"],
                    "refs": [w["box"] for w in refs], "any": anyb, "feats": features(raw, p, words)})
    return out


def evaluate(prep: list[dict], pool: str, rules: dict, tier_: str | None = "real") -> dict:
    tp = fp = fn = lost = 0
    for p in prep:
        if tier_ and p["tier"] != tier_:
            continue
        boxes = seal_boxes(p["feats"], pool, rules)
        m = len(geometry.match(p["seals"], boxes, IOU))
        tp, fp, fn = tp + m, fp + len(boxes) - m, fn + len(p["seals"]) - m
        lost += sum(1 for w in p["refs"] if any(score.lost_under(w, b) for b in boxes))
    return {"tp": tp, "fp": fp, "fn": fn, "lost": lost,
            "recall": tp / (tp + fn) if tp + fn else None, "precision": tp / (tp + fp) if tp + fp else None}


def ceiling(prep: list[dict], tier_: str) -> tuple[int, int]:
    hit = total = 0
    for p in prep:
        if p["tier"] == tier_:
            hit += len(geometry.match(p["seals"], p["any"], IOU))
            total += len(p["seals"])
    return hit, total


def cost(meta: dict, run: dict, ocr_run: dict, tier_: str | None = "real") -> dict:
    ids = [p["id"] for p in meta["pages"] if (tier_ is None or tier(p) == tier_)
           and p["id"] in run["page_ms"] and p["id"] in ocr_run["page_ms"]]
    if not ids:
        return {}
    total = sum(run["page_ms"][i] for i in ids)
    base = sum(ocr_run["page_ms"][i] for i in ids)
    return {"median_ms": statistics.median(run["page_ms"][i] for i in ids), "total_ms": total,
            "overhead": total / base if base else None, "peak_mb": run["peak_bytes"] / 1e6}


def tune_runs(out: Path, set_name: str) -> list[dict]:
    runs = []
    folder = out / "runs" / set_name / "tune"
    for v in variants():
        r = score.load_run(folder / variant_id(v))
        if r is not None:
            runs.append(r)
    return runs


def pct(x) -> str:
    return "–" if x is None else f"{100 * x:.1f}%"


def gate(out: Path, set_name: str) -> bool:
    meta = load_set(out, set_name)
    require_role(meta, True)
    ocr_run = score.load_run(out / "runs" / set_name / "ocr")
    if ocr_run is None or score.missing(meta["pages"], ocr_run):
        raise SystemExit(f"run `python -m vision_eval.run ocr --sets {set_name}` first")
    rows, best = [], None
    for run in tune_runs(out, set_name):
        if score.missing(meta["pages"], run):
            continue
        prep = prepared(meta, run, ocr_run)
        h, t = ceiling(prep, "real")
        hs, ts = ceiling(prep, "synthetic")
        c = cost(meta, run, ocr_run)
        ok_time = c["median_ms"] <= GATE_MEDIAN_MS
        rows.append((run["id"], h, t, hs, ts, c, ok_time))
        if ok_time and t and (best is None or h / t > best[1] / best[2]):
            best = (run["id"], h, t)
    print("| Variant | Real seals boxed (any name) | Synthetic | Median ms | Within 190 ms |")
    print("|---|---|---|---|---|")
    for vid, h, t, hs, ts, c, ok in sorted(rows, key=lambda r: (-r[1], r[5]["median_ms"])):
        print(f"| {vid} | {h}/{t} ({pct(h / t if t else None)}) | {hs}/{ts} | {c['median_ms']:.0f} | {'yes' if ok else 'no'} |")
    passed = best is not None and best[1] / best[2] >= GATE
    print("\n" + BARS_NOTE)
    print(f"Gate (decision 19, ≥{GATE:.0%}, lowered from 70% after the result, within {GATE_MEDIAN_MS} ms median): "
          + (f"best {best[0]} {best[1]}/{best[2]} ({pct(best[1] / best[2])}): " if best else "no variant within time: ")
          + ("pass" if passed else "fail"))
    return passed


def rule_combos() -> list[dict]:
    keys = list(RULE_GRID)
    return [dict(zip(keys, vals)) for vals in itertools.product(*(RULE_GRID[k] for k in keys))]


def active(rules: dict) -> int:
    return sum(v is not None for v in rules.values())


def sweep(out: Path, set_name: str, write: Path | None) -> dict | None:
    """Decision 11: keep settings with zero words lost, precision ≥85% and
    time within the bar on the tuning split's real pages, then take the
    highest seal recall; ties go to the faster variant, then to higher
    precision, then to fewer active rules, then to grid order."""
    meta = load_set(out, set_name)
    require_role(meta, True)
    ocr_run = score.load_run(out / "runs" / set_name / "ocr")
    if ocr_run is None or score.missing(meta["pages"], ocr_run):
        raise SystemExit(f"run `python -m vision_eval.run ocr --sets {set_name}` first")
    combos = rule_combos()
    cands, per_variant = [], []
    for run in tune_runs(out, set_name):
        if score.missing(meta["pages"], run):
            continue
        c = cost(meta, run, ocr_run)
        prep = prepared(meta, run, ocr_run)
        if not sum(len(p["refs"]) for p in prep if p["tier"] == "real"):
            raise SystemExit("no reference words on the real pages: words lost can't be checked")
        within = c["overhead"] is not None and c["overhead"] <= BARS["time"] and c["peak_mb"] <= BARS["memory_mb"]
        best_v = None
        for pi, pool in enumerate(POOLS):
            for ri, rules in enumerate(combos):
                e = evaluate(prep, pool, rules)
                ok = e["lost"] == 0 and e["precision"] is not None and e["precision"] >= BARS["precision"]
                key = (-(e["recall"] or 0), c["total_ms"], -(e["precision"] or 0), active(rules), pi, ri)
                if ok and (best_v is None or key < best_v[0]):
                    best_v = (key, pool, rules, e)
        per_variant.append((run, c, within, best_v, prep))
        if within and best_v:
            cands.append((best_v[0], run, c, best_v[1], best_v[2], best_v[3], prep))
    print("| Variant | Time vs OCR | Median ms | Peak MB | Within bars | Best valid: pool, recall, precision, lost |")
    print("|---|---|---|---|---|---|")
    for run, c, within, b, _ in sorted(per_variant, key=lambda x: (x[3][0] if x[3] else (1,))):
        best_s = "none" if not b else f"{b[1]}, {pct(b[3]['recall'])} ({b[3]['tp']}/{b[3]['tp'] + b[3]['fn']}), {pct(b[3]['precision'])}, {b[3]['lost']}"
        print(f"| {run['id']} | {pct(c['overhead'])} | {c['median_ms']:.0f} | {c['peak_mb']:.0f} | {'yes' if within else 'no'} | {best_s} |")
    print("\n" + BARS_NOTE)
    if not cands:
        print("No setting meets zero words lost, precision ≥85% and the time and memory bars.")
        return None
    key, run, c, pool, rules, e, prep = min(cands, key=lambda x: x[0])
    syn = evaluate(prep, pool, rules, "synthetic")
    setting = {"issue": "lawkitt/mdoc#918", "engine": "libcactus 4.1.1", "variant": run["variant"],
               "variant_id": run["id"], "options": run["options"], "keywords": run["keywords"],
               "name_map": {"seal": dict(d33.D33_MAP)["seal"]}, "pool": pool, "rules": rules,
               "nms_iou": IOU, "crop_rule": {"per_page": run["variant"]["crops"], "side": list(CROP_SIDE), "max_aspect": CROP_ASPECT},
               "tuning": {"set": set_name, "real": e, "synthetic": syn,
                          "median_ms": c["median_ms"], "overhead": c["overhead"], "peak_mb": c["peak_mb"]}}
    print(f"Selected: {run['id']}, pool {pool}, rules {json.dumps(rules)}; real recall {pct(e['recall'])} "
          f"({e['tp']}/{e['tp'] + e['fn']}), precision {pct(e['precision'])}, lost {e['lost']}; "
          f"synthetic recall {pct(syn['recall'])}, precision {pct(syn['precision'])}")
    # Decision 21: the setting must already meet every final bar on the
    # tuning split before #911's sets are used. Precision, words lost, time
    # and memory hold by selection; recall is the one left to check.
    setting["tuning"]["meets_final_bars"] = passes = e["recall"] is not None and e["recall"] >= BARS["recall"]
    print(f"Decision 21 check on {set_name}: recall ≥{BARS['recall']:.0%}, precision ≥{BARS['precision']:.0%}, "
          f"zero words lost, time and memory: {'pass' if passes else 'fail'}")
    if write:
        write.write_text(json.dumps(setting, indent=1, sort_keys=True) + "\n", encoding="utf-8")
        print(f"setting written; sha256 {digest(write)}")
    return setting


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def setting_problems(setting: dict) -> list[str]:
    """Every field of a saved setting must equal what this harness executes."""
    v = setting.get("variant") or {}
    if v not in variants():
        return ["unknown variant"]
    expected = {"variant_id": variant_id(v), "options": options(v),
                "keywords": MIRAGE_KEYWORDS if v["keywords"] else None,
                "name_map": {"seal": dict(d33.D33_MAP)["seal"]}, "nms_iou": IOU,
                "crop_rule": {"per_page": v["crops"], "side": list(CROP_SIDE), "max_aspect": CROP_ASPECT}}
    out = [f"{k} differs" for k, want in expected.items() if setting.get(k) != want]
    if setting.get("pool") not in POOLS:
        out.append("unknown pool")
    rules = setting.get("rules")
    if not isinstance(rules, dict) or set(rules) != set(RULE_GRID) or any(
            rules[k] not in RULE_GRID[k] for k in RULE_GRID):
        out.append("rules outside the grid")
    return out


def final(out: Path, setting_path: Path, set_names: list[str]) -> None:
    """The one run of the pre-registered setting on #911's sets (decision 12)."""
    marker = out / "tune-final.json"
    if marker.exists():
        raise SystemExit(f"the final run already happened ({marker}); decision 12 allows one")
    setting = json.loads(setting_path.read_text(encoding="utf-8"))
    sha = digest(setting_path)
    v = setting["variant"]
    problems = setting_problems(setting)
    if not setting.get("tuning", {}).get("meets_final_bars"):
        problems.append("the setting doesn't meet the final bars on the tuning split (decision 21)")
    if problems:
        raise SystemExit("the setting doesn't match what this harness would run: " + "; ".join(problems))
    metas = [load_set(out, n) for n in set_names]
    for meta in metas:
        require_role(meta, False)
        ocr_run = score.load_run(out / "runs" / meta["name"] / "ocr")
        if ocr_run is None or score.missing(meta["pages"], ocr_run):
            raise SystemExit(f"run `python -m vision_eval.run ocr --sets {meta['name']}` first")
    marker.write_text(json.dumps({"setting_sha256": sha, "sets": set_names,
                                  "started": datetime.now(timezone.utc).isoformat()}, indent=1) + "\n")
    print(f"# Final run (lawkitt/mdoc#918), setting sha256 {sha}\n")
    print("| Set | Seals | Recall | Precision | Words lost (reference) | Time vs OCR | Median ms (OCR) | Peak MB | Any-name ceiling | Verdict |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    verdicts = {}
    for meta in metas:
        folder = out / "runs" / meta["name"] / "tune-final"
        ocr_run = score.load_run(out / "runs" / meta["name"] / "ocr")
        run_variant(meta, v, folder, ocr_run)
        run = score.load_run(folder)
        prep = prepared(meta, run, ocr_run)
        e = evaluate(prep, setting["pool"], setting["rules"], None)
        h, t = sum(ceiling(prep, k)[0] for k in ("real", "synthetic")), sum(ceiling(prep, k)[1] for k in ("real", "synthetic"))
        c = cost(meta, run, ocr_run, None)
        ocr_median = statistics.median(ocr_run["page_ms"][p["id"]] for p in meta["pages"])
        refs = sum(len(p["refs"]) for p in prep)
        checks = [e["recall"] is not None and e["recall"] >= BARS["recall"],
                  e["precision"] is not None and e["precision"] >= BARS["precision"],
                  e["lost"] <= BARS["lost_words"] and refs > 0,
                  c["overhead"] <= BARS["time"], c["peak_mb"] <= BARS["memory_mb"]]
        verdicts[meta["name"]] = all(checks)
        print(f"| {meta['name']} | {e['tp'] + e['fn']} | {pct(e['recall'])} ({e['tp']}) | {pct(e['precision'])} ({e['tp'] + e['fp']} pred) | "
              f"{e['lost']} ({refs}) | {pct(c['overhead'])} | {c['median_ms']:.0f} ({ocr_median:.0f}) | {c['peak_mb']:.0f} | "
              f"{h}/{t} | {'pass' if all(checks) else '**fail**'} |")
    print(f"\nBars (decisions 18 and 20; recall lowered from 90% and then 70%): seal recall ≥{BARS['recall']:.0%}, precision ≥{BARS['precision']:.0%}, zero words lost, "
          f"time ≤+{BARS['time']:.0%} of OCR, peak ≤{BARS['memory_mb']} MB.")
    print(BARS_NOTE)
    real = [n for n in verdicts if n != "overlays"]
    if real and all(verdicts[n] for n in real):
        print(f"Outcome: pass on the real sets, reported as a {GUARD} (seal recall bar 67%, not #911's 90%).")
    else:
        print("Outcome: fail (a real set misses a bar).")
    data = json.loads(marker.read_text())
    data.update(verdicts=verdicts, finished=datetime.now(timezone.utc).isoformat())
    marker.write_text(json.dumps(data, indent=1) + "\n")


# ---- disjointness ----

# Sets with no sources.json whose pages are generated from public fixtures
# (`overlay.py`): compared by page hashes only.
HASH_ONLY = {"overlays"}


def entry_ids(entry) -> set[str]:
    entry = entry if isinstance(entry, dict) else {"file": str(entry)}
    out = set()
    if entry.get("item"):
        out.add(f"item:{entry['item']}")
    if entry.get("file"):
        f = str(entry["file"])
        out.add(f"item:{f.split('__')[0]}" if "__" in f else f"file:{f}")
    for k in ("url", "sha256", "source_sha256"):
        if entry.get(k):
            out.add(f"{k}:{entry[k]}")
    return out


def page_source_ids(set_dir: Path, page_ids: list[str]) -> dict[str, set[str]]:
    """Per page, its source identifiers from the set's sources.json: a RECAP
    item (the docket behind the page), a source file, a URL or a file digest.
    A page without any is missing from the result."""
    path = set_dir / "sources.json"
    src = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    return {pid: ids for pid in page_ids if (ids := entry_ids(src.get(pid, {})))}


def source_ids(set_dir: Path) -> set[str]:
    path = set_dir / "sources.json"
    if not path.exists():
        return set()
    return set().union(*(entry_ids(e) for e in json.loads(path.read_text(encoding="utf-8")).values()))


def dhash(path: Path) -> int:
    from PIL import Image
    with Image.open(path) as im:
        g = np.asarray(im.convert("L").resize((17, 16)), dtype=np.int16)
    bits = (g[:, 1:] > g[:, :-1]).flatten()
    return int("".join("1" if b else "0" for b in bits), 2)


def disjoint(out: Path, tune_name: str, against: list[str]) -> bool:
    tmeta = load_set(out, tune_name)
    require_role(tmeta, True)
    tper = page_source_ids(tmeta["dir"], [p["id"] for p in tmeta["pages"]])
    unknown = len(tmeta["pages"]) - len(tper)
    if unknown:
        print(f"{tune_name}: {unknown} pages without source ids: provenance is incomplete")
        return False
    tids = set().union(*tper.values())
    tpages = {p["id"]: (pages.rgb_digest(p["path"]), dhash(p["path"])) for p in tmeta["pages"]}
    ok = True
    for name in against:
        meta = load_set(out, name)
        per = page_source_ids(meta["dir"], [p["id"] for p in meta["pages"]])
        ids = set().union(set(), *per.values())
        lacking = len(meta["pages"]) - len(per)
        if lacking and name not in HASH_ONLY:
            print(f"{tune_name} vs {name}: {lacking} pages of {name} without source ids: provenance is incomplete")
            ok = False
        shared = tids & ids
        digests = {pages.rgb_digest(p["path"]): p["id"] for p in meta["pages"]}
        hashes = [dhash(p["path"]) for p in meta["pages"]]
        same = sum(1 for d, _ in tpages.values() if d in digests)
        near = sum(1 for _, h in tpages.values() if any(bin(h ^ x).count("1") <= 6 for x in hashes))
        print(f"{tune_name} vs {name}: {len(tids)}/{len(ids)} source ids, {len(shared)} shared; "
              f"{same} identical pages; {near} near-identical pages (dHash ≤6 of 256 bits)")
        ok = ok and not shared and not same and not near
    print("disjoint" if ok else "NOT disjoint")
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("disjoint"); d.add_argument("--tune", default="tune-seal")
    d.add_argument("--against", default="en-recap,ru-filings,overlays,ru-commons")
    r = sub.add_parser("run"); r.add_argument("--sets", default=",".join(TUNE_SETS)); r.add_argument("--variants", default="all")
    g = sub.add_parser("gate"); g.add_argument("--sets", default="tune-seal")
    s = sub.add_parser("sweep"); s.add_argument("--sets", default="tune-seal"); s.add_argument("--write", type=Path)
    f = sub.add_parser("final"); f.add_argument("setting", type=Path); f.add_argument("--sets", default=",".join(FINAL_SETS))
    for p in (d, r, g, s, f):
        p.add_argument("--out")
    args = ap.parse_args()
    out = out_root(args.out)
    if args.cmd == "disjoint":
        raise SystemExit(0 if disjoint(out, args.tune, args.against.split(",")) else 1)
    if args.cmd == "run":
        wanted = variants() if args.variants == "all" else [v for v in variants() if variant_id(v) in args.variants.split(",")]
        if args.variants != "all" and len(wanted) != len(args.variants.split(",")):
            raise SystemExit(f"unknown variants; choose from {[variant_id(v) for v in variants()]}")
        for set_name in args.sets.split(","):
            meta = load_set(out, set_name)
            require_role(meta, True)
            ocr_run = score.load_run(out / "runs" / set_name / "ocr")
            for v in wanted:
                t = time.perf_counter()
                run_variant(meta, v, out / "runs" / set_name / "tune" / variant_id(v), ocr_run)
                print(f"{set_name}/{variant_id(v)}: done in {time.perf_counter() - t:.0f} s", flush=True)
    elif args.cmd == "gate":
        raise SystemExit(0 if gate(out, args.sets) else 1)
    elif args.cmd == "sweep":
        sweep(out, args.sets, args.write)
    else:
        final(out, args.setting, args.sets.split(","))


if __name__ == "__main__":
    main()
