# Rust pseudonymization qualification

Developer evaluation tooling, outside mdoc's workspace and dependency graph;
it enables no product action. Read the
[qualification result](../../tests/fixtures/pseudonymization/README.md)
before choosing a detector. The tooling compares GLiNER-family models; it
does not qualify the app's experimental options (see
[app runtime verification](../../tests/fixtures/model-settings/README.md)).
The [hybrid evaluation](../../tests/fixtures/pseudonymization/hybrid/README.md)
adds a structured-rule comparison; no production detector selection has
been approved.

The shared crate handles explicit setup, artifact integrity, annotated
fixtures, exact scoring, measurements and process supervision; two small
adapters wrap existing Rust engines. Separate Cargo locks are needed:
`gline-rs` pins `ort` rc.9, while `gliner2-rs` and mdoc's OCR use rc.13. No
Python, Docker or model export is needed to reproduce inference.

## Build and check

From the repository root. Ordinary tests fetch no models and need no native
runtime.

```sh
cargo build --release --locked --manifest-path tools/pseudonymization/Cargo.toml
cargo build --release --locked --manifest-path tools/pseudonymization/gline/Cargo.toml
cargo build --release --locked --manifest-path tools/pseudonymization/gliner2/Cargo.toml
cargo test --locked --manifest-path tools/pseudonymization/Cargo.toml
```

`cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`
take the same three manifests; the `gliner2` one also with
`--features ocr-compat`.

## Setup

The candidates are `multi-v2.1-q8`, `pii-base-q8` and `gliner2-pii-fp16`,
pinned in `models.json`. Setup verifies every file before publishing it,
reuses verified files and never downloads document contents; inference
rejects an incomplete or modified installation.

```sh
tools/pseudonymization/target/release/mdoc-pseudonymization-qualification setup <candidate> .qualification/models
```

Models stay in the ignored `.qualification/` directory; clean up by hand.
`gline-rs` unavoidably enables its tokenizer's HTTP feature, so run that
candidate under a network block.

## Offline measurements

Use a fresh output filename. `run` records the child's exit status and
deadline apart from the inference report, so a crash after writing results
shows. Inference never runs setup or reaches the network. On Apple Silicon
macOS with mdoc's OCR set up:

```sh
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  tools/pseudonymization/target/release/mdoc-pseudonymization-qualification run \
  tools/pseudonymization/gline/target/release/mdoc-qualify-gline \
  multi-v2.1-q8 .qualification/models tests/fixtures/pseudonymization/corpus.json \
  .qualification/results/multi-t05.json \
  "$HOME/Library/Application Support/mdoc/ocr/v1/libonnxruntime.1.32.0.dylib" \
  "describe CPU, RAM, OS and rustc" 0.5 600
```

`pii-base-q8` uses the same adapter; `gliner2-pii-fp16` uses
`tools/pseudonymization/gliner2/target/release/mdoc-qualify-gliner2`. Repeat
at threshold `0.3` into a new file. Thresholds and label policies are
experiments, not product settings. On Windows use the `.exe` files, the
installed `onnxruntime.1.32.0.dll` and an external network block; Linux
needs a separately verified native runtime. Neither has been run.

Reading a report: peak RSS is the whole process's high-water mark,
including stress fixtures, not model size, idle memory or GPU memory. Each
short fixture runs three times in one loaded engine (determinism and warm
latency); long documents run once. Both adapters use 128-word regex windows
with 32-word overlap, GLiNER2 its library's chunker. Word windows are not a
subword budget: production needs a tokenizer-aware bound.

`summarize <report.json>...` prints the category, language and fixture
tables. The retained macOS run is no longer in the tree; restore it with
`git checkout f368e436cd2d3e70789d51879b9f54f379900e1e -- docs/evidence/pseudonymization/2026-10-01`
and pass its `*-t0.[35].json`.

A short deadline (say three seconds) checks that the child is killed and
reaped mid-inference; its `.process.json` records `deadline_exceeded: true`.

## Same-process OCR compatibility

The opt-in probe keeps the qualification's `pdf-inspector` pin and shares
`ort` rc.13 with GLiNER2. It checks EN/RU scanned-PDF transcripts before PII
load, with both engines loaded and after PII drop, compares the PDFs' hashes
and validates PII offsets. It downloads no OCR.

```sh
cargo build --release --locked --manifest-path tools/pseudonymization/gliner2/Cargo.toml \
  --features ocr-compat --bin mdoc-qualify-ocr-compat
env GLINER2_DEVICE=cpu tools/pseudonymization/gliner2/target/release/mdoc-qualify-ocr-compat \
  .qualification/models "$HOME/Library/Application Support/mdoc/ocr/v1" \
  tests/fixtures/ocr-qualification .qualification/results/ocr-coexistence.json
```

Run it under a network block. On Windows pass the installed OCR bundle
directory.

`provenance.json` records engine, wrapper and tokenizer revisions, licence
digests, model-card revisions and the tested runtime; `models.json` the
exported weights' and tokenizers' bytes. Shipping any model still needs
the applicable licences and notices and the open export-provenance
questions resolved.

## Restoration policy performance

`python3 tools/pseudonymization/benchmark_policy.py` builds an older policy
(its pinned commit) and the current app policy in isolated optimized
harnesses, from cached dependencies only, and writes timings and source
hashes under `.qualification/restoration/reproduced`. It loads no GPUI or
model. The retained
[report](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
has the geometry and storage commands and the evidence's limits.
