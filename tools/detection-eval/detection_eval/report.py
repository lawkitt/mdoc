"""The aggregate scoreboard of every option, against decision 7.

    python -m detection_eval.report [<option>...] [--out DIR] [--markdown FILE]

For each option folder under `<out>` (from `run` and `combo`) and each set in
it, runs `just recognize --scoreboard <folder> --snapshot`: the full
correction pipeline over the option's snapshots. Only aggregates are
printed: recall and precision per language and set, English pooled over the
holdout and real set, and per category (English, pooled).

Decision 7: an option is listed if its pooled English recall is at least
90% and its pooled English precision is no more than 10 points below the
default's; a combo must also add at least 2 points of recall over its best
member.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

from . import fixtures
from .combo import combo_members
from .models import BY_ID, COMBOS
from .run import out_root

SETS = ("fixtures", "holdout", "real")
PRIVATE = ("holdout", "real")
RECALL_BAR = 90.0
PRECISION_GAP = 10.0
COMBO_GAIN = 2.0


def scoreboard(folder: Path) -> dict[tuple[str, str], list[int]]:
    """(language, category) → [gold, found, proposed, right], summed over kinds."""
    result = subprocess.run(
        ["just", "recognize", "--scoreboard", str(folder), "--snapshot"],
        cwd=fixtures.REPO,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    cells: dict[tuple[str, str], list[int]] = {}
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) != 9 or not parts[3].isdigit():
            continue
        lang, category, kind = parts[0], parts[1], parts[2]
        if lang == "all" or (category == "all" and kind == "all"):
            continue  # totals: recomputed below
        counts = [int(parts[3]), int(parts[4]), int(parts[6]), int(parts[7])]
        cell = cells.setdefault((lang, category), [0, 0, 0, 0])
        for i, value in enumerate(counts):
            cell[i] += value
    return cells


def total(cells: dict, lang: str | None = None, category: str | None = None) -> list[int]:
    out = [0, 0, 0, 0]
    for (l, c), counts in cells.items():
        if (lang is None or l == lang) and (category is None or c == category):
            for i, v in enumerate(counts):
                out[i] += v
    return out


def rp(counts: list[int]) -> tuple[float | None, float | None]:
    gold, found, proposed, right = counts
    recall = 100.0 * found / gold if gold else None
    precision = 100.0 * right / proposed if proposed else None
    return recall, precision


def fmt(counts: list[int]) -> str:
    r, p = rp(counts)
    return f"{'-' if r is None else f'{r:.1f}'} / {'-' if p is None else f'{p:.1f}'}"


def add(*counts: list[int]) -> list[int]:
    return [sum(c[i] for c in counts) for i in range(4)]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("options", nargs="*")
    parser.add_argument("--out")
    parser.add_argument("--markdown", help="also write the tables to this file")
    parser.add_argument("--json", help="write the aggregate counts to this file")
    args = parser.parse_args()
    out = out_root(args.out)
    options = args.options or sorted(p.name for p in out.iterdir() if p.is_dir())
    boards: dict[str, dict[str, dict]] = {}
    for option in options:
        for name in SETS:
            folder = out / option / name
            if folder.is_dir():
                boards.setdefault(option, {})[name] = scoreboard(folder)
                print(f"scored {option} {name}", flush=True)

    def pooled_en(option: str) -> list[int]:
        sets = boards.get(option, {})
        return add(*(total(sets[s], "en") for s in PRIVATE if s in sets))

    default_p = rp(pooled_en("default"))[1] if "default" in boards else None
    lines = [
        "| option | size | EN holdout R / P | EN real R / P | **EN pooled R / P** | RU holdout R / P | RU real R / P | fixtures R / P | decision 7 |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    verdicts = {}
    for option, sets in boards.items():
        pooled = pooled_en(option)
        recall, precision = rp(pooled)
        reasons = []
        if recall is None or recall < RECALL_BAR:
            reasons.append(f"recall < {RECALL_BAR:.0f}")
        if default_p is not None and (precision is None or precision < default_p - PRECISION_GAP):
            reasons.append(f"precision < default − {PRECISION_GAP:.0f}")
        if option in COMBOS or "+" in option:
            member_recalls = [rp(pooled_en(m))[0] or 0.0 for m in combo_members(option) if m in boards]
            if member_recalls and (recall or 0.0) < max(member_recalls) + COMBO_GAIN:
                reasons.append(f"gain < {COMBO_GAIN:.0f} over best member")
        verdict = "pass" if not reasons and "holdout" in sets and "real" in sets else (
            "; ".join(reasons) or "incomplete"
        )
        verdicts[option] = verdict
        size = BY_ID[option].size if option in BY_ID else ("default" if option == "default" else "combo")
        cell = lambda s, lang: fmt(total(sets[s], lang)) if s in sets else "n/a"  # noqa: E731
        lines.append(
            f"| {option} | {size} | {cell('holdout', 'en')} | {cell('real', 'en')} | **{fmt(pooled)}** | "
            f"{cell('holdout', 'ru')} | {cell('real', 'ru')} | {cell('fixtures', None)} | {verdict} |"
        )
    categories = sorted({c for sets in boards.values() for s in PRIVATE if s in sets for (_, c) in sets[s]})
    cat_lines = [
        "| option | " + " | ".join(categories) + " |",
        "|---|" + "---|" * len(categories),
    ]
    for option, sets in boards.items():
        row = []
        for category in categories:
            counts = add(*(total(sets[s], "en", category) for s in PRIVATE if s in sets))
            r, _ = rp(counts)
            row.append("-" if r is None else f"{r:.0f}")
        cat_lines.append(f"| {option} | " + " | ".join(row) + " |")
    text = (
        "Recall / precision (%), wordings per document, through the full pipeline "
        "(`just recognize --scoreboard --snapshot`).\n\n"
        + "\n".join(lines)
        + "\n\nEnglish recall per category (%), holdout and real set pooled:\n\n"
        + "\n".join(cat_lines)
        + "\n"
    )
    print(text)
    if args.markdown:
        Path(args.markdown).write_text(text, encoding="utf-8")
    if args.json:
        dump = {
            option: {
                s: {f"{l}|{c}": v for (l, c), v in cells.items()} for s, cells in sets.items()
            }
            for option, sets in boards.items()
        }
        Path(args.json).write_text(json.dumps({"verdicts": verdicts, "counts": dump}, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
