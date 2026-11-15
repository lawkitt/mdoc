"""CPU time per 10,000 characters and peak memory of an ONNX build.

    python -m detection_eval.time_onnx <model-id> --onnx <model.onnx>
        [--lang en|ru] [--threads 4] [--repeat 3]

The text is the public recognition fixtures of one language, joined in
sorted order and cut at the last space before 10,000 characters (never a
private set). Each measurement runs in a fresh process on ONNX Runtime's CPU
provider, as the app does (4 intra-op threads, its default): engine load,
the first scan (cold), then `--repeat` scans (warm, the median reported).
Peak memory is the process's peak working set (peak RSS off Windows) and
its rise over the process before the model loaded. Prints one JSON line.

GLiNER2 builds run in the app's engine instead (gliner2-rs).
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time

CHARS = 10_000


def sample(lang: str) -> str:
    from . import fixtures

    root = fixtures.sets()["fixtures"] / lang
    parts, size = [], 0
    for path in fixtures.fixture_paths(root):
        source = fixtures.source_of(path)
        parts.append(source)
        size += len(source) + 2
        if size > CHARS:
            break
    text = "\n\n".join(parts)
    if len(text) > CHARS:
        text = text[: text.rfind(" ", 0, CHARS)]
    return text


def peak_bytes() -> int:
    import psutil

    info = psutil.Process().memory_info()
    peak = getattr(info, "peak_wset", None)
    if peak is not None:
        return int(peak)
    import resource

    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(rss if sys.platform == "darwin" else rss * 1024)


def child(model_id: str, onnx: str, lang: str, threads: int, repeat: int) -> dict:
    from .adapters import load_onnx
    from .models import BY_ID
    from .windows import bounded_windows

    text = sample(lang)
    import psutil

    before = psutil.Process().memory_info().rss
    started = time.perf_counter()
    adapter = load_onnx(BY_ID[model_id], onnx, threads)
    load = time.perf_counter() - started

    def scan() -> float:
        t = time.perf_counter()
        for start, end in bounded_windows(text, adapter.fits):
            adapter.predict(text[start:end])
        return time.perf_counter() - t

    cold = scan()
    warm = [scan() for _ in range(repeat)]
    peak = peak_bytes()
    scale = CHARS / len(text)
    return {
        "model": model_id,
        "onnx": onnx,
        "lang": lang,
        "threads": threads,
        "chars": len(text),
        "load_s": round(load, 2),
        "cold_s_per_10k": round(cold * scale, 2),
        "warm_s_per_10k": round(statistics.median(warm) * scale, 2),
        "peak_mb": round(peak / 2**20),
        "peak_rise_mb": round((peak - before) / 2**20),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("model")
    parser.add_argument("--onnx", required=True)
    parser.add_argument("--lang", default="en")
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--child", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.child:
        print(json.dumps(child(args.model, args.onnx, args.lang, args.threads, args.repeat)))
        return
    result = subprocess.run(
        [sys.executable, "-m", "detection_eval.time_onnx", args.model, "--onnx", args.onnx,
         "--lang", args.lang, "--threads", str(args.threads), "--repeat", str(args.repeat), "--child"],
        check=True, capture_output=True, text=True,
    )
    print(result.stdout.strip().splitlines()[-1])


if __name__ == "__main__":
    main()
