"""Combo snapshots: the union of member snapshots (decision 11).

    python -m detection_eval.combo [<combo-id>... | <a>+<b>[+<c>]...]
        [--sets fixtures,holdout,real] [--out DIR]

Every member's spans go to the pipeline. Spans of different members that
overlap (directly or through a chain) form a group; a group with spans of one
member only passes unchanged. A mixed group becomes one span over its
longest span (on a tie, the higher-ranked member's), with the category of
the highest-ranked member in the group: one entity per label that member
gave there, at that member's best score for it. Members run in sequence, so
a combo's peak memory is its largest member's.

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
    """Merged entities (byte offsets, without text) of ranked members."""
    items = [(rank, e) for rank, entities in enumerate(members) for e in entities]
    parent = list(range(len(items)))

    def find(i: int) -> int:
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    order = sorted(range(len(items)), key=lambda i: items[i][1]["start"])
    for a_pos, a in enumerate(order):
        ra, ea = items[a]
        for b in order[a_pos + 1 :]:
            rb, eb = items[b]
            if eb["start"] >= ea["end"]:
                break
            if ra != rb:
                parent[find(a)] = find(b)
    groups: dict[int, list[int]] = {}
    for i in range(len(items)):
        groups.setdefault(find(i), []).append(i)
    out = []
    for group in groups.values():
        ranks = {items[i][0] for i in group}
        if len(ranks) == 1:
            out.extend(items[i][1] for i in group)
            continue
        longest = min(
            group,
            key=lambda i: (
                -(items[i][1]["end"] - items[i][1]["start"]),
                items[i][0],
                items[i][1]["start"],
            ),
        )
        span = items[longest][1]
        top = min(ranks)
        best: dict[str, float] = {}
        for i in group:
            rank, e = items[i]
            if rank == top:
                best[e["label"]] = max(best.get(e["label"], 0.0), e["score"])
        for label, score in best.items():
            out.append({"start": span["start"], "end": span["end"], "label": label, "score": score})
    return out


def combo_members(combo: str) -> list[str]:
    return COMBOS.get(combo) or combo.split("+")


def build(combo: str, sets: dict[str, Path], out: Path) -> None:
    members = combo_members(combo)
    for name, root in sets.items():
        target = out / combo / name
        count = 0
        for original, copy in fixtures.mirror(root, target):
            member_entities = []
            thresholds = []
            for member in members:
                snap_path = fixtures.snapshot_path(out / member / name / original.relative_to(root))
                snapshot = json.loads(snap_path.read_text(encoding="utf-8"))
                member_entities.append(snapshot["entities"])
                thresholds.append(snapshot["threshold"])
            source = fixtures.source_of(original)
            data = source.encode("utf-8")
            merged = merge(member_entities)
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
                if e["start"] in char_at and e["end"] in char_at
                and data[e["start"] : e["end"]]
            ]
            fixtures.write_snapshot(copy, source, combo, min(thresholds), entities)
            count += 1
        write_meta(target, combo, 0.0, {"members": members})
        print(f"{combo} {name}: {count} snapshots", flush=True)


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
