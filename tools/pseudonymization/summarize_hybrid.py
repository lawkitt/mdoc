"""Summarize captured research results; never runs inference or modifies gold."""
import collections
import hashlib
import json
import statistics
import sys
from pathlib import Path


def totals(assessments):
    result = collections.Counter()
    for assessment in assessments:
        for counts in assessment["counts"].values():
            result.update(counts)
    return result


def cells(counts):
    return f"{counts['true_positives']} / {counts['false_positives']} / {counts['misses']}"


def main():
    directory = Path(sys.argv[1])
    report = json.loads((directory / "comparison.json").read_text())
    model = json.loads((directory / "model.json").read_text())
    fixtures = report["fixtures"]
    cohorts = {
        "Existing short corpus": [f for f in fixtures if not f["id"].startswith("hybrid-")],
        "Calibration": [f for f in fixtures if f["kind"] == "calibration"],
        "Holdout, including four negative fixtures": [f for f in fixtures if "holdout-" in f["id"]],
        "All": fixtures,
    }
    lines = [
        "# Hybrid PII evaluation — 2026-10-07", "",
        "Research only. No production detector or dependency changes are selected.", "",
        "## Exact scoring", "",
        "Each cell is **true positives / false positives / misses**. A hit requires",
        "an exact half-open UTF-8 source range and category; partial/category errors",
        "count as a false positive and a miss. Model outputs use the same research",
        "overlap resolver as the hybrid. This measures detection, before the app's",
        "source-syntax protection, exact-repeat expansion and replacement policy.", "",
        "| Cohort | Fixtures | Gold | GLiNER2 | Rules | Full hybrid | Selected hybrid |",
        "| --- | ---: | ---: | --- | --- | --- | --- |",
    ]
    for name, cohort in cohorts.items():
        counts = [totals(f["pipelines"][p]["assessment"] for f in cohort)
                  for p in ("model", "rules", "hybrid", "selected_hybrid")]
        lines.append(f"| {name} | {len(cohort)} | {counts[0]['expected']} | "
                     + " | ".join(cells(c) for c in counts) + " |")
    lines += ["", "Selected hybrid includes email, contextual checksum-valid INN and SNILS.",
              "It achieves the full hybrid's exact counts on every fixture, with no",
              "changes to the existing 13 fixtures. This is a small researcher-authored",
              "synthetic sample, not independent real-document qualification.", "",
              "## Individual rule ablations", "",
              "Each row adds only that recognizer to the captured GLiNER2 predictions.",
              "The model baseline is 79 / 40 / 27 across 106 expected mentions.", "",
              "| Rule | TP / FP / misses |", "| --- | --- |"]
    for label in fixtures[0]["individual_rule_ablations"]:
        counts = totals(f["individual_rule_ablations"][label] for f in fixtures)
        lines.append(f"| {label} | {cells(counts)} |")
    lines += ["", "## Category and language detail", ""]
    for dimension in ("language", "category"):
        lines += [f"| {dimension.capitalize()} | Gold | GLiNER2 | Selected hybrid |",
                  "| --- | ---: | --- | --- |"]
        keys = sorted({f["language"] for f in fixtures}) if dimension == "language" else sorted(fixtures[0]["pipelines"]["model"]["assessment"]["counts"])
        for key in keys:
            counts = []
            for pipeline in ("model", "selected_hybrid"):
                if dimension == "language":
                    count = totals(f["pipelines"][pipeline]["assessment"] for f in fixtures if f["language"] == key)
                else:
                    count = collections.Counter()
                    for f in fixtures:
                        count.update(f["pipelines"][pipeline]["assessment"]["counts"][key])
                counts.append(count)
            lines.append(f"| {key} | {counts[0]['expected']} | {cells(counts[0])} | {cells(counts[1])} |")
        lines.append("")
    stress = report["stress"]
    lines += ["## Timing and execution", "",
              "Apple M4, 24 GiB RAM, macOS 15.7.7, Rust 1.98.1; release CPU, four model threads.",
              f"- Cached regex construction: {report['rule_build_ms']:.3f} ms.",
              f"- Rules scan, median of 33 fixture medians (101 samples each): {statistics.median(f['rules_median_ms'] for f in fixtures):.6f} ms.",
              f"- Hybrid merge, median of one measurement per fixture: {statistics.median(f['merge_ms'] for f in fixtures):.6f} ms; too small for a firm budget.",
              f"- Rules stress: {stress['source_bytes']:,} bytes, {stress['occurrences']:,} matches, median {stress['median_ms']:.3f} ms over seven calls; includes overlap resolution and offset validation, excludes editor/model work.",
              f"- GLiNER2 artifact verification: {model['verify_ms']:.1f} ms; engine load: {model['load_ms']:.1f} ms; whole-process peak RSS: {model['peak_rss_bytes'] / 1048576:.1f} MiB.",
              "- Each short fixture ran three model calls in one loaded engine; all source offsets valid and outputs deterministic. Process exited successfully without deadline expiry under a macOS deny-network sandbox.",
              "- Adapter uses the current app's label families, threshold 0.5, FP16 and 512 actual schema/text-token bounded windows (128 words, 32 overlap). Engine reuse here differs from the app's scan lifecycle; these are not end-to-end app timings.", "",
              "## Limits and decision proposal", "",
              "Add only email + contextual checksum-valid INN/SNILS experimentally, pending Q15.",
              "Phone, IBAN and RU bank-format rules produced no additional exact hits.",
              "The selected hybrid still has 35 false positives and 19 misses overall.",
              "Rules had zero false positives on these fixtures, which does not establish general precision.", "",
              "Context only looks backward on the same line, up to 80 characters: table",
              "headers and labels such as 'ИНН физлица' are not recognized. Invalid/OCR",
              "identifiers remain sensitive gold; checksum rejection never suppresses a",
              "non-overlapping model detection. Historical SNILS formats, registry",
              "existence, broad email syntax and international phone formats remain",
              "unqualified. Larger representative EN/RU corpora and native editor/history",
              "performance need separate verification. Existing long/stress model fixtures",
              "were excluded from this detector comparison; only the rules stress ran.", "",
              "Holdout text and gold were frozen before first inference. No recognizer",
              "was retuned after inspecting holdout predictions. The stress padding was",
              "corrected from an oversized first run, then rerun at exactly 2 MiB; this",
              "did not change any corpus fixture or detection rule.", "",
              "## Raw evidence", "",
              "[Comparison](comparison.json), [model](model.json), [process](model.process.json),",
              "[stderr](model.stderr.log), [source and binary hashes](provenance.json).",
              "Every miss and false positive, including the four holdout negatives, is",
              "retained per fixture in the comparison JSON. See the [corpus and commands](../../hybrid/README.md).", ""]
    (directory / "measurements.md").write_text("\n".join(lines))
    root = Path(__file__).resolve().parents[2]
    sources = [
        "tools/pseudonymization/src/rules.rs", "tools/pseudonymization/src/bin/hybrid.rs",
        "tools/pseudonymization/src/lib.rs", "tools/pseudonymization/src/main.rs",
        "tools/pseudonymization/gliner2/src/bounded.rs", "tools/pseudonymization/gliner2/Cargo.toml",
        "tools/pseudonymization/Cargo.toml", "tools/pseudonymization/Cargo.lock",
        "tools/pseudonymization/gliner2/Cargo.lock", "tools/pseudonymization/generate_hybrid_corpus.py",
        "tools/pseudonymization/summarize_hybrid.py", "src/pseudonymization_detector.rs",
        "tests/fixtures/pseudonymization/corpus.json", "tests/fixtures/pseudonymization/hybrid/corpus.json",
        "tools/pseudonymization/target/release/hybrid",
        "tools/pseudonymization/target/release/mdoc-pseudonymization-qualification",
        "tools/pseudonymization/gliner2/target/release/mdoc-qualify-gliner2-bounded",
    ]
    sources += [str((directory / name).resolve().relative_to(root)) for name in
                ("comparison.json", "model.json", "model.process.json", "model.stderr.log")]
    hashes = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in sources}
    (directory / "provenance.json").write_text(json.dumps({
        "sha256": hashes, "model_configuration": model["configuration"],
        "model_revision": model["model"]["revision"], "runtime_sha256": model["runtime_sha256"],
        "validation": ["tooling fmt checks", "tooling clippy -D warnings", "14 tests passed; 1 model integration test ignored", "33 model fixtures: valid offsets, deterministic, successful process; deny network", "rules stress: exact 2 MiB and 20,000 matches"],
        "note": "No production changes or app/native UI checks. Model artifact hashes are in model.json. Binary files are ignored build outputs; source hashes identify this research snapshot.",
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
