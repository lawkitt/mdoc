# Rust pseudonymization qualification

This is developer evaluation tooling, outside mdoc's workspace and dependency
graph. It does not enable a product action. Read the
[dated qualification result](../../tests/fixtures/pseudonymization/README.md)
before choosing a detector.

The shared crate handles explicit setup, artifact integrity, annotated fixtures,
exact scoring, measurements, and process supervision. Two small adapters use
existing Rust engines. Separate Cargo locks are necessary: `gline-rs` pins
`ort` rc.9, while `gliner2-rs` and mdoc's OCR use rc.13. No Python, Docker, or
model-export step is required to reproduce inference.

## Build and check

Run from the repository root. Model setup and inference are opt-in; ordinary
tests do not fetch models or require a native runtime. The adapter dependencies
are fixed by their committed lockfiles.

```sh
rtk cargo build --release --locked --manifest-path tools/pseudonymization/Cargo.toml
rtk cargo build --release --locked --manifest-path tools/pseudonymization/gline/Cargo.toml
rtk cargo build --release --locked --manifest-path tools/pseudonymization/gliner2/Cargo.toml

rtk cargo fmt --check --manifest-path tools/pseudonymization/Cargo.toml
rtk cargo fmt --check --manifest-path tools/pseudonymization/gline/Cargo.toml
rtk cargo fmt --check --manifest-path tools/pseudonymization/gliner2/Cargo.toml
rtk cargo clippy --locked --all-targets --manifest-path tools/pseudonymization/Cargo.toml -- -D warnings
rtk cargo clippy --locked --all-targets --manifest-path tools/pseudonymization/gline/Cargo.toml -- -D warnings
rtk cargo clippy --locked --all-targets --features ocr-compat --manifest-path tools/pseudonymization/gliner2/Cargo.toml -- -D warnings
rtk cargo test --locked --manifest-path tools/pseudonymization/Cargo.toml
```

## Explicit setup

The three candidates are `multi-v2.1-q8`, `pii-base-q8`, and `gliner2-pii-fp16`.
`models.json` pins export revisions, paths, byte counts and SHA-256 digests,
including tokenizers and model cards. Setup streams bounded downloads into
temporary files, verifies them, and publishes individual files atomically.
An incomplete or modified installation cannot pass inference validation.
Verified files are reused; setup never downloads document contents.

```sh
rtk proxy tools/pseudonymization/target/release/mdoc-pseudonymization-qualification setup multi-v2.1-q8 .qualification/models
rtk proxy tools/pseudonymization/target/release/mdoc-pseudonymization-qualification setup pii-base-q8 .qualification/models
rtk proxy tools/pseudonymization/target/release/mdoc-pseudonymization-qualification setup gliner2-pii-fp16 .qualification/models
```

Models are retained in the ignored `.qualification/` directory. Cleanup is manual.
The total pinned artifacts are approximately 365.45, 205.43, and 631.52 MB,
respectively, excluding runtime and compiled tooling. The inference binary uses
local paths; GLiNER2's Hub feature is disabled. `gline-rs` unavoidably enables
its tokenizer dependency's HTTP feature, so network-denied reproduction is
especially useful for that candidate.

## Offline measurements

Use a fresh output filename. `run` records the child's exit status and deadline
separately from the inference report, so a crash after writing results is visible.
The supervisor sets the library path only in the child environment; inference
never runs setup or recovers missing artifacts through the network.

On Apple Silicon macOS with mdoc's OCR already set up:

```sh
rtk proxy tools/pseudonymization/target/release/mdoc-pseudonymization-qualification run \
  tools/pseudonymization/gline/target/release/mdoc-qualify-gline \
  multi-v2.1-q8 .qualification/models tests/fixtures/pseudonymization/corpus.json \
  .qualification/results/multi-t05.json \
  "$HOME/Library/Application Support/mdoc/ocr/v1/libonnxruntime.1.27.0.dylib" \
  "describe CPU, RAM, OS and rustc" 0.5 600
```

For `pii-base-q8`, use the same adapter. For `gliner2-pii-fp16`, use
`tools/pseudonymization/gliner2/target/release/mdoc-qualify-gliner2`. Repeat at
threshold `0.3` with a new output filename. Threshold and label policies are
recorded in each report; they are experiments, not accepted product settings.

The dated macOS runs prefixed the executable with
`sandbox-exec -p '(version 1)(allow default)(deny network*)'`, denying network
access to the supervisor and its child. Windows reproduction uses the `.exe`
executables, the installed `onnxruntime.dll`, and an externally enforced network
block. Linux requires a separately verified compatible native runtime. Those
platforms have not been run in this qualification.

Reports include exact predictions, every miss/false positive, invalid source
offsets, duplicate counts, source/corpus/runtime hashes, CPU threads, build
profile, verify/load/drop times, first and warm inference times, and process
peak RSS. The supervisor adds machine details and successful-exit/deadline
evidence. Peak RSS is the whole process high-water mark, including stress
fixtures; it is not model-file size, retained idle memory, or GPU memory.

Each short fixture runs three times in one loaded engine to check deterministic
results and warm latency. Long documents run once. Both adapters use 128 regex
word windows with 32-word overlap; GLiNER2 uses its library's chunker and merge.
Word windows do not establish a strict subword budget. Stress fixtures expose
that distinction; production needs a tokenizer-aware bound before adoption.

Generate the category/language and fixture tables from raw evidence:

```sh
rtk proxy tools/pseudonymization/target/release/mdoc-pseudonymization-qualification summarize \
  tests/fixtures/pseudonymization/results/2026-10-01/*-t0.[35].json
```

Set a short deadline, for example three seconds, to exercise termination during
actual synchronous inference. The child is killed and reaped; the `.process.json`
records `deadline_exceeded: true`. This is tooling cancellation, not an
implementation of the app's future revision/tab review lifecycle.

## Same-process OCR compatibility

The opt-in probe retains the original qualification `pdf-inspector` fork pin
(`620afae42eac4b92fffa2437b89e10a912bb93ee`) and shares `ort` rc.13
with GLiNER2. It checks EN/RU scanned-PDF transcripts before PII loading, while
both engines coexist, and after PII is dropped. It compares source PDF hashes
and validates PII byte offsets. No OCR download is attempted.

```sh
rtk cargo build --release --locked --manifest-path tools/pseudonymization/gliner2/Cargo.toml \
  --features ocr-compat --bin mdoc-qualify-ocr-compat
rtk proxy env GLINER2_DEVICE=cpu tools/pseudonymization/gliner2/target/release/mdoc-qualify-ocr-compat \
  .qualification/models "$HOME/Library/Application Support/mdoc/ocr/v1" \
  tests/fixtures/ocr-qualification .qualification/results/ocr-coexistence.json
```

Run with a network block for an offline assertion. On Windows, pass the installed
OCR bundle directory instead of the macOS path. The macOS result is retained
with the corpus evidence.

`provenance.json` records exact engine/wrapper/tokenizer code revisions and
license-file digests, upstream model-card revisions, and the tested runtime.
`models.json` identifies the actual exported weights and tokenizer bytes.
Shipping any model still requires retaining applicable licenses/notices and
resolving the documented export provenance questions.
