# Vision evaluation harness

Measures vision candidates for the page-artifact guard (lawkitt/mdoc#911):
each runs on every page, writes its raw output, and `score` checks that
against the annotations and `score.BARS`. Report-only: nothing here ships or
changes the app.

`vision_eval/run.py` lists the candidates; `vision_eval/pins.py` pins every
model file by source revision, size and SHA-256.

## Setup

Use Python 3.12 (paddlepaddle has no wheels for newer versions). `segno` and
`python-barcode` are needed only to generate the synthetic overlay set:

```sh
python3.12 -m venv ~/.cache/mdoc-vision-eval/venv
~/.cache/mdoc-vision-eval/venv/bin/pip install -r tools/vision-eval/requirements.txt segno python-barcode
```

Run every command from `tools/vision-eval/` with that interpreter.

```sh
python -m vision_eval.fetch        # download, verify, check the layout classes, convert seal detection
```

On Windows, run `fetch --no-convert` and convert seal detection on Linux, WSL
or macOS: paddle2onnx 2.1.0's extension doesn't load against paddlepaddle
3.3.1 on native Windows. Another machine's conversion may have a different
digest; that is fine as long as the outputs agree (`compare`).

Build the two native hosts into `MDOC_VISION_BIN` (default
`~/.cache/mdoc-vision-eval/bin`). On macOS:

```sh
M=~/.cache/mdoc-vision-eval/models/cactus-4.1.1/macos-arm64
xcrun clang -O2 -I$M host/mirage_host.c $M/libcactus.a -lc++ -framework Accelerate \
  -o ~/.cache/mdoc-vision-eval/bin/mirage_host
cargo build --release --manifest-path rxing-host/Cargo.toml
cp rxing-host/target/release/rxing-host ~/.cache/mdoc-vision-eval/bin/
```

On Windows x64, `windows-x86_64/libcactus.a` is an llvm-mingw build (COFF,
Itanium C++ symbols, libc++ 21), not MSVC. Link it with llvm-mingw's clang:

```sh
x86_64-w64-mingw32-clang -O2 -I<dir> host/mirage_host.c <dir>/libcactus.a -lc++ -lpsapi -static -o mirage_host.exe
```

`rxing-host` has its own `[workspace]`, so it is not part of mdoc's. When
Xcode's command-line tools disagree with the SDK,
`DEVELOPER_DIR=/Applications/Xcode-<version>.app/Contents/Developer` fixes
`xcrun clang`.

## Page sets

The set format is in `vision_eval/pages.py`.

- `fixtures`: the public `tests/fixtures/ocr-qualification` pages, with no
  artifacts.
- Private sets: every folder under `MDOC_VISION_SETS`, or else
  `$MDOC_HOLDOUT/vision-eval`. Pages are stored already rendered, so every
  machine reads the same pixels.
- The synthetic overlay set (`vision_eval/overlay.py`) uses macOS system
  fonts, so it is generated once and stored with the private sets.

The private sets and their outputs never go into this repository. Runs copy
outputs under `MDOC_VISION_OUT` (default `~/.cache/mdoc-vision-eval/out`), so
keep that folder outside the repository. Post only the report's aggregates.

## Run, score, compare

```sh
python -m vision_eval.run all                          # every candidate on every set
python -m vision_eval.run layout seal --sets overlays  # some candidates or sets
python -m vision_eval.score --markdown report.md       # per-class verdicts, aggregates only
python -m vision_eval.compare digests                  # one digest per set and candidate
python -m vision_eval.compare diff <out-a> <out-b>     # Mac against Windows, tolerant
python -m unittest discover tests                      # self-tests; no model needed
```

## What the numbers mean

The scoring rules (`classes.py`, `score.py`, `worker.SEAL_GATE`) were fixed
on the public fixtures and a synthetic set before any private page was
scored. A change to them is a maintainer decision on #911, not a tuning
step.

- **Recall and precision** count a prediction for an annotated artifact of
  its class at box IoU 0.5, one to one; IoU 0.3 is a diagnostic. The report
  scores only the classes a candidate can produce.
- **Mirage names** in the report are only generic ones; every other name
  counts as `other`, since open-vocabulary names could repeat words from a
  private page.
- **Text loss** counts OCR words outside every annotated artifact that a
  predicted text-replacing box would hide. Headers and footers replace
  nothing, since the app drops only exact repeats. With no reference words
  there is no verdict.
- **Seal text** is CER on what a stand-in would inline. A reading inlined
  where no seal is annotated always fails.
- **Time** is a candidate's page time over the `ocr` run's on the same
  pages, at 4 threads, as the app. The `ocr` run reproduces the app's
  settings but isn't the app's code, so the app's own page time
  (`pdf2md --ocr --json` reports `ocr_ms`) is a cross-check, not the
  baseline.
- **Memory** is added peak: a Python runner's peak minus its peak before any
  model loaded, or a native host's own peak.
- **Coverage.** A run missing any page of its set gets no verdict, so a
  stale run can't pass.

The seal reader's curved-text unwrapping (`seal.py`) is the harness's own,
not PaddleOCR's, and is part of the candidate as measured.

`python -m vision_eval.tune` tunes Mirage's usage on the `tune-seal` set;
its steps and rules are in `vision_eval/tune.py`.
