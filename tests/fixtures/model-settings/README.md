# Local model settings verification

Measured 2026-10-02 after the user authorized ADR 0006 implementation.
Settings persist configuration only; comparisons retain immutable input and
read-only results in memory. Explicit reports in this folder use synthetic data.
No weights, source-document replacement maps or private fixtures are included.

## Runtime and resource probes

Machine: Apple M4, Mac16,13, 10 CPU cores, 24 GB RAM, macOS 15.7.7,
Rust 1.98.1. Build: unoptimized debug GPUI test executable. Model checks,
verification, loading and inference are included in per-run elapsed time.
Peak RSS is the process maximum across sequential English and Russian runs,
measured with macOS `/usr/bin/time -l`, not a model allocation limit or a release
performance guarantee. Exact machine metadata and measurements are in
[machine.json](machine.json) and [measurements.json](measurements.json).

All four explicitly installed bundles loaded and ran with network access denied
by `sandbox-exec`. Models use the pinned ONNX Runtime 1.27.0. OCR additionally
uses PDFium native-v7988 and the pdf-inspector fork at
`7a11f9f3f2423edd0035f87e7152b49c08a7ce1a`; pseudonymization uses gliner2-rs
0.9.6 / ort 2.0.0-rc.13. Both precisions use export revision
`e594898629d452e8311796f5f329c7edbeda907c`. Reports retain configuration, model
revisions/artifact SHA-256 digests, input SHA-256, timings, warnings and outputs.

| Bundle | English elapsed | Russian elapsed | Peak RSS | Evidence |
| --- | ---: | ---: | ---: | --- |
| Cyrillic OCR | 1.448 s | 1.383 s | 701.9 MiB | [report](ocr-cyrillic.json), [log](ocr-cyrillic-offline.log) |
| Original v6 Small OCR | 1.606 s | 1.354 s | 796.7 MiB | [report](ocr-v6.json), [log](ocr-v6-offline.log) |
| GLiNER2 FP16 | 5.686 s | 5.852 s | 2,014.8 MiB | [report](pii-fp16.json), [log](pii-fp16-offline.log) |
| GLiNER2 FP32 | 8.164 s | 8.218 s | 2,587.3 MiB | [report](pii-fp32.json), [log](pii-fp32-offline.log) |

OCR uses the existing [one-page image-only EN/RU fixtures](../ocr-qualification/README.md),
150 DPI, confidence 0 and Force on all pages. Cyrillic raw recognition matches
both complete transcripts. V6 matches English but loses Russian words; its known
language failure remains visible in Settings and the report. Runtime success
therefore does not establish recognition quality.

Pseudonymization uses threshold 0.5 and complete synthetic Markdown: four copies
of a short person/organization/contact example per language, including an English
mailto destination. Both precisions returned the same 16 source spans/categories
for each language, with slightly different confidence values. FP32 was slower
and used more memory in this probe; the FP16 default and resource limits remain
unchanged. These small smoke cases do not replace the existing 17-fixture
[qualification corpus](../../../tools/pseudonymization/README.md), establish recall,
or qualify either precision. Complete-document review and experimental warnings
remain required.

To reproduce, first compile the ignored probes, find the executable shown by
Cargo, and explicitly choose one of `ocr-cyrillic`, `ocr-v6`, `pii-fp16`, `pii-fp32`:

```sh
cargo test -p mdoc --bin mdoc --no-run
# Explicit verified setup; may download the selected bundle:
env MDOC_MODEL=pii-fp32 <test-executable> comparison::tests::settings_model_setup_probe --exact --ignored --nocapture
# Offline inference; uses installed artifacts and writes only this explicit report:
env MDOC_MODEL=pii-fp32 MDOC_REPORT=/absolute/path/report.json /usr/bin/time -l sandbox-exec -p '(version 1)(allow default)(deny network*)' <test-executable> comparison::tests::settings_model_offline_probe --exact --ignored --nocapture
```

Setup logs accompany each report. Completed verified artifacts were reused by
setup; existing default cache paths were preserved. The alternative downloads
were explicit. No inference path invokes the downloader.

## Automated checks

The mdoc full gate passed: `cargo fmt --check`, workspace/all-target clippy with
`-D warnings`, and `cargo test --workspace`: 351 passed, 11 opt-in tests ignored.
Focused app coverage includes:

- Explicit Apply, discarded drafts, finite/bounded controls, invalid/obsolete
  preferences, failed persistence preserving old active defaults and atomic files.
- Immutable input/configuration, cancellation/late completion rejection,
  mapping-free reports and protecting source/document paths and symlink aliases.
