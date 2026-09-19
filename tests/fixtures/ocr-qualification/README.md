# Synthetic OCR qualification fixtures

The English and Russian text and raster-only PDFs were generated for this
project and are dedicated to the public domain under CC0-1.0. They contain no
user document content. Adjacent `.txt` files are expected recognition text.

`*-result.json` records the original PP-OCRv6 Small baseline, including its
Russian failure. `*-cyrillic-result.json` records the qualified replacement.
These JSON files are evidence, not exact-output snapshots to regenerate on
every test run. The ignored OCR setup smoke test compares recognized text to
the `.txt` files and checks that PDF bytes remain unchanged.

See `docs/local-ocr-qualification.md` for generation parameters, artifact
versions, checksums, and reproduction details. No models or native runtimes
are checked into the repository.
