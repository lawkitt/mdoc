"""Page sets: the public fixtures, and private sets stored as rendered pages.

A set is a folder with `set.json` ({"name", "pages": [{"id", "image",
"lang", "annotation"}]}), page images (PNG, RGB) and one annotation per page:

    {"id", "width", "height", "lang",
     "artifacts": [{"class", "box": [x0, y0, x1, y1] in pixels,
                    "text" (seal, QR code, barcode), "segments" (seal)}],
     "text": optional ground-truth page text}

Private sets are rendered once and stored with their annotations in the
holdout repository, so every lane reads the same pixels. `fixtures` is built
from the public `tests/fixtures/ocr-qualification` PDFs at 150 DPI.

    python -m vision_eval.pages fixtures <out-dir>   # write the fixtures set
    python -m vision_eval.pages digests <set-dir>     # RGB SHA-256 per page
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parents[3]
FIXTURES = REPO / "tests" / "fixtures" / "ocr-qualification"
DPI = 150


def render_fixture_pages() -> list[dict]:
    import pypdfium2 as pdfium
    out = []
    for name, lang in (("english", "en"), ("russian", "ru")):
        pdf = pdfium.PdfDocument(str(FIXTURES / f"{name}.pdf"))
        for i in range(len(pdf)):
            image = pdf[i].render(scale=DPI / 72).to_pil().convert("RGB")
            out.append({"id": f"{lang}-fixture-{i + 1}", "image": image, "lang": lang,
                        "source": f"{name}.pdf#{i + 1}",
                        "text": (FIXTURES / f"{name}.txt").read_text(encoding="utf-8")})
    return out


def write_fixtures(out: Path) -> Path:
    (out / "pages").mkdir(parents=True, exist_ok=True)
    (out / "annotations").mkdir(parents=True, exist_ok=True)
    pages = []
    for p in render_fixture_pages():
        p["image"].save(out / "pages" / f"{p['id']}.png")
        ann = {"id": p["id"], "width": p["image"].width, "height": p["image"].height, "lang": p["lang"],
               "source": p["source"], "artifacts": [], "text": p["text"]}
        (out / "annotations" / f"{p['id']}.json").write_text(json.dumps(ann, ensure_ascii=False, indent=1) + "\n",
                                                            encoding="utf-8")
        pages.append({"id": p["id"], "image": f"pages/{p['id']}.png", "lang": p["lang"],
                      "annotation": f"annotations/{p['id']}.json"})
    (out / "set.json").write_text(json.dumps({"name": "fixtures", "pages": pages}, indent=1) + "\n", encoding="utf-8")
    return out


def load(set_dir: Path) -> dict:
    meta = json.loads((set_dir / "set.json").read_text(encoding="utf-8"))
    for p in meta["pages"]:
        p["path"] = set_dir / p["image"]
        ann = set_dir / p["annotation"] if p.get("annotation") else None
        p["truth"] = json.loads(ann.read_text(encoding="utf-8")) if ann and ann.exists() else None
    meta["dir"] = set_dir
    return meta


def sets_root() -> Path | None:
    if os.environ.get("MDOC_VISION_SETS"):
        return Path(os.environ["MDOC_VISION_SETS"]).expanduser()
    if os.environ.get("MDOC_HOLDOUT"):
        return Path(os.environ["MDOC_HOLDOUT"]).expanduser() / "vision-eval"
    return None


def available(out_root: Path) -> dict[str, Path]:
    """Set name → folder: `fixtures` (written under the output root when
    missing) and every set folder under the private sets root."""
    found = {}
    fixtures = out_root / "sets" / "fixtures"
    if not (fixtures / "set.json").exists():
        write_fixtures(fixtures)
    found["fixtures"] = fixtures
    root = sets_root()
    if root and root.exists():
        for d in sorted(root.iterdir()):
            if (d / "set.json").exists():
                found[d.name] = d
    return found


def rgb_digest(path: Path) -> str:
    with Image.open(path) as im:
        return hashlib.sha256(im.convert("RGB").tobytes()).hexdigest()


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "fixtures":
        print(write_fixtures(Path(sys.argv[2])))
    elif len(sys.argv) == 3 and sys.argv[1] == "digests":
        meta = load(Path(sys.argv[2]))
        for p in meta["pages"]:
            print(p["id"], rgb_digest(p["path"]))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
