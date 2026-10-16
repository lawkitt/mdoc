# Stabilization implementation, 2026-10-16

Decision: [ADR 0049](../../../adr/0049-measured-stabilization-and-architecture-review.md).
The [audit](README.md) describes the unchanged pre-fix tree at `fd3c685`.

## Changes

| Finding | Implementation commit | Result |
| --- | --- | --- |
| F6 | `253b833` | Public DOCX input, effective width/profile logging, complete failure collection and end-to-end review/case coverage |
| F1 | `3a1d67f` | Asynchronous final case writes with Retry/Cancel/explicit quit recovery; pending revisions and later document edits retained |
| F3 | `1c222ce` | Repeated lines reuse immutable results within and across checks, with exclusion keys and pruning preserved |
| F2 | `6ec16da` | Sorted/merged layout-only diagnostic coverage and binary-search intersections; original word records unchanged |
| F4 | `1531c9d` | Explicit review/presentation effects; snapshots after debounce; one worker plus one latest request without a cache mutex |
| F5 | `eb7d767` | Revision-keyed grouped/flat panel data, cached counts and case-name membership, constant-time compare invalidation and indexed offset mapping |

A follow-up (`6018730`) fixes the dense wrapped-link geometry cost exposed by
the full matrix: reuse the existing position index and bound cursor hitboxes to
the viewport. The final verification commit supplies the public PDF default,
correct Review-tab phase assertions, this evidence and completed-interview cleanup.
Case coordination/document lifecycle are still separate roadmap work; retained
layout remains conditional on profiling.

## Correctness and routine gate

`just check` passes on this macOS host: formatting, cargo-deny, strict workspace
Clippy and all 561 non-ignored tests (21 ignored). It was run before every
implementation commit. New checks cover actual IO/store-open failures, quit
retry/cancel/discard, newer pending revisions, edits during quit, repeated-line
cache reuse, dense diagnostic intersections, debounce/admission/stale spelling,
exclusion changes and panel/history/filter invalidation. Existing source/history,
identity, preview lifetime and encryption checks remain intact.

## Measurement boundaries

Apple M4 (`Mac16,13`), 24 GiB RAM, macOS 15.7.7 (`24G720`); ordinary desktop
session, serial benchmark commands. These are CPU/headless observations, not
native input-to-display latency. Effective editor widths are logged (760 px in
the host matrix; 714 px in the 12,000-byte review matrix), with unchanged source
workloads.

The public DOCX scroll workload now differs from the unavailable private
historical corpus, so its timings are a new baseline, not a speedup comparison.
No timing ceiling was widened. Resource-release assertions are distinct from
RSS observations; retained process caches do not by themselves prove a leak.

## Debug measurements

Command: `just perf`. [Metric lines and test outcomes](debug-metrics.log).

| Measurement | Before | After |
| --- | ---: | ---: |
| 490,000-byte repeated-line spelling, warm | 224.741 ms | 6.070 ms |
| 2 MiB / 20,000 mentions, 64 expanded entities, panel data only | Not isolated | 15.671 ms cold / 0.009666 ms warm |
| Ordinary host scroll, median | 14.128 ms | 14.317 ms |
| Unchanged 135,000-byte Markdown frame, median | 235.993 ms (initial audit) | 243.798 ms |

The first review-matrix captures inadvertently retained the post-Apply Share tab.
The final harness selects Review explicitly and asserts each phase's candidate,
applied and comparison state. Those earlier phase timings are excluded from
before/after claims.

The local algorithm/scheduling fixes do not resolve the remaining whole-document
layout cost. The unchanged debug host ceilings still flag ordinary scroll,
typing, query-burst and navigation. Before/after painting and unchanged long
Markdown remain layout-bound; the cached data-preparation timings must not be
presented as frame timings. `just perf` attempts all 12 checks and reports two
failures: the ordinary host workload and the public DOCX preview scroll p95
(16.76 ms versus 16.667 ms). All other checks, including all five 2 MiB geometry
workloads, pass. The 64-page synthetic PDF check passes with 4.11 ms frame p95
and 15.04 ms settle p95; blank pages exercise page admission and raster lifetime,
not complex PDF content.

The corrected 12,000-byte Review-tab matrix records frame medians of 19.318 ms
proposed, 22.742 ms applied and 25.704 ms before/after, with 247.239 ms for a
three-member mapping update. These are new baselines without a comparable
pre-change Review-tab measurement.

