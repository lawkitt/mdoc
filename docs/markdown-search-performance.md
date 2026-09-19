# Markdown search UI and performance

Measured 2026-09-19 on macOS arm64, Rust 1.98.1, unoptimized test profile.

## UI

The find bar uses the toolbar's 13 px type, 28 px controls, neutral theme
surfaces, a green focus border, a compact Aa case toggle, and a muted 12 px
counter. The toggle exposes its on/off state in its accessible label. Stable
control IDs and existing shortcuts remain in place. The input's 20 px text
layout fits inside the field without a full-height caret.

Human visual acceptance in light/dark themes and native shortcut/IME checks
remain outstanding. Headless tests are not visual approval.

## Reproduction

```sh
rtk proxy cargo test -p mdoc-editor search_performance_matrix -- --ignored --nocapture --test-threads=1
rtk proxy cargo test -p mdoc-editor active_search_navigation_retains_allocations -- --nocapture
```

The ignored matrix runs seven samples per scenario and reports median and
nearest-rank p95 (the maximum with seven samples). All queries with a known
count assert it. Run on an idle machine; these are debug CPU measurements,
not release timings or end-to-end input/presentation latency. Thresholds are
coarse regression alarms, not frame-time promises: cold/edit work gets
100/500/5000 ms by fixture size; cached queries get 50/250 ms; dense matching
gets 250 ms. The typing sequence budget is seven times the query budget.

The mixed fixture includes headings, paragraphs, formatting, entities, tables,
fenced code, Cyrillic, emoji and Unicode case expansion. Sizes are 2,256,
72,192 and 2,310,144 bytes. A separate single code segment has 100,000 hits.

## Initial results

Median milliseconds, including result allocation and destruction:

| Scenario | 2.3 KB | 72 KB | 2.31 MB |
| --- | ---: | ---: | ---: |
| Build + first folded query | 2.08 | 23.03 | 765.84 |
| No match, cached | 0.021 | 0.274 | 8.97 |
| Case sensitive | 0.033 | 0.497 | 20.43 |
| Cached case-insensitive | 0.037 | 0.536 | 21.36 |
| Unicode expansion | 0.024 | 0.330 | 13.05 |
| Cyrillic | 0.031 | 0.423 | 17.42 |
| Emoji | 0.016 | 0.244 | 11.52 |
| Across formatting | 0.021 | 0.317 | 13.49 |
| Seven typing/backspace queries | 0.237 | 3.55 | 139.20 |
| Edit rebuild + query | 1.34 | 23.10 | 799.79 |

Empty queries took less than a microsecond against an existing index. Dense
100,000-hit matching took 31.75 ms median / 37.28 ms p95. At 2.31 MB,
cold p95 was 773.72 ms, cached folded p95 21.91 ms and edit p95 982.06 ms.

A repeat with the final test configuration passed all budgets: the 2.31 MB
cold median was 817.96 ms (p95 1035.43 ms), cached folded median 21.44 ms,
and edit median 816.27 ms. Dense matching measured 28.21 ms. These differences
illustrate run-to-run variation rather than a matcher optimization.

## Findings and changes

Index construction and first-time folding dominate cold search and edits.
The existing background path above 64 KiB remains necessary. Cached queries
are much cheaper, but the large fixture still exceeds a 16.7 ms frame in debug.
Rapid queries during an unfinished initial build may rebuild the index; this
matrix measures sequential queries, not cancellation or worker contention.

Opening/clearing an empty query now skips index construction entirely.
Next/previous used to clone every occurrence and its source ranges on each
step. It now changes only the active index, preserving result allocations and
geometry. The ordinary regression test checks allocation identity for 100,000
matches across 10,000 updates (9.88 ms observed for the update loop). This
measurement excludes paint and scroll refinement. Initial publication still
copies results into the editor; highlight painting still visits occurrences.
These timings do not establish rendering or interactive responsiveness.

## Verification limits

The workspace test `wrapped_table_search_uses_painted_cell_geometry` fails
with `table glyph bounds`. The same focused test failed with HEAD versions of
main.rs, style.rs, markdown_search.rs and editor lib.rs restored, establishing
that the failure predates these changes. No assertion was relaxed.
Windows/Linux and human visual acceptance were not run.

Final checks: `cargo fmt --check`, workspace Clippy with `-D warnings`, and
`git diff --check` pass. `cargo test --workspace --no-fail-fast` reports 259
passing tests, four ignored tests, and the single baseline failure above.
The ignored performance matrix was run explicitly and passed; the large-query
host test also passes with assertions that an empty query creates neither an
index nor a background task. Rustfmt additionally normalized existing search
syntax and UI tests.
