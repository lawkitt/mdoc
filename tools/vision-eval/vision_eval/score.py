"""Scores runs against annotations and decision 13's bars; prints the report.

    python -m vision_eval.score [--sets ...] [--out DIR] [--markdown report.md] [--json scores.json]

Per class and candidate: recall and precision at box IoU 0.5 (0.3 as a
diagnostic). Text loss: page words (from the `ocr` run) whose centre lies
outside every annotated artifact but at least half of whose box falls inside
a candidate's predicted box of a text-replacing class. Seal text: CER of the
seal reader on matched seals, and of the readings it would inline (reader
confidence at or above the gate). Time: a candidate's total page time over
the `ocr` run's on the same pages. Memory: a Python runner's peak minus its
peak before loading a model; a native host's own peak.

Only aggregates are printed, never page names or text, so the report can be
posted with private sets included.
"""
from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

from . import classes, geometry, pages
from .run import out_root

IOU = 0.5
IOU_LOOSE = 0.3
BARS = {"recall": 0.90, "precision": 0.85, "cer": 0.10, "time": 0.30, "memory_mb": 300, "lost_words": 0}
DETECTORS = ["mirage", "layout", "rxing"]
READERS = ["seal", "seal-mobile", "seal-gt", "seal-mobile-gt"]


def load_run(folder: Path) -> dict | None:
    if not (folder / "run.json").exists():
        return None
    run = json.loads((folder / "run.json").read_text(encoding="utf-8"))
    run["outputs"] = {p.stem: json.loads(p.read_text(encoding="utf-8"))
                      for p in folder.glob("*.json") if p.name != "run.json"}
    return run


def predictions(candidate: str, raw: dict) -> list[dict]:
    """A candidate's page output as [{"class", "box", "score"}] (classes only)."""
    if candidate == "layout":
        return [{"class": i["class"], "box": i["box"], "score": i["score"]}
                for i in raw.get("items", []) if i["class"] and i["score"] >= classes.LAYOUT_THRESHOLD]
    if candidate == "mirage":
        return [{"class": o["class"], "box": o["box"], "score": o["score"]}
                for o in raw.get("objects", []) if o["class"]]
    if candidate == "rxing":
        return [{"class": c["class"], "box": c["box"], "score": 1.0} for c in raw.get("codes", [])]
    return []


def detection_scores(set_pages: list[dict], run: dict, candidate: str) -> dict:
    """Per class and language: tp, fp, fn at IoU and loose IoU."""
    counts = defaultdict(lambda: defaultdict(int))
    for page in set_pages:
        truth = page["truth"]
        raw = run["outputs"].get(page["id"])
        if truth is None or raw is None:
            continue
        preds = predictions(candidate, raw)
        for cls in classes.CAPABLE[candidate]:
            g = [a["box"] for a in truth["artifacts"] if a["class"] == cls]
            p = [x["box"] for x in preds if x["class"] == cls]
            if not g and not p:
                continue
            for lang in (page["lang"], "all"):
                c = counts[(cls, lang)]
                m = len(geometry.match(g, p, IOU))
                c["tp"] += m
                c["fp"] += len(p) - m
                c["fn"] += len(g) - m
                ml = len(geometry.match(g, p, IOU_LOOSE))
                c["tp_loose"] += ml
                c["fn_loose"] += len(g) - ml
                c["fp_loose"] += len(p) - ml
    return counts


def cross_class(set_pages: list[dict], run: dict, candidate: str, truth_cls: str, pred_cls: str) -> tuple[int, int]:
    """Annotated `truth_cls` boxes matched by predicted `pred_cls` boxes (a diagnostic)."""
    hit = total = 0
    for page in set_pages:
        raw = run["outputs"].get(page["id"])
        if page["truth"] is None or raw is None:
            continue
        g = [a["box"] for a in page["truth"]["artifacts"] if a["class"] == truth_cls]
        p = [x["box"] for x in predictions(candidate, raw) if x["class"] == pred_cls]
        total += len(g)
        hit += len(geometry.match(g, p, IOU))
    return hit, total


