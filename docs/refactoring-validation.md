# Ownership and rendering refactor validation

2026-09-20, Apple Silicon macOS, debug test profile. Scope follows
`refactoring-decisions.md`; these are headless CPU/geometry measurements, not
input-to-display latency or visual approval.

## Ownership and deterministic checks

- `DocumentSession` owns accepted document identity and import provenance.
  Replacing/importing advances identity once; an empty import remains unsaved.
- `SearchSession` keeps scheduling, index donation/reuse, and publication
  together. Tests enforce one running worker, latest pending query, stale-result
  rejection, and release of old snapshots. Existing navigation tests retain
  result allocations.
- `PreviewState` owns loaded/pending preview lifetimes separately. Existing GPUI
  tests cover replacement failure, newer requests, close, comments, and cleanup.
- `ImportSession` owns single-job admission, dialog-delayed completion, stale
  result disposal, and the OCR retry continuation. Workspace retains dialogs and
  installation status: a successful installation remains useful after changing
  documents. Completing a stale import still clears the busy indicator.
- Image grips are keyed by logical row and culled with paint's viewport band.
  Real GPUI layout/prepaint/paint tests cover scrolling past early images,
  non-resizable math, list indentation, live resize, scrolling back, and exact
  viewport-margin boundaries. Mutation checks confirmed the tests reject both
  disabled culling and positional row keys.
- The wrapped-table search fixture now supplies explicit `cols=200,100` widths.
  Without widths, the original fixture creates a naturally wide, unwrapped
  column: the match is clipped horizontally. This corrects the test, not the
  separate lack of horizontal search reveal in wide tables.

## Performance evidence

Run serially on an idle machine, separately from the deterministic gate:

```sh
rtk proxy cargo test --bin mdoc perf_tests::host_performance_matrix -- --ignored --nocapture --test-threads=1
```

The matrix exercises local images, mixed Markdown, long paragraphs, tables,
dense matches, scrolling, typing, query bursts with painting, navigation, and
12 document/preview switch cycles. RSS is observational: allocator and global
text/render caches can retain memory, so these samples do not prove leak freedom.
The matrix now includes a 74,400-byte mixed document above the 64 KiB
background-search threshold, checks final-query results against a fresh index,
and measures opening plus PDF/DOCX preview replacement. GPUI's test executor
covers the asynchronous path; these timings do not establish native event-loop
latency under operating-system worker contention.

`MDOC_PERF_MARKDOWN=/absolute/document.md` adds a local Markdown workload,
retaining its asset directory and never saving edits. An optional local preview
benchmark logs only timing and RSS:

```sh
rtk proxy env MDOC_PERF_PREVIEW=/absolute/document.docx cargo test --bin mdoc perf_tests::local_preview_performance -- --ignored --nocapture --test-threads=1
```

The synthetic `comments.docx` fixture is now included in the index (verified
against its deterministic generator), so the normal gate no longer depends on
an omitted local binary. Private examples remain untracked.

Historical logs from this working-tree refactor (milliseconds):

| Workload | Baseline burst / navigation median | Indexed geometry | After image-grip correction |
| --- | ---: | ---: | ---: |
| ordinary | 116.730 / 10.244 | 108.554 / 9.717 | 106.115 / 9.484 |
| large mixed | 2973.644 / 264.286 | 2705.632 / 240.864 | 2723.393 / 243.810 |
| long paragraph | 24822.710 / 1563.701 | 649.236 / 48.696 | 646.694 / 48.685 |
| tables | 644.795 / 56.070 | 622.035 / 52.546 | 620.544 / 53.056 |
| dense matches | 78875.183 / 6144.977 | 1119.850 / 85.043 | 1124.752 / 85.684 |

Sources: `/tmp/mdoc-refactor-baseline-host.log`,
`/tmp/mdoc-after-geometry-full.log`, `/tmp/mdoc-after-grips-host.log`.
These temporary logs are local evidence, not repository fixtures.

## Completion measurements

The expanded host matrix passed in 61.26 seconds on this machine; a final
verification run with all regression limits enabled passed in 62.25 seconds.
The first expanded run is tabulated below. Medians
except the single burst sample (milliseconds):

