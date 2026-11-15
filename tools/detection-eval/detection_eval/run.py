"""Writes snapshots for options over fixture sets.

    python -m detection_eval.run <model-id>... [--sets fixtures,holdout,real]
        [--out DIR] [--device cuda|cpu]
    python -m detection_eval.run default [--sets ...]
    python -m detection_eval.run <model-id> --onnx <model.onnx> --as <name>

Each set is mirrored to `<out>/<option>/<set>/` and every copy gets its
`<stem>.model.json` from the model. A set is written beside its folder and
replaces it only when complete (its `meta.json` marks that), so a failed
run leaves the previous one whole. `default` is the app's own detector (the
installed pinned FP16 through `just recognize --refresh-snapshots`): the
default that decision 7 compares against. `all` runs every model. `--onnx`
runs one model's ONNX build on ONNX Runtime's CPU provider instead, into
`<out>/<name>/` (`<model-id>.onnx-int8`, say): the parity re-score of a
build against its PyTorch snapshots.

`<out>` defaults to `MDOC_EVAL_OUT`, else `~/.cache/mdoc-detection-eval`;
it holds private documents when the holdout or real set is in, so keep it
outside the repository.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

from . import fixtures
from .models import BY_ID, MODELS
from .windows import bounded_windows


def out_root(arg: str | None) -> Path:
    """Absolute: `just recognize` runs from the repository root."""
    if arg:
        root = Path(arg)
    elif os.environ.get("MDOC_EVAL_OUT"):
        root = Path(os.environ["MDOC_EVAL_OUT"])
    else:
        root = Path.home() / ".cache" / "mdoc-detection-eval"
    return root.expanduser().resolve()


def chosen_sets(arg: str | None) -> dict[str, Path]:
    available = fixtures.sets()
    names = arg.split(",") if arg else list(available)
    missing = [n for n in names if n not in available]
    if missing:
        raise SystemExit(
            f"sets {missing} not available (set MDOC_HOLDOUT / MDOC_REAL_SET)"
        )
    return {n: available[n] for n in names}


def run_app(sets: dict[str, Path], out: Path) -> None:
    for name, root in sets.items():
        with fixtures.staged(out / "default" / name) as target:
            fixtures.mirror(root, target)
            started = time.time()
            subprocess.run(
                ["just", "recognize", "--refresh-snapshots", str(target)],
                cwd=fixtures.REPO,
                check=True,
                stdout=subprocess.DEVNULL,
            )
            write_meta(target, "default", time.time() - started, None)
        print(f"default {name}: {time.time() - started:.0f}s", flush=True)


def write_meta(target: Path, option: str, seconds: float, extra: dict | None) -> None:
    meta = {"option": option, "seconds": round(seconds, 1), **(extra or {})}
    (target / "meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")


def run_model(
    model_id: str, sets: dict[str, Path], out: Path, device: str,
    onnx: str | None = None, name: str | None = None,
) -> None:
    from .adapters import load, load_onnx

    model = BY_ID[model_id]
    loaded = time.time()
    adapter = load_onnx(model, onnx) if onnx else load(model, device)
    option = name or model.id
    load_seconds = time.time() - loaded
    snapshot_model = f"{option}@{model.revision[:8]}"
    for name, root in sets.items():
        with fixtures.staged(out / option / name) as target:
            started = time.time()
            chars = 0
            windows_run = 0
            for original, copy in fixtures.mirror(root, target):
                source = fixtures.source_of(original)
                chars += len(source)
                cache: dict[str, list[fixtures.Entity]] = {}
                entities: list[fixtures.Entity] = []
                for start, end in bounded_windows(source, adapter.fits):
                    text = source[start:end]
                    if text not in cache:
                        cache[text] = adapter.predict(text)
                        windows_run += 1
                    for e in cache[text]:
                        entities.append(
                            fixtures.Entity(start + e.start, start + e.end, e.label, e.score)
                        )
                fixtures.write_snapshot(copy, source, snapshot_model, model.cut, entities)
            seconds = time.time() - started
            write_meta(
                target,
                option,
                seconds,
                {"load_seconds": round(load_seconds, 1), "chars": chars, "windows": windows_run,
                 "device": "cpu (onnx)" if onnx else device},
            )
        print(f"{option} {name}: {seconds:.0f}s, {windows_run} windows", flush=True)
    del adapter
    try:
        import torch

        torch.cuda.empty_cache()
    except ImportError:
        pass


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("options", nargs="+", help="model ids, `default`, or `all`")
    parser.add_argument("--sets")
    parser.add_argument("--out")
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--onnx", help="run this ONNX build of the one model named")
    parser.add_argument("--as", dest="name", help="the option folder for an --onnx run")
    parser.add_argument(
        "--keep-going", action="store_true", help="report a model that fails to load or run, and go on"
    )
    args = parser.parse_args()
    sets = chosen_sets(args.sets)
    out = out_root(args.out)
    options = [m.id for m in MODELS] if args.options == ["all"] else args.options
    if args.onnx and (len(options) != 1 or not args.name):
        raise SystemExit("--onnx takes one model id and --as <name>")
    failed = []
    for option in options:
        if option != "default" and option not in BY_ID:
            raise SystemExit(f"unknown option {option}; one of default, {', '.join(BY_ID)}")
        try:
            if option == "default":
                run_app(sets, out)
            else:
                run_model(option, sets, out, args.device, args.onnx, args.name)
        except Exception as error:  # noqa: BLE001 - reported, then next model
            if not args.keep_going:
                raise
            failed.append((option, f"{type(error).__name__}: {error}"))
            print(f"FAILED {option}: {type(error).__name__}: {error}", file=sys.stderr, flush=True)
    for option, why in failed:
        print(f"failed: {option}: {why}")


if __name__ == "__main__":
    main()
