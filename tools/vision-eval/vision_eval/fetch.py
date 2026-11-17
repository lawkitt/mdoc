"""Download and verify the pinned candidate files; convert seal detection to ONNX.

    python -m vision_eval.fetch [--platform macos-arm64|windows-x86_64|ios-arm64|all]
                                [--no-convert]

Files land in `$MDOC_VISION_MODELS` (default `~/.cache/mdoc-vision-eval/models`).
A file already present with the pinned size and digest is kept. Nothing is
uploaded anywhere.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import platform
import subprocess
import sys
import urllib.request
from pathlib import Path

from .pins import CONVERSIONS, LAYOUT_LABELS, PINS


def models_dir() -> Path:
    return Path(os.environ.get("MDOC_VISION_MODELS", "~/.cache/mdoc-vision-eval/models")).expanduser()


def host_platform() -> str:
    machine = platform.machine().lower()
    if sys.platform == "darwin" and machine == "arm64":
        return "macos-arm64"
    if sys.platform == "win32" and machine in ("amd64", "x86_64"):
        return "windows-x86_64"
    return f"{sys.platform}-{machine}"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def fetch(want: str) -> None:
    root = models_dir()
    for pin in PINS:
        if pin.platform not in ("any", want) and want != "all":
            continue
        dest = root / pin.path
        if dest.exists() and dest.stat().st_size == pin.size and sha256(dest) == pin.sha256:
            print(f"ok      {pin.path}")
            continue
        dest.parent.mkdir(parents=True, exist_ok=True)
        part = dest.with_name(dest.name + ".part")
        urllib.request.urlretrieve(pin.url, part)
        size, digest = part.stat().st_size, sha256(part)
        if size != pin.size or digest != pin.sha256:
            part.unlink()
            raise SystemExit(f"{pin.path}: got {size} bytes {digest}, pinned {pin.size} {pin.sha256}")
        part.replace(dest)
        print(f"fetched {pin.path}")
    check_layout_labels(root)


def check_layout_labels(root: Path) -> None:
    for yml in ("paddle/PP-DocLayout_plus-L/inference.yml", "paddle/PP-DocLayout_plus-L_onnx/inference.yml"):
        lines = (root / yml).read_text(encoding="utf-8").splitlines()
        start = lines.index("label_list:") + 1
        labels = []
        for line in lines[start:]:
            if not line.startswith("- "):
                break
            labels.append(line[2:].strip())
        if labels != LAYOUT_LABELS:
            raise SystemExit(f"{yml}: label_list differs from LAYOUT_LABELS: {labels}")
    print("ok      PP-DocLayout_plus-L label_list matches the official inference.yml")


def convert() -> None:
    root = models_dir()
    for out, (src, expected) in CONVERSIONS.items():
        dest = root / out
        if not dest.exists():
            dest.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run(
                [sys.executable, "-m", "paddle2onnx.command",
                 "--model_dir", str(root / src),
                 "--model_filename", "inference.json",
                 "--params_filename", "inference.pdiparams",
                 "--save_file", str(dest),
                 "--opset_version", "16",
                 "--enable_onnx_checker", "True"],
                check=True,
            )
        digest = sha256(dest)
        note = "matches the macOS arm64 build" if digest == expected else f"differs from the macOS arm64 build {expected}"
        print(f"onnx    {out} {digest} ({note})")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--platform", default=host_platform())
    ap.add_argument("--no-convert", action="store_true")
    args = ap.parse_args()
    fetch(args.platform)
    if not args.no_convert:
        convert()


if __name__ == "__main__":
    main()
