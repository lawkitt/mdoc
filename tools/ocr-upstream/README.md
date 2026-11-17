# Upstream OCR comparison

`compare.py` uses only the Python standard library to verify source/reference
digests, compare prepared/raw/provenance captures and report exact per-page
CER/WER. It returns nonzero for any changed artifact requiring review or synthetic
transcript mismatch. This is offline QA tooling, not a product worker. Recognition
and capture remain in the Rust pdf-inspector fork.

See [the corpus](../../tests/fixtures/ocr-upstream/README.md).

```sh
python3 tools/ocr-upstream/compare.py \
  tests/fixtures/ocr-upstream/captures/baseline \
  tests/fixtures/ocr-upstream/captures/updated \
  --output /tmp/ocr-upstream-comparison.json
```

## Render resolution

`dpi.py` measures OCR accuracy, time and peak memory per render resolution
with the app's own OCR options (the `ocr_dpi` example of `mdoc-import`):
each page runs in its own process under `/usr/bin/time -l`, three times by
default, in interleaved order, and the table reports medians. OCR storage is
throwaway; never point `--root` at the app's data.

```sh
cargo build --release -p mdoc-import --example ocr_dpi
target/release/examples/ocr_dpi --setup target/ocr-dpi/root
python3 tools/ocr-upstream/dpi.py --root target/ocr-dpi/root --out target/ocr-dpi/corpus
swift tools/ocr-upstream/small_print.swift target/ocr-dpi/small-print-pages
python3 tools/ocr-upstream/dpi.py --root target/ocr-dpi/root --out target/ocr-dpi/small-print \
  --corpus target/ocr-dpi/small-print-pages
```

`small_print.swift` writes the small-print probe: a self-made 30-row
statement in English and Russian at 6–10 pt, image-only at 300 dpi.
`scripts/ios/ios-ocr-dpi.sh` runs the same corpus on an iPhone or a
Simulator and scores it here with `--runs-from`.