The first dense wrapped-link run was stopped after a three-second `sample`
capture found all 142 busy samples in prepaint's repeated GPUI glyph-position
lookups. It supplies profiling evidence, not a completed before timing. After
`6018730`, the five-case geometry matrix completes in 203.05 seconds; the dense
wrapped-link frame median is 1,232.770 ms. Added annotation medians across the
five workloads range from -2.413 to 10.845 ms (noise included), while baseline
layout remains expensive. Sample and full logs are in the task artifact folder
recorded in the audit.

## Release measurements

Command: `MDOC_PERF_PROFILE=release just perf` (serial `-j 1`, optimized/LTO).
These are new release baselines, not before/after speedup claims. The interrupted
initial compile produced no measurements; the resumed run supplies the results.
All 12 checks pass with unchanged ceilings; [metric lines and outcomes](release-metrics.log).

| Workload | Median / observation | p95 where recorded |
| --- | ---: | ---: |
| Ordinary scroll, 1,984 bytes / 760 px editor | 1.567 ms | 1.575 ms |
| Ordinary typing | 1.610 ms | 2.476 ms |
| Ordinary search burst and paint | 19.120 ms | — |
| Ordinary highlight navigation | 1.684 ms | 1.708 ms |
| Unchanged 135,000-byte Markdown frame / 760 px editor | 28.769 ms | 33.860 ms |
| Review proposed, 12,000 bytes / 160 mentions / 714 px editor | 1.849 ms | 1.858 ms |
| Review applied | 2.045 ms | 2.061 ms |
| Review before/after | 2.203 ms | 2.226 ms |
| Three-member case update, ordinary workload | 26.788 ms | — |
| 2 MiB / 20,000 mentions / 64 entities, panel data only | 1.030 ms cold / 0.000375 ms warm | — |
| 490,000-byte repeated-line spelling | 0.425 ms cold / 0.343 ms warm | — |

All five release geometry workloads pass: unannotated layout medians range from
66.187 to 322.485 ms, with annotation additions from 0.200 to 4.441 ms. The
largest sampled RSS is 714,416 KiB; this is a sample, not a peak-memory bound.
The 64-page blank PDF records 0.55 ms frame p95 and 5.00 ms raster-settle p95.

The ordinary host and public DOCX scroll checks pass their unchanged ceilings in
release. This does not establish a universal 60 Hz target: the 63,488-byte and
74,400-byte host fixtures still take 37.041 ms and 42.369 ms median to scroll,
and the unchanged long document also exceeds one 60 Hz frame. These whole-layout
costs justify further profiling under the conditional roadmap investigation.

## Release stress: review and case fan-out

Reproduce the workload with:

```sh
MDOC_PERF_STRESS=1 cargo test --release --locked -j 1 -p mdoc \
  review_host_performance -- --ignored --nocapture --test-threads=1
```

The [measured run](release-review-stress.log) used the already-built
test binary under `/usr/bin/time -l`, excluding compilation. It exercises three
case members, each with 2 MiB / 20,000 mentions / 64 identities, at a 714 px
editor width. Proposed/applied counts, Review/compare phases and the final
shared alias in all three members are asserted.

| Phase | Frame median | Frame p95 |
| --- | ---: | ---: |
| Proposed review | 211.895 ms | 218.379 ms |
| Applied review | 223.153 ms | 234.546 ms |
| Before/after | 248.241 ms | 261.973 ms |

The single measured three-member mapping update takes 2,266.986 ms, including
closing comparison, retagging, case propagation, queued work settling and one
redraw. The complete run takes 24.10 seconds, including setup; maximum resident set size is
1,753,088,000 bytes (1.633 GiB). Darwin separately reports 448,956,048 bytes
(428.158 MiB) peak memory footprint. These are distinct process-wide metrics,
including all three documents, history, comparison and runtime state; the run
does not attribute allocations to a subsystem or establish a leak.

State assertions pass, but these substantial latency/memory observations do not
establish stress-tier performance acceptance. Keep large case fan-out and peak
memory in the follow-up profiling scope, alongside whole-document layout.

## Verification boundaries

Native UI/IME/accessibility, Windows/Linux execution and detector/OCR
qualification have not been performed by this change.
