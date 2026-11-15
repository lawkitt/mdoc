"""Fixture sets, their sources and snapshots, as `just recognize` reads them.

A fixture is a Markdown file with optional front matter and gold marks
(`⟦CATEGORY|text⟧`). The model reads the document with the marks removed
(`source`); a snapshot (`<stem>.model.json` beside the fixture) lists the
model's raw entities in UTF-8 byte offsets of that source. This mirrors
`crates/mdoc-detect/src/recognition.rs` (`Fixture::parse`, `strip_gold`,
`fixture_paths`, `Snapshot::to_json`); `just recognize --snapshot` checks
every entity against the source, so a drift here fails loudly there.
"""

from __future__ import annotations

import json
import os
import shutil
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]

# The GLiNER2 label names the app maps to categories
# (`FAMILIES` in crates/mdoc-detect/src/lib.rs). Any other label is dropped
# by the pipeline, so adapters rename into these.
LABELS = (
    "person",
    "full_name",
    "organization",
    "email",
    "phone_number",
    "address",
    "passport_number",
    "national_id_number",
    "tax_id",
    "bank_account",
    "iban",
    "routing_number",
)

# The app's schema: one model pass per family (`FAMILIES`).
FAMILIES = (
    ("person", "full_name"),
    ("organization",),
    ("email", "phone_number", "address"),
    ("passport_number", "national_id_number", "tax_id"),
    ("bank_account", "iban", "routing_number"),
)


def sets() -> dict[str, Path]:
    """The fixture sets by name: the public fixtures, and the private holdout
    and real set when their environment variables name them."""
    found = {"fixtures": REPO / "tests" / "fixtures" / "recognition"}
    for name, var in (("holdout", "MDOC_HOLDOUT"), ("real", "MDOC_REAL_SET")):
        value = os.environ.get(var)
        if value:
            path = Path(value)
            if not path.is_dir():
                raise SystemExit(f"{var}={value} is not a folder")
            found[name] = path
    return found


def fixture_paths(root: Path) -> list[Path]:
    """Every `*.md` under `root`, sorted, skipping README.md and GUIDE.md."""
    return sorted(
        p
        for p in root.rglob("*.md")
        if p.is_file() and p.name not in ("README.md", "GUIDE.md")
    )


def strip_gold(annotated: str) -> str:
    """The document with its gold marks removed (`strip_gold`)."""
    out = []
    tail = annotated
    while True:
        open_ = tail.find("⟦")
        if open_ < 0:
            out.append(tail)
            return "".join(out)
        out.append(tail[:open_])
        mark = tail[open_ + 1 :]
        close = mark.find("⟧")
        if close < 0:
            raise ValueError("`⟦` without `⟧`")
        inner = mark[:close]
        _category, _, text = inner.partition("|")
        out.append(text)
        tail = mark[close + 1 :]


def source_of(path: Path) -> str:
    """The text the model reads: front matter and gold marks removed."""
    # newline="" keeps the bytes as they are: the app reads them unchanged.
    with open(path, encoding="utf-8", newline="") as f:
        text = f.read()
    body = text
    if text.startswith("---\n"):
        rest = text[4:]
        end = rest.find("\n---\n")
        if end < 0:
            raise ValueError(f"{path}: front matter has no closing `---` line")
        body = rest[end + 5 :]
    return strip_gold(body)


@dataclass
class Entity:
    """One raw model span, in character offsets of the source."""

    start: int
    end: int
    label: str
    score: float


def snapshot_path(fixture: Path) -> Path:
    return fixture.with_suffix(".model.json")


def write_snapshot(
    fixture: Path, source: str, model: str, threshold: float, entities: list[Entity]
) -> None:
    """Writes `<stem>.model.json` in the app's format: byte offsets, one
    entity per line, sorted by start."""
    # Character → UTF-8 byte offsets.
    byte_at = [0] * (len(source) + 1)
    total = 0
    for i, ch in enumerate(source):
        byte_at[i] = total
        total += len(ch.encode("utf-8"))
    byte_at[len(source)] = total
    rows = []
    seen = set()
    for e in sorted(entities, key=lambda e: (e.start, e.end, e.label, -e.score)):
        if not (0 <= e.start < e.end <= len(source)):
            continue
        key = (e.start, e.end, e.label)
        if key in seen:
            continue
        seen.add(key)
        rows.append(
            {
                "start": byte_at[e.start],
                "end": byte_at[e.end],
                "label": e.label,
                "score": round(float(e.score), 8),
                "text": source[e.start : e.end],
            }
        )
    out = (
        "{\n  \"model\": "
        + json.dumps(model)
        + ",\n  \"threshold\": "
        + json.dumps(threshold)
        + ",\n  \"entities\": ["
    )
    for i, row in enumerate(rows):
        out += ("\n    " if i == 0 else ",\n    ") + json.dumps(
            row, ensure_ascii=False, separators=(",", ":")
        )
    out += "]\n}\n" if not rows else "\n  ]\n}\n"
    snapshot_path(fixture).write_text(out, encoding="utf-8", newline="\n")


def read_snapshot(fixture: Path) -> dict:
    return json.loads(snapshot_path(fixture).read_text(encoding="utf-8"))


def mirror(root: Path, out: Path) -> list[tuple[Path, Path]]:
    """Copies every fixture of `root` under `out` at its relative path (so
    language and kind read the same) and returns (original, copy) pairs.
    `out` is expected empty (`staged`)."""
    pairs = []
    for path in fixture_paths(root):
        copy = out / path.relative_to(root)
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, copy)
        pairs.append((path, copy))
    return pairs


@contextmanager
def staged(target: Path) -> Iterator[Path]:
    """A fresh folder beside `target` that replaces it only if the block
    completes: a failed or partial run never mixes with a previous one, and
    fixtures removed since are not left behind."""
    partial = target.with_name(target.name + ".partial")
    if partial.exists():
        shutil.rmtree(partial)
    partial.mkdir(parents=True)
    yield partial
    if target.exists():
        shutil.rmtree(target)
    partial.rename(target)


def complete(folder: Path) -> bool:
    """A set folder a run finished (`staged` plus its `meta.json`)."""
    return (folder / "meta.json").is_file()