- Verified artifact reuse offline, cancellation retaining completed files,
  model-slot ownership until the worker exits, and removing one OCR bundle while
  retaining the other bundle and shared runtime files.
- Existing consent, source-tab lifecycle, stale import/scan rejection,
  editor/source preservation, undo and review flows.

The converter fork passed formatting, its repository clippy gate with OCR and
all OCR-feature tests: 1,661 passed. Its chunking regression retains ordered raw
recognition without additional engine calls or bitmap retention; Force and page
selection tests remain passing. The external `pdf-evals` checkout was unavailable.
The fork's broader all-target lint run encountered existing test-only lint failures;
no unrelated fork lint cleanup was included.

## Native and platform boundaries

The actual macOS development app was inspected through native screenshots and
computer input. Observed: Settings from the toolbar and Cmd-comma, dialog layout,
Advanced scrolling, numeric editing, Tab between fields, Escape from a focused
field and discarded drafts. An OCR comparison captured the PDF, ran both models
sequentially, froze All pages, and showed separate read-only recognition results
and elapsed times. The final layout keeps Details with its model and DPI controls
in one row. Native input observations are smoke checks; they are not a human
visual, shortcut, focus-restoration or IME approval.

Remaining: human macOS acceptance (including focus restoration, small-window
layout, IME and native file dialogs), native pseudonymization comparison/review
acceptance, and new Windows runtime/UI operation for model selection/FP32.
Only the Apple Silicon host toolchain was available; Windows/Linux target builds
were not run here. Existing Windows default-OCR evidence remains separate.

## GLiNER-only cleanup — 2026-10-04

Pseudonymization now offers only GLiNER2 FP16 and FP32. Removed the other
detector adapters, manifests, dedicated probe files and classifier-only review
categories. Settings descriptions, language evidence, source links and isolated
comparisons remain. FP16 stays the default and OCR selection is unchanged.

Focused preference tests verify FP16/FP32 compatibility and reject obsolete
model selections with an explicit reset notice, preserving the stored file
until the user applies supported preferences. Formatting, workspace/all-target
clippy with denied warnings and workspace tests passed: 354 tests passed,
11 opt-in tests ignored. The development application binary was rebuilt.

Both installed precisions were rerun through `settings_model_offline_probe`
with `GLINER2_DEVICE=cpu` and network access denied by `sandbox-exec`, using
the same short synthetic EN/RU input and debug build as the earlier probes.
Each returned the same 16 source spans/categories per language as the retained
2026-10-02 reports. Current per-run timings and process peak RSS were:

| Bundle | English elapsed | Russian elapsed | Peak RSS |
| --- | ---: | ---: | ---: |
| GLiNER2 FP16 | 5.530 s | 5.858 s | 2,064.5 MiB |
| GLiNER2 FP32 | 8.496 s | 8.359 s | 2,763.2 MiB |

These are runtime smoke checks, not accuracy qualification or resource limits.
Native UI/IME acceptance and Windows runtime operation remain outstanding.


## Settings and review UI cleanup — 2026-10-04

The main toolbar has one Settings control with a unique ID and direct panel
listener. A GPUI mouse regression reproduces the former duplicate-ID failure
when the pseudonymization bar is open and verifies opening/closing without
editor focus. It also checks Details/Advanced and the footer within a 640×480
viewport. A successful-scan regression removes the setup prompt only for the
scanned model and respects a later missing-model status.

Settings uses compact model cards, one shared experimental PII caution,
Details for evidence/repair/removal, and a fixed footer. The review bar keeps
actions and the source-review caution visible; scan provenance is disclosed
under Details. An additional regression exercises shutdown while a worker owns
model admission, rejects new work, and waits for that worker to release it.

A temporary native preview revealed a quit race: the main thread was in
`exit`/`__cxa_finalize_ranges` while a background ONNX session was initializing.
The app now waits synchronously for model work in its quit callback, before
GPUI's 200 ms asynchronous cleanup deadline. Sequential native checks of both
OCR bundles and both GLiNER precisions succeeded on the main thread, a 512 KiB
Rust thread, and the macOS dispatch queue. These checks used installed local
artifacts and performed no downloads. A controlled quit requested during
active catalog loading waited 12.47 s for the worker and exited successfully.
Formatting, clippy with denied warnings, all workspace tests (357 passed,
11 ignored), and the development app build passed. The isolated native preview
also closed without producing a new crash report.

Native screenshots used an isolated in-memory synthetic document and temporary
app bundle. Coordinate input was rejected by the native automation service
(`noWindowsAvailable`), so those screenshots do not establish native mouse or
keyboard acceptance. Human macOS/IME and Windows acceptance remain open.
