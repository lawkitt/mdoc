"""Runs candidates over page sets.

    python -m vision_eval.run <candidate>... [--sets fixtures,overlays,...] [--out DIR]
    python -m vision_eval.run all

Candidates: ocr (the app's OCR models: page words and the time baseline),
mirage, layout, seal and seal-mobile (the seal reader on the layout run's
seal boxes; `seal-gt` and `seal-mobile-gt` read the annotated seal boxes
instead, to measure the reader alone), rxing. `all` runs every one, layout
before the seal readers.

Each candidate runs in its own process (`worker.py`) so its peak memory is
its own. Output: `<out>/runs/<set>/<name>/<page>.json` and `run.json`,
written beside the folder and moved into place only when complete. `<out>`
defaults to `MDOC_VISION_OUT`, else `~/.cache/mdoc-vision-eval/out`; it holds
private pages' outputs, so keep it outside the repository.
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from . import pages

# The server seal readers peak near 8 GB, so they run last: memory pressure
# right after them slowed the next candidate tenfold on the M4.
ORDER = ["ocr", "layout", "seal-mobile", "seal-mobile-gt", "mirage", "mirage-kw", "rxing", "seal", "seal-gt"]


def out_root(arg: str | None) -> Path:
    root = arg or os.environ.get("MDOC_VISION_OUT") or "~/.cache/mdoc-vision-eval/out"
    return Path(root).expanduser().resolve()


def run_one(name: str, set_name: str, set_dir: Path, out: Path) -> None:
    final = out / "runs" / set_name / name
    stage = final.with_name(name + ".partial")
    if stage.exists():
        shutil.rmtree(stage)
    candidate, extra = name, []
    if name.endswith("-gt"):
        candidate, extra = name[:-3], ["--regions", "gt"]
    elif name.startswith("seal"):
        layout = out / "runs" / set_name / "layout"
        if not (layout / "run.json").exists():
            raise SystemExit(f"{name} on {set_name} needs a complete layout run first")
        extra = ["--regions", str(layout)]
    subprocess.run([sys.executable, "-m", "vision_eval.worker", candidate, str(set_dir), str(stage)] + extra,
                   check=True, cwd=Path(__file__).resolve().parents[1])
    if final.exists():
        shutil.rmtree(final)
    stage.rename(final)
    print(f"{set_name}/{name}: done", flush=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("candidates", nargs="+")
    ap.add_argument("--sets", default=None)
    ap.add_argument("--out", default=None)
    ap.add_argument("--keep-going", action="store_true")
    args = ap.parse_args()
    out = out_root(args.out)
    found = pages.available(out)
    names = args.sets.split(",") if args.sets else list(found)
    missing = [n for n in names if n not in found]
    if missing:
        raise SystemExit(f"sets {missing} not found (MDOC_VISION_SETS or MDOC_HOLDOUT)")
    wanted = ORDER if args.candidates == ["all"] else args.candidates
    unknown = [c for c in wanted if c not in ORDER]
    if unknown:
        raise SystemExit(f"unknown candidates {unknown}; choose from {ORDER}")
    wanted = [c for c in ORDER if c in wanted]
    for set_name in names:
        for name in wanted:
            try:
                run_one(name, set_name, found[set_name], out)
            except (subprocess.CalledProcessError, SystemExit) as e:
                if not args.keep_going:
                    raise
                print(f"{set_name}/{name}: FAILED ({e})", flush=True)


if __name__ == "__main__":
    main()
