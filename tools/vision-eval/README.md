# Vision evaluation harness

This harness measures the vision candidates for the page-artifact guard (lawkitt/mdoc#911, decisions 8, 12–14 and 27). Each candidate runs on every page and writes its raw output: boxes in pixels, classes or names, scores and text. The harness then scores those outputs against the annotations and decision 13's bars. It is report-only, like `tools/detection-eval`: nothing here ships or changes the app.

| Candidate | What it finds | How it runs |
|---|---|---|
| `mirage` | open-vocabulary objects (named) and text | Cactus engine 4.1.1 (`libcactus.a`) through `host/mirage_host.c`; reference only (decision 25) |
| `layout` | `seal`, `header`, `footer`, `image` (as `picture`) | PP-DocLayout_plus-L, PaddlePaddle's ONNX build, on ONNX Runtime 1.27.0 |
| `seal`, `seal-mobile` | what a seal says | PP-OCRv4 seal text detection (server or mobile, converted to ONNX) + the app's PP-OCRv5 Cyrillic recognizer, on the layout run's seal boxes |
| `seal-gt`, `seal-mobile-gt` | the same reader on the annotated seal boxes | measures the reader without the layout model's misses |
| `rxing` | QR codes and barcodes, decoded | `rxing` 0.9.4 through `rxing-host` |
| `ocr` | page words with boxes; the time baseline | the app's OCR models (PP-OCRv6 Small detector, PP-OCRv6 Small or PP-OCRv5 Cyrillic recognizer) at the app's settings |

`vision_eval/pins.py` lists every file with its source revision, size and SHA-256.

## Setup

Use Python 3.12 (paddlepaddle has no wheels for newer versions):

```sh
python3.12 -m venv ~/.cache/mdoc-vision-eval/venv
~/.cache/mdoc-vision-eval/venv/bin/pip install -r tools/vision-eval/requirements.txt segno python-barcode
```

Run every command from `tools/vision-eval/` with that interpreter.

```sh
python -m vision_eval.fetch        # download, verify, check the layout classes, convert seal detection
```

On Windows, run `fetch --no-convert` and convert seal detection on Linux, WSL or macOS: paddle2onnx 2.1.0's extension doesn't load against paddlepaddle 3.3.1 on native Windows. The Windows lane's conversions were byte-identical to the macOS ones.

`fetch` stores the files under `MDOC_VISION_MODELS` (default `~/.cache/mdoc-vision-eval/models`). It reports whether your seal-detection conversion matches the macOS digest; a different digest is fine as long as the outputs agree (see `compare`).

Next, build the two native hosts into `MDOC_VISION_BIN` (default `~/.cache/mdoc-vision-eval/bin`).

On macOS:

```sh
M=~/.cache/mdoc-vision-eval/models/cactus-4.1.1/macos-arm64
xcrun clang -O2 -I$M host/mirage_host.c $M/libcactus.a -lc++ -framework Accelerate \
  -o ~/.cache/mdoc-vision-eval/bin/mirage_host
cargo build --release --manifest-path rxing-host/Cargo.toml
cp rxing-host/target/release/rxing-host ~/.cache/mdoc-vision-eval/bin/
```

On Windows x64, `windows-x86_64/libcactus.a` is COFF with Itanium C++ symbols and libc++ 21: an llvm-mingw build, not MSVC. Link it with llvm-mingw's clang:

```sh
x86_64-w64-mingw32-clang -O2 -I<dir> host/mirage_host.c <dir>/libcactus.a -lc++ -lpsapi -static -o mirage_host.exe
```

`rxing-host` builds with the ordinary Rust toolchain. It has its own `[workspace]`, so it is not part of mdoc's.

When Xcode's command-line tools disagree with the SDK, `DEVELOPER_DIR=/Applications/Xcode-<version>.app/Contents/Developer` fixes `xcrun clang`.

## Page sets

A set is a folder with `set.json`, page images (PNG, RGB) and one annotation per page. See `vision_eval/pages.py` for the format. In an annotation each artifact has a class (`classes.CLASSES`) and a pixel box; a seal also has its text segments in reading order (ring first, then centre lines), and a QR code or barcode its decoded text.

- `fixtures`: the public `tests/fixtures/ocr-qualification` pages, rendered with pypdfium2 at 150 DPI. They have no artifacts. Built under the output folder when missing.
- Private sets: every folder under `MDOC_VISION_SETS`, or else `$MDOC_HOLDOUT/vision-eval`. Pages are stored already rendered, so every lane reads the same pixels (`python -m vision_eval.pages digests <set>` prints their RGB digests).
- `python -m vision_eval.overlay <dir> --seed <n>` generates a synthetic set: fictional seals, script signatures, QR codes and Code 128 barcodes over the fixture pages, with exact annotations. It uses macOS system fonts, so it is generated once and stored with the private sets.

The private sets and their outputs never go into this repository. Runs copy outputs under `MDOC_VISION_OUT` (default `~/.cache/mdoc-vision-eval/out`), so keep that folder outside the repository. Post only the report's aggregates.

## Run, score, compare

```sh
python -m vision_eval.run all                          # every candidate on every set
python -m vision_eval.run layout seal --sets overlays  # some candidates or sets
python -m vision_eval.score --markdown report.md       # per-class verdicts, aggregates only
python -m vision_eval.compare digests                  # one digest per set and candidate
python -m vision_eval.compare diff <out-a> <out-b>     # Mac against Windows, tolerant
python -m unittest discover tests                      # self-tests; no model needed
```

Each candidate runs in its own process (`worker.py`), so its peak memory is its own. A set's output goes to `runs/<set>/<candidate>/` (a JSON file per page plus `run.json`). It is written beside that folder and moved into place only when complete. `run.json` records per-page milliseconds without model loading. The first page runs twice, and its first call is a warm-up that isn't counted. It also records load time, peak memory, ONNX Runtime's version, thread count and platform.

Mirage runs with `CACTUS_TELEMETRY=0`, `DO_NOT_TRACK=1` and `HF_HUB_OFFLINE=1`, on local files only. Its boxes are on a 0–1000 scale per image axis, relative to the whole image; `geometry.mirage_box` converts them to pixels.

## Rules fixed before any numbers

Decision 13's bars are in `score.BARS`. These rules decide how the bars are measured. They were fixed on the public fixtures and a synthetic smoke set before any private page was scored, and a change to them is a decision on #911:

- **Matching:** a prediction counts for an annotated artifact of its class at box IoU 0.5 or more, one to one, best pairs first. The report also shows recall at IoU 0.3 as a diagnostic. Signature and seal recall must each reach 90%. Precision must reach 85% for every class except `picture` (pictures aren't page artifacts).
- **Score cuts:** layout boxes count at 0.5, the model's `draw_threshold`; the raw output keeps boxes down to 0.3. Mirage uses the engine's defaults (options `{}`). rxing reports what it decodes.
- **Mirage names to classes:** whole-word matches in `classes.MIRAGE_WORDS` (`stamp` is a seal, for example). The report scores only the classes a candidate can produce. Its name diagnostic prints only names from `score.GENERIC_NAMES` and counts every other name as `other`, since open-vocabulary names could repeat words from a private page.
- **Text loss:** reference words are the `ocr` run's words whose centre lies outside every annotated artifact. Where the annotation has the page's ground-truth `text`, the OCR words in reading order are aligned to it (longest matching blocks), and only aligned words count. OCR junk, or a duplicate out of sequence, is never a reference word. The report says how many pages were checked this way. Word boxes are slices of the line box, in proportion to characters. A word is lost when at least half its box lies inside a predicted box of a text-replacing class: seal, signature, handwriting, QR code, barcode, watermark, punch hole or face. Headers and footers replace nothing, since only exact repeats are dropped (decision 10). The bar is zero lost words; with no reference words there is no verdict.
- **Seal text:** CER compares the reader's joined text (what a stand-in would inline) with the annotated segments joined in reading order. Both sides are NFKC case-folded, with whitespace, bullets and quote styles ignored. The reader inlines a seal's text only when its lowest segment confidence is at least 0.90 (`worker.SEAL_GATE`). The bar holds when no inlined reading exceeds 10% CER. A reading inlined where no seal is annotated always fails it.
- **Time:** a candidate's total page time over the `ocr` run's total on the same pages, at 4 intra-op threads, as the app.
- **Memory:** a Python runner's peak minus its own peak before any model loaded. `mirage_host` reports its own peak: on macOS the peak footprint (what `/usr/bin/time -l` prints), on Windows the peak working set. `rxing-host`'s peak is read when it exits: maximum resident set on macOS, peak working set on Windows.
- **Coverage:** a run missing any page of its set gets no verdict, so a stale run can't pass.

The `ocr` run reproduces the app's OCR settings (detector cap 960 px, scores 0.3/0.6, unclip 2.0, 48 px recognition padded to at least 320 px, minimum confidence 0). It isn't the app's code, so the app's own page time (`pdf2md --ocr --json` reports `ocr_ms`) is a cross-check, not the baseline.

## The seal reader

PaddleOCR's seal pipeline straightens curved text before recognition. `seal.py` does the same in a simpler way. A detected polygon that fills its minimum-area rectangle is straight text, and gets a perspective crop. Any other polygon is a ring arc, which is unwrapped around the seal's centre with a polar transform and read in both directions, keeping the more confident reading. A whole ring starts at its widest blank run. These are the harness's choices, not upstream's, and are part of the candidate as measured.
