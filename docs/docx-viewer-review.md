# DOCX viewer implementation review

Reviewed commit `a614027e07cca5c2d9d51567e189ab66c8d2e4a1` against
`docx-viewer-decisions.md` on 2026-09-18. The library-led conversion approach
is retained; the worker supervision and preview replacement lifecycle were corrected.

## Findings and repairs

| Finding in the reviewed commit | Repair |
| --- | --- |
| Worker errors printed a message and exited successfully. A missing/corrupt output could discard the current preview. | Failure and argument errors have nonzero exits. Error/warning sidecars avoid a full stderr pipe deadlock. Missing output is rejected. |
| DOCX validation ran in the parent before the timeout, then the worker reread the source. Superseded workers could overlap. | A single supervised worker validates and renders one bounded snapshot. Timeout covers validation; a process guard kills and reaps on all exits. Serialization includes teardown. |
| Tests replaced worker execution with direct conversion. | Actual executable integration tests cover successful conversion, source preservation, worker failure, oversize input, timeout, cancellation, abnormal exit, and false success. The GPUI tests still use a direct adapter, explicitly limited to UI behavior. |
| ZIP policy used XML substring checks and trusted declared binary entry sizes. | Namespace-aware XML checks detect revision elements and macro content types, ignore comments in XML, distinguish hyperlinks from external resources, validate actual inflation, reject traversal/duplicates and nested ZIP signatures, and warn about deferred content. |
| Replacement removed the old pane before asynchronous PDF loading completed; PDF failures had no retry source. | A pending PDF view must finish loading before installation. Failed/locked replacements preserve the old pane; accepted imports first clear it. Both PDF and DOCX failures retain their source and Retry. |
| Initial conversion could not be closed; close left warning messages; native clean-window close did not cancel conversion. | Close Preview is available while loading and after failure, clears state and invalidates completions. Window close releases textures and cancels; graceful application quit waits for worker teardown and background artifact cleanup. |
| Temporary directory deletion ran on the UI thread. | Artifact ownership schedules deletion off-thread, including stale results. |
| New macOS file associations had no native file-open event handler. | OS file URLs route to the existing `open_path` workflow, including launch-time delivery. Packaged Finder behavior still needs human verification. |

Markdown extraction policy remains separate from preview validation. No Office
runtime, network conversion service, custom layout engine, or memory cap was added.
The pinned renderer's resource readers use ZIP lookups, not HTTP/filesystem
fetches. The worker is crash containment, not a security sandbox.

## Repeatable qualification evidence

`tests/fixtures/docx-preview/generate.py` creates the checked-in synthetic
`coverage.docx` using Python's standard library. It contains paragraphs/styles,
Latin/Cyrillic, horizontal and vertical merged cells, nested numbering, a red PNG,
page and section breaks, a landscape section, headers/footers, a footnote, and an
HTTPS hyperlink. It rendered as ten pages on the measured machine.

`tests/docx_worker.rs` checks retained text across pages, Cyrillic search text,
landscape dimensions, hyperlink targets, and visible red image pixels. Text
coverage compares with whitespace removed because extraction sometimes joins
adjacent words; this is a known limitation, not evidence that exact phrase
search works. The existing simple DOCX fixture is also rendered and parsed.

GPUI tests check replacement failure preservation, accepted-import failure and
PDF retry, immediate Markdown availability, close during pending work, stale
completion suppression, and independent Markdown transitions. Existing
unsaved-change/import tests remain in the workspace gate.

Measured locally on Apple M4, 24 GiB RAM, macOS 15.7.7, debug/test build:

- Isolated coverage integration test: worker completion **1.166 s**, parsed PDF
  and first headless raster **1.227 s** (includes worker startup).
- A separate warm invocation through `/usr/bin/time -l`: **0.04–0.05 s** elapsed,
  **23,969,792–24,035,328 bytes maximum resident set size**. These are individual
  samples, not a percentile or a worst-case memory estimate.
- Reproduce the end-to-end headless measurement with
  `cargo test -p mdoc --test docx_worker coverage_corpus_preserves_text_links_and_landscape -- --exact --nocapture`.

These are CPU conversion/render measurements. They do not establish actual
first presentation, input latency, or the 100 ms feedback target. No enforceable
memory cap is inferred from one synthetic document.

## Remaining acceptance boundaries

- Human review of rendered pages, native keyboard shortcuts, typing/scrolling
  during conversion, and packaged Finder file-open behavior is pending.
- Linux and Windows builds/runtime qualification were not run on this Mac.
- Broader real-world corpus, font fallback on clean installations, cold-start
  distributions, and large-document peak memory remain unqualified.
- Layout is approximate. Word spacing in extracted text can be lost, affecting
  multiword search. Equations, charts, SmartArt, embedded fonts, CJK and RTL have
  warnings when detected and remain outside first-release qualification.
- Auxiliary XML accepts UTF-8 and BOM-marked UTF-16 for policy validation;
  this does not extend the renderer's supported content encodings.
- No Word/LibreOffice parity or visual approval is claimed by automated tests.

## Verification result

After the repairs: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (**215 passing tests**), and `git diff --check` pass.
Cargo still emits its registry configuration warning and the existing upstream
`block 0.1.6` future-compatibility notice; these are not new Clippy findings.

