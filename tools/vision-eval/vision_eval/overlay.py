"""Synthetic seal, signature and code overlays on the public fixture pages.

    python -m vision_eval.overlay <out-dir> [--variants 12] [--seed 911]

Writes a page set (set.json, pages/*.png, annotations/*.json) of the two
`ocr-qualification` pages with fictional round seals (ring text plus centre
lines), script signatures with pen flourishes, QR codes and Code 128
barcodes, placed over and beside the printed text, then lightly degraded like
a scan (blur, noise, JPEG). Every name and number is invented. Fonts are the
macOS system fonts, so the set is generated on a Mac and stored with the
private sets; other lanes use the stored pages.
"""
from __future__ import annotations

import argparse
import io
import json
import math
import random
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

from .pages import render_fixture_pages

FONTS = "/System/Library/Fonts/Supplemental"
SEAL_FONTS = [f"{FONTS}/Arial Bold.ttf", f"{FONTS}/Times New Roman Bold.ttf", f"{FONTS}/Arial Narrow Bold.ttf"]
SCRIPT_FONTS = [f"{FONTS}/Apple Chancery.ttf", f"{FONTS}/Brush Script.ttf", f"{FONTS}/Bradley Hand Bold.ttf",
                "/System/Library/Fonts/Supplemental/SnellRoundhand.ttc", f"{FONTS}/Savoye LET.ttc"]
INKS_SEAL = [(38, 58, 160), (70, 50, 150), (30, 80, 170), (110, 40, 120)]
INKS_PEN = [(20, 30, 120), (15, 15, 25), (30, 50, 150)]

RU_ORGS = ["ОБЩЕСТВО С ОГРАНИЧЕННОЙ ОТВЕТСТВЕННОСТЬЮ «СЕВЕРНЫЙ ВЕТЕР»",
           "АКЦИОНЕРНОЕ ОБЩЕСТВО «ТЕХНОСТРОЙ-М»",
           "ГОСУДАРСТВЕННОЕ БЮДЖЕТНОЕ УЧРЕЖДЕНИЕ «ГОРОДСКАЯ БОЛЬНИЦА № 7»",
           "ООО «ЛУГОВАЯ РОМАШКА»", "МУНИЦИПАЛЬНОЕ КАЗЁННОЕ УЧРЕЖДЕНИЕ «ЦЕНТР ЗАКУПОК»"]
RU_CITIES = ["г. Москва", "г. Казань", "г. Тверь", "г. Самара", "г. Пермь"]
EN_ORGS = ["NORTHWIND LOGISTICS, INC.", "BLUE HERON CAPITAL LLC", "EXAMPLE HOLDINGS CORPORATION",
           "MAPLE & STONE PARTNERS LP", "RIVERBEND MEDICAL GROUP, INC."]
EN_PLACES = ["DELAWARE", "NEVADA", "NEW YORK", "TEXAS", "OHIO"]
RU_SIGNERS = ["Иванов", "Petrova", "Сидоренко", "Kuznetsov", "Морозова"]
EN_SIGNERS = ["J. Hartwell", "Maria L. Ortega", "Samuel Pike", "A. Whitmore", "Dana K. Reyes"]


