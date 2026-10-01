# Synthetic OCR qualification fixtures

The English and Russian text and raster-only PDFs were generated for this
project and are dedicated to the public domain under CC0-1.0. They contain no
user document content. Adjacent `.txt` files are expected recognition text.

`*-result.json` records the original PP-OCRv6 Small baseline, including its
Russian failure. `*-cyrillic-result.json` records the qualified replacement.
These JSON files are evidence, not exact-output snapshots to regenerate on
every test run. The ignored OCR setup smoke test compares recognized text to
the `.txt` files and checks that PDF bytes remain unchanged.

No models or native runtimes are checked into the repository. The current model
manifest lives in the pinned pdf-inspector fork, and runtime URLs/digests live in
`src/ocr.rs`; Cargo.toml/Cargo.lock hold the dependency revisions. See the
[OCR ADR](../../../docs/adr/0004-explicit-offline-ocr-with-qualified-models.md)
for the integration decision.

## Fixture generation

Generated with Pillow: white RGB image, 1654 × 2339 pixels, Arial.ttf at 40 px,
black text starting at (120, 180), line spacing 110 px, PDF resolution 200 dpi.
Each expected `.txt` line was drawn separately. Original qualification used
the pipeline's default 150 dpi rendering and automatic native/OCR routing.

## Historical qualification — 2026-09-19

The original PP-OCRv6 Small recognizer produced exact English after removing
Markdown heading markers and normalizing whitespace. Russian retained zero
Cyrillic letters despite reported confidence 0.9339 and a successful process
exit. Its verified dictionary lacked Russian letters; high confidence did not
establish usable language coverage.

The approved replacement retained the PP-OCRv6 Small detector and used OAR's
v0.3.0 PP-OCRv5 Cyrillic mobile recognizer plus matching dictionary. Both synthetic
transcripts then matched under the same normalization. Cyrillic coverage includes
Ё/ё. Model identity/digests were added to the fork's existing manifest/cache path
rather than replacing the OCR pipeline or mutating process environment variables.

| Target | Historical evidence | Remaining limit |
| --- | --- | --- |
| Apple Silicon macOS | Fresh verified setup, actual runtime/model loading, repeated offline EN/RU conversion, and source-byte preservation passed. | Real scans were recognized but not scored against human transcripts; a poor identity-document scan remained flagged for review. |
| Windows x64, build 26200, Rust 1.98.1 | The same smoke coverage passed with the same model manifest and platform-specific runtimes. | Test machine had the x64 Visual C++ Redistributable installed; a clean image without it was not qualified. |
| Intel macOS, Linux, Windows ARM64 | Native-text import remains available; local OCR is disabled. | Runtime qualification is required before enabling. |

Tested runtime family: PDFium native-v7988 and ONNX Runtime 1.27.0; the app uses
explicit library paths and offline model policy. Windows needs the x64 Visual
C++ Redistributable for the ONNX DLL even though mdoc.exe uses a static CRT.
Handwriting, complex table reconstruction, other languages, and DOCX image OCR
are not qualified by these fixtures.

These are dated runtime/text results, not current full-gate or native UI approval.
The historical workspace gate also exposed `wrapped_table_search_uses_painted_cell_geometry`
at `table glyph bounds`, reproduced on unchanged HEAD. Later opening-UI work
recorded a passing full gate; recheck the current tree rather than treating either
historical outcome as current.

## Reproduce current app setup and offline recognition

On an enabled platform, run this opt-in test from the repository root. It
downloads the pinned runtime/model artifacts into throwaway storage, validates
loading, checks EN/RU text, and verifies unchanged source bytes:

```sh
rtk cargo test -p mdoc --bin mdoc setup_and_offline_english_russian_smoke -- --ignored --nocapture
```

Ordinary workspace tests do not download models. Do not copy raw outputs from
local user documents into fixture evidence or regenerate the retained historical
JSON files as desired snapshots. Controlled text recognition is not exact
layout reconstruction or a claim of real-corpus accuracy.
