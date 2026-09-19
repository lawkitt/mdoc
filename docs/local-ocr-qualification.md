# Local OCR qualification — 2026-09-19

Initial result: the upstream runtime works on Apple Silicon macOS, but its default
PP-OCRv6 Small model cannot satisfy the agreed English/Russian scope. Per Q11,
propose a replacement before expanding dependency work. At that initial gate,
no application code, Cargo dependencies, or user-provided documents were changed.
The user subsequently approved the replacement; final results follow below.

## Tested configuration

- pdf-inspector 1.21.0, commit `dc9bdc162a611b957fc6afc7139970793bacdd0e`.
- Rust 1.98.1; release `pdf2md` built with `--features ocr` on macOS ARM64.
- PDFium `native-v7988`, `firecrawl-pdfium-mac-arm64.tgz`, SHA-256
  `4168356c2e62ad5e79553e2e9162f5c99949759d90cb83876a50311f0c32b9b3`.
- ONNX Runtime 1.27.0, `onnxruntime-osx-arm64-1.27.0.tgz`, SHA-256
  `545e81c58152353acb0d1e8bd6ce4b62f830c0961f5b3acfedc790ffd76e477a`.
- PP-OCRv6 Small detection, recognition, and dictionary files from OAR release
  v0.7.0, verified against the exact SHA-256 values in the upstream manifest.
- Runtime archives were also SHA-256 verified before extraction. Components
  and build outputs remain local; nothing was uploaded.

## Fixtures and results

The synthetic image-only PDFs and their expected plain text are in
`tests/fixtures/ocr-qualification/`. The `*-result.json` files are the actual
unmodified baseline CLI output, retained as failure evidence rather than
desired-output snapshots. These fixtures contain no user document content.

Generated with Pillow: white RGB image, 1654 × 2339 pixels, Arial.ttf at 40 px,
black text starting at (120, 180), line spacing 110 px, PDF resolution 200 dpi.
Each line of the adjacent expected `.txt` file was drawn as a separate line.
The OCR pipeline used its default 150 dpi rendering and automatic routing.

| Test | Result | Reported OCR confidence | Hosted recommendation |
| --- | --- | --- | --- |
| English, five printed lines | Exact text after removing Markdown heading markers and normalizing whitespace | 0.9985 | None |
| Russian, six printed lines | Zero Cyrillic letters; only punctuation and numbers survived | 0.9339 | None |

Both processes exited successfully and routed page 1 to OCR. English had no
warnings. Russian reported `discarded 1 regions without usable recognition
output`, but did not classify the page as needing further recognition.
High average confidence is therefore insufficient to establish language
coverage or completeness. English heading inference was not structurally
accurate; the text match is not a layout-quality claim.

### User-provided scanned fixtures

Also ran all three PDFs in `tests/fixtures/scanned-pdf` locally using the same
offline runtime. All 21 pages were routed to OCR and all processes succeeded:

| Fixture | Pages | Elapsed wall time | Cyrillic output letters |
| --- | --- | --- | --- |
| case-files.pdf | 19 | 20.07 s | 0 |
| migration-card.pdf | 1 | 1.81 s | 0 |
| passport.pdf | 1 | 0.48 s | 0 |

No page was marked as recommending further recognition; every page had one
warning. Average page confidence ranged from 0.6836 to 0.9305. These are single
runs, not benchmarks or accuracy scores against a human transcription. The
synthetic Russian fixture establishes the failure independently of the contents
of these documents. Raw extracted text remains in local temporary files under
`/tmp/mdoc-ocr-qualification`; it is not copied into repository evidence.

The checksum-verified default dictionary contains all 52 English uppercase and
lowercase letters and none of the 66 Russian uppercase/lowercase letters,
including Ё/ё. This explains the failure independently of scan quality.

## Reproduction

Build in the cloned pdf-inspector repository:

```sh
rtk cargo build --release --features ocr --bin pdf2md
```

The clone had no Cargo.lock, so the initial `--locked` build could not run.
The subsequent build generated an ignored local lockfile and succeeded.
Use the downloaded library paths and verified model directory for each fixture:

```sh
rtk proxy env \
  PDFIUM_LIB_PATH=/absolute/path/to/libpdfium.dylib \
  ORT_DYLIB_PATH=/absolute/path/to/libonnxruntime.1.27.0.dylib \
  /absolute/path/to/pdf-inspector/target/release/pdf2md \
  tests/fixtures/ocr-qualification/russian.pdf \
  --ocr auto --ocr-offline --ocr-model-dir /absolute/path/to/models --json
```

Repeat with `english.pdf`. The actual run used downloaded components under
`/tmp/mdoc-ocr-qualification` and processed both documents in offline mode.
No real-document corpus accuracy acceptance, native UI acceptance, Windows/Linux
runtime test, or mdoc workspace gate is claimed by this isolated qualification.

## Replacement approved and adopted

Keep PDFium, ONNX Runtime, OAR, and PDF native/OCR fusion. Replace the recognizer
and matching dictionary with **PP-OCRv5 Cyrillic mobile** while retaining the
current detector, subject to an actual compatibility and quality test.