def _font(path: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(path, size)


def seal(rng: random.Random, lang: str) -> tuple[Image.Image, list[str]]:
    """A round seal on a transparent canvas and its text segments."""
    d = rng.randint(230, 300)
    pad = 12
    size = d + 2 * pad
    img = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(img)
    c = size / 2
    R = d / 2
    draw.ellipse([c - R, c - R, c + R, c + R], outline=255, width=rng.randint(3, 5))
    r_in = R * 0.72
    draw.ellipse([c - r_in, c - r_in, c + r_in, c + r_in], outline=255, width=2)
    font_path = rng.choice(SEAL_FONTS)
    if lang == "ru":
        org = rng.choice(RU_ORGS)
        inn = "".join(str(rng.randint(0, 9)) for _ in range(10))
        ogrn = "".join(str(rng.randint(0, 9)) for _ in range(13))
        ring = org
        centre = [f"ИНН {inn}", f"ОГРН {ogrn}", rng.choice(RU_CITIES)]
        if rng.random() < 0.4:
            centre = [f"ИНН {inn}", rng.choice(RU_CITIES)]
    else:
        org = rng.choice(EN_ORGS)
        ring = f"{org} • CORPORATE SEAL"
        centre = [str(rng.randint(1990, 2024)), rng.choice(EN_PLACES)]
    # Ring text, clockwise from the left, letter tops outward.
    r_text = (R + r_in) / 2
    band = R - r_in
    fsize = int(band * 0.62)
    font = _font(font_path, fsize)
    widths = [font.getlength(ch) for ch in ring]
    total = sum(widths) * 1.08
    sweep = min(340.0, math.degrees(total / r_text))
    scale = math.radians(sweep) * r_text / total if total else 1
    if sweep >= 340.0:  # long names: shrink letters to fit the ring
        font = _font(font_path, max(10, int(fsize * scale)))
        widths = [font.getlength(ch) for ch in ring]
        total = sum(widths) * 1.08
        sweep = math.degrees(total / r_text)
    angle = -90 - sweep / 2  # centred on the top
    for ch, wch in zip(ring, widths):
        step = math.degrees(wch * 1.08 / r_text)
        a = math.radians(angle + step / 2)
        glyph = Image.new("L", (fsize * 2, fsize * 2), 0)
        ImageDraw.Draw(glyph).text((fsize, fsize), ch, font=font, fill=255, anchor="mm")
        glyph = glyph.rotate(-(angle + step / 2 + 90), resample=Image.BICUBIC)
        x = c + r_text * math.cos(a) - fsize
        y = c + r_text * math.sin(a) - fsize
        img.paste(255, (int(x), int(y)), glyph)
        angle += step
    # Centre lines.
    lines = centre
    lfont = _font(font_path, int(r_in * 0.2))
    heights = lfont.getbbox("Hg")[3] * 1.15
    top = c - heights * len(lines) / 2 + heights / 2
    for i, line in enumerate(lines):
        f = lfont
        while f.getlength(line) > r_in * 1.7 and f.size > 8:
            f = _font(font_path, f.size - 1)
        draw.text((c, top + i * heights), line, font=f, fill=255, anchor="mm")
    img = img.rotate(rng.uniform(-25, 25), resample=Image.BICUBIC)
    return img, [ring] + lines


def signature(rng: random.Random, lang: str) -> Image.Image:
    name = rng.choice(RU_SIGNERS if lang == "ru" else EN_SIGNERS)
    font = _font(rng.choice(SCRIPT_FONTS), rng.randint(44, 72))
    w = int(font.getlength(name)) + 80
    h = int(font.size * 2.2)
    img = Image.new("L", (w, h), 0)
    draw = ImageDraw.Draw(img)
    draw.text((40, h / 2), name, font=font, fill=255, anchor="lm")
    # A flourish under the name: a jittered cubic Bezier.
    p = [(30, h * 0.75), (w * 0.3, h * rng.uniform(0.85, 1.0)), (w * 0.7, h * rng.uniform(0.4, 0.7)),
         (w - 20, h * rng.uniform(0.55, 0.8))]
    pts = []
    for i in range(60):
        t = i / 59
        x = sum(c[0] * k for c, k in zip(p, [(1 - t) ** 3, 3 * t * (1 - t) ** 2, 3 * t * t * (1 - t), t ** 3]))
        y = sum(c[1] * k for c, k in zip(p, [(1 - t) ** 3, 3 * t * (1 - t) ** 2, 3 * t * t * (1 - t), t ** 3]))
        pts.append((x + rng.uniform(-1, 1), y + rng.uniform(-1, 1)))
    draw.line(pts, fill=255, width=rng.randint(2, 3))
    img = img.transform(img.size, Image.AFFINE, (1, rng.uniform(-0.25, 0.1), 0, 0, 1, 0), resample=Image.BICUBIC)
    return img.rotate(rng.uniform(-8, 8), expand=True, resample=Image.BICUBIC)


def qr(rng: random.Random, lang: str) -> tuple[Image.Image, str]:
    import segno
    text = (f"https://example.gov.test/contract/{rng.randint(10**9, 10**10)}" if lang == "ru"
            else f"https://filings.example.test/exhibit/{rng.randint(10**6, 10**7)}")
    buf = io.BytesIO()
    segno.make(text, error="m").save(buf, kind="png", scale=rng.choice([3, 4]), border=0)
    return Image.open(buf).convert("L"), text


def code128(rng: random.Random) -> tuple[Image.Image, str]:
    import barcode
    from barcode.writer import ImageWriter
    text = f"DOC-{rng.randint(2020, 2026)}-{rng.randint(10000, 99999)}"
    buf = io.BytesIO()
    barcode.get("code128", text, writer=ImageWriter()).write(
        buf, {"module_width": 0.3, "module_height": 10, "quiet_zone": 1, "write_text": False, "dpi": 150})
    return Image.open(buf).convert("L"), text


def _ink(page: np.ndarray, mask: Image.Image, xy: tuple[int, int], color, rng: random.Random, dropout: float):
    """Multiplies coloured ink onto the page; returns the ink's tight box."""
    m = np.asarray(mask, np.float32) / 255.0
    if dropout:
        noise = np.asarray(Image.fromarray((np.random.default_rng(rng.randint(0, 2**31)).random(m.shape) * 255)
                                           .astype(np.uint8)).filter(ImageFilter.GaussianBlur(2)), np.float32) / 255.0
        m = m * np.clip((noise - dropout * 0.5) * 3, 0.15, 1.0)
    m *= rng.uniform(0.75, 0.95)
    x, y = xy
    h, w = m.shape
    H, W = page.shape[:2]
    x1, y1 = min(W, x + w), min(H, y + h)
    m = m[: y1 - y, : x1 - x]
    region = page[y:y1, x:x1].astype(np.float32)
    ink = np.array(color, np.float32) / 255.0
    region *= (1 - m[..., None]) + m[..., None] * ink
    page[y:y1, x:x1] = region.astype(np.uint8)
    ys, xs = np.nonzero(m > 0.2)
    return [float(x + xs.min()), float(y + ys.min()), float(x + xs.max() + 1), float(y + ys.max() + 1)]


def _paste_code(page: np.ndarray, img: Image.Image, xy: tuple[int, int]) -> list[float]:
    a = np.asarray(img, np.uint8)
    x, y = xy
    h, w = a.shape
    page[y:y + h, x:x + w] = np.minimum(page[y:y + h, x:x + w], a[..., None])
    return [float(x), float(y), float(x + w), float(y + h)]


def degrade(img: Image.Image, rng: random.Random) -> Image.Image:
    if rng.random() < 0.3:  # many real scans are grey
        img = img.convert("L").convert("RGB")
    img = img.filter(ImageFilter.GaussianBlur(rng.uniform(0.3, 0.9)))
    a = np.asarray(img, np.float32)
    a += np.random.default_rng(rng.randint(0, 2**31)).normal(0, rng.uniform(2, 7), a.shape)
    img = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))
    buf = io.BytesIO()
    img.save(buf, "JPEG", quality=rng.randint(55, 85))
    return Image.open(buf).convert("RGB")