def text_loss(set_pages: list[dict], ocr_run: dict, run: dict, candidate: str) -> dict:
    lost = defaultdict(int)
    reference = 0
    for page in set_pages:
        truth = page["truth"]
        raw = run["outputs"].get(page["id"])
        words = (ocr_run["outputs"].get(page["id"]) or {}).get("words", [])
        if truth is None or raw is None:
            continue
        true_boxes = [a["box"] for a in truth["artifacts"]]
        preds = [x for x in predictions(candidate, raw) if x["class"] in classes.REPLACING]
        for w in words:
            b = w["box"]
            cx, cy = (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
            if any(t[0] <= cx <= t[2] and t[1] <= cy <= t[3] for t in true_boxes):
                continue
            reference += 1
            a = geometry.area(b)
            for x in preds:
                if a > 0 and geometry.intersection(b, x["box"]) / a >= 0.5:
                    lost[x["class"]] += 1
                    break
    return {"reference_words": reference, "lost": dict(lost), "lost_total": sum(lost.values())}


def seal_text(set_pages: list[dict], run: dict) -> dict:
    errors = chars = inl_errors = inl_chars = 0
    read = inlined = inlined_over = unread = 0
    worst = 0.0
    for page in set_pages:
        truth = page["truth"]
        raw = run["outputs"].get(page["id"])
        if truth is None or raw is None:
            continue
        gt = [a for a in truth["artifacts"] if a["class"] == "seal" and a.get("segments")]
        seals = raw.get("seals", [])
        pairs = geometry.match([a["box"] for a in gt], [s["box"] for s in seals], IOU)
        unread += len(gt) - len(pairs)
        for gi, si in pairs:
            e, n = geometry.seal_cer(gt[gi]["segments"], seals[si]["text"])
            errors += e
            chars += n
            read += 1
            if seals[si]["inline"]:
                inlined += 1
                inl_errors += e
                inl_chars += n
                cer = e / n if n else 0.0
                worst = max(worst, cer)
                inlined_over += cer > BARS["cer"]
    return {"seals_read": read, "seals_not_found": unread,
            "cer": errors / chars if chars else None,
            "inlined": inlined, "inlined_cer": inl_errors / inl_chars if inl_chars else None,
            "inlined_worst_cer": worst if inlined else None, "inlined_over_bar": inlined_over}


def timing(run: dict, ocr_run: dict) -> dict:
    ids = [i for i in run["page_ms"] if i in ocr_run["page_ms"]]
    if not ids:
        return {}
    cand = sum(run["page_ms"][i] for i in ids)
    base = sum(ocr_run["page_ms"][i] for i in ids)
    added = run["peak_bytes"] - run["idle_peak_bytes"]
    return {"median_ms": statistics.median(run["page_ms"][i] for i in ids),
            "ocr_median_ms": statistics.median(ocr_run["page_ms"][i] for i in ids),
            "overhead": cand / base if base else None,
            "added_peak_mb": added / 1e6 if run["peak_bytes"] else None}


def pct(x) -> str:
    return "–" if x is None else f"{100 * x:.1f}%"


def verdict(ok) -> str:
    return "–" if ok is None else ("pass" if ok else "**fail**")


def report(out: Path, set_names: list[str]) -> tuple[str, dict]:
    found = pages.available(out)
    lines, data = ["# Vision evaluation report", ""], {}
    for set_name in set_names:
        meta = pages.load(found[set_name])
        sp = meta["pages"]
        runs_dir = out / "runs" / set_name
        ocr_run = load_run(runs_dir / "ocr")
        n_truth = sum(1 for p in sp if p["truth"] is not None)
        art = defaultdict(int)
        for p in sp:
            for a in (p["truth"] or {"artifacts": []})["artifacts"]:
                art[a["class"]] += 1
        lines += [f"## Set `{set_name}`", "",
                  f"{len(sp)} pages ({n_truth} annotated); artifacts: "
                  + (", ".join(f"{k} {v}" for k, v in sorted(art.items())) or "none"), ""]
        sd = data[set_name] = {"pages": len(sp), "artifacts": dict(art)}
        # Detection per class.
        lines += ["| Candidate | Class | Lang | GT | Pred | Recall | Precision | Recall@0.3 | Verdict |",
                  "|---|---|---|---|---|---|---|---|---|"]
        for cand in DETECTORS:
            run = load_run(runs_dir / cand)
            if run is None:
                continue
            counts = detection_scores(sp, run, cand)
            sd.setdefault("detection", {})[cand] = {f"{k[0]}/{k[1]}": dict(v) for k, v in counts.items()}
            for (cls, lang), c in sorted(counts.items(), key=lambda kv: (classes.CLASSES.index(kv[0][0]), kv[0][1])):
                gt, pred = c["tp"] + c["fn"], c["tp"] + c["fp"]
                rec = c["tp"] / gt if gt else None
                prec = c["tp"] / pred if pred else None
                recl = c["tp_loose"] / gt if gt else None
                ok = None
                if lang == "all" and pred + gt and cls not in classes.NO_BAR:
                    checks = []
                    if prec is not None:
                        checks.append(prec >= BARS["precision"])
                    if cls in classes.RECALL_BAR_CLASSES and rec is not None:
                        checks.append(rec >= BARS["recall"])
                    ok = all(checks) if checks else None
                lines.append(f"| {cand} | {cls} | {lang} | {gt} | {pred} | {pct(rec)} | {pct(prec)} | {pct(recl)} | {verdict(ok)} |")
        lines.append("")
        layout = load_run(runs_dir / "layout")
        if layout:
            hit, total = cross_class(sp, layout, "layout", "signature", "picture")
            if total:
                lines += [f"Diagnostic: layout `image` boxes match {hit} of {total} annotated signatures at IoU 0.5.", ""]
        # Text loss, time and memory.
        if ocr_run:
            lines += ["| Candidate | Reference words | Words lost (by predicted class) | Time vs OCR | Median ms (OCR) | Added peak | Verdicts (loss, time, memory) |",
                      "|---|---|---|---|---|---|---|"]
            for cand in DETECTORS + READERS:
                run = load_run(runs_dir / cand)
                if run is None:
                    continue
                t = timing(run, ocr_run)
                loss = text_loss(sp, ocr_run, run, cand) if cand in DETECTORS else None
                sd.setdefault("cost", {})[cand] = {"timing": t, "loss": loss}
                lost = "–" if loss is None else (f"{loss['lost_total']}" + (
                    " (" + ", ".join(f"{k} {v}" for k, v in sorted(loss["lost"].items())) + ")" if loss["lost"] else ""))
                ref = "–" if loss is None else str(loss["reference_words"])
                mem = t.get("added_peak_mb")
                v_loss = None if loss is None else loss["lost_total"] <= BARS["lost_words"]
                v_time = None if t.get("overhead") is None else t["overhead"] <= BARS["time"]
                v_mem = None if mem is None else mem <= BARS["memory_mb"]
                lines.append(f"| {cand} | {ref} | {lost} | {pct(t.get('overhead'))} | {t.get('median_ms', 0):.0f} ({t.get('ocr_median_ms', 0):.0f}) | "
                             f"{'–' if mem is None else f'{mem:.0f} MB'} | {verdict(v_loss)}, {verdict(v_time)}, {verdict(v_mem)} |")
            lines.append("")
        # Seal text.
        rows = []
        for cand in READERS:
            run = load_run(runs_dir / cand)
            if run is None:
                continue
            st = seal_text(sp, run)
            sd.setdefault("seal_text", {})[cand] = st
            if st["seals_read"] + st["seals_not_found"] == 0:
                continue
            ok = None if not st["inlined"] else st["inlined_over_bar"] == 0
            rows.append(f"| {cand} | {st['seals_read']} | {st['seals_not_found']} | {pct(st['cer'])} | {st['inlined']} | "
                        f"{pct(st['inlined_cer'])} | {pct(st['inlined_worst_cer'])} | {st['inlined_over_bar']} | {verdict(ok)} |")
        if rows:
            lines += ["| Reader | Seals read | Not found | CER (all read) | Inlined | CER (inlined) | Worst inlined | Inlined over 10% | Verdict |",
                      "|---|---|---|---|---|---|---|---|---|"] + rows + [""]
    lines += [f"Bars (decision 13): recall ≥ {BARS['recall']:.0%} for seal and signature; precision ≥ {BARS['precision']:.0%} per class; "
              f"zero words lost; seal text inlined only at CER ≤ {BARS['cer']:.0%}; time ≤ +{BARS['time']:.0%} of OCR; added peak ≤ {BARS['memory_mb']} MB.", ""]
    return "\n".join(lines), data


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sets", default=None)
    ap.add_argument("--out", default=None)
    ap.add_argument("--markdown", type=Path)
    ap.add_argument("--json", type=Path)
    args = ap.parse_args()
    out = out_root(args.out)
    found = pages.available(out)
    names = args.sets.split(",") if args.sets else list(found)
    text, data = report(out, names)
    print(text)
    if args.markdown:
        args.markdown.write_text(text + "\n", encoding="utf-8")
    if args.json:
        args.json.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
