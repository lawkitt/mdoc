# Real legal opinion workload, 2026-10-16

Roadmap must-fix item "[Perf] Ordinary legal documents are responsive".
Follows the [layout fixes](../2026-10-16/README.md) measured the same day,
on the same reference Mac and boundaries: Apple M4 (`Mac16,13`), 24 GiB RAM,
macOS 15.7.7 (`24G720`), ordinary desktop session, serial runs, headless CPU
frame observations at logged editor widths, not native input-to-display latency.

## Workload

The earlier evidence measured only a synthetic ~30-page contract, because no real
contract was in the repository. This adds one:
[`legal-opinion.docx`](../../../../tests/fixtures/docx-preview/README.md#scrubbed-legal-opinion),
a real 37-page English legal opinion scrubbed by `scrub.py`. Its structure and
formatting are unchanged and its words are replaced by same-length dictionary
words. Its imported Markdown is 115,662 bytes and 790 lines (1.7× the synthetic
contract): one heading, 139 numbered and bulleted items, four tables (47 rows),
eight footnotes, defined terms in bold and long wrapped paragraphs. The
original's Markdown has the same structure (115,967 bytes).

- `host_performance_matrix` gains a `legal_opinion` workload: scroll, typing,
  search burst and highlight navigation at 760 px.
- `large_markdown_scroll_budget` runs `coverage.docx` and `legal-opinion.docx`
  by default (Markdown only, DOCX preview open at 483 px, preview closed).

No existing ceiling was changed. The new debug ceilings follow the existing
rule (recorded value plus about 20%): host matrix `legal_opinion` scroll 45,
typing 44, burst 1,730 and navigation 22 ms (medians); the DOCX scroll test
keeps 16.667 ms p95 scroll and 50 ms p95 typing for every file in release,
and in debug gives `legal-opinion.docx` 34 ms and 64 ms.

## Release results

Command: `MDOC_PERF_PROFILE=release just perf`; all 12 checks pass.
[Metric lines](release-metrics.log).

| Workload (release, p95) | Scroll | Typing | Navigation |
| --- | ---: | ---: | ---: |
| Ordinary | 1.489 ms | 2.355 ms | 1.605 ms |
| Synthetic contract | 3.144 ms | 2.580 ms | 2.608 ms |
| **Legal opinion** | **4.974 ms** | **4.160 ms** | **2.004 ms** |

| Legal opinion DOCX (release, p95) | Scroll | Typing |
| --- | ---: | ---: |
| Markdown only, 760 px | 2.86 ms | 4.50 ms |
| **DOCX preview open, 483 px** | **3.23 ms** | **5.23 ms** |
| Preview closed, 760 px | 2.84 ms | 4.54 ms |

The public `coverage.docx` with preview open is 1.25 ms; public PDF frames are
0.10 / 0.41 ms (1 / 64 pages). Every ordinary workload, the real legal opinion
and the public DOCX/PDF previews are at or below a 16.7 ms p95 frame in this
release headless run, at least 3× under it.

The legal opinion's search burst (5 incremental queries plus paint) is 184 ms
in release (contract: 47 ms): 3.9× for 1.7× the bytes, not yet profiled. It is
not a frame and search runs as a task, but it is the largest ordinary-document
cost observed and worth profiling with the stress tier.

## Debug results

Command: `just perf`; all 12 checks pass. [Metric lines](debug-metrics.log).

| Workload (debug) | Median | p95 | Ceiling |
| --- | ---: | ---: | ---: |
| Legal opinion scroll | 36.633 ms | 44.543 ms | 45 ms (median) |
| Legal opinion typing | 36.402 ms | 36.928 ms | 44 ms (median) |
| Legal opinion navigation | 17.586 ms | 17.805 ms | 22 ms (median) |
| Legal opinion DOCX preview open, scroll | 22.00 ms | 27.45 ms | 34 ms (p95) |
| Legal opinion DOCX preview open, typing | — | 47.38 ms | 64 ms (p95) |

In one earlier, isolated debug run the `coverage.docx` preview-closed scroll p95
was 18.87 ms (median 9.08 ms), a single outlier frame; the full run's value is
10.01 ms.

## Native spot-check

**Not completed.** A first attempt recorded `xctrace` *Animation Hitches* with
`--launch target/release/mdoc`, but Instruments resolved the bundle identifier
and launched the installed 0.1.0-beta.11 `/Applications/mdoc.app` instead,
and the desktop was in use, so synthetic input could not be isolated. That trace
is discarded. The check needs an idle desktop:

1. Quit any running mdoc; build with `cargo build --release --locked`.
2. Start `target/release/mdoc <real ~30-page contract.docx>` directly (not via
   `xctrace --launch`), note its PID, and run
   `xcrun xctrace record --template 'Animation Hitches' --attach <pid> --time-limit 60s`.
3. Scroll the whole document both ways with the DOCX preview open, type a
   sentence, and step through search results.
4. Record hitches and frame durations in this folder. A pass is no visible
   hitching and p95 frame ≤ 16.7 ms.
