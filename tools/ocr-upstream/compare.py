#!/usr/bin/env python3
"""Compare immutable corpus captures; scoring uses only the standard library."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import unicodedata


def normalize(text):
    text = unicodedata.normalize("NFKC", text).replace("\u00ad", "")
    text = re.sub(r"(\w)-\s*\n(\w)", r"\1\2", text)
    text = re.sub(r"^\s*(?:#+\s*|[=*_-]{3,}\s*$)", "", text, flags=re.MULTILINE)
    text = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", text)
    text = text.replace("**", "").replace("__", "")
    return " ".join(text.split())


def distance(reference, candidate):
    """Exact Levenshtein distance using bit-parallel Myers, for chars or words."""
    if not reference:
        return len(candidate)
    masks = {}
    for i, token in enumerate(reference):
        masks[token] = masks.get(token, 0) | (1 << i)
    positive, negative, score = (1 << len(reference)) - 1, 0, len(reference)
    top = 1 << (len(reference) - 1)
    for token in candidate:
        equal = masks.get(token, 0)
        vertical = equal | negative
        horizontal = (((equal & positive) + positive) ^ positive) | equal | negative
        plus = negative | ~(horizontal | positive)
        minus = positive & horizontal
        score += bool(plus & top) - bool(minus & top)
        plus = (plus << 1) | 1
        minus <<= 1
        positive = minus | ~(vertical | plus)
        negative = plus & horizontal
    return score


def rates(reference, candidate):
    a, b = normalize(reference), normalize(candidate)
    words = a.split()
    if not a or not words:
        raise ValueError("empty reference cannot establish CER/WER")
    return {
        "cer": round(distance(a, b) / len(a), 6),
        "wer": round(distance(words, b.split()) / len(words), 6),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("updated", type=Path)
    parser.add_argument("--corpus", type=Path,
                        default=Path(__file__).resolve().parents[2] / "tests/fixtures/ocr-upstream")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads((args.corpus / "manifest.json").read_text())
    report = {"baseline_fork": manifest["baseline_fork"], "upstream": manifest["upstream"],
              "settings": manifest["settings"], "comparisons": []}
    changes = []
    for fixture in manifest["fixtures"]:
        paths = [(fixture["id"], fixture["pdf"], fixture["pdf_sha256"], False)]
        if "native_pdf" in fixture:
            paths.append((fixture["id"] + ".native", fixture["native_pdf"],
                          fixture["native_pdf_sha256"], True))
        reference = (args.corpus / fixture["reference_file"]).read_text()
        if hashlib.sha256(reference.encode()).hexdigest() != fixture["reference_sha256"]:
            raise ValueError(f"reference digest changed: {fixture['id']}")
        for stem, pdf, expected_sha, native in paths:
            if hashlib.sha256((args.corpus / pdf).read_bytes()).hexdigest() != expected_sha:
                raise ValueError(f"source digest changed: {pdf}")
            row = {"id": stem, "reference_basis": fixture["reference"], "outputs": {}}
            for suffix in ["prepared.md", "recognition.txt", "metadata.txt"]:
                name = f"{stem}.{suffix}"
                before_bytes, after_bytes = (directory.joinpath(name).read_bytes()
                                              for directory in [args.baseline, args.updated])
                before, after = before_bytes.decode(), after_bytes.decode()
                result = {"byte_identical": before_bytes == after_bytes,
                          "baseline_sha256": hashlib.sha256(before_bytes).hexdigest(),
                          "updated_sha256": hashlib.sha256(after_bytes).hexdigest()}
                if before_bytes != after_bytes:
                    changes.append(name)
                if suffix != "metadata.txt" and not (native and suffix == "recognition.txt"):
                    result["baseline"] = rates(reference, before)
                    result["updated"] = rates(reference, after)
                row["outputs"][suffix] = result
            report["comparisons"].append(row)
    synthetic = args.corpus.parent / "ocr-qualification"
    for language in ["english", "russian"]:
        reference = synthetic.joinpath(language + ".txt").read_text()
        row = {"id": language, "reference_basis": "project CC0 synthetic transcript", "outputs": {}}
        for suffix in ["prepared.md", "recognition.txt", "metadata.txt"]:
            before_bytes, after_bytes = (directory.joinpath(f"{language}.{suffix}").read_bytes()
                                          for directory in [args.baseline, args.updated])
            before, after = before_bytes.decode(), after_bytes.decode()
            result = {"byte_identical": before_bytes == after_bytes,
                      "baseline_sha256": hashlib.sha256(before_bytes).hexdigest(),
                      "updated_sha256": hashlib.sha256(after_bytes).hexdigest()}
            if before_bytes != after_bytes:
                changes.append(f"{language}.{suffix}")
            if suffix != "metadata.txt":
                result["baseline"], result["updated"] = rates(reference, before), rates(reference, after)
                if normalize(after) != normalize(reference):
                    changes.append(f"{language}.{suffix}: synthetic transcript mismatch")
            row["outputs"][suffix] = result
        report["comparisons"].append(row)
    report["changed_outputs"] = changes
    report["review_required"] = bool(changes)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{len(report['comparisons'])} captures; {len(changes)} changed outputs")
    # An output change requires individual review, even if average CER improves.
    raise SystemExit(1 if changes else 0)


if __name__ == "__main__":
    main()
