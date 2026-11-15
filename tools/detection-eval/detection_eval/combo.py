"""Combo snapshots: the union of member snapshots (decision 11).

    python -m detection_eval.combo [<combo-id>... | <a>+<b>[+<c>]...]
        [--sets fixtures,holdout,real] [--out DIR]

Every member's spans go to the pipeline. Members fold in rank order: the
higher-ranked side's spans all stay, with their categories; a lower-ranked
span that overlaps one of them is dropped, and widens it to its own extent
when it is longer and not inside it (the longest span, the higher-ranked
member's category; a span nested in another of the same member is not
widened).
A lower-ranked span that overlaps nothing passes. A member's own
overlapping spans (a person inside an organization, say) are kept as the
member gave them, so a combo of a member with itself is that member.
Members run in sequence, so a combo's peak memory is its largest member's.

Members are ranked in the order named; their snapshots must exist under
`<out>/<member>/<set>/` (`detection_eval.run`). With no ids, decision 9's
combos run.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from . import fixtures
from .models import COMBOS
from .run import chosen_sets, out_root, write_meta


def merge(members: list[list[dict]]) -> list[dict]:
    """Merged entities (byte offsets, without text) of ranked members: each
    member folded into the result of the higher-ranked ones. Every span
    keeps the rank of the member whose category it carries."""
    ranked = [[dict(e, rank=r) for e in entities] for r, entities in enumerate(members)]
    out = dedup(ranked[0]) if ranked else []
    for lower in ranked[1:]:
        out = merge_pair(out, lower)
    return [{k: v for k, v in e.items() if k != "rank"} for e in out]


def merge_pair(high: list[dict], low: list[dict]) -> list[dict]:
    """`high`'s spans keep their categories; a `low` span overlapping one of
    them is dropped, and widens the outermost ones it overlaps (those not
    inside another it overlaps) of the best rank among them to its own
    extent when longer: the longest span wins, with the highest rank's
    category. A `low` span inside a `high` one widens nothing, and one
    overlapping none passes. Each side's own overlapping spans are left as
    they are."""

    def length(e: dict) -> int:
        return e["end"] - e["start"]

    def inside(a: dict, b: dict) -> bool:
        return b["start"] <= a["start"] and a["end"] <= b["end"]

    widened = [dict(h) for h in high]
    passed = []
    for l in low:
        hits = [i for i, h in enumerate(high) if l["start"] < h["end"] and h["start"] < l["end"]]
        if not hits:
            passed.append(dict(l))
            continue
        if any(inside(l, high[i]) for i in hits):
            continue
        outermost = [
            i for i in hits
            if not any(inside(high[i], high[j]) and length(high[j]) > length(high[i]) for j in hits)
        ]
        top = min(high[i].get("rank", 0) for i in outermost)
        for i in outermost:
            if high[i].get("rank", 0) == top and length(l) > length(widened[i]):
                widened[i]["start"], widened[i]["end"] = l["start"], l["end"]
    return dedup(widened + passed)


def dedup(entities: list[dict]) -> list[dict]:
    """One entity per (start, end, label), at its best score."""
    best: dict[tuple, dict] = {}
    for e in entities:
        key = (e["start"], e["end"], e["label"])
        if key not in best or e["score"] > best[key]["score"]:
            best[key] = dict(e)
    return sorted(best.values(), key=lambda e: (e["start"], e["end"], e["label"]))


def combo_members(combo: str) -> list[str]:
    return COMBOS.get(combo) or combo.split("+")


def build(combo: str, sets: dict[str, Path], out: Path) -> None:
    members = combo_members(combo)
    for name, root in sets.items():
        missing = [m for m in members if not fixtures.complete(out / m / name)]
        if missing:
            raise SystemExit(f"{combo} {name}: no complete run of {', '.join(missing)}")
        corpus = fixtures.corpus_id(root)
        stale = [m for m in members if fixtures.corpus_id(out / m / name) != corpus]
        if stale:
            raise SystemExit(f"{combo} {name}: {', '.join(stale)} ran on other documents; rerun them")
        with fixtures.staged(out / combo / name) as target:
            count = 0
            for original, copy in fixtures.mirror(root, target):
                relative = original.relative_to(root)
                snapshots = [
                    json.loads(
                        fixtures.snapshot_path(out / m / name / relative).read_text(encoding="utf-8")
                    )
                    for m in members
                ]
                write_merged(original, copy, combo, snapshots)
                count += 1
            write_meta(target, combo, 0.0, {"members": members})
        print(f"{combo} {name}: {count} snapshots", flush=True)


def write_merged(original: Path, copy: Path, combo: str, snapshots: list[dict]) -> None:
    source = fixtures.source_of(original)
    data = source.encode("utf-8")
    for snapshot in snapshots:
        for e in snapshot["entities"]:
            if data[e["start"] : e["end"]].decode("utf-8", "replace") != e["text"]:
                raise SystemExit(f"{snapshot['model']}: a span of {copy.name} no longer matches its text")
    merged = merge([s["entities"] for s in snapshots])
    # Back to character offsets for the shared writer.
    char_at = {}
    pos = 0
    for i, ch in enumerate(source):
        char_at[pos] = i
        pos += len(ch.encode("utf-8"))
    char_at[pos] = len(source)
    entities = [
        fixtures.Entity(char_at[e["start"]], char_at[e["end"]], e["label"], e["score"])
        for e in merged
        if e["start"] in char_at and e["end"] in char_at and data[e["start"] : e["end"]]
    ]
    threshold = min(s["threshold"] for s in snapshots)
    fixtures.write_snapshot(copy, source, combo, threshold, entities)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("combos", nargs="*")
    parser.add_argument("--sets")
    parser.add_argument("--out")
    args = parser.parse_args()
    sets = chosen_sets(args.sets)
    out = out_root(args.out)
    for combo in args.combos or list(COMBOS):
        build(combo, sets, out)


if __name__ == "__main__":
    main()