def generate(out: Path, variants: int, seed: int) -> None:
    rng = random.Random(seed)
    (out / "pages").mkdir(parents=True, exist_ok=True)
    (out / "annotations").mkdir(parents=True, exist_ok=True)
    pages = []
    for base in render_fixture_pages():
        lang = base["lang"]
        for v in range(variants):
            page = np.asarray(base["image"].convert("RGB")).copy()
            H, W = page.shape[:2]
            arts = []
            # Text occupies roughly y 130–590 at 150 DPI; the lower page is blank.
            n_seals = [1, 1, 2, 0][v % 4]
            n_sigs = [1, 2, 1, 1][v % 4]
            for i in range(n_sigs):
                img = signature(rng, lang)
                over_text = (v + i) % 3 == 0
                y = rng.randint(380, 520) if over_text else rng.randint(700, 1500)
                x = rng.randint(80, max(81, W - img.width - 80))
                box = _ink(page, img, (x, y), rng.choice(INKS_PEN), rng, 0.0)
                arts.append({"class": "signature", "box": box})
            for i in range(n_seals):
                img, segs = seal(rng, lang)
                over_text = (v + i) % 2 == 0
                y = rng.randint(150, 420) if over_text else rng.randint(650, 1400)
                x = rng.randint(80, W - img.width - 60)
                box = _ink(page, img, (x, y), rng.choice(INKS_SEAL), rng, rng.choice([0.0, 0.3, 0.6]))
                arts.append({"class": "seal", "box": box, "segments": segs, "text": " ".join(segs)})
            if v % 3 == 1:
                img, text = qr(rng, lang)
                box = _paste_code(page, img, (rng.randint(800, W - img.width - 40), rng.randint(1450, H - img.height - 60)))
                arts.append({"class": "qr_code", "box": box, "text": text})
            if v % 3 == 2:
                img, text = code128(rng)
                box = _paste_code(page, img, (rng.randint(60, 400), rng.randint(1500, H - img.height - 60)))
                arts.append({"class": "barcode", "box": box, "text": text})
            image = degrade(Image.fromarray(page), rng)
            pid = f"{lang}-overlay-{v + 1:02d}"
            image.save(out / "pages" / f"{pid}.png")
            ann = {"id": pid, "width": W, "height": H, "lang": lang, "source": f"fixture:{base['source']}",
                   "artifacts": arts, "text": base["text"]}
            (out / "annotations" / f"{pid}.json").write_text(json.dumps(ann, ensure_ascii=False, indent=1) + "\n",
                                                            encoding="utf-8")
            pages.append({"id": pid, "image": f"pages/{pid}.png", "lang": lang,
                          "annotation": f"annotations/{pid}.json"})
    (out / "set.json").write_text(json.dumps({"name": out.name, "synthetic": True, "seed": seed,
                                              "pages": pages}, indent=1) + "\n", encoding="utf-8")
    print(f"{len(pages)} pages in {out}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out", type=Path)
    ap.add_argument("--variants", type=int, default=12)
    ap.add_argument("--seed", type=int, default=911)
    args = ap.parse_args()
    generate(args.out, args.variants, args.seed)


if __name__ == "__main__":
    main()