## Long-document performance follow-up

The uploaded DOCX corpus was converted to PDF and exercised through the actual
`PdfView` scroll path. In an unoptimized development build, the 14-page legal
document spent about 3.4 seconds rasterizing its initial near-viewport pages,
and fast scrolling left roughly 873 ms of background raster work before the
viewport settled. CPU layout for a scroll frame was already low (about 1.6 ms),
so the measured bottleneck was PDF rasterization, not the GPUI page-column layout.

The viewer now keeps a one-page raster margin, prioritizes pages intersecting the
viewport, limits concurrent raster jobs to two, and drops obsolete results when
the viewport or zoom generation changes. `release` also invalidates pending jobs.
The fit-to-width path is constrained with zero-width flex children so sidebar
measurement cannot repeatedly change the page width and trigger a render storm.
The application starts generated DOCX previews in fit-width mode, avoiding a
large off-pane bitmap before the first visible page is shown.

The development profile optimizes `gpui-pdf`, Hayro, and Vello CPU/vector
dependencies at level 2 while leaving the application code debuggable. Against
the same corpus on this Apple M4, the 14-page fixture measured 142 ms initial
viewer preparation, 1.75 ms frame p95, and 23.7 ms settle p95 in the final run.
The other five fixtures also passed the same scroll budget; their initial
preparation was 47–94 ms. These measurements use a fixed 550×750 test viewport and measure
CPU layout plus raster completion, not display presentation or input latency.

The ignored harness is reproducible with:

```text
cargo test -p gpui-pdf --features search,forms long_document_scroll_budget -- --ignored --nocapture
```

Set `PDF_PERF_DIR` to a directory of converted PDFs and optionally
`PDF_PERF_FIT=1` to exercise fit-to-width. The uploaded Dubai document also
exercised the UTF-16 auxiliary-XML decoder added to the DOCX package policy.

### Performance change review

A follow-up review found that enabling fit-width before loading did not itself
prevent initial oversized rasters: the first render can precede scroll-area
measurement. Raster scheduling now waits for that measurement when fit is active,
requesting one follow-up frame without continually redrawing an unmeasured pane.
`initial_fit_waits_for_measured_viewport` failed with two premature raster requests
before the fix and passes afterward.

The original benchmark only traversed about three pages and did not assert bitmap
availability. Its earlier timing samples above are historical, not whole-document
coverage. The harness now visits every page forwards and backwards, uses actual
page geometry, and requires current-generation bitmaps throughout the retained
viewport window. The rapid-scroll test also checks that the final destination
renders and that releasing a view with queued replacements drains the jobs without
restoring bitmaps. The throwaway cache-sharing probe was removed.

With this stronger harness, all six PDFs pass. The 14-page document measured
114.5 ms initial preparation, 1.81 ms draw p95, and 26.0 ms settle p95; other
fixtures measured 48–94 ms initial preparation and 2.4–21.8 ms settle p95.
These are individual local headless runs, not end-to-end DOCX opening or native
input-latency measurements. The bounded queue and selective dev optimizations
are retained; no renderer replacement was necessary.

## Remaining lag: Markdown editor root cause (2026-09-19)

The PDF-only benchmark did not cover the editor alongside it. A full-workspace
benchmark reproduced the reported lag using Markdown imported from the 14-page
legal document (39,138 bytes, 377 lines), at a fixed 1100×750 viewport in the
development profile:

| State | Before scroll p95 | After scroll p95 |
| --- | ---: | ---: |
| Markdown alone, before any preview | 217.9 ms | 9.2 ms |
| DOCX preview open | 221.7 ms | 11.9 ms |
| Preview closed | 219.7 ms | 9.5 ms |

A three-second macOS sampling profile attributed 1,993 of 2,096 active test-thread
samples to `EditorState::line_end` rebuilding `line_starts` from the whole source
inside the heading/link prepaint loops. That made frame preparation proportional
to document bytes times row count. Preview teardown was not required to reproduce
this bottleneck. Reusing one frame-local line index for row ends and byte-to-row
lookups removes the repeated scans without persistent cache invalidation or extra
compiler optimization. Selection/find geometry and code-fence lookup reuse the
same index.

The initial reproducer scrolled the first 2,320 px. The final harness traverses
the entire scroll extent in both directions, asserts actual scrolling and preview
state, and measures text input plus redraw (18–26 ms p95 for the legal document).
It checks the resulting Markdown bytes to ensure the input actually occurred.
Timing remains a headless CPU measurement, not native input/presentation latency.
The optional test requires the locally supplied DOCX fixtures; it is ignored by
the portable workspace test gate.

```sh
cargo test -p mdoc --bin mdoc large_markdown_scroll_budget -- --ignored --nocapture
# Select another fixture by filename:
MD_PERF_FILE=coverage.docx cargo test -p mdoc --bin mdoc large_markdown_scroll_budget -- --ignored --nocapture
```

The asserted local budgets are 16.667 ms scroll p95 and 50 ms input-plus-redraw p95.
The editor change is in `crates/mdoc-editor/src/element.rs`; it leaves source bytes
and the shared PDF rendering changes intact.