[OAR's model catalog](https://github.com/GreatV/oar-ocr/blob/main/docs/models.md)
publishes the recognizer and its matching dictionary in release v0.3.0:

- `cyrillic_pp-ocrv5_mobile_rec.onnx`, 8,076,390 bytes, SHA-256
  `a18d96d7c8d73d90f2ed056549caa1de3a8e6cb744cccba16cd593ea8cd2d569`.
- `ppocrv5_cyrillic_dict.txt`, 2,781 bytes, SHA-256
  `db40aa52ceb112055be80c694afdf655d5d2c4f7873704524cc16a447ca913ba`.

The replacement dictionary was downloaded, checksum verified, and checked to
contain all English and Russian letters. Recognition was then tested as recorded
below; character coverage alone was not treated as acceptance.

The high-level pdf-inspector pipeline hardcodes `PP_OCR_V6_SMALL`; a different
`model_directory` does not select a model. Implemented change: a small external
pdf-inspector fork patch allowing a pinned model manifest in OCR options,
including its identity/digests in the existing cache key and using the same
manifest for resolution, installation, and recognition. Keep the upstream
default, expose no model chooser in mdoc, and test the candidate on both
synthetic languages plus representative local scans before proceeding.

The fork patch is published at
<https://github.com/lawkitt/pdf-inspector/commit/620afae42eac4b92fffa2437b89e10a912bb93ee>
and mdoc pins that exact revision. It preserves the upstream default model and
also permits explicit PDFium/ONNX Runtime paths without changing process-wide
environment variables. The fork remains external to the workspace.

## Replacement recognition results

Both controlled fixtures matched the expected text exactly after removing
Markdown heading markers and normalizing whitespace. Russian includes all the
original letters in `Съёмка, объём, ёжик, щука, юрист, заявление.` Actual results
are retained as `*-cyrillic-result.json` beside the synthetic PDFs.

All user-provided fixtures were rerun locally, without uploading their contents:

| Fixture | Pages | Single-run wall time | Result |
| --- | --- | --- | --- |
| case-files.pdf | 19 | 20.63 s | 17,994 Cyrillic letters extracted |
| migration-card.pdf | 1 | 1.99 s | 491 Cyrillic letters extracted |
| passport.pdf | 1 | 0.46 s | No Cyrillic letters; page 1 flagged for review |

The real-document counts demonstrate recognition, not accuracy against a human
transcript. Passport quality remains limited. mdoc reports all pages with
pipeline warnings or low-confidence/incomplete results, including passport
page 1. It never invokes hosted recognition. Raw real-document output remains
in temporary local storage, outside the repository.

## App implementation and validation

- Main-bar states: Set up OCR, Setting up OCR…, OCR ready, Retry OCR setup.
  Startup checks installed files in the background and never downloads.
- Explicit setup downloads and verifies about 54 MB into application-local
  `mdoc/ocr/v1`; extracted libraries have independently pinned SHA-256 values.
  Only selected regular archive files are copied; licenses/notices are retained.
- Setup validates actual runtime/model loading. Imports use the chosen manifest,
  explicit library paths, and `ModelDownloadPolicy::Offline`.
- Missing setup offers setup, skip, or cancel. Partial native/OCR output has
  persistent page warnings; failure leaves the current document unchanged.
  Runtime failures expose setup retry, including when native text is retained.
- One background job, completion-time unsaved-change protection, and generation
  guards remain in place. Setup finishing after a document switch cannot replace
  the new document. There is no percentage progress or hard cancellation.
- Explicit setup smoke test passed against the final Git dependency: fresh
  download/verification/load in throwaway storage, offline English/Russian
  conversion, and byte-for-byte source preservation. It is an ignored test so
  ordinary workspace tests do not download artifacts.
- New tests cover setup integrity, archive selection, offline options, content
  detection, skip behavior, partial fallback, setup prompt/cancel/retry, stale
  completion, and existing document lifecycle protection.
- `cargo build`, formatting, strict workspace/all-target Clippy, and license
  generation passed on Apple Silicon macOS. The full workspace test run reports
  268 passing tests and one pre-existing failure:
  `ui_tests::wrapped_table_search_uses_painted_cell_geometry` (`table glyph bounds`).
  The exact same failure was reproduced on an isolated unchanged HEAD checkout (`3404d20d30a311b913e976f37d00a82f7299a14b`),
  with the existing untracked `comments.docx` fixture copied in so it could build.
- The dependency fork passed default tests (1,564), OCR-feature tests (1,661),
  formatting, and its required strict Clippy invocation. An additional
  all-target Clippy run encountered existing upstream test-only warnings; these
  unrelated tests were not rewritten. The optional sibling `pdf-evals` checkout
  was unavailable.
- Native UI inspection was attempted but blocked by macOS Accessibility and
  Screen Recording permissions. Headless tests are not visual or shortcut
  approval. Windows/Linux/Intel macOS runtime qualification was not performed;
  OCR setup stays disabled there, while native-text imports remain available.

The reviewed native-PDF snapshot changes and formatting limitation are recorded
in `tests/fixtures/import/README.md`. Office snapshots remain unchanged.
