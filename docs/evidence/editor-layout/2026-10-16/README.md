# Editor layout cost, 2026-10-16

Roadmap must-fix item "[Perf] Ordinary legal documents are responsive";
decision context in [ADR 0049](../../../adr/0049-measured-stabilization-and-architecture-review.md).
Earlier measurements: [stabilization results](../../stabilization/2026-10-16/results.md).

## Machine and boundaries

Apple M4 (`Mac16,13`), 24 GiB RAM, macOS 15.7.7 (`24G720`), the same reference
Mac as the stabilization evidence; ordinary desktop session, serial runs. These
are headless CPU frame observations at logged editor widths (760 px host matrix,
714 px review matrix, 483 px with the DOCX preview open), not native
input-to-display latency.

## Profiling

Release builds with symbols (`strip = false`, line tables, separate
`CARGO_TARGET_DIR`) were sampled with macOS `sample` while the host matrix
scrolled. Whole-document work per frame, not PII annotation, dominated:

1. Every offscreen line pushed a placeholder that shaped an empty string
   through GPUI's text system: thousands of shapes, allocations and layout-cache
   lookups per frame.
2. The layout measure closure computed the caret row with `row_col`, which
   built the full line-start table (a `Vec` over every line) on each call;
   `caret_table_cell_pos` did the same in prepaint.
3. List and alert markers were re-measured (shaped) on every list line, onscreen
   or not.
4. Table rows were styled and shaped from raw source on every frame, although
   only painted rows and the caret's revealed row use that layout.

## Changes

- Shaped lines are shared (`Rc<WrappedLine>`); all placeholder rows in one
  shaping pass reuse a single empty layout.
- `row_col` counts newlines before the offset without allocating;
  `caret_table_cell_pos` finds its row bounds locally.
- Marker widths are memoized per shaping pass (`GlyphWidths`).
- Table rows outside the shaping band keep their grid data and exact,
  cached height but skip styling and shaping. A new editor test checks that
  geometry (line tops, heights, wrap rows) is identical to the unwindowed first
  frame at three scroll positions, and that offscreen rows are windowed.

No ceiling was raised. The host matrix gains a `contract` workload: a synthetic
69,871-byte agreement (24 articles, numbered and nested clauses, defined terms,
schedule tables, long wrapped paragraphs). Its debug ceilings follow the
existing rule (recorded median plus about 20%). `review_host_performance` gains a
`plain_layout` phase at the same editor width, separating layout from PII
overhead.

## Release results

Command: `MDOC_PERF_PROFILE=release just perf`; all 12 checks pass.
[Metric lines](release-metrics.log). "Before" for `contract` is the same
workload on the pre-change tree with a symbolized release build; other rows
come from the [stabilization release log](../../stabilization/2026-10-16/release-metrics.log).

| Workload (release, p95) | Before | After |
| --- | ---: | ---: |
| Ordinary scroll / typing / navigation | 1.575 / 2.476 / 1.708 ms | 1.493 / 2.346 / 1.602 ms |
| **Contract scroll / typing / navigation** | 6.174 / 5.696 / 5.269 ms | **3.128 / 2.555 / 2.548 ms** |
| Tables scroll | 8.311 ms | 5.475 ms |
| 63,488-byte image/code document scroll | 37.704 ms | 27.301 ms |
| 74,400-byte image/code document scroll | 43.618 ms | 30.681 ms |
| Unchanged 135,000-byte Markdown frame | 33.860 ms | 12.062 ms |
| Public DOCX, preview open | 1.49 ms | 1.27 ms |
| Public PDF, 1 page / 64 pages, frame | 0.35 / 0.55 ms | 0.28 / 0.62 ms |

Every ordinary scroll, typing and navigation workload, and the public DOCX/PDF
preview workloads, are at or below a 16.7 ms p95 frame in this release headless
run. The contract search burst (5 queries plus paint) is 49 ms in total, not a
frame.

Layout versus PII overhead, 12,000 bytes / 160 mentions / 714 px (median):

| Phase | Frame | Over plain layout |
| --- | ---: | ---: |
| Plain layout (no review) | 1.102 ms | — |
| Proposed review | 1.554 ms | +0.45 ms |
| Applied review | 1.749 ms | +0.65 ms |
| Before/after | 1.902 ms | +0.80 ms |

The three-member case update takes 24.360 ms. The 2 MiB geometry workloads'
unannotated medians range from 70.647 to 184.020 ms (previously 66.187–322.485 ms);
annotation adds -0.541 to 1.511 ms (noise included). These are stress-tier
observations, not budgets.

## Debug results

Command: `just perf`; all 12 checks pass, including the two that failed
before. [Metric lines](debug-metrics.log). The debug run's host matrix predates
the `contract` ceiling; its measured medians (25.3 / 24.6 / 25.0 ms) are within it.

| Workload (debug) | Before | After | Ceiling |
| --- | ---: | ---: | ---: |
| Ordinary scroll, median | 14.317 ms | 11.058 ms | 12 ms |
| Ordinary navigation, median | 15.384 ms | 12.147 ms | 13 ms |
| DOCX preview open, p95 | 16.76 ms | 12.26 ms | 16.667 ms |
| Tables scroll, median | 58.383 ms | 29.269 ms | 62 ms |
| Unchanged long Markdown frame, median | 243.798 ms | 113.634 ms | — |

## Remaining

- **Native spot-check not performed.** The finish line requires a native check
  on a real ~30-page contract on this Mac. No real contract is in the
  repository (private documents are not distributed), and headless timings do
  not establish native presentation latency.
- Image- and code-heavy documents still lay out every widget and code line per
  frame (27–31 ms release at 63–74 KB); their rows are not yet windowed.
- Process memory and large case fan-out remain stress-tier follow-up
  ([Later](../../../../ROADMAP.md#later)).
