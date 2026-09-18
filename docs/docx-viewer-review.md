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
- Package XML unsupported by the renderer's UTF-8 path is rejected explicitly.
- No Word/LibreOffice parity or visual approval is claimed by automated tests.

## Verification result

After the repairs: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (**215 passing tests**), and `git diff --check` pass.
Cargo still emits its registry configuration warning and the existing upstream
`block 0.1.6` future-compatibility notice; these are not new Clippy findings.
