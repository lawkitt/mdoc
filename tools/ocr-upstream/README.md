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
