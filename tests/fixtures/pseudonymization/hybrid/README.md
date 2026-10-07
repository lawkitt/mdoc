# Isolated hybrid detector research — 2026-10-07

The user confirmed Q14: evaluate a small structured-rule prototype alongside
GLiNER2 before choosing any production additions. This tooling is outside the
app dependency graph. Q15 remains open; the restoration/refactor feature has not
been implemented. Read the [measurements](../results/2026-10-07-hybrid/measurements.md)
and [OSS source review](../../../../docs/design/pii-oss-research.md).

## Corpus

`corpus.json` is original synthetic material dedicated to CC0-1.0, with no user
documents. It preserves 13 existing short fixtures verbatim and adds eight
calibration fixtures and 12 holdout fixtures, including four negative fixtures.
Gold ranges/categories require exact matches; invalid/OCR identifiers remain
sensitive gold. INN/SNILS values are illustrative/generated, not verified registry
records. Positive email/URL domains use `.invalid`.

The holdout was frozen before its first model run. These are researcher-authored
cases, not an independent corpus or general accuracy qualification. No rules
were retuned after reading its results. Existing long/model stress fixtures were
excluded; the separate rules stress is exactly 2 MiB with 20,000 occurrences.

## Reproduce

From the repo root, use the existing explicitly installed pinned models and
verified ONNX Runtime 1.27.0. No setup/download is performed by these commands.
Use a fresh output directory. `generate_hybrid_corpus.py` only generates fixtures;
Python performs no detection, inference or app work.

```sh
python3 tools/pseudonymization/generate_hybrid_corpus.py
cargo build --release --locked --manifest-path tools/pseudonymization/Cargo.toml
cargo build --release --locked --manifest-path tools/pseudonymization/gliner2/Cargo.toml --bin mdoc-qualify-gliner2-bounded
mkdir -p .qualification/hybrid-reproduction
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  tools/pseudonymization/target/release/mdoc-pseudonymization-qualification run \
  tools/pseudonymization/gliner2/target/release/mdoc-qualify-gliner2-bounded \
  gliner2-pii-fp16 .qualification/models \
  tests/fixtures/pseudonymization/hybrid/corpus.json \
  .qualification/hybrid-reproduction/model.json \
  "$HOME/Library/Application Support/mdoc/ocr/v1/libonnxruntime.1.27.0.dylib" \
  "describe CPU, RAM, OS and rustc; release CPU 4 threads; network denied" 0.5 600
tools/pseudonymization/target/release/hybrid \
  tests/fixtures/pseudonymization/hybrid/corpus.json \
  .qualification/hybrid-reproduction/model.json \
  .qualification/hybrid-reproduction/comparison.json
python3 tools/pseudonymization/summarize_hybrid.py .qualification/hybrid-reproduction
```

`summarize_hybrid.py` records the current local sources/binaries with the captured
results; compare those hashes with the retained run before claiming reproduction.
Its dated report text describes the retained macOS run. Change that description
when running on another machine. Linux/Windows and native app checks have not run.

Rule hits explicitly precede overlapping model hits. Heuristic rule scores are
never compared with model probabilities. Reject invalid offsets before sorting;
use ordered-neighbor overlap checks rather than a nested all-kept scan. This is
experimental detection policy, not an approved replacement for app policy.

## Narrow components

Email supports Unicode letter/number addresses and hidden Markdown source.
Phones require a plus prefix or a nearby explicit label and 10–15 digits.
INN/SNILS require a same-line label and checksum. RU accounts/BIC have only
format/context checks; IBAN has a seven-country length whitelist and mod-97.
No engine crate or model wrapper was adopted. Rules are small reimplementations
in the research tool using its existing `regex` dependency.

INN coefficients were checked against the
[published algorithm](https://isupport.softlab.ru/portal/Samples/sample.asp?Typ=&id=1);
the [FNS description](https://www.nalog.gov.ru/rn77/fl/interest/inn/) confirms
the organization's ten-digit identifier includes a control digit. SNILS/IBAN
cross-check sources and pinned inspected revisions are in the OSS review.
Checksums support conservative recognition; they do not determine whether a
damaged value is sensitive. Original model detections remain eligible when a
rule declines a value.
