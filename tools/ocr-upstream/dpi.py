#!/usr/bin/env python3
"""OCR accuracy, time and peak memory per render resolution.

On the Mac it runs the `ocr_dpi` example once per page, resolution and
repeat under `/usr/bin/time -l`, in interleaved order. With `--runs-from` it
reads the `runs-<dpi>.tsv` tables iOS's OCRResolutionTests writes beside its
`<dpi>/` outputs instead (scripts/ios/ios-ocr-dpi.sh). Either way it scores
each page's output against the adjacent `.txt` reference with compare.py's
normalization and exact CER/WER, and counts whole reference lines and number
tokens found. Native `*.native.pdf` pages are skipped: they never reach OCR.
"""
import argparse
import csv
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys

from compare import normalize, rates

REPO = Path(__file__).resolve().parents[2]
CORPORA = [REPO / "tests/fixtures/ocr-upstream", REPO / "tests/fixtures/ocr-qualification"]
NUMBER = re.compile(r"\d[\d.,:/-]*\d|\d")
MIB = 2**20


def recall(reference, output):
    """Reference lines found whole, and number tokens found, in the output."""
    text = normalize(output)
    lines = [normalize(line) for line in reference.splitlines() if normalize(line)]
    pool = NUMBER.findall(text)
    wanted = NUMBER.findall(normalize(reference))
    found = 0
    for token in wanted:
        if token in pool:
            pool.remove(token)
            found += 1
    return {"lines": sum(line in text for line in lines), "lines_total": len(lines),
            "numbers": found, "numbers_total": len(wanted)}


def convert(binary, root, dpi, output, pdf):
    """One Mac conversion: seconds, max RSS and peak footprint in bytes."""
    done = subprocess.run(["/usr/bin/time", "-l", binary, root, str(dpi), output, pdf],
                          capture_output=True, text=True, check=False)
    if done.returncode:
        sys.exit(f"{pdf} at {dpi} dpi failed:\n{done.stderr}")
    return {"seconds": float(done.stdout.strip().split("\t")[1]),
            "max_rss": int(re.search(r"(\d+)\s+maximum resident set size", done.stderr)[1]),
            "peak_footprint": int(re.search(r"(\d+)\s+peak memory footprint", done.stderr)[1])}


def measure(args, pdfs):
    """Runs keyed by (page, dpi), with the folder holding each output."""
    runs, folders = {}, {}
    for repeat in range(args.runs):
        for pdf in pdfs:
            for dpi in args.dpi:
                output = args.out / str(dpi)
                output.mkdir(parents=True, exist_ok=True)
                measured = convert(args.binary, args.root, dpi, output, pdf)
                runs.setdefault((pdf.stem, dpi), []).append(measured)
                folders[(pdf.stem, dpi)] = output
                print(f"run {repeat + 1}/{args.runs} {pdf.stem} {dpi} dpi: "
                      f"{measured['seconds']:.2f} s, {measured['peak_footprint'] / MIB:.0f} MiB",
                      file=sys.stderr)
    return runs, folders


def read_runs(tables):
    """Runs from OCRResolutionTests' tables, keyed by (page, dpi)."""
    runs, folders = {}, {}
    for table in tables:
        with table.open() as rows:
            for row in csv.DictReader(rows, delimiter="\t"):
                key = (row["page"], int(row["dpi"]))
                runs.setdefault(key, []).append({
                    "seconds": float(row["seconds"]),
                    "peak_footprint": int(row["peak_footprint"]),
                    "start_footprint": int(row["start_footprint"])})
                folders[key] = table.parent / row["dpi"]
    return runs, folders


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="outputs and results.json")
    parser.add_argument("--root", type=Path,
                        help="throwaway OCR storage that `ocr_dpi --setup ROOT` filled")
    parser.add_argument("--binary", type=Path, default=REPO / "target/release/examples/ocr_dpi")
    parser.add_argument("--dpi", type=int, nargs="+", default=[150, 200, 300])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--runs-from", type=Path, nargs="+", metavar="TSV",
                        help="score iOS runs instead of converting here")
    parser.add_argument("--corpus", type=Path, nargs="+", default=CORPORA,
                        help="folders of PDFs, each with an adjacent .txt reference")
    args = parser.parse_args()
    pdfs = sorted(pdf for folder in args.corpus for pdf in folder.glob("*.pdf")
                  if not pdf.name.endswith(".native.pdf"))
    if args.runs_from:
        runs, folders = read_runs(args.runs_from)
    elif args.root:
        runs, folders = measure(args, pdfs)
    else:
        parser.error("give --root to convert here, or --runs-from")
    references = {pdf.stem: pdf.with_suffix(".txt") for pdf in pdfs}
    rows = []
    for (page, dpi), measured in sorted(runs.items()):
        reference = references[page].read_text()
        folder = folders[(page, dpi)]
        prepared = (folder / f"{page}.prepared.md").read_text()
        raw = (folder / f"{page}.recognition.txt").read_text()
        row = {"page": page, "dpi": dpi,
               "prepared": rates(reference, prepared), "raw": rates(reference, raw),
               "recall": recall(reference, prepared),
               "reference_chars": len(normalize(reference)), "runs": measured}
        for key in measured[0]:
            row[key] = statistics.median(m[key] for m in measured)
        rows.append(row)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")
    memory = "max RSS MiB" if "max_rss" in rows[0] else "start footprint MiB"
    print(f"| page | dpi | CER | WER | raw CER | lines | numbers | median s "
          f"| peak footprint MiB | {memory} |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for row in rows:
        found = row["recall"]
        other = row.get("max_rss", row.get("start_footprint", 0))
        print(f"| {row['page']} | {row['dpi']} | {row['prepared']['cer']:.4f} "
              f"| {row['prepared']['wer']:.4f} | {row['raw']['cer']:.4f} "
              f"| {found['lines']}/{found['lines_total']} "
              f"| {found['numbers']}/{found['numbers_total']} | {row['seconds']:.2f} "
              f"| {row['peak_footprint'] / MIB:.0f} | {other / MIB:.0f} |")


if __name__ == "__main__":
    main()