| Workload | Scroll | Typing | Burst and paint | Navigation |
| --- | ---: | ---: | ---: | ---: |
| ordinary | 9.176 | 9.464 | 107.522 | 9.527 |
| large mixed | 251.987 | 259.667 | 2768.065 | 247.829 |
| background mixed | 301.320 | 305.033 | 4569.794 | 294.983 |
| long paragraph | 38.218 | 50.587 | 680.015 | 49.748 |
| tables | 50.438 | 51.284 | 633.538 | 54.217 |
| dense matches | 38.535 | 45.541 | 1124.339 | 84.917 |

The opt-in matrix enforces the following debug-profile regression ceilings,
rounded up with at least 20% headroom from the recorded runs. They are regression
limits for this host/profile, **not** interactive latency targets; noisy shared
machines should not run this benchmark as a normal CI test.

| Workload | Scroll | Typing | Burst and paint | Navigation |
| --- | ---: | ---: | ---: | ---: |
| ordinary | 12 | 12 | 140 | 13 |
| large mixed | 320 | 320 | 3400 | 310 |
| background mixed | 380 | 380 | 5700 | 370 |
| long paragraph | 50 | 65 | 800 | 62 |
| tables | 62 | 64 | 780 | 68 |
| dense matches | 50 | 60 | 1400 | 110 |

RSS ranged from 252,576 to 294,432 KiB over 12 switches. Every cycle proved that
old PDF/comment entities were dropped and temporary PDFs were deleted. The final
verification run ranged from 256,928 to 281,968 KiB and passed the same ownership
assertions. A normal
gate test also repeats DOCX-to-PDF replacement and close three times with weak
entity handles and temporary-file assertions. These checks establish ownership
release, not absence of all allocator/global-cache retention.

The local 14-page DOCX completed three preview open/close cycles: open median
471.756 ms, maximum 494.189 ms; final RSS 98,080 KiB. No private content is
included in the benchmark output or repository.

The existing local-document 60 Hz scroll target remains unmet on this host.
An isolated unchanged `HEAD` (`fc6c5d0`) run also fails. Before/after scroll p95:
Markdown only 17.29/16.31 ms; preview open 20.39/19.45 ms; preview closed
17.52/17.57 ms. Typing p95 stays below its 50 ms ceiling in both versions.
This is not evidence of a refactor regression, nor a claim of 60 Hz acceptance.
The original benchmark and its thresholds are unchanged.

The standalone editor search matrix also passed, including 2.31 MB projection,
query, incremental typing, and edit/rebuild workloads.

Evidence logs: `/tmp/mdoc-refactor-final-perf.log`,
`/tmp/mdoc-refactor-final-perf-verified.log`,
`/tmp/mdoc-refactor-local-preview.log`, `/tmp/mdoc-refactor-scroll.log`,
`/tmp/mdoc-refactor-baseline-scroll.log`, `/tmp/mdoc-refactor-search-perf.log`.
The numerical results above are retained here independently of those local logs.

Frame-local glyph/wrap indexing removes repeated linear position scans for
dense matches on long LTR lines. Nonmonotone/bidi layouts retain the original
path. The small `scan.math` reuse removes a redundant structural scan without
adding invalidation rules.

Image-grip correction does **not** explain or solve the mixed-document cost:
scroll medians were 248.858 ms before and 250.323 ms after that correction.
Keep it for correct hitbox association and bounded insertion, not a claimed
frame-time improvement. Hidden widget-source shaping and broader cache changes
remain unimplemented pending geometry evidence and isolated measurements.

## Gate and limitations

The completion finishes the interrupted preview callback and routes acceptance
and failure through generation-checked state methods. Duplicate/stale completion,
close, cancellation on drop, import provenance, structural-scan reuse, table-width
cache validity, and shaped glyph geometry have regression coverage.

Required gate:

```sh
rtk proxy cargo fmt --check
rtk proxy cargo clippy --workspace --all-targets -- -D warnings
rtk proxy cargo test --workspace
```

Formatting and strict Clippy pass; **290 tests pass, seven ignored, no failures**
(includes one doctest). The expanded host matrix, standalone search matrix, and
local preview benchmark pass separately. The existing 60 Hz local-document
scroll benchmark fails on both unchanged HEAD and this refactor, as detailed
above. Windows/Linux
builds, live shortcut checks, native input/presentation timing, and human visual
acceptance remain unverified. Existing registry configuration and `block v0.1.6`
future-incompatibility warnings remain. No commit was created.
