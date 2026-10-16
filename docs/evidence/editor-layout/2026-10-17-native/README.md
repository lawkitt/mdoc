# Native spot-check and intrinsic-sizing fix, 2026-10-17

Roadmap must-fix item "[Perf] Ordinary legal documents are responsive": the
native spot-check on a real ~30-page contract that the
[legal-opinion evidence](../2026-10-16-legal-opinion/README.md) left open.
Apple M4 (`Mac16,13`), 24 GiB RAM, macOS 15.7.7 (`24G720`), built-in display
(60 Hz maximum refresh, so 16.7 ms is one refresh), idle desktop.

## Method

The real 37-page legal opinion (the original of the scrubbed
`legal-opinion.docx`; not distributed) was opened with its DOCX preview in a
release build with symbols (`CARGO_TARGET_DIR=target/profiling
CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only
cargo build --release --locked`), started directly and recorded with
`xcrun xctrace record --template 'Animation Hitches' --attach <pid>`. (With
`--launch`, Instruments resolves the bundle identifier and starts an installed
`/Applications/mdoc.app` instead.)

[`drive.swift`](drive.swift) posts native HID events at fixed rates: editor
scroll, then preview scroll (four passes of 360 pixel-wheel events at 120 Hz),
typing (60 × `x`, then 60 backspaces, at 10 keys/s) and search (Cmd+F,
`company`, then Return 30 times at 5/s; `Q=17,4,14` searches `the`).
[`analyze.py`](analyze.py) reports hitches, presented frames and the main
thread's CPU per run-loop pass (Running time between two waits for events),
the native analogue of a headless frame. [Metric lines](native-metrics.log).

## Finding

Scrolling met the target, but search navigation did not: each Return took
34–54 ms of main-thread CPU (p95 39 ms, 26 of 37 passes over 16.7 ms), and
typing had periodic 20–48 ms passes. The headless matrix missed both: it
searched `alpha`, which barely occurs in real documents, and the headless text
system does not shape with Core Text.

Time Profiler put 881 of 1,617 busy main-thread samples in the editor's layout
measure, creating Core Text lines. Logging the measure showed Taffy laying the
editor out about ten times per frame at different widths: min-content, max
content, 0 px, 1,026 px and a narrow width equal to the markdown column's
min-content, which the search bar sets. The final width (760 px headless,
465 px native) was the last call. GPUI's line-layout cache keeps only the
lines used in the previous frame, so a stable probe width was cheap, but the
narrow one changed with the search bar's match counter (164–232 px) and missed
every line: the band was shaped again at a width nobody painted.

## Changes

- The document scroll container (and each before/after pane's) is absolutely
  positioned inside its sized wrapper, so intrinsic sizing no longer reaches
  the editor. It is measured once per frame, at its final width. Rendering
  and all workspace tests are unchanged.
- Offscreen search matches no longer compute exact highlight quads (which
  shaped table cells across the whole document every frame); their row box is
  enough to scroll one into view, and the next frame refines it. A new test
  reveals an offscreen table match on its painted cell glyphs.
- The host matrix gains `common_word_navigation_frame`: query `the`, 12 steps
  of three frames each (scroll refinement).
- Debug host-matrix ceilings are re-derived from two runs (recorded median
  plus 20%, rounded up), so they guard this fix; the DOCX scroll test's debug
  exception for `legal-opinion.docx` is gone (it now meets 16.7 / 50 ms).

## Native results

| Real legal opinion, main-thread CPU per pass | Before p95 / max | After p95 / max |
| --- | ---: | ---: |
| Editor scroll | 5.63 / 14.98 ms | 4.06 / 12.96 ms |
| Preview scroll (DOCX preview) | 5.18 / 8.89 ms | 3.98 / 15.92 ms |
| Typing | 9.82 / 48.41 ms | 6.25 / 19.87 ms |
| **Search navigation** | **39.29 / 54.20 ms** | **6.24 / 14.08 ms** |

Instruments reports no hitches in either run. Scrolling presents 59.8–59.9
frames per second, with two late frames in each 15 s pass (at most 50 ms).
Two typing passes still take about 20 ms (one dropped frame each, roughly every
3 s); their cause is not profiled. With `the` (adjacent matches), before the layout
fix, 20 of 30 steps took 25–54 ms, so jumps between distant matches were not
the cause.

## Headless results

Commands: `just perf` and `MDOC_PERF_PROFILE=release just perf`; all 12 checks
pass in each. [Release](release-metrics.log), [debug](debug-metrics.log).

| Workload (release, p95) | Before | After |
| --- | ---: | ---: |
| Ordinary scroll / typing | 1.489 / 2.355 ms | 0.619 / 1.051 ms |
| Contract scroll / typing | 3.144 / 2.580 ms | 1.011 / 1.040 ms |
| Legal opinion scroll / typing | 4.974 / 4.160 ms | 1.146 / 1.513 ms |
| Legal opinion common-word navigation | — | 1.198 ms |
| Legal opinion search burst | 183.7 ms | 24.6 ms |
| Tables scroll | 5.327 ms | 1.082 ms |
| 63,488-byte image/code document scroll | 27.464 ms | 4.107 ms |
| 74,400-byte image/code document scroll | 30.670 ms | 4.375 ms |
| Unchanged 135,000-byte Markdown frame | 14.469 ms | 2.488 ms |
| Legal opinion DOCX, preview open | 3.23 ms | 1.18 ms |
| 2 MiB PII prose, unannotated (median) | 97.459 ms | 14.423 ms |

"Before" is the [legal-opinion release log](../2026-10-16-legal-opinion/release-metrics.log).
The image/code-heavy documents, previously left to the stress tier, are now
also well under a frame.

## Limits

One native run per state on one Mac; the CPU-per-pass measure excludes GPU and
compositor time, which Instruments' hitch and frame data cover. Windows and
other displays are checked under native acceptance.
